//! **Giants** — big summons you *fight*, not set pieces that launch you (GIANTS worker; owner:
//! "how can we make the giant summons feel more like part of the fight rather than big set pieces
//! that you can't interact with besides getting launched by? they currently are basically giant
//! death balls").
//!
//! A creature is a **giant** when it is a Kaiju, colossal, or at least `giants.min_height` tall
//! (autonomous creatures only: not piloted, not a transformer, not a swarm copy / payload). Everything
//! here is host-authoritative and deterministic; every number is in `assets/tuning/combat.ron` →
//! `giants` ([`GiantTuning`], strict Rust-default parity).
//!
//! * **Anatomy** ([`anatomy`], computed once from the same `ShapeSpec` the client builds the model
//!   from): one box per body part in the giant's local frame (torso, head, each arm / leg / wing, every
//!   tail segment on its own — a staircase up the back), grouped into [`Limb`]s, plus glowing **weak
//!   points** ([`Spot`]): the **nape** (behind the head, the AoT spot), the **core** (chest) and the
//!   **eye** (front of the head). The host sends it once ([`GiantEvent::Born`], also in late-join
//!   manifests) so clients collide / predict / draw with exactly the host's boxes.
//! * **Weak points**: a hit there deals `weak_*` × (Blade / Pierce; other hits get `blunt_frac` of
//!   the bonus; ×`topple_weak` more while it lies toppled), staggers it (`stagger_secs`: the attack in
//!   progress is cancelled — a punish window; `stagger_immune` before the next), and drains a
//!   separate **weak pool** (`weak_pool` × max integrity): empty = it is felled. Per attacker the bonus
//!   applies at most once per `weak_cd` (a minigun on the nape is not a free kill).
//! * **Limbs** have their own hit points (`*_hp` × max integrity). An arm / tail / wing at 0 is
//!   **severed**: its collider vanishes (ropes on it snap), its attack is gone (no swipe from that side,
//!   no tail sweep, no flight) and the client drops it as voxel debris. A damaged leg **limps** (up to
//!   `limp_slow` slower); a leg at 0 **topples** the giant: it rolls onto that side for
//!   `topple_down` s (weak points at ground height, ×`topple_weak`), then gets up (the leg is back at
//!   `leg_recover`; `topple_immune` s before the next topple).
//! * **Solid & rideable**: a giant's part boxes are solid, anchorable colliders that carry the
//!   giant's velocity (movement's moving-platform rule: `Collider::vel` → ground velocity), so players
//!   stand / run on it, tether to the exact part (the anchor follows that part), and wall-touch
//!   refreshes work on its body. Clients get the velocity in `ConstructSnap::vel` and the rotation at
//!   high precision, so prediction uses the same pose as the host.
//! * **Readable attacks**: every giant attack starts with a *warning* ([`GiantEvent::Warn`]: glow +
//!   audio, `warn_*` s) before the animation clip's own telegraph. Stomps land a small **foot** blow and
//!   then send a **shockwave ring** along the ground ([`GiantEvent::Shock`], `shock_speed` m/s) that only
//!   hits grounded players near the ground — any jump clears it. Swipes are **arcs** that sweep across
//!   the front (`swipe_arc`°): the blow reaches you when the arm passes your bearing, so a dodge's
//!   i-frames go through it. Walking never hurts: footsteps only crack the ground (no more ambient
//!   launches), riders on the back are above the tail sweep and the shockwave.
//! * **Command**: the summoner holds the crosshair on an enemy for `command_hold` s (or names them:
//!   the cast's `TargetHint::Player` → `last_target`) and the giant focuses that enemy
//!   ([`GiantEvent::Focus`]) for `focus_secs`, ignoring its temperament's leash.
//! * **Hijack** (`hijack`, off-switchable): an enemy at the nape (standing within `hijack_reach` or
//!   tethered to the head / neck) who keeps charging (the Charge key) for `hijack_secs` without being
//!   knocked off steals it: it becomes their summon for the rest of its life.

use bd_core::input::buttons;
use bd_core::mechanics::{Form, FormCategory, Mechanic, Recipe};
use bd_core::shape::{PartId, PartRole, ShapeSpec};
use bd_core::{ConstructId, PlayerId, Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::ai::AttackKind;
use super::{Construct, ConstructOut, Env, player_key};
use crate::combat::{ConstructHit, HitInfo, HitKind};
use crate::events::GameEvent;
use crate::movement::{AnchorTarget, Collider, ColliderId};

// ============================================================================ tuning

/// Every giant number (`combat.ron` → `giants`). Fractions of max integrity unless noted.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GiantTuning {
    /// Master switch (off = giants behave like the old big creatures).
    pub enabled: bool,
    /// Body height (m) from which a creature is a giant (Kaiju / colossal always are).
    pub min_height: f32,
    /// Weak-point multipliers for Blade / Pierce hits: nape, core (chest), eye.
    pub weak_nape: f32,
    pub weak_core: f32,
    pub weak_eye: f32,
    /// Other hits get this fraction of the bonus (`1 + (m − 1) · blunt_frac`).
    pub blunt_frac: f32,
    /// Extra multiplier while toppled (the weak point is exposed).
    pub topple_weak: f32,
    /// Weak-point damage that fells it.
    pub weak_pool: f32,
    /// Per attacker: seconds before their next weak-point bonus (faster hits count as body hits).
    pub weak_cd: f32,
    /// Weak-point radius as a fraction of body height (clamped 0.8 … 3.5 m).
    pub weak_radius: f32,
    /// Stagger length / immunity after it / minimum weak-point damage that staggers.
    pub stagger_secs: f32,
    pub stagger_immune: f32,
    pub stagger_min: f32,
    /// Limb hit points.
    pub arm_hp: f32,
    pub leg_hp: f32,
    pub tail_hp: f32,
    pub wing_hp: f32,
    /// A hit within this fraction of body height (≥ 0.5 m) of a part box counts on that part.
    pub limb_margin: f32,
    /// Speed lost with a leg at 0 (linear in the worst leg's lost fraction).
    pub limp_slow: f32,
    /// Topple: fall / lying / getting-up seconds, roll angle (deg), immunity after getting up,
    /// hit points a toppling leg gets back.
    pub topple_fall: f32,
    pub topple_down: f32,
    pub topple_rise: f32,
    pub topple_angle: f32,
    pub topple_immune: f32,
    pub leg_recover: f32,
    /// Warning before the clip's telegraph (s): bite / swipe, stomp, tail sweep, breath / charge.
    pub warn_melee: f32,
    pub warn_stomp: f32,
    pub warn_tail: f32,
    pub warn_other: f32,
    /// Swipe arc across the front (deg).
    pub swipe_arc: f32,
    /// Stomp shockwave: ring speed (m/s), reach (× body height, clamped 6 … 30 m), ring thickness
    /// (m), height above the ground it reaches (m; a jump clears it), foot blow radius (× height,
    /// clamped 1.2 … 4 m).
    pub shock_speed: f32,
    pub shock_reach: f32,
    pub shock_band: f32,
    pub shock_height: f32,
    pub foot_radius: f32,
    /// Command: crosshair hold (s), cone half-angle (deg), range (m), focus length (s).
    pub command_hold: f32,
    pub command_cone: f32,
    pub command_range: f32,
    pub focus_secs: f32,
    /// Hijack: on/off, seconds of charging at the nape, reach (m) from the nape.
    pub hijack: bool,
    pub hijack_secs: f32,
    pub hijack_reach: f32,
    /// TITANS (`titan.rs`, piloted walkers): walk speed × this, seconds to full speed / to a stop,
    /// dash speed (× walk speed) and length, dash charges and seconds per charge, torso lag (s).
    pub titan_speed: f32,
    pub titan_accel: f32,
    pub titan_stop: f32,
    pub titan_dash: f32,
    pub titan_dash_secs: f32,
    pub titan_charges: f32,
    pub titan_recharge: f32,
    pub titan_torso_lag: f32,
}

impl Default for GiantTuning {
    fn default() -> Self {
        Self {
            enabled: true,
            min_height: 6.0,
            weak_nape: 3.0,
            weak_core: 2.0,
            weak_eye: 2.5,
            blunt_frac: 0.5,
            topple_weak: 1.5,
            weak_pool: 0.5,
            weak_cd: 0.3,
            weak_radius: 0.12,
            stagger_secs: 0.9,
            stagger_immune: 2.0,
            stagger_min: 4.0,
            arm_hp: 0.16,
            leg_hp: 0.22,
            tail_hp: 0.2,
            wing_hp: 0.14,
            limb_margin: 0.08,
            limp_slow: 0.55,
            topple_fall: 0.55,
            topple_down: 2.5,
            topple_rise: 0.8,
            topple_angle: 78.0,
            topple_immune: 6.0,
            leg_recover: 0.5,
            warn_melee: 0.3,
            warn_stomp: 0.4,
            warn_tail: 0.4,
            warn_other: 0.15,
            swipe_arc: 150.0,
            shock_speed: 15.0,
            shock_reach: 1.2,
            shock_band: 1.5,
            shock_height: 0.9,
            foot_radius: 0.12,
            command_hold: 0.25,
            command_cone: 4.0,
            command_range: 160.0,
            focus_secs: 10.0,
            hijack: true,
            hijack_secs: 2.5,
            hijack_reach: 1.5,
            titan_speed: 1.2,
            titan_accel: 0.2,
            titan_stop: 0.12,
            titan_dash: 2.6,
            titan_dash_secs: 0.28,
            titan_charges: 2.0,
            titan_recharge: 5.0,
            titan_torso_lag: 0.12,
        }
    }
}

impl GiantTuning {
    /// NaN → default, then generous clamps (hostile / hand-edited files stay safe).
    pub fn validated(mut self) -> Self {
        let d = Self::default();
        let fin = |v: f32, d: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.min_height = fin(self.min_height, d.min_height, 1.0, 200.0);
        self.weak_nape = fin(self.weak_nape, d.weak_nape, 1.0, 20.0);
        self.weak_core = fin(self.weak_core, d.weak_core, 1.0, 20.0);
        self.weak_eye = fin(self.weak_eye, d.weak_eye, 1.0, 20.0);
        self.blunt_frac = fin(self.blunt_frac, d.blunt_frac, 0.0, 1.0);
        self.topple_weak = fin(self.topple_weak, d.topple_weak, 1.0, 10.0);
        self.weak_pool = fin(self.weak_pool, d.weak_pool, 0.01, 10.0);
        self.weak_cd = fin(self.weak_cd, d.weak_cd, 0.0, 10.0);
        self.weak_radius = fin(self.weak_radius, d.weak_radius, 0.01, 1.0);
        self.stagger_secs = fin(self.stagger_secs, d.stagger_secs, 0.0, 10.0);
        self.stagger_immune = fin(self.stagger_immune, d.stagger_immune, 0.0, 60.0);
        self.stagger_min = fin(self.stagger_min, d.stagger_min, 0.0, 1e4);
        self.arm_hp = fin(self.arm_hp, d.arm_hp, 0.01, 10.0);
        self.leg_hp = fin(self.leg_hp, d.leg_hp, 0.01, 10.0);
        self.tail_hp = fin(self.tail_hp, d.tail_hp, 0.01, 10.0);
        self.wing_hp = fin(self.wing_hp, d.wing_hp, 0.01, 10.0);
        self.limb_margin = fin(self.limb_margin, d.limb_margin, 0.0, 1.0);
        self.limp_slow = fin(self.limp_slow, d.limp_slow, 0.0, 0.95);
        self.topple_fall = fin(self.topple_fall, d.topple_fall, 0.05, 5.0);
        self.topple_down = fin(self.topple_down, d.topple_down, 0.0, 20.0);
        self.topple_rise = fin(self.topple_rise, d.topple_rise, 0.05, 5.0);
        self.topple_angle = fin(self.topple_angle, d.topple_angle, 0.0, 90.0);
        self.topple_immune = fin(self.topple_immune, d.topple_immune, 0.0, 120.0);
        self.leg_recover = fin(self.leg_recover, d.leg_recover, 0.01, 1.0);
        self.warn_melee = fin(self.warn_melee, d.warn_melee, 0.0, 3.0);
        self.warn_stomp = fin(self.warn_stomp, d.warn_stomp, 0.0, 3.0);
        self.warn_tail = fin(self.warn_tail, d.warn_tail, 0.0, 3.0);
        self.warn_other = fin(self.warn_other, d.warn_other, 0.0, 3.0);
        self.swipe_arc = fin(self.swipe_arc, d.swipe_arc, 10.0, 360.0);
        self.shock_speed = fin(self.shock_speed, d.shock_speed, 1.0, 200.0);
        self.shock_reach = fin(self.shock_reach, d.shock_reach, 0.1, 10.0);
        self.shock_band = fin(self.shock_band, d.shock_band, 0.1, 10.0);
        self.shock_height = fin(self.shock_height, d.shock_height, 0.0, 10.0);
        self.foot_radius = fin(self.foot_radius, d.foot_radius, 0.0, 1.0);
        self.command_hold = fin(self.command_hold, d.command_hold, 0.0, 10.0);
        self.command_cone = fin(self.command_cone, d.command_cone, 0.1, 45.0);
        self.command_range = fin(self.command_range, d.command_range, 1.0, 1000.0);
        self.focus_secs = fin(self.focus_secs, d.focus_secs, 0.0, 600.0);
        self.hijack_secs = fin(self.hijack_secs, d.hijack_secs, 0.1, 60.0);
        self.hijack_reach = fin(self.hijack_reach, d.hijack_reach, 0.1, 20.0);
        self.titan_speed = fin(self.titan_speed, d.titan_speed, 0.1, 5.0);
        self.titan_accel = fin(self.titan_accel, d.titan_accel, 0.02, 3.0);
        self.titan_stop = fin(self.titan_stop, d.titan_stop, 0.02, 3.0);
        self.titan_dash = fin(self.titan_dash, d.titan_dash, 1.0, 10.0);
        self.titan_dash_secs = fin(self.titan_dash_secs, d.titan_dash_secs, 0.02, 2.0);
        self.titan_charges = fin(self.titan_charges, d.titan_charges, 0.0, 10.0);
        self.titan_recharge = fin(self.titan_recharge, d.titan_recharge, 0.1, 120.0);
        self.titan_torso_lag = fin(self.titan_torso_lag, d.titan_torso_lag, 0.0, 2.0);
        self
    }
}

// ============================================================================ anatomy

/// Body regions with their own hit points (`u8` on the wire).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Limb {
    #[default]
    Torso = 0,
    Head = 1,
    ArmL = 2,
    ArmR = 3,
    LegFL = 4,
    LegFR = 5,
    LegBL = 6,
    LegBR = 7,
    Tail = 8,
    WingL = 9,
    WingR = 10,
}

/// Number of [`Limb`] variants.
pub const LIMBS: usize = 11;

impl Limb {
    pub const ALL: [Limb; LIMBS] =
        [Limb::Torso, Limb::Head, Limb::ArmL, Limb::ArmR, Limb::LegFL, Limb::LegFR, Limb::LegBL, Limb::LegBR, Limb::Tail, Limb::WingL, Limb::WingR];

    pub fn from_u8(v: u8) -> Limb {
        Self::ALL.get(v as usize).copied().unwrap_or(Limb::Torso)
    }
    #[inline]
    pub fn is_leg(self) -> bool {
        matches!(self, Limb::LegFL | Limb::LegFR | Limb::LegBL | Limb::LegBR)
    }
    #[inline]
    pub fn is_arm(self) -> bool {
        matches!(self, Limb::ArmL | Limb::ArmR)
    }
    /// Arms, the tail and wings can be cut off; the torso, head and legs can't.
    #[inline]
    pub fn severable(self) -> bool {
        matches!(self, Limb::ArmL | Limb::ArmR | Limb::Tail | Limb::WingL | Limb::WingR)
    }
    /// −1 left, +1 right, 0 centre (local −X = left).
    pub fn side(self) -> f32 {
        match self {
            Limb::ArmL | Limb::LegFL | Limb::LegBL | Limb::WingL => -1.0,
            Limb::ArmR | Limb::LegFR | Limb::LegBR | Limb::WingR => 1.0,
            _ => 0.0,
        }
    }
    /// The limb a part role starts (None: not a limb role — the part belongs to its parent's limb).
    pub fn of_role(r: PartRole) -> Option<Limb> {
        Some(match r {
            PartRole::Head | PartRole::Jaw => Limb::Head,
            PartRole::ArmLeft => Limb::ArmL,
            PartRole::ArmRight => Limb::ArmR,
            PartRole::LegFrontLeft => Limb::LegFL,
            PartRole::LegFrontRight => Limb::LegFR,
            PartRole::LegBackLeft => Limb::LegBL,
            PartRole::LegBackRight => Limb::LegBR,
            PartRole::Tail => Limb::Tail,
            PartRole::WingLeft => Limb::WingL,
            PartRole::WingRight => Limb::WingR,
            _ => return None,
        })
    }
}

/// Which limb a spec part belongs to: its own role, else its parent's (up the chain), else the torso.
/// Clients use it to hide a severed limb's parts.
pub fn limb_of_part(spec: &ShapeSpec, part: PartId) -> Limb {
    let mut id = part;
    for _ in 0..32 {
        if id == 0 {
            return Limb::Torso;
        }
        let Some(p) = spec.parts.iter().find(|p| p.id == id) else { return Limb::Torso };
        if let Some(l) = Limb::of_role(p.role) {
            return l;
        }
        if p.parent == id {
            return Limb::Torso;
        }
        id = p.parent;
    }
    Limb::Torso
}

/// Weak-point kinds (`u8` on the wire).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Spot {
    /// Behind the head (the Attack-on-Titan spot).
    #[default]
    Nape = 0,
    /// Chest.
    Core = 1,
    /// Front of the head.
    Eye = 2,
}

/// One collision / hit box of a part, giant-local (pivot = feet centre, −Z forward), metres at
/// size 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartBox {
    pub center: Vec3,
    pub half: Vec3,
    pub limb: Limb,
    /// Spec part id (the first part merged into this box).
    pub part: u8,
}

/// A weak point: a sphere at `pos` whose zone also extends `2·radius` outward along `out` (so a
/// blow landing on the body box's surface above / in front of it still finds it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WeakPoint {
    pub spot: Spot,
    pub pos: Vec3,
    pub out: Vec3,
    pub radius: f32,
}

/// Max part boxes per giant (torso, head, 2 arms, 4 legs, 2 wings, tail segments; extra tail
/// segments merge).
pub const MAX_PARTS: usize = 20;
pub const MAX_WEAK: usize = 3;

/// A giant's body layout (giant-local, metres at size 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Anatomy {
    pub parts: [PartBox; MAX_PARTS],
    pub np: u8,
    pub weak: [WeakPoint; MAX_WEAK],
    pub nw: u8,
    /// Box around every part (hit tests: melee / shots find the snout and the tail).
    pub union_center: Vec3,
    pub union_half: Vec3,
    /// Model height (m).
    pub height: f32,
}

impl Anatomy {
    pub fn parts(&self) -> &[PartBox] {
        &self.parts[..(self.np as usize).min(MAX_PARTS)]
    }
    pub fn weak_points(&self) -> &[WeakPoint] {
        &self.weak[..(self.nw as usize).min(MAX_WEAK)]
    }
    /// Does it have this limb at all?
    pub fn has(&self, l: Limb) -> bool {
        self.parts().iter().any(|p| p.limb == l)
    }
    /// Union of the boxes of limb `l` (None when absent).
    pub fn limb_box(&self, l: Limb) -> Option<(Vec3, Vec3)> {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for p in self.parts().iter().filter(|p| p.limb == l) {
            lo = lo.min(p.center - p.half);
            hi = hi.max(p.center + p.half);
        }
        lo.is_finite().then(|| ((lo + hi) * 0.5, (hi - lo) * 0.5))
    }
}

/// Extent (spec units) of the additive prims of parts accepted by `f` (rotated prims: their
/// bounding cube, as `constructs::body_box` does).
fn extent(spec: &ShapeSpec, f: impl Fn(PartId) -> bool) -> Option<(Vec3, Vec3)> {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for p in spec.prims.iter().filter(|p| !p.subtract && f(p.part) && p.center.is_finite() && p.size.is_finite()) {
        let h = if p.rot_deg == Vec3::ZERO { p.size.abs() * 0.5 } else { Vec3::splat(p.size.abs().max_element() * 0.5) };
        lo = lo.min(p.center - h);
        hi = hi.max(p.center + h);
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// The body layout of a giant built from `spec` (the cast's shape, or `bd_shapes::generate(recipe)`)
/// — pure: host and tools compute the same thing.
pub fn anatomy(recipe: &Recipe, spec: &ShapeSpec) -> Anatomy {
    let unit = bd_core::CONSTRUCT_VOXEL_SIZE * super::shape_scale(recipe, spec);
    let mut a = Anatomy::default();
    let mut ids: Vec<PartId> = spec.prims.iter().filter(|p| !p.subtract).map(|p| p.part).collect();
    ids.sort_unstable();
    ids.dedup();
    let to_m = |lo: Vec3, hi: Vec3| ((lo + hi) * 0.5 * unit, ((hi - lo) * 0.5 * unit).max(Vec3::splat(0.1)));
    for id in ids {
        if id == bd_shapes::SEAT_PART {
            continue;
        }
        let Some((lo, hi)) = extent(spec, |p| p == id) else { continue };
        let limb = limb_of_part(spec, id);
        let (c, h) = to_m(lo, hi);
        let n = a.np as usize;
        // Every limb is one box except the tail (each segment: a step up the back). A full list
        // merges into the last box of the same limb.
        let merge = if limb != Limb::Tail || n >= MAX_PARTS { a.parts[..n].iter().rposition(|p| p.limb == limb) } else { None };
        match merge {
            Some(i) => {
                let p = &mut a.parts[i];
                let (l2, h2) = ((p.center - p.half).min(c - h), (p.center + p.half).max(c + h));
                p.center = (l2 + h2) * 0.5;
                p.half = (h2 - l2) * 0.5;
            }
            None if n < MAX_PARTS => {
                a.parts[n] = PartBox { center: c, half: h, limb, part: id };
                a.np += 1;
            }
            None => {}
        }
    }
    if a.np == 0 {
        let (lo, hi) = extent(spec, |_| true).unwrap_or((Vec3::new(-2.0, 0.0, -2.0), Vec3::new(2.0, 8.0, 2.0)));
        let (c, h) = to_m(lo, hi);
        a.parts[0] = PartBox { center: c, half: h, limb: Limb::Torso, part: 0 };
        a.np = 1;
    }
    // Union + height.
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for p in a.parts() {
        lo = lo.min(p.center - p.half);
        hi = hi.max(p.center + p.half);
    }
    a.union_center = (lo + hi) * 0.5;
    a.union_half = (hi - lo) * 0.5;
    a.height = (hi.y - lo.y.min(0.0)).max(0.5);
    // Weak points.
    let r = (a.height * GiantTuning::default().weak_radius).clamp(0.8, 3.5);
    let torso = a.limb_box(Limb::Torso).unwrap_or((a.union_center, a.union_half));
    let head = a.limb_box(Limb::Head);
    let head_pivot = spec.parts.iter().find(|p| p.role == PartRole::Head).map(|p| p.pivot * unit);
    let push = |a: &mut Anatomy, spot: Spot, pos: Vec3, out: Vec3, radius: f32| {
        let n = a.nw as usize;
        if n < MAX_WEAK && pos.is_finite() {
            a.weak[n] = WeakPoint { spot, pos, out: out.normalize_or(Vec3::Y), radius };
            a.nw += 1;
        }
    };
    match (head, head_pivot) {
        (Some((hc, hh)), pivot) => {
            // Behind and above the neck pivot (as bd_shapes' model weak point), never inside the
            // head's own front half.
            let pv = pivot.unwrap_or(hc + Vec3::new(0.0, -hh.y * 0.5, hh.z * 0.6));
            let nape = pv + Vec3::new(0.0, r * 0.35, r * 0.6);
            push(&mut a, Spot::Nape, nape, Vec3::new(0.0, 0.6, 0.8), r);
        }
        _ => {
            let (tc, th) = torso;
            push(&mut a, Spot::Nape, tc + Vec3::new(0.0, th.y - r * 0.3, th.z - r * 0.5), Vec3::new(0.0, 0.7, 0.7), r);
        }
    }
    let (tc, th) = torso;
    push(&mut a, Spot::Core, Vec3::new(tc.x, tc.y + th.y * 0.15, tc.z - th.z), Vec3::NEG_Z, r * 0.8);
    if let Some((hc, hh)) = head {
        push(&mut a, Spot::Eye, Vec3::new(hc.x, hc.y + hh.y * 0.2, hc.z - hh.z), Vec3::new(0.0, 0.2, -1.0), (r * 0.6).max(0.6));
    }
    a
}

// ============================================================================ state

/// A stomp shockwave ring in flight.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shock {
    pub on: bool,
    pub origin: Vec3,
    pub r: f32,
    pub r_max: f32,
    pub dmg: f32,
    pub kb: f32,
    /// Players already hit (bit = player id & 63).
    pub hit: u64,
}

/// Per-giant state, embedded in the creature state (`c.ai.giant`). Fixed size, no heap.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GiantState {
    /// Decided (first active tick).
    pub init: bool,
    /// Is a giant.
    pub on: bool,
    pub body: Anatomy,
    pub limb_hp: [f32; LIMBS],
    pub limb_max: [f32; LIMBS],
    pub severed: u16,
    pub weak_hp: f32,
    pub weak_max: f32,
    /// Seconds of stagger left / immunity left; `stagger_new` = not yet seen by the controller.
    pub stagger: f32,
    pub stagger_immune: f32,
    pub stagger_new: bool,
    /// Topple: seconds since it started (< 0: standing), side (±1), pending side (0 = none),
    /// standing yaw + ground height, immunity left, last phase reported.
    pub topple_t: f32,
    pub topple_side: f32,
    pub topple_pending: f32,
    pub topple_yaw: Quat,
    pub topple_y: f32,
    pub topple_immune: f32,
    pub topple_phase: u8,
    /// Per-attacker weak-point cooldowns: (player id + 1, seconds left).
    pub cds: [(u8, f32); 4],
    pub shocks: [Shock; 2],
    /// Swipe: which arm swings (±1 = right / left side, sweeping across).
    pub swipe_side: f32,
    /// Command: focused enemy + seconds left, the enemy under the summoner's crosshair + seconds held,
    /// the summoner's `last_target` already acted on.
    pub focus: Option<PlayerId>,
    pub focus_t: f32,
    pub aim_pid: Option<PlayerId>,
    pub aim_t: f32,
    pub seen_last: Option<PlayerId>,
    /// The cast's name lock (`comp.named`: "kaiju after Jake") already acted on.
    pub seen_named: Option<PlayerId>,
    /// Hijack: who is charging at the nape and for how long.
    pub hijacker: Option<PlayerId>,
    pub hijack_t: f32,
    /// Angular velocity over the last update (rad/s, world): colliders carry riders round with it.
    /// Host: measured in [`end_tick`]; clients: from `ConstructSnap::spin`.
    pub spin: Vec3,
    pub prev_rot: Quat,
    /// Displacement over the last update / dt (the colliders' velocity: a giant blocked by a wall
    /// carries nobody, whatever its steering velocity says). Clients: from `ConstructSnap::vel`.
    pub move_vel: Vec3,
}

impl GiantState {
    #[inline]
    pub fn toppled(&self) -> bool {
        self.topple_t >= 0.0
    }
    #[inline]
    pub fn is_severed(&self, l: Limb) -> bool {
        self.severed & (1 << l as u16) != 0
    }
    /// Has limb `l` and it is still attached.
    #[inline]
    pub fn intact(&self, l: Limb) -> bool {
        self.body.has(l) && !self.is_severed(l)
    }
}

/// Would `c` be a giant (autonomous creature bodies only)?
pub fn qualifies(c: &Construct, t: &GiantTuning) -> bool {
    t.enabled
        && c.category == FormCategory::Creature
        && c.pilot.is_none()
        && !c.suit
        && c.model.is_none()
        && c.parent.is_none()
        && c.slice.is_none()
        && !c.has(Mechanic::Transforms)
        && !c.has(Mechanic::Swarm)
        && (c.recipe.form == Form::Kaiju || c.colossal || c.half.y * 2.0 >= t.min_height)
}

/// First active tick of a creature: decide whether it is a giant and build its anatomy.
pub fn init(c: &mut Construct, t: &GiantTuning, out: &mut ConstructOut) {
    if c.ai.giant.init {
        return;
    }
    c.ai.giant.init = true;
    c.ai.giant.topple_t = -1.0;
    if !qualifies(c, t) {
        return;
    }
    let generated;
    let spec = match c.shape.as_deref() {
        Some(s) if !s.prims.is_empty() => s,
        _ => {
            generated = bd_shapes::generate(&c.recipe);
            &generated
        }
    };
    let body = anatomy(&c.recipe, spec);
    let rot = c.rot;
    let g = &mut c.ai.giant;
    g.on = true;
    g.prev_rot = rot;
    g.body = body;
    g.swipe_side = 1.0;
    let max = c.max_integrity.max(1.0);
    for l in Limb::ALL {
        let frac = match l {
            _ if l.is_arm() => t.arm_hp,
            _ if l.is_leg() => t.leg_hp,
            Limb::Tail => t.tail_hp,
            Limb::WingL | Limb::WingR => t.wing_hp,
            _ => 0.0,
        };
        g.limb_max[l as usize] = frac * max;
        g.limb_hp[l as usize] = frac * max;
    }
    g.weak_max = t.weak_pool * max;
    g.weak_hp = g.weak_max;
    out.events.push(GameEvent::Giant(GiantEvent::Born { id: c.id, body: Box::new(body) }));
}

/// Timers (every tick while it lives).
pub fn tick(c: &mut Construct, dt: f32) {
    let g = &mut c.ai.giant;
    if !g.on {
        return;
    }
    g.stagger = (g.stagger - dt).max(0.0);
    if g.stagger <= 0.0 {
        g.stagger_immune = (g.stagger_immune - dt).max(0.0);
    }
    g.topple_immune = (g.topple_immune - dt).max(0.0);
    for cd in g.cds.iter_mut() {
        cd.1 = (cd.1 - dt).max(0.0);
    }
    g.focus_t = (g.focus_t - dt).max(0.0);
    if g.focus_t <= 0.0 {
        g.focus = None;
    }
}

/// End of the construct update (host): the rotation this tick became the colliders' spin.
pub fn end_tick(c: &mut Construct, dt: f32) {
    let g = &mut c.ai.giant;
    if !g.on {
        return;
    }
    let d = (c.rot * g.prev_rot.inverse()).normalize();
    let (axis, angle) = d.to_axis_angle();
    let angle = if angle > std::f32::consts::PI { angle - std::f32::consts::TAU } else { angle };
    let w = axis * (angle / dt.max(1e-4));
    // Quantised like the snapshot, so host and prediction carry riders identically.
    g.spin = if w.is_finite() && w.length() < 20.0 { crate::replicate::dq_spin(crate::replicate::q_spin(w)) } else { Vec3::ZERO };
    g.prev_rot = c.rot;
    let v = (c.pos - c.prev_pos) / dt.max(1e-4);
    g.move_vel = if v.is_finite() { crate::replicate::QVel::new(v).get() } else { Vec3::ZERO };
}

// ============================================================================ geometry

/// World → giant-local (size 1).
#[inline]
fn to_local(c: &Construct, p: Vec3) -> Vec3 {
    c.rot.inverse() * (p - c.pos) / c.size_scale.max(0.05)
}

/// Giant-local (size 1) → world.
#[inline]
pub fn to_world(c: &Construct, l: Vec3) -> Vec3 {
    c.pos + c.rot * (l * c.size_scale.max(0.05))
}

/// Hit box (world centre, half extents) of a giant: every part (the snout and tail too).
pub fn hit_box(c: &Construct) -> (Vec3, Vec3) {
    let b = &c.ai.giant.body;
    (to_world(c, b.union_center), b.union_half * c.size_scale.max(0.05))
}

/// World position of a weak point.
pub fn weak_world(c: &Construct, w: &WeakPoint) -> Vec3 {
    to_world(c, w.pos)
}

fn in_zone(w: &WeakPoint, l: Vec3) -> bool {
    let d = l - w.pos;
    if d.length_squared() <= w.radius * w.radius {
        return true;
    }
    let along = d.dot(w.out);
    along >= 0.0 && along <= 2.0 * w.radius && (d - w.out * along).length_squared() <= w.radius * w.radius
}

/// Which weak point (if any) a giant-local point is on.
pub fn weak_at(b: &Anatomy, l: Vec3) -> Option<WeakPoint> {
    b.weak_points().iter().copied().find(|w| in_zone(w, l))
}

/// Which intact part a giant-local point is on (nearest box within the margin).
fn part_at(g: &GiantState, l: Vec3, margin: f32) -> Option<Limb> {
    let mut best: Option<(f32, f32, Limb)> = None;
    for p in g.body.parts() {
        if g.is_severed(p.limb) {
            continue;
        }
        let q = (l - p.center).abs() - p.half;
        let d = q.max(Vec3::ZERO).length();
        let vol = p.half.x * p.half.y * p.half.z;
        if d <= margin && best.is_none_or(|b| d < b.0 - 1e-4 || (d <= b.0 + 1e-4 && vol < b.1)) {
            best = Some((d, vol, p.limb));
        }
    }
    best.map(|b| b.2)
}

/// Solid, anchorable part boxes that carry the giant's velocity (host and client prediction).
pub fn push_colliders(c: &Construct, out: &mut Vec<Collider>) {
    let g = &c.ai.giant;
    let s = c.size_scale.max(0.05);
    for (i, p) in g.body.parts().iter().enumerate() {
        if g.is_severed(p.limb) {
            continue;
        }
        let center = c.pos + c.rot * (p.center * s);
        if !center.is_finite() {
            continue;
        }
        let mut col = Collider::new(center, (p.half * s).max(Vec3::splat(0.05)), c.rot, ColliderId::Construct { id: c.id, part: i as u16 + 1 });
        col.vel = g.move_vel;
        col.spin = g.spin;
        col.pivot = c.pos;
        col.solid = true;
        col.anchorable = true;
        out.push(col);
    }
}

/// Which limb (if any) a collider id is on.
pub fn limb_of_collider(c: &Construct, part: u16) -> Option<Limb> {
    (part as usize).checked_sub(1).and_then(|i| c.ai.giant.body.parts().get(i)).map(|p| p.limb)
}

// ============================================================================ damage

fn spot_mult(t: &GiantTuning, s: Spot) -> f32 {
    match s {
        Spot::Nape => t.weak_nape,
        Spot::Core => t.weak_core,
        Spot::Eye => t.weak_eye,
    }
}

/// Integrity damage for a hit on a giant (`base` = the hit's damage with the generic Blade ×2),
/// applying weak points (× bonus, stagger, weak pool), limb hit points (sever / limp / topple).
/// Emits [`GiantEvent`]s. A felled giant gets damage ≥ its integrity.
pub fn damage(c: &mut Construct, hit: &ConstructHit, base: f32, t: &GiantTuning, events: &mut Vec<GameEvent>) -> f32 {
    let l = to_local(c, hit.point);
    let id = c.id;
    let integrity = c.integrity;
    let g = &mut c.ai.giant;
    if !l.is_finite() || base <= 0.0 {
        return base.max(0.0);
    }
    let sharp = hit.blade || hit.pierce;
    // 1. Weak point (per-attacker cooldown: faster hits are body hits).
    if let Some(w) = weak_at(&g.body, l) {
        let key = hit.source.map(|p| p.0.wrapping_add(1));
        let ready = key.is_none_or(|k| !g.cds.iter().any(|(p, s)| *p == k && *s > 0.0));
        if ready {
            if let Some(k) = key {
                let slot = g.cds.iter().position(|(p, _)| *p == k).unwrap_or_else(|| {
                    // Reuse the slot closest to expiring.
                    let mut best = 0;
                    for i in 1..g.cds.len() {
                        if g.cds[i].1 < g.cds[best].1 {
                            best = i;
                        }
                    }
                    best
                });
                g.cds[slot] = (k, t.weak_cd);
            }
            let m = spot_mult(t, w.spot);
            let mut mult = if sharp { m } else { 1.0 + (m - 1.0) * t.blunt_frac };
            if g.toppled() {
                mult *= t.topple_weak;
            }
            let dmg = base * mult;
            g.weak_hp -= dmg;
            let stagger = dmg >= t.stagger_min && g.stagger <= 0.0 && g.stagger_immune <= 0.0 && !g.toppled();
            if stagger {
                g.stagger = t.stagger_secs;
                g.stagger_immune = t.stagger_immune;
                g.stagger_new = true;
            }
            events.push(GameEvent::Giant(GiantEvent::WeakHit { id, spot: w.spot as u8, by: hit.source, point: hit.point, damage: dmg, stagger }));
            if g.weak_hp <= 0.0 {
                g.weak_hp = 0.0;
                events.push(GameEvent::Giant(GiantEvent::Felled { id, by: hit.source }));
                return dmg.max(integrity + 1.0);
            }
            return dmg;
        }
    }
    // 2. Limb.
    let margin = (g.body.height * t.limb_margin).max(0.5);
    if let Some(limb) = part_at(g, l, margin).filter(|l| *l != Limb::Torso && *l != Limb::Head) {
        let i = limb as usize;
        let before = g.limb_hp[i];
        g.limb_hp[i] = (g.limb_hp[i] - base).max(0.0);
        let max = g.limb_max[i].max(1e-3);
        let half = before > max * 0.5 && g.limb_hp[i] <= max * 0.5;
        if g.limb_hp[i] <= 0.0 && before > 0.0 {
            if limb.severable() {
                g.severed |= 1 << i;
                events.push(GameEvent::Giant(GiantEvent::Limb { id, limb: limb as u8, hp: 0.0, severed: true, point: hit.point }));
                return base;
            }
            if limb.is_leg() && !g.toppled() && g.topple_immune <= 0.0 {
                g.topple_pending = if limb.side() < 0.0 { -1.0 } else { 1.0 };
            }
        }
        if half || (g.limb_hp[i] <= 0.0 && before > 0.0) {
            events.push(GameEvent::Giant(GiantEvent::Limb { id, limb: limb as u8, hp: g.limb_hp[i] / max, severed: false, point: hit.point }));
        }
    }
    base
}

// ============================================================================ behaviour

/// Speed multiplier from limping legs (1 = healthy).
pub fn speed_mult(c: &Construct, t: &GiantTuning) -> f32 {
    let g = &c.ai.giant;
    if !g.on {
        return 1.0;
    }
    let mut worst = 0.0f32;
    for l in [Limb::LegFL, Limb::LegFR, Limb::LegBL, Limb::LegBR] {
        let i = l as usize;
        if g.body.has(l) && g.limb_max[i] > 0.0 {
            worst = worst.max(1.0 - g.limb_hp[i] / g.limb_max[i]);
        }
    }
    (1.0 - t.limp_slow * worst.clamp(0.0, 1.0)).max(0.05)
}

/// Both wings gone (a flier that lost its wings walks).
pub fn grounded_by_wings(c: &Construct) -> bool {
    let g = &c.ai.giant;
    g.on && (g.body.has(Limb::WingL) || g.body.has(Limb::WingR)) && !g.intact(Limb::WingL) && !g.intact(Limb::WingR)
}

/// Can the giant still do `kind` (the limb that does it is attached)?
pub fn can(c: &Construct, kind: AttackKind) -> bool {
    let g = &c.ai.giant;
    if !g.on {
        return true;
    }
    match kind {
        AttackKind::Swipe | AttackKind::Grab => !(g.body.has(Limb::ArmL) || g.body.has(Limb::ArmR)) || g.intact(Limb::ArmL) || g.intact(Limb::ArmR),
        AttackKind::TailSweep => !g.body.has(Limb::Tail) || g.intact(Limb::Tail),
        _ => true,
    }
}

/// Is any arm left to swipe with (for choosing Swipe over Bite)?
pub fn has_arm(c: &Construct) -> bool {
    let g = &c.ai.giant;
    !g.on || g.intact(Limb::ArmL) || g.intact(Limb::ArmR) || !(g.body.has(Limb::ArmL) || g.body.has(Limb::ArmR))
}

/// Warning before the clip of `kind` starts (s).
pub fn warn_secs(t: &GiantTuning, kind: AttackKind) -> f32 {
    match kind {
        AttackKind::Bite | AttackKind::Swipe | AttackKind::Lash | AttackKind::Grab | AttackKind::Constrict => t.warn_melee,
        AttackKind::Stomp => t.warn_stomp,
        AttackKind::TailSweep => t.warn_tail,
        _ => t.warn_other,
    }
}

/// Pick the swiping arm: the one on the target's side, unless it is gone.
pub fn choose_swipe_side(c: &mut Construct, target: Vec3) {
    let right = c.rot * Vec3::X;
    let want = if (target - c.pos).dot(right) >= 0.0 { 1.0 } else { -1.0 };
    let g = &mut c.ai.giant;
    let ok = |s: f32| if s > 0.0 { g.intact(Limb::ArmR) || !g.body.has(Limb::ArmR) } else { g.intact(Limb::ArmL) || !g.body.has(Limb::ArmL) };
    g.swipe_side = if ok(want) { want } else { -want };
}

/// Height (above the feet) under which the tail sweep hits (riders on the back are above it).
pub fn tail_top(c: &Construct) -> f32 {
    let g = &c.ai.giant;
    let s = c.size_scale.max(0.05);
    g.body.limb_box(Limb::Tail).map_or(g.body.height * 0.4, |(tc, th)| tc.y + th.y) * s + 1.2
}

/// Swipe arc (giants): the arm sweeps from its own side across the front over the active window;
/// players whose bearing it passed this tick (`u0 → u1`, clip seconds) are hit once. Dodge i-frames
/// go through it (the hit is `HitKind::Creature`).
#[allow(clippy::too_many_arguments)]
pub fn swipe(c: &mut Construct, env: &Env<'_>, t: &GiantTuning, reach: f32, radius: f32, u0: f32, u1: f32, a0: f32, span: f32, dmg: f32, kb: f32, impact: f32, out: &mut ConstructOut) {
    let fwd = flat(c.forward()).normalize_or(Vec3::NEG_Z);
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    let half = (t.swipe_arc.to_radians() * 0.5).min(std::f32::consts::PI);
    let side = c.ai.giant.swipe_side;
    let ang = |u: f32| side * half * (1.0 - 2.0 * ((u - a0) / span.max(1e-3)).clamp(0.0, 1.0));
    let (b0, b1) = (ang(u0), ang(u1));
    let (lo, hi) = (b0.min(b1) - 0.2, b0.max(b1) + 0.2);
    let here = c.pos;
    let h = c.half.y * 2.0;
    let far = radius + reach;
    for p in env.ctx.enemies_of(c.owner) {
        let d = flat(p.pos - here);
        let dist = d.length();
        if dist > far || p.pos.y < here.y - 1.5 || p.pos.y > here.y + h * 0.75 {
            continue;
        }
        let bearing = d.dot(right).atan2(d.dot(fwd));
        if bearing < lo || bearing > hi || !c.hits.insert(player_key(p.id)) {
            continue;
        }
        // Swatted sideways along the sweep, and up.
        let sweep = right * -side;
        let dir = sweep * 0.8 + d.normalize_or_zero() * 0.5 + Vec3::Y * 0.45;
        out.player_hits.push(
            HitInfo::new(p.id, Some(c.owner), HitKind::Creature, dmg, kb, dir, p.hull.center())
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(impact),
        );
        c.ai.attack.emitted |= 8;
    }
}

/// Stomp (giants): the foot itself hits whoever stands right there (grounded, near the ground), then
/// a shockwave ring runs out along the ground.
#[allow(clippy::too_many_arguments)]
pub fn stomp(c: &mut Construct, env: &Env<'_>, t: &GiantTuning, at: Vec3, dmg: f32, kb: f32, impact: f32, out: &mut ConstructOut) {
    let h = c.half.y * 2.0;
    let foot = (h * t.foot_radius).clamp(1.2, 4.0);
    for p in env.ctx.enemies_of(c.owner) {
        let d = flat(p.pos - at).length();
        if d > foot || !p.grounded || (p.pos.y - at.y).abs() > t.shock_height + 0.6 || !c.hits.insert(player_key(p.id)) {
            continue;
        }
        let dir = flat(p.pos - at).normalize_or(Vec3::X) + Vec3::Y * 0.8;
        out.player_hits.push(
            HitInfo::new(p.id, Some(c.owner), HitKind::Creature, dmg, kb, dir, p.pos).with_element(c.recipe.element).with_construct(c.id).with_impact(impact),
        );
    }
    let r_max = (h * t.shock_reach).clamp(6.0, 30.0);
    let g = &mut c.ai.giant;
    let slot = g.shocks.iter().position(|s| !s.on).unwrap_or(0);
    let mut hit = 0u64;
    for p in env.ctx.enemies_of(c.owner) {
        if c.hits.contains(player_key(p.id)) {
            hit |= 1u64 << (p.id.0 & 63);
        }
    }
    g.shocks[slot] = Shock { on: true, origin: at, r: foot, r_max, dmg: dmg * 0.7, kb: kb * 0.8, hit };
    out.events.push(GameEvent::Giant(GiantEvent::Shock { id: c.id, origin: at, radius: r_max, speed: t.shock_speed }));
}

/// Advance the shockwave rings: a ring hits grounded players near the ground as its front passes them.
pub fn step_shocks(c: &mut Construct, env: &Env<'_>, t: &GiantTuning, out: &mut ConstructOut) {
    if !c.ai.giant.shocks.iter().any(|s| s.on) {
        return;
    }
    let dt = env.dt();
    let (owner, id, element) = (c.owner, c.id, c.recipe.element);
    for s in c.ai.giant.shocks.iter_mut().filter(|s| s.on) {
        let r0 = s.r;
        let r1 = (r0 + t.shock_speed * dt).min(s.r_max);
        for p in env.ctx.enemies_of(owner) {
            let bit = 1u64 << (p.id.0 & 63);
            if s.hit & bit != 0 || !p.grounded || (p.pos.y - s.origin.y).abs() > t.shock_height {
                continue;
            }
            let d = flat(p.pos - s.origin).length();
            if d < r0 - t.shock_band || d > r1 {
                continue;
            }
            s.hit |= bit;
            let fall = 1.0 - 0.6 * (d / s.r_max.max(1.0)).clamp(0.0, 1.0);
            let dir = flat(p.pos - s.origin).normalize_or(Vec3::X) + Vec3::Y * 0.9;
            out.player_hits.push(HitInfo::new(p.id, Some(owner), HitKind::Creature, s.dmg * fall, s.kb * fall, dir, p.pos).with_element(element).with_construct(id).with_impact(0.9));
        }
        s.r = r1;
        if r1 >= s.r_max {
            s.on = false;
        }
    }
}

/// Start a pending topple (a leg gave out). Returns true when it began this tick.
pub fn begin_topple(c: &mut Construct, out: &mut ConstructOut) -> bool {
    let side = c.ai.giant.topple_pending;
    if side == 0.0 || c.ai.giant.toppled() {
        c.ai.giant.topple_pending = 0.0;
        return false;
    }
    let yaw = crate::geom::yaw_rotation(flat(c.forward()).normalize_or(Vec3::NEG_Z));
    let g = &mut c.ai.giant;
    g.topple_pending = 0.0;
    g.topple_t = 0.0;
    g.topple_side = side;
    g.topple_yaw = yaw;
    g.topple_y = c.pos.y;
    g.topple_phase = 0;
    g.stagger = 0.0;
    c.vel = Vec3::ZERO;
    out.events.push(GameEvent::Giant(GiantEvent::Topple { id: c.id, phase: 0, side }));
    true
}

#[inline]
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Roll angle (rad) `t` seconds into a topple.
pub fn topple_angle(t: &GiantTuning, secs: f32) -> f32 {
    let a = t.topple_angle.to_radians();
    if secs < t.topple_fall {
        // Falls: accelerating (gravity-like), lands hard.
        let x = secs / t.topple_fall;
        a * x * x
    } else if secs < t.topple_fall + t.topple_down {
        a
    } else {
        a * (1.0 - smooth((secs - t.topple_fall - t.topple_down) / t.topple_rise))
    }
}

/// While toppled: roll onto its side about its feet, lie there, get up. Owns the body (pose, no
/// motion). Returns false once standing (then the creature brain runs as usual).
pub fn step_topple(c: &mut Construct, env: &Env<'_>, t: &GiantTuning, out: &mut ConstructOut) -> bool {
    if !c.ai.giant.toppled() {
        return false;
    }
    let dt = env.dt();
    let secs = c.ai.giant.topple_t + dt;
    let total = t.topple_fall + t.topple_down + t.topple_rise;
    let g = c.ai.giant;
    if secs >= total {
        // Up again: the leg that gave out carries it (partly) again.
        let g = &mut c.ai.giant;
        g.topple_t = -1.0;
        g.topple_immune = t.topple_immune;
        for l in [Limb::LegFL, Limb::LegFR, Limb::LegBL, Limb::LegBR] {
            let i = l as usize;
            g.limb_hp[i] = g.limb_hp[i].max(g.limb_max[i] * t.leg_recover);
        }
        c.rot = g.topple_yaw;
        c.pos.y = g.topple_y;
        c.vel = Vec3::ZERO;
        out.events.push(GameEvent::Giant(GiantEvent::Topple { id: c.id, phase: 3, side: g.topple_side }));
        return false;
    }
    c.ai.giant.topple_t = secs;
    let phase = if secs < t.topple_fall {
        0
    } else if secs < t.topple_fall + t.topple_down {
        1
    } else {
        2
    };
    if phase != g.topple_phase {
        c.ai.giant.topple_phase = phase;
        out.events.push(GameEvent::Giant(GiantEvent::Topple { id: c.id, phase, side: g.topple_side }));
    }
    let roll = Quat::from_rotation_z(-g.topple_side * topple_angle(t, secs));
    // Lift so the lowest corner of the (rolled) body box rests on the ground.
    let (lc, h) = (c.local_center, c.half);
    let mut low = 0.0f32;
    for k in 0..8 {
        let s = Vec3::new(if k & 1 == 0 { -1.0 } else { 1.0 }, if k & 2 == 0 { -1.0 } else { 1.0 }, if k & 4 == 0 { -1.0 } else { 1.0 });
        low = low.min((roll * (lc + h * s)).y);
    }
    c.rot = (g.topple_yaw * roll).normalize();
    c.pos.y = g.topple_y - low;
    c.vel = Vec3::ZERO;
    true
}

/// Command: the summoner's crosshair held on an enemy (or a newly named / hit enemy) focuses the
/// giant on them. Returns the focused enemy while valid.
pub fn command(c: &mut Construct, env: &Env<'_>, t: &GiantTuning, out: &mut ConstructOut) -> Option<PlayerId> {
    let o = env.owner;
    let dt = env.dt();
    let me = c.owner;
    let valid = |pid: PlayerId| env.ctx.player(pid).is_some_and(|p| p.alive && !p.shielded && p.id != me);
    // A named / hit enemy (`last_target` changed): focus at once.
    if o.last_target != c.ai.giant.seen_last {
        c.ai.giant.seen_last = o.last_target;
        if let Some(p) = o.last_target.filter(|p| valid(*p)) {
            set_focus(c, p, t, out);
        }
    }
    // Cast at someone by name: focus at once (the name lock keeps it on them after the focus).
    if c.comp.named != c.ai.giant.seen_named {
        c.ai.giant.seen_named = c.comp.named;
        if let Some(p) = c.comp.named.filter(|p| valid(*p)) {
            set_focus(c, p, t, out);
        }
    }
    // Crosshair hold.
    let mut under: Option<(PlayerId, f32)> = None;
    if o.alive && !o.busy {
        let cos = t.command_cone.to_radians().cos();
        let look = o.look.normalize_or_zero();
        for p in env.ctx.enemies_of(c.owner) {
            let to = p.hull.center() - o.eye;
            let d = to.length();
            if d < 1.0 || d > t.command_range {
                continue;
            }
            let dot = to.dot(look) / d;
            if dot >= cos && under.is_none_or(|u| dot > u.1) {
                under = Some((p.id, dot));
            }
        }
        if let Some((pid, _)) = under {
            let seen = env.ctx.player(pid).is_some_and(|p| super::ai::line_of_sight(env.ctx.voxels, o.eye, p.hull.center()));
            if !seen {
                under = None;
            }
        }
    }
    match under {
        Some((pid, _)) if c.ai.giant.aim_pid == Some(pid) => {
            c.ai.giant.aim_t += dt;
            if c.ai.giant.aim_t >= t.command_hold && c.ai.giant.focus != Some(pid) {
                set_focus(c, pid, t, out);
            } else if c.ai.giant.focus == Some(pid) {
                // Keep it fresh while aimed at.
                c.ai.giant.focus_t = c.ai.giant.focus_t.max(t.focus_secs * 0.5);
            }
        }
        Some((pid, _)) => {
            c.ai.giant.aim_pid = Some(pid);
            c.ai.giant.aim_t = 0.0;
        }
        None => {
            c.ai.giant.aim_pid = None;
            c.ai.giant.aim_t = 0.0;
        }
    }
    let f = c.ai.giant.focus.filter(|p| valid(*p));
    if f.is_none() {
        c.ai.giant.focus = None;
    }
    f
}

fn set_focus(c: &mut Construct, pid: PlayerId, t: &GiantTuning, out: &mut ConstructOut) {
    c.ai.giant.focus = Some(pid);
    c.ai.giant.focus_t = t.focus_secs;
    out.events.push(GameEvent::Giant(GiantEvent::Focus { id: c.id, owner: c.owner, target: pid }));
}

/// Hijack (optional): an enemy clinging to the nape — standing within `hijack_reach` of it, or with a
/// rope on the head — who keeps the Charge key held for `hijack_secs` takes the giant over. Being
/// knocked off / letting go resets it. Returns the new owner when it changes hands.
pub fn step_hijack(c: &mut Construct, env: &Env<'_>, t: &GiantTuning, out: &mut ConstructOut) -> Option<PlayerId> {
    if !t.hijack {
        return None;
    }
    let Some(nape) = c.ai.giant.body.weak_points().iter().find(|w| w.spot == Spot::Nape).copied() else { return None };
    let at = weak_world(c, &nape);
    let reach = t.hijack_reach + nape.radius * c.size_scale.max(0.05);
    let mut who: Option<PlayerId> = None;
    for p in env.ctx.enemies_of(c.owner) {
        if !p.input.held(buttons::CHARGE) || p.seat.is_some() {
            continue;
        }
        let close = (p.hull.center() - at).length() <= reach || crate::geom::dist_sq_point_aabb(&p.hull, at) <= reach * reach;
        if close && who.is_none_or(|w| c.ai.giant.hijacker == Some(p.id) && w != p.id) {
            who = Some(p.id);
        }
    }
    let g = &mut c.ai.giant;
    match who {
        Some(pid) if g.hijacker == Some(pid) => {
            g.hijack_t += env.dt();
            if g.hijack_t >= t.hijack_secs {
                g.hijacker = None;
                g.hijack_t = 0.0;
                g.focus = None;
                g.aim_pid = None;
                out.events.push(GameEvent::Giant(GiantEvent::Hijack { id: c.id, by: pid, progress: 1.0, done: true }));
                return Some(pid);
            }
            // Progress pings every ~0.5 s (client: a filling ring at the nape).
            let k0 = ((g.hijack_t - env.dt()) / 0.5).floor();
            if (g.hijack_t / 0.5).floor() != k0 {
                out.events.push(GameEvent::Giant(GiantEvent::Hijack { id: c.id, by: pid, progress: g.hijack_t / t.hijack_secs, done: false }));
            }
        }
        Some(pid) => {
            g.hijacker = Some(pid);
            g.hijack_t = 0.0;
            out.events.push(GameEvent::Giant(GiantEvent::Hijack { id: c.id, by: pid, progress: 0.0, done: false }));
        }
        None => {
            g.hijacker = None;
            g.hijack_t = 0.0;
        }
    }
    None
}

/// Does player hook `target` hang on this giant's head / neck (for the hijack grip)?
pub fn rope_on_head(c: &Construct, target: AnchorTarget) -> bool {
    match target {
        AnchorTarget::Collider(ColliderId::Construct { id, part }) if id == c.id => limb_of_collider(c, part) == Some(Limb::Head),
        _ => false,
    }
}

// ============================================================================ events / replication

/// Giant moments (`GameEvent::Giant`): presentation + client replica state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GiantEvent {
    /// A creature is a giant: its anatomy (giant-local, size 1). Clients store it on the replica
    /// (colliders for prediction, weak-point glow, severed limbs).
    Born { id: ConstructId, body: Box<Anatomy> },
    /// A weak point took a critical hit (`spot` = [`Spot`]); `stagger` = it reels (attack cancelled).
    WeakHit { id: ConstructId, spot: u8, by: Option<PlayerId>, point: Vec3, damage: f32, stagger: bool },
    /// A limb fell to half / to 0 (`hp` fraction left) or was severed (hide its parts, voxel debris).
    Limb { id: ConstructId, limb: u8, hp: f32, severed: bool, point: Vec3 },
    /// Topple phases: 0 falling (`side` ±1), 1 down, 2 getting up, 3 standing again.
    Topple { id: ConstructId, phase: u8, side: f32 },
    /// Attack warning (`kind` = `AttackKind as u8`): glow + audio cue `secs` before the clip starts.
    Warn { id: ConstructId, kind: u8, aim: Vec3, secs: f32 },
    /// A stomp shockwave ring expanding from `origin` at `speed` m/s up to `radius` m.
    Shock { id: ConstructId, origin: Vec3, radius: f32, speed: f32 },
    /// The summoner pointed the giant at `target` (ping).
    Focus { id: ConstructId, owner: PlayerId, target: PlayerId },
    /// Felled through its weak points.
    Felled { id: ConstructId, by: Option<PlayerId> },
    /// Hijack progress (0…1) at the nape; `done` = it now belongs to `by`.
    Hijack { id: ConstructId, by: PlayerId, progress: f32, done: bool },
}

/// Client replica: apply a giant event to the mirrored construct (anatomy, severed limbs, owner).
pub fn apply_event(store: &mut super::ConstructStore, e: &GiantEvent) {
    match e {
        GiantEvent::Born { id, body } => {
            if let Some(c) = store.get_mut(*id) {
                let g = &mut c.ai.giant;
                g.init = true;
                g.on = true;
                g.body = **body;
                g.topple_t = -1.0;
            }
        }
        GiantEvent::Limb { id, limb, severed: true, .. } => {
            if let Some(c) = store.get_mut(*id) {
                c.ai.giant.severed |= 1 << (*limb as u16).min(15);
            }
        }
        GiantEvent::Hijack { id, by, done: true, .. } => {
            if let Some(c) = store.get_mut(*id) {
                c.owner = *by;
            }
        }
        _ => {}
    }
}

/// Late-join manifest entries for a giant (anatomy + severed limbs).
pub fn manifest(c: &Construct, out: &mut Vec<GameEvent>) {
    let g = &c.ai.giant;
    if !g.on {
        return;
    }
    out.push(GameEvent::Giant(GiantEvent::Born { id: c.id, body: Box::new(g.body) }));
    for l in Limb::ALL {
        if g.is_severed(l) {
            out.push(GameEvent::Giant(GiantEvent::Limb { id: c.id, limb: l as u8, hp: 0.0, severed: true, point: c.center() }));
        }
    }
}

#[inline]
fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}
