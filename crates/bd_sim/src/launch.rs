//! WILLPOWER = LAUNCH (host match rule [`MatchRules::willpower_launch`], off by default so it can be
//! A/B tested; owner idea: "willpower = how much you launch opponents").
//!
//! With the rule on, willpower stops being "can you cast" and becomes "how hard you hit":
//!
//! * **Never refused.** Casts / moves / signatures always fire at full size and still drain their
//!   spark price; the bar floors at 0 (a cast at 0 fires and drains nothing more). No sputtering and
//!   no construct starvation at 0 (that would be a refusal in disguise). Upkeep still drains.
//! * **Launch scaling** ([`mult`]): every hit's knockback — from a player, a projectile, an
//!   explosion or a construct you summoned (the hit's `source` is its owner) — is multiplied by
//!   [`curve`] of the ATTACKER's willpower fraction read at hit time (≈ 0.55× empty → 1.0× at half →
//!   1.35× full). Integrity impulses on constructs scale the same way. **Damage % is untouched**:
//!   spamming still builds % (the setup), only a loaded hit cashes it in as a KO; scaling damage too
//!   would punish a drained player twice and snowball a loaded one. Juggle protection counts the
//!   hit's pre-willpower knockback, so a drained attacker can't slip under the chain threshold and
//!   juggle forever (the guard behaves exactly as in normal rules). Wall splats and self hits are
//!   not scaled (not the attacker's swing).
//! * **Refill economy**: passive regen × [`LaunchTuning::regen_mult`] (slow), charging ×
//!   [`LaunchTuning::charge_mult`] and oath refills × [`LaunchTuning::oath_mult`] (fast) — the loop is
//!   spend → drained → charge / recite → hunt a KO.
//! * **Anti-camp leak** ([`leak`]): above [`LaunchTuning::leak_above`] of the bar, a player who hasn't
//!   cast / used a move for [`LaunchTuning::leak_after_secs`] slowly leaks down to that line (not
//!   while charging or reciting).
//!
//! Numbers live in `CombatTuning::launch` (`assets/tuning/combat.ron`, host-synced). Infinite
//! willpower makes the rule inert (the bar is pinned full, so it would just be ×1.35 everywhere).

use bd_core::{DT, PlayerId};
use serde::{Deserialize, Serialize};

use crate::World;
use crate::rules::MatchRules;

/// `CombatTuning::launch`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchTuning {
    /// Knockback multiplier with an empty bar.
    pub empty_mult: f32,
    /// …at `mid_frac` of the bar (the "normal rules" point).
    pub mid_mult: f32,
    /// …with a full bar.
    pub full_mult: f32,
    /// Bar fraction where the multiplier is `mid_mult`.
    pub mid_frac: f32,
    /// Shape above the mid point (> 1: the last sparks matter most; 1 = linear).
    pub top_exp: f32,
    /// Passive regen multiplier in this mode (slow: charging / the oath are the refill).
    pub regen_mult: f32,
    /// Charging refill multiplier in this mode.
    pub charge_mult: f32,
    /// Oath refill multiplier in this mode (a full recital already fills the bar; this helps a
    /// sloppy or interrupted one).
    pub oath_mult: f32,
    /// Anti-camp: above this fraction of the bar…
    pub leak_above: f32,
    /// …after this many seconds without a cast / move…
    pub leak_after_secs: f32,
    /// …willpower leaks at this rate (per second) down to `leak_above`.
    pub leak_per_sec: f32,
}

impl Default for LaunchTuning {
    fn default() -> Self {
        Self {
            empty_mult: 0.55,
            mid_mult: 1.0,
            full_mult: 1.35,
            mid_frac: 0.5,
            top_exp: 1.4,
            regen_mult: 0.4,
            charge_mult: 2.2,
            oath_mult: 1.25,
            leak_above: 0.8,
            leak_after_secs: 5.0,
            leak_per_sec: 2.5,
        }
    }
}

impl LaunchTuning {
    /// Clamp a hand-edited / received tuning to sane, finite values.
    pub fn validated(self) -> Self {
        let d = Self::default();
        let fin = |v: f32, def: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { def };
        let empty_mult = fin(self.empty_mult, d.empty_mult, 0.0, 3.0);
        let mid_mult = fin(self.mid_mult, d.mid_mult, empty_mult, 3.0);
        Self {
            empty_mult,
            mid_mult,
            full_mult: fin(self.full_mult, d.full_mult, mid_mult, 4.0),
            mid_frac: fin(self.mid_frac, d.mid_frac, 0.05, 0.95),
            top_exp: fin(self.top_exp, d.top_exp, 0.25, 4.0),
            regen_mult: fin(self.regen_mult, d.regen_mult, 0.0, 10.0),
            charge_mult: fin(self.charge_mult, d.charge_mult, 0.0, 10.0),
            oath_mult: fin(self.oath_mult, d.oath_mult, 0.0, 10.0),
            leak_above: fin(self.leak_above, d.leak_above, 0.0, 1.0),
            leak_after_secs: fin(self.leak_after_secs, d.leak_after_secs, 0.0, 120.0),
            leak_per_sec: fin(self.leak_per_sec, d.leak_per_sec, 0.0, 100.0),
        }
    }
}

/// The rule is live (on, and willpower isn't infinite).
pub fn active(rules: &MatchRules) -> bool {
    rules.willpower_launch && !rules.infinite_willpower()
}

/// Knockback multiplier for a bar fraction `frac` (0 = empty, 1 = full): linear from `empty_mult` to
/// `mid_mult` over the bottom part, then `mid_mult → full_mult` shaped by `top_exp`.
pub fn curve(t: &LaunchTuning, frac: f32) -> f32 {
    let f = if frac.is_finite() { frac.clamp(0.0, 1.0) } else { 0.0 };
    let m = t.mid_frac.clamp(0.05, 0.95);
    if f <= m {
        t.empty_mult + (t.mid_mult - t.empty_mult) * (f / m)
    } else {
        let u = (f - m) / (1.0 - m);
        t.mid_mult + (t.full_mult - t.mid_mult) * u.powf(t.top_exp.max(0.01))
    }
}

/// Launch power 0 (empty: `empty_mult`) … 1 (full: `full_mult`) for the HUD / glow.
pub fn power01(t: &LaunchTuning, frac: f32) -> f32 {
    let span = t.full_mult - t.empty_mult;
    if span.abs() < 1e-5 { 0.5 } else { ((curve(t, frac) - t.empty_mult) / span).clamp(0.0, 1.0) }
}

/// The mid point (×`mid_mult`) as launch power 0..1 (the gauge's "normal" notch).
pub fn mid_power01(t: &LaunchTuning) -> f32 {
    power01(t, t.mid_frac)
}

impl World {
    /// Bar fraction of a player (0 when unknown).
    pub fn willpower_frac(&self, id: PlayerId) -> f32 {
        let max = self.cfg.rules.willpower_max().max(1.0);
        self.player(id).map_or(0.0, |p| (p.willpower / max).clamp(0.0, 1.0))
    }

    /// WILLPOWER = LAUNCH: the knockback multiplier a hit from `source` gets right now (1 when the
    /// rule is off, for sourceless hits and for unknown sources).
    pub fn launch_mult(&self, source: Option<PlayerId>) -> f32 {
        if !active(&self.cfg.rules) {
            return 1.0;
        }
        match source.filter(|s| self.player(*s).is_some()) {
            Some(s) => curve(&self.cfg.tuning.launch, self.willpower_frac(s)),
            None => 1.0,
        }
    }
}

/// Passive regen scale (1 when off).
pub fn regen_scale(w: &World) -> f32 {
    if active(&w.cfg.rules) { w.cfg.tuning.launch.regen_mult } else { 1.0 }
}

/// Charging refill scale (1 when off).
pub fn charge_scale(w: &World) -> f32 {
    if active(&w.cfg.rules) { w.cfg.tuning.launch.charge_mult } else { 1.0 }
}

/// Oath refill scale (1 when off).
pub fn oath_scale(w: &World) -> f32 {
    if active(&w.cfg.rules) { w.cfg.tuning.launch.oath_mult } else { 1.0 }
}

/// Casts / moves are never refused for willpower (rule on) — callers skip their affordability check.
pub fn never_refuse(w: &World) -> bool {
    active(&w.cfg.rules)
}

/// Anti-camp leak (after passive regen / upkeep each tick). No-op while the rule is off.
pub(crate) fn leak(w: &mut World, dt: f32) {
    if !active(&w.cfg.rules) {
        return;
    }
    let t = w.cfg.tuning.launch.clone();
    let max = w.cfg.rules.willpower_max();
    let line = t.leak_above * max;
    let after = (t.leak_after_secs / DT) as u64;
    let now = w.tick;
    for p in &mut w.players {
        if !p.alive || p.willpower <= line || p.charge.active || p.oath.reciting() {
            continue;
        }
        if now.saturating_sub(p.casts.last_tick()) < after {
            continue;
        }
        p.willpower = (p.willpower - t.leak_per_sec * dt).max(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_hits_its_anchors_and_rises() {
        let t = LaunchTuning::default();
        assert!((curve(&t, 0.0) - 0.55).abs() < 1e-5);
        assert!((curve(&t, 0.5) - 1.0).abs() < 1e-5);
        assert!((curve(&t, 1.0) - 1.35).abs() < 1e-5);
        let mut last = 0.0;
        for k in 0..=100 {
            let m = curve(&t, k as f32 / 100.0);
            assert!(m >= last - 1e-6);
            last = m;
        }
        assert!((curve(&t, f32::NAN) - 0.55).abs() < 1e-5);
        assert!((power01(&t, 1.0) - 1.0).abs() < 1e-5 && power01(&t, 0.0).abs() < 1e-5);
    }

    #[test]
    fn validated_keeps_the_curve_monotone() {
        let t = LaunchTuning { empty_mult: 2.0, mid_mult: 1.0, full_mult: f32::NAN, ..LaunchTuning::default() }.validated();
        assert!(t.empty_mult <= t.mid_mult && t.mid_mult <= t.full_mult);
    }
}
