//! Small, dependency-free math helpers: 2D vectors in f64 plus formatting.
//!
//! All simulation math is done in SI units (metres, seconds, kilograms). The
//! world is a single plane, so positions are `V2` in f64 which keeps sub-metre
//! precision even for Neptune-distance coordinates.

use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

pub const G: f64 = 6.674_30e-11;
/// Standard gravity, used for Isp -> exhaust velocity.
pub const G0: f64 = 9.806_65;
pub const TAU: f64 = std::f64::consts::TAU;
pub const PI: f64 = std::f64::consts::PI;

/// Astronomical unit in metres.
pub const AU: f64 = 1.495_978_707e11;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct V2 {
    pub x: f64,
    pub y: f64,
}

impl V2 {
    pub const ZERO: V2 = V2 { x: 0.0, y: 0.0 };

    #[inline]
    pub const fn new(x: f64, y: f64) -> V2 {
        V2 { x, y }
    }

    #[inline]
    pub fn polar(r: f64, angle: f64) -> V2 {
        let (s, c) = angle.sin_cos();
        V2::new(r * c, r * s)
    }

    /// Unit vector pointing along `angle` measured from +X, counter clockwise.
    #[inline]
    pub fn dir(angle: f64) -> V2 {
        let (s, c) = angle.sin_cos();
        V2::new(c, s)
    }

    #[inline]
    pub fn len_sq(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    #[inline]
    pub fn len(self) -> f64 {
        self.len_sq().sqrt()
    }

    #[inline]
    pub fn dot(self, o: V2) -> f64 {
        self.x * o.x + self.y * o.y
    }

    #[inline]
    pub fn cross(self, o: V2) -> f64 {
        self.x * o.y - self.y * o.x
    }

    /// Rotated 90 degrees counter clockwise.
    #[inline]
    pub fn perp(self) -> V2 {
        V2::new(-self.y, self.x)
    }

    #[inline]
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    #[inline]
    pub fn norm(self) -> V2 {
        let l = self.len();
        if l > 0.0 {
            self / l
        } else {
            V2::ZERO
        }
    }

    #[inline]
    pub fn rot(self, angle: f64) -> V2 {
        let (s, c) = angle.sin_cos();
        V2::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }

    #[inline]
    pub fn clamp_len(self, max: f64) -> V2 {
        let l = self.len();
        if l > max && l > 0.0 {
            self * (max / l)
        } else {
            self
        }
    }

    #[inline]
    pub fn mix(self, o: V2, t: f64) -> V2 {
        self + (o - self) * t
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    #[inline]
    pub fn to_f32(self) -> (f32, f32) {
        (self.x as f32, self.y as f32)
    }
}

impl Add for V2 {
    type Output = V2;
    #[inline]
    fn add(self, o: V2) -> V2 {
        V2::new(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for V2 {
    #[inline]
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl Sub for V2 {
    type Output = V2;
    #[inline]
    fn sub(self, o: V2) -> V2 {
        V2::new(self.x - o.x, self.y - o.y)
    }
}

impl SubAssign for V2 {
    #[inline]
    fn sub_assign(&mut self, o: V2) {
        self.x -= o.x;
        self.y -= o.y;
    }
}

impl Mul<f64> for V2 {
    type Output = V2;
    #[inline]
    fn mul(self, s: f64) -> V2 {
        V2::new(self.x * s, self.y * s)
    }
}

impl MulAssign<f64> for V2 {
    #[inline]
    fn mul_assign(&mut self, s: f64) {
        self.x *= s;
        self.y *= s;
    }
}

impl Div<f64> for V2 {
    type Output = V2;
    #[inline]
    fn div(self, s: f64) -> V2 {
        V2::new(self.x / s, self.y / s)
    }
}

impl Neg for V2 {
    type Output = V2;
    #[inline]
    fn neg(self) -> V2 {
        V2::new(-self.x, -self.y)
    }
}

#[inline]
pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[inline]
pub fn clamp01(x: f64) -> f64 {
    clamp(x, 0.0, 1.0)
}

#[inline]
pub fn clamp01f(x: f32) -> f32 {
    if x < 0.0 {
        0.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}

#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[inline]
pub fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = clamp01((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

/// Wrap an angle into `[-PI, PI)`.
#[inline]
pub fn wrap_pi(mut a: f64) -> f64 {
    while a >= PI {
        a -= TAU;
    }
    while a < -PI {
        a += TAU;
    }
    a
}

/// Move `v` towards `target` by at most `step`.
#[inline]
pub fn approach(v: f64, target: f64, step: f64) -> f64 {
    if v < target {
        (v + step).min(target)
    } else {
        (v - step).max(target)
    }
}

pub fn hypot(a: f64, b: f64) -> f64 {
    (a * a + b * b).sqrt()
}

// ---------------------------------------------------------------------------
// Formatting helpers (small allocations are fine: only used for HUD text).
// ---------------------------------------------------------------------------

/// Distance with sensible unit, e.g. `412 m`, `12.4 km`, `0.998 Gm`.
pub fn fmt_dist(m: f64) -> String {
    let a = m.abs();
    let sign = if m < 0.0 { "-" } else { "" };
    if a < 1000.0 {
        format!("{}{:.0} m", sign, a)
    } else if a < 1.0e7 {
        format!("{}{:.1} km", sign, a / 1.0e3)
    } else if a < 1.0e9 {
        format!("{}{:.0} km", sign, a / 1.0e3)
    } else if a < 1.0e12 {
        format!("{}{:.3} Gm", sign, a / 1.0e9)
    } else {
        format!("{}{:.4} Tm", sign, a / 1.0e12)
    }
}

/// Speed with sensible unit.
pub fn fmt_speed(v: f64) -> String {
    let a = v.abs();
    let sign = if v < 0.0 { "-" } else { "" };
    if a < 1000.0 {
        format!("{}{:.1} m/s", sign, a)
    } else {
        format!("{}{:.2} km/s", sign, a / 1.0e3)
    }
}

/// Mission clock, e.g. `T+ 00:12:34` or `T- 00:00:07`.
pub fn fmt_time(sec: f64) -> String {
    let neg = sec < 0.0;
    let mut s = sec.abs().round() as i64;
    let days = s / 86_400;
    s %= 86_400;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    let sign = if neg { "-" } else { "+" };
    if days > 0 {
        format!("T{} {}d {:02}:{:02}:{:02}", sign, days, h, m, sec)
    } else {
        format!("T{} {:02}:{:02}:{:02}", sign, h, m, sec)
    }
}

/// Duration, e.g. `1h 12m`, `45s`.
pub fn fmt_duration(sec: f64) -> String {
    let s = sec.abs();
    if s < 90.0 {
        format!("{:.0}s", s)
    } else if s < 5400.0 {
        format!("{:.0}m", s / 60.0)
    } else if s < 172_800.0 {
        format!("{:.1}h", s / 3600.0)
    } else {
        format!("{:.1}d", s / 86_400.0)
    }
}

pub fn fmt_mass(kg: f64) -> String {
    if kg.abs() < 1000.0 {
        format!("{:.0} kg", kg)
    } else {
        format!("{:.2} t", kg / 1000.0)
    }
}

pub fn fmt_num(x: f64, decimals: usize) -> String {
    format!("{:.*}", decimals, x)
}

/// Clamp a camera-relevant helper: shortest angular difference.
pub fn ang_diff(a: f64, b: f64) -> f64 {
    wrap_pi(b - a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_ops() {
        let a = V2::new(3.0, 4.0);
        assert!((a.len() - 5.0).abs() < 1e-12);
        assert!((a.norm().len() - 1.0).abs() < 1e-12);
        let r = a.rot(PI / 2.0);
        assert!((r.x + 4.0).abs() < 1e-9 && (r.y - 3.0).abs() < 1e-9);
        assert!((a.dot(a) - 25.0).abs() < 1e-12);
    }

    #[test]
    fn wrapping() {
        assert!((wrap_pi(3.0 * PI) - PI).abs() < 1e-9);
        assert!((wrap_pi(-3.0 * PI) + PI).abs() < 1e-9);
        assert_eq!(wrap_pi(0.5), 0.5);
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_dist(412.0), "412 m");
        assert_eq!(fmt_dist(12_400.0), "12.4 km");
        assert_eq!(fmt_speed(2400.0), "2.40 km/s");
        assert_eq!(fmt_time(3723.0), "T+ 01:02:03");
        assert_eq!(fmt_time(-12.0), "T- 00:00:12");
        assert_eq!(fmt_duration(50.0), "50s");
    }
}
