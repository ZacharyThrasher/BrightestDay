//! **Held guns** (COMBAT.md §4b — DOOM feel, never pea shooters) — held engineer.
//!
//! Called from `held.rs` for held constructs with a gun archetype (`weapons::archetype_for`: hand
//! cannon / super shotgun / minigun / rocket / railgun, plus bow / cannon / staff / disc). The
//! generic [`WeaponState`] machine (in `weapons.rs`) decides *when*; this turns shots into
//! projectiles (100 % shooter-velocity inheritance and lag compensation are applied by the world),
//! lag-compensated hitscan beams, recoil impulses (the world applies air ×1.0 / ground ×0.35) and
//! `ShotFired` juice (archetype, muzzle, recoil, pellets).
//!
//! * **Alt-fire (RMB) per archetype:** hand cannon = fan-the-hammer 3-shot burst (1.2 s cd); super
//!   shotgun = both barrels at once (double recoil: the big shotgun-jump), then reload-snap; minigun =
//!   pre-spin without firing; rocket = 3-rocket volley at half damage (2.4 s cd); railgun = scope
//!   (client-side zoom only); staff = continuous Beam while held if tagged `Beam`.
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
use crate::weapons::{GunArchetype, GunSpec, HitscanShot, WeaponPhase, WeaponState, archetype_for, pellet_dirs, spec_for};

/// Max fall speed (m/s) while hovering on a downward minigun stream.
pub const MINIGUN_SINK: f32 = 4.0;
/// Staff beam: damage per 0.1 s pulse, range.
pub const BEAM_DAMAGE: f32 = 3.0;
pub const BEAM_RANGE: f32 = 30.0;
pub const BEAM_PERIOD: f32 = 0.1;

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
    let spec = spec_for(&c.recipe, arch);
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
    let trigger = if arch == GunArchetype::Minigun { h1 || h2 } else { h1 };
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
                let n = pellet_dirs(o.look, 3, 5.0, &mut dirs);
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
                    owner: c.owner,
                    construct: Some(c.id),
                    origin: o.eye,
                    dir: o.look,
                    range: BEAM_RANGE,
                    damage: dmg,
                    knockback: 3.0,
                    element: c.recipe.element,
                    pierce: 0,
                    wall: 0.0,
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
    if speed <= 0.0 {
        out.hitscans.push(HitscanShot {
            owner: c.owner,
            construct: Some(c.id),
            origin: o.eye,
            dir,
            range: 250.0,
            damage: dmg,
            knockback: kb,
            element: c.recipe.element,
            pierce: spec.pierce,
            wall: 1.0,
        });
        return;
    }
    let ricochet = if c.has(Mechanic::Ricochet) || arch == GunArchetype::Disc { 2 } else { 0 };
    let mut dirs = [Vec3::ZERO; 16];
    let n = pellet_dirs(dir, spec.pellets, spec.spread_deg, &mut dirs);
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
            effects: ray_effects(c),
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
