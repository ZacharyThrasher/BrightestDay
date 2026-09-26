//! Lag-compensation history: every player's hull for the last [`HISTORY_TICKS`] ticks (~267 ms),
//! in fixed arrays (no allocation). Hit tests for a shooter rewind the *other* players to the
//! shooter's view tick (`bd_net::InputPacket::view_tick`, fractional ⇒ interpolated).

use bd_core::voxel::Aabb;
use bd_core::{MAX_PLAYERS, PlayerId, Tick, Vec3};

/// Ticks of history kept (32 ticks ≈ 267 ms at 120 Hz).
pub const HISTORY_TICKS: usize = 32;
/// Maximum rewind honoured (ticks). Older view ticks are clamped (≈ 250 ms).
pub const MAX_REWIND_TICKS: f64 = 30.0;

#[derive(Clone, Copy)]
struct Frame {
    tick: Tick,
    n: u8,
    ids: [u8; MAX_PLAYERS],
    hulls: [Aabb; MAX_PLAYERS],
}

const EMPTY_BOX: Aabb = Aabb { min: Vec3::ZERO, max: Vec3::ZERO };

impl Frame {
    const EMPTY: Frame = Frame { tick: Tick::MAX, n: 0, ids: [0; MAX_PLAYERS], hulls: [EMPTY_BOX; MAX_PLAYERS] };

    fn get(&self, id: PlayerId) -> Option<Aabb> {
        (0..self.n as usize).find(|&i| self.ids[i] == id.0).map(|i| self.hulls[i])
    }
}

/// Ring buffer of per-tick player hulls.
pub struct HullHistory {
    frames: [Frame; HISTORY_TICKS],
    newest: Option<Tick>,
}

impl Default for HullHistory {
    fn default() -> Self {
        Self { frames: [Frame::EMPTY; HISTORY_TICKS], newest: None }
    }
}

impl HullHistory {
    /// Record the hulls of living players at `tick` (call once per tick, ticks increasing).
    pub fn record(&mut self, tick: Tick, hulls: impl Iterator<Item = (PlayerId, Aabb)>) {
        let f = &mut self.frames[(tick % HISTORY_TICKS as Tick) as usize];
        f.tick = tick;
        f.n = 0;
        for (id, h) in hulls {
            if (f.n as usize) < MAX_PLAYERS {
                f.ids[f.n as usize] = id.0;
                f.hulls[f.n as usize] = h;
                f.n += 1;
            }
        }
        self.newest = Some(tick);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn newest(&self) -> Option<Tick> {
        self.newest
    }

    fn frame(&self, tick: Tick) -> Option<&Frame> {
        let f = &self.frames[(tick % HISTORY_TICKS as Tick) as usize];
        (f.tick == tick).then_some(f)
    }

    /// Clamp a (fractional) view tick into the honoured window.
    pub fn clamp_view(&self, view_tick: f64) -> Option<f64> {
        let now = self.newest? as f64;
        if !view_tick.is_finite() {
            return Some(now);
        }
        Some(view_tick.clamp(now - MAX_REWIND_TICKS, now))
    }

    /// Hull of `id` at fractional tick `view_tick` (interpolated between recorded ticks).
    pub fn hull_at(&self, id: PlayerId, view_tick: f64) -> Option<Aabb> {
        let t = self.clamp_view(view_tick)?;
        let t0 = t.floor() as Tick;
        let frac = (t - t0 as f64) as f32;
        let a = self.frame(t0).and_then(|f| f.get(id));
        let b = self.frame(t0 + 1).and_then(|f| f.get(id));
        match (a, b) {
            (Some(a), Some(b)) => Some(Aabb { min: a.min.lerp(b.min, frac), max: a.max.lerp(b.max, frac) }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// All players recorded at (fractional) `view_tick`: (id, hull).
    pub fn for_each_at(&self, view_tick: f64, mut f: impl FnMut(PlayerId, Aabb)) {
        let Some(t) = self.clamp_view(view_tick) else { return };
        let Some(fr) = self.frame(t.floor() as Tick) else { return };
        for i in 0..fr.n as usize {
            let id = PlayerId(fr.ids[i]);
            if let Some(h) = self.hull_at(id, t) {
                f(id, h);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hull_at_x(x: f32) -> Aabb {
        Aabb { min: Vec3::new(x - 0.4, 0.0, -0.4), max: Vec3::new(x + 0.4, 1.8, 0.4) }
    }

    #[test]
    fn rewinds_and_interpolates() {
        let mut h = HullHistory::default();
        for t in 0..100u64 {
            h.record(t, std::iter::once((PlayerId(1), hull_at_x(t as f32))));
        }
        let a = h.hull_at(PlayerId(1), 90.5).unwrap();
        assert!((a.center().x - 90.5).abs() < 1e-4);
        // Older than the window is clamped to 30 ticks back.
        let b = h.hull_at(PlayerId(1), 10.0).unwrap();
        assert!((b.center().x - 69.0).abs() < 1e-4);
        assert!(h.hull_at(PlayerId(2), 95.0).is_none());
    }
}
