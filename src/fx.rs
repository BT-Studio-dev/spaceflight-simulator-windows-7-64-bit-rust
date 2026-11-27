//! Visual effects: exhaust plume particles, explosion debris, re-entry fire
//! and surface dust. Everything is integrated in world coordinates so the
//! effects stay glued to the scene while the camera moves.

use crate::math::{V2, PI};
use crate::render::canvas::rgb;
use crate::rng::Rng;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Rocket exhaust, short lived, follows the vessel.
    Exhaust,
    /// Explosion debris.
    Debris,
    /// Re-entry plasma streak.
    Fire,
    /// Surface dust kicked up on touchdown.
    Dust,
    /// Small puff emitted by separation motors during staging.
    Puff,
}

#[derive(Copy, Clone)]
pub struct Particle {
    pub pos: V2,
    pub vel: V2,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    pub color: u32,
    pub kind: Kind,
    pub grow: f32,
}

pub struct Effects {
    pub particles: Vec<Particle>,
    rng: Rng,
    exhaust_accum: f32,
    dust_accum: f32,
}

const MAX_PARTICLES: usize = 1400;

impl Effects {
    pub fn new() -> Effects {
        Effects {
            particles: Vec::with_capacity(MAX_PARTICLES),
            rng: Rng::new(0xC0FFEE),
            exhaust_accum: 0.0,
            dust_accum: 0.0,
        }
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    fn push(&mut self, p: Particle) {
        if self.particles.len() >= MAX_PARTICLES {
            // Recycle the oldest particle rather than growing without bound.
            if let Some(slot) = self.particles.iter().position(|q| q.life <= 0.0) {
                self.particles[slot] = p;
            }
            return;
        }
        self.particles.push(p);
    }

    /// Emit exhaust behind a vessel whose base is at `base`, pointing at `angle`.
    pub fn exhaust(&mut self, base: V2, angle: f64, throttle: f64, dt: f32, pressure: f32) {
        if throttle <= 0.01 {
            return;
        }
        self.exhaust_accum += dt * 90.0 * throttle as f32;
        let n = self.exhaust_accum as i32;
        self.exhaust_accum -= n as f32;
        let back = V2::dir(angle + PI);
        let mut rng = self.rng;
        for _ in 0..n {
            let spread = rng.range_f32(-0.18, 0.18);
            let speed = rng.range_f32(90.0, 260.0) * (0.4 + throttle as f32 * 0.8);
            let dir = V2::dir(angle + PI + spread as f64);
            let hot = rng.range_f32(0.0, 1.0);
            let color = if hot > 0.55 {
                rgb(255, 240, 200)
            } else if hot > 0.2 {
                rgb(255, 170, 70)
            } else {
                rgb(210, 90, 40)
            };
            // In thick air the plume billows and greys out.
            let color = if pressure > 0.6 && rng.f32() > 0.6 {
                rgb(210, 200, 190)
            } else {
                color
            };
            self.push(Particle {
                pos: base + back * 1.5 + V2::dir(angle + PI / 2.0) * rng.range(-1.0, 1.0),
                vel: dir * speed as f64 + V2::dir(angle).perp() * rng.range(-20.0, 20.0),
                life: rng.range_f32(0.25, 0.7),
                max_life: 0.7,
                size: rng.range_f32(1.2, 3.2),
                color,
                kind: Kind::Exhaust,
                grow: 2.5,
            });
        }
        self.rng = rng;
    }

    pub fn explosion(&mut self, at: V2, scale: f32) {
        let mut rng = self.rng;
        for _ in 0..90 {
            let a = rng.range(0.0, std::f64::consts::TAU);
            let s = rng.range(20.0, 220.0) * scale as f64;
            let hot = rng.f32();
            let color = if hot > 0.6 {
                rgb(255, 240, 190)
            } else if hot > 0.25 {
                rgb(255, 150, 50)
            } else {
                rgb(120, 70, 50)
            };
            self.push(Particle {
                pos: at + V2::dir(a) * rng.range(0.0, 8.0) * scale as f64,
                vel: V2::dir(a) * s,
                life: rng.range_f32(0.6, 2.2),
                max_life: 2.2,
                size: rng.range_f32(1.5, 4.5) * scale,
                color,
                kind: Kind::Debris,
                grow: 0.4,
            });
        }
        self.rng = rng;
    }

    pub fn reentry(&mut self, at: V2, vel: V2, intensity: f32, dt: f32) {
        let n = (dt * 110.0 * intensity) as i32;
        let mut rng = self.rng;
        for _ in 0..n {
            let c = if rng.f32() > 0.5 {
                rgb(255, 220, 150)
            } else {
                rgb(255, 130, 60)
            };
            self.push(Particle {
                pos: at + V2::dir(rng.range(0.0, std::f64::consts::TAU)) * rng.range(0.0, 12.0),
                vel: vel * rng.range(0.05, 0.25) + V2::dir(rng.range(0.0, std::f64::consts::TAU)) * 30.0,
                life: rng.range_f32(0.2, 0.6),
                max_life: 0.6,
                size: rng.range_f32(1.0, 2.6),
                color: c,
                kind: Kind::Fire,
                grow: 1.5,
            });
        }
        self.rng = rng;
    }

    pub fn dust(&mut self, at: V2, dt: f32, strength: f32) {
        self.dust_accum += dt * 40.0 * strength;
        let n = self.dust_accum as i32;
        self.dust_accum -= n as f32;
        let mut rng = self.rng;
        for _ in 0..n {
            let a = rng.range(0.0, std::f64::consts::TAU);
            let p = Particle {
                pos: at,
                vel: V2::dir(a) * rng.range(4.0, 30.0),
                life: rng.range_f32(0.5, 1.6),
                max_life: 1.6,
                size: rng.range_f32(1.0, 2.4),
                color: rgb(190, 180, 160),
                kind: Kind::Dust,
                grow: 3.0,
            };
            self.push(p);
        }
        self.rng = rng;
    }

    pub fn puff(&mut self, at: V2, angle: f64) {
        let mut rng = self.rng;
        for _ in 0..14 {
            let side = if rng.f32() > 0.5 { 1.0 } else { -1.0 };
            let dir = V2::dir(angle + PI / 2.0 * side + rng.range(-0.4, 0.4));
            let p = Particle {
                pos: at,
                vel: dir * rng.range(15.0, 45.0),
                life: rng.range_f32(0.3, 0.8),
                max_life: 0.8,
                size: rng.range_f32(1.0, 2.4),
                color: rgb(225, 225, 230),
                kind: Kind::Puff,
                grow: 2.0,
            };
            self.push(p);
        }
        self.rng = rng;
    }

    /// Integrate particles. `world_scale` is metres per pixel so that sizes
    /// can stay in pixels regardless of zoom.
    pub fn update(&mut self, dt: f32) {
        for p in self.particles.iter_mut() {
            p.life -= dt;
            p.pos += p.vel * dt as f64;
            p.vel *= (1.0 - (0.9 * dt).min(0.9)) as f64;
            p.size += p.grow * dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }
}

impl Default for Effects {
    fn default() -> Self {
        Effects::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particles_expire_and_are_capped() {
        let mut fx = Effects::new();
        for _ in 0..500 {
            fx.explosion(V2::ZERO, 1.0);
        }
        assert!(fx.len() <= MAX_PARTICLES + 1, "len {}", fx.len());
        let before = fx.len();
        assert!(before > 0);
        for _ in 0..400 {
            fx.update(0.05);
        }
        assert_eq!(fx.len(), 0, "all particles should have expired");
    }

    #[test]
    fn exhaust_requires_throttle() {
        let mut fx = Effects::new();
        fx.exhaust(V2::ZERO, 0.0, 0.0, 1.0, 0.0);
        assert_eq!(fx.len(), 0);
        fx.exhaust(V2::ZERO, 0.0, 1.0, 0.5, 1.0);
        assert!(fx.len() > 0);
    }

    #[test]
    fn particles_stay_finite() {
        let mut fx = Effects::new();
        fx.explosion(V2::new(1e9, -1e9), 3.0);
        fx.reentry(V2::ZERO, V2::new(1e4, 2e4), 1.0, 0.1);
        fx.dust(V2::ZERO, 0.1, 1.0);
        fx.puff(V2::ZERO, 1.0);
        for _ in 0..100 {
            fx.update(0.05);
        }
        assert!(fx.particles.iter().all(|p| p.pos.is_finite() && p.vel.is_finite()));
    }
}
