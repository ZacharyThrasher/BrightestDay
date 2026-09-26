//! Mechanic visuals: Link chains, portal pairs, Attract/Repel force fields.

use std::collections::HashMap;

use bd_shapes::{chain_link_mesh, chain_transform, portal_ring_spec, voxelize};
use bevy::camera::visibility::VisibilitySystems;
use bevy::light::NotShadowCaster;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::construct::{ConstructMeshCache, FxAnchor, FxSlot, ModelKey, SlotAlloc, construct_gpu, write_anchor, write_slot};
use crate::gpu::{AnchorGpu, FxUpload};
use crate::material::FxMaterials;
use crate::meshing::build_part_mesh;
use crate::palette::Palette;
use crate::particles::{Emit, EmitSystems, FxParticles, PFlags, PKind};
use crate::util::{aff, fin, g3};
use crate::{FxSet, FxSettings};

/// Dynamic hard-light chain between two world points (Mechanic::Link). Spawn it on its own entity;
/// bd_fx owns that entity's mesh/transform.
#[derive(Component, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct ChainFx {
    pub a: Vec3,
    pub b: Vec3,
    pub corps_color: [f32; 3],
    pub integrity: f32,
    pub flash: f32,
}

impl ChainFx {
    pub fn new(a: Vec3, b: Vec3, corps_color: [f32; 3]) -> Self {
        ChainFx { a, b, corps_color, integrity: 1.0, flash: 0.0 }
    }
}

/// Portal gateway: voxel ring (bd_shapes `portal_ring_spec`) + swirling inner surface. Faces −Z of
/// the entity's transform. `open` 0 → 1 grows it in.
#[derive(Component, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct PortalFx {
    pub diameter: f32,
    pub corps_color: [f32; 3],
    pub accent: Option<[f32; 3]>,
    pub open: f32,
    pub flash: f32,
}

impl PortalFx {
    pub fn new(diameter: f32, corps_color: [f32; 3]) -> Self {
        PortalFx { diameter, corps_color, accent: None, open: 1.0, flash: 0.0 }
    }
}

/// Attract/Repel force field: particles spiral in (attract) or out (repel) around the entity.
#[derive(Component, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct ForceFieldFx {
    pub radius: f32,
    pub attract: bool,
    pub color: [f32; 3],
    /// Emission multiplier (1 = default ~70 particles/s).
    pub intensity: f32,
}

#[derive(Component)]
struct ChainState {
    bucket: u32,
    mesh_len: f32,
}

#[derive(Component)]
struct PortalState {
    ring: Entity,
    disc: Entity,
    inner: f32,
}

#[derive(Component, Default)]
struct FieldState {
    credit: f32,
}

#[derive(Resource, Default)]
struct ChainMeshes(HashMap<u32, Handle<Mesh>>);

#[derive(Resource)]
struct DiscMesh(Handle<Mesh>);

pub fn build(app: &mut App) {
    app.init_resource::<ChainMeshes>()
        .add_systems(Startup, |mut c: Commands, mut m: ResMut<Assets<Mesh>>| {
            c.insert_resource(DiscMesh(m.add(Circle::new(1.0).mesh().resolution(48))));
        })
        .add_systems(PostUpdate, ((init_chains, init_portals, init_fields), update_chains).chain().in_set(FxSet::Lifecycle))
        .add_systems(PostUpdate, (write_portals, emit_fields).in_set(EmitSystems).before(VisibilitySystems::CheckVisibility));
}

const CHAIN_BUCKET: f32 = 0.5;

fn chain_mesh(cache: &mut ChainMeshes, meshes: &mut Assets<Mesh>, bucket: u32) -> Handle<Mesh> {
    cache
        .0
        .entry(bucket)
        .or_insert_with(|| {
            let len = bucket as f32 * CHAIN_BUCKET;
            let pm = chain_link_mesh(len);
            match build_part_mesh(&pm, bd_core::CONSTRUCT_VOXEL_SIZE, Vec3::ZERO, &[]) {
                Some((m, _)) => meshes.add(m),
                None => meshes.add(Cuboid::new(0.1, 0.1, len)),
            }
        })
        .clone()
}

fn bucket_of(len: f32) -> u32 {
    ((fin(len, 1.0) / CHAIN_BUCKET).round() as u32).clamp(1, 60)
}

fn init_chains(
    mut commands: Commands,
    mut alloc: ResMut<SlotAlloc>,
    mut cache: ResMut<ChainMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mats: Res<FxMaterials>,
    settings: Res<FxSettings>,
    q: Query<(Entity, &ChainFx), Without<ChainState>>,
) {
    for (e, c) in &q {
        let bucket = bucket_of(c.a.distance(c.b));
        let slot = alloc.slot();
        commands.entity(e).insert((
            FxSlot(slot),
            ChainState { bucket, mesh_len: bucket as f32 * CHAIN_BUCKET },
            Mesh3d(chain_mesh(&mut cache, &mut meshes, bucket)),
            MeshMaterial3d(mats.construct(settings.translucent)),
            MeshTag(slot),
            NotShadowCaster,
        ));
    }
}

fn update_chains(
    mut cache: ResMut<ChainMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut up: ResMut<FxUpload>,
    mut q: Query<(&ChainFx, &FxSlot, &mut ChainState, &mut Mesh3d, &mut Transform)>,
) {
    for (c, slot, mut st, mut mesh, mut tf) in &mut q {
        let len = c.a.distance(c.b);
        let bucket = bucket_of(len);
        // Hysteresis: only re-mesh when the stretch gets noticeable.
        let stretch = len / st.mesh_len.max(0.1);
        if bucket != st.bucket && !(0.9..=1.1).contains(&stretch) {
            st.bucket = bucket;
            st.mesh_len = bucket as f32 * CHAIN_BUCKET;
            mesh.0 = chain_mesh(&mut cache, &mut meshes, bucket);
        }
        let m = aff(&chain_transform(g3(c.a), g3(c.b), st.mesh_len));
        let (s, r, t) = m.to_scale_rotation_translation();
        if s.is_finite() && r.is_finite() && t.is_finite() {
            *tf = Transform { translation: t, rotation: r, scale: s };
        }
        let pal = Palette::new(c.corps_color, None);
        write_slot(&mut up, slot.0, construct_gpu(&pal, c.integrity, 1.0, c.flash, 0.0, bd_core::CONSTRUCT_VOXEL_SIZE, slot.0, 0.8, false, c.a, 0.1));
    }
}

fn init_portals(
    mut commands: Commands,
    mut alloc: ResMut<SlotAlloc>,
    mut cache: ResMut<ConstructMeshCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mats: Res<FxMaterials>,
    disc: Option<Res<DiscMesh>>,
    settings: Res<FxSettings>,
    q: Query<(Entity, &PortalFx), Without<PortalState>>,
) {
    let Some(disc) = disc else { return };
    for (e, p) in &q {
        let dq = ((fin(p.diameter, 2.5).clamp(1.0, 40.0)) * 4.0).round() as u32;
        let key = ModelKey::Named("portal_ring", dq);
        let model = voxelize(&portal_ring_spec(dq as f32 / 4.0));
        // Inner hole radius from the voxels (ring plane = XY).
        let mut inner = f32::MAX;
        for part in &model.parts {
            for (v, _) in &part.voxels {
                let c = model.voxel_center(*v);
                inner = inner.min((c.x * c.x + c.y * c.y).sqrt());
            }
        }
        let inner = if inner.is_finite() { (inner - model.voxel_size * 0.5).max(0.2) } else { dq as f32 / 8.0 };
        let parts = cache.insert_now(key, &model, &mut meshes);
        let slot = alloc.slot();
        let mut ring = Entity::PLACEHOLDER;
        if let Some(Some(cp)) = parts.first() {
            ring = commands
                .spawn((
                    Mesh3d(cp.mesh.clone()),
                    MeshMaterial3d(mats.construct(settings.translucent)),
                    MeshTag(slot),
                    Transform::from_translation(cp.offset),
                    NotShadowCaster,
                    ChildOf(e),
                ))
                .id();
        }
        let disc_e = commands
            .spawn((
                Mesh3d(disc.0.clone()),
                MeshMaterial3d(mats.portal.clone()),
                MeshTag(slot),
                Transform::from_scale(Vec3::splat(inner + 0.02)),
                NotShadowCaster,
                ChildOf(e),
            ))
            .id();
        commands.entity(e).insert((FxSlot(slot), PortalState { ring, disc: disc_e, inner }));
    }
}

fn write_portals(mut up: ResMut<FxUpload>, q: Query<(&PortalFx, &PortalState, &FxSlot, &GlobalTransform)>) {
    for (p, st, slot, gt) in &q {
        let _ = (st.ring, st.disc, st.inner);
        let pal = Palette::new(p.corps_color, p.accent);
        let open = fin(p.open, 1.0).clamp(0.0, 1.0);
        let r = fin(p.diameter, 2.5).max(0.5) * 0.5;
        let centre = gt.translation();
        write_slot(&mut up, slot.0, construct_gpu(&pal, 1.0, open, p.flash, 0.0, bd_core::CONSTRUCT_VOXEL_SIZE, slot.0 * 13, 0.8, false, centre, 1.0 / (r * gt.scale().x.max(1e-3) + 0.5)));
    }
}

fn init_fields(mut commands: Commands, mut alloc: ResMut<SlotAlloc>, q: Query<Entity, (With<ForceFieldFx>, Without<FieldState>)>) {
    for e in &q {
        let a = alloc.anchor();
        commands.entity(e).insert((FxAnchor(a), FieldState::default()));
    }
}

fn emit_fields(
    mut fx: ResMut<FxParticles>,
    mut up: ResMut<FxUpload>,
    settings: Res<FxSettings>,
    time: Res<Time>,
    mut q: Query<(&ForceFieldFx, &FxAnchor, &mut FieldState, &GlobalTransform)>,
) {
    let dt = time.delta_secs();
    let budget = settings.particle_budget.clamp(0.0, 1.0);
    for (f, anchor, mut st, gt) in &mut q {
        let aff = gt.affine();
        write_anchor(&mut up, anchor.0, AnchorGpu::from_affine(&aff, aff.translation.into()));
        let radius = fin(f.radius, 3.0).clamp(0.3, 60.0);
        st.credit += 70.0 * f.intensity.max(0.0) * budget * dt * (radius / 3.0).sqrt();
        let mut guard = 0;
        while st.credit >= 1.0 && guard < 128 {
            guard += 1;
            st.credit -= 1.0;
            let r = &mut fx.rng;
            let life = r.range(0.7, 1.3);
            let w = r.range(2.0, 4.5) * if r.f() < 0.5 { 1.0 } else { -1.0 };
            let (r0, radial) = if f.attract {
                let r0 = radius * r.range(0.75, 1.1);
                (r0, -r0 / life * 0.95)
            } else {
                let r0 = radius * r.range(0.08, 0.25);
                (r0, (radius - r0) / life)
            };
            let seed = r.next_u32();
            let white = r.f() < 0.25;
            let size = r.range(0.04, 0.08) * (radius / 3.0).sqrt().clamp(0.7, 2.0);
            fx.emit(Emit {
                kind: PKind::Orbit,
                flags: PFlags::STRETCH | PFlags::PLAIN,
                vel: Vec3::new(r0, w, radial),
                size,
                color: if white { [1.0, 1.0, 1.0] } else { f.color },
                emissive: if white { 4.0 } else { 3.0 },
                life,
                spin: 3.0,
                anchor: anchor.0,
                seed,
                ..default()
            });
        }
    }
}
