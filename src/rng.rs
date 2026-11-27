//! Deterministic pseudo random numbers (splitmix64) and integer hashing.
//!
//! Everything that looks random in the game (star field, craters, particles)
//! is produced from these so that screenshots and replays are reproducible.

#[derive(Copy, Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub const fn new(seed: u64) -> Rng {
        // Avoid the all-zero state.
        Rng {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in `[0, 1)`.
    #[inline]
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    /// Uniform in `[0, 1)`.
    #[inline]
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    #[inline]
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }

    #[inline]
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    #[inline]
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            self.next_u32() % n
        }
    }

    /// Roughly normal distributed value (mean 0, sd 1) via summing.
    pub fn normal(&mut self) -> f64 {
        let s = self.f64() + self.f64() + self.f64() + self.f64() + self.f64() + self.f64();
        (s - 3.0) * std::f64::consts::FRAC_1_SQRT_2
    }
}

/// Cheap, well distributed hash of a 2D integer lattice point.
#[inline]
pub fn hash2(ix: i32, iy: i32, seed: u32) -> u32 {
    let mut h = (ix as u32).wrapping_mul(0x27d4_eb2d) ^ (iy as u32).wrapping_mul(0x1656_67b1) ^ seed;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    h
}

/// Hash of an integer, in `[0, 1)`.
#[inline]
pub fn hash01(ix: i32, iy: i32, seed: u32) -> f32 {
    (hash2(ix, iy, seed) >> 8) as f32 * (1.0 / 16_777_216.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_in_range() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..1000 {
            let x = a.f64();
            let y = b.f64();
            assert_eq!(x, y);
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn hash_is_uniformish() {
        let mut sum = 0.0f64;
        let n = 2000;
        for i in 0..n {
            sum += hash01(i, i * 7 + 3, 99) as f64;
        }
        let mean = sum / n as f64;
        assert!((mean - 0.5).abs() < 0.03, "mean {mean}");
    }
}
