//! Master dynamics: a soft-knee look-ahead peak limiter that provably never exceeds its ceiling,
//! and the night-mode ("reduced dynamic range") compressor.

use crate::dsp::{coef, db_to_lin, flush, lin_to_db};
use crate::mix::{LimiterConfig, NightConfig};

/// Stereo-linked look-ahead limiter.
///
/// For each input sample the required gain `g_req = ceiling / |x|` (with a soft knee below the
/// ceiling) enters a sliding-minimum window of `W` samples and then a `W`-sample moving average.
/// The output is delayed by `W − 1` samples, so every sample is multiplied by an average of values
/// that are all ≤ its own requirement: `|y| ≤ ceiling` by construction. A final clamp guards float
/// rounding. Release is an exponential recovery of the hold signal.
pub struct Limiter {
    w: usize,
    req: Box<[f32]>,
    hold_hist: Box<[f32]>,
    delay: Box<[f32]>,
    pos: usize,
    sum: f64,
    hold: f32,
    release: f32,
    ceiling: f32,
    knee_lo: f32,
    knee_db: f32,
    ceiling_db: f32,
    /// Deepest gain reduction in the last block (dB, ≤ 0).
    pub last_gr_db: f32,
}

impl Limiter {
    pub fn new(cfg: &LimiterConfig, sr: f32) -> Self {
        let w = ((cfg.lookahead_ms.clamp(0.2, 10.0) * 0.001 * sr) as usize).max(2);
        let ceiling_db = cfg.ceiling_db.clamp(-24.0, 0.0);
        let knee_db = cfg.knee_db.clamp(0.0, 12.0);
        Self {
            w,
            req: vec![1.0; w].into_boxed_slice(),
            hold_hist: vec![1.0; w].into_boxed_slice(),
            delay: vec![0.0; 2 * w].into_boxed_slice(),
            pos: 0,
            sum: w as f64,
            hold: 1.0,
            release: coef(cfg.release.max(0.005), sr),
            ceiling: db_to_lin(ceiling_db),
            knee_lo: db_to_lin(ceiling_db - knee_db),
            knee_db,
            ceiling_db,
            last_gr_db: 0.0,
        }
    }

    /// Latency in frames.
    pub fn latency(&self) -> usize {
        self.w - 1
    }

    #[inline]
    fn required(&self, peak: f32) -> f32 {
        if peak <= self.knee_lo {
            return 1.0;
        }
        // Soft knee: output level follows a quadratic from (ceiling − knee) up to the ceiling.
        let in_db = lin_to_db(peak);
        let lo = self.ceiling_db - self.knee_db;
        let out_db = if self.knee_db <= 0.0 {
            self.ceiling_db
        } else {
            let t = ((in_db - lo) / (2.0 * self.knee_db)).min(1.0);
            // Smoothly bends from slope 1 at `lo` to slope 0 at lo + 2·knee (the ceiling).
            lo + self.knee_db * 2.0 * (t - 0.5 * t * t)
        };
        let out_db = out_db.min(self.ceiling_db);
        (db_to_lin(out_db - in_db)).min(1.0)
    }

    /// Process interleaved stereo in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        let w = self.w;
        let mut min_g = 1.0f32;
        for f in 0..buf.len() / 2 {
            let l = buf[2 * f];
            let r = buf[2 * f + 1];
            let (l, r) = (if l.is_finite() { l } else { 0.0 }, if r.is_finite() { r } else { 0.0 });
            let peak = l.abs().max(r.abs());
            let slot = self.pos % w;
            self.req[slot] = self.required(peak);
            // Sliding minimum over the last W requirements (W ≤ a few dozen: a scan is cheapest).
            let mut m = 1.0f32;
            for &g in self.req.iter() {
                m = m.min(g);
            }
            // Instant attack, exponential release.
            self.hold = if m < self.hold { m } else { self.hold + (m - self.hold) * self.release };
            let hold = self.hold;
            // Moving average of the hold signal over W samples.
            self.sum += hold as f64 - self.hold_hist[slot] as f64;
            self.hold_hist[slot] = hold;
            let g = ((self.sum / w as f64) as f32).min(1.0);
            // Delay line (W − 1 samples): write slot n, read slot n + 1 (written W − 1 samples ago).
            let wi = slot * 2;
            self.delay[wi] = l;
            self.delay[wi + 1] = r;
            let ri = ((self.pos + 1) % w) * 2;
            let out_l = self.delay[ri];
            let out_r = self.delay[ri + 1];
            let c = self.ceiling;
            buf[2 * f] = flush((out_l * g).clamp(-c, c));
            buf[2 * f + 1] = flush((out_r * g).clamp(-c, c));
            min_g = min_g.min(g);
            self.pos = self.pos.wrapping_add(1);
        }
        self.hold = flush(self.hold).max(1e-6);
        self.last_gr_db = lin_to_db(min_g).min(0.0);
    }
}

/// Feed-forward RMS compressor, stereo-linked (night mode).
pub struct Compressor {
    env: f32,
    att: f32,
    rel: f32,
    threshold_db: f32,
    ratio: f32,
    makeup: f32,
    pub gr_db: f32,
}

impl Compressor {
    pub fn new(cfg: &NightConfig, sr: f32) -> Self {
        Self {
            env: 0.0,
            att: coef(0.01, sr),
            rel: coef(0.25, sr),
            threshold_db: cfg.threshold_db,
            ratio: cfg.ratio.max(1.0),
            makeup: db_to_lin(cfg.makeup_db),
            gr_db: 0.0,
        }
    }
    pub fn process(&mut self, buf: &mut [f32], amount: f32) {
        if amount <= 0.001 {
            self.gr_db = 0.0;
            return;
        }
        for f in 0..buf.len() / 2 {
            let p = 0.5 * (buf[2 * f] * buf[2 * f] + buf[2 * f + 1] * buf[2 * f + 1]);
            let c = if p > self.env { self.att } else { self.rel };
            self.env += (p - self.env) * c;
            let lvl = 10.0 * (self.env + 1e-12).log10();
            let over = (lvl - self.threshold_db).max(0.0);
            let gr = -over * (1.0 - 1.0 / self.ratio);
            let g = db_to_lin(gr * amount) * (1.0 + (self.makeup - 1.0) * amount);
            buf[2 * f] *= g;
            buf[2 * f + 1] *= g;
            self.gr_db = gr;
        }
        self.env = flush(self.env);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::Rng;

    #[test]
    fn limiter_never_exceeds_ceiling() {
        let cfg = LimiterConfig::default();
        let mut l = Limiter::new(&cfg, 48000.0);
        let ceiling = db_to_lin(cfg.ceiling_db);
        let mut rng = Rng::new(3);
        let mut worst = 0.0f32;
        for b in 0..2000 {
            let mut buf = [0.0f32; 128];
            for (i, s) in buf.iter_mut().enumerate() {
                // Bursts up to +24 dBFS, silence, impulses, DC steps.
                let k = (b / 50) % 4;
                *s = match k {
                    0 => rng.range(-16.0, 16.0),
                    1 => 0.0,
                    2 => {
                        if i % 37 == 0 {
                            12.0
                        } else {
                            0.01
                        }
                    }
                    _ => 3.0,
                };
            }
            l.process(&mut buf);
            for s in buf {
                assert!(s.is_finite());
                worst = worst.max(s.abs());
            }
        }
        assert!(worst <= ceiling + 1e-6, "peak {worst} > ceiling {ceiling}");
        assert!(worst <= 1.0);
    }

    #[test]
    fn limiter_is_transparent_below_knee() {
        let cfg = LimiterConfig::default();
        let mut l = Limiter::new(&cfg, 48000.0);
        let lat = l.latency();
        let n = 2048;
        let x: Vec<f32> = (0..n).map(|i| 0.3 * ((i as f32) * 0.05).sin()).collect();
        let mut buf: Vec<f32> = x.iter().flat_map(|&s| [s, s]).collect();
        l.process(&mut buf);
        for i in lat + 10..n {
            assert!((buf[2 * i] - x[i - lat]).abs() < 1e-4, "at {i}: {} vs {}", buf[2 * i], x[i - lat]);
        }
    }
}
