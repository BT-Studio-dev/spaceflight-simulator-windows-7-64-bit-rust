//! A tiny software rasteriser: 32-bit RGB framebuffer plus the handful of
//! primitives needed for the scene and the HUD (Aero-glass panels, text,
//! discs, rings, gradients). No GPU, no external crates.

use crate::font;

/// 0x00RRGGBB, which matches both the PNG writer's expectations and the
/// `0x00BBGGRR` DIB layout after a channel swap in the platform layer.
pub type Color = u32;

#[inline]
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Pixel centre offset used by the rasteriser.
#[inline]
fn dx_of(x: i32, cx: f32) -> f32 {
    x as f32 + 0.5 - cx
}

#[inline]
pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let ar = ((a >> 16) & 0xFF) as f32;
    let ag = ((a >> 8) & 0xFF) as f32;
    let ab = (a & 0xFF) as f32;
    let br = ((b >> 16) & 0xFF) as f32;
    let bg = ((b >> 8) & 0xFF) as f32;
    let bb = (b & 0xFF) as f32;
    rgb(
        (ar + (br - ar) * t) as u8,
        (ag + (bg - ag) * t) as u8,
        (ab + (bb - ab) * t) as u8,
    )
}

#[inline]
pub fn shade(c: Color, f: f32) -> Color {
    let f = f.clamp(0.0, 8.0);
    let r = (((c >> 16) & 0xFF) as f32 * f).min(255.0) as u8;
    let g = (((c >> 8) & 0xFF) as f32 * f).min(255.0) as u8;
    let b = ((c & 0xFF) as f32 * f).min(255.0) as u8;
    rgb(r, g, b)
}

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub px: Vec<Color>,
}

impl Canvas {
    pub fn new(w: i32, h: i32) -> Canvas {
        let w = w.max(1);
        let h = h.max(1);
        Canvas {
            w,
            h,
            px: vec![0; (w * h) as usize],
        }
    }

    /// Resize, keeping the allocation when possible.
    pub fn resize(&mut self, w: i32, h: i32) {
        let w = w.max(1);
        let h = h.max(1);
        if w != self.w || h != self.h {
            self.w = w;
            self.h = h;
            self.px.resize((w * h) as usize, 0);
        }
    }

    #[inline]
    pub fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            Some((y * self.w + x) as usize)
        } else {
            None
        }
    }

    pub fn clear(&mut self, c: Color) {
        for p in self.px.iter_mut() {
            *p = c;
        }
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: Color) {
        if let Some(i) = self.index(x, y) {
            self.px[i] = c;
        }
    }

    /// Source-over blend of `c` with coverage `a` in `[0, 1]`.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: Color, a: f32) {
        if a <= 0.0 {
            return;
        }
        if let Some(i) = self.index(x, y) {
            let d = self.px[i];
            if a >= 1.0 {
                self.px[i] = c;
            } else {
                self.px[i] = lerp_color(d, c, a);
            }
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.w);
        let y1 = (y + h).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for yy in y0..y1 {
            let row = (yy * self.w) as usize;
            for xx in x0..x1 {
                self.px[row + xx as usize] = c;
            }
        }
    }

    pub fn blend_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color, a: f32) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.w);
        let y1 = (y + h).min(self.h);
        if x0 >= x1 || y0 >= y1 || a <= 0.0 {
            return;
        }
        if a >= 1.0 {
            self.fill_rect(x, y, w, h, c);
            return;
        }
        for yy in y0..y1 {
            let row = (yy * self.w) as usize;
            for xx in x0..x1 {
                let i = row + xx as usize;
                self.px[i] = lerp_color(self.px[i], c, a);
            }
        }
    }

    /// Vertical gradient from `top` to `bottom`, exclusive of `y1`.
    pub fn vgradient(&mut self, y0: i32, y1: i32, top: Color, bottom: Color) {
        if y1 <= y0 {
            return;
        }
        let span = (y1 - y0) as f32;
        for y in y0.max(0)..y1.min(self.h) {
            let t = (y - y0) as f32 / span;
            let c = lerp_color(top, bottom, t);
            let row = (y * self.w) as usize;
            for x in 0..self.w {
                self.px[row + x as usize] = c;
            }
        }
    }

    pub fn rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.fill_rect(x, y, w, 1, c);
        self.fill_rect(x, y + h - 1, w, 1, c);
        self.fill_rect(x, y, 1, h, c);
        self.fill_rect(x + w - 1, y, 1, h, c);
    }

    pub fn hline(&mut self, x0: i32, x1: i32, y: i32, c: Color) {
        let (a, b) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        self.fill_rect(a, y, b - a + 1, 1, c);
    }

    pub fn vline(&mut self, x: i32, y0: i32, y1: i32, c: Color) {
        let (a, b) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        self.fill_rect(x, a, 1, b - a + 1, c);
    }

    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Color) {
        self.line_alpha(x0, y0, x1, y1, c, 1.0);
    }

    pub fn line_alpha(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Color, a: f32) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let (mut x, mut y) = (x0, y0);
        let mut guard = 0;
        loop {
            self.blend(x, y, c, a);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
            guard += 1;
            if guard > 100_000 {
                break;
            }
        }
    }

    /// Thick line approximated with parallel 1px lines (used for trajectory).
    pub fn line_thick(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Color, w: i32, a: f32) {
        if w <= 1 {
            self.line_alpha(x0, y0, x1, y1, c, a);
            return;
        }
        for o in -(w / 2)..=(w / 2) {
            self.line_alpha(x0, y0 + o, x1, y1 + o, c, a);
        }
    }

    /// Clamped bounding box of a circle, guaranteed non-empty or `None`.
    ///
    /// This is what keeps a shape with a huge radius from walking millions of
    /// off-screen pixels: the loop is bounded by the canvas, never by `r`.
    #[inline]
    fn clip_box(&self, cx: f32, cy: f32, r: f32) -> Option<(i32, i32, i32, i32)> {
        if r <= 0.0 || !r.is_finite() || !cx.is_finite() || !cy.is_finite() {
            return None;
        }
        let x0 = ((cx - r - 1.0).floor() as i64).max(0) as i32;
        let x1 = ((cx + r + 1.0).ceil() as i64).min(self.w as i64 - 1) as i32;
        let y0 = ((cy - r - 1.0).floor() as i64).max(0) as i32;
        let y1 = ((cy + r + 1.0).ceil() as i64).min(self.h as i64 - 1) as i32;
        if x0 > x1 || y0 > y1 {
            None
        } else {
            Some((x0, y0, x1, y1))
        }
    }

    /// Per-row horizontal span `[xa, xb]` (inclusive, clamped to the canvas)
    /// for a filled circle of radius `r` centred at `(cx, cy)`.
    /// `None` when the row misses the disc entirely.
    #[inline]
    fn disc_row_span(&self, cx: f32, cy: f32, r: f32, y: i32) -> Option<(i32, i32)> {
        let dy = y as f32 + 0.5 - cy;
        // Take the widest half-chord whose pixels are still at least 50 %
        // covered, then add one pixel for the anti-aliased edge.
        let margin = (r * r - dy * dy).max(0.0).sqrt();
        if !margin.is_finite() {
            return None;
        }
        let xa = (cx - margin - 0.5).floor() as i64;
        let xb = (cx + margin + 0.5).ceil() as i64;
        let xa = xa.max(0) as i32;
        let xb = xb.min(self.w as i64 - 1) as i32;
        if xa > xb {
            None
        } else {
            Some((xa, xb))
        }
    }

    /// Filled circle with a soft one-pixel edge.
    ///
    /// Instead of testing every pixel of the bounding box (which is O(r^2) and
    /// catastrophic for a star rendered 14 million pixels across), the fully
    /// covered interior of every row is filled with an optimised span fill and
    /// only the two edge pixels are anti-aliased.
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Color, a: f32) {
        if r <= 0.0 || !r.is_finite() || !cx.is_finite() || !cy.is_finite() {
            return;
        }
        let Some((_, y0, _, y1)) = self.clip_box(cx, cy, r) else {
            return;
        };
        let aa = a.clamp(0.0, 1.0);
        // Interior half-width: pixels inside this are fully covered.
        let inner = (r - 0.5).max(0.0);
        for y in y0..=y1 {
            let dy = y as f32 + 0.5 - cy;
            if dy.abs() > r + 0.5 {
                continue;
            }
            let Some((xa, xb)) = self.disc_row_span(cx, cy, r, y) else {
                continue;
            };
            if aa >= 1.0 {
                if inner > 0.0 {
                    let m = (inner * inner - dy * dy).max(0.0).sqrt();
                    let ix0 = ((cx - m).ceil() as i64).max(0) as i32;
                    let ix1 = ((cx + m).floor() as i64).min(self.w as i64 - 1) as i32;
                    if ix0 <= ix1 {
                        self.fill_rect(ix0, y, ix1 - ix0 + 1, 1, c);
                        // Anti-aliased fringe.
                        for x in [ix0 - 1, ix1 + 1] {
                            if x >= xa && x <= xb {
                                let d = (dx_of(x, cx) * dx_of(x, cx) + dy * dy).sqrt();
                                let cov = (r - d + 0.5).clamp(0.0, 1.0);
                                if cov > 0.0 {
                                    self.blend(x, y, c, cov);
                                }
                            }
                        }
                        continue;
                    }
                }
            }
            // Thin rows, translucent discs and the fringe: per-pixel.
            for x in xa..=xb {
                let dx = x as f32 + 0.5 - cx;
                let d = (dx * dx + dy * dy).sqrt();
                let cov = (r - d + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, c, cov * aa);
                }
            }
        }
    }

    /// Ring (annulus) with soft edges, good for atmospheres and orbit lines.
    pub fn ring(&mut self, cx: f32, cy: f32, r_in: f32, r_out: f32, c: Color, a: f32) {
        if r_out <= 0.0 || r_out <= r_in {
            return;
        }
        let Some((x0, y0, x1, y1)) = self.clip_box(cx, cy, r_out) else {
            return;
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                let cov = (r_out - d + 0.5).clamp(0.0, 1.0) * (d - r_in + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, c, cov * a);
                }
            }
        }
    }

    /// Soft radial glow, brightest at the centre.
    pub fn glow(&mut self, cx: f32, cy: f32, r: f32, c: Color, a: f32) {
        let Some((x0, y0, x1, y1)) = self.clip_box(cx, cy, r) else {
            return;
        };
        let inv = 1.0 / r;
        for y in y0..=y1 {
            let dy = y as f32 + 0.5 - cy;
            if dy.abs() > r {
                continue;
            }
            // Only the chord of the glow circle matters.
            let half = (r * r - dy * dy).max(0.0).sqrt();
            let xa = ((cx - half).floor() as i64).max(x0 as i64) as i32;
            let xb = ((cx + half).ceil() as i64).min(x1 as i64) as i32;
            for x in xa..=xb {
                let dx = x as f32 + 0.5 - cx;
                let d = (dx * dx + dy * dy).sqrt() * inv;
                if d < 1.0 {
                    let fall = (1.0 - d) * (1.0 - d);
                    self.blend(x, y, c, fall * a);
                }
            }
        }
    }

    /// Filled convex/concave polygon via scanline, with optional alpha.
    pub fn polygon(&mut self, pts: &[(f32, f32)], c: Color, a: f32) {
        if pts.len() < 3 {
            return;
        }
        let mut miny = f32::MAX;
        let mut maxy = f32::MIN;
        for p in pts {
            if !p.0.is_finite() || !p.1.is_finite() {
                return;
            }
            miny = miny.min(p.1);
            maxy = maxy.max(p.1);
        }
        let y0 = (miny.floor() as i64).max(0) as i32;
        let y1 = (maxy.ceil() as i64).min(self.h as i64 - 1) as i32;
        if y0 > y1 {
            return;
        }
        let mut xs: Vec<f32> = Vec::with_capacity(8);
        for y in y0..=y1 {
            let sy = y as f32 + 0.5;
            xs.clear();
            for i in 0..pts.len() {
                let (x0, y0p) = pts[i];
                let (x1, y1p) = pts[(i + 1) % pts.len()];
                if (y0p <= sy && y1p > sy) || (y1p <= sy && y0p > sy) {
                    let t = (sy - y0p) / (y1p - y0p);
                    xs.push(x0 + (x1 - x0) * t);
                }
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mut i = 0;
            while i + 1 < xs.len() {
                let xa = xs[i].round() as i32;
                let xb = xs[i + 1].round() as i32;
                for x in xa..xb.max(xa + 1) {
                    self.blend(x, y, c, a);
                }
                i += 2;
            }
        }
    }

    // -----------------------------------------------------------------
    // Text
    // -----------------------------------------------------------------

    pub fn text(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Color) {
        let scale = scale.max(1);
        font::for_each_pixel(s, |gx, gy| {
            if scale == 1 {
                self.put(x + gx, y + gy, c);
            } else {
                self.fill_rect(x + gx * scale, y + gy * scale, scale, scale, c);
            }
        });
    }

    pub fn text_alpha(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Color, a: f32) {
        let scale = scale.max(1);
        font::for_each_pixel(s, |gx, gy| {
            if scale == 1 {
                self.blend(x + gx, y + gy, c, a);
            } else {
                self.blend_rect(x + gx * scale, y + gy * scale, scale, scale, c, a);
            }
        });
    }

    /// Text with a 1px drop shadow, for readability over the scene.
    pub fn text_shadow(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Color) {
        let sh = scale.max(1);
        self.text_alpha(x + sh, y + sh, s, scale, rgb(0, 0, 0), 0.65);
        self.text(x, y, s, scale, c);
    }

    pub fn text_centered(&mut self, cx: i32, y: i32, s: &str, scale: i32, c: Color) {
        let w = font::text_width(s, scale.max(1));
        self.text(cx - w / 2, y, s, scale, c);
    }

    /// Multi-line text block with shadow; returns the height used.
    pub fn text_block(&mut self, x: i32, y: i32, lines: &[String], scale: i32, c: Color) -> i32 {
        let mut yy = y;
        for line in lines {
            self.text_shadow(x, yy, line, scale, c);
            yy += (font::GLYPH_H as i32 + 2) * scale.max(1);
        }
        yy - y
    }

    // -----------------------------------------------------------------
    // Windows 7 Aero-style chrome
    // -----------------------------------------------------------------

    /// Translucent "glass" panel with a light border and inner highlight,
    /// echoing the Aero taskbar / window look of Windows 7.
    pub fn aero_panel(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if w <= 2 || h <= 2 {
            return;
        }
        // Body: vertical gradient of cool grey-blue, translucent.
        for yy in 0..h {
            let t = yy as f32 / h as f32;
            let base = lerp_color(rgb(26, 34, 48), rgb(12, 17, 26), t);
            self.blend_rect(x, y + yy, w, 1, base, 0.80);
        }
        // Glossy top half.
        self.blend_rect(x, y + 1, w - 2, (h / 3).max(2), rgb(255, 255, 255), 0.06);
        // Borders: bright top/left, darker bottom/right (Aero bevel).
        self.hline(x, x + w - 1, y, rgb(150, 190, 230));
        self.hline(x, x + w - 1, y + h - 1, rgb(40, 60, 80));
        self.vline(x, y, y + h - 1, rgb(120, 155, 195));
        self.vline(x + w - 1, y, y + h - 1, rgb(40, 60, 80));
        // Inner hairline.
        self.rect_outline(x + 1, y + 1, w - 2, h - 2, rgb(70, 95, 125));
    }

    /// Small Aero-style key cap, used for the controls legend.
    pub fn keycap(&mut self, x: i32, y: i32, label: &str, scale: i32) {
        let tw = font::text_width(label, scale);
        let w = tw + 8;
        let h = 7 * scale + 6;
        self.blend_rect(x, y, w, h, rgb(60, 80, 105), 0.9);
        self.rect_outline(x, y, w, h, rgb(150, 190, 230));
        self.hline(x + 1, x + w - 2, y + 1, rgb(120, 160, 200));
        self.text(x + 4, y + 3, label, scale, rgb(240, 245, 252));
    }

    /// Horizontal progress/fuel bar with Aero blue fill.
    pub fn bar(&mut self, x: i32, y: i32, w: i32, h: i32, frac: f32, fill: Color, back: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.blend_rect(x, y, w, h, back, 0.85);
        let fw = ((w as f32 - 2.0) * frac.clamp(0.0, 1.0)).round() as i32;
        if fw > 0 {
            self.blend_rect(x + 1, y + 1, fw, h - 2, fill, 0.95);
            // Gloss.
            self.blend_rect(x + 1, y + 1, fw, (h - 2) / 2, rgb(255, 255, 255), 0.20);
        }
        self.rect_outline(x, y, w, h, rgb(20, 30, 42));
        self.hline(x + 1, x + w - 2, y + 1, rgb(120, 150, 190));
    }

    /// Triangle marker (prograde / retrograde symbols on the HUD).
    pub fn marker_triangle(&mut self, cx: i32, cy: i32, r: i32, up: bool, c: Color) {
        let dir = if up { 1 } else { -1 };
        for i in 0..r {
            let w = r - i;
            self.fill_rect(cx - w / 2, cy + dir * i, (w).max(1), 1, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_clips_without_panicking() {
        let mut cv = Canvas::new(64, 48);
        cv.clear(rgb(0, 0, 0));
        cv.put(-5, 10, rgb(255, 0, 0));
        cv.put(1000, 10, rgb(255, 0, 0));
        cv.fill_rect(-10, -10, 40, 40, rgb(10, 20, 30));
        cv.disc(32.0, 24.0, 100.0, rgb(200, 100, 50), 0.5);
        cv.ring(32.0, 24.0, 10.0, 20.0, rgb(255, 255, 255), 0.5);
        cv.line(-10, -10, 100, 100, rgb(255, 255, 0));
        cv.text(-3, -3, "HELLO", 3, rgb(255, 255, 255));
        cv.aero_panel(-5, -5, 50, 40);
        cv.bar(5, 5, 20, 6, 1.5, rgb(0, 120, 215), rgb(0, 0, 0));
        cv.marker_triangle(10, 10, 8, true, rgb(255, 255, 255));
        let mut pts = [(0.0, 0.0), (10.0, 2.0), (5.0, 12.0)];
        cv.polygon(&pts, rgb(1, 2, 3), 0.8);
        pts[0] = (1e6, 1e6);
        cv.polygon(&pts, rgb(1, 2, 3), 0.8);
        assert_eq!(cv.px.len(), (64 * 48) as usize);
    }

    #[test]
    fn blend_matches_expectation() {
        let mut cv = Canvas::new(2, 1);
        cv.clear(rgb(0, 0, 0));
        cv.blend(0, 0, rgb(200, 100, 50), 0.5);
        let p = cv.px[0];
        assert_eq!((p >> 16) & 0xFF, 100);
        assert_eq!((p >> 8) & 0xFF, 50);
        assert_eq!(p & 0xFF, 25);
    }

    #[test]
    fn huge_primitives_are_clipped_and_cheap() {
        // A star that overflows the screen by orders of magnitude must still
        // draw instantly: the loop is bounded by the canvas, not the radius.
        let mut cv = Canvas::new(64, 48);
        cv.clear(0);
        let t0 = std::time::Instant::now();
        // A filled disc much larger than the canvas covers every pixel.
        cv.disc(32.0, 24.0, 1.0e7, rgb(255, 255, 255), 1.0);
        assert!(
            cv.px.iter().all(|p| *p == 0x00FF_FFFF),
            "a disc far larger than the canvas must fill it completely"
        );
        // A ring that would enclose the screen touches no pixel at all.
        cv.clear(0);
        cv.ring(32.0, 24.0, 9_000.0, 9_002.0, rgb(255, 0, 0), 1.0);
        assert!(cv.px.iter().all(|p| *p == 0), "off-screen ring drew pixels");
        cv.glow(32.0, 24.0, 5_000.0, rgb(0, 255, 0), 1.0);
        cv.polygon(
            &[(-1e7, -1e7), (1e7, -1e7), (1e7, 1e7)],
            rgb(0, 0, 255),
            1.0,
        );
        // A triangle whose corners are far off screen must still be clipped.
        cv.clear(0);
        cv.polygon(&[(10.0, 40.0), (200.0, 40.0), (120.0, -900.0)], rgb(9, 9, 9), 1.0);
        assert_eq!(cv.px[20 * 64 + 100], rgb(9, 9, 9), "polygon was not drawn");
        let dt = t0.elapsed();
        assert!(dt.as_millis() < 500, "clipping failed, took {:?}", dt);
    }

    #[test]
    fn colors() {
        assert_eq!(rgb(1, 2, 3), 0x0001_0203);
        assert_eq!(shade(rgb(10, 10, 10), 2.0), rgb(20, 20, 20));
        assert_eq!(shade(rgb(200, 200, 200), 4.0), rgb(255, 255, 255));
        assert_eq!(lerp_color(rgb(0, 0, 0), rgb(255, 255, 255), 0.5), rgb(127, 127, 127));
    }
}
