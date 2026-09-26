//! Tiny deterministic hashing, RNG and value noise.
//!
//! Everything here is integer hashing plus basic IEEE float arithmetic (`+ - * floor sqrt`), so results
//! are bit-identical on every client. That matters: carve edges and arena layouts must match across
//! the network without shipping voxel data. We deliberately avoid external RNG/noise crates so a
//! dependency bump can never silently change a generated map or a replicated crater.

use glam::IVec3;

/// 32-bit integer finaliser (lowbias32 by Chris Wellons). Good avalanche, very cheap.
#[inline]
pub fn mix32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Hash of an integer lattice point + seed.
#[inline]
pub fn hash3(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = mix32(seed ^ 0x9e37_79b9);
    h = mix32(h ^ x as u32);
    h = mix32(h.wrapping_add(0x85eb_ca6b) ^ y as u32);
    mix32(h.wrapping_add(0xc2b2_ae35) ^ z as u32)
}

/// Hash of a voxel coordinate + seed.
#[inline]
pub fn hash_v(v: IVec3, seed: u32) -> u32 {
    hash3(v.x, v.y, v.z, seed)
}

/// Hash mapped to `[0, 1)`.
#[inline]
pub fn unit(h: u32) -> f32 {
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

/// Hash mapped to `[-1, 1)`.
#[inline]
pub fn signed(h: u32) -> f32 {
    unit(h) * 2.0 - 1.0
}

#[inline]
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 2D value noise in `[-1, 1]`, unit lattice spacing.
pub fn value2(x: f32, z: f32, seed: u32) -> f32 {
    let (fx, fz) = (x.floor(), z.floor());
    let (ix, iz) = (fx as i32, fz as i32);
    let (tx, tz) = (smooth(x - fx), smooth(z - fz));
    let c = |dx: i32, dz: i32| signed(hash3(ix + dx, 0, iz + dz, seed));
    lerp(lerp(c(0, 0), c(1, 0), tx), lerp(c(0, 1), c(1, 1), tx), tz)
}

/// 3D value noise in `[-1, 1]`, unit lattice spacing.
pub fn value3(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let (fx, fy, fz) = (x.floor(), y.floor(), z.floor());
    let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
    let (tx, ty, tz) = (smooth(x - fx), smooth(y - fy), smooth(z - fz));
    let c = |dx: i32, dy: i32, dz: i32| signed(hash3(ix + dx, iy + dy, iz + dz, seed));
    let y0 = lerp(lerp(c(0, 0, 0), c(1, 0, 0), tx), lerp(c(0, 0, 1), c(1, 0, 1), tx), tz);
    let y1 = lerp(lerp(c(0, 1, 0), c(1, 1, 0), tx), lerp(c(0, 1, 1), c(1, 1, 1), tx), tz);
    lerp(y0, y1, ty)
}

/// Fractal (fBm) 2D value noise, roughly in `[-1, 1]`.
pub fn fbm2(x: f32, z: f32, seed: u32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += amp * value2(x * freq, z * freq, seed.wrapping_add(o * 1013));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// Fractal (fBm) 3D value noise, roughly in `[-1, 1]`.
pub fn fbm3(x: f32, y: f32, z: f32, seed: u32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += amp * value3(x * freq, y * freq, z * freq, seed.wrapping_add(o * 1013));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

pub const PI: f32 = std::f32::consts::PI;
pub const TAU: f32 = std::f32::consts::TAU;

/// Deterministic sine (polynomial, |err| < 1e-6). `f32::sin` goes through the platform libm and may
/// differ by an ULP between OSes; generation must be identical on every client.
pub fn dsin(x: f32) -> f32 {
    let mut x = x - (x / TAU).round() * TAU; // [-π, π]
    if x > PI * 0.5 {
        x = PI - x;
    } else if x < -PI * 0.5 {
        x = -PI - x;
    }
    let x2 = x * x;
    x * (1.0 + x2 * (-1.0 / 6.0 + x2 * (1.0 / 120.0 + x2 * (-1.0 / 5040.0 + x2 * (1.0 / 362_880.0)))))
}

/// Deterministic cosine.
pub fn dcos(x: f32) -> f32 {
    dsin(x + PI * 0.5)
}

/// Deterministic atan2 (|err| < 2e-5 rad).
pub fn datan2(y: f32, x: f32) -> f32 {
    fn atan_unit(t: f32) -> f32 {
        let t2 = t * t;
        t * (0.999_977_26 + t2 * (-0.332_623_47 + t2 * (0.193_543_46 + t2 * (-0.116_432_87 + t2 * (0.052_653_32 + t2 * -0.011_721_20)))))
    }
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }
    if x.abs() >= y.abs() {
        let a = atan_unit(y / x);
        if x >= 0.0 {
            a
        } else if y >= 0.0 {
            a + PI
        } else {
            a - PI
        }
    } else {
        let a = atan_unit(x / y);
        if y > 0.0 { PI * 0.5 - a } else { -PI * 0.5 - a }
    }
}

/// Hermite smoothstep.
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// SplitMix64 — a tiny, fully specified RNG for level generation. Stable forever (unlike external
/// crates whose sampling algorithms may change between versions).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x5851_f42d_4c95_7f2d)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
    /// Uniform in `[0, 1)`.
    pub fn f(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / 16_777_216.0)
    }
    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
    /// Uniform integer in `[lo, hi)`.
    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        debug_assert!(hi > lo);
        lo + (self.next_u64() % (hi - lo) as u64) as i32
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.int(0, items.len() as i32) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_bounded_and_deterministic() {
        for i in 0..1000 {
            let x = i as f32 * 0.37 - 100.0;
            let a = value3(x, x * 0.5, -x, 7);
            assert!((-1.0..=1.0).contains(&a));
            assert_eq!(a.to_bits(), value3(x, x * 0.5, -x, 7).to_bits());
        }
        for i in -400..400 {
            let x = i as f32 * 0.05;
            assert!((dsin(x) - x.sin()).abs() < 1e-5, "sin {x}");
            assert!((dcos(x) - x.cos()).abs() < 1e-5, "cos {x}");
            let (y, xx) = (dsin(x) * 3.0, dcos(x * 1.3) - 0.2);
            assert!((datan2(y, xx) - y.atan2(xx)).abs() < 1e-4, "atan2 {y} {xx}");
        }
        let mut a = Rng::new(3);
        let mut b = Rng::new(3);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
