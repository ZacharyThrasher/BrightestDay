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
    // ---- TAUNT: wordless body taunts (`bd_sim::taunt`; `t` = seconds since the taunt started) ----
    /// Double-biceps flex: arms up and out, two pumps, a glance at the ring-hand bicep.
    Flex,
    /// "Come here": ring hand out, palm up, two beckons pulling back to the chest, chin up.
    Beckon,
    /// Shrug (elbows out, palms up, head tilt) then a dismissive brush-off flick of the ring hand.
    Shrug,
    /// A theatrical bow: ring arm sweeps across the belly, off arm out behind, a deep bend.
    Bow,
    /// A little bounce dance: hips sway, knees pump, fists punch up on the beat (loops).
    Dance,
}

impl Channel {
    /// A wordless taunt (not "busy with the ring").
    pub fn is_taunt(self) -> bool {
        matches!(self, Channel::Flex | Channel::Beckon | Channel::Shrug | Channel::Bow | Channel::Dance)
    }
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
    if kind.is_taunt() {
        return taunt_local(m, i, kind, w, if t.is_finite() { t.max(0.0) } else { 0.0 });
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
        // Taunts returned above.
        _ => Local::IDENTITY,
    }
}

// ------------------------------------------------------------------------------------ taunts

/// Seconds a taunt pose plays before it settles (the dance loops until the caller drops it).
pub fn taunt_duration(kind: Channel) -> f32 {
    match kind {
        Channel::Flex => 1.1,
        Channel::Beckon => 1.0,
        Channel::Shrug => 0.95,
        Channel::Bow => 1.25,
        Channel::Dance => 4.0,
        _ => 0.0,
    }
}

/// A smooth hump centred on `c`, `width` wide (0 outside).
fn bump(t: f32, c: f32, width: f32) -> f32 {
    let x = ((t - c) / (width * 0.5)).clamp(-1.0, 1.0);
    let y = 1.0 - x * x;
    y * y
}

/// Wordless taunts: big, readable silhouettes from any angle (the observer's view first), settling
/// back to rest by [`taunt_duration`] (and through `w` when cancelled).
fn taunt_local(m: &VoxelModel, i: usize, kind: Channel, w: f32, t: f32) -> Local {
    let rig = &m.rig;
    let ll = rig.leg_len.max(0.1);
    let pivot = m.parts[i].pivot;
    let hip = Vec3::Y * ll;
    let dur = taunt_duration(kind);
    // In / hold / out envelope (the dance only eases in).
    let env = |a: f32, out: f32| track(t, &[K(0.0, 0.0, Ease::Linear), K(a, 1.0, Ease::OutBack), K(out, 1.0, Ease::Linear), K(dur, 0.0, Ease::InOutSine)]);
    match kind {
        Channel::Flex => {
            let e = w * env(0.16, 0.86);
            // Two pumps and a glance at the ring-hand bicep.
            let pump = bump(t, 0.34, 0.2) + bump(t, 0.6, 0.2);
            let look = w * track(t, &[K(0.45, 0.0, Ease::Linear), K(0.6, 1.0, Ease::OutCubic), K(0.88, 1.0, Ease::Linear), K(1.02, 0.0, Ease::InOutSine)]);
            match rig.kinds[i] {
                // Both fists thrown up and out (a big V — reads from across the map), pumping twice.
                // (Rig limbs are one segment or a chain sharing the kind: a chain curls into biceps.)
                PartKind::Arm(sd) => {
                    let s = sd as f32;
                    Local::rot(rz(s * (2.2 + 0.22 * pump) * e) * rx((-0.2 - 0.1 * pump) * e))
                }
                PartKind::Root => {
                    let mut l = about(rx(0.07 * e + 0.03 * pump * e), hip, pivot);
                    l.pos.y -= 0.04 * ll * e;
                    let sq = 0.03 * pump * e;
                    l.scale = Vec3::new(1.0 + sq, 1.0, 1.0 + sq);
                    l
                }
                PartKind::Head => Local::rot(ry(-0.6 * look) * rz(-0.12 * look) * rx(-0.12 * look + 0.05 * e)),
                PartKind::Leg(k) => {
                    let s = leg_side(m, k);
                    Local::rot(rz(s * 0.14 * e) * rx(0.08 * e))
                }
                PartKind::Tail { k, .. } => Local::rot(rx(-0.2 * pump * e / (1.0 + k as f32))),
                _ => Local::IDENTITY,
            }
        }
        Channel::Beckon => {
            let e = w * env(0.14, 0.8);
            let pull = bump(t, 0.36, 0.22) + bump(t, 0.62, 0.22);
            match rig.kinds[i] {
                // Ring hand out front, then curls back toward the chest on each beckon.
                PartKind::Arm(1) => Local::rot(rz((0.3 + 0.1 * pull) * e) * rx((1.35 + 0.75 * pull) * e)),
                // Off hand on the hip.
                PartKind::Arm(sd) => {
                    let s = sd as f32;
                    Local::rot(rz(s * 0.55 * e) * rx(-0.35 * e))
                }
                // Side-on, weight back, chin up (and a nod up with each beckon).
                PartKind::Root => about(ry(0.32 * e) * rx((0.1 + 0.04 * pull) * e), hip, pivot),
                PartKind::Head => Local::rot(ry(-0.3 * e) * rx((0.2 + 0.16 * pull) * e)),
                PartKind::Leg(k) => {
                    let lead = leg_side(m, k) < 0.0;
                    Local::rot(rx(if lead { 0.28 } else { -0.14 } * e))
                }
                PartKind::Tail { k, .. } => Local::rot(ry(0.3 * pull * e / (1.0 + k as f32))),
                _ => Local::IDENTITY,
            }
        }
        Channel::Shrug => {
            let e = w * env(0.12, 0.76);
            // Then the brush-off: the ring hand flicks out, the body turns away.
            let flick = w * track(t, &[K(0.46, 0.0, Ease::Linear), K(0.58, 1.0, Ease::OutCubic), K(0.72, 0.85, Ease::Linear), K(0.95, 0.0, Ease::InOutSine)]);
            let shrug = e * (1.0 - 0.5 * flick);
            match rig.kinds[i] {
                PartKind::Arm(1) => {
                    let s = 1.0;
                    Local::rot(rz(s * (0.55 * shrug + 1.15 * flick)) * rx(0.7 * shrug + 0.3 * flick))
                }
                PartKind::Arm(sd) => {
                    let s = sd as f32;
                    Local::rot(rz(s * 0.55 * e) * rx(0.7 * e))
                }
                PartKind::Root => {
                    let mut l = about(ry(0.22 * flick) * rz(-0.05 * shrug) * rx(0.05 * shrug), hip, pivot);
                    // Up on the toes for the shrug.
                    l.pos.y += 0.025 * ll * shrug;
                    l
                }
                PartKind::Head => Local::rot(ry(0.3 * flick) * rz(0.28 * shrug) * rx(0.1 * shrug + 0.12 * flick)),
                PartKind::Tail { k, .. } => Local::rot(rz(0.25 * shrug / (1.0 + k as f32))),
                _ => Local::IDENTITY,
            }
        }
        Channel::Bow => {
            let bend = w * track(t, &[K(0.0, 0.0, Ease::Linear), K(0.3, 1.0, Ease::InOutSine), K(0.82, 1.0, Ease::Linear), K(dur, 0.0, Ease::InOutSine)]);
            let sweep = w * track(t, &[K(0.0, 0.0, Ease::Linear), K(0.22, 1.0, Ease::OutBack), K(0.86, 1.0, Ease::Linear), K(dur, 0.0, Ease::InOutSine)]);
            let lean = 0.62;
            match rig.kinds[i] {
                PartKind::Root => about(rx(-lean * bend), hip, pivot),
                PartKind::Head => Local::rot(rx(-0.15 * bend)),
                // Ring arm sweeps across the belly; the off arm flourishes out behind.
                PartKind::Arm(1) => Local::rot(rz(-0.55 * sweep) * rx(0.75 * sweep + 0.4 * lean * bend)),
                PartKind::Arm(sd) => {
                    let s = sd as f32;
                    Local::rot(rz(s * 0.75 * sweep) * rx(-0.55 * sweep))
                }
                // Legs undo the hip bend (feet stay planted), the ring side's foot slides back.
                PartKind::Leg(k) => {
                    let back = leg_side(m, k) > 0.0;
                    Local::rot(rx(lean * bend + if back { -0.22 } else { 0.1 } * bend))
                }
                PartKind::Tail { k, .. } => Local::rot(rx(0.35 * bend / (1.0 + k as f32))),
                _ => Local::IDENTITY,
            }
        }
        Channel::Dance => {
            let e = w * smoothstep(0.0, 0.18, t);
            // 132 bpm: a dip on every beat, hips / fists alternate every beat.
            let ph = t * TAU * 2.2;
            let dip = 0.5 + 0.5 * ph.cos();
            let alt = (ph * 0.5).sin();
            match rig.kinds[i] {
                PartKind::Root => {
                    let mut l = about(rz(0.2 * alt * e) * ry(0.34 * alt * e) * rx(-0.06 * e), hip, pivot);
                    l.pos.y -= 0.08 * ll * (1.0 - dip) * e;
                    l
                }
                PartKind::Leg(k) => {
                    let s = leg_side(m, k);
                    let knee = (0.42 * (s * alt).max(0.0) + 0.1 * (1.0 - dip)) * e;
                    Local::rot(rz(-0.2 * alt * e + s * 0.06 * e) * rx(knee))
                }
                // Disco point: one fist punches up to the sky, the other drops across the hips.
                PartKind::Arm(sd) => {
                    let s = sd as f32;
                    let up = (0.5 + 0.5 * s * alt).clamp(0.0, 1.0);
                    Local::rot(rz(s * (0.3 - 0.18 * up) * e) * rx((0.35 + 2.45 * up) * e))
                }
                PartKind::Head => Local::rot(rz(-0.16 * alt * e) * ry(-0.2 * alt * e) * rx((0.08 - 0.16 * (1.0 - dip)) * e)),
                PartKind::Tail { k, .. } => Local::rot(ry(0.5 * alt * e / (1.0 + k as f32))),
                _ => Local::IDENTITY,
            }
        }
        _ => Local::IDENTITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CostumeTweaks, costume_model, costume_with, pose_layered};
    use bd_core::mechanics::{Form, Recipe};
    use glam::Affine3A;

    fn body() -> VoxelModel {
        let mut r = Recipe::unknown("knight");
        r.form = Form::Biped;
        costume_model(&costume_with(&r, None, None, 3, CostumeTweaks::default()))
    }

    fn lowest(m: &VoxelModel, layers: &[(AnimState, f32)]) -> (f32, Vec<Affine3A>) {
        let mut p = vec![Affine3A::IDENTITY; m.parts.len()];
        pose_layered(m, layers, &mut p);
        let mut lo = f32::MAX;
        for (i, part) in m.parts.iter().enumerate() {
            for (v, _) in &part.voxels {
                lo = lo.min(p[i].transform_point3(part.voxel_center(*v)).y);
            }
        }
        (lo, p)
    }

    /// Every body layer poses finitely for garbage inputs too, and the grounded ones keep the feet
    /// near the floor (no sinking through it, no floating).
    #[test]
    fn body_layers_are_finite_and_grounded_poses_keep_the_feet_down() {
        let m = body();
        let base = (AnimState::Locomotion { speed: 0.0, distance: 0.0, airborne: false }, 0.3);
        let grounded = [
            AnimState::Motion(BodyMotion { crouch: 1.0, ..Default::default() }),
            AnimState::Motion(BodyMotion { side: 8.0, fwd: -3.0, turn: 3.0, ..Default::default() }),
            AnimState::Action { kind: BodyAction::Land, dir: Vec3::NEG_Z, strength: 1.5 },
            AnimState::Action { kind: BodyAction::Land, dir: Vec3::NEG_Z, strength: 0.3 },
            AnimState::Channel { kind: Channel::Charge, w: 1.0 },
            AnimState::Channel { kind: Channel::Typing, w: 1.0 },
        ];
        for a in grounded {
            for t in [0.0, 0.05, 0.12, 0.3, 0.6] {
                let (lo, p) = lowest(&m, &[base, (a, t)]);
                assert!(p.iter().all(|x| x.is_finite()), "{a:?} @ {t}");
                assert!((-0.2..0.25).contains(&lo), "{a:?} @ {t}: lowest voxel {lo}");
            }
        }
        let junk = BodyMotion { fwd: f32::NAN, side: f32::INFINITY, vy: -1e9, turn: f32::NAN, air: 7.0, crouch: -3.0, boost: f32::NAN };
        let (_, p) = lowest(&m, &[base, (AnimState::Motion(junk), f32::NAN)]);
        assert!(p.iter().all(|x| x.is_finite()));
        for kind in [BodyAction::AirJump, BodyAction::Dodge, BodyAction::Release] {
            let (_, p) = lowest(&m, &[base, (AnimState::Action { kind, dir: Vec3::new(f32::NAN, 0.0, 0.0), strength: 9.0 }, 0.1)]);
            assert!(p.iter().all(|x| x.is_finite()), "{kind:?}");
        }
    }

    /// The layers actually read: crouching lowers the head, typing raises the ring hand, the air-jump
    /// flip turns the body over at its midpoint, and every action is back to rest at its end.
    #[test]
    fn body_layers_read_and_settle() {
        let m = body();
        let base = (AnimState::Locomotion { speed: 0.0, distance: 0.0, airborne: false }, 0.3);
        let top = |layers: &[(AnimState, f32)]| {
            let (_, p) = lowest(&m, layers);
            p[0].transform_point3(Vec3::new(0.0, m.bounds.max.y, 0.0)).y
        };
        let stand = top(&[base]);
        assert!(top(&[base, (AnimState::Motion(BodyMotion { crouch: 1.0, ..Default::default() }), 0.0)]) < stand - 0.1, "crouch is lower");
        let flipped = top(&[base, (AnimState::Action { kind: BodyAction::AirJump, dir: Vec3::NEG_Z, strength: 1.0 }, 0.2)]);
        assert!(flipped < stand * 0.5, "mid-flip the head is down ({flipped} vs {stand})");
        for kind in [BodyAction::Land, BodyAction::AirJump, BodyAction::Dodge, BodyAction::Release] {
            let d = kind.duration(1.0);
            let (_, rest) = lowest(&m, &[base]);
            let (_, end) = lowest(&m, &[base, (AnimState::Action { kind, dir: Vec3::X, strength: 1.0 }, d)]);
            for (a, b) in rest.iter().zip(&end) {
                assert!(a.translation.distance(b.translation) < 0.02, "{kind:?} settles by {d}s");
            }
        }
    }

    /// Taunts: finite for any time / weight, the feet stay on the floor (they are grounded poses),
    /// they read (the flex raises both fists above the head line of the idle body, the bow drops
    /// the head), and the timed ones are back at rest by their end.
    #[test]
    fn taunts_read_keep_the_feet_down_and_settle() {
        let m = body();
        let base = (AnimState::Locomotion { speed: 0.0, distance: 0.0, airborne: false }, 0.3);
        let taunts = [Channel::Flex, Channel::Beckon, Channel::Shrug, Channel::Bow, Channel::Dance];
        for kind in taunts {
            let d = taunt_duration(kind);
            for k in 0..=24 {
                let t = d * k as f32 / 24.0;
                let (lo, p) = lowest(&m, &[base, (AnimState::Channel { kind, w: 1.0 }, t)]);
                assert!(p.iter().all(|x| x.is_finite()), "{kind:?} @ {t}");
                assert!((-0.2..0.25).contains(&lo), "{kind:?} @ {t}: lowest voxel {lo}");
            }
            let (_, p) = lowest(&m, &[base, (AnimState::Channel { kind, w: f32::NAN }, f32::INFINITY)]);
            assert!(p.iter().all(|x| x.is_finite()), "{kind:?} junk");
            if kind != Channel::Dance {
                let (_, rest) = lowest(&m, &[base]);
                let (_, end) = lowest(&m, &[base, (AnimState::Channel { kind, w: 1.0 }, d)]);
                for (a, b) in rest.iter().zip(&end) {
                    assert!(a.translation.distance(b.translation) < 0.02, "{kind:?} settles by {d}s");
                }
            }
        }
        // Readable: the highest voxel of a flexing body is far above the idle arms' reach, the bow's
        // head is lower than standing.
        let top = |layers: &[(AnimState, f32)]| {
            let mut p = vec![Affine3A::IDENTITY; m.parts.len()];
            pose_layered(&m, layers, &mut p);
            let mut hi = f32::MIN;
            for (i, part) in m.parts.iter().enumerate() {
                for (v, _) in &part.voxels {
                    hi = hi.max(p[i].transform_point3(part.voxel_center(*v)).y);
                }
            }
            hi
        };
        let stand = top(&[base]);
        let bow = top(&[base, (AnimState::Channel { kind: Channel::Bow, w: 1.0 }, 0.5)]);
        assert!(bow < stand - 0.15, "the bow bends ({bow} vs {stand})");
        let arm_tip = |layers: &[(AnimState, f32)]| {
            let mut p = vec![Affine3A::IDENTITY; m.parts.len()];
            pose_layered(&m, layers, &mut p);
            let mut hi = f32::MIN;
            for (i, part) in m.parts.iter().enumerate() {
                if matches!(m.rig.kinds[i], PartKind::Arm(_)) {
                    for (v, _) in &part.voxels {
                        hi = hi.max(p[i].transform_point3(part.voxel_center(*v)).y);
                    }
                }
            }
            hi
        };
        let idle_arm = arm_tip(&[base]);
        let flex_arm = arm_tip(&[base, (AnimState::Channel { kind: Channel::Flex, w: 1.0 }, 0.5)]);
        assert!(flex_arm > idle_arm + 0.25, "flex raises the fists ({flex_arm} vs {idle_arm})");
    }
}
