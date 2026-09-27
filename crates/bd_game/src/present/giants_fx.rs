//! **Giants you fight** — presentation of `bd_sim::constructs::giant` (GIANTS worker). Everything
//! here reads the replica's giant state (`c.ai.giant`: anatomy from `GiantEvent::Born`, severed
//! limbs) and `GameEvent::Giant` moments; nothing feeds back into the sim.
//!
//! * **Weak points**: a hard-light core + halo in the owner's corps colour at each weak point (nape,
//!   core, eye), gently pulsing, readable at range; brighter and larger while the giant lies toppled
//!   (exposed) and in a burst when struck.
//! * **Warnings**: a growing glow on the limb about to strike (arm / foot / tail / head), a deep
//!   telegraph cue (+ a roar for stomps and tail sweeps) and, for stomps, the shockwave's reach drawn
//!   on the ground before it lands.
//! * **Impacts with gravitas**: stomp shockwave rings that sweep outward at the sim's speed with dust
//!   bursts along the front, lingering dust, a sub-bass boom with a short world duck, a small
//!   hit-stop and distance-scaled camera shake near the camera; the topple's ground slam likewise.
//!   All camera / flash channels go through the comfort-gated juice messages.
//! * **Severed limbs** vanish from the body (their part entities hide, late joins too) and burst
//!   into voxel debris (`ShatterModelFx` of just that limb).
//! * **Command / hijack**: a ping over the enemy the summoner pointed at, a danger cue for that enemy,
//!   a filling glow at the nape while someone hijacks it.

use super::sound::{MixDuck, Sfx3d};
use super::{bv, corps_rgb, PresentSet};
use crate::camera::PlayerCamera;
use crate::juice::{CameraShakeEvent, HitStop};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use bd_core::ConstructId;
use bd_fx::{BurstFx, BurstKind, ConstructParts, ConstructVisual, ShatterModelFx, SmokeFx};
use bd_shapes::VoxelModel;
use bd_sim::constructs::ai::AttackKind;
use bd_sim::constructs::giant::{GiantEvent, Limb, Spot};
use bd_sim::GameEvent;
use bevy::light::NotShadowCaster;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::sync::Arc;

pub struct GiantsFxPlugin;

impl Plugin for GiantsFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GiantFx>()
            .add_message::<BurstFx>()
            .add_message::<ShatterModelFx>()
            .add_systems(Update, giant_events.in_set(PresentSet::Events))
            .add_systems(Update, (weak_markers, hide_severed, animate_fx).in_set(PresentSet::Sync));
        scenario::build(app);
    }
}

/// Per-giant presentation state + shared meshes.
#[derive(Resource, Default)]
struct GiantFx {
    markers: HashMap<ConstructId, Markers>,
    sphere: Option<Handle<Mesh>>,
    ring: Option<Handle<Mesh>>,
    /// Severed limbs to burst on the next frames (the part entities may not exist yet).
    sever: Vec<(ConstructId, Limb, f32)>,
}

struct Markers {
    /// (core, halo) per weak point.
    ents: Vec<(Entity, Entity)>,
    mat: Handle<StandardMaterial>,
    halo: Handle<StandardMaterial>,
    /// Seconds of "struck" flash left, toppled (exposed), hijack progress (0 = none).
    flash: f32,
    exposed: bool,
    hijack: f32,
}

/// Growing glow on the limb about to strike (follows the giant).
#[derive(Component)]
struct WarnGlow {
    id: ConstructId,
    limb: Limb,
    t: f32,
    secs: f32,
}

/// Stomp shockwave ring (sweeps out at the sim's speed) or its reach preview (`preview`).
#[derive(Component)]
struct ShockRing {
    origin: Vec3,
    r0: f32,
    r_max: f32,
    speed: f32,
    t: f32,
    preview: Option<f32>,
    dust_t: f32,
}

/// Something that despawns after `left` seconds (lingering dust, pings).
#[derive(Component)]
struct Fade {
    left: f32,
}

/// Ping over a commanded target.
#[derive(Component)]
struct FocusPing {
    target: bd_core::PlayerId,
    t: f32,
}

fn glow(color: [f32; 3], k: f32, add: bool) -> StandardMaterial {
    let c = LinearRgba::rgb(color[0] * k, color[1] * k, color[2] * k);
    StandardMaterial {
        base_color: Color::LinearRgba(c),
        emissive: c * 2.0,
        unlit: true,
        alpha_mode: if add { AlphaMode::Add } else { AlphaMode::Opaque },
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

/// A flat unit annulus (inner radius 0.9) in the XZ plane.
fn ring_mesh() -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let n = 64;
    let mut pos = Vec::with_capacity(n * 2);
    let mut idx = Vec::with_capacity(n * 6);
    for k in 0..n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        pos.push([c * 0.9, 0.0, s * 0.9]);
        pos.push([c, 0.0, s]);
        let (i0, i1, j0, j1) = (2 * k as u32, 2 * k as u32 + 1, 2 * ((k + 1) % n) as u32, 2 * ((k + 1) % n) as u32 + 1);
        idx.extend_from_slice(&[i0, j0, i1, i1, j0, j1]);
    }
    let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
    let uvs = vec![[0.0, 0.0]; pos.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(idx))
}

/// Which limb a model part belongs to (its role, else its parent's), as the sim's `limb_of_part`.
pub fn limb_of_model_part(m: &VoxelModel, i: usize) -> Limb {
    let mut k = i;
    for _ in 0..32 {
        let Some(p) = m.parts.get(k) else { return Limb::Torso };
        if let Some(l) = p.role.and_then(Limb::of_role) {
            return l;
        }
        if k == 0 || p.parent_index == k {
            return Limb::Torso;
        }
        k = p.parent_index;
    }
    Limb::Torso
}

/// Render transform of a construct (interpolated view).
fn view_tf(views: &WorldViews, id: ConstructId) -> Option<(Vec3, Quat, f32)> {
    views.constructs.iter().find(|v| v.id == id).map(|v| (v.pos, v.rot, v.size))
}

#[allow(clippy::too_many_arguments)]
fn giant_events(
    mut events: MessageReader<GameEvents>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    mut fx: ResMut<GiantFx>,
    mut sfx: MessageWriter<Sfx3d>,
    mut burst: MessageWriter<BurstFx>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut stop: MessageWriter<HitStop>,
    mut duck: ResMut<MixDuck>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Some(active) = active else {
        events.clear();
        return;
    };
    let eye = cam.single().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    let sphere = fx.sphere.get_or_insert_with(|| meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(12, 8)))).clone();
    let ring = fx.ring.get_or_insert_with(|| meshes.add(ring_mesh())).clone();
    // Distance falloff for camera channels (1 at the camera … 0 at `r`).
    let near = |p: Vec3, r: f32| (1.0 - eye.distance(p) / r).clamp(0.0, 1.0);
    let corps_of = |id: ConstructId| active.world.constructs.get(id).map_or([1.0, 1.0, 1.0], |c| corps_rgb(active.corps_of(c.owner)));
    for ev in events.read() {
        // TITANS: the pilot's dash thud, the landing crunch, a piloted stomp's gravitas.
        match &ev.event {
            GameEvent::Move { player, flags, .. } => {
                use bd_sim::events::move_flags;
                let seated = active.world.player(*player).and_then(|p| p.seat).filter(|s| !s.suit).and_then(|s| active.world.constructs.get(s.construct)).filter(|c| bd_sim::constructs::titan::walks(c));
                if let Some(c) = seated {
                    let p = bv(c.pos);
                    let local = active.local == Some(*player);
                    if flags & move_flags::DODGE != 0 {
                        sfx.write(Sfx3d::at("boost_burst", p).pitch(0.55).gain(1.1));
                        sfx.write(Sfx3d::at("land_heavy", p).pitch(0.6));
                        burst.write(BurstFx { kind: BurstKind::Dust, pos: p + Vec3::Y * 0.3, dir: Vec3::Y, scale: 1.6, color: None });
                        if local {
                            shake.write(CameraShakeEvent { trauma: 0.12, dir: Vec3::ZERO });
                        }
                    }
                    if flags & move_flags::LAND_HARD != 0 {
                        sfx.write(Sfx3d::at("land_heavy", p).pitch(0.5).gain(1.3));
                        sfx.write(Sfx3d::at("kaiju_step", p).gain(1.2));
                        burst.write(BurstFx { kind: BurstKind::Dust, pos: p + Vec3::Y * 0.3, dir: Vec3::Y, scale: 2.4, color: None });
                        let k = if local { 1.0 } else { near(p, 50.0) };
                        if k > 0.0 {
                            shake.write(CameraShakeEvent { trauma: 0.3 * k, dir: Vec3::NEG_Y });
                        }
                    }
                }
                continue;
            }
            GameEvent::HeldFx { id, fx: bd_sim::events::HeldFx::PilotAttack, pos, dir, strength, .. } if dir.y < -0.9 && *strength >= 1.5 => {
                // A piloted stomp: dust ring, boom, shake with distance.
                let p = bv(*pos);
                let col = corps_of(*id);
                burst.write(BurstFx { kind: BurstKind::Dust, pos: p + Vec3::Y * 0.3, dir: Vec3::Y, scale: 2.8, color: None });
                burst.write(BurstFx { kind: BurstKind::Shards, pos: p + Vec3::Y * 0.3, dir: Vec3::Y, scale: 1.4, color: Some(col) });
                commands.spawn((Transform::from_translation(p + Vec3::Y * 0.5), dust(22.0, 1.0, 0.9), Fade { left: 2.2 }));
                sfx.write(Sfx3d::at("kaiju_stomp", p).gain(1.1));
                sfx.write(Sfx3d::at("amb_distant_boom", p).pitch(0.55).gain(1.1));
                let k = near(p, 60.0);
                if k > 0.0 {
                    shake.write(CameraShakeEvent { trauma: 0.3 * k, dir: Vec3::NEG_Y });
                    duck.duck(0.3 * k);
                }
                continue;
            }
            _ => {}
        }
        let GameEvent::Giant(g) = &ev.event else { continue };
        match g {
            GiantEvent::Born { .. } => {}
            GiantEvent::Warn { id, kind, secs, .. } => {
                let kind = AttackKind::from_u8(*kind);
                let Some(c) = active.world.constructs.get(*id) else { continue };
                let limb = match kind {
                    AttackKind::Swipe => {
                        if c.ai.giant.swipe_side > 0.0 {
                            Limb::ArmR
                        } else {
                            Limb::ArmL
                        }
                    }
                    AttackKind::Stomp => {
                        if c.ai.giant.body.has(Limb::LegBR) {
                            Limb::LegBR
                        } else {
                            Limb::LegFR
                        }
                    }
                    AttackKind::TailSweep => Limb::Tail,
                    _ => Limb::Head,
                };
                let col = corps_of(*id);
                let mat = materials.add(glow(col, 3.0, true));
                let Some((p, r, s)) = view_tf(&views, *id) else { continue };
                let at = limb_center(c, limb).map_or(p, |l| p + r * (bv(l) * s));
                commands.spawn((
                    WarnGlow { id: *id, limb, t: 0.0, secs: secs.max(0.05) },
                    Mesh3d(sphere.clone()),
                    MeshMaterial3d(mat),
                    Transform::from_translation(at).with_scale(Vec3::splat(0.01)),
                    NotShadowCaster,
                ));
                let big = matches!(kind, AttackKind::Stomp | AttackKind::TailSweep);
                sfx.write(Sfx3d::at("strike_telegraph", at).pitch(if big { 0.55 } else { 0.75 }).gain(0.9));
                if big {
                    sfx.write(Sfx3d::at("kaiju_roar", p + Vec3::Y * c.half.y).pitch(0.85).gain(0.9));
                }
                if kind == AttackKind::Stomp {
                    // The shockwave's reach, drawn on the ground before it lands.
                    let reach = (c.half.y * 2.0 * active.world.cfg.tuning.giants.shock_reach).clamp(6.0, 30.0);
                    let rm = materials.add(glow(col, 0.9, true));
                    commands.spawn((
                        ShockRing { origin: p + Vec3::Y * 0.08, r0: reach, r_max: reach, speed: 0.0, t: 0.0, preview: Some(secs.max(0.1) + 0.45), dust_t: 0.0 },
                        Mesh3d(ring.clone()),
                        MeshMaterial3d(rm),
                        Transform::from_translation(p + Vec3::Y * 0.08).with_scale(Vec3::splat(reach)),
                        NotShadowCaster,
                    ));
                }
            }
            GiantEvent::Shock { id, origin, radius, speed } => {
                let o = bv(*origin);
                let col = corps_of(*id);
                let mat = materials.add(glow(col, 2.2, true));
                commands.spawn((
                    ShockRing { origin: o + Vec3::Y * 0.12, r0: 1.5, r_max: *radius, speed: *speed, t: 0.0, preview: None, dust_t: 0.0 },
                    Mesh3d(ring.clone()),
                    MeshMaterial3d(mat),
                    Transform::from_translation(o + Vec3::Y * 0.12).with_scale(Vec3::splat(1.5)),
                    NotShadowCaster,
                ));
                // Lingering dust at the foot.
                commands.spawn((Transform::from_translation(o + Vec3::Y * 0.5), dust(26.0, 1.1, 0.9), Fade { left: 2.6 }));
                burst.write(BurstFx { kind: BurstKind::Dust, pos: o + Vec3::Y * 0.3, dir: Vec3::Y, scale: 3.0, color: None });
                burst.write(BurstFx { kind: BurstKind::Shards, pos: o + Vec3::Y * 0.3, dir: Vec3::Y, scale: 1.6, color: Some(col) });
                // Sub-bass boom under the stomp (constructs.rs already plays the stomp itself) + duck.
                sfx.write(Sfx3d::at("amb_distant_boom", o).pitch(0.55).gain(1.3));
                let k = near(o, 70.0);
                if k > 0.0 {
                    shake.write(CameraShakeEvent { trauma: 0.35 * k, dir: Vec3::NEG_Y });
                    duck.duck(0.35 * k);
                }
                if near(o, 22.0) > 0.0 {
                    stop.write(HitStop { secs: 0.05 });
                }
            }
            GiantEvent::WeakHit { id, spot, point, stagger, .. } => {
                let p = bv(*point);
                let col = corps_of(*id);
                burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::Y, scale: if *stagger { 2.6 } else { 1.6 }, color: Some(col) });
                if comfort.flashes > 0.0 {
                    burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::ZERO, scale: if *stagger { 2.0 } else { 1.2 }, color: Some([1.0, 1.0, 1.0]) });
                }
                sfx.write(Sfx3d::at("hit_crit", p).gain(1.1));
                sfx.write(Sfx3d::at("break_hardlight", p).pitch(0.8).gain(0.8));
                if let Some(m) = fx.markers.get_mut(id) {
                    m.flash = if *stagger { 0.9 } else { 0.45 };
                }
                if *stagger {
                    // A pained roar, higher.
                    sfx.write(Sfx3d::at("kaiju_roar", p).pitch(1.25).gain(0.8));
                    let k = near(p, 40.0);
                    if k > 0.0 {
                        shake.write(CameraShakeEvent { trauma: 0.18 * k, dir: Vec3::ZERO });
                        stop.write(HitStop { secs: 0.07 });
                    }
                }
                let _ = spot;
            }
            GiantEvent::Limb { id, limb, severed, point, .. } => {
                let p = bv(*point);
                let col = corps_of(*id);
                if *severed {
                    fx.sever.push((*id, Limb::from_u8(*limb), 1.0));
                    sfx.write(Sfx3d::at("construct_shatter", p).pitch(0.7).gain(1.2));
                    sfx.write(Sfx3d::at("break_hardlight", p).pitch(0.6));
                    sfx.write(Sfx3d::at("kaiju_roar", p).pitch(1.1).gain(0.9));
                    burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::Y, scale: 3.0, color: Some(col) });
                    let k = near(p, 50.0);
                    if k > 0.0 {
                        shake.write(CameraShakeEvent { trauma: 0.25 * k, dir: Vec3::ZERO });
                        stop.write(HitStop { secs: 0.08 });
                    }
                } else {
                    // A limb at half: a crack of light.
                    burst.write(BurstFx { kind: BurstKind::Shards, pos: p, dir: Vec3::Y, scale: 1.2, color: Some(col) });
                    sfx.write(Sfx3d::at("break_hardlight", p).gain(0.7));
                }
            }
            GiantEvent::Topple { id, phase, .. } => {
                let Some((p, r, s)) = view_tf(&views, *id) else { continue };
                let c = active.world.constructs.get(*id);
                let h = c.map_or(10.0, |c| c.half.y * 2.0) * s;
                if let Some(m) = fx.markers.get_mut(id) {
                    m.exposed = *phase < 3;
                }
                match phase {
                    0 => {
                        sfx.write(Sfx3d::at("kaiju_roar", p + Vec3::Y * h * 0.8).pitch(0.7).gain(1.1));
                    }
                    1 => {
                        // Ground slam along the body.
                        let side = r * Vec3::X;
                        for k in 0..4 {
                            let at = p + side * (k as f32 / 3.0 * h * 0.7);
                            burst.write(BurstFx { kind: BurstKind::Dust, pos: at + Vec3::Y * 0.4, dir: Vec3::Y, scale: 2.4, color: None });
                        }
                        commands.spawn((Transform::from_translation(p + side * h * 0.35 + Vec3::Y * 0.6), dust(30.0, 1.3, 0.7), Fade { left: 3.0 }));
                        sfx.write(Sfx3d::at("collapse_rumble", p).gain(1.3));
                        sfx.write(Sfx3d::at("explosion_big", p).pitch(0.45).gain(1.1));
                        let k = near(p, 80.0);
                        if k > 0.0 {
                            shake.write(CameraShakeEvent { trauma: 0.5 * k, dir: Vec3::NEG_Y });
                            duck.duck(0.5 * k);
                        }
                        if near(p, 25.0) > 0.0 {
                            stop.write(HitStop { secs: 0.08 });
                        }
                    }
                    3 => {
                        sfx.write(Sfx3d::at("kaiju_step", p).gain(1.1));
                    }
                    _ => {}
                }
            }
            GiantEvent::Focus { owner, target, .. } => {
                if active.local == Some(*owner) {
                    sfx.write(Sfx3d::ui("summon_ping").gain(0.8));
                    let col = corps_rgb(active.corps_of(*owner));
                    let mat = materials.add(glow(col, 3.0, true));
                    commands.spawn((FocusPing { target: *target, t: 0.0 }, Mesh3d(sphere.clone()), MeshMaterial3d(mat), Transform::from_scale(Vec3::splat(0.01)), NotShadowCaster));
                } else if active.local == Some(*target) {
                    sfx.write(Sfx3d::ui("alert_danger").gain(0.8));
                }
            }
            GiantEvent::Felled { id, .. } => {
                if let Some((p, _, _)) = view_tf(&views, *id) {
                    sfx.write(Sfx3d::at("final_blow_suck", p).gain(1.0));
                }
            }
            GiantEvent::Hijack { id, progress, done, .. } => {
                if let Some(m) = fx.markers.get_mut(id) {
                    m.hijack = if *done { 0.0 } else { progress.max(0.05) };
                }
                if let Some((p, _, _)) = view_tf(&views, *id) {
                    if *done {
                        sfx.write(Sfx3d::at("transform_combine", p).gain(1.1));
                        burst.write(BurstFx { kind: BurstKind::Flash, pos: p + Vec3::Y * 4.0, dir: Vec3::ZERO, scale: 3.0, color: None });
                    } else {
                        sfx.write(Sfx3d::at("charge_loop", p).pitch(0.8 + progress * 0.6).gain(0.5));
                    }
                }
            }
        }
    }
}

/// Giant-local centre of limb `l` (size 1).
fn limb_center(c: &bd_sim::constructs::Construct, l: Limb) -> Option<bd_core::Vec3> {
    c.ai.giant.body.limb_box(l).map(|(center, _)| center)
}

/// Weak-point cores + halos on every giant, following the interpolated body.
fn weak_markers(
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut fx: ResMut<GiantFx>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tfs: Query<&mut Transform>,
    mut commands: Commands,
) {
    let Some(active) = active else { return };
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();
    let sphere = fx.sphere.get_or_insert_with(|| meshes.add(Sphere::new(1.0).mesh().uv(12, 8))).clone();
    let mut live: Vec<ConstructId> = Vec::new();
    for v in &views.constructs {
        let Some(c) = active.world.constructs.get(v.id) else { continue };
        if !c.ai.giant.on || v.flags & bd_sim::replicate::cflags::FORMING != 0 {
            continue;
        }
        live.push(v.id);
        let wps = c.ai.giant.body.weak_points();
        let m = fx.markers.entry(v.id).or_insert_with(|| {
            let col = corps_rgb(active.corps_of(c.owner));
            let mat = materials.add(glow(col, 4.0, false));
            let halo = materials.add(glow(col, 0.6, true));
            let ents = wps
                .iter()
                .map(|_| {
                    let a = commands.spawn((Mesh3d(sphere.clone()), MeshMaterial3d(mat.clone()), Transform::default(), NotShadowCaster)).id();
                    let b = commands.spawn((Mesh3d(sphere.clone()), MeshMaterial3d(halo.clone()), Transform::default(), NotShadowCaster)).id();
                    (a, b)
                })
                .collect();
            Markers { ents, mat, halo, flash: 0.0, exposed: false, hijack: 0.0 }
        });
        let _ = (&m.mat, &m.halo);
        m.flash = (m.flash - dt).max(0.0);
        let (p, r, s) = (v.pos, v.rot, v.size);
        for (k, w) in wps.iter().enumerate() {
            let Some(&(core, halo)) = m.ents.get(k) else { continue };
            let at = p + r * (bv(w.pos) * s);
            let base = w.radius * s * if w.spot == Spot::Nape { 0.42 } else { 0.3 };
            let pulse = 1.0 + 0.12 * (t * 3.2 + k as f32 * 1.7).sin();
            let boost = 1.0 + if m.exposed { 0.6 } else { 0.0 } + m.flash * 1.2 + if w.spot == Spot::Nape { m.hijack } else { 0.0 };
            if let Ok(mut tf) = tfs.get_mut(core) {
                *tf = Transform::from_translation(at).with_scale(Vec3::splat(base * pulse * boost));
            }
            if let Ok(mut tf) = tfs.get_mut(halo) {
                *tf = Transform::from_translation(at).with_scale(Vec3::splat(base * 2.6 * pulse * boost));
            }
        }
    }
    // Giants gone: their markers go.
    live.sort_unstable();
    let gone: Vec<ConstructId> = fx.markers.keys().copied().filter(|id| live.binary_search(id).is_err()).collect();
    for id in gone {
        if let Some(m) = fx.markers.remove(&id) {
            for (a, b) in m.ents {
                commands.entity(a).try_despawn();
                commands.entity(b).try_despawn();
            }
        }
    }
}

/// Severed limbs: hide their part entities (every frame: late joins, rebuilt models) and burst the
/// fresh ones into voxel debris.
fn hide_severed(
    active: Option<Res<ActiveWorld>>,
    mut fx: ResMut<GiantFx>,
    map: Res<super::constructs::ConstructEntities>,
    q: Query<(&ConstructVisual, &ConstructParts, &GlobalTransform)>,
    mut vis: Query<&mut Visibility>,
    mut shatter: MessageWriter<ShatterModelFx>,
    time: Res<Time>,
) {
    let Some(active) = active else { return };
    for (id, e) in map.0.iter() {
        let Some(c) = active.world.constructs.get(*id) else { continue };
        let g = &c.ai.giant;
        if !g.on || g.severed == 0 {
            continue;
        }
        let Ok((v, parts, _)) = q.get(*e) else { continue };
        for (i, pe) in &parts.parts {
            let l = limb_of_model_part(&v.model, *i);
            if g.is_severed(l)
                && let Ok(mut vv) = vis.get_mut(*pe)
                && *vv != Visibility::Hidden
            {
                *vv = Visibility::Hidden;
            }
        }
    }
    let dt = time.delta_secs().min(0.1);
    let pending = std::mem::take(&mut fx.sever);
    for (id, limb, ttl) in pending {
        let ready = map.0.get(&id).and_then(|e| q.get(*e).ok());
        let Some((v, _, gt)) = ready else {
            if ttl > dt {
                fx.sever.push((id, limb, ttl - dt));
            }
            continue;
        };
        let mut sub: VoxelModel = (*v.model).clone();
        let mut n = 0;
        for i in 0..sub.parts.len() {
            if limb_of_model_part(&v.model, i) != limb {
                sub.parts[i].voxels.clear();
            } else {
                n += sub.parts[i].voxels.len();
            }
        }
        if n == 0 {
            continue;
        }
        sub.voxel_count = n;
        let transform = gt.affine();
        shatter.write(ShatterModelFx {
            model: Arc::new(sub),
            transform,
            pose: None,
            corps_color: v.corps_color,
            accent: v.accent,
            impulse: Vec3::new(0.0, -2.0, 0.0),
            point: None,
            floor: None,
        });
    }
}

/// Lingering dust (lit brown puffs).
fn dust(rate: f32, size: f32, rise: f32) -> SmokeFx {
    let mut s = SmokeFx::default();
    s.rate = rate;
    s.color = [0.46, 0.4, 0.33];
    s.size = size;
    s.rise = rise;
    s
}

/// Warning glows, shock rings, pings and lingering dust over time.
#[allow(clippy::too_many_arguments)]
fn animate_fx(
    time: Res<Time>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    mut commands: Commands,
    mut warns: Query<(Entity, &mut WarnGlow, &mut Transform), (Without<ShockRing>, Without<FocusPing>)>,
    mut rings: Query<(Entity, &mut ShockRing, &mut Transform, &MeshMaterial3d<StandardMaterial>), (Without<WarnGlow>, Without<FocusPing>)>,
    mut pings: Query<(Entity, &mut FocusPing, &mut Transform), (Without<WarnGlow>, Without<ShockRing>)>,
    mut fades: Query<(Entity, &mut Fade)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut burst: MessageWriter<BurstFx>,
) {
    let dt = time.delta_secs().min(0.1);
    for (e, mut w, mut tf) in &mut warns {
        w.t += dt;
        if w.t >= w.secs + 0.2 {
            commands.entity(e).try_despawn();
            continue;
        }
        let Some((p, r, s)) = view_tf(&views, w.id) else {
            commands.entity(e).try_despawn();
            continue;
        };
        let c = active.as_ref().and_then(|a| a.world.constructs.get(w.id));
        let (center, size) = c
            .and_then(|c| c.ai.giant.body.limb_box(w.limb))
            .map_or((p, 1.0), |(lc, lh)| (p + r * (bv(lc) * s), (lh.max_element() * s).clamp(0.6, 4.0)));
        let k = (w.t / w.secs).clamp(0.0, 1.0);
        // Swells, then a bright pop as the clip starts.
        let pop = if w.t > w.secs { 1.0 + (w.t - w.secs) * 4.0 } else { 1.0 };
        tf.translation = center;
        tf.scale = Vec3::splat(size * (0.35 + 0.65 * k * k) * pop);
    }
    for (e, mut ring, mut tf, mat) in &mut rings {
        ring.t += dt;
        match ring.preview {
            Some(life) => {
                // Reach preview: fades in, holds, gone when the stomp lands (its own ring takes over).
                if ring.t >= life {
                    commands.entity(e).try_despawn();
                    continue;
                }
                let a = (ring.t / 0.2).min(1.0) * (1.0 - ((ring.t - (life - 0.15)) / 0.15).clamp(0.0, 1.0));
                if let Some(mut m) = materials.get_mut(&mat.0) {
                    m.base_color.set_alpha(a);
                }
                tf.scale = Vec3::splat(ring.r_max);
            }
            None => {
                let r = (ring.r0 + ring.speed * ring.t).min(ring.r_max);
                tf.scale = Vec3::new(r, 1.0, r);
                tf.translation = ring.origin;
                let left = 1.0 - (r / ring.r_max.max(0.1));
                if let Some(mut m) = materials.get_mut(&mat.0) {
                    let fade = left.clamp(0.0, 1.0).sqrt();
                    m.emissive = LinearRgba::from(m.base_color) * (2.0 * fade);
                }
                // Dust kicked up along the front.
                ring.dust_t -= dt;
                if ring.dust_t <= 0.0 && r < ring.r_max {
                    ring.dust_t = 0.09;
                    let n = 6;
                    for k in 0..n {
                        let a = (k as f32 + ring.t * 7.0) / n as f32 * std::f32::consts::TAU;
                        let at = ring.origin + Vec3::new(a.cos() * r, 0.2, a.sin() * r);
                        burst.write(BurstFx { kind: BurstKind::Dust, pos: at, dir: Vec3::new(a.cos(), 0.8, a.sin()), scale: 0.8, color: None });
                    }
                }
                if r >= ring.r_max && ring.t > ring.r_max / ring.speed.max(1.0) + 0.25 {
                    commands.entity(e).try_despawn();
                }
            }
        }
    }
    for (e, mut ping, mut tf) in &mut pings {
        ping.t += dt;
        let Some(p) = views.player(ping.target) else {
            commands.entity(e).try_despawn();
            continue;
        };
        if ping.t > 1.4 {
            commands.entity(e).try_despawn();
            continue;
        }
        let k = (ping.t / 0.15).min(1.0) * (1.0 - ((ping.t - 1.1) / 0.3).clamp(0.0, 1.0));
        tf.translation = p.pos + Vec3::Y * (2.8 + 0.2 * (ping.t * 6.0).sin());
        tf.scale = Vec3::splat(0.35 * k.max(0.01));
    }
    for (e, mut f) in &mut fades {
        f.left -= dt;
        if f.left <= 0.0 {
            commands.entity(e).try_despawn();
        }
    }
}

/// `--scenario giants --giant-shot ride|nape|sever|topple|stomp` (practice): dev frames of the
/// giant fight. A kaiju is summoned by bot 2 on open ground near you; bots stop thinking; the script
/// drives your input (`InputOverride` + look) and, for the sever / topple shots, deals the damage
/// directly (host world) so the moment lands on a known frame.
pub mod scenario {
    use crate::input::{InputOverride, LookAngles};
    use crate::sim_bridge::ActiveWorld;
    use bd_core::input::buttons;
    use bd_core::mechanics::{Element, Form, Mechanic};
    use bd_core::{ConstructId, PlayerId, Vec3};
    use bd_sim::combat::ConstructHit;
    use bd_sim::constructs::giant::{self, Limb, Spot};
    use bevy::prelude::{info, warn, App, IntoScheduleConfigs, Res, ResMut, Resource, Time, Update};
    use bevy::time::Real;
    use bevy::ecs::schedule::common_conditions::resource_exists;

    #[derive(Resource, Clone, Debug, Default)]
    pub struct GiantScenario {
        shot: String,
        t: f32,
        placed: bool,
        kaiju: Option<ConstructId>,
        /// Kaiju spot + the direction from you to it.
        spot: Vec3,
        dir: Vec3,
        step: u32,
    }

    pub fn build(app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "giants") {
            return;
        }
        let shot = args.windows(2).find(|w| w[0] == "--giant-shot").map(|w| w[1].clone()).unwrap_or_else(|| "ride".into());
        info!("giants scenario: {shot}");
        app.insert_resource(GiantScenario { shot, ..Default::default() }).add_systems(Update, script.run_if(resource_exists::<ActiveWorld>));
    }

    fn ground(w: &bd_sim::World, x: f32, z: f32, near_y: f32) -> Option<f32> {
        let from = Vec3::new(x, near_y + 12.0, z);
        let hit = bd_core::voxel::raycast(&w.voxels, from, Vec3::NEG_Y, 30.0)?;
        ((hit.point.y - near_y).abs() < 2.5).then_some(hit.point.y)
    }

    /// A kaiju spot on flat, open ground (a 10 m square within 2 m, nothing overhead) and a spot for
    /// you on the ground `dist` m away that sees it. Near you first, then anywhere on the map.
    fn stage(w: &bd_sim::World, me: Vec3, dist: f32) -> Option<(Vec3, Vec3, Vec3)> {
        let top = |x: f32, z: f32| bd_core::voxel::raycast(&w.voxels, Vec3::new(x, 150.0, z), Vec3::NEG_Y, 170.0).map(|h| h.point.y);
        let clear_up = |p: Vec3, h: f32| bd_core::voxel::raycast(&w.voxels, p + Vec3::Y * 0.6, Vec3::Y, h).is_none();
        let mut cands: Vec<Vec3> = Vec::new();
        for r in [16.0f32, 24.0, 32.0] {
            for k in 0..16 {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                cands.push(me + Vec3::new(a.cos() * r, 0.0, a.sin() * r));
            }
        }
        for gx in -7..=7 {
            for gz in -7..=7 {
                cands.push(Vec3::new(gx as f32 * 12.0, 0.0, gz as f32 * 12.0));
            }
        }
        for c in cands {
            let Some(y0) = top(c.x, c.z) else { continue };
            let spot = Vec3::new(c.x, y0, c.z);
            let flat = (-1..=1).all(|i| (-1..=1).all(|j| top(c.x + i as f32 * 5.0, c.z + j as f32 * 5.0).is_some_and(|y| (y - y0).abs() < 2.0)));
            if !flat || !clear_up(spot, 10.0) {
                continue;
            }
            for k in 0..12 {
                let a = k as f32 / 12.0 * std::f32::consts::TAU;
                let d = Vec3::new(a.cos(), 0.0, a.sin());
                let at = spot - d * dist;
                let Some(y) = top(at.x, at.z).filter(|y| (y - y0).abs() < 3.0) else { continue };
                let from = Vec3::new(at.x, y + 0.05, at.z);
                let eye = from + Vec3::Y * 1.6;
                let to = spot + Vec3::Y * 5.0;
                if clear_up(from, 5.0) && bd_core::voxel::raycast(&w.voxels, eye, (to - eye).normalize_or_zero(), (to - eye).length()).is_none() {
                    return Some((from, spot, d));
                }
            }
        }
        None
    }

    fn aim(look: &mut LookAngles, eye: Vec3, at: Vec3) {
        let d = (at - eye).normalize_or_zero();
        look.yaw = (-d.x).atan2(-d.z);
        look.pitch = d.y.clamp(-1.0, 1.0).asin();
    }

    fn part_world(w: &bd_sim::World, id: ConstructId, l: Limb) -> Option<Vec3> {
        let c = w.constructs.get(id)?;
        let (lc, _) = c.ai.giant.body.limb_box(l)?;
        Some(giant::to_world(c, lc))
    }

    fn nape_world(w: &bd_sim::World, id: ConstructId) -> Option<Vec3> {
        let c = w.constructs.get(id)?;
        let wp = c.ai.giant.body.weak_points().iter().find(|p| p.spot == Spot::Nape).copied()?;
        Some(giant::weak_world(c, &wp))
    }

    fn script(time: Res<Time<Real>>, mut sc: ResMut<GiantScenario>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>, mut ov: ResMut<InputOverride>) {
        let dt = time.delta_secs();
        sc.t += dt;
        if sc.t < 0.8 {
            return;
        }
        let Some((me, my_pos)) = active.local_player().filter(|p| p.alive).map(|p| (p.id, p.mv.pos)) else { return };
        let others: Vec<PlayerId> = active.world.players.iter().map(|p| p.id).filter(|id| *id != me).collect();
        let (Some(&owner), Some(&far)) = (others.first(), others.get(1)) else { return };
        if !sc.placed {
            // Stage: flat open ground for the kaiju, you on the ground `dist` m away with a clear view.
            let dist = match sc.shot.as_str() { "stomp" => 10.0, "sever" | "topple" => 16.0, _ => 20.0 };
            let Some((from, spot, dir)) = stage(&active.world, my_pos, dist) else {
                if sc.t > 6.0 {
                    warn!("giants scenario: no stage found");
                    sc.placed = true;
                }
                return;
            };
            if let Some(p) = active.world.player_mut(me) {
                p.mv.pos = from;
                p.mv.vel = Vec3::ZERO;
            }
            let my_pos = from;            active.bots.clear();
            active.world.cfg.rules.willpower = bd_sim::WillpowerMode::Infinite;
            // The kaiju's owner stands behind you; its target (another bot) far beyond it, so it
            // walks away from you (ride) — or you are its target (stomp).
            let behind = my_pos - dir * 30.0;
            let beyond = spot + dir * 40.0;
            for (id, at) in [(owner, behind), (far, beyond)] {
                let gy = ground(&active.world, at.x, at.z, spot.y).unwrap_or(spot.y);
                if let Some(p) = active.world.player_mut(id) {
                    p.mv.pos = Vec3::new(at.x, gy + 0.1, at.z);
                    p.mv.vel = Vec3::ZERO;
                    p.spawn_shield = 0.0;
                }
            }
            let mechs: &[Mechanic] = if sc.shot == "stomp" { &[Mechanic::Stomp] } else { &[Mechanic::ArticulatedTail] };
            let r = bd_sim::bots::recipe("godzilla", Form::Kaiju, mechs, Element::Atomic, 13.0);
            let eye = active.world.player(owner).map(|p| p.eye(active.world.params())).unwrap_or(spot);
            match active.world.cast(owner, r, None, bd_sim::Aim::new(eye, (spot - eye).normalize_or_zero())) {
                Ok(id) => {
                    if let Some(c) = active.world.constructs.get_mut(id) {
                        c.pos = spot;
                        // Side-on to you (its back / tail toward you for the ride).
                        let face = if sc.shot == "stomp" { -dir } else { Vec3::new(dir.z, 0.0, -dir.x) };
                        c.rot = bd_sim::geom::yaw_rotation(face);
                        c.life = 1e6;
                    }
                    sc.kaiju = Some(id);
                }
                Err(e) => warn!("giants scenario: cast failed {e:?}"),
            }
            if sc.shot == "stomp" {
                // You are its target: the other bot goes away.
                if let Some(p) = active.world.player_mut(far) {
                    p.alive = false;
                    p.respawn_timer = 1e6;
                }
            }
            if sc.shot == "nape" {
                let scale = bd_sim::constructs::form_spec(Form::Sword).default_scale;
                let eye = active.world.player(me).map(|p| p.eye(active.world.params())).unwrap_or(my_pos);
                let _ = active.world.cast(me, bd_sim::bots::recipe("sword", Form::Sword, &[Mechanic::Blade], Element::Light, scale), None, bd_sim::Aim::new(eye, dir));
            }
            sc.spot = spot;
            sc.dir = dir;
            sc.placed = true;
            sc.t = 1.0;
            info!("giants scenario: kaiju {:?} at {spot:?}", sc.kaiju);
            return;
        }
        let Some(id) = sc.kaiju.filter(|id| active.world.constructs.get(*id).is_some()) else { return };
        let eye = active.world.player(me).map(|p| p.eye(active.world.params())).unwrap_or(my_pos);
        ov.active = true;
        ov.held = 0;
        ov.move_x = 0;
        ov.move_z = 0;
        let t = sc.t;
        if (t * 2.0).floor() != ((t - dt) * 2.0).floor() {
            if let Some(c) = active.world.constructs.get(id) {
                let g = &c.ai.giant;
                info!("giants scenario t={t:.1}: on {} toppled {} stagger {:.2} severed {:#x} legBL {:.0}/{:.0} integrity {:.0} anim {:?} mode {:?}", g.on, g.toppled(), g.stagger, g.severed, g.limb_hp[Limb::LegBL as usize], g.limb_max[Limb::LegBL as usize], c.integrity, c.anim_state, c.ai.mode);
            }
        }
        if sc.shot != "ride" {
            if let Some(c) = active.world.constructs.get_mut(id) {
                c.pos.x = sc.spot.x;
                c.pos.z = sc.spot.z;
                c.vel.x = 0.0;
                c.vel.z = 0.0;
            }
        }
        match sc.shot.as_str() {
            // Hook the tail, reel in, land on it, run up the back.
            "ride" => {
                if t < 2.2 {
                    if let Some(p) = part_world(&active.world, id, Limb::Tail) {
                        aim(&mut look, eye, p);
                    }
                } else if t < 3.6 {
                    if let Some(p) = part_world(&active.world, id, Limb::Tail) {
                        aim(&mut look, eye, p + Vec3::Y * 1.5);
                    }
                    ov.held = buttons::TETHER_L | buttons::REEL;
                } else {
                    // On the tail: face the head and run up (hop the steps).
                    if let Some(h) = part_world(&active.world, id, Limb::Head) {
                        let mut to = h - eye;
                        to.y = 0.0;
                        aim(&mut look, eye, eye + to.normalize_or_zero() * 10.0 + Vec3::Y * -2.0);
                    }
                    ov.move_z = 1;
                    if (t * 3.0) as i32 % 2 == 0 {
                        ov.held = buttons::JUMP;
                    }
                }
            }
            // Stand on its back at the nape and slash it (a stagger), again after the cooldown.
            "nape" => {
                let Some(n) = nape_world(&active.world, id) else { return };
                if let Some(c) = active.world.constructs.get_mut(id) {
                    c.vel = Vec3::ZERO;
                    for cd in c.ai.cd.iter_mut() {
                        *cd = 99.0;
                    }
                }
                if t < 1.6 {
                    let back = n - sc.dir * 2.2 + Vec3::Y * 0.2;
                    if let Some(p) = active.world.player_mut(me) {
                        p.mv.pos = back;
                        p.mv.vel = Vec3::ZERO;
                    }
                }
                aim(&mut look, eye, n);
                let c = ((t - 1.6) / 1.4).fract();
                if t > 1.6 && c < 0.06 {
                    ov.held = buttons::PRIMARY;
                }
            }
            // An arm is cut off (hits on the arm), watched from the side.
            "sever" => {
                if let Some(a) = part_world(&active.world, id, Limb::ArmR) {
                    aim(&mut look, eye, a);
                    if t > 3.0 && sc.step < 12 && (t * 8.0) as u32 > sc.step + 24 {
                        sc.step += 1;
                        active.world.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: id, source: Some(me), damage: 9.0, impulse: Vec3::ZERO, blade: true, point: a });
                    }
                }
            }
            // A leg is broken: it topples onto its side, the nape comes down.
            "topple" => {
                if let Some(c) = active.world.constructs.get_mut(id) {
                    for cd in c.ai.cd.iter_mut() {
                        *cd = 99.0;
                    }
                }
                if let Some(k) = active.world.constructs.get(id).map(|c| c.center()) {
                    aim(&mut look, eye, k);
                }
                if t > 3.5 && sc.step == 0 && active.world.constructs.get(id).is_some_and(|c| c.ai.giant.on) {
                    sc.step = 1;
                    if let Some(l) = part_world(&active.world, id, Limb::LegBL) {
                        let max = active.world.constructs.get(id).map_or(100.0, |c| c.ai.giant.limb_max[Limb::LegBL as usize]);
                        active.world.apply_construct_hit(ConstructHit { pierce: false, blunt: false, target: id, source: Some(me), damage: max + 1.0, impulse: Vec3::ZERO, blade: false, point: l });
                    }
                }
            }
            // You stand in front of it; it stomps (you are its target).
            "stomp" => {
                // A victim's view: the feet and the ground the shockwave runs along.
                aim(&mut look, eye, sc.spot + Vec3::Y * 2.5);
                if let Some(c) = active.world.constructs.get_mut(id) {
                    c.vel.x = 0.0;
                    c.vel.z = 0.0;
                    c.pos.x = sc.spot.x;
                    c.pos.z = sc.spot.z;
                }
            }
            _ => {}
        }
    }
}
