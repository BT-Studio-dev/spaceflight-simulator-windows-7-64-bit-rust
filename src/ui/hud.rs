//! In-flight HUD and the shared Aero-style widget drawing used by the menus.
//!
//! The HUD deliberately borrows Windows 7's visual language: translucent
//! glass panels, a glossy title bar, and the classic "Aero" blue.

use crate::font;
use crate::math::{fmt_dist, fmt_duration, fmt_mass, fmt_speed, fmt_time};
use crate::render::canvas::{lerp_color, rgb, Canvas, Color};
use crate::rocket::Vessel;
use crate::world::{BodyKind, System};

pub const AERO_TITLE_TOP: Color = rgb(196, 220, 242);
pub const AERO_TITLE_BOTTOM: Color = rgb(140, 178, 216);
pub const AERO_TEXT: Color = rgb(236, 244, 252);
pub const AERO_ACCENT: Color = rgb(120, 200, 255);
pub const AERO_WARN: Color = rgb(255, 196, 90);
pub const AERO_BAD: Color = rgb(255, 120, 110);
pub const AERO_GOOD: Color = rgb(140, 240, 160);

/// A Windows-7-like window frame: gradient title bar, glass body, bevel.
/// Rounded corners are faked by knocking out the four corner pixels.
pub fn aero_window(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, title: &str) {
    aero_window_hover(cv, x, y, w, h, title, false)
}

/// Rectangle of the (decorative but clickable) close button.
pub fn close_button_rect(x: i32, y: i32, w: i32) -> (i32, i32, i32, i32) {
    let bw = 13;
    (x + w - bw - 3, y + 2, bw, 12)
}

/// Same as [`aero_window`] but highlights the close button on hover.
pub fn aero_window_hover(
    cv: &mut Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    title: &str,
    close_hover: bool,
) {
    if w < 24 || h < 18 {
        return;
    }
    // Outer glow/shadow.
    cv.blend_rect(x - 2, y - 2, w + 4, h + 4, rgb(0, 0, 0), 0.35);
    // Body glass.
    for yy in 0..h {
        let t = yy as f32 / h as f32;
        let c = lerp_color(rgb(22, 30, 44), rgb(10, 15, 24), t);
        cv.blend_rect(x, y + yy, w, 1, c, 0.90);
    }
    // Title bar.
    let th = 15;
    for yy in 0..th {
        let t = yy as f32 / th as f32;
        let c = lerp_color(AERO_TITLE_TOP, AERO_TITLE_BOTTOM, t);
        cv.blend_rect(x + 1, y + 1 + yy, w - 2, 1, c, 0.92);
    }
    // Gloss on the title bar.
    cv.blend_rect(x + 1, y + 1, w - 2, th / 2, rgb(255, 255, 255), 0.25);
    // Title text.
    cv.text(x + 6, y + 4, title, 1, rgb(16, 32, 54));
    // Close button (drawn on every panel; only the pad's is wired up).
    let bw = 13;
    let bx = x + w - bw - 3;
    let by = y + 2;
    if close_hover {
        cv.blend_rect(bx, by, bw, th - 3, rgb(246, 190, 180), 0.95);
    } else {
        cv.blend_rect(bx, by, bw, th - 3, rgb(214, 232, 250), 0.85);
    }
    cv.rect_outline(bx, by, bw, th - 3, rgb(120, 150, 185));
    cv.text(bx + 4, by + 3, "X", 1, rgb(60, 70, 85));
    // Frame.
    cv.rect_outline(x, y, w, h, rgb(120, 160, 200));
    cv.rect_outline(x + 1, y + 1, w - 2, h - 2, rgb(50, 70, 95));
    // Corner knock-outs for a soft rounded appearance.
    for (cx, cy) in [(x, y), (x + w - 1, y), (x, y + h - 1), (x + w - 1, y + h - 1)] {
        cv.blend(cx, cy, rgb(0, 0, 0), 0.0);
    }
}

/// Draw a labelled key/value row, right-aligned value.
pub fn kv(cv: &mut Canvas, x: i32, y: i32, w: i32, key: &str, value: &str) {
    cv.text_alpha(x, y, key, 1, rgb(160, 185, 210), 0.95);
    let vw = font::text_width(value, 1);
    cv.text_shadow(x + w - vw, y, value, 1, AERO_TEXT);
}

pub fn kv_color(cv: &mut Canvas, x: i32, y: i32, w: i32, key: &str, value: &str, c: Color) {
    cv.text_alpha(x, y, key, 1, rgb(160, 185, 210), 0.95);
    let vw = font::text_width(value, 1);
    cv.text_shadow(x + w - vw, y, value, 1, c);
}

/// A clickable Aero button. Returns the hit rectangle for the caller.
pub fn button(
    cv: &mut Canvas,
    x: i32,
    y: i32,
    min_w: i32,
    label: &str,
    hover: bool,
    enabled: bool,
) -> (i32, i32, i32, i32) {
    let tw = font::text_width(label, 1);
    let w = (tw + 26).max(min_w);
    let h = 20;
    let top = if hover && enabled {
        rgb(214, 236, 255)
    } else if !enabled {
        rgb(70, 78, 90)
    } else {
        rgb(178, 206, 234)
    };
    let bottom = if hover && enabled {
        rgb(150, 196, 240)
    } else if !enabled {
        rgb(48, 54, 64)
    } else {
        rgb(120, 160, 204)
    };
    for yy in 0..h {
        let t = yy as f32 / h as f32;
        cv.blend_rect(x, y + yy, w, 1, lerp_color(top, bottom, t), 0.95);
    }
    cv.blend_rect(x + 1, y + 1, w - 2, h / 2, rgb(255, 255, 255), 0.22);
    cv.rect_outline(x, y, w, h, if hover && enabled { rgb(230, 245, 255) } else { rgb(150, 180, 215) });
    cv.rect_outline(x + 1, y + 1, w - 2, h - 2, rgb(40, 60, 85));
    let tc = if enabled { rgb(18, 34, 56) } else { rgb(120, 128, 140) };
    cv.text(x + (w - tw) / 2, y + 7, label, 1, tc);
    (x, y, w, h)
}

pub fn hit(rect: (i32, i32, i32, i32), mx: i32, my: i32) -> bool {
    let (x, y, w, h) = rect;
    mx >= x && my >= y && mx < x + w && my < y + h
}

/// A big centred title with a subtle glow, for the menus.
pub fn banner(cv: &mut Canvas, cx: i32, y: i32, text: &str, scale: i32, c: Color) {
    let tw = font::text_width(text, scale);
    cv.glow(cx as f32, y as f32 + 6.0 * scale as f32, tw as f32 * 0.75, rgb(30, 90, 160), 0.55);
    for ox in [-1, 0, 1] {
        for oy in [-1, 0, 1] {
            if ox != 0 || oy != 0 {
                cv.text_alpha(cx - tw / 2 + ox * scale, y + oy * scale, text, scale, rgb(8, 20, 34), 0.8);
            }
        }
    }
    cv.text(cx - tw / 2, y, text, scale, c);
}

// ---------------------------------------------------------------------------
// Flight HUD
// ---------------------------------------------------------------------------

pub struct HudInput<'a> {
    pub sys: &'a System,
    pub vessel: &'a Vessel,
    pub t: f64,
    pub warp: f64,
    pub paused: bool,
    pub message: &'a str,
    pub message_age: f64,
    pub show_help: bool,
    pub fps: f32,
    pub sim_hz: f32,
    pub sas_locked: bool,
}

fn body_angle_label(sys: &System, v: &Vessel, t: f64) -> String {
    let (dr, _) = v.rel(sys, v.reference, t);
    let deg = dr.angle().to_degrees().rem_euclid(360.0);
    format!("{:.1}°", deg)
}

pub fn draw_flight_hud(cv: &mut Canvas, h: &HudInput) {
    let w = cv.w;
    let hgt = cv.h;
    let v = h.vessel;
    let sys = h.sys;
    let b = &sys.bodies[v.reference];

    // ---- top-left: flight data ----------------------------------------
    let px = 12;
    let mut py = 12;
    aero_window(cv, px, py, 232, 176, &format!("FLIGHT · {}", v.name));
    py += 22;
    let ix = px + 10;
    let iw = 232 - 20;
    let alt = v.altitude(sys, v.reference, h.t);
    kv(cv, ix, py, iw, "Altitude", &fmt_dist(alt));
    py += 11;
    kv(cv, ix, py, iw, "Speed", &fmt_speed(v.vel.len()));
    py += 11;
    let (dr, dv) = v.rel(sys, v.reference, h.t);
    let up = dr.norm();
    let vv = dv.dot(up);
    let vh = (dv - up * vv).len();
    kv(cv, ix, py, iw, "Vertical", &fmt_speed(vv));
    py += 11;
    kv(cv, ix, py, iw, "Horizontal", &fmt_speed(vh));
    py += 11;
    kv(cv, ix, py, iw, "Body", b.name);
    py += 11;
    kv(cv, ix, py, iw, "Time", &fmt_time(h.t));
    py += 11;
    kv(cv, ix, py, iw, "Warp", &format!("x{:.0}", h.warp));
    py += 12;

    // Fuel bars for every stage, bottom stage first.
    let mut stages: Vec<u32> = v.parts.iter().map(|p| p.stage).collect();
    stages.sort_unstable();
    stages.dedup();
    for s in stages {
        let cap: f64 = v
            .parts
            .iter()
            .filter(|p| p.stage == s)
            .map(|p| p.def.fuel)
            .sum();
        let cur: f64 = v
            .parts
            .iter()
            .filter(|p| p.stage == s)
            .map(|p| p.fuel)
            .sum();
        if cap <= 0.0 {
            continue;
        }
        let frac = (cur / cap) as f32;
        let active = s == v.current_stage;
        let label = format!("S{}", s);
        cv.text_alpha(ix, py + 1, &label, 1, if active { AERO_TEXT } else { rgb(140, 155, 175) }, 0.95);
        let fill = if frac > 0.25 {
            rgb(90, 190, 120)
        } else if frac > 0.08 {
            AERO_WARN
        } else {
            AERO_BAD
        };
        cv.bar(ix + 18, py, iw - 18, 7, frac, fill, rgb(12, 20, 30));
        py += 10;
    }

    // ---- top-right: orbit + environment --------------------------------
    let ow = 214;
    let ox = w - ow - 12;
    let mut oy = 12;
    let o = v.orbit(sys, v.reference, h.t);
    aero_window(cv, ox, oy, ow, 132, "ORBIT");
    oy += 22;
    let jx = ox + 10;
    let jw = ow - 20;
    let apo = if o.apo.is_finite() {
        fmt_dist(o.apo - b.radius)
    } else {
        "ESCAPE".to_string()
    };
    let peri = fmt_dist(o.peri - b.radius);
    kv(cv, jx, oy, jw, "Apoapsis", &apo);
    oy += 11;
    kv(cv, jx, oy, jw, "Periapsis", &peri);
    oy += 11;
    let per = if o.period.is_finite() {
        fmt_duration(o.period)
    } else {
        "-".to_string()
    };
    kv(cv, jx, oy, jw, "Period", &per);
    oy += 11;
    kv(cv, jx, oy, jw, "Eccentricity", &format!("{:.3}", o.eccentricity));
    oy += 11;
    kv(cv, jx, oy, jw, "Stage dV", &fmt_speed(v.stage_delta_v()));
    oy += 11;
    let twr = v.twr(b.surface_gravity(), v.density);
    let twr_c = if twr >= 1.2 { AERO_GOOD } else if twr >= 1.0 { AERO_WARN } else { AERO_BAD };
    kv_color(cv, jx, oy, jw, "TWR", &format!("{:.2}", twr), twr_c);
    oy += 11;
    kv(cv, jx, oy, jw, "Mass", &fmt_mass(v.total_mass()));

    // Environment window under it.
    let ey = 12 + 140;
    aero_window(cv, ox, ey, ow, 92, "ENVIRONMENT");
    let mut yy = ey + 22;
    let jx = ox + 10;
    kv(cv, jx, yy, jw, "Pressure", &format!("{:.0}%", v.density * 100.0));
    yy += 11;
    kv(cv, jx, yy, jw, "Air density", &format!("{:.4} kg/m³", {
        let dens = if v.density > 0.0 { v.density * b.atmo_rho0 } else { 0.0 };
        dens
    }));
    yy += 11;
    kv(cv, jx, yy, jw, "Q", &format!("{:.0} Pa", 0.5 * v.density * b.atmo_rho0 * v.airspeed * v.airspeed));
    yy += 11;
    kv(cv, jx, yy, jw, "Position", &body_angle_label(sys, v, h.t));
    yy += 11;
    kv_color(cv, jx, yy, jw, "G-force", &format!("{:.2} g", v.g_force), if v.g_force > 6.0 { AERO_WARN } else { AERO_TEXT });

    // ---- bottom-left: throttle + staging -------------------------------
    let bw = 232;
    let by = hgt - 84;
    aero_window(cv, 12, by, bw, 72, "CONTROL");
    let tix = 22;
    let mut ty = by + 22;
    cv.text_alpha(tix, ty, "THROTTLE", 1, rgb(160, 185, 210), 0.95);
    let pct = (v.throttle * 100.0).round() as i32;
    let label = format!("{}%", pct);
    let lw = font::text_width(&label, 1);
    cv.text_shadow(12 + bw - 10 - lw, ty, &label, 1, AERO_TEXT);
    ty += 9;
    cv.bar(tix, ty, bw - 20, 8, v.throttle as f32, rgb(120, 190, 250), rgb(10, 16, 24));
    ty += 13;
    let stage_state = if v.stage_count > 1 {
        format!("STAGE {}/{}   [SPACE]", v.current_stage, v.stage_count)
    } else {
        "FINAL STAGE".to_string()
    };
    cv.text_shadow(tix, ty, &stage_state, 1, if v.stage_count > 1 { AERO_TEXT } else { rgb(150, 165, 180) });
    ty += 10;
    let sas = if v.sas { "SAS ON" } else { "SAS OFF" };
    let chute = if v.chute { "CHUTE ARMED" } else { "CHUTE SAFE" };
    let fc = if v.chute { AERO_GOOD } else { rgb(150, 165, 180) };
    cv.text_shadow(tix, ty, &format!("{}  ·  ", sas), 1, if v.sas { AERO_ACCENT } else { rgb(150, 165, 180) });
    let off = font::text_width(&format!("{}  ·  ", sas), 1);
    cv.text_shadow(tix + off, ty, chute, 1, fc);

    // ---- bottom-right: mini system map --------------------------------
    draw_minimap(cv, w - 12 - 168, hgt - 12 - 124, 168, 124, sys, v, h.t);

    // ---- centre markers -------------------------------------------------
    draw_markers(cv, h);

    // ---- transient message ---------------------------------------------
    if !h.message.is_empty() && h.message_age < 4.0 {
        let alpha = (1.0 - ((h.message_age - 3.0) / 1.0).max(0.0) as f32).clamp(0.0, 1.0);
        let tw = font::text_width(h.message, 2);
        let bw2 = tw + 28;
        let bx = w / 2 - bw2 / 2;
        let byy = 58;
        for i in 0..22 {
            let t = i as f32 / 22.0;
            cv.blend_rect(bx, byy + i, bw2, 1, lerp_color(rgb(20, 40, 64), rgb(10, 20, 32), t), 0.8 * alpha);
        }
        cv.rect_outline(bx, byy, bw2, 22, rgb(130, 190, 240));
        cv.text_alpha(bx + 14, byy + 8, h.message, 2, AERO_TEXT, alpha);
    }

    // ---- paused / warp badge -------------------------------------------
    if h.paused {
        let m = "PAUSED";
        let tw = font::text_width(m, 4);
        cv.text_alpha(w / 2 - tw / 2, hgt / 2 - 60, m, 4, rgb(255, 255, 255), 0.85);
        let sub = "PRESS P TO RESUME";
        let sw = font::text_width(sub, 2);
        cv.text_alpha(w / 2 - sw / 2, hgt / 2 - 22, sub, 2, rgb(190, 215, 240), 0.8);
    }

    // ---- help overlay ----------------------------------------------------
    if h.show_help {
        draw_help(cv, w, hgt, h.fps, h.sim_hz);
    } else {
        let hint = "F1 HELP   P PAUSE   F2 SCREENSHOT";
        let tw = font::text_width(hint, 1);
        cv.text_alpha(w / 2 - tw / 2, hgt - 14, hint, 1, rgb(150, 180, 210), 0.65);
    }
}

fn draw_markers(cv: &mut Canvas, h: &HudInput) {
    // Prograde marker: where the vessel is heading, projected as a small ring
    // offset from the vessel by its velocity direction.
    let v = h.vessel;
    let speed = v.vel.len();
    if speed < 1.0 || v.crashed {
        return;
    }
    let dir = v.vel.norm();
    let (cx, cy) = (cv.w / 2, cv.h / 2);
    let r = 42.0;
    let mx = cx as f32 + dir.x as f32 * r;
    let my = cy as f32 - dir.y as f32 * r;
    cv.ring(mx, my, 5.0, 7.0, AERO_GOOD, 0.75);
    cv.line((mx - 9.0) as i32, my as i32, (mx - 6.0) as i32, my as i32, AERO_GOOD);
    cv.line((mx + 6.0) as i32, my as i32, (mx + 9.0) as i32, my as i32, AERO_GOOD);
    cv.line(mx as i32, (my - 9.0) as i32, mx as i32, (my - 6.0) as i32, AERO_GOOD);
    cv.line(mx as i32, (my + 6.0) as i32, mx as i32, (my + 9.0) as i32, AERO_GOOD);
    // Retrograde marker on the other side.
    let rx = cx as f32 - dir.x as f32 * r;
    let ry = cy as f32 - dir.y as f32 * r;
    cv.ring(rx, ry, 5.0, 7.0, AERO_WARN, 0.5);
}

fn draw_minimap(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, sys: &System, v: &Vessel, t: f64) {
    aero_window(cv, x, y, w, h, "SYSTEM MAP");
    let cx = x + w / 2;
    let cy = y + (h + 12) / 2;
    let rad = (h.min(w) as f32 - 34.0) * 0.5;
    // Scale so Mars fits.
    let mut maxr = 1.0f64;
    for i in 0..sys.len() {
        if sys.bodies[i].kind != BodyKind::Star {
            maxr = maxr.max(sys.position(i, t).len());
        }
    }
    let scale = rad as f64 / (maxr * 1.06);
    // Star.
    cv.disc(cx as f32, cy as f32, 3.5, rgb(255, 210, 120), 1.0);
    for i in 0..sys.len() {
        let b = &sys.bodies[i];
        if b.kind == BodyKind::Star {
            continue;
        }
        let p = sys.position(i, t);
        let bx = cx as f64 + p.x * scale;
        let by = cy as f64 - p.y * scale;
        let (r, g, bb) = b.color_lo;
        let c = rgb(r, g, bb);
        if b.kind == BodyKind::Planet {
            // Orbit ring.
            let orad = (b.orbit_a * scale) as f32;
            if orad > 4.0 {
                cv.ring(cx as f32, cy as f32, orad - 0.5, orad + 0.5, rgb(70, 100, 140), 0.45);
            }
            cv.disc(bx as f32, by as f32, 2.6, c, 1.0);
            cv.text_alpha(bx as i32 + 4, by as i32 - 3, b.name, 1, rgb(170, 200, 230), 0.85);
        } else {
            cv.disc(bx as f32, by as f32, 1.4, c, 1.0);
        }
    }
    // Vessel.
    let vx = cx as f64 + v.pos.x * scale;
    let vy = cy as f64 - v.pos.y * scale;
    cv.disc(vx as f32, vy as f32, 2.2, rgb(120, 255, 170), 1.0);
    cv.ring(vx as f32, vy as f32, 3.0, 5.0, rgb(120, 255, 170), 0.6);
}

fn draw_help(cv: &mut Canvas, w: i32, h: i32, fps: f32, sim_hz: f32) {
    let bw = 400;
    let bh = 254;
    let x = w / 2 - bw / 2;
    let y = h / 2 - bh / 2;
    aero_window(cv, x, y, bw, bh, "CONTROLS");
    let rows: &[(&str, &str)] = &[
        ("W / UP", "throttle up"),
        ("S / DOWN", "throttle down"),
        ("Z / X", "full / cut throttle"),
        ("SPACE", "stage separation"),
        ("C", "arm parachute"),
        ("T", "toggle SAS"),
        ("A / D", "rotate left / right"),
        ("MOUSE WHEEL", "zoom in / out"),
        ("RIGHT DRAG", "pan camera"),
        ("F", "follow vessel"),
        ("M", "cycle focus body"),
        ("R", "revert to launch"),
        ("P / ESC", "pause / menu"),
        ("F2", "screenshot (PNG)"),
        ("F1", "toggle this help"),
    ];
    let mut ry = y + 24;
    for (k, d) in rows {
        cv.keycap(x + 12, ry, k, 1);
        cv.text_alpha(x + 12 + 110, ry + 4, d, 1, AERO_TEXT, 0.95);
        ry += 13;
    }
    let foot = format!("{:.0} FPS · SIM {:.0} Hz", fps, sim_hz);
    cv.text_alpha(x + 12, y + bh - 14, &foot, 1, rgb(150, 175, 200), 0.9);
    let foot2 = "F1 TO CLOSE";
    let fw = font::text_width(foot2, 1);
    cv.text_alpha(x + bw - 12 - fw, y + bh - 14, foot2, 1, rgb(150, 175, 200), 0.9);
}

/// Big status overlay for a crash or a completed landing.
pub fn draw_result(cv: &mut Canvas, title: &str, lines: &[String], good: bool, prompt: &str) {
    let w = cv.w;
    let h = cv.h;
    cv.blend_rect(0, 0, w, h, rgb(0, 0, 0), 0.45);
    let bw = 340;
    let bh = 132 + lines.len() as i32 * 12;
    let x = w / 2 - bw / 2;
    let y = h / 2 - bh / 2;
    aero_window(cv, x, y, bw, bh, if good { "MISSION STATUS" } else { "FLIGHT TERMINATED" });
    let c = if good { AERO_GOOD } else { AERO_BAD };
    let tw = font::text_width(title, 2);
    cv.text_shadow(x + bw / 2 - tw / 2, y + 26, title, 2, c);
    let mut ly = y + 54;
    for l in lines {
        let lw = font::text_width(l, 1);
        cv.text_alpha(x + bw / 2 - lw / 2, ly, l, 1, AERO_TEXT, 0.95);
        ly += 12;
    }
    let pw = font::text_width(prompt, 1);
    cv.text_alpha(x + bw / 2 - pw / 2, y + bh - 22, prompt, 1, rgb(160, 200, 240), 0.9);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Blueprint;

    #[test]
    fn hud_renders_in_all_states() {
        let sys = System::solar();
        let bp = Blueprint::new("SCOUT", &["eng_hawk", "tank_medium", "eng_colt", "tank_small", "pod_capsule"]);
        let mut v = crate::rocket::in_orbit(&bp, &sys, 1, 20_000.0, 0.0, 0.0);
        v.throttle = 0.7;
        v.chute = true;
        let mut cv = Canvas::new(1024, 700);
        for (paused, help, msg) in [
            (false, false, ""),
            (true, true, "LIFTOFF"),
            (false, false, "STAGE 1 SEPARATED AND A RATHER LONG MESSAGE"),
        ] {
            cv.clear(rgb(0, 0, 0));
            let hud = HudInput {
                sys: &sys,
                vessel: &v,
                t: 42.0,
                warp: 1.0,
                paused,
                message: msg,
                message_age: 0.5,
                show_help: help,
                fps: 60.0,
                sim_hz: 120.0,
                sas_locked: false,
            };
            draw_flight_hud(&mut cv, &hud);
            assert!(cv.px.iter().any(|p| *p != 0));
        }
        draw_result(&mut cv, "MISSION FAILURE", &["Vehicle destroyed on impact.".into()], false, "PRESS R TO REVERT");
        draw_result(&mut cv, "TOUCHDOWN", &["Gentle landing at 2.1 m/s.".into()], true, "PRESS R TO FLY AGAIN");
    }

    #[test]
    fn kv_and_buttons_draw() {
        let mut cv = Canvas::new(200, 100);
        kv(&mut cv, 4, 4, 100, "KEY", "VALUE");
        kv_color(&mut cv, 4, 16, 100, "KEY", "VALUE", AERO_GOOD);
        let r = button(&mut cv, 4, 30, 60, "LAUNCH", true, true);
        assert!(hit(r, 10, 36));
        assert!(!hit(r, 300, 400));
        let r2 = button(&mut cv, 4, 54, 60, "DISABLED", false, false);
        assert!(r2.2 >= 60);
    }
}
