//! Player-body layers (ANIMATION PASS): the humanoid costume rig's continuous **motion** (air by
//! vertical speed, bank into turns / air-strafe, strafe + backpedal lean, crouch, gas boost), one-shot
//! **actions** (landing squash by fall height, air-jump flip, dodge lean, tether-release fling) and
//! **channels** (typing at the ring, reciting the oath, charging). Layers on top of `Locomotion` like
//! any other [`AnimState`](crate::AnimState); the client (`bd_game::present::body_motion`) derives
//! every input from replicated state — nothing here is simulated, nothing touches hitboxes.
//!
//! Conventions (model faces −Z, +X = the body's right): on a hanging limb `rx(+a)` swings it forward,
//! `rz(side·a)` swings it out; on the root `rx(−a)` leans the top forward, `rz(−a)` leans it right.
//! Limb chains (upper + fore arm) share a kind, so a limb rotation compounds down the chain: amplitudes
//! below are per segment and small.

use glam::{Quat, Vec3};

use crate::anim::{AnimState, Local, PartKind, about};
use crate::ease::{Ease, K, smoothstep, track};
use crate::voxelize::VoxelModel;

use std::f32::consts::{PI, TAU};

/// Continuous body state (all velocities body-local, m/s; weights 0..1 already eased by the caller).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyMotion {
    /// Forward speed (+ = the way the body faces).
    pub fwd: f32,
    /// Sideways speed (+ = right).
    pub side: f32,
    /// Vertical speed (+ = up).
    pub vy: f32,
    /// Yaw rate (rad/s, + = turning left).
    pub turn: f32,
    /// 0 grounded … 1 airborne.
    pub air: f32,
    /// 0 standing … 1 crouched (not sliding).
    pub crouch: f32,
    /// 0 … 1 gas boost held.
    pub boost: f32,
}

/// One-shot body actions (`t` = seconds since it started).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BodyAction {
    /// Touchdown squash + recover; `strength` 0..1.5 from the impact speed (≥ 1 = the big crouch).
    #[default]
    Land,
    /// The air jump: tuck and flip about the axis ⟂ `dir` (`strength` = 1 full flip, 0 = tuck only).
    AirJump,
    /// A dodge: low twisted lean toward `dir`.
    Dodge,
    /// Let go of the tethers: arms fling open, knees tuck (`strength` ≥ 1 = a forward flip too).
    Release,
}

impl BodyAction {
    /// Seconds the action takes at `strength` (the caller drops the layer after this).
    pub fn duration(self, strength: f32) -> f32 {
        match self {
            BodyAction::Land => land_timing(strength).3,
            BodyAction::AirJump => 0.4,
            BodyAction::Dodge => 0.3,
            BodyAction::Release => 0.5,
        }
    }
}

/// Held poses that say "this player is busy with the ring" (`t` = seconds since it began, `w` =
/// eased weight so the pose can fade out).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    /// Ring console open / push-to-talk: ring fist up at the face, the off hand steadying the wrist,
    /// head bowed to the ring, a little tap per beat.
    #[default]
    Typing,
    /// Reciting the oath: ring fist raised high, off fist at the chest, chin up.
    Oath,
    /// Charging willpower: planted wide stance, fists pulled to the hips, trembling harder over time.
    Charge,
}

pub(crate) fn sanitize_motion(m: BodyMotion) -> BodyMotion {
    let f = |x: f32, lo: f32, hi: f32| if x.is_finite() { x.clamp(lo, hi) } else { 0.0 };
    BodyMotion {
        fwd: f(m.fwd, -200.0, 200.0),
        side: f(m.side, -200.0, 200.0),
        vy: f(m.vy, -200.0, 200.0),
        turn: f(m.turn, -60.0, 60.0),
        air: f(m.air, 0.0, 1.0),
        crouch: f(m.crouch, 0.0, 1.0),
        boost: f(m.boost, 0.0, 1.0),
    }
}

fn rx(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}
fn ry(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}
fn rz(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}

fn leg_side(m: &VoxelModel, k: u8) -> f32 {
    m.rig.legs.get(k as usize).map_or(1.0, |l| l.side)
}

/// The per-part delta of a body layer (`Motion`, `Action`, `Channel`); identity for anything else.
pub(crate) fn body_local(m: &VoxelModel, i: usize, a: AnimState, t: f32) -> Local {
    match a {
        AnimState::Motion(mo) => motion_local(m, i, &mo, t),
        AnimState::Action { kind, dir, strength } => action_local(m, i, kind, dir, strength, t),
        AnimState::Channel { kind, w } => channel_local(m, i, kind, w, t),
        _ => Local::IDENTITY,
    }
}

// ------------------------------------------------------------------------------------ motion

fn motion_local(m: &VoxelModel, i: usize, mo: &BodyMotion, t: f32) -> Local {
    let rig = &m.rig;
    let ll = rig.leg_len.max(0.1);
    let air = mo.air;
    let ground = 1.0 - air;
    let boost = mo.boost * air.max(0.35);
    let crouch = mo.crouch * ground;
    let hs = (mo.fwd * mo.fwd + mo.side * mo.side).sqrt();
    // Air: rising tuck, falling reach, plunge flail (by vertical speed).
    let rise = smoothstep(0.5, 7.0, mo.vy) * air;
    let fall = smoothstep(1.5, 14.0, -mo.vy) * air;
    let plunge = smoothstep(14.0, 30.0, -mo.vy) * air;
    let hang = (air - rise - fall).max(0.0);
    // Lean into the turn (air-strafe reads as a bank) and into sideways speed; backpedal leans back.
    let bank_air = (mo.turn * 0.085).clamp(-0.42, 0.42) * air;
    let bank_ground = (mo.turn * 0.035).clamp(-0.14, 0.14) * ground * smoothstep(4.0, 12.0, hs);
    let strafe = (-mo.side * 0.014).clamp(-0.2, 0.2);
    let roll = bank_air + bank_ground + strafe * (1.0 - 0.5 * boost);
    let back = 0.12 * smoothstep(1.0, 6.0, -mo.fwd) * ground;
    let sprint = 0.13 * smoothstep(11.0, 26.0, mo.fwd) * ground;
    // Root pitch (− = top forward) and the drop.
    let pitch = back - sprint - 0.14 * rise + 0.06 * fall - 0.3 * crouch - 0.55 * boost;
    let drop = 0.3 * ll * crouch;
    match rig.kinds[i] {
        PartKind::Root => {
            let hip = Vec3::Y * ll;
            let mut l = about(rz(roll) * rx(pitch), hip, m.parts[i].pivot);
            l.pos.y -= drop;
            l
        }
        PartKind::Head => {
            // Eyes stay on the horizon: undo most of the root lean / roll.
            Local::rot(rz(-roll * 0.55) * rx(-pitch * 0.6 - 0.1 * plunge))
        }
        PartKind::Leg(k) => {
            let s = leg_side(m, k);
            let lead = s < 0.0;
            // Hang: a relaxed stride; rise: both knees up; fall: legs reach down, apart; plunge: flutter.
            let stride = if lead { 0.42 } else { -0.22 };
            let flutter = 0.16 * plunge * (t * 13.0 + s * 1.9).sin();
            let air_ang = stride * (hang + 0.4 * rise) + 0.62 * rise + (0.06 + flutter) * fall;
            let air_out = s * (0.05 * rise + 0.16 * fall + 0.12 * plunge);
            // Crouch: world thigh angle ~0.8 (feet forward under the dropped hips), knees out.
            let crouch_ang = (0.8 + 0.3) * crouch;
            // Boost: legs trail back together (the gas-rig flight pose).
            let boost_ang = -0.35 * boost;
            Local::rot(rz(air_out + s * 0.2 * crouch - s * 0.05 * boost) * rx(air_ang + crouch_ang + boost_ang - pitch * air * 0.5))
        }
        PartKind::Arm(sd) => {
            let s = sd as f32;
            // Hop: arms a little back and out; rise: swing up; fall: rise out for balance; plunge: wave.
            let wave = 0.3 * plunge * (t * 9.0 + s * 1.3).sin();
            let fwd = -0.28 * hang + 0.35 * rise + 0.15 * fall + wave - 0.6 * boost + 0.25 * crouch;
            let out = 0.18 * hang + 0.12 * rise + (0.5 + 0.25 * plunge) * fall + 0.3 * boost + 0.1 * crouch;
            Local::rot(rz(s * out) * rx(fwd))
        }
        PartKind::Tail { k, .. } => Local::rot(rx(0.25 * fall / (1.0 + k as f32) - 0.2 * boost) * ry(-roll * 0.5)),
        _ => Local::IDENTITY,
    }
}

// ------------------------------------------------------------------------------------ actions

/// Landing envelope keys: (squash-in, hold end, recover end) and the total.
fn land_timing(strength: f32) -> (f32, f32, f32, f32) {
    let s = strength.clamp(0.0, 1.5);
    let t1 = 0.045;
    let t2 = t1 + 0.04 + 0.16 * smoothstep(0.6, 1.2, s);
    let t3 = t2 + 0.2 + 0.18 * s;
    (t1, t2, t3, t3)
}

fn action_local(m: &VoxelModel, i: usize, kind: BodyAction, dir: Vec3, strength: f32, t: f32) -> Local {
    let rig = &m.rig;
    let ll = rig.leg_len.max(0.1);
    let h = rig.height.max(0.2);
    let pivot = m.parts[i].pivot;
    let d = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::NEG_Z);
    let axis = Vec3::Y.cross(d).normalize_or(Vec3::NEG_X);
    let com = Vec3::Y * (h * 0.5);
    match kind {
        BodyAction::Land => {
            let s = strength.clamp(0.0, 1.5);
            let (t1, t2, t3, _) = land_timing(s);
            let e = track(t, &[K(0.0, 0.0, Ease::Linear), K(t1, 1.0, Ease::OutQuad), K(t2, 0.88, Ease::Linear), K(t3, 0.0, Ease::InOutSine)]);
            let k = e * s.min(1.0);
            let big = e * smoothstep(0.9, 1.3, s);
            match rig.kinds[i] {
                PartKind::Root => {
                    let lean = -0.32 * k - 0.2 * big;
                    let mut l = about(rx(lean), Vec3::Y * ll, pivot);
                    l.pos.y -= (0.24 * k + 0.06 * big) * ll;
                    let sq = 0.07 * k;
                    l.scale = Vec3::new(1.0 + sq, 1.0 - sq, 1.0 + sq);
                    l
                }
                PartKind::Leg(kk) => {
                    let side = leg_side(m, kk);
                    // Knees forward under the dropped hips; the big one staggers (lead foot forward,
                    // the other knee down behind).
                    let stag = if side < 0.0 { 0.45 } else { -1.5 };
                    Local::rot(rz(side * 0.22 * k) * rx(1.0 * k + 0.2 * big + stag * big))
                }
                PartKind::Arm(sd) => {
                    let s2 = sd as f32;
                    // Out for balance; the big one plants the ring fist on the ground, off arm back.
                    let (f, o) = if sd > 0 { (0.35 * k + 0.55 * big, 0.3 * k) } else { (0.2 * k - 0.6 * big, 0.45 * k + 0.35 * big) };
                    Local::rot(rz(s2 * o) * rx(f))
                }
                PartKind::Head => Local::rot(rx(0.18 * k + 0.12 * big)),
                PartKind::Tail { k: kk, .. } => Local::rot(rx(-0.3 * k / (1.0 + kk as f32))),
                _ => Local::IDENTITY,
            }
        }
        BodyAction::AirJump => {
            let dur = BodyAction::AirJump.duration(strength);
            let u = (t / dur).clamp(0.0, 1.0);
            let tuck = (PI * u).sin();
            let flip = if strength >= 0.5 { TAU * Ease::InOutSine.apply(u) } else { 0.0 };
            match rig.kinds[i] {
                PartKind::Root => {
                    let mut l = about(Quat::from_axis_angle(axis, flip), com, pivot);
                    let sq = 0.08 * tuck;
                    l.scale = Vec3::new(1.0 + sq * 0.5, 1.0 - sq, 1.0 + sq * 0.5);
                    l
                }
                PartKind::Leg(kk) => {
                    let side = leg_side(m, kk);
                    Local::rot(rz(side * 0.1 * tuck) * rx(1.15 * tuck))
                }
                PartKind::Arm(sd) => Local::rot(rz(-(sd as f32) * 0.15 * tuck) * rx(0.7 * tuck)),
                PartKind::Head => Local::rot(rx(-0.3 * tuck)),
                _ => Local::IDENTITY,
            }
        }
        BodyAction::Dodge => {
            let e = track(t, &[K(0.0, 0.0, Ease::Linear), K(0.05, 1.0, Ease::OutCubic), K(0.16, 0.9, Ease::Linear), K(0.3, 0.0, Ease::InOutSine)]);
            // Lateral dodges twist the shoulders away; every dodge dives the top toward `dir`.
            let lateral = d.x;
            match rig.kinds[i] {
                PartKind::Root => {
                    let mut l = about(Quat::from_axis_angle(axis, 0.62 * e) * ry(-lateral * 0.45 * e), Vec3::Y * ll, pivot);
                    l.pos.y -= 0.14 * ll * e;
                    l
                }
                PartKind::Leg(kk) => {
                    let side = leg_side(m, kk);
                    // Trailing leg kicks out behind the dive.
                    let trail = if side * lateral > 0.0 { 0.1 } else { 0.45 };
                    Local::rot(rz(-side * 0.05 * e) * rx((0.55 - trail * lateral.abs()) * e))
                }
                PartKind::Arm(sd) => Local::rot(rz(-(sd as f32) * 0.25 * e) * rx(0.8 * e)),
                PartKind::Head => Local::rot(Quat::from_axis_angle(axis, -0.35 * e)),
                _ => Local::IDENTITY,
            }
        }
        BodyAction::Release => {
            let dur = BodyAction::Release.duration(strength);
            let u = (t / dur).clamp(0.0, 1.0);
            let fling = track(t, &[K(0.0, 0.0, Ease::Linear), K(0.07, 1.0, Ease::OutBack), K(0.22, 0.85, Ease::Linear), K(dur, 0.0, Ease::InOutSine)]);
            let tuck = (PI * u).sin();
            let flip = if strength >= 1.0 { TAU * Ease::InOutSine.apply(u) } else { 0.0 };
            match rig.kinds[i] {
                PartKind::Root => about(Quat::from_axis_angle(axis, flip), com, pivot),
                PartKind::Arm(sd) => Local::rot(rz((sd as f32) * 0.7 * fling) * rx(1.0 * fling - 0.3 * tuck)),
                PartKind::Leg(kk) => {
                    let side = leg_side(m, kk);
                    Local::rot(rz(side * 0.08 * tuck) * rx(0.9 * tuck))
                }
                PartKind::Head => Local::rot(rx(0.2 * fling)),
                _ => Local::IDENTITY,
            }
        }
    }
}

// ------------------------------------------------------------------------------------ channels

fn channel_local(m: &VoxelModel, i: usize, kind: Channel, w: f32, t: f32) -> Local {
    let rig = &m.rig;
    let ll = rig.leg_len.max(0.1);
    let w = if w.is_finite() { w.clamp(0.0, 1.0) } else { 0.0 };
    if w <= 1e-4 {
        return Local::IDENTITY;
    }
    // Ease in over 0.12 s (the caller eases out through `w`).
    let e = w * Ease::OutBack.apply(smoothstep(0.0, 0.12, t));
    match kind {
        Channel::Typing => {
            // A tap per beat (reads as working the ring), a faint tremble.
            let tap = 0.06 * (t * 11.0).sin().max(0.0);
            match rig.kinds[i] {
                PartKind::Arm(1) => Local::rot(rz(-0.4 * e) * rx((1.25 + tap) * e)),
                PartKind::Arm(_) => Local::rot(rz(0.5 * e) * rx(1.0 * e)),
                PartKind::Head => Local::rot(rx(-0.26 * e) * ry(0.08 * e)),
                PartKind::Root => Local::rot(rx(-0.07 * e) * ry(0.1 * e)),
                _ => Local::IDENTITY,
            }
        }
        Channel::Oath => {
            let pulse = 0.03 * (t * 3.0).sin();
            match rig.kinds[i] {
                PartKind::Arm(1) => Local::rot(rz(0.2 * e) * rx((2.75 + pulse) * e)),
                PartKind::Arm(_) => Local::rot(rz(0.5 * e) * rx(0.62 * e)),
                PartKind::Head => Local::rot(rx(0.3 * e)),
                PartKind::Root => Local::rot(rx(0.05 * e)),
                PartKind::Leg(k) => Local::rot(rz(leg_side(m, k) * 0.08 * e)),
                _ => Local::IDENTITY,
            }
        }
        Channel::Charge => {
            // The longer it charges, the harder it shakes.
            let amp = 0.018 * (1.0 + 1.2 * smoothstep(0.0, 2.5, t));
            let shake = amp * (t * 47.0).sin();
            let shake2 = amp * (t * 39.0 + 1.3).sin();
            match rig.kinds[i] {
                PartKind::Root => {
                    let mut l = Local::rot(rx(-0.1 * e + shake) * rz(shake2));
                    l.pos.y -= 0.14 * ll * e;
                    l
                }
                PartKind::Leg(k) => {
                    let s = leg_side(m, k);
                    Local::rot(rz(s * 0.26 * e) * rx(0.12 * e))
                }
                PartKind::Arm(sd) => {
                    let s = sd as f32;
                    Local::rot(rz(s * (0.42 * e + shake2)) * rx(-0.3 * e + shake))
                }
                PartKind::Head => Local::rot(rx(-0.14 * e + shake * 2.0)),
                PartKind::Tail { k, .. } => Local::rot(ry(shake * 4.0 / (1.0 + k as f32))),
                _ => Local::IDENTITY,
            }
        }
    }
}
