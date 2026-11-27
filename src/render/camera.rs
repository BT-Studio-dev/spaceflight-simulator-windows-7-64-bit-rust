//! The view transform: a centred camera with a log-scale zoom (pixels per
//! metre), smoothed so that following a vessel feels like tracking the real
//! thing rather than a locked-on HUD.

use crate::math::{clamp, V2};

/// Smallest zoom: ~1e-11 px/m shows a whole solar system in ~1500 px.
pub const MIN_PPU: f64 = 2e-12;
/// Largest zoom: 40 px/m means the rocket tower fills the screen.
pub const MAX_PPU: f64 = 40.0;

#[derive(Clone)]
pub struct Camera {
    pub center: V2,
    pub ppu: f64,
    pub target_center: V2,
    pub target_ppu: f64,
    /// Seconds between camera updates; larger = snappier.
    pub responsiveness: f64,
    pub shake: f64,
    pub follow: bool,
}

impl Camera {
    pub fn new(center: V2, ppu: f64) -> Camera {
        Camera {
            center,
            ppu,
            target_center: center,
            target_ppu: ppu,
            responsiveness: 6.0,
            shake: 0.0,
            follow: true,
        }
    }

    pub fn snap(&mut self) {
        self.center = self.target_center;
        self.ppu = self.target_ppu;
    }

    pub fn look_at(&mut self, p: V2) {
        self.target_center = p;
    }

    pub fn zoom_by(&mut self, factor: f64) {
        self.target_ppu = clamp(self.target_ppu * factor, MIN_PPU, MAX_PPU);
    }

    pub fn set_zoom(&mut self, ppu: f64) {
        self.target_ppu = clamp(ppu, MIN_PPU, MAX_PPU);
    }

    /// Zoom keeping the world point under the screen point fixed is not very
    /// useful for a follow camera, so zooming always targets screen centre.
    pub fn update(&mut self, dt: f64) {
        let k = (self.responsiveness * dt).min(1.0);
        // Interpolate in log space for zoom so it feels linear while wheeling.
        let l0 = self.ppu.max(1e-300).ln();
        let l1 = self.target_ppu.max(1e-300).ln();
        let nl = l0 + (l1 - l0) * k;
        self.ppu = nl.exp();
        self.center = self.center.mix(self.target_center, k);
        self.shake *= (1.0 - (dt * 6.0).min(1.0)).max(0.0);
    }

    #[inline]
    pub fn world_to_screen(&self, p: V2, w: i32, h: i32) -> (f32, f32) {
        let dx = (p.x - self.center.x) * self.ppu;
        let dy = (p.y - self.center.y) * self.ppu;
        let sx = w as f64 * 0.5 + dx + self.shake * 3.0;
        let sy = h as f64 * 0.5 - dy;
        (sx as f32, sy as f32)
    }

    #[inline]
    pub fn screen_to_world(&self, x: f32, y: f32, w: i32, h: i32) -> V2 {
        let dx = (x as f64 - w as f64 * 0.5) / self.ppu;
        let dy = (h as f64 * 0.5 - y as f64) / self.ppu;
        V2::new(self.center.x + dx, self.center.y + dy)
    }

    /// Metres per pixel.
    #[inline]
    pub fn mpp(&self) -> f64 {
        1.0 / self.ppu
    }

    /// World-space radius of the visible area.
    pub fn visible_radius(&self, w: i32, h: i32) -> f64 {
        let half = ((w * w + h * h) as f64).sqrt() * 0.5;
        half / self.ppu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_round_trips() {
        let cam = Camera::new(V2::new(1.0e6, -2.0e6), 0.5);
        let (w, h) = (800, 600);
        for &p in &[
            V2::new(0.0, 0.0),
            V2::new(1.0e6, -2.0e6),
            V2::new(1.0e6 + 100.0, -2.0e6 + 50.0),
        ] {
            let (sx, sy) = cam.world_to_screen(p, w, h);
            let back = cam.screen_to_world(sx, sy, w, h);
            assert!((back - p).len() < 1e-3, "{:?} -> {:?}", p, back);
        }
        // World +Y is up on screen.
        let (_, y0) = cam.world_to_screen(V2::new(1.0e6, -2.0e6), w, h);
        let (_, y1) = cam.world_to_screen(V2::new(1.0e6, -2.0e6 + 100.0), w, h);
        assert!(y1 < y0, "increasing Y must go up the screen");
    }

    #[test]
    fn zoom_is_clamped_and_smoothed() {
        let mut cam = Camera::new(V2::ZERO, 1.0);
        for _ in 0..200 {
            cam.zoom_by(1.3);
        }
        assert!(cam.target_ppu <= MAX_PPU);
        for _ in 0..400 {
            cam.zoom_by(0.7);
        }
        assert!(cam.target_ppu >= MIN_PPU);
        cam.update(0.016);
        assert!(cam.ppu.is_finite() && cam.ppu > 0.0);
        cam.snap();
        assert_eq!(cam.ppu, cam.target_ppu);
    }

    #[test]
    fn follow_smoothing_converges() {
        let mut cam = Camera::new(V2::ZERO, 1.0);
        cam.look_at(V2::new(1000.0, 500.0));
        for _ in 0..600 {
            cam.update(1.0 / 60.0);
        }
        assert!((cam.center - V2::new(1000.0, 500.0)).len() < 0.5);
    }
}
