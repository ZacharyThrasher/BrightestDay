//! Follow-through for secondary parts (capes, cloth, tails, hair, plumes, antennae, ears, chains):
//! a tiny damped spring per part driven by the root's world acceleration, so they overshoot and snap
//! back after sudden starts, stops, turns, landings and hits.
//!
//! The client keeps one [`SecondaryMotion`] per posed model: call [`SecondaryMotion::update`] once per
//! frame with the root's world transform and the frame time, then [`SecondaryMotion::apply`] on the
//! output of `anim::pose`. Deterministic (fixed 1/120 s substeps), allocation-free after `new`,
//! cosmetic only (the sim never needs it).

use glam::{Affine3A, Quat, Vec2, Vec3};

use crate::anim::PartKind;
use crate::voxelize::VoxelModel;

const STEP: f32 = 1.0 / 120.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SecondaryMotion {
    /// (part index, response gain, spring frequency Hz).
    parts: Vec<(usize, f32, f32)>,
    /// Per part: (pitch, roll) offset (rad) and velocity.
    ang: Vec<Vec2>,
    vel: Vec<Vec2>,
    prev_pos: Vec3,
    prev_vel: Vec3,
    acc: Vec3,
    frames: u8,
    carry: f32,
}

impl SecondaryMotion {
    /// Collect the model's secondary parts (tails, cloth/chain chains, swaying bits, wings folded
    /// on the back are left to the wing overlay).
    pub fn new(model: &VoxelModel) -> Self {
        let mut parts = Vec::new();
        for (i, k) in model.rig.kinds.iter().enumerate() {
            let cloth = model.parts[i].role == Some(bd_core::shape::PartRole::Cloth);
            let (gain, hz) = match *k {
                _ if cloth && !matches!(*k, PartKind::Chain { .. } | PartKind::Machine) => (0.06, 2.4),
                PartKind::Tail { k, .. } => (0.05 * (1.0 + 0.35 * k as f32), 2.4),
                PartKind::Chain { k, .. } => (0.07 * (1.0 + 0.4 * k as f32), 1.8),
                PartKind::Sway => (0.045, 2.6),
                PartKind::Machine if model.machine.is_cosmetic(i) && matches!(model.machine.motion(i), crate::features::Motion::Sway { .. }) => (0.05, 2.2),
                _ => continue,
            };
            // Size-aware: big creatures swing slower.
            let hz = hz / (model.rig.height / 2.0).clamp(0.5, 20.0).sqrt();
            parts.push((i, gain, hz));
        }
        let n = parts.len();
        SecondaryMotion { parts, ang: vec![Vec2::ZERO; n], vel: vec![Vec2::ZERO; n], ..Default::default() }
    }

    /// Advance with the root's world transform this frame (`dt` seconds since the last call).
    pub fn update(&mut self, root_world: Affine3A, dt: f32) {
        if !root_world.is_finite() {
            return;
        }
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        let pos = Vec3::from(root_world.translation);
        if self.frames == 0 || dt <= 0.0 {
            self.prev_pos = pos;
            self.prev_vel = Vec3::ZERO;
            self.frames = self.frames.saturating_add(1);
            return;
        }
        let v = (pos - self.prev_pos) / dt;
        let a = if self.frames >= 2 { (v - self.prev_vel) / dt } else { Vec3::ZERO };
        let dv_world = if self.frames >= 2 { v - self.prev_vel } else { Vec3::ZERO };
        self.prev_pos = pos;
        self.prev_vel = v;
        self.frames = self.frames.saturating_add(1);
        // Acceleration in the model's own frame (clamped: teleports/respawns must not explode).
        let inv = root_world.matrix3.inverse();
        let al = Vec3::from(inv * glam::Vec3A::from(a)).clamp(Vec3::splat(-120.0), Vec3::splat(120.0));
        self.acc = if al.is_finite() { al } else { Vec3::ZERO };
        // Sudden velocity changes kick the springs directly (snappy overshoot on stops/starts/hits).
        if self.frames > 2 {
            let dv = Vec3::from(inv * glam::Vec3A::from(dv_world)).clamp(Vec3::splat(-20.0), Vec3::splat(20.0));
            let dv = if dv.is_finite() { dv } else { Vec3::ZERO };
            for (k, &(_, gain, _)) in self.parts.iter().enumerate() {
                self.vel[k] += Vec2::new(dv.z - 0.3 * dv.y.abs(), -dv.x) * (gain * 14.0);
            }
        }
        self.carry += dt;
        let mut n = 0;
        while self.carry >= STEP && n < 16 {
            self.carry -= STEP;
            n += 1;
            for (k, &(_, gain, hz)) in self.parts.iter().enumerate() {
                let w = std::f32::consts::TAU * hz;
                let zeta = 0.32;
                // Target lean: forward acceleration swings the part back, sideways the other way,
                // upward (landing/jump) droops/lifts it.
                let target = (Vec2::new(self.acc.z - 0.3 * self.acc.y.abs(), -self.acc.x) * (gain * 0.15)).clamp(Vec2::splat(-0.6), Vec2::splat(0.6));
                let x = self.ang[k];
                let acc = (target - x) * (w * w) - self.vel[k] * (2.0 * zeta * w);
                self.vel[k] += acc * STEP;
                self.ang[k] = (self.ang[k] + self.vel[k] * STEP).clamp(Vec2::splat(-1.4), Vec2::splat(1.4));
                if !self.ang[k].is_finite() || !self.vel[k].is_finite() {
                    self.ang[k] = Vec2::ZERO;
                    self.vel[k] = Vec2::ZERO;
                }
            }
        }
    }

    /// Add the follow-through rotation to each secondary part of a posed model (`out` from `pose`).
    pub fn apply(&self, model: &VoxelModel, out: &mut [Affine3A]) {
        for (k, &(i, _, _)) in self.parts.iter().enumerate() {
            let (Some(o), Some(p)) = (out.get(i).copied(), model.parts.get(i)) else { continue };
            let a = self.ang[k];
            let q = Quat::from_rotation_x(a.x) * Quat::from_rotation_z(a.y);
            let local = Affine3A::from_translation(p.pivot) * Affine3A::from_quat(q) * Affine3A::from_translation(-p.pivot);
            let r = o * local;
            if r.is_finite() {
                out[i] = r;
            }
        }
    }

    /// Current offsets (pitch, roll) per tracked part — for debugging/tests.
    pub fn offsets(&self) -> &[Vec2] {
        &self.ang
    }
}
