//! Pipeline warm-up: one entity per bd_fx material/mesh-layout variant, for the game's loading
//! phase (it parks them in front of the camera for a few frames so every pipeline specialises and
//! compiles before the match — no first-use hitches).

use bd_core::shape::{Prim, PrimKind, ShapeSpec, Tint};
use bevy::light::NotShadowCaster;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::construct::{ConstructMeshCache, ModelKey, SlotAlloc, construct_gpu, write_slot};
use crate::gpu::FxUpload;
use crate::material::FxMaterials;
use crate::palette::Palette;
use crate::particles::{Emit, FxParticles, PFlags, PKind};

/// Spawn one of every bd_fx render variant (translucent + solid construct material, portal
/// surface, particle cubes). Returns the entities; tag them for the loading screen and despawn them
/// when loading ends (bd_game's `WarmupItem` does both). Each entity is a single mesh entity at the
/// origin — reposition freely.
pub fn spawn_warmup(world: &mut World) -> Vec<Entity> {
    let mats = world.resource::<FxMaterials>().clone();
    let spec = ShapeSpec {
        prims: vec![
            Prim { kind: PrimKind::Box, center: glam::Vec3::ZERO, size: glam::Vec3::new(3.0, 3.0, 3.0), rot_deg: glam::Vec3::ZERO, tint: Tint::Primary, part: 0, subtract: false },
            Prim { kind: PrimKind::Box, center: glam::Vec3::new(0.0, 2.0, 0.0), size: glam::Vec3::ONE, rot_deg: glam::Vec3::ZERO, tint: Tint::Core, part: 0, subtract: false },
        ],
        parts: vec![],
    };
    let model = bd_shapes::voxelize(&spec);
    let parts = world.resource_scope(|world, mut cache: Mut<ConstructMeshCache>| {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        cache.insert_now(ModelKey::Named("warmup", 0), &model, &mut meshes)
    });
    let slot = world.resource_mut::<SlotAlloc>().slot();
    let pal = Palette::new([0.2, 1.0, 0.35], None);
    world.resource_scope(|_, mut up: Mut<FxUpload>| {
        write_slot(&mut up, slot, construct_gpu(&pal, 0.5, 0.7, 0.0, 0.0, model.voxel_size, 1, 0.7, false, Vec3::ZERO, 1.0));
    });
    let disc = world.resource_mut::<Assets<Mesh>>().add(Circle::new(0.5).mesh().resolution(48));
    let mut out = Vec::new();
    if let Some(Some(p)) = parts.first() {
        for translucent in [true, false] {
            out.push(
                world
                    .spawn((
                        Name::new("bd_fx warmup construct"),
                        Mesh3d(p.mesh.clone()),
                        MeshMaterial3d(mats.construct(translucent)),
                        MeshTag(slot),
                        Transform::default(),
                        NotShadowCaster,
                    ))
                    .id(),
            );
        }
    }
    out.push(
        world
            .spawn((Name::new("bd_fx warmup portal"), Mesh3d(disc), MeshMaterial3d(mats.portal.clone()), MeshTag(slot), Transform::default(), NotShadowCaster))
            .id(),
    );
    // Particle batches are always drawn (batch 0 stays visible), but emit a few so the ring batch
    // path is exercised too.
    if let Some(mut fx) = world.get_resource_mut::<FxParticles>() {
        for i in 0..8 {
            fx.emit(Emit { kind: PKind::Flash, flags: PFlags::PLAIN, pos: Vec3::new(0.0, -1000.0, 0.0), size: 0.01, life: 0.5, seed: i, ..default() });
        }
    }
    out
}
