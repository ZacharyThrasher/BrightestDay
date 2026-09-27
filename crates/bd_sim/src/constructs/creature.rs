//! **Creatures** (COMBAT.md §4 Creatures, §5c, §5d, §5d-bis, §5e, §8) — OWNED BY THE CREATURE ENGINEER.
//!
//! Controller [`super::Controller::Creature`]: one brain for every autonomous body.
//!
//! * **Body plans** ([`Kind`]): walkers (biped / quadruped / arachnid / insect-on-foot / kaiju)
//!   steer over the voxel heightfield with bounded probes (`ai::steer_ground`), hop ledges ≤ 1 m
//!   (kaiju: ≤ ~¼ of their height) and never step off an island (the cliff guard in [`walk`])
//!   unless dashing. Serpents slither (a travelling S on the heading). Fliers (birds, insects,
//!   anything with Flies) path in 3D around terrain (`ai::steer_air`) and swoop. **Fish swim through
//!   the air** as hard-light fish, hugging ~1.2 m above the ground (or the target's height over the
//!   void) with a sinuous weave — sharks *can* cross gaps, which is the fun of them. Blobs hop;
//!   **spirits** (a Blob with Flies or a mood — "rage", "sadness") are bobbing wisps.
//! * **Speed** `9·(scale/2)^-0.25` (Fast ×1.6, Heavy ×0.7) × the temperament's pace.
//! * **Targeting** (`ai::pick_target`): owner's last aim/hit enemy, else nearest; never the owner;
//!   Chaotic also picks *constructs* of other players.
//! * **Attacks by tags** ([`AttackKind`]), each with telegraph → active window → recovery taken
//!   from `ai::attack_timing` = `bd_shapes::anim::attack_timing` for the same clip, and announced
//!   with `GameEvent::CreatureAttack` (telegraph / active / impact(+scalar) / end) so the client
//!   animates exactly what hits. Anim state: `Windup` (or `Telegraph` for Charge/Breath/Swoop)
//!   from the attack start → `Active` at the window → `Recover` at the impact frame.
//! * **Temperament** (`temperament::profile`) changes the brain, not the numbers.
//! * **Counters**: `c.comp.rooted` (framework) or overlapping an enemy Cage/Trap roots the
//!   creature (it thrashes and damages the cage until it breaks; a cage *topples* a kaiju: pinned
//!   2 s); a Heavy / DropFromSky / anvil construct falling onto a flier slams it out of the sky
//!   (pinned 1.5 s, falls). Tether shots break Grab / Constrict holds.
//! * **Transforms** (autonomous): alt mode drives fast toward the target (vehicle steering, rams)
//!   or runs on its beast gait; transforms (0.9 s, stationary) ~12 m out; fights in robot mode;
//!   transforms back to reposition beyond 30 m. Squads ("autobots": swarm units) surround the
//!   target from different sides.
//! * **Piloted** creatures: [`piloted_step`] exposes the same locomotion + attacks to `pilot.rs`.
//!
//! Contract with the framework (`mod.rs`): only steer when `c.body.claim(Driver::Steer)` succeeds;
//! this controller integrates itself (`Loco::Kinematic`). All state lives in `c.ai`.

use bd_core::input::buttons;
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Temperament};
use bd_core::voxel::{VoxelQuery, world_to_voxel};
use bd_core::{ConstructId, PlayerId, Quat, Vec3};

use super::ai::{self, AttackKind, Mode, Target, phase};
use super::components::{self, XMode};
use super::giant;
use super::swarm;
use super::temperament::{self, Profile};
use super::{AnimState, Construct, ConstructOut, Driver, Env, Interaction, Loco, PlayerView, SpawnParams, player_key};
use crate::combat::{CarveRequest, ConstructHit, HitInfo, HitKind};
use crate::events::GameEvent;
use crate::geom;
use crate::projectile::{ProjectileKind, ProjectileSpawn};

/// Creature gravity (m/s²) — heavy, like players.
pub const GRAVITY: f32 = 25.0;
/// Charge dash speed (COMBAT.md §4).
pub const CHARGE_SPEED: f32 = 25.0;
pub const CHARGE_KB: f32 = 16.0;
/// Flier swoop dive speed.
pub const SWOOP_SPEED: f32 = 22.0;
/// Alt-mode (vehicle) drive speed for autonomous transformers — below skilled movement.
pub const ALT_SPEED: f32 = 20.0;
/// Grab hold time before the fling; Constrict max squeeze time.
pub const GRAB_SECS: f32 = 1.0;
pub const CONSTRICT_SECS: f32 = 2.5;
/// Breath damage per pulse and pulse interval ("4 dmg/tick" of COMBAT.md §4 at a 0.25 s tick).
pub const BREATH_DMG: f32 = 4.0;
pub const BREATH_PULSE: f32 = 0.25;
/// Breath cone half-angle (rad).
pub const BREATH_HALF_ANGLE: f32 = 0.45;

// ============================================================================ body plan

/// What kind of body this is (derived each tick from recipe + collider; cheap, no state).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kind {
    pub flier: bool,
    /// Hard-light fish: swims through the air near the ground.
    pub swimmer: bool,
    pub serpent: bool,
    /// Legless blob: moves in hops.
    pub hopper: bool,
    /// Flying wisp (abstract-noun spirit).
    pub spirit: bool,
    pub kaiju: bool,
    /// A ground vehicle (autonomous transformer in alt mode, or a vehicle-shaped creature).
    pub vehicle: bool,
    /// Has arms (swipes instead of biting).
    pub arms: bool,
    /// Body height (m).
    pub h: f32,
    /// Horizontal body radius (m).
    pub radius: f32,
    /// Cruise speed (m/s).
    pub speed: f32,
    /// Tallest ledge it hops (m).
    pub step_up: f32,
    /// Deepest drop it walks down (m); deeper = cliff.
    pub safe_drop: f32,
    /// Melee reach from the front of the body (m).
    pub reach: f32,
}

/// Scale-law speed of COMBAT.md §4.
pub fn base_speed(c: &Construct) -> f32 {
    let scale = (c.recipe.scale * c.size_scale).max(0.2);
    let mut s = 9.0 * (scale / 2.0).powf(-0.25);
    if c.has(Mechanic::Fast) {
        s *= 1.6;
    }
    if c.has(Mechanic::Heavy) {
        s *= 0.7;
    }
    s.clamp(2.0, 20.0)
}

/// Body plan of `c`.
pub fn kind_of(c: &Construct) -> Kind {
    let form = c.recipe.form;
    let h = (c.half.y * 2.0).clamp(0.2, 200.0);
    let radius = c.half.x.max(c.half.z).clamp(0.1, 100.0);
    let kaiju = form == Form::Kaiju || c.colossal || h >= 9.0;
    let spirit = form == Form::Blob && (c.has(Mechanic::Flies) || c.recipe.temperament != Temperament::Calm);
    let swimmer = form == Form::Fish && !c.has(Mechanic::Flies);
    let robot = is_robot(c);
    // GIANTS: a giant whose wings were cut off walks.
    let flier = !robot && (spirit || c.has(Mechanic::Flies) || matches!(form, Form::Bird | Form::Insect)) && !giant::grounded_by_wings(c);
    let vehicle = !robot && (form == Form::Vehicle || (c.has(Mechanic::Transforms) && c.category != FormCategory::Creature));
    let hopper = form == Form::Blob && !spirit;
    let mut speed = base_speed(c) * temperament::speed_mult(c);
    if vehicle {
        speed = speed.max(ALT_SPEED * temperament::of(c).pace.max(0.6));
    }
    if swarm::is_unit(c) {
        speed = speed.min(14.0);
    }
    let step_up = if kaiju { (h * 0.25).clamp(2.0, 10.0) } else { 1.05 };
    Kind {
        flier,
        swimmer,
        serpent: form == Form::Serpent && !robot,
        hopper,
        spirit,
        kaiju,
        vehicle,
        arms: robot || matches!(form, Form::Biped | Form::Kaiju),
        h,
        radius,
        speed,
        step_up,
        safe_drop: (h * 1.5).clamp(3.0, 30.0),
        // EXTENDS: a creature carrying a telescoping weapon strikes from far outside its body.
        reach: if c.has(Mechanic::Extends) { super::extend::CREATURE_REACH } else { (0.6 + 0.25 * h).min(8.0) },
    }
}

// ============================================================================ entry points

/// [`super::StepFn`] for [`super::Controller::Creature`] — also handles any autonomous construct
/// routed here (a Transforms vehicle, an animated tree with legs…).
pub fn step(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let dt = env.dt();
    if !c.active() {
        // Forming: stand still where it was cast (fliers hover).
        c.body.loco = Loco::Kinematic;
        c.vel = Vec3::ZERO;
        return;
    }
    init(c);
    tick_timers(c, dt);
    // GIANTS: anatomy on the first active tick; timers; shockwaves run whatever the body does.
    let gt = &env.ctx.tuning.giants;
    giant::init(c, gt, out);
    if c.ai.giant.on {
        giant::tick(c, dt);
        giant::step_shocks(c, env, gt, out);
    }
    // Framework holds: frozen / rooted (cage) / pinned (slammed, toppled) / carried, or a
    // transformation in progress (`components::drive` already claimed `Hold` and moves the body).
    if components::is_held_down(c) || c.comp.xform.busy {
        abort_attack(c, env, out);
        cancel_hold(c);
        let m = if c.comp.xform.busy {
            Mode::Transforming
        } else if c.comp.pinned > 0.0 {
            Mode::Pinned
        } else {
            Mode::Rooted
        };
        set_mode(c, m, out);
        // The framework owns the body meanwhile: let its physics drop a slammed flier / a
        // mid-air transformer (Kinematic would freeze it in the air). Attached = carried.
        if c.comp.attached_to.is_none() && c.body.loco == Loco::Kinematic {
            c.body.loco = Loco::Ballistic;
        }
        c.grounded = false;
        return;
    }
    if !c.body.claim(Driver::Steer) {
        // A higher driver (a Hold carrying it, a framework Charge) moves the body this tick.
        cancel_hold(c);
        return;
    }
    c.body.loco = Loco::Kinematic;
    let prof = temperament::of(c);
    let mut k = kind_of(c);

    // ---- GIANTS: hijack, topple (a leg gave out), stagger (weak-point crit) ----
    if c.ai.giant.on {
        if let Some(pid) = giant::step_hijack(c, env, gt, out) {
            c.owner = pid;
            c.target = None;
            c.ai.target = Target::None;
            cancel_hold(c);
        }
        if giant::begin_topple(c, out) {
            abort_attack(c, env, out);
            cancel_hold(c);
            if k.flier {
                c.ai.giant.topple_y = ai::ground_at(env.ctx.voxels, c.pos, 1.0, 60.0).unwrap_or(c.pos.y);
            }
        }
        if giant::step_topple(c, env, gt, out) {
            abort_attack(c, env, out);
            cancel_hold(c);
            set_mode(c, Mode::Toppled, out);
            c.grounded = true;
            c.ai.airborne = false;
            c.ai.speed = 0.0;
            return;
        }
        if c.ai.giant.stagger > 0.0 {
            if c.ai.giant.stagger_new {
                c.ai.giant.stagger_new = false;
                abort_attack(c, env, out);
                cancel_hold(c);
                c.anim_state = AnimState::Idle;
                c.set_anim(AnimState::Recover, env.tick());
            }
            set_mode(c, Mode::Staggered, out);
            c.vel.x *= 0.85;
            c.vel.z *= 0.85;
            if k.flier || k.swimmer {
                c.vel.y *= 0.85;
                fly(c, env, &k);
            } else {
                walk(c, env, &k, false);
            }
            c.ai.speed = flat(c.vel).length();
            return;
        }
    }

    // ---- counters: pinned (slammed / toppled), rooted (caged) ----
    detect_counters(c, env, &k, out);
    if c.ai.pinned > 0.0 {
        set_mode(c, Mode::Pinned, out);
        abort_attack(c, env, out);
        cancel_hold(c);
        k.flier = false;
        k.swimmer = false;
        c.vel.x *= 0.9;
        c.vel.z *= 0.9;
        walk(c, env, &k, true);
        anim_moving(c, env, 0.0);
        return;
    }
    if c.comp.rooted > 0.0 || c.ai.caged > 0.0 {
        set_mode(c, Mode::Rooted, out);
        cancel_hold(c);
        c.vel.x = 0.0;
        c.vel.z = 0.0;
        if !(k.flier || k.swimmer) {
            walk(c, env, &k, false);
        } else {
            c.vel = Vec3::ZERO;
        }
        // Thrash: jerky facing, and it can still snap at anything in reach.
        let jerk = ai::noise1(c.id.0 ^ 0xA5, c.age * 9.0) * 0.6;
        c.rot = (Quat::from_rotation_y(jerk * dt * 8.0) * c.rot).normalize();
        c.set_anim(AnimState::Charging, env.tick());
        return;
    }

    // ---- target ----
    let target = choose_target(c, env, &prof, out);
    let tpos = target_pos(env, target);

    // ---- holds (Grab / Constrict) ----
    if c.ai.hold.is_some() {
        step_hold(c, env, &k, out);
    }

    // ---- attack in progress ----
    let mut move_scale = 1.0;
    if c.ai.attack.running {
        move_scale = advance_attack(c, env, &k, &prof, out);
    } else if c.ai.hold.is_none() {
        if let (Target::None, _) | (_, None) = (target, tpos) {
        } else {
            // The start tick is clip time 0 (the client's `anim_since`); it advances from the next
            // tick, so the sim's clip time always equals the client's `(tick − since)·DT`.
            try_start_attack(c, env, &k, &prof, target, out);
            if c.ai.attack.running {
                move_scale = 0.45;
            }
        }
    }
    if c.ai.attack.running
        && matches!(c.ai.attack.kind, AttackKind::Charge | AttackKind::Swoop)
        && c.ai.attack.t >= timing(c, c.ai.attack.kind).anticipation
    {
        // The dash drives the body; integrate along the dash.
        let allow_void = c.ai.attack.kind == AttackKind::Charge;
        integrate(c, env, &k, allow_void);
        anim_moving(c, env, CHARGE_SPEED);
        return;
    }

    // ---- locomotion ----
    let (goal, stop, mode) = goal_for(c, env, &k, &prof, target, tpos);
    set_mode(c, mode, out);
    let speed = k.speed * move_scale * giant::speed_mult(c, gt); // GIANTS: a damaged leg limps
    locomote(c, env, &k, &prof, goal, stop, speed, tpos);
    anim_moving(c, env, speed);
    if k.kaiju || c.has(Mechanic::Stomp) && k.h >= 3.0 {
        footsteps(c, env, &k, out);
    }
}

fn init(c: &mut Construct) {
    // Swarm copies are cloned from their parent (ai state included): make them fragile anyway.
    if swarm::is_unit(c) && c.max_integrity > swarm::UNIT_INTEGRITY {
        c.max_integrity = swarm::UNIT_INTEGRITY;
        c.integrity = c.integrity.min(c.max_integrity);
        c.ai.timing_h = timing_height(c);
    }
    if c.ai.init {
        return;
    }
    c.ai.init = true;
    c.ai.heading = flat(c.forward()).normalize_or(Vec3::NEG_Z);
    c.ai.side = if ai::hash01(c.id.0, 0, 21) < 0.5 { -1.0 } else { 1.0 };
    c.ai.orbit = ai::hash01(c.id.0, 0, 22) * std::f32::consts::TAU;
    c.ai.timing_h = timing_height(c);
    c.ai.best_dist = f32::INFINITY;
    c.ai.sent_mode = u8::MAX;
    // Staggered first attacks so a pack doesn't bite in unison.
    for (i, cd) in c.ai.cd.iter_mut().enumerate() {
        *cd = 0.15 + 0.35 * ai::hash01(c.id.0, i as u64, 23);
    }
    if swarm::is_unit(c) {
        c.max_integrity = c.max_integrity.min(swarm::UNIT_INTEGRITY);
        c.integrity = c.integrity.min(c.max_integrity);
    }
    // Probe ground right away (spawned on the aim point, maybe slightly in the air).
    c.grounded = false;
}

/// Height (m) the attack clips are timed for: the rig height the client animates with
/// (`bd_shapes::attack_timing` reads `model.rig.height`). Computed once at spawn (never per tick)
/// from the same shape the client builds — the cast's shape, or the recipe's procedural shape:
/// normal creatures are voxelized exactly like the client's `build_model` (≤ 6k voxels, cheap);
/// kaiju / colossal bodies use the voxel-exact spec bounds (within a voxel of the rig: < 1 ms of
/// timing at their size) so a 40k-voxel voxelization never hitches the sim. Terrain: its box.
pub fn timing_height(c: &Construct) -> f32 {
    let h = if c.model.is_some() {
        c.half.y * 2.0
    } else {
        let generated;
        let spec = match c.shape.as_deref() {
            Some(s) => s,
            None => {
                generated = bd_shapes::generate(&c.recipe);
                &generated
            }
        };
        let k = super::shape_scale(&c.recipe, spec) * c.size_scale;
        if c.recipe.form == Form::Kaiju || c.colossal {
            let b = bd_shapes::spec_bounds(spec);
            (b.max.y - b.min.y + 1.0) * bd_core::CONSTRUCT_VOXEL_SIZE * k
        } else {
            bd_shapes::voxelize_capped(spec, bd_shapes::voxel_cap(c.recipe.form)).rig.height * k
        }
    };
    if h.is_finite() { h.clamp(0.2, 200.0) } else { 2.0 }
}

fn tick_timers(c: &mut Construct, dt: f32) {
    for cd in c.ai.cd.iter_mut() {
        *cd = (*cd - dt).max(0.0);
    }
    c.ai.pinned = (c.ai.pinned - dt).max(0.0);
    c.ai.caged = (c.ai.caged - dt).max(0.0);
    c.ai.cover_t -= dt;
    c.ai.retarget_t -= dt;
    c.ai.hop_t -= dt;
}

// ============================================================================ targeting

/// Current target (players via `ai::pick_target`; Chaotic also other people's constructs).
fn choose_target(c: &mut Construct, env: &Env<'_>, prof: &Profile, out: &mut ConstructOut) -> Target {
    if prof.hits_anyone {
        // Chaotic: re-roll among everything in range every `retarget_secs`.
        let still_ok = match c.ai.target {
            Target::Player(p) => env.ctx.player(p).is_some_and(|t| t.alive && !t.shielded && t.id != c.owner),
            Target::Construct(id) => env.proxy(id).is_some_and(|p| p.active && !p.attached && p.owner != c.owner),
            Target::None => false,
        };
        if c.ai.retarget_t <= 0.0 || !still_ok {
            c.ai.retarget_t = prof.retarget_secs;
            let here = c.center();
            let range = 35.0f32;
            let np = env.ctx.enemies_of(c.owner).filter(|p| (p.pos - here).length() < range).count();
            let nc = env.others.iter().filter(|p| chaos_construct_ok(c, p, here, range)).count();
            let n = np + nc;
            c.ai.target = if n == 0 {
                env.ctx.nearest_enemy(c.owner, here).map_or(Target::None, |p| Target::Player(p.id))
            } else {
                let k = ((ai::hash01(c.id.0, env.tick(), 31) * n as f32) as usize).min(n - 1);
                if k < np {
                    env.ctx
                        .enemies_of(c.owner)
                        .filter(|p| (p.pos - here).length() < range)
                        .nth(k)
                        .map_or(Target::None, |p| Target::Player(p.id))
                } else {
                    env.others
                        .iter()
                        .filter(|p| chaos_construct_ok(c, p, here, range))
                        .nth(k - np)
                        .map_or(Target::None, |p| Target::Construct(p.id))
                }
            };
        }
    } else {
        // GIANTS: the summoner's command (crosshair hold / a named enemy) comes first.
        let focus = if c.ai.giant.on { giant::command(c, env, &env.ctx.tuning.giants, out) } else { None };
        c.ai.target = match focus {
            Some(p) => Target::Player(p),
            None => env.target_for(c).map_or(Target::None, |p| Target::Player(p.id)),
        };
    }
    c.target = match c.ai.target {
        Target::Player(p) => Some(p),
        _ => None,
    };
    c.ai.target
}

fn chaos_construct_ok(c: &Construct, p: &super::Proxy, here: Vec3, range: f32) -> bool {
    p.id != c.id && p.owner != c.owner && p.active && !p.attached && (p.center - here).length() < range
}

/// Feet position of a target.
fn target_pos(env: &Env<'_>, t: Target) -> Option<Vec3> {
    match t {
        Target::Player(p) => env.ctx.player(p).map(|v| v.pos),
        Target::Construct(id) => env.proxy(id).map(|p| p.center - Vec3::Y * p.half.y),
        Target::None => None,
    }
}

fn target_view(env: &Env<'_>, t: Target) -> Option<PlayerView> {
    match t {
        Target::Player(p) => env.ctx.player(p).copied(),
        _ => None,
    }
}

// ============================================================================ goals

/// (goal point, stop distance, behaviour mode) for this tick.
fn goal_for(c: &mut Construct, env: &Env<'_>, k: &Kind, prof: &Profile, target: Target, tpos: Option<Vec3>) -> (Vec3, f32, Mode) {
    let owner = env.owner.pos;
    let here = c.pos;
    let time = env.tick() as f32 * env.dt();
    let melee_stop = k.radius + k.reach * 0.6;
    let tview = target_view(env, target);
    let Some(tp) = tpos else {
        // No target: hang around the owner (Protective orbits).
        let r = if prof.guard_radius > 0.0 { prof.guard_radius } else { 3.0 + k.radius };
        c.ai.orbit += env.dt() * if prof.guard_radius > 0.0 { 0.8 } else { 0.15 };
        let g = owner + Vec3::new(c.ai.orbit.cos(), 0.0, c.ai.orbit.sin()) * r;
        return (g, 0.8, if prof.guard_radius > 0.0 { Mode::Guard } else { Mode::Idle });
    };
    let dist = flat(tp - here).length();
    // Transformer alt mode: drive straight at it (it transforms when close).
    if c.has(Mechanic::Transforms) && !is_robot(c) {
        return (tp, 0.0, Mode::AltMode);
    }
    match prof.temperament {
        Temperament::Protective => {
            let threat_near_owner = (tp - owner).length() <= prof.intercept_radius;
            if threat_near_owner {
                return (tp, melee_stop, Mode::Pursue);
            }
            // Body-block: stand on the line between the owner and the threat.
            let g = temperament::steer_goal(c, env, tview.as_ref(), tp);
            return (g, 0.3, Mode::Guard);
        }
        Temperament::Fearful => {
            if dist < prof.keep_away {
                // Kite: toward cover if any, else straight away.
                if c.ai.cover_t <= 0.0 {
                    c.ai.cover_t = 0.6;
                    c.ai.cover = find_cover(c, env, k, tp).unwrap_or(Vec3::NAN);
                }
                let g = if c.ai.cover.is_finite() { c.ai.cover } else { temperament::steer_goal(c, env, tview.as_ref(), tp) };
                return (g, 0.4, Mode::Kite);
            }
            // At range: hold position, facing the target (it spits / breathes from here).
            return (here, 0.0, Mode::Kite);
        }
        Temperament::Lazy => {
            if temperament::sitting(c, prof, time, Some(dist)) {
                return (here, 0.0, Mode::Sit);
            }
        }
        Temperament::Sneaky => {
            if let Some(t) = tview.as_ref() {
                let g = temperament::flank_point(t, here, prof.flank, c.ai.side);
                let flanking = flat(g - t.pos).length() > 0.5;
                return (g, if flanking { 0.6 } else { melee_stop }, if flanking { Mode::Flank } else { Mode::Pursue });
            }
        }
        Temperament::Playful | Temperament::Chaotic => {
            let g = temperament::steer_goal(c, env, tview.as_ref(), tp);
            return (g, melee_stop * 0.8, Mode::Pursue);
        }
        _ => {}
    }
    // Squads (swarm units that walk, e.g. mini transformers): surround the target.
    if swarm::is_unit(c) && !k.flier && dist > 5.0 {
        return (swarm::squad_goal(c, tp), 0.5, Mode::Flank);
    }
    // Ranged-only bodies hold at breath range.
    let stop = if c.has(Mechanic::Breath) && !prof.relentless { melee_stop.max(breath_range(k) * 0.5) } else { melee_stop };
    (tp, stop, Mode::Pursue)
}

/// Fearful cover: a spot `keep_away` from the target that terrain hides from its eyes. Six probes.
fn find_cover(c: &Construct, env: &Env<'_>, k: &Kind, tp: Vec3) -> Option<Vec3> {
    let prof = temperament::of(c);
    let eye = tp + Vec3::Y * 1.6;
    let away = flat(c.pos - tp).normalize_or(Vec3::Z);
    let mut best: Option<(Vec3, f32)> = None;
    for i in 0..6 {
        let a = (i as f32 - 2.5) * 0.5;
        let (s, co) = a.sin_cos();
        let d = Vec3::new(away.x * co + away.z * s, 0.0, -away.x * s + away.z * co);
        let p = tp + d * (prof.keep_away + 2.0);
        let Some(gy) = ai::ground_at(env.ctx.voxels, Vec3::new(p.x, c.pos.y, p.z), k.step_up * 3.0, k.safe_drop) else { continue };
        let spot = Vec3::new(p.x, gy, p.z);
        if !ai::hidden_from(env.ctx.voxels, eye, spot, (k.h * 0.5).min(1.2)) {
            continue;
        }
        let cost = (spot - c.pos).length();
        if best.is_none_or(|b| cost < b.1) {
            best = Some((spot, cost));
        }
    }
    best.map(|b| b.0)
}

// ============================================================================ locomotion

#[allow(clippy::too_many_arguments)]
fn locomote(c: &mut Construct, env: &Env<'_>, k: &Kind, prof: &Profile, goal: Vec3, stop: f32, speed: f32, tpos: Option<Vec3>) {
    let dt = env.dt();
    let to = goal - c.pos;
    let dist = flat(to).length();
    let arrive = if dist <= stop { 0.0 } else { ((dist - stop) / (0.6 + speed * 0.25)).clamp(0.0, 1.0) };
    let mut want = flat(to).normalize_or_zero() * speed * arrive;
    // Face the target when standing (kiting / guarding / sitting still looks at it).
    let face = tpos.map(|t| flat(t - c.pos)).filter(|v| v.length_squared() > 0.01);
    // Serpents slither: a travelling S on the heading. Fish weave.
    if (k.serpent || k.swimmer) && want != Vec3::ZERO {
        let w = (c.ai.distance / (k.h * 2.0 + 1.0) * std::f32::consts::TAU).sin() * if k.serpent { 0.55 } else { 0.35 };
        want = rot_y(want, w);
    }
    // Swarm units: boids on top of the goal seeking.
    if swarm::is_unit(c) {
        let (b, _) = swarm::boids(c, env, speed);
        want += if k.flier { b } else { flat(b) };
    }
    if k.flier || k.swimmer {
        let (alt, fly_speed) = cruise_altitude(c, env, k, goal, tpos);
        let mut v = Vec3::new(want.x, 0.0, want.z);
        let dy = alt - c.pos.y;
        v.y = (dy * 2.0).clamp(-fly_speed, fly_speed);
        if k.spirit {
            v.y += (c.age * 2.4 + c.id.0 as f32).sin() * 0.8;
        }
        if prof.hop_rate > 0.0 {
            v.y += (c.age * prof.hop_rate * std::f32::consts::TAU).sin() * 2.0;
        }
        let v = ai::steer_air(env.ctx.voxels, c.center(), v, k.radius, (fly_speed * 0.6).max(3.0), if k.swimmer { 0.4 } else { 1.0 });
        c.vel += (v - c.vel) * (4.0 * dt).min(1.0);
        fly(c, env, k);
    } else {
        // Re-probe the heading every 3 ticks (or when the goal direction changed a lot).
        let desired = want;
        if desired.length_squared() > 1e-4 {
            let dn = desired.normalize_or_zero();
            if c.ai.probe_cd == 0 || c.ai.heading.dot(dn) < 0.7 {
                c.ai.probe_cd = 3;
                let look = (k.radius + speed * 0.3).clamp(1.0, 10.0);
                let s = ai::steer_ground(env.ctx.voxels, c.pos, desired, look, k.step_up, k.safe_drop, false, c.ai.side);
                c.ai.heading = s.dir;
                if s.blocked && s.dir == Vec3::ZERO {
                    c.ai.side = -c.ai.side;
                }
            } else {
                c.ai.probe_cd -= 1;
            }
            want = c.ai.heading * desired.length();
        }
        let accel = if k.vehicle { 3.0 } else { 8.0 };
        let blend = (accel * dt).min(1.0);
        c.vel.x += (want.x - c.vel.x) * blend;
        c.vel.z += (want.z - c.vel.z) * blend;
        // Hops: blobs always, Playful often.
        let hop_rate = if k.hopper { 1.6 } else { prof.hop_rate };
        if c.grounded && hop_rate > 0.0 && c.ai.hop_t <= 0.0 && flat(c.vel).length() > 0.5 {
            c.ai.hop_t = 1.0 / hop_rate;
            c.vel.y = (4.0 + k.h).min(9.0);
            c.grounded = false;
        }
        walk(c, env, k, false);
    }
    // Facing: along motion, or at the target when (nearly) still.
    let hv = flat(c.vel);
    let dir = if hv.length_squared() > 0.25 { hv } else { face.unwrap_or(hv) };
    if dir.length_squared() > 1e-4 {
        let want_rot = geom::yaw_rotation(dir);
        let turn = if k.kaiju { 2.0 } else { 10.0 } * dt;
        c.rot = c.rot.slerp(want_rot, turn.min(1.0)).normalize();
    }
    // Progress watchdog → Stranded (target across a gap): pace, don't jitter.
    if let Some(t) = tpos {
        let d = flat(t - c.pos).length();
        if d < c.ai.best_dist - 0.5 {
            c.ai.best_dist = d;
            c.ai.stuck_t = 0.0;
        } else {
            c.ai.stuck_t += dt;
            if c.ai.stuck_t > 4.0 {
                c.ai.best_dist = d;
                c.ai.stuck_t = 0.0;
                c.ai.side = -c.ai.side;
            }
        }
    }
}

/// Fliers cruise a few metres above their target (swoop down to hit); fish ~1.2 m above the
/// ground (or at the target's height over the void); spirits bob at chest height.
fn cruise_altitude(c: &Construct, env: &Env<'_>, k: &Kind, goal: Vec3, tpos: Option<Vec3>) -> (f32, f32) {
    let ref_y = tpos.map_or(goal.y, |t| t.y);
    let ground = ai::ground_at(env.ctx.voxels, c.pos, 2.0, 30.0);
    if k.swimmer {
        let alt = ground.map_or(ref_y + 1.0, |g| (g + 1.2).max(ref_y + 0.6));
        return (alt, k.speed);
    }
    if k.spirit {
        return (ground.map_or(ref_y, |g| g.max(ref_y)) + 1.3, k.speed);
    }
    let above = 2.5 + k.h * 0.5;
    let floor = ground.map_or(f32::NEG_INFINITY, |g| g + 1.5 + k.h * 0.3);
    ((ref_y + above).max(floor), k.speed)
}

/// Ground integration with gravity, ledge hops, walls and the **cliff guard**: a grounded walker
/// never moves onto a column with no ground within `safe_drop` unless `allow_void`.
pub fn walk(c: &mut Construct, env: &Env<'_>, k: &Kind, allow_void: bool) {
    let v = env.ctx.voxels;
    let dt = env.dt();
    let was_grounded = c.grounded;
    // Un-embed (spawned in / carved around).
    if v.is_solid(world_to_voxel(c.pos + Vec3::Y * 0.05))
        && let Some(gy) = ai::ground_at(v, c.pos, k.step_up.max(2.0), 0.0)
    {
        c.pos.y = gy;
    }
    c.vel.y -= GRAVITY * env.ctx.gravity_at(c.pos.y) * dt;
    c.vel.y = c.vel.y.max(-60.0);
    let mut next = c.pos + c.vel * dt;
    let hmove = Vec3::new(next.x - c.pos.x, 0.0, next.z - c.pos.z);
    if hmove.length_squared() > 1e-10 {
        let lead = hmove.normalize_or_zero() * (k.radius * 0.5).min(0.45);
        let col = Vec3::new(next.x, c.pos.y, next.z) + lead;
        let probe_y = if was_grounded { k.step_up + 0.25 } else { 0.1 };
        let wall = v.is_solid(world_to_voxel(col + Vec3::Y * probe_y))
            || v.is_solid(world_to_voxel(col + Vec3::Y * (k.h * 0.6).clamp(0.3, 2.0).max(probe_y)));
        let cliff = !allow_void && was_grounded && ai::ground_at(v, col, k.step_up + 0.05, k.safe_drop).is_none();
        if wall || cliff {
            next.x = c.pos.x;
            next.z = c.pos.z;
            if cliff {
                // Skid at the edge.
                c.vel.x *= 0.5;
                c.vel.z *= 0.5;
            }
        }
    }
    let mut grounded = false;
    if was_grounded && c.vel.y <= 0.0 {
        match ai::ground_at(v, next, k.step_up + 0.05, 0.6) {
            Some(gy) if gy > c.pos.y + 0.3 => {
                // Hop up the ledge (rise first; the airborne wall probe holds it off the face).
                c.vel.y = (2.0 * GRAVITY * (gy - c.pos.y + 0.3)).sqrt();
                next = Vec3::new(c.pos.x, c.pos.y + c.vel.y * dt, c.pos.z);
            }
            Some(gy) => {
                next.y = gy;
                c.vel.y = 0.0;
                grounded = true;
            }
            None => {}
        }
    } else if c.vel.y <= 0.0 {
        let fall = (c.pos.y - next.y).max(0.0) + 0.05;
        if let Some(gy) = ai::ground_at(v, Vec3::new(next.x, c.pos.y, next.z), 0.05, fall)
            && gy >= next.y - 0.01
        {
            next.y = gy;
            c.vel.y = 0.0;
            grounded = true;
        }
    }
    let moved = flat(next - c.pos).length();
    c.ai.distance += moved;
    c.pos = next;
    c.grounded = grounded;
    c.ai.airborne = !grounded;
}

/// Air integration: no gravity; never enters terrain (climbs instead).
pub fn fly(c: &mut Construct, env: &Env<'_>, k: &Kind) {
    let v = env.ctx.voxels;
    let dt = env.dt();
    let spd = c.vel.length();
    if spd > k.speed * 1.5 + 5.0 {
        c.vel *= (k.speed * 1.5 + 5.0) / spd;
    }
    let next = c.pos + c.vel * dt;
    let body = next + Vec3::Y * (k.h * 0.5).min(1.5);
    if v.is_solid(world_to_voxel(body)) || v.is_solid(world_to_voxel(next + Vec3::Y * 0.1)) {
        c.vel = Vec3::new(c.vel.x * 0.3, c.vel.y.max(0.0) + 3.0, c.vel.z * 0.3);
        c.pos.y += 3.0 * dt;
    } else {
        c.ai.distance += (next - c.pos).length();
        c.pos = next;
    }
    c.grounded = false;
    c.ai.airborne = true;
}

/// Integrate by body kind (used by dashes and holds).
fn integrate(c: &mut Construct, env: &Env<'_>, k: &Kind, allow_void: bool) {
    if k.flier || k.swimmer {
        fly(c, env, k);
    } else {
        walk(c, env, k, allow_void);
    }
}

fn anim_moving(c: &mut Construct, env: &Env<'_>, _speed: f32) {
    c.ai.speed = flat(c.vel).length();
    if c.ai.attack.running {
        return;
    }
    let s = if c.ai.speed > 0.4 || (c.ai.airborne && !c.grounded) { AnimState::Move } else { AnimState::Idle };
    c.set_anim(s, env.tick());
}

/// Kaiju (and big Stomp creatures) crack the ground at every footstep: a carve under the foot and
/// a small shockwave for anyone standing close.
fn footsteps(c: &mut Construct, env: &Env<'_>, k: &Kind, out: &mut ConstructOut) {
    if !c.grounded {
        return;
    }
    let stride = (k.h * 0.45).max(1.5);
    if c.ai.next_step <= 0.0 {
        c.ai.next_step = c.ai.distance + stride;
        return;
    }
    if c.ai.distance < c.ai.next_step {
        return;
    }
    c.ai.next_step = c.ai.distance + stride;
    c.ai.foot = !c.ai.foot;
    let side = if c.ai.foot { 1.0 } else { -1.0 };
    let right = c.rot * Vec3::X;
    let foot = c.pos + right * (k.radius * 0.45 * side) + c.forward() * (k.radius * 0.3);
    let r = (k.h * 0.08).clamp(1.0, 4.0);
    out.carves.push(CarveRequest { center: foot, radius: r, power: 0.6 });
    if c.ai.giant.on {
        return; // GIANTS: walking never hurts (no ambient launches) — only its attacks do
    }
    let ring = (k.h * 0.3).clamp(2.0, 12.0);
    for t in env.ctx.enemies_of(c.owner) {
        if t.grounded && (t.pos - foot).length() < ring {
            let dir = flat(t.pos - foot).normalize_or_zero() + Vec3::Y;
            out.player_hits
                .push(HitInfo::new(t.id, Some(c.owner), HitKind::Creature, 3.0, 8.0, dir, t.pos).with_construct(c.id).with_impact(0.8));
        }
    }
}

// ============================================================================ counters

/// Self-detected §8 counters (the framework's `comp.rooted` works too):
/// * an enemy Cage / Trap structure overlapping the body roots it (a kaiju is toppled: pinned 2 s);
///   thrashing damages the cage (it breaks out in a few seconds);
/// * a Heavy / DropFromSky / anvil construct falling onto a flier slams it down (pinned 1.5 s).
fn detect_counters(c: &mut Construct, env: &Env<'_>, k: &Kind, out: &mut ConstructOut) {
    let b = c.bounds();
    // Contact margin: framework physics stops a falling body *on* the creature (no overlap).
    let touch = bd_core::voxel::Aabb { min: b.min - Vec3::splat(0.3), max: b.max + Vec3::splat(0.3) };
    let mut caged_by: Option<ConstructId> = None;
    for p in env.others {
        if p.owner == c.owner || !p.active || p.attached || p.id == c.id {
            continue;
        }
        let pb = p.bounds();
        if !pb.intersects(&touch) {
            continue;
        }
        let cage = p.category == FormCategory::Structure && (p.form == Form::Cage || p.has(Mechanic::Trap));
        if cage && pb.intersects(&b) {
            caged_by = Some(p.id);
        }
        let heavy = p.has(Mechanic::Heavy) || p.has(Mechanic::DropFromSky) || p.form == Form::Anvil;
        if k.flier && heavy && p.vel.y < -4.0 && p.center.y > c.center().y && c.ai.pinned <= 0.0 {
            c.ai.pinned = 1.5;
            c.vel = Vec3::new(c.vel.x * 0.2, -18.0, c.vel.z * 0.2);
            out.events.push(GameEvent::CreatureMode { id: c.id, mode: Mode::Pinned as u8 });
        }
    }
    if let Some(cage) = caged_by {
        c.ai.caged = 0.25;
        c.ai.thrash += env.dt();
        if k.kaiju && !c.ai.toppled {
            c.ai.toppled = true;
            c.ai.pinned = 2.0;
            abort_attack(c, env, out);
        }
        // Thrash against the bars: ~12 integrity/s (kaiju ×2) — it can break out.
        let dps = if k.kaiju { 24.0 } else { 12.0 };
        if (c.ai.thrash / 0.5).floor() != ((c.ai.thrash - env.dt()) / 0.5).floor() {
            out.construct_hits.push(ConstructHit { pierce: false, blunt: false,
                target: cage,
                source: Some(c.owner),
                damage: dps * 0.5,
                impulse: Vec3::ZERO,
                blade: false,
                point: c.center(),
            });
        }
    } else {
        c.ai.thrash = 0.0;
        if c.ai.caged <= 0.0 {
            c.ai.toppled = false;
        }
    }
}

// ============================================================================ transformers

/// Transforms (COMBAT.md §5d-bis): the mode machine lives in `components` (it transforms
/// autonomous transformers ~12 m from the target and back beyond ~25 m, holding the body while
/// the panels fold). A creature-controlled transformer (a beast alt: Grimlock's T-rex) runs its
/// creature gait in alt mode and fights as a biped robot (swipes, stomps, temperament) in robot mode.
#[inline]
pub fn is_robot(c: &Construct) -> bool {
    c.has(Mechanic::Transforms) && c.comp.xform.mode == XMode::Robot
}

// ============================================================================ attacks

/// Timing of `kind` for this body (same numbers the client animates with).
pub fn timing(c: &Construct, kind: AttackKind) -> ai::Timing {
    let h = if c.ai.timing_h > 0.0 { c.ai.timing_h } else { (c.half.y * 2.0).max(0.2) };
    // Mass-scaled (weight pass): the client poses with `model.weight = weight_of(recipe)`.
    let mut t = ai::attack_timing_hw(h, kind.clip(), bd_shapes::weight_of(&c.recipe));
    // Aggressive attacks faster between blows (recovery), never with a shorter telegraph.
    t.recovery *= temperament::cooldown_mult(c).clamp(0.4, 1.0);
    t
}

fn breath_range(k: &Kind) -> f32 {
    (6.0 + k.h * 0.6).clamp(6.0, 30.0)
}

/// Default melee attack of this body.
fn melee_kind(c: &Construct, k: &Kind) -> AttackKind {
    if c.has(Mechanic::Constrict) {
        AttackKind::Constrict
    } else if c.has(Mechanic::Grab) {
        AttackKind::Grab
    } else if c.has(Mechanic::Pull) {
        AttackKind::Lash
    } else if k.arms && giant::has_arm(c) {
        AttackKind::Swipe
    } else {
        AttackKind::Bite
    }
}

/// Base cooldown after an attack (s), before the temperament rhythm.
fn base_cooldown(kind: AttackKind) -> f32 {
    match kind {
        AttackKind::Bite | AttackKind::Swipe | AttackKind::Lash => 0.5,
        AttackKind::Charge => 2.5,
        AttackKind::Stomp => 2.5,
        AttackKind::TailSweep => 3.0,
        AttackKind::Breath => 3.5,
        AttackKind::Swoop => 2.0,
        AttackKind::Grab | AttackKind::Constrict => 3.0,
        AttackKind::Spit => 1.6,
    }
}

fn try_start_attack(c: &mut Construct, env: &Env<'_>, k: &Kind, prof: &Profile, target: Target, out: &mut ConstructOut) {
    let Some(tp) = target_pos(env, target) else { return };
    let here = c.center();
    let tcenter = match target {
        Target::Player(p) => env.ctx.player(p).map_or(tp, |v| v.hull.center()),
        Target::Construct(id) => env.proxy(id).map_or(tp, |p| p.center),
        Target::None => return,
    };
    let to = tcenter - here;
    let dist = to.length();
    let hdist = flat(tp - c.pos).length();
    let front = flat(c.forward()).normalize_or(Vec3::NEG_Z);
    let facing = flat(to).normalize_or_zero().dot(front);
    let ready = |kind: AttackKind| c.ai.cd[kind as usize] <= 0.0;
    let melee_r = k.radius + k.reach;
    let los = ai::line_of_sight(env.ctx.voxels, here + Vec3::Y * (k.h * 0.3), tcenter);
    // Alt-mode vehicles only ram (contact), handled as a Charge when lined up.
    if k.vehicle {
        if ready(AttackKind::Charge) && (3.0..16.0).contains(&hdist) && facing > 0.8 {
            start_attack(c, env, AttackKind::Charge, flat(to).normalize_or(front), out);
        }
        return;
    }
    // 1. ArticulatedTail: 360° sweep when someone is close (especially behind).
    let mk = melee_kind(c, k);
    if c.has(Mechanic::ArticulatedTail)
        && giant::can(c, AttackKind::TailSweep)
        && ready(AttackKind::TailSweep)
        && hdist < k.radius + k.h * 0.9 + 1.0
        && (facing < 0.3 || k.kaiju || !ready(mk))
    {
        start_attack(c, env, AttackKind::TailSweep, flat(to).normalize_or(front), out);
        return;
    }
    // 2. Stomp: ring shockwave (kaiju always can).
    // GIANTS: the shockwave runs further than the old ring, so they stomp from further out.
    let stomp_from = if c.ai.giant.on { (k.h * env.ctx.tuning.giants.shock_reach).clamp(6.0, 30.0) * 0.6 } else { stomp_ring(k) * 0.85 };
    if (c.has(Mechanic::Stomp) || k.kaiju) && ready(AttackKind::Stomp) && c.grounded && hdist < stomp_from {
        start_attack(c, env, AttackKind::Stomp, front, out);
        return;
    }
    // 3. Breath cone.
    if c.has(Mechanic::Breath) && ready(AttackKind::Breath) && dist < breath_range(k) && dist > k.radius * 0.5 && los {
        start_attack(c, env, AttackKind::Breath, to.normalize_or(front), out);
        return;
    }
    // 4. Charge dash.
    if c.has(Mechanic::Charge) && !k.flier && ready(AttackKind::Charge) && c.grounded && (4.0..18.0).contains(&hdist) && los {
        start_attack(c, env, AttackKind::Charge, flat(to).normalize_or(front), out);
        return;
    }
    // 5. Fliers swoop.
    if k.flier && !k.spirit && ready(AttackKind::Swoop) && hdist < 14.0 && tcenter.y < here.y + 1.0 && los {
        start_attack(c, env, AttackKind::Swoop, to.normalize_or(front), out);
        return;
    }
    // 6. Ranged spit (Fearful kiting, or anything with Shoots / a payload and no reach yet).
    let wants_range = prof.ranged || c.has(Mechanic::Shoots);
    if wants_range && ready(AttackKind::Spit) && dist > melee_r + 1.0 && dist < 28.0 && los {
        start_attack(c, env, AttackKind::Spit, to.normalize_or(front), out);
        return;
    }
    // 7. Melee.
    if ready(mk) && dist < melee_r + 0.8 {
        start_attack(c, env, mk, to.normalize_or(front), out);
    }
}

fn stomp_ring(k: &Kind) -> f32 {
    (k.h * 0.35 + 2.5).clamp(3.0, 16.0)
}

fn start_attack(c: &mut Construct, env: &Env<'_>, kind: AttackKind, aim: Vec3, out: &mut ConstructOut) {
    c.ai.attack = ai::AttackRun { kind, t: 0.0, running: true, aim, emitted: 1, pulse: 0.0 };
    c.hits.clear();
    // GIANTS: a warning (glow + audio) before the clip's own telegraph — nothing a giant does is
    // unreadable. The clip starts when it runs out (`advance_attack`).
    if c.ai.giant.on {
        if kind == AttackKind::Swipe {
            giant::choose_swipe_side(c, c.center() + aim * 5.0);
        }
        let warn = giant::warn_secs(&env.ctx.tuning.giants, kind);
        if warn > 0.0 {
            c.ai.attack.t = -warn;
            out.events.push(GameEvent::Giant(giant::GiantEvent::Warn { id: c.id, kind: kind as u8, aim, secs: warn }));
            return;
        }
    }
    begin_clip(c, env, kind, aim, out);
}

/// The attack clip starts (clip time 0 = this tick = the client's `anim_since`).
fn begin_clip(c: &mut Construct, env: &Env<'_>, kind: AttackKind, aim: Vec3, out: &mut ConstructOut) {
    let s = match kind {
        AttackKind::Charge | AttackKind::Breath | AttackKind::Swoop => AnimState::Telegraph,
        _ => AnimState::Windup,
    };
    // Force a fresh anim_since even if the state is unchanged (back-to-back attacks).
    c.anim_state = AnimState::Idle;
    c.set_anim(s, env.tick());
    out.events.push(GameEvent::CreatureAttack { id: c.id, kind: kind as u8, phase: phase::TELEGRAPH, aim, impact: 0.0 });
}

fn end_attack(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    let kind = c.ai.attack.kind;
    c.ai.attack.running = false;
    c.ai.cd[kind as usize] = base_cooldown(kind) * temperament::cooldown_mult(c);
    c.set_anim(AnimState::Idle, env.tick());
    out.events.push(GameEvent::CreatureAttack { id: c.id, kind: kind as u8, phase: phase::END, aim: c.ai.attack.aim, impact: 0.0 });
}

fn abort_attack(c: &mut Construct, env: &Env<'_>, out: &mut ConstructOut) {
    if c.ai.attack.running {
        end_attack(c, env, out);
    }
}

/// Advance the running attack; returns the locomotion speed scale for this tick.
fn advance_attack(c: &mut Construct, env: &Env<'_>, k: &Kind, prof: &Profile, out: &mut ConstructOut) -> f32 {
    let dt = env.dt();
    let kind = c.ai.attack.kind;
    let tm = timing(c, kind);
    if c.ai.attack.t < 0.0 {
        // GIANTS: the warning — slowed, still tracking, then the clip begins.
        if let Some(tc) = target_center(env, c.ai.target) {
            let want = (tc - c.center()).normalize_or(c.ai.attack.aim);
            c.ai.attack.aim = (c.ai.attack.aim + (want - c.ai.attack.aim) * (4.0 * dt).min(1.0)).normalize_or(want);
        }
        let fa = flat(c.ai.attack.aim);
        if fa.length_squared() > 1e-4 {
            c.rot = c.rot.slerp(geom::yaw_rotation(fa), (4.0 * dt).min(1.0)).normalize();
        }
        let t1 = c.ai.attack.t + dt;
        if t1 >= -1e-5 {
            c.ai.attack.t = 0.0;
            if kind == AttackKind::Swipe {
                giant::choose_swipe_side(c, c.center() + c.ai.attack.aim * 5.0);
            }
            begin_clip(c, env, kind, c.ai.attack.aim, out);
        } else {
            c.ai.attack.t = t1;
        }
        return 0.45;
    }
    let t0 = c.ai.attack.t;
    let t1 = t0 + dt;
    c.ai.attack.t = t1;
    let (a0, a1) = (tm.anticipation, tm.impact());
    // Aim tracking during the telegraph (slow: dodgeable) — breath keeps tracking while active.
    if let Some(tc) = target_center(env, c.ai.target) {
        let want = (tc - c.center()).normalize_or(c.ai.attack.aim);
        let rate = match kind {
            AttackKind::Breath => 1.2,
            AttackKind::Charge | AttackKind::Swoop if t0 >= a0 => 0.0,
            _ => 4.0,
        };
        c.ai.attack.aim = (c.ai.attack.aim + (want - c.ai.attack.aim) * (rate * dt).min(1.0)).normalize_or(want);
    }
    let aim = c.ai.attack.aim;
    let fa = flat(aim);
    if fa.length_squared() > 1e-4 {
        c.rot = c.rot.slerp(geom::yaw_rotation(fa), (8.0 * dt).min(1.0)).normalize();
    }
    // Phase edges.
    if t0 < a0 && t1 >= a0 {
        c.set_anim(AnimState::Active, env.tick());
        c.ai.attack.emitted |= 2;
        out.events.push(GameEvent::CreatureAttack { id: c.id, kind: kind as u8, phase: phase::ACTIVE, aim, impact: 0.0 });
        if kind == AttackKind::Swoop
            && let Some(tc) = target_center(env, c.ai.target)
        {
            c.ai.attack.aim = (tc - c.center()).normalize_or(aim);
        }
    }
    let active = t1 >= a0 && t0 < a1;
    let mut scale: f32 = match kind {
        AttackKind::Charge | AttackKind::Breath | AttackKind::Stomp | AttackKind::TailSweep | AttackKind::Swoop => 0.0,
        AttackKind::Spit => 0.2,
        _ => 0.45,
    };
    if prof.relentless {
        scale = scale.max(0.3);
    }
    if active {
        attack_window(c, env, k, prof, kind, t0.max(a0), t1.min(a1), &tm, out);
    }
    if t0 < a1 && t1 >= a1 {
        // Impact frame.
        let impact = impact_of(c, k, kind);
        let landed = c.ai.attack.emitted & 8 != 0
            || matches!(kind, AttackKind::Stomp | AttackKind::TailSweep | AttackKind::Breath | AttackKind::Spit);
        c.set_anim(AnimState::Recover, env.tick());
        out.events.push(GameEvent::CreatureAttack {
            id: c.id,
            kind: kind as u8,
            phase: phase::IMPACT,
            aim: c.ai.attack.aim,
            impact: if landed { impact } else { 0.0 },
        });
        if matches!(kind, AttackKind::Charge | AttackKind::Swoop) {
            // Skid / pull up.
            c.vel.x *= 0.3;
            c.vel.z *= 0.3;
            if kind == AttackKind::Swoop {
                c.vel.y = 6.0;
            }
        }
    }
    if t1 >= tm.total() {
        end_attack(c, env, out);
    }
    scale
}

fn target_center(env: &Env<'_>, t: Target) -> Option<Vec3> {
    match t {
        Target::Player(p) => env.ctx.player(p).filter(|v| v.alive).map(|v| v.hull.center()),
        Target::Construct(id) => env.proxy(id).filter(|p| p.active).map(|p| p.center),
        Target::None => None,
    }
}

fn impact_of(c: &Construct, k: &Kind, kind: AttackKind) -> f32 {
    let size = (k.h / 2.0).clamp(0.4, 2.0).sqrt();
    let base = match kind {
        AttackKind::Bite | AttackKind::Swipe | AttackKind::Lash | AttackKind::Spit => 0.9,
        AttackKind::Grab | AttackKind::Constrict => 0.8,
        AttackKind::Charge | AttackKind::Swoop => 1.3,
        AttackKind::TailSweep => 1.5,
        AttackKind::Breath => 0.7,
        AttackKind::Stomp => 1.4,
    };
    let heavy = if c.has(Mechanic::Heavy) { 1.2 } else { 1.0 };
    (base * size * heavy).clamp(0.2, 2.0)
}

/// Damage / knockback multipliers from traits (Heavy, Fast, Blunt) and the Playful prank.
fn mults(c: &Construct, prof: &Profile) -> (f32, f32) {
    let mut d = 1.0;
    let mut kb = 1.0;
    if c.has(Mechanic::Heavy) {
        d *= 1.5;
        kb *= 1.5;
    }
    if c.has(Mechanic::Fast) {
        d *= 0.8;
    }
    if c.has(Mechanic::Blunt) {
        kb *= 1.3;
    }
    d *= 1.0 - prof.prank;
    kb *= 1.0 + prof.prank;
    (d, kb)
}

fn base_damage(c: &Construct, t: &crate::tuning::CombatTuning) -> (f32, f32) {
    // TUNING HOOK: form table + swarm unit damage from `tuning`.
    if swarm::is_unit(c) {
        return (t.creature.swarm_unit_damage, 3.0);
    }
    let s = t.form_spec(c.recipe.form);
    let d = if c.category == FormCategory::Creature { s.damage.max(3.0) } else { 8.0 };
    let kb = if c.category == FormCategory::Creature { s.knockback.max(4.0) } else { 9.0 };
    (d, kb)
}

/// The live part of an attack between clip times `u0..u1` (inside the active window).
#[allow(clippy::too_many_arguments)]
fn attack_window(
    c: &mut Construct,
    env: &Env<'_>,
    k: &Kind,
    prof: &Profile,
    kind: AttackKind,
    u0: f32,
    u1: f32,
    tm: &ai::Timing,
    out: &mut ConstructOut,
) {
    let (bd, bk) = base_damage(c, env.ctx.tuning);
    let (dm, km) = mults(c, prof);
    let fwd = flat(c.forward()).normalize_or(Vec3::NEG_Z);
    let aim = c.ai.attack.aim;
    let here = c.center();
    match kind {
        // GIANTS: a swipe is an arc sweeping across the front (dodge through it).
        AttackKind::Swipe if c.ai.giant.on => {
            let impact = impact_of(c, k, kind);
            giant::swipe(c, env, &env.ctx.tuning.giants, k.reach, k.radius, u0, u1, tm.anticipation, tm.active, bd * 1.1 * dm, bk * km, impact, out);
        }
        // EXTENDS: the staff shoots out along the aim (once per attack) instead of a close swipe.
        AttackKind::Bite | AttackKind::Swipe if c.has(Mechanic::Extends) => {
            if c.ai.attack.emitted & 64 == 0 {
                c.ai.attack.emitted |= 64;
                let from = here + Vec3::Y * (k.h * 0.12);
                let dir = aim.normalize_or(fwd);
                super::extend::line_strike(c, env, from, dir, k.radius + k.reach, bd * 1.1 * dm, bk * km, impact_of(c, k, kind), out);
            }
        }
        AttackKind::Bite | AttackKind::Swipe | AttackKind::Lash | AttackKind::Grab | AttackKind::Constrict => {
            let reach = if kind == AttackKind::Lash { k.reach * 2.5 + 1.0 } else { k.reach };
            // Strike along the (tracked, 3D) attack aim: a kaiju swipes *down* at a player.
            let dir = aim.normalize_or(fwd);
            let p = here + dir * (k.radius + reach * 0.5);
            let r = reach * 0.6 + 0.35;
            let dmg = match kind {
                AttackKind::Grab | AttackKind::Constrict => bd * 0.5,
                AttackKind::Swipe => bd * 1.1,
                _ => bd,
            };
            let hit = strike(c, env, p, r, dmg * dm, bk * km, fwd + Vec3::Y * 0.3, impact_of(c, k, kind), out);
            if let Some(pid) = hit.filter(|_| matches!(kind, AttackKind::Grab | AttackKind::Constrict))
                && c.ai.hold.is_none()
            {
                c.ai.hold = Some(pid);
                c.ai.hold_t = 0.0;
                c.ai.attack.pulse = 0.0;
            }
            // Lash yanks the victim toward the creature.
            if kind == AttackKind::Lash
                && let Some(t) = hit.and_then(|pid| env.ctx.player(pid))
            {
                let dv = flat(here - t.pos).normalize_or_zero() * 9.0 + Vec3::Y * 3.0;
                out.impulses.push((t.id, dv, 0.0));
            }
        }
        AttackKind::Charge => {
            c.vel.x = aim.x * CHARGE_SPEED;
            c.vel.z = aim.z * CHARGE_SPEED;
            let dir = flat(aim).normalize_or(fwd) + Vec3::Y * 0.35;
            strike(c, env, here + fwd * (k.radius * 0.6), k.radius + 0.4, bd * 1.2 * dm, env.ctx.tuning.creature.charge_knockback * km, dir, impact_of(c, k, kind), out);
        }
        AttackKind::Swoop => {
            c.vel = aim * SWOOP_SPEED;
            strike(
                c,
                env,
                here + aim * (k.radius * 0.6),
                k.radius + 0.5,
                bd * dm,
                bk * 1.3 * km,
                aim + Vec3::Y * 0.4,
                impact_of(c, k, kind),
                out,
            );
        }
        AttackKind::Stomp => {
            // Lands on the impact frame only.
            if u1 >= tm.impact() - 1e-5 && c.ai.attack.emitted & 4 == 0 {
                c.ai.attack.emitted |= 4;
                stomp(c, env, k, prof, out);
            }
        }
        AttackKind::TailSweep => tail_sweep(c, env, k, prof, u0, u1, tm, out),
        AttackKind::Breath => breath(c, env, k, prof, out),
        AttackKind::Spit => {
            if u1 >= tm.impact() - 1e-5 && c.ai.attack.emitted & 4 == 0 {
                c.ai.attack.emitted |= 4;
                spit(c, env, k, out);
            }
        }
    }
}

/// Hit every valid thing within `r` of `p` once per attack (players, and constructs for Chaotic /
/// construct targets). Returns the first player hit.
#[allow(clippy::too_many_arguments)]
fn strike(
    c: &mut Construct,
    env: &Env<'_>,
    p: Vec3,
    r: f32,
    dmg: f32,
    kb: f32,
    dir: Vec3,
    impact: f32,
    out: &mut ConstructOut,
) -> Option<PlayerId> {
    let mut first = None;
    for t in env.ctx.enemies_of(c.owner) {
        let hull = env.ctx.hull_for(c.owner, t);
        if geom::dist_sq_point_aabb(&hull, p) > r * r || !c.hits.insert(player_key(t.id)) {
            continue;
        }
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::Creature, dmg, kb, dir, geom::closest_point(&hull, p))
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(impact),
        );
        c.ai.attack.emitted |= 8;
        first = first.or(Some(t.id));
    }
    let hits_constructs = temperament::of(c).hits_anyone || matches!(c.ai.target, Target::Construct(_));
    if hits_constructs {
        for q in env.others {
            if q.id == c.id || q.owner == c.owner || !q.active || q.attached {
                continue;
            }
            if !geom::sphere_obb(p, r, q.center, q.half, q.rot) || !c.hits.insert(q.id.0) {
                continue;
            }
            out.construct_hits.push(ConstructHit { pierce: false, blunt: false,
                target: q.id,
                source: Some(c.owner),
                damage: dmg * 1.5,
                impulse: dir.normalize_or_zero() * kb,
                blade: c.has(Mechanic::Blade),
                point: p,
            });
            c.ai.attack.emitted |= 8;
        }
    }
    first
}

fn stomp(c: &mut Construct, env: &Env<'_>, k: &Kind, prof: &Profile, out: &mut ConstructOut) {
    let (dm, km) = mults(c, prof);
    let ring = stomp_ring(k);
    let at = c.pos + flat(c.forward()) * (k.radius * 0.3);
    let ct = &env.ctx.tuning.creature; // TUNING HOOK
    let dmg = if k.kaiju { ct.stomp_damage_kaiju } else { ct.stomp_damage } * dm;
    let kb = if k.kaiju { ct.stomp_knockback_kaiju } else { ct.stomp_knockback } * km;
    // GIANTS: the foot blow, then a shockwave ring along the ground (a jump clears it).
    let giant_ring = c.ai.giant.on;
    if giant_ring {
        let impact = impact_of(c, k, AttackKind::Stomp);
        giant::stomp(c, env, &env.ctx.tuning.giants, at, dmg, kb, impact, out);
    }
    for t in env.ctx.enemies_of(c.owner).filter(|_| !giant_ring) {
        let d = flat(t.pos - at).length();
        if d > ring || (t.pos.y - at.y) > 2.0 + k.h * 0.1 || !c.hits.insert(player_key(t.id)) {
            continue;
        }
        let fall = 1.0 - 0.6 * (d / ring);
        let dir = flat(t.pos - at).normalize_or(Vec3::X) + Vec3::Y * 0.8;
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::Creature, dmg * fall, kb * fall, dir, t.pos)
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(impact_of(c, k, AttackKind::Stomp)),
        );
    }
    for q in env.others {
        if q.owner != c.owner && q.active && !q.attached && flat(q.center - at).length() < ring && c.hits.insert(q.id.0) {
            out.construct_hits.push(ConstructHit { pierce: false, blunt: false,
                target: q.id,
                source: Some(c.owner),
                damage: dmg,
                impulse: (flat(q.center - at).normalize_or_zero() + Vec3::Y) * kb * 0.5,
                blade: false,
                point: at,
            });
        }
    }
    let r = if k.kaiju { (k.h * 0.12).clamp(1.5, 5.0) } else { (0.6 + k.h * 0.15).min(1.6) };
    let earth = if c.recipe.element == Element::Earth { 1.26 } else { 1.0 };
    out.carves.push(CarveRequest { center: at, radius: r * earth, power: if k.kaiju { 0.9 } else { 0.6 } });
}

#[allow(clippy::too_many_arguments)]
fn tail_sweep(c: &mut Construct, env: &Env<'_>, k: &Kind, prof: &Profile, u0: f32, u1: f32, tm: &ai::Timing, out: &mut ConstructOut) {
    let (bd, _) = base_damage(c, env.ctx.tuning);
    let (dm, km) = mults(c, prof);
    let reach = k.radius + k.h * 0.9 + 1.0;
    let here = c.pos;
    // 360°: sweep angle progresses over the active window; everything within reach whose bearing
    // the tail passed this tick is hit (once).
    let back = -flat(c.forward()).normalize_or(Vec3::NEG_Z);
    // GIANTS: the tail sweeps along the ground — riders on the back are above it.
    let top = if c.ai.giant.on { giant::tail_top(c) } else { k.h + 1.5 };
    let span = tm.active.max(1e-3);
    let a0 = ((u0 - tm.anticipation) / span).clamp(0.0, 1.0) * std::f32::consts::TAU;
    let a1 = ((u1 - tm.anticipation) / span).clamp(0.0, 1.0) * std::f32::consts::TAU;
    for t in env.ctx.enemies_of(c.owner) {
        let d = flat(t.pos - here);
        if d.length() > reach || (t.pos.y - here.y).abs() > k.h + 1.5 || t.pos.y - here.y > top {
            continue;
        }
        let bearing = angle_from(back, d);
        if !(bearing >= a0 - 0.35 && bearing <= a1 + 0.35) || !c.hits.insert(player_key(t.id)) {
            continue;
        }
        // Tangential + outward: a big sideways swat.
        let tang = Vec3::new(-d.z, 0.0, d.x).normalize_or_zero();
        let dir = d.normalize_or_zero() * 0.6 + tang * 0.8 + Vec3::Y * 0.5;
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::Creature, bd * 1.3 * dm, env.ctx.tuning.creature.tail_knockback * km, dir, t.pos)
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(impact_of(c, k, AttackKind::TailSweep)),
        );
        c.ai.attack.emitted |= 8;
    }
    // The tail smashes terrain (big bodies): three carves along the sweep at the tip.
    if k.h >= 3.0 {
        for s in 1..=3 {
            let a = s as f32 / 3.0 * std::f32::consts::TAU;
            if a > a0 && a <= a1 {
                let tip = here + rot_y(back, -a) * (reach * 0.85) + Vec3::Y * 0.3;
                out.carves.push(CarveRequest { center: tip, radius: (k.h * 0.1).clamp(1.0, 3.5), power: 0.8 });
            }
        }
    }
}

/// Signed-free bearing of `d` measured from `from` counter-clockwise, in [0, 2π).
fn angle_from(from: Vec3, d: Vec3) -> f32 {
    let a = from.z.atan2(from.x);
    let b = d.z.atan2(d.x);
    (a - b).rem_euclid(std::f32::consts::TAU)
}

fn breath(c: &mut Construct, env: &Env<'_>, k: &Kind, prof: &Profile, out: &mut ConstructOut) {
    let dt = env.dt();
    c.ai.attack.pulse -= dt;
    if c.ai.attack.pulse > 0.0 {
        return;
    }
    c.ai.attack.pulse += BREATH_PULSE;
    let (dm, km) = mults(c, prof);
    let mouth = c.center() + flat(c.forward()) * (k.radius * 0.9) + Vec3::Y * (k.h * 0.3);
    let aim = c.ai.attack.aim.normalize_or(c.forward());
    let range = breath_range(k);
    let cos = BREATH_HALF_ANGLE.cos();
    let kb = if c.recipe.element == Element::Wind { 9.0 } else { 3.0 };
    for t in env.ctx.enemies_of(c.owner) {
        let p = t.hull.center();
        let d = p - mouth;
        let len = d.length();
        if len > range || (len > 0.5 && d.dot(aim) / len < cos) {
            continue;
        }
        if !ai::line_of_sight(env.ctx.voxels, mouth, p) {
            continue;
        }
        out.player_hits.push(
            HitInfo::new(t.id, Some(c.owner), HitKind::Creature, env.ctx.tuning.creature.breath_damage * dm, kb * km, aim, p)
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(0.5),
        );
        c.ai.attack.emitted |= 8;
    }
    let hits_constructs = temperament::of(c).hits_anyone || matches!(c.ai.target, Target::Construct(_));
    if hits_constructs {
        for q in env.others {
            let d = q.center - mouth;
            let len = d.length();
            if q.owner != c.owner && q.active && !q.attached && len < range && (len < 0.5 || d.dot(aim) / len >= cos) {
                out.construct_hits.push(ConstructHit { pierce: false, blunt: false,
                    target: q.id,
                    source: Some(c.owner),
                    damage: BREATH_DMG * dm,
                    impulse: aim * kb,
                    blade: false,
                    point: q.center,
                });
            }
        }
    }
    // "godzilla breathing bees": the payload streams out (≤ 3 per breath).
    let n = (c.ai.attack.emitted >> 4) & 3;
    if c.recipe.payload.is_some() && n < 3 {
        c.ai.attack.emitted = (c.ai.attack.emitted & 0x0F) | ((n + 1) << 4);
        spawn_payload(c, env, mouth, aim * 16.0, out);
    }
}

fn spit(c: &mut Construct, env: &Env<'_>, k: &Kind, out: &mut ConstructOut) {
    let mouth = c.center() + flat(c.forward()) * (k.radius * 0.9) + Vec3::Y * (k.h * 0.2);
    let mut aim = c.ai.attack.aim.normalize_or(c.forward());
    // Lead the target a little.
    if let Some(t) = target_view(env, c.ai.target) {
        let to = t.hull.center() - mouth;
        let tt = to.length() / 45.0;
        aim = (to + t.vel * tt * 0.6).normalize_or(aim);
    }
    if c.recipe.payload.is_some() {
        spawn_payload(c, env, mouth, aim * 24.0, out);
        return;
    }
    let (bd, bk) = base_damage(c, env.ctx.tuning);
    out.projectiles.push(ProjectileSpawn {
        owner: c.owner,
        construct: Some(c.id),
        kind: ProjectileKind::Bolt,
        pos: mouth,
        vel: aim * 45.0,
        damage: (bd * 0.75).max(3.0),
        knockback: bk * 0.7,
        element: c.recipe.element,
        radius: 0.3,
        life: 1.5,
        explode: None,
        hit_kind: HitKind::Shot,
        ricochet: 0,
        effects: 0,
        falloff_start: 0.0,
        chip: 0.0,
        muzzle_speed: 45.0,
    });
}

/// Launch a depth-1 payload construct (validated, no nested payload).
fn spawn_payload(c: &Construct, env: &Env<'_>, pos: Vec3, vel: Vec3, out: &mut ConstructOut) {
    let Some(p) = c.recipe.payload.as_deref() else { return };
    let mut r = p.clone();
    r.payload = None;
    let r = crate::willpower::validate_recipe(r, env.ctx.rules);
    let life = super::lifetime_secs(r.form).min(6.0);
    let params = SpawnParams {
        owner: c.owner,
        recipe: r,
        shape: None,
        pos,
        rot: geom::look_rotation(vel),
        vel,
        target: c.target,
        cost: 0.0,
        integrity: 10.0,
        forming: 0.05,
        life,
        tick: env.tick(),
        colossal: false,
        model: None,
    };
    out.queue.push(Interaction::Spawn { parent: c.id, params: Box::new(params) });
}

// ============================================================================ holds

fn cancel_hold(c: &mut Construct) {
    c.ai.hold = None;
    c.ai.hold_t = 0.0;
}

/// Grab: carry the player at the jaws for [`GRAB_SECS`], then fling. Constrict: squeeze (DoT) for up
/// to [`CONSTRICT_SECS`]. Firing a tether (the victim's skill save) breaks either hold.
fn step_hold(c: &mut Construct, env: &Env<'_>, k: &Kind, out: &mut ConstructOut) {
    let Some(pid) = c.ai.hold else { return };
    let dt = env.dt();
    let Some(t) = env.ctx.player(pid).copied().filter(|t| t.alive) else {
        cancel_hold(c);
        return;
    };
    let constrict = c.has(Mechanic::Constrict);
    c.ai.hold_t += dt;
    set_mode(c, Mode::Holding, out);
    // Tether breaks it: a short stagger for the creature.
    if t.pressed & (buttons::TETHER_L | buttons::TETHER_R) != 0 {
        cancel_hold(c);
        c.ai.cd[AttackKind::Grab as usize] = 2.0;
        c.ai.cd[AttackKind::Constrict as usize] = 2.0;
        out.impulses.push((pid, flat(t.pos - c.pos).normalize_or_zero() * 6.0 + Vec3::Y * 4.0, 0.0));
        return;
    }
    let fwd = flat(c.forward()).normalize_or(Vec3::NEG_Z);
    let hold_pt = if constrict { c.center() } else { c.center() + fwd * (k.radius + 0.7) + Vec3::Y * (k.h * 0.2) };
    let want = ((hold_pt - t.hull.center()) * 12.0).clamp_length_max(30.0) + c.vel;
    out.impulses.push((pid, want - t.vel, 0.0));
    let (bd, _) = base_damage(c, env.ctx.tuning);
    if constrict {
        c.ai.attack.pulse -= dt;
        if c.ai.attack.pulse <= 0.0 {
            c.ai.attack.pulse = 0.5;
            out.player_hits.push(
                HitInfo::new(pid, Some(c.owner), HitKind::Creature, (bd * 0.5).max(3.0), 0.5, Vec3::Y, t.hull.center())
                    .with_element(c.recipe.element)
                    .with_construct(c.id)
                    .with_impact(0.5),
            );
        }
        if c.ai.hold_t >= CONSTRICT_SECS {
            out.player_hits.push(
                HitInfo::new(pid, Some(c.owner), HitKind::Creature, 2.0, 10.0, fwd + Vec3::Y, t.hull.center())
                    .with_construct(c.id)
                    .with_impact(0.9),
            );
            cancel_hold(c);
        }
    } else if c.ai.hold_t >= GRAB_SECS {
        // Fling.
        let dir = fwd + Vec3::Y * 0.7;
        out.player_hits.push(
            HitInfo::new(pid, Some(c.owner), HitKind::Creature, bd, 20.0, dir, t.hull.center())
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(1.5),
        );
        out.events.push(GameEvent::CreatureAttack { id: c.id, kind: AttackKind::Grab as u8, phase: phase::IMPACT, aim: dir, impact: 1.5 });
        cancel_hold(c);
    }
}

// ============================================================================ piloting

/// What a pilot asks of a creature this tick (pilot.rs fills it from the seated player's input).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PilotControls {
    /// Desired horizontal direction (world, need not be unit; ZERO = stop).
    pub wish: Vec3,
    /// Fliers: −1 descend … +1 climb.
    pub ascend: f32,
    /// Walkers: jump (hop).
    pub jump: bool,
    /// Primary (LMB): the body's melee (bite / swipe / stomp for kaiju).
    pub primary: bool,
    /// Secondary (RMB): its signature move (breath > charge > tail sweep > swoop > spit).
    pub secondary: bool,
    /// Where the pilot aims (world direction).
    pub aim: Vec3,
}

/// Drive a piloted creature (a ridden dragon, a mech kaiju) with the creature's own locomotion,
/// attacks, timings and events. Call from `pilot::step_piloted` after claiming `Driver::Attached`.
pub fn piloted_step(c: &mut Construct, env: &Env<'_>, ctl: &PilotControls, out: &mut ConstructOut) {
    let dt = env.dt();
    c.body.loco = Loco::Kinematic;
    if !c.active() {
        return;
    }
    init(c);
    tick_timers(c, dt);
    let k = kind_of(c);
    let prof = temperament::of(c);
    let aim = ctl.aim.normalize_or(c.forward());
    let mut started = false;
    if !c.ai.attack.running {
        let pick = if ctl.secondary {
            [
                (c.has(Mechanic::Breath), AttackKind::Breath),
                (c.has(Mechanic::Charge) && !k.flier, AttackKind::Charge),
                (c.has(Mechanic::ArticulatedTail), AttackKind::TailSweep),
                (k.flier, AttackKind::Swoop),
                (true, AttackKind::Spit),
            ]
            .into_iter()
            .find(|(ok, _)| *ok)
            .map(|x| x.1)
        } else if ctl.primary {
            Some(if k.kaiju || c.has(Mechanic::Stomp) { AttackKind::Stomp } else { melee_kind(c, &k) })
        } else {
            None
        };
        if let Some(kind) = pick.filter(|kd| c.ai.cd[*kd as usize] <= 0.0) {
            start_attack(c, env, kind, aim, out);
            started = true;
        }
    }
    let mut scale = 1.0;
    if c.ai.attack.running && !started {
        // The pilot aims: keep the attack aim on the crosshair.
        c.ai.attack.aim = aim;
        scale = advance_attack(c, env, &k, &prof, out).max(0.25);
        if matches!(c.ai.attack.kind, AttackKind::Charge | AttackKind::Swoop)
            && c.ai.attack.t >= timing(c, c.ai.attack.kind).anticipation
            && c.ai.attack.running
        {
            integrate(c, env, &k, false);
            anim_moving(c, env, CHARGE_SPEED);
            return;
        }
    }
    if c.ai.hold.is_some() {
        step_hold(c, env, &k, out);
    }
    let want = flat(ctl.wish).normalize_or_zero() * k.speed * scale;
    let blend = (6.0 * dt).min(1.0);
    if k.flier || k.swimmer {
        let v = Vec3::new(want.x, ctl.ascend.clamp(-1.0, 1.0) * k.speed * 0.6, want.z);
        let v = ai::steer_air(env.ctx.voxels, c.center(), v, k.radius, 3.0, 0.0);
        c.vel += (v - c.vel) * blend;
        fly(c, env, &k);
    } else {
        c.vel.x += (want.x - c.vel.x) * blend;
        c.vel.z += (want.z - c.vel.z) * blend;
        if ctl.jump && c.grounded {
            c.vel.y = (7.0 + k.h * 0.3).min(14.0);
            c.grounded = false;
        }
        walk(c, env, &k, false);
    }
    let fa = flat(aim);
    if fa.length_squared() > 1e-4 {
        c.rot = c.rot.slerp(geom::yaw_rotation(fa), (8.0 * dt).min(1.0)).normalize();
    }
    anim_moving(c, env, k.speed);
    if k.kaiju {
        footsteps(c, env, &k, out);
    }
}

// ============================================================================ helpers

fn set_mode(c: &mut Construct, m: Mode, out: &mut ConstructOut) {
    c.ai.mode = m;
    if c.ai.sent_mode != m as u8 {
        c.ai.sent_mode = m as u8;
        out.events.push(GameEvent::CreatureMode { id: c.id, mode: m as u8 });
    }
}

#[inline]
fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

#[inline]
fn rot_y(d: Vec3, a: f32) -> Vec3 {
    let (s, co) = a.sin_cos();
    Vec3::new(d.x * co + d.z * s, d.y, -d.x * s + d.z * co)
}

/// Client helper: animation parameters for a creature (temperament mood + behaviour mode).
pub fn anim_params(c: &Construct) -> temperament::TemperamentAnim {
    temperament::anim_params(c.recipe.temperament, c.recipe.intensity)
}
