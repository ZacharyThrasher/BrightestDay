//! Mechanic visuals: Link chains, portal pairs, Attract/Repel force fields.

use std::collections::HashMap;

use bd_shapes::{chain_link_mesh, chain_transform, portal_ring_spec, voxelize};
use bd_core::shape::{Prim, PrimKind, ShapeSpec, Tint};
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
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

/// Portal gateway: voxel ring (bd_shapes `portal_ring_spec`) or a wooden door frame + the inner
/// surface. Faces −Z of the entity's transform. `open` 0 → 1 grows it in.
///
/// See-through: the game renders the partner portal's view into one of two pictures
/// ([`crate::PortalMaterial`]'s `view_a` / `view_b`) and sets `view` (1 = a, 2 = b) + `view_uv`
/// (used fraction of the picture) + `view_mix` (0 swirl → 1 window). With `view_layers` set the
/// surface is drawn twice: the window-capable disc on `view_layers[0]` (the gameplay camera's
/// layer) and a swirl-only disc on `view_layers[1]` (the portal-view cameras' layer: a portal seen
/// through a portal is a swirl, never a recursive render).
#[derive(Component, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct PortalFx {
    /// Ring: outer diameter (m). Doorway: opening height (m).
    pub diameter: f32,
    pub corps_color: [f32; 3],
    pub accent: Option<[f32; 3]>,
    pub open: f32,
    pub flash: f32,
    /// Opening shape: [`PortalFx::RING`] or [`PortalFx::DOOR`] (width = height × `door_aspect`).
    pub style: u8,
    pub door_aspect: f32,
    pub view: u8,
    pub view_uv: Vec2,
    pub view_mix: f32,
    pub view_layers: Option<[usize; 2]>,
}

impl PortalFx {
    pub const RING: u8 = 0;
    pub const DOOR: u8 = 1;

    pub fn new(diameter: f32, corps_color: [f32; 3]) -> Self {
        PortalFx {
            diameter,
            corps_color,
            accent: None,
            open: 1.0,
            flash: 0.0,
            style: Self::RING,
            door_aspect: 0.5,
            view: 0,
            view_uv: Vec2::ONE,
            view_mix: 0.0,
            view_layers: None,
        }
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
    /// The surface's own slot (the ring / frame keeps the entity's `FxSlot`).
    disc_slot: u32,
    /// Door frame slot (wood palette), if a doorway.
    frame_slot: Option<u32>,
}

#[derive(Component, Default)]
struct FieldState {
    credit: f32,
}

#[derive(Resource, Default)]
struct ChainMeshes(HashMap<u32, Handle<Mesh>>);

#[derive(Resource)]
struct DiscMesh(Handle<Mesh>, Handle<Mesh>);

pub fn build(app: &mut App) {
    app.init_resource::<ChainMeshes>()
        .add_systems(Startup, |mut c: Commands, mut m: ResMut<Assets<Mesh>>| {
            // Unit disc (rings) and ±1 quad (doorways), both in local XY.
            c.insert_resource(DiscMesh(m.add(Circle::new(1.0).mesh().resolution(48)), m.add(Rectangle::new(2.0, 2.0))));
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
        let door = p.style == PortalFx::DOOR;
        let dq = ((fin(p.diameter, 2.5).clamp(1.0, 40.0)) * 4.0).round() as u32;
        let aspect = fin(p.door_aspect, 0.5).clamp(0.2, 1.5);
        let (model, key) = if door {
            let aq = (aspect * 20.0).round() as u32;
            (voxelize(&door_frame_spec(dq as f32 / 4.0, aq as f32 / 20.0)), ModelKey::Named("portal_door", dq * 64 + aq))
        } else {
            (voxelize(&portal_ring_spec(dq as f32 / 4.0)), ModelKey::Named("portal_ring", dq))
        };
        // Opening from the voxels (plane = XY): ring → hole radius; doorway → the frame's inner edges.
        let (mut inner, mut hx, mut hy) = (f32::MAX, f32::MAX, f32::MAX);
        for part in &model.parts {
            for (v, _) in &part.voxels {
                let c = model.voxel_center(*v);
                inner = inner.min((c.x * c.x + c.y * c.y).sqrt());
                if c.y.abs() < dq as f32 / 8.0 * 0.8 {
                    hx = hx.min(c.x.abs());
                }
                if c.x.abs() < dq as f32 / 8.0 * aspect * 0.8 {
                    hy = hy.min(c.y.abs());
                }
            }
        }
        let half = model.voxel_size * 0.5;
        let inner = if inner.is_finite() { (inner - half).max(0.2) } else { dq as f32 / 8.0 };
        let scale = if door {
            let hx = if hx.is_finite() { (hx - half).max(0.2) } else { dq as f32 / 8.0 * aspect };
            let hy = if hy.is_finite() { (hy - half).max(0.2) } else { dq as f32 / 8.0 };
            Vec3::new(hx + 0.02, hy + 0.02, 1.0)
        } else {
            Vec3::splat(inner + 0.02)
        };
        let parts = cache.insert_now(key, &model, &mut meshes);
        let slot = alloc.slot();
        let disc_slot = alloc.slot();
        let frame_slot = door.then(|| alloc.slot());
        let mut ring = Entity::PLACEHOLDER;
        if let Some(Some(cp)) = parts.first() {
            ring = commands
                .spawn((
                    Mesh3d(cp.mesh.clone()),
                    // A wooden door frame is solid; a hard-light ring keeps the construct look.
                    MeshMaterial3d(if door { mats.construct(false) } else { mats.construct(settings.translucent) }),
                    MeshTag(frame_slot.unwrap_or(slot)),
                    Transform::from_translation(cp.offset),
                    NotShadowCaster,
                    ChildOf(e),
                ))
                .id();
        }
        let mesh = if door { disc.1.clone() } else { disc.0.clone() };
        let surface = |mat: Handle<crate::PortalMaterial>| (Mesh3d(mesh.clone()), MeshMaterial3d(mat), MeshTag(disc_slot), Transform::from_scale(scale), NotShadowCaster, ChildOf(e));
        let disc_e = match p.view_layers {
            Some([main, rec]) => {
                commands.spawn((surface(mats.portal_vortex.clone()), RenderLayers::layer(rec)));
                commands.spawn((surface(mats.portal.clone()), RenderLayers::layer(main))).id()
            }
            None => commands.spawn(surface(mats.portal.clone())).id(),
        };
        if door {
            // Warm light spilling out of the open door (both faces).
            for side in [-1.0f32, 1.0] {
                commands.spawn((
                    PointLight { color: Color::srgb(1.0, 0.72, 0.42), intensity: 90_000.0, range: 7.0, radius: 0.4, shadow_maps_enabled: false, ..default() },
                    Transform::from_xyz(0.0, 0.0, side * 0.8),
                    ChildOf(e),
                ));
            }
        }
        commands.entity(e).insert((FxSlot(slot), PortalState { ring, disc: disc_e, inner, disc_slot, frame_slot }));
    }
}

/// A plain wooden door frame (doorway portals): two jambs, a lintel under a small cornice, a
/// threshold and a warm inner trim, standing upright facing −Z, origin at the opening's centre.
/// `height_m` = opening height, width = height × `aspect`. Construct-voxel units (0.25 m).
pub fn door_frame_spec(height_m: f32, aspect: f32) -> ShapeSpec {
    let vs = bd_core::CONSTRUCT_VOXEL_SIZE;
    let h = (if height_m.is_finite() { height_m } else { 3.0 }).clamp(1.0, 12.0) / vs;
    let w = h * aspect.clamp(0.2, 1.5);
    let t = (h * 0.06).clamp(1.0, 3.0).round();
    let d = t + 1.0;
    let bx = |c: glam::Vec3, s: glam::Vec3, tint: Tint| Prim { kind: PrimKind::Box, center: c, size: s, rot_deg: glam::Vec3::ZERO, tint, part: 0, subtract: false };
    let (hw, hh) = (w * 0.5, h * 0.5);
    ShapeSpec {
        prims: vec![
            // Jambs.
            bx(glam::Vec3::new(-hw - t * 0.5, 0.0, 0.0), glam::Vec3::new(t, h + t * 2.0, d), Tint::Primary),
            bx(glam::Vec3::new(hw + t * 0.5, 0.0, 0.0), glam::Vec3::new(t, h + t * 2.0, d), Tint::Primary),
            // Lintel + cornice.
            bx(glam::Vec3::new(0.0, hh + t * 0.5, 0.0), glam::Vec3::new(w + t * 2.0, t, d), Tint::Primary),
            bx(glam::Vec3::new(0.0, hh + t * 1.5, 0.0), glam::Vec3::new(w + t * 3.0, t, d + 1.0), Tint::Dark),
            // Threshold.
            bx(glam::Vec3::new(0.0, -hh - t * 0.5, 0.0), glam::Vec3::new(w + t * 2.0, t, d + 1.0), Tint::Dark),
            // Warm inner trim (the light catching the edge).
            bx(glam::Vec3::new(-hw - 0.5, 0.0, -d * 0.5), glam::Vec3::new(1.0, h, 1.0), Tint::Core),
            bx(glam::Vec3::new(hw + 0.5, 0.0, -d * 0.5), glam::Vec3::new(1.0, h, 1.0), Tint::Core),
            bx(glam::Vec3::new(0.0, hh + 0.5, -d * 0.5), glam::Vec3::new(w + 2.0, 1.0, 1.0), Tint::Core),
        ],
        parts: vec![],
    }
}

fn write_portals(mut up: ResMut<FxUpload>, q: Query<(&PortalFx, &PortalState, &FxSlot, &GlobalTransform)>) {
    for (p, st, slot, gt) in &q {
        let _ = (st.ring, st.disc, st.inner);
        let pal = Palette::new(p.corps_color, p.accent);
        let open = fin(p.open, 1.0).clamp(0.0, 1.0);
        let r = fin(p.diameter, 2.5).max(0.5) * 0.5;
        let centre = gt.translation();
        let g = construct_gpu(&pal, 1.0, open, p.flash, 0.0, bd_core::CONSTRUCT_VOXEL_SIZE, slot.0 * 13, 0.8, false, centre, 1.0 / (r * gt.scale().x.max(1e-3) + 0.5));
        write_slot(&mut up, slot.0, g);
        if let Some(fs) = st.frame_slot {
            // Doorway: a wooden frame (warm wood, dark trim, a glowing inner edge).
            let wood = Palette::new([0.42, 0.24, 0.11], Some([0.3, 0.17, 0.08]));
            let mut fg = construct_gpu(&wood, 1.0, open, p.flash, 0.0, bd_core::CONSTRUCT_VOXEL_SIZE, slot.0 * 13, 1.0, false, centre, 1.0 / (r + 0.5));
            fg.dark = [0.16, 0.09, 0.04, 0.0];
            fg.core = [1.0, 0.62, 0.28, 0.0];
            write_slot(&mut up, fs, fg);
        }
        // The surface: the same colours + the view data (see portal.wgsl).
        let mut d = g;
        let uv = Vec2::new(fin(p.view_uv.x, 1.0), fin(p.view_uv.y, 1.0)).clamp(Vec2::splat(0.01), Vec2::ONE);
        d.hair = [uv.x, uv.y, fin(p.view_mix, 0.0).clamp(0.0, 1.0), p.view.min(2) as f32];
        d.eyes = [(p.style == PortalFx::DOOR) as u8 as f32, 0.0, 0.0, 0.0];
        write_slot(&mut up, st.disc_slot, d);
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
