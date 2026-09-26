//! **Generic mechanic components** (COMBAT.md §5b) — the composition engine. Construct engineer.
//!
//! Every [`Mechanic`] is a behaviour component that works on ANY form: the form's controller gives
//! shape + default control, each mechanic adds a *verb*, and components only talk through shared
//! body state (velocity, contacts, target, owner, triggers). Nothing here scales numbers for
//! "creativity": each combination makes something different *happen* (a flying hammer hunts and
//! bonks, a bouncy wall rebounds players, a homing trampoline slides under an enemy and launches
//! them, a charging wall bulldozes, an anvil swarm is a hailstorm, a flying cage carries its victim
//! over the void, a sticky rocket rides its target, an exploding boomerang craters out *and* back,
//! a black hole = Attract + Explodes collapses at the end of its life…).
//!
//! Pipeline hooks (see `mod.rs`): [`drive`] (before the controller: attachments, frozen/rooted,
//! DropFromSky telegraph, Transforms, Charge, holds), [`steer`] (after the controller: Returning,
//! Flies, Homing, Thrust, SlowFall/Glide, Transforms locomotion), [`react`] (after physics:
//! contacts → hits / Explodes / Sticky / Ricochet / Pull / Grab / Constrict / Trap / Blade…,
//! emitters Shoots / Beam / Breath with depth-1 payloads, Stomp, ArticulatedTail, Spin sweep).
//!
//! ## What a creature controller does NOT get from here
//! Constructs run by [`Controller::Creature`] implement their own locomotion and attacks
//! (Flies, Homing, Charge, Breath, Stomp, ArticulatedTail, Grab, Constrict, Shoots, Swarm units) in
//! `creature.rs`. They still get: attachments (Sticky on them, carried by a cage/grab), frozen /
//! rooted / pinned holds (`c.comp.frozen/rooted/pinned > 0` ⇒ `drive` claims `Hold`), DropFromSky
//! spawn-and-drop, the Transforms *mode machine* (read `c.comp.xform.mode` / `progress` for the
//! gait: `Robot` = biped), Explodes / Sticky / Returning / Ricochet contact reactions, and
//! chemistry. Attached constructs (held / worn / suit) get none of this: `held.rs` owns them.
//!
//! ## Hooks for the other engineers
//! * [`request_transform`] — pilot/suit transform key (held engineer) and "transform" grafts.
//! * [`is_frozen`], [`is_held_down`] — "can this construct act this tick?" (frozen / rooted / pinned).
//! * [`eff_has`] — effective mechanics (recipe + form defaults: a Rocket homes, thrusts and
//!   explodes; an Anvil drops from the sky; a Trampoline bounces; a Cage traps…).

use bd_core::mechanics::{Element, Feature, Form, FormCategory, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Quat, Vec3};

use super::{
    AnimState, AttachKind, Construct, ConstructCtx, ConstructOut, ConstructStore, ContactWith, Controller, Driver, Env, HitList, Interaction, Loco, MechSet, PlayerView,
    SpawnParams, body, controller_of, form_spec, player_key, temperament,
};
use crate::combat::{CarveRequest, ConstructHit, Explosion, HitInfo, HitKind};
use crate::events::{GameEvent, GoneReason};
use crate::geom;
use crate::projectile::{ProjectileKind, ProjectileSpawn, effects};

// ------------------------------------------------------------------------------------ tuning

/// Contact re-hit cooldown per player (s).
pub const TOUCH_REHIT: f32 = 0.6;
/// Returning: comes back after this long in flight (s).
pub const RETURN_AFTER: f32 = 1.2;
pub const RETURN_SPEED: f32 = 30.0;
/// Flies: cruise speed (m/s) before Fast/Heavy/temperament.
pub const FLY_SPEED: f32 = 11.0;
/// Homing turn rate (rad/s).
pub const HOMING_TURN: f32 = 2.5;
/// Walks: lumbering speed of a non-creature walker (m/s).
pub const WALK_SPEED: f32 = 4.5;
/// Charge: telegraph, dash and cooldown (s), dash speed (m/s).
pub const CHARGE_TELEGRAPH: f32 = 0.4;
pub const CHARGE_DASH: f32 = 0.6;
pub const CHARGE_COOLDOWN: f32 = 2.0;
pub const CHARGE_SPEED: f32 = 25.0;
/// DropFromSky: height above the aim point and shadow telegraph (s).
pub const DROP_HEIGHT: f32 = 12.0;
pub const DROP_TELEGRAPH: f32 = 0.5;
/// Trap / Grab / Constrict hold times (s). Trap is the Cage's 1.5 s (COMBAT.md §4).
pub const TRAP_SECS: f32 = 1.5;
pub const GRAB_SECS: f32 = 1.0;
pub const CONSTRICT_SECS: f32 = 1.5;
/// Transforms: one transformation (s); autonomous switch distances (m).
pub const XFORM_SECS: f32 = 0.9;
pub const XFORM_ENGAGE: f32 = 12.0;
pub const XFORM_DISENGAGE: f32 = 25.0;
/// Alt-mode drive speed cap (COMBAT.md §5d-bis: 24 m/s).
pub const XFORM_DRIVE_SPEED: f32 = 22.0;
/// Combiner sequence length (s).
pub const COMBINE_SECS: f32 = 2.0;
/// Live children (payload rounds, …) an emitter may have at once.
pub const MAX_EMITTED: usize = 3;

// ------------------------------------------------------------------------------------ state

/// What a Grab / Constrict / Trap construct is holding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HoldTarget {
    #[default]
    None,
    Player(PlayerId),
    Construct(ConstructId),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ChargePhase {
    #[default]
    Ready,
    Telegraph,
    Dash,
}

/// Transforms mode (COMBAT.md §5d-bis).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum XMode {
    /// Vehicle / animal.
    #[default]
    Alt,
    Robot,
}

/// Transforms component state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transform {
    pub mode: XMode,
    /// 0 = alt … 1 = robot (clients pose the panel sequence from it).
    pub progress: f32,
    pub want: XMode,
    /// Transformation in progress.
    pub busy: bool,
    /// Seconds in the current mode.
    pub since: f32,
    /// Transformed vehicle → robot in mid-air: the landing slams.
    pub slam: bool,
    /// This is a combined giant (one per player).
    pub combined: bool,
    /// Combining into this hub (the hub points at itself).
    pub hub: Option<ConstructId>,
    pub combine_t: f32,
    /// Limb slot while combining; the hub stores the part count.
    pub slot: u8,
    /// Robot punch cooldown.
    pub punch_cd: f32,
}

/// Per-construct component state (fixed size, no heap).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompState {
    /// Effective mechanics (recipe + form defaults), refreshed on init / graft.
    pub eff: MechSet,
    // --- chemistry / counters ---
    /// Seconds left frozen solid (brittle: the next hit shatters it).
    pub frozen: f32,
    /// Seconds left burning (non-fire constructs set alight; fire-element ones burn while not snuffed).
    pub burning: f32,
    /// A fire construct doused / frozen: its fire is out for good.
    pub snuffed: bool,
    /// Soaked (electric arcs through it).
    pub wet: f32,
    /// Electrified water: arcs to everything near it; credited to `shock_by`.
    pub electrified: f32,
    pub shock_by: Option<PlayerId>,
    pub shock_cd: f32,
    /// Explodes neutralised by water (a dud).
    pub dud: bool,
    /// Seconds left rooted in place (caged / trapped creature).
    pub rooted: f32,
    /// Seconds left pinned (slammed out of the sky / toppled kaiju).
    pub pinned: f32,
    // --- attachment ---
    /// Riding / held by another construct.
    pub attached_to: Option<ConstructId>,
    /// Pivot offset in the host's local frame.
    pub attach_offset: Vec3,
    pub attach_kind: Option<AttachKind>,
    // --- holds ---
    pub hold: HoldTarget,
    pub hold_t: f32,
    pub regrab_cd: f32,
    pub squeeze_cd: f32,
    /// Where a carrying flier takes its victim (toward the void).
    pub carry_dir: Vec3,
    // --- motion verbs ---
    pub charge: ChargePhase,
    pub charge_t: f32,
    pub charge_cd: f32,
    pub charge_dir: Vec3,
    pub launched: bool,
    pub dropped: bool,
    pub drop_t: f32,
    pub returning: bool,
    pub bounces: u8,
    pub bursts: u8,
    pub thrust_t: f32,
    /// Bomb fuse / sticky fuse (s, 0 = none).
    pub fuse: f32,
    pub rest_t: f32,
    /// Max fall speed since last supported (landing slams / stomps).
    pub fall: f32,
    /// Swarm split done.
    pub split: bool,
    /// A command phrase ("roll out", "combine") was checked.
    pub cmd_checked: bool,
    // --- emitters / area ---
    pub emit_cd: f32,
    pub beam_cd: f32,
    pub breath_t: f32,
    pub breath_on: bool,
    pub stomp_cd: f32,
    pub tail_cd: f32,
    pub ignite_cd: f32,
    // --- contact damage ---
    pub touched: HitList,
    pub touch_cd: f32,
    // --- Transforms ---
    pub xform: Transform,
}

// ------------------------------------------------------------------------------------ helpers

/// Effective mechanics: the recipe's plus what the form implies.
pub fn effective(r: &Recipe) -> MechSet {
    let mut s = MechSet::of(r);
    let mut add = |m: Mechanic| s.0 |= 1u64 << (m as u32 & 63);
    match r.form {
        Form::Rocket => {
            add(Mechanic::Homing);
            add(Mechanic::Thrust);
            add(Mechanic::Explodes);
        }
        Form::Bomb => add(Mechanic::Explodes),
        Form::Anvil => {
            add(Mechanic::DropFromSky);
            add(Mechanic::Heavy);
        }
        Form::Trampoline => add(Mechanic::Bounce),
        Form::Ramp => add(Mechanic::Launch),
        Form::Cage => add(Mechanic::Trap),
        Form::Wall | Form::Dome => add(Mechanic::Barrier),
        Form::Vehicle => add(Mechanic::Charge),
        _ => {}
    }
    // Features that imply verbs (bd_core: Rotor/Propeller/Jet → can fly, Turret/Barrels → Shoots).
    for f in &r.features {
        match f {
            Feature::Rotor | Feature::Propeller | Feature::Jet if r.form.category() != FormCategory::Structure => add(Mechanic::Flies),
            Feature::Turret | Feature::Barrels => add(Mechanic::Shoots),
            Feature::Drill | Feature::Saw => add(Mechanic::Spin),
            Feature::Spikes => add(Mechanic::Pierce),
            _ => {}
        }
    }
    s
}

#[inline]
pub fn eff_has(c: &Construct, m: Mechanic) -> bool {
    c.comp.eff.has(m)
}

/// Frozen solid (no actions; brittle).
#[inline]
pub fn is_frozen(c: &Construct) -> bool {
    c.comp.frozen > 0.0
}

/// Frozen, rooted or pinned: can't act this tick.
#[inline]
pub fn is_held_down(c: &Construct) -> bool {
    c.comp.frozen > 0.0 || c.comp.rooted > 0.0 || c.comp.pinned > 0.0 || c.comp.attached_to.is_some()
}

/// On fire (fire element not snuffed, or set alight).
#[inline]
pub fn on_fire(c: &Construct) -> bool {
    (c.recipe.element == Element::Fire && !c.comp.snuffed) || c.comp.burning > 0.0
}

/// (damage, knockback) multipliers of the physical traits (COMBAT.md §5).
pub fn mech_mult(c: &Construct) -> (f32, f32) {
    let mut dmg = 1.0;
    let mut kb = 1.0;
    if eff_has(c, Mechanic::Heavy) {
        dmg *= 1.5;
        kb *= 1.5;
    }
    if eff_has(c, Mechanic::Fast) {
        dmg *= 0.8;
    }
    if eff_has(c, Mechanic::Blunt) {
        kb *= 1.3;
    }
    (dmg, kb)
}

/// Speed multiplier of the physical traits (Fast ×1.4, Heavy ×0.8).
pub fn speed_mult(c: &Construct) -> f32 {
    let mut s = 1.0;
    if eff_has(c, Mechanic::Fast) {
        s *= 1.4;
    }
    if eff_has(c, Mechanic::Heavy) {
        s *= 0.8;
    }
    s
}

pub fn is_blade(c: &Construct) -> bool {
    eff_has(c, Mechanic::Blade) || matches!(c.recipe.form, Form::Sword | Form::Axe)
}

/// Pierce trait (spears, Pierce): goes through Guard, finds weak points.
pub fn is_pierce(c: &Construct) -> bool {
    eff_has(c, Mechanic::Pierce) || c.recipe.form == Form::Spear
}

/// Blunt trait (hammers, Blunt): knocks structures loose, craters.
pub fn is_blunt(c: &Construct) -> bool {
    eff_has(c, Mechanic::Blunt) || c.recipe.form == Form::Hammer
}

/// (blade, pierce, blunt) of an attacking construct.
pub fn hit_traits(c: &Construct) -> (bool, bool, bool) {
    (is_blade(c), is_pierce(c), is_blunt(c))
}

/// Projectile side effects from a construct's ray mechanics.
pub fn hit_effects(c: &Construct) -> u8 {
    let mut e = 0;
    if eff_has(c, Mechanic::Shrink) {
        e |= effects::SHRINK;
    }
    if eff_has(c, Mechanic::Grow) {
        e |= effects::GROW;
    }
    if eff_has(c, Mechanic::Link) {
        e |= effects::LINK;
    }
    if eff_has(c, Mechanic::Pull) {
        e |= effects::PULL;
    }
    if eff_has(c, Mechanic::Portal) {
        e |= effects::PORTAL;
    }
    e
}

/// Size factor relative to the form's default (big things move slower, hit wider).
fn size_k(c: &Construct) -> f32 {
    let spec = form_spec(c.recipe.form);
    (c.recipe.scale * c.size_scale / spec.default_scale.max(0.1)).clamp(0.2, 20.0)
}

/// Base contact damage / knockback of a construct (form table; structures and terrain bonk).
fn contact_base(c: &Construct) -> (f32, f32) {
    let spec = form_spec(c.recipe.form);
    let dmg = if spec.damage > 0.0 { spec.damage } else { 8.0 };
    let kb = if spec.knockback > 0.0 { spec.knockback } else { 12.0 };
    (dmg, kb)
}

/// Thrown-like free bodies (objects, free-flying held items, terrain lumps).
fn thrown_like(c: &Construct) -> bool {
    c.category == FormCategory::Object || c.model.is_some() || c.free_flier()
}

fn is_creature_ctrl(c: &Construct) -> bool {
    controller_of(c) == Controller::Creature
}

/// Run by another engineer's controller that owns all of its behaviour (held items — including
/// thrown ones out of the hand —, worn, suits).
fn player_owned(c: &Construct) -> bool {
    c.attached() || matches!(controller_of(c), Controller::Held | Controller::Worn | Controller::Suit)
}

/// Free (non-attached, non-piloted) and not creature-controlled: the generic verbs drive it.
fn generic_body(c: &Construct) -> bool {
    !player_owned(c) && c.pilot.is_none() && !is_creature_ctrl(c)
}

fn push_hit(out: &mut ConstructOut, c: &Construct, target: PlayerId, kind: HitKind, dmg: f32, kb: f32, dir: Vec3, point: Vec3, impact: f32) {
    out.player_hits.push(
        HitInfo::new(target, Some(c.owner), kind, dmg, kb, dir, point)
            .with_element(if c.comp.snuffed { Element::Light } else { c.recipe.element })
            .with_construct(c.id)
            .with_impact(impact),
    );
}

fn hostile(c: &Construct, p: &PlayerView) -> bool {
    p.id != c.owner && p.alive && !p.shielded
}

fn yaw_toward(c: &mut Construct, dir: Vec3, rate: f32, dt: f32) {
    let d = Vec3::new(dir.x, 0.0, dir.z);
    if d.length_squared() < 1e-6 {
        return;
    }
    let want = geom::yaw_rotation(d);
    c.rot = c.rot.slerp(want, (rate * dt).clamp(0.0, 1.0)).normalize();
}

// ------------------------------------------------------------------------------------ init / expiry

/// Called once when a construct is built.
pub fn init(c: &mut Construct) {
    c.comp.eff = effective(&c.recipe);
    c.body.loco = match c.category {
        FormCategory::Held if c.free_flier() => Loco::Fly,
        FormCategory::Held | FormCategory::Worn => Loco::Kinematic,
        FormCategory::Structure => Loco::Static,
        FormCategory::Creature => {
            if eff_has(c, Mechanic::Flies) {
                Loco::Fly
            } else {
                Loco::Walk
            }
        }
        FormCategory::Object => Loco::Ballistic,
    };
    c.body.restitution = if eff_has(c, Mechanic::Bounce) {
        0.95
    } else if eff_has(c, Mechanic::Ricochet) {
        0.85
    } else if c.recipe.form == Form::Ball {
        0.7
    } else {
        0.15
    };
    if c.vel.length_squared() > 0.01 {
        c.comp.launched = true;
    }
    // DropFromSky on anything that is not an object (the world already lifts objects).
    if eff_has(c, Mechanic::DropFromSky) && !c.attached() && c.category != FormCategory::Object && c.model.is_none() {
        c.pos.y += DROP_HEIGHT;
        c.prev_pos = c.pos;
    }
    c.comp.charge_cd = 0.5;
    c.comp.emit_cd = 0.8;
    c.comp.carry_dir = Vec3::ZERO;
}

/// Refresh derived state after a graft changed the recipe.
pub fn on_graft(c: &mut Construct) {
    let had_flies = eff_has(c, Mechanic::Flies);
    c.comp.eff = effective(&c.recipe);
    if eff_has(c, Mechanic::Bounce) {
        c.body.restitution = c.body.restitution.max(0.95);
    }
    if eff_has(c, Mechanic::Ricochet) {
        c.body.restitution = c.body.restitution.max(0.85);
    }
    if eff_has(c, Mechanic::Flies) && !had_flies && c.category == FormCategory::Structure {
        // A structure given wings lifts off its foundations.
        c.body.loose = false;
    }
    // Fire grafted onto something: it burns (and a doused thing can be relit).
    if c.recipe.element == Element::Fire {
        c.comp.snuffed = false;
    }
    if eff_has(c, Mechanic::Swarm) {
        c.comp.split = false;
    }
}

/// Death triggers (lifetime expiry): Explodes detonates (a black hole collapses).
pub fn on_expire(c: &mut Construct, out: &mut ConstructOut) {
    if eff_has(c, Mechanic::Explodes) && !c.comp.dud && c.forming_t <= 0.0 && !c.attached() {
        let k = if eff_has(c, Mechanic::Attract) { 1.5 } else { 1.0 };
        explosion(c, out, None, k);
    }
}

// ------------------------------------------------------------------------------------ explosions

/// Detonate `c` (does not mark it gone). `env` (when available) adds the trampoline combo:
/// players standing on a Bounce structure in range get launched ×1.5 upward (COMBAT.md §9).
pub fn explosion(c: &Construct, out: &mut ConstructOut, env: Option<&Env<'_>>, scale: f32) {
    if c.comp.dud {
        return;
    }
    let spec = form_spec(c.recipe.form);
    let (dm, km) = mech_mult(c);
    let base_r = if c.recipe.form == Form::Bomb { 4.0 } else { 3.0 };
    let atomic = if c.recipe.element == Element::Atomic { 1.3 } else { 1.0 };
    let r = base_r * size_k(c).sqrt().clamp(0.7, 3.0) * scale * atomic;
    let dmg = if spec.damage > 0.0 { spec.damage } else { 20.0 };
    let kb = spec.knockback.max(14.0) * km;
    let pos = c.center();
    out.explosions.push(Explosion {
        pos,
        radius: r,
        damage: dmg * dm,
        knockback: kb,
        source: Some(c.owner),
        construct: Some(c.id),
        element: c.recipe.element,
        carve_radius: r * 0.55,
        self_damage: false,
    });
    if c.recipe.element == Element::Fire || c.recipe.element == Element::Atomic {
        out.queue.push(Interaction::IgniteTerrain { pos, radius: (r * 0.6).min(3.0), by: Some(c.owner) });
    }
    let Some(env) = env else { return };
    for o in env.others {
        if !o.active || !(o.has(Mechanic::Bounce) || o.form == Form::Trampoline) || o.id == c.id {
            continue;
        }
        let ob = o.bounds();
        if geom::dist_sq_point_aabb(&ob, pos) > (r + 1.0) * (r + 1.0) {
            continue;
        }
        for p in env.ctx.players {
            if !p.alive {
                continue;
            }
            let feet = p.pos;
            let over = feet.x > ob.min.x - 0.3 && feet.x < ob.max.x + 0.3 && feet.z > ob.min.z - 0.3 && feet.z < ob.max.z + 0.3;
            if over && (feet.y - ob.max.y).abs() < 0.8 {
                out.impulses.push((p.id, Vec3::Y * kb * 0.5, 0.0));
            }
        }
    }
}

fn detonate(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    explosion(c, out, Some(env), 1.0);
    c.gone = Some(GoneReason::Spent);
}

/// Does contact set this construct off? (Attract+Explodes = black hole: only at end of life.)
fn contact_detonates(c: &Construct) -> bool {
    eff_has(c, Mechanic::Explodes) && !eff_has(c, Mechanic::Attract) && !c.comp.dud && c.comp.fuse <= 0.0
}

// ------------------------------------------------------------------------------------ drive (step 3a)

/// Pipeline step 3a: holds that override everything below `Attached` — being carried / stuck,
/// frozen / rooted / pinned, the DropFromSky telegraph, a transformation or combination in
/// progress, a Charge (telegraph / dash).
pub fn drive(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let dt = env.dt();
    tick_timers(c, dt);
    if player_owned(c) || c.forming_t > 0.0 {
        // Suits / held transformers: the mode machine still runs (the pilot toggles it).
        if c.forming_t <= 0.0 && eff_has(c, Mechanic::Transforms) {
            advance_transform(c, env, false);
        }
        return;
    }
    // Swarm: split on the first active tick.
    if eff_has(c, Mechanic::Swarm) && !c.comp.split {
        c.comp.split = true;
        swarm_split(c, out);
    }
    // Carried / stuck to another construct.
    if let Some(host) = c.comp.attached_to {
        match env.proxy(host).filter(|h| h.active) {
            Some(h) => {
                c.body.claim(Driver::Hold);
                c.body.loco = Loco::Kinematic;
                c.pos = h.center + h.rot * c.comp.attach_offset;
                c.vel = h.vel;
                if c.comp.attach_kind == Some(AttachKind::Stick) {
                    c.set_anim(AnimState::Stuck, env.tick());
                }
            }
            None => detach(c),
        }
    }
    // Stuck to a player (Sticky): ride them.
    if let Some(pid) = c.stuck_to {
        match env.ctx.player(pid).filter(|t| t.alive) {
            Some(t) => {
                c.body.claim(Driver::Hold);
                c.body.loco = Loco::Kinematic;
                c.pos = t.pos + c.stuck_offset;
                c.vel = t.vel;
                // A sticky rocket rides its target: its thrust drags them along.
                if eff_has(c, Mechanic::Thrust) && c.comp.thrust_t < 3.0 {
                    c.comp.thrust_t += dt;
                    out.impulses.push((pid, c.forward() * (26.0 * dt) + Vec3::Y * (14.0 * dt), 0.0));
                }
            }
            None => {
                c.stuck_to = None;
                c.body.loco = Loco::Ballistic;
            }
        }
        if c.comp.fuse > 0.0 {
            c.comp.fuse -= dt;
            if c.comp.fuse <= 0.0 && eff_has(c, Mechanic::Explodes) {
                detonate(c, env, out);
                return;
            }
        }
    }
    if c.comp.frozen > 0.0 {
        c.body.claim(Driver::Hold);
        c.vel.x = 0.0;
        c.vel.z = 0.0;
        if c.body.loco != Loco::Kinematic && c.body.loco != Loco::Static {
            c.body.loco = Loco::Ballistic;
        }
        c.set_anim(AnimState::Stuck, env.tick());
        return;
    }
    if c.comp.rooted > 0.0 || c.comp.pinned > 0.0 {
        c.body.claim(Driver::Hold);
        c.vel.x *= 0.5;
        c.vel.z *= 0.5;
        if c.comp.pinned > 0.0 && c.body.loco == Loco::Fly {
            // Slammed out of the sky.
            c.body.loco = Loco::Ballistic;
            c.body.gravity = 1.5;
        }
        if c.body.loco == Loco::Fly {
            c.vel.y *= 0.5;
        }
        c.set_anim(AnimState::Windup, env.tick());
    }
    // DropFromSky (non-objects): hang with a shadow telegraph, then fall.
    if eff_has(c, Mechanic::DropFromSky) && !c.comp.dropped && c.category != FormCategory::Object && c.model.is_none() {
        c.comp.drop_t += dt;
        if c.comp.drop_t < DROP_TELEGRAPH {
            c.body.claim(Driver::Hold);
            c.body.loco = Loco::Kinematic;
            c.vel = Vec3::ZERO;
            c.set_anim(AnimState::Telegraph, env.tick());
            return;
        }
        c.comp.dropped = true;
        c.body.loco = Loco::Ballistic;
        c.body.gravity = 1.4;
        c.vel = Vec3::new(0.0, -8.0, 0.0);
    }
    if eff_has(c, Mechanic::Transforms) {
        step_transform(c, env, out);
        if c.gone.is_some() {
            return;
        }
    }
    if eff_has(c, Mechanic::Charge) && generic_body(c) && !is_held_down(c) && c.recipe.form != Form::Vehicle {
        step_charge(c, env, out);
    }
}

fn tick_timers(c: &mut Construct, dt: f32) {
    let s = &mut c.comp;
    for t in [
        &mut s.frozen,
        &mut s.wet,
        &mut s.electrified,
        &mut s.shock_cd,
        &mut s.rooted,
        &mut s.pinned,
        &mut s.regrab_cd,
        &mut s.squeeze_cd,
        &mut s.charge_cd,
        &mut s.emit_cd,
        &mut s.beam_cd,
        &mut s.stomp_cd,
        &mut s.tail_cd,
        &mut s.ignite_cd,
        &mut s.touch_cd,
        &mut s.xform.punch_cd,
    ] {
        *t = (*t - dt).max(0.0);
    }
    if s.burning.is_finite() && s.burning > 0.0 {
        s.burning = (s.burning - dt).max(0.0);
    }
    if s.touch_cd <= 0.0 {
        s.touched.clear();
    }
    if s.pinned <= 0.0 && c.body.gravity > 1.0 && !s.dropped {
        c.body.gravity = 1.0;
    }
}

fn detach(c: &mut Construct) {
    c.comp.attached_to = None;
    c.comp.attach_kind = None;
    if c.body.loco == Loco::Kinematic {
        c.body.loco = match c.category {
            FormCategory::Structure => Loco::Ballistic,
            FormCategory::Creature => {
                if eff_has(c, Mechanic::Flies) {
                    Loco::Fly
                } else {
                    Loco::Walk
                }
            }
            _ => Loco::Ballistic,
        };
        if c.category == FormCategory::Structure {
            c.body.loose = true;
        }
    }
}

/// Charge (generic): periodic telegraphed dash along the facing toward the target, rams.
fn step_charge(c: &mut Construct, env: &Env<'_>, _out: &mut ConstructOut) {
    let dt = env.dt();
    match c.comp.charge {
        ChargePhase::Ready => {
            if c.comp.charge_cd > 0.0 || !c.body.free(Driver::Charge) {
                return;
            }
            let Some(t) = env.target_for(c) else { return };
            let to = t.pos - c.pos;
            let dh = Vec3::new(to.x, 0.0, to.z).length();
            if (3.0..24.0).contains(&dh) {
                c.comp.charge = ChargePhase::Telegraph;
                c.comp.charge_t = 0.0;
                c.comp.charge_dir = Vec3::new(to.x, 0.0, to.z).normalize_or(c.forward());
                c.comp.touched.clear();
                c.set_anim(AnimState::Telegraph, env.tick());
            }
        }
        ChargePhase::Telegraph => {
            c.comp.charge_t += dt;
            if c.body.claim(Driver::Charge) {
                c.vel.x = 0.0;
                c.vel.z = 0.0;
                yaw_toward(c, c.comp.charge_dir, 12.0, dt);
                if c.body.loco == Loco::Static || c.body.loco == Loco::Kinematic {
                    c.body.loco = Loco::Walk;
                }
            }
            if c.comp.charge_t >= CHARGE_TELEGRAPH {
                c.comp.charge = ChargePhase::Dash;
                c.comp.charge_t = 0.0;
                c.set_anim(AnimState::Active, env.tick());
            }
        }
        ChargePhase::Dash => {
            c.comp.charge_t += dt;
            if c.body.claim(Driver::Charge) {
                let v = c.comp.charge_dir * CHARGE_SPEED * speed_mult(c);
                c.vel.x = v.x;
                c.vel.z = v.z;
                if c.body.loco == Loco::Static || c.body.loco == Loco::Kinematic {
                    c.body.loco = Loco::Walk;
                }
                if c.body.loco == Loco::Fly {
                    c.vel.y = 0.0;
                }
            }
            if c.comp.charge_t >= CHARGE_DASH {
                c.comp.charge = ChargePhase::Ready;
                c.comp.charge_cd = CHARGE_COOLDOWN * temperament::cooldown_mult(c);
                c.set_anim(AnimState::Idle, env.tick());
                c.vel.x *= 0.2;
                c.vel.z *= 0.2;
            }
        }
    }
}

fn dashing(c: &Construct) -> bool {
    c.comp.charge == ChargePhase::Dash
}

// ------------------------------------------------------------------------------------ steer (step 3b)

/// Pipeline step 3b: steering components on generic bodies (not creatures / attached / piloted).
pub fn steer(c: &mut Construct, env: &Env<'_>, _out: &mut ConstructOut) {
    if !generic_body(c) || !c.active() || is_held_down(c) || c.stuck_to.is_some() {
        return;
    }
    let dt = env.dt();
    // SlowFall / Glide on a free body: it drifts down / glides.
    if eff_has(c, Mechanic::SlowFall) || eff_has(c, Mechanic::Glide) {
        c.body.gravity = c.body.gravity.min(0.25);
        let cap = if eff_has(c, Mechanic::SlowFall) { 3.0 } else { 5.0 };
        if c.vel.y < -cap {
            let excess = -cap - c.vel.y;
            c.vel.y = -cap;
            if eff_has(c, Mechanic::Glide) {
                let f = Vec3::new(c.vel.x, 0.0, c.vel.z).normalize_or(c.forward());
                c.vel += f * excess * 0.8;
            }
        }
    }
    // Returning: after contact / timeout, back to the owner (caught in `react`).
    let thrown = thrown_like(c);
    if eff_has(c, Mechanic::Returning) && thrown && c.comp.launched && (c.comp.returning || c.age > RETURN_AFTER + c.forming_t) {
        c.comp.returning = true;
        if c.body.claim(Driver::Steer) {
            let home = env.owner.hand;
            c.vel = (home - c.center()).normalize_or_zero() * RETURN_SPEED;
            c.body.loco = Loco::Fly;
            c.set_anim(AnimState::Returning, env.tick());
            face_velocity(c, dt);
        }
        return;
    }
    // Transforms locomotion (vehicle drive / robot stride).
    if eff_has(c, Mechanic::Transforms) && c.comp.xform.hub.is_none() {
        xform_locomotion(c, env);
        return;
    }
    let flies = eff_has(c, Mechanic::Flies);
    let homing = eff_has(c, Mechanic::Homing);
    let thrust = eff_has(c, Mechanic::Thrust);
    if flies && c.body.claim(Driver::Steer) {
        fly_hunt(c, env);
    } else if homing && c.body.free(Driver::Steer) {
        if c.category == FormCategory::Object || c.body.loco == Loco::Fly || c.comp.launched && c.body.loco == Loco::Ballistic && !c.body.supported {
            // In flight: turn-rate-limited steering toward the target.
            if let Some(t) = env.target_for(c) {
                c.body.claim(Driver::Steer);
                let want = (t.hull.center() - c.center()).normalize_or_zero();
                let speed = c.vel.length().max(if thrust { 20.0 } else { 14.0 });
                let cur = c.vel.normalize_or(want);
                let turn = (HOMING_TURN * speed_mult(c) * dt).min(1.0);
                let nd = (cur + (want - cur) * turn).normalize_or(cur);
                c.vel = nd * speed;
                if !thrust {
                    // Homing holds its line against gravity (it is steering, not falling).
                    c.body.gravity = c.body.gravity.min(0.2);
                }
                face_velocity(c, dt);
            }
        } else if c.body.claim(Driver::Steer) {
            // On the ground: slide toward the target (a homing trampoline slides under them).
            slide_to_target(c, env);
        }
    }
    if thrust && !flies {
        // Rocket propulsion: objects along their facing, structures / terrain lift off (rockets
        // on a pillar launch it). Burns for 2.5 s.
        if c.comp.thrust_t < 2.5 && c.body.free(Driver::Steer) {
            c.comp.thrust_t += dt;
            let up_thrust = c.category != FormCategory::Object;
            if up_thrust {
                c.body.loose = true;
                c.body.loco = Loco::Ballistic;
                c.vel += Vec3::Y * (34.0 * dt) + c.forward() * (6.0 * dt);
            } else {
                c.body.gravity = 0.0;
                let f = c.vel.normalize_or(c.forward());
                let s = (c.vel.length() + 30.0 * dt).min(38.0);
                c.vel = f * s;
            }
            c.set_anim(AnimState::Move, env.tick());
        } else if c.comp.thrust_t >= 2.5 && c.category == FormCategory::Object {
            c.body.gravity = 1.0;
        }
    }
}

fn face_velocity(c: &mut Construct, dt: f32) {
    if c.vel.length_squared() > 1.0 {
        let want = geom::look_rotation(c.vel);
        c.rot = c.rot.slerp(want, (10.0 * dt).min(1.0)).normalize();
    }
}

/// Flies: self-propelled, hunts its target and bumps it; carries a held victim toward the void;
/// with no target it hovers near its owner.
fn fly_hunt(c: &mut Construct, env: &Env<'_>) {
    let dt = env.dt();
    c.body.loco = Loco::Fly;
    c.body.loose = false;
    let big = size_k(c).max(1.0).powf(-0.25);
    let speed = FLY_SPEED * speed_mult(c) * temperament::speed_mult(c) * big;
    let here = c.center();
    let desired = if let HoldTarget::Player(_) = c.comp.hold {
        // Carrying someone: up and out over the void.
        let d = if c.comp.carry_dir == Vec3::ZERO { Vec3::Y } else { c.comp.carry_dir };
        (d + Vec3::Y * 0.35).normalize_or(Vec3::Y) * (speed * 1.2)
    } else {
        let target = env.target_for(c);
        c.target = target.map(|t| t.id);
        let goal = match target.as_ref() {
            Some(t) => {
                let g = temperament::steer_goal(c, env, Some(t), t.hull.center());
                // After a bonk it backs off briefly before diving in again.
                if c.comp.touch_cd > 0.2 { g + (here - g).normalize_or_zero() * 4.0 + Vec3::Y * 2.0 } else { g }
            }
            None => env.owner.pos + Vec3::new(1.5, 3.0, 1.5),
        };
        let to = goal - here;
        let dist = to.length();
        let s = if target.is_none() { (dist * 2.0).min(speed) } else { speed };
        to.normalize_or_zero() * s
    };
    // Look ahead only up to the goal (a target standing on the ground must not read as "terrain
    // ahead, climb").
    let ahead = if let HoldTarget::Player(_) = c.comp.hold { 3.0 } else { (c.target.and_then(|t| env.ctx.player(t)).map_or(3.0, |t| (t.hull.center() - here).length() - 0.5)).clamp(0.0, 3.0) };
    let desired = if ahead > 0.3 { super::ai::steer_air(env.ctx.voxels, here, desired, c.half.min_element().max(0.3), ahead, 0.8) } else { desired };
    let blend = (4.0 * dt).min(1.0);
    c.vel += (desired - c.vel) * blend;
    face_velocity(c, dt);
    c.set_anim(AnimState::Move, env.tick());
}

/// Ground slide toward the target (homing structures / grounded homing things).
fn slide_to_target(c: &mut Construct, env: &Env<'_>) {
    let dt = env.dt();
    let Some(t) = env.target_for(c) else {
        c.vel.x *= 0.8;
        c.vel.z *= 0.8;
        return;
    };
    c.target = Some(t.id);
    let to = t.pos - c.pos;
    let dh = Vec3::new(to.x, 0.0, to.z);
    let speed = 8.0 * speed_mult(c) * temperament::speed_mult(c);
    let st = super::ai::steer_ground(env.ctx.voxels, c.pos, dh, 3.0, 1.0, 3.0, false, 1.0);
    let want = if dh.length() < 0.3 { Vec3::ZERO } else { st.dir * speed.min(dh.length() * 4.0) };
    let blend = (5.0 * dt).min(1.0);
    c.vel.x += (want.x - c.vel.x) * blend;
    c.vel.z += (want.z - c.vel.z) * blend;
    c.body.loco = Loco::Walk;
    c.set_anim(AnimState::Move, env.tick());
}

/// [`super::StepFn`] for [`Controller::Walker`]: a non-creature that grew legs ("wall with legs",
/// a tree with legs from terrain). Lumbers toward its target avoiding cliffs; bumps what it meets
/// (contact hits in [`react`]); Stomp makes its steps shake the ground.
pub fn step_walker(c: &mut Construct, env: &Env<'_>, _out: &mut ConstructOut) {
    if !c.active() {
        if c.category == FormCategory::Object {
            // Forming in the hand like any object.
            c.body.loco = Loco::Kinematic;
            c.pos = env.owner.hand + env.owner.look * (c.half.max_element() + 0.3);
        }
        return;
    }
    c.body.loco = Loco::Walk;
    c.body.loose = false;
    if !c.body.claim(Driver::Steer) {
        return;
    }
    walk_toward_target(c, env, WALK_SPEED);
}

/// Shared walking behaviour (walkers, robot mode, terrain with legs).
fn walk_toward_target(c: &mut Construct, env: &Env<'_>, base_speed: f32) {
    let dt = env.dt();
    let target = env.target_for(c);
    c.target = target.map(|t| t.id);
    let goal = match target.as_ref() {
        Some(t) => temperament::steer_goal(c, env, Some(t), t.pos),
        None => env.owner.pos + Vec3::new(2.0, 0.0, 2.0),
    };
    let to = goal - c.pos;
    let dh = Vec3::new(to.x, 0.0, to.z);
    let stop = c.half.x.max(c.half.z) * 0.5 + 0.6;
    let speed = base_speed * speed_mult(c) * temperament::speed_mult(c) * size_k(c).clamp(0.5, 4.0).powf(0.25);
    let look = (c.half.x.max(c.half.z) + 1.5).min(8.0);
    let st = super::ai::steer_ground(env.ctx.voxels, c.pos, dh, look, (c.half.y * 0.5).clamp(0.6, 4.0), 3.0, false, if c.id.0 % 2 == 0 { 1.0 } else { -1.0 });
    let want = if dh.length() <= stop { Vec3::ZERO } else { st.dir * speed };
    let blend = (4.0 * dt).min(1.0);
    c.vel.x += (want.x - c.vel.x) * blend;
    c.vel.z += (want.z - c.vel.z) * blend;
    if want != Vec3::ZERO {
        yaw_toward(c, want, 3.0, dt);
        c.set_anim(AnimState::Move, env.tick());
    } else {
        if let Some(t) = target.as_ref() {
            yaw_toward(c, t.pos - c.pos, 3.0, dt);
        }
        c.set_anim(AnimState::Idle, env.tick());
    }
}

// ------------------------------------------------------------------------------------ react (step 5)

/// Pipeline step 5: contact reactions, holds, emitters, area verbs.
pub fn react(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    if player_owned(c) || !c.active() || c.gone.is_some() {
        return;
    }
    let dt = env.dt();
    let creature = is_creature_ctrl(c);
    // Contacts (copy: the list is small and fixed-size).
    let contacts = c.body.contacts;
    for k in contacts.as_slice() {
        if c.gone.is_some() {
            return;
        }
        match k.with {
            ContactWith::Player(pid) => {
                let Some(p) = env.ctx.player(pid).copied() else { continue };
                if pid == c.owner {
                    // Caught on the way back.
                    if c.comp.returning && thrown_like(c) {
                        c.gone = Some(GoneReason::Spent);
                        return;
                    }
                    continue;
                }
                if hostile(c, &p) {
                    on_player_contact(c, env, out, &p, k.point, k.normal, k.speed, creature);
                }
            }
            ContactWith::Terrain => on_terrain_contact(c, env, out, k.point, k.normal, k.speed),
            ContactWith::Construct(o) => on_construct_contact(c, env, out, o, k.point, k.normal, k.speed),
        }
    }
    if c.gone.is_some() {
        return;
    }
    // Exploding boomerang on the way back: a proximity burst at the first enemy it passes.
    if c.comp.returning && eff_has(c, Mechanic::Returning) && eff_has(c, Mechanic::Explodes) && c.comp.bursts < 3 && !creature {
        let here = c.center();
        let near = env.ctx.enemies_of(c.owner).find(|p| geom::dist_sq_point_aabb(&p.hull, here) < 1.8 * 1.8 && !c.comp.touched.contains(player_key(p.id))).map(|p| p.id);
        if let Some(pid) = near {
            c.comp.bursts += 1;
            c.comp.touched.insert(player_key(pid));
            c.comp.touch_cd = TOUCH_REHIT;
            explosion(c, out, Some(env), 1.0);
        }
    }
    // Returning objects near the owner's hand are caught.
    if c.comp.returning && (c.center() - env.owner.hand).length() < 1.5 && thrown_like(c) {
        c.gone = Some(GoneReason::Spent);
        return;
    }
    // Bomb fuse.
    if c.comp.fuse > 0.0 && c.stuck_to.is_none() {
        c.comp.fuse -= dt;
        if c.comp.fuse <= 0.0 && eff_has(c, Mechanic::Explodes) {
            detonate(c, env, out);
            return;
        }
    }
    // Fall tracking and landing slams.
    if c.body.supported || c.grounded {
        if c.comp.xform.slam {
            c.comp.xform.slam = false;
            let fall = c.comp.fall;
            stomp(c, env, out, (1.0 + fall / 10.0).clamp(1.0, 3.0), true);
        }
        c.comp.fall = 0.0;
    } else {
        c.comp.fall = c.comp.fall.max(-c.vel.y);
    }
    if is_frozen(c) {
        return;
    }
    update_hold(c, env, out);
    if !creature && c.pilot.is_none() && !is_held_down(c) {
        trap_check(c, env, out);
        emitters(c, env, out);
        area_verbs(c, env, out);
    }
    if on_fire(c) && c.body.contacts.as_slice().iter().any(|k| k.with == ContactWith::Terrain) {
        if c.comp.ignite_cd <= 0.0 {
            c.comp.ignite_cd = 0.3;
            out.queue.push(Interaction::IgniteTerrain { pos: c.pos, radius: 1.0, by: Some(c.owner) });
        }
    }
    // Objects that came to rest expire shortly (unless something keeps them busy).
    let lingers = eff_has(c, Mechanic::Flies) || eff_has(c, Mechanic::Walks) || eff_has(c, Mechanic::Attract) || eff_has(c, Mechanic::Repel) || eff_has(c, Mechanic::Transforms);
    if c.category == FormCategory::Object && c.comp.launched && !lingers && c.stuck_to.is_none() && c.comp.attached_to.is_none() {
        if c.body.supported && c.vel.length_squared() < 0.25 {
            c.comp.rest_t += dt;
            if c.comp.rest_t > 1.0 && !eff_has(c, Mechanic::Transforms) {
                c.life = c.life.min(c.age + 1.0);
            }
        } else {
            c.comp.rest_t = 0.0;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn on_player_contact(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, p: &PlayerView, point: Vec3, normal: Vec3, speed: f32, creature: bool) {
    let rel = (c.vel - p.vel).length();
    // Only the construct's own approach counts (running into a wall doesn't make the wall hit you).
    let approach = c.vel.dot(-normal).max(0.0);
    let closing = speed.min(approach + 1.0).max(approach);
    let spinning = eff_has(c, Mechanic::Spin);
    let walking = c.body.loco == Loco::Walk && Vec3::new(c.vel.x, 0.0, c.vel.z).length() > 1.5;
    let moving = closing >= 5.0 || dashing(c) || spinning || walking || (c.body.loco == Loco::Fly && rel > 2.5) || (c.comp.xform.mode == XMode::Alt && eff_has(c, Mechanic::Transforms) && rel > 4.0);
    if !moving {
        return;
    }
    // Sticky: attach to the first thing it touches and ride it.
    if eff_has(c, Mechanic::Sticky) && c.stuck_to.is_none() && c.comp.attached_to.is_none() && !creature {
        c.stuck_to = Some(p.id);
        c.stuck_offset = point - p.pos;
        c.comp.touched.insert(player_key(p.id));
        if eff_has(c, Mechanic::Explodes) && c.comp.fuse <= 0.0 {
            c.comp.fuse = 1.0;
        }
        c.set_anim(AnimState::Stuck, env.tick());
        return;
    }
    if !c.comp.touched.insert(player_key(p.id)) {
        return;
    }
    c.comp.touch_cd = if spinning { 0.3 } else { TOUCH_REHIT };
    // Exploding boomerang: craters on the way out and on the way back (≤ 3 bursts).
    if eff_has(c, Mechanic::Returning) && eff_has(c, Mechanic::Explodes) && !creature {
        if c.comp.bursts < 3 {
            c.comp.bursts += 1;
            explosion(c, out, Some(env), 1.0);
        }
        c.comp.returning = true;
        return;
    }
    if contact_detonates(c) && !creature {
        detonate(c, env, out);
        return;
    }
    if (eff_has(c, Mechanic::Grab) || eff_has(c, Mechanic::Constrict)) && !creature && c.comp.hold == HoldTarget::None && c.comp.regrab_cd <= 0.0 {
        c.comp.hold = HoldTarget::Player(p.id);
        c.comp.hold_t = if eff_has(c, Mechanic::Constrict) { CONSTRICT_SECS } else { GRAB_SECS };
        c.comp.carry_dir = void_dir(env, c.center(), c.owner);
        c.set_anim(AnimState::Active, env.tick());
        return;
    }
    if creature {
        // Creatures deal their own damage; only the generic reactions above apply.
        return;
    }
    let (base_d, base_k) = contact_base(c);
    let (dm, km) = mech_mult(c);
    let falling = c.vel.y < -6.0 && (eff_has(c, Mechanic::DropFromSky) || c.body.loose);
    let kind = if falling || (c.recipe.form == Form::Boulder && closing > 6.0) { HitKind::Crush } else { HitKind::Thrown };
    let mut dir = if c.vel.length_squared() > 1.0 { c.vel } else { -normal };
    if spinning {
        // Tangential flick.
        let r = p.hull.center() - c.center();
        dir = Vec3::Y.cross(r).normalize_or(dir) + r.normalize_or_zero() * 0.5;
    }
    let impact = if kind == HitKind::Crush { 1.6 } else { 1.0 + (closing / 30.0).min(0.8) };
    push_hit(out, c, p.id, kind, base_d * dm, base_k * km, dir, point, impact);
    // Pull: yank the victim toward the owner.
    if eff_has(c, Mechanic::Pull) {
        let to = (env.owner.pos - p.pos).normalize_or_zero();
        out.impulses.push((p.id, to * 16.0 + Vec3::Y * 4.0, 0.0));
    }
    // Bounce: the victim is flung back off it.
    if eff_has(c, Mechanic::Bounce) {
        out.impulses.push((p.id, -normal * 12.0 + Vec3::Y * 5.0, 0.0));
    }
    // What the body does after the hit.
    if c.body.loco == Loco::Fly && !eff_has(c, Mechanic::Pierce) {
        // Bonk: rebound away and up, then dive again.
        c.vel = normal * 7.0 + Vec3::Y * 4.0;
    } else if eff_has(c, Mechanic::Ricochet) && c.comp.bounces < 4 {
        c.comp.bounces += 1;
        reaim_ricochet(c, env, Some(p.id));
    } else if eff_has(c, Mechanic::Returning) && thrown_like(c) {
        c.comp.returning = true;
    } else if c.category == FormCategory::Object && !eff_has(c, Mechanic::Pierce) && !dashing(c) && c.recipe.form != Form::Vehicle && c.recipe.form != Form::Boulder {
        c.vel *= 0.3;
    }
}

fn on_terrain_contact(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, point: Vec3, normal: Vec3, speed: f32) {
    let creature = is_creature_ctrl(c);
    if speed < 2.0 && !(eff_has(c, Mechanic::Sticky) && speed > 0.5) {
        return;
    }
    if eff_has(c, Mechanic::Sticky) && c.stuck_to.is_none() && c.comp.attached_to.is_none() && !creature && c.comp.launched {
        c.vel = Vec3::ZERO;
        c.body.loco = Loco::Kinematic;
        c.pos += normal * 0.02;
        c.set_anim(AnimState::Stuck, env.tick());
        if eff_has(c, Mechanic::Explodes) && c.comp.fuse <= 0.0 {
            c.comp.fuse = 1.0;
        }
        return;
    }
    if eff_has(c, Mechanic::Returning) && eff_has(c, Mechanic::Explodes) && !creature && speed > 4.0 && c.comp.launched {
        if c.comp.bursts < 3 {
            c.comp.bursts += 1;
            explosion(c, out, Some(env), 1.0);
        }
        c.comp.returning = true;
        return;
    }
    if contact_detonates(c) && !creature && speed > 3.0 && (c.comp.launched || c.comp.dropped || c.category != FormCategory::Object) {
        detonate(c, env, out);
        return;
    }
    // Heavy impacts crater (Blunt / Heavy / DropFromSky).
    let crater = (eff_has(c, Mechanic::DropFromSky) && speed > 8.0) || ((eff_has(c, Mechanic::Heavy) || eff_has(c, Mechanic::Blunt)) && speed > 14.0);
    if crater && normal.y > 0.5 {
        let rad = (1.0 + c.recipe.scale * c.size_scale * 0.3).min(if c.colossal { 30.0 } else { 4.0 }) * if c.recipe.element == Element::Earth { 1.26 } else { 1.0 };
        out.carves.push(CarveRequest { center: point, radius: rad, power: 0.9 });
        if eff_has(c, Mechanic::Stomp) || c.colossal {
            stomp(c, env, out, 1.2, false);
        }
        if c.category == FormCategory::Object {
            // Landed: an obstacle for a moment (anvils on ramps slide on, though).
            c.life = c.life.min(c.age + 2.0);
        }
    } else if eff_has(c, Mechanic::Stomp) && speed > 8.0 && normal.y > 0.5 && !creature {
        stomp(c, env, out, 1.0, false);
    }
    if eff_has(c, Mechanic::Ricochet) && c.comp.bounces < 4 && speed > 3.0 && !creature {
        c.comp.bounces += 1;
        reaim_ricochet(c, env, None);
    }
    if eff_has(c, Mechanic::Returning) && c.comp.launched && thrown_like(c) {
        c.comp.returning = true;
    }
}

fn on_construct_contact(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, oid: ConstructId, point: Vec3, _normal: Vec3, speed: f32) {
    let Some(o) = env.proxy(oid).copied() else { return };
    let creature = is_creature_ctrl(c);
    let enemy = o.owner != c.owner;
    // Sticky: sticks to whatever it touches first (any construct), and rides it.
    if eff_has(c, Mechanic::Sticky) && c.stuck_to.is_none() && c.comp.attached_to.is_none() && !creature && c.comp.launched && speed > 1.0 {
        let offset = o.rot.inverse() * (c.pos - o.center);
        out.queue.push(Interaction::Attach { id: c.id, to: o.id, offset, kind: AttachKind::Stick });
        c.comp.attached_to = Some(o.id);
        c.comp.attach_offset = offset;
        c.comp.attach_kind = Some(AttachKind::Stick);
        if eff_has(c, Mechanic::Explodes) && c.comp.fuse <= 0.0 {
            c.comp.fuse = 1.0;
        }
        return;
    }
    if !enemy {
        return;
    }
    if contact_detonates(c) && !creature && speed > 3.0 {
        detonate(c, env, out);
        return;
    }
    // Cage / Trap around a creature: rooted in place, thrashing; a kaiju cage topples it.
    if eff_has(c, Mechanic::Trap) && o.category == FormCategory::Creature && !creature {
        let kaiju = o.form == Form::Kaiju;
        out.queue.push(Interaction::Root { id: o.id, secs: if kaiju { 2.0 } else { 2.5 }, pinned: kaiju, by: c.id });
    }
    // Grab / Constrict a free construct: carry it (you steer the grabbed creature).
    if (eff_has(c, Mechanic::Grab) || eff_has(c, Mechanic::Constrict)) && !creature && c.comp.hold == HoldTarget::None && c.comp.regrab_cd <= 0.0 && o.free && o.mass <= body::body_mass(c) * 4.0 {
        let offset = c.rot.inverse() * (o.center - c.center());
        let offset = offset.clamp_length_max(c.half.max_element() + o.half.max_element());
        out.queue.push(Interaction::Attach { id: o.id, to: c.id, offset, kind: AttachKind::Hold });
        c.comp.hold = HoldTarget::Construct(o.id);
        c.comp.hold_t = 3.0;
    }
    // Blade through a structure: sliced clean in two.
    if is_blade(c) && o.category == FormCategory::Structure && speed > 5.0 && !creature {
        let n = o.rot * Vec3::X;
        out.queue.push(Interaction::Slice { id: o.id, point, normal: n, by: Some(c.owner) });
    }
    // Heavy / DropFromSky on a flier: slammed out of the sky and pinned.
    if (eff_has(c, Mechanic::Heavy) || eff_has(c, Mechanic::DropFromSky)) && o.has(Mechanic::Flies) && c.vel.y < -4.0 {
        out.queue.push(Interaction::Root { id: o.id, secs: 1.5, pinned: true, by: c.id });
    }
    // Shrink / Grow rays on constructs.
    if eff_has(c, Mechanic::Shrink) {
        out.queue.push(Interaction::Resize { id: o.id, factor: crate::space::SHRINK_SCALE });
    } else if eff_has(c, Mechanic::Grow) {
        out.queue.push(Interaction::Resize { id: o.id, factor: crate::space::GROW_SCALE });
    }
    if eff_has(c, Mechanic::Ricochet) && c.comp.bounces < 4 && !creature && speed > 3.0 {
        c.comp.bounces += 1;
        reaim_ricochet(c, env, None);
    }
    if eff_has(c, Mechanic::Returning) && c.comp.launched && thrown_like(c) {
        c.comp.returning = true;
    }
}

/// Ricochet: after bouncing, snap toward an enemy within 25° of the new heading.
fn reaim_ricochet(c: &mut Construct, env: &Env<'_>, skip: Option<PlayerId>) {
    let speed = c.vel.length().max(12.0);
    let dir = c.vel.normalize_or(c.forward());
    let here = c.center();
    let cos = 25f32.to_radians().cos();
    let mut best: Option<(Vec3, f32)> = None;
    for p in env.ctx.enemies_of(c.owner) {
        if Some(p.id) == skip {
            continue;
        }
        let to = p.hull.center() - here;
        let d = to.length();
        if d < 0.5 || d > 30.0 {
            continue;
        }
        let u = to / d;
        if u.dot(dir) >= cos && best.is_none_or(|b| d < b.1) && super::ai::line_of_sight(env.ctx.voxels, here, p.hull.center()) {
            best = Some((u, d));
        }
    }
    if let Some((u, _)) = best {
        c.vel = u * speed;
    }
}

/// Direction from `from` toward the nearest void (no ground within 40 m below), probing 8
/// headings at 8–32 m. Falls back to "away from the owner".
pub fn void_dir(env: &Env<'_>, from: Vec3, owner: PlayerId) -> Vec3 {
    let v = env.ctx.voxels;
    for d in [8.0f32, 16.0, 24.0, 32.0] {
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::FRAC_PI_4;
            let dir = Vec3::new(a.cos(), 0.0, a.sin());
            let q = from + dir * d;
            if body::void_under(v, q + Vec3::Y * 4.0, 60.0) {
                return dir;
            }
        }
    }
    let o = env.ctx.player(owner).map_or(from, |p| p.pos);
    Vec3::new(from.x - o.x, 0.0, from.z - o.z).normalize_or(Vec3::X)
}

// ------------------------------------------------------------------------------------ holds

fn update_hold(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let dt = env.dt();
    match c.comp.hold {
        HoldTarget::None => {}
        HoldTarget::Player(pid) => {
            let Some(p) = env.ctx.player(pid).filter(|p| p.alive).copied() else {
                release(c, None);
                return;
            };
            c.comp.hold_t -= dt;
            let trap = eff_has(c, Mechanic::Trap);
            let point = if trap { c.center() - Vec3::Y * (c.half.y * 0.3) } else { c.center() + c.forward() * (c.half.z + 0.5) };
            let hc = p.hull.center();
            let dv = ((point - hc) * 10.0 + c.vel - p.vel).clamp_length_max(40.0);
            out.impulses.push((pid, dv, 0.0));
            if eff_has(c, Mechanic::Constrict) && c.comp.squeeze_cd <= 0.0 {
                c.comp.squeeze_cd = 0.5;
                push_hit(out, c, pid, HitKind::Creature, 4.0, 0.0, Vec3::Y, hc, 0.5);
            }
            if c.comp.hold_t <= 0.0 {
                // Grab throws; Trap / Constrict just let go.
                let fling = if eff_has(c, Mechanic::Grab) && !trap {
                    let f = c.forward();
                    Some(Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::X) * 16.0 + Vec3::Y * 8.0)
                } else {
                    None
                };
                if let Some(v) = fling {
                    out.impulses.push((pid, v, 0.3));
                }
                release(c, None);
            }
        }
        HoldTarget::Construct(oid) => {
            c.comp.hold_t -= dt;
            let alive = env.proxy(oid).is_some_and(|o| o.active);
            if !alive {
                release(c, None);
            } else if c.comp.hold_t <= 0.0 {
                let f = c.forward();
                out.queue.push(Interaction::Release { holder: c.id, vel: f * 14.0 + Vec3::Y * 6.0 });
                release(c, None);
            }
        }
    }
}

fn release(c: &mut Construct, _why: Option<()>) {
    c.comp.hold = HoldTarget::None;
    c.comp.hold_t = 0.0;
    c.comp.regrab_cd = 1.5;
    c.comp.carry_dir = Vec3::ZERO;
}

/// Trap (cage-like): snaps shut around an enemy it can enclose.
fn trap_check(c: &mut Construct, env: &Env<'_>, _out: &mut ConstructOut) {
    if !eff_has(c, Mechanic::Trap) || c.comp.hold != HoldTarget::None || c.comp.regrab_cd > 0.0 {
        return;
    }
    let center = c.center();
    let inv = c.rot.inverse();
    for p in env.ctx.enemies_of(c.owner) {
        let hc = p.hull.center();
        let l = inv * (hc - center);
        let room = c.half + Vec3::splat(0.2);
        if l.x.abs() <= room.x && l.y.abs() <= room.y + 0.4 && l.z.abs() <= room.z {
            c.comp.hold = HoldTarget::Player(p.id);
            c.comp.hold_t = TRAP_SECS;
            c.comp.carry_dir = if eff_has(c, Mechanic::Flies) { void_dir(env, center, c.owner) } else { Vec3::ZERO };
            c.set_anim(AnimState::Active, env.tick());
            return;
        }
    }
}

// ------------------------------------------------------------------------------------ emitters

/// Shoots / Beam / Breath from the construct's front, at its target. Payload recipes (depth 1)
/// are fired as child constructs ("a wall that shoots anvils", "a tree breathing bees").
fn emitters(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let shoots = eff_has(c, Mechanic::Shoots);
    let beam = eff_has(c, Mechanic::Beam);
    let breath = eff_has(c, Mechanic::Breath);
    if !(shoots || beam || breath) {
        return;
    }
    // Transformers only fire in robot mode.
    if eff_has(c, Mechanic::Transforms) && c.comp.xform.mode != XMode::Robot {
        return;
    }
    let Some(t) = env.target_for(c) else {
        c.comp.breath_on = false;
        return;
    };
    let dt = env.dt();
    let muzzle = c.center() + c.forward() * (c.half.z + 0.3);
    let aim = t.hull.center() - muzzle;
    let dist = aim.length();
    // Static emitters turn to face their target (a turret).
    if matches!(c.body.loco, Loco::Static | Loco::Kinematic) || c.body.driver <= Driver::Steer {
        yaw_toward(c, aim, 2.5, dt);
    }
    let los = super::ai::line_of_sight(env.ctx.voxels, muzzle, t.hull.center());
    if shoots && c.comp.emit_cd <= 0.0 && dist < 45.0 && los {
        c.comp.emit_cd = 1.2 * temperament::cooldown_mult(c);
        fire_shot(c, env, out, muzzle, aim / dist.max(1e-3), &t);
    }
    if beam {
        fire_beam(c, env, out, muzzle, &t);
    }
    if breath {
        step_breath(c, env, out, muzzle, &t);
    }
}

fn live_children(env: &Env<'_>, id: ConstructId) -> usize {
    env.others.iter().filter(|p| p.parent == Some(id) && p.active).count()
}

fn fire_shot(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, muzzle: Vec3, dir: Vec3, t: &PlayerView) {
    if let Some(payload) = c.recipe.payload.as_deref() {
        if live_children(env, c.id) >= MAX_EMITTED {
            return;
        }
        let params = payload_params(c, payload, muzzle, dir, t, env.tick());
        out.queue.push(Interaction::Spawn { parent: c.id, params: Box::new(params) });
        c.set_anim(AnimState::Active, env.tick());
        return;
    }
    let (dm, _) = mech_mult(c);
    out.projectiles.push(ProjectileSpawn {
        owner: c.owner,
        construct: Some(c.id),
        kind: ProjectileKind::Bolt,
        pos: muzzle,
        vel: dir * 70.0,
        damage: 8.0 * dm,
        knockback: 8.0,
        element: c.recipe.element,
        radius: 0.25,
        life: 1.5,
        explode: if eff_has(c, Mechanic::Explodes) { Some((2.5, 1.2)) } else { None },
        hit_kind: HitKind::Shot,
        ricochet: if eff_has(c, Mechanic::Ricochet) { 2 } else { 0 },
        effects: hit_effects(c),
        falloff_start: 0.0,
        chip: 0.2,
        muzzle_speed: 70.0,
    });
    c.set_anim(AnimState::Active, env.tick());
}

/// A payload round as a child construct (depth 1: its own payload is dropped).
pub fn payload_params(c: &Construct, payload: &Recipe, muzzle: Vec3, dir: Vec3, t: &PlayerView, tick: bd_core::Tick) -> SpawnParams {
    let mut r = payload.clone();
    r.payload = None;
    r.mechanics.retain(|m| *m != Mechanic::Piloted && *m != Mechanic::Swarm && *m != Mechanic::Portal);
    let spec = form_spec(r.form);
    r.scale = r.scale.clamp(spec.min_scale, spec.max_scale.min(spec.default_scale * 1.5));
    r.count = 1;
    let creature = r.form.category() == FormCategory::Creature;
    let speed = if creature { 12.0 } else { 26.0 };
    let (half, _) = super::body_box(&r, None);
    SpawnParams {
        owner: c.owner,
        recipe: r,
        shape: None,
        pos: muzzle + dir * (half.max_element() + 0.2),
        rot: geom::look_rotation(dir),
        vel: dir * speed + Vec3::Y * 2.0,
        target: Some(t.id),
        cost: 0.0,
        integrity: 12.0,
        forming: 0.0,
        life: if creature { 6.0 } else { 4.0 },
        tick,
        colossal: false,
        model: None,
    }
}

/// Beam: continuous ray from the front along the facing (25 m), damage ticks at 10 Hz. A Guard /
/// Barrier construct in the way bends it off along its own facing — the shield redirects it.
fn fire_beam(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, muzzle: Vec3, t: &PlayerView) {
    if c.comp.beam_cd > 0.0 {
        return;
    }
    c.comp.beam_cd = 0.1;
    let dir = (t.hull.center() - muzzle).normalize_or(c.forward());
    let dir = (c.forward() * 0.7 + dir * 0.3).normalize_or(dir);
    let (dm, _) = mech_mult(c);
    let mut origin = muzzle;
    let mut d = dir;
    let mut range = 25.0f32;
    let mut credit = c.owner;
    for _bend in 0..2 {
        let end = origin + d * range;
        // Terrain.
        let t_ter = bd_core::voxel::raycast(env.ctx.voxels, origin, d, range).map_or(range, |h| h.t);
        // A guard in the way?
        let mut guard: Option<(f32, &super::Proxy)> = None;
        for o in env.others {
            if o.id == c.id || !o.active || !(o.has(Mechanic::Guard) || o.has(Mechanic::Barrier) || o.form == Form::Shield) || o.owner == c.owner {
                continue;
            }
            if let Some((tt, _)) = geom::segment_obb(origin, end, o.center, o.half, o.rot, 0.1) {
                let dd = tt * range;
                if dd < t_ter && guard.is_none_or(|g| dd < g.0) {
                    guard = Some((dd, o));
                }
            }
        }
        let stop = guard.map_or(t_ter, |g| g.0);
        // First player along the segment.
        let seg_end = origin + d * stop;
        let mut best: Option<(f32, PlayerId, Vec3)> = None;
        for p in env.ctx.players {
            if p.id == credit || !p.alive || p.shielded {
                continue;
            }
            if let Some((tt, _)) = geom::segment_aabb(origin, seg_end, &p.hull) {
                if best.is_none_or(|b| tt < b.0) {
                    best = Some((tt, p.id, origin + (seg_end - origin) * tt));
                }
            }
        }
        let hit_end = best.map_or(seg_end, |b| b.2);
        out.events.push(GameEvent::Hitscan { owner: credit, origin, end: hit_end, hits: best.is_some() as u8 });
        if let Some((_, pid, point)) = best {
            out.player_hits.push(
                HitInfo::new(pid, Some(credit), HitKind::Shot, 2.5 * dm, 3.0, d, point)
                    .with_element(c.recipe.element)
                    .with_construct(c.id)
                    .with_impact(0.4),
            );
            return;
        }
        match guard {
            Some((dd, g)) => {
                // Redirected along the shield's facing, credited to the shield's owner.
                origin = origin + d * dd;
                d = (g.rot * Vec3::NEG_Z).normalize_or(-d);
                range = (range - dd).max(4.0);
                credit = g.owner;
            }
            None => return,
        }
    }
}

/// Breath: 0.6 s glowing telegraph, then a 1 s cone of the element (8 m, ~30°), 4 dmg per 0.2 s;
/// fire breath sets leaves and wood alight.
fn step_breath(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, muzzle: Vec3, t: &PlayerView) {
    let dt = env.dt();
    if !c.comp.breath_on {
        if c.comp.emit_cd <= 0.0 && (t.hull.center() - muzzle).length() < 10.0 {
            c.comp.breath_on = true;
            c.comp.breath_t = 0.0;
            c.set_anim(AnimState::Telegraph, env.tick());
        }
        return;
    }
    c.comp.breath_t += dt;
    if c.comp.breath_t < 0.6 {
        return;
    }
    if c.comp.breath_t >= 1.6 {
        c.comp.breath_on = false;
        c.comp.emit_cd = 2.5 * temperament::cooldown_mult(c);
        c.set_anim(AnimState::Idle, env.tick());
        return;
    }
    c.set_anim(AnimState::Active, env.tick());
    let k = ((c.comp.breath_t - 0.6) / 0.2).floor() as i32;
    let prev = ((c.comp.breath_t - dt - 0.6) / 0.2).floor() as i32;
    if k == prev && c.comp.breath_t - dt >= 0.6 {
        return;
    }
    let f = c.forward();
    let cos = 30f32.to_radians().cos();
    for p in env.ctx.enemies_of(c.owner) {
        let to = p.hull.center() - muzzle;
        let d = to.length();
        if d > 8.0 || d < 1e-3 || (to / d).dot(f) < cos {
            continue;
        }
        push_hit(out, c, p.id, HitKind::Creature, 4.0, 3.0, f, p.hull.center(), 0.4);
    }
    if on_fire(c) {
        out.queue.push(Interaction::IgniteTerrain { pos: muzzle + f * 5.0, radius: 2.0, by: Some(c.owner) });
    }
}

// ------------------------------------------------------------------------------------ area verbs

fn area_verbs(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let dt = env.dt();
    // Spin: continuous rotation about the up axis; tall things sweep an arm (lighthouse).
    if eff_has(c, Mechanic::Spin) {
        let rate = 6.0 * speed_mult(c);
        c.rot = (Quat::from_rotation_y(rate * dt) * c.rot).normalize();
        let tall = c.half.y > c.half.x.max(c.half.z) * 1.5;
        if tall {
            let arm = c.forward();
            let reach = (c.half.y * 1.2).clamp(3.0, 14.0);
            let center = c.center();
            for p in env.ctx.enemies_of(c.owner) {
                let to = p.hull.center() - center;
                let h = Vec3::new(to.x, 0.0, to.z);
                let d = h.length();
                if d > reach || d < 0.3 || to.y.abs() > c.half.y + 1.0 {
                    continue;
                }
                let u = h / d;
                if u.dot(Vec3::new(arm.x, 0.0, arm.z).normalize_or(Vec3::X)) > 0.95 && c.comp.touched.insert(player_key(p.id)) {
                    c.comp.touch_cd = 0.5;
                    let tangent = Vec3::Y.cross(u);
                    push_hit(out, c, p.id, HitKind::Thrown, 8.0, 14.0, tangent + u * 0.3, p.hull.center(), 1.0);
                }
            }
        }
    }
    // Stomp on walkers: every step shakes the ground.
    let walking = c.body.loco == Loco::Walk && c.grounded && Vec3::new(c.vel.x, 0.0, c.vel.z).length() > 1.0;
    if eff_has(c, Mechanic::Stomp) && walking && c.comp.stomp_cd <= 0.0 {
        c.comp.stomp_cd = 0.9;
        stomp(c, env, out, 1.0, false);
    }
    // Combined giants shake the island with every step.
    if c.comp.xform.combined && walking && c.comp.stomp_cd <= 0.0 {
        c.comp.stomp_cd = 1.0;
        stomp(c, env, out, 1.5, false);
    }
    // ArticulatedTail: sweeps anything behind it.
    if eff_has(c, Mechanic::ArticulatedTail) && c.comp.tail_cd <= 0.0 {
        let back = -c.forward();
        let reach = c.half.z * 2.0 + 3.0;
        let center = c.center();
        let mut swept = false;
        for p in env.ctx.enemies_of(c.owner) {
            let to = p.hull.center() - center;
            let d = to.length();
            if d < reach && to.normalize_or_zero().dot(back) > 0.2 {
                swept = true;
                let side = Vec3::Y.cross(back);
                push_hit(out, c, p.id, HitKind::Creature, 10.0, 18.0, side + back * 0.5, p.hull.center(), 1.3);
            }
        }
        if swept {
            c.comp.tail_cd = 1.6;
            c.set_anim(AnimState::Active, env.tick());
        }
    }
}

/// Ground shockwave + carve (Stomp, landing slams, combined giants' steps). `k` scales radius.
fn stomp(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut, k: f32, slam: bool) {
    let ring = ((2.0 + c.recipe.scale * c.size_scale * 0.3) * k).clamp(2.0, 12.0);
    out.carves.push(CarveRequest { center: c.pos, radius: (ring * 0.25).clamp(0.8, 3.0), power: 0.6 });
    let (dmg, kb) = if slam { (6.0 + 6.0 * k, 12.0 + 4.0 * k) } else { (6.0, 12.0) };
    for p in env.ctx.enemies_of(c.owner) {
        let d = Vec3::new(p.pos.x - c.pos.x, 0.0, p.pos.z - c.pos.z).length();
        if d < ring && (p.grounded || (p.pos.y - c.pos.y).abs() < 1.0) {
            let dir = (p.pos - c.pos).normalize_or_zero() + Vec3::Y;
            push_hit(out, c, p.id, HitKind::Creature, dmg, kb, dir, p.pos, 1.4);
        }
    }
    out.events.push(GameEvent::Explosion { pos: c.pos, radius: ring, element: Element::Earth, source: Some(c.owner) });
}

// ------------------------------------------------------------------------------------ swarm

/// Swarm on a non-creature: split into `count` smaller independent copies (queued). An anvil swarm
/// spreads its copies over the target area and staggers the drops — a hailstorm.
pub fn swarm_split(c: &mut Construct, out: &mut ConstructOut) {
    let plan = if c.category == FormCategory::Creature { super::swarm::split_plan(c) } else { None };
    let (n, size) = plan.unwrap_or_else(|| {
        let n = c.recipe.count.clamp(2, super::forms::MAX_SWARM_COUNT);
        (n, (1.0 / (n as f32).cbrt()).clamp(0.4, 1.0))
    });
    if n >= 2 {
        out.queue.push(Interaction::Split { id: c.id, count: n, scale: size });
    }
}

// ------------------------------------------------------------------------------------ Transforms

/// Toggle a Transforms construct's mode (pilot / suit transform key, "transform" / "roll out").
/// Returns false if it can't transform right now.
pub fn request_transform(c: &mut Construct) -> bool {
    if !eff_has(c, Mechanic::Transforms) || c.comp.xform.busy || c.comp.xform.hub.is_some() {
        return false;
    }
    let x = &mut c.comp.xform;
    x.want = if x.mode == XMode::Alt { XMode::Robot } else { XMode::Alt };
    x.busy = true;
    true
}

fn step_transform(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let dt = env.dt();
    c.comp.xform.since += dt;
    if c.comp.xform.hub.is_some() {
        step_combine(c, env, out);
        return;
    }
    if c.comp.xform.busy {
        advance_transform(c, env, true);
        return;
    }
    // Autonomous: drive in, transform ~12 m out, fight, transform back to reposition.
    if c.pilot.is_none() && !c.attached() {
        let d = env.target_for(c).map(|t| (t.pos - c.pos).length());
        let x = c.comp.xform;
        match x.mode {
            XMode::Alt if d.is_some_and(|d| d < XFORM_ENGAGE) && x.since > 0.8 => {
                request_transform(c);
            }
            XMode::Robot if !x.combined && d.is_none_or(|d| d > XFORM_DISENGAGE) && x.since > 2.0 => {
                request_transform(c);
            }
            _ => {}
        }
    }
}

/// Advance a transformation in progress (`busy`). `claim` = hold the body still (free bodies).
fn advance_transform(c: &mut Construct, env: &Env<'_>, claim: bool) {
    let dt = env.dt();
    if !c.comp.xform.busy {
        return;
    }
    {
        let x = &mut c.comp.xform;
        let dir = if x.want == XMode::Robot { 1.0 } else { -1.0 };
        let start = x.progress == 0.0 || x.progress == 1.0;
        x.progress = (x.progress + dir * dt / XFORM_SECS).clamp(0.0, 1.0);
        let done = (dir > 0.0 && x.progress >= 1.0) || (dir < 0.0 && x.progress <= 0.0);
        if start {
            c.set_anim(AnimState::Telegraph, env.tick());
        }
        // Panels fold: no steering while transforming (momentum is kept — mid-air transforms fall).
        if claim {
            c.body.claim(Driver::Hold);
            c.vel.x *= (1.0 - 3.0 * dt).max(0.0);
            c.vel.z *= (1.0 - 3.0 * dt).max(0.0);
        }
        if done {
            let airborne = !(c.body.supported || c.grounded);
            let x = &mut c.comp.xform;
            x.mode = x.want;
            x.busy = false;
            x.since = 0.0;
            if x.mode == XMode::Robot && airborne {
                x.slam = true;
            }
            let mode = x.mode;
            reshape_for_mode(c, mode);
            c.set_anim(AnimState::Idle, env.tick());
        }
    }
}

/// The collision box follows the silhouette: the alt mode lies long (vehicle), the robot stands
/// up (its length becomes its height). Swaps the box's long horizontal axis with the vertical one.
fn reshape_for_mode(c: &mut Construct, mode: XMode) {
    // Two-mode icons: the authored alt / robot boxes (from `alt_shape`).
    if let Some([alt, robot]) = c.authored_boxes {
        let (half, lc) = if mode == XMode::Robot { robot } else { alt };
        let bottom = c.bounds().min.y;
        c.half = half * c.size_scale;
        c.local_center = lc * c.size_scale;
        let nb = c.bounds().min.y;
        c.pos.y += bottom - nb;
        return;
    }
    let h = c.half;
    let long = h.z.max(h.x);
    let short = h.z.min(h.x);
    let new = match mode {
        XMode::Robot if h.y < long => Vec3::new(h.x.min(long * 0.45).max(short), long, (h.y).max(short * 0.6)),
        XMode::Alt if h.y > long => Vec3::new(h.x, h.z, h.y),
        _ => return,
    };
    let bottom = c.bounds().min.y;
    c.half = new;
    if c.local_center.y > 0.0 {
        c.local_center.y = new.y;
    }
    // Keep the feet where they were.
    let nb = c.bounds().min.y;
    c.pos.y += bottom - nb;
}

/// Mode-dependent locomotion + attacks for generic (non-creature) transformers.
fn xform_locomotion(c: &mut Construct, env: &Env<'_>) {
    if c.comp.xform.busy || !c.body.free(Driver::Steer) {
        return;
    }
    let dt = env.dt();
    let air_alt = eff_has(c, Mechanic::Flies);
    match c.comp.xform.mode {
        XMode::Alt => {
            c.body.claim(Driver::Steer);
            if air_alt {
                fly_hunt(c, env);
                return;
            }
            // Vehicle drive: fast, turn-limited, rams (contact hits in `react`).
            c.body.loco = Loco::Walk;
            c.body.loose = false;
            let target = env.target_for(c);
            c.target = target.map(|t| t.id);
            let goal = target.map_or(env.owner.pos + env.owner.look * 6.0, |t| t.pos);
            let to = goal - c.pos;
            let st = super::ai::steer_ground(env.ctx.voxels, c.pos, to, 6.0, 1.0, 3.0, false, 1.0);
            let fwd = Vec3::new(c.forward().x, 0.0, c.forward().z).normalize_or(st.dir);
            if st.dir != Vec3::ZERO {
                yaw_toward(c, st.dir, 2.5, dt);
            }
            let speed = if target.is_some() { XFORM_DRIVE_SPEED * speed_mult(c) } else { 6.0 };
            let want = if st.dir == Vec3::ZERO { Vec3::ZERO } else { fwd * speed };
            let blend = (2.5 * dt).min(1.0);
            c.vel.x += (want.x - c.vel.x) * blend;
            c.vel.z += (want.z - c.vel.z) * blend;
            c.set_anim(AnimState::Move, env.tick());
        }
        XMode::Robot => {
            c.body.claim(Driver::Steer);
            c.body.loco = Loco::Walk;
            c.body.loose = false;
            let speed = if c.comp.xform.combined { 6.0 } else { 5.5 };
            walk_toward_target(c, env, speed);
        }
    }
}

/// Robot-mode melee: called from `react` via `transform_attacks`.
fn transform_attacks(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    if !eff_has(c, Mechanic::Transforms) || c.comp.xform.mode != XMode::Robot || c.comp.xform.busy || c.comp.xform.punch_cd > 0.0 || is_creature_ctrl(c) || c.pilot.is_some() {
        return;
    }
    let Some(t) = env.target_for(c) else { return };
    let reach = c.half.x.max(c.half.z) + 1.8;
    if geom::dist_sq_point_aabb(&t.hull, c.center()) <= reach * reach {
        c.comp.xform.punch_cd = 0.8 * temperament::cooldown_mult(c);
        let dir = (t.hull.center() - c.center()).normalize_or(c.forward());
        let k = if c.comp.xform.combined { 1.6 } else { 1.0 };
        push_hit(out, c, t.id, HitKind::Creature, 12.0, 14.0 * k, dir, t.hull.center(), 1.2 * k);
        c.set_anim(AnimState::Active, env.tick());
    }
}

/// Start a combiner for `owner`: ≥ 3 owned Transforms constructs merge into one Kaiju-scale
/// robot (one per player). Returns true if started.
pub fn start_combine(items: &mut [Construct], owner: PlayerId) -> bool {
    if items.iter().any(|c| c.owner == owner && c.gone.is_none() && (c.comp.xform.combined || c.comp.xform.hub.is_some())) {
        return false;
    }
    let mut parts = [usize::MAX; 5];
    let mut n = 0;
    for (i, c) in items.iter().enumerate() {
        if n >= parts.len() {
            break;
        }
        if c.owner == owner && c.gone.is_none() && c.active() && eff_has(c, Mechanic::Transforms) && !c.attached() && c.pilot.is_none() {
            parts[n] = i;
            n += 1;
        }
    }
    if n < 3 {
        return false;
    }
    let hub = items[parts[0]].id;
    for (k, &i) in parts[..n].iter().enumerate() {
        let x = &mut items[i].comp.xform;
        x.hub = Some(hub);
        x.combine_t = 0.0;
        x.busy = false;
        x.slot = if k == 0 { n as u8 } else { k as u8 };
    }
    true
}

/// Limb slots around the hub (in hub-local units of its half extents).
const LIMBS: [Vec3; 5] = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(-1.6, 1.4, 0.0), Vec3::new(1.6, 1.4, 0.0), Vec3::new(-0.8, -1.5, 0.0), Vec3::new(0.8, -1.5, 0.0)];

fn step_combine(c: &mut Construct, env: &Env<'_>, _out: &mut ConstructOut) {
    let dt = env.dt();
    let Some(hub) = c.comp.xform.hub else { return };
    c.comp.xform.combine_t += dt;
    let t = c.comp.xform.combine_t;
    c.body.claim(Driver::Hold);
    c.body.loco = Loco::Kinematic;
    c.vel = Vec3::ZERO;
    c.set_anim(AnimState::Telegraph, env.tick());
    if hub == c.id {
        // The torso rises a little while the limbs fly in.
        c.pos.y += (1.0 - t / COMBINE_SECS).max(0.0) * 1.5 * dt;
        if t >= COMBINE_SECS {
            let n = c.comp.xform.slot.max(3) as f32;
            // Kaiju scale: ~10–14 m tall whatever the parts were (two steps: resize clamps ×4).
            let want = ((5.0 + 0.5 * n) / c.half.y.max(0.1)).clamp(1.5, 12.0);
            let f1 = want.min(4.0);
            c.resize(f1);
            c.resize(want / f1);
            c.recipe.mass = (c.recipe.mass * n).min(super::forms::MAX_MASS);
            c.max_integrity *= n;
            c.integrity = c.max_integrity;
            c.life = c.life.max(c.age + 25.0);
            let x = &mut c.comp.xform;
            x.hub = None;
            x.combined = true;
            x.mode = XMode::Robot;
            x.progress = 1.0;
            x.since = 0.0;
            c.comp.eff.0 |= 1u64 << (Mechanic::Stomp as u32 & 63);
            c.body.loco = Loco::Walk;
            c.set_anim(AnimState::Idle, env.tick());
        }
        return;
    }
    let Some(h) = env.proxy(hub).filter(|h| h.active) else {
        // The hub is gone: the sequence aborts.
        c.comp.xform.hub = None;
        c.body.loco = Loco::Walk;
        return;
    };
    let slot = LIMBS[(c.comp.xform.slot as usize).min(LIMBS.len() - 1)];
    let goal = h.center + h.rot * (slot * h.half.max(Vec3::splat(0.5)));
    let k = (3.0 * dt / (COMBINE_SECS - t).max(dt)).min(1.0);
    c.pos += (goal - c.pos) * k;
    if t >= COMBINE_SECS {
        c.gone = Some(GoneReason::Spent);
    }
}

// ------------------------------------------------------------------------------------ interactions

/// Apply one queued cross-construct interaction (pipeline step 6).
pub fn apply_interaction(store: &mut ConstructStore, it: Interaction, ctx: &ConstructCtx, out: &mut ConstructOut) {
    match it {
        Interaction::Push { id, dv } => {
            if let Some(c) = store.get_mut(id) {
                if dv.is_finite() && c.gone.is_none() && !c.attached() {
                    c.vel += dv;
                }
            }
        }
        Interaction::Loosen { id, spin } => {
            if let Some(c) = store.get_mut(id) {
                loosen(c, spin);
            }
        }
        Interaction::Attach { id, to, offset, kind } => {
            if id == to || store.get(to).is_none_or(|h| h.gone.is_some() || h.comp.attached_to == Some(id)) {
                return;
            }
            if let Some(c) = store.get_mut(id) {
                if c.gone.is_none() && !c.attached() && c.pilot.is_none() {
                    c.comp.attached_to = Some(to);
                    c.comp.attach_offset = offset;
                    c.comp.attach_kind = Some(kind);
                }
            }
        }
        Interaction::Release { holder, vel } => {
            for c in store.iter_mut() {
                if c.comp.attached_to == Some(holder) && c.comp.attach_kind == Some(AttachKind::Hold) {
                    detach(c);
                    c.vel = vel;
                }
            }
        }
        Interaction::Slice { id, point, normal, by } => {
            super::counters::slice(store, id, point, normal, by, &mut out.events);
        }
        Interaction::Freeze { id, secs } => {
            if let Some(c) = store.get_mut(id) {
                c.comp.frozen = c.comp.frozen.max(secs);
                c.comp.snuffed = true;
                c.comp.burning = 0.0;
            }
        }
        Interaction::Ignite { id, secs } => {
            if let Some(c) = store.get_mut(id) {
                if c.comp.frozen <= 0.0 && c.comp.wet <= 0.0 {
                    c.comp.burning = c.comp.burning.max(secs);
                }
            }
        }
        Interaction::Douse { id } => {
            if let Some(c) = store.get_mut(id) {
                c.comp.burning = 0.0;
                c.comp.wet = 3.0;
                if c.recipe.element == Element::Fire {
                    c.comp.snuffed = true;
                }
                if eff_has(c, Mechanic::Explodes) && c.category == FormCategory::Object {
                    c.comp.dud = true;
                }
            }
        }
        Interaction::Root { id, secs, pinned, by } => {
            let _ = by;
            if let Some(c) = store.get_mut(id) {
                if pinned {
                    c.comp.pinned = c.comp.pinned.max(secs);
                    if c.recipe.form == Form::Kaiju {
                        // A caged kaiju topples.
                        c.body.spin = c.rot * Vec3::X * 1.2;
                    }
                }
                c.comp.rooted = c.comp.rooted.max(secs);
            }
        }
        Interaction::Resize { id, factor } => {
            if let Some(c) = store.get_mut(id) {
                if c.gone.is_none() && (c.size_scale - 1.0).abs() < 0.01 {
                    c.resize(factor);
                }
            }
        }
        Interaction::Split { id, count, scale } => split(store, id, count, scale, ctx, &mut out.events),
        Interaction::Spawn { parent, params } => {
            store.spawn_child(parent, *params, &mut out.events);
        }
        Interaction::IgniteTerrain { pos, radius, by } => {
            store.chem.ignite_area(ctx.voxels, pos, radius, by);
        }
    }
}

/// Knock a structure loose: it now falls and topples.
pub fn loosen(c: &mut Construct, spin: Vec3) {
    if c.gone.is_some() || c.attached() || matches!(c.recipe.form, Form::Platform | Form::Pillar | Form::Dome) || eff_has(c, Mechanic::Flies) {
        return;
    }
    c.body.loose = true;
    if matches!(c.body.loco, Loco::Static | Loco::Kinematic) && c.comp.attached_to.is_none() {
        c.body.loco = Loco::Ballistic;
    }
    if spin.is_finite() {
        c.body.spin += spin;
    }
}

fn split(store: &mut ConstructStore, id: ConstructId, count: u8, scale: f32, ctx: &ConstructCtx, events: &mut Vec<GameEvent>) {
    let Some(src) = store.get(id).cloned() else { return };
    if src.gone.is_some() {
        return;
    }
    let n = count.clamp(2, super::forms::MAX_SWARM_COUNT);
    let mut r = src.recipe.clone();
    r.count = 1;
    r.mechanics.retain(|m| *m != Mechanic::Swarm);
    r.scale *= scale;
    let hail = eff_has(&src, Mechanic::DropFromSky);
    // Around the target for a hailstorm, else a ring around the original.
    let center = if hail {
        src.target.and_then(|t| ctx.player(t)).map_or(src.pos, |t| Vec3::new(t.pos.x, src.pos.y, t.pos.z))
    } else {
        src.pos
    };
    let spread = if hail { 3.5 } else { (src.half.max_element() * 2.0).clamp(1.0, 4.0) };
    for k in 1..n {
        let a = k as f32 * 2.399_963; // golden angle: an even, deterministic scatter
        let rr = spread * ((k as f32) / n as f32).sqrt();
        let pos = center + Vec3::new(a.cos() * rr, 0.0, a.sin() * rr);
        let params = SpawnParams {
            owner: src.owner,
            recipe: r.clone(),
            shape: None,
            pos,
            rot: src.rot,
            vel: src.vel,
            target: src.target,
            cost: 0.0,
            integrity: (src.max_integrity * scale).max(4.0),
            forming: 0.0,
            life: (src.life - src.age).max(1.0),
            tick: ctx.tick,
            colossal: false,
            model: None,
        };
        if let Some(cid) = store.spawn_child(id, params, events) {
            if let Some(c) = store.get_mut(cid) {
                c.comp.split = true;
                c.comp.launched = src.comp.launched;
                c.anim_state = src.anim_state;
                if hail {
                    // Staggered drops: a rolling hailstorm.
                    c.comp.drop_t = -(k as f32) * 0.12;
                }
            }
        }
    }
    if let Some(c) = store.get_mut(id) {
        c.recipe.count = 1;
        c.recipe.mechanics.retain(|m| *m != Mechanic::Swarm);
        c.comp.eff = effective(&c.recipe);
        // `resize` clamps each step to ×0.25: tiny swarm units need two steps.
        let f1 = scale.max(0.25);
        c.resize(f1);
        if scale < f1 {
            c.resize(scale / f1);
        }
        c.integrity = c.integrity.min(c.max_integrity * scale.max(0.25));
        if hail {
            c.pos = Vec3::new(center.x, c.pos.y, center.z);
        }
    }
}

/// After reaping: release anything attached to / held by a construct that no longer exists.
pub fn release_orphans(items: &mut [Construct]) {
    for i in 0..items.len() {
        let host = items[i].comp.attached_to;
        if let Some(h) = host {
            if items.binary_search_by_key(&h.0, |c| c.id.0).is_err() {
                detach(&mut items[i]);
            }
        }
        if let HoldTarget::Construct(h) = items[i].comp.hold {
            if items.binary_search_by_key(&h.0, |c| c.id.0).is_err() {
                release(&mut items[i], None);
            }
        }
    }
}

/// Robot-mode attacks hook (kept separate so `react` stays readable).
pub(super) fn post_react(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    if c.gone.is_none() && c.active() && !c.attached() && !is_held_down(c) {
        transform_attacks(c, env, out);
    }
}

/// Damage a construct takes from contact with an enemy construct (used by clash).
pub fn ram_hit(target: ConstructId, source: PlayerId, dmg: f32, impulse: Vec3, blade: bool, point: Vec3) -> ConstructHit {
    ConstructHit { pierce: false, blunt: false, target, source: Some(source), damage: dmg, impulse, blade, point }
}
