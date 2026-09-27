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
use bd_core::mechanics::{Element, Feature, Form, Mechanic, Recipe};
use bd_core::voxel::VoxelQuery;
use bd_core::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::ai::{AttackKind, phase as ai_phase};
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
    /// BEASTS: a 360° tail sweep around the body (knocks everyone round it away).
    TailSweep,
    /// BEASTS: leap onto the aim; landing on someone pins them (a short stun) and knocks them down.
    Pounce,
    /// A size-scaled ground stomp: a ring shockwave + crater (combiners, dinos).
    Stomp,
}

/// What RMB does (robot or beast mode).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RangedKind {
    /// The blaster: bolts (shells when `blaster_explode`, an exploding payload or Explodes).
    #[default]
    Bolt,
    /// Held: a cone of the construct's element (fire breath) while RMB is held.
    Breath,
    /// A roar: a short cone shockwave that staggers (never a lock).
    Roar,
}

/// The body the light string hits with (presentation + creature clips for beast modes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrikeKind {
    #[default]
    Punch,
    Slash,
    /// Jaws: plays the creature Bite clip.
    Bite,
    /// Claws: plays the creature Swipe clip.
    Claw,
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
    /// BEASTS: the beast leaps in an arc onto the target (a pin + knock-down on landing).
    Pounce,
    /// DINOS: the beast charges jaws-first through everything, scooping the first victim.
    Stampede,
    /// COMBINERS (and anything too big to burst out as a vehicle): both fists into the ground —
    /// a huge ring shockwave around the body.
    Quake,
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
    /// Any of these elements (empty = any element).
    pub elements: Vec<Element>,
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
            && (self.elements.is_empty() || self.elements.contains(&r.element))
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
    /// Presentation flavour: "tyre_smoke", "exhaust", "afterburner", "rotor_wash", "tread_dust",
    /// "dino", "beast", "wake", "freight".
    pub fx: String,
    // ---- beasts / flavour (FEEL pass)
    /// BEAST MODES: the row the ALT mode fights with when the alt mode walks (a T-rex, a gorilla):
    /// "" = the alt mode doesn't brawl (vehicles drive and ram).
    pub alt: String,
    /// What RMB does.
    pub ranged: RangedKind,
    /// The body the string hits with.
    pub strike: StrikeKind,
    /// Tail sweep / stomp / quake radius past the body (m; 0 = scaled from the body).
    pub heavy_radius: f32,
    /// Ram / stampede: the first victim is scooped onto the nose (≤ `CARRY_SECS`), then flung.
    pub xattack_carry: bool,
    /// V2 HERO SCALE: this row is big by nature (a combiner, a colossus): a piloted one keeps its
    /// size instead of standing at hero scale.
    pub giant: bool,
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
            alt: String::new(),
            ranged: RangedKind::Bolt,
            strike: StrikeKind::Punch,
            heavy_radius: 0.0,
            xattack_carry: false,
            giant: false,
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
    /// V2 FLASH WINDOW: a string's ender or a launcher that CONNECTS opens it (s). Transform inside
    /// it cancels the melee recovery into the vehicle attack; anywhere else Transform just transforms.
    pub flash_secs: f32,
    /// V2 BRANCH: once a vehicle attack connects, this long to pick the follow-up (s): LMB = rush
    /// (a second pass), RMB / held LMB = the air launch, Transform = drift back into robot melee.
    pub branch_secs: f32,
    /// V2: an LMB held this long inside the branch window reads as the heavy (the air launch; s).
    pub branch_hold: f32,
    /// V2: vehicle-attack hits launch: at least this much lift (dir.y) and this knockback (m/s).
    pub vehicle_lift: f32,
    pub vehicle_min_kb: f32,
    /// V2: the connect that opens the branch pops them instead: × the row's knockback, and its lift.
    pub vehicle_pop_kb: f32,
    pub vehicle_pop_lift: f32,
    /// V2 launch branch: the juggle launcher's knockback (× the row's heavy) and the robot's rise
    /// after them (m/s) — matched so the air string connects.
    pub juggle_kb: f32,
    pub juggle_rise: f32,
    /// V2 HERO SCALE: a PILOTED combat transformer's recipe scale (≈ its robot height, m) is capped
    /// here so it fights 1.8 m players at readable range; recipes at or past `giant_min_scale` (said
    /// big: "giant optimus", combiners) and `giant` rows keep their size. 0 = off.
    pub hero_max_scale: f32,
    pub giant_min_scale: f32,
    /// AI followers: seconds into the flash window before they take it (a reaction, not a frame-1 read).
    pub ai_flash_delay: f32,
    /// V2 STAGE 3 INPUT BUFFER: an LMB / RMB press during recovery (string `cd` / blaster `cd`) is
    /// remembered this long (s) and fires the instant it's legal instead of being dropped.
    pub buffer_secs: f32,
    /// V2 STAGE 3 EARLY-TRANSFORM BUFFER: a Transform press up to this long (s) before a pending
    /// string ender / Launcher heavy connects still counts once the flash window opens (see
    /// `brawl::ender_pending`), instead of firing an immediate plain transform.
    pub early_xform_secs: f32,
    /// V2 STAGE 3 DODGE OFFSET: a boost-step dodge started mid-string tops the combo window up to at
    /// least this long (s), so the dash's own time doesn't drop the string (the next hit continues it).
    pub dodge_grace_secs: f32,
    /// TRANSFORMERS V3: mobility parity (the pilot's kit in robot mode), chase tools, vehicle weapons.
    pub v3: super::kit::V3Tuning,
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
            flash_secs: 0.3,
            branch_secs: 0.45,
            branch_hold: 0.15,
            vehicle_lift: 0.35,
            vehicle_min_kb: 22.0,
            vehicle_pop_kb: 0.35,
            vehicle_pop_lift: 1.0,
            juggle_kb: 0.45,
            juggle_rise: 14.0,
            hero_max_scale: 4.6,
            giant_min_scale: 7.0,
            ai_flash_delay: 0.08,
            buffer_secs: 0.12,
            early_xform_secs: 0.1,
            dodge_grace_secs: 0.5,
            v3: super::kit::V3Tuning::default(),
            rules: vec![
                rule("helicopter", |r| r.features_any = vec![F::Rotor]),
                rule("jet", |r| r.features_any = vec![F::Jet]),
                rule("jet", |r| r.mechanics_all = vec![M::Flies, M::Thrust]),
                // Beast alts with a tail or a breath (a T-rex, a dragon): bite / tail / fire.
                rule("dino", |r| {
                    r.forms = vec![Form::Kaiju, Form::Quadruped, Form::Serpent];
                    r.mechanics_any = vec![M::ArticulatedTail, M::Breath];
                }),
                // Kaiju-sized construction robots (a combiner) and planet-sized robots.
                rule("colossus", |r| {
                    r.forms = vec![Form::Kaiju];
                    r.features_any = vec![F::CraneArm, F::Plow, F::Drill, F::Saw];
                }),
                rule("colossus", |r| {
                    r.forms = vec![Form::Kaiju];
                    r.min_scale = 14.0;
                }),
                rule("train", |r| {
                    r.features_all = vec![F::Plow];
                    r.features_any = vec![F::Wheels, F::Cab, F::Smokestack];
                    r.features_none = vec![F::CraneArm, F::Drill, F::Saw, F::Treads];
                }),
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
                rule("boat", |r| r.features_any = vec![F::Sails]),
                rule("boat", |r| {
                    r.features_any = vec![F::Propeller];
                    r.elements = vec![Element::Water];
                }),
                // Animal alts that walk (a gorilla, a panther, a rhino, a scorpion): claws + pounce.
                rule("beast", |r| r.forms = vec![Form::Quadruped, Form::Serpent, Form::Arachnid, Form::Insect, Form::Fish]),
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
                // DINO (robot mode): a heavy sword brawler; the transform attack is the beast
                // stampeding jaws-first through them, then the robot's overhead chop.
                arch("dino", |a| {
                    a.robot_speed = 0.8;
                    a.air_control = 0.3;
                    a.dash = DashKind::Heavy;
                    a.dash_speed = 15.0;
                    a.dash_secs = 0.22;
                    a.string_hits = 4;
                    a.string_interval = 0.3;
                    a.string_dmg = 8.5;
                    a.string_kb = 13.0;
                    a.string_reach = 2.4;
                    a.strike = StrikeKind::Slash;
                    a.heavy = HeavyKind::Armor;
                    a.heavy_hold = 0.45;
                    a.heavy_dmg = 17.0;
                    a.heavy_kb = 28.0;
                    a.heavy_armor = 0.4;
                    a.xattack = XAttack::Stampede;
                    a.xattack_secs = 0.7;
                    a.xattack_speed = 20.0;
                    a.xattack_dmg = 14.0;
                    a.xattack_kb = 30.0;
                    a.xattack_width = 2.6;
                    a.xattack_carry = true;
                    a.finisher = Finisher::Axe;
                    a.finisher_dmg = 20.0;
                    a.finisher_kb = 32.0;
                    a.drift_boost = 0.0;
                    a.alt = "dino_beast".into();
                    a.fx = "dino".into();
                }),
                // DINO (beast mode): bites that drive in, a tail sweep, fire breath on RMB.
                arch("dino_beast", |a| {
                    a.robot_speed = 1.0;
                    a.air_control = 0.2;
                    a.dash = DashKind::Heavy;
                    a.dash_speed = 16.0;
                    a.dash_secs = 0.25;
                    a.string_hits = 3;
                    a.string_interval = 0.36;
                    a.string_dmg = 10.0;
                    a.string_kb = 12.0;
                    a.string_reach = 3.0;
                    a.strike = StrikeKind::Bite;
                    a.heavy = HeavyKind::TailSweep;
                    a.heavy_hold = 0.35;
                    a.heavy_dmg = 12.0;
                    a.heavy_kb = 26.0;
                    a.ranged = RangedKind::Breath;
                    a.blaster_dmg = 3.0;
                    a.blaster_cd = 0.12;
                    a.drift_boost = 0.0;
                    a.fx = "dino".into();
                }),
                // BEAST (robot mode): quick hands; the transform attack is the beast pouncing on them.
                arch("beast", |a| {
                    a.robot_speed = 1.05;
                    a.air_control = 0.6;
                    a.dash_speed = 22.0;
                    a.string_interval = 0.22;
                    a.string_dmg = 6.0;
                    a.string_kb = 10.0;
                    a.string_reach = 1.8;
                    a.heavy_hold = 0.33;
                    a.heavy_dmg = 12.0;
                    a.heavy_kb = 23.0;
                    a.xattack = XAttack::Pounce;
                    a.xattack_secs = 0.55;
                    a.xattack_speed = 22.0;
                    a.xattack_dmg = 13.0;
                    a.xattack_kb = 18.0;
                    a.xattack_width = 2.2;
                    a.finisher_dmg = 15.0;
                    a.finisher_kb = 27.0;
                    a.drift_boost = 0.0;
                    a.alt = "beast_alt".into();
                    a.fx = "beast".into();
                }),
                // BEAST (beast mode): fast claws, a pounce on hold, a roar on RMB.
                arch("beast_alt", |a| {
                    a.robot_speed = 1.3;
                    a.air_control = 0.7;
                    a.dash_speed = 24.0;
                    a.dash_secs = 0.15;
                    a.string_interval = 0.18;
                    a.string_dmg = 5.0;
                    a.string_kb = 8.0;
                    a.string_reach = 1.8;
                    a.strike = StrikeKind::Claw;
                    a.heavy = HeavyKind::Pounce;
                    a.heavy_hold = 0.3;
                    a.heavy_dmg = 12.0;
                    a.heavy_kb = 20.0;
                    a.ranged = RangedKind::Roar;
                    a.blaster_cd = 2.5;
                    a.blaster_dmg = 4.0;
                    a.drift_boost = 0.0;
                    a.fx = "beast".into();
                }),
                // BOAT: a wake-surge ram that throws a wave, then an anchor chop; slides on turns.
                arch("boat", |a| {
                    a.dash_speed = 20.0;
                    a.string_interval = 0.24;
                    a.string_dmg = 6.0;
                    a.string_kb = 10.0;
                    a.string_reach = 1.8;
                    a.xattack = XAttack::Ram;
                    a.xattack_secs = 0.5;
                    a.xattack_speed = 24.0;
                    a.xattack_dmg = 12.0;
                    a.xattack_kb = 26.0;
                    a.xattack_width = 3.2;
                    a.finisher = Finisher::Axe;
                    a.finisher_dmg = 16.0;
                    a.finisher_kb = 28.0;
                    a.vehicle_grip = 0.7;
                    a.drift_boost = 9.0;
                    a.fx = "wake".into();
                }),
                // TRAIN: the longest, straightest ram (the first victim rides the plough) and a
                // coupling-hook launcher; barely turns, never drifts.
                arch("train", |a| {
                    a.robot_speed = 0.85;
                    a.air_control = 0.35;
                    a.dash = DashKind::Heavy;
                    a.dash_speed = 16.0;
                    a.dash_secs = 0.22;
                    a.string_interval = 0.3;
                    a.string_dmg = 8.0;
                    a.string_kb = 13.0;
                    a.string_reach = 2.0;
                    a.heavy = HeavyKind::Armor;
                    a.heavy_hold = 0.45;
                    a.heavy_dmg = 16.0;
                    a.heavy_kb = 26.0;
                    a.heavy_armor = 0.4;
                    a.blaster_dmg = 9.0;
                    a.blaster_cd = 0.6;
                    a.xattack = XAttack::Ram;
                    a.xattack_secs = 0.8;
                    a.xattack_speed = 28.0;
                    a.xattack_dmg = 16.0;
                    a.xattack_kb = 34.0;
                    a.xattack_width = 2.4;
                    a.xattack_carry = true;
                    a.finisher_dmg = 18.0;
                    a.finisher_kb = 30.0;
                    a.vehicle_speed = 1.15;
                    a.drift_boost = 0.0;
                    a.vehicle_turn = 0.55;
                    a.fx = "freight".into();
                }),
                // COLOSSUS (a combiner, a planet-sized robot): slow, enormous, stomps; its transform
                // attack is a two-fisted quake around the body, then the overhead smash.
                arch("colossus", |a| {
                    a.robot_speed = 0.7;
                    a.air_control = 0.15;
                    a.dash = DashKind::Planted;
                    a.dash_speed = 12.0;
                    a.dash_secs = 0.16;
                    a.string_hits = 3;
                    a.string_interval = 0.42;
                    a.string_dmg = 12.0;
                    a.string_kb = 18.0;
                    a.string_reach = 3.5;
                    a.heavy = HeavyKind::Stomp;
                    a.heavy_hold = 0.4;
                    a.heavy_dmg = 15.0;
                    a.heavy_kb = 26.0;
                    a.heavy_armor = 0.3;
                    a.blaster_dmg = 14.0;
                    a.blaster_cd = 1.0;
                    a.blaster_speed = 60.0;
                    a.blaster_explode = true;
                    a.xattack = XAttack::Quake;
                    a.xattack_secs = 0.5;
                    a.xattack_speed = 0.0;
                    a.xattack_dmg = 18.0;
                    a.xattack_kb = 32.0;
                    a.xattack_width = 8.0;
                    a.finisher = Finisher::Axe;
                    a.finisher_dmg = 24.0;
                    a.finisher_kb = 36.0;
                    a.vehicle_speed = 0.8;
                    a.drift_boost = 0.0;
                    a.vehicle_turn = 0.7;
                    a.fx = "tread_dust".into();
                    a.giant = true;
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
        self.heavy_radius = fin(self.heavy_radius, 0.0, 0.0, 30.0);
        self.name.truncate(24);
        self.fx.truncate(24);
        self.alt.truncate(24);
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
        self.flash_secs = fin(self.flash_secs, d.flash_secs, 0.05, 1.5);
        self.branch_secs = fin(self.branch_secs, d.branch_secs, 0.05, 2.0);
        self.branch_hold = fin(self.branch_hold, d.branch_hold, 0.02, 1.0);
        self.vehicle_lift = fin(self.vehicle_lift, d.vehicle_lift, 0.0, 3.0);
        self.vehicle_min_kb = fin(self.vehicle_min_kb, d.vehicle_min_kb, 0.0, 100.0);
        self.vehicle_pop_kb = fin(self.vehicle_pop_kb, d.vehicle_pop_kb, 0.0, 2.0);
        self.vehicle_pop_lift = fin(self.vehicle_pop_lift, d.vehicle_pop_lift, 0.0, 3.0);
        self.juggle_kb = fin(self.juggle_kb, d.juggle_kb, 0.0, 3.0);
        self.juggle_rise = fin(self.juggle_rise, d.juggle_rise, 0.0, 40.0);
        self.hero_max_scale = fin(self.hero_max_scale, d.hero_max_scale, 0.0, 30.0);
        self.giant_min_scale = fin(self.giant_min_scale, d.giant_min_scale, 0.0, 100.0);
        self.ai_flash_delay = fin(self.ai_flash_delay, d.ai_flash_delay, 0.0, 1.0);
        self.buffer_secs = fin(self.buffer_secs, d.buffer_secs, 0.0, 0.5);
        self.early_xform_secs = fin(self.early_xform_secs, d.early_xform_secs, 0.0, 0.5);
        self.dodge_grace_secs = fin(self.dodge_grace_secs, d.dodge_grace_secs, 0.0, 2.0);
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
        name.and_then(|n| self.named(n)).or_else(|| self.archetypes.first()).unwrap_or_else(|| fallback())
    }

    /// The row called `name`.
    pub fn named(&self, name: &str) -> Option<&Archetype> {
        if name.is_empty() { None } else { self.archetypes.iter().find(|a| a.name == name) }
    }

    /// The row a construct fights with in ROBOT mode: its recipe's archetype; a combined giant
    /// (a combiner) fights as the "colossus" row when there is one.
    pub fn robot_row(&self, c: &Construct) -> &Archetype {
        if c.comp.xform.combined {
            if let Some(a) = self.named("colossus") {
                return a;
            }
        }
        self.archetype_of(&c.recipe)
    }

    /// BEAST MODES: the row the ALT mode fights with (a T-rex bites, a gorilla claws), if any.
    pub fn alt_row(&self, c: &Construct) -> Option<&Archetype> {
        self.named(&self.robot_row(c).alt)
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

/// V2 HERO SCALE (owner: "transformers are just a bigger guy"): a combat transformer you PILOT stands
/// at hero scale (≤ `hero_max_scale`, ~3.5–5 m) so its fights with 1.8 m players read and connect.
/// Generic by data: a recipe said big (scale ≥ `giant_min_scale`: "giant optimus", "huge", a
/// combiner's own size), a `giant` archetype row (colossus), a squad or a suit keep their size.
/// Mass is kept (the archetype and the weight of its hits stay). `robot_ratio` = the robot's height
/// per metre of recipe scale (1 for a generic transformer; a two-mode icon's authored robot can stand
/// taller than its alt mode is long): the ROBOT stands ≤ `hero_max_scale`. True when it shrank it.
pub fn hero_scale(t: &TransformerTuning, r: &mut Recipe, robot_ratio: f32) -> bool {
    let cap = t.hero_max_scale / robot_ratio.clamp(0.5, 3.0);
    if cap <= 0.0 || !r.has(Mechanic::Transforms) || !r.has(Mechanic::Piloted) || r.count > 1 || r.scale <= cap || r.scale >= t.giant_min_scale || t.archetype_of(r).giant {
        return false;
    }
    r.scale = cap;
    true
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

/// A voice order a seated pilot said mid-fight ([`order`]; FEEL pass): "optimus prime combo",
/// "ion blaster", "energon axe", "charge"... mapped by the move's tags / ops onto the robot's own
/// moveset (never a second body of rules).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Order {
    #[default]
    None,
    /// The whole combo on its own: the string, the transform attack, the finisher.
    Combo,
    /// The transform attack + finisher right now (no string needed).
    Super,
    /// The heavy (launcher / armored slam / tail sweep / pounce / stomp).
    Heavy,
    /// The blaster / breath / roar.
    Ranged,
    /// A boost-step toward the aim.
    Dash,
    /// Guard: a boost-step in place whose perfect window stays open longer (a counter stance).
    Guard,
}

/// A pounce's longest flight (s).
pub const POUNCE_SECS: f32 = 1.0;
/// Seconds a scooped victim rides the nose of a ram / stampede before the fling.
pub const CARRY_SECS: f32 = 0.45;
/// A guard order keeps the perfect-dodge window open this long (s).
pub const GUARD_SECS: f32 = 0.5;

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
    /// Breath held (s; beast modes) — its cone ticks on `blaster_cd`.
    pub breath_t: f32,
    /// Drifting (vehicle mode, s held).
    pub drift_t: f32,
    /// Super armor left (s) while a heavy winds up.
    pub armor_t: f32,
    /// BEASTS: a pounce in the air (s left); it hits what it lands on.
    pub pounce_t: f32,
    /// Transform attack.
    pub xatk: XPhase,
    pub xatk_t: f32,
    pub xatk_dir: Vec3,
    pub xatk_tick: f32,
    pub xatk_n: u8,
    /// Ram / stampede: the victim riding the nose and for how long (s).
    pub carry: Option<bd_core::PlayerId>,
    pub carry_t: f32,
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
    /// VOICE: the order being carried out ([`Order`]) and its script clock (s) / step.
    pub order: Order,
    pub order_t: f32,
    pub order_n: u8,
    /// Voice orders carried out (tests / tools).
    pub orders: u16,
    /// V2 FLASH WINDOW left (s): Transform now = the vehicle attack (see `TransformerTuning::flash_secs`).
    pub flash: f32,
    /// V2 BRANCH window left (s) after the vehicle attack connected, whether this attack already
    /// opened / used it, the LMB hold inside it, and the last branch taken (tests / tools).
    pub branch: f32,
    pub branch_opened: bool,
    pub branch_hold: f32,
    pub last_branch: Branch,
    /// Flash windows opened / taken, branches taken (tests / tools / the V2 measurements).
    pub flashes: u16,
    pub flashes_taken: u16,
    pub branches: u16,
    /// V2 STAGE 3 INPUT BUFFER: the latest LMB / RMB press remembered this long (s) so a recovery
    /// frame doesn't drop it (`TransformerTuning::buffer_secs`; fires the moment it's legal).
    pub press1_buf: f32,
    pub press2_buf: f32,
    /// V2 STAGE 3 EARLY-TRANSFORM BUFFER (`pilot.rs`): a Transform press held this long (s) while a
    /// string ender / Launcher heavy is about to connect (`TransformerTuning::early_xform_secs`),
    /// instead of firing an immediate plain transform.
    pub xform_buf: f32,
}

/// V2: the follow-up picked inside the branch window after a vehicle attack connects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Branch {
    #[default]
    None,
    /// LMB: the vehicle comes round for a second pass through them.
    Rush,
    /// RMB / held LMB: back to robot with a launcher that takes you up after them (air juggle).
    Launch,
    /// Transform: a drift-turn back into robot mode facing them, the string open again.
    Drift,
}

/// Is the brawl machine moving the body this tick (the AI's locomotion must not override it)?
pub fn moving(c: &Construct) -> bool {
    c.brawl.xatk != XPhase::None || c.brawl.dodge_t > 0.0 || c.brawl.approach || c.brawl.pounce_t > 0.0
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
    b.flash = (b.flash - dt).max(0.0);
    b.branch = (b.branch - dt).max(0.0);
    b.press1_buf = (b.press1_buf - dt).max(0.0);
    b.press2_buf = (b.press2_buf - dt).max(0.0);
}

/// V2 FLASH WINDOW: open it (a string's ender / a launcher / a counter just CONNECTED). Everyone sees
/// the gold burst on the robot (`HeldFx::Flash`, strength = the window, s); the pilot also hears the
/// chime and sees the crosshair pip (client).
fn open_flash(c: &mut Construct, t: &TransformerTuning, out: &mut ConstructOut) {
    if c.held.vehicle || !c.has(Mechanic::Transforms) {
        return;
    }
    c.brawl.flash = t.flash_secs;
    c.brawl.flashes = c.brawl.flashes.saturating_add(1);
    let at = c.center() + Vec3::Y * c.half.y * 0.55;
    fx(out, c, HeldFx::Flash, at, c.forward(), t.flash_secs);
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

/// SUITS: the wearer's own dodge let a hit pass through: that is a perfect dodge for a transformer
/// suit too (the counter opens, the chrome flash plays).
pub fn open_counter(c: &mut Construct, counter_secs: f32, point: Vec3, events: &mut Vec<GameEvent>) {
    if c.gone.is_some() || !c.has(Mechanic::Transforms) {
        return;
    }
    let fresh = c.brawl.counter <= 0.0;
    c.brawl.counter = counter_secs;
    if fresh {
        let owner = c.pilot.unwrap_or(c.owner);
        events.push(GameEvent::HeldFx { id: c.id, owner, fx: HeldFx::Parry, pos: if point.is_finite() { point } else { c.center() }, dir: Vec3::Y, strength: 2.0 });
    }
}

/// A fast combat transform to robot (a voice order said in vehicle mode carries on as the robot).
pub fn snap_robot(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    if c.held.vehicle {
        snap(c, true, &env.ctx.tuning.transformer, out);
    }
}

/// Knockback / damage multiplier while a heavy's super armor is up (1 = none).
pub fn armor(c: &Construct, t: &TransformerTuning) -> f32 {
    if c.brawl.armor_t > 0.0 { t.robot_row(c).heavy_armor.min(t.alt_row(c).map_or(1.0, |a| a.heavy_armor)) } else { 1.0 }
}

// ------------------------------------------------------------------------------------ input

/// What the robot is asked to do this tick (a pilot's buttons, or the AI's decisions).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub press1: bool,
    pub hold1: bool,
    pub press2: bool,
    /// RMB held (breath).
    pub hold2: bool,
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
            hold2: !typing && inp.held(buttons::SECONDARY),
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

/// Body height (m).
fn height(c: &Construct) -> f32 {
    (c.half.y * 2.0).max(0.5)
}

/// Reach grows gently with the body: a 10 m dino's bite reaches further than a 4 m car's jab.
fn reach_k(c: &Construct) -> f32 {
    (height(c) / 5.0).sqrt().clamp(0.8, 2.2)
}

/// Non-ender string hits keep this fraction of the row's knockback (a flinch that keeps them close).
pub const STRING_FLINCH: f32 = 0.35;
/// String magnetism: how far past its reach a string hit slides the robot to its aimed target (m).
pub const MAGNET_RANGE: f32 = 4.0;

/// The enemy the pilot / AI is aiming at: the nearest within `max` m (hull) inside ~55° of `aim`.
fn aim_target(c: &Construct, env: &Env<'_>, aim: Vec3, max: f32) -> Option<Vec3> {
    let here = c.center();
    let fa = flat(aim).normalize_or(flat(c.forward()).normalize_or(Vec3::NEG_Z));
    let body = c.half.x.max(c.half.z);
    let mut best: Option<(f32, Vec3)> = None;
    for t in env.ctx.enemies_of(c.owner) {
        if Some(t.id) == c.pilot || !t.alive {
            continue;
        }
        let p = t.hull.center();
        let d = flat(p - here);
        let dist = d.length() - body;
        if dist > max || (p.y - here.y).abs() > c.half.y + 4.0 {
            continue;
        }
        if d.length() > 0.5 && d.normalize_or_zero().dot(fa) < 0.57 {
            continue;
        }
        if best.is_none_or(|(bd, _)| dist < bd) {
            best = Some((dist, p));
        }
    }
    best.map(|b| b.1)
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

/// Hit weight: light string hits are `Melee`; finishers, counters, heavies, slams are `HeavyMelee`
/// (clients: the heavy-weapon impact frame on the connect, the heavy crunch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum W {
    Light,
    Heavy,
    /// V2: a vehicle-attack hit — heavy (the impact frame) and GUARD-BREAKING (beats guard / counter /
    /// deflect stances and super armor).
    Vehicle,
    /// V2: a vehicle attack's ticking hits (rotor sweep, grind): guard-breaking, light juice.
    VehicleTick,
}

#[allow(clippy::too_many_arguments)]
fn hit(c: &Construct, out: &mut ConstructOut, target: bd_core::PlayerId, at: Vec3, dmg: f32, kb: f32, dir: Vec3, impact: f32, stun: f32, w: W) {
    let kind = if matches!(w, W::Heavy | W::Vehicle) { HitKind::HeavyMelee } else { HitKind::Melee };
    let mut h = HitInfo::new(target, Some(c.owner), kind, dmg, kb, dir.normalize_or(Vec3::Y), at).with_element(c.recipe.element).with_construct(c.id).with_impact(impact);
    if matches!(w, W::Vehicle | W::VehicleTick) {
        h = h.with_guard_break();
    }
    out.player_hits.push(h);
    if stun > 0.0 {
        out.statuses.push((target, StatusKind::Stun, stun, Some(c.owner)));
    }
}

/// V2: a vehicle-attack hit: guard-breaking, with a short stun. The connect that opens the branch
/// POPS them (up and close: × `vehicle_pop_kb`, `vehicle_pop_lift`) so the follow-up you pick
/// connects; the attack's later hits (the rush's second pass) LAUNCH (≥ `vehicle_lift` up,
/// ≥ `vehicle_min_kb`) — a launch, or a wall splat.
#[allow(clippy::too_many_arguments)]
fn vhit(c: &Construct, t: &TransformerTuning, out: &mut ConstructOut, target: bd_core::PlayerId, at: Vec3, dmg: f32, kb: f32, dir: Vec3, impact: f32) {
    let f = flat(dir).normalize_or(flat(c.forward()).normalize_or(Vec3::NEG_Z));
    let (lift, kb) = if c.brawl.branch_opened { (dir.y.max(t.vehicle_lift), kb.max(t.vehicle_min_kb)) } else { (dir.y.max(t.vehicle_pop_lift), kb * t.vehicle_pop_kb) };
    hit(c, out, target, at, dmg, kb, f + Vec3::Y * lift, impact.max(2.0), t.launch_stun, W::Vehicle);
}

/// A creature clip event for beast-mode attacks (clients play the bite / swipe / tail / breath /
/// stomp clip on the beast's body).
fn clip(out: &mut ConstructOut, c: &Construct, kind: AttackKind, ph: u8, aim: Vec3, impact: f32) {
    out.events.push(GameEvent::CreatureAttack { id: c.id, kind: kind as u8, phase: ph, aim, impact });
}

/// A melee strike in front of the robot along `dir`. Returns how many it hit.
#[allow(clippy::too_many_arguments)]
fn strike(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, dir: Vec3, reach: f32, dmg: f32, kb: f32, kb_dir: Vec3, impact: f32, stun: f32, fxk: HeldFx, w: W) -> u8 {
    let d = flat(dir).normalize_or(flat(c.forward()).normalize_or(Vec3::NEG_Z));
    let body = c.half.x.max(c.half.z);
    let center = c.center() + d * (body * 0.6 + reach * 0.5) - Vec3::Y * c.half.y * 0.2;
    let r = reach * 0.6 + c.half.y * 0.35;
    let mut n = 0u8;
    for (id, at) in in_reach(c, env, center, r) {
        hit(c, out, id, at, dmg, kb, kb_dir, impact, stun, w);
        n = n.saturating_add(1);
    }
    c.rot = geom::yaw_rotation(d);
    fx(out, c, fxk, center, d, impact);
    c.set_anim(AnimState::Active, env.tick());
    n
}

/// A 360° strike around the robot.
#[allow(clippy::too_many_arguments)]
fn sweep(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, r: f32, dmg: f32, kb: f32, up: f32, impact: f32, once: bool, w: W) -> u8 {
    let center = c.center();
    let mut n = 0u8;
    for (id, at) in in_reach(c, env, center, r + c.half.x.max(c.half.z)) {
        if once && !c.hits.insert(player_key(id)) {
            continue;
        }
        let dir = flat(at - center).normalize_or(c.forward()) + Vec3::Y * up;
        hit(c, out, id, at, dmg, kb, dir, impact, 0.0, w);
        n = n.saturating_add(1);
    }
    n
}

/// A size-scaled ground shockwave at the feet (stomps, quakes, big landings): a ring that knocks
/// grounded players up and out, and a crater. Returns how many it hit.
#[allow(clippy::too_many_arguments)]
fn quake(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, at: Vec3, radius: f32, dmg: f32, kb: f32, fxk: HeldFx, w: W) -> u8 {
    let mut n = 0u8;
    for (id, hp) in in_reach(c, env, at, radius) {
        let d = flat(hp - at);
        let fall = 1.0 - 0.5 * (d.length() / radius.max(0.1)).min(1.0);
        hit(c, out, id, hp, dmg * fall, kb * fall, d.normalize_or(c.forward()) + Vec3::Y * 1.1, 2.2, 0.0, w);
        n = n.saturating_add(1);
    }
    out.carves.push(crate::combat::CarveRequest { center: at, radius: (height(c) * 0.18).clamp(1.2, 4.0), power: 0.8 });
    fx(out, c, fxk, at, Vec3::NEG_Y, (radius / 3.0).clamp(1.0, 4.0));
    n
}

fn feet(c: &Construct) -> Vec3 {
    c.pos + c.rot * c.local_center - Vec3::Y * c.half.y
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

/// The jaw / head point of a beast (front, upper body).
fn mouth(c: &Construct, aim: Vec3) -> Vec3 {
    c.center() + Vec3::Y * c.half.y * 0.45 + flat(aim).normalize_or(c.forward()) * (c.half.x.max(c.half.z) * 0.9)
}

// ------------------------------------------------------------------------------------ robot mode

/// Robot-mode combat for one tick (pilot or AI). Movement overrides (boost-step, launcher hop, air
/// float) are written into `c.vel`; the caller integrates. `grounded` = the body's ground contact.
pub fn robot(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, inp: &Input, grounded: bool) {
    let a = env.ctx.tuning.transformer.robot_row(c).clone();
    fight(c, env, out, inp, grounded, &a);
}

/// BEAST MODES: the alt mode fights (a T-rex bites, sweeps its tail and breathes fire; a gorilla
/// claws, pounces and roars) with the robot row's `alt` row. Returns false when the alt mode doesn't
/// brawl (vehicles drive and ram instead).
pub fn beast(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, inp: &Input, grounded: bool) -> bool {
    let Some(a) = env.ctx.tuning.transformer.alt_row(c).cloned() else { return false };
    fight(c, env, out, inp, grounded, &a);
    true
}

/// Does `c`'s alt mode fight as a beast (its robot row names an `alt` row)?
pub fn beast_alt(c: &Construct, t: &TransformerTuning) -> bool {
    t.alt_row(c).is_some()
}

/// One tick of a moveset (robot or beast row `a`).
pub fn fight(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, inp: &Input, grounded: bool, a: &Archetype) {
    let t = &env.ctx.tuning.transformer;
    let dt = env.dt();
    tick_timers(&mut c.brawl, dt);
    let (dm, km) = scale_k(c);
    if grounded {
        c.brawl.air_hits = 0;
    }
    // A pounce in the air: it lands on whoever is under it.
    if c.brawl.pounce_t > 0.0 {
        step_pounce(c, env, out, a, grounded, dm, km, a.heavy_dmg, a.heavy_kb);
        return;
    }
    // Boost-step (perfect-dodge window at its start).
    if inp.dodge && c.brawl.dodge_cd <= 0.0 && c.brawl.dodge_t <= 0.0 {
        let back = -flat(inp.aim).normalize_or(c.forward());
        c.brawl.dodge_dir = if inp.wish != Vec3::ZERO { inp.wish } else { back };
        c.brawl.dodge_t = a.dash_secs;
        c.brawl.dodge_cd = t.dodge_cd;
        c.brawl.iframes = t.perfect_window;
        c.brawl.perfect_fx = false;
        // V2 STAGE 3 DODGE OFFSET: mid-string, a boost-step keeps the string's position — top the
        // combo window up to `dodge_grace_secs` instead of letting the dash's own time eat into it.
        if c.brawl.step > 0 && c.brawl.open > 0.0 {
            c.brawl.open = c.brawl.open.max(t.dodge_grace_secs);
        }
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
        let n = strike(c, env, out, dir, a.string_reach * 1.4 * reach_k(c), a.heavy_dmg * t.counter_mult * dm, a.heavy_kb * t.counter_mult * km, kb_dir, 2.4, t.counter_stun, HeldFx::Uppercut, W::Heavy);
        c.brawl.open = t.combo_window;
        c.brawl.cd = a.string_interval;
        if n > 0 {
            open_flash(c, t, out); // V2: a counter is a launcher
        }
        return;
    }
    // LMB: press = the next string hit; held past `heavy_hold` = the heavy.
    // V2 STAGE 3 INPUT BUFFER: a press remembered for `buffer_secs` fires the instant the recovery
    // (`cd`) ends instead of being dropped — a tap at the string's own rhythm never loses a hit.
    if inp.press1 {
        c.brawl.press1_buf = t.buffer_secs;
    }
    let buffered1 = c.brawl.press1_buf > 0.0 && c.brawl.cd <= 0.0;
    if inp.press1 || buffered1 {
        c.brawl.hold = 0.0;
        c.brawl.heavy_done = false;
        if c.brawl.cd <= 0.0 {
            c.brawl.press1_buf = 0.0;
            if grounded {
                string_hit(c, env, out, a, t, inp.aim, dm, km);
            } else {
                air_hit(c, env, out, a, t, inp.aim, dm, km);
            }
        }
    } else if inp.hold1 {
        c.brawl.hold += dt;
        if !c.brawl.heavy_done && c.brawl.hold >= a.heavy_hold * 0.6 && matches!(a.heavy, HeavyKind::Armor | HeavyKind::Stomp) {
            c.brawl.armor_t = c.brawl.armor_t.max(0.2); // super armor while it winds up
        }
        if !c.brawl.heavy_done && c.brawl.hold >= a.heavy_hold {
            heavy(c, env, out, a, t, inp.aim, dm, km, grounded);
            if a.heavy != HeavyKind::Grind {
                c.brawl.heavy_done = true;
            }
        }
    } else {
        c.brawl.hold = 0.0;
        c.brawl.heavy_done = false;
    }
    ranged(c, env, out, a, t, inp, dm, km);
}

/// RMB: the blaster, a held breath, or a roar.
fn ranged(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, t: &TransformerTuning, inp: &Input, dm: f32, km: f32) {
    match a.ranged {
        RangedKind::Bolt => {
            // V2 STAGE 3 INPUT BUFFER: same as LMB — a tap during the blaster's own cooldown fires
            // the instant it clears instead of being dropped.
            if inp.press2 {
                c.brawl.press2_buf = t.buffer_secs;
            }
            if (inp.press2 || c.brawl.press2_buf > 0.0) && c.brawl.blaster_cd <= 0.0 && a.blaster_shots > 0 {
                c.brawl.press2_buf = 0.0;
                let explode = a.blaster_explode || c.has(Mechanic::Explodes) || c.recipe.payload.as_ref().is_some_and(|p| p.has(Mechanic::Explodes) || p.form == Form::Bomb);
                let right = inp.aim.cross(Vec3::Y).normalize_or_zero();
                for k in 0..a.blaster_shots {
                    let spread = (k as f32 - (a.blaster_shots - 1) as f32 * 0.5) * 0.035;
                    shoot_bolt(c, out, muzzle(c, inp.aim), inp.aim + right * spread, a, a.blaster_dmg * dm, 9.0 * km, explode);
                }
                c.brawl.blaster_cd = a.blaster_cd;
                fx(out, c, HeldFx::PilotAttack, muzzle(c, inp.aim), inp.aim, 1.0);
                c.set_anim(AnimState::Active, env.tick());
            }
        }
        RangedKind::Breath => {
            let aim = inp.aim;
            if inp.hold2 || (inp.press2 && c.brawl.breath_t <= 0.0) {
                if c.brawl.breath_t <= 0.0 {
                    clip(out, c, AttackKind::Breath, ai_phase::ACTIVE, aim, 0.0);
                }
                c.brawl.breath_t += env.dt();
                c.rot = geom::yaw_rotation(flat(aim).normalize_or(c.forward()));
                if c.brawl.blaster_cd <= 0.0 {
                    c.brawl.blaster_cd = a.blaster_cd;
                    let head = mouth(c, aim);
                    let range = (5.0 + height(c) * 1.4).min(26.0);
                    for t in env.ctx.enemies_of(c.owner) {
                        if Some(t.id) == c.pilot {
                            continue;
                        }
                        let d = t.hull.center() - head;
                        let fd = flat(d);
                        let len = fd.length();
                        let fa = flat(aim).normalize_or(c.forward());
                        let band = d.y > -(height(c) + 2.0) && d.y < 2.0 + len * 0.3;
                        if len < range && len > 1e-3 && fd.dot(fa) / len > 0.8 && band {
                            hit(c, out, t.id, t.hull.center(), a.blaster_dmg * dm, 3.0 * km, fa + Vec3::Y * 0.2, 0.5, 0.0, W::Light);
                        }
                    }
                }
                c.set_anim(AnimState::Active, env.tick());
                // A breath runs out (3 s) and needs a breather.
                if c.brawl.breath_t > 3.0 {
                    c.brawl.breath_t = 0.0;
                    c.brawl.blaster_cd = 1.2;
                    clip(out, c, AttackKind::Breath, ai_phase::END, aim, 0.0);
                }
            } else if c.brawl.breath_t > 0.0 {
                c.brawl.breath_t = 0.0;
                clip(out, c, AttackKind::Breath, ai_phase::END, aim, 0.0);
            }
        }
        RangedKind::Roar => {
            if inp.press2 && c.brawl.blaster_cd <= 0.0 {
                c.brawl.blaster_cd = a.blaster_cd;
                let aim = flat(inp.aim).normalize_or(c.forward());
                let head = mouth(c, aim);
                let range = (6.0 + height(c) * 1.5).min(22.0);
                for t in env.ctx.enemies_of(c.owner) {
                    if Some(t.id) == c.pilot {
                        continue;
                    }
                    let d = t.hull.center() - head;
                    let len = d.length();
                    if len < range && len > 1e-3 && flat(d).normalize_or_zero().dot(aim) > 0.5 {
                        // A short stagger, never a lock.
                        hit(c, out, t.id, t.hull.center(), a.blaster_dmg * dm, 10.0 * km, aim + Vec3::Y * 0.3, 1.2, 0.25, W::Light);
                    }
                }
                c.rot = geom::yaw_rotation(aim);
                clip(out, c, AttackKind::Breath, ai_phase::TELEGRAPH, aim, 0.0);
                clip(out, c, AttackKind::Breath, ai_phase::END, aim, 0.0);
                fx(out, c, HeldFx::Roar, head, aim, (range / 6.0).clamp(1.0, 4.0));
                c.set_anim(AnimState::Active, env.tick());
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn string_hit(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, t: &TransformerTuning, aim: Vec3, dm: f32, km: f32) {
    let b = &mut c.brawl;
    b.step = if b.open > 0.0 { b.step + 1 } else { 1 };
    let ender = b.step >= a.string_hits;
    b.string_done = ender;
    // Non-ender hits FLINCH (a small knockback) so the string stays connected; the ender launches.
    let (mult, kmult, impact) = if ender { (1.6, 1.8, 1.6) } else { (1.0, STRING_FLINCH, 1.0 + 0.1 * b.step as f32) };
    let kb_dir = flat(aim).normalize_or(c.forward()) + Vec3::Y * if ender { 0.6 } else { 0.25 };
    let mut reach = a.string_reach * reach_k(c);
    // MAGNETISM (Devastation-style): the step in carries the robot to the aimed target so the next
    // hit reaches (a short slide, ≤ `MAGNET_RANGE` past the reach, never through them). V2: THIS
    // hit connects too — the strike reaches over the gap the slide is closing (a human-sized target
    // at your feet or a step away is hit, not whiffed).
    let aim = match aim_target(c, env, aim, reach + MAGNET_RANGE) {
        Some(tp) => {
            let d = flat(tp - c.center());
            let gap = d.length() - (c.half.x.max(c.half.z) + reach * 0.6);
            if gap > 0.0 {
                let v = d.normalize_or_zero() * (gap / 0.12).min(18.0);
                c.vel.x = v.x;
                c.vel.z = v.z;
                reach += gap.min(MAGNET_RANGE);
            }
            (tp - c.center()).normalize_or(aim)
        }
        None => aim,
    };
    let n = strike(c, env, out, aim, reach, a.string_dmg * mult * dm, a.string_kb * kmult * km, kb_dir, impact, 0.0, HeldFx::PilotAttack, if ender { W::Heavy } else { W::Light });
    if ender && n > 0 {
        open_flash(c, t, out); // V2: the string's end — Transform now = the vehicle attack
    }
    // Beast bodies bite / claw with their creature clips.
    let d = flat(aim).normalize_or(c.forward());
    match a.strike {
        StrikeKind::Bite => {
            clip(out, c, AttackKind::Bite, ai_phase::TELEGRAPH, d, 0.0);
            clip(out, c, AttackKind::Bite, ai_phase::IMPACT, d, if n > 0 { impact } else { 0.0 });
        }
        StrikeKind::Claw => {
            clip(out, c, AttackKind::Swipe, ai_phase::TELEGRAPH, d, 0.0);
            clip(out, c, AttackKind::Swipe, ai_phase::IMPACT, d, if n > 0 { impact } else { 0.0 });
        }
        _ => {}
    }
    // A short step into the hit (bites lunge further).
    let lunge = d * if a.strike == StrikeKind::Bite { 6.0 } else { 4.0 };
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
    strike(c, env, out, aim, a.string_reach * reach_k(c), a.string_dmg * dm, a.string_kb * 0.8 * km, kb_dir, 1.1, 0.0, HeldFx::PilotAttack, W::Light);
    c.vel.y = c.vel.y.max(t.air_float * (0.6 + 0.4 * a.air_control));
    c.brawl.cd = a.string_interval;
    c.brawl.open = t.combo_window;
}

#[allow(clippy::too_many_arguments)]
fn heavy(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, t: &TransformerTuning, aim: Vec3, dm: f32, km: f32, grounded: bool) {
    let d = flat(aim).normalize_or(c.forward());
    let reach = a.string_reach * reach_k(c);
    // V2: a launcher-class heavy that connects opens the flash window.
    let mut launched = 0u8;
    match a.heavy {
        HeavyKind::Launcher => {
            launched = strike(c, env, out, aim, reach * 1.2, a.heavy_dmg * dm, a.heavy_kb * km, d * 0.25 + Vec3::Y * 1.4, 2.0, t.launch_stun, HeldFx::Uppercut, W::Heavy);
            // Fliers follow their launcher up (air combos).
            if a.air_control >= 0.9 {
                c.vel.y = c.vel.y.max(10.0);
            }
        }
        HeavyKind::Armor => {
            c.brawl.armor_t = c.brawl.armor_t.max(0.3);
            launched = strike(c, env, out, aim, reach * 1.4, a.heavy_dmg * dm, a.heavy_kb * km, d + Vec3::Y * 0.5, 2.2, t.launch_stun, HeldFx::GroundPound, W::Heavy);
        }
        HeavyKind::Grind => {
            c.brawl.grind_t -= env.dt();
            if c.brawl.grind_t <= 0.0 {
                c.brawl.grind_t = 0.12;
                strike(c, env, out, aim, reach, a.heavy_dmg * dm, a.heavy_kb * km, d + Vec3::Y * 0.1, 0.6, 0.0, HeldFx::PilotAttack, W::Light);
            }
            if c.brawl.hold > a.heavy_hold + 1.2 {
                c.brawl.heavy_done = true; // a grind lasts ≤ 1.2 s per hold
            }
        }
        HeavyKind::TailSweep => {
            // Spin the tail round: everyone around the body is swept away.
            let r = if a.heavy_radius > 0.0 { a.heavy_radius } else { 1.5 + height(c) * 0.45 };
            c.hits.clear();
            launched = sweep(c, env, out, r, a.heavy_dmg * dm, a.heavy_kb * km, 0.55, 2.0, true, W::Heavy);
            clip(out, c, AttackKind::TailSweep, ai_phase::TELEGRAPH, d, 0.0);
            clip(out, c, AttackKind::TailSweep, ai_phase::IMPACT, d, 2.0);
            fx(out, c, HeldFx::TailSweep, c.center(), d, r);
            c.set_anim(AnimState::Active, env.tick());
        }
        HeavyKind::Pounce => {
            if grounded {
                // An arc that LANDS on the aimed prey (≤ 14 m), else a leap ahead.
                let (dir, dist) = match aim_target(c, env, aim, 14.0) {
                    Some(tp) => {
                        let v = flat(tp - c.center());
                        (v.normalize_or(d), (v.length() - c.half.x.max(c.half.z) * 0.5).max(1.0))
                    }
                    None => (d, 7.0 + height(c)),
                };
                let (speed, up) = pounce_arc(dist, a.dash_speed * 1.2);
                start_pounce(c, out, dir, speed, up);
            } else {
                // In the air: a claw swipe down.
                strike(c, env, out, aim, reach, a.heavy_dmg * dm, a.heavy_kb * km, d * 0.4 - Vec3::Y * 0.6, 1.8, 0.0, HeldFx::PilotAttack, W::Heavy);
            }
        }
        HeavyKind::Stomp => {
            c.brawl.armor_t = c.brawl.armor_t.max(0.3);
            let r = if a.heavy_radius > 0.0 { a.heavy_radius } else { 3.0 + height(c) * 0.55 };
            let at = feet(c);
            launched = quake(c, env, out, at, r, a.heavy_dmg * dm, a.heavy_kb * km, HeldFx::GroundPound, W::Heavy);
            clip(out, c, AttackKind::Stomp, ai_phase::IMPACT, d, 2.0);
            c.set_anim(AnimState::Active, env.tick());
        }
    }
    c.brawl.open = t.combo_window;
    c.brawl.cd = a.string_interval;
    c.brawl.step = 0;
    if launched > 0 && c.brawl.flash <= 0.0 {
        open_flash(c, t, out);
    }
}

/// A pounce covering `dist` m: (horizontal speed ≤ `max_speed`, launch speed up) under the
/// piloted body's 25 m/s² gravity, 0.3–0.6 s in the air.
fn pounce_arc(dist: f32, max_speed: f32) -> (f32, f32) {
    let t = (dist / max_speed.max(4.0)).clamp(0.3, 0.6);
    (dist / t, 12.5 * t)
}

/// Leap along `d` (a beast's pounce): the landing hits what it lands on.
fn start_pounce(c: &mut Construct, out: &mut ConstructOut, d: Vec3, speed: f32, up: f32) {
    c.vel = Vec3::new(d.x * speed, up, d.z * speed);
    c.grounded = false;
    c.brawl.pounce_t = POUNCE_SECS;
    c.hits.clear();
    c.rot = geom::yaw_rotation(d);
    clip(out, c, AttackKind::Charge, ai_phase::ACTIVE, d, 0.0);
    fx(out, c, HeldFx::Pounce, c.center(), d, speed);
}

/// A pounce in flight: contact pins (a short stun) and knocks down; landing ends it.
#[allow(clippy::too_many_arguments)]
fn step_pounce(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, _a: &Archetype, grounded: bool, dm: f32, km: f32, dmg: f32, kb: f32) -> u8 {
    c.brawl.pounce_t = (c.brawl.pounce_t - env.dt()).max(0.0);
    let body = geom::inflate(&c.bounds(), 0.7);
    let d = flat(c.vel).normalize_or(c.forward());
    let mut n = 0u8;
    for t2 in env.ctx.enemies_of(c.owner) {
        if Some(t2.id) == c.pilot || !t2.hull.intersects(&body) || !c.hits.insert(player_key(t2.id)) {
            continue;
        }
        hit(c, out, t2.id, t2.hull.center(), dmg * dm, kb * km, d * 0.6 - Vec3::Y * 0.2 + Vec3::Y * 0.5, 2.1, 0.3, W::Heavy);
        clip(out, c, AttackKind::Bite, ai_phase::IMPACT, d, 2.1);
        n = n.saturating_add(1);
    }
    c.set_anim(AnimState::Move, env.tick());
    let landed = c.brawl.pounce_t < POUNCE_SECS - 0.15 && (grounded || (c.vel.y <= 0.0 && near_ground(c, env, 0.4)));
    if landed || c.brawl.pounce_t <= 0.0 {
        c.brawl.pounce_t = 0.0;
        c.vel.x *= 0.3;
        c.vel.z *= 0.3;
        clip(out, c, AttackKind::Charge, ai_phase::END, d, 0.0);
        fx(out, c, HeldFx::GroundPound, feet(c), Vec3::NEG_Y, 1.0);
    }
    n
}

/// A slam that was falling lands (`grounded`): the shockwave (bigger bodies, bigger rings).
/// Returns true when it fired.
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
    let at = feet(c);
    slam(c, env, out, at, fall, HeldFx::TransformSlam);
    // Big robots shake the ground further than a hammer does.
    let h = height(c);
    if h > 5.0 {
        let (dm, km) = scale_k(c);
        let r = 2.0 + h * 0.6;
        c.hits.clear();
        for (id, hp) in in_reach(c, env, at, r) {
            if (hp - at).length() > 3.0 {
                hit(c, out, id, hp, 4.0 * dm, 14.0 * km, flat(hp - at).normalize_or(c.forward()) + Vec3::Y * 1.2, 1.4, 0.0, W::Light);
            }
        }
    }
    true
}

// ------------------------------------------------------------------------------------ transform attacks

/// Can the Transform key start a transform attack now (robot mode, inside the V2 FLASH window —
/// a connected ender / launcher / counter)? Outside it the Transform key just transforms.
pub fn wants_xattack(c: &Construct) -> bool {
    c.brawl.xatk == XPhase::None && c.brawl.flash > 0.0 && !c.held.vehicle && c.has(Mechanic::Transforms) && c.brawl.pounce_t <= 0.0
}

/// V2 STAGE 3 EARLY-TRANSFORM BUFFER: is a string's ender, or a Launcher heavy's wind-up, about to
/// connect and open the flash window (`open_flash`)? A Transform press now (`pilot.rs`) is held for
/// `TransformerTuning::early_xform_secs` instead of firing an immediate plain transform.
pub fn ender_pending(c: &Construct, a: &Archetype) -> bool {
    (c.brawl.cd > 0.0 && !c.brawl.string_done && c.brawl.step == a.string_hits.saturating_sub(1))
        || (c.brawl.hold > 0.0 && !c.brawl.heavy_done && a.heavy == HeavyKind::Launcher)
}

/// BEAST MODES: Transform during a beast-mode string = the robot bursts out of the beast straight
/// into its finisher (the reverse of a transform attack).
pub fn wants_burst_out(c: &Construct, t: &TransformerTuning) -> bool {
    c.brawl.xatk == XPhase::None && c.brawl.open > 0.0 && c.held.vehicle && c.has(Mechanic::Transforms) && beast_alt(c, t)
}

/// Burst out of the beast into the robot's finisher (see [`wants_burst_out`]).
pub fn burst_out(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, aim: Vec3) {
    let t = &env.ctx.tuning.transformer;
    let a = t.robot_row(c).clone();
    let (dm, km) = scale_k(c);
    let here = c.center();
    let dir = match target_point(c, env) {
        Some(tp) if (tp - here).length() < 30.0 => flat(tp - here).normalize_or(flat(aim)),
        _ => flat(aim).normalize_or(flat(c.forward())),
    }
    .normalize_or(Vec3::NEG_Z);
    c.brawl.xatk_dir = dir;
    c.brawl.last_xattack = None;
    c.brawl.last_xattack_hits = 0;
    c.brawl.open = 0.0;
    c.brawl.step = 0;
    c.brawl.string_done = false;
    c.rot = geom::yaw_rotation(dir);
    let grounded = c.grounded || near_ground(c, env, 1.2);
    begin_finish(c, env, out, &a, grounded, dm, km);
}

/// Burst out as the vehicle through the target (see [`XAttack`]).
pub fn start_xattack(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, aim: Vec3) {
    let t = &env.ctx.tuning.transformer;
    let a = t.robot_row(c).clone();
    let here = c.center();
    let dir = match target_point(c, env) {
        Some(tp) if (tp - here).length() < 30.0 => flat(tp - here).normalize_or(flat(aim)),
        _ => flat(aim).normalize_or(flat(c.forward())),
    }
    .normalize_or(Vec3::NEG_Z);
    let b = &mut c.brawl;
    if b.flash > 0.0 {
        b.flashes_taken = b.flashes_taken.saturating_add(1);
    }
    b.flash = 0.0;
    b.branch = 0.0;
    b.branch_opened = false;
    b.branch_hold = 0.0;
    b.last_branch = Branch::None;
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
    b.carry = None;
    b.carry_t = 0.0;
    b.pounce_t = 0.0;
    c.hits.clear();
    c.rot = geom::yaw_rotation(dir);
    // A quake is the robot's own body (a combiner never splits mid-fight); every other shape bursts
    // out as the alt mode.
    if a.xattack != XAttack::Quake {
        snap(c, false, t, out);
    }
    burst_setup(c, env, out, &a, dir, here);
}

/// The shape's opening beat (a pounce's arc, a stampede's roar, a quake's wind-up); a rush (V2
/// branch) replays it for the second pass.
fn burst_setup(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, dir: Vec3, here: Vec3) {
    match a.xattack {
        XAttack::Pounce => {
            let dist = target_point(c, env).map(|tp| flat(tp - here).length()).filter(|d| *d < 16.0).unwrap_or(8.0 + height(c));
            let (speed, up) = pounce_arc(dist.max(2.0), a.xattack_speed);
            c.vel = Vec3::new(dir.x * speed, up, dir.z * speed);
            c.grounded = false;
            clip(out, c, AttackKind::Charge, ai_phase::ACTIVE, dir, 0.0);
            fx(out, c, HeldFx::Pounce, here, dir, speed);
        }
        XAttack::Stampede => {
            clip(out, c, AttackKind::Charge, ai_phase::ACTIVE, dir, 0.0);
            fx(out, c, HeldFx::Roar, mouth(c, dir), dir, 2.0);
        }
        XAttack::Quake => {
            clip(out, c, AttackKind::Stomp, ai_phase::TELEGRAPH, dir, 0.0);
            c.set_anim(AnimState::Telegraph, env.tick());
        }
        _ => {}
    }
    fx(out, c, HeldFx::PilotAttack, here, dir, 2.0);
}

/// Keep a scooped victim on the nose (velocity-matched through impulses: the sim never teleports a
/// player), then fling them after [`CARRY_SECS`] or when the burst ends (`release`).
fn step_carry(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, km: f32, release: bool) {
    let Some(id) = c.brawl.carry else { return };
    c.brawl.carry_t += env.dt();
    let Some(v) = env.ctx.player(id).copied().filter(|v| v.alive) else {
        c.brawl.carry = None;
        return;
    };
    let dir = c.brawl.xatk_dir;
    if release || c.brawl.carry_t >= CARRY_SECS {
        c.brawl.carry = None;
        // The fling: whatever speed the nose had, plus the ram's knockback.
        vhit(c, &env.ctx.tuning.transformer, out, id, v.hull.center(), 2.0, a.xattack_kb * km, dir + Vec3::Y * 0.45, 2.2);
        return;
    }
    let body = c.half.x.max(c.half.z);
    let nose = c.center() + dir * (body + 0.9);
    let want = Vec3::new(c.vel.x, c.vel.y.max(0.0), c.vel.z) + flat(nose - v.hull.center()) * 8.0;
    let dv = want - v.vel;
    if dv.is_finite() {
        out.impulses.push((id, dv.clamp_length_max(60.0), 0.0));
    }
}

/// Advance a transform attack. Sets the body's velocity / facing; returns true while it flies (the
/// caller integrates with flight, else on the ground). `again` = Transform pressed again (the finisher
/// comes now; inside the V2 branch window: the drift back into melee). `inp` = this tick's buttons
/// (the branch: LMB tap = rush, RMB / held LMB = the air launch). `grounded` = ground contact before
/// this tick.
pub fn step_xattack(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, again: bool, grounded: bool, inp: &Input) -> bool {
    let t = &env.ctx.tuning.transformer;
    let a = t.robot_row(c).clone();
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
                XAttack::Ram | XAttack::Grind | XAttack::DriftRam | XAttack::SpinOut | XAttack::Stampede => {
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
                        // The first victim of a carrying ram rides the nose (the fling comes later).
                        let scoop = a.xattack_carry && c.brawl.carry.is_none() && matches!(a.xattack, XAttack::Ram | XAttack::Stampede);
                        if scoop {
                            hit(c, out, t2.id, t2.hull.center(), a.xattack_dmg * dm, a.xattack_kb * km * 0.15, kb_dir, 2.0, t.launch_stun, W::Vehicle);
                        } else if a.xattack == XAttack::Grind {
                            hit(c, out, t2.id, t2.hull.center(), a.xattack_dmg * dm, a.xattack_kb * km, kb_dir, 1.6, 0.0, W::VehicleTick);
                        } else {
                            vhit(c, t, out, t2.id, t2.hull.center(), a.xattack_dmg * dm, a.xattack_kb * km, kb_dir, 1.6);
                        }
                        if scoop {
                            c.brawl.carry = Some(t2.id);
                            c.brawl.carry_t = 0.0;
                        }
                        if a.xattack == XAttack::Stampede {
                            clip(out, c, AttackKind::Bite, ai_phase::IMPACT, vdir, 1.8);
                        }
                        c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(1);
                        fx(out, c, HeldFx::PilotAttack, t2.hull.center(), vdir, 1.6);
                    }
                    step_carry(c, env, out, &a, km, false);
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
                        let n = sweep(c, env, out, a.xattack_width, a.xattack_dmg * dm, a.xattack_kb * km, 0.5, 1.0, false, W::VehicleTick);
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
                XAttack::Pounce => {
                    // An arc onto the target: steer a little toward it while climbing, land on it.
                    if let Some(p) = tp {
                        let to = flat(p - c.center());
                        let want = to.normalize_or(dir) * a.xattack_speed.min(to.length() / (a.xattack_secs * 0.6).max(0.1)).max(6.0);
                        c.vel.x += (want.x - c.vel.x) * (6.0 * dt).min(1.0);
                        c.vel.z += (want.z - c.vel.z) * (6.0 * dt).min(1.0);
                    }
                    c.rot = geom::yaw_rotation(flat(c.vel).normalize_or(dir));
                    let body = geom::inflate(&c.bounds(), a.xattack_width * 0.4);
                    for t2 in env.ctx.enemies_of(c.owner) {
                        if Some(t2.id) == c.pilot || !t2.hull.intersects(&body) || !c.hits.insert(player_key(t2.id)) {
                            continue;
                        }
                        vhit(c, t, out, t2.id, t2.hull.center(), a.xattack_dmg * dm, a.xattack_kb * km, flat(c.vel).normalize_or(dir) * 0.5 + Vec3::Y * 0.3, 2.0);
                        clip(out, c, AttackKind::Bite, ai_phase::IMPACT, dir, 2.0);
                        c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(1);
                    }
                    // Landed: the burst is over (the finisher follows at once).
                    if grounded && c.brawl.xatk_t > 0.15 && c.brawl.xatk_t < a.xattack_secs {
                        c.brawl.xatk_t = a.xattack_secs;
                    }
                }
                XAttack::Quake => {
                    // Wind up (arms raised) through the first half, then both fists into the ground.
                    c.vel.x *= 0.3;
                    c.vel.z *= 0.3;
                    c.rot = geom::yaw_rotation(dir);
                    if c.brawl.xatk_n == 0 && u >= 0.5 {
                        c.brawl.xatk_n = 1;
                        c.hits.clear();
                        let at = feet(c) + dir * c.half.z.max(c.half.x);
                        let n = quake(c, env, out, at, a.xattack_width.max(3.0 + height(c) * 0.5), a.xattack_dmg * dm, a.xattack_kb.max(t.vehicle_min_kb) * km, HeldFx::GroundPound, W::Vehicle);
                        clip(out, c, AttackKind::Stomp, ai_phase::IMPACT, dir, 2.5);
                        c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(n);
                    }
                }
            }
            c.set_anim(AnimState::Move, env.tick());
            // V2 BRANCH: the first connect opens the branch window (a blue cue on the vehicle).
            if !c.brawl.branch_opened && c.brawl.last_xattack_hits > 0 {
                c.brawl.branch_opened = true;
                c.brawl.branch = t.branch_secs;
                c.brawl.branch_hold = 0.0;
                let at = c.center() + Vec3::Y * c.half.y * 0.55;
                fx(out, c, HeldFx::Flash, at, dir, -t.branch_secs);
            }
            if c.brawl.branch > 0.0 {
                if let Some(b) = pick_branch(c, t, again, inp, dt) {
                    c.brawl.branch = 0.0;
                    c.brawl.last_branch = b;
                    c.brawl.branches = c.brawl.branches.saturating_add(1);
                    step_carry(c, env, out, &a, km, true);
                    if matches!(a.xattack, XAttack::Pounce | XAttack::Stampede) {
                        clip(out, c, AttackKind::Charge, ai_phase::END, dir, 0.0);
                    }
                    match b {
                        Branch::Rush => rush(c, env, out, &a, tp),
                        Branch::Launch => begin_finish_as(c, env, out, &a, true, dm, km, Some(Finisher::Uppercut)),
                        Branch::Drift | Branch::None => drift_back(c, env, out, &a, tp),
                    }
                    return flying && c.brawl.xatk == XPhase::Burst;
                }
            }
            // V2 STAGE 3 BRANCH WINDOW FIX: a still-open branch window (the vehicle attack connected)
            // runs its own full `branch_secs`, never cut short by the burst's `xattack_secs` ending —
            // a late connect (near the end of the burst) still gets the whole window to pick from.
            if c.brawl.branch <= 0.0 && (c.brawl.xatk_t >= a.xattack_secs || (again && c.brawl.xatk_t > 0.1)) {
                step_carry(c, env, out, &a, km, true);
                if matches!(a.xattack, XAttack::Pounce | XAttack::Stampede) {
                    clip(out, c, AttackKind::Charge, ai_phase::END, dir, 0.0);
                }
                let low = !flying || near_ground(c, env, 1.2);
                begin_finish(c, env, out, &a, grounded && low, dm, km);
            }
            flying
        }
        XPhase::Finish => {
            // The V2 launch branch is an uppercut whatever the row's finisher is.
            let own = c.brawl.last_branch != Branch::Launch;
            let slam_kind = (own && matches!(a.finisher, Finisher::DiveKick | Finisher::DropSlam)) || c.brawl.slam_pending;
            if own && a.finisher == Finisher::DiveKick && !grounded {
                // Keep diving at the target; the contact hits.
                let to = tp.map_or(Vec3::NEG_Y, |p| (p - c.center()).normalize_or(Vec3::NEG_Y));
                let d = (to + Vec3::NEG_Y * 0.6).normalize_or(Vec3::NEG_Y);
                c.vel = d * 28.0;
                let body = geom::inflate(&c.bounds(), 0.8);
                for t2 in env.ctx.enemies_of(c.owner) {
                    if Some(t2.id) != c.pilot && t2.hull.intersects(&body) && c.hits.insert(player_key(t2.id)) {
                        hit(c, out, t2.id, t2.hull.center(), a.finisher_dmg * dm, a.finisher_kb * km, flat(d) + Vec3::Y * 0.5, 2.2, 0.0, W::Heavy);
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

/// V2 BRANCH input: Transform = drift back; RMB = the launch; an LMB tap = the rush, an LMB held past
/// `branch_hold` = the launch. None = nothing yet (the default finisher follows the burst).
fn pick_branch(c: &mut Construct, t: &TransformerTuning, again: bool, inp: &Input, dt: f32) -> Option<Branch> {
    if again {
        return Some(Branch::Drift);
    }
    if inp.press2 {
        return Some(Branch::Launch);
    }
    let b = &mut c.brawl;
    if inp.press1 {
        b.branch_hold = dt.max(1e-4);
        return None;
    }
    if b.branch_hold > 0.0 {
        if inp.hold1 {
            b.branch_hold += dt;
            if b.branch_hold >= t.branch_hold {
                return Some(Branch::Launch);
            }
        } else {
            return Some(Branch::Rush);
        }
    }
    None
}

/// V2 BRANCH — rush: the vehicle comes round (a J-turn onto them) for a second pass. One branch per
/// transform attack; the finisher still follows the second pass.
fn rush(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, tp: Option<Vec3>) {
    let here = c.center();
    let dir = tp.map_or(-c.brawl.xatk_dir, |p| flat(p - here).normalize_or(-c.brawl.xatk_dir)).normalize_or(Vec3::NEG_Z);
    let b = &mut c.brawl;
    b.xatk_t = 0.0;
    b.xatk_tick = 0.0;
    b.xatk_n = 0;
    b.xatk_dir = dir;
    b.carry = None;
    b.carry_t = 0.0;
    c.hits.clear();
    c.rot = geom::yaw_rotation(dir);
    burst_setup(c, env, out, a, dir, here);
    fx(out, c, HeldFx::Thrust, here, dir, 1.5);
}

/// V2 BRANCH — drift: a drift-turn back into ROBOT mode facing them, snapped in close, the string
/// open again (no finisher: your next LMB is the next hit).
fn drift_back(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, tp: Option<Vec3>) {
    let t = &env.ctx.tuning.transformer;
    if c.held.vehicle || c.comp.xform.mode != super::components::XMode::Robot {
        snap(c, true, t, out);
    }
    let here = c.center();
    let to = tp.map(|p| p - here).filter(|v| flat(*v).length() < 14.0);
    let dir = to.map_or(-c.brawl.xatk_dir, |v| flat(v).normalize_or(c.brawl.xatk_dir)).normalize_or(Vec3::NEG_Z);
    c.rot = geom::yaw_rotation(dir);
    if !c.suit {
        snap_in(c, env, out, to, dir, here);
    }
    c.vel.x *= 0.2;
    c.vel.z *= 0.2;
    let b = &mut c.brawl;
    b.xatk = XPhase::None;
    b.slam_pending = false;
    b.step = 0;
    b.string_done = false;
    b.open = t.combo_window;
    b.cd = 0.0;
    fx(out, c, HeldFx::Thrust, here, dir, 1.0);
    let _ = a;
}

/// The robot SNAPS back in on the target (the burst knocked it along the travel): a short lunge
/// (≤ 9 m) so the next hit connects — never into terrain. V2 (hero scale): a target the vehicle
/// knocked up is met in the air (a hop ≤ 3 m), so a human-sized victim is still in the swing.
fn snap_in(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, to: Option<Vec3>, dir: Vec3, here: Vec3) {
    if let Some(v) = to {
        let body = c.half.x.max(c.half.z);
        let gap = (flat(v).length() - (body + 1.2)).clamp(0.0, 9.0);
        let up = (v.y - c.half.y * 0.25).clamp(0.0, 3.0);
        let up = if up > 0.3 { up } else { 0.0 };
        if gap > 0.0 || up > 0.0 {
            let dest = c.pos + dir * gap + Vec3::Y * up;
            let mid = dest + c.rot * c.local_center;
            if !env.ctx.voxels.is_solid(bd_core::voxel::world_to_voxel(mid)) {
                c.pos = dest;
                if up > 0.0 {
                    c.vel.y = c.vel.y.max(3.0);
                    c.grounded = false;
                }
                fx(out, c, HeldFx::Lunge, here, dir, 1.5);
            }
        }
    }
}

fn begin_finish(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, grounded: bool, dm: f32, km: f32) {
    begin_finish_as(c, env, out, a, grounded, dm, km, None);
}

/// The finisher (`fin` overrides the row's: the V2 launch branch is an uppercut).
#[allow(clippy::too_many_arguments)]
fn begin_finish_as(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, a: &Archetype, grounded: bool, dm: f32, km: f32, fin: Option<Finisher>) {
    let t = &env.ctx.tuning.transformer;
    let finisher = fin.unwrap_or(a.finisher);
    let grounded = grounded || near_ground(c, env, 1.2);
    c.brawl.xatk = XPhase::Finish;
    c.brawl.xatk_t = 0.0;
    c.hits.clear();
    // Back to robot for the finisher (a quake never left it).
    if c.held.vehicle || c.comp.xform.mode != super::components::XMode::Robot {
        snap(c, true, t, out);
    }
    let here = c.center();
    let to = target_point(c, env).map(|p| p - here).filter(|v| flat(*v).length() < 14.0);
    let dir = to.map_or(c.brawl.xatk_dir, |v| flat(v).normalize_or(c.brawl.xatk_dir));
    // The robot SNAPS back in on the target so the finisher connects.
    if grounded && !c.suit && matches!(finisher, Finisher::Uppercut | Finisher::JTurn | Finisher::Axe | Finisher::SpinSlash) {
        snap_in(c, env, out, to, dir, here);
    }
    let here = c.center();
    let fall = |c: &mut Construct| {
        c.brawl.slam_pending = true;
        c.brawl.fall_speed = 22.0;
        c.vel = Vec3::new(c.vel.x * 0.2, -22.0, c.vel.z * 0.2);
    };
    let rk = reach_k(c);
    let mut n = 0u8;
    match finisher {
        Finisher::DiveKick => {
            if grounded {
                n = strike(c, env, out, dir, 2.0 * rk, a.finisher_dmg * dm, a.finisher_kb * km, dir + Vec3::Y * 0.8, 2.2, 0.0, HeldFx::Uppercut, W::Heavy);
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
            // V2 launch branch: a juggle launcher — they float to a height the robot follows them to
            // (the air string takes it from there), not a send-off.
            let juggle = c.brawl.last_branch == Branch::Launch;
            let kb = if juggle { a.heavy_kb * t.juggle_kb * km } else { a.finisher_kb * km };
            n = strike(c, env, out, dir, 2.0 * rk, a.finisher_dmg * dm, kb, dir * 0.3 + Vec3::Y * 1.6, 2.4, t.launch_stun, HeldFx::Uppercut, W::Heavy);
            c.vel.y = c.vel.y.max(if juggle { t.juggle_rise } else { 6.0 });
        }
        Finisher::JTurn => {
            // Flip round, facing back through the target, and launch it.
            let back = to.map_or(-c.brawl.xatk_dir, |v| flat(v).normalize_or(-c.brawl.xatk_dir));
            n = strike(c, env, out, back, 2.2 * rk, a.finisher_dmg * dm, a.finisher_kb * km, back * 0.4 + Vec3::Y * 1.4, 2.3, t.launch_stun, HeldFx::Uppercut, W::Heavy);
        }
        Finisher::Axe => {
            n = strike(c, env, out, dir, 2.8 * rk, a.finisher_dmg * dm, a.finisher_kb * km, dir + Vec3::Y * 0.7, 2.6, t.launch_stun, HeldFx::GroundPound, W::Heavy);
            out.carves.push(crate::combat::CarveRequest { center: here + dir * (c.half.z + 2.0) - Vec3::Y * c.half.y, radius: (c.half.y * 0.35).clamp(1.0, 2.5), power: 0.6 });
        }
        Finisher::CannonBlast => {
            let aim = to.map_or(dir, |v| v.normalize_or(dir));
            super::pilot::shoot(c, out, muzzle(c, aim), aim, 70.0, a.finisher_dmg * dm, a.finisher_kb * km, true, GunArchetype::Cannon, 5.0);
            fx(out, c, HeldFx::PilotAttack, muzzle(c, aim), aim, 2.5);
            n = 1;
        }
        Finisher::SpinSlash => {
            n = sweep(c, env, out, 2.5 * rk, a.finisher_dmg * dm, a.finisher_kb * km, 0.6, 2.2, true, W::Heavy);
            fx(out, c, HeldFx::PilotAttack, here, Vec3::Y, 2.0);
        }
    }
    c.brawl.last_xattack_hits = c.brawl.last_xattack_hits.saturating_add(n);
}

// ------------------------------------------------------------------------------------ voice orders

/// VOICE (FEEL pass): a seated pilot said a move mid-fight. Returns true if the robot takes it (the
/// world then doesn't fire the move from the pilot's own body). Generic by the move's data: its
/// tags (Combo / Ultimate / Movement / Defense / Counter) and, untagged, what its ops do (a thrown /
/// shot / beam thing → the blaster; a dash / ram → a boost-step; a slam / shove / blast → the heavy).
pub fn order(c: &mut Construct, o: Order) -> bool {
    if o == Order::None || !c.has(Mechanic::Transforms) || c.gone.is_some() || !c.active() {
        return false;
    }
    c.brawl.order = o;
    c.brawl.order_t = 0.0;
    c.brawl.order_n = 0;
    c.brawl.orders = c.brawl.orders.saturating_add(1);
    true
}

/// About how long order `o` keeps the robot busy (s): a chain's next step waits for it.
pub fn order_secs(c: &Construct, o: Order, t: &TransformerTuning) -> f32 {
    let a = t.robot_row(c);
    match o {
        Order::None => 0.0,
        Order::Combo => 0.3 + a.string_interval * (a.string_hits as f32 + 1.0) + a.xattack_secs + t.finisher_secs + t.combat_transform_secs * 2.0,
        Order::Super => a.xattack_secs + t.finisher_secs + t.combat_transform_secs * 2.0,
        Order::Heavy => a.heavy_hold + 0.25,
        Order::Ranged => 0.3,
        Order::Dash => a.dash_secs + 0.1,
        Order::Guard => GUARD_SECS,
    }
}

/// Carry out the pending voice order for one tick: rewrites this tick's input (a scripted combo
/// presses the buttons in rhythm). Returns true when the order asks for Transform this tick.
pub fn run_order(c: &mut Construct, env: &Env<'_>, inp: &mut Input) -> bool {
    let o = c.brawl.order;
    if o == Order::None {
        return false;
    }
    let dt = env.dt();
    c.brawl.order_t += dt;
    let t = &env.ctx.tuning.transformer;
    let a = if c.held.vehicle { t.alt_row(c).unwrap_or_else(|| t.robot_row(c)) } else { t.robot_row(c) };
    let (interval, hits, hold) = (a.string_interval, a.string_hits, a.heavy_hold);
    // Aim at the current target (a voice order is said, not aimed).
    if let Some(tp) = target_point(c, env) {
        let d = tp - c.center();
        if flat(d).length() < 30.0 {
            inp.aim = d.normalize_or(inp.aim);
        }
    }
    let mut transform = false;
    let done = |c: &mut Construct| c.brawl.order = Order::None;
    match o {
        Order::None => {}
        Order::Combo => {
            // Close in, the whole string in rhythm, then the transform attack (the finisher follows).
            if c.brawl.xatk != XPhase::None {
                if c.brawl.order_n > hits {
                    done(c);
                }
                return false;
            }
            if c.brawl.order_n < hits {
                if c.brawl.cd <= 0.0 {
                    if let Some(tp) = target_point(c, env) {
                        let gap = flat(tp - c.center()).length() - (c.half.x.max(c.half.z) + a.string_reach * reach_k(c));
                        if gap > 0.5 && gap < 12.0 && c.brawl.dodge_cd <= 0.0 && c.brawl.order_n == 0 {
                            inp.dodge = true; // close the gap with a boost-step first
                            inp.wish = flat(tp - c.center()).normalize_or_zero();
                            return false;
                        }
                    }
                    inp.press1 = true;
                    c.brawl.order_n += 1;
                }
            } else if c.brawl.order_n == hits && (c.brawl.flash > 0.0 || c.brawl.cd <= interval) {
                // The extender, in the flash window (a said combo never whiffs its extender: the
                // window is held open for it).
                c.brawl.flash = c.brawl.flash.max(0.1);
                transform = true;
                done(c);
            } else if c.brawl.order_t > 0.4 + interval * (hits as f32 + 3.0) {
                done(c);
            }
        }
        Order::Super => {
            c.brawl.open = c.brawl.open.max(0.1);
            c.brawl.flash = c.brawl.flash.max(0.1);
            transform = true;
            done(c);
        }
        Order::Heavy => {
            inp.hold1 = true;
            if c.brawl.heavy_done || c.brawl.order_t > hold + 0.4 {
                inp.hold1 = false;
                done(c);
            }
        }
        Order::Ranged => {
            inp.press2 = c.brawl.order_n == 0;
            inp.hold2 = true;
            c.brawl.order_n = 1;
            if a.ranged != RangedKind::Breath || c.brawl.order_t > 1.2 {
                inp.hold2 = a.ranged == RangedKind::Breath && c.brawl.order_t <= 1.2;
                if c.brawl.order_t > 0.05 {
                    done(c);
                }
            }
        }
        Order::Dash => {
            inp.dodge = true;
            inp.wish = flat(inp.aim).normalize_or_zero();
            done(c);
        }
        Order::Guard => {
            if c.brawl.order_n == 0 {
                c.brawl.order_n = 1;
                inp.dodge = true;
                inp.wish = Vec3::ZERO;
                c.brawl.dodge_cd = 0.0;
            } else {
                // Hold the perfect window open while the guard lasts (a counter stance).
                c.brawl.iframes = c.brawl.iframes.max(dt);
                c.brawl.dodge_t = 0.0;
                if c.brawl.order_t > GUARD_SECS {
                    done(c);
                }
            }
        }
    }
    transform
}

/// Which robot order a said MOVE is (generic by data: tags first, then what its first real op does).
pub fn order_for_move(mv: &crate::powers::MoveDef) -> Order {
    use crate::powers::{MoveOp as O, MoveTag as T};
    if mv.tags.contains(&T::Combo) {
        return Order::Combo;
    }
    if mv.tags.contains(&T::Ultimate) {
        return Order::Super;
    }
    if mv.tags.contains(&T::Defense) || mv.tags.contains(&T::Counter) {
        return Order::Guard;
    }
    if mv.tags.contains(&T::Movement) {
        return Order::Dash;
    }
    for op in &mv.ops {
        return match op {
            O::Transform | O::Tier(_) => continue,
            O::Barrage { .. } | O::Rush { .. } | O::Finisher { .. } => Order::Combo,
            O::Ram { .. } | O::Impulse { .. } | O::Blink { .. } | O::Reel { .. } | O::Leap { .. } | O::Pounce { .. } | O::Surge { .. } | O::WarpTo { .. } | O::BlinkCarry { .. } | O::Relocate { .. } => {
                Order::Dash
            }
            O::Counter { .. } | O::Deflect { .. } | O::Evade { .. } => Order::Guard,
            O::Cast { recipe, .. } => order_for_form(recipe.form, &recipe.mech),
            O::Seeker { .. } | O::Salvo { .. } | O::BounceBeam { .. } | O::Hook { .. } => Order::Ranged,
            _ => Order::Heavy,
        };
    }
    Order::Heavy
}

/// Which robot order a said THING is ("energon axe" → the robot's heavy, "fusion cannon" → its
/// blaster), or None (the words build nothing for a robot: a wall, a dragon…).
pub fn order_for_recipe(r: &Recipe) -> Option<Order> {
    use bd_core::mechanics::FormCategory as C;
    match r.form.category() {
        C::Held | C::Object => Some(order_for_form(r.form, &r.mechanics)),
        _ => None,
    }
}

fn order_for_form(f: Form, mech: &[Mechanic]) -> Order {
    let shoots = mech.iter().any(|m| matches!(m, Mechanic::Shoots | Mechanic::Beam | Mechanic::Explodes | Mechanic::Homing));
    match f {
        Form::Gun | Form::Cannon | Form::Bow | Form::Hands | Form::Ball | Form::Bomb | Form::Rocket | Form::Disc => Order::Ranged,
        _ if shoots && f.category() != bd_core::mechanics::FormCategory::Held => Order::Ranged,
        Form::Shield => Order::Guard,
        _ => Order::Heavy,
    }
}

// ------------------------------------------------------------------------------------ world side

impl crate::world::World {
    /// TRANSFORMERS FEEL: words a COCKPIT pilot says mid-fight still work — "transform", "roll
    /// out" switch modes; a weapon / shot ("energon axe", "fusion cannon") is the robot's own heavy /
    /// blaster. Returns None when the words are for nobody's robot (the cast is refused as before).
    pub(crate) fn seated_words(&mut self, player: bd_core::PlayerId, cid: bd_core::ConstructId, recipe: &Recipe) -> Option<Result<bd_core::ConstructId, crate::events::CastError>> {
        use super::graft::Command;
        let transformer = self.constructs.get(cid).is_some_and(|c| c.gone.is_none() && c.has(Mechanic::Transforms));
        if !transformer {
            return None;
        }
        match super::graft::command_of(recipe) {
            Some(Command::Transform) => {
                self.toggle_transform(player);
                return Some(Ok(cid));
            }
            Some(Command::RollOut) => {
                self.toggle_transform(player);
                let others: Vec<bd_core::ConstructId> = self.constructs.iter().filter(|c| c.owner == player && c.id != cid && c.pilot.is_none()).map(|c| c.id).collect();
                for o in others {
                    if let Some(c) = self.constructs.get_mut(o) {
                        super::components::request_transform(c);
                    }
                }
                return Some(Ok(cid));
            }
            Some(_) => return None,
            None => {}
        }
        let o = order_for_recipe(recipe)?;
        let c = self.constructs.get_mut(cid)?;
        order(c, o).then_some(Ok(cid))
    }
}

// ------------------------------------------------------------------------------------ AI followers

/// A transformer follower's robot-mode brain: close in (the mode machine walks it), strings when in
/// reach, a transform attack as the string's extender, the blaster at range, boost-steps when
/// someone is close (a hit inside the window = a counter). Runs from `components::react`.
pub fn ai(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let grounded = c.grounded || c.body.supported;
    if c.brawl.xatk != XPhase::None {
        c.body.claim(Driver::Charge);
        // V2 BRANCH: a follower picks a follow-up a beat into the window (by an id / attack hash).
        let t = &env.ctx.tuning.transformer;
        let mut inp = Input::default();
        let mut again = false;
        if c.brawl.branch > 0.0 && c.brawl.branch <= t.branch_secs - t.ai_flash_delay {
            let h = crate::constructs::ai::hash01(c.id.0, c.brawl.ai_xattacks as u64, 91);
            if h < 0.34 {
                inp.press1 = true; // rush (a tap: released next tick)
            } else if h < 0.67 {
                inp.press2 = true; // the air launch
            } else {
                again = true; // drift back into the string
            }
        }
        let flying = step_xattack(c, env, out, again, grounded, &inp);
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
    let a = t.robot_row(c).clone();
    let here = c.center();
    let to = tv.hull.center() - here;
    let dist = flat(to).length();
    let reach = c.half.x.max(c.half.z) + a.string_reach * reach_k(c) + 1.0;
    let aim = to.normalize_or(c.forward());
    let mut inp = Input { aim, ..Input::default() };
    // Close the last metres into melee (the mode machine's walk stops short of a big robot's reach).
    c.brawl.approach = dist > reach * 0.85 && dist < 18.0 && c.brawl.pounce_t <= 0.0;
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
    // V2 FLASH WINDOW: the ender / launcher connected — take it a beat in (a reaction, not a read).
    if c.brawl.flash > 0.0 && c.brawl.flash <= t.flash_secs - t.ai_flash_delay && dist < 18.0 {
        c.brawl.ai_xattacks = c.brawl.ai_xattacks.saturating_add(1);
        c.brawl.ai_t = t.ai_string_gap;
        start_xattack(c, env, out, aim);
        return;
    }
    if dist <= reach {
        if c.brawl.counter > 0.0 {
            inp.press1 = true;
        } else if c.brawl.cd <= 0.0 && (c.brawl.open > 0.0 || c.brawl.ai_t <= 0.0) {
            inp.press1 = true;
        }
    } else if dist < 28.0 && c.brawl.blaster_cd <= 0.0 && (c.has(Mechanic::Shoots) || c.recipe.payload.is_some() || a.blaster_explode || a.ranged != RangedKind::Bolt) {
        inp.press2 = true;
    }
    robot(c, env, out, &inp, grounded);
    if c.brawl.dodge_t > 0.0 || c.brawl.pounce_t > 0.0 {
        c.body.claim(Driver::Charge);
    }
}
