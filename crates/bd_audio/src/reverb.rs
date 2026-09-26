//! The shared space: one reverb every World voice sends to, so the brain hears one place.
//!
//! * 8-line feedback delay network, Hadamard mixing, per-line HF damping, RT60-exact loop gains.
//! * Early reflections: a short multi-tap cluster scaled by the room size, plus one **slapback**
//!   tap (the dominant distant surface — a street canyon wall, a cliff face) at a delay the
//!   acoustics estimate derives from voxel raycasts.
//! * Size changes glide (fractional delay reads) so a room smoothly "opens up" as it is smashed.
//!
//! Cost: ~60 flops per stereo frame. Allocation only in [`Reverb::new`].

use serde::Deserialize;

use crate::dsp::{Delay, coef, db_to_lin, finite_or, flush};

/// Target reverb parameters (from the acoustics estimate + map preset).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct ReverbParams {
    /// Delay-line scale, 0.25 (closet) .. 2 (hall/canyon).
    pub size: f32,
    /// Mid-frequency decay time (s).
    pub rt60: f32,
    /// In-loop damping cutoff (Hz): lower = darker, faster HF decay.
    pub damping_hz: f32,
    pub predelay_ms: f32,
    /// Late reverb return (dB).
    pub wet_db: f32,
    /// Early reflection cluster level (dB).
    pub er_db: f32,
    /// Slapback echo delay (ms, 0 = off) and level (dB).
    pub slap_ms: f32,
    pub slap_db: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self { size: 0.8, rt60: 1.1, damping_hz: 6000.0, predelay_ms: 12.0, wet_db: -14.0, er_db: -16.0, slap_ms: 0.0, slap_db: -60.0 }
    }
}

impl ReverbParams {
    pub fn sanitized(mut self) -> Self {
        self.size = finite_or(self.size, 0.8).clamp(0.25, 2.0);
        self.rt60 = finite_or(self.rt60, 1.0).clamp(0.1, 8.0);
        self.damping_hz = finite_or(self.damping_hz, 6000.0).clamp(500.0, 18000.0);
        self.predelay_ms = finite_or(self.predelay_ms, 10.0).clamp(0.0, 80.0);
        self.wet_db = finite_or(self.wet_db, -60.0).clamp(-90.0, 6.0);
        self.er_db = finite_or(self.er_db, -60.0).clamp(-90.0, 6.0);
        self.slap_ms = finite_or(self.slap_ms, 0.0).clamp(0.0, 350.0);
        self.slap_db = finite_or(self.slap_db, -60.0).clamp(-90.0, 0.0);
        self
    }
    /// Interpolate (used by the acoustics smoother).
    pub fn lerp(&self, o: &ReverbParams, t: f32) -> ReverbParams {
        let l = |a: f32, b: f32| a + (b - a) * t;
        ReverbParams {
            size: l(self.size, o.size),
            rt60: l(self.rt60, o.rt60),
            damping_hz: l(self.damping_hz, o.damping_hz),
            predelay_ms: l(self.predelay_ms, o.predelay_ms),
            wet_db: l(self.wet_db, o.wet_db),
            er_db: l(self.er_db, o.er_db),
            slap_ms: l(self.slap_ms, o.slap_ms),
            slap_db: l(self.slap_db, o.slap_db),
        }
    }
}

const LINES: usize = 8;
/// Base line lengths (ms) at size 1: mutually prime-ish so the modes don't pile up.
const BASE_MS: [f32; LINES] = [31.3, 37.9, 41.7, 47.9, 53.1, 59.3, 67.1, 73.7];
/// Early reflection taps (ms at size 1, gain, pan −1..1).
const ER_TAPS: [(f32, f32, f32); 6] =
    [(7.1, 0.9, -0.6), (11.3, 0.75, 0.7), (16.9, 0.62, -0.3), (23.3, 0.5, 0.45), (29.9, 0.4, -0.8), (37.1, 0.32, 0.8)];

pub struct Reverb {
    sr: f32,
    /// Send high-pass: sub never feeds the tail (no low-end mud build-up).
    in_hp: crate::dsp::Svf,
    lines: [Delay; LINES],
    damp: [f32; LINES],
    pre: Delay,
    er: Delay,
    target: ReverbParams,
    // Smoothed live values.
    size: f32,
    rt60: f32,
    damp_a: f32,
    wet: f32,
    er_g: f32,
    slap_ms: f32,
    slap_g: f32,
    pre_ms: f32,
    gains: [f32; LINES],
    slap_lp: [f32; 2],
}

impl Reverb {
    pub fn new(sr: f32) -> Self {
        let max_line = (BASE_MS[LINES - 1] * 2.05 * sr / 1000.0) as usize + 8;
        let lines = std::array::from_fn(|_| Delay::new(max_line));
        let p = ReverbParams::default();
        let mut r = Self {
            sr,
            in_hp: crate::dsp::Svf::new(160.0, 0.707, sr),
            lines,
            damp: [0.0; LINES],
            pre: Delay::new((0.09 * sr) as usize),
            er: Delay::new((0.4 * sr) as usize),
            target: p,
            size: p.size,
            rt60: p.rt60,
            damp_a: 0.5,
            wet: db_to_lin(p.wet_db),
            er_g: db_to_lin(p.er_db),
            slap_ms: p.slap_ms,
            slap_g: 0.0,
            pre_ms: p.predelay_ms,
            gains: [0.0; LINES],
            slap_lp: [0.0; 2],
        };
        r.set_params(p);
        r.snap_to_target();
        r
    }

    pub fn set_params(&mut self, p: ReverbParams) {
        self.target = p.sanitized();
    }

    /// Jump straight to the target (no glide) — for construction / offline starts.
    pub fn snap_to_target(&mut self) {
        let t = self.target;
        self.size = t.size;
        self.rt60 = t.rt60;
        self.wet = db_to_lin(t.wet_db);
        self.er_g = db_to_lin(t.er_db);
        self.slap_ms = t.slap_ms;
        self.slap_g = db_to_lin(t.slap_db);
        self.pre_ms = t.predelay_ms;
        self.update_coefs();
    }

    fn update_coefs(&mut self) {
        let x = (-2.0 * std::f32::consts::PI * self.target.damping_hz / self.sr).exp();
        self.damp_a = x;
        for (g, ms) in self.gains.iter_mut().zip(BASE_MS) {
            let d = ms * self.size * 0.001; // seconds
            *g = 10f32.powf(-3.0 * d / self.rt60.max(0.05));
        }
    }

    /// Per-block parameter glide (`n` frames).
    fn glide(&mut self, n: usize) {
        let rate = self.sr / n as f32; // blocks per second
        let t = self.target;
        // Size glides slowly (≤ 0.6 units/s) so delay-length changes stay inaudible.
        let max_step = 0.6 / rate;
        self.size += (t.size - self.size).clamp(-max_step, max_step);
        let c = coef(0.25, rate);
        self.rt60 += (t.rt60 - self.rt60) * c;
        self.wet += (db_to_lin(t.wet_db) - self.wet) * c;
        self.er_g += (db_to_lin(t.er_db) - self.er_g) * c;
        self.slap_g += (db_to_lin(t.slap_db) - self.slap_g) * c;
        let ms_step = 40.0 / rate; // slapback delay glides ≤ 40 ms/s
        self.slap_ms += (t.slap_ms - self.slap_ms).clamp(-ms_step, ms_step);
        self.pre_ms += (t.predelay_ms - self.pre_ms).clamp(-ms_step, ms_step);
        self.update_coefs();
    }

    /// Process one block: `inp` interleaved stereo send, `out` interleaved stereo return (added).
    pub fn process(&mut self, inp: &[f32], out: &mut [f32]) {
        let n = inp.len() / 2;
        if n == 0 {
            return;
        }
        self.glide(n);
        let ms = self.sr / 1000.0;
        let line_len: [f32; LINES] = std::array::from_fn(|i| BASE_MS[i] * self.size * ms);
        let pre_d = (self.pre_ms * ms).max(1.0);
        let slap_d = (self.slap_ms * ms).max(1.0);
        let slap_on = self.slap_ms > 1.0 && self.slap_g > 1e-4;
        let er_d: [f32; 6] = std::array::from_fn(|i| (ER_TAPS[i].0 * self.size.max(0.4) * ms).max(1.0));
        let a = self.damp_a;
        let norm = 1.0 / (LINES as f32).sqrt();
        for f in 0..n {
            let mono = self.in_hp.hp(0.5 * (inp[2 * f] + inp[2 * f + 1]));
            // Early reflections read from the (un-predelayed) send.
            self.er.push(mono);
            let mut el = 0.0;
            let mut er = 0.0;
            for (k, &(_, g, pan)) in ER_TAPS.iter().enumerate() {
                let s = self.er.read(er_d[k]) * g;
                el += s * (0.5 - 0.5 * pan);
                er += s * (0.5 + 0.5 * pan);
            }
            el *= self.er_g;
            er *= self.er_g;
            if slap_on {
                let s = self.er.read(slap_d) * self.slap_g;
                // The echo comes back darker than it left.
                self.slap_lp[0] += 0.35 * (s - self.slap_lp[0]);
                el += self.slap_lp[0];
                er += self.slap_lp[0] * 0.85;
            }
            // Late: predelay → FDN.
            self.pre.push(mono);
            let x = self.pre.read(pre_d);
            let mut v = [0.0f32; LINES];
            for i in 0..LINES {
                let s = self.lines[i].read(line_len[i]);
                // One-pole damping inside the loop.
                self.damp[i] = s + a * (self.damp[i] - s);
                v[i] = self.damp[i] * self.gains[i];
            }
            // Fast Walsh–Hadamard (8).
            hadamard8(&mut v);
            for (i, (line, vi)) in self.lines.iter_mut().zip(v.iter()).enumerate() {
                let inj = if i & 1 == 0 { x } else { -x };
                line.push(flush(vi * norm + inj * 0.35));
            }
            let mut ll = 0.0;
            let mut rr = 0.0;
            for i in 0..LINES {
                let s = self.damp[i];
                if i % 2 == 0 {
                    ll += if i % 4 == 0 { s } else { -s };
                } else {
                    rr += if i % 4 == 1 { s } else { -s };
                }
            }
            out[2 * f] += el + ll * self.wet * 0.5;
            out[2 * f + 1] += er + rr * self.wet * 0.5;
        }
        for d in self.damp.iter_mut() {
            *d = flush(*d);
        }
        self.in_hp.sanitize();
        self.slap_lp[0] = flush(self.slap_lp[0]);
    }

    pub fn clear(&mut self) {
        for l in self.lines.iter_mut() {
            l.clear();
        }
        self.pre.clear();
        self.er.clear();
        self.damp = [0.0; LINES];
    }
}

#[inline]
fn hadamard8(v: &mut [f32; LINES]) {
    let mut h = 1;
    while h < LINES {
        let mut i = 0;
        while i < LINES {
            for j in i..i + h {
                let a = v[j];
                let b = v[j + h];
                v[j] = a + b;
                v[j + h] = a - b;
            }
            i += h * 2;
        }
        h *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tail_energy(p: ReverbParams) -> (f32, f32) {
        let sr = 48000.0;
        let mut r = Reverb::new(sr);
        r.set_params(p);
        r.snap_to_target();
        let n = 64;
        let mut e_early = 0.0;
        let mut e_late = 0.0;
        for b in 0..(sr as usize * 2 / n) {
            let mut inp = vec![0.0f32; 2 * n];
            if b == 0 {
                inp[0] = 1.0;
                inp[1] = 1.0;
            }
            let mut out = vec![0.0f32; 2 * n];
            r.process(&inp, &mut out);
            let e: f32 = out.iter().map(|x| x * x).sum();
            assert!(out.iter().all(|x| x.is_finite()));
            let t = b * n;
            if t < 24000 {
                e_early += e;
            } else {
                e_late += e;
            }
        }
        (e_early, e_late)
    }

    #[test]
    fn longer_rt60_has_more_late_energy() {
        let short = tail_energy(ReverbParams { rt60: 0.3, ..Default::default() });
        let long = tail_energy(ReverbParams { rt60: 2.5, ..Default::default() });
        assert!(long.1 > short.1 * 10.0, "late energy short {:?} long {:?}", short, long);
    }

    #[test]
    fn decays_and_stays_stable() {
        let (_, late) = tail_energy(ReverbParams { rt60: 8.0, wet_db: 0.0, ..Default::default() });
        assert!(late.is_finite() && late < 1e3);
    }
}
