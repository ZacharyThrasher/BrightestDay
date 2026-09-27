//! Charging — the anime power-up (owner request; COMBAT.md §2 "Charging"). **Hold** the charge
//! button to refill willpower: the easy, loud, exposed refill (the oath recital stays the skilful,
//! faster one).
//!
//! * Willpower rises at [`ChargeTuning::rate_per_sec`] (× the lobby willpower mode's regen
//!   multiplier), ramping in quadratically over `ramp_secs`, so tapping is inefficient. Passive
//!   regen keeps running as usual; charging ignores the post-cast regen pause.
//! * While charging you can't attack, cast or use powers ([`crate::Player::busy`]). On the ground
//!   your feet are planted (`ground_speed_mult`); in the air you just drift (`air_control_mult`).
//! * Getting hit (any damage) interrupts it ([`ChargeEvent::Broken`]); still holding, it restarts
//!   from a cold ramp after `stagger_secs`. Releasing ends it. That's all the rules.
//!
//! The movement slow-down is a pure function of the input ([`slows`]: button held, not typing), so
//! client prediction matches the host without replicated timers. Replicated for presentation:
//! `pflags::CHARGING` + `PlayerSnap::charge` (hold time, 1/20 s steps) — everyone sees who is
//! charging. Events: [`GameEvent::Charge`] (start / full / broken / released) for audio + VFX.

use bd_core::input::{PlayerInput, buttons};
use serde::{Deserialize, Serialize};

use crate::events::GameEvent;
use crate::movement::MoveModifiers;
use crate::rng::SimRng;
use crate::world::Player;

/// Charging numbers (`assets/tuning/combat.ron` → `charge`, hot reload). Never shown to players.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChargeTuning {
    /// Willpower per second at full ramp (on top of passive regen). The oath on target is ~7/s
    /// and up to ~10/s when fast, so charging stays the slower refill.
    pub rate_per_sec: f32,
    /// Seconds for the rate to ramp 0 → full (quadratic): tapping earns almost nothing.
    pub ramp_secs: f32,
    /// After a hit, seconds before a still-held button starts charging again (ramp restarts).
    pub stagger_secs: f32,
    /// Run / wish speed multiplier on the ground while holding (feet planted).
    pub ground_speed_mult: f32,
    /// Air acceleration multiplier while holding (you drift / fall).
    pub air_control_mult: f32,
    /// Jump speed multiplier while holding (a small hop, not a bhop).
    pub jump_mult: f32,
}

impl Default for ChargeTuning {
    fn default() -> Self {
        Self { rate_per_sec: 4.5, ramp_secs: 0.8, stagger_secs: 0.6, ground_speed_mult: 0.2, air_control_mult: 0.3, jump_mult: 0.5 }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl ChargeTuning {
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.rate_per_sec = fin(self.rate_per_sec, d.rate_per_sec, 0.0, 100.0);
        self.ramp_secs = fin(self.ramp_secs, d.ramp_secs, 0.0, 10.0);
        self.stagger_secs = fin(self.stagger_secs, d.stagger_secs, 0.0, 10.0);
        self.ground_speed_mult = fin(self.ground_speed_mult, d.ground_speed_mult, 0.0, 1.0);
        self.air_control_mult = fin(self.air_control_mult, d.air_control_mult, 0.0, 1.0);
        self.jump_mult = fin(self.jump_mult, d.jump_mult, 0.0, 1.0);
        self
    }

    /// Refill rate (willpower/s, before the lobby multiplier) `held` seconds into a charge.
    #[inline]
    pub fn rate_at(&self, held: f32) -> f32 {
        let k = if self.ramp_secs > 0.0 { (held / self.ramp_secs).clamp(0.0, 1.0) } else { 1.0 };
        self.rate_per_sec * k * k
    }
}

/// Per-player charging state (host-side; clients mirror `active` + `held` from snapshots).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChargeState {
    /// Powering up this tick: aura on, refilling, can't act.
    pub active: bool,
    /// Seconds since this charge started (ramp; aura size).
    pub held: f32,
    /// After a hit: seconds before holding the button charges again.
    pub stagger: f32,
    /// The pool filled during this charge (`Full` is announced once).
    pub full: bool,
}

/// Replicated hold time: 1/20 s steps, saturating (12.75 s).
pub const WIRE_STEPS_PER_SEC: f32 = 20.0;

impl ChargeState {
    /// `PlayerSnap::charge`: 0 = not charging, else hold time in 1/20 s (≥ 1).
    pub fn wire(&self) -> u8 {
        if !self.active {
            return 0;
        }
        (self.held * WIRE_STEPS_PER_SEC).round().clamp(1.0, 255.0) as u8
    }

    /// Mirror a snapshot (client replica).
    pub fn apply_wire(&mut self, charging: bool, level: u8) {
        self.active = charging;
        self.held = if charging { level as f32 / WIRE_STEPS_PER_SEC } else { 0.0 };
        self.stagger = 0.0;
    }
}

/// What happened to a charge (audio + VFX hooks).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChargeEvent {
    /// Started powering up.
    Start,
    /// The pool filled during this charge (the burst moment).
    Full,
    /// Interrupted by a hit (or a stun / launch / seat).
    Broken,
    /// Released (or opened the ring / oath).
    Released,
}

/// Is the charge button held for movement purposes (feet planted)? Pure function of the input:
/// host and client prediction agree.
#[inline]
pub fn holding(input: &PlayerInput) -> bool {
    input.held(buttons::CHARGE) && !input.held(buttons::TYPING)
}

/// Movement while holding the charge button (host `step_movement` and `World::move_mods_for`).
#[inline]
pub fn modify_movement(t: &ChargeTuning, input: &PlayerInput, grounded: bool, mods: &mut MoveModifiers) {
    if !holding(input) {
        return;
    }
    if grounded {
        mods.speed_mult *= t.ground_speed_mult;
    } else {
        mods.air_control *= t.air_control_mult;
    }
    mods.jump_mult *= t.jump_mult;
}

/// Can this player charge right now (besides holding the button)?
fn can_charge(p: &Player) -> bool {
    p.alive
        && !p.eliminated
        && !p.status.stunned()
        && !p.oath.reciting()
        && p.seat.is_none()
        && p.launch.is_none()
        && p.mv.tumble <= 0.0
        && p.punch.is_none()
}

impl crate::World {
    /// One tick of charging for every player (after inputs + timers, before movement / actions,
    /// so the tick a charge starts already blocks attacks).
    pub(crate) fn step_charge(&mut self, dt: f32) {
        let t = self.cfg.tuning.charge.clone();
        let max = self.cfg.rules.willpower_max();
        let mult = self.cfg.rules.regen_mult() * crate::launch::charge_scale(self); // WILLPOWER = LAUNCH (×1 off)
        let ended = self.match_state.phase == crate::rules::MatchPhase::Ended;
        for p in &mut self.players {
            let c = &mut p.charge;
            c.stagger = (c.stagger - dt).max(0.0);
            let want = holding(&p.input) && !ended;
            if !want {
                if c.active {
                    self.events.push(GameEvent::Charge { player: p.id, kind: ChargeEvent::Released, held: c.held });
                }
                *c = ChargeState { stagger: c.stagger, ..ChargeState::default() };
                continue;
            }
            if !can_charge(p) {
                if p.charge.active {
                    let held = p.charge.held;
                    self.events.push(GameEvent::Charge { player: p.id, kind: ChargeEvent::Broken, held });
                    p.charge = ChargeState { stagger: t.stagger_secs, ..ChargeState::default() };
                }
                continue;
            }
            let c = &mut p.charge;
            if c.stagger > 0.0 {
                continue;
            }
            if !c.active {
                // Already full: power up for show, but no "filled up" burst.
                *c = ChargeState { active: true, full: p.willpower >= max, ..ChargeState::default() };
                self.events.push(GameEvent::Charge { player: p.id, kind: ChargeEvent::Start, held: 0.0 });
            }
            c.held = (c.held + dt).min(3600.0);
            p.willpower = (p.willpower + t.rate_at(c.held) * mult * dt).min(max);
            if !c.full && p.willpower >= max {
                c.full = true;
                self.events.push(GameEvent::Charge { player: p.id, kind: ChargeEvent::Full, held: c.held });
            }
        }
    }

    /// Damage interrupts a charge (called wherever damage lands, next to the oath interrupt).
    pub(crate) fn interrupt_charge(&mut self, i: usize) {
        let stagger = self.cfg.tuning.charge.stagger_secs;
        let Some(p) = self.players.get_mut(i) else { return };
        if p.charge.active {
            let held = p.charge.held;
            p.charge = ChargeState { stagger, ..ChargeState::default() };
            self.events.push(GameEvent::Charge { player: p.id, kind: ChargeEvent::Broken, held });
        } else if holding(&p.input) {
            // Still holding through the stagger: a new hit restarts the wait.
            p.charge.stagger = stagger;
        }
    }
}

// ------------------------------------------------------------------------------------------ bots

/// Bots: power up when low on willpower and nobody is close (so charging shows up in practice).
/// Keeps holding until nearly full or an enemy comes within reach. `near` = distance to the
/// nearest living enemy. Returns true when the bot should hold the charge button this tick.
pub fn bot_wants_charge(w: &crate::World, me: &Player, near: f32, rng: &mut SimRng) -> bool {
    if w.rules().infinite_willpower() || !can_charge(me) || me.charge.stagger > 0.0 {
        return false;
    }
    let max = w.rules().willpower_max();
    if crate::launch::active(w.rules()) {
        // WILLPOWER = LAUNCH: the bar is launch power — top it up whenever it's safe-ish, fill it
        // (nearly) all the way, then go hunting.
        if holding(&me.input) {
            return me.willpower < max * 0.95 && near > 9.0;
        }
        if !me.mv.grounded || near <= 13.0 {
            return false;
        }
        return me.willpower < max * 0.4 || (me.willpower < max * 0.7 && near > 22.0 && rng.chance(0.05));
    }
    if holding(&me.input) {
        // Already powering up: keep going while safe and not yet (nearly) full.
        return me.willpower < max * 0.9 && near > 11.0;
    }
    if !me.mv.grounded || near <= 16.0 {
        return false;
    }
    if me.willpower < 20.0 && near > 18.0 {
        // Where a bot would start its oath: a coin flip between the two refills.
        return rng.chance(0.5);
    }
    me.willpower < max * 0.4 && rng.chance(0.02)
}

/// The input a charging bot sends: the charge button, aim kept, everything else released.
pub fn bot_charge_input(mut input: PlayerInput) -> PlayerInput {
    input.buttons = buttons::CHARGE;
    input.move_x = 0;
    input.move_z = 0;
    input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_is_quadratic_and_saturates() {
        let t = ChargeTuning::default();
        assert_eq!(t.rate_at(0.0), 0.0);
        assert!((t.rate_at(t.ramp_secs * 0.5) - t.rate_per_sec * 0.25).abs() < 1e-5);
        assert_eq!(t.rate_at(t.ramp_secs), t.rate_per_sec);
        assert_eq!(t.rate_at(100.0), t.rate_per_sec);
    }

    #[test]
    fn wire_round_trip() {
        let mut s = ChargeState { active: true, held: 1.5, ..Default::default() };
        let w = s.wire();
        assert_eq!(w, 30);
        let mut m = ChargeState::default();
        m.apply_wire(true, w);
        assert!((m.held - 1.5).abs() < 1e-6 && m.active);
        s.active = false;
        assert_eq!(s.wire(), 0);
        // Just started: still reads as charging.
        assert_eq!(ChargeState { active: true, held: 0.0, ..Default::default() }.wire(), 1);
    }

    #[test]
    fn hostile_tuning_is_clamped() {
        let t = ChargeTuning { rate_per_sec: f32::NAN, ground_speed_mult: 7.0, ..Default::default() }.validated();
        assert_eq!(t.rate_per_sec, ChargeTuning::default().rate_per_sec);
        assert_eq!(t.ground_speed_mult, 1.0);
    }
}
