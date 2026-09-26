//! **Held guns** (COMBAT.md §4b — DOOM feel, never pea shooters) — held engineer.
//!
//! Called from `held.rs` for held constructs with a gun archetype (`weapons::archetype_for`: hand
//! cannon / super shotgun / minigun / rocket / railgun, plus bow / cannon / staff / disc). The
//! generic [`WeaponState`] machine (in `weapons.rs`) decides *when*; this turns shots into
//! lag-compensated hitscan rays (hand cannon, per-pellet super shotgun, minigun, railgun) and
//! projectiles (rockets, bows, cannons, payload / explosive / ricochet / portal rounds; 100 %
//! shooter-velocity inheritance and lag compensation are applied by the world), recoil impulses
//! (the world applies air ×1.0 / ground ×0.35) and `ShotFired` juice (archetype, muzzle, recoil,
//! pellets). Hitscan rounds emit `ShotTrace` so clients draw a chunky voxel tracer.
//!
//! * **Alt-fire (RMB) per archetype:** hand cannon = fan-the-hammer 3-shot burst (1.2 s cd); super
//!   shotgun = both barrels at once (double recoil: the big shotgun-jump), then reload-snap; minigun =
//!   pre-spin without firing; rocket = 3-rocket volley at half damage (2.4 s cd); railgun = scope
//!   (client-side zoom only); staff = continuous Beam while held if tagged `Beam`.
//! * **ADS** (`buttons::ADS`, CAMERA): with a gun in hand the client maps RMB to aim-down-sights
//!   and alt-fire (`SECONDARY`) to its ALT FIRE key. ADS tightens pellet cones ([`ADS_SPREAD`]) and
//!   pre-spins the minigun; the railgun "scope" is the client's ADS zoom. Never touches movement.
//! * **Minigun hover:** firing while airborne and aiming down caps the fall at [`MINIGUN_SINK`] m/s
//!   (see [`movement_mods`]) — the recoil stream holds you up.
//! * **Payload rounds** (`recipe.payload`, "a cannon that shoots anvils / sharks"): slow, heavy,
//!   arcing novelty rounds (×0.55 speed, ×1.3 dmg, ×1.6 kb, big radius; explode if the payload
//!   would).
//! * **Dual wield** (`count = 2`): barrels alternate sides (cadence already halved by `spec_for`).
//! * Shrink / Grow / Link / Pull guns are rays: the effect rides the projectile (`effects` bits).

use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Form, Mechanic};
use bd_core::Vec3;

use super::held::HeldState;
use super::{AnimState, Construct, ConstructOut, Env};
use crate::combat::HitKind;
use crate::events::GameEvent;
use crate::geom;
use crate::movement::{MoveModifiers, PlayerState};
use crate::projectile::{ProjectileKind, ProjectileSpawn, effects};
use crate::weapons::{GunArchetype, GunSpec, HitscanShot, WeaponPhase, WeaponState, archetype_for, pellet_dirs};

/// Max fall speed (m/s) while hovering on a downward minigun stream.
pub const MINIGUN_SINK: f32 = 4.0;
/// Staff beam: damage per 0.1 s pulse, range.
pub const BEAM_DAMAGE: f32 = 3.0;
pub const BEAM_RANGE: f32 = 30.0;
pub const BEAM_PERIOD: f32 = 0.1;
/// Reach of hitscan gun rounds (m).
pub const HITSCAN_RANGE: f32 = 250.0;

/// CAMERA HOOK: aiming down sights (`buttons::ADS`) tightens a gun's pellet cone to this fraction.
/// Accuracy only — ADS never touches movement. Deterministic (a pure function of the tick's input).
pub const ADS_SPREAD: f32 = 0.6;

/// Projectile muzzle parallax (CAMERA): how far down the eye ray the convergence point is searched.
/// Past this the ~0.3 m hand offset is < 0.15°, i.e. nothing.
pub const CONVERGE_RANGE: f32 = 120.0;

/// Projectile muzzle parallax (CAMERA): the direction from `muzzle` to what the eye ray `eye → dir`
/// hits first (terrain or an enemy hull as the shooter saw it, lag-compensated), so a projectile
/// lands on the crosshair even point-blank — in both views, like hitscan (which starts at the eye)
/// and hooks (hand → the anchor the eye found). Unchanged when the point is not ahead of the muzzle
/// (a face-hug: the target is between the eye and the barrel's end).
pub fn converge_muzzle(env: &Env<'_>, owner: bd_core::PlayerId, eye: Vec3, dir: Vec3, muzzle: Vec3) -> Vec3 {
    let ctx = env.ctx;
    let mut t = bd_core::voxel::raycast(ctx.voxels, eye, dir, CONVERGE_RANGE).map_or(CONVERGE_RANGE, |h| h.t);
    for e in ctx.enemies_of(owner) {
        let hull = ctx.hull_for(owner, e);
        if let Some((f, _)) = geom::segment_aabb(eye, eye + dir * t, &hull) {
            t *= f;
        }
    }
    let to = eye + dir * t - muzzle;
    let len = to.length();
    if !len.is_finite() || len < 0.3 || to.dot(dir) < 0.8 * len { dir } else { to / len }
}

/// Pellet-cone multiplier for this tick's input.
#[inline]
pub fn ads_spread(input: &PlayerInput) -> f32 {
    if input.held(buttons::ADS) { ADS_SPREAD } else { 1.0 }
}

/// Does this held construct fire like a gun?
pub fn is_gun(c: &Construct) -> bool {
    archetype_for(&c.recipe).is_some()
}

fn ray_effects(c: &Construct) -> u8 {
    let mut e = 0;
    if c.has(Mechanic::Shrink) {
        e |= effects::SHRINK;
    }
    if c.has(Mechanic::Grow) {
        e |= effects::GROW;
    }
    if c.has(Mechanic::Link) {
        e |= effects::LINK;
    }
    if c.has(Mechanic::Pull) {
        e |= effects::PULL;
    }
    if c.has(Mechanic::Portal) {
        e |= effects::PORTAL;
    }
    e
}

/// One trigger step (called by `held::step` every tick while the gun is in hand).
pub fn step(c: &mut Construct, s: &mut HeldState, env: &Env<'_>, out: &mut ConstructOut) {
    let Some(arch) = archetype_for(&c.recipe) else { return };
    let o = env.owner;
    let dt = env.dt();
    let spec = env.ctx.tuning.spec_for(&c.recipe, arch); // TUNING HOOK
    let mut ws = match c.weapon {
        Some(w) if w.arch == arch => w,
        _ => WeaponState::new(arch, &spec),
    };
    let ok = !o.busy;
    let h1 = ok && o.input.held(buttons::PRIMARY);
    let r1 = o.released & buttons::PRIMARY != 0;
    let p2 = ok && o.pressed & buttons::SECONDARY != 0;
    let h2 = ok && o.input.held(buttons::SECONDARY);
    c.rot = geom::look_rotation(o.look);
    s.burst_t -= dt;
    s.beam_t -= dt;

    // Primary through the generic machine (the minigun's RMB pre-spins without firing).
    // CAMERA HOOK: aiming down sights (RMB with a gun) also pre-spins the minigun.
    let ads = ok && o.input.held(buttons::ADS);
    let trigger = if arch == GunArchetype::Minigun { h1 || h2 || ads } else { h1 };
    let cmd = ws.step(&spec, trigger, r1, dt);
    let cmd = if arch == GunArchetype::Minigun && !h1 { None } else { cmd };
    if let Some(f) = cmd {
        let mut dmg = 1.0;
        let mut speed = 1.0;
        if arch == GunArchetype::Bow {
            // Draw: 8 → 30 damage, faster arrow the further it is drawn.
            dmg = 8.0 / 30.0 + (1.0 - 8.0 / 30.0) * f.charge;
            speed = 0.55 + 0.45 * f.charge;
        }
        fire(c, env, out, arch, &spec, o.look, f.alt, dmg, speed, 1.0);
    }

    // Alt-fire.
    match arch {
        GunArchetype::SuperShotgun => {
            if p2 && ws.phase != WeaponPhase::Reloading && ws.cooldown <= 0.0 && ws.clip >= 2 {
                fire(c, env, out, arch, &spec, o.look, false, 1.0, 1.0, 1.0);
                fire(c, env, out, arch, &spec, o.look, true, 1.0, 1.0, 1.0);
                ws.clip = 0;
                ws.phase = WeaponPhase::Reloading;
                ws.t = 0.0;
                ws.cooldown = spec.cadence;
            }
        }
        GunArchetype::HandCannon => {
            if p2 && s.cd2 <= 0.0 && s.burst == 0 {
                s.burst = 3;
                s.burst_t = 0.0;
                s.cd2 = 1.2;
            }
            if s.burst > 0 && s.burst_t <= 0.0 {
                // Fixed, learnable fan: centre, a touch right, a touch left.
                let right = o.look.cross(Vec3::Y).normalize_or_zero();
                let k = [0.0f32, 0.035, -0.035][(3 - s.burst.min(3)) as usize % 3];
                let dir = (o.look + right * k).normalize_or_zero();
                fire(c, env, out, arch, &spec, dir, s.burst.is_multiple_of(2), 0.8, 1.0, 1.0);
                s.burst -= 1;
                s.burst_t = 0.08;
            }
        }
        GunArchetype::Rocket if p2 && s.cd2 <= 0.0 => {
            {
                let mut dirs = [Vec3::ZERO; 16];
                let n = pellet_dirs(o.look, 3, 5.0 * ads_spread(&o.input), &mut dirs);
                for (k, d) in dirs[..n].iter().enumerate() {
                    fire(c, env, out, arch, &spec, *d, k % 2 == 1, 0.5, 1.0, if k == 0 { 1.0 } else { 0.0 });
                }
                s.cd2 = 2.4;
            }
        }
        GunArchetype::Staff if h2 && c.has(Mechanic::Beam) && s.beam_t <= 0.0 => {
            {
                s.beam_t = BEAM_PERIOD;
                let dmg = BEAM_DAMAGE * if c.has(Mechanic::Heavy) { 1.5 } else { 1.0 };
                out.hitscans.push(HitscanShot {
                    construct: Some(c.id),
                    range: BEAM_RANGE,
                    element: c.recipe.element,
                    ..HitscanShot::new(c.owner, o.eye, o.look, dmg, 3.0)
                });
            }
        }
        _ => {}
    }

    c.weapon = Some(ws);
    let anim = match ws.phase {
        WeaponPhase::Charging => AnimState::Charging,
        WeaponPhase::SpinUp | WeaponPhase::SpinDown => AnimState::Windup,
        WeaponPhase::Firing => AnimState::Active,
        WeaponPhase::Reloading => AnimState::Recover,
        WeaponPhase::Idle if h2 && c.has(Mechanic::Beam) => AnimState::Active,
        WeaponPhase::Idle => AnimState::Idle,
    };
    c.set_anim(anim, env.tick());
}

/// Does this gun fire lag-compensated hitscan rays (vs projectiles)? Hitscan archetypes do, unless
/// the round's point is its flight: payloads, explosive rounds, ricochets, portal shots.
pub fn fires_hitscan(c: &Construct, arch: GunArchetype, spec: &GunSpec) -> bool {
    spec.speed <= 0.0
        || (spec.hitscan
            && c.recipe.payload.is_none()
            && spec.explode.is_none()
            && !c.has(Mechanic::Explodes)
            && !c.has(Mechanic::Ricochet)
            && !c.has(Mechanic::Portal)
            && arch != GunArchetype::Disc)
}

/// Fire one shot (all its pellets) along `dir`. `dmg_k` / `speed_k` scale the round; `recoil_k`
/// scales the kick (volleys kick once).
#[allow(clippy::too_many_arguments)]
fn fire(c: &Construct, env: &Env<'_>, out: &mut ConstructOut, arch: GunArchetype, spec: &GunSpec, dir: Vec3, alt: bool, dmg_k: f32, speed_k: f32, recoil_k: f32) {
    let o = env.owner;
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    let right = dir.cross(Vec3::Y).normalize_or_zero();
    let side = if c.recipe.count == 2 { if alt { 0.25 } else { -0.25 } } else { 0.0 };
    let muzzle = o.hand + dir * (c.half.z * 2.0) + right * side;
    // Hitscan starts at the eye; projectiles leave the muzzle toward the eye ray's hit (parallax).
    let ray = fires_hitscan(c, arch, spec);
    let dir = if ray { dir } else { converge_muzzle(env, c.owner, o.eye, dir, muzzle) };
    let mut dmg = spec.damage * dmg_k;
    let mut kb = spec.knockback;
    let mut speed = spec.speed * speed_k;
    let mut radius = spec.radius;
    let mut kind = spec.kind;
    let mut explode = spec.explode.or(if c.has(Mechanic::Explodes) { Some((2.5, 1.2)) } else { None });
    if let Some(p) = &c.recipe.payload {
        // Novelty rounds: slow, heavy, arcing — sharks, anvils, bees.
        if speed > 0.0 {
            speed *= 0.55;
            dmg *= 1.3;
            kb *= 1.6;
            radius = (radius * 3.0).clamp(0.3, 1.0);
            kind = ProjectileKind::CannonBall;
            if p.has(Mechanic::Explodes) || matches!(p.form, Form::Bomb | Form::Rocket) {
                explode = explode.or(Some((2.5, 1.2)));
            }
        }
    }
    let recoil = spec.recoil * recoil_k;
    out.events.push(GameEvent::ShotFired { owner: c.owner, construct: Some(c.id), archetype: arch, muzzle, dir, recoil, pellets: spec.pellets });
    if recoil > 0.0 {
        out.recoils.push((c.owner, dir, recoil));
    }
    let ricochet = if c.has(Mechanic::Ricochet) || arch == GunArchetype::Disc { 2 } else { 0 };
    let fx = ray_effects(c);
    // Hitscan (COMBAT.md §4b): players move 20–50 m/s, so the hand cannon, every super-shotgun
    // pellet, the minigun and the railgun are instant, lag-compensated rays. Rounds whose point is
    // the flight stay projectiles: payloads, explosive rounds, ricochets and portal shots.
    let mut dirs = [Vec3::ZERO; 16];
    let n = pellet_dirs(dir, spec.pellets, spec.spread_deg * ads_spread(&o.input), &mut dirs);
    if ray {
        let beam = arch == GunArchetype::Railgun || spec.pierce > 0;
        for d in &dirs[..n] {
            out.hitscans.push(HitscanShot {
                owner: c.owner,
                construct: Some(c.id),
                origin: o.eye,
                dir: *d,
                range: HITSCAN_RANGE,
                damage: dmg,
                knockback: kb,
                element: c.recipe.element,
                pierce: spec.pierce,
                wall: if beam { 1.0 } else { 0.0 },
                muzzle,
                falloff_start: spec.falloff_start,
                chip: spec.chip,
                effects: fx,
                trace: (!beam).then_some(arch),
            });
        }
        return;
    }
    for d in &dirs[..n] {
        out.projectiles.push(ProjectileSpawn {
            owner: c.owner,
            construct: Some(c.id),
            kind,
            pos: muzzle,
            vel: *d * speed,
            damage: dmg,
            knockback: kb,
            element: c.recipe.element,
            radius,
            life: 2.5,
            explode,
            hit_kind: HitKind::Shot,
            ricochet,
            effects: fx,
            falloff_start: spec.falloff_start,
            chip: spec.chip,
            muzzle_speed: speed,
        });
    }
}

/// Minigun hover (called from `worn::movement_mods` for the owner's held gun).
pub fn movement_mods(c: &Construct, input: &PlayerInput, st: &PlayerState, mods: &mut MoveModifiers) {
    let Some(w) = c.weapon else { return };
    if w.arch == GunArchetype::Minigun
        && w.phase == WeaponPhase::Firing
        && !st.grounded
        && input.held(buttons::PRIMARY)
        && input.look_dir().y < -0.5
    {
        mods.slow_fall = if mods.slow_fall > 0.0 { mods.slow_fall.min(MINIGUN_SINK) } else { MINIGUN_SINK };
    }
}
