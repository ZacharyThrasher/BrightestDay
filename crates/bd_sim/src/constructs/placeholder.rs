//! **PLACEHOLDER** construct behaviours — one simple controller per form category so the whole
//! pipeline is exercised and bots can fight. To be replaced by the component system
//! (COMBAT.md §5b). Deliberately small and readable; it only talks to the world through
//! [`ConstructCtx`] (read) and [`ConstructOut`] (intents).
//!
//! * **Held** — rides the owner's right hand. Melee forms: tap PRIMARY = quick swing (70/90/160 ms),
//!   hold = charge ≤ 0.7 s → heavy (×2.2 dmg, ×1.8 kb); swept arc sampled 3× per tick. Ranged
//!   forms fire projectiles (gun auto, bow draw, cannon recoil = rocket-jump, staff bolt, disc).
//! * **Worn** — wings glide / parachute slow-fall + tumble cancel / jetpack thrust / cape.
//! * **Object** — thrown ballistic (or dropped from the sky), swept vs players/constructs/terrain;
//!   bounces (Ball/Bounce/Ricochet), explodes (Bomb/Rocket/Explodes), homes (Rocket/Homing),
//!   returns (Returning), sticks (Sticky), craters (Anvil/Heavy/Blunt).
//! * **Structure** — static at the aim point; trampolines bounce; Dome/Cage are non-solid.
//! * **Creature** (and anything with Walks/Flies) — walks/flies to the nearest enemy (or the
//!   owner's last target), bites; Charge dashes; Kaiju stomps (carves). Pilots drive it instead.
//! * **Clash** — overlapping opposing constructs trade integrity by relative speed; two swinging
//!   weapons knock both wielders back (sword-lock).

use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, FormCategory, Mechanic, Temperament};
use bd_core::voxel::{VoxelQuery, raycast, world_to_voxel};
use bd_core::{PlayerId, Quat, Vec3};

use super::{AnimState, Construct, ConstructCtx, ConstructOut, PlayerView, form_spec, player_key};
use crate::combat::{CarveRequest, ConstructHit, Explosion, HitInfo, HitKind};
use crate::events::{GameEvent, GoneReason};
use crate::geom;
use crate::movement::{MoveEvents, MoveModifiers, MoveParams, PlayerState};
use crate::projectile::{ProjectileSpawn, effects};
use crate::weapons::{GunArchetype, HitscanShot, WeaponPhase, WeaponState, archetype_for, pellet_dirs, spec_for};
use crate::status::StatusKind;

const WINDUP: f32 = 0.07;
const ACTIVE: f32 = 0.09;
const RECOVER: f32 = 0.16;
const CHARGE_MAX: f32 = 0.7;
const OBJECT_GRAVITY: f32 = 20.0;
const DROP_TELEGRAPH: f32 = 0.5;

fn mech_mult(c: &Construct) -> (f32, f32) {
    let mut dmg = 1.0;
    let mut kb = 1.0;
    if c.has(Mechanic::Heavy) {
        dmg *= 1.5;
        kb *= 1.5;
    }
    if c.has(Mechanic::Fast) {
        dmg *= 0.8;
    }
    if c.has(Mechanic::Blunt) {
        kb *= 1.3;
    }
    (dmg, kb)
}

fn hit_effects(c: &Construct) -> u8 {
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
    e
}

fn is_blade(c: &Construct) -> bool {
    c.has(Mechanic::Blade) || matches!(c.recipe.form, Form::Sword | Form::Axe)
}

/// Autonomous mover? (creatures, and any form taught to walk / fly — "a wall with legs").
fn autonomous(c: &Construct) -> bool {
    match c.category {
        FormCategory::Creature => true,
        FormCategory::Held | FormCategory::Worn => false,
        _ => c.has(Mechanic::Walks) || c.has(Mechanic::Flies),
    }
}

fn flies(c: &Construct) -> bool {
    c.has(Mechanic::Flies) || matches!(c.recipe.form, Form::Bird | Form::Insect)
}

// ------------------------------------------------------------------------------------ movement

pub(super) fn movement_mods(store: &super::ConstructStore, owner: PlayerId, input: &PlayerInput, st: &PlayerState, mods: &mut MoveModifiers) {
    let jump = input.held(buttons::JUMP) && !input.held(buttons::TYPING);
    for c in store.owned_by(owner).filter(|c| c.attached() && c.active()) {
        let air = !st.grounded;
        let form = c.recipe.form;
        let glide = matches!(form, Form::Wings) || c.has(Mechanic::Glide);
        let cape = matches!(form, Form::Cape);
        let chute = matches!(form, Form::Parachute) || c.has(Mechanic::SlowFall);
        let jet = matches!(form, Form::Jetpack) || c.has(Mechanic::Thrust);
        if jump && air {
            if glide {
                mods.glide = mods.glide.max(1.0);
            } else if cape {
                mods.glide = mods.glide.max(0.5);
            }
            if chute {
                mods.slow_fall = if mods.slow_fall > 0.0 { mods.slow_fall.min(3.0) } else { 3.0 };
            }
        }
        if jump && jet {
            mods.thrust = mods.thrust.max(30.0);
            mods.thrust_max_speed = 16.0;
        }
    }
}

pub(super) fn post_move(
    store: &super::ConstructStore,
    pid: PlayerId,
    input: &PlayerInput,
    st: &mut PlayerState,
    pre_vel: Vec3,
    ev: &MoveEvents,
    p: &MoveParams,
) {
    // Parachute: holding jump cancels tumble (a ring-out save).
    if input.held(buttons::JUMP) && st.tumble > 0.0 {
        let chute = store
            .owned_by(pid)
            .any(|c| c.active() && c.attached() && (c.recipe.form == Form::Parachute || c.has(Mechanic::SlowFall)));
        if chute {
            st.tumble = 0.0;
        }
    }
    // Bouncy / launch structures: landing on them flings you.
    if ev.landed.is_some() || st.grounded {
        let feet = st.pos;
        for c in store.iter() {
            if !c.active() || c.category != FormCategory::Structure {
                continue;
            }
            let bounce = c.recipe.form == Form::Trampoline || c.has(Mechanic::Bounce);
            let launch = c.has(Mechanic::Launch);
            if !bounce && !launch {
                continue;
            }
            let b = c.bounds();
            let hw = p.hull_half_width;
            let over = feet.x + hw > b.min.x && feet.x - hw < b.max.x && feet.z + hw > b.min.z && feet.z - hw < b.max.z;
            if !over || (feet.y - b.max.y).abs() > 0.3 {
                continue;
            }
            if bounce && pre_vel.y < -2.0 {
                let up = (-pre_vel.y * 1.1).clamp(12.0, 35.0);
                st.apply_impulse(Vec3::new(0.0, up - st.vel.y.max(0.0), 0.0), 0.0);
            } else if launch {
                let fwd = c.rot * Vec3::NEG_Z;
                st.apply_impulse(fwd * 14.0 + Vec3::Y * 12.0, 0.0);
            }
        }
    }
}

// ------------------------------------------------------------------------------------ update

pub(super) fn update(items: &mut [Construct], ctx: &ConstructCtx, out: &mut ConstructOut) {
    let dt = ctx.dt;
    for c in items.iter_mut() {
        if c.gone.is_some() {
            continue;
        }
        c.prev_pos = c.pos;
        c.age += dt;
        c.clash_cd = (c.clash_cd - dt).max(0.0);
        c.cooldown = (c.cooldown - dt).max(0.0);
        c.portal_cd = (c.portal_cd - dt).max(0.0);
        // Constructs outlive their owner's KO (normal upkeep/integrity/lifetime rules apply); they
        // only go when the owner leaves the match.
        let Some(owner) = ctx.player(c.owner).copied() else {
            c.gone = Some(GoneReason::OwnerGone);
            continue;
        };
        if c.forming_t > 0.0 {
            c.forming_t -= dt;
            if c.forming_t <= 0.0 {
                c.forming_t = 0.0;
                c.set_anim(AnimState::Idle, ctx.tick);
            }
        }
        if c.age >= c.life {
            if c.has(Mechanic::Explodes) || matches!(c.recipe.form, Form::Bomb) {
                explode(c, out);
            }
            c.gone = Some(GoneReason::Expired);
            continue;
        }
        if c.suit {
            suit(c, &owner, ctx);
        } else if c.attached() && !owner.alive {
            // Held/worn items wait on their (dead) owner; nothing to swing.
        } else if c.pilot.is_some() && !c.attached() {
            piloted(c, ctx, out);
        } else {
            match c.category {
                FormCategory::Held => held(c, &owner, ctx, out),
                FormCategory::Worn => worn(c, &owner, ctx, out),
                _ if autonomous(c) => creature(c, &owner, ctx, out),
                FormCategory::Object => object(c, &owner, ctx, out),
                FormCategory::Structure => structure(c, &owner, ctx, out),
                FormCategory::Creature => creature(c, &owner, ctx, out),
            }
        }
        if c.pos.y < ctx.void_y - 20.0 || !c.pos.is_finite() {
            c.gone = Some(GoneReason::Expired);
        }
    }
    clash(items, ctx, out);
}

// ------------------------------------------------------------------------------------ held

fn hand_pose(c: &mut Construct, o: &PlayerView) {
    c.pos = o.hand;
    let look = o.look;
    // Idle: weapon angled up-forward; the swing code overrides the direction while attacking.
    c.rot = geom::look_rotation((look + Vec3::Y * 0.6).normalize_or_zero());
}

fn held(c: &mut Construct, o: &PlayerView, ctx: &ConstructCtx, out: &mut ConstructOut) {
    let prev = c.pos;
    hand_pose(c, o);
    if !c.active() {
        return;
    }
    let dt = ctx.dt;
    if archetype_for(&c.recipe).is_some() {
        ranged(c, o, ctx, out);
    } else {
        melee(c, o, ctx, out);
    }
    c.vel = (c.pos - prev) / dt;
}

fn swing_dir(c: &Construct, o: &PlayerView, u: f32) -> (Vec3, f32) {
    let look = o.look;
    let right = look.cross(Vec3::Y).normalize_or_zero();
    let right = if right == Vec3::ZERO { Vec3::X } else { right };
    let up = right.cross(look).normalize_or_zero();
    match c.recipe.form {
        // Overhead arc: +70° above the look line to −50° below.
        Form::Hammer => {
            let a = (70.0 - 120.0 * u).to_radians();
            (look * a.cos() + up * a.sin(), 1.0)
        }
        // Thrusts: reach grows over the active window.
        Form::Spear | Form::Gauntlet | Form::Shield | Form::Whip => (look, 0.35 + 0.65 * u),
        // Horizontal cleave right → left.
        _ => {
            let a = (75.0 - 150.0 * u).to_radians();
            (look * a.cos() + right * a.sin(), 1.0)
        }
    }
}

fn melee(c: &mut Construct, o: &PlayerView, ctx: &ConstructCtx, out: &mut ConstructOut) {
    let dt = ctx.dt;
    let pace = if c.has(Mechanic::Fast) { 0.75 } else if c.has(Mechanic::Heavy) { 1.25 } else { 1.0 };
    let held_primary = o.input.held(buttons::PRIMARY) && !o.busy;
    c.timer += dt;
    match c.anim_state {
        AnimState::Idle | AnimState::Forming => {
            if o.pressed & buttons::PRIMARY != 0 && !o.busy {
                c.timer = 0.0;
                c.heavy = false;
                c.charge = 0.0;
                c.set_anim(AnimState::Windup, ctx.tick);
            }
        }
        AnimState::Windup => {
            if c.timer >= WINDUP * pace {
                if held_primary {
                    c.set_anim(AnimState::Charging, ctx.tick);
                } else {
                    start_swing(c, ctx);
                }
            }
        }
        AnimState::Charging => {
            c.charge += dt;
            if !held_primary || c.charge >= CHARGE_MAX {
                c.heavy = c.charge >= 0.2;
                start_swing(c, ctx);
            }
        }
        AnimState::Active => {
            let dur = ACTIVE * pace;
            let u_prev = c.swing_u;
            let u = (c.timer / dur).min(1.0);
            c.swing_u = u;
            let (d, _) = swing_dir(c, o, u);
            c.rot = geom::look_rotation(d);
            swing_hits(c, o, ctx, out, u_prev, u);
            if c.timer >= dur {
                c.timer = 0.0;
                c.set_anim(AnimState::Recover, ctx.tick);
            }
        }
        AnimState::Recover => {
            if c.timer >= RECOVER * pace {
                c.set_anim(AnimState::Idle, ctx.tick);
            }
        }
        _ => c.set_anim(AnimState::Idle, ctx.tick),
    }
}

fn start_swing(c: &mut Construct, ctx: &ConstructCtx) {
    c.timer = 0.0;
    c.swing_u = 0.0;
    c.hits.clear();
    c.set_anim(AnimState::Active, ctx.tick);
}

fn swing_hits(c: &mut Construct, o: &PlayerView, ctx: &ConstructCtx, out: &mut ConstructOut, u0: f32, u1: f32) {
    let spec = form_spec(c.recipe.form);
    let reach = (c.half.z * 2.0 + 0.3) * o.size.max(0.25) * if o.size > 1.0 { o.size.min(1.4) / o.size } else { 1.0 };
    let r = c.half.x.max(c.half.y).max(0.25);
    let (dm, km) = mech_mult(c);
    let (hd, hk) = if c.heavy { (2.2, 1.8) } else { (1.0, 1.0) };
    let kind = if c.heavy { HitKind::HeavyMelee } else { HitKind::Melee };
    let blade = is_blade(c);
    for s in 1..=3 {
        let u = u0 + (u1 - u0) * s as f32 / 3.0;
        let (d, ext) = swing_dir(c, o, u);
        for f in [0.4f32, 0.7, 1.0] {
            let p = o.hand + d * (reach * ext * f);
            for t in ctx.enemies_of(c.owner) {
                let hull = ctx.hull_for(c.owner, t);
                if geom::dist_sq_point_aabb(&hull, p) > r * r || !c.hits.insert(player_key(t.id)) {
                    continue;
                }
                let dir = (o.look + (hull.center() - o.eye).normalize_or_zero()) * 0.5;
                out.player_hits.push(
                    HitInfo::new(t.id, Some(c.owner), kind, spec.damage * dm * hd, spec.knockback * km * hk, dir, p)
                        .with_element(c.recipe.element)
                        .with_construct(c.id)
                        .with_impact(if c.heavy { 1.5 } else { 1.0 }),
                );
            }
        }
        // Weapon-vs-construct contact is resolved by the clash pass (a swinging weapon clashes).
        let _ = blade;
    }
}

/// Held guns: the generic [`WeaponState`] decides when; this turns shots into projectiles (pellets
/// with a fixed learnable spread), hitscan beams, recoil and fire juice.
fn ranged(c: &mut Construct, o: &PlayerView, ctx: &ConstructCtx, out: &mut ConstructOut) {
    let Some(arch) = archetype_for(&c.recipe) else { return };
    let spec = spec_for(&c.recipe, arch);
    let mut ws = match c.weapon {
        Some(w) if w.arch == arch => w,
        _ => WeaponState::new(arch, &spec),
    };
    let held = o.input.held(buttons::PRIMARY) && !o.busy;
    let released = o.released & buttons::PRIMARY != 0;
    let cmd = ws.step(&spec, held, released, ctx.dt);
    c.weapon = Some(ws);
    c.set_anim(
        match ws.phase {
            WeaponPhase::Charging => AnimState::Charging,
            WeaponPhase::SpinUp | WeaponPhase::SpinDown => AnimState::Windup,
            WeaponPhase::Firing => AnimState::Active,
            WeaponPhase::Reloading => AnimState::Recover,
            WeaponPhase::Idle => AnimState::Idle,
        },
        ctx.tick,
    );
    let Some(f) = cmd else { return };
    let right = o.look.cross(Vec3::Y).normalize_or_zero();
    let side = if c.recipe.count == 2 { if f.alt { 0.25 } else { -0.25 } } else { 0.0 };
    let muzzle = o.hand + o.look * (c.half.z * 2.0) + right * side;
    let (mut dmg, mut speed) = (spec.damage, spec.speed);
    if arch == GunArchetype::Bow {
        dmg *= 8.0 / 30.0 + (1.0 - 8.0 / 30.0) * f.charge;
        speed *= 0.55 + 0.45 * f.charge;
    }
    out.events.push(GameEvent::ShotFired { owner: c.owner, construct: Some(c.id), archetype: arch, muzzle, dir: o.look, recoil: spec.recoil, pellets: spec.pellets });
    out.recoils.push((c.owner, o.look, spec.recoil));
    if spec.speed <= 0.0 {
        out.hitscans.push(HitscanShot {
            owner: c.owner,
            construct: Some(c.id),
            origin: o.eye,
            dir: o.look,
            range: 250.0,
            damage: dmg,
            knockback: spec.knockback,
            element: c.recipe.element,
            pierce: spec.pierce,
            wall: 1.0,
        });
        return;
    }
    let explode = spec.explode.or(if c.has(Mechanic::Explodes) { Some((2.5, 1.2)) } else { None });
    let ricochet = if c.has(Mechanic::Ricochet) || arch == GunArchetype::Disc { 2 } else { 0 };
    let mut dirs = [Vec3::ZERO; 16];
    let n = pellet_dirs(o.look, spec.pellets, spec.spread_deg, &mut dirs);
    for d in &dirs[..n] {
        out.projectiles.push(ProjectileSpawn {
            owner: c.owner,
            construct: Some(c.id),
            kind: spec.kind,
            pos: muzzle,
            vel: *d * speed,
            damage: dmg,
            knockback: spec.knockback,
            element: c.recipe.element,
            radius: spec.radius,
            life: 2.5,
            explode,
            hit_kind: HitKind::Shot,
            ricochet,
            effects: hit_effects(c),
            falloff_start: spec.falloff_start,
            chip: spec.chip,
            muzzle_speed: speed,
        });
    }
}

// ------------------------------------------------------------------------------------ worn

/// Power suit (COMBAT.md §3d): wraps its pilot, who keeps normal movement (×0.85, applied by the
/// world). PLACEHOLDER: follows the pilot; the component system fires its mechanics on LMB/RMB.
fn suit(c: &mut Construct, o: &PlayerView, ctx: &ConstructCtx) {
    let pilot = c.pilot.and_then(|p| ctx.player(p)).copied().unwrap_or(*o);
    let prev = c.pos;
    c.pos = pilot.pos;
    c.rot = Quat::from_rotation_y(pilot.yaw);
    c.vel = (c.pos - prev) / ctx.dt.max(1e-6);
    if c.active() {
        c.set_anim(if pilot.vel.length_squared() > 1.0 { AnimState::Move } else { AnimState::Idle }, ctx.tick);
    }
}

fn worn(c: &mut Construct, o: &PlayerView, ctx: &ConstructCtx, out: &mut ConstructOut) {
    let back = Vec3::new(o.look.x, 0.0, o.look.z).normalize_or_zero();
    c.pos = o.hull.center() - back * 0.35;
    c.rot = geom::yaw_rotation(back);
    c.vel = o.vel;
    let thrusting = o.input.held(buttons::JUMP) && (c.recipe.form == Form::Jetpack || c.has(Mechanic::Thrust));
    if thrusting && c.active() {
        out.drain.push((c.owner, 6.0 * ctx.dt));
        c.set_anim(AnimState::Active, ctx.tick);
    } else if c.active() {
        c.set_anim(AnimState::Idle, ctx.tick);
    }
}

// ------------------------------------------------------------------------------------ objects

fn explode(c: &mut Construct, out: &mut ConstructOut) {
    let spec = form_spec(c.recipe.form);
    let (dm, km) = mech_mult(c);
    let r = if c.recipe.form == Form::Bomb { 4.0 } else { 3.0 } * (c.recipe.scale / spec.default_scale).sqrt().clamp(0.7, 3.0);
    let dmg = if spec.damage > 0.0 { spec.damage } else { 20.0 };
    out.explosions.push(Explosion {
        pos: c.center(),
        radius: r,
        damage: dmg * dm,
        knockback: spec.knockback.max(14.0) * km,
        source: Some(c.owner),
        construct: Some(c.id),
        element: c.recipe.element,
        carve_radius: r * 0.55,
        self_damage: false,
    });
}

fn object(c: &mut Construct, o: &PlayerView, ctx: &ConstructCtx, out: &mut ConstructOut) {
    let dt = ctx.dt;
    if !c.active() {
        // Forming in the hand; thrown when formed.
        if !c.has(Mechanic::DropFromSky) && c.recipe.form != Form::Anvil {
            c.pos = o.hand + o.look * (c.half.max_element() + 0.3);
            c.vel = Vec3::ZERO;
        }
        return;
    }
    let spec = form_spec(c.recipe.form);
    // Launch on the first active tick.
    if c.anim_state == AnimState::Idle {
        let drop = c.has(Mechanic::DropFromSky) || c.recipe.form == Form::Anvil;
        if drop {
            c.set_anim(AnimState::Telegraph, ctx.tick);
            c.timer = 0.0;
        } else {
            let mut speed = spec.speed.max(18.0);
            if c.has(Mechanic::Fast) {
                speed *= 1.4;
            }
            if c.has(Mechanic::Heavy) {
                speed *= 0.8;
            }
            c.vel = o.look * speed + o.vel * 0.5 + Vec3::Y * 2.0;
            c.set_anim(AnimState::Move, ctx.tick);
        }
        return;
    }
    if c.anim_state == AnimState::Telegraph {
        c.timer += dt;
        if c.timer >= DROP_TELEGRAPH {
            c.set_anim(AnimState::Move, ctx.tick);
        }
        return;
    }
    if let Some(p) = c.stuck_to {
        match ctx.player(p).filter(|t| t.alive) {
            Some(t) => {
                c.pos = t.pos + c.stuck_offset;
                c.vel = t.vel;
            }
            None => c.gone = Some(GoneReason::Spent),
        }
        if c.timer > 0.0 {
            c.timer -= dt;
            if c.timer <= 0.0 {
                explode(c, out);
                c.gone = Some(GoneReason::Spent);
            }
        }
        return;
    }
    let target = c.target.and_then(|t| ctx.player(t)).filter(|t| t.alive).copied().or_else(|| ctx.nearest_enemy(c.owner, c.pos).copied());
    // Steering.
    let homing = c.has(Mechanic::Homing) || c.recipe.form == Form::Rocket;
    let thrust = c.recipe.form == Form::Rocket || c.has(Mechanic::Thrust);
    if c.has(Mechanic::Returning) && (c.returning || c.age > 1.2) {
        c.returning = true;
        c.set_anim(AnimState::Returning, ctx.tick);
        let to = o.hand - c.pos;
        if to.length() < 1.5 {
            c.gone = Some(GoneReason::Spent);
            return;
        }
        c.vel = to.normalize_or_zero() * 30.0;
    } else {
        if homing {
            if let Some(t) = target {
                let want = (t.hull.center() - c.pos).normalize_or_zero();
                let cur = c.vel.normalize_or_zero();
                let turn = 2.5 * dt;
                let nd = (cur + (want - cur) * turn.min(1.0)).normalize_or_zero();
                c.vel = nd * c.vel.length().max(20.0);
            }
        }
        if !thrust {
            c.vel.y -= OBJECT_GRAVITY * dt;
        }
    }
    if c.vel.length_squared() > 1e-6 {
        c.rot = geom::look_rotation(c.vel);
    }
    // Swept move.
    let p0 = c.pos;
    let p1 = p0 + c.vel * dt;
    let r = c.half.min_element().max(0.15);
    let (dm, km) = mech_mult(c);
    // Players first (thrown things never hit their owner).
    let mut best_t = 2.0f32;
    let mut hit_player: Option<(PlayerId, Vec3)> = None;
    for t in ctx.enemies_of(c.owner) {
        if c.hits.contains(player_key(t.id)) {
            continue;
        }
        let hull = geom::inflate(&t.hull, r);
        if let Some((tt, _)) = geom::segment_aabb(p0, p1, &hull) {
            if tt < best_t {
                best_t = tt;
                hit_player = Some((t.id, p0 + (p1 - p0) * tt));
            }
        }
    }
    let seg = p1 - p0;
    let len = seg.length();
    let terrain = if len > 1e-5 { raycast(ctx.voxels, p0, seg, len + r) } else { None };
    let t_terrain = terrain.map(|h| (h.t - r).max(0.0) / len.max(1e-5)).unwrap_or(2.0);
    if let Some((pid, point)) = hit_player.filter(|_| best_t <= t_terrain) {
        c.hits.insert(player_key(pid));
        let kind = if c.has(Mechanic::DropFromSky) || c.recipe.form == Form::Anvil { HitKind::Crush } else { HitKind::Thrown };
        if c.has(Mechanic::Sticky) {
            if let Some(t) = ctx.player(pid) {
                c.stuck_to = Some(pid);
                c.stuck_offset = point - t.pos;
                c.timer = 1.0;
                c.set_anim(AnimState::Stuck, ctx.tick);
            }
            return;
        }
        if c.has(Mechanic::Explodes) || matches!(c.recipe.form, Form::Bomb | Form::Rocket) {
            c.pos = point;
            explode(c, out);
            c.gone = Some(GoneReason::Spent);
            return;
        }
        let dmg = spec.damage.max(4.0) * dm;
        out.player_hits.push(
            HitInfo::new(pid, Some(c.owner), kind, dmg, spec.knockback.max(6.0) * km, c.vel, point)
                .with_element(c.recipe.element)
                .with_construct(c.id)
                .with_impact(if kind == HitKind::Crush { 1.6 } else { 1.0 }),
        );
        c.returning = true;
        if c.has(Mechanic::Ricochet) || c.recipe.form == Form::Ball || c.has(Mechanic::Bounce) {
            c.vel = -c.vel * 0.5 + Vec3::Y * 4.0;
            c.pos = point;
        } else if !c.has(Mechanic::Returning) {
            c.gone = Some(GoneReason::Spent);
        }
        return;
    }
    if let Some(h) = terrain.filter(|_| t_terrain <= 1.0) {
        let n = h.normal.as_vec3();
        let point = h.point;
        if c.has(Mechanic::Sticky) {
            c.pos = point + n * r;
            c.vel = Vec3::ZERO;
            c.set_anim(AnimState::Stuck, ctx.tick);
            if c.has(Mechanic::Explodes) {
                explode(c, out);
                c.gone = Some(GoneReason::Spent);
            }
            return;
        }
        if c.has(Mechanic::Explodes) || matches!(c.recipe.form, Form::Bomb | Form::Rocket) {
            c.pos = point;
            explode(c, out);
            c.gone = Some(GoneReason::Spent);
            return;
        }
        let speed = c.vel.length();
        if c.has(Mechanic::DropFromSky) || c.recipe.form == Form::Anvil || (c.has(Mechanic::Heavy) && speed > 15.0) {
            let rad = (1.0 + c.recipe.scale * 0.3).min(4.0) * if c.recipe.element == Element::Earth { 1.26 } else { 1.0 };
            out.carves.push(CarveRequest { center: point, radius: rad, power: 0.9 });
            c.gone = Some(GoneReason::Spent);
            return;
        }
        let bouncy = c.recipe.form == Form::Ball || c.has(Mechanic::Bounce) || c.has(Mechanic::Ricochet);
        let max_b = if c.has(Mechanic::Ricochet) { 4 } else { 3 };
        if bouncy && c.bounces < max_b && speed > 3.0 {
            c.bounces += 1;
            let rest = if c.has(Mechanic::Bounce) { 1.0 } else { 0.7 };
            let mut v = c.vel - n * (1.0 + rest) * c.vel.dot(n);
            if c.has(Mechanic::Ricochet) {
                if let Some(t) = target {
                    let want = (t.hull.center() - point).normalize_or_zero();
                    if want.dot(v.normalize_or_zero()) > 25f32.to_radians().cos() * 0.5 {
                        v = want * v.length();
                    }
                }
            }
            c.vel = v;
            c.pos = point + n * (r + 0.02);
            c.returning = true;
            return;
        }
        if c.recipe.form == Form::Boulder || c.recipe.form == Form::Vehicle {
            // Roll: keep momentum along the surface.
            c.vel -= n * c.vel.dot(n);
            c.pos = point + n * (r + 0.02);
            return;
        }
        // Lands and rests; expires shortly.
        c.pos = point + n * (r + 0.02);
        c.vel = Vec3::ZERO;
        c.returning = true;
        c.life = c.life.min(c.age + 1.0);
        return;
    }
    c.pos = p1;
}

// ------------------------------------------------------------------------------------ structures

fn structure(c: &mut Construct, _o: &PlayerView, ctx: &ConstructCtx, out: &mut ConstructOut) {
    c.vel = Vec3::ZERO;
    if !c.active() {
        return;
    }
    // Cage (Trap): enemies inside are slowed hard (placeholder for the real snap-shut trap).
    if c.recipe.form == Form::Cage || c.has(Mechanic::Trap) {
        let b = c.bounds();
        for t in ctx.enemies_of(c.owner) {
            if b.intersects(&t.hull) {
                out.statuses.push((t.id, StatusKind::Chill, 0.2, Some(c.owner)));
            }
        }
    }
    if c.has(Mechanic::Spin) {
        c.rot = (Quat::from_rotation_y(2.0 * ctx.dt) * c.rot).normalize();
    }
}

// ------------------------------------------------------------------------------------ creatures

/// Highest walkable surface under (x, z) within [y − down, y + up]. Returns the top y.
fn ground_at<V: VoxelQuery + ?Sized>(v: &V, p: Vec3, up: f32, down: f32) -> Option<f32> {
    let top = world_to_voxel(p + Vec3::Y * up);
    let bottom = world_to_voxel(p - Vec3::Y * down);
    let mut y = top.y;
    while y >= bottom.y {
        let cell = bd_core::IVec3::new(top.x, y, top.z);
        if v.is_solid(cell) && !v.is_solid(cell + bd_core::IVec3::Y) {
            return Some((y + 1) as f32 * bd_core::VOXEL_SIZE);
        }
        y -= 1;
    }
    None
}

fn creature_speed(c: &Construct) -> f32 {
    let mut s = 9.0 * (c.recipe.scale.max(0.3) / 2.0).powf(-0.25);
    if c.has(Mechanic::Fast) {
        s *= 1.6;
    }
    if c.has(Mechanic::Heavy) {
        s *= 0.7;
    }
    if c.recipe.temperament == Temperament::Lazy {
        s *= 0.5;
    }
    s.clamp(2.0, 20.0)
}

fn pick_target(c: &Construct, o: &PlayerView, ctx: &ConstructCtx) -> Option<PlayerView> {
    if let Some(t) = o.last_target.and_then(|t| ctx.player(t)).filter(|t| t.alive && !t.shielded && t.id != c.owner) {
        if (t.pos - c.pos).length() < 60.0 {
            return Some(*t);
        }
    }
    let from = if c.recipe.temperament == Temperament::Protective { o.pos } else { c.pos };
    ctx.nearest_enemy(c.owner, from).copied()
}

fn creature(c: &mut Construct, o: &PlayerView, ctx: &ConstructCtx, out: &mut ConstructOut) {
    let dt = ctx.dt;
    if !c.active() {
        return;
    }
    let spec = form_spec(c.recipe.form);
    let target = pick_target(c, o, ctx);
    c.target = target.map(|t| t.id);
    let speed = creature_speed(c);
    let flier = flies(c);
    let reach = c.half.z.max(c.half.x) + 1.2;
    // Where to go.
    let mut goal = match target {
        Some(t) => t.pos,
        None => o.pos + Vec3::new(2.0, 0.0, 2.0),
    };
    if c.recipe.temperament == Temperament::Protective {
        if let Some(t) = target.filter(|t| (t.pos - o.pos).length() > 10.0) {
            let _ = t;
            goal = o.pos + (c.pos - o.pos).normalize_or_zero() * 2.5;
        }
    }
    let to = goal - c.pos;
    let dist_h = Vec3::new(to.x, 0.0, to.z).length();
    // Charge: telegraph 0.4 s then dash 25 m/s for 0.5 s.
    if c.has(Mechanic::Charge) {
        match c.anim_state {
            AnimState::Telegraph => {
                c.timer += dt;
                c.vel = Vec3::new(0.0, c.vel.y, 0.0);
                if c.timer >= 0.4 {
                    c.timer = 0.0;
                    c.hits.clear();
                    c.set_anim(AnimState::Active, ctx.tick);
                }
                integrate_creature(c, ctx, flier);
                return;
            }
            AnimState::Active => {
                c.timer += dt;
                let fwd = c.rot * Vec3::NEG_Z;
                c.vel = Vec3::new(fwd.x * 25.0, c.vel.y, fwd.z * 25.0);
                contact_hits(c, ctx, out, spec.damage * 1.2, 16.0, HitKind::Creature);
                if c.timer >= 0.5 {
                    c.timer = 0.0;
                    c.cooldown = 2.0;
                    c.set_anim(AnimState::Move, ctx.tick);
                }
                integrate_creature(c, ctx, flier);
                return;
            }
            _ => {
                if target.is_some() && c.cooldown <= 0.0 && (4.0..16.0).contains(&dist_h) {
                    c.rot = geom::yaw_rotation(to);
                    c.timer = 0.0;
                    c.set_anim(AnimState::Telegraph, ctx.tick);
                    return;
                }
            }
        }
    }
    // Steer.
    let desired = if flier {
        let aim = goal + Vec3::Y * 1.5;
        (aim - c.pos).normalize_or_zero() * speed
    } else {
        Vec3::new(to.x, 0.0, to.z).normalize_or_zero() * speed
    };
    let keep = if c.recipe.temperament == Temperament::Fearful && dist_h < 8.0 { -1.0 } else { 1.0 };
    let desired = desired * keep;
    let blend = (6.0 * dt).min(1.0);
    if flier {
        c.vel += (desired - c.vel) * blend;
    } else {
        c.vel.x += (desired.x - c.vel.x) * blend;
        c.vel.z += (desired.z - c.vel.z) * blend;
    }
    if Vec3::new(c.vel.x, 0.0, c.vel.z).length_squared() > 0.25 {
        c.rot = geom::yaw_rotation(c.vel);
        c.set_anim(AnimState::Move, ctx.tick);
    }
    integrate_creature(c, ctx, flier);
    // Attack.
    if let Some(t) = target {
        let d = geom::dist_sq_point_aabb(&t.hull, c.center());
        if d <= reach * reach && c.cooldown <= 0.0 {
            let (dm, km) = mech_mult(c);
            let dir = (t.hull.center() - c.center()).normalize_or_zero();
            out.player_hits.push(
                HitInfo::new(t.id, Some(c.owner), HitKind::Creature, spec.damage * dm, spec.knockback * km, dir, t.hull.center())
                    .with_element(c.recipe.element)
                    .with_construct(c.id),
            );
            c.cooldown = if c.recipe.temperament == Temperament::Aggressive { 0.7 } else { 1.0 };
            c.set_anim(AnimState::Active, ctx.tick);
        }
    }
    // Kaiju / Stomp: craters under the feet while walking, and a shockwave.
    if (c.recipe.form == Form::Kaiju || c.has(Mechanic::Stomp)) && c.grounded {
        c.charge += dt;
        if c.charge >= 0.9 {
            c.charge = 0.0;
            let r = (1.0 + c.recipe.scale * 0.06).clamp(1.0, 3.0);
            out.carves.push(CarveRequest { center: c.pos, radius: r, power: 0.6 });
            let ring = (c.recipe.scale * 0.35).clamp(2.0, 8.0);
            for t in ctx.enemies_of(c.owner) {
                if t.grounded && (t.pos - c.pos).length() < ring {
                    let dir = (t.pos - c.pos).normalize_or_zero() + Vec3::Y;
                    out.player_hits.push(
                        HitInfo::new(t.id, Some(c.owner), HitKind::Creature, 6.0, 12.0, dir, t.pos)
                            .with_construct(c.id)
                            .with_impact(1.4),
                    );
                }
            }
        }
    }
}

fn contact_hits(c: &mut Construct, ctx: &ConstructCtx, out: &mut ConstructOut, dmg: f32, kb: f32, kind: HitKind) {
    let b = c.bounds();
    for t in ctx.enemies_of(c.owner) {
        if b.intersects(&t.hull) && c.hits.insert(player_key(t.id)) {
            out.player_hits.push(
                HitInfo::new(t.id, Some(c.owner), kind, dmg, kb, c.vel, t.hull.center())
                    .with_element(c.recipe.element)
                    .with_construct(c.id)
                    .with_impact(1.3),
            );
        }
    }
}

fn integrate_creature(c: &mut Construct, ctx: &ConstructCtx, flier: bool) {
    let dt = ctx.dt;
    if flier {
        c.pos += c.vel * dt;
        // Don't fly into terrain: stop at it.
        if ctx.voxels.is_solid(world_to_voxel(c.center())) {
            c.pos -= c.vel * dt;
            c.vel = Vec3::ZERO;
        }
        c.grounded = false;
        return;
    }
    c.vel.y -= 25.0 * dt;
    let step_up = (c.recipe.scale * 0.2).clamp(1.0, 4.0);
    let next = c.pos + c.vel * dt;
    match ground_at(ctx.voxels, next, step_up, (c.vel.y.abs() * dt + 0.05).max(0.05)) {
        Some(gy) if gy >= next.y - 0.05 && c.vel.y <= 0.5 => {
            c.pos = Vec3::new(next.x, gy, next.z);
            c.vel.y = 0.0;
            c.grounded = true;
        }
        _ => {
            // Blocked by a wall taller than a hop? Stay and hop.
            let wall = ground_at(ctx.voxels, next, 30.0, 0.0).is_some_and(|gy| gy > c.pos.y + step_up);
            if wall && c.grounded {
                c.vel.x = 0.0;
                c.vel.z = 0.0;
                c.vel.y = 7.0;
                c.pos.y += c.vel.y * dt;
                c.grounded = false;
            } else {
                c.pos = next;
                c.grounded = false;
            }
        }
    }
}

fn piloted(c: &mut Construct, ctx: &ConstructCtx, out: &mut ConstructOut) {
    let Some(pv) = c.pilot.and_then(|p| ctx.player(p)).copied() else {
        c.pilot = None;
        return;
    };
    if !c.active() {
        return;
    }
    let dt = ctx.dt;
    let inp = pv.input;
    let (sy, cy) = inp.yaw.sin_cos();
    let fwd = Vec3::new(-sy, 0.0, -cy);
    let right = Vec3::new(cy, 0.0, -sy);
    let wish = (fwd * inp.move_z.clamp(-1, 1) as f32 + right * inp.move_x.clamp(-1, 1) as f32).normalize_or_zero();
    let speed = creature_speed(c);
    let flier = flies(c);
    let blend = (6.0 * dt).min(1.0);
    let mut want = wish * speed;
    if flier {
        if inp.held(buttons::JUMP) {
            want.y = speed * 0.6;
        } else if inp.held(buttons::CROUCH) {
            want.y = -speed * 0.6;
        }
        c.vel += (want - c.vel) * blend;
    } else {
        c.vel.x += (want.x - c.vel.x) * blend;
        c.vel.z += (want.z - c.vel.z) * blend;
        if inp.held(buttons::JUMP) && c.grounded {
            c.vel.y = 9.0;
            c.grounded = false;
        }
    }
    c.rot = Quat::from_rotation_y(inp.yaw);
    c.set_anim(if wish != Vec3::ZERO { AnimState::Move } else { AnimState::Idle }, ctx.tick);
    integrate_creature(c, ctx, flier);
    if pv.pressed & buttons::PRIMARY != 0 && c.cooldown <= 0.0 {
        let spec = form_spec(c.recipe.form);
        c.cooldown = 0.6;
        c.hits.clear();
        let reach = c.half.z.max(c.half.x) + 2.0;
        let front = c.center() + fwd * reach * 0.6;
        for t in ctx.enemies_of(c.owner) {
            if geom::dist_sq_point_aabb(&t.hull, front) <= reach * reach {
                out.player_hits.push(
                    HitInfo::new(t.id, Some(c.owner), HitKind::Creature, spec.damage.max(8.0), spec.knockback.max(10.0), fwd, t.hull.center())
                        .with_element(c.recipe.element)
                        .with_construct(c.id)
                        .with_impact(1.4),
                );
            }
        }
        c.set_anim(AnimState::Active, ctx.tick);
    }
}

// ------------------------------------------------------------------------------------ clash

fn clash(items: &mut [Construct], ctx: &ConstructCtx, out: &mut ConstructOut) {
    let n = items.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let (lo, hi) = items.split_at_mut(j);
            let a = &mut lo[i];
            let b = &mut hi[0];
            if a.owner == b.owner || !a.active() || !b.active() || a.clash_cd > 0.0 || b.clash_cd > 0.0 {
                continue;
            }
            // Worn things don't clash; held things only while swinging.
            let live = |c: &Construct| match c.category {
                FormCategory::Worn => false,
                FormCategory::Held => c.anim_state == AnimState::Active,
                _ => true,
            };
            if !live(a) || !live(b) {
                continue;
            }
            let rel = (a.vel - b.vel).length();
            let swinging = a.category == FormCategory::Held || b.category == FormCategory::Held;
            if rel < 3.0 && !swinging {
                continue;
            }
            if !a.bounds().intersects(&b.bounds()) {
                continue;
            }
            // Narrow phase: either centre within the other's inflated box.
            let ra = a.half.min_element().max(0.2);
            let rb = b.half.min_element().max(0.2);
            if !geom::sphere_obb(a.center(), ra + b.half.max_element() * 0.2, b.center(), b.half, b.rot)
                && !geom::sphere_obb(b.center(), rb + a.half.max_element() * 0.2, a.center(), a.half, a.rot)
            {
                continue;
            }
            let strength = (rel * 1.2 + 4.0).min(60.0);
            let point = (a.center() + b.center()) * 0.5;
            let n = (a.center() - b.center()).normalize_or_zero();
            a.clash_cd = 0.3;
            b.clash_cd = 0.3;
            out.construct_hits.push(ConstructHit { target: a.id, source: Some(b.owner), damage: strength, impulse: n * rel * 0.5, blade: is_blade(b), point });
            out.construct_hits.push(ConstructHit { target: b.id, source: Some(a.owner), damage: strength, impulse: -n * rel * 0.5, blade: is_blade(a), point });
            if a.category == FormCategory::Held && b.category == FormCategory::Held {
                // Sword-lock: both wielders knocked back a little.
                out.impulses.push((a.owner, n * 6.0 + Vec3::Y * 2.0, 0.0));
                out.impulses.push((b.owner, -n * 6.0 + Vec3::Y * 2.0, 0.0));
            }
            out.events.push(GameEvent::Clash { a: a.id, b: b.id, point, strength });
            let _ = ctx;
        }
    }
}
