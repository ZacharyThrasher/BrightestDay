//! Snapshot interpolation buffer for remote entities.
//!
//! Snapshots are stamped with the host tick they describe. The buffer learns how late snapshots
//! arrive (mean + deviation, in ticks) and how far apart they are, then keeps a smoothly advancing
//! *render tick* a little behind the newest data: `delay = max(min_snapshots × interval,
//! interval + jitter_mult × jitter + extra)`. The render clock is steered with gentle time warping
//! (±10%) so it never jumps unless the stream breaks badly.

use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct InterpConfig {
    pub tick_hz: u32,
    /// Never render closer than this many snapshot intervals behind the newest (design: ~2).
    pub min_delay_snapshots: f64,
    /// Multiplier on the measured arrival jitter (ticks).
    pub jitter_mult: f64,
    /// Constant safety margin (ticks).
    pub extra_ticks: f64,
    /// Hard ceiling on the delay (ticks).
    pub max_delay_ticks: f64,
    /// When starved, allow extrapolating past the newest snapshot by at most this many ticks.
    pub max_extrapolate_ticks: f64,
    /// Max stored snapshots.
    pub capacity: usize,
}

impl Default for InterpConfig {
    fn default() -> Self {
        Self {
            tick_hz: bd_core::TICK_HZ,
            min_delay_snapshots: 2.0,
            jitter_mult: 3.0,
            extra_ticks: 1.0,
            max_delay_ticks: 36.0,
            max_extrapolate_ticks: 0.0,
            capacity: 64,
        }
    }
}

/// Two snapshots and a blend factor: `value = lerp(from, to, alpha)`.
#[derive(Debug)]
pub struct Interp<'a, T> {
    pub from: &'a T,
    pub to: &'a T,
    pub from_tick: u64,
    pub to_tick: u64,
    /// 0 → `from`, 1 → `to`; may exceed 1 only when extrapolation is enabled.
    pub alpha: f32,
    pub render_tick: f64,
}

pub struct SnapshotBuffer<T> {
    pub cfg: InterpConfig,
    snaps: VecDeque<(u64, T)>,
    /// Mean lateness in ticks: newest tick ≈ now·hz − late_mean.
    late_mean: f64,
    late_dev: f64,
    interval: f64,
    render_tick: f64,
    last_now: Option<f64>,
    received: u64,
    /// Number of times the render clock had to snap.
    pub snaps_count: u64,
    /// Frames on which we had no newer snapshot to interpolate towards.
    pub starved_frames: u64,
}

impl<T> SnapshotBuffer<T> {
    pub fn new(cfg: InterpConfig) -> Self {
        Self {
            cfg,
            snaps: VecDeque::with_capacity(64),
            late_mean: 0.0,
            late_dev: 0.0,
            interval: 2.0,
            render_tick: 0.0,
            last_now: None,
            received: 0,
            snaps_count: 0,
            starved_frames: 0,
        }
    }

    /// Add a snapshot for host tick `tick`, received at local time `now` (seconds).
    /// Older-or-equal ticks than the newest are ignored.
    pub fn push(&mut self, tick: u64, snap: T, now: f64) {
        let hz = self.cfg.tick_hz as f64;
        let late = now * hz - tick as f64;
        match self.snaps.back() {
            Some((newest, _)) if tick <= *newest => return,
            Some((newest, _)) => {
                let d = (tick - newest) as f64;
                self.interval += 0.1 * (d.min(60.0) - self.interval);
                self.late_dev += 0.1 * ((late - self.late_mean).abs() - self.late_dev);
                self.late_mean += 0.1 * (late - self.late_mean);
            }
            None if self.received == 0 => {
                self.late_mean = late;
            }
            None => {
                self.late_dev += 0.1 * ((late - self.late_mean).abs() - self.late_dev);
                self.late_mean += 0.1 * (late - self.late_mean);
            }
        }
        self.received += 1;
        self.snaps.push_back((tick, snap));
        while self.snaps.len() > self.cfg.capacity {
            self.snaps.pop_front();
        }
    }

    /// Current interpolation delay target in ticks.
    pub fn delay_ticks(&self) -> f64 {
        let c = &self.cfg;
        (self.interval * c.min_delay_snapshots).max(self.interval + c.jitter_mult * self.late_dev + c.extra_ticks).min(c.max_delay_ticks)
    }

    /// Measured arrival jitter (mean deviation, ticks).
    pub fn jitter_ticks(&self) -> f64 {
        self.late_dev
    }

    /// Measured snapshot spacing (ticks).
    pub fn interval_ticks(&self) -> f64 {
        self.interval
    }

    /// The (fractional) host tick currently being rendered.
    pub fn render_tick(&self) -> f64 {
        self.render_tick
    }

    pub fn latest(&self) -> Option<(u64, &T)> {
        self.snaps.back().map(|(t, s)| (*t, s))
    }

    pub fn len(&self) -> usize {
        self.snaps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snaps.is_empty()
    }

    /// Advance the render clock to local time `now` and return what to draw.
    pub fn update(&mut self, now: f64) -> Option<Interp<'_, T>> {
        if self.snaps.is_empty() {
            return None;
        }
        let hz = self.cfg.tick_hz as f64;
        let target = now * hz - self.late_mean - self.delay_ticks();
        match self.last_now {
            None => self.render_tick = target,
            Some(prev) => {
                let dt = (now - prev).max(0.0);
                let err = target - self.render_tick;
                let snap_at = (self.delay_ticks() * 2.0).max(12.0);
                if err.abs() > snap_at {
                    self.render_tick = target;
                    self.snaps_count += 1;
                } else {
                    let rate = 1.0 + (err * 0.05).clamp(-0.1, 0.1);
                    self.render_tick += dt * hz * rate;
                }
            }
        }
        self.last_now = Some(now);

        // Drop snapshots we will never need again (keep the one just before render_tick).
        while self.snaps.len() >= 3 && (self.snaps[1].0 as f64) <= self.render_tick {
            self.snaps.pop_front();
        }
        let rt = self.render_tick;
        let (t0, s0) = &self.snaps[0];
        if rt <= *t0 as f64 || self.snaps.len() == 1 {
            if rt > *t0 as f64 {
                self.starved_frames += 1;
            }
            return Some(Interp { from: s0, to: s0, from_tick: *t0, to_tick: *t0, alpha: 0.0, render_tick: rt });
        }
        let (t1, s1) = &self.snaps[1];
        let span = (*t1 - *t0) as f64;
        let mut alpha = (rt - *t0 as f64) / span;
        if alpha > 1.0 {
            // Only possible when snaps[1] is the newest: we are starved.
            self.starved_frames += 1;
            let max_a = 1.0 + self.cfg.max_extrapolate_ticks / span;
            alpha = alpha.min(max_a);
        }
        Some(Interp { from: s0, to: s1, from_tick: *t0, to_tick: *t1, alpha: alpha as f32, render_tick: rt })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(state: &mut u64) -> f64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        (*state >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Host sends a position x = tick/120 m (1 m/s) at 60 Hz with 40 ms ± 15 ms latency (in-order,
    /// like TCP). Client renders at 144 fps. The rendered value must be smooth and never starve.
    #[test]
    fn smooth_under_jitter() {
        let hz = 120.0;
        let mut buf = SnapshotBuffer::new(InterpConfig::default());
        let mut s = 99u64;
        let mut arrivals = Vec::new();
        let mut last_arrival: f64 = 0.0;
        for k in 0..600u64 {
            let tick = 1000 + k * 2;
            let send = tick as f64 / hz;
            let arr = (send + 0.040 + rng(&mut s) * 0.015).max(last_arrival);
            last_arrival = arr;
            arrivals.push((arr, tick));
        }
        let mut i = 0;
        let mut now = arrivals[0].0;
        let mut prev_x: Option<f64> = None;
        let mut max_step_dev: f64 = 0.0;
        let frame = 1.0 / 144.0;
        let mut frames = 0;
        while now < arrivals.last().unwrap().0 {
            while i < arrivals.len() && arrivals[i].0 <= now {
                let t = arrivals[i].1;
                buf.push(t, t as f64 / hz, arrivals[i].0);
                i += 1;
            }
            let x = {
                let it = buf.update(now).unwrap();
                it.from + (it.to - it.from) * it.alpha as f64
            };
            frames += 1;
            if frames > 144 {
                // Expected per-frame advance is 1/144 m; allow up to ±10% time warp.
                let step = x - prev_x.unwrap();
                max_step_dev = max_step_dev.max((step - frame).abs() / frame);
            }
            prev_x = Some(x);
            now += frame;
        }
        assert!(max_step_dev <= 0.11, "max step deviation {max_step_dev}");
        assert_eq!(buf.snaps_count, 0);
        assert!(buf.starved_frames < 5, "starved {}", buf.starved_frames);
        let d = buf.delay_ticks();
        assert!(d >= 4.0 && d < 12.0, "delay {d}");
    }
}
