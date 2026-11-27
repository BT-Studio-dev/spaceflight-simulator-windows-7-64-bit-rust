//! Draws the world: space background, celestials with mottled surfaces,
//! atmospheres, ground terrain, the rocket, effects and trajectory lines.
//!
//! Two rendering regimes keep the game believable at every zoom level:
//!
//! * **Disc mode** (`r_px < 800`): the body is far enough that it fits on
//!   screen, so it is drawn as a lit sphere with a limb, atmosphere halo and
//!   surface mottling.
//! * **Ground mode** (`r_px >= 800`): the body is so large that its curvature
//!   is invisible; the surface is drawn as a layered terrain silhouette in
//!   world space, which is what you see standing on the pad or climbing out.
//!
//! Both modes derive their geometry from real world coordinates, so there is
//! no visible jump when a vessel crosses the threshold (the vessel is a
//! sub-pixel dot at that zoom either way, and is drawn as a fixed-size icon).

use crate::fx::{Effects, Kind};
use crate::math::{clamp01f, smoothstep, V2};
use crate::noise::fbm;
use crate::render::camera::Camera;
use crate::render::canvas::{lerp_color, rgb, shade, Canvas, Color};
use crate::rng::{hash01, Rng};
use crate::rocket::Vessel;
use crate::world::{BodyKind, PartKind, System};

/// Screen radius above which a body is rendered as ground rather than a disc.
const DISC_MAX_PX: f32 = 800.0;

/// Convert a float screen coordinate to an `i32` without ever overflowing.
/// `as` casts saturate, but arithmetic on the saturated value could still
/// overflow, so callers get a value bounded by a few screens' worth of pixels.
#[inline]
fn px(v: f32) -> i32 {
    let v = if v.is_finite() { v } else { 0.0 };
    v.clamp(-1.0e6, 1.0e6) as i32
}

pub struct SceneOptions {
    /// Draw the planned trajectory and the apsis markers.
    pub show_trajectory: bool,
    /// Draw body labels and orbit rings.
    pub show_labels: bool,
    /// Draw terrain relief.
    pub terrain_detail: bool,
}

impl Default for SceneOptions {
    fn default() -> Self {
        SceneOptions {
            show_trajectory: true,
            show_labels: true,
            terrain_detail: true,
        }
    }
}

pub struct Scene {
    starfield: Vec<(f32, f32, f32, Color)>,
    nebula: Vec<(f32, f32, f32, u32)>,
}

impl Scene {
    pub fn new() -> Scene {
        let mut rng = Rng::new(0x5EED_1234);
        let mut starfield = Vec::with_capacity(460);
        for _ in 0..460 {
            let x = rng.f32();
            let y = rng.f32();
            let br = rng.f32();
            let tint = rng.f32();
            let c = if tint > 0.86 {
                rgb(190, 210, 255)
            } else if tint < 0.12 {
                rgb(255, 226, 190)
            } else {
                rgb(238, 242, 255)
            };
            starfield.push((x, y, 0.25 + br * 0.75, c));
        }
        let mut nebula = Vec::with_capacity(16);
        for _ in 0..16 {
            nebula.push((
                rng.f32(),
                rng.f32(),
                rng.range_f32(0.06, 0.20),
                rng.next_u32(),
            ));
        }
        Scene {
            starfield,
            nebula,
        }
    }

    /// Space background: gradient, faint nebulae and a parallax star field.
    pub fn draw_background(&self, cv: &mut Canvas, cam: &Camera) {
        cv.vgradient(0, cv.h, rgb(4, 6, 14), rgb(1, 2, 6));
        for (nx, ny, nr, seed) in &self.nebula {
            let cx = nx * cv.w as f32;
            let cy = ny * cv.h as f32;
            let r = nr * cv.w as f32;
            let c = match seed % 3 {
                0 => rgb(40, 30, 80),
                1 => rgb(20, 45, 70),
                _ => rgb(55, 25, 55),
            };
            cv.glow(cx, cy, r, c, 0.5);
        }
        // Stars drift slowly with zoom so the background never feels frozen.
        let par = (cam.ppu.log10() * 9.0) as f32;
        for (x, y, b, c) in &self.starfield {
            let sx = (x * cv.w as f32 + par * 5.0).rem_euclid(cv.w as f32 + 8.0) - 4.0;
            let sy = (y * cv.h as f32 + par * 2.0).rem_euclid(cv.h as f32 + 8.0) - 4.0;
            if *b > 0.92 {
                cv.blend(sx as i32, sy as i32, *c, *b);
                cv.blend(sx as i32 + 1, sy as i32, *c, *b * 0.45);
                cv.blend(sx as i32, sy as i32 + 1, *c, *b * 0.45);
                cv.blend(sx as i32 + 1, sy as i32 + 1, *c, *b * 0.45);
            } else {
                cv.blend(sx as i32, sy as i32, *c, *b);
            }
        }
    }

    pub fn draw_system(
        &self,
        cv: &mut Canvas,
        cam: &Camera,
        sys: &System,
        t: f64,
        vessel: &Vessel,
        opts: &SceneOptions,
    ) {
        let w = cv.w;
        let h = cv.h;
        let mut order: Vec<usize> = (0..sys.len()).collect();
        order.sort_by(|a, b| {
            sys.bodies[*b]
                .radius
                .partial_cmp(&sys.bodies[*a].radius)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Orbit rings first, so bodies draw over them.
        if opts.show_labels {
            for i in 0..sys.len() {
                let b = &sys.bodies[i];
                if b.parent == crate::world::NO_PARENT || b.kind == BodyKind::Star {
                    continue;
                }
                let parent = sys.position(b.parent, t);
                let (px, py) = cam.world_to_screen(parent, w, h);
                let r = (b.orbit_a * cam.ppu) as f32;
                if r > 6.0 && r < 30_000.0 {
                    cv.ring(px, py, r - 1.0, r + 1.0, rgb(90, 130, 180), 0.22);
                }
            }
        }

        for &i in &order {
            let b = &sys.bodies[i];
            let tb = std::time::Instant::now();
            let p = sys.position(i, t);
            let (sx, sy) = cam.world_to_screen(p, w, h);
            let r_px = (b.radius * cam.ppu) as f32;
            crate::trace::trace(&format!(
                "  body {} start sx={:.0} sy={:.0} r={:.0}",
                b.name, sx, sy, r_px
            ));
            let margin = 400.0;
            let visible = sx > -margin && sx < w as f32 + margin && sy > -margin && sy < h as f32 + margin;

            if !visible && r_px < 30.0 {
                if opts.show_labels && b.kind != BodyKind::Star {
                    let mx = sx.clamp(8.0, w as f32 - 8.0);
                    let my = sy.clamp(8.0, h as f32 - 8.0);
                    cv.disc(mx, my, 3.0, rgb(150, 200, 255), 0.85);
                    cv.text_shadow(mx as i32 + 6, my as i32 - 4, b.name, 1, rgb(170, 210, 255));
                }
                continue;
            }

            crate::trace::trace(&format!("  body {} mode", b.name));
            match b.kind {
                BodyKind::Star => {
                    crate::trace::trace("    star: disc");
                    self.draw_star(cv, sx, sy, r_px)
                }
                _ if r_px >= DISC_MAX_PX => {
                    crate::trace::trace("    ground mode");
                    if opts.terrain_detail {
                        self.draw_ground(cv, cam, sys, i, t, sy, r_px);
                    } else {
                        self.draw_disc(cv, cam, sys, i, sx, sy, r_px);
                    }
                }
                _ => {
                    crate::trace::trace("    disc mode");
                    self.draw_disc(cv, cam, sys, i, sx, sy, r_px)
                }
            }

            crate::trace::trace(&format!(
                "  body {} done in {:?} (r={:.0}px)",
                b.name,
                tb.elapsed(),
                r_px
            ));
            if opts.show_labels && r_px < 300.0 && b.kind != BodyKind::Star {
                let ly = (sy - r_px.max(5.0) - 14.0) as i32;
                if ly > 20 && ly < h - 20 {
                    let tw = crate::font::text_width(b.name, 1);
                    cv.text_shadow(sx as i32 - tw / 2, ly, b.name, 1, rgb(200, 220, 245));
                }
            }
        }

        if opts.show_trajectory {
            self.draw_orbit_prediction(cv, cam, sys, t, vessel);
        }
        self.draw_vessel(cv, cam, sys, t, vessel);
    }

    // -----------------------------------------------------------------
    // Bodies
    // -----------------------------------------------------------------

    fn draw_star(&self, cv: &mut Canvas, sx: f32, sy: f32, r: f32) {
        let r = r.max(1.5);
        if r < 400.0 {
            cv.glow(sx, sy, r * 6.0, rgb(255, 170, 80), 0.32);
            cv.glow(sx, sy, r * 2.2, rgb(255, 220, 150), 0.45);
        }
        crate::trace::trace("      star disc");
        cv.disc(sx, sy, r, rgb(255, 246, 214), 1.0);
        crate::trace::trace("      star disc done");
        if r > 10.0 && r < 400.0 {
            let mut rng = Rng::new(0x5017);
            let n = ((r * r) / 24.0).clamp(20.0, 700.0) as i32;
            for _ in 0..n {
                let a = rng.range(0.0, std::f64::consts::TAU);
                let d = rng.range(0.0, 1.0).sqrt() * r as f64 * 0.95;
                let px = sx + (a.cos() * d) as f32;
                let py = sy + (a.sin() * d) as f32;
                cv.blend(px as i32, py as i32, rgb(255, 190, 110), 0.22);
            }
            cv.ring(sx, sy, r * 0.99, r * 1.08, rgb(255, 200, 120), 0.45);
        }
    }

    /// Draw a body as a lit sphere with surface mottling.
    fn draw_disc(
        &self,
        cv: &mut Canvas,
        cam: &Camera,
        sys: &System,
        i: usize,
        sx: f32,
        sy: f32,
        r: f32,
    ) {
        let b = &sys.bodies[i];
        let r = r.max(1.0);
        let pad = 8.0f32;
        if sx + r + pad < 0.0
            || sy + r + pad < 0.0
            || sx - r - pad > cv.w as f32
            || sy - r - pad > cv.h as f32
        {
            return;
        }

        // Atmosphere: a soft halo when small, a thin ring when large, so the
        // cost stays bounded no matter how big the disc gets.
        if b.atmo_rho0 > 0.0 {
            let (ar, ag, ab) = b.atmo_color;
            let ac = rgb(ar, ag, ab);
            let halo = ((b.atmo_height * cam.ppu) as f32).max(1.5);
            if r < 220.0 {
                cv.glow(sx, sy, r + halo * 2.0, ac, 0.22);
                cv.ring(sx, sy, r, r + halo, ac, 0.30);
            } else {
                cv.ring(sx, sy, r, r + halo.min(30.0), ac, 0.30);
            }
        }

        let lo = {
            let (a, c, d) = b.color_lo;
            rgb(a, c, d)
        };
        let hi = {
            let (a, c, d) = b.color_hi;
            rgb(a, c, d)
        };

        draw_lit_sphere(cv, sx, sy, r, lo, hi, b.mottle);

        if b.kind == BodyKind::Moon {
            let mut rng = Rng::new(b.seed as u64 ^ 0xABCD_1234);
            let n = ((r / 5.0) as i32).clamp(4, 90);
            for _ in 0..n {
                let a = rng.range(0.0, std::f64::consts::TAU);
                let d = rng.range(0.0, 1.0).sqrt() * r as f64 * 0.9;
                let cx = sx + (a.cos() * d) as f32;
                let cy = sy + (a.sin() * d) as f32;
                let cr = rng.range_f32(1.0, (r * 0.16).max(2.0));
                cv.disc(cx, cy, cr, shade(lo, 1.35), 0.30);
                cv.disc(cx, cy + cr * 0.3, cr * 0.75, shade(lo, 0.62), 0.30);
            }
        }
        // Limb darkening.
        if r > 14.0 {
            cv.ring(sx, sy, r - 1.5, r, rgb(0, 0, 0), 0.45);
        }
    }

    /// Ground mode: the surface fills the lower screen with layered terrain.
    #[allow(clippy::too_many_arguments)]
    fn draw_ground(
        &self,
        cv: &mut Canvas,
        cam: &Camera,
        sys: &System,
        i: usize,
        t: f64,
        cy: f32,
        r_px: f32,
    ) {
        let b = &sys.bodies[i];
        let w = cv.w;
        let h = cv.h;
        let mpp = cam.mpp();
        let lo = {
            let (a, c, d) = b.color_lo;
            rgb(a, c, d)
        };
        let hi = {
            let (a, c, d) = b.color_hi;
            rgb(a, c, d)
        };

        // Screen row of the nominal surface directly below the body centre.
        let surface_row = cy - r_px;
        if surface_row > h as f32 + 4.0 {
            return; // whole body is below the screen
        }
        let spin = b.spin(t);

        // Base fill: everything below the surface row is ground. `top` is
        // clamped to the canvas: when the camera is far above the surface the
        // row lands far off screen and an unclamped loop would walk millions
        // of off-screen rows.
        let top = px(surface_row.max(-1.0)).max(0);
        if top < h {
            for y in top..h {
                let depth = (y as f32 - surface_row).max(0.0);
                // Slight vertical gradient so the ground is not flat colour.
                let g = lerp_color(lo, shade(lo, 0.55), smoothstep(0.0, h as f64 * 1.2, depth as f64) as f32);
                cv.fill_rect(0, y, w, 1, g);
            }
        }

        let t_base = std::time::Instant::now();
        crate::trace::trace(&format!(
            "    {} base fill done {:?} surface_row={:.0} r_px={:.0}",
            b.name,
            t_base.elapsed(),
            surface_row,
            r_px
        ));
        // Local terrain: two noise layers in *screen space*, anchored to the
        // body so hills scroll past realistically while flying.
        if b.terrain > 0.0 && surface_row < h as f32 + 4.0 {
            let base_angle = (cam.center - sys.position(i, t)).angle();
            // Metres per pixel along the surface; terrain wavelength scales
            // with the visible span so hills stay visible at every altitude.
            let span_m = (w as f64 * mpp).max(1.0);
            let wavelength = (span_m * 0.35).max(b.terrain * 3.0);
            let amp_far = (b.terrain * 0.9 * cam.ppu).min(h as f64 * 0.55) as f32;
            let amp_near = (b.terrain * 0.45 * cam.ppu).min(h as f64 * 0.35) as f32;

            let draw_layer = |cv: &mut Canvas, amp: f32, seed_off: u32, tint: f32, base_row: f32| {
                if amp < 1.5 {
                    return;
                }
                let mut prev: Option<(i32, i32)> = None;
                for x in 0..w {
                    let u = (x as f64 - w as f64 * 0.5) * mpp;
                    let n = fbm(
                        (u / wavelength) as f32 + (spin * 13.0) as f32,
                        seed_off as f32 * 3.7,
                        4,
                        b.seed ^ seed_off,
                        0,
                    );
                    let ridged = 1.0 - (n * 2.0 - 1.0).abs();
                    let hgt = (ridged * 0.55 + n * 0.45) * amp;
                    let y = base_row - hgt;
                    let col = lerp_color(lo, hi, (0.35 + tint * 0.5).clamp(0.0, 1.0));
                    let col = shade(col, tint);
                    let yc = px(y);
                    if let Some((prev_x, prev_y)) = prev {
                        // Fill the vertical gap between neighbouring columns so
                        // the silhouette is watertight.
                        let (ya, yb) = if prev_y < yc { (prev_y, yc) } else { (yc, prev_y) };
                        let xa = prev_x.min(x);
                        let xb = prev_x.max(x);
                        for yy in ya..=yb {
                            cv.fill_rect(xa, yy, (xb - xa + 1).max(1), 1, col);
                        }
                    }
                    if yc < h {
                        cv.fill_rect(x, yc, 1, h - yc, col);
                    }
                    prev = Some((x, yc));
                }
            };

            // Far range (darker, higher), then near range on top.
            crate::trace::trace(&format!("    {} terrain layer 1", b.name));
            draw_layer(cv, amp_far, 0x9E37_79B9, 0.75, surface_row + 4.0);
            crate::trace::trace(&format!("    {} terrain layer 2", b.name));
            draw_layer(cv, amp_near, 0x51ED_2701, 1.0, surface_row + h as f32 * 0.06);
            let _ = base_angle;
        }

        crate::trace::trace(&format!("    {} terrain done", b.name));
        // Surface rocks / speckles for close zoom.
        if b.terrain > 0.0 && cam.ppu > 0.02 {
            let points = 220;
            for k in 0..points {
                let r1 = hash01(k, 11, b.seed);
                let r2 = hash01(k, 37, b.seed);
                let x = px(r1 * w as f32);
                let y = px(surface_row + 4.0 + r2 * (h as f32 - surface_row).max(4.0));
                if y >= 0 && y < h {
                    let size = 0.7 + r1 * 1.8;
                    let c = shade(lerp_color(lo, hi, r2), 0.8 + r1 * 0.5);
                    cv.disc(x as f32, y as f32, size, c, 0.55);
                }
            }
        }

        // Atmosphere haze band at the top of the ground.
        if b.atmo_rho0 > 0.0 {
            let (ar, ag, ab) = b.atmo_color;
            let band = ((b.atmo_height * cam.ppu) as f32).min(h as f32 * 0.9).max(6.0);
            for k in 0..24 {
                let fy = k as f32 / 24.0;
                let a = (1.0 - fy) * 0.16;
                cv.blend_rect(
                    0,
                    px(surface_row - band * (1.0 - fy)),
                    w,
                    px((band / 24.0).ceil() + 1.0).max(1),
                    rgb(ar, ag, ab),
                    a,
                );
            }
        }
    }

    // -----------------------------------------------------------------
    // Trajectory
    // -----------------------------------------------------------------

    fn draw_orbit_prediction(
        &self,
        cv: &mut Canvas,
        cam: &Camera,
        sys: &System,
        t: f64,
        v: &Vessel,
    ) {
        if v.crashed || v.parts.is_empty() {
            return;
        }
        let i = v.reference;
        let b = &sys.bodies[i];
        let mu = b.gm;
        let (dr, _) = v.rel(sys, i, t);
        if !dr.is_finite() || dr.len() <= 0.0 {
            return;
        }
        // Integrate a two-body prediction forward and draw it as a dotted line.
        let mut p = v.pos;
        let mut vel = v.vel;
        let dt = 1.0;
        let bound = v.orbit(sys, i, t).apo.is_finite();
        let color = if bound {
            rgb(120, 230, 160)
        } else {
            rgb(255, 180, 110)
        };
        let mut prev = cam.world_to_screen(p, cv.w, cv.h);
        for k in 0..600 {
            let tk = t + (k as f64) * dt;
            let bp = sys.position(i, tk);
            let d = bp - p;
            let r2 = d.len_sq().max(1.0);
            let acc = d * (mu / (r2 * r2.sqrt()));
            vel += acc * dt;
            p += vel * dt;
            if !p.is_finite() {
                break;
            }
            let s = cam.world_to_screen(p, cv.w, cv.h);
            if k % 5 == 0 {
                cv.line_alpha(
                    prev.0 as i32,
                    prev.1 as i32,
                    s.0 as i32,
                    s.1 as i32,
                    color,
                    0.5,
                );
            }
            prev = s;
            if (p - bp).len() < b.radius {
                break; // impact
            }
            if k > 30 && (p - sys.position(i, t)).len() > b.radius * 200.0 {
                break;
            }
        }

        // Apsis markers.
        let o = v.orbit(sys, i, t);
        if o.valid {
            let dir = dr.norm();
            mark_apsis(
                cv,
                cam,
                sys.position(i, t) + dir * o.apo,
                rgb(120, 210, 255),
                "APO",
            );
            mark_apsis(
                cv,
                cam,
                sys.position(i, t) + dir * o.peri,
                rgb(255, 200, 120),
                "PERI",
            );
        }
    }

    // -----------------------------------------------------------------
    // Vessel
    // -----------------------------------------------------------------

    pub fn draw_vessel(&self, cv: &mut Canvas, cam: &Camera, sys: &System, t: f64, v: &Vessel) {
        if v.parts.is_empty() {
            return;
        }
        let (bx, by) = cam.world_to_screen(v.pos, cv.w, cv.h);
        let mpp = cam.mpp();
        let px_height = (v.height() / mpp) as f32;

        // Far away the stack is sub-pixel: draw a fixed-size icon instead so
        // the player can always see where they are.
        if px_height < 9.0 {
            self.draw_vessel_icon(cv, bx, by, v, sys, t);
            return;
        }

        let up = (v.angle.sin() as f32, -v.angle.cos() as f32);
        let right = (up.1, -up.0);
        let total_px = px_height;
        let mut s = 0.0f64;
        for (idx, p) in v.parts.iter().enumerate() {
            let s0 = s;
            let s1 = s + p.def.height;
            s = s1;
            let wpx = ((p.def.width / mpp) as f32).max(2.0);
            let base_pad = if idx == 0 { (wpx * 0.12).max(1.0) } else { 0.0 };
            let y0 = (s0 / mpp) as f32 + base_pad;
            let y1 = (s1 / mpp) as f32 + base_pad;
            if y0 > total_px + 20_000.0 {
                continue;
            }
            let corner = |x: f32, y: f32| -> (f32, f32) {
                (bx + up.0 * y + right.0 * x, by + up.1 * y + right.1 * x)
            };
            let hw = wpx * 0.5;
            let quad = [
                corner(-hw, y0),
                corner(hw, y0),
                corner(hw, y1),
                corner(-hw, y1),
            ];
            let (fill, dark) = part_colors(p.def.kind);
            cv.polygon(&quad, fill, 1.0);
            for k in 0..4 {
                let a = quad[k];
                let c = quad[(k + 1) % 4];
                cv.line(a.0 as i32, a.1 as i32, c.0 as i32, c.1 as i32, dark);
            }
            match p.def.kind {
                PartKind::Command => {
                    let c = corner(0.0, (y0 + y1) * 0.5);
                    cv.disc(c.0, c.1, (wpx * 0.13).max(1.0), rgb(150, 220, 255), 0.9);
                }
                PartKind::Tank => {
                    let c = corner(0.0, (y0 + y1) * 0.5);
                    cv.disc(c.0, c.1, (wpx * 0.07).max(0.8), dark, 0.5);
                }
                PartKind::Nose => {
                    let apex = corner(0.0, y1 + wpx * 0.8);
                    let l = corner(-hw, y1);
                    let r = corner(hw, y1);
                    cv.polygon(&[l, r, apex], fill, 1.0);
                    cv.line(l.0 as i32, l.1 as i32, apex.0 as i32, apex.1 as i32, dark);
                    cv.line(r.0 as i32, r.1 as i32, apex.0 as i32, apex.1 as i32, dark);
                }
                PartKind::Legs => {
                    let l = corner(-hw * 1.7, y0 - wpx * 0.35);
                    let l2 = corner(-hw, y0 + wpx * 0.9);
                    let r = corner(hw * 1.7, y0 - wpx * 0.35);
                    let r2 = corner(hw, y0 + wpx * 0.9);
                    cv.line(l2.0 as i32, l2.1 as i32, l.0 as i32, l.1 as i32, dark);
                    cv.line(r2.0 as i32, r2.1 as i32, r.0 as i32, r.1 as i32, dark);
                }
                PartKind::Chute => {
                    if v.chute_frac > 0.02 {
                        let bell = (28.0 * (v.chute_frac as f32).sqrt()).max(4.0);
                        let top = corner(0.0, y1 + bell);
                        let l = corner(-bell, y1 + 2.0);
                        let r = corner(bell, y1 + 2.0);
                        cv.polygon(&[l, r, top], rgb(242, 242, 248), 0.92);
                        cv.line(l.0 as i32, l.1 as i32, top.0 as i32, top.1 as i32, rgb(210, 210, 222));
                        cv.line(r.0 as i32, r.1 as i32, top.0 as i32, top.1 as i32, rgb(210, 210, 222));
                    }
                }
                PartKind::Engine | PartKind::Booster => {
                    let nl = corner(-hw * 1.3, y0 - wpx * 0.2);
                    let nr = corner(hw * 1.3, y0 - wpx * 0.2);
                    let tl = corner(-hw * 0.6, y0 + wpx * 0.25);
                    let tr = corner(hw * 0.6, y0 + wpx * 0.25);
                    cv.polygon(&[tl, tr, nr, nl], dark, 1.0);
                    let firing = v.throttle > 0.02
                        && p.stage == v.current_stage
                        && (p.fuel > 0.0 || p.def.kind == PartKind::Booster);
                    if firing {
                        let c = corner(0.0, y0 - wpx * 0.25);
                        let len = ((30.0 + 60.0 * v.throttle as f32) / mpp as f32 * 6.0).clamp(6.0, 90.0);
                        let flick = 0.85 + 0.15 * (t * 40.0).sin() as f32;
                        cv.glow(
                            c.0 + up.0 * len * 0.4,
                            c.1 + up.1 * len * 0.4,
                            len * 0.55 * flick,
                            rgb(255, 190, 110),
                            0.55,
                        );
                    }
                }
            }
        }
    }

    /// Fixed-size icon used when the vessel would be smaller than a few pixels.
    fn draw_vessel_icon(
        &self,
        cv: &mut Canvas,
        bx: f32,
        by: f32,
        v: &Vessel,
        sys: &System,
        t: f64,
    ) {
        let up = (v.angle.sin() as f32, -v.angle.cos() as f32);
        let right = (up.1, -up.0);
        let len = 9.0;
        let wdt = 3.4;
        let tip = (bx + up.0 * len, by + up.1 * len);
        let l = (bx - right.0 * wdt, by - right.1 * wdt);
        let r = (bx + right.0 * wdt, by + right.1 * wdt);
        cv.polygon(&[l, r, tip], rgb(228, 234, 244), 0.95);
        cv.line(l.0 as i32, l.1 as i32, r.0 as i32, r.1 as i32, rgb(120, 130, 145));
        if v.throttle > 0.05 {
            cv.glow(bx - up.0 * 5.0, by - up.1 * 5.0, 7.0, rgb(255, 180, 90), 0.6);
        }
        // Velocity arrow for orientation cues.
        if v.vel.len() > 1.0 {
            let d = v.vel.norm();
            cv.line_alpha(
                bx as i32 + 10,
                by as i32,
                (bx + d.x as f32 * 22.0) as i32,
                (by - d.y as f32 * 22.0) as i32,
                rgb(120, 255, 160),
                0.55,
            );
        }
        let _ = (sys, t);
    }

    pub fn draw_effects(&self, cv: &mut Canvas, cam: &Camera, fx: &Effects) {
        for p in &fx.particles {
            let (sx, sy) = cam.world_to_screen(p.pos, cv.w, cv.h);
            if sx < -40.0 || sy < -40.0 || sx > cv.w as f32 + 40.0 || sy > cv.h as f32 + 40.0 {
                continue;
            }
            let t = (p.life / p.max_life).clamp(0.0, 1.0);
            let a = match p.kind {
                Kind::Debris => t.sqrt(),
                Kind::Dust => t * 0.4,
                _ => t,
            };
            cv.disc(sx, sy, p.size.max(0.7), p.color, a.min(1.0));
        }
    }
}

impl Default for Scene {
    fn default() -> Self {
        Scene::new()
    }
}

fn mark_apsis(cv: &mut Canvas, cam: &Camera, point: V2, c: Color, label: &str) {
    if !point.is_finite() {
        return;
    }
    let (sx, sy) = cam.world_to_screen(point, cv.w, cv.h);
    if sx < -50.0 || sy < -50.0 || sx > cv.w as f32 + 50.0 || sy > cv.h as f32 + 50.0 {
        return;
    }
    cv.disc(sx, sy, 3.0, c, 0.9);
    cv.text_shadow(sx as i32 + 6, sy as i32 - 3, label, 1, c);
}

fn part_colors(kind: PartKind) -> (Color, Color) {
    match kind {
        PartKind::Command => (rgb(228, 234, 242), rgb(140, 150, 165)),
        PartKind::Tank => (rgb(208, 213, 221), rgb(120, 128, 140)),
        PartKind::Engine => (rgb(122, 126, 134), rgb(58, 60, 68)),
        PartKind::Booster => (rgb(234, 228, 212), rgb(120, 110, 95)),
        PartKind::Chute => (rgb(240, 240, 246), rgb(150, 150, 160)),
        PartKind::Legs => (rgb(152, 157, 167), rgb(80, 82, 90)),
        PartKind::Nose => (rgb(232, 122, 92), rgb(130, 60, 45)),
    }
}

/// Sphere with directional lighting, a mottled surface and soft edges.
fn draw_lit_sphere(
    cv: &mut Canvas,
    sx: f32,
    sy: f32,
    r: f32,
    lo: Color,
    hi: Color,
    mottle: f32,
) {
    // Clip to the canvas: a body can be thousands of pixels in radius when the
    // camera is close, and only the on-screen part must be rasterised.
    let x0 = ((sx - r - 1.0).floor() as i64).max(0) as i32;
    let x1 = ((sx + r + 1.0).ceil() as i64).min(cv.w as i64 - 1) as i32;
    let y0 = ((sy - r - 1.0).floor() as i64).max(0) as i32;
    let y1 = ((sy + r + 1.0).ceil() as i64).min(cv.h as i64 - 1) as i32;
    if x0 > x1 || y0 > y1 {
        return;
    }
    let inv = 1.0 / r.max(1e-6);
    // Mottling is evaluated on a coarse lattice (the pattern is smooth), which
    // keeps large discs fast.
    let cell = (r / 28.0).max(2.0);
    let cells = 28.max(2);
    let mut texture = vec![0f32; (cells * cells) as usize];
    for cy in 0..cells {
        for cx in 0..cells {
            let n = fbm(
                (cx as f32 + 0.5) * 0.42 * mottle.max(0.3),
                (cy as f32 + 0.5) * 0.42 * mottle.max(0.3),
                3,
                0x7A11_0000 ^ ((cx * 31 + cy * 17) as u32),
                0,
            );
            texture[(cy * cells + cx) as usize] = n;
        }
    }
    let _ = cell;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = (x as f32 + 0.5 - sx) * inv;
            let dy = (y as f32 + 0.5 - sy) * inv;
            let d2 = dx * dx + dy * dy;
            if d2 > 1.0 {
                continue;
            }
            let nz = (1.0 - d2).max(0.0).sqrt();
            let light = (-0.42 * dx - 0.42 * dy + 0.8 * nz).clamp(0.0, 1.5);
            let light = 0.16 + light * 0.95;
            let gx = (((dx + 1.0) * 0.5 * cells as f32) as i32).clamp(0, cells - 1);
            let gy = (((dy + 1.0) * 0.5 * cells as f32) as i32).clamp(0, cells - 1);
            let n = texture[(gy * cells + gx) as usize];
            let base = lerp_color(lo, hi, n.clamp(0.0, 1.0));
            let c = shade(base, light);
            let rim = clamp01f((1.0 - d2.sqrt()) * 7.0);
            let c = lerp_color(rgb(0, 0, 0), c, 0.30 + 0.70 * rim);
            let cov = ((1.0 - d2.sqrt()) * r + 0.5).clamp(0.0, 1.0);
            cv.blend(x, y, c, cov);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Blueprint;

    fn setup() -> (System, Vessel, Camera) {
        let sys = System::solar();
        let bp = Blueprint::new("SCOUT", &["eng_colt", "tank_small", "pod_probe", "nose_cone"]);
        let v = crate::rocket::in_orbit(&bp, &sys, 1, 5000.0, 0.4, 0.0);
        let mut cam = Camera::new(v.pos, 0.002);
        cam.snap();
        (sys, v, cam)
    }

    #[test]
    fn renders_at_several_zooms_without_panicking() {
        let (sys, v, mut cam) = setup();
        let scene = Scene::new();
        let fx = Effects::new();
        for ppu in [3e-12, 1e-9, 1e-6, 1e-3, 0.01, 0.5, 4.0, 30.0] {
            cam.set_zoom(ppu);
            cam.snap();
            cam.look_at(v.pos);
            cam.snap();
            let mut cv = Canvas::new(320, 200);
            scene.draw_background(&mut cv, &cam);
            scene.draw_system(&mut cv, &cam, &sys, 0.0, &v, &SceneOptions::default());
            scene.draw_effects(&mut cv, &cam, &fx);
            assert!(cv.px.iter().any(|p| *p != 0), "blank frame at ppu {ppu}");
        }
    }

    #[test]
    fn ground_mode_fills_the_lower_screen_on_the_pad() {
        let sys = System::solar();
        let bp = Blueprint::new("SCOUT", &["eng_colt", "tank_small", "pod_probe", "nose_cone"]);
        let v = crate::rocket::on_pad(&bp, &sys, 1, 0.0, 0.0);
        // Sit well above the hills: this test is about the ground filling the
        // lower screen, not about terrain relief.
        // Note the offset is *towards* the body centre (local down) so the
        // surface is genuinely below the camera.
        let mut cam = Camera::new(v.pos - V2::polar(60_000.0, 0.0), 0.02);
        cam.snap();
        let scene = Scene::new();
        let mut cv = Canvas::new(240, 180);
        cv.clear(rgb(0, 0, 0));
        scene.draw_system(&mut cv, &cam, &sys, 0.0, &v, &SceneOptions::default());
        // The lower part of the screen is ground; the top-left corner is sky.
        let bottom = cv.px[170 * 240 + 120];
        assert_ne!(bottom, 0, "no ground drawn at the pad");
        let top_left = cv.px[2 * 240 + 2];
        assert_eq!(top_left, 0, "sky should stay empty before the background pass");
        // Ground must be a broad band, not a single row.
        let rows = (0..180)
            .filter(|y| cv.px[y * 240 + 120] != 0)
            .count();
        assert!(rows > 40, "ground band too thin: {rows} rows");
    }

    #[test]
    fn disc_mode_draws_a_round_planet() {
        let sys = System::solar();
        let bp = Blueprint::new("SCOUT", &["eng_colt", "tank_small", "pod_probe", "nose_cone"]);
        let v = crate::rocket::in_orbit(&bp, &sys, 1, 50_000.0, 0.0, 0.0);
        let mut cam = Camera::new(sys.position(1, 0.0), 0.0008);
        cam.snap();
        let scene = Scene::new();
        let mut cv = Canvas::new(400, 400);
        cv.clear(rgb(0, 0, 0));
        scene.draw_system(&mut cv, &cam, &sys, 0.0, &v, &SceneOptions::default());
        let centre = cv.px[200 * 400 + 200];
        assert_ne!(centre, 0, "planet centre is blank");
        let corner = cv.px[2 * 400 + 2];
        assert_eq!(corner, 0, "outside the planet should be untouched");
    }
}
