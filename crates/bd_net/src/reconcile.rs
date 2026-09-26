//! Client-side prediction + reconciliation.
//!
//! ```text
//! each client tick:   seq = inputs.push(input); rec.predict(seq, input, |s, i| step_player(s, i, world, DT));
//! on snapshot:        inputs.ack(ack); rec.reconcile(ack, my_state_from_snapshot, |s, i| step_player(...));
//! each render frame:  rec.update_smoothing(frame_dt); draw at rec.render_position();
//! ```
//! When the host's state for `ack` disagrees with what we predicted for `ack`, we reset to the
//! host's state and re-simulate every newer input. The visual jump is hidden by an error offset
//! that decays over ~100 ms (large errors such as respawns snap instead).

use std::collections::VecDeque;

use glam::Vec3;

/// A predicted state (usually the local player's movement state).
pub trait Reconcilable: Clone {
    /// Does `self` (our prediction) differ from `auth` (host truth) enough to need a correction?
    /// Use a small tolerance for floats.
    fn diverges_from(&self, auth: &Self) -> bool;
    /// Position used for visual error smoothing.
    fn render_position(&self) -> Vec3;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReconcileOutcome {
    /// Prediction matched the host; nothing changed.
    Match,
    /// State was reset to the host's and `replayed` inputs were re-simulated. `error` is the
    /// positional jump that is now being smoothed (zero if it was snapped).
    Corrected { replayed: usize, error: Vec3 },
    /// The acknowledgement is older than one already processed; ignored.
    Stale,
}

pub struct Reconciler<S, I> {
    current: S,
    /// (seq, input, predicted state after applying input), ascending by seq.
    history: VecDeque<(u32, I, S)>,
    last_ack: Option<u32>,
    offset: Vec3,
    /// Time for the visual error to (mostly, ~95%) disappear.
    pub smoothing_secs: f32,
    /// Errors larger than this snap instead of being smoothed (teleports, respawns).
    pub snap_distance: f32,
    /// Max predicted ticks kept (≈ 2 s at 120 Hz by default).
    pub capacity: usize,
    /// Number of corrections so far.
    pub corrections: u64,
}

impl<S: Reconcilable, I: Clone> Reconciler<S, I> {
    pub fn new(initial: S) -> Self {
        Self {
            current: initial,
            history: VecDeque::with_capacity(256),
            last_ack: None,
            offset: Vec3::ZERO,
            smoothing_secs: 0.1,
            snap_distance: 4.0,
            capacity: 256,
            corrections: 0,
        }
    }

    /// Latest predicted state.
    pub fn current(&self) -> &S {
        &self.current
    }

    /// Mutable access, e.g. to apply a host-dictated teleport outside the input stream.
    pub fn current_mut(&mut self) -> &mut S {
        &mut self.current
    }

    /// Hard reset (spawn, map change): clears history and smoothing.
    pub fn reset(&mut self, state: S) {
        self.current = state;
        self.history.clear();
        self.offset = Vec3::ZERO;
    }

    /// Predict one tick locally.
    pub fn predict(&mut self, seq: u32, input: I, mut step: impl FnMut(&mut S, &I)) {
        step(&mut self.current, &input);
        self.history.push_back((seq, input, self.current.clone()));
        while self.history.len() > self.capacity {
            self.history.pop_front();
        }
    }

    /// Apply an authoritative state: `auth` is the host's state after consuming input `ack_seq`.
    pub fn reconcile(&mut self, ack_seq: u32, auth: S, mut step: impl FnMut(&mut S, &I)) -> ReconcileOutcome {
        if self.last_ack.is_some_and(|a| ack_seq < a) {
            return ReconcileOutcome::Stale;
        }
        self.last_ack = Some(ack_seq);
        while self.history.front().is_some_and(|h| h.0 < ack_seq) {
            self.history.pop_front();
        }
        // No prediction for this seq (not started yet / history overflowed / host moved us
        // without consuming input) => re-base and replay anyway; it is cheap.
        let found = self.history.front().is_some_and(|h| h.0 == ack_seq);
        if found {
            let (_, _, predicted) = self.history.pop_front().unwrap();
            if !predicted.diverges_from(&auth) {
                return ReconcileOutcome::Match;
            }
        }
        let shown_before = self.current.render_position() + self.offset;
        let before = self.current.clone();
        let mut s = auth;
        for (_, input, predicted) in self.history.iter_mut() {
            step(&mut s, input);
            *predicted = s.clone();
        }
        self.current = s;
        if !found && !before.diverges_from(&self.current) {
            // Re-based to an equivalent result (e.g. the host has not consumed any input yet).
            return ReconcileOutcome::Match;
        }
        self.corrections += 1;
        let error = shown_before - self.current.render_position();
        if error.length() > self.snap_distance || !error.is_finite() {
            self.offset = Vec3::ZERO;
            ReconcileOutcome::Corrected { replayed: self.history.len(), error: Vec3::ZERO }
        } else {
            self.offset = error;
            ReconcileOutcome::Corrected { replayed: self.history.len(), error }
        }
    }

    /// Decay the visual error offset; call once per rendered frame.
    pub fn update_smoothing(&mut self, dt: f32) {
        // exp(-3) ≈ 5% left after `smoothing_secs`.
        let k = (-3.0 * dt / self.smoothing_secs.max(1e-4)).exp();
        self.offset *= k;
        if self.offset.length_squared() < 1e-8 {
            self.offset = Vec3::ZERO;
        }
    }

    /// Current visual correction offset (add to the rendered position).
    pub fn visual_offset(&self) -> Vec3 {
        self.offset
    }

    /// Where to draw the predicted entity (before render-interpolation between ticks).
    pub fn render_position(&self) -> Vec3 {
        self.current.render_position() + self.offset
    }

    /// Number of predicted, unacknowledged ticks.
    pub fn pending(&self) -> usize {
        self.history.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct P {
        pos: Vec3,
        vel: Vec3,
    }
    impl Reconcilable for P {
        fn diverges_from(&self, a: &Self) -> bool {
            (self.pos - a.pos).length() > 1e-3 || (self.vel - a.vel).length() > 1e-3
        }
        fn render_position(&self) -> Vec3 {
            self.pos
        }
    }
    fn step(s: &mut P, i: &f32) {
        s.vel.x = *i * 10.0;
        s.pos += s.vel / 120.0;
    }

    #[test]
    fn match_and_correct() {
        let mut host = P { pos: Vec3::ZERO, vel: Vec3::ZERO };
        let mut r: Reconciler<P, f32> = Reconciler::new(host.clone());
        for seq in 1..=10 {
            r.predict(seq, 1.0, step);
        }
        for _ in 1..=4 {
            step(&mut host, &1.0);
        }
        assert_eq!(r.reconcile(4, host.clone(), step), ReconcileOutcome::Match);
        assert_eq!(r.pending(), 6);
        // Host got knocked back at tick 5: +2 m in y.
        step(&mut host, &1.0);
        host.pos.y += 2.0;
        match r.reconcile(5, host.clone(), step) {
            ReconcileOutcome::Corrected { replayed, error } => {
                assert_eq!(replayed, 5);
                assert!((error.y + 2.0).abs() < 1e-5, "{error:?}");
            }
            o => panic!("{o:?}"),
        }
        assert!((r.current().pos.y - 2.0).abs() < 1e-6);
        // Visual position is continuous at the moment of correction...
        assert!(r.render_position().y.abs() < 1e-5);
        // ...and converges within ~100 ms.
        for _ in 0..7 {
            r.update_smoothing(1.0 / 60.0);
        }
        assert!(r.visual_offset().length() < 0.15, "{:?}", r.visual_offset());
        assert_eq!(r.reconcile(3, host.clone(), step), ReconcileOutcome::Stale);
        // Replayed predictions are now consistent with the host.
        for _ in 6..=10 {
            step(&mut host, &1.0);
        }
        assert_eq!(r.reconcile(10, host.clone(), step), ReconcileOutcome::Match);
        assert_eq!(r.corrections, 1);
    }

    #[test]
    fn big_error_snaps() {
        let mut r: Reconciler<P, f32> = Reconciler::new(P { pos: Vec3::ZERO, vel: Vec3::ZERO });
        r.predict(1, 0.0, step);
        let o = r.reconcile(1, P { pos: Vec3::new(50.0, 0.0, 0.0), vel: Vec3::ZERO }, step);
        assert_eq!(o, ReconcileOutcome::Corrected { replayed: 0, error: Vec3::ZERO });
        assert_eq!(r.render_position().x, 50.0);
    }
}
