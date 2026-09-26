//! CONNECT assist (owner: "I feel like I'm slapping noodles around and it is hard to hit anyone…
//! make connecting a bit easier"). Four small, invisible helpers — never a lock-on, never the camera,
//! never through walls — all host-side (lag-compensated like the hit tests they help), all numbers in
//! [`AimTuning`] (`assets/tuning/combat.ron` → `assist.aim`), all neutral at [`AimTuning::neutral`]:
//!
//! * **Melee hit volume** ([`melee_radius`]): the swing capsule is fatter than the item (×1.5 by
//!   default, per form, heavier weapons a bit more), capped. The extra only counts with a clear line
//!   from the blade to the body (`constructs/held.rs` `sweep`). Swing *magnetism* (the hitbox turning
//!   toward the target over the windup) is the lunge's aim nudge (`melee_assist`, `assist.lunge.aim_*`).
//! * **Projectile homing** ([`pick_target`] at release, [`steer`] in flight): ring blasts, bolts,
//!   arrows, discs bend toward the enemy nearest the crosshair inside a small cone; the turn rate is
//!   strongest at release and decays to zero (`proj_decay_secs`); line of sight every tick; explosive
//!   rounds don't home (their splash is their assist). The `Homing` mechanic stays far stronger.
//! * **Thrown constructs**: the same, with their own (gentler, longer) numbers.
//! * **Hitscan magnetism** ([`magnet_point`]): a ray that misses every body by at most a small angle
//!   (per gun: hand cannon small, minigun smaller, shotgun pellets none, beams tiny) is bent onto
//!   the nearest one, if the bent ray is clear of terrain and constructs.

use bd_core::mechanics::Form;
use bd_core::voxel::Aabb;
use bd_core::{PlayerId, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::geom;
use crate::melee_assist::FormReach;
use crate::projectile::ProjectileKind;
use crate::weapons::GunArchetype;

/// Homing strength per projectile kind (× the turn rate).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct KindTurn {
    pub kind: ProjectileKind,
    pub mult: f32,
}

/// Hitscan magnetism per gun archetype (degrees a miss may be bent by).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchAngle {
    pub arch: GunArchetype,
    pub deg: f32,
}

/// CONNECT assist numbers. Never shown to players.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AimTuning {
    /// Melee swing capsule radius × this (forms without a `melee_forms` row)…
    pub melee_radius: f32,
    /// …per-form multiplier instead of `melee_radius`…
    pub melee_forms: Vec<FormReach>,
    /// …× (1 + melee_weight · (swing weight − 1)): heavier weapons bite wider…
    pub melee_weight: f32,
    /// …the extra radius beyond the item's own capped at this (m).
    pub melee_extra_max: f32,
    /// Projectile homing: acquisition cone half-angle (deg) around the crosshair at release…
    pub proj_cone_deg: f32,
    /// …and range (m, eye to hull).
    pub proj_range: f32,
    /// Turn rate at release (deg/s), falling linearly to 0 over `proj_decay_secs`.
    pub proj_turn_deg: f32,
    pub proj_decay_secs: f32,
    /// A target further than this off the flight direction (deg) is let go.
    pub proj_release_deg: f32,
    /// Turn-rate multiplier per projectile kind (kinds without a row: 1).
    pub proj_kinds: Vec<KindTurn>,
    /// Turn-rate multiplier for explosive rounds (rockets, payload bombs): their splash is their assist.
    pub proj_explosive: f32,
    /// Thrown constructs (axes, hammers, discs, shields…): cone (deg), range (m), turn rate at
    /// release (deg/s), decay (s), release angle (deg).
    pub thrown_cone_deg: f32,
    pub thrown_range: f32,
    pub thrown_turn_deg: f32,
    pub thrown_decay_secs: f32,
    pub thrown_release_deg: f32,
    /// Hitscan magnetism: how far (deg) a missing ray may be bent onto a body, per gun archetype
    /// (archetypes without a row: none)…
    pub hitscan: Vec<ArchAngle>,
    /// …for beams (railgun, staff: no tracer archetype)…
    pub hitscan_beam_deg: f32,
    /// …and only within this range (m).
    pub hitscan_range: f32,
}

impl Default for AimTuning {
    fn default() -> Self {
        Self {
            melee_radius: 1.5,
            melee_forms: vec![FormReach { form: Form::Whip, mult: 1.15 }, FormReach { form: Form::Shield, mult: 1.25 }],
            melee_weight: 0.3,
            melee_extra_max: 0.45,
            proj_cone_deg: 12.0,
            proj_range: 80.0,
            proj_turn_deg: 220.0,
            proj_decay_secs: 0.3,
            proj_release_deg: 35.0,
            proj_kinds: vec![
                KindTurn { kind: ProjectileKind::RingBlast, mult: 1.0 },
                KindTurn { kind: ProjectileKind::Bolt, mult: 1.0 },
                KindTurn { kind: ProjectileKind::Arrow, mult: 0.8 },
                KindTurn { kind: ProjectileKind::CannonBall, mult: 0.5 },
                KindTurn { kind: ProjectileKind::Disc, mult: 0.8 },
            ],
            proj_explosive: 0.0,
            thrown_cone_deg: 14.0,
            thrown_range: 45.0,
            thrown_turn_deg: 160.0,
            thrown_decay_secs: 0.5,
            thrown_release_deg: 45.0,
            hitscan: vec![
                ArchAngle { arch: GunArchetype::HandCannon, deg: 1.6 },
                ArchAngle { arch: GunArchetype::Minigun, deg: 0.9 },
                ArchAngle { arch: GunArchetype::SuperShotgun, deg: 0.0 },
                ArchAngle { arch: GunArchetype::Railgun, deg: 0.5 },
            ],
            hitscan_beam_deg: 0.5,
            hitscan_range: 90.0,
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl AimTuning {
    /// Everything off: the sim behaves exactly as before the CONNECT pass (tests' "before").
    pub fn neutral() -> Self {
        Self {
            melee_radius: 1.0,
            melee_forms: Vec::new(),
            melee_weight: 0.0,
            melee_extra_max: 0.0,
            proj_turn_deg: 0.0,
            thrown_turn_deg: 0.0,
            hitscan: Vec::new(),
            hitscan_beam_deg: 0.0,
            ..Self::default()
        }
    }

    pub fn validated(mut self) -> Self {
        let d = Self::default();
        self.melee_radius = fin(self.melee_radius, d.melee_radius, 0.5, 3.0);
        let mut forms: Vec<FormReach> = Vec::with_capacity(self.melee_forms.len());
        for r in &self.melee_forms {
            if !forms.iter().any(|f| f.form == r.form) {
                forms.push(FormReach { form: r.form, mult: fin(r.mult, 1.0, 0.5, 3.0) });
            }
        }
        self.melee_forms = forms;
        self.melee_weight = fin(self.melee_weight, d.melee_weight, 0.0, 2.0);
        self.melee_extra_max = fin(self.melee_extra_max, d.melee_extra_max, 0.0, 1.5);
        self.proj_cone_deg = fin(self.proj_cone_deg, d.proj_cone_deg, 0.0, 45.0);
        self.proj_range = fin(self.proj_range, d.proj_range, 0.0, 400.0);
        self.proj_turn_deg = fin(self.proj_turn_deg, d.proj_turn_deg, 0.0, 1440.0);
        self.proj_decay_secs = fin(self.proj_decay_secs, d.proj_decay_secs, 0.0, 5.0);
        self.proj_release_deg = fin(self.proj_release_deg, d.proj_release_deg, 0.0, 90.0);
        let mut kinds: Vec<KindTurn> = Vec::with_capacity(self.proj_kinds.len());
        for k in &self.proj_kinds {
            if !kinds.iter().any(|q| q.kind == k.kind) {
                kinds.push(KindTurn { kind: k.kind, mult: fin(k.mult, 0.0, 0.0, 4.0) });
            }
        }
        self.proj_kinds = kinds;
        self.proj_explosive = fin(self.proj_explosive, d.proj_explosive, 0.0, 4.0);
        self.thrown_cone_deg = fin(self.thrown_cone_deg, d.thrown_cone_deg, 0.0, 45.0);
        self.thrown_range = fin(self.thrown_range, d.thrown_range, 0.0, 200.0);
        self.thrown_turn_deg = fin(self.thrown_turn_deg, d.thrown_turn_deg, 0.0, 1440.0);
        self.thrown_decay_secs = fin(self.thrown_decay_secs, d.thrown_decay_secs, 0.0, 5.0);
        self.thrown_release_deg = fin(self.thrown_release_deg, d.thrown_release_deg, 0.0, 90.0);
        let mut arch: Vec<ArchAngle> = Vec::with_capacity(self.hitscan.len());
        for a in &self.hitscan {
            if !arch.iter().any(|q| q.arch == a.arch) {
                arch.push(ArchAngle { arch: a.arch, deg: fin(a.deg, 0.0, 0.0, 10.0) });
            }
        }
        self.hitscan = arch;
        self.hitscan_beam_deg = fin(self.hitscan_beam_deg, d.hitscan_beam_deg, 0.0, 10.0);
        self.hitscan_range = fin(self.hitscan_range, d.hitscan_range, 0.0, 400.0);
        self
    }

    /// Radius multiplier for a melee swing of `form` with swing weight `weight` (1 = nominal).
    pub fn melee_mult(&self, form: Form, weight: f32) -> f32 {
        let m = self.melee_forms.iter().find(|f| f.form == form).map_or(self.melee_radius, |f| f.mult);
        let w = if weight.is_finite() { weight.clamp(0.25, 4.0) } else { 1.0 };
        (m * (1.0 + self.melee_weight * (w - 1.0))).max(1.0)
    }

    /// Projectile turn rate at release (rad/s) for a round of `kind` (`explosive` = splash round).
    pub fn proj_turn(&self, kind: ProjectileKind, explosive: bool) -> f32 {
        let k = self.proj_kinds.iter().find(|q| q.kind == kind).map_or(1.0, |q| q.mult);
        let e = if explosive { self.proj_explosive } else { 1.0 };
        (self.proj_turn_deg * k * e).max(0.0).to_radians()
    }

    /// Hitscan magnetism angle (rad) for a round (`trace` = the gun archetype; `None` = a beam).
    pub fn hitscan_rad(&self, trace: Option<GunArchetype>) -> f32 {
        let deg = match trace {
            Some(a) => self.hitscan.iter().find(|q| q.arch == a).map_or(0.0, |q| q.deg),
            None => self.hitscan_beam_deg,
        };
        deg.max(0.0).to_radians()
    }
}

/// Capsule radius of a melee sweep: `base` (the item's own) widened by `mult`, the extra capped.
#[inline]
pub fn melee_radius(base: f32, mult: f32, extra_max: f32) -> f32 {
    if !(mult > 1.0) || !(extra_max > 0.0) {
        return base;
    }
    base + (base * (mult - 1.0)).min(extra_max)
}

/// The enemy nearest the crosshair: inside `range` (eye to hull) and a cone of half-angle
/// acos(`cos_cone`) around `look` (or under it), smallest angle first, then nearest;
/// `clear(eye, point)` = line of sight. Returns (id, hull centre). Deterministic: ties keep the
/// earlier candidate.
pub fn pick_target(
    eye: Vec3,
    look: Vec3,
    cos_cone: f32,
    range: f32,
    candidates: impl IntoIterator<Item = (PlayerId, Aabb)>,
    mut clear: impl FnMut(Vec3, Vec3) -> bool,
) -> Option<(PlayerId, Vec3)> {
    let look = look.normalize_or_zero();
    if look == Vec3::ZERO || !eye.is_finite() || !(range > 0.0) {
        return None;
    }
    let mut best: Option<(PlayerId, Vec3, f32)> = None;
    for (id, hull) in candidates {
        if !hull.min.is_finite() || !hull.max.is_finite() {
            continue;
        }
        let c = hull.center();
        let to = c - eye;
        let dc = to.length();
        if dc < 1e-4 {
            continue;
        }
        let dist = geom::dist_sq_point_aabb(&hull, eye).sqrt();
        if dist > range {
            continue;
        }
        let cos = to.dot(look) / dc;
        let under = geom::segment_aabb(eye, eye + look * range, &hull).is_some();
        if cos < cos_cone && !under {
            continue;
        }
        // Angle off the crosshair first (degrees), distance as a tie-break (≤ 2 deg worth).
        let score = cos.clamp(-1.0, 1.0).acos().to_degrees() + 2.0 * dist / range;
        if best.is_some_and(|(_, _, s)| score >= s) {
            continue;
        }
        if !clear(eye, c) {
            continue;
        }
        best = Some((id, c, score));
    }
    best.map(|(id, c, _)| (id, c))
}

/// One tick of homing: turn `vel` toward `target` (seen from `pos`) by at most `max_turn` radians,
/// keeping its speed. `None` when the target is further than `release` radians off the flight
/// direction (let go), or nothing to do.
pub fn steer(vel: Vec3, pos: Vec3, target: Vec3, max_turn: f32, release: f32) -> Option<Vec3> {
    let speed = vel.length();
    let to = target - pos;
    if !(speed > 1e-3) || !(max_turn > 0.0) || !to.is_finite() || to.length_squared() < 1e-6 {
        return None;
    }
    let v = vel / speed;
    let t = to.normalize();
    let ang = v.dot(t).clamp(-1.0, 1.0).acos();
    if ang < 1e-5 || ang > release {
        return None;
    }
    let axis = v.cross(t).normalize_or_zero();
    if axis == Vec3::ZERO {
        return None;
    }
    Some(Quat::from_axis_angle(axis, ang.min(max_turn)) * v * speed)
}

/// Linear decay of a release-time turn rate over `decay` seconds (0 after).
#[inline]
pub fn decayed(turn: f32, age: f32, decay: f32) -> f32 {
    if !(decay > 0.0) || !(age >= 0.0) || age >= decay {
        return 0.0;
    }
    turn * (1.0 - age / decay)
}

/// Hitscan magnetism: where a ray from `origin` along unit `dir` (length ≤ `range`) should be bent
/// to hit `hull`, if it misses it by at most `max_rad`. Returns (aim point inside the hull, angle).
pub fn magnet_point(origin: Vec3, dir: Vec3, hull: &Aabb, max_rad: f32, range: f32) -> Option<(Vec3, f32)> {
    if !(max_rad > 0.0) || !hull.min.is_finite() || !hull.max.is_finite() {
        return None;
    }
    let c = hull.center();
    let t = (c - origin).dot(dir);
    if !(t > 0.0) || t > range {
        return None;
    }
    // The body point nearest the ray (near its closest approach), pulled a little inside the hull
    // so the bent ray really crosses it.
    let q = origin + dir * t;
    let near = geom::closest_point(hull, q);
    let aim = near + (c - near) * 0.2;
    let to = aim - origin;
    let len = to.length();
    if len < 1e-3 || len > range {
        return None;
    }
    let ang = dir.dot(to / len).clamp(-1.0, 1.0).acos();
    (ang <= max_rad).then_some((aim, ang))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hull_at(p: Vec3) -> Aabb {
        Aabb { min: p - Vec3::new(0.4, 0.0, 0.4), max: p + Vec3::new(0.4, 1.8, 0.4) }
    }

    #[test]
    fn neutral_is_identity() {
        let n = AimTuning::neutral();
        assert_eq!(melee_radius(0.35, n.melee_mult(Form::Sword, 2.0), n.melee_extra_max), 0.35);
        assert_eq!(n.proj_turn(ProjectileKind::RingBlast, false), 0.0);
        assert_eq!(n.hitscan_rad(Some(GunArchetype::HandCannon)), 0.0);
        assert_eq!(n.hitscan_rad(None), 0.0);
        assert_eq!(n.thrown_turn_deg, 0.0);
    }

    #[test]
    fn melee_radius_widens_and_caps() {
        let t = AimTuning::default();
        let r = melee_radius(0.35, t.melee_mult(Form::Sword, 1.0), t.melee_extra_max);
        assert!((r - 0.525).abs() < 1e-4, "{r}");
        let heavy = melee_radius(0.35, t.melee_mult(Form::Hammer, 2.0), t.melee_extra_max);
        assert!(heavy > r);
        assert!(melee_radius(1.0, 3.0, 0.45) <= 1.45 + 1e-5);
    }

    #[test]
    fn pick_prefers_crosshair_and_needs_sight() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let a = hull_at(Vec3::new(1.5, 0.0, -20.0));
        let b = hull_at(Vec3::new(0.3, 0.0, -30.0));
        let got = pick_target(eye, Vec3::NEG_Z, 12f32.to_radians().cos(), 80.0, [(PlayerId(1), a), (PlayerId(2), b)], |_, _| true);
        assert_eq!(got.map(|g| g.0), Some(PlayerId(2)), "nearest the crosshair wins");
        assert!(pick_target(eye, Vec3::NEG_Z, 12f32.to_radians().cos(), 80.0, [(PlayerId(2), b)], |_, _| false).is_none());
        assert!(pick_target(eye, Vec3::X, 12f32.to_radians().cos(), 80.0, [(PlayerId(2), b)], |_, _| true).is_none());
    }

    #[test]
    fn steer_is_bounded_and_keeps_speed() {
        let v = Vec3::new(0.0, 0.0, -50.0);
        let out = steer(v, Vec3::ZERO, Vec3::new(10.0, 0.0, -10.0), 0.05, 1.0).unwrap();
        assert!((out.length() - 50.0).abs() < 1e-3);
        assert!((out.normalize().dot(Vec3::NEG_Z).acos() - 0.05).abs() < 1e-4);
        assert!(steer(v, Vec3::ZERO, Vec3::new(10.0, 0.0, 10.0), 0.05, 1.0).is_none(), "behind: let go");
        assert_eq!(decayed(1.0, 0.15, 0.3), 0.5);
        assert_eq!(decayed(1.0, 0.4, 0.3), 0.0);
    }

    #[test]
    fn magnet_bends_near_misses_only() {
        let hull = hull_at(Vec3::new(0.0, 0.0, -20.0));
        let origin = Vec3::new(0.0, 1.6, 0.0);
        // 1.5 degrees to the side of the body edge.
        let miss = (Quat::from_rotation_y(-(0.4f32 / 20.0).atan() - 1.5f32.to_radians()) * Vec3::NEG_Z).normalize();
        let (aim, ang) = magnet_point(origin, miss, &hull, 2f32.to_radians(), 90.0).expect("bent");
        assert!(ang <= 2f32.to_radians());
        let d = (aim - origin).normalize();
        assert!(geom::segment_aabb(origin, origin + d * 30.0, &hull).is_some(), "the bent ray hits");
        assert!(magnet_point(origin, miss, &hull, 1f32.to_radians(), 90.0).is_none(), "too far off");
        assert!(magnet_point(origin, Vec3::Z, &hull, 0.5, 90.0).is_none(), "behind");
    }
}
