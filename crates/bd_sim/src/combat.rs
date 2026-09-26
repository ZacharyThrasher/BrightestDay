//! Combat core (COMBAT.md §0–1): the data every hit goes through and the pure formulas. The one
//! place that *applies* a hit to a player is [`crate::World::apply_hit`]; constructs, projectiles,
//! explosions, wall splats and status chains all build a [`HitInfo`] and call it.
//!
//! Numbers (all per design doc unless marked *ours*):
//! * health 100; `knock_mult = 1 + 1.25·(1 − hp/100)` (evaluated after the hit's damage);
//! * knockback direction gets a minimum upward component ([`MIN_LIFT`], *ours*: 0.25; Wind 0.45);
//! * final knockback > 14 m/s ⇒ tumble 1.2 s (air control ×0.25 until ground/tether/timeout);
//! * the part of the victim's velocity *opposing* the knockback is cancelled up to the knockback's
//!   size (*ours*: running into a hit can't neutralise it, but a hit never zeroes a fast player);
//! * melee momentum bonus `1 + clamp((v_close − 8)/30, 0, 0.6)` on damage and knockback;
//! * wall splat: tumbling into terrain/constructs at > 15 m/s ⇒ `(v − 15)·1.4` dmg (cap 30),
//!   crater, bounce at 30 % speed;
//! * void KO credited to the last attacker within 6 s; respawn 3 s; spawn shield 1.5 s.

use bd_core::mechanics::Element;
use bd_core::{ConstructId, PlayerId, Vec3};
use serde::{Deserialize, Serialize};

pub const MAX_HEALTH: f32 = 100.0;
/// Knockback (m/s, after multipliers) above which the victim tumbles.
pub const TUMBLE_IMPULSE: f32 = 14.0;
pub const TUMBLE_SECS: f32 = 1.2;
/// Minimum upward component of every knockback direction (guaranteed lift).
pub const MIN_LIFT: f32 = 0.25;
pub const WIND_MIN_LIFT: f32 = 0.45;
pub const SPLAT_MIN_SPEED: f32 = 15.0;
pub const SPLAT_DMG_PER_MS: f32 = 1.4;
pub const SPLAT_MAX_DMG: f32 = 30.0;
pub const SPLAT_BOUNCE: f32 = 0.3;
pub const SPLAT_CRATER_RADIUS: f32 = 1.1;
/// Seconds a hit keeps credit for a later void / splat / DoT KO.
pub const KO_CREDIT_SECS: f32 = 6.0;
pub const RESPAWN_SECS: f32 = 3.0;
pub const SPAWN_SHIELD_SECS: f32 = 1.5;
/// Melee momentum bonus: starts at this closing speed…
pub const MOMENTUM_START: f32 = 8.0;
/// …and grows by 1 per this many m/s…
pub const MOMENTUM_SPAN: f32 = 30.0;
/// …up to this bonus.
pub const MOMENTUM_MAX_BONUS: f32 = 0.6;
/// Closing speed that earns the streak VFX.
pub const STREAK_SPEED: f32 = 20.0;

// Ring (empty hand): a meaty fist of light (COMBAT.md §4).
pub const BLAST_SPEED: f32 = 75.0;
pub const BLAST_DAMAGE: f32 = 12.0;
pub const BLAST_KNOCKBACK: f32 = 9.0;
pub const BLAST_COOLDOWN: f32 = 0.4;
pub const BLAST_RADIUS: f32 = 0.25;
pub const BLAST_LIFETIME: f32 = 1.4;
/// Recoil impulses on the ground are scaled by this (in the air ×1.0) — COMBAT.md §4b.
pub const GROUND_RECOIL: f32 = 0.35;
pub const SHOVE_RANGE: f32 = 4.0;
/// Half-angle of the shove cone (radians, 45°).
pub const SHOVE_HALF_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
pub const SHOVE_KNOCKBACK: f32 = 14.0;
pub const SHOVE_COOLDOWN: f32 = 1.5;

/// What kind of hit this is (drives momentum bonus, juice, audio).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HitKind {
    RingBlast,
    RingShove,
    /// Quick held-weapon attack.
    Melee,
    /// Charged held-weapon attack.
    HeavyMelee,
    /// Shot fired by a held ranged construct.
    Shot,
    /// Thrown / launched object impact.
    Thrown,
    Explosion,
    /// Creature bite / swipe / charge.
    Creature,
    /// Something heavy landed on the victim.
    Crush,
    WallSplat,
    /// Electric chain arc.
    Chain,
}

impl HitKind {
    /// Gets the bounded movement bonus (COMBAT.md §0 axis 1).
    pub fn momentum_bonus(self) -> bool {
        matches!(self, HitKind::Melee | HitKind::HeavyMelee)
    }

    /// Can land a head-zone critical (projectiles, hitscan, ring blast, thrown things; not melee
    /// or area damage).
    pub fn can_headshot(self) -> bool {
        matches!(self, HitKind::RingBlast | HitKind::Shot | HitKind::Thrown)
    }
}

/// Head zone: the top of the hull (scaled with Shrink / Grow).
pub const HEAD_ZONE: f32 = 0.35;
/// Damage multiplier for a head-zone critical.
pub const HEAD_MULT: f32 = 1.5;

/// Everything needed to hit a player. Build one and call [`crate::World::apply_hit`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HitInfo {
    pub target: PlayerId,
    /// Attacker to credit (None = the world).
    pub source: Option<PlayerId>,
    /// Construct that delivered it, if any.
    pub construct: Option<ConstructId>,
    /// Base damage before momentum bonus.
    pub damage: f32,
    /// Direction the victim is pushed (need not be normalised; lift is added).
    pub impulse_dir: Vec3,
    /// Base knockback speed (m/s) before health scaling / bonuses.
    pub knockback: f32,
    pub element: Element,
    /// Juice weight multiplier (1 = normal; heavy/charged/dive moves use more).
    pub impact: f32,
    pub kind: HitKind,
    /// Contact point (debris / sparks).
    pub point: Vec3,
    /// Closing speed for the bounded momentum bonus when it isn't attacker-vs-victim (projectiles:
    /// projectile-vs-target closing speed beyond the muzzle speed, i.e. what the shooter's own
    /// motion added). `None` = melee rule (attacker vs victim) for melee kinds, no bonus otherwise.
    #[serde(default)]
    pub closing_speed: Option<f32>,
    /// Forced critical (construct weak points set it; their own multiplier is already in
    /// `damage`). Head-zone crits are detected by the world from `point`.
    #[serde(default)]
    pub crit: bool,
}

impl HitInfo {
    pub fn new(target: PlayerId, source: Option<PlayerId>, kind: HitKind, damage: f32, knockback: f32, dir: Vec3, point: Vec3) -> Self {
        Self {
            target,
            source,
            construct: None,
            damage,
            impulse_dir: dir,
            knockback,
            element: Element::Light,
            impact: 1.0,
            kind,
            point,
            closing_speed: None,
            crit: false,
        }
    }
    pub fn with_crit(mut self) -> Self {
        self.crit = true;
        self
    }
    pub fn with_closing(mut self, v: f32) -> Self {
        self.closing_speed = Some(v);
        self
    }
    pub fn with_element(mut self, e: Element) -> Self {
        self.element = e;
        self
    }
    pub fn with_construct(mut self, c: ConstructId) -> Self {
        self.construct = Some(c);
        self
    }
    pub fn with_impact(mut self, k: f32) -> Self {
        self.impact = k;
        self
    }
}

/// Client juice for one hit (DESIGN "Impact"): everything scales from `strength`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImpactFx {
    /// ≈ 0.1 tap … 1.0 big hit … 2.0 kaiju stomp.
    pub strength: f32,
    /// Hit-stop for attacker + victim visuals (30–140 ms).
    pub hitstop_ms: u8,
    /// Screen-shake trauma to add (0..1).
    pub shake: f32,
    /// Unit direction the hit travels (shake kicks along it).
    pub dir: Vec3,
    /// Closing speed > 20 m/s: speed-streak VFX.
    pub streak: bool,
    /// A heavy hit: longer hit-stop, onomatopoeia ("BLAM"), victim flinch.
    pub heavy: bool,
}

/// Juice from the applied numbers.
pub fn impact_fx(damage: f32, knockback: f32, weight: f32, dir: Vec3, v_close: f32) -> ImpactFx {
    let strength = ((knockback / 20.0 + damage / 40.0) * weight.max(0.1)).clamp(0.05, 2.0);
    let s1 = strength.min(1.0);
    ImpactFx {
        strength,
        hitstop_ms: (30.0 + 110.0 * s1).round() as u8,
        shake: (0.15 + 0.6 * s1).min(1.0),
        dir: dir.normalize_or_zero(),
        streak: v_close > STREAK_SPEED,
        heavy: strength >= 0.8,
    }
}

/// `1 + 1.25·(1 − hp/100)`.
#[inline]
pub fn knock_mult(hp: f32) -> f32 {
    1.0 + 1.25 * (1.0 - (hp / MAX_HEALTH).clamp(0.0, 1.0))
}

/// Melee bonus fraction from closing speed (0..0.6).
#[inline]
pub fn momentum_bonus(v_close: f32) -> f32 {
    ((v_close - MOMENTUM_START) / MOMENTUM_SPAN).clamp(0.0, MOMENTUM_MAX_BONUS)
}

/// Knockback multiplier from the element (Water +40 %, Wind +25 %).
#[inline]
pub fn element_knockback(e: Element) -> f32 {
    match e {
        Element::Water => 1.4,
        Element::Wind => 1.25,
        _ => 1.0,
    }
}

/// Knockback direction with guaranteed lift.
pub fn knockback_dir(dir: Vec3, e: Element) -> Vec3 {
    let mut d = dir.normalize_or_zero();
    if d == Vec3::ZERO {
        d = Vec3::Y;
    }
    let lift = if e == Element::Wind { WIND_MIN_LIFT } else { MIN_LIFT };
    if d.y < lift {
        let h = Vec3::new(d.x, 0.0, d.z).normalize_or_zero();
        d = (h * (1.0 - lift * lift).sqrt() + Vec3::Y * lift).normalize_or_zero();
        if d == Vec3::ZERO {
            d = Vec3::Y;
        }
    }
    d
}

/// Wall-splat damage for a tumbling impact at `speed` (0 below the threshold).
#[inline]
pub fn splat_damage(speed: f32) -> f32 {
    if speed <= SPLAT_MIN_SPEED { 0.0 } else { ((speed - SPLAT_MIN_SPEED) * SPLAT_DMG_PER_MS).min(SPLAT_MAX_DMG) }
}

/// What [`crate::World::apply_hit`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HitResult {
    /// False if blocked (spawn shield, dead target, match over…).
    pub applied: bool,
    pub damage: f32,
    pub knockback: f32,
    pub lethal: bool,
}

/// A hit on a construct (integrity damage). Blade doubles it (COMBAT.md §3).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConstructHit {
    pub target: ConstructId,
    pub source: Option<PlayerId>,
    pub damage: f32,
    /// Momentum handed to the construct (and to its shards if it shatters).
    pub impulse: Vec3,
    pub blade: bool,
    pub point: Vec3,
}

/// A radial blast resolved by the world: knockback + damage with linear falloff to 30 % at the
/// edge, integrity damage to constructs, and a crater.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Explosion {
    pub pos: Vec3,
    pub radius: f32,
    pub damage: f32,
    pub knockback: f32,
    pub source: Option<PlayerId>,
    pub construct: Option<ConstructId>,
    pub element: Element,
    /// Crater radius (0 = no carve). Earth element doubles carved volume.
    pub carve_radius: f32,
    /// Hit the source player too (rocket jumps).
    pub self_damage: bool,
}

/// A request to destroy terrain. The world turns it into a replicated `CarveOp` (seeded,
/// rate-limited, progressive for huge ones).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CarveRequest {
    pub center: Vec3,
    pub radius: f32,
    /// Destructive strength vs material hardness (1.0 removes stone).
    pub power: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knockback_scales_with_missing_health() {
        assert_eq!(knock_mult(100.0), 1.0);
        assert!((knock_mult(0.0) - 2.25).abs() < 1e-6);
    }

    #[test]
    fn momentum_bonus_is_bounded() {
        assert_eq!(momentum_bonus(5.0), 0.0);
        assert!((momentum_bonus(23.0) - 0.5).abs() < 1e-6);
        assert_eq!(momentum_bonus(100.0), MOMENTUM_MAX_BONUS);
    }

    #[test]
    fn lift_is_guaranteed() {
        let d = knockback_dir(Vec3::new(1.0, -1.0, 0.0), Element::Light);
        assert!(d.y >= MIN_LIFT - 1e-5 && d.x > 0.0);
        assert!((d.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn splat_curve() {
        assert_eq!(splat_damage(14.0), 0.0);
        assert!((splat_damage(25.0) - 14.0).abs() < 1e-4);
        assert_eq!(splat_damage(80.0), SPLAT_MAX_DMG);
    }
}
