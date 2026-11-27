//! Self-test and demo-frame generation.
//!
//! `spaceflight --self-test` runs a set of end-to-end scenarios (ascent,
//! staging, orbit, re-entry, soft landing, crash, screenshot round-trip) and
//! prints a report. `--demo DIR` renders a series of PNG frames through the
//! real renderer, which is how CI proves the game draws correctly even on a
//! Windows runner with no interactive desktop.

use std::io;
use std::path::Path;

use crate::app::{scripted_ascent, Game, Key, Screen};
use crate::math::fmt_dist;
use crate::png;
use crate::rocket::{in_orbit, Event};

pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    pub fn all_ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }

    pub fn print(&self) {
        println!("Spaceflight Simulator {} self-test", crate::VERSION);
        println!("target: {}", crate::TARGET_OS);
        println!("{:-<74}", "");
        for c in &self.checks {
            println!(
                "[{}] {:<34} {}",
                if c.ok { "PASS" } else { "FAIL" },
                c.name,
                c.detail
            );
        }
        println!("{:-<74}", "");
        let failed = self.checks.iter().filter(|c| !c.ok).count();
        if failed == 0 {
            println!("ALL {} CHECKS PASSED", self.checks.len());
        } else {
            println!("{} OF {} CHECKS FAILED", failed, self.checks.len());
        }
    }
}

fn check(name: &'static str, ok: bool, detail: impl Into<String>) -> Check {
    Check {
        name,
        ok,
        detail: detail.into(),
    }
}

/// Run every scenario. `workdir` receives temporary PNGs.
pub fn run(workdir: &Path) -> io::Result<Report> {
    std::fs::create_dir_all(workdir)?;
    let mut checks = Vec::new();

    // 1. Renderer: every screen draws a non-blank frame.
    let mut g = Game::new(960, 600);
    let t0 = std::time::Instant::now();
    g.draw();
    println!("[selftest] pad frame rendered in {:?}", t0.elapsed());
    let pad_pixels = g.canvas.px.iter().filter(|p| **p != 0).count();
    checks.push(check(
        "pad screen renders",
        pad_pixels > 50_000,
        format!("{} lit pixels", pad_pixels),
    ));

    // 2. PNG round-trip of a live frame.
    let file = workdir.join("selftest-pad.png");
    png::write_png(&file, g.canvas.w as u32, g.canvas.h as u32, &g.canvas.px)?;
    let bytes = std::fs::read(&file)?;
    match png::decode(&bytes) {
        Ok(d) => checks.push(check(
            "png round-trip",
            d.w == g.canvas.w as u32 && d.h == g.canvas.h as u32,
            format!("{} bytes, {}x{}", bytes.len(), d.w, d.h),
        )),
        Err(e) => checks.push(check("png round-trip", false, e)),
    }

    // 3. Ascent to orbit.
    let mut g2 = Game::new(960, 600);
    let events = scripted_ascent(&mut g2, 150.0);
    let v = g2.vessel.as_ref().expect("vessel");
    let alt = v.altitude(&g2.sys, v.reference, g2.t);
    let speed = v.vel.len();
    let staged = events.iter().any(|e| matches!(e, Event::Staged(_)));
    checks.push(check("ascent reaches 40 km+", alt > 40_000.0, fmt_dist(alt)));
    checks.push(check(
        "ascent builds 900 m/s+",
        speed > 900.0,
        format!("{:.0} m/s", speed),
    ));
    checks.push(check(
        "staging occurred",
        staged || g2.pad.blueprint.stage_count() == 1,
        format!("stages={}", g2.pad.blueprint.stage_count()),
    ));
    let o = v.orbit(&g2.sys, v.reference, g2.t);
    checks.push(check(
        "orbit elements valid",
        o.valid && o.peri > 0.0,
        format!(
            "apo {:.0} km / peri {:.0} km / e {:.3}",
            o.apo / 1e3,
            o.peri / 1e3,
            o.eccentricity
        ),
    ));

    // 4. Flight frame renders and is saved.
    g2.draw();
    let flight_file = workdir.join("selftest-flight.png");
    png::write_png(
        &flight_file,
        g2.canvas.w as u32,
        g2.canvas.h as u32,
        &g2.canvas.px,
    )?;
    let lit = g2.canvas.px.iter().filter(|p| **p != 0).count();
    checks.push(check(
        "flight screen renders",
        lit > 50_000,
        format!("{} lit pixels", lit),
    ));

    // 5. Crash path produces the result screen.
    let sys = crate::world::System::solar();
    let earth = sys.find("EARTH").expect("earth");
    let bp = crate::world::Blueprint::presets()[1].clone();
    let mut falling = in_orbit(&bp, &sys, earth, 3000.0, 0.0, 0.0);
    falling.vel = sys.velocity(earth, 0.0);
    let mut t = 0.0;
    let mut crashed = false;
    for _ in 0..40_000 {
        let ev = falling.step(&sys, t, 0.01, 0.0);
        t += 0.01;
        if ev
            .iter()
            .any(|e| matches!(e, Event::Crashed { .. }))
        {
            crashed = true;
            break;
        }
        if falling.on_ground {
            break;
        }
    }
    checks.push(check(
        "uncontrolled impact detected",
        crashed,
        format!("impact after {:.1} s", t),
    ));

    // 6. Parachute landing from 3 km.
    let chute_bp = crate::world::Blueprint::new(
        "CHUTE TEST",
        &["legs", "eng_colt", "tank_small", "pod_capsule", "chute"],
    );
    let mut v2 = in_orbit(&chute_bp, &sys, earth, 3000.0, 0.0, 0.0);
    v2.vel = sys.velocity(earth, 0.0) + crate::math::V2::new(120.0, 0.0);
    v2.chute = true;
    v2.sas = false;
    let mut t2 = 0.0;
    let mut landed = None;
    for _ in 0..120_000 {
        let ev = v2.step(&sys, t2, 0.01, 0.0);
        t2 += 0.01;
        for e in ev {
            if let Event::Landed { speed } = e {
                landed = Some(speed);
            }
        }
        if v2.crashed || v2.on_ground {
            break;
        }
    }
    checks.push(check(
        "parachute landing survives",
        landed.is_some() && !v2.crashed,
        match landed {
            Some(s) => format!("touchdown at {:.1} m/s", s),
            None => "no touchdown".into(),
        },
    ));

    // 7. Keyboard and menu plumbing.
    let mut g3 = Game::new(800, 500);
    g3.key_pressed(Key::Enter);
    let launched = g3.screen == Screen::Flying;
    g3.key_pressed(Key::Pause);
    let paused = g3.screen == Screen::Paused;
    checks.push(check(
        "menu -> flight -> pause",
        launched && paused,
        format!("launched={} paused={}", launched, paused),
    ));

    // 8. Resizing mid-flight is stable.
    g3.key_pressed(Key::Escape);
    g3.resize(1280, 720);
    g3.draw();
    g3.resize(640, 400);
    g3.draw();
    checks.push(check(
        "live resize is stable",
        g3.canvas.w == 640 && g3.canvas.h == 400,
        format!("{}x{}", g3.canvas.w, g3.canvas.h),
    ));

    Ok(Report { checks })
}

/// Render a spread of demo frames through the real renderer.
pub fn render_demo_frames(game: &mut Game, dir: &Path) -> io::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let mut n = 0;

    fn shot(game: &mut Game, dir: &Path, name: &str) -> io::Result<()> {
        crate::trace::trace(&format!("demo: {}", name));
        eprintln!("[demo] rendering {}", name);
        let t0 = std::time::Instant::now();
        game.draw();
        eprintln!("[demo]   drew in {:?}", t0.elapsed());
        png::write_png(
            &dir.join(format!("{}.png", name)),
            game.canvas.w as u32,
            game.canvas.h as u32,
            &game.canvas.px,
        )
    }

    // 1. Assembly screen.
    shot(game, dir, "01-assembly")?;
    n += 1;

    // 2. On the pad, engine lit.
    game.launch();
    if let Some(v) = game.vessel.as_mut() {
        v.throttle = 1.0;
    }
    for _ in 0..90 {
        game.tick(1.0 / 60.0);
    }
    shot(game, dir, "02-liftoff")?;
    n += 1;

    // 3. Ascent through the atmosphere.
    for _ in 0..600 {
        game.tick(1.0 / 60.0);
    }
    shot(game, dir, "03-ascent")?;
    n += 1;

    // 4. Staged, high altitude.
    game.key_pressed(Key::Stage);
    for _ in 0..240 {
        game.tick(1.0 / 60.0);
    }
    shot(game, dir, "04-staging")?;
    n += 1;

    // 5. Orbit view zoomed out.
    game.key_pressed(Key::Focus);
    game.wheel(-4);
    for _ in 0..120 {
        game.tick(1.0 / 60.0);
    }
    shot(game, dir, "05-orbit")?;
    n += 1;

    // 6. Help overlay.
    game.key_pressed(Key::Help);
    shot(game, dir, "06-help")?;
    n += 1;
    game.key_pressed(Key::Help);

    // 7. Pause menu.
    game.key_pressed(Key::Pause);
    game.draw();
    game.draw();
    png::write_png(
        &dir.join("07-pause.png"),
        game.canvas.w as u32,
        game.canvas.h as u32,
        &game.canvas.px,
    )?;
    n += 1;

    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test_passes() {
        let dir = std::env::temp_dir().join("spaceflight-selftest");
        let report = run(&dir).expect("self test");
        report.print();
        assert!(report.all_ok(), "self-test failures");
    }

    #[test]
    fn demo_frames_are_written_and_valid() {
        let dir = std::env::temp_dir().join("spaceflight-demo");
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = Game::new(800, 500);
        let n = render_demo_frames(&mut g, &dir).expect("demo frames");
        assert_eq!(n, 7);
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            let bytes = std::fs::read(&p).unwrap();
            let dec = png::decode(&bytes).expect("valid png");
            assert_eq!(dec.w, 800);
            assert_eq!(dec.h, 500);
            count += 1;
        }
        assert_eq!(count, 7);
    }
}
