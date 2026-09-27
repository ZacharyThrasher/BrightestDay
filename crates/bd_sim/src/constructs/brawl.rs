//! **Transformer combat** (COMBAT.md §5d-quater; owner: "could transformers have combat kind of like
//! Transformers: Devastation?" + "a jet transformer should feel different than a car transformer").
//! Generic for ANY `Transforms` construct, piloted by you (pilot.rs) or running as an AI follower
//! (components.rs `transform_attacks`), tailored by an ARCHETYPE derived from the alt mode's data.
//!
//! * **Archetypes** ([`archetype_of`]): `combat.ron` → `transformer.rules` map a recipe's features /
//!   mechanics / form / mass / scale to a row of `transformer.archetypes` (first rule that matches
//!   wins): car, truck, jet, helicopter, tank, construction, bike — "a transforming bulldozer", "a
//!   helicopter robot", "a motorcycle transformer" land in the right one automatically. Each row tunes
//!   the robot moveset (speed, dash, string, heavy, blaster), the transform attack's SHAPE
//!   ([`XAttack`]) + finisher ([`Finisher`]), vehicle handling (top speed, grip, drift boost, turn) and
//!   the fx flavour (tyre smoke / afterburner / rotor wash / tread dust). New archetypes are data.
//! * **Robot mode** ([`robot`]): LMB press = the next hit of a light string (4–5 hits by archetype,
//!   the last one a stronger ender); hold LMB = the heavy (launcher / super-armor slam / grind);
//!   airborne, LMB = an air string that keeps you up, then an air slam; RMB = the blaster (payload);
//!   Dodge = a quick boost-step. Hits ride the ordinary hit pipeline (juggle protection, juice tiers).
//! * **Transform attacks** ([`start_xattack`] / [`step_xattack`]): Transform pressed during or right
//!   after a string (`combo_window`) bursts out as the vehicle THROUGH the target (the archetype's
//!   shape: freight-train ram, drift-ram, strafing fly-by, rotor sweep, shell barrage, grind,
//!   spin-out), then on a second press or after `xattack_secs` snaps back to robot with a finisher
//!   (uppercut / axe / dive-kick / drop slam / cannon blast / spin slash / J-turn; a slam if
//!   airborne). Combat transforms take `combat_transform_secs` (clients play the panel sequence
//!   faster: `HeldFx::Transform` strength = ± speed).
//! * **Perfect dodge → counter** ([`perfect_dodge`]): a hit that lands in the first
//!   `perfect_window` of a boost-step is dodged; the next attack within `counter_secs` is a
//!   guaranteed heavy that staggers (`HeldFx::Parry` → a chrome flash + a brief desaturation for the
//!   dodger only, presentation-side and comfort-gated). The sim never slows down for anyone.
//! * **Vehicle mode** (pilot.rs): ramming at speed, drift boost (hold Dodge while turning, release
//!   for a burst), off a jump the existing transform-slam.
//!
//! Deterministic (no RNG but id/tick hashes); a piloted transformer is host-driven like every cockpit
//! (prediction never moves the seated pilot), so host = prediction for the pilot.

use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Feature, Form, Mechanic, Recipe};
use bd_core::voxel::VoxelQuery;
use bd_core::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::held::{fx, mech_mult, slam};
use super::{AnimState, Construct, ConstructOut, Driver, Env, Loco, player_key};
use crate::combat::HitInfo;
use crate::combat::HitKind;
use crate::events::{GameEvent, HeldFx};
use crate::geom;
use crate::status::StatusKind;
use crate::weapons::GunArchetype;

// ------------------------------------------------------------------------------------ tuning

/// What holding LMB does in robot mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HeavyKind {
    /// Knock-up (air combos follow).
    #[default]
    Launcher,
    /// A slow slam with super armor while it winds up.
    Armor,
    /// Continuous contact hits while held (drill / saw / rotor).
    Grind,
}

/// The shape of the transform attack's vehicle burst.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum XAttack {
    /// Straight charge THROUGH everything in the way (freight train).
    #[default]
    Ram,
    /// A sliding arc through the target.
    DriftRam,
    /// A strafing gun run over the target (flies).
    FlyBy,
    /// Hovering 360° blade sweep (multi-hit).
    RotorSweep,
    /// Planted shell barrage at the target.
    Barrage,
    /// Slow push with continuous grinding contact hits.
    Grind,
    /// A fast spinning dash through the target.
    SpinOut,
}

/// The robot's finisher after the burst.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Finisher {
    #[default]
    Uppercut,
    /// Big overhead chop (axe / crane arm).
    Axe,
    /// Dive down onto the target from the air.
    DiveKick,
    /// Drop from the hover into a slam.
    DropSlam,
    /// One big explosive cannon-arm shot.
    CannonBlast,
    /// 360° slash.
    SpinSlash,
    /// Flip round to face back through the target and launch it.
    JTurn,
}

/// The robot-mode boost-step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DashKind {
    /// Short fast burst.
    #[default]
    Burst,
    /// Heavy shoulder step (slower, longer).
    Heavy,
    /// A floaty step with a little lift (fliers).
    Hover,
    /// A short planted shuffle (tanks).
    Planted,
}

/// One rule of the archetype table: every condition that is set must hold ("any" lists need one hit).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArchRule {
    /// Archetype name (a row of [`TransformerTuning::archetypes`]).
    pub to: String,
    pub features_any: Vec<Feature>,
    pub features_all: Vec<Feature>,
    pub features_none: Vec<Feature>,
    pub mechanics_any: Vec<Mechanic>,
    pub mechanics_all: Vec<Mechanic>,
    pub forms: Vec<Form>,
    /// 0 = no bound.
    pub min_mass: f32,
    pub max_mass: f32,
    pub min_scale: f32,
    pub max_scale: f32,
}

impl ArchRule {
    pub fn matches(&self, r: &Recipe) -> bool {
        let f = |x: &Feature| r.features.contains(x);
        (self.features_any.is_empty() || self.features_any.iter().any(f))
            && self.features_all.iter().all(f)
            && !self.features_none.iter().any(f)
            && (self.mechanics_any.is_empty() || self.mechanics_any.iter().any(|m| r.has(*m)))
            && self.mechanics_all.iter().all(|m| r.has(*m))
            && (self.forms.is_empty() || self.forms.contains(&r.form))
            && (self.min_mass <= 0.0 || r.mass >= self.min_mass)
            && (self.max_mass <= 0.0 || r.mass <= self.max_mass)
            && (self.min_scale <= 0.0 || r.scale >= self.min_scale)
            && (self.max_scale <= 0.0 || r.scale <= self.max_scale)
    }
}

/// One archetype's moveset / handling / flavour (see the module docs).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Archetype {
    pub name: String,
    // ---- robot movement
    /// × the robot walk speed.
    pub robot_speed: f32,
    /// 0..1 steering while airborne.
    pub air_control: f32,
    pub dash: DashKind,
    pub dash_speed: f32,
    pub dash_secs: f32,
    // ---- light string (LMB presses)
    pub string_hits: u8,
    /// Seconds between string hits.
    pub string_interval: f32,
    pub string_dmg: f32,
    pub string_kb: f32,
    /// Reach past the body (m).
    pub string_reach: f32,
    // ---- heavy (hold LMB)
    pub heavy: HeavyKind,
    pub heavy_hold: f32,
    pub heavy_dmg: f32,
    pub heavy_kb: f32,
    /// Knockback / damage taken while a heavy winds up (1 = none).
    pub heavy_armor: f32,
    // ---- blaster (RMB)
    pub blaster_shots: u8,
    pub blaster_cd: f32,
    pub blaster_dmg: f32,
    pub blaster_speed: f32,
    pub blaster_explode: bool,
    // ---- transform attack
    pub xattack: XAttack,
    pub xattack_secs: f32,
    pub xattack_speed: f32,
    pub xattack_dmg: f32,
    pub xattack_kb: f32,
    /// Hit width / sweep radius (m).
    pub xattack_width: f32,
    pub finisher: Finisher,
    pub finisher_dmg: f32,
    pub finisher_kb: f32,
    // ---- vehicle mode
    /// × the piloted vehicle top speed.
    pub vehicle_speed: f32,
    /// Lateral grip 0..1 (low = slides).
    pub vehicle_grip: f32,
    /// m/s burst after a drift (0 = no drift boost).
    pub drift_boost: f32,
    /// × the turn rate.
    pub vehicle_turn: f32,
    /// Presentation flavour: "tyre_smoke", "exhaust", "afterburner", "rotor_wash", "tread_dust".
    pub fx: String,
}

impl Default for Archetype {
    fn default() -> Self {
        Archetype {
            name: "car".into(),
            robot_speed: 1.0,
            air_control: 0.5,
            dash: DashKind::Burst,
            dash_speed: 20.0,
            dash_secs: 0.16,
            string_hits: 4,
            string_interval: 0.22,
            string_dmg: 5.0,
            string_kb: 9.0,
            string_reach: 1.6,
            heavy: HeavyKind::Launcher,
            heavy_hold: 0.35,
            heavy_dmg: 11.0,
            heavy_kb: 22.0,
            heavy_armor: 1.0,
            blaster_shots: 1,
            blaster_cd: 0.5,
            blaster_dmg: 6.0,
            blaster_speed: 90.0,
            blaster_explode: false,
            xattack: XAttack::Ram,
            xattack_secs: 0.45,
            xattack_speed: 24.0,
            xattack_dmg: 12.0,
            xattack_kb: 24.0,
            xattack_width: 1.5,
            finisher: Finisher::Uppercut,
            finisher_dmg: 14.0,
            finisher_kb: 26.0,
            vehicle_speed: 1.0,
            vehicle_grip: 1.0,
            drift_boost: 8.0,
            vehicle_turn: 1.0,
            fx: "tyre_smoke".into(),
        }
    }
}

fn arch(name: &str, f: impl FnOnce(&mut Archetype)) -> Archetype {
    let mut a = Archetype { name: name.into(), ..Archetype::default() };
    f(&mut a);
    a
}

fn rule(to: &str, f: impl FnOnce(&mut ArchRule)) -> ArchRule {
    let mut r = ArchRule { to: to.into(), ..ArchRule::default() };
    f(&mut r);
    r
}

/// `assets/tuning/combat.ron` → `transformer` (host-synced).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransformerTuning {
    /// A combat transform (mid-combo) takes this long instead of the full sequence (s).
    pub combat_transform_secs: f32,
    /// After a string hit, the Transform key starts a transform attack within this window (s).
    pub combo_window: f32,
    /// The finisher phase length (s; dive-kicks / drop slams last until they land, ≤ 1.2 s).
    pub finisher_secs: f32,
    /// A hit this early into a boost-step is a PERFECT dodge (s).
    pub perfect_window: f32,
    /// The counter window a perfect dodge opens (s).
    pub counter_secs: f32,
    /// Counter heavy: × heavy damage / knockback, and its stagger (stun, s).
    pub counter_mult: f32,
    pub counter_stun: f32,
    /// Boost-step cooldown (s).
    pub dodge_cd: f32,
    /// Upward speed each air-string hit gives (m/s) and air hits before the slam.
    pub air_float: f32,
    pub air_hits: u8,
    /// Launcher / counter stun on players (s; short — juggle protection still applies).
    pub launch_stun: f32,
    /// AI followers: seconds between strings, and boost-step chance per second when close.
    pub ai_string_gap: f32,
    pub ai_dodge_rate: f32,
    /// Ordered: the first rule that matches picks the archetype (unmatched → the first archetype).
    pub rules: Vec<ArchRule>,
    pub archetypes: Vec<Archetype>,
}

impl Default for TransformerTuning {
    fn default() -> Self {
        use Feature as F;
        use Mechanic as M;
        TransformerTuning {
            combat_transform_secs: 0.3,
            combo_window: 0.7,
            finisher_secs: 0.4,
            perfect_window: 0.15,
            counter_secs: 1.0,
            counter_mult: 1.6,
            counter_stun: 0.4,
            dodge_cd: 0.45,
            air_float: 4.0,
            air_hits: 3,
            launch_stun: 0.25,
            ai_string_gap: 0.9,
            ai_dodge_rate: 0.6,
            rules: vec![
                rule("helicopter", |r| r.features_any = vec![F::Rotor]),
                rule("jet", |r| r.features_any = vec![F::Jet]),
                rule("jet", |r| r.mechanics_all = vec![M::Flies, M::Thrust]),
                rule("construction", |r| r.features_any = vec![F::Plow, F::Drill, F::Saw, F::CraneArm]),
                rule("tank", |r| {
                    r.features_all = vec![F::Treads];
                    r.features_any = vec![F::Turret, F::Barrels];
                }),
                rule("bike", |r| {
                    r.features_all = vec![F::Wheels, F::Exhaust];
                    r.features_none = vec![F::Windows, F::Cab, F::Cockpit];
                    r.max_scale = 5.0;
                }),
                rule("truck", |r| r.features_any = vec![F::Cab, F::Smokestack]),
                rule("truck", |r| r.mechanics_any = vec![M::Heavy, M::Stomp]),
                rule("truck", |r| r.min_mass = 5000.0),
                rule("tank", |r| r.features_any = vec![F::Treads]),
                rule("jet", |r| r.mechanics_any = vec![M::Flies]),
                rule("car", |_| {}),
            ],
            archetypes: vec![
                // CAR / SPORTS: agile, fast light strings, drift-ram + J-turn, short burst dashes.
                arch("car", |a| {
                    a.robot_speed = 1.15;
                    a.air_control = 0.6;
                    a.dash_speed = 22.0;
                    a.string_hits = 5;
                    a.string_interval = 0.2;
                    a.blaster_shots = 2;
                    a.blaster_cd = 0.45;
                    a.xattack = XAttack::DriftRam;
                    a.xattack_speed = 26.0;
                    a.xattack_width = 1.3;
                    a.finisher = Finisher::JTurn;
                    a.vehicle_speed = 1.1;
                    a.vehicle_grip = 0.85;
                    a.drift_boost = 10.0;
                    a.vehicle_turn = 1.2;
                }),
                // TRUCK / HEAVY HAULER: slower, heavy hits, armored heavies, freight-train ram, axe.
                arch("truck", |a| {
                    a.robot_speed = 0.85;
                    a.air_control = 0.35;
                    a.dash = DashKind::Heavy;
                    a.dash_speed = 16.0;
                    a.dash_secs = 0.22;
                    a.string_hits = 4;
                    a.string_interval = 0.3;
                    a.string_dmg = 8.0;
                    a.string_kb = 13.0;
                    a.string_reach = 2.0;
                    a.heavy = HeavyKind::Armor;
                    a.heavy_hold = 0.45;
                    a.heavy_dmg = 16.0;
                    a.heavy_kb = 26.0;
                    a.heavy_armor = 0.4;
                    a.blaster_dmg = 10.0;
                    a.blaster_cd = 0.6;
                    a.blaster_speed = 80.0;
                    a.xattack = XAttack::Ram;
                    a.xattack_secs = 0.6;
                    a.xattack_speed = 22.0;
                    a.xattack_dmg = 15.0;
                    a.xattack_kb = 30.0;
                    a.xattack_width = 2.2;
                    a.finisher = Finisher::Axe;
                    a.finisher_dmg = 18.0;
                    a.finisher_kb = 30.0;
                    a.vehicle_speed = 0.95;
                    a.drift_boost = 6.0;
                    a.vehicle_turn = 0.8;
                    a.fx = "exhaust".into();
                }),
                // JET: aerial fighter — launchers into air combos, strafing fly-by + dive-kick; fragile
                // on the ground (weaker ground string), stays airborne with boosts.
                arch("jet", |a| {
                    a.air_control = 1.0;
                    a.dash = DashKind::Hover;
                    a.dash_speed = 20.0;
                    a.dash_secs = 0.18;
                    a.string_dmg = 4.5;
                    a.string_kb = 8.0;
                    a.string_reach = 1.5;
                    a.heavy_hold = 0.3;
                    a.heavy_dmg = 10.0;
                    a.heavy_kb = 24.0;
                    a.blaster_shots = 3;
                    a.blaster_dmg = 5.0;
                    a.blaster_speed = 110.0;
                    a.xattack = XAttack::FlyBy;
                    a.xattack_secs = 0.55;
                    a.xattack_speed = 34.0;
                    a.xattack_dmg = 6.0;
                    a.xattack_kb = 10.0;
                    a.xattack_width = 3.0;
                    a.finisher = Finisher::DiveKick;
                    a.drift_boost = 0.0;
                    a.fx = "afterburner".into();
                }),
                // HELICOPTER: hover control, rotor spin attacks, rotor sweep + drop slam.
                arch("helicopter", |a| {
                    a.robot_speed = 0.95;
                    a.air_control = 0.9;
                    a.dash = DashKind::Hover;
                    a.dash_speed = 16.0;
                    a.dash_secs = 0.2;
                    a.string_interval = 0.24;
                    a.string_reach = 1.8;
                    a.heavy = HeavyKind::Grind;
                    a.heavy_dmg = 3.0;
                    a.heavy_kb = 6.0;
                    a.blaster_shots = 2;
                    a.xattack = XAttack::RotorSweep;
                    a.xattack_secs = 0.6;
                    a.xattack_speed = 0.0;
                    a.xattack_dmg = 4.0;
                    a.xattack_kb = 12.0;
                    a.xattack_width = 5.0;
                    a.finisher = Finisher::DropSlam;
                    a.drift_boost = 0.0;
                    a.fx = "rotor_wash".into();
                }),
                // TANK: slow, ranged-heavy, heaviest armor, short planted dash; shell barrage + cannon arm.
                arch("tank", |a| {
                    a.robot_speed = 0.75;
                    a.air_control = 0.2;
                    a.dash = DashKind::Planted;
                    a.dash_speed = 12.0;
                    a.dash_secs = 0.14;
                    a.string_hits = 3;
                    a.string_interval = 0.35;
                    a.string_dmg = 8.0;
                    a.string_kb = 12.0;
                    a.string_reach = 1.8;
                    a.heavy = HeavyKind::Armor;
                    a.heavy_hold = 0.45;
                    a.heavy_dmg = 14.0;
                    a.heavy_kb = 22.0;
                    a.heavy_armor = 0.35;
                    a.blaster_cd = 0.8;
                    a.blaster_dmg = 14.0;
                    a.blaster_speed = 60.0;
                    a.blaster_explode = true;
                    a.xattack = XAttack::Barrage;
                    a.xattack_secs = 0.6;
                    a.xattack_speed = 0.0;
                    a.xattack_dmg = 12.0;
                    a.xattack_kb = 18.0;
                    a.xattack_width = 3.0;
                    a.finisher = Finisher::CannonBlast;
                    a.finisher_dmg = 18.0;
                    a.finisher_kb = 28.0;
                    a.vehicle_speed = 0.8;
                    a.drift_boost = 0.0;
                    a.vehicle_turn = 0.7;
                    a.fx = "tread_dust".into();
                }),
                // CONSTRUCTION: drill / saw grinds, plow shove, crane axe (they combine: §5d-bis).
                arch("construction", |a| {
                    a.robot_speed = 0.8;
                    a.air_control = 0.3;
                    a.dash = DashKind::Heavy;
                    a.dash_speed = 14.0;
                    a.dash_secs = 0.2;
                    a.string_hits = 3;
                    a.string_interval = 0.3;
                    a.string_dmg = 7.0;
                    a.string_kb = 12.0;
                    a.string_reach = 2.2;
                    a.heavy = HeavyKind::Grind;
                    a.heavy_hold = 0.3;
                    a.heavy_dmg = 3.0;
                    a.heavy_kb = 5.0;
                    a.blaster_dmg = 8.0;
                    a.blaster_cd = 0.8;
                    a.blaster_speed = 70.0;
                    a.xattack = XAttack::Grind;
                    a.xattack_secs = 0.7;
                    a.xattack_speed = 10.0;
                    a.xattack_dmg = 3.0;
                    a.xattack_kb = 8.0;
                    a.xattack_width = 2.5;
                    a.finisher = Finisher::Axe;
                    a.finisher_dmg = 16.0;
                    a.finisher_kb = 26.0;
                    a.vehicle_speed = 0.8;
                    a.drift_boost = 0.0;
                    a.vehicle_turn = 0.8;
                    a.fx = "tread_dust".into();
                }),
                // BIKE: fastest, hit-and-run chip damage, spin-out slash.
                arch("bike", |a| {
                    a.robot_speed = 1.3;
                    a.air_control = 0.7;
                    a.dash_speed = 26.0;
                    a.dash_secs = 0.14;
                    a.string_hits = 5;
                    a.string_interval = 0.17;
                    a.string_dmg = 4.0;
                    a.string_kb = 8.0;
                    a.string_reach = 1.4;
                    a.heavy_hold = 0.3;
                    a.heavy_dmg = 9.0;
                    a.heavy_kb = 20.0;
                    a.blaster_shots = 2;
                    a.blaster_cd = 0.4;
                    a.blaster_dmg = 5.0;
                    a.xattack = XAttack::SpinOut;
                    a.xattack_secs = 0.4;
                    a.xattack_speed = 30.0;
                    a.xattack_dmg = 10.0;
                    a.xattack_kb = 22.0;
                    a.xattack_width = 3.0;
                    a.finisher = Finisher::SpinSlash;
                    a.finisher_dmg = 12.0;
                    a.finisher_kb = 24.0;
                    a.vehicle_speed = 1.2;
                    a.vehicle_grip = 0.75;
                    a.drift_boost = 12.0;
                    a.vehicle_turn = 1.4;
                }),
            ],
        }
    }
}

fn fin(v: f32, d: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl Archetype {
    fn validated(mut self) -> Self {
        let d = Archetype::default();
        self.robot_speed = fin(self.robot_speed, d.robot_speed, 0.2, 3.0);
        self.air_control = fin(self.air_control, d.air_control, 0.0, 1.0);
        self.dash_speed = fin(self.dash_speed, d.dash_speed, 0.0, 60.0);
        self.dash_secs = fin(self.dash_secs, d.dash_secs, 0.02, 1.0);
        self.string_hits = self.string_hits.clamp(1, 8);
        self.string_interval = fin(self.string_interval, d.string_interval, 0.05, 2.0);
        self.string_dmg = fin(self.string_dmg, d.string_dmg, 0.0, 100.0);
        self.string_kb = fin(self.string_kb, d.string_kb, 0.0, 100.0);
        self.string_reach = fin(self.string_reach, d.string_reach, 0.2, 10.0);
        self.heavy_hold = fin(self.heavy_hold, d.heavy_hold, 0.05, 3.0);
        self.heavy_dmg = fin(self.heavy_dmg, d.heavy_dmg, 0.0, 100.0);
        self.heavy_kb = fin(self.heavy_kb, d.heavy_kb, 0.0, 100.0);
        self.heavy_armor = fin(self.heavy_armor, d.heavy_armor, 0.0, 1.0);
        self.blaster_shots = self.blaster_shots.clamp(0, 8);
        self.blaster_cd = fin(self.blaster_cd, d.blaster_cd, 0.05, 10.0);
        self.blaster_dmg = fin(self.blaster_dmg, d.blaster_dmg, 0.0, 100.0);
        self.blaster_speed = fin(self.blaster_speed, d.blaster_speed, 5.0, 300.0);
        self.xattack_secs = fin(self.xattack_secs, d.xattack_secs, 0.1, 3.0);
        self.xattack_speed = fin(self.xattack_speed, d.xattack_speed, 0.0, 80.0);
        self.xattack_dmg = fin(self.xattack_dmg, d.xattack_dmg, 0.0, 100.0);
        self.xattack_kb = fin(self.xattack_kb, d.xattack_kb, 0.0, 100.0);
        self.xattack_width = fin(self.xattack_width, d.xattack_width, 0.2, 20.0);
        self.finisher_dmg = fin(self.finisher_dmg, d.finisher_dmg, 0.0, 100.0);
        self.finisher_kb = fin(self.finisher_kb, d.finisher_kb, 0.0, 100.0);
        self.vehicle_speed = fin(self.vehicle_speed, d.vehicle_speed, 0.2, 3.0);
        self.vehicle_grip = fin(self.vehicle_grip, d.vehicle_grip, 0.0, 1.0);
        self.drift_boost = fin(self.drift_boost, d.drift_boost, 0.0, 40.0);
        self.vehicle_turn = fin(self.vehicle_turn, d.vehicle_turn, 0.1, 4.0);
        self.name.truncate(24);
        self.fx.truncate(24);
        self
    }
}

impl TransformerTuning {
    pub fn validated(mut self) -> Self {
        let d = TransformerTuning::default();
        self.combat_transform_secs = fin(self.combat_transform_secs, d.combat_transform_secs, 0.05, 2.0);
        self.combo_window = fin(self.combo_window, d.combo_window, 0.0, 3.0);
        self.finisher_secs = fin(self.finisher_secs, d.finisher_secs, 0.1, 2.0);
        self.perfect_window = fin(self.perfect_window, d.perfect_window, 0.0, 0.5);
        self.counter_secs = fin(self.counter_secs, d.counter_secs, 0.0, 5.0);
        self.counter_mult = fin(self.counter_mult, d.counter_mult, 0.5, 5.0);
        self.counter_stun = fin(self.counter_stun, d.counter_stun, 0.0, 1.5);
        self.dodge_cd = fin(self.dodge_cd, d.dodge_cd, 0.05, 5.0);
        self.air_float = fin(self.air_float, d.air_float, 0.0, 20.0);
        self.air_hits = self.air_hits.clamp(1, 8);
        self.launch_stun = fin(self.launch_stun, d.launch_stun, 0.0, 1.0);
        self.ai_string_gap = fin(self.ai_string_gap, d.ai_string_gap, 0.1, 10.0);
        self.ai_dodge_rate = fin(self.ai_dodge_rate, d.ai_dodge_rate, 0.0, 10.0);
        self.rules.truncate(64);
        self.archetypes.truncate(32);
        if self.archetypes.is_empty() {
            self.archetypes = d.archetypes;
        }
        self.archetypes = self.archetypes.into_iter().map(Archetype::validated).collect();
        self
    }

    /// The archetype `recipe` fights as (the first rule that matches; unmatched → the first row).
    pub fn archetype_of(&self, recipe: &Recipe) -> &Archetype {
        let name = self.rules.iter().find(|r| r.matches(recipe)).map(|r| r.to.as_str());
        name.and_then(|n| self.archetypes.iter().find(|a| a.name == n)).or_else(|| self.archetypes.first()).unwrap_or_else(|| fallback())
    }
}

fn fallback() -> &'static Archetype {
    static A: std::sync::OnceLock<Archetype> = std::sync::OnceLock::new();
    A.get_or_init(Archetype::default)
}

/// The archetype `recipe` fights as, with the shipped table (tests, tools).
pub fn archetype_of<'a>(t: &'a TransformerTuning, recipe: &Recipe) -> &'a Archetype {
    t.archetype_of(recipe)
}

// ------------------------------------------------------------------------------------ state

/// Transform-attack phase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum XPhase {
    #[default]
    None,
    /// The vehicle burst.
    Burst,
    /// Back to robot: the finisher.
    Finish,
}

/// Per-construct transformer combat state (`Construct::brawl`; fixed size, host-only).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrawlState {
    /// String hits landed so far in the current string.
    pub step: u8,
    /// The last string hit was the string's ender (the moment for a transform attack).
    pub string_done: bool,
    /// Until the next string hit may start (s).
    pub cd: f32,
    /// The combo window: Transform now = a transform attack (s left).
    pub open: f32,
    /// LMB held (s) and whether this hold already fired its heavy.
    pub hold: f32,
    pub heavy_done: bool,
    /// Grind heavy tick timer (s).
    pub grind_t: f32,
    pub air_hits: u8,
    /// Boost-step in progress (s left), its direction, its cooldown.
    pub dodge_t: f32,
    pub dodge_dir: Vec3,
    pub dodge_cd: f32,
    /// Perfect-dodge window left (s) and whether this dodge already flashed.
    pub iframes: f32,
    pub perfect_fx: bool,
    /// Counter window left (s): the next attack is a guaranteed staggering heavy.
    pub counter: f32,
    pub blaster_cd: f32,
    /// Drifting (vehicle mode, s held).
    pub drift_t: f32,
    /// Super armor left (s) while a heavy winds up.
    pub armor_t: f32,
    /// Transform attack.
    pub xatk: XPhase,
    pub xatk_t: f32,
    pub xatk_dir: Vec3,
    pub xatk_tick: f32,
    pub xatk_n: u8,
    /// The finisher started airborne (a slam), or a slam is falling to land.
    pub slam_pending: bool,
    pub fall_speed: f32,
    /// The last transform attack's shape and hit count (tests / tools).
    pub last_xattack: Option<XAttack>,
    pub last_xattack_hits: u8,
    /// AI: until the next decision / string (s).
    pub ai_t: f32,
    /// AI: transform attacks performed (tests / tools).
    pub ai_xattacks: u16,
    /// AI: closing the last metres to melee reach (the brawl machine drives the body).
    pub approach: bool,
}

/// Is the brawl machine moving the body this tick (the AI's locomotion must not override it)?
pub fn moving(c: &Construct) -> bool {
    c.brawl.xatk != XPhase::None || c.brawl.dodge_t > 0.0 || c.brawl.approach
}

fn tick_timers(b: &mut BrawlState, dt: f32) {
    b.cd = (b.cd - dt).max(0.0);
    b.open = (b.open - dt).max(0.0);
    b.dodge_t = (b.dodge_t - dt).max(0.0);
    b.dodge_cd = (b.dodge_cd - dt).max(0.0);
    b.iframes = (b.iframes - dt).max(0.0);
    b.counter = (b.counter - dt).max(0.0);
    b.blaster_cd = (b.blaster_cd - dt).max(0.0);
    b.armor_t = (b.armor_t - dt).max(0.0);
    b.ai_t = (b.ai_t - dt).max(0.0);
}

/// PERFECT DODGE hook (the world's hit paths: a hit on the seated cockpit pilot, `counters::on_damage`
/// for hits on the body): inside the boost-step's first `perfect_window` the hit is dodged and a
/// counter window opens. Returns true when the hit must be ignored.
pub fn perfect_dodge(c: &mut Construct, counter_secs: f32, point: Vec3, events: &mut Vec<GameEvent>) -> bool {
    if c.brawl.iframes <= 0.0 || c.gone.is_some() {
        return false;
    }
    c.brawl.counter = counter_secs;
    if !c.brawl.perfect_fx {
        c.brawl.perfect_fx = true;
        let owner = c.pilot.unwrap_or(c.owner);
        events.push(GameEvent::HeldFx { id: c.id, owner, fx: HeldFx::Parry, pos: if point.is_finite() { point } else { c.center() }, dir: Vec3::Y, strength: 2.0 });
    }
    true
}

/// Knockback / damage multiplier while a heavy's super armor is up (1 = none).
pub fn armor(c: &Construct, t: &TransformerTuning) -> f32 {
    if c.brawl.armor_t > 0.0 { t.archetype_of(&c.recipe).heavy_armor } else { 1.0 }
}

// ------------------------------------------------------------------------------------ input

/// What the robot is asked to do this tick (a pilot's buttons, or the AI's decisions).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub press1: bool,
    pub hold1: bool,
    pub press2: bool,
    pub dodge: bool,
    pub transform: bool,
    /// Horizontal wish direction (unit or zero).
    pub wish: Vec3,
    /// Aim (unit).
    pub aim: Vec3,
}

impl Input {
    /// From a seated pilot's input (Transform comes through `HeldState::transform_req`).
    pub fn of_pilot(inp: &PlayerInput, pressed: u16, look: Vec3, transform: bool) -> Input {
        let (sy, cy) = inp.yaw.sin_cos();
        let fwd = Vec3::new(-sy, 0.0, -cy);
        let right = Vec3::new(cy, 0.0, -sy);
        let typing = inp.held(buttons::TYPING);
        let wish = if typing { Vec3::ZERO } else { (fwd * inp.move_z.clamp(-1, 1) as f32 + right * inp.move_x.clamp(-1, 1) as f32).normalize_or_zero() };
        Input {
            press1: !typing && pressed & buttons::PRIMARY != 0,
            hold1: !typing && inp.held(buttons::PRIMARY),
            press2: !typing && pressed & buttons::SECONDARY != 0,
            dodge: !typing && pressed & buttons::DODGE != 0,
            transform,
            wish,
            aim: look.normalize_or(fwd),
        }
    }
}

// ------------------------------------------------------------------------------------ helpers

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Damage scale with the robot's size (a 9 m Optimus hits harder than a 3 m Arcee), × mechanics.
fn scale_k(c: &Construct) -> (f32, f32) {
    let (dm, km) = mech_mult(c);
    let k = ((c.half.y * 2.0).max(0.5) / 4.0).sqrt().clamp(0.7, 1.8);
    (dm * k, km)
}

/// Enemies whose hull is within `r` of `center` (lag-compensated like every melee).
fn in_reach(c: &Construct, env: &Env<'_>, center: Vec3, r: f32) -> Vec<(bd_core::PlayerId, Vec3)> {
    let mut v = Vec::new();
    for t in env.ctx.enemies_of(c.owner) {
        if Some(t.id) == c.pilot {
            continue;
        }
        let hull = env.ctx.hull_for(c.pilot.unwrap_or(c.owner), t);
        if geom::dist_sq_point_aabb(&hull, center) <= r * r {
            v.push((t.id, hull.center()));
        }
    }
    v
}

#[allow(clippy::too_many_arguments)]
fn hit(c: &Construct, out: &mut ConstructOut, target: bd_core::PlayerId, at: Vec3, dmg: f32, kb: f32, dir: Vec3, impact: f32, stun: f32) {
    out.player_hits.push(
        HitInfo::new(target, Some(c.owner), HitKind::Melee, dmg, kb, dir.normalize_or(Vec3::Y), at).with_element(c.recipe.element).with_construct(c.id).with_impact(impact),
    );
    if stun > 0.0 {
        out.statuses.push((target, StatusKind::Stun, stun, Some(c.owner)));
    }
}

/// A melee strike in front of the robot along `dir`. Returns how many it hit.
#[allow(clippy::too_many_arguments)]
fn strike(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, dir: Vec3, reach: f32, dmg: f32, kb: f32, kb_dir: Vec3, impact: f32, stun: f32, fxk: HeldFx) -> u8 {
    let d = flat(dir).normalize_or(flat(c.forward()).normalize_or(Vec3::NEG_Z));
    let body = c.half.x.max(c.half.z);
    let center = c.center() + d * (body * 0.6 + reach * 0.5) - Vec3::Y * c.half.y * 0.2;
    let r = reach * 0.6 + c.half.y * 0.35;
    let mut n = 0u8;
    for (id, at) in in_reach(c, env, center, r) {
        hit(c, out, id, at, dmg, kb, kb_dir, impact, stun);
        n = n.saturating_add(1);
    }
    c.rot = geom::yaw_rotation(d);
    fx(out, c, fxk, center, d, impact);
    c.set_anim(AnimState::Active, env.tick());
    n
}

/// A 360° strike around the robot.
#[allow(clippy::too_many_arguments)]
fn sweep(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, r: f32, dmg: f32, kb: f32, up: f32, impact: f32, once: bool) -> u8 {
    let center = c.center();
    let mut n = 0u8;
    for (id, at) in in_reach(c, env, center, r + c.half.x.max(c.half.z)) {
        if once && !c.hits.insert(player_key(id)) {
            continue;
        }
        let dir = flat(at - center).normalize_or(c.forward()) + Vec3::Y * up;
        hit(c, out, id, at, dmg, kb, dir, impact, 0.0);
        n = n.saturating_add(1);
    }
    n
}

fn target_point(c: &Construct, env: &Env<'_>) -> Option<Vec3> {
    let t = env.target_for(c)?;
    Some(t.hull.center())
}

/// Instantly switch modes for a combat transform (clients play the panel sequence `speed`× faster).
fn snap(c: &mut Construct, to_robot: bool, t: &TransformerTuning, out: &mut ConstructOut) {
    let secs = t.combat_transform_secs;
    c.held.vehicle = !to_robot;
    c.held.mode_init = true;
    c.held.transform_t = secs;
    super::components::mirror_pilot_mode(c, to_robot);
    let speed = (super::pilot::TRANSFORM_SECS / secs.max(0.05)).max(1.0);
    fx(out, c, HeldFx::Transform, c.center(), c.forward(), if to_robot { speed } else { -speed });
}

fn shoot_bolt(c: &Construct, out: &mut ConstructOut, from: Vec3, dir: Vec3, a: &Archetype, dmg: f32, kb: f32, explode: bool) {
    let arch = if explode { GunArchetype::Cannon } else { GunArchetype::HandCannon };
    super::pilot::shoot(c, out, from, dir, a.blaster_speed, dmg, kb, explode, arch, if explode { 2.0 } else { 0.6 });
}

fn muzzle(c: &Construct, aim: Vec3) -> Vec3 {
    c.center() + Vec3::Y * c.half.y * 0.35 + flat(aim).normalize_or(c.forward()) * (c.half.x.max(c.half.z) + 0.5)
}

// ------------------------------------------------------------------------------------ robot mode

/// Robot-mode combat for one tick (pilot or AI). Movement overrides (boost-step, launcher hop, air
/// float) are written into `c.vel`; the caller integrates. `grounded` = the body's ground contact.
pub fn robot(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, inp: &Input, grounded: bool) {
    let t = &env.ctx.tuning.transformer;
    let a = t.archetype_of(&c.recipe).clone();
    let dt = env.dt();
    tick_timers(&mut c.brawl, dt);
    let (dm, km) = scale_k(c);
    if grounded {
        c.brawl.air_hits = 0;
    }
    // Boost-step (perfect-dodge window at its start).
    if inp.dodge && c.brawl.dodge_cd <= 0.0 && c.brawl.dodge_t <= 0.0 {
        let back = -flat(inp.aim).normalize_or(c.forward());
        c.brawl.dodge_dir = if inp.wish != Vec3::ZERO { inp.wish } else { back };
        c.brawl.dodge_t = a.dash_secs;
        c.brawl.dodge_cd = t.dodge_cd;
        c.brawl.iframes = t.perfect_window;
        c.brawl.perfect_fx = false;
        fx(out, c, HeldFx::Lunge, c.center(), c.brawl.dodge_dir, 1.0);
    }
    if c.brawl.dodge_t > 0.0 {
        let d = c.brawl.dodge_dir * a.dash_speed;
        c.vel.x = d.x;
        c.vel.z = d.z;
        if a.dash == DashKind::Hover {
            c.vel.y = c.vel.y.max(3.0);
        }
    }
    // Counter: the next attack is a guaranteed staggering heavy.
    if c.brawl.counter > 0.0 && inp.press1 {
        c.brawl.counter = 0.0;
        c.brawl.hold = 0.0;
        c.brawl.heavy_done = true;
        let dir = inp.aim;
        let kb_dir = flat(dir).normalize_or(c.forward()) + Vec3::Y * 0.9;
        strike(c, env, out, dir, a.string_reach * 1.4, a.heavy_dmg * t.counter_mult * dm, a.heavy_kb * t.counter_mult * km, kb_dir, 2.4, t.counter_stun, HeldFx::Uppercut);
        c.brawl.open = t.combo_window;
        c.brawl.cd = a.string_interval;
        return;
    }
    // LMB: press = the next string hit; held past `heavy_hold` = the heavy.
    if inp.press1 {
        c.brawl.hold = 0.0;
        c.brawl.heavy_done = false;
        if c.brawl.cd <= 0.0 {
            if grounded {
                string_hit(c, env, out, &a, t, inp.aim, dm, km);
            } else {
                air_hit(c, env, out, &a, t, inp.aim, dm, km);
            }
        }
    } else if inp.hold1 {
        c.brawl.hold += dt;
        if !c.brawl.heavy_done && c.brawl.hold >= a.heavy_hold * 0.6 && a.heavy == HeavyKind::Armor {
            c.brawl.armor_t = c.brawl.armor_t.max(0.2); // super armor while it winds up
        }
        if !c.brawl.heavy_done && c.brawl.hold >= a.heavy_hold {
            heavy(c, env, out, &a, t, inp.aim, dm, km);
            if a.heavy != HeavyKind::Grind {
                c.brawl.heavy_done = true;
            }
        } else if c.brawl.heavy_done && a.heavy == HeavyKind::Grind {
            // (never: grind keeps `heavy_done` false while held)
        }
    } else {
        c.brawl.hold = 0.0;
        c.brawl.heavy_done = false;
    }
    // RMB: the blaster.
    if inp.press2 && c.brawl.blaster_cd <= 0.0 && a.blaster_shots > 0 {
        let explode = a.blaster_explode || c.has(Mechanic::Explodes) || c.recipe.payload.as_ref().is_some_and(|p| p.has(Mechanic::Explodes) || p.form == Form::Bomb);
        let right = inp.aim.cross(Vec3::Y).normalize_or_zero();
        for k in 0..a.blaster_shots {
            let spread = (k as f32 - (a.blaster_shots - 1) as f32 * 0.5) * 0.035;
            shoot_bolt(c, out, muzzle(c, inp.aim), inp.aim + right * spread, &a, a.blaster_dmg * dm, 9.0 * km, explode);
        }
        c.brawl.blaster_cd = a.blaster_cd;
        fx(out, c, HeldFx::PilotAttack, muzzle(c, inp.aim), inp.aim, 1.0);
        c.set_anim(AnimState::Active, env.tick());
    }
}

#[allow(clippy::too_many_arguments)]
fn string_hit(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, t: &TransformerTuning, aim: Vec3, dm: f32, km: f32) {
    let b = &mut c.brawl;
    b.step = if b.open > 0.0 { b.step + 1 } else { 1 };
    let ender = b.step >= a.string_hits;
    b.string_done = ender;
    let (mult, kmult, impact) = if ender { (1.6, 1.8, 1.6) } else { (1.0, 1.0, 1.0 + 0.1 * b.step as f32) };
    let kb_dir = flat(aim).normalize_or(c.forward()) + Vec3::Y * if ender { 0.6 } else { 0.25 };
    strike(c, env, out, aim, a.string_reach, a.string_dmg * mult * dm, a.string_kb * kmult * km, kb_dir, impact, 0.0, HeldFx::PilotAttack);
    // A short step into the hit keeps the string connected.
    let lunge = flat(aim).normalize_or(c.forward()) * 4.0;
    c.vel.x += lunge.x;
    c.vel.z += lunge.z;
    let b = &mut c.brawl;
    b.cd = if ender { a.string_interval * 2.2 } else { a.string_interval };
    b.open = t.combo_window;
    if ender {
        b.step = 0;
    }
}

#[allow(clippy::too_many_arguments)]
fn air_hit(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, t: &TransformerTuning, aim: Vec3, dm: f32, km: f32) {
    if c.brawl.air_hits >= t.air_hits {
        // Air slam finisher: dive to the ground; the landing slams.
        c.vel = Vec3::new(c.vel.x * 0.3, -26.0, c.vel.z * 0.3);
        c.brawl.slam_pending = true;
        c.brawl.fall_speed = 26.0;
        c.brawl.air_hits = 0;
        c.brawl.cd = a.string_interval * 2.0;
        fx(out, c, HeldFx::DiveStart, c.center(), Vec3::NEG_Y, 1.5);
        return;
    }
    c.brawl.air_hits += 1;
    let kb_dir = flat(aim).normalize_or(c.forward()) * 0.3 + Vec3::Y * 0.9;
    strike(c, env, out, aim, a.string_reach, a.string_dmg * dm, a.string_kb * 0.8 * km, kb_dir, 1.1, 0.0, HeldFx::PilotAttack);
    c.vel.y = c.vel.y.max(t.air_float * (0.6 + 0.4 * a.air_control));
    c.brawl.cd = a.string_interval;
    c.brawl.open = t.combo_window;
}

#[allow(clippy::too_many_arguments)]
fn heavy(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, t: &TransformerTuning, aim: Vec3, dm: f32, km: f32) {
    let d = flat(aim).normalize_or(c.forward());
    match a.heavy {
        HeavyKind::Launcher => {
            strike(c, env, out, aim, a.string_reach * 1.2, a.heavy_dmg * dm, a.heavy_kb * km, d * 0.25 + Vec3::Y * 1.4, 2.0, t.launch_stun, HeldFx::Uppercut);
            // Fliers follow their launcher up (air combos).
            if a.air_control >= 0.9 {
                c.vel.y = c.vel.y.max(10.0);
            }
        }
        HeavyKind::Armor => {
            c.brawl.armor_t = c.brawl.armor_t.max(0.3);
            strike(c, env, out, aim, a.string_reach * 1.4, a.heavy_dmg * dm, a.heavy_kb * km, d + Vec3::Y * 0.5, 2.2, t.launch_stun, HeldFx::GroundPound);
        }
        HeavyKind::Grind => {
            c.brawl.grind_t -= env.dt();
            if c.brawl.grind_t <= 0.0 {
                c.brawl.grind_t = 0.12;
                strike(c, env, out, aim, a.string_reach, a.heavy_dmg * dm, a.heavy_kb * km, d + Vec3::Y * 0.1, 0.6, 0.0, HeldFx::PilotAttack);
            }
            if c.brawl.hold > a.heavy_hold + 1.2 {
                c.brawl.heavy_done = true; // a grind lasts ≤ 1.2 s per hold
            }
        }
    }
    c.brawl.open = t.combo_window;
    c.brawl.cd = a.string_interval;
    c.brawl.step = 0;
}

/// A slam that was falling lands (`grounded`): the shockwave. Returns true when it fired.
pub fn land(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, grounded: bool) -> bool {
    if !c.brawl.slam_pending {
        return false;
    }
    c.brawl.fall_speed = c.brawl.fall_speed.max(-c.vel.y);
    if !grounded {
        return false;
    }
    c.brawl.slam_pending = false;
    let fall = c.brawl.fall_speed;
    let at = c.pos + c.rot * c.local_center - Vec3::Y * c.half.y;
    slam(c, env, out, at, fall, HeldFx::TransformSlam);
    true
}

// ------------------------------------------------------------------------------------ transform attacks

/// Can the Transform key start a transform attack now (robot mode, inside the combo window)?
pub fn wants_xattack(c: &Construct) -> bool {
    c.brawl.xatk == XPhase::None && c.brawl.open > 0.0 && !c.held.vehicle && c.has(Mechanic::Transforms)
}

/// Burst out as the vehicle through the target (see [`XAttack`]).
pub fn start_xattack(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, aim: Vec3) {
    let t = &env.ctx.tuning.transformer;
    let a = t.archetype_of(&c.recipe);
    let here = c.center();
    let dir = match target_point(c, env) {
        Some(tp) if (tp - here).length() < 30.0 => flat(tp - here).normalize_or(flat(aim)),
        _ => flat(aim).normalize_or(flat(c.forward())),
    }
    .normalize_or(Vec3::NEG_Z);
    let b = &mut c.brawl;
    b.xatk = XPhase::Burst;
    b.xatk_t = 0.0;
    b.xatk_tick = 0.0;
    b.xatk_n = 0;
    b.xatk_dir = dir;
    b.open = 0.0;
    b.step = 0;
    b.string_done = false;
    b.last_xattack = Some(a.xattack);
    b.last_xattack_hits = 0;
    c.hits.clear();
    c.rot = geom::yaw_rotation(dir);
    snap(c, false, t, out);
    fx(out, c, HeldFx::PilotAttack, here, dir, 2.0);
}

/// Advance a transform attack. Sets the body's velocity / facing; returns true while it flies (the
/// caller integrates with flight, else on the ground). `again` = Transform pressed again (the finisher
/// comes now). `grounded` = ground contact before this tick.
pub fn step_xattack(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, again: bool, grounded: bool) -> bool {
    let t = &env.ctx.tuning.transformer;
    let a = t.archetype_of(&c.recipe).clone();
    let dt = env.dt();
    let (dm, km) = scale_k(c);
    tick_timers(&mut c.brawl, dt);
    c.brawl.xatk_t += dt;
    let dir = c.brawl.xatk_dir;
    let tp = target_point(c, env);
    match c.brawl.xatk {
        XPhase::None => false,
        XPhase::Burst => {
            let u = (c.brawl.xatk_t / a.xattack_secs).min(1.0);
            let mut flying = false;
            c.brawl.xatk_tick -= dt;
            let tick = c.brawl.xatk_tick <= 0.0;
            match a.xattack {
                XAttack::Ram | XAttack::Grind | XAttack::DriftRam | XAttack::SpinOut => {
                    let (vdir, face) = match a.xattack {
                        // A sliding arc: the travel swings ~70° while the nose points into the slide.
                        XAttack::DriftRam => {
                            let v = Quat::from_rotation_y(1.2 * u) * dir;
                            (v, Quat::from_rotation_y(1.2 * u + 0.6) * dir)
                        }
                        _ => (dir, dir),
                    };
                    c.vel.x = vdir.x * a.xattack_speed;
                    c.vel.z = vdir.z * a.xattack_speed;
                    c.rot = if a.xattack == XAttack::SpinOut { Quat::from_rotation_y(c.brawl.xatk_t * 18.0) * geom::yaw_rotation(face) } else { geom::yaw_rotation(face) };
                    if a.xattack == XAttack::Grind && tick {
                        c.brawl.xatk_tick = 0.12;
                        c.hits.clear();
                    }
                    let side = Vec3::new(-vdir.z, 0.0, vdir.x);
                    let body = geom::inflate(&c.bounds(), a.xattack_width * 0.5);
                    for t2 in env.ctx.enemies_of(c.owner) {
                        if Some(t2.id) == c.pilot || !t2.hull.intersects(&body) || !c.hits.insert(player_key(t2.id)) {
                            continue;
                        }
                        let kb_dir = match a.xattack {
                            XAttack::DriftRam => side + vdir * 0.5 + Vec3::Y * 0.4,
                            XAttack::SpinOut => side * if (t2.pos - c.pos).dot(side) >= 0.0 { 1.0 } else { -1.0 } + Vec3::Y * 0.4,
                            _ => vdir + Vec3::Y * 0.35,
                        };
                        hit(c, out, t2.id, t2.hull.center(), a.xattack_dmg * dm, a.xattack_kb * km, kb_dir, 1.6, 0.0);
                        c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(1);
                        fx(out, c, HeldFx::PilotAttack, t2.hull.center(), vdir, 1.6);
                    }
                }
                XAttack::FlyBy => {
                    // Climb over the target and strafe it with bolts on the way past.
                    flying = true;
                    let over = tp.map_or(c.pos.y + 5.0, |p| p.y + 5.0);
                    let climb = ((over - c.pos.y) * 4.0).clamp(-10.0, 14.0);
                    c.vel = dir * a.xattack_speed + Vec3::Y * climb;
                    c.rot = geom::look_rotation(c.vel.normalize_or(dir));
                    if tick && c.brawl.xatk_n < 8 {
                        c.brawl.xatk_tick = a.xattack_secs / 8.0;
                        c.brawl.xatk_n += 1;
                        let from = c.center() - Vec3::Y * c.half.y;
                        let aim_at = tp.unwrap_or(c.pos + dir * 8.0 - Vec3::Y * 4.0);
                        let d = (aim_at + dir * (c.brawl.xatk_n as f32 - 4.0) * 0.8 - from).normalize_or(Vec3::NEG_Y);
                        shoot_bolt(c, out, from, d, &a, a.xattack_dmg * dm, a.xattack_kb * km, false);
                        c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(1);
                    }
                }
                XAttack::RotorSweep => {
                    // Hover up a little and sweep everything around with the blades.
                    flying = true;
                    c.vel = Vec3::new(c.vel.x * 0.5, (2.5 - c.brawl.xatk_t * 4.0).max(0.0), c.vel.z * 0.5);
                    c.rot = Quat::from_rotation_y(c.brawl.xatk_t * 16.0) * geom::yaw_rotation(dir);
                    if tick {
                        c.brawl.xatk_tick = 0.15;
                        let n = sweep(c, env, out, a.xattack_width, a.xattack_dmg * dm, a.xattack_kb * km, 0.5, 1.0, false);
                        c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(n);
                        fx(out, c, HeldFx::PilotAttack, c.center(), Vec3::Y, 1.2);
                    }
                }
                XAttack::Barrage => {
                    // Planted: a volley of shells at the target.
                    c.vel.x *= 0.5;
                    c.vel.z *= 0.5;
                    c.rot = geom::yaw_rotation(dir);
                    if tick && c.brawl.xatk_n < 4 {
                        c.brawl.xatk_tick = a.xattack_secs / 4.0;
                        c.brawl.xatk_n += 1;
                        let from = muzzle(c, dir) + Vec3::Y * 0.5;
                        let aim_at = tp.unwrap_or(from + dir * 20.0);
                        let spread = Vec3::new(-dir.z, 0.0, dir.x) * (c.brawl.xatk_n as f32 - 2.5) * 0.9;
                        let d = (aim_at + spread - from).normalize_or(dir);
                        super::pilot::shoot(c, out, from, d, 55.0, a.xattack_dmg * dm, a.xattack_kb * km, true, GunArchetype::Cannon, 2.5);
                        c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(1);
                    }
                }
            }
            c.set_anim(AnimState::Move, env.tick());
            if c.brawl.xatk_t >= a.xattack_secs || (again && c.brawl.xatk_t > 0.1) {
                let low = !flying || near_ground(c, env, 1.2);
                begin_finish(c, env, out, &a, grounded && low, dm, km);
            }
            flying
        }
        XPhase::Finish => {
            let slam_kind = matches!(a.finisher, Finisher::DiveKick | Finisher::DropSlam) || c.brawl.slam_pending;
            if a.finisher == Finisher::DiveKick && !grounded {
                // Keep diving at the target; the contact hits.
                let to = tp.map_or(Vec3::NEG_Y, |p| (p - c.center()).normalize_or(Vec3::NEG_Y));
                let d = (to + Vec3::NEG_Y * 0.6).normalize_or(Vec3::NEG_Y);
                c.vel = d * 28.0;
                let body = geom::inflate(&c.bounds(), 0.8);
                for t2 in env.ctx.enemies_of(c.owner) {
                    if Some(t2.id) != c.pilot && t2.hull.intersects(&body) && c.hits.insert(player_key(t2.id)) {
                        hit(c, out, t2.id, t2.hull.center(), a.finisher_dmg * dm, a.finisher_kb * km, flat(d) + Vec3::Y * 0.5, 2.2, 0.0);
                        c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(1);
                    }
                }
            }
            let landed = land(c, env, out, grounded);
            let limit = if slam_kind { 1.2 } else { t.finisher_secs };
            if landed || c.brawl.xatk_t >= limit {
                c.brawl.slam_pending = false;
                c.brawl.xatk = XPhase::None;
                c.brawl.cd = a.string_interval;
                c.brawl.open = 0.0;
            }
            false
        }
    }
}

/// Ground within `depth` m under the body's sole (bumpy ground / a hop still counts as grounded).
fn near_ground(c: &Construct, env: &Env<'_>, depth: f32) -> bool {
    let sole = c.pos + c.rot * c.local_center - Vec3::Y * c.half.y;
    let v = env.ctx.voxels;
    let mut y = 0.0;
    while y <= depth {
        if v.is_solid(bd_core::voxel::world_to_voxel(sole - Vec3::Y * (y + 0.05))) {
            return true;
        }
        y += 0.25;
    }
    false
}

fn begin_finish(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, grounded: bool, dm: f32, km: f32) {
    let t = &env.ctx.tuning.transformer;
    let grounded = grounded || near_ground(c, env, 1.2);
    c.brawl.xatk = XPhase::Finish;
    c.brawl.xatk_t = 0.0;
    c.hits.clear();
    snap(c, true, t, out);
    let here = c.center();
    let to = target_point(c, env).map(|p| p - here).filter(|v| flat(*v).length() < 14.0);
    let dir = to.map_or(c.brawl.xatk_dir, |v| flat(v).normalize_or(c.brawl.xatk_dir));
    // The robot SNAPS back in on the target (the burst knocked it along the travel): a short lunge
    // (≤ 9 m) so the finisher connects — never into terrain.
    if grounded && matches!(a.finisher, Finisher::Uppercut | Finisher::JTurn | Finisher::Axe | Finisher::SpinSlash) {
        if let Some(v) = to {
            let body = c.half.x.max(c.half.z);
            let gap = flat(v).length() - (body + 1.2);
            if gap > 0.0 {
                let step = dir * gap.min(9.0);
                let dest = c.pos + step;
                let mid = dest + c.rot * c.local_center;
                if !env.ctx.voxels.is_solid(bd_core::voxel::world_to_voxel(mid)) {
                    c.pos = dest;
                    fx(out, c, HeldFx::Lunge, here, dir, 1.5);
                }
            }
        }
    }
    let here = c.center();
    let fall = |c: &mut Construct| {
        c.brawl.slam_pending = true;
        c.brawl.fall_speed = 22.0;
        c.vel = Vec3::new(c.vel.x * 0.2, -22.0, c.vel.z * 0.2);
    };
    let mut n = 0u8;
    match a.finisher {
        Finisher::DiveKick => {
            if grounded {
                n = strike(c, env, out, dir, 2.0, a.finisher_dmg * dm, a.finisher_kb * km, dir + Vec3::Y * 0.8, 2.2, 0.0, HeldFx::Uppercut);
            } else {
                c.brawl.slam_pending = true;
                c.brawl.fall_speed = 28.0;
                fx(out, c, HeldFx::DiveStart, here, Vec3::NEG_Y, 2.0);
            }
        }
        Finisher::DropSlam => {
            if grounded {
                c.brawl.slam_pending = true;
                c.brawl.fall_speed = 22.0;
            } else {
                fall(c);
            }
            fx(out, c, HeldFx::DiveStart, here, Vec3::NEG_Y, 1.8);
        }
        _ if !grounded => fall(c), // any finisher in the air becomes a slam
        Finisher::Uppercut => {
            n = strike(c, env, out, dir, 2.0, a.finisher_dmg * dm, a.finisher_kb * km, dir * 0.3 + Vec3::Y * 1.6, 2.4, t.launch_stun, HeldFx::Uppercut);
            c.vel.y = c.vel.y.max(6.0);
        }
        Finisher::JTurn => {
            // Flip round, facing back through the target, and launch it.
            let back = to.map_or(-c.brawl.xatk_dir, |v| flat(v).normalize_or(-c.brawl.xatk_dir));
            n = strike(c, env, out, back, 2.2, a.finisher_dmg * dm, a.finisher_kb * km, back * 0.4 + Vec3::Y * 1.4, 2.3, t.launch_stun, HeldFx::Uppercut);
        }
        Finisher::Axe => {
            n = strike(c, env, out, dir, 2.8, a.finisher_dmg * dm, a.finisher_kb * km, dir + Vec3::Y * 0.7, 2.6, t.launch_stun, HeldFx::GroundPound);
            out.carves.push(crate::combat::CarveRequest { center: here + dir * (c.half.z + 2.0) - Vec3::Y * c.half.y, radius: (c.half.y * 0.35).clamp(1.0, 2.5), power: 0.6 });
        }
        Finisher::CannonBlast => {
            let aim = to.map_or(dir, |v| v.normalize_or(dir));
            super::pilot::shoot(c, out, muzzle(c, aim), aim, 70.0, a.finisher_dmg * dm, a.finisher_kb * km, true, GunArchetype::Cannon, 5.0);
            fx(out, c, HeldFx::PilotAttack, muzzle(c, aim), aim, 2.5);
            n = 1;
        }
        Finisher::SpinSlash => {
            n = sweep(c, env, out, 2.5, a.finisher_dmg * dm, a.finisher_kb * km, 0.6, 2.2, true);
            fx(out, c, HeldFx::PilotAttack, here, Vec3::Y, 2.0);
        }
    }
    c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(n);
}

// ------------------------------------------------------------------------------------ AI followers

/// A transformer follower's robot-mode brain: close in (the mode machine walks it), strings when in
/// reach, a transform attack as the string's extender, the blaster at range, boost-steps when
/// someone is close (a hit inside the window = a counter). Runs from `components::react`.
pub fn ai(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let grounded = c.grounded || c.body.supported;
    if c.brawl.xatk != XPhase::None {
        c.body.claim(Driver::Charge);
        let flying = step_xattack(c, env, out, false, grounded);
        c.body.loco = if flying { Loco::Fly } else { Loco::Walk };
        return;
    }
    if land(c, env, out, grounded) {
        return;
    }
    let t = &env.ctx.tuning.transformer;
    let Some(tv) = env.target_for(c) else {
        c.brawl.approach = false;
        tick_timers(&mut c.brawl, env.dt());
        return;
    };
    let a = t.archetype_of(&c.recipe).clone();
    let here = c.center();
    let to = tv.hull.center() - here;
    let dist = flat(to).length();
    let reach = c.half.x.max(c.half.z) + a.string_reach + 1.0;
    let aim = to.normalize_or(c.forward());
    let mut inp = Input { aim, ..Input::default() };
    // Close the last metres into melee (the mode machine's walk stops short of a big robot's reach).
    c.brawl.approach = dist > reach * 0.85 && dist < 18.0;
    if c.brawl.approach {
        let d = flat(to).normalize_or(c.forward());
        let v = d * 6.0 * a.robot_speed * super::components::speed_mult(c);
        c.vel.x = v.x;
        c.vel.z = v.z;
        c.rot = geom::yaw_rotation(d);
        c.body.claim(Driver::Steer);
        c.body.loco = Loco::Walk;
        c.set_anim(AnimState::Move, env.tick());
    }
    let hash = crate::constructs::ai::hash01(c.id.0, env.tick() as u64, 77);
    // Boost-step now and then when they're close (sideways): a hit inside the window is a counter.
    if dist < reach + 2.0 && c.brawl.dodge_cd <= 0.0 && hash < t.ai_dodge_rate * env.dt() {
        let side = Vec3::new(-aim.z, 0.0, aim.x) * if hash < t.ai_dodge_rate * env.dt() * 0.5 { 1.0 } else { -1.0 };
        inp.dodge = true;
        inp.wish = side.normalize_or_zero();
    }
    if dist <= reach {
        if c.brawl.counter > 0.0 {
            inp.press1 = true;
        } else if c.brawl.string_done && c.brawl.open > 0.0 && c.brawl.cd <= a.string_interval {
            // The string is done: extend it with a transform attack.
            c.brawl.ai_xattacks = c.brawl.ai_xattacks.saturating_add(1);
            c.brawl.ai_t = t.ai_string_gap;
            start_xattack(c, env, out, aim);
            return;
        } else if c.brawl.cd <= 0.0 && (c.brawl.open > 0.0 || c.brawl.ai_t <= 0.0) {
            inp.press1 = true;
        }
    } else if dist < 28.0 && c.brawl.blaster_cd <= 0.0 && (c.has(Mechanic::Shoots) || c.recipe.payload.is_some() || a.blaster_explode) {
        inp.press2 = true;
    }
    robot(c, env, out, &inp, grounded);
    if c.brawl.dodge_t > 0.0 {
        c.body.claim(Driver::Charge);
    }
}
