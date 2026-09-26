//! Small real-time DSP building blocks: dB helpers, a zipper-free TPT state-variable filter, RBJ
//! biquads, one-pole smoothers and a fractional delay line. Everything is allocation-free after
//! construction and flushes denormals in its feedback paths.

use std::f32::consts::PI;

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    if db <= -120.0 { 0.0 } else { 10f32.powf(db * 0.05) }
}

#[inline]
pub fn lin_to_db(x: f32) -> f32 {
    if x <= 1e-6 { -120.0 } else { 20.0 * x.log10() }
}

/// Replace denormal-range / non-finite values by 0 (keeps feedback paths clean and fast).
#[inline]
pub fn flush(x: f32) -> f32 {
    if x.is_finite() && x.abs() > 1e-20 { x } else { 0.0 }
}

/// Finite or a fallback.
#[inline]
pub fn finite_or(x: f32, fallback: f32) -> f32 {
    if x.is_finite() { x } else { fallback }
}

/// Interpolate two frequencies in log space.
#[inline]
pub fn lerp_hz(a: f32, b: f32, t: f32) -> f32 {
    let (a, b) = (a.max(1.0), b.max(1.0));
    (a.ln() + (b.ln() - a.ln()) * t.clamp(0.0, 1.0)).exp()
}

#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One-pole smoothing coefficient for a time constant `tau` seconds at `rate` updates per second.
#[inline]
pub fn coef(tau: f32, rate: f32) -> f32 {
    if tau <= 0.0 { 1.0 } else { 1.0 - (-1.0 / (tau * rate)).exp() }
}

/// Topology-preserving-transform state-variable filter (Zavalishin / Simper). Stable and quiet under
/// per-block cutoff modulation, which is exactly what distance / occlusion / snapshots do.
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
    /// Cutoff the coefficients were computed for (skip recomputation when unchanged).
    fc: f32,
}

impl Svf {
    pub fn new(fc: f32, q: f32, sr: f32) -> Self {
        let mut s = Self::default();
        s.set(fc, q, sr);
        s
    }
    /// Set cutoff (Hz) and Q. Cheap enough per block; skipped if the cutoff barely moved.
    #[inline]
    pub fn set(&mut self, fc: f32, q: f32, sr: f32) {
        let fc = finite_or(fc, 1000.0).clamp(10.0, sr * 0.49);
        if (fc - self.fc).abs() <= self.fc * 1e-4 && self.k != 0.0 {
            return;
        }
        self.fc = fc;
        let g = (PI * fc / sr).tan();
        let k = 1.0 / q.max(0.1);
        self.k = k;
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }
    #[inline]
    fn tick(&mut self, x: f32) -> (f32, f32) {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v1, v2)
    }
    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        self.tick(x).1
    }
    #[inline]
    pub fn hp(&mut self, x: f32) -> f32 {
        let (v1, v2) = self.tick(x);
        x - self.k * v1 - v2
    }
    /// Kill denormals / NaN in the state (call once per block).
    #[inline]
    pub fn sanitize(&mut self) {
        self.ic1 = flush(self.ic1);
        self.ic2 = flush(self.ic2);
    }
    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }
    pub fn cutoff(&self) -> f32 {
        self.fc
    }
}

/// RBJ biquad, transposed direct form II.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }
}

impl Biquad {
    fn set_coefs(&mut self, b: [f32; 3], a: [f32; 3]) {
        let inv = 1.0 / a[0];
        self.b0 = b[0] * inv;
        self.b1 = b[1] * inv;
        self.b2 = b[2] * inv;
        self.a1 = a[1] * inv;
        self.a2 = a[2] * inv;
    }
    /// Peaking EQ (bell).
    pub fn set_peaking(&mut self, fc: f32, q: f32, gain_db: f32, sr: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * fc.clamp(10.0, sr * 0.49) / sr;
        let (sw, cw) = w0.sin_cos();
        let alpha = sw / (2.0 * q.max(0.1));
        self.set_coefs([1.0 + alpha * a, -2.0 * cw, 1.0 - alpha * a], [1.0 + alpha / a, -2.0 * cw, 1.0 - alpha / a]);
    }
    /// High shelf (slope 1).
    pub fn set_high_shelf(&mut self, fc: f32, gain_db: f32, sr: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * fc.clamp(10.0, sr * 0.49) / sr;
        let (sw, cw) = w0.sin_cos();
        let alpha = sw / 2.0 * 2f32.sqrt();
        let sa = 2.0 * a.sqrt() * alpha;
        self.set_coefs(
            [a * ((a + 1.0) + (a - 1.0) * cw + sa), -2.0 * a * ((a - 1.0) + (a + 1.0) * cw), a * ((a + 1.0) + (a - 1.0) * cw - sa)],
            [(a + 1.0) - (a - 1.0) * cw + sa, 2.0 * ((a - 1.0) - (a + 1.0) * cw), (a + 1.0) - (a - 1.0) * cw - sa],
        );
    }
    /// K-weighting stage coefficients (BS.1770) are passed in directly.
    pub fn from_coefs(b: [f32; 3], a: [f32; 3]) -> Self {
        let mut s = Self::default();
        s.set_coefs(b, a);
        s
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
    #[inline]
    pub fn sanitize(&mut self) {
        self.z1 = flush(self.z1);
        self.z2 = flush(self.z2);
    }
}

/// A parameter smoothed once per block, rendered as a per-sample linear ramp within the block.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ramp {
    pub cur: f32,
    pub step: f32,
}

impl Ramp {
    pub fn new(v: f32) -> Self {
        Self { cur: v, step: 0.0 }
    }
    /// Plan a linear ramp from the current value to `target` over `n` samples.
    #[inline]
    pub fn to(&mut self, target: f32, n: usize) {
        let target = finite_or(target, 0.0);
        self.step = (target - self.cur) / n as f32;
    }
    #[inline]
    pub fn tick(&mut self) -> f32 {
        self.cur += self.step;
        self.cur
    }
    /// Snap exactly to the end value after a block (removes float drift).
    #[inline]
    pub fn settle(&mut self, target: f32) {
        self.cur = flush(finite_or(target, 0.0));
        self.step = 0.0;
    }
}

/// Mono delay line with fractional (linear) reads. Length is a power of two.
#[derive(Clone, Debug)]
pub struct Delay {
    buf: Box<[f32]>,
    mask: usize,
    w: usize,
}

impl Delay {
    pub fn new(max_samples: usize) -> Self {
        let n = (max_samples + 4).next_power_of_two();
        Self { buf: vec![0.0; n].into_boxed_slice(), mask: n - 1, w: 0 }
    }
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.buf[self.w & self.mask] = x;
        self.w = self.w.wrapping_add(1);
    }
    /// Sample written `d` samples ago (d >= 1 means before the latest push), fractional.
    #[inline]
    pub fn read(&self, d: f32) -> f32 {
        let d = d.clamp(1.0, (self.mask - 2) as f32);
        let di = d as usize;
        let fr = d - di as f32;
        let i0 = self.w.wrapping_sub(di) & self.mask;
        let i1 = self.w.wrapping_sub(di + 1) & self.mask;
        self.buf[i0] + (self.buf[i1] - self.buf[i0]) * fr
    }
    pub fn max_delay(&self) -> usize {
        self.mask - 2
    }
    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
    }
}

/// Tiny deterministic PRNG (xorshift64*), used on both threads.
#[derive(Clone, Copy, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    #[inline]
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    #[inline]
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f32()
    }
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next_u64() % n as u64) as usize }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svf_lowpass_attenuates_highs() {
        let sr = 48000.0;
        let mut f = Svf::new(500.0, 0.707, sr);
        let mut e_hi = 0.0;
        for i in 0..4800 {
            let x = (2.0 * PI * 8000.0 * i as f32 / sr).sin();
            let y = f.lp(x);
            if i > 480 {
                e_hi += y * y;
            }
        }
        let rms = (e_hi / 4320.0f32).sqrt();
        assert!(rms < 0.01, "8 kHz through 500 Hz LP: {rms}");
    }

    #[test]
    fn db_roundtrip() {
        for db in [-60.0f32, -12.0, 0.0, 6.0] {
            assert!((lin_to_db(db_to_lin(db)) - db).abs() < 1e-3);
        }
    }

    #[test]
    fn delay_reads_back() {
        let mut d = Delay::new(100);
        for i in 0..50 {
            d.push(i as f32);
        }
        assert_eq!(d.read(1.0), 49.0);
        assert_eq!(d.read(10.0), 40.0);
        assert!((d.read(10.5) - 39.5).abs() < 1e-6);
    }
}
