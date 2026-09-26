//! Small helpers: a tiny RNG and glam 0.30 (bd_shapes) ↔ Bevy math conversions.

use bevy::math::{Affine3A, Vec3};

/// xorshift-style RNG (cosmetic only; not deterministic across clients on purpose).
#[derive(Clone, Debug)]
pub struct FxRng(u64);

impl Default for FxRng {
    fn default() -> Self {
        FxRng(0x9E37_79B9_7F4A_7C15)
    }
}

impl FxRng {
    pub fn new(seed: u64) -> Self {
        FxRng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u32
    }
    /// 0..1
    #[inline]
    pub fn f(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
    /// a..b
    #[inline]
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    /// -1..1 per component
    #[inline]
    pub fn vec(&mut self) -> Vec3 {
        Vec3::new(self.f() * 2.0 - 1.0, self.f() * 2.0 - 1.0, self.f() * 2.0 - 1.0)
    }
    /// Uniform-ish unit vector.
    pub fn dir(&mut self) -> Vec3 {
        for _ in 0..8 {
            let v = self.vec();
            let l = v.length_squared();
            if l > 1e-3 && l <= 1.0 {
                return v / l.sqrt();
            }
        }
        Vec3::Y
    }
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next_u32() as usize) % n }
    }
}

#[inline]
pub fn aff(a: &glam::Affine3A) -> Affine3A {
    Affine3A::from_cols_array(&a.to_cols_array())
}

#[inline]
pub fn v3(v: glam::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

#[inline]
pub fn g3(v: Vec3) -> glam::Vec3 {
    glam::Vec3::new(v.x, v.y, v.z)
}

/// Finite or fallback.
#[inline]
pub fn fin(v: f32, fb: f32) -> f32 {
    if v.is_finite() { v } else { fb }
}

#[inline]
pub fn fin3(v: Vec3) -> Vec3 {
    if v.is_finite() { v } else { Vec3::ZERO }
}
