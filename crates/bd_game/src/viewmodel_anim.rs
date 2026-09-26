//! Procedural first-person hand animation (child module of `viewmodel`).
//!
//! Everything the player does shows in the hands on the same frame, with a little follow-through:
//! look sway, footstep bob (same cadence as the footstep sounds), strafe lean, speed tuck, jump /
//! land, slide brace, per-hand tethers (throw, grip toward the anchor, reel tremble, whip back), the
//! ring (raise + glow while typing / talking, thrust on cast, steady during the oath), held weight
//! (heavier things lag and settle slower, swing anticipation), gun kick, hit flinch, launch flail,
//! KO drop and the charge clench.
//!
//! * **Pure core**: [`HandAnimator::step`] turns one frame of [`AnimInput`] (+ one-shot impulses) into
//!   a [`HandPose`]. Springs are integrated with the exact damped-oscillator solution
//!   ([`spring_exact`]): stable for any dt, frame-rate independent, allocation-free.
//! * **Bevy glue**: [`drive_hand_anim`] gathers the frame's inputs (this frame's look, the
//!   predicted + interpolated local player, this frame's movement / game events, UI state) right
//!   before `animate_viewmodel`, which folds [`HandPose`] into the arm / root transforms.
//! * [`publish_hands`] (PostUpdate, after the camera is placed, before transform propagation) turns
//!   the camera-space hand points into world space from *this frame's* camera, and re-places the
//!   first-person held construct from its camera-space grip, so neither lags the view by a frame.
//! * Tuning: `assets/tuning/viewmodel.ron` (hot-reloaded; written with defaults if missing).
//! * Comfort: sway / bob / landing follow the comfort "landing dip / camera kick" slider (the
//!   Comfort preset leaves `comfort_motion` of it); at 0 the viewmodel never rolls. The camera is
//!   never touched.
//! * `--practice --script hands`: a scripted demo of every hand action (for recordings).

use super::FirstPersonHeld;
use crate::camera::{CameraUpdateSet, PlayerCamera, ViewAnchor};
use crate::console::{RingConsole, RingCue};
use crate::input::{LocalInputState, LookAngles, UiFocus};
use crate::player::{FrameMoveEvents, LocalPlayer, PlayerSim};
use crate::present::constructs::{ConstructEntities, SimConstruct};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, SimRole};
use crate::tick::TickClock;
use crate::util::v3;
use bd_core::input::buttons;
use bd_core::mechanics::{Form, FormCategory};
use bd_sim::events::HeldFx;
use bd_sim::movement::HookPhase;
use bd_sim::weapons::GunArchetype;
use bd_sim::GameEvent;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};
use std::path::PathBuf;
use std::time::SystemTime;

/// Hand index: 0 = left (Q tether), 1 = right (E tether, ring, held item).
pub const LEFT: usize = 0;
pub const RIGHT: usize = 1;

// ------------------------------------------------------------------------------------------ tuning

/// All hand-animation amplitudes (`assets/tuning/viewmodel.ron`). Metres / radians in camera space
/// (x right, y up, z back); impulses are velocities (m/s, rad/s) added to a spring.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VmTuning {
    /// Scales every procedural offset (0 = the old static hands).
    pub master: f32,
    /// Share of sway / bob / landing kept when the comfort "landing dip" slider is 0 (Comfort preset).
    pub comfort_motion: f32,

    // idle
    pub breath_amp: f32,
    pub breath_rate: f32,
    pub breath_rot: f32,
    pub drift_amp: f32,
    pub drift_rot: f32,

    // look sway: hands trail the mouse (per rad/s of turn rate), spring back
    pub look_pos: f32,
    pub look_rot: f32,
    pub look_roll: f32,
    pub look_max_rate: f32,
    pub look_omega: f32,
    pub look_zeta: f32,

    // footstep bob (one step per `step_len` metres on the ground, same as the footstep sounds)
    pub step_len: f32,
    pub bob_y: f32,
    pub bob_x: f32,
    pub bob_roll: f32,
    pub bob_pitch: f32,
    pub bob_ref_speed: f32,
    pub bob_max: f32,

    // movement poses (spring toward a target)
    pub strafe_x: f32,
    pub strafe_roll: f32,
    pub tuck_from: f32,
    pub tuck_to: f32,
    pub tuck_pos: [f32; 3],
    pub tuck_pitch: f32,
    pub tuck_in: f32,
    pub air_float: f32,
    pub air_float_max: f32,
    pub crouch_y: f32,
    pub move_omega: f32,
    pub move_zeta: f32,

    // jump / land / boost impulses
    pub jump_lift: f32,
    pub jump_pitch: f32,
    pub land_per_speed: f32,
    pub land_max: f32,
    pub land_pitch: f32,
    pub heavy_land_speed: f32,
    pub heavy_crunch: f32,
    pub heavy_squeeze: f32,
    pub boost_kick: f32,
    pub imp_omega: f32,
    pub imp_zeta: f32,

    // slide: braced pose
    pub slide_pos: [f32; 3],
    pub slide_rot: [f32; 3],
    pub slide_brace: [f32; 3],

    // per-hand springs + pose blend
    pub hand_omega: f32,
    pub hand_zeta: f32,
    pub pose_omega: f32,
    pub pose_zeta: f32,

    // tethers (per hand)
    pub tether_throw: f32,
    pub tether_throw_pitch: f32,
    pub tether_aim: f32,
    pub tether_aim_max: f32,
    pub tether_pull: f32,
    pub tether_lift: f32,
    pub tether_tremble: f32,
    pub tether_whip: f32,
    pub tether_whip_rot: f32,

    // ring (right hand)
    pub ring_raise_pos: [f32; 3],
    pub ring_raise_rot: [f32; 3],
    pub ring_type_pulse: f32,
    pub ring_voice_tremble: f32,
    pub cast_thrust: f32,
    pub cast_rot: f32,
    pub cast_recoil: f32,
    pub oath_raise_pos: [f32; 3],
    pub oath_raise_rot: [f32; 3],
    /// Sway / bob multiplier while reciting (steady hand).
    pub oath_steady: f32,

    // held constructs (`bd_shapes::weight_of`, 1 = nominal)
    /// Look-lag gain × weight^this.
    pub held_lag_exp: f32,
    /// Spring stiffness × weight^-this (heavier settles slower).
    pub held_settle_exp: f32,
    pub swing_anticipation: f32,
    pub swing_follow: f32,
    pub swing_impact: f32,
    pub coil_pos: [f32; 3],
    pub coil_tremble: f32,

    // guns (on top of the base kick in `viewmodel`): per archetype scale
    pub gun_kick_pitch: f32,
    pub gun_kick_back: f32,
    pub gun_kick_up: f32,
    pub gun_kick_yaw: f32,
    pub gun_kick_roll: f32,
    pub gun_omega: f32,
    pub gun_zeta: f32,
    pub gun_hand_cannon: f32,
    pub gun_shotgun: f32,
    pub gun_minigun: f32,
    pub gun_rocket: f32,
    pub gun_railgun: f32,
    pub gun_bow: f32,
    pub gun_cannon: f32,
    pub gun_staff: f32,
    pub gun_disc: f32,
    pub minigun_buzz: f32,
    /// Client role: fraction of the kick played on the trigger press (the host's shot follows).
    pub gun_predict: f32,

    // getting hit
    pub flinch: f32,
    pub flinch_rot: f32,
    pub flail: f32,
    pub flail_rate: f32,
    pub ko_drop: f32,
    pub ko_hold: f32,

    // charge (power-up): both fists clench in, trembling (left hand values; right is mirrored)
    pub charge_pos: [f32; 3],
    pub charge_rot: [f32; 3],
    pub charge_tremble: f32,
    pub charge_tremble_full: f32,
    pub charge_glow: f32,
}

impl Default for VmTuning {
    fn default() -> Self {
        Self {
            master: 1.0,
            comfort_motion: 0.3,
            breath_amp: 0.0025,
            breath_rate: 0.22,
            breath_rot: 0.006,
            drift_amp: 0.0018,
            drift_rot: 0.006,
            look_pos: 0.0045,
            look_rot: 0.018,
            look_roll: 0.012,
            look_max_rate: 12.0,
            look_omega: 11.0,
            look_zeta: 0.6,
            step_len: 2.6,
            bob_y: 0.010,
            bob_x: 0.007,
            bob_roll: 0.015,
            bob_pitch: 0.012,
            bob_ref_speed: 10.0,
            bob_max: 1.4,
            strafe_x: 0.010,
            strafe_roll: 0.06,
            tuck_from: 11.0,
            tuck_to: 26.0,
            tuck_pos: [0.0, -0.018, 0.022],
            tuck_pitch: -0.09,
            tuck_in: 0.018,
            air_float: 0.0009,
            air_float_max: 0.022,
            crouch_y: -0.01,
            move_omega: 8.0,
            move_zeta: 0.85,
            jump_lift: 0.28,
            jump_pitch: 1.1,
            land_per_speed: 0.022,
            land_max: 0.45,
            land_pitch: 3.0,
            heavy_land_speed: 14.0,
            heavy_crunch: 0.35,
            heavy_squeeze: 0.5,
            boost_kick: 0.35,
            imp_omega: 13.0,
            imp_zeta: 0.5,
            slide_pos: [0.0, -0.025, -0.015],
            slide_rot: [0.07, 0.0, -0.1],
            slide_brace: [0.02, -0.03, -0.05],
            hand_omega: 15.0,
            hand_zeta: 0.55,
            pose_omega: 11.0,
            pose_zeta: 0.72,
            tether_throw: 1.1,
            tether_throw_pitch: 3.0,
            tether_aim: 0.8,
            tether_aim_max: 0.55,
            tether_pull: 0.035,
            tether_lift: 0.015,
            tether_tremble: 0.0022,
            tether_whip: 1.3,
            tether_whip_rot: 5.0,
            ring_raise_pos: [-0.075, 0.07, 0.03],
            ring_raise_rot: [0.35, 0.3, 0.55],
            ring_type_pulse: 1.2,
            ring_voice_tremble: 0.002,
            cast_thrust: 1.8,
            cast_rot: 4.0,
            cast_recoil: 0.35,
            oath_raise_pos: [-0.07, 0.1, 0.01],
            oath_raise_rot: [0.55, 0.3, 0.45],
            oath_steady: 0.25,
            held_lag_exp: 0.9,
            held_settle_exp: 0.5,
            swing_anticipation: 1.0,
            swing_follow: 1.0,
            swing_impact: 1.0,
            coil_pos: [0.01, -0.01, 0.04],
            coil_tremble: 0.0018,
            gun_kick_pitch: 2.5,
            gun_kick_back: 0.25,
            gun_kick_up: 0.2,
            gun_kick_yaw: 1.0,
            gun_kick_roll: 2.0,
            gun_omega: 24.0,
            gun_zeta: 0.42,
            gun_hand_cannon: 1.0,
            gun_shotgun: 1.8,
            gun_minigun: 0.3,
            gun_rocket: 1.5,
            gun_railgun: 1.3,
            gun_bow: 0.4,
            gun_cannon: 1.9,
            gun_staff: 0.5,
            gun_disc: 0.5,
            minigun_buzz: 0.0025,
            gun_predict: 0.3,
            flinch: 0.9,
            flinch_rot: 4.0,
            flail: 0.14,
            flail_rate: 2.3,
            ko_drop: 0.3,
            ko_hold: 0.5,
            charge_pos: [0.07, -0.01, 0.06],
            charge_rot: [0.25, 0.35, 0.5],
            charge_tremble: 0.0025,
            charge_tremble_full: 0.005,
            charge_glow: 0.6,
        }
    }
}

impl VmTuning {
    fn gun_scale(&self, a: GunArchetype) -> f32 {
        match a {
            GunArchetype::Ring => 0.0,
            GunArchetype::HandCannon => self.gun_hand_cannon,
            GunArchetype::SuperShotgun => self.gun_shotgun,
            GunArchetype::Minigun => self.gun_minigun,
            GunArchetype::Rocket => self.gun_rocket,
            GunArchetype::Railgun => self.gun_railgun,
            GunArchetype::Bow => self.gun_bow,
            GunArchetype::Cannon => self.gun_cannon,
            GunArchetype::Staff => self.gun_staff,
            GunArchetype::Disc => self.gun_disc,
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct VmTuningRes(pub VmTuning);

fn tuning_path() -> PathBuf {
    crate::assets_dir().join("tuning").join("viewmodel.ron")
}

fn load_tuning() -> VmTuning {
    let path = tuning_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => ron::from_str::<VmTuning>(&text).unwrap_or_else(|e| {
            eprintln!("viewmodel tuning: failed to parse {}: {e}; using defaults", path.display());
            VmTuning::default()
        }),
        Err(_) => {
            let t = VmTuning::default();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(text) = ron::ser::to_string_pretty(&t, ron::ser::PrettyConfig::default()) {
                let _ = std::fs::write(&path, format!("// First-person hand animation (bd_game viewmodel_anim). Hot-reloaded. Missing fields use defaults.\n{text}\n"));
            }
            t
        }
    }
}

fn hot_reload_tuning(time: Res<Time<Real>>, mut tuning: ResMut<VmTuningRes>, mut last: Local<(f32, Option<SystemTime>)>) {
    last.0 += time.delta_secs();
    if last.0 < 0.5 {
        return;
    }
    last.0 = 0.0;
    let Ok(modified) = std::fs::metadata(tuning_path()).and_then(|m| m.modified()) else { return };
    match last.1 {
        None => last.1 = Some(modified),
        Some(prev) if prev != modified => {
            last.1 = Some(modified);
            if let Ok(text) = std::fs::read_to_string(tuning_path()) {
                match ron::from_str::<VmTuning>(&text) {
                    Ok(t) => {
                        info!("viewmodel tuning reloaded");
                        tuning.0 = t;
                    }
                    Err(e) => warn!("viewmodel tuning: parse error, keeping previous: {e}"),
                }
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------------------------------ springs

/// Exact step of a damped spring `y'' = -ω²y - 2ζωy'` over `dt` (y = offset from the goal).
/// Unconditionally stable (no substeps), frame-rate independent, finite for any input.
pub fn spring_exact(y0: f32, v0: f32, omega: f32, zeta: f32, dt: f32) -> (f32, f32) {
    let dt = if dt.is_finite() { dt.clamp(0.0, 1.0) } else { 0.0 };
    let w = if omega.is_finite() { omega.clamp(0.0, 400.0) } else { 0.0 };
    let z = if zeta.is_finite() { zeta.clamp(0.0, 20.0) } else { 1.0 };
    let (y0, v0) = if y0.is_finite() && v0.is_finite() { (y0.clamp(-1e4, 1e4), v0.clamp(-1e5, 1e5)) } else { (0.0, 0.0) };
    if dt <= 0.0 {
        return (y0, v0);
    }
    if w < 1e-4 {
        return (y0, v0);
    }
    let out = if (z - 1.0).abs() < 1e-2 {
        let e = (-w * dt).exp();
        let c2 = v0 + w * y0;
        let y = e * (y0 + c2 * dt);
        (y, e * (c2 - w * (y0 + c2 * dt)))
    } else if z < 1.0 {
        let wd = w * (1.0 - z * z).sqrt();
        let e = (-z * w * dt).exp();
        let (s, c) = (wd * dt).sin_cos();
        let c2 = (v0 + z * w * y0) / wd;
        (e * (y0 * c + c2 * s), e * ((c2 * wd - z * w * y0) * c - (y0 * wd + z * w * c2) * s))
    } else {
        let sq = (z * z - 1.0).sqrt();
        let (r1, r2) = (-w * (z - sq), -w * (z + sq));
        let c1 = (v0 - r2 * y0) / (r1 - r2);
        let c2 = y0 - c1;
        let (e1, e2) = ((r1 * dt).exp(), (r2 * dt).exp());
        (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
    };
    if out.0.is_finite() && out.1.is_finite() { out } else { (0.0, 0.0) }
}

/// A scalar spring chasing a goal.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring {
    pub x: f32,
    pub v: f32,
}

impl Spring {
    pub fn step(&mut self, goal: f32, omega: f32, zeta: f32, dt: f32) {
        let goal = if goal.is_finite() { goal } else { 0.0 };
        let (y, v) = spring_exact(self.x - goal, self.v, omega, zeta, dt);
        self.x = y + goal;
        self.v = v;
    }
}

/// Three independent springs (a position or small-angle euler offset).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring3 {
    pub x: Vec3,
    pub v: Vec3,
}

impl Spring3 {
    pub fn step(&mut self, goal: Vec3, omega: f32, zeta: f32, dt: f32) {
        for i in 0..3 {
            let mut s = Spring { x: self.x[i], v: self.v[i] };
            s.step(goal[i], omega, zeta, dt);
            self.x[i] = s.x;
            self.v[i] = s.v;
        }
    }
    pub fn kick(&mut self, dv: Vec3) {
        if dv.is_finite() {
            self.v += dv;
        }
    }
}

// ------------------------------------------------------------------------------------------ core

/// One hand's tether this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HookIn {
    pub phase: HookPhase,
    /// Unit direction from the eye to the hook head / anchor, camera space.
    pub dir: Vec3,
    /// Reeling in (wheel / R) or boosting while attached: the arm strains.
    pub straining: bool,
}

/// Everything continuous the hands react to, sampled once per rendered frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimInput {
    pub dt: f32,
    /// (yaw rate, pitch rate) rad/s from this frame's mouse look. + yaw = turning left.
    pub look_rate: Vec2,
    /// Interpolated velocity in camera space (x right, y up, z back).
    pub vel_cam: Vec3,
    pub grounded: bool,
    pub sliding: bool,
    pub crouched: bool,
    pub tethered: bool,
    /// Ticks run this frame and the last tick's horizontal distance (footstep cadence).
    pub ticks: u32,
    pub tick_dist: f32,
    /// Render interpolation alpha between the previous and current tick.
    pub alpha: f32,
    pub tumbling: bool,
    pub hooks: [HookIn; 2],
    pub ring_open: bool,
    pub oath: bool,
    pub voice_level: f32,
    pub charging: bool,
    pub charge_held: f32,
    pub charge_full: bool,
    pub held: bool,
    pub held_weight: f32,
    /// Comfort: sway / bob / landing scale and root-roll scale.
    pub motion_scale: f32,
    pub roll_scale: f32,
    pub alive: bool,
}

/// The procedural pose, applied on top of the viewmodel's rest pose (camera space).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct HandPose {
    pub root_t: Vec3,
    pub root_r: Quat,
    /// Per hand (0 left, 1 right): fist offset and a rotation about the fist.
    pub hand_t: [Vec3; 2],
    pub hand_r: [Quat; 2],
    /// Extra ring glow (charge, ring raised).
    pub glow: f32,
    /// Keep the hands visible just after a KO (they drop).
    pub ko_visible: bool,
}

impl Default for HandPose {
    fn default() -> Self {
        Self { root_t: Vec3::ZERO, root_r: Quat::IDENTITY, hand_t: [Vec3::ZERO; 2], hand_r: [Quat::IDENTITY; 2], glow: 0.0, ko_visible: false }
    }
}

/// Rest direction of each arm (elbow → fist) in camera space, matching `viewmodel`'s rest yaw/pitch.
fn rest_dir(hand: usize) -> Vec3 {
    let (yaw, pitch) = if hand == RIGHT { (0.16, 0.30) } else { (-0.20, 0.22) };
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * Vec3::NEG_Z
}

/// Deterministic smooth noise in [-1, 1]³ (sum of incommensurate sines).
fn wobble(t: f32, seed: f32) -> Vec3 {
    let s = seed * 1.618;
    Vec3::new(
        0.6 * (t * 1.00 + s).sin() + 0.4 * (t * 2.13 + s * 2.1).sin(),
        0.6 * (t * 1.31 + s * 1.7).sin() + 0.4 * (t * 2.71 + s * 0.3).sin(),
        0.6 * (t * 0.87 + s * 2.9).sin() + 0.4 * (t * 1.93 + s * 1.1).sin(),
    )
}

fn euler(v: Vec3) -> Quat {
    Quat::from_euler(EulerRot::YXZ, v.y, v.x, v.z)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    if b <= a {
        return if x >= b { 1.0 } else { 0.0 };
    }
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn v3a(a: [f32; 3]) -> Vec3 {
    Vec3::from_array(a)
}

/// Rotation turning `from` toward `to`, at most `max` radians.
fn arc_limited(from: Vec3, to: Vec3, max: f32) -> Quat {
    let (Some(a), Some(b)) = (from.try_normalize(), to.try_normalize()) else { return Quat::IDENTITY };
    let ang = a.dot(b).clamp(-1.0, 1.0).acos();
    if ang < 1e-4 {
        return Quat::IDENTITY;
    }
    let axis = a.cross(b).try_normalize().unwrap_or(Vec3::Y);
    Quat::from_axis_angle(axis, ang.min(max.max(0.0)))
}

/// The hand animation state machine. Pure: no Bevy resources, no allocation.
#[derive(Clone, Debug, Default)]
pub struct HandAnimator {
    t: f32,
    look_pos: Spring3,
    look_rot: Spring3,
    move_pos: Spring3,
    move_rot: Spring3,
    imp_pos: Spring3,
    imp_rot: Spring3,
    gun_pos: Spring3,
    gun_rot: Spring3,
    hand_pos: [Spring3; 2],
    hand_rot: [Spring3; 2],
    /// Aim toward the tether (weight + smoothed direction).
    aim_w: [Spring; 2],
    aim_dir: [Vec3; 2],
    bob_amt: Spring,
    /// Footstep cadence (mirrors `audio::movement_sounds`).
    stride: f32,
    steps: f32,
    // pose weights
    ring_w: Spring,
    oath_w: Spring,
    slide_w: Spring,
    charge_w: Spring,
    coil_w: Spring,
    ko_w: Spring,
    coiled: bool,
    flail: f32,
    buzz: f32,
    ko_timer: f32,
    was_alive: bool,
    shots: u32,
    type_sign: f32,
    rush_hand: usize,
}

impl HandAnimator {
    // ----- one-shot impulses (call before `step` in the same frame)

    pub fn jump(&mut self, tu: &VmTuning, perfect: bool, m: f32) {
        let k = if perfect { 1.25 } else { 1.0 } * m;
        self.imp_pos.kick(Vec3::Y * tu.jump_lift * k);
        self.imp_rot.kick(Vec3::X * tu.jump_pitch * k);
    }

    pub fn land(&mut self, tu: &VmTuning, impact_speed: f32, m: f32) {
        let s = if impact_speed.is_finite() { impact_speed.max(0.0) } else { 0.0 };
        let dip = (s * tu.land_per_speed).min(tu.land_max) * m;
        self.imp_pos.kick(Vec3::NEG_Y * dip);
        self.imp_rot.kick(Vec3::NEG_X * dip * tu.land_pitch);
        if s > tu.heavy_land_speed {
            // Crunch: a second, harder drop and both fists squeeze in.
            let h = ((s - tu.heavy_land_speed) / tu.heavy_land_speed.max(1.0)).min(1.0) + 0.5;
            self.imp_pos.kick(Vec3::new(0.0, -tu.heavy_crunch, 0.1) * h * m);
            self.hand_pos[LEFT].kick(Vec3::X * tu.heavy_squeeze * h * m);
            self.hand_pos[RIGHT].kick(Vec3::NEG_X * tu.heavy_squeeze * h * m);
            self.hand_rot[LEFT].kick(Vec3::new(-2.0, 0.0, -1.5) * h * m);
            self.hand_rot[RIGHT].kick(Vec3::new(-2.0, 0.0, 1.5) * h * m);
        }
    }

    pub fn boost(&mut self, tu: &VmTuning, m: f32) {
        self.imp_pos.kick(Vec3::Z * tu.boost_kick * m);
    }

    pub fn tether_fire(&mut self, tu: &VmTuning, hand: usize, dir: Vec3) {
        let h = hand.min(1);
        let mut d = dir.try_normalize().unwrap_or(Vec3::NEG_Z);
        d.z = d.z.min(-0.3);
        let d = d.normalize_or(Vec3::NEG_Z);
        self.hand_pos[h].kick(d * tu.tether_throw);
        self.hand_rot[h].kick(Vec3::X * tu.tether_throw_pitch);
        self.aim_dir[h] = d;
    }

    pub fn tether_release(&mut self, tu: &VmTuning, hand: usize) {
        let h = hand.min(1);
        let side = if h == RIGHT { 1.0 } else { -1.0 };
        self.hand_pos[h].kick(Vec3::new(0.2 * side, -0.35, 1.0) * tu.tether_whip);
        self.hand_rot[h].kick(Vec3::new(-tu.tether_whip_rot, 0.3 * side * tu.tether_whip_rot, -0.4 * side * tu.tether_whip_rot));
    }

    /// A phrase left the ring: the ring hand punches forward, the view recoils.
    pub fn cast(&mut self, tu: &VmTuning) {
        self.hand_pos[RIGHT].kick(Vec3::new(-0.15, 0.1, -1.0) * tu.cast_thrust);
        self.hand_rot[RIGHT].kick(Vec3::NEG_X * tu.cast_rot);
        self.imp_pos.kick(Vec3::Z * tu.cast_recoil);
        self.imp_rot.kick(Vec3::X * tu.cast_recoil * 2.0);
    }

    /// Stand rush token: alternate fists.
    pub fn rush(&mut self, tu: &VmTuning) {
        let h = self.rush_hand;
        self.rush_hand = 1 - h.min(1);
        let side = if h == RIGHT { -1.0 } else { 1.0 };
        self.hand_pos[h].kick(Vec3::new(0.2 * side, 0.1, -1.0) * tu.cast_thrust * 0.8);
        self.imp_pos.kick(Vec3::Z * tu.cast_recoil * 0.4);
    }

    /// A letter typed into the ring console / oath: a small twitch of the ring hand.
    pub fn type_key(&mut self, tu: &VmTuning) {
        self.type_sign = if self.type_sign > 0.0 { -1.0 } else { 1.0 };
        self.hand_rot[RIGHT].kick(Vec3::new(0.3, 0.0, self.type_sign) * tu.ring_type_pulse);
        self.hand_pos[RIGHT].kick(Vec3::new(0.0, 0.02, -0.06));
    }

    /// Gun fired (`scale` from the archetype); `predicted` = played on the trigger press.
    pub fn gun_kick(&mut self, tu: &VmTuning, archetype: GunArchetype, scale: f32) {
        let s = tu.gun_scale(archetype) * scale;
        if s <= 0.0 {
            return;
        }
        self.shots = self.shots.wrapping_add(1);
        // Deterministic per-shot scatter in [-1, 1].
        let h = |k: u32| crate::util::hash3(self.shots as i32, k as i32, 17) * 2.0 - 1.0;
        self.gun_rot.kick(Vec3::new(tu.gun_kick_pitch, h(1) * tu.gun_kick_yaw, h(2) * tu.gun_kick_roll) * s);
        self.gun_pos.kick(Vec3::new(h(3) * 0.05, tu.gun_kick_up, tu.gun_kick_back) * s);
        if archetype == GunArchetype::Minigun {
            self.buzz = 0.12;
        }
    }

    /// Swing wind-up (held construct): the hand coils back before the swing.
    pub fn swing_start(&mut self, tu: &VmTuning, weight: f32, heavy: bool) {
        let k = tu.swing_anticipation * weight.clamp(0.5, 2.5) * if heavy { 1.5 } else { 1.0 };
        self.imp_rot.kick(Vec3::new(0.8, -1.2, 0.0) * k);
        self.imp_pos.kick(Vec3::new(0.05, 0.05, 0.3) * k);
        self.coiled = false;
    }

    pub fn swing_active(&mut self, tu: &VmTuning, weight: f32) {
        let k = tu.swing_follow * weight.clamp(0.5, 2.5).sqrt();
        self.imp_rot.kick(Vec3::new(-1.0, 2.0, 0.0) * k);
        self.imp_pos.kick(Vec3::new(-0.1, 0.0, -0.6) * k);
    }

    pub fn swing_impact(&mut self, tu: &VmTuning, weight: f32) {
        let k = tu.swing_impact * weight.clamp(0.5, 2.5);
        self.imp_pos.kick(Vec3::new(0.0, 0.25, 0.6) * k);
        self.imp_rot.kick(Vec3::new(1.8, 0.0, 0.0) * k);
    }

    pub fn set_coiled(&mut self, on: bool) {
        self.coiled = on;
    }

    /// Took a hit travelling along `dir_cam` (camera space) with impact `strength` (≈0.1…2).
    pub fn hit(&mut self, tu: &VmTuning, dir_cam: Vec3, strength: f32) {
        let s = if strength.is_finite() { strength.clamp(0.0, 1.5) } else { 0.0 };
        let d = dir_cam.try_normalize().unwrap_or(Vec3::Z);
        self.imp_pos.kick(d * tu.flinch * s);
        self.imp_rot.kick(Vec3::new(d.y, -d.x, -d.x * 0.5) * tu.flinch_rot * s);
        self.hand_rot[LEFT].kick(Vec3::new(1.0, 0.5, 0.0) * tu.flinch_rot * 0.5 * s);
        self.hand_rot[RIGHT].kick(Vec3::new(1.0, -0.5, 0.0) * tu.flinch_rot * 0.5 * s);
    }

    pub fn launched(&mut self, speed: f32) {
        let s = if speed.is_finite() { speed.max(0.0) } else { 0.0 };
        self.flail = self.flail.max((0.5 + s / 60.0).min(1.5));
    }

    pub fn ko(&mut self, tu: &VmTuning) {
        self.ko_timer = tu.ko_hold;
        self.imp_pos.kick(Vec3::new(0.0, -0.4, 0.2));
    }

    /// Advance one rendered frame.
    pub fn step(&mut self, tu: &VmTuning, inp: &AnimInput) -> HandPose {
        let dt = if inp.dt.is_finite() { inp.dt.clamp(0.0, 0.1) } else { 0.0 };
        self.t = (self.t + dt) % 10_000.0;
        let t = self.t;
        let m = if inp.motion_scale.is_finite() { inp.motion_scale.clamp(0.0, 2.0) } else { 1.0 };
        let w = if inp.held && inp.held_weight.is_finite() { inp.held_weight.clamp(0.5, 2.5) } else { 1.0 };
        let steady = 1.0 + (tu.oath_steady - 1.0) * self.oath_w.x.clamp(0.0, 1.0);

        // KO: hands drop, stay visible briefly.
        if inp.alive {
            if !self.was_alive {
                self.ko_w = Spring::default();
                self.ko_timer = 0.0;
            }
        } else if self.was_alive && self.ko_timer <= 0.0 {
            self.ko_timer = tu.ko_hold;
        }
        self.was_alive = inp.alive;
        self.ko_timer = (self.ko_timer - dt).max(0.0);
        let ko = !inp.alive;

        // --- pose weights (slightly underdamped: they arrive with a little follow-through)
        let (po, pz) = (tu.pose_omega, tu.pose_zeta);
        self.ring_w.step(if inp.ring_open && !inp.oath { 1.0 } else { 0.0 }, po, pz, dt);
        self.oath_w.step(if inp.oath { 1.0 } else { 0.0 }, po, pz, dt);
        self.slide_w.step(if inp.sliding { 1.0 } else { 0.0 }, po * 1.2, 0.9, dt);
        self.charge_w.step(if inp.charging { 1.0 } else { 0.0 }, po, pz, dt);
        self.coil_w.step(if self.coiled && inp.held { 1.0 } else { 0.0 }, po, 0.8, dt);
        self.ko_w.step(if ko { 1.0 } else { 0.0 }, po * 0.8, 0.9, dt);
        let clamp01 = |s: &Spring| s.x.clamp(-0.2, 1.3);
        let (ring_w, oath_w, slide_w, charge_w, coil_w, ko_w) =
            (clamp01(&self.ring_w), clamp01(&self.oath_w), clamp01(&self.slide_w), clamp01(&self.charge_w), clamp01(&self.coil_w), clamp01(&self.ko_w));

        // --- look sway (heavier held things trail more and settle slower)
        let max_rate = tu.look_max_rate.max(0.0);
        let rate = if inp.look_rate.is_finite() { inp.look_rate.clamp(Vec2::splat(-max_rate), Vec2::splat(max_rate)) } else { Vec2::ZERO };
        let lag = w.powf(tu.held_lag_exp) * m * steady;
        let settle = w.powf(-tu.held_settle_exp);
        let lp = Vec3::new(rate.x * tu.look_pos, -rate.y * tu.look_pos, 0.0) * lag;
        let lr = Vec3::new(-rate.y * tu.look_rot, -rate.x * tu.look_rot, -rate.x * tu.look_roll) * lag;
        self.look_pos.step(lp.clamp(Vec3::splat(-0.04), Vec3::splat(0.04)), tu.look_omega * settle, tu.look_zeta, dt);
        self.look_rot.step(lr.clamp(Vec3::splat(-0.2), Vec3::splat(0.2)), tu.look_omega * settle, tu.look_zeta, dt);

        // --- footstep cadence (same rule as the footstep sounds)
        let step_len = tu.step_len.max(0.2);
        let tick_dist = if inp.tick_dist.is_finite() { inp.tick_dist.clamp(0.0, 2.0) } else { 0.0 };
        let on_feet = inp.grounded && !inp.sliding;
        if on_feet {
            if inp.ticks > 0 {
                self.stride += tick_dist * inp.ticks as f32;
                if self.stride > step_len {
                    self.stride = 0.0;
                    self.steps = (self.steps + 1.0) % 1024.0;
                }
            }
        } else {
            self.stride = step_len * (1.8 / 2.6);
        }
        let render_stride = if on_feet { (self.stride - (1.0 - inp.alpha.clamp(0.0, 1.0)) * tick_dist).max(0.0) } else { self.stride };
        let phase = self.steps + (render_stride / step_len).min(1.0);
        let vel = if inp.vel_cam.is_finite() { inp.vel_cam } else { Vec3::ZERO };
        let h_speed = Vec2::new(vel.x, vel.z).length();
        let bob_target = if on_feet { (h_speed / tu.bob_ref_speed.max(0.1)).min(tu.bob_max) } else { 0.0 };
        self.bob_amt.step(bob_target, 10.0, 1.0, dt);
        let bob = self.bob_amt.x.max(0.0) * m * steady * w.sqrt();
        let (sp, cp) = ((PI * phase).sin(), (PI * phase).sin().abs());
        let bob_pos = Vec3::new(tu.bob_x * sp, -tu.bob_y * (1.0 - cp), 0.0) * bob;
        let bob_rot = Vec3::new(-tu.bob_pitch * (1.0 - cp), 0.0, tu.bob_roll * sp) * bob;

        // --- movement poses
        let ref_speed = tu.bob_ref_speed.max(0.1);
        let lat = (vel.x / ref_speed).clamp(-1.5, 1.5);
        let tuck = smoothstep(tu.tuck_from, tu.tuck_to, h_speed);
        let mut mp = Vec3::new(-lat * tu.strafe_x, 0.0, 0.0) + v3a(tu.tuck_pos) * tuck;
        let mut mr = Vec3::new(tu.tuck_pitch * tuck, 0.0, -lat * tu.strafe_roll);
        if !inp.grounded && !inp.tethered {
            mp.y += (-vel.y * tu.air_float).clamp(-tu.air_float_max, tu.air_float_max);
        }
        if inp.crouched && !inp.sliding {
            mp.y += tu.crouch_y;
        }
        mp += v3a(tu.slide_pos) * slide_w;
        mr += v3a(tu.slide_rot) * slide_w;
        self.move_pos.step(mp * m, tu.move_omega * settle, tu.move_zeta, dt);
        self.move_rot.step(mr * m, tu.move_omega * settle, tu.move_zeta, dt);

        // --- impulse springs (jump / land / hits / casts / swings)
        self.imp_pos.step(Vec3::ZERO, tu.imp_omega * settle, tu.imp_zeta, dt);
        self.imp_rot.step(Vec3::ZERO, tu.imp_omega * settle, tu.imp_zeta, dt);
        self.gun_pos.step(Vec3::ZERO, tu.gun_omega, tu.gun_zeta, dt);
        self.gun_rot.step(Vec3::ZERO, tu.gun_omega, tu.gun_zeta, dt);

        // --- idle breathing + micro drift (quieter while the feet carry the motion)
        let idle = (1.0 - 0.6 * self.bob_amt.x.clamp(0.0, 1.0)) * steady;
        let br = (TAU * tu.breath_rate * t).sin();
        let idle_pos = Vec3::new(
            tu.drift_amp * ((t * 0.37).sin() + 0.6 * (t * 0.61 + 1.3).sin()),
            tu.breath_amp * br + tu.drift_amp * 0.7 * (t * 0.29 + 0.4).sin(),
            0.0,
        ) * idle;
        let idle_rot = Vec3::new(tu.breath_rot * (TAU * tu.breath_rate * t + 0.6).sin(), tu.drift_rot * (t * 0.23 + 2.0).sin(), 0.0) * idle;

        // --- per-hand targets
        let mut tgt_pos = [Vec3::ZERO; 2];
        let mut tgt_rot = [Vec3::ZERO; 2];
        let mut tremble = [0.0f32; 2];
        tgt_pos[LEFT].x += tu.tuck_in * tuck * m;
        tgt_pos[RIGHT].x -= tu.tuck_in * tuck * m;
        tgt_pos[LEFT] += v3a(tu.slide_brace) * slide_w;
        tgt_pos[RIGHT] += v3a(tu.slide_brace) * Vec3::new(-0.5, 0.5, 0.3) * slide_w;
        // Tethers: reach toward the hook, grip taut toward the anchor, strain while reeling.
        for h in 0..2 {
            let hk = inp.hooks[h];
            let on = hk.phase != HookPhase::Idle;
            if on && let Some(d) = hk.dir.try_normalize() {
                let k = 1.0 - (-18.0 * dt).exp();
                self.aim_dir[h] = self.aim_dir[h].lerp(d, k).normalize_or(d);
                if hk.phase == HookPhase::Attached {
                    tgt_pos[h] += d * tu.tether_pull + Vec3::Y * tu.tether_lift;
                    if hk.straining {
                        tremble[h] += tu.tether_tremble;
                    }
                } else {
                    tgt_pos[h] += d * tu.tether_pull * 0.5;
                }
            }
            self.aim_w[h].step(if on { 1.0 } else { 0.0 }, tu.hand_omega, 0.8, dt);
        }
        // Ring raised into view (typing / push-to-talk) or held up steady for the oath.
        tgt_pos[RIGHT] += v3a(tu.ring_raise_pos) * ring_w + v3a(tu.oath_raise_pos) * oath_w;
        tgt_rot[RIGHT] += v3a(tu.ring_raise_rot) * ring_w + v3a(tu.oath_raise_rot) * oath_w;
        tremble[RIGHT] += inp.voice_level.clamp(0.0, 1.0) * tu.ring_voice_tremble * ring_w.max(0.0);
        // Held charge / draw: the hand coils back and quivers.
        tgt_pos[RIGHT] += v3a(tu.coil_pos) * coil_w;
        tremble[RIGHT] += tu.coil_tremble * coil_w.max(0.0);
        // Charge: both fists clench in, trembling harder as it builds.
        let ramp = (inp.charge_held / 0.8).clamp(0.0, 1.0);
        let ct = if inp.charge_full { tu.charge_tremble_full } else { tu.charge_tremble * (0.35 + 0.65 * ramp) };
        let (cpos, crot) = (v3a(tu.charge_pos), v3a(tu.charge_rot));
        tgt_pos[LEFT] += cpos * charge_w;
        tgt_rot[LEFT] += crot * charge_w;
        tgt_pos[RIGHT] += Vec3::new(-cpos.x, cpos.y, cpos.z) * charge_w;
        tgt_rot[RIGHT] += Vec3::new(crot.x, -crot.y, -crot.z) * charge_w;
        tremble[LEFT] += ct * charge_w.max(0.0);
        tremble[RIGHT] += ct * charge_w.max(0.0);
        // Launched / tumbling: a little flail.
        if inp.tumbling {
            self.flail = self.flail.max(0.3);
        }
        self.flail = (self.flail - dt).max(0.0);
        let flail = self.flail.min(1.0) * tu.flail;
        if flail > 0.0 {
            let ft = t * TAU * tu.flail_rate;
            tgt_rot[LEFT] += wobble(ft, 1.0) * flail * Vec3::new(1.0, 0.7, 1.2);
            tgt_rot[RIGHT] += wobble(ft, 4.0) * flail * Vec3::new(1.0, 0.7, 1.2);
            tgt_pos[LEFT] += wobble(ft * 0.8, 7.0) * flail * 0.15;
            tgt_pos[RIGHT] += wobble(ft * 0.8, 9.0) * flail * 0.15;
        }

        let mut pose = HandPose::default();
        for h in 0..2 {
            let tp = tgt_pos[h].clamp(Vec3::splat(-0.15), Vec3::splat(0.15));
            let tr = tgt_rot[h].clamp(Vec3::splat(-1.2), Vec3::splat(1.2));
            self.hand_pos[h].step(tp, tu.hand_omega * settle.max(0.5), tu.hand_zeta, dt);
            self.hand_rot[h].step(tr, tu.hand_omega * settle.max(0.5), tu.hand_zeta, dt);
            let tr_amp = tremble[h];
            let shake = if tr_amp > 0.0 { wobble(t * 61.0, h as f32 * 3.0 + 0.5) * tr_amp } else { Vec3::ZERO };
            let aim = arc_limited(rest_dir(h), self.aim_dir[h], tu.tether_aim_max);
            let aim_w = (self.aim_w[h].x * tu.tether_aim).clamp(0.0, 1.0);
            pose.hand_t[h] = (self.hand_pos[h].x + shake).clamp(Vec3::splat(-0.2), Vec3::splat(0.2)) * tu.master;
            let rot = self.hand_rot[h].x.clamp(Vec3::splat(-1.5), Vec3::splat(1.5)) + shake * 8.0;
            pose.hand_r[h] = Quat::IDENTITY.slerp(Quat::IDENTITY.slerp(aim, aim_w) * euler(rot), tu.master.clamp(0.0, 1.0));
        }

        // --- minigun buzz
        self.buzz = (self.buzz - dt).max(0.0);
        let buzz = if self.buzz > 0.0 { wobble(t * 90.0, 11.0) * tu.minigun_buzz } else { Vec3::ZERO };

        // --- root: sum of layers; roll respects comfort
        let ko_pos = Vec3::new(0.0, -tu.ko_drop, 0.06) * ko_w;
        let ko_rot = Vec3::new(-0.6, 0.0, 0.15) * ko_w;
        let root_t = self.look_pos.x + self.move_pos.x + self.imp_pos.x + self.gun_pos.x + bob_pos + idle_pos + buzz + ko_pos;
        let mut root_e = self.look_rot.x + self.move_rot.x + self.imp_rot.x + self.gun_rot.x + bob_rot + idle_rot + buzz * 6.0 + ko_rot;
        root_e.z *= inp.roll_scale.clamp(0.0, 1.0);
        pose.root_t = root_t.clamp(Vec3::splat(-0.4), Vec3::splat(0.4)) * tu.master;
        pose.root_r = euler(root_e.clamp(Vec3::splat(-1.0), Vec3::splat(1.0)) * tu.master);
        pose.glow = charge_w.max(0.0) * tu.charge_glow * (0.4 + 0.6 * ramp) + 0.25 * ring_w.max(0.0);
        pose.ko_visible = ko && self.ko_timer > 0.0;

        let finite = pose.root_t.is_finite() && pose.root_r.is_finite() && pose.hand_t.iter().all(|v| v.is_finite()) && pose.hand_r.iter().all(|q| q.is_finite());
        if !finite {
            *self = HandAnimator::default();
            return HandPose::default();
        }
        pose
    }
}

// ------------------------------------------------------------------------------------------ bevy glue

/// The local hands in camera space (from `animate_viewmodel`) and world space (this frame's camera).
/// Tether ropes start at `world`.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ViewmodelHands {
    pub active: bool,
    /// Fist points in camera space (0 left, 1 right).
    pub local: [Vec3; 2],
    /// Fist points in world space, valid in PostUpdate after [`HandsPublishSet`].
    pub world: [Vec3; 2],
}

/// PostUpdate: after the camera is placed, before transform propagation. Readers of
/// [`ViewmodelHands::world`] order after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandsPublishSet;

#[derive(Resource, Default)]
pub(super) struct AnimState {
    anim: HandAnimator,
    last_look: Option<(f32, f32)>,
    last_buf: usize,
    last_oath: f32,
    prev_buttons: u16,
    prof: Prof,
}

/// System cost (demo / profiling).
#[derive(Default)]
struct Prof {
    n: u32,
    sum_us: f64,
    max_us: f64,
    t: f32,
}

pub struct HandAnimPlugin;

impl Plugin for HandAnimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VmTuningRes(load_tuning()))
            .init_resource::<AnimState>()
            .init_resource::<HandPose>()
            .init_resource::<ViewmodelHands>()
            .add_systems(Update, hot_reload_tuning)
            .configure_sets(PostUpdate, HandsPublishSet.after(CameraUpdateSet).before(TransformSystems::Propagate))
            .add_systems(PostUpdate, publish_hands.in_set(HandsPublishSet));
        demo::register(app);
    }
}

type AnimRes<'w> = (
    Res<'w, VmTuningRes>,
    Res<'w, LookAngles>,
    Res<'w, ViewAnchor>,
    Res<'w, TickClock>,
    Res<'w, FrameMoveEvents>,
    Res<'w, LocalInputState>,
    Res<'w, UiFocus>,
    Res<'w, ComfortSettings>,
);

type UiRes<'w> = (Option<Res<'w, RingConsole>>, Res<'w, crate::oath::OathState>, Res<'w, crate::voice_ui::VoiceUi>, Option<Res<'w, crate::present::impact::HitFreeze>>);

/// Gather this frame's inputs and advance the hand animation (before `animate_viewmodel`).
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_hand_anim(
    time: Res<Time>,
    (tuning, look, anchor, clock, move_events, input, focus, comfort): AnimRes,
    (console, oath, voice, freeze): UiRes,
    active: Option<Res<ActiveWorld>>,
    local_q: Query<&PlayerSim, With<LocalPlayer>>,
    mut game_events: MessageReader<GameEvents>,
    mut cues: MessageReader<RingCue>,
    mut st: ResMut<AnimState>,
    mut pose_out: ResMut<HandPose>,
) {
    let t0 = std::time::Instant::now();
    let tu = &tuning.0;
    let st = &mut *st;
    let (Some(a), Ok(sim)) = (active.as_deref(), local_q.single()) else {
        game_events.clear();
        cues.clear();
        st.last_look = None;
        *pose_out = HandPose::default();
        return;
    };
    let Some(me) = a.local_player() else {
        game_events.clear();
        cues.clear();
        return;
    };
    let local = me.id;
    let mut dt = time.delta_secs();
    // Render-side hit-stop: our own hands hold still for the freeze (reads as contact).
    if freeze.as_deref().is_some_and(|f| f.player(local)) {
        dt *= 0.1;
    }
    let cam_rot = look.rotation();
    let to_cam = cam_rot.inverse();
    let motion = tu.comfort_motion + (1.0 - tu.comfort_motion) * comfort.landing_dip.clamp(0.0, 1.0);
    let roll_scale = comfort.landing_dip.clamp(0.0, 1.0);
    let anim = &mut st.anim;

    // Look rate from this frame's raw mouse look (teleports / respawn facing are ignored).
    let mut look_rate = Vec2::ZERO;
    if let Some((ly, lp)) = st.last_look
        && dt > 1e-4
    {
        let dy = (look.yaw - ly + PI).rem_euclid(TAU) - PI;
        let dp = look.pitch - lp;
        if dy.abs() < 0.6 && dp.abs() < 0.6 {
            look_rate = Vec2::new(dy, dp) / time.delta_secs().max(1e-4);
        }
    }
    st.last_look = Some((look.yaw, look.pitch));

    // Held construct: weight and kind.
    let held = a.world.constructs.held_by(local).filter(|c| c.category == FormCategory::Held);
    let held_weight = held.map_or(1.0, |c| bd_shapes::weight_of(&c.recipe));
    let held_gun = held.is_some_and(|c| matches!(c.recipe.form, Form::Gun | Form::Cannon | Form::Bow | Form::Staff));

    // Movement events (predicted, this frame's ticks).
    for ev in &move_events.0 {
        if ev.jumped {
            anim.jump(tu, ev.perfect_hop, motion);
        }
        if let Some(l) = ev.landed {
            anim.land(tu, l.impact_speed, motion);
        }
        if ev.boost_burst {
            anim.boost(tu, motion);
        }
        for h in 0..2 {
            if ev.tether_fired[h] {
                let hp = v3(sim.state.hooks[h].pos);
                let d = to_cam * (hp - anchor.eye);
                let d = if d.length_squared() > 1e-4 { d } else { to_cam * look.forward() };
                anim.tether_fire(tu, h, d);
            }
            if ev.tether_released[h] || ev.tether_broken[h] {
                anim.tether_release(tu, h);
            }
        }
    }

    // Ring cues fire on the key press (before the sim answers).
    for c in cues.read() {
        match c {
            RingCue::Forming { .. } => anim.cast(tu),
            RingCue::Rush => anim.rush(tu),
            RingCue::Wake | RingCue::Sleep => {}
        }
    }
    // Typing: every letter twitches the ring hand (console buffer / oath progress).
    let ring_open = console.as_deref().is_some_and(|c| c.open) || voice.listening;
    let buf_len = console.as_deref().map_or(0, |c| c.buf.len());
    if console.as_deref().is_some_and(|c| c.open) && buf_len != st.last_buf {
        anim.type_key(tu);
    }
    st.last_buf = buf_len;
    let oath_p = oath.progress();
    if oath.open && oath_p > st.last_oath {
        anim.type_key(tu);
    }
    st.last_oath = oath_p;

    // Client: the host confirms shots ~RTT later; show the trigger pull now.
    let b = input.current.buttons;
    let pressed = b & !st.prev_buttons;
    st.prev_buttons = b;
    if a.role == SimRole::Client && held_gun && pressed & buttons::PRIMARY != 0 && !focus.typing {
        anim.gun_kick(tu, GunArchetype::HandCannon, tu.gun_predict);
    }

    for e in game_events.read() {
        match &e.event {
            GameEvent::ShotFired { owner, construct: Some(_), archetype, .. } if *owner == local => {
                let predicted = if a.role == SimRole::Client { 1.0 - tu.gun_predict } else { 1.0 };
                anim.gun_kick(tu, *archetype, predicted);
            }
            GameEvent::HeldFx { owner, fx, strength, .. } if *owner == local => match fx {
                HeldFx::SwingStart => anim.swing_start(tu, held_weight, *strength >= 1.9),
                HeldFx::SwingActive => anim.swing_active(tu, held_weight),
                HeldFx::SwingImpact | HeldFx::DiveSlam | HeldFx::GroundPound => anim.swing_impact(tu, held_weight),
                HeldFx::ChargeStart => anim.set_coiled(true),
                HeldFx::Throw => {
                    anim.set_coiled(false);
                    anim.cast(tu);
                }
                _ => anim.set_coiled(false),
            },
            GameEvent::Hit { target, impact, .. } if *target == local => {
                let d = to_cam * v3(impact.dir);
                anim.hit(tu, d, impact.strength);
            }
            GameEvent::Launched { player, speed, .. } if *player == local => anim.launched(*speed),
            GameEvent::Ko { victim, .. } if *victim == local => anim.ko(tu),
            _ => {}
        }
    }

    // Continuous state: the predicted local player, interpolated like the camera.
    let alpha = clock.alpha;
    let vel = v3(sim.prev.vel).lerp(v3(sim.state.vel), alpha);
    let s = &sim.state;
    let d = v3(s.pos - sim.prev.pos);
    let reeling = input.current.held(buttons::REEL) || input.current.reel_notches != 0 || s.boosting;
    let mut hooks = [HookIn::default(); 2];
    for (h, hk) in hooks.iter_mut().enumerate() {
        let hook = s.hooks[h];
        if hook.phase == HookPhase::Idle {
            continue;
        }
        let p = if hook.phase == HookPhase::Flying && sim.prev.hooks[h].phase == HookPhase::Flying {
            v3(sim.prev.hooks[h].pos).lerp(v3(hook.pos), alpha)
        } else {
            v3(hook.pos)
        };
        *hk = HookIn { phase: hook.phase, dir: (to_cam * (p - anchor.eye)).normalize_or(Vec3::NEG_Z), straining: hook.phase == HookPhase::Attached && reeling };
    }
    let charging = me.charge.active || (a.role == SimRole::Client && input.current.held(buttons::CHARGE) && !focus.typing && me.alive);
    let inp = AnimInput {
        dt,
        look_rate,
        vel_cam: to_cam * vel,
        grounded: s.grounded,
        sliding: s.sliding,
        crouched: s.crouched,
        tethered: s.tethered(),
        ticks: move_events.0.len() as u32,
        tick_dist: Vec2::new(d.x, d.z).length(),
        alpha,
        tumbling: s.tumble > 0.0 || me.launch.is_some(),
        hooks,
        ring_open,
        oath: oath.open,
        voice_level: voice.level,
        charging,
        charge_held: me.charge.held,
        charge_full: me.charge.full,
        held: held.is_some(),
        held_weight,
        motion_scale: motion,
        roll_scale,
        alive: me.alive,
    };
    *pose_out = anim.step(tu, &inp);

    let p = &mut st.prof;
    let us = t0.elapsed().as_secs_f64() * 1e6;
    p.n += 1;
    p.sum_us += us;
    p.max_us = p.max_us.max(us);
    p.t += time.delta_secs();
    if demo::active() && p.t > 2.0 {
        println!("HANDS_PERF frames={} avg_us={:.2} max_us={:.2}", p.n, p.sum_us / p.n.max(1) as f64, p.max_us);
        *p = Prof::default();
    }
}

/// PostUpdate: hand points → world space from this frame's camera; the first-person held construct
/// re-placed from its camera-space grip (keeps `sync_constructs`' coverage clamp).
#[allow(clippy::type_complexity)]
fn publish_hands(
    mut hands: ResMut<ViewmodelHands>,
    held: Res<FirstPersonHeld>,
    map: Option<Res<ConstructEntities>>,
    cam: Query<&Transform, (With<PlayerCamera>, Without<SimConstruct>)>,
    mut items: Query<&mut Transform, (With<SimConstruct>, Without<PlayerCamera>)>,
) {
    let Ok(cam) = cam.single() else {
        hands.active = false;
        return;
    };
    if hands.active {
        hands.world = [cam.transform_point(hands.local[0]), cam.transform_point(hands.local[1])];
    }
    if !held.active {
        return;
    }
    let (Some(id), Some(map)) = (held.construct, map) else { return };
    let Some(&e) = map.0.get(&id) else { return };
    let Ok(mut tf) = items.get_mut(e) else { return };
    let world = cam.mul_transform(held.grip_local);
    let base = held.grip.scale.max_element();
    let clamp = if base > 1e-6 { tf.scale.max_element() / base } else { 1.0 };
    if world.is_finite() && clamp.is_finite() {
        *tf = Transform { scale: world.scale * clamp, ..world };
    }
}

// ------------------------------------------------------------------------------------------ demo

/// `--practice --script hands`: every hand action in sequence, scripted (for recordings).
mod demo {
    use super::*;
    use crate::console::Caster;
    use crate::input::InputOverride;
    use crate::state::AppState;

    pub fn active() -> bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| {
            let a: Vec<String> = std::env::args().collect();
            a.windows(2).any(|w| w[0] == "--script" && w[1] == "hands") || std::env::var_os("BD_HANDS_PROF").is_some()
        })
    }

    fn scripted() -> bool {
        let a: Vec<String> = std::env::args().collect();
        a.windows(2).any(|w| w[0] == "--script" && w[1] == "hands")
    }

    pub fn register(app: &mut App) {
        if scripted() {
            app.init_resource::<Demo>().add_systems(Update, run_demo.before(crate::tick::FrameSet::Input).run_if(in_state(AppState::InGame)));
        }
    }

    /// `BD_HANDS_DEMO=idle,run,...` picks segments; default = all. `BD_HANDS_T0` offsets the clock.
    #[derive(Resource, Default)]
    pub struct Demo {
        t: f32,
        segment: usize,
        seg_t: f32,
        started: bool,
        cast_done: bool,
        base_yaw: f32,
        hit_n: u32,
    }

    const SEGMENTS: &[(&str, f32)] = &[
        ("idle", 3.0),
        ("look", 3.0),
        ("run", 4.0),
        ("jump", 3.5),
        ("tether", 5.0),
        ("charge", 3.0),
        ("hit", 3.0),
        // Casts replace the held item, so these come last.
        ("ring", 4.5),
        ("heavy", 5.0),
        ("gun", 4.0),
        ("minigun", 3.0),
    ];

    fn selected() -> Vec<&'static str> {
        match std::env::var("BD_HANDS_DEMO") {
            Ok(s) if !s.is_empty() => SEGMENTS.iter().map(|(n, _)| *n).filter(|n| s.split(',').any(|x| x.trim() == *n)).collect(),
            _ => SEGMENTS.iter().map(|(n, _)| *n).collect(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_demo(
        time: Res<Time>,
        mut demo: ResMut<Demo>,
        mut look: ResMut<LookAngles>,
        mut ov: ResMut<InputOverride>,
        mut console: Option<ResMut<RingConsole>>,
        mut caster: Caster,
        active: Option<Res<ActiveWorld>>,
        mut events: MessageWriter<GameEvents>,
    ) {
        let dt = time.delta_secs();
        let d = &mut *demo;
        d.t += dt;
        if !d.started {
            d.started = true;
            d.base_yaw = look.yaw;
        }
        let segs = selected();
        let Some(&name) = segs.get(d.segment) else {
            ov.active = true;
            ov.held = 0;
            ov.move_x = 0;
            ov.move_z = 0;
            return;
        };
        let dur = SEGMENTS.iter().find(|(n, _)| *n == name).map_or(3.0, |s| s.1);
        let t = d.seg_t;
        d.seg_t += dt;
        if d.seg_t >= dur {
            d.segment += 1;
            d.seg_t = 0.0;
            d.cast_done = false;
            d.hit_n = 0;
            look.yaw = d.base_yaw;
            look.pitch = -0.05;
            if let Some(c) = console.as_deref_mut() {
                c.open = false;
                c.buf.clear();
            }
        }
        ov.active = true;
        ov.held = 0;
        ov.move_x = 0;
        ov.move_z = 0;
        let local = active.as_ref().and_then(|a| a.local);
        match name {
            "idle" => look.pitch = -0.05,
            "look" => {
                // Flicks left/right and up/down.
                let s = (t * 2.2).sin();
                look.yaw = d.base_yaw + 0.5 * s * s.abs();
                look.pitch = -0.05 + 0.18 * (t * 1.7).sin();
            }
            "run" => {
                ov.move_z = 1;
                ov.move_x = if t > 1.5 && t < 2.8 { 1 } else if t >= 2.8 { -1 } else { 0 };
                ov.held |= if t > 3.4 { buttons::CROUCH } else { 0 };
            }
            "jump" => {
                ov.move_z = 1;
                if (0.3..0.4).contains(&t) || (1.5..1.6).contains(&t) || (2.5..3.4).contains(&t) {
                    ov.held |= buttons::JUMP;
                }
            }
            "tether" => {
                look.pitch = 0.35;
                if (0.4..2.2).contains(&t) {
                    ov.held |= buttons::TETHER_L;
                }
                if (1.2..3.8).contains(&t) {
                    ov.held |= buttons::TETHER_R;
                }
                if (1.6..3.0).contains(&t) {
                    ov.held |= buttons::REEL;
                }
                ov.move_z = if t > 0.4 { 1 } else { 0 };
            }
            "ring" => {
                if let Some(c) = console.as_deref_mut() {
                    let word = "hammer";
                    if (0.4..2.6).contains(&t) {
                        c.open = true;
                        let n = (((t - 0.8) / 0.22).max(0.0) as usize).min(word.len());
                        if c.buf.len() != n {
                            c.buf.clear();
                            c.buf.push_str(&word[..n]);
                        }
                    } else if t >= 2.6 && !d.cast_done {
                        d.cast_done = true;
                        c.open = false;
                        c.buf.clear();
                        caster.cast(word, false, 0);
                    }
                }
            }
            "gun" => {
                if !d.cast_done {
                    d.cast_done = true;
                    caster.cast("hand cannon", false, 0);
                }
                if t > 1.0 && ((t * 2.5) % 1.0) < 0.15 {
                    ov.held |= buttons::PRIMARY;
                }
            }
            "minigun" => {
                if !d.cast_done {
                    d.cast_done = true;
                    caster.cast("minigun", false, 0);
                }
                if t > 1.0 {
                    ov.held |= buttons::PRIMARY;
                }
            }
            "heavy" => {
                if !d.cast_done {
                    d.cast_done = true;
                    caster.cast("giant anvil hammer", false, 0);
                }
                if t > 0.8 {
                    let s = (t * 2.0).sin();
                    look.yaw = d.base_yaw + 0.45 * s;
                    ov.move_z = 1;
                }
                if (3.6..3.7).contains(&t) {
                    ov.held |= buttons::PRIMARY;
                }
            }
            "charge" => {
                if (0.3..2.6).contains(&t) {
                    ov.held |= buttons::CHARGE;
                }
            }
            "hit" => {
                // Presentation-only hits on the local player (flinch in three directions).
                let at = [0.4, 1.2, 2.0];
                if let (Some(&when), Some(me)) = (at.get(d.hit_n as usize), local)
                    && t >= when
                {
                    let dirs = [bd_core::Vec3::new(1.0, 0.1, 0.0), bd_core::Vec3::new(-1.0, 0.3, 0.2), bd_core::Vec3::new(0.0, -0.2, 1.0)];
                    let dir = dirs[d.hit_n as usize % 3];
                    d.hit_n += 1;
                    let impact = bd_sim::combat::impact_fx(12.0, 14.0, 1.0, dir, 0.0);
                    events.write(GameEvents {
                        tick: 0,
                        event: GameEvent::Hit {
                            target: me,
                            source: None,
                            construct: None,
                            kind: bd_sim::HitKind::RingBlast,
                            element: Default::default(),
                            damage: 12.0,
                            knockback: 14.0,
                            point: bd_core::Vec3::ZERO,
                            bonus: 0.0,
                            tumble: false,
                            lethal: false,
                            crit: false,
                            impact,
                        },
                    });
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_settles_at_any_dt() {
        for &(omega, zeta) in &[(4.0, 0.3), (11.0, 0.6), (15.0, 1.0), (24.0, 0.42), (8.0, 0.999), (8.0, 1.005), (8.0, 3.0)] {
            for &dt in &[1.0 / 1000.0, 1.0 / 240.0, 1.0 / 60.0, 1.0 / 20.0, 0.1, 0.5, 1.0] {
                let mut s = Spring { x: 1.0, v: -3.0 };
                let mut peak = 0.0f32;
                let steps = (6.0 / dt) as usize + 1;
                for _ in 0..steps {
                    s.step(0.25, omega, zeta, dt);
                    assert!(s.x.is_finite() && s.v.is_finite(), "ω={omega} ζ={zeta} dt={dt}");
                    peak = peak.max((s.x - 0.25).abs());
                }
                assert!((s.x - 0.25).abs() < 1e-3 && s.v.abs() < 1e-2, "ω={omega} ζ={zeta} dt={dt} x={} v={}", s.x, s.v);
                // Never gains energy.
                assert!(peak <= 1.5, "ω={omega} ζ={zeta} dt={dt} peak={peak}");
            }
        }
    }

    #[test]
    fn spring_is_frame_rate_independent() {
        // Same impulse, integrated at 30 vs 480 fps, lands in the same place.
        let run = |dt: f32| {
            let mut s = Spring { x: 0.0, v: 2.0 };
            for _ in 0..(0.5 / dt).round() as usize {
                s.step(0.0, 14.0, 0.55, dt);
            }
            s.x
        };
        let (a, b) = (run(1.0 / 30.0), run(1.0 / 480.0));
        assert!((a - b).abs() < 1e-4, "{a} vs {b}");
    }

    #[test]
    fn spring_matches_ode() {
        // Compare the closed form against a tiny-step semi-implicit Euler integration.
        for &zeta in &[0.3f32, 1.0, 2.0] {
            let (omega, dt) = (12.0f32, 0.05f32);
            let (mut x, mut v) = (0.7f32, 1.5f32);
            let h = 1e-5f32;
            for _ in 0..(dt / h) as usize {
                v += (-omega * omega * x - 2.0 * zeta * omega * v) * h;
                x += v * h;
            }
            let (ex, ev) = spring_exact(0.7, 1.5, omega, zeta, dt);
            assert!((ex - x).abs() < 2e-3 && (ev - v).abs() < 2e-2, "ζ={zeta}: exact ({ex},{ev}) vs ode ({x},{v})");
        }
    }

    #[test]
    fn spring_rejects_garbage() {
        for &(y, v, w, z, dt) in &[
            (f32::NAN, 0.0, 10.0, 0.5, 0.01),
            (0.0, f32::INFINITY, 10.0, 0.5, 0.01),
            (1.0, 0.0, f32::NAN, 0.5, 0.01),
            (1.0, 0.0, 10.0, f32::NAN, 0.01),
            (1.0, 0.0, 10.0, 0.5, f32::NAN),
            (1.0, 0.0, 10.0, 0.5, -1.0),
            (1.0, 0.0, 1e30, 0.0, 1e30),
            (1e30, -1e30, 0.0, 0.0, 1.0),
        ] {
            let (a, b) = spring_exact(y, v, w, z, dt);
            assert!(a.is_finite() && b.is_finite(), "{y} {v} {w} {z} {dt} -> {a} {b}");
        }
        let mut s = Spring { x: 1.0, v: 0.0 };
        s.step(f32::NAN, 10.0, 0.5, 0.016);
        assert!(s.x.is_finite());
    }

    fn base_input(dt: f32) -> AnimInput {
        AnimInput { dt, alive: true, grounded: true, held_weight: 1.0, motion_scale: 1.0, roll_scale: 1.0, alpha: 0.5, ..Default::default() }
    }

    fn pose_finite(p: &HandPose) -> bool {
        p.root_t.is_finite() && p.root_r.is_finite() && p.hand_t.iter().all(|v| v.is_finite()) && p.hand_r.iter().all(|q| q.is_finite())
    }

    #[test]
    fn idle_is_subtle_and_events_settle() {
        let tu = VmTuning::default();
        let mut a = HandAnimator::default();
        let dt = 1.0 / 144.0;
        for _ in 0..600 {
            let p = a.step(&tu, &base_input(dt));
            assert!(p.root_t.length() < 0.01, "idle too big: {}", p.root_t);
        }
        a.jump(&tu, true, 1.0);
        a.land(&tu, 30.0, 1.0);
        a.cast(&tu);
        a.gun_kick(&tu, GunArchetype::Cannon, 1.0);
        a.hit(&tu, Vec3::X, 2.0);
        a.tether_fire(&tu, LEFT, Vec3::new(-0.3, 0.6, -0.7));
        a.tether_release(&tu, RIGHT);
        let mut peak = 0.0f32;
        for _ in 0..(3.0 / dt) as usize {
            let p = a.step(&tu, &base_input(dt));
            assert!(pose_finite(&p));
            peak = peak.max(p.root_t.length());
        }
        assert!(peak < 0.4);
        let p = a.step(&tu, &base_input(dt));
        assert!(p.root_t.length() < 0.01 && p.hand_t[0].length() < 0.005 && p.hand_t[1].length() < 0.005, "did not settle: {p:?}");
    }

    #[test]
    fn stable_under_random_inputs_and_dts() {
        let tu = VmTuning::default();
        let mut a = HandAnimator::default();
        let mut r = crate::util::Rng::new(99);
        for i in 0..20_000 {
            let dt = match i % 7 {
                0 => 0.0,
                1 => 1e-6,
                2 => 0.25,
                3 => f32::NAN,
                _ => r.range(0.001, 0.05),
            };
            let mut inp = base_input(dt);
            inp.look_rate = Vec2::new(r.range(-100.0, 100.0), r.range(-100.0, 100.0));
            inp.vel_cam = r.unit_vec3() * r.range(0.0, 80.0);
            inp.grounded = r.f32() < 0.5;
            inp.sliding = r.f32() < 0.2;
            inp.ticks = (r.f32() * 3.0) as u32;
            inp.tick_dist = r.range(0.0, 1.0);
            inp.charging = r.f32() < 0.3;
            inp.charge_held = r.range(0.0, 5.0);
            inp.ring_open = r.f32() < 0.3;
            inp.held = r.f32() < 0.5;
            inp.held_weight = if i % 13 == 0 { f32::INFINITY } else { r.range(0.6, 2.2) };
            inp.alive = r.f32() < 0.95;
            inp.hooks[0] = HookIn { phase: HookPhase::Attached, dir: r.unit_vec3(), straining: true };
            if i % 11 == 0 {
                inp.vel_cam = Vec3::NAN;
            }
            if i % 50 == 0 {
                a.land(&tu, r.range(0.0, 60.0), 1.0);
                a.gun_kick(&tu, GunArchetype::Minigun, 1.0);
                a.hit(&tu, r.unit_vec3(), r.range(0.0, 3.0));
                a.launched(r.range(0.0, 100.0));
            }
            let p = a.step(&tu, &inp);
            assert!(pose_finite(&p), "step {i}: {p:?}");
            assert!(p.root_t.length() < 0.8);
        }
    }

    #[test]
    fn bob_follows_footsteps_and_stops_in_air() {
        let tu = VmTuning::default();
        let mut a = HandAnimator::default();
        let dt = 1.0 / 120.0;
        let mut inp = base_input(dt);
        inp.vel_cam = Vec3::new(0.0, 0.0, -10.0);
        inp.ticks = 1;
        inp.tick_dist = 10.0 / 120.0;
        let mut ys = Vec::new();
        for _ in 0..240 {
            ys.push(a.step(&tu, &inp).root_t.y);
        }
        let (lo, hi) = ys[120..].iter().fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(*y), h.max(*y)));
        assert!(hi - lo > tu.bob_y * 0.6, "no bob: {lo}..{hi}");
        // In the air: bob fades out.
        inp.grounded = false;
        for _ in 0..120 {
            a.step(&tu, &inp);
        }
        let mut ys = Vec::new();
        for _ in 0..60 {
            ys.push(a.step(&tu, &inp).root_t.y);
        }
        let (lo, hi) = ys.iter().fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(*y), h.max(*y)));
        assert!(hi - lo < tu.bob_y * 0.3, "bob in the air: {lo}..{hi}");
    }

    #[test]
    fn comfort_removes_roll() {
        let tu = VmTuning::default();
        let mut a = HandAnimator::default();
        let mut inp = base_input(1.0 / 120.0);
        inp.vel_cam = Vec3::new(10.0, 0.0, -5.0);
        inp.look_rate = Vec2::new(4.0, 0.0);
        inp.roll_scale = 0.0;
        inp.motion_scale = tu.comfort_motion;
        for _ in 0..240 {
            let p = a.step(&tu, &inp);
            let (_, _, roll) = p.root_r.to_euler(EulerRot::YXZ);
            assert!(roll.abs() < 1e-5, "roll {roll}");
        }
    }

    #[test]
    fn heavy_held_sways_more() {
        let tu = VmTuning::default();
        let sway = |w: f32| {
            let mut a = HandAnimator::default();
            let mut inp = base_input(1.0 / 120.0);
            inp.held = true;
            inp.held_weight = w;
            inp.look_rate = Vec2::new(5.0, 0.0);
            let mut peak = 0.0f32;
            for _ in 0..60 {
                peak = peak.max(a.step(&tu, &inp).root_t.x.abs());
            }
            peak
        };
        assert!(sway(2.0) > sway(0.7) * 1.5);
    }

    #[test]
    fn step_is_cheap() {
        let tu = VmTuning::default();
        let mut a = HandAnimator::default();
        let mut inp = base_input(1.0 / 240.0);
        inp.hooks[1] = HookIn { phase: HookPhase::Attached, dir: Vec3::new(0.3, 0.5, -0.8).normalize(), straining: true };
        inp.charging = true;
        inp.look_rate = Vec2::new(2.0, 1.0);
        let n = 100_000;
        let t0 = std::time::Instant::now();
        let mut acc = 0.0;
        for _ in 0..n {
            acc += a.step(&tu, &inp).root_t.x;
        }
        let us = t0.elapsed().as_secs_f64() * 1e6 / n as f64;
        println!("HandAnimator::step: {us:.3} µs/frame (acc {acc})");
        // Generous (debug builds): well under 1% of a 7 ms frame.
        assert!(us < 50.0, "{us} µs");
    }

    #[test]
    fn default_tuning_roundtrips() {
        let t = VmTuning::default();
        let text = ron::ser::to_string_pretty(&t, Default::default()).unwrap();
        let back: VmTuning = ron::from_str(&text).unwrap();
        assert_eq!(t, back);
        let partial: VmTuning = ron::from_str("(bob_y: 0.02)").unwrap();
        assert_eq!(partial.bob_y, 0.02);
        assert_eq!(partial.look_pos, t.look_pos);
    }
}
