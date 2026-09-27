//! The Smash model (COMBAT.md §1a): no health — damage **%** that makes knockback grow; KOs only by
//! leaving the arena through a blast zone; DI (steer your launch up to ±15°), tech (Jump just
//! before a wall/ground contact cancels the splat), and punch-through (a fast launch plows through
//! destructible terrain). Numbers live in [`SmashTuning`] (`SimConfig.tuning.smash`); this module
//! holds the formulas and per-player launch state. The world applies them in `world/combat.rs`
//! (hits), `world/mod.rs` (movement, blast zones, stocks) and `world/punch.rs` (punch-through).

use bd_core::{Quat, Vec3};
use serde::{Deserialize, Serialize};

pub use crate::tuning::SmashTuning;

/// Which blast zone a KO went through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum BlastZone {
    /// Below the void / water.
    Bottom = 0,
    /// Past a side bound.
    Side = 1,
    /// Over the top: a "star KO" (twinkle in the sky, delayed poof).
    Top = 2,
}

/// A player flying from a launch (knockback ≥ `kb_min_launch`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LaunchState {
    /// Seconds since the launch.
    pub t: f32,
    /// Launch speed (m/s) — trail intensity, telemetry.
    pub speed: f32,
    /// DI already applied (radians), bounded by `di_max_deg`.
    pub di: f32,
    /// Who launched them (pairs `Launched` with `Recovered` / the KO).
    pub source: Option<bd_core::PlayerId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PunchState {
    pub entry: Vec3,
    pub material: u8,
    /// Far end of the tunnel carved so far (the player is out once past it in open air).
    pub carved_to: Vec3,
}

/// `(base·kb_base + growth·(pct_after/100)·(dmg/divisor + 1)) · (1/weight) · kb_scale · lobby`,
/// capped at `kb_max`.
pub fn knockback(t: &SmashTuning, base_kb: f32, dmg: f32, pct_after: f32, weight: f32, lobby_scale: f32) -> f32 {
    let growth = t.kb_growth * (pct_after.max(0.0) / 100.0) * (dmg.max(0.0) / t.kb_dmg_divisor + 1.0);
    let kb = (base_kb.max(0.0) * t.kb_base + growth) / weight.max(0.05) * t.kb_scale * lobby_scale.max(0.0);
    if kb.is_finite() { kb.min(t.kb_max) } else { 0.0 }
}

/// LAUNCH FEEL: the knockback every player hit uses. Classic (`MatchRules::classic_launch`) is
/// exactly [`knockback`]. Modern: the move's base knockback is scaled by [`base_scale`] (small hits
/// barely displace you at low %; strong hits keep more of their punch) and the growth term by
/// `curve_growth_mult` — so it stays `base + growth × %`, monotonic in % and in hit strength. The
/// attacker's WILLPOWER = LAUNCH multiplier (`crate::launch`) and juggle decay multiply the result.
pub fn launch_knockback(t: &SmashTuning, classic: bool, base_kb: f32, dmg: f32, pct_after: f32, weight: f32, lobby_scale: f32) -> f32 {
    if classic {
        return knockback(t, base_kb, dmg, pct_after, weight, lobby_scale);
    }
    let base = base_kb.max(0.0) * base_scale(t, base_kb, pct_after);
    let growth = t.curve_growth_mult * t.kb_growth * (pct_after.max(0.0) / 100.0) * (dmg.max(0.0) / t.kb_dmg_divisor + 1.0);
    let kb = (base * t.kb_base + growth) / weight.max(0.05) * t.kb_scale * lobby_scale.max(0.0);
    if kb.is_finite() { kb.min(t.kb_max) } else { 0.0 }
}

/// Modern launch curve: the fraction of a move's base knockback a hit keeps at `pct` —
/// `curve_base_low` at 0 % → 1 at `curve_base_full_pct`, plus up to `curve_strong_keep` for strong
/// moves (base knockback toward `curve_strong_kb`). Non-decreasing in both, within [low, 1].
pub fn base_scale(t: &SmashTuning, base_kb: f32, pct: f32) -> f32 {
    let low = t.curve_base_low.clamp(0.0, 1.0);
    let ramp = (pct.max(0.0) / t.curve_base_full_pct.max(1.0)).min(1.0);
    let s = (base_kb.max(0.0) / t.curve_strong_kb.max(0.1)).min(1.0);
    let b = low + (1.0 - low) * ramp + t.curve_strong_keep.max(0.0) * s * s;
    if b.is_finite() { b.clamp(low, 1.0) } else { 1.0 }
}

/// LAUNCH FEEL: how much of a tumble of knockback `kb` is hitstun (no air jump / air dodge). Classic:
/// all of it. Modern: `hitstun_secs_min + kb · per_kb`, capped at `hitstun_secs_max` and never
/// longer than the tumble itself.
pub fn hitstun_secs(t: &SmashTuning, classic: bool, kb: f32) -> f32 {
    let tumble = tumble_secs(t, kb);
    if classic {
        return tumble;
    }
    let h = (t.hitstun_secs_min + kb.max(0.0) * t.hitstun_secs_per_kb).clamp(t.hitstun_secs_min, t.hitstun_secs_max);
    if h.is_finite() { h.min(tumble) } else { tumble }
}

/// Body weight: 1 normally, heavier grown (1.6) / in a power suit (×1.3), lighter shrunk (0.5).
pub fn weight(t: &SmashTuning, size_scale: f32, suit: bool) -> f32 {
    let s = if size_scale.is_finite() { size_scale } else { 1.0 };
    let w = if s < 1.0 {
        let k = ((1.0 - s) / 0.5).clamp(0.0, 1.0);
        t.weight_default + (t.weight_shrink - t.weight_default) * k
    } else {
        let k = ((s - 1.0) / 0.6).clamp(0.0, 1.0);
        t.weight_default + (t.weight_grow - t.weight_default) * k
    };
    if suit { w * t.weight_suit } else { w }
}

/// How long a hit of knockback `kb` tumbles (longer launches stay out of control longer; tethers
/// and landing still end it — the recovery skill).
pub fn tumble_secs(t: &SmashTuning, kb: f32) -> f32 {
    (t.tumble_secs_min + kb * t.tumble_secs_per_kb).clamp(t.tumble_secs_min, t.tumble_secs_max)
}

/// Speed needed to punch through a material of this hardness.
pub fn punch_threshold(t: &SmashTuning, hardness: f32) -> f32 {
    t.punch_speed * (1.0 + (hardness.max(t.punch_min_hardness) - 1.0) * t.punch_hardness_speed_scale).max(0.05)
}

/// A launch starts: the full DI budget over the DI window (on the movement state, so client
/// prediction steers exactly like the host).
pub fn start_di(t: &SmashTuning, st: &mut crate::movement::PlayerState) {
    st.di_left = t.di_max_deg.to_radians().max(0.0);
    st.di_window = t.di_ramp_secs.max(0.0);
}

/// One tick of DI (host `step_movement` and `World::predict_player`): steer the launch toward the
/// move keys, at most `di_max_deg` over `di_ramp_secs`. Returns the rotation applied (radians).
pub fn apply_di(t: &SmashTuning, st: &mut crate::movement::PlayerState, input: &bd_core::input::PlayerInput, dt: f32) -> f32 {
    if st.di_window <= 0.0 {
        return 0.0;
    }
    let mut applied = 0.0;
    if st.di_left > 0.0 && !input.held(bd_core::input::buttons::TYPING) {
        let max = t.di_max_deg.to_radians();
        let (sy, cy) = input.yaw.sin_cos();
        let wish = (Vec3::new(-sy, 0.0, -cy) * input.move_z.clamp(-1, 1) as f32 + Vec3::new(cy, 0.0, -sy) * input.move_x.clamp(-1, 1) as f32).normalize_or_zero();
        let step = (max * dt / t.di_ramp_secs.max(dt)).min(st.di_left);
        applied = di_rotate(&mut st.vel, wish, step);
        st.di_left = (st.di_left - applied).max(0.0);
    }
    st.di_window = (st.di_window - dt).max(0.0);
    applied
}

/// DI: rotate `vel` toward the horizontal `wish` by at most `max_step` radians (the part of the
/// wish perpendicular to the launch only). Returns the rotation applied (radians).
pub fn di_rotate(vel: &mut Vec3, wish: Vec3, max_step: f32) -> f32 {
    let speed = vel.length();
    if speed < 1e-3 || wish == Vec3::ZERO || max_step <= 0.0 {
        return 0.0;
    }
    let d = *vel / speed;
    let axis = d.cross(wish);
    let s = axis.length();
    if s < 1e-4 {
        return 0.0;
    }
    let angle = max_step * s.min(1.0);
    *vel = Quat::from_axis_angle(axis / s, angle) * *vel;
    angle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knockback_grows_with_percent_and_damage() {
        let t = SmashTuning::default();
        let k0 = knockback(&t, 16.0, 14.0, 0.0, 1.0, 1.0);
        let k1 = knockback(&t, 16.0, 14.0, 150.0, 1.0, 1.0);
        assert!((k0 - 16.0).abs() < 1e-4);
        assert!(k1 > k0 * 1.5, "{k0} → {k1}");
        assert!(knockback(&t, 16.0, 14.0, 150.0, 1.6, 1.0) < k1, "heavier flies less");
    }

    #[test]
    fn di_is_bounded_and_perpendicular() {
        let mut v = Vec3::new(0.0, 0.0, -40.0);
        let a = di_rotate(&mut v, Vec3::X, 15f32.to_radians());
        assert!((a - 15f32.to_radians()).abs() < 1e-4);
        assert!((v.length() - 40.0).abs() < 1e-3);
        assert!(v.x > 0.0);
        let mut w = Vec3::new(0.0, 0.0, -40.0);
        assert_eq!(di_rotate(&mut w, Vec3::NEG_Z, 0.3), 0.0, "parallel input: no DI");
    }
}
