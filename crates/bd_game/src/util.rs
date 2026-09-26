//! Small shared helpers: a fast deterministic RNG and frame-time statistics.

use bevy::prelude::*;

// ---- bd_core (workspace glam) <-> Bevy (glam re-exported by bevy_math) -------------------------
// The workspace pins glam 0.30 while Bevy 0.19 uses glam 0.32, so the types differ. These are
// zero-cost copies and keep compiling unchanged if the workspace moves to Bevy's glam.

#[inline]
pub fn v3(v: bd_core::Vec3) -> Vec3 {
    Vec3::from_array(v.to_array())
}
#[inline]
pub fn core_v3(v: Vec3) -> bd_core::Vec3 {
    bd_core::Vec3::from_array(v.to_array())
}
#[inline]
pub fn iv3(v: bd_core::IVec3) -> IVec3 {
    IVec3::from_array(v.to_array())
}
#[inline]
pub fn core_iv3(v: IVec3) -> bd_core::IVec3 {
    bd_core::IVec3::from_array(v.to_array())
}

/// PCG32 (O'Neill). Tiny, fast, good enough for VFX.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng { state: seed.wrapping_add(0x853c_49e6_748f_ea9b) };
        r.next_u32();
        r
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }
    /// Uniform in [0, 1).
    #[inline]
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
    #[inline]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
    /// Uniform direction on the unit sphere.
    pub fn unit_vec3(&mut self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let a = self.range(0.0, std::f32::consts::TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * a.cos(), r * a.sin(), z)
    }
    pub fn quat(&mut self) -> Quat {
        Quat::from_axis_angle(self.unit_vec3(), self.range(0.0, std::f32::consts::TAU))
    }
}

/// Integer hash → [0,1). Stable per voxel coordinate (colour jitter etc.).
#[inline]
pub fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

/// Summary statistics over a set of frame times (milliseconds).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    pub frames: usize,
    pub avg_ms: f32,
    pub p50_ms: f32,
    pub p99_ms: f32,
    pub max_ms: f32,
    /// Frames longer than 16 ms.
    pub hitches: usize,
    /// FPS computed from the average frame time.
    pub avg_fps: f32,
    /// "1 % low" FPS: average FPS of the slowest 1 % of frames.
    pub low1_fps: f32,
}

impl FrameStats {
    pub const HITCH_MS: f32 = 16.0;

    /// `scratch` is reused to avoid allocating every call.
    pub fn compute(samples_ms: impl Iterator<Item = f32>, scratch: &mut Vec<f32>) -> Self {
        scratch.clear();
        scratch.extend(samples_ms);
        let n = scratch.len();
        if n == 0 {
            return Self::default();
        }
        let sum: f32 = scratch.iter().sum();
        let hitches = scratch.iter().filter(|&&t| t > Self::HITCH_MS).count();
        scratch.sort_unstable_by(|a, b| a.total_cmp(b));
        let pct = |p: f32| scratch[(((n - 1) as f32) * p).round() as usize];
        let worst = (n / 100).max(1);
        let worst_avg: f32 = scratch[n - worst..].iter().sum::<f32>() / worst as f32;
        let avg = sum / n as f32;
        FrameStats {
            frames: n,
            avg_ms: avg,
            p50_ms: pct(0.5),
            p99_ms: pct(0.99),
            max_ms: scratch[n - 1],
            hitches,
            avg_fps: 1000.0 / avg.max(1e-3),
            low1_fps: 1000.0 / worst_avg.max(1e-3),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats() {
        let mut v = Vec::new();
        let s = FrameStats::compute((0..1000).map(|i| if i == 500 { 20.0 } else { 5.0 }), &mut v);
        assert_eq!(s.frames, 1000);
        assert_eq!(s.hitches, 1);
        assert_eq!(s.max_ms, 20.0);
        assert_eq!(s.p50_ms, 5.0);
        assert!(s.low1_fps < s.avg_fps);
    }

    #[test]
    fn rng_range() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            let x = r.f32();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
