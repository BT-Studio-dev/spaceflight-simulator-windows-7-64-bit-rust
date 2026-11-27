//! Value noise, fractal Brownian motion and terrain profiles.
//!
//! Used for planet surface maps (generated once at start-up) and for the local
//! terrain height field the vessel lands on.

use crate::rng::hash01;

#[inline]
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in `[0, 1]`. `period` wraps the lattice so textures tile.
///
/// Coordinates are clamped to a very wide but finite range first: callers work
/// in metres (Earth's circumference is ~1.9e7 m, but a view can span billions
/// of metres) and the lattice arithmetic must never overflow an `i32`.
pub fn value_noise(x: f32, y: f32, seed: u32, period: i32) -> f32 {
    let x = if x.is_finite() { x.clamp(-6.0e8, 6.0e8) } else { 0.0 };
    let y = if y.is_finite() { y.clamp(-6.0e8, 6.0e8) } else { 0.0 };
    let xi = x.floor();
    let yi = y.floor();
    let tx = smooth(x - xi);
    let ty = smooth(y - yi);
    let (ix, iy) = (xi as i32, yi as i32);
    let wrap = |v: i32| -> i32 {
        if period > 0 {
            v.rem_euclid(period)
        } else {
            v
        }
    };
    let x0 = wrap(ix);
    let x1 = wrap(ix.wrapping_add(1));
    let y0 = wrap(iy);
    let y1 = wrap(iy.wrapping_add(1));
    let a = hash01(x0, y0, seed);
    let b = hash01(x1, y0, seed);
    let c = hash01(x0, y1, seed);
    let d = hash01(x1, y1, seed);
    let top = a + (b - a) * tx;
    let bot = c + (d - c) * tx;
    top + (bot - top) * ty
}

/// Fractal sum of value noise in `[0, 1]`.
pub fn fbm(x: f32, y: f32, octaves: u32, seed: u32, period: i32) -> f32 {
    let mut total = 0.0;
    let mut amp = 0.5;
    let mut norm = 0.0;
    let mut freq = 1.0;
    for o in 0..octaves {
        let p = if period > 0 {
            (period as f32 * freq).round().max(1.0) as i32
        } else {
            0
        };
        total += amp * value_noise(x * freq, y * freq, seed.wrapping_add(o * 7919), p);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    total / norm
}

/// Ridged fbm, good for mountainous terrain; range `[0, 1]`.
pub fn ridged(x: f32, y: f32, octaves: u32, seed: u32, period: i32) -> f32 {
    let mut total = 0.0;
    let mut amp = 0.5;
    let mut norm = 0.0;
    let mut freq = 1.0;
    for o in 0..octaves {
        let p = if period > 0 {
            (period as f32 * freq).round().max(1.0) as i32
        } else {
            0
        };
        let n = value_noise(x * freq, y * freq, seed.wrapping_add(o * 6151), p);
        let r = 1.0 - (n * 2.0 - 1.0).abs();
        total += amp * r * r;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    total / norm
}

/// Height profile of a body's ground at angle `theta` (radians), scaled by the
/// body's `terrain_scale`. Returns metres above the reference radius.
///
/// The profile is periodic in `theta` so a vessel can drive all the way around.
pub fn terrain_height(theta: f64, seed: u32, scale: f64) -> f64 {
    if scale <= 0.0 {
        return 0.0;
    }
    // Work in a normalised circle of 64 lattice cells circumference.
    let u = (theta / std::f64::consts::TAU) * 64.0;
    let base = fbm(
        u as f32,
        3.7,
        4,
        seed ^ 0x51ed_2701,
        64,
    ) as f64;
    let detail = ridged(u as f32, 11.3, 3, seed ^ 0x2f9a_1f3b, 64) as f64;
    // Bias towards flatter valleys: terrain sticks up in ridges.
    let h = (base - 0.35) * 0.75 + detail * 0.55;
    let h = if h < 0.0 { h * 0.45 } else { h };
    h * scale
}

/// Colour of the ground at angle `theta`, as a linear blend factor used by the
/// renderer to mix between two body colours.
pub fn terrain_tint(theta: f64, seed: u32) -> f32 {
    let u = (theta / std::f64::consts::TAU) * 96.0;
    fbm(u as f32, 21.0, 3, seed ^ 0x77aa_1234, 96).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_range() {
        for i in 0..500 {
            let x = i as f32 * 0.37;
            let v = value_noise(x, x * 1.7, 3, 0);
            assert!((0.0..=1.0).contains(&v), "value noise out of range: {v}");
            let f = fbm(x, x * 0.31, 5, 11, 0);
            assert!((0.0..=1.0).contains(&f), "fbm out of range: {f}");
            let r = ridged(x, x * 0.77, 4, 5, 0);
            assert!((0.0..=1.0).contains(&r), "ridged out of range: {r}");
        }
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        for &v in &[0.0, 1.0, -1.0, 1.0e30, -1.0e30, f32::MAX, f32::MIN, f32::NAN] {
            let n = value_noise(v, v * 0.5, 7, 64);
            assert!(n.is_finite(), "noise at {v} was {n}");
            let f = fbm(v, v, 4, 3, 0);
            assert!(f.is_finite(), "fbm at {v} was {f}");
        }
    }

    #[test]
    fn terrain_is_periodic() {
        let a = terrain_height(0.3, 1234, 800.0);
        let b = terrain_height(0.3 + std::f64::consts::TAU * 3.0, 1234, 800.0);
        // The profile is sampled through f32 noise, so equality is only up to
        // the precision of the lattice coordinate, not bit-exact.
        assert!(
            (a - b).abs() < 1.0,
            "terrain not periodic: {a} vs {b} (delta {})",
            (a - b).abs()
        );
        assert!(terrain_height(1.0, 99, 0.0) == 0.0);
    }
}
