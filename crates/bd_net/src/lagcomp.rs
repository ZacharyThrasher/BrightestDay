//! Host-side lag compensation history.
//!
//! The host records every player's hull (or any state) each tick. When a client fires a hitscan /
//! melee, the host rewinds the *other* players to the tick the shooter was looking at
//! (`QueuedInput::view_tick`), clamped to at most `max_rewind` in the past, and tests hits there.

use std::collections::VecDeque;

use glam::{Quat, Vec2, Vec3};

/// Linear interpolation for rewound values.
pub trait Lerp: Sized {
    fn lerp_to(&self, other: &Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp_to(&self, o: &Self, t: f32) -> Self {
        self + (o - self) * t
    }
}
impl Lerp for f64 {
    fn lerp_to(&self, o: &Self, t: f32) -> Self {
        self + (o - self) * t as f64
    }
}
impl Lerp for Vec2 {
    fn lerp_to(&self, o: &Self, t: f32) -> Self {
        self.lerp(*o, t)
    }
}
impl Lerp for Vec3 {
    fn lerp_to(&self, o: &Self, t: f32) -> Self {
        self.lerp(*o, t)
    }
}
impl Lerp for Quat {
    fn lerp_to(&self, o: &Self, t: f32) -> Self {
        self.slerp(*o, t)
    }
}
impl<A: Lerp, B: Lerp> Lerp for (A, B) {
    fn lerp_to(&self, o: &Self, t: f32) -> Self {
        (self.0.lerp_to(&o.0, t), self.1.lerp_to(&o.1, t))
    }
}
impl<T: Lerp + Clone> Lerp for Option<T> {
    /// `None` on either side (e.g. dead player) → no interpolation, take the nearer side.
    fn lerp_to(&self, o: &Self, t: f32) -> Self {
        match (self, o) {
            (Some(a), Some(b)) => Some(a.lerp_to(b, t)),
            _ => {
                if t < 0.5 {
                    self.clone()
                } else {
                    o.clone()
                }
            }
        }
    }
}

/// Two recorded ticks bracketing the requested time.
#[derive(Debug)]
pub struct Rewind<'a, T> {
    pub from: &'a T,
    pub to: &'a T,
    pub from_tick: u64,
    pub to_tick: u64,
    pub alpha: f32,
    /// The tick actually used after clamping to the stored window.
    pub tick: f64,
}

/// Ring of per-tick records covering the last `window_ticks`.
#[derive(Clone, Debug)]
pub struct LagCompHistory<T> {
    entries: VecDeque<(u64, T)>,
    window_ticks: u64,
}

impl<T> LagCompHistory<T> {
    /// Keep `max_ms` milliseconds of history at `tick_hz`.
    pub fn new(max_ms: u32, tick_hz: u32) -> Self {
        let window_ticks = (max_ms as u64 * tick_hz as u64).div_ceil(1000).max(1);
        Self { entries: VecDeque::with_capacity(window_ticks as usize + 2), window_ticks }
    }

    /// Record the value at `tick` (ticks must increase; a repeated tick overwrites).
    pub fn record(&mut self, tick: u64, value: T) {
        match self.entries.back_mut() {
            Some((t, v)) if *t == tick => {
                *v = value;
                return;
            }
            Some((t, _)) if *t > tick => {
                // Time went backwards (map restart): start over.
                self.entries.clear();
            }
            _ => {}
        }
        self.entries.push_back((tick, value));
        while self.entries.front().is_some_and(|(t, _)| t + self.window_ticks < tick) {
            self.entries.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn oldest_tick(&self) -> Option<u64> {
        self.entries.front().map(|e| e.0)
    }

    pub fn newest_tick(&self) -> Option<u64> {
        self.entries.back().map(|e| e.0)
    }

    /// Exact record for a tick.
    pub fn get(&self, tick: u64) -> Option<&T> {
        let first = self.entries.front()?.0;
        let idx = tick.checked_sub(first)? as usize;
        match self.entries.get(idx) {
            Some((t, v)) if *t == tick => Some(v),
            // Ticks were skipped; fall back to a search.
            _ => self.entries.iter().find(|(t, _)| *t == tick).map(|(_, v)| v),
        }
    }

    /// Bracketing records for fractional `tick`, clamped to the stored window.
    pub fn rewind(&self, tick: f64) -> Option<Rewind<'_, T>> {
        let (first, _) = self.entries.front()?;
        let (last, _) = self.entries.back()?;
        let t = tick.clamp(*first as f64, *last as f64);
        // Binary search for the last entry with tick <= t.
        let idx = self.entries.partition_point(|(et, _)| (*et as f64) <= t).saturating_sub(1);
        let (a_t, a) = &self.entries[idx];
        let (b_t, b) = self.entries.get(idx + 1).map(|(bt, b)| (bt, b)).unwrap_or((a_t, a));
        let alpha = if b_t > a_t { ((t - *a_t as f64) / (*b_t - *a_t) as f64) as f32 } else { 0.0 };
        Some(Rewind { from: a, to: b, from_tick: *a_t, to_tick: *b_t, alpha, tick: t })
    }

    /// Interpolated value at fractional `tick` (clamped to the window).
    pub fn sample(&self, tick: f64) -> Option<T>
    where
        T: Lerp,
    {
        let r = self.rewind(tick)?;
        Some(r.from.lerp_to(r.to, r.alpha))
    }
}

/// Clamp a client-claimed view tick to `[now - max_rewind_ticks, now]` so cheaters / very laggy
/// players cannot rewind arbitrarily far.
pub fn clamp_view_tick(view_tick: f64, now_tick: u64, max_rewind_ticks: f64) -> f64 {
    let now = now_tick as f64;
    view_tick.clamp(now - max_rewind_ticks, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_rewinds() {
        let mut h = LagCompHistory::new(250, 120);
        for t in 0..200u64 {
            h.record(t, Vec3::new(t as f32, 0.0, 0.0));
        }
        assert_eq!(h.newest_tick(), Some(199));
        assert_eq!(h.oldest_tick(), Some(199 - 30));
        assert_eq!(h.get(180).unwrap().x, 180.0);
        assert!(h.get(10).is_none());
        let v = h.sample(185.25).unwrap();
        assert!((v.x - 185.25).abs() < 1e-4);
        // Clamped to the window.
        assert_eq!(h.sample(0.0).unwrap().x, 169.0);
        assert_eq!(h.sample(1e9).unwrap().x, 199.0);
        let r = h.rewind(190.5).unwrap();
        assert_eq!((r.from_tick, r.to_tick), (190, 191));
        assert!((r.alpha - 0.5).abs() < 1e-6);
        assert_eq!(clamp_view_tick(10.0, 199, 24.0), 175.0);
        // Option: dead players stay dead.
        let mut o: LagCompHistory<Option<Vec3>> = LagCompHistory::new(100, 120);
        o.record(1, Some(Vec3::ONE));
        o.record(2, None);
        assert_eq!(o.sample(1.2).unwrap(), Some(Vec3::ONE));
        assert_eq!(o.sample(1.8).unwrap(), None);
    }
}
