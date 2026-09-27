//! WEIGHT: a walking transformer sounds and feels like a machine of its size.
//!
//! * **Footfalls** from the pose's own gait (`bd_shapes::footfalls` on the biped `rig.robot` in robot
//!   mode, on the alt model when a beast alt walks), so each sound lands on the visible step: a
//!   metal foot (`robot_step_light` suits … `robot_step_mid` … `robot_footstep_heavy` + `kaiju_step`
//!   for giants) and, near the listener, a knee servo (`robot_servo_step`) pitched by size.
//!   Dust puffs at the foot; the ground shakes by size and distance (the local pilot always feels
//!   their own steps, lightly). Pilots, suits and followers alike (no new traffic).
//! * **Landing**: back on the ground after ≥ 0.25 s airborne — a metal thud (`robot_land`, the alt
//!   mode a suspension slam) scaled by the fall speed, dust ring, a kick for the local pilot. The
//!   sim's own transform-slam (`HeldFx::TransformSlam`) plays its impact on top.

use super::super::sound::Sfx3d;
use super::{TfFeel, size_class};
use crate::camera::PlayerCamera;
use crate::juice::{CameraShakeEvent, FovPunch};
use bd_fx::{BurstFx, BurstKind, ConstructEnd, ConstructVisual, TransformFx};
use bd_shapes::{AnimState, Footfall};
use bevy::prelude::*;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn steps(
    time: Res<Time<Real>>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut q: Query<(&mut TfFeel, &TransformFx, &ConstructVisual, &GlobalTransform), Without<ConstructEnd>>,
    mut sfx: MessageWriter<Sfx3d>,
    mut bursts: MessageWriter<BurstFx>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut fov: MessageWriter<FovPunch>,
    mut feet: Local<Vec<Footfall>>,
) {
    let dt = time.delta_secs().min(0.1);
    let eye = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    for (mut f, tfx, vis, gt) in &mut q {
        let (speed, dist, airborne) = match vis.layers.as_slice().first() {
            Some((AnimState::Locomotion { speed, distance, airborne }, _)) => (*speed, *distance, *airborne),
            _ => (0.0, f.dist, false),
        };
        let grounded = f.grounded && !airborne;
        let w = f.weight();
        let pitch = f.pitch();
        let near = eye.distance(gt.translation());
        f.step_gap = (f.step_gap - dt).max(0.0);
        // ---- landing
        if !grounded {
            f.air_t += dt;
            f.fall = f.fall.max(-f.vel.y);
        } else {
            if f.air_t > 0.25 && f.fall > 3.0 && f.age > 1.0 {
                let k = ((f.fall - 3.0) / 14.0).clamp(0.0, 1.0);
                let at = gt.translation();
                let (name, p) = if f.robot { ("robot_land", pitch) } else { ("land_heavy", pitch * 0.8) };
                sfx.write(Sfx3d::at(name, at).pitch(p.clamp(0.55, 1.5)).gain((0.5 + 0.6 * k) * (0.7 + 0.3 * w.min(2.0))));
                bursts.write(BurstFx { kind: BurstKind::Dust, pos: at, dir: Vec3::Y, scale: ((0.4 + 0.8 * k) * w).clamp(0.4, 3.5), color: None });
                if f.local {
                    shake.write(CameraShakeEvent { trauma: (0.08 + 0.22 * k) * (0.6 + 0.4 * w.min(2.0)), dir: Vec3::NEG_Y });
                    fov.write(FovPunch { degrees: -(1.0 + 2.0 * k) });
                } else if w >= 1.2 {
                    let d = (1.0 - near / (25.0 + 10.0 * w)).clamp(0.0, 1.0);
                    if d > 0.0 {
                        shake.write(CameraShakeEvent { trauma: (0.1 * w * k * d).min(0.35), dir: Vec3::NEG_Y });
                    }
                }
            }
            f.air_t = 0.0;
            f.fall = 0.0;
        }
        // ---- footfalls (standing robot, or a beast alt that walks)
        let model = if tfx.to_robot && tfx.t >= 0.85 {
            Some(&tfx.rig.robot)
        } else if tfx.alt_settled_animates() {
            Some(&*vis.model)
        } else {
            None
        };
        let prev = f.prev_dist.replace(dist);
        f.dist = dist;
        let (Some(model), Some(d0)) = (model, prev) else { continue };
        if !grounded || speed < 0.4 || (dist - d0).abs() > 5.0 || near > 110.0 + 40.0 * w {
            continue;
        }
        feet.clear();
        bd_shapes::footfalls(model, speed, d0, dist, &mut feet);
        let class = size_class(f.height);
        for foot in feet.iter() {
            // One footfall at a time (a render hitch can cross both feet in one frame).
            if f.step_gap > 0.0 {
                continue;
            }
            f.step_gap = (0.14 + 0.05 * w).min(0.4);
            let p = gt.transform_point(Vec3::from_array(foot.pos.to_array()));
            let pace = (speed / (2.0 + 1.5 * w)).clamp(0.5, 1.4);
            match class {
                0 => {
                    sfx.write(Sfx3d::at("robot_step_light", p).pitch((pitch * 0.9).clamp(0.9, 1.5)).gain(0.45 + 0.2 * pace));
                }
                1 => {
                    sfx.write(Sfx3d::at("robot_step_mid", p).pitch(pitch.clamp(0.8, 1.2)).gain(0.6 + 0.25 * pace));
                }
                _ => {
                    sfx.write(Sfx3d::at("robot_footstep_heavy", p).pitch((pitch * 1.1).clamp(0.55, 1.1)).gain((0.7 + 0.2 * w).min(1.3)));
                    if f.height >= 14.0 {
                        sfx.write(Sfx3d::at("kaiju_step", p).pitch((pitch * 1.2).clamp(0.6, 1.1)).gain((0.35 + 0.15 * w).min(1.1)));
                    }
                }
            }
            // The knee servo: only close by (a detail, not a wall of whine in a crowd).
            if near < 22.0 + 8.0 * w && f.robot {
                sfx.write(Sfx3d::at("robot_servo_step", p + Vec3::Y * f.height * 0.3).pitch((pitch * (0.95 + 0.1 * pace)).clamp(0.6, 1.6)).gain(0.3 + 0.1 * pace));
            }
            if w >= 0.6 {
                bursts.write(BurstFx { kind: BurstKind::Dust, pos: p, dir: Vec3::Y, scale: (0.3 * w * pace).clamp(0.25, 2.5), color: None });
            }
            if f.local {
                shake.write(CameraShakeEvent { trauma: (0.02 + 0.035 * w).min(0.14) * pace, dir: Vec3::NEG_Y });
            } else if w >= 1.0 {
                let k = (1.0 - near / (18.0 + 9.0 * w)).clamp(0.0, 1.0);
                if k > 0.0 {
                    shake.write(CameraShakeEvent { trauma: (0.05 * w * k * pace).min(0.3), dir: Vec3::NEG_Y });
                }
            }
        }
    }
}
