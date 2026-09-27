//! AoTTG-style tethers: two independent hook projectiles, inextensible ratcheting ropes, reel, and the
//! crosshair anchor query (with a small aim-assist cone).

use bd_core::Vec3;
use bd_core::input::{buttons, look_dir};
use bd_core::voxel::{VoxelQuery, raycast, world_to_voxel};

use super::controller::Inp;
use super::params::MoveParams;
use super::state::{Hand, Hook, HookPhase, MoveEvents, PlayerState};
use super::world::{AnchorTarget, CollisionWorld, raycast_world};

/// Result of [`query_anchor`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorQuery {
    /// Where the hook would anchor.
    pub point: Vec3,
    pub normal: Vec3,
    pub target: AnchorTarget,
    /// Distance from the eye.
    pub dist: f32,
    /// Found by the aim-assist cone rather than the exact crosshair ray.
    pub assisted: bool,
}

/// Is there a valid anchor under the crosshair? Cheap enough to call every rendered frame (≤ 17 rays).
/// The client shows the result on the crosshair; firing uses the exact same query, so what you see is
/// what you get.
pub fn query_anchor<V: VoxelQuery + ?Sized>(
    w: &CollisionWorld<V>,
    eye: Vec3,
    look: Vec3,
    p: &MoveParams,
) -> Option<AnchorQuery> {
    let look = look.normalize_or_zero();
    if look == Vec3::ZERO {
        return None;
    }
    let range = p.tether_range;
    if let Some(h) = raycast_world(w, eye, look, range, true) {
        if !h.anchorable {
            // Blocked by something hooks can't grip: no anchor (and no assist through it).
            return None;
        }
        return Some(AnchorQuery { point: h.point, normal: h.normal, target: h.target, dist: h.t, assisted: false });
    }
    if p.aim_assist_deg <= 0.0 {
        return None;
    }
    // Aim assist: two rings of 8 rays; the innermost ring with any hit wins, nearest-to-centre first.
    let (u, v) = look.any_orthonormal_pair();
    for ring in [0.5f32, 1.0] {
        let ang = (p.aim_assist_deg * ring).to_radians();
        let (sa, ca) = ang.sin_cos();
        let mut best: Option<AnchorQuery> = None;
        for k in 0..8 {
            let phi = k as f32 * std::f32::consts::FRAC_PI_4;
            let (sp, cp) = phi.sin_cos();
            let dir = look * ca + (u * cp + v * sp) * sa;
            if let Some(h) = raycast_world(w, eye, dir, range, true).filter(|h| h.anchorable) {
                if best.is_none_or(|b| h.t < b.dist) {
                    best = Some(AnchorQuery { point: h.point, normal: h.normal, target: h.target, dist: h.t, assisted: true });
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

/// World position a hook is launched from.
pub fn hand_pos(s: &PlayerState, p: &MoveParams, yaw: f32, pitch: f32, hand: Hand) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let right = Vec3::new(cy, 0.0, -sy);
    let fwd = look_dir(yaw, pitch);
    let side = if hand == Hand::Left { -1.0 } else { 1.0 };
    s.eye(p) + right * (side * p.hand_offset.x) + Vec3::Y * p.hand_offset.y + fwd * p.hand_offset.z
}

/// Fire / cancel / release / advance hooks and refresh anchors on moving or destructible targets.
pub(crate) fn update_hooks<V: VoxelQuery + ?Sized>(
    s: &mut PlayerState,
    inp: &Inp,
    w: &CollisionWorld<V>,
    p: &MoveParams,
    dt: f32,
    ev: &mut MoveEvents,
) {
    for hand in Hand::BOTH {
        let i = hand as usize;
        let bit = hand.button();
        let held = inp.held(bit);
        match s.hooks[i].phase {
            HookPhase::Idle => {
                if inp.pressed(bit) {
                    fire(s, hand, inp, w, p, ev);
                    advance(s, i, w, p, dt, ev);
                }
            }
            HookPhase::Flying => {
                if held {
                    advance(s, i, w, p, dt, ev);
                } else {
                    s.hooks[i] = Hook::default();
                }
            }
            HookPhase::Attached => {
                if !held {
                    s.hooks[i] = Hook::default();
                    ev.tether_released[i] = true;
                } else if !refresh_anchor(&mut s.hooks[i], w) || rope_under_terrain(&s.hooks[i], s.chest(p), s.height(p) * 0.4 + 0.5, p.hull_half_width, w) {
                    s.hooks[i] = Hook::default();
                    ev.tether_released[i] = true;
                    ev.tether_broken[i] = true;
                }
            }
        }
    }
}

fn fire<V: VoxelQuery + ?Sized>(s: &mut PlayerState, hand: Hand, inp: &Inp, w: &CollisionWorld<V>, p: &MoveParams, ev: &mut MoveEvents) {
    let hp = hand_pos(s, p, inp.yaw, inp.pitch, hand);
    let eye = s.eye(p);
    let aim = match query_anchor(w, eye, inp.look, p) {
        Some(q) => q.point,
        None => eye + inp.look * p.tether_range,
    };
    let mut dir = (aim - hp).normalize_or_zero();
    if dir == Vec3::ZERO {
        dir = inp.look;
    }
    s.hooks[hand as usize] = Hook { phase: HookPhase::Flying, pos: hp, vel: dir * p.hook_speed, ..Hook::default() };
    ev.tether_fired[hand as usize] = true;
}

fn advance<V: VoxelQuery + ?Sized>(s: &mut PlayerState, i: usize, w: &CollisionWorld<V>, p: &MoveParams, dt: f32, ev: &mut MoveEvents) {
    let h = s.hooks[i];
    let speed = h.vel.length();
    if speed <= 0.0 {
        s.hooks[i] = Hook::default();
        return;
    }
    let dir = h.vel / speed;
    let seg = (speed * dt).min(p.tether_range - h.travelled).max(0.0);
    if let Some(hit) = raycast_world(w, h.pos, dir, seg, true) {
        if !hit.anchorable {
            // Glanced off something hooks can't grip.
            s.hooks[i] = Hook::default();
            ev.tether_missed[i] = true;
            return;
        }
        let mut hook = Hook {
            phase: HookPhase::Attached,
            pos: hit.point,
            vel: Vec3::ZERO,
            travelled: h.travelled + hit.t,
            target: Some(hit.target),
            local: Vec3::ZERO,
            rope_len: 0.0,
        };
        if let AnchorTarget::Collider(id) = hit.target {
            if let Some(c) = w.colliders.iter().find(|c| c.id == id) {
                hook.local = c.to_local(hit.point);
                hook.vel = c.surface_vel(hit.point);
            }
        }
        // The hook head never flies past `tether_range`, but the player keeps moving during the
        // flight. If they are now further away than the range, the rope attaches taut *at* the range
        // (and winds in at `rope_retract_speed`): a rope is never longer than `tether_range`.
        hook.rope_len = (hit.point - s.chest(p)).length().clamp(p.tether_min_len, p.tether_range.max(p.tether_min_len));
        s.hooks[i] = hook;
        s.tumble = 0.0;
        ev.tether_attached[i] = true;
        return;
    }
    let mut hook = h;
    hook.pos += dir * seg;
    hook.travelled += seg;
    if hook.travelled >= p.tether_range - 1e-3 {
        s.hooks[i] = Hook::default();
        ev.tether_missed[i] = true;
    } else {
        s.hooks[i] = hook;
    }
}

/// Follow moving colliders; detect destroyed anchors. Returns false if the anchor is gone.
fn refresh_anchor<V: VoxelQuery + ?Sized>(h: &mut Hook, w: &CollisionWorld<V>) -> bool {
    match h.target {
        Some(AnchorTarget::Voxel(v)) => w.voxels.is_solid(v),
        Some(AnchorTarget::Collider(id)) => match w.colliders.iter().find(|c| c.id == id && c.anchorable) {
            Some(c) => {
                h.pos = c.to_world(h.local);
                h.vel = c.surface_vel(h.pos);
                true
            }
            None => false,
        },
        None => false,
    }
}

/// Ropes ignore terrain once attached, so a swing past a ledge can carry the player in *under* it,
/// and the pull then pins them against the underside for good (alive under the floor, above the
/// void). The rope is cut (`tether_broken`; they drop) when it runs up through the underside of
/// solid terrain right over the player's head (within `head_room` m above the chest) **and** that
/// ceiling also roofs the whole hull footprint (`half_width`) under the anchor, i.e. sliding along
/// the underside toward the anchor can't get them round an edge. Unaffected: reeling back up from
/// just under a cliff lip (slides out round the corner), dangling clear below an overhang, side /
/// top faces (a rope over a cliff edge, round a pillar), an anchor on the underside itself (hanging
/// from a bridge). Voxels only; ropes on players (hook-and-strike) are exempt. One short ray (about
/// `head_room` along the rope) + 4 voxel reads when it hits: cheap every tick.
fn rope_under_terrain<V: VoxelQuery + ?Sized>(h: &Hook, chest: Vec3, head_room: f32, half_width: f32, w: &CollisionWorld<V>) -> bool {
    if matches!(h.target, Some(AnchorTarget::Collider(super::world::ColliderId::Player(_)))) {
        return false;
    }
    let d = h.pos - chest;
    let dist = d.length();
    // Only an anchor above can pull you up into a ceiling; the last 0.5 m is the anchor's own face.
    if !(d.y > 0.25 && dist > 1.0) {
        return false;
    }
    let reach = (dist - 0.5).min(head_room * dist / d.y);
    let Some(hit) = raycast(w.voxels, chest, d / dist, reach).filter(|hit| hit.t > 0.0 && hit.normal.y < 0) else {
        return false;
    };
    let y = hit.point.y + 0.25;
    [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .iter()
        .all(|&(sx, sz)| w.voxels.is_solid(world_to_voxel(Vec3::new(h.pos.x + sx * half_width, y, h.pos.z + sz * half_width))))
}

/// Rope pull, reel, notches and the ratchet. Returns the continuous acceleration applied (for the
/// lift-off check).
pub(crate) fn apply_forces(s: &mut PlayerState, inp: &Inp, p: &MoveParams, dt: f32) -> Vec3 {
    let n_att = s.hooks.iter().filter(|h| h.attached()).count();
    if n_att == 0 {
        return Vec3::ZERO;
    }
    let blend = if n_att == 2 { p.dual_pull_scale } else { 1.0 };
    let chest = s.chest(p);
    let reel = inp.held(buttons::REEL);
    let notches = inp.reel_notches as f32;
    let mut acc = Vec3::ZERO;
    for i in 0..2 {
        if !s.hooks[i].attached() {
            continue;
        }
        let d = s.hooks[i].pos - chest;
        let dist = d.length();
        if dist < 1e-4 {
            continue;
        }
        let mut dir = d / dist;
        // ASSIST HOOK: a rope on a player pulls in a flat upward arc (see `hook_player_lift`).
        if matches!(s.hooks[i].target, Some(AnchorTarget::Collider(super::world::ColliderId::Player(_)))) {
            dir = (dir + Vec3::Y * p.hook_player_lift).normalize_or(dir);
        }
        let hook = &mut s.hooks[i];
        // Ratchet: the rope never gets longer by itself.
        hook.rope_len = hook.rope_len.min(dist).max(p.tether_min_len);
        let fade = ((dist - p.tether_min_len) / p.rope_pull_fade.max(1e-3)).clamp(0.0, 1.0);
        let mut a = p.rope_pull;
        if reel && (s.vel - hook.vel).dot(dir) < p.reel_max_speed {
            a += p.reel_pull;
        }
        acc += dir * (a * blend * fade);
        if notches > 0.0 {
            hook.rope_len = (hook.rope_len - p.reel_notch_len * notches).max(p.tether_min_len);
            s.vel += dir * (p.reel_notch_impulse * notches * blend * fade);
        } else if notches < 0.0 {
            hook.rope_len = (hook.rope_len - p.reel_notch_len * notches).min(p.tether_range);
        }
    }
    s.vel += acc * dt;
    acc
}

/// Inextensible rope as a predictive position constraint on velocity: if this tick's motion would
/// stretch a rope, the velocity is bent so the chest ends exactly on the rope sphere. Outward radial
/// velocity disappears, tangential velocity (the swing) is kept.
pub(crate) fn constrain_velocity(s: &mut PlayerState, p: &MoveParams, dt: f32) {
    if !s.tethered() {
        return;
    }
    let chest = s.chest(p);
    for _ in 0..2 {
        for i in 0..2 {
            let h = s.hooks[i];
            // ASSIST HOOK: a rope on a player (hook-and-strike) only pulls — the victim never tows you.
            if !h.attached() || matches!(h.target, Some(AnchorTarget::Collider(super::world::ColliderId::Player(_)))) {
                continue;
            }
            // Work relative to the anchor's motion.
            let anchor_next = h.pos + h.vel * dt;
            // A rope that is shorter than the current distance (wheel reel, pushed by a wall) winds in
            // at a bounded speed instead of teleporting the player.
            let cur = (chest - h.pos).length();
            let len = h.rope_len.max(cur - p.rope_retract_speed * dt);
            let pred = chest + s.vel * dt;
            let off = pred - anchor_next;
            let dist = off.length();
            if dist > len && dist > 1e-5 {
                let target = anchor_next + off * (len / dist);
                s.vel = (target - chest) / dt;
            }
        }
    }
}

/// True if the ropes are currently holding the player up off the ground.
pub(crate) fn lifting(acc: Vec3, s: &PlayerState, p: &MoveParams) -> bool {
    acc.y > p.gravity * s.mods.gravity_scale * p.tether_lift_frac
}
