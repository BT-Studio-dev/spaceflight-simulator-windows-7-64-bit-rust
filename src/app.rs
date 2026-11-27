//! Game state machine, input handling and the fixed-step simulation loop.
//!
//! Everything here is platform independent: the Win32 layer feeds it key and
//! mouse events and blits the resulting framebuffer, while the CI screenshot
//! tool drives exactly the same code paths without a desktop session.

use crate::fx::Effects;
use crate::math::{fmt_dist, fmt_speed, fmt_time, V2};
use crate::render::camera::Camera;
use crate::render::canvas::{rgb, Canvas};
use crate::render::scene::{Scene, SceneOptions};
use crate::rocket::{Event, Vessel};
use crate::ui::hud::{draw_flight_hud, draw_result, HudInput};
use crate::ui::menus::{draw_pad, draw_pause, launch_sites, PadAction, PadState, PauseAction};
use crate::world::{Blueprint, PartKind, System};

/// Physics sub-steps per second of simulated time.
pub const SIM_HZ: f64 = 120.0;
/// Available time-warp factors.
pub const WARP_STEPS: [f64; 8] = [1.0, 2.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0];

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Screen {
    /// Rocket assembly / launch pad.
    Pad,
    Flying,
    Paused,
    /// Terminal state: crashed, or landed safely.
    Result { good: bool },
}

pub struct Game {
    pub sys: System,
    pub canvas: Canvas,
    pub cam: Camera,
    pub scene: Scene,
    pub fx: Effects,
    pub pad: PadState,
    pub screen: Screen,
    pub vessel: Option<Vessel>,
    /// Mission elapsed time, seconds.
    pub t: f64,
    pub warp_index: usize,
    pub show_help: bool,
    pub message: String,
    pub message_age: f64,
    pub screenshot_request: bool,
    pub quit: bool,
    pub result_lines: Vec<String>,
    pub result_title: String,
    pub frames: u64,
    pub fps: f32,
    pub sim_hz: f32,
    pub show_trajectory: bool,
    /// Held keys (continuous input).
    pub input_up: bool,
    pub input_down: bool,
    pub input_left: bool,
    pub input_right: bool,
    pub mouse_pos: (i32, i32),
    pub mouse_down: bool,
    pub mouse_right_down: bool,
    pub dragging_cam: bool,
    mouse_clicked: bool,
}

impl Game {
    pub fn new(w: i32, h: i32) -> Game {
        let mut cam = Camera::new(V2::ZERO, 0.01);
        cam.snap();
        Game {
            sys: System::solar(),
            canvas: Canvas::new(w, h),
            cam,
            scene: Scene::new(),
            fx: Effects::new(),
            pad: PadState::new(),
            screen: Screen::Pad,
            vessel: None,
            t: 0.0,
            warp_index: 0,
            show_help: false,
            message: String::new(),
            message_age: 99.0,
            screenshot_request: false,
            quit: false,
            result_lines: Vec::new(),
            result_title: String::new(),
            frames: 0,
            fps: 60.0,
            sim_hz: SIM_HZ as f32,
            show_trajectory: true,
            input_up: false,
            input_down: false,
            input_left: false,
            input_right: false,
            mouse_pos: (0, 0),
            mouse_down: false,
            mouse_right_down: false,
            dragging_cam: false,
            mouse_clicked: false,
        }
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.canvas.resize(w, h);
    }

    pub fn warp(&self) -> f64 {
        WARP_STEPS[self.warp_index.min(WARP_STEPS.len() - 1)]
    }

    pub fn set_message(&mut self, m: impl Into<String>) {
        self.message = m.into();
        self.message_age = 0.0;
    }

    /// Build the vehicle and place it on the pad at the selected site.
    pub fn launch(&mut self) {
        if let Err(e) = self.pad.blueprint.validate() {
            self.pad.set_message(format!("CANNOT LAUNCH: {}", e));
            return;
        }
        let bp = self.pad.blueprint.clone();
        let site = self.pad.body;
        let theta = 0.35;
        self.t = 0.0;
        self.warp_index = 0;
        self.fx.clear();
        let v = crate::rocket::on_pad(&bp, &self.sys, site, theta, 0.0);
        let b = &self.sys.bodies[site];
        let surf = self.sys.position(site, 0.0) + V2::polar(b.radius, theta);
        self.cam = Camera::new(surf + V2::polar(b.radius * 0.06, theta), 0.02);
        self.cam.snap();
        self.vessel = Some(v);
        self.screen = Screen::Flying;
        self.set_message("LIFTOFF! PRESS W TO THROTTLE UP");
    }

    /// Return to the pad.
    pub fn revert(&mut self) {
        self.vessel = None;
        self.screen = Screen::Pad;
        self.t = 0.0;
        self.fx.clear();
    }

    // -----------------------------------------------------------------
    // Simulation
    // -----------------------------------------------------------------

    /// Advance by `dt` real seconds.
    pub fn tick(&mut self, dt: f64) {
        self.frames += 1;
        if dt > 0.0 {
            let inst = 1.0 / dt;
            self.fps = self.fps * 0.92 + (inst as f32) * 0.08;
        }
        self.message_age += dt;
        match self.screen {
            Screen::Pad => {}
            Screen::Paused => {}
            Screen::Flying => self.step_flight(dt),
            Screen::Result { .. } => {
                self.fx.update(dt.min(0.05) as f32);
                self.cam.update(dt);
            }
        }
    }

    fn step_flight(&mut self, dt: f64) {
        if self.vessel.is_none() {
            return;
        }
        let warp = self.warp();
        // Total simulated time covered by this frame, then split into steps of
        // at most ~1/50 s. The step count is capped so that even 1000x warp
        // costs a bounded amount of work per frame; at that speed the vessel is
        // in space, where the larger steps are still accurate enough.
        let sim_total = dt * warp;
        let steps = (sim_total * 50.0).ceil().clamp(1.0, 240.0) as i32;
        let sub_dt = sim_total / steps as f64;
        self.sim_hz = (sim_total / dt.max(1e-6)).round() as f32;

        let rotate = if self.input_right {
            1.0
        } else if self.input_left {
            -1.0
        } else {
            0.0
        };

        let mut events: Vec<Event> = Vec::new();
        for _ in 0..steps {
            let (up, down) = (self.input_up, self.input_down);
            let t = self.t;
            let sys = &self.sys;
            let Some(v) = self.vessel.as_mut() else {
                break;
            };
            if up {
                v.throttle = (v.throttle + sub_dt * 0.9).min(1.0);
            }
            if down {
                v.throttle = (v.throttle - sub_dt * 0.9).max(0.0);
            }
            events.extend(v.step_warp(sys, t, sub_dt, rotate, warp));
            self.t += sub_dt;
            if v.crashed {
                break;
            }
        }

        // Effects.
        if let Some(v) = &self.vessel {
            self.fx.exhaust(v.pos, v.angle, v.throttle, dt as f32, v.density as f32);
            if v.density > 0.02 && v.vel.len() > 1200.0 && !v.on_ground {
                let intensity = (v.density as f32)
                    * ((v.vel.len() as f32 - 1200.0) / 5000.0).clamp(0.0, 1.0);
                self.fx.reentry(v.pos, v.vel, intensity, dt as f32);
                if intensity > 0.3 {
                    self.cam.shake = self.cam.shake.max((intensity * 0.8) as f64);
                }
            }
        }
        self.fx.update(dt.min(0.05) as f32);

        // Camera.
        if self.cam.follow {
            if let Some(v) = &self.vessel {
                let up = V2::polar(v.height() * 0.5, v.angle);
                self.cam.look_at(v.pos + up);
                let want = (self.canvas.h as f64 * 0.06) / v.height().max(2.0);
                let limit = self.auto_zoom_limit();
                self.cam.set_zoom(want.min(limit));
            }
        }
        self.cam.update(dt);

        for ev in events {
            self.handle_event(ev);
        }
    }

    /// Keep a body's radius at least ~30 px so surfaces are not faked at a
    /// resolution where they would look obviously wrong.
    fn auto_zoom_limit(&self) -> f64 {
        match &self.vessel {
            Some(v) => (self.canvas.h as f64 * 0.5) / self.sys.bodies[v.reference].radius.max(1.0),
            None => 1.0,
        }
    }

    fn handle_event(&mut self, ev: Event) {
        match ev {
            Event::Liftoff => self.set_message("LIFTOFF"),
            Event::Staged(n) => self.set_message(format!("STAGE {} SEPARATION", n)),
            Event::ChuteDeploy => self.set_message("PARACHUTE DEPLOYED"),
            Event::ChuteFull => self.set_message("PARACHUTE FULLY OPEN"),
            Event::ThrottleUp | Event::ThrottleDown => {}
            Event::SasOn => self.set_message("SAS ENGAGED"),
            Event::SasOff => self.set_message("SAS DISENGAGED"),
            Event::FuelOut => self.set_message("STAGE OUT OF FUEL - PRESS SPACE TO STAGE"),
            Event::Landed { speed } => {
                let pos = self.vessel.as_ref().map(|v| v.pos).unwrap_or(V2::ZERO);
                let body = self
                    .vessel
                    .as_ref()
                    .map(|v| self.sys.bodies[v.reference].name)
                    .unwrap_or("BODY");
                let fuel = self.vessel.as_ref().map(|v| v.fuel_mass()).unwrap_or(0.0);
                self.fx.dust(pos, 0.1, 3.0);
                self.result_title = "TOUCHDOWN".into();
                self.result_lines = vec![
                    format!("Landed on {} at {:.1} m/s.", body, speed),
                    format!("Mission time: {}", fmt_time(self.t)),
                    format!("Fuel remaining: {:.0} kg", fuel),
                ];
                self.screen = Screen::Result { good: true };
            }
            Event::Crashed { speed, reason } => {
                let pos = self.vessel.as_ref().map(|v| v.pos).unwrap_or(V2::ZERO);
                self.fx.explosion(pos, 1.4);
                self.cam.shake = 2.0;
                self.result_title = "VEHICLE DESTROYED".into();
                self.result_lines = vec![
                    format!("{} at {:.1} m/s.", reason, speed),
                    format!("Mission time: {}", fmt_time(self.t)),
                    "Revert and try again with a gentler descent.".into(),
                ];
                self.screen = Screen::Result { good: false };
            }
        }
    }

    // -----------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------

    pub fn key_down(&mut self, key: Key, down: bool) {
        match key {
            Key::Up => self.input_up = down,
            Key::Down => self.input_down = down,
            Key::Left => self.input_left = down,
            Key::Right => self.input_right = down,
            _ => {}
        }
    }

    pub fn key_pressed(&mut self, key: Key) {
        match self.screen {
            Screen::Pad => self.pad_key(key),
            Screen::Flying => self.flight_key(key),
            Screen::Paused => match key {
                Key::Escape | Key::Pause => self.screen = Screen::Flying,
                Key::Revert => self.revert(),
                Key::Quit => self.quit = true,
                Key::Help => self.show_help = !self.show_help,
                Key::Screenshot => self.screenshot_request = true,
                _ => {}
            },
            Screen::Result { .. } => match key {
                Key::Revert | Key::Enter => self.revert(),
                Key::Escape | Key::Pause => {
                    self.revert();
                }
                Key::Quit => self.quit = true,
                Key::Screenshot => self.screenshot_request = true,
                Key::Help => self.show_help = !self.show_help,
                _ => {}
            },
        }
    }

    fn pad_key(&mut self, key: Key) {
        match key {
            Key::Enter => self.launch(),
            Key::Escape => self.quit = true,
            Key::Help => self.show_help = !self.show_help,
            Key::Screenshot => self.screenshot_request = true,
            Key::Revert => {
                let presets = PadState::preset_list();
                self.pad.preset = (self.pad.preset + 1) % presets.len();
                self.pad.blueprint = presets[self.pad.preset].clone();
                let name = self.pad.blueprint.name.clone();
                self.pad.set_message(format!("LOADED PRESET: {}", name));
            }
            Key::CycleBody => {
                let sites = launch_sites(&self.sys);
                if !sites.is_empty() {
                    let cur = sites.iter().position(|&i| i == self.pad.body).unwrap_or(0);
                    self.pad.body = sites[(cur + 1) % sites.len()];
                    let name = self.sys.bodies[self.pad.body].name;
                    self.pad.set_message(format!("LAUNCH SITE: {}", name));
                }
            }
            _ => {}
        }
    }

    fn flight_key(&mut self, key: Key) {
        // Messages and whole-`self` actions are deferred so the vessel borrow
        // stays local.
        let mut msg: Option<String> = None;
        let mut screen: Option<Screen> = None;
        let mut revert = false;
        let mut quit = false;
        let mut shot = false;
        let mut warp_delta: i32 = 0;
        let mut puff = false;

        if let Some(v) = self.vessel.as_mut() {
            match key {
                Key::Z => {
                    v.throttle = 1.0;
                    msg = Some("THROTTLE 100%".into());
                }
                Key::X => {
                    v.throttle = 0.0;
                    msg = Some("THROTTLE CUT".into());
                }
                Key::Stage => match v.stage() {
                    Some(n) => {
                        puff = true;
                        msg = Some(format!("STAGE {} SEPARATION", n));
                    }
                    None => msg = Some("NO STAGE TO SEPARATE".into()),
                },
                Key::Chute => {
                    if v.has(PartKind::Chute) {
                        v.chute = !v.chute;
                        msg = Some(if v.chute {
                            "PARACHUTE ARMED".into()
                        } else {
                            "PARACHUTE DISARMED".into()
                        });
                    } else {
                        msg = Some("NO PARACHUTE FITTED".into());
                    }
                }
                Key::Sas => {
                    v.sas = !v.sas;
                    msg = Some(if v.sas {
                        "SAS ENGAGED".into()
                    } else {
                        "SAS DISENGAGED".into()
                    });
                }
                Key::Pause | Key::Escape => screen = Some(Screen::Paused),
                Key::Focus => {
                    self.cam.follow = !self.cam.follow;
                    msg = Some(
                        if self.cam.follow {
                            "CAMERA: FOLLOW"
                        } else {
                            "CAMERA: FREE (RIGHT-DRAG TO PAN)"
                        }
                        .into(),
                    );
                }
                Key::CycleBody => {
                    let cur = v.reference;
                    let sites = launch_sites(&self.sys);
                    let next = sites.iter().copied().find(|&i| i != cur).unwrap_or(cur);
                    self.cam.follow = false;
                    self.cam.look_at(self.sys.position(next, self.t));
                    self.cam.snap();
                    msg = Some(format!("FOCUS: {}", self.sys.bodies[next].name));
                }
                Key::Revert => revert = true,
                Key::Help => self.show_help = !self.show_help,
                Key::Screenshot => shot = true,
                Key::WarpUp => warp_delta = 1,
                Key::WarpDown => warp_delta = -1,
                Key::Quit => quit = true,
                _ => {}
            }
        }

        if puff {
            if let Some(v) = &self.vessel {
                self.fx.puff(v.pos, v.angle);
            }
        }
        if warp_delta != 0 {
            let n = WARP_STEPS.len() as i32 - 1;
            self.warp_index = (self.warp_index as i32 + warp_delta).clamp(0, n) as usize;
            msg = Some(format!("TIME WARP x{:.0}", self.warp()));
        }
        if let Some(s) = screen {
            self.screen = s;
        }
        if shot {
            self.screenshot_request = true;
        }
        if quit {
            self.quit = true;
        }
        if revert {
            self.revert();
        } else if let Some(m) = msg {
            self.set_message(m);
        }
    }

    pub fn mouse_move(&mut self, x: i32, y: i32) {
        let dx = x - self.mouse_pos.0;
        let dy = y - self.mouse_pos.1;
        self.mouse_pos = (x, y);
        if self.dragging_cam {
            self.cam.follow = false;
            self.cam.center.x -= dx as f64 / self.cam.ppu;
            self.cam.center.y += dy as f64 / self.cam.ppu;
            self.cam.target_center = self.cam.center;
        }
    }

    pub fn mouse_button(&mut self, down: bool) {
        if down && !self.mouse_down {
            self.mouse_clicked = true;
        }
        self.mouse_down = down;
    }

    pub fn mouse_right_button(&mut self, down: bool) {
        self.mouse_right_down = down;
        self.dragging_cam = down && self.screen != Screen::Pad;
        if down {
            self.cam.follow = false;
        }
    }

    pub fn wheel(&mut self, ticks: i32) {
        self.cam.zoom_by(1.25f64.powi(ticks));
    }

    /// Consume a pending screenshot request (the platform layer writes the
    /// PNG, so the simulation stays free of file IO policy).
    pub fn take_screenshot_request(&mut self) -> bool {
        let r = self.screenshot_request;
        self.screenshot_request = false;
        r
    }

    // -----------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------

    pub fn draw(&mut self) {
        let clicked = self.mouse_clicked;
        self.mouse_clicked = false;
        let mouse = self.mouse_pos;
        match self.screen {
            Screen::Pad => {
                let action = draw_pad(
                    &mut self.canvas,
                    &mut self.pad,
                    &self.sys,
                    &mut self.cam,
                    &self.scene,
                    mouse,
                    clicked,
                    1.0 / 60.0,
                );
                match action {
                    PadAction::Launch => self.launch(),
                    PadAction::AddPart(i) => {
                        let id = crate::world::parts()[i].id;
                        self.pad.add(id);
                    }
                    PadAction::RemovePart => self.pad.remove(),
                    PadAction::Clear => {
                        self.pad.blueprint.parts.clear();
                        self.pad.set_message("STACK CLEARED");
                    }
                    PadAction::WindowClose => self.quit = true,
                    PadAction::CycleBody | PadAction::CyclePreset | PadAction::None => {}
                }
            }
            Screen::Flying | Screen::Result { .. } => {
                let good = matches!(self.screen, Screen::Result { good: true });
                let is_result = matches!(self.screen, Screen::Result { .. });
                self.draw_flight();
                if is_result {
                    let title = self.result_title.clone();
                    let lines = self.result_lines.clone();
                    draw_result(
                        &mut self.canvas,
                        &title,
                        &lines,
                        good,
                        "PRESS R TO RETURN TO THE PAD",
                    );
                }
            }
            Screen::Paused => {
                self.draw_flight();
                if draw_pause(&mut self.canvas, mouse, clicked) == PauseAction::Quit {
                    self.quit = true;
                }
            }
        }
    }

    fn draw_flight(&mut self) {
        let Some(v) = self.vessel.as_ref() else {
            self.canvas.clear(rgb(0, 0, 0));
            return;
        };
        let opts = SceneOptions {
            show_trajectory: self.show_trajectory,
            show_labels: true,
            terrain_detail: true,
        };
        self.canvas.clear(rgb(0, 0, 0));
        self.scene.draw_background(&mut self.canvas, &self.cam);
        self.scene
            .draw_system(&mut self.canvas, &self.cam, &self.sys, self.t, v, &opts);
        self.scene.draw_effects(&mut self.canvas, &self.cam, &self.fx);
        let hud = HudInput {
            sys: &self.sys,
            vessel: v,
            t: self.t,
            warp: self.warp(),
            paused: false,
            message: &self.message,
            message_age: self.message_age,
            show_help: self.show_help,
            fps: self.fps,
            sim_hz: self.sim_hz,
            sas_locked: false,
        };
        draw_flight_hud(&mut self.canvas, &hud);
    }

    /// One-line status used by the window title and CI logs.
    pub fn status_line(&self) -> String {
        match (&self.screen, &self.vessel) {
            (Screen::Pad, _) => format!(
                "Assembly: {} ({} parts, {})",
                self.pad.blueprint.name,
                self.pad.blueprint.parts.len(),
                self.sys.bodies[self.pad.body].name
            ),
            (Screen::Flying, Some(v)) => {
                let alt = v.altitude(&self.sys, v.reference, self.t);
                format!(
                    "{} | alt {} | speed {} | {}",
                    self.sys.bodies[v.reference].name,
                    fmt_dist(alt),
                    fmt_speed(v.vel.len()),
                    fmt_time(self.t)
                )
            }
            (Screen::Paused, _) => "Paused".into(),
            (Screen::Result { good }, Some(v)) => {
                if *good {
                    format!("Landed on {}", self.sys.bodies[v.reference].name)
                } else {
                    "Vehicle destroyed".into()
                }
            }
            _ => "Ready".into(),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Z,
    X,
    Stage,
    Chute,
    Sas,
    Pause,
    Escape,
    Focus,
    CycleBody,
    Revert,
    Help,
    Screenshot,
    WarpUp,
    WarpDown,
    Enter,
    Quit,
}

impl Key {
    /// Map a Win32 virtual-key code to a game key.
    pub fn from_vk(vk: u32) -> Option<Key> {
        Some(match vk {
            0x26 | 0x57 => Key::Up,        // VK_UP, W
            0x28 | 0x53 => Key::Down,      // VK_DOWN, S
            0x25 | 0x41 => Key::Left,      // VK_LEFT, A
            0x27 | 0x44 => Key::Right,     // VK_RIGHT, D
            0x5A => Key::Z,
            0x58 => Key::X,
            0x20 => Key::Stage,            // SPACE
            0x43 => Key::Chute,            // C
            0x54 => Key::Sas,              // T
            0x50 => Key::Pause,            // P
            0x1B => Key::Escape,           // ESC
            0x46 => Key::Focus,            // F
            0x4D => Key::CycleBody,        // M
            0x52 => Key::Revert,           // R
            0x70 => Key::Help,             // F1
            0x71 => Key::Screenshot,       // F2
            0xBB | 0x6B => Key::WarpUp,    // = / numpad +
            0xBD | 0x6D => Key::WarpDown,  // - / numpad -
            0x0D => Key::Enter,            // ENTER
            _ => return None,
        })
    }

    /// Keys reported as held rather than as one-shot presses.
    pub fn is_held(&self) -> bool {
        matches!(self, Key::Up | Key::Down | Key::Left | Key::Right)
    }
}

/// Fly a scripted ascent for `seconds` of simulated time. Used by the CI
/// screenshot generator and by the integration tests.
pub fn scripted_ascent(game: &mut Game, seconds: f64) -> Vec<Event> {
    game.pad.blueprint = Blueprint::presets()[2].clone();
    game.pad.body = 1;
    game.launch();
    if let Some(v) = game.vessel.as_mut() {
        v.throttle = 1.0;
    }
    let mut events = Vec::new();
    let dt = 1.0 / SIM_HZ;
    let steps = (seconds * SIM_HZ) as usize;
    for k in 0..steps {
        let t = game.t;
        let sys = &game.sys;
        let Some(v) = game.vessel.as_mut() else {
            break;
        };
        events.extend(v.step(sys, t, dt, 0.0));
        game.t += dt;
        if v.crashed {
            break;
        }
        // Stage when the current stage runs dry, like a simple autopilot.
        if k % 60 == 0 && v.stage_fuel(v.current_stage) <= 0.0 && v.can_stage() {
            if let Some(n) = v.stage() {
                events.push(Event::Staged(n));
            }
        }
    }
    game.fx.update(0.05);
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_mapping_covers_the_documented_controls() {
        let checks = [
            (0x57u32, Key::Up),
            (0x53, Key::Down),
            (0x41, Key::Left),
            (0x44, Key::Right),
            (0x20, Key::Stage),
            (0x43, Key::Chute),
            (0x54, Key::Sas),
            (0x52, Key::Revert),
            (0x70, Key::Help),
            (0x71, Key::Screenshot),
            (0x1B, Key::Escape),
            (0x0D, Key::Enter),
        ];
        for (vk, k) in checks {
            assert_eq!(Key::from_vk(vk), Some(k), "vk {vk:#x}");
        }
        assert_eq!(Key::from_vk(0x99), None);
        assert!(Key::Up.is_held());
        assert!(!Key::Stage.is_held());
    }

    #[test]
    fn launch_from_pad_creates_a_vessel() {
        let mut g = Game::new(640, 400);
        assert_eq!(g.screen, Screen::Pad);
        g.launch();
        assert_eq!(g.screen, Screen::Flying);
        let v = g.vessel.as_ref().expect("vessel");
        assert!(v.parts.len() > 3);
        let alt = v.altitude(&g.sys, v.reference, g.t);
        assert!(alt.abs() < 2.0, "should start on the pad, alt {alt}");
    }

    #[test]
    fn cannot_launch_an_invalid_stack() {
        let mut g = Game::new(640, 400);
        g.pad.blueprint = Blueprint::new("EMPTY", &[]);
        g.launch();
        assert_eq!(g.screen, Screen::Pad);
        assert!(g.pad.message.contains("CANNOT LAUNCH"));
    }

    #[test]
    fn full_ascent_reaches_orbit_altitude() {
        let mut g = Game::new(1024, 700);
        scripted_ascent(&mut g, 120.0);
        let v = g.vessel.as_ref().expect("vessel");
        let alt = v.altitude(&g.sys, v.reference, g.t);
        assert!(!v.crashed, "crashed during ascent: {:?}", g.result_lines);
        assert!(alt > 40_000.0, "altitude after 120 s: {alt}");
        assert!(v.vel.len() > 900.0, "speed {}", v.vel.len());
    }

    #[test]
    fn draw_works_on_every_screen() {
        let mut g = Game::new(800, 600);
        g.draw();
        assert!(g.canvas.px.iter().any(|p| *p != 0));
        g.launch();
        if let Some(v) = g.vessel.as_mut() {
            v.throttle = 1.0;
        }
        for _ in 0..60 {
            g.tick(1.0 / 60.0);
        }
        g.draw();
        assert!(g.canvas.px.iter().any(|p| *p != 0));
        g.key_pressed(Key::Pause);
        assert_eq!(g.screen, Screen::Paused);
        g.draw();
        g.key_pressed(Key::Escape);
        assert_eq!(g.screen, Screen::Flying);
        g.key_pressed(Key::Revert);
        assert_eq!(g.screen, Screen::Pad);
        // Result screen.
        g.launch();
        if let Some(v) = g.vessel.as_mut() {
            v.crashed = true;
        }
        g.handle_event(Event::Crashed {
            speed: 120.0,
            reason: "IMPACT SPEED TOO HIGH",
        });
        assert!(matches!(g.screen, Screen::Result { good: false }));
        g.draw();
        g.key_pressed(Key::Revert);
        assert_eq!(g.screen, Screen::Pad);
    }

    #[test]
    fn resize_keeps_the_canvas_valid() {
        let mut g = Game::new(320, 240);
        g.resize(1280, 720);
        assert_eq!(g.canvas.w, 1280);
        assert_eq!(g.canvas.h, 720);
        g.draw();
        g.resize(1, 1);
        g.draw();
    }

    #[test]
    fn wheel_and_pan_only_affect_the_camera() {
        let mut g = Game::new(640, 480);
        let before = g.cam.ppu;
        g.wheel(2);
        assert!(g.cam.target_ppu > before);
        g.launch();
        g.mouse_right_button(true);
        assert!(g.dragging_cam);
        assert!(!g.cam.follow);
        g.mouse_move(100, 100);
        g.mouse_move(140, 60);
        g.mouse_right_button(false);
        assert!(g.cam.center.x.is_finite());
    }

    #[test]
    fn throttle_keys_change_throttle() {
        let mut g = Game::new(640, 400);
        g.launch();
        g.key_pressed(Key::Z);
        assert_eq!(g.vessel.as_ref().unwrap().throttle, 1.0);
        g.key_pressed(Key::X);
        assert_eq!(g.vessel.as_ref().unwrap().throttle, 0.0);
        g.key_down(Key::Up, true);
        for _ in 0..30 {
            g.tick(1.0 / 60.0);
        }
        g.key_down(Key::Up, false);
        assert!(g.vessel.as_ref().unwrap().throttle > 0.2);
    }

    #[test]
    fn warp_and_time_advance() {
        let mut g = Game::new(640, 400);
        g.launch();
        g.key_pressed(Key::WarpUp);
        g.key_pressed(Key::WarpUp);
        assert_eq!(g.warp(), 5.0);
        let t0 = g.t;
        g.tick(1.0 / 60.0);
        assert!((g.t - t0) > 0.01, "time warp should advance faster than real time");
    }
}
