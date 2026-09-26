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
use super::{bq, bv, corps_rgb, PresentSet};
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
    views.constructs.iter().find(|v| v.id == id).map(|v| (bv(v.pos), bq(v.rot), v.size))
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
    for ev in events.read() {
        let GameEvent::Giant(g) = &ev.event else { continue };
        let corps_of = |id: ConstructId| active.world.constructs.get(id).map_or([1.0, 1.0, 1.0], |c| corps_rgb(active.corps_of(c.owner)));
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
            let halo = materials.add(glow(col, 0.35, true));
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
        let (p, r, s) = (bv(v.pos), bq(v.rot), v.size);
        for (k, w) in wps.iter().enumerate() {
            let Some(&(core, halo)) = m.ents.get(k) else { continue };
            let at = p + r * (bv(w.pos) * s);
            let base = w.radius * s * if w.spot == Spot::Nape { 0.32 } else { 0.24 };
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
                if let Some(m) = materials.get_mut(&mat.0) {
                    m.base_color.set_alpha(a);
                }
                tf.scale = Vec3::splat(ring.r_max);
            }
            None => {
                let r = (ring.r0 + ring.speed * ring.t).min(ring.r_max);
                tf.scale = Vec3::new(r, 1.0, r);
                tf.translation = ring.origin;
                let left = 1.0 - (r / ring.r_max.max(0.1));
                if let Some(m) = materials.get_mut(&mat.0) {
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
        tf.translation = bv(p.pos) + Vec3::Y * (2.8 + 0.2 * (ping.t * 6.0).sin());
        tf.scale = Vec3::splat(0.35 * k.max(0.01));
    }
    for (e, mut f) in &mut fades {
        f.left -= dt;
        if f.left <= 0.0 {
            commands.entity(e).try_despawn();
        }
    }
}
