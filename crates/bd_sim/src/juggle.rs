//! Juggle protection (owner: "put a cooldown after a player has been hit so they can't just be
//! knocked around endlessly"; COMBAT.md §1a "Juggle protection").
//!
//! Per victim, host-side ([`crate::world::Player::juggle`]):
//! * A **chain hit** is one whose knockback (Smash formula, before this module's decay) is at least
//!   [`JuggleTuning::chain_min_kb`] — the hits that take control away. Every hit a victim takes in
//!   the same tick is one chain step (a super-shotgun's 9 pellets, an explosion + its chain arc).
//! * The first chain hit is untouched (single big hits still KO, star KOs included). Every later hit
//!   inside the [`JuggleTuning::window`] (refreshed by each chain hit) has its knockback × the next
//!   entry of [`JuggleTuning::kb_decay`] (the last entry repeats) and its damage ×
//!   [`JuggleTuning::dmg_decay`] — combos stay rewarding, never infinite.
//! * After [`JuggleTuning::max_hits`] chain hits the victim is **guarded** for
//!   [`JuggleTuning::immune_secs`]: hits still land (events, statuses, damage × `immune_dmg`) but
//!   push nobody anywhere. The client draws a white hard-light shimmer on a guarded body (status
//!   byte bit [`GUARD_STATUS_BIT`]) — no UI text.
//! * The chain (and a guard) resets when it times out, or the victim is back in control: standing on
//!   the ground out of tumble, hanging on their own tether, or teching.
//!
//! Knockback reaches a predicting client through its exact movement state (`OwnState`), so the
//! protection needs no prediction state of its own: the host decides, prediction replays the result.
//! `launch_player` (tools / tests / scripted launches) and `WallSplat` (the impact of the launch
//! itself) bypass it.

use bd_core::Tick;
use serde::{Deserialize, Serialize};

/// Bit of `PlayerSnap::status` (above every `StatusKind` bit) set while the player is guarded.
pub const GUARD_STATUS_BIT: u8 = 1 << 7;

/// Juggle protection numbers (`assets/tuning/combat.ron` → `juggle`, hot reload). Never shown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JuggleTuning {
    /// Off = every hit at full strength (the pre-protection sim, bit-identical).
    pub enabled: bool,
    /// Knockback (m/s, after %, weight and the lobby scale; before the decay) at which a hit counts
    /// as a chain hit. Default = `hit.tumble_impulse` (a hit that tumbles you).
    pub chain_min_kb: f32,
    /// Seconds after a chain hit during which later hits decay (each chain hit refreshes it).
    pub window: f32,
    /// Knockback × for the 2nd, 3rd, … hit of a chain (the last entry repeats).
    pub kb_decay: Vec<f32>,
    /// Damage × for every later hit inside the window.
    pub dmg_decay: f32,
    /// Chain hits (first included) before the victim is guarded.
    pub max_hits: u8,
    /// Seconds of the guard (no knockback, no launch).
    pub immune_secs: f32,
    /// Damage × while guarded (hits still register).
    pub immune_dmg: f32,
}

impl Default for JuggleTuning {
    fn default() -> Self {
        Self { enabled: true, chain_min_kb: 14.0, window: 1.0, kb_decay: vec![0.6, 0.35], dmg_decay: 0.75, max_hits: 3, immune_secs: 0.7, immune_dmg: 0.5 }
    }
}

impl JuggleTuning {
    /// Off: the pre-protection sim (A/B, tests of other systems).
    pub fn off() -> Self {
        Self { enabled: false, ..Self::default() }
    }

    pub(crate) fn validated(mut self) -> Self {
        let d = Self::default();
        let fin = |v: f32, d: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.chain_min_kb = fin(self.chain_min_kb, d.chain_min_kb, 0.0, 500.0);
        self.window = fin(self.window, d.window, 0.0, 10.0);
        self.kb_decay.truncate(8);
        for k in &mut self.kb_decay {
            *k = fin(*k, 1.0, 0.0, 1.0);
        }
        self.dmg_decay = fin(self.dmg_decay, d.dmg_decay, 0.0, 1.0);
        self.max_hits = self.max_hits.clamp(1, 20);
        self.immune_secs = fin(self.immune_secs, d.immune_secs, 0.0, 5.0);
        self.immune_dmg = fin(self.immune_dmg, d.immune_dmg, 0.0, 1.0);
        self
    }

    /// Knockback × of the hit after `n` completed chain steps (`n ≥ 1`).
    fn kb_mult(&self, n: u8) -> f32 {
        let i = (n as usize).saturating_sub(1);
        self.kb_decay.get(i).or(self.kb_decay.last()).copied().unwrap_or(1.0)
    }
}

/// One victim's chain (host-side; reset on death / respawn).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct JuggleState {
    /// Chain steps so far (0 = no chain).
    pub hits: u8,
    /// Seconds left in the window.
    pub window: f32,
    /// Seconds of guard left.
    pub immune: f32,
    /// Tick of the latest chain step (hits in that same tick share it).
    pub step_tick: Tick,
}

impl JuggleState {
    /// Knockback-immune right now (the client's shimmer).
    #[inline]
    pub fn guarded(&self) -> bool {
        self.immune > 0.0
    }

    /// (damage ×, knockback ×) for a hit landing at tick `now`.
    pub fn scale(&self, t: &JuggleTuning, now: Tick) -> (f32, f32) {
        if !t.enabled || self.hits == 0 {
            return (1.0, 1.0);
        }
        // Another hit in the tick of the latest step shares that step's numbers.
        let same = self.step_tick == now;
        if self.immune > 0.0 && !same {
            return (t.immune_dmg, 0.0);
        }
        let n = if same { self.hits - 1 } else { self.hits };
        if n == 0 || (!same && self.window <= 0.0) {
            return (1.0, 1.0);
        }
        (t.dmg_decay, t.kb_mult(n))
    }

    /// Record a hit of pre-decay knockback `raw_kb` at tick `now`. True when it starts the guard.
    pub fn on_hit(&mut self, t: &JuggleTuning, raw_kb: f32, now: Tick) -> bool {
        if !t.enabled || !(raw_kb >= t.chain_min_kb) || self.immune > 0.0 || (self.hits > 0 && self.step_tick == now) {
            return false;
        }
        self.hits = self.hits.saturating_add(1);
        self.step_tick = now;
        self.window = t.window;
        if self.hits >= t.max_hits {
            self.immune = t.immune_secs;
            return true;
        }
        false
    }

    /// One tick: timers run down; back in control (grounded out of tumble, own tether attached,
    /// tech) ends the chain and any guard.
    pub fn step(&mut self, dt: f32, in_control: bool) {
        if self.hits == 0 {
            return;
        }
        if in_control {
            *self = Self::default();
            return;
        }
        if self.immune > 0.0 {
            self.immune -= dt;
            if self.immune <= 0.0 {
                *self = Self::default();
            }
            return;
        }
        self.window -= dt;
        if self.window <= 0.0 {
            *self = Self::default();
        }
    }
}

/// Per-tick update of every player's chain, right after movement (so "grounded" is this tick's).
pub(crate) fn step(w: &mut crate::World, dt: f32) {
    if !w.cfg.tuning.juggle.enabled {
        for p in &mut w.players {
            p.juggle = JuggleState::default();
        }
        return;
    }
    let crate::World { players, events, .. } = w;
    for p in players.iter_mut() {
        if p.juggle.hits == 0 {
            continue;
        }
        if !p.alive {
            p.juggle = JuggleState::default();
            continue;
        }
        let teched = events.iter().any(|e| matches!(e, crate::events::GameEvent::Tech { player, .. } if *player == p.id));
        let in_control = (p.mv.grounded && p.mv.tumble <= 0.0) || p.mv.tethered() || teched;
        p.juggle.step(dt, in_control);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_decays_then_guards_then_times_out() {
        let t = JuggleTuning::default();
        let mut s = JuggleState::default();
        assert_eq!(s.scale(&t, 10), (1.0, 1.0));
        assert!(!s.on_hit(&t, 30.0, 10));
        // Same tick (pellets): same numbers, no extra step.
        assert_eq!(s.scale(&t, 10), (1.0, 1.0));
        assert!(!s.on_hit(&t, 30.0, 10));
        assert_eq!(s.hits, 1);
        assert_eq!(s.scale(&t, 20), (0.75, 0.6));
        assert!(!s.on_hit(&t, 30.0, 20));
        assert_eq!(s.scale(&t, 30), (0.75, 0.35));
        assert!(s.on_hit(&t, 30.0, 30), "third chain hit starts the guard");
        assert_eq!(s.scale(&t, 30), (0.75, 0.35), "the rest of that tick shares the third step");
        assert_eq!(s.scale(&t, 31), (0.5, 0.0));
        assert!(s.guarded());
        for _ in 0..85 {
            s.step(1.0 / 120.0, false);
        }
        assert!(!s.guarded() && s.hits == 0, "guard over, chain reset: {s:?}");
        assert_eq!(s.scale(&t, 200), (1.0, 1.0));
    }

    #[test]
    fn small_hits_do_not_chain_and_control_resets() {
        let t = JuggleTuning::default();
        let mut s = JuggleState::default();
        assert!(!s.on_hit(&t, 5.0, 1));
        assert_eq!(s.hits, 0);
        s.on_hit(&t, 20.0, 2);
        assert_eq!(s.scale(&t, 3), (0.75, 0.6), "any later hit in the window decays");
        s.step(1.0 / 120.0, true);
        assert_eq!(s, JuggleState::default());
        s.on_hit(&t, 20.0, 5);
        for _ in 0..121 {
            s.step(1.0 / 120.0, false);
        }
        assert_eq!(s, JuggleState::default(), "window timed out");
        let off = JuggleTuning::off();
        assert!(!s.on_hit(&off, 99.0, 9));
        assert_eq!(s.scale(&off, 9), (1.0, 1.0));
    }

    #[test]
    fn hostile_tuning_is_clamped() {
        let t = JuggleTuning { window: f32::NAN, kb_decay: vec![2.0, -1.0, f32::INFINITY], max_hits: 0, ..Default::default() }.validated();
        assert_eq!(t.window, 1.0);
        assert_eq!(t.kb_decay, vec![1.0, 0.0, 1.0]);
        assert_eq!(t.max_hits, 1);
        assert_eq!(JuggleTuning::default().validated(), JuggleTuning::default());
    }
}
