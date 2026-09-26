//! The character controller: `step_player`.
//!
//! Order of a tick:
//! 1. timers, input edges; depenetrate (things may have moved into us)
//! 2. crouch / stand
//! 3. tethers: fire / release / hook flight / anchor refresh, then rope pull + reel (+ lift-off)
//! 4. jump (before friction ⇒ bhop keeps speed); 4b. air jump (`air_jump.rs`)
//! 5. ground (friction+accel or slide) / air (air-accel + gravity + modifiers), flight thrust
//! 6. gas boost; 6b. dodge (`dodge.rs`)
//! 7. rope constraint (inextensible, ratcheting)
//! 8. speed caps
//! 9. collide-and-slide with step-up (swept, never tunnels)
//! 10. ground categorisation, step-down snap, landing

use bd_core::Vec3;
use bd_core::input::{PlayerInput, buttons};
use bd_core::voxel::VoxelQuery;

use super::params::MoveParams;
use super::state::{BUTTON_BITS, HOP_CHAIN_MAX, LandEvent, MoveEvents, MoveModifiers, PlayerState, WallHit, hull_at};
use super::tether;
use super::world::{Broadphase, CollisionWorld, Hit, SKIN, safe_t};

/// Ground probe distance (m).
const GROUND_PROBE: f32 = 0.02;

/// Sanitised per-tick input with derived directions.
pub(crate) struct Inp {
    pub yaw: f32,
    pub pitch: f32,
    pub look: Vec3,
    /// Horizontal camera forward / right.
    pub fwd_h: Vec3,
    /// Horizontal wish direction (unit or zero).
    pub wish_h: Vec3,
    /// Camera-relative 3D wish (look-forward incl. pitch + strafe), unit or zero.
    pub wish3: Vec3,
    /// Only the forward key is held (no strafe keys): the W-strafe assist applies.
    pub fwd_only: bool,
    pub held: u16,
    pub pressed: u16,
    pub reel_notches: i8,
}

impl Inp {
    fn new(i: &PlayerInput, prev: u16) -> Self {
        let yaw = if i.yaw.is_finite() { i.yaw } else { 0.0 };
        let pitch = if i.pitch.is_finite() { i.pitch.clamp(-1.56, 1.56) } else { 0.0 };
        let typing = i.held(buttons::TYPING);
        let mut held = i.buttons & BUTTON_BITS;
        if typing {
            held &= !(buttons::JUMP | buttons::CROUCH | buttons::BOOST);
        }
        // TYPING keeps the move axes: the client latches the direction held when the ring opened
        // (voice == typing: talking never stops you, so typing mustn't either).
        let (mx, mz) = (i.move_x.clamp(-1, 1) as f32, i.move_z.clamp(-1, 1) as f32);
        let (sy, cy) = yaw.sin_cos();
        let fwd_h = Vec3::new(-sy, 0.0, -cy);
        let right = Vec3::new(cy, 0.0, -sy);
        let look = bd_core::input::look_dir(yaw, pitch);
        Self {
            yaw,
            pitch,
            look,
            fwd_h,
            wish_h: (fwd_h * mz + right * mx).normalize_or_zero(),
            wish3: (look * mz + right * mx).normalize_or_zero(),
            fwd_only: mz > 0.0 && mx == 0.0,
            held,
            pressed: held & !prev,
            reel_notches: i.reel_notches,
        }
    }
    #[inline]
    pub fn held(&self, b: u16) -> bool {
        self.held & b != 0
    }
    #[inline]
    pub fn pressed(&self, b: u16) -> bool {
        self.pressed & b != 0
    }
}

/// Source `Accelerate`: add up to `amount` along `dir`, never pushing the speed along `dir` past `wishspeed`.
#[inline]
fn accelerate(v: &mut Vec3, dir: Vec3, wishspeed: f32, amount: f32) {
    let add = wishspeed - v.dot(dir);
    if add > 0.0 {
        *v += dir * amount.min(add);
    }
}

/// W-strafe assist: holding only forward while the view is turned (|angle| < 90°) off the horizontal
/// velocity, swing the velocity toward the view — at most `k` × the per-tick turn a synced strafer gets
/// from `min(budget, cap)`, never past the view — and grant `k` × the speed a synced A/D strafer gains
/// for that rotation (an add of m = speed·turn along the strafe direction gains m·(cap − m/2)/speed).
/// Looking along the velocity (no turn) does nothing, so W alone never gains speed in a straight line.
#[inline]
fn wstrafe_assist(v: &mut Vec3, fwd: Vec3, k: f32, cap: f32, budget: f32) {
    let h = Vec3::new(v.x, 0.0, v.z);
    let sp = h.length();
    // (Inputs are finite: velocity and modifiers are sanitised at the top of the tick.)
    if sp <= 1.0 || cap <= 0.0 || budget <= 0.0 {
        return;
    }
    // Signed angle from the velocity to the view about +Y (positive = view is counter-clockwise).
    let e = h.cross(fwd).y.atan2(h.dot(fwd));
    if e.abs() >= std::f32::consts::FRAC_PI_2 {
        return;
    }
    let k = k.min(1.0);
    let turn = e.abs().min(k * budget.min(cap) / sp);
    if turn <= 0.0 {
        return;
    }
    let gain = k * turn * (cap - 0.5 * sp * turn);
    let (sn, cs) = turn.copysign(e).sin_cos();
    let scale = (sp + gain) / sp;
    v.x = (h.x * cs + h.z * sn) * scale;
    v.z = (h.z * cs - h.x * sn) * scale;
}

/// Source friction with stop-speed.
#[inline]
fn apply_friction(v: &mut Vec3, friction: f32, stop: f32, dt: f32, scale: f32) {
    let sp = v.length();
    if sp < 1e-3 {
        *v = Vec3::ZERO;
        return;
    }
    let drop = sp.max(stop) * friction * dt * scale;
    *v *= (sp - drop).max(0.0) / sp;
}

/// Horizontal direction laid onto a ground plane.
#[inline]
fn on_plane(dir: Vec3, n: Vec3) -> Vec3 {
    (dir - n * dir.dot(n)).normalize_or_zero()
}

/// Remove the into-surface component of `v` relative to a surface moving at `sv`.
#[inline]
fn clip(v: Vec3, n: Vec3, sv: Vec3) -> Vec3 {
    let into = (v - sv).dot(n);
    if into < 0.0 { v - n * into } else { v }
}

/// Advance the whole simulation of one player by `dt`. Pure: depends only on its arguments.
pub fn step_player<V: VoxelQuery + ?Sized>(
    s: &mut PlayerState,
    input: &PlayerInput,
    world: &CollisionWorld<V>,
    p: &MoveParams,
    dt: f32,
) -> MoveEvents {
    let mut ev = MoveEvents::default();
    if !(dt > 0.0 && dt.is_finite()) || !s.pos.is_finite() {
        return ev;
    }
    if !s.vel.is_finite() {
        s.vel = Vec3::ZERO;
    }
    // Garbage from outside (a 1e30 knockback, NaN/absurd construct modifiers) must never reach the
    // maths below: |v|² overflows to inf for |v| ≳ 1e19 and turns the tick into NaN. Both limits are
    // far outside legitimate play, so normal ticks are bit-identical with or without this.
    s.vel = clamp_speed(s.vel, p.hard_cap_speed * 4.0);
    sanitize_mods(&mut s.mods);
    s.extra.sanitize();
    let inp = Inp::new(input, s.prev_buttons & BUTTON_BITS);
    let mut chain = s.hop_chain.min(HOP_CHAIN_MAX);
    let speed0 = s.horizontal_speed();
    let mut bp = Broadphase::new();
    bp.prime(world, s.hull(p), s.vel * dt);

    // ---------------------------------------------------------------- 1. timers & depenetration
    s.jump_buffer = (s.jump_buffer - dt).max(0.0);
    if inp.pressed(buttons::JUMP) {
        s.jump_buffer = p.jump_buffer;
    }
    s.slide_cooldown = (s.slide_cooldown - dt).max(0.0);
    if s.tumble > 0.0 {
        s.tumble = (s.tumble - dt).max(0.0);
    }
    let height = s.height(p);
    if let Some((n, sv)) = bp.depenetrate(world, &mut s.pos, p.hull_half_width, height, p.step_height) {
        s.vel = clip(s.vel, n, sv);
    }

    // ---------------------------------------------------------------- 2. crouch
    update_crouch(s, &inp, &mut bp, world, p);

    // ---------------------------------------------------------------- 3. tethers
    tether::update_hooks(s, &inp, world, p, dt, &mut ev);
    let rope_acc = tether::apply_forces(s, &inp, p, dt);
    if s.grounded && tether::lifting(rope_acc, s, p) {
        s.grounded = false;
        s.sliding = false;
        s.coyote_ok = false;
    }

    // ---------------------------------------------------------------- 4. jump
    let mut jumped = false;
    let fresh = s.jump_buffer > 0.0;
    let wants_jump = fresh || (p.auto_hop && inp.held(buttons::JUMP));
    let coyote = !s.grounded && s.coyote_ok && s.air_time <= p.coyote_time;
    if wants_jump && (s.grounded || coyote) {
        // While grounded `since_landing` is the time since touchdown (0 on the touchdown tick).
        let from_ground = s.grounded;
        let touchdown = from_ground && s.since_landing < 0.5 * dt;
        if from_ground && s.since_landing < p.land_grace {
            chain = (chain + 1).min(HOP_CHAIN_MAX);
        }
        let gv = if s.grounded { s.ground_vel } else { Vec3::ZERO };
        if s.grounded && !fresh && p.autohop_friction > 0.0 && !s.sliding {
            let mut rel = s.vel - gv;
            apply_friction(&mut rel, p.friction, p.stop_speed, dt, p.autohop_friction);
            s.vel = rel + gv;
        }
        let rel_vy = (s.vel.y - gv.y).max(0.0);
        s.vel.y = gv.y + rel_vy + p.jump_speed * s.mods.jump_mult;
        s.grounded = false;
        s.coyote_ok = false;
        s.sliding = false;
        s.jump_buffer = 0.0;
        jumped = true;
        ev.jumped = true;
        ev.perfect_hop = fresh && touchdown;
    }
    super::air_jump::air_jump(s, &inp, input, world, p, jumped, &mut ev); // AIR JUMP (4b)

    // ---------------------------------------------------------------- 5. ground / air
    let g = p.gravity * s.mods.gravity_scale;
    let speed_mult = s.mods.speed_mult;
    if s.grounded {
        let n = s.ground_normal;
        let gv = s.ground_vel;
        let mut rel = s.vel - gv;
        if !s.sliding && s.crouched && inp.held(buttons::CROUCH) && rel.length() >= p.slide_min_speed {
            start_slide(s, &mut rel, p, &mut ev);
        }
        if s.sliding {
            // Gravity along the slope: sliding downhill gains speed.
            let ga = Vec3::new(0.0, -g, 0.0);
            rel += (ga - n * ga.dot(n)) * dt;
            let sp = rel.length();
            if sp > 1e-4 {
                rel *= (sp - p.slide_friction * dt).max(0.0) / sp;
            }
            if inp.wish_h != Vec3::ZERO {
                let w = on_plane(inp.wish_h, n);
                accelerate(&mut rel, w, p.slide_steer_speed, p.slide_steer_accel * dt);
            }
            if rel.length() < p.slide_exit_speed {
                s.sliding = false;
            }
        } else {
            // Landing grace: just after touchdown friction barely bites (a slightly late hop keeps its speed).
            let grace = if s.since_landing < p.land_grace { p.land_grace_friction } else { 1.0 };
            apply_friction(&mut rel, p.friction, p.stop_speed, dt, grace);
            if inp.wish_h != Vec3::ZERO {
                let ws = if s.crouched { p.crouch_speed } else { p.run_speed } * speed_mult;
                let w = on_plane(inp.wish_h, n);
                accelerate(&mut rel, w, ws, p.ground_accel * s.mods.accel_mult * ws * dt);
            }
        }
        rel -= n * rel.dot(n);
        s.vel = rel + gv;
    } else {
        if inp.wish_h != Vec3::ZERO {
            let mut ctrl = s.mods.air_control * s.mods.accel_mult;
            if s.tumble > 0.0 {
                ctrl *= p.tumble_air_control;
            }
            let ws = p.run_speed * speed_mult;
            let aa = super::surf::air_accel(s, p); // SURF: surf_air_accel on a surf face, else air_accel
            accelerate(&mut s.vel, inp.wish_h, ws.min(p.air_wish_cap), aa * ws * dt * ctrl);
            if inp.fwd_only && p.wstrafe_assist > 0.0 && s.mods.thrust <= 0.0 && !s.tethered() {
                wstrafe_assist(&mut s.vel, inp.fwd_h, p.wstrafe_assist, ws.min(p.air_wish_cap), p.air_accel * ws * dt * ctrl);
            }
        }
        s.vel.y -= g * dt;
        let m = s.mods;
        if m.glide > 0.0 && s.vel.y < 0.0 {
            let conv = ((-s.vel.y) * m.glide.min(1.0) * p.glide_rate * dt).min(-s.vel.y);
            s.vel.y += conv;
            s.vel += inp.fwd_h * (conv * p.glide_efficiency);
        }
        if m.slow_fall > 0.0 && s.vel.y < -m.slow_fall {
            // Cancel gravity and bleed the excess fall speed (settles in ~0.5 s, no instant stop).
            s.vel.y = (s.vel.y + g * dt).min(-m.slow_fall);
            s.vel.y += (-m.slow_fall - s.vel.y) * (6.0 * dt).min(1.0);
        }
    }
    if s.mods.thrust > 0.0 {
        let dir = if inp.wish3 != Vec3::ZERO { inp.wish3 } else { inp.look };
        accelerate(&mut s.vel, dir, s.mods.thrust_max_speed, s.mods.thrust * dt);
    }

    // ---------------------------------------------------------------- 6. gas boost
    update_boost(s, &inp, p, dt, &mut ev);
    super::dodge::step(s, &inp, input, p, dt, jumped || ev.air_jumped, &mut ev); // DODGE (6b)

    // ---------------------------------------------------------------- 7. ropes
    tether::constrain_velocity(s, p, dt);

    // ---------------------------------------------------------------- 8. speed caps
    let h = Vec3::new(s.vel.x, 0.0, s.vel.z);
    let hs = h.length();
    if hs > p.soft_cap_speed && !super::surf::surfing(s, p) {
        let e = hs - p.soft_cap_speed;
        let e2 = (e - (p.soft_cap_linear * e + p.soft_cap_quadratic * e * e) * dt).max(0.0);
        let k = (p.soft_cap_speed + e2) / hs;
        s.vel.x *= k;
        s.vel.z *= k;
    }
    let sp = s.vel.length();
    if sp > p.hard_cap_speed {
        s.vel *= p.hard_cap_speed / sp;
    }

    // ---------------------------------------------------------------- 9. move
    let was_grounded = s.grounded;
    let height = s.height(p);
    let allow_step = was_grounded || p.air_step;
    let mut out = MoveOut::default();
    let stepped = step_slide_move(&mut bp, world, p, &mut s.pos, &mut s.vel, height, dt, allow_step, &mut out, was_grounded);
    if stepped != 0.0 {
        ev.stepped += stepped;
        if s.sliding {
            stair_energy(s, p, g, -stepped);
        }
    }
    if let Some(w) = out.wall {
        if w.speed >= p.wall_hit_min_speed {
            ev.wall_hit = Some(w);
        }
    }

    // ---------------------------------------------------------------- 10. ground
    categorize(s, &inp, &mut bp, world, p, g, was_grounded, jumped, rope_acc, out.floor_impact, dt, &mut ev);

    // Hop chain: broken by real ground friction (stayed down past the grace) or a slide.
    if s.grounded && (s.sliding || s.since_landing >= p.land_grace) {
        chain = 0;
    }
    // AIR JUMP / DODGE: back on landing, a tether attach, or a (different) wall / surf-ramp touch.
    super::air_jump::refresh(s, out.wall, p, &mut ev);
    super::surf::update_contact(s, out.wall, p); // SURF: remember a surf face for next tick
    s.prev_buttons = inp.held;
    s.hop_chain = chain;
    ev.hop_chain = chain;
    ev.speed_gain = s.horizontal_speed() - speed0;
    ev
}

/// `v` with its length limited to `cap`, computed without overflowing for huge components.
fn clamp_speed(v: Vec3, cap: f32) -> Vec3 {
    let m = v.abs().max_element();
    if !(m > cap) && v.length_squared() <= cap * cap {
        return v;
    }
    (v / m).normalize_or_zero() * cap
}

/// Replace non-finite modifier values with defaults and clamp the rest to generous bounds (legit
/// constructs use single-digit multipliers; the bounds only stop inf/NaN arithmetic).
fn sanitize_mods(m: &mut MoveModifiers) {
    let d = MoveModifiers::default();
    let fix = |v: f32, default: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { default };
    m.gravity_scale = fix(m.gravity_scale, d.gravity_scale, -100.0, 100.0);
    m.glide = fix(m.glide, d.glide, 0.0, 100.0);
    m.thrust = fix(m.thrust, d.thrust, -1e4, 1e4);
    m.thrust_max_speed = fix(m.thrust_max_speed, d.thrust_max_speed, 0.0, 1e3);
    m.slow_fall = fix(m.slow_fall, d.slow_fall, -1e3, 1e3);
    m.speed_mult = fix(m.speed_mult, d.speed_mult, -100.0, 100.0);
    m.air_control = fix(m.air_control, d.air_control, -100.0, 100.0);
    m.jump_mult = fix(m.jump_mult, d.jump_mult, -100.0, 100.0);
    m.accel_mult = fix(m.accel_mult, d.accel_mult, -100.0, 100.0);
}

fn start_slide(s: &mut PlayerState, rel: &mut Vec3, p: &MoveParams, ev: &mut MoveEvents) {
    s.sliding = true;
    ev.slide_start = true;
    let sp = rel.length();
    if s.slide_cooldown <= 0.0 && sp < p.slide_burst_max_speed && sp > 1e-3 {
        let target = (sp + p.slide_burst).min(p.slide_burst_max_speed.max(sp));
        *rel *= target / sp;
        s.slide_cooldown = p.slide_cooldown;
        ev.slide_burst = true;
    }
}

/// Convert height changes on voxel stairs into slide speed (and back): stair "slopes" slide like ramps.
fn stair_energy(s: &mut PlayerState, p: &MoveParams, g: f32, drop: f32) {
    let rel = s.vel - s.ground_vel;
    let sp = rel.length();
    if sp < 1e-3 {
        return;
    }
    let e = (sp * sp + 2.0 * g * drop * p.slide_stair_gain).max(p.slide_exit_speed * p.slide_exit_speed);
    s.vel = s.ground_vel + rel * (e.sqrt() / sp);
}

fn update_crouch<V: VoxelQuery + ?Sized>(s: &mut PlayerState, inp: &Inp, bp: &mut Broadphase, w: &CollisionWorld<V>, p: &MoveParams) {
    let diff = p.stand_height - p.crouch_height;
    let want = inp.held(buttons::CROUCH);
    if want && !s.crouched {
        s.crouched = true;
        if !s.grounded {
            // Tuck the legs: the head stays put, feet come up (crouch-jump reaches higher ledges).
            s.pos.y += diff;
        }
    } else if !want && s.crouched {
        if s.grounded {
            if !bp.overlaps(w, hull_at(s.pos, p.hull_half_width, p.stand_height)) {
                s.crouched = false;
            }
        } else {
            // Untuck: lower the feet as far as there is room, grow up by the remainder.
            let small = hull_at(s.pos, p.hull_half_width, p.crouch_height);
            let d = Vec3::new(0.0, -diff, 0.0);
            let down = match bp.sweep(w, small, d) {
                None => diff,
                Some(h) => diff * safe_t(h.t, d, h.normal),
            };
            let np = s.pos - Vec3::Y * down;
            if !bp.overlaps(w, hull_at(np, p.hull_half_width, p.stand_height)) {
                s.pos = np;
                s.crouched = false;
            }
        }
        if !s.crouched {
            s.sliding = false;
        }
    }
    if s.sliding && !want {
        s.sliding = false;
    }
}

fn update_boost(s: &mut PlayerState, inp: &Inp, p: &MoveParams, dt: f32, ev: &mut MoveEvents) {
    s.burst_cooldown = (s.burst_cooldown - dt).max(0.0);
    let want = inp.held(buttons::BOOST);
    if want && s.gas > 0.0 {
        let dir = if inp.wish3 != Vec3::ZERO { inp.wish3 } else { inp.look };
        let taper = |along: f32| {
            let k = ((along - p.boost_taper_start) / (p.boost_taper_end - p.boost_taper_start).max(1e-3)).clamp(0.0, 1.0);
            1.0 - k * (1.0 - p.boost_taper_min)
        };
        if !s.boosting {
            ev.boost_start = true;
            if s.burst_cooldown <= 0.0 && s.gas >= p.burst_cost {
                s.vel += dir * (p.boost_burst * taper(s.vel.dot(dir)));
                s.gas -= p.burst_cost;
                s.burst_cooldown = p.burst_cooldown;
                ev.boost_burst = true;
            }
        }
        s.boosting = true;
        s.vel += dir * (p.boost_accel * taper(s.vel.dot(dir)) * dt);
        s.gas = (s.gas - p.gas_drain * dt).max(0.0);
        s.gas_regen_delay = p.gas_regen_delay;
        if dir.y > 0.0 && s.grounded && s.vel.y > p.ramp_launch_vy {
            s.grounded = false;
        }
    } else {
        s.boosting = false;
        if !want {
            if s.gas_regen_delay > 0.0 {
                s.gas_regen_delay = (s.gas_regen_delay - dt).max(0.0);
            } else {
                let rate = if s.grounded {
                    p.gas_regen_ground
                } else if s.tethered() {
                    p.gas_regen_tethered
                } else {
                    p.gas_regen_air
                };
                s.gas = (s.gas + rate * dt).min(p.gas_max);
            }
        }
    }
}

#[derive(Default)]
struct MoveOut {
    wall: Option<WallHit>,
    /// Largest into-surface speed against walkable ground during the move (landing impact).
    floor_impact: f32,
    /// A non-walkable surface blocked the move.
    steep: bool,
}

/// Quake/Source collide-and-slide: up to 4 bumps, clip against every plane touched, slide along creases,
/// stop in corners. Returns true if anything was hit; `steep` reports whether a non-walkable surface
/// blocked us (the only case where a step-up is attempted).
#[allow(clippy::too_many_arguments)]
fn slide_move<V: VoxelQuery + ?Sized>(
    bp: &mut Broadphase,
    w: &CollisionWorld<V>,
    p: &MoveParams,
    pos: &mut Vec3,
    vel: &mut Vec3,
    height: f32,
    dt: f32,
    out: &mut MoveOut,
) -> bool {
    let primal = *vel;
    let mut planes = [Vec3::ZERO; 5];
    let mut np = 0usize;
    let mut time_left = dt;
    let mut hit_any = false;
    for _ in 0..4 {
        let d = *vel * time_left;
        if d.length_squared() < 1e-14 {
            break;
        }
        let hull = hull_at(*pos, p.hull_half_width, height);
        let Some(h): Option<Hit> = bp.sweep(w, hull, d) else {
            *pos += d;
            break;
        };
        hit_any = true;
        let tm = safe_t(h.t, d, h.normal);
        *pos += d * tm;
        time_left *= 1.0 - tm;
        let into = -(*vel - h.vel).dot(h.normal);
        if h.normal.y < p.walkable_normal_y {
            out.steep = true;
            if h.normal.y > -0.7 && out.wall.is_none_or(|wh| into > wh.speed) {
                out.wall = Some(WallHit { speed: into, normal: h.normal });
            }
        } else {
            out.floor_impact = out.floor_impact.max(into);
        }
        if np == planes.len() {
            *vel = Vec3::ZERO;
            break;
        }
        if !planes[..np].iter().any(|q| q.dot(h.normal) > 0.999) {
            planes[np] = h.normal;
            np += 1;
        } else if tm == 0.0 {
            // Same plane again without progress: nudge off it (Quake's fix for tiny oscillations).
            *pos += h.normal * (SKIN * 0.5);
        }
        // Find a velocity that clips against the new plane and doesn't re-enter any other.
        let mut resolved = None;
        for i in 0..np {
            let v = clip(*vel, planes[i], if planes[i] == h.normal { h.vel } else { Vec3::ZERO });
            if (0..np).all(|j| j == i || v.dot(planes[j]) >= -1e-5) {
                resolved = Some(v);
                break;
            }
        }
        match resolved {
            Some(v) => *vel = v,
            None if np == 2 => {
                let dir = planes[0].cross(planes[1]).normalize_or_zero();
                *vel = dir * dir.dot(*vel);
            }
            None => {
                *vel = Vec3::ZERO;
                break;
            }
        }
        if np >= 2 && vel.dot(primal) <= 0.0 {
            *vel = Vec3::ZERO;
            break;
        }
    }
    hit_any
}

/// Slide move with Quake-style step-up. Returns the vertical step offset applied (0 if none).
#[allow(clippy::too_many_arguments)]
fn step_slide_move<V: VoxelQuery + ?Sized>(
    bp: &mut Broadphase,
    w: &CollisionWorld<V>,
    p: &MoveParams,
    pos: &mut Vec3,
    vel: &mut Vec3,
    height: f32,
    dt: f32,
    allow_step: bool,
    out: &mut MoveOut,
    grounded: bool,
) -> f32 {
    let start_pos = *pos;
    let start_vel = *vel;
    let blocked = slide_move(bp, w, p, pos, vel, height, dt, out);
    if !blocked || !out.steep || !allow_step {
        return 0.0;
    }
    // Airborne "lip assist": only a small lip (clipping a ledge edge at the end of a hop, a seam where a
    // construct ramp meets terrain). Small enough that it never replaces crouch-jumping or mantles.
    let step_h = if grounded { p.step_height } else { p.air_step_height };
    let down_pos = *pos;
    // Up.
    let up = Vec3::new(0.0, step_h, 0.0);
    let hull0 = hull_at(start_pos, p.hull_half_width, height);
    let up_amt = match bp.sweep(w, hull0, up) {
        None => step_h,
        Some(h) => step_h * safe_t(h.t, up, h.normal),
    };
    if up_amt < 0.02 {
        return 0.0;
    }
    // Across.
    let mut spos = start_pos + Vec3::Y * up_amt;
    let mut svel = start_vel;
    let mut sout = MoveOut::default();
    slide_move(bp, w, p, &mut spos, &mut svel, height, dt, &mut sout);
    // Down onto the step.
    let dn = Vec3::new(0.0, -(up_amt + SKIN * 4.0 + (-start_vel.y * dt).max(0.0)), 0.0);
    match bp.sweep(w, hull_at(spos, p.hull_half_width, height), dn) {
        Some(h) if h.normal.y >= p.walkable_normal_y => {
            spos += dn * safe_t(h.t, dn, h.normal);
        }
        _ => return 0.0,
    }
    let prog_down = Vec3::new(down_pos.x - start_pos.x, 0.0, down_pos.z - start_pos.z).length_squared();
    let prog_step = Vec3::new(spos.x - start_pos.x, 0.0, spos.z - start_pos.z).length_squared();
    if prog_step <= prog_down + 1e-8 {
        return 0.0;
    }
    let expected_y = down_pos.y;
    *pos = spos;
    // Keep full speed through the step: velocity comes from the unobstructed path.
    *vel = svel;
    out.wall = sout.wall;
    out.floor_impact = out.floor_impact.max(sout.floor_impact).max(-start_vel.y);
    spos.y - expected_y
}

#[allow(clippy::too_many_arguments)]
fn categorize<V: VoxelQuery + ?Sized>(
    s: &mut PlayerState,
    inp: &Inp,
    bp: &mut Broadphase,
    w: &CollisionWorld<V>,
    p: &MoveParams,
    g: f32,
    was_grounded: bool,
    jumped: bool,
    rope_acc: Vec3,
    floor_impact: f32,
    dt: f32,
    ev: &mut MoveEvents,
) {
    let height = s.height(p);
    let lifted = tether::lifting(rope_acc, s, p);
    let hull = hull_at(s.pos, p.hull_half_width, height);
    let probe = Vec3::new(0.0, -GROUND_PROBE, 0.0);
    let mut ground: Option<Hit> = None;
    if !jumped && !lifted {
        // Sweeps ignore boxes the hull already overlaps by less than the depenetration epsilon. A
        // slide along a sloped construct can leave the hull a few µm *inside* the floor; the probe
        // then missed the floor and the step-down snap below fell straight through a 1-voxel floor
        // (found by tests/movement_props.rs). Retry from just above (only when the normal probe
        // finds nothing, so ordinary ticks are bit-identical).
        let lift = Vec3::new(0.0, SKIN, 0.0);
        let found = bp.sweep(w, hull, probe).or_else(|| bp.sweep(w, hull.translate(lift), probe - lift));
        if let Some(h) = found {
            let rel = s.vel - h.vel;
            let ok = h.normal.y >= p.walkable_normal_y
                && (rel.y <= p.ramp_launch_vy || (rel.length() <= p.ramp_slide_speed && rel.dot(h.normal) <= 0.5));
            if ok {
                ground = Some(h);
            }
        }
        // Step-down snap: stay glued to stairs / crests unless going fast enough to launch.
        if ground.is_none() && was_grounded {
            let d = Vec3::new(0.0, -(p.step_height + SKIN * 2.0), 0.0);
            if let Some(h) = bp.sweep(w, hull, d) {
                let rel = s.vel - h.vel;
                let ok = h.normal.y >= p.walkable_normal_y && (rel.y <= p.ramp_launch_vy || rel.length() <= p.ramp_slide_speed);
                if ok {
                    let t = safe_t(h.t, d, h.normal);
                    let drop = -d.y * t;
                    s.pos.y -= drop;
                    ev.stepped -= drop;
                    if s.sliding {
                        stair_energy(s, p, g, drop);
                    }
                    ground = Some(h);
                }
            }
        }
    }
    match ground {
        Some(h) => {
            let n = h.normal;
            let rel = s.vel - h.vel;
            let new_rel = if !was_grounded {
                let impact = (-rel.dot(n)).max(floor_impact).max(0.0);
                ev.landed = Some(LandEvent { impact_speed: impact });
                s.tumble = 0.0;
                rel - n * rel.dot(n)
            } else {
                // Ground-to-ground: lay velocity onto the new plane keeping its magnitude (crests,
                // ramp bottoms and slope changes never eat speed).
                let sp = rel.length();
                let proj = rel - n * rel.dot(n);
                let pl = proj.length();
                if pl > 1e-4 { proj * (sp / pl) } else { proj }
            };
            s.vel = new_rel + h.vel;
            s.grounded = true;
            s.ground_normal = n;
            s.ground_vel = h.vel;
            // Grounded: time since touchdown (bounded; only the first `land_grace` matters).
            s.since_landing = if was_grounded { (s.since_landing + dt).min(10.0) } else { 0.0 };
            s.air_time = 0.0;
            s.coyote_ok = true;
            if !was_grounded && s.crouched && inp.held(buttons::CROUCH) && !s.sliding && new_rel.length() >= p.slide_min_speed {
                let mut r = new_rel;
                start_slide(s, &mut r, p, ev);
                s.vel = r + h.vel;
            }
        }
        None => {
            // Left the ground this tick — or earlier this tick / between ticks (jump, tether lift,
            // boost launch, impulse): a non-positive air_time is still the grounded phase.
            if was_grounded || s.air_time <= 0.0 {
                s.air_time = 0.0;
            }
            s.air_time += dt;
            s.since_landing = 0.0;
            s.grounded = false;
            s.ground_normal = Vec3::Y;
            s.ground_vel = Vec3::ZERO;
            if jumped || lifted {
                s.coyote_ok = false;
            }
        }
    }
}
