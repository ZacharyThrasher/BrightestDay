//! POWER HUD readout: what a player HOLDS and how ready it is — no names, no numbers, fractions only.
//!
//! Derived from sim state when a snapshot is written and replicated compactly, so the client's
//! first-person ring gems, the third-person motes (everyone sees everyone's readiness: telegraphed
//! power is a design goal) and the minimal HUD strip (bd_game `power_hud`) read one source that is
//! identical in practice, on a host and on a client:
//! * [`ReadySnap`] (in `PlayerSnap::ready`, every player): per held power, the cooldown still to run
//!   of its slowest move and its form's time left;
//! * [`OwnHud`] (in `OwnState::hud`, the recipient only): seconds left of each status on you, the
//!   juggle guard and a shrink / grow.
//!
//! Held order is `PowerSnap::held()` / `PlayerPower::held()` (the active power first).

use serde::{Deserialize, Serialize};

use crate::powers::{effects, move_sel::MAX_MOVES, HeldView, MoveOp, PlayerPower, PowerDef, MAX_EXTRA_POWERS};
use crate::status::STATUS_COUNT;

/// Most powers held at once.
pub const HELD_MAX: usize = 1 + MAX_EXTRA_POWERS;
/// [`OwnHud::timers`] slots: every [`crate::status::StatusKind`], then the juggle guard, then a
/// shrink / grow.
pub const HUD_TIMERS: usize = STATUS_COUNT + 2;
pub const GUARD_TIMER: usize = STATUS_COUNT;
pub const SIZE_TIMER: usize = STATUS_COUNT + 1;
/// [`OwnHud::timers`] resolution (1/10 s; ≤ 25.5 s).
pub const TIMER_STEP: f32 = 0.1;

/// Per-player power readiness (replicated for everyone).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReadySnap {
    /// Cooldown still to run of each held power's slowest move: 0 = every move ready, 255 = just
    /// fired. Rounded UP, so anything still cooling is ≥ 1 (the ready edge is exact).
    pub cd: [u8; HELD_MAX],
    /// Each held power's form: 0 = none, 1..=255 = time left (a sustained form is 255).
    pub form: [u8; HELD_MAX],
}

impl ReadySnap {
    /// Readiness of `pw` (all zero while holding nothing).
    pub fn of(pw: &PlayerPower) -> Self {
        let mut out = Self::default();
        if pw.id.is_none() {
            return out;
        }
        let mult = effects::cooldown_mult(pw);
        for (k, h) in pw.held().enumerate().take(HELD_MAX) {
            let cds = if h.slot == 0 { pw.move_cd } else { pw.others.get(h.slot - 1).copied().flatten().map_or([0.0; MAX_MOVES], |o| o.move_cd) };
            out.cd[k] = quantize_up(cooldown_left(h.def, h.tier, &cds, mult));
            out.form[k] = form_left(pw, &h);
        }
        out
    }

    /// 0 = just fired … 1 = ready, for held power `k`.
    pub fn ready(&self, k: usize) -> f32 {
        self.cd.get(k).map_or(1.0, |c| 1.0 - *c as f32 / 255.0)
    }

    /// Held power `k` is ready (every move off cooldown).
    pub fn is_ready(&self, k: usize) -> bool {
        self.cd.get(k).is_none_or(|c| *c == 0)
    }

    /// Held power `k`'s form: time left 0..1 (1 = full / sustained), `None` = not in a form.
    pub fn form(&self, k: usize) -> Option<f32> {
        self.form.get(k).copied().filter(|f| *f > 0).map(|f| f as f32 / 255.0)
    }
}

/// Fraction (0..1) of the cooldown still to run of the slowest move of a power at `tier`.
pub fn cooldown_left(def: &PowerDef, tier: u8, move_cd: &[f32; MAX_MOVES], mult: f32) -> f32 {
    let moves = def.moves_at(tier);
    let mut left = 0.0f32;
    for (m, cd) in moves.iter().zip(move_cd.iter()) {
        if *cd > 0.0 && cd.is_finite() {
            let full = (m.cooldown * mult).max(0.05);
            left = left.max(cd / full);
        }
    }
    // A move whose def vanished (tier switched) still counts as cooling, as a full one.
    for cd in move_cd.iter().skip(moves.len()) {
        if *cd > 0.0 {
            left = left.max(1.0);
        }
    }
    left.clamp(0.0, 1.0)
}

/// Seconds of the time stop a power's move grants at `tier` (the same clamp the move applies).
pub fn time_stop_secs(def: &PowerDef, tier: u8) -> f32 {
    def.moves_at(tier)
        .iter()
        .flat_map(|m| m.ops.iter())
        .find_map(|o| if let MoveOp::TimeStop { secs, .. } = o { Some(secs.clamp(0.1, 3.0)) } else { None })
        .unwrap_or(1.0)
}

/// Is held power `h` in a form: transformed, a tier above base, or an awakened technique (a
/// technique with a transformation moment is itself the form: ultra instinct, super saiyan)?
pub fn in_form(h: &HeldView) -> bool {
    h.transformed || h.tier > 0 || (h.def.kind.is_technique() && h.def.ascend_at(h.tier).is_some())
}

/// Form byte of held power `h` (see [`ReadySnap::form`]). Timed: the active power's time stop.
/// Forms themselves are sustained today (they end at KO or when switched off): full.
fn form_left(pw: &PlayerPower, h: &HeldView) -> u8 {
    if h.slot == 0 && pw.time_stop > 0.0 {
        return quantize_up(pw.time_stop / time_stop_secs(h.def, h.tier)).max(1);
    }
    if in_form(h) { 255 } else { 0 }
}

/// 0..1 → 0..=255, rounded up (a tiny remainder is never 0).
fn quantize_up(x: f32) -> u8 {
    if !(x > 0.0) {
        return 0;
    }
    (x.min(1.0) * 255.0).ceil().clamp(1.0, 255.0) as u8
}

/// Recipient-only HUD timers (replicated in `OwnState::hud`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnHud {
    /// Seconds left in [`TIMER_STEP`]s: each status ([`crate::status::StatusKind`] order), then
    /// [`GUARD_TIMER`], then [`SIZE_TIMER`]. Rounded up (a live timer is ≥ 1).
    pub timers: [u8; HUD_TIMERS],
}

impl OwnHud {
    /// `None` when nothing is running (keeps the recipient's snapshot as small as before).
    pub fn of(p: &crate::Player) -> Option<Self> {
        let mut t = [0u8; HUD_TIMERS];
        for (k, s) in p.status.timers.iter().enumerate().take(STATUS_COUNT) {
            t[k] = secs_byte(*s);
        }
        t[GUARD_TIMER] = secs_byte(p.juggle.immune);
        t[SIZE_TIMER] = if (p.size.target - 1.0).abs() > 1e-3 { secs_byte(p.size.timer) } else { 0 };
        t.iter().any(|x| *x > 0).then_some(Self { timers: t })
    }

    /// Seconds left of timer `k` (0 = not running).
    pub fn secs(&self, k: usize) -> f32 {
        self.timers.get(k).map_or(0.0, |t| *t as f32 * TIMER_STEP)
    }
}

fn secs_byte(s: f32) -> u8 {
    if !(s > 0.0) {
        return 0;
    }
    (s / TIMER_STEP).ceil().clamp(1.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_never_rounds_a_remainder_to_ready() {
        assert_eq!(quantize_up(0.0), 0);
        assert_eq!(quantize_up(1e-6), 1);
        assert_eq!(quantize_up(1.0), 255);
        assert_eq!(quantize_up(7.0), 255);
        assert_eq!(quantize_up(f32::NAN), 0);
        assert_eq!(secs_byte(0.01), 1);
        assert_eq!(secs_byte(0.0), 0);
        assert_eq!(secs_byte(100.0), 255);
    }

    #[test]
    fn ready_and_form_readers() {
        let s = ReadySnap { cd: [0, 255, 128], form: [0, 255, 51] };
        assert!(s.is_ready(0) && !s.is_ready(1));
        assert_eq!(s.ready(0), 1.0);
        assert_eq!(s.ready(1), 0.0);
        assert!((s.ready(2) - (1.0 - 128.0 / 255.0)).abs() < 1e-6);
        assert_eq!(s.form(0), None);
        assert_eq!(s.form(1), Some(1.0));
        assert!((s.form(2).unwrap() - 0.2).abs() < 1e-6);
        // Out of range: ready, no form.
        assert!(s.is_ready(9) && s.form(9).is_none());
    }
}
