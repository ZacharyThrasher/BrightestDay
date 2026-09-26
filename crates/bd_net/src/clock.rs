//! RTT measurement, host clock estimation and client tick pacing.
//!
//! The host answers pings on its network thread with its current (extrapolated) simulation tick.
//! [`ClockSync`] turns those samples into a smooth estimate of "which host tick is it right now",
//! and how far ahead the client must simulate so its inputs arrive just before the host needs
//! them. [`TickPacer`] then drives the client's fixed-step loop towards that target by gently
//! warping time (±5%) instead of jumping, and snaps only on large errors.

use std::collections::VecDeque;

/// Smoothed round-trip statistics (seconds), RFC 6298 style.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RttStats {
    /// Smoothed RTT.
    pub srtt: f64,
    /// Smoothed mean deviation (a jitter measure).
    pub rttvar: f64,
    /// Smallest RTT seen.
    pub min: f64,
    /// Last raw sample.
    pub last: f64,
    /// Number of samples.
    pub count: u64,
}

impl RttStats {
    pub fn add(&mut self, rtt: f64) {
        let rtt = rtt.max(0.0);
        if self.count == 0 {
            self.srtt = rtt;
            self.rttvar = rtt / 2.0;
            self.min = rtt;
        } else {
            self.rttvar += 0.25 * ((self.srtt - rtt).abs() - self.rttvar);
            self.srtt += 0.125 * (rtt - self.srtt);
            self.min = self.min.min(rtt);
        }
        self.last = rtt;
        self.count += 1;
    }

    /// Smoothed RTT in milliseconds (for UI).
    pub fn ms(&self) -> f32 {
        (self.srtt * 1000.0) as f32
    }
}

/// Estimates the host's simulation tick from ping samples.
#[derive(Clone, Debug)]
pub struct ClockSync {
    hz: f64,
    /// (rtt, offset) for the last few samples. `host_tick ≈ local_secs * hz + offset`.
    window: VecDeque<(f64, f64)>,
    offset: f64,
    rtt: RttStats,
    /// Snap instead of slewing when the estimate moves by more than this many ticks.
    pub snap_ticks: f64,
    /// Fraction of the remaining error corrected per sample.
    pub slew: f64,
}

const WINDOW: usize = 16;

impl ClockSync {
    pub fn new(tick_hz: u32) -> Self {
        Self { hz: tick_hz as f64, window: VecDeque::with_capacity(WINDOW), offset: 0.0, rtt: RttStats::default(), snap_ticks: 12.0, slew: 0.2 }
    }

    /// Feed one ping result: the round trip took `rtt` seconds, the host said its tick was
    /// `host_tick_at_reply` when it replied, and we received the reply at local time `local_recv`.
    pub fn add_sample(&mut self, rtt: f64, host_tick_at_reply: f64, local_recv: f64) {
        let rtt = rtt.max(0.0);
        self.rtt.add(rtt);
        let sample_offset = host_tick_at_reply + rtt * 0.5 * self.hz - local_recv * self.hz;
        if self.window.len() == WINDOW {
            self.window.pop_front();
        }
        self.window.push_back((rtt, sample_offset));
        // The lowest-RTT sample in the window has the least queueing delay and asymmetry: trust it.
        let target = self.window.iter().min_by(|a, b| a.0.total_cmp(&b.0)).map(|s| s.1).unwrap_or(sample_offset);
        if self.rtt.count == 1 || (target - self.offset).abs() > self.snap_ticks {
            self.offset = target;
        } else {
            self.offset += (target - self.offset) * self.slew;
        }
    }

    pub fn is_synced(&self) -> bool {
        self.rtt.count > 0
    }

    /// Estimated (fractional) host tick at local time `now`.
    pub fn host_tick(&self, now: f64) -> f64 {
        now * self.hz + self.offset
    }

    pub fn rtt(&self) -> &RttStats {
        &self.rtt
    }

    /// How many ticks ahead of the host the client should simulate so that an input for tick T
    /// reaches the host shortly before it simulates T: one-way latency + 2×jitter + `buffer_ticks`
    /// (host jitter-buffer depth plus send batching; ~2–3 ticks is a good default at 60 Hz sends).
    pub fn lead_ticks(&self, buffer_ticks: f64) -> f64 {
        (self.rtt.srtt * 0.5 + 2.0 * self.rtt.rttvar) * self.hz + buffer_ticks
    }

    /// The tick the client should be simulating at local time `now`.
    pub fn target_client_tick(&self, now: f64, buffer_ticks: f64) -> f64 {
        self.host_tick(now) + self.lead_ticks(buffer_ticks)
    }
}

/// Result of [`TickPacer::advance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacerStep {
    /// First tick to simulate this frame.
    pub first_tick: u64,
    /// Number of ticks to simulate this frame (`first_tick..first_tick + count`).
    pub count: u32,
    /// The pacer jumped to re-sync with the target (predicted state should be re-based).
    pub snapped: bool,
}

/// Fixed-step accumulator that drifts towards a target tick with gentle time warping.
#[derive(Clone, Debug)]
pub struct TickPacer {
    hz: f64,
    /// Next tick to simulate.
    tick: u64,
    /// Fraction of a tick accumulated (0..1); use as the render interpolation alpha.
    acc: f64,
    /// Max ticks simulated per frame (spiral-of-death guard).
    pub max_ticks_per_frame: u32,
    /// Snap when further than this from the target.
    pub snap_ticks: f64,
    /// Max time warp (0.05 = ±5% speed).
    pub max_warp: f64,
    /// Warp per tick of error.
    pub gain: f64,
}

impl TickPacer {
    pub fn new(tick_hz: u32, start_tick: u64) -> Self {
        Self { hz: tick_hz as f64, tick: start_tick, acc: 0.0, max_ticks_per_frame: 12, snap_ticks: 30.0, max_warp: 0.05, gain: 0.02 }
    }

    /// Next tick that will be simulated.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Fraction of the next tick already elapsed — the render interpolation factor.
    pub fn alpha(&self) -> f32 {
        self.acc as f32
    }

    /// Advance by `dt` seconds of real time. `target_tick` is where the client should be at the
    /// *end* of this frame (e.g. `ClockSync::target_client_tick(now, ..)`). On a snap the pacer
    /// jumps straight to the target and simulates nothing this frame.
    pub fn advance(&mut self, dt: f64, target_tick: Option<f64>) -> PacerStep {
        let adv = dt.max(0.0) * self.hz;
        let mut scale = 1.0;
        if let Some(target) = target_tick {
            let err = target - (self.tick as f64 + self.acc + adv);
            if err.abs() > self.snap_ticks {
                let t = target.max(0.0);
                self.tick = t.floor() as u64;
                self.acc = t.fract();
                return PacerStep { first_tick: self.tick, count: 0, snapped: true };
            }
            scale = 1.0 + (err * self.gain).clamp(-self.max_warp, self.max_warp);
        }
        self.acc += adv * scale;
        let n = self.acc.floor();
        self.acc -= n;
        let count = (n as u64).min(self.max_ticks_per_frame as u64) as u32;
        let first_tick = self.tick;
        self.tick += count as u64;
        PacerStep { first_tick, count, snapped: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random jitter.
    fn rng(state: &mut u64) -> f64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        (*state >> 11) as f64 / (1u64 << 53) as f64
    }

    #[test]
    fn converges_with_jitter() {
        let hz = 120.0;
        let mut cs = ClockSync::new(120);
        // Host tick = 5000 + host_secs*hz; host clock is 3.7 s ahead of ours.
        let host_offset_secs = 3.7;
        let mut s = 0x1234_5678u64;
        let mut t = 10.0;
        for _ in 0..100 {
            let up = 0.030 + rng(&mut s) * 0.015;
            let down = 0.030 + rng(&mut s) * 0.015;
            let host_tick = 5000.0 + (t + up + host_offset_secs) * hz;
            let recv = t + up + down;
            cs.add_sample(up + down, host_tick, recv);
            t += 0.1;
        }
        let truth = 5000.0 + (t + host_offset_secs) * hz;
        let err = cs.host_tick(t) - truth;
        assert!(err.abs() < 1.5, "clock error {err} ticks");
        assert!((cs.rtt().srtt - 0.075).abs() < 0.01, "srtt {}", cs.rtt().srtt);
        let lead = cs.lead_ticks(2.0);
        assert!(lead > 6.0 && lead < 10.0, "lead {lead}");
    }

    #[test]
    fn pacer_tracks_and_snaps() {
        let mut p = TickPacer::new(120, 0);
        let mut target = 1000.0;
        let st = p.advance(1.0 / 60.0, Some(target));
        assert!(st.snapped);
        // Now run at 60 fps with the target 3 ticks ahead of us; we should catch up smoothly.
        target = p.tick() as f64 + p.alpha() as f64 + 3.0;
        let mut ticks = 0u32;
        for _ in 0..600 {
            target += 2.0;
            let st = p.advance(1.0 / 60.0, Some(target));
            assert!(!st.snapped);
            assert!(st.count <= 3);
            ticks += st.count;
        }
        let err = target - (p.tick() as f64 + p.alpha() as f64);
        assert!(err.abs() < 0.5, "err {err}");
        assert!(ticks >= 1200);
    }
}
