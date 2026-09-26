//! Status effects (COMBAT.md §6): short, visible, data-driven.
//!
//! A player carries one timer per [`StatusKind`] (re-applying refreshes to the longer duration; no
//! stacking), plus who applied it (damage-over-time KOs are credited). The table [`STATUS_DEFS`] says
//! what each status does; [`element_statuses`] says which statuses an element inflicts on hit.
//! Elements also *cancel* each other (water/ice douse burn, fire dries wet and thaws chill) so
//! counters are qualitative.

use bd_core::PlayerId;
use bd_core::mechanics::Element;
use serde::{Deserialize, Serialize};

use crate::movement::MoveModifiers;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum StatusKind {
    /// Fire / Atomic: damage over time.
    Burn = 0,
    Poison = 1,
    /// Ice: acceleration ×0.6.
    Chill = 2,
    /// Electric: micro-stun (movement keys + attacks ignored; tethers and aim stay live).
    Stun = 3,
    /// Psychic: gravity ×0.3 — juggle window.
    Float = 4,
    /// Shadow: client draws a vision vignette. No sim effect.
    Shadow = 5,
    /// Water: soaked — electric chains reach twice as far and can arc back through you.
    Wet = 6,
}

pub const STATUS_COUNT: usize = 7;

impl StatusKind {
    pub const ALL: [StatusKind; STATUS_COUNT] = [
        StatusKind::Burn,
        StatusKind::Poison,
        StatusKind::Chill,
        StatusKind::Stun,
        StatusKind::Float,
        StatusKind::Shadow,
        StatusKind::Wet,
    ];

    #[inline]
    pub fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// What a status does while active.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusDef {
    /// Damage per second (credited to whoever applied it).
    pub dps: f32,
    /// Multiplies [`MoveModifiers::accel_mult`].
    pub accel_mult: f32,
    /// Multiplies [`MoveModifiers::gravity_scale`].
    pub gravity_mult: f32,
    /// Movement keys, jump and attacks are ignored.
    pub stun: bool,
}

const NONE: StatusDef = StatusDef { dps: 0.0, accel_mult: 1.0, gravity_mult: 1.0, stun: false };

/// Indexed by `StatusKind as usize`.
pub const STATUS_DEFS: [StatusDef; STATUS_COUNT] = [
    StatusDef { dps: 3.0, ..NONE },          // Burn
    StatusDef { dps: 2.0, ..NONE },          // Poison
    StatusDef { accel_mult: 0.6, ..NONE },   // Chill
    StatusDef { stun: true, ..NONE },        // Stun
    StatusDef { gravity_mult: 0.3, ..NONE }, // Float
    NONE,                                    // Shadow
    NONE,                                    // Wet
];

/// Longest a stun may last (design rule: nothing takes control away for more than ~0.35 s).
pub const MAX_STUN_SECS: f32 = 0.35;

/// Statuses an element inflicts on a hit: (kind, seconds). Light / Wind / Earth inflict none —
/// their effect is physical (knockback, lift, carving).
pub fn element_statuses(e: Element) -> &'static [(StatusKind, f32)] {
    match e {
        Element::Fire => &[(StatusKind::Burn, 2.5)],
        Element::Atomic => &[(StatusKind::Burn, 2.5)],
        Element::Electric => &[(StatusKind::Stun, 0.12)],
        Element::Ice => &[(StatusKind::Chill, 1.5)],
        Element::Water => &[(StatusKind::Wet, 4.0)],
        Element::Poison => &[(StatusKind::Poison, 4.0)],
        Element::Psychic => &[(StatusKind::Float, 1.0)],
        Element::Shadow => &[(StatusKind::Shadow, 1.0)],
        Element::Light | Element::Wind | Element::Earth => &[],
    }
}

/// Statuses a newly applied status removes (qualitative counters).
fn cancels(k: StatusKind) -> &'static [StatusKind] {
    match k {
        StatusKind::Wet => &[StatusKind::Burn],
        StatusKind::Chill => &[StatusKind::Burn],
        StatusKind::Burn => &[StatusKind::Wet, StatusKind::Chill],
        _ => &[],
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StatusSet {
    /// Remaining seconds per kind (0 = inactive).
    pub timers: [f32; STATUS_COUNT],
    /// Who applied each status (DoT credit).
    pub sources: [Option<PlayerId>; STATUS_COUNT],
}

impl StatusSet {
    #[inline]
    pub fn active(&self, k: StatusKind) -> bool {
        self.timers[k as usize] > 0.0
    }

    /// Bitmask of active statuses (bit = `StatusKind as u8`), for snapshots and HUD.
    pub fn bits(&self) -> u8 {
        let mut b = 0;
        for k in StatusKind::ALL {
            if self.active(k) {
                b |= k.bit();
            }
        }
        b
    }

    /// Apply / refresh. Returns true if it was newly applied (for events).
    pub fn apply(&mut self, k: StatusKind, secs: f32, source: Option<PlayerId>) -> bool {
        for c in cancels(k) {
            self.timers[*c as usize] = 0.0;
            self.sources[*c as usize] = None;
        }
        let secs = if k == StatusKind::Stun { secs.min(MAX_STUN_SECS) } else { secs };
        let i = k as usize;
        let fresh = self.timers[i] <= 0.0;
        self.timers[i] = self.timers[i].max(secs);
        self.sources[i] = source.or(self.sources[i]);
        fresh
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn stunned(&self) -> bool {
        self.active(StatusKind::Stun)
    }

    /// Advance timers. Returns (damage this tick, the source to credit for the largest share).
    pub fn tick(&mut self, dt: f32) -> (f32, Option<PlayerId>) {
        let mut dmg = 0.0;
        let mut src = None;
        let mut best = 0.0;
        for k in StatusKind::ALL {
            let i = k as usize;
            if self.timers[i] <= 0.0 {
                continue;
            }
            let step = self.timers[i].min(dt);
            let d = STATUS_DEFS[i].dps * step;
            if d > best {
                best = d;
                src = self.sources[i];
            }
            dmg += d;
            self.timers[i] -= dt;
            if self.timers[i] <= 0.0 {
                self.timers[i] = 0.0;
                self.sources[i] = None;
            }
        }
        (dmg, src)
    }

    /// Fold active statuses into the movement modifiers written before `step_player`.
    pub fn modify_movement(&self, m: &mut MoveModifiers) {
        for k in StatusKind::ALL {
            if self.active(k) {
                let d = &STATUS_DEFS[k as usize];
                m.accel_mult *= d.accel_mult;
                m.gravity_scale *= d.gravity_mult;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burn_ticks_and_expires() {
        let mut s = StatusSet::default();
        s.apply(StatusKind::Burn, 2.5, Some(PlayerId(3)));
        let mut total = 0.0;
        for _ in 0..(3 * 120) {
            let (d, src) = s.tick(1.0 / 120.0);
            if d > 0.0 {
                assert_eq!(src, Some(PlayerId(3)));
            }
            total += d;
        }
        assert!((total - 7.5).abs() < 0.05, "{total}");
        assert!(!s.active(StatusKind::Burn));
    }

    #[test]
    fn water_douses_fire_and_stun_is_capped() {
        let mut s = StatusSet::default();
        s.apply(StatusKind::Burn, 2.5, None);
        s.apply(StatusKind::Wet, 4.0, None);
        assert!(!s.active(StatusKind::Burn));
        s.apply(StatusKind::Stun, 5.0, None);
        assert!(s.timers[StatusKind::Stun as usize] <= MAX_STUN_SECS);
    }

    #[test]
    fn chill_and_float_modify_movement() {
        let mut s = StatusSet::default();
        s.apply(StatusKind::Chill, 1.0, None);
        s.apply(StatusKind::Float, 1.0, None);
        let mut m = MoveModifiers::default();
        s.modify_movement(&mut m);
        assert!((m.accel_mult - 0.6).abs() < 1e-6);
        assert!((m.gravity_scale - 0.3).abs() < 1e-6);
    }
}
