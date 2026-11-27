//! The launch pad: rocket assembly, blueprint presets, world selection and a
//! live launch-pad view with the pad stats. Also hosts the pause menu.

use crate::font;
use crate::math::{fmt_mass, fmt_speed, V2};
use crate::render::camera::Camera;
use crate::render::canvas::{rgb, Canvas};
use crate::render::scene::{Scene, SceneOptions};
use crate::ui::hud::{
    aero_window, aero_window_hover, banner, button, close_button_rect, hit, kv, kv_color, AERO_BAD,
    AERO_GOOD, AERO_TEXT, AERO_WARN,
};
use crate::world::{part_by_id, parts, Blueprint, PartKind, System};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PadAction {
    None,
    /// The window's close button was clicked.
    WindowClose,
    AddPart(usize),
    RemovePart,
    Clear,
    CycleBody,
    CyclePreset,
    Launch,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PauseAction {
    Resume,
    Revert,
    Quit,
}

/// Screen rectangles of the controls on the assembly screen. Exposed so the
/// draw code and the tests agree on where the buttons are.
#[derive(Copy, Clone, Debug)]
pub struct PadLayout {
    pub panel_w: i32,
    pub panel_h: i32,
    pub launch: (i32, i32, i32, i32),
    pub remove: (i32, i32, i32, i32),
    pub clear: (i32, i32, i32, i32),
    pub close: (i32, i32, i32, i32),
}

pub fn pad_layout(w: i32, h: i32) -> PadLayout {
    let panel_w = 300;
    let panel_h = h - 24;
    let px0 = 12;
    let py0 = 12;
    let ix = px0 + 12;
    let by_btn = py0 + panel_h - 26;
    let _ = w;
    PadLayout {
        panel_w,
        panel_h,
        launch: (ix, by_btn, 70, 20),
        remove: (ix + 76, by_btn, 64, 20),
        clear: (ix + 148, by_btn, 62, 20),
        close: close_button_rect(px0, py0, panel_w),
    }
}

pub struct PadState {
    pub blueprint: Blueprint,
    pub body: usize,
    pub preset: usize,
    pub message: String,
    pub message_age: f64,
    pub tab: usize,
    /// Scroll offset for the palette.
    pub scroll: i32,
}

impl PadState {
    pub fn new() -> PadState {
        let presets = Blueprint::presets();
        PadState {
            blueprint: presets[1].clone(),
            body: 1,
            preset: 1,
            message: String::new(),
            message_age: 99.0,
            tab: 0,
            scroll: 0,
        }
    }

    pub fn set_message(&mut self, s: impl Into<String>) {
        self.message = s.into();
        self.message_age = 0.0;
    }

    pub fn add(&mut self, pid: &'static str) {
        if self.blueprint.parts.len() >= 18 {
            self.set_message("STACK FULL (18 PARTS MAX)");
            return;
        }
        // The stack is stored bottom-first. Parts are appended at the top,
        // which is what clicking a palette entry should feel like, except for
        // engines and boosters: those belong underneath the stack they burn
        // for, so an engine click wraps a *new* lower stage around whatever is
        // already there.
        let is_engine = part_by_id(pid).map(|p| p.kind.is_engine()).unwrap_or(false);
        // An engine joins the stage it is standing next to (that is what makes
        // a two-engine first stage); it only starts a new lower stage when it
        // is separated from the rest by a tank, or when the stack has no
        // engine yet. Anything else is appended at the top.
        let starts_new_stage = is_engine
            && matches!(
                self.blueprint.parts.first().and_then(|id| part_by_id(id)),
                Some(p) if p.kind == PartKind::Tank
            );
        if starts_new_stage {
            let mut parts = vec![pid];
            parts.extend(self.blueprint.parts.iter().copied());
            self.blueprint.parts = parts;
        } else {
            self.blueprint.parts.push(pid);
        }
        self.set_message(format!(
            "ADDED {}",
            part_by_id(pid).map(|p| p.name).unwrap_or("PART")
        ));
    }

    pub fn remove(&mut self) {
        if self.blueprint.parts.pop().is_some() {
            self.set_message("REMOVED TOP PART");
        }
    }

    pub fn can_launch(&self) -> Result<(), String> {
        self.blueprint.validate()
    }

    pub fn preset_list() -> Vec<Blueprint> {
        Blueprint::presets()
    }
}

impl Default for PadState {
    fn default() -> Self {
        PadState::new()
    }
}

/// Body names that can host a launch site.
pub fn launch_sites(sys: &System) -> Vec<usize> {
    (0..sys.len())
        .filter(|&i| sys.bodies[i].terrain > 0.0)
        .collect()
}

pub fn draw_pad(
    cv: &mut Canvas,
    state: &mut PadState,
    sys: &System,
    cam: &mut Camera,
    scene: &Scene,
    mouse: (i32, i32),
    mouse_down: bool,
    dt: f64,
) -> PadAction {
    let w = cv.w;
    let h = cv.h;
    let mpp = cam.mpp();

    // ---- launch pad scene ------------------------------------------------
    crate::trace::trace("pad: background");
    scene.draw_background(cv, cam);
    crate::trace::trace("pad: ghost vessel");
    let site = state.body;
    let t = 0.0;
    let theta = 0.35;
    let ghost = if state.can_launch().is_ok() {
        crate::rocket::on_pad(&state.blueprint, sys, site, theta, t)
    } else {
        // Draw whatever is valid so the user sees their stack.
        crate::rocket::on_pad(&state.blueprint, sys, site, theta, t)
    };
    // Camera frames the stack.
    let b = &sys.bodies[site];
    let surf = sys.position(site, t) + V2::polar(b.radius, theta);
    let stack_h = ghost.height();
    let want_ppu = (h as f64 * 0.42) / stack_h.max(1.0);
    cam.set_zoom(want_ppu);
    cam.look_at(surf + V2::polar(b.radius * 0.02 + stack_h * 0.5, theta));
    if dt > 0.0 {
        cam.update(dt);
    }
    cam.snap();
    crate::trace::trace("pad: system");
    scene.draw_system(cv, cam, sys, t, &ghost, &SceneOptions {
        show_trajectory: false,
        show_labels: false,
        terrain_detail: true,
    });
    crate::trace::trace("pad: pad markers");
    // A subtle pad glow under the stack.
    let (px, py) = cam.world_to_screen(surf, w, h);
    let pad_w = (b.radius * 0.09 / mpp) as f32;
    cv.glow(px, py, pad_w.max(20.0), rgb(120, 190, 255), 0.20);
    cv.hline((px - pad_w.max(20.0)) as i32, (px + pad_w.max(20.0)) as i32, py as i32, rgb(150, 200, 250));

    // ---- left panel: assembly -------------------------------------------
    crate::trace::trace("pad: panels");
    let layout = pad_layout(w, h);
    let panel_w = layout.panel_w;
    let panel_h = layout.panel_h;
    let px0 = 12;
    let py0 = 12;
    let hover_close = hit(layout.close, mouse.0, mouse.1);
    aero_window_hover(cv, px0, py0, panel_w, panel_h, "VEHICLE ASSEMBLY BUILD 7601", hover_close);
    let ix = px0 + 12;
    let iw = panel_w - 24;
    let mut y = py0 + 24;
    cv.text_alpha(ix, y, "PALETTE", 1, rgb(160, 190, 215), 0.9);
    y += 12;

    // Palette list, two columns.
    let col_w = iw / 2;
    let row_h = 14;
    // Reserve room for the stack list (6 rows + header) and the stats block.
    let reserved = 176;
    let visible_rows = ((panel_h - reserved) / row_h).clamp(4, 8);
    let start = (state.scroll as usize / 2).min(parts().len().saturating_sub(2));
    let mut action = PadAction::None;
    let mut idx = start;
    let mut ry = y;
    for _row in 0..visible_rows {
        for col in 0..2 {
            if idx >= parts().len() {
                break;
            }
            let p = &parts()[idx];
            let bx = ix + col * col_w;
            let hover = mouse.0 >= bx && mouse.0 < bx + col_w - 4 && mouse.1 >= ry && mouse.1 < ry + row_h - 2;
            if hover {
                cv.blend_rect(bx, ry - 1, col_w - 4, row_h - 1, rgb(90, 150, 210), 0.35);
            }
            let c = match p.kind {
                PartKind::Command => rgb(180, 220, 255),
                PartKind::Tank => rgb(210, 215, 225),
                PartKind::Engine | PartKind::Booster => rgb(255, 190, 120),
                PartKind::Nose => rgb(255, 160, 140),
                _ => rgb(190, 230, 190),
            };
            cv.text_shadow(bx + 2, ry, p.name, 1, c);
            if hover && mouse_down {
                action = PadAction::AddPart(idx);
            }
            idx += 1;
        }
        ry += row_h;
    }

    // ---- stack summary ---------------------------------------------------
    let sy = py0 + panel_h - 160;
    cv.hline(ix, ix + iw, sy - 6, rgb(70, 95, 120));
    cv.text_alpha(ix, sy, "STACK (BOTTOM → TOP)", 1, rgb(160, 190, 215), 0.9);
    let mut yy = sy + 12;
    let max_rows = 6;
    let total = state.blueprint.parts.len();
    let from = total.saturating_sub(max_rows);
    if from > 0 {
        cv.text_alpha(ix, yy, &format!("... {} more", from), 1, rgb(140, 160, 180), 0.8);
        yy += 10;
    }
    for id in &state.blueprint.parts[from..] {
        if let Some(p) = part_by_id(id) {
            let c = if p.kind.is_engine() { AERO_WARN } else { AERO_TEXT };
            cv.text_shadow(ix + 4, yy, p.name, 1, c);
            let m = format!("{} kg", p.dry as i64);
            let mw = font::text_width(&m, 1);
            cv.text_alpha(ix + iw - mw, yy, &m, 1, rgb(150, 175, 200), 0.9);
            yy += 10;
        }
    }

    // ---- validity + stats ------------------------------------------------
    let vy = py0 + panel_h - 86;
    match state.can_launch() {
        Ok(()) => {
            cv.text_shadow(ix, vy, "● VEHICLE IS FLIGHT-READY", 1, AERO_GOOD);
        }
        Err(e) => {
            cv.text_shadow(ix, vy, &format!("● {}", e), 1, AERO_BAD);
        }
    }
    let bp = &state.blueprint;
    let dv = bp.delta_v();
    let site_g = sys.bodies[site].surface_gravity();
    let twr = bp.twr(site_g);
    kv(cv, ix, vy + 12, iw, "Mass", &fmt_mass(bp.mass()));
    kv(cv, ix, vy + 23, iw, "Height", &format!("{:.1} m", bp.height()));
    kv(cv, ix, vy + 34, iw, "Stages", &format!("{}", bp.stage_count()));
    kv_color(cv, ix, vy + 45, iw, "Total dV", &fmt_speed(dv), if dv > 4000.0 { AERO_GOOD } else { AERO_WARN });
    kv_color(cv, ix, vy + 56, iw, "Pad TWR", &format!("{:.2}", twr), if twr >= 1.15 { AERO_GOOD } else { AERO_BAD });

    // ---- buttons ---------------------------------------------------------
    let by_btn = py0 + panel_h - 26;
    let hover1 = hit(layout.launch, mouse.0, mouse.1);
    let hover2 = hit(layout.remove, mouse.0, mouse.1);
    let hover3 = hit(layout.clear, mouse.0, mouse.1);
    let b1 = button(cv, ix, by_btn, 70, "LAUNCH", hover1, true);
    let b2 = button(cv, ix + 76, by_btn, 64, "REMOVE", hover2, !bp.parts.is_empty());
    let b3 = button(cv, ix + 148, by_btn, 62, "CLEAR", hover3, true);
    if mouse_down {
        if hit(b1, mouse.0, mouse.1) {
            action = PadAction::Launch;
        } else if hit(b2, mouse.0, mouse.1) && !bp.parts.is_empty() {
            action = PadAction::RemovePart;
        } else if hit(b3, mouse.0, mouse.1) {
            action = PadAction::Clear;
        }
    }

    crate::trace::trace("pad: palette+stats");
    // ---- right panel: mission setup -------------------------------------
    let rw = 268;
    let rx = w - rw - 12;
    aero_window(cv, rx, py0, rw, 214, "LAUNCH SITE");
    let rix = rx + 12;
    let riw = rw - 24;
    let mut ry2 = py0 + 24;
    let sites = launch_sites(sys);
    for &i in &sites {
        let b = &sys.bodies[i];
        let sel = i == state.body;
        let row = (rix, ry2 - 1, riw, 16);
        let hov = mouse_hit(mouse, row.0, row.1, row.2, row.3);
        if sel {
            cv.blend_rect(row.0, row.1, row.2, row.3, rgb(80, 140, 200), 0.55);
        } else if hov {
            cv.blend_rect(row.0, row.1, row.2, row.3, rgb(80, 130, 180), 0.30);
        }
        let (r, g, bb) = b.color_lo;
        cv.disc((rix + 5) as f32, (ry2 + 5) as f32, 3.0, rgb(r, g, bb), 1.0);
        cv.text_shadow(rix + 14, ry2, b.name, 1, AERO_TEXT);
        let gg = format!("{:.2} g", b.surface_gravity() / 9.80665);
        let gw = font::text_width(&gg, 1);
        cv.text_alpha(rix + riw - gw, ry2, &gg, 1, rgb(160, 185, 210), 0.95);
        if hov && mouse_down && !sel {
            state.body = i;
            state.set_message(format!("LAUNCH SITE: {}", b.name));
        }
        ry2 += 18;
    }
    ry2 += 4;
    let b = &sys.bodies[state.body];
    kv(cv, rix, ry2, riw, "Radius", &format!("{:.0} km", b.radius / 1000.0));
    ry2 += 11;
    kv(cv, rix, ry2, riw, "Escape v", &fmt_speed(b.escape_velocity()));
    ry2 += 11;
    let atmo = if b.atmo_rho0 > 0.0 {
        format!("{:.0} km", b.atmo_height / 1000.0)
    } else {
        "NONE".into()
    };
    kv(cv, rix, ry2, riw, "Atmosphere", &atmo);
    ry2 += 11;
    kv(cv, rix, ry2, riw, "Orbit v", &fmt_speed(crate::rocket::surface_orbit_speed(sys, state.body)));

    // Presets.
    let pyy = py0 + 224;
    aero_window(cv, rx, pyy, rw, 150, "PRESET BLUEPRINTS");
    let mut yy = pyy + 22;
    let presets = PadState::preset_list();
    for (i, bp) in presets.iter().enumerate() {
        let row = (rx + 10, yy - 1, rw - 20, 22);
        let hov = mouse_hit(mouse, row.0, row.1, row.2, row.3);
        let sel = i == state.preset;
        if sel {
            cv.blend_rect(row.0, row.1, row.2, row.3, rgb(80, 140, 200), 0.55);
        } else if hov {
            cv.blend_rect(row.0, row.1, row.2, row.3, rgb(80, 130, 180), 0.30);
        }
        cv.text_shadow(row.0 + 6, yy + 1, &bp.name, 1, AERO_TEXT);
        let info = format!("{} parts · {}", bp.parts.len(), fmt_speed(bp.delta_v()));
        let iw2 = font::text_width(&info, 1);
        cv.text_alpha(row.0 + row.2 - iw2 - 6, yy + 1, &info, 1, rgb(160, 190, 220), 0.9);
        if hov && mouse_down && !sel {
            state.preset = i;
            state.blueprint = bp.clone();
            state.set_message(format!("LOADED PRESET: {}", bp.name));
        }
        yy += 26;
    }

    crate::trace::trace("pad: right panels");
    // ---- launch splash ---------------------------------------------------
    let centre_x = (px0 + panel_w + rx) / 2;
    banner(cv, centre_x, 18, "SPACEFLIGHT SIMULATOR", 2, rgb(230, 245, 255));
    let sub = "WINDOWS 7 · 64-BIT BUILD";
    let sw = font::text_width(sub, 1);
    cv.text_alpha(centre_x - sw / 2, 40, sub, 1, rgb(150, 190, 225), 0.9);

    // Tip line.
    let tip = "CLICK A PALETTE ENTRY TO ADD IT · ENTER TO LAUNCH · ESC TO QUIT";
    let tw = font::text_width(tip, 1);
    cv.text_alpha(centre_x - tw / 2, h - 20, tip, 1, rgb(140, 170, 200), 0.7);

    // Clicking the title bar's close button quits, exactly like Windows 7.
    if hover_close && mouse_down {
        action = PadAction::WindowClose;
    }

    if state.message_age < 3.5 {
        let a = (1.0 - ((state.message_age - 2.5) / 1.0).max(0.0) as f32).clamp(0.0, 1.0);
        let mw = font::text_width(&state.message, 1);
        cv.text_alpha(w / 2 - mw / 2, h - 40, &state.message, 1, rgb(255, 230, 160), a);
    }

    action
}

fn mouse_hit(mouse: (i32, i32), x: i32, y: i32, w: i32, h: i32) -> bool {
    hit((x, y, w, h), mouse.0, mouse.1)
}

/// Screen rectangles of the pause menu buttons.
pub fn pause_layout(w: i32, h: i32) -> [(i32, i32, i32, i32); 3] {
    let bw = 260;
    let bh = 176;
    let x = w / 2 - bw / 2;
    let y = h / 2 - bh / 2;
    let mut out = [(0, 0, 0, 0); 3];
    for (i, r) in out.iter_mut().enumerate() {
        *r = (x + 30, y + 30 + i as i32 * 34, bw - 60, 24);
    }
    out
}

pub fn draw_pause(cv: &mut Canvas, mouse: (i32, i32), mouse_down: bool) -> PauseAction {
    let w = cv.w;
    let h = cv.h;
    cv.blend_rect(0, 0, w, h, rgb(0, 0, 0), 0.55);
    let bw = 260;
    let bh = 176;
    let x = w / 2 - bw / 2;
    let y = h / 2 - bh / 2;
    aero_window(cv, x, y, bw, bh, "PAUSED");
    let mut action = PauseAction::Resume;
    let items = [
        ("RESUME FLIGHT", PauseAction::Resume),
        ("REVERT TO LAUNCH", PauseAction::Revert),
        ("QUIT TO DESKTOP", PauseAction::Quit),
    ];
    let rects = pause_layout(w, h);
    for (i, (label, act)) in items.iter().enumerate() {
        let r = rects[i];
        let hov = hit(r, mouse.0, mouse.1);
        button(cv, r.0, r.1, r.2, label, hov, true);
        if hov {
            action = *act;
            if mouse_down {
                return *act;
            }
        }
    }
    cv.text_alpha(x + 30, y + bh - 20, "ESC TO RESUME", 1, rgb(150, 180, 210), 0.85);
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_operations_keep_the_stack_sane() {
        let mut st = PadState::new();
        st.blueprint = Blueprint::new("EMPTY", &[]);
        st.add("pod_capsule");
        st.add("eng_hawk");
        st.add("tank_medium");
        // The stack is stored bottom-first: the engine is appended above the
        // pod that was added first.
        assert_eq!(st.blueprint.parts[0], "pod_capsule");
        assert_eq!(st.blueprint.parts[1], "eng_hawk");
        assert_eq!(st.blueprint.parts[2], "tank_medium");
        assert_eq!(st.blueprint.stage_count(), 1);
        assert!(st.can_launch().is_ok());

        // A second engine directly under the first joins that stage...
        st.add("eng_hawk");
        assert_eq!(st.blueprint.stage_count(), 1, "engines in a row share a stage");

        // ...but a tank in between separates the stages.
        st.blueprint = Blueprint::new("TWO", &["eng_hawk", "tank_medium", "pod_capsule"]);
        st.add("eng_colt");
        assert_eq!(st.blueprint.parts[0], "eng_colt");
        assert_eq!(st.blueprint.stage_count(), 2);
        assert!(st.can_launch().is_ok());

        st.remove();
        assert_eq!(st.blueprint.parts.len(), 3);
        for _ in 0..40 {
            st.add("tank_small");
        }
        assert_eq!(st.blueprint.parts.len(), 18, "stack size must be capped");
    }

    #[test]
    fn pad_launch_button_triggers_launch() {
        let sys = System::solar();
        let scene = Scene::new();
        let mut cam = Camera::new(V2::ZERO, 0.01);
        let mut st = PadState::new();
        let mut cv = Canvas::new(1024, 700);
        let act = draw_pad(&mut cv, &mut st, &sys, &mut cam, &scene, (500, 400), false, 0.0);
        assert_eq!(act, PadAction::None);

        let l = pad_layout(1024, 700);
        let (bx, by, bw, bh) = l.launch;
        let cx = bx + bw / 2;
        let cy = by + bh / 2;
        assert_eq!(
            draw_pad(&mut cv, &mut st, &sys, &mut cam, &scene, (cx, cy), true, 0.0),
            PadAction::Launch
        );
        // The close button quits.
        let (qx, qy, qw, qh) = l.close;
        assert_eq!(
            draw_pad(&mut cv, &mut st, &sys, &mut cam, &scene, (qx + qw / 2, qy + qh / 2), true, 0.0),
            PadAction::WindowClose
        );
        // A click on empty sky does nothing.
        assert_eq!(
            draw_pad(&mut cv, &mut st, &sys, &mut cam, &scene, (700, 120), true, 0.0),
            PadAction::None
        );
    }

    #[test]
    fn pause_menu_buttons_are_clickable() {
        let mut cv = Canvas::new(1024, 700);
        let rects = pause_layout(1024, 700);
        assert_eq!(draw_pause(&mut cv, (0, 0), false), PauseAction::Resume);
        let (x, y, w, h) = rects[0];
        assert_eq!(
            draw_pause(&mut cv, (x + w / 2, y + h / 2), true),
            PauseAction::Resume
        );
        let (x, y, w, h) = rects[1];
        assert_eq!(
            draw_pause(&mut cv, (x + w / 2, y + h / 2), true),
            PauseAction::Revert
        );
        let (x, y, w, h) = rects[2];
        assert_eq!(
            draw_pause(&mut cv, (x + w / 2, y + h / 2), true),
            PauseAction::Quit
        );
    }

    #[test]
    fn every_body_can_be_selected() {
        let sys = System::solar();
        assert!(launch_sites(&sys).len() >= 3);
        for &i in &launch_sites(&sys) {
            assert!(sys.bodies[i].terrain > 0.0);
        }
    }

    #[test]
    fn palette_entries_are_clickable() {
        // Walk the palette grid using the same geometry the draw code uses and
        // check that each visible entry maps to a distinct part index.
        let sys = System::solar();
        let scene = Scene::new();
        let mut cam = Camera::new(V2::ZERO, 0.01);
        let mut st = PadState::new();
        st.blueprint = Blueprint::new("EMPTY", &[]);
        let mut cv = Canvas::new(1024, 700);
        let mut found = Vec::new();
        // Palette rows start just under the panel header, two columns wide.
        let l = pad_layout(1024, 700);
        let ix = 12 + 12;
        let iw = l.panel_w - 24;
        let col_w = iw / 2;
        let row_h = 14;
        let mut ry = 12 + 24 + 12;
        for _ in 0..6 {
            for col in 0..2 {
                let bx = ix + col * col_w + 4;
                if let PadAction::AddPart(i) = draw_pad(
                    &mut cv,
                    &mut st,
                    &sys,
                    &mut cam,
                    &scene,
                    (bx, ry + 2),
                    true,
                    0.0,
                ) {
                    found.push(i);
                    st.blueprint.parts.clear();
                }
            }
            ry += row_h;
        }
        assert!(!found.is_empty(), "palette produced no clickable entries");
        found.sort_unstable();
        found.dedup();
        assert!(found.len() >= 6, "only {} palette entries clickable", found.len());
    }
}
