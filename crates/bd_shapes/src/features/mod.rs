//! Machine kit: a procedural generator + motion rig for every `bd_core::mechanics::Feature` (wheels,
//! treads, pistons, gears, boilers, rotors, sails, turrets, drills…). "Built from the inside out": every
//! assembly shows its moving parts — spoked wheels, crank-linked connecting rods, meshing gears,
//! scrolling tread cleats — and those parts move in sync with the construct.
//!
//! # Pipeline
//! * [`crate::generate_machine`] (and [`crate::generate`], which drops the sidecar) builds the shape. The
//!   `Vehicle` form picks an archetype from its features (locomotive, tank, helicopter, ship, plane,
//!   crane truck, car/truck — see `vehicles`); every feature the archetype did not build itself is
//!   placed generically on the body's bounds/facing by `place` (any form: a clockwork dragon's gears,
//!   a steampunk mech's boiler and pipes).
//! * Moving parts are ordinary `Part`s. How each one moves is recorded in a [`MachineSpec`] *sidecar*
//!   (ShapeSpec units, keyed by `PartId`) because the frozen `ShapeSpec` contract has no place for it.
//!   [`crate::build_model`] resolves it into [`VoxelModel::machine`](crate::VoxelModel) (metres, part
//!   indices). A spec voxelised on its own still animates by role: `Wheel` parts spin (rolling about
//!   their thin axis, or spinning when horizontal/face-on), `Exhaust` flickers, `Cloth` sways.
//! * Every frame `anim::pose` asks [`animate_features`] for each part (Idle / Locomotion layers), and
//!   the [`AnimState::Aim`](crate::AnimState) layer turns turrets (yaw) and barrels (pitch).
//!
//! # Linked motion
//! * Wheels: angle = −distance / radius (they roll without slipping; radius measured from voxels).
//! * Crank-slider: a piston's crosshead slides on a line; the connecting rod joins it to a crank pin
//!   on the driving wheel (exact kinematics, `Motion::Crank`); coupling rods translate with the pin.
//! * Gears: angle = ±(surface travel) / pitch radius — meshing gears share surface speed and
//!   counter-rotate. Clockwork gears tick.
//! * Treads: cleat rows scroll by `distance mod period` (top run forward, bottom run back).
//! * Rotors/propellers/turbines/drills/saws/dishes spin with time; sails/flags/antennae/bells/chains
//!   sway (more with speed); smokestacks stretch on every chuff.
//! * Puffs: [`machine_puffs`] reports smoke/steam puffs that happened between two drive states (a
//!   locomotive chuffs 4× per driver revolution) so the client spawns cube smoke in sync; the stack's
//!   squash uses the same phase.
//!
//! Budgets: machine parts stop animating (they are built static into their parent) once a spec holds
//! [`MAX_ANIMATED_PARTS`] parts. Repeated parts (wheels) share meshes: see [`crate::PartInstance`].
//!
//! Use `AnimState::Locomotion { speed, distance, .. }` (speed may be 0) for anything with wheels so
//! the wheel phase persists while stopped; `Idle` evaluates machines at distance 0.

mod kit;
mod place;
pub(crate) use kit::{around, ring};
pub(crate) mod vehicles;

use std::f32::consts::TAU;

use bd_core::mechanics::{Feature, Form};
use bd_core::shape::{PartId, PartRole, ShapeSpec};
use glam::{Affine3A, Quat, Vec3};

use crate::anim::{Local, PartKind};
use crate::build::B;
use crate::ease::smoothstep;
use crate::voxelize::{PartModel, VoxelModel};



/// Parts (including the root and markers) beyond which machine parts are built static.
pub const MAX_ANIMATED_PARTS: usize = 64;

/// Every feature, in declaration order.
pub const ALL_FEATURES: [Feature; 32] = {
    use Feature::*;
    [
        Wheels, Treads, Pistons, Gears, Boiler, Smokestack, Pipes, Exhaust, Propeller, Rotor, Jet, Turbine, Sails, Mast, Cockpit, Cab,
        Windows, Turret, Barrels, Drill, Saw, CraneArm, Chains, Antenna, Dish, Headlight, Plow, Fins, Spikes, Armor, Bell, Cage,
    ]
};

// ------------------------------------------------------------------------------------ sidecar

/// Build-time motion of one machine part (ShapeSpec units, rest-pose model space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotionSpec {
    /// Rotation about `axis` through the part pivot:
    /// `angle = sign · (rate·T + (surf·T − roll·distance) / R)`, T = seconds (stepped when `tick`),
    /// R = `radius` units (0 = measured from the part's voxels). `roll = 1` rolls along the ground
    /// (axis +X, forward = −Z); gears pass their pitch radius and alternate `sign`.
    Spin { axis: Vec3, sign: f32, rate: f32, surf: f32, roll: f32, radius: f32, tick: bool },
    /// Member of a crank-slider driven by the `Spin` part `wheel`: `pin` = crank-pin rest position,
    /// `cross` = crosshead rest position. The crosshead slides on the line through `cross`,
    /// perpendicular to the wheel axis and pointing away from the wheel centre.
    Crank { wheel: PartId, member: CrankMember, pin: Vec3, cross: Vec3 },
    /// Reciprocates along `axis`: `amp · sin(TAU·(hz·t + per_m·distance) + phase)` (units).
    Slide { axis: Vec3, amp: f32, hz: f32, per_m: f32, phase: f32 },
    /// Translates by `distance mod period` along `dir` (tread cleats; the pattern repeats per period).
    Scroll { dir: Vec3, period: f32 },
    /// Rotation about `axis`: `(amp + speed_amp·s)·sin(TAU·hz·t + phase) + lean·s`, s = 0..1 with speed.
    Sway { axis: Vec3, amp: f32, hz: f32, phase: f32, speed_amp: f32, lean: f32 },
    /// Turret yaw (`pitch = false`, about +Y) or barrel pitch (about +X), driven by `AnimState::Aim`.
    Aim { pitch: bool },
    /// Smokestack: stretches and settles on every puff of emitter `puff`.
    Pulse { puff: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CrankMember {
    /// Side/coupling rod: translates with the crank pin (parallel motion). Pivot = the pin.
    Coupling,
    /// Connecting rod: pin → crosshead. Pivot = the pin.
    Rod,
    /// Crosshead + piston rod: slides on the line. Pivot = the crosshead.
    Piston,
}

impl MotionSpec {
    /// A ground wheel on an X axle.
    pub fn roll() -> Self {
        MotionSpec::Spin { axis: Vec3::X, sign: 1.0, rate: 0.0, surf: 0.0, roll: 1.0, radius: 0.0, tick: false }
    }
    /// Constant spin (rad/s) about `axis`.
    pub fn spin(axis: Vec3, rate: f32) -> Self {
        MotionSpec::Spin { axis, sign: 1.0, rate, surf: 0.0, roll: 0.0, radius: 0.0, tick: false }
    }
}

/// A smoke/steam emitter (ShapeSpec units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PuffSpec {
    pub part: PartId,
    /// Rest-pose emission point and direction.
    pub at: Vec3,
    pub dir: Vec3,
    /// Puff diameter (units).
    pub size: f32,
    /// Puffs per revolution of `wheel` (a locomotive chuffs 4×).
    pub per_rev: f32,
    pub wheel: Option<PartId>,
    /// Puffs per second regardless of motion.
    pub idle_hz: f32,
    /// Puffs per metre travelled (exhausts without a wheel).
    pub per_m: f32,
}

/// Motion links + emitters produced alongside a `ShapeSpec` (see module docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineSpec {
    pub links: Vec<(PartId, MotionSpec)>,
    pub puffs: Vec<PuffSpec>,
}

// ------------------------------------------------------------------------------------ runtime

/// Runtime motion of one part (metres, model rest-pose space, part indices).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Motion {
    #[default]
    None,
    /// `angle = sign·(rate·T + surf_k·T − roll_k·distance)`.
    Spin { axis: Vec3, sign: f32, rate: f32, surf_k: f32, roll_k: f32, tick: bool },
    Crank { wheel: usize, member: CrankMember, center: Vec3, pin: Vec3, cross: Vec3, dir: Vec3, len: f32 },
    Slide { axis: Vec3, amp: f32, hz: f32, per_m: f32, phase: f32 },
    Scroll { dir: Vec3, period: f32 },
    Sway { axis: Vec3, amp: f32, hz: f32, phase: f32, speed_amp: f32, lean: f32 },
    Aim { pitch: bool },
    Pulse { puff: u8 },
}

/// A resolved smoke/steam emitter (metres, rest pose of `part_index`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PuffEmitter {
    pub part_index: usize,
    pub pos: Vec3,
    pub dir: Vec3,
    pub size: f32,
    pub per_rev: f32,
    pub wheel: Option<usize>,
    pub idle_hz: f32,
    pub per_m: f32,
}

/// Per-model machine rig (index-aligned with `VoxelModel::parts`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineRig {
    pub motions: Vec<Motion>,
    pub puffs: Vec<PuffEmitter>,
}

impl MachineRig {
    pub fn motion(&self, i: usize) -> Motion {
        self.motions.get(i).copied().unwrap_or_default()
    }
    /// Small linkage/cloth parts the sim may ignore for collision (rods, pistons, cleats, flags…).
    pub fn is_cosmetic(&self, i: usize) -> bool {
        matches!(self.motion(i), Motion::Crank { .. } | Motion::Scroll { .. } | Motion::Slide { .. } | Motion::Sway { .. })
    }
    /// Role-based fallback (a spec voxelised without its sidecar): `Wheel` parts spin.
    pub(crate) fn infer(parts: &[PartModel]) -> MachineRig {
        let motions = parts
            .iter()
            .map(|p| {
                if p.role != Some(PartRole::Wheel) || p.voxels.is_empty() {
                    return Motion::None;
                }
                let ext = (p.vmax - p.vmin).as_vec3() + Vec3::ONE;
                let (thin, long) = (ext.min_element(), ext.max_element());
                let axis = if ext.x == thin { Vec3::X } else if ext.y == thin { Vec3::Y } else { Vec3::Z };
                if thin > long * 0.75 {
                    // Not a disc (drill cone…): spin about the longest axis.
                    let ax = if ext.x == long { Vec3::X } else if ext.y == long { Vec3::Y } else { Vec3::Z };
                    return Motion::Spin { axis: ax, sign: 1.0, rate: 9.0, surf_k: 0.0, roll_k: 0.0, tick: false };
                }
                if axis == Vec3::X {
                    let r = radius_about(p, axis).max(0.05);
                    Motion::Spin { axis, sign: 1.0, rate: 0.0, surf_k: 0.0, roll_k: 1.0 / r, tick: false }
                } else {
                    Motion::Spin { axis, sign: 1.0, rate: if axis == Vec3::Y { 12.0 } else { 15.0 }, surf_k: 0.0, roll_k: 0.0, tick: false }
                }
            })
            .collect();
        MachineRig { motions, puffs: Vec::new() }
    }
}

/// Distance from the part pivot to its farthest voxel, perpendicular to `axis` (m).
fn radius_about(p: &PartModel, axis: Vec3) -> f32 {
    let mut r2: f32 = 0.0;
    for (v, _) in &p.voxels {
        let d = p.voxel_center(*v) - p.pivot;
        let perp = d - axis * d.dot(axis);
        r2 = r2.max(perp.length_squared());
    }
    r2.sqrt() + p.voxel_size * 0.5
}

fn unit(v: Vec3, fb: Vec3) -> Vec3 {
    if v.is_finite() { v.normalize_or(fb) } else { fb }
}

fn fin(x: f32) -> f32 {
    if x.is_finite() { x } else { 0.0 }
}

/// Resolve a build-time sidecar into `model.machine` (metres / part indices) and mark the linked
/// parts as machine-driven in the rig. Called by [`crate::build_model`]; call it yourself if you
/// voxelise the spec of [`crate::generate_machine`] separately.
pub fn attach_machine(model: &mut VoxelModel, spec: &MachineSpec) {
    let u = if model.unit_m.is_finite() && model.unit_m > 0.0 { model.unit_m } else { bd_core::CONSTRUCT_VOXEL_SIZE };
    let n = model.parts.len();
    if model.machine.motions.len() != n {
        model.machine = MachineRig::infer(&model.parts);
    }
    let idx = |id: PartId| model.parts.iter().position(|p| p.id == id);
    // Spins first (cranks read their wheel's axis).
    let mut resolved: Vec<(usize, Motion)> = Vec::new();
    for pass in 0..2 {
        for (id, ms) in &spec.links {
            let Some(i) = idx(*id) else { continue };
            if i == 0 {
                continue;
            }
            let is_spin = matches!(ms, MotionSpec::Spin { .. });
            if (pass == 0) != is_spin {
                continue;
            }
            let p = &model.parts[i];
            let m = match *ms {
                MotionSpec::Spin { axis, sign, rate, surf, roll, radius, tick } => {
                    let axis = unit(axis, Vec3::X);
                    let r = if radius > 0.0 && radius.is_finite() { radius * u } else { radius_about(p, axis) }.max(0.02);
                    Motion::Spin { axis, sign: fin(sign), rate: fin(rate), surf_k: fin(surf) / r, roll_k: fin(roll) / r, tick }
                }
                MotionSpec::Crank { wheel, member, pin, cross } => {
                    let Some(w) = idx(wheel) else { continue };
                    let wm = resolved.iter().find(|(k, _)| *k == w).map(|(_, m)| *m);
                    let Some(Motion::Spin { axis, .. }) = wm else { continue };
                    let center = model.parts[w].pivot;
                    let (pin, cross) = (pin * u, cross * u);
                    if !pin.is_finite() || !cross.is_finite() {
                        continue;
                    }
                    let off = cross - center;
                    let dir = unit(off - axis * off.dot(axis), Vec3::NEG_Z);
                    let len = (cross - pin).length();
                    if len < 1e-3 {
                        continue;
                    }
                    Motion::Crank { wheel: w, member, center, pin, cross, dir, len }
                }
                MotionSpec::Slide { axis, amp, hz, per_m, phase } => Motion::Slide { axis: unit(axis, Vec3::Y), amp: fin(amp) * u, hz: fin(hz), per_m: fin(per_m), phase: fin(phase) },
                MotionSpec::Scroll { dir, period } => Motion::Scroll { dir: unit(dir, Vec3::Z), period: (fin(period) * u).max(0.01) },
                MotionSpec::Sway { axis, amp, hz, phase, speed_amp, lean } => Motion::Sway { axis: unit(axis, Vec3::X), amp: fin(amp), hz: fin(hz), phase: fin(phase), speed_amp: fin(speed_amp), lean: fin(lean) },
                MotionSpec::Aim { pitch } => Motion::Aim { pitch },
                MotionSpec::Pulse { puff } => Motion::Pulse { puff },
            };
            resolved.push((i, m));
        }
    }
    for (i, m) in resolved {
        model.machine.motions[i] = m;
        if let Some(k) = model.rig.kinds.get_mut(i) {
            *k = PartKind::Machine;
        }
    }
    model.machine.puffs = spec
        .puffs
        .iter()
        .filter_map(|e| {
            let pi = idx(e.part)?;
            let pos = e.at * u;
            pos.is_finite().then_some(PuffEmitter {
                part_index: pi,
                pos,
                dir: unit(e.dir, Vec3::Y),
                size: (fin(e.size) * u).max(0.05),
                per_rev: fin(e.per_rev),
                wheel: e.wheel.and_then(idx),
                idle_hz: fin(e.idle_hz),
                per_m: fin(e.per_m),
            })
        })
        .take(16)
        .collect();
}

// --------------------------------------------------------------------------------- evaluation

/// Inputs available to feature rigs every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FeatureDrive {
    /// Seconds (monotonic, construct lifetime).
    pub time: f32,
    /// Ground speed m/s.
    pub speed: f32,
    /// Metres travelled (wheel/tread phase).
    pub distance: f32,
}

/// Clockwork time: holds, then snaps forward with a little overshoot, twice a second.
fn ticked(t: f32) -> f32 {
    let u = t * 2.0;
    let k = u.floor();
    let f = u - k;
    let s = crate::ease::Ease::OutBack.apply((f / 0.3).min(1.0));
    (k + s) * 0.5
}

fn spin_angle(m: &Motion, d: &FeatureDrive) -> Option<(Vec3, f32)> {
    if let Motion::Spin { axis, sign, rate, surf_k, roll_k, tick } = *m {
        let t = if tick { ticked(d.time) } else { d.time };
        let a = sign * (rate * t + surf_k * t - roll_k * d.distance);
        Some((axis, fin(a).rem_euclid(TAU)))
    } else {
        None
    }
}

/// Additive local transform (about the part pivot, parent space) for machine part `index`, or `None`
/// if the part is not machine-driven. Aim parts return identity here (see [`aim_local`]).
pub fn animate_features(model: &VoxelModel, index: usize, drive: &FeatureDrive) -> Option<Local> {
    let d = FeatureDrive { time: fin(drive.time).clamp(-1e6, 1e6), speed: fin(drive.speed).clamp(0.0, 500.0), distance: fin(drive.distance).clamp(-1e6, 1e6) };
    let rig = &model.machine;
    let m = rig.motions.get(index)?;
    let s = smoothstep(0.0, 10.0, d.speed);
    let out = match *m {
        Motion::None => return None,
        Motion::Aim { .. } => Local::IDENTITY,
        Motion::Spin { .. } => {
            let (axis, a) = spin_angle(m, &d)?;
            Local::rot(Quat::from_axis_angle(axis, a))
        }
        Motion::Crank { wheel, member, center, pin, cross, dir, len } => {
            let (axis, a) = spin_angle(rig.motions.get(wheel)?, &d)?;
            let p = center + Quat::from_axis_angle(axis, a) * (pin - center);
            match member {
                CrankMember::Coupling => Local { pos: p - pin, ..Local::IDENTITY },
                CrankMember::Piston | CrankMember::Rod => {
                    let q = cross - p;
                    let qd = q.dot(dir);
                    let s = -qd + (qd * qd - q.length_squared() + len * len).max(0.0).sqrt();
                    let x = cross + dir * s;
                    if member == CrankMember::Piston {
                        Local { pos: dir * s, ..Local::IDENTITY }
                    } else {
                        let u0 = (cross - pin).normalize_or(dir);
                        let u1 = (x - p).normalize_or(u0);
                        Local { rot: Quat::from_rotation_arc(u0, u1), pos: p - pin, scale: Vec3::ONE }
                    }
                }
            }
        }
        Motion::Slide { axis, amp, hz, per_m, phase } => {
            let ph = (hz * d.time + per_m * d.distance).rem_euclid(1.0);
            Local { pos: axis * (amp * (TAU * ph + phase).sin()), ..Local::IDENTITY }
        }
        Motion::Scroll { dir, period } => Local { pos: dir * d.distance.rem_euclid(period), ..Local::IDENTITY },
        Motion::Sway { axis, amp, hz, phase, speed_amp, lean } => {
            let ph = (hz * d.time).rem_euclid(1000.0);
            let a = (amp + speed_amp * s) * (TAU * ph + phase).sin() + lean * s;
            Local::rot(Quat::from_axis_angle(axis, a))
        }
        Motion::Pulse { puff } => {
            let p = rig.puffs.get(puff as usize).map_or(0.0, |e| {
                let ph = puff_phase(rig, e, &d);
                (-7.0 * (ph - ph.floor())).exp()
            });
            Local { scale: Vec3::new(1.0 + 0.1 * p, 1.0 + 0.24 * p, 1.0 + 0.1 * p), ..Local::IDENTITY }
        }
    };
    Some(out)
}

/// Aim layer: turret yaw / barrel pitch for machine parts (`None` for anything else).
pub fn aim_local(model: &VoxelModel, index: usize, yaw: f32, pitch: f32) -> Option<Local> {
    match model.machine.motion(index) {
        Motion::Aim { pitch: false } => Some(Local::rot(Quat::from_rotation_y(fin(yaw)))),
        Motion::Aim { pitch: true } => Some(Local::rot(Quat::from_rotation_x(fin(pitch).clamp(-0.35, 0.8)))),
        _ => None,
    }
}

fn puff_phase(rig: &MachineRig, e: &PuffEmitter, d: &FeatureDrive) -> f32 {
    let mut ph = e.idle_hz * d.time + e.per_m * d.distance.abs();
    if let Some(Motion::Spin { roll_k, .. }) = e.wheel.and_then(|w| rig.motions.get(w)) {
        ph += (roll_k * d.distance).abs() * e.per_rev / TAU;
    }
    fin(ph)
}

/// A smoke/steam puff for the client (model space, posed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puff {
    pub pos: Vec3,
    /// Unit direction to launch along.
    pub dir: Vec3,
    /// Suggested cube size (m).
    pub size: f32,
    /// 0.4..1: bigger/faster puffs when moving fast.
    pub strength: f32,
    /// Emitter index (stable per model).
    pub emitter: u8,
}

/// Puffs emitted between drive states `prev` and `now` (at most 4 per emitter per call), posed with
/// `pose` (from `anim::pose`). Call once per rendered frame with the previous frame's drive.
pub fn machine_puffs(model: &VoxelModel, pose: &[Affine3A], prev: &FeatureDrive, now: &FeatureDrive, out: &mut Vec<Puff>) {
    let rig = &model.machine;
    if now.time.partial_cmp(&prev.time).is_none_or(|o| o.is_lt()) {
        return;
    }
    for (k, e) in rig.puffs.iter().enumerate() {
        let a = puff_phase(rig, e, prev).floor();
        let b = puff_phase(rig, e, now).floor();
        let n = if a.is_finite() && b.is_finite() { (b - a).clamp(0.0, 4.0) as usize } else { 0 };
        if n == 0 {
            continue;
        }
        let tr = pose.get(e.part_index).copied().unwrap_or(Affine3A::IDENTITY);
        let pos = tr.transform_point3(e.pos);
        let dir = tr.transform_vector3(e.dir).normalize_or(Vec3::Y);
        let strength = 0.4 + 0.6 * smoothstep(0.0, 12.0, fin(now.speed));
        for _ in 0..n {
            out.push(Puff { pos, dir, size: e.size, strength, emitter: k as u8 });
        }
    }
}

// ----------------------------------------------------------------------------------- building

/// Place every feature in `features` that is not in `handled` on `spec` (generic placement relative
/// to the root part's bounds and the shape conventions: +Y up, −Z front). Motion links go to
/// `machine`. Called by `forms::generate` after the form generator and the generic mechanic attach.
pub(crate) fn attach_features(spec: &mut ShapeSpec, machine: &mut MachineSpec, features: &[Feature], form: Form, handled: &[Feature]) {
    let rest: Vec<Feature> = features.iter().copied().filter(|f| !handled.contains(f)).collect();
    if rest.is_empty() {
        return;
    }
    let mut b = B::from_spec(std::mem::take(spec));
    b.machine = std::mem::take(machine);
    place::place(&mut b, &rest, form);
    b.on(0);
    *spec = b.spec;
    *machine = b.machine;
}

/// Graft machine features onto any spec (e.g. "…with gears" cast onto an existing construct).
/// `form` only tunes placement (creatures get back-mounted boilers etc.).
pub fn attach_feature_parts(spec: &mut ShapeSpec, machine: &mut MachineSpec, features: &[Feature], form: Form) {
    attach_features(spec, machine, features, form, &[]);
}
