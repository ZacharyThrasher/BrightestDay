//! Transformation VFX (COMBAT.md §5d-bis), driven by bd_shapes `sequence_events`:
//! * hard-light **seam flash** along each moving panel's hinge while it unlatches/flips,
//! * **spark bursts** on every panel lock (in sync with the clicks) and a click spark on unlatch,
//! * a brief overall **glow pulse** at stand-up + an **eye-light flare** on the robot head,
//! * **combiners**: an energy lattice linking the vehicles during the ~2 s merge + a lock flash.
//!
//! Use: put [`TransformFx`] on a [`ConstructVisual`] entity whose `model` is `rig.alt`; bd_fx poses
//! it with `pose_transform` (instead of `ConstructVisual::layers`) and plays the effects. Everything
//! goes through the pooled particle ring / immediate region.

use std::sync::Arc;

use bd_shapes::{TRANSFORM_SECONDS, TransformEvent, TransformEventKind, TransformRig, sequence_events};
use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

use crate::arcs::draw_arc;
use crate::construct::{ConstructRuntime, ConstructVisual};
use crate::palette::Palette;
use crate::particles::{Emit, EmitSystems, FxParticles, PFlags, PKind};
use crate::util::{aff, fin, v3};

/// Transformation state of a construct (model = `rig.alt`).
#[derive(Component, Clone)]
pub struct TransformFx {
    pub rig: Arc<TransformRig>,
    /// Normalised progress 0..1 (see `bd_shapes::pose_transform`).
    pub t: f32,
    /// true: alt → robot, false: robot → alt.
    pub to_robot: bool,
    /// When true bd_fx advances `t` over `TRANSFORM_SECONDS` itself.
    pub auto: bool,
    /// Playback speed (1 = the full sequence; combat transforms mid-combo play faster).
    pub speed: f32,
    events: Arc<Vec<TransformEvent>>,
    last_t: f32,
    head_flare: f32,
    /// TRANSFORMER FEEL: `rig.robot` part index per `RobotRole::group()` (u8::MAX = none) — the
    /// robot's own clips (walk, attacks) ride on the panels (see [`pose_override`]).
    groups: [u8; 6],
    /// The alt model carries animatable roles (a beast's legs / jaw / tail, a car's wheels): settled
    /// in alt mode it animates like any construct (clip layers + role behaviours).
    alt_animates: bool,
}

impl TransformFx {
    pub fn new(rig: Arc<TransformRig>, to_robot: bool) -> Self {
        let events = Arc::new(sequence_events(&rig));
        let groups = robot_groups(&rig.robot);
        let alt_animates = rig.alt.parts.iter().any(|p| matches!(p.role, Some(r) if !matches!(r, bd_core::shape::PartRole::Other | bd_core::shape::PartRole::Support)));
        TransformFx { rig, t: 0.0, to_robot, auto: true, speed: 1.0, events, last_t: 0.0, head_flare: 0.0, groups, alt_animates }
    }
    /// The sequence's events (alt → robot order; reversed play mirrors `time`) — sounds / sparks.
    pub fn events(&self) -> &[TransformEvent] {
        &self.events
    }
    /// Settled in alt mode with an animatable alt model: pose it with the normal clip layers.
    pub fn alt_settled_animates(&self) -> bool {
        !self.to_robot && self.t >= 1.0 && self.alt_animates
    }
    /// Start a new transformation in `to_robot` direction from t = 0.
    pub fn restart(&mut self, to_robot: bool) {
        self.restart_at(to_robot, 1.0);
    }
    /// [`Self::restart`] at `speed` × (a combat transform: `HeldFx::Transform` strength = ± speed).
    pub fn restart_at(&mut self, to_robot: bool, speed: f32) {
        self.speed = if speed.is_finite() { speed.clamp(0.25, 8.0) } else { 1.0 };
        self.to_robot = to_robot;
        self.t = 0.0;
        self.last_t = 0.0;
    }
}

/// Combiner merge: an energy lattice between the source vehicles (their `GlobalTransform`s) while
/// `t` runs 0 → 1 (the game drives `t` alongside `bd_shapes::pose_combine`). A flash + shard ring
/// fires when `t` crosses 0.97.
#[derive(Component, Clone, Debug)]
pub struct CombineFx {
    pub sources: Vec<Entity>,
    pub t: f32,
    pub color: [f32; 3],
    last_t: f32,
}

impl CombineFx {
    pub fn new(sources: Vec<Entity>, color: [f32; 3]) -> Self {
        CombineFx { sources, t: 0.0, color, last_t: 0.0 }
    }
}

pub fn build(app: &mut App) {
    app.add_systems(PostUpdate, advance.in_set(crate::FxSet::Lifecycle))
        .add_systems(PostUpdate, (transform_fx, combine_fx).in_set(EmitSystems).before(VisibilitySystems::CheckVisibility));
}

fn advance(time: Res<Time>, mut q: Query<&mut TransformFx>) {
    let dt = time.delta_secs();
    for mut tf in &mut q {
        if tf.auto && tf.t < 1.0 {
            let speed = tf.speed.max(0.25);
            tf.t = (tf.t + dt * speed / TRANSFORM_SECONDS).min(1.0);
        }
    }
}

/// `rig.robot` part index per robot group (0 torso, 1 head, 2/3 arms, 4/5 legs; see
/// `bd_shapes::transform::assemble`, which skips empty limb groups).
fn robot_groups(robot: &bd_shapes::VoxelModel) -> [u8; 6] {
    use bd_core::shape::PartRole as R;
    let mut g = [u8::MAX; 6];
    for (i, p) in robot.parts.iter().enumerate().take(u8::MAX as usize) {
        let k = match p.role {
            None => 0,
            Some(R::Head) => 1,
            Some(R::ArmLeft) => 2,
            Some(R::ArmRight) => 3,
            Some(R::LegBackLeft) => 4,
            Some(R::LegBackRight) => 5,
            _ => continue,
        };
        if g[k] == u8::MAX {
            g[k] = i as u8;
        }
    }
    g
}

/// Most robot-model parts evaluated (torso, head, 2 arms, 2 legs).
const ROBOT_PARTS: usize = 8;

/// Posed override used by `construct::pose_constructs`: the panel sequence, plus (TRANSFORMER FEEL)
/// the robot's own motion — `layers` (the construct's locomotion / attack clips) evaluated on the
/// biped `rig.robot` and carried by every panel of that body group, so a robot-mode transformer
/// WALKS (legs stride, arms swing, attacks play) instead of gliding as a statue. Eased in over the
/// stand-up beat and out as it starts folding, so the sequence itself is untouched.
pub(crate) fn pose_override(tf: &TransformFx, layers: &[(bd_shapes::AnimState, f32)], out: &mut [glam::Affine3A]) {
    let t = fin(tf.t, 0.0).clamp(0.0, 1.0);
    bd_shapes::pose_transform(&tf.rig, t, tf.to_robot, out);
    let w = if tf.to_robot { smooth(0.8, 1.0, t) } else { 1.0 - smooth(0.0, 0.2, t) };
    let n = tf.rig.robot.parts.len();
    if w <= 1e-3 || layers.is_empty() || n == 0 || n > ROBOT_PARTS {
        return;
    }
    let mut grp = [glam::Affine3A::IDENTITY; ROBOT_PARTS];
    bd_shapes::pose_layered(&tf.rig.robot, layers, &mut grp[..n]);
    if w < 1.0 {
        for g in grp.iter_mut().take(n) {
            let (s, r, p) = g.to_scale_rotation_translation();
            *g = glam::Affine3A::from_scale_rotation_translation(glam::Vec3::ONE.lerp(s, w), glam::Quat::IDENTITY.slerp(r, w), p * w);
        }
    }
    for (o, p) in out.iter_mut().zip(tf.rig.panels.iter()).skip(1) {
        let gi = tf.groups[p.role.group()] as usize;
        if let Some(g) = grp.get(gi).filter(|_| gi < n) {
            let m = *g * *o;
            if m.is_finite() {
                *o = m;
            }
        }
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let u = ((x - a) / (b - a)).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

fn spark_burst(fx: &mut FxParticles, p: Vec3, color: [f32; 3], n: usize, speed: f32, size: f32) {
    for i in 0..n {
        let r = &mut fx.rng;
        let seed = r.next_u32();
        let vel = r.dir() * r.range(0.5, 1.0) * speed + Vec3::Y * 0.8;
        let life = r.range(0.12, 0.28);
        let c = if i % 3 == 0 { [1.0, 1.0, 1.0] } else { color };
        fx.emit(Emit { flags: PFlags::STRETCH | PFlags::PLAIN, pos: p, vel, size, color: c, emissive: 6.0, life, gravity: 1.0, drag: 3.0, spin: 4.0, seed, ..default() });
    }
}

fn transform_fx(time: Res<Time>, mut fx: ResMut<FxParticles>, mut q: Query<(&ConstructVisual, &mut TransformFx, &mut ConstructRuntime, &GlobalTransform)>) {
    for (v, mut tf, mut rt, gt) in &mut q {
        let root = gt.affine();
        let scale = root.matrix3.x_axis.length().max(1e-3);
        let pal = Palette::new(v.corps_color, v.accent);
        let hot = pal.slot(3);
        let (t0, t1) = (tf.last_t, tf.t);
        let tf = &mut *tf;
        // Events crossed this frame (sequence time is normalised alt → robot; reversed play mirrors).
        if t1 > t0 {
            for e in tf.events.iter() {
                let et = if tf.to_robot { e.time } else { 1.0 - e.time };
                if !(et > t0 && et <= t1) {
                    continue;
                }
                let local = rt.pose.get(e.panel).map(|p| p.transform_point3(e.pos)).unwrap_or(e.pos);
                let wp = root.transform_point3(v3(local));
                match e.kind {
                    TransformEventKind::Unlatch => spark_burst(&mut fx, wp, hot, 4, 3.0 * scale.sqrt(), 0.025 * scale.sqrt()),
                    TransformEventKind::Hinge => spark_burst(&mut fx, wp, pal.slot(1), 3, 2.0 * scale.sqrt(), 0.02 * scale.sqrt()),
                    TransformEventKind::Lock => {
                        spark_burst(&mut fx, wp, hot, 10, 5.0 * scale.sqrt(), 0.03 * scale.sqrt());
                        fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: wp, size: 0.25 * scale, color: [1.0, 1.0, 1.0], emissive: 6.0, life: 0.07, ..default() });
                    }
                    TransformEventKind::HeadUp => {
                        tf.head_flare = 1.0;
                        spark_burst(&mut fx, wp, hot, 12, 4.0 * scale.sqrt(), 0.03 * scale.sqrt());
                    }
                    TransformEventKind::StandUp => {
                        rt.auto_flash = rt.auto_flash.max(0.55);
                        let b = &v.model.bounds;
                        let c = root.transform_point3(v3((b.min + b.max) * 0.5));
                        fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: c, size: v3(b.max - b.min).length() * scale * 0.6, color: pal.slot(0), emissive: 2.0, life: 0.18, ..default() });
                    }
                }
            }
        }
        tf.last_t = t1;
        // Seam flash: glowing hinge line through each panel while it flips.
        if t1 < 1.0 {
            let t_seq = if tf.to_robot { t1 } else { 1.0 - t1 };
            for (i, p) in tf.rig.panels.iter().enumerate().skip(1) {
                if p.scale_mode != 0 || p.angle.abs() < 0.1 {
                    continue;
                }
                let u = (t_seq - p.unfold) / 0.3;
                if !(0.0..1.0).contains(&u) {
                    continue;
                }
                let k = (u * std::f32::consts::PI).sin();
                let Some(part) = tf.rig.alt.parts.get(i) else { continue };
                let aabb = part.aabb();
                let ext = v3(aabb.max - aabb.min);
                let axis = v3(p.axis).normalize_or_zero();
                let half = axis * (ext.abs().dot(axis.abs()) * 0.5 + 0.05);
                let c = v3(p.alt_center);
                let pose = rt.pose.get(i).map(aff).unwrap_or_default();
                let a = root.transform_point3(pose.transform_point3(c - half));
                let b = root.transform_point3(pose.transform_point3(c + half));
                fx.immediate_segment(a, b, 0.045 * scale * (0.5 + k), [hot[0] * 0.5 + 0.5, hot[1] * 0.5 + 0.5, hot[2] * 0.5 + 0.5], 4.0 + 5.0 * k, PFlags::PLAIN);
            }
        }
        // Eye-light flare on the head (two small hot boxes + a soft glow cube), decays over ~0.6 s.
        if tf.head_flare > 0.0 {
            tf.head_flare = (tf.head_flare - time.delta_secs() * 1.6).max(0.0);
            if let Some((hi, hp)) = tf.rig.panels.iter().enumerate().find(|(_, p)| p.role == bd_shapes::RobotRole::Head) {
                let pose = rt.pose.get(hi).map(aff).unwrap_or_default();
                let (_, rot, _) = (root * pose).to_scale_rotation_translation();
                let head = root.transform_point3(pose.transform_point3(v3(hp.alt_center)));
                let fwd = rot * Vec3::NEG_Z;
                let side = rot * Vec3::X;
                let f = tf.head_flare;
                let es = 0.06 * scale;
                for s in [-1.0f32, 1.0] {
                    let p = head + fwd * 0.12 * scale + side * s * 0.07 * scale + Vec3::Y * 0.03 * scale;
                    fx.immediate_box(p, rot, Vec3::new(es * 1.4, es * 0.6, es * 0.3) * (1.0 + f), [1.0, 1.0, 1.0], 4.0 + 10.0 * f, PFlags::PLAIN);
                }
            }
        }
    }
}

fn combine_fx(time: Res<Time>, mut fx: ResMut<FxParticles>, mut q: Query<&mut CombineFx>, gts: Query<&GlobalTransform>, mut pts: Local<Vec<Vec3>>) {
    let phase = (time.elapsed_secs_wrapped() * 24.0) as u32;
    for mut c in &mut q {
        let t = fin(c.t, 0.0).clamp(0.0, 1.0);
        pts.clear();
        pts.extend(c.sources.iter().filter_map(|e| gts.get(*e).ok()).map(|g| g.translation()));
        if pts.len() < 2 {
            continue;
        }
        let centre = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        // Lattice intensity: ramps in, holds, collapses into the lock.
        let k = (t / 0.15).min(1.0) * (1.0 - ((t - 0.85) / 0.15).clamp(0.0, 1.0));
        if k > 0.01 && t < 1.0 {
            for (i, a) in pts.iter().copied().enumerate() {
                draw_arc(&mut fx, a, centre + Vec3::Y * 0.5, c.color, 0.8 + 0.8 * k, 101 + i as u32, phase);
                let b = pts[(i + 1) % pts.len()];
                draw_arc(&mut fx, a, b, c.color, 0.5 + 0.6 * k, 211 + i as u32, phase);
            }
            // Pulsing lattice nodes.
            for a in &pts {
                fx.immediate_box(*a + Vec3::Y * 0.3, Quat::from_rotation_y(t * 9.0), Vec3::splat(0.18 + 0.12 * k), c.color, 3.0 + 4.0 * k, PFlags::PLAIN);
            }
        }
        if c.last_t < 0.97 && t >= 0.97 {
            fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: centre + Vec3::Y, size: 3.0, color: [1.0, 1.0, 1.0], emissive: 8.0, life: 0.2, ..default() });
            for i in 0..48 {
                let a = i as f32 / 48.0 * std::f32::consts::TAU;
                let r = &mut fx.rng;
                let seed = r.next_u32();
                let vel = Vec3::new(a.cos(), r.range(-0.1, 0.4), a.sin()) * r.range(8.0, 14.0);
                fx.emit(Emit { flags: PFlags::STRETCH | PFlags::WHITE_BIRTH, pos: centre + Vec3::Y, vel, size: 0.12, color: c.color, emissive: 3.5, life: 0.5, gravity: 0.4, drag: 2.5, spin: 3.0, seed, ..default() });
            }
        }
        c.last_t = t;
    }
}
