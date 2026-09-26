//! Voxel chunk rendering with a worker-thread remesh queue.
//!
//! Integration (bd_voxel glue, main thread):
//! ```ignore
//! // when chunk `key` changed: snapshot what the mesher needs (Arc-cloned chunk + borders) and submit
//! queue.submit(key, Vec3::ZERO, chunk_center, move || mesh_it(&world_handle, key));
//! // when a chunk is unloaded:
//! queue.remove(key);
//! ```
//! The job runs on the `AsyncComputeTaskPool`; the resulting [`ChunkMesh`] is converted to a Bevy
//! `Mesh` *on the worker*, and at most [`RemeshQueue::max_uploads_per_frame`] chunks are swapped in
//! per frame. Re-submitting a key supersedes older jobs (stale results are dropped). Jobs start
//! nearest-to-[`RemeshQueue::focus`] first (the camera).
//!
//! All chunks share one vertex-coloured material ([`VoxelMaterials::opaque`]); Bevy batches them.
//! It is a `StandardMaterial` extended with a neon glow ([`VoxelGlow`]): the mesher's per-vertex
//! emissive channel rides in the vertex colour's alpha (ignored by the opaque PBR path), and
//! materials with emissive ≥ 0.5 (neon, crystals) gain `base colour × weight × neon_boost` of raw HDR
//! emission so they bloom. `neon_boost` = the map's (`MapInfo::neon_boost`; 0 on Archipelago ⇒
//! output identical to a plain `StandardMaterial`).

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::asset::{load_internal_asset, uuid_handle, RenderAssetUsages};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use std::collections::VecDeque;

/// The mesher output (`bd_voxel::ChunkMesh`): positions (world metres or chunk-relative — see
/// `translation` in [`RemeshQueue::submit`]), normals, **linear** RGBA colours, u32 indices.
pub use bd_voxel::ChunkMesh;

/// Local-space bounds (None when empty).
pub fn chunk_mesh_aabb(cm: &ChunkMesh) -> Option<Aabb> {
    Aabb::enclosing(cm.positions.iter().map(|p| Vec3::from_array(*p)))
}

/// Moves the arrays into a Bevy mesh (no copies). CPU data is dropped after GPU upload. The
/// emissive channel is packed into the colour alpha (see the module docs).
pub fn chunk_mesh_into_bevy(mut cm: ChunkMesh) -> Mesh {
    if cm.emissive.len() == cm.colors.len() {
        for (c, e) in cm.colors.iter_mut().zip(&cm.emissive) {
            c[3] = e.clamp(0.0, 1.0);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, cm.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, cm.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, cm.colors)
        .with_inserted_indices(Indices::U32(cm.indices))
}

const VOXEL_SHADER: Handle<Shader> = uuid_handle!("6c1f3a50-8e2b-4d71-9c3e-5b0f7a2d9e11");

/// Neon glow extension of the terrain material.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct VoxelGlow {
    /// x = neon boost (map), y = glow gain, zw unused.
    #[uniform(100)]
    pub params: Vec4,
}

/// Emission gain on top of `neon_boost` (neon base colour × boost × gain, raw HDR).
pub const NEON_GAIN: f32 = 1.6;

impl MaterialExtension for VoxelGlow {
    fn fragment_shader() -> ShaderRef {
        VOXEL_SHADER.into()
    }
}

pub type VoxelMaterial = ExtendedMaterial<StandardMaterial, VoxelGlow>;

#[derive(Resource, Clone)]
pub struct VoxelMaterials {
    /// Shared vertex-colour terrain material.
    pub opaque: Handle<VoxelMaterial>,
}

/// Render entity of one chunk.
#[derive(Component, Debug, Clone, Copy)]
pub struct VoxelChunk {
    pub key: IVec3,
}

pub type MeshJob = Box<dyn FnOnce() -> ChunkMesh + Send + Sync + 'static>;

struct Pending {
    origin: Vec3,
    /// Priority position (chunk centre, world space).
    center: Vec3,
    generation: u64,
    job: MeshJob,
}

struct InFlight {
    key: IVec3,
    origin: Vec3,
    generation: u64,
    task: Task<Option<(Mesh, Aabb)>>,
}

struct Ready {
    key: IVec3,
    origin: Vec3,
    generation: u64,
    result: Option<(Mesh, Aabb)>,
}

#[derive(Resource)]
pub struct RemeshQueue {
    pending: HashMap<IVec3, Pending>,
    in_flight: Vec<InFlight>,
    ready: VecDeque<Ready>,
    removals: HashSet<IVec3>,
    generations: HashMap<IVec3, u64>,
    /// Chunk render entities by key.
    pub entities: HashMap<IVec3, Entity>,
    /// Max meshing tasks running at once.
    pub max_in_flight: usize,
    /// Max finished meshes swapped in per frame (bounds main-thread + upload cost).
    pub max_uploads_per_frame: usize,
    /// Jobs nearest to this point start first (set to the camera position each frame).
    pub focus: Vec3,
    /// Total meshes uploaded (diagnostics).
    pub uploaded: u64,
}

impl Default for RemeshQueue {
    fn default() -> Self {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
        Self {
            pending: default(),
            in_flight: default(),
            ready: default(),
            removals: default(),
            generations: default(),
            entities: default(),
            max_in_flight: threads.clamp(2, 16),
            max_uploads_per_frame: 12,
            focus: Vec3::ZERO,
            uploaded: 0,
        }
    }
}

impl RemeshQueue {
    /// Queue a meshing job for chunk `key`. The render entity is placed at `translation` (use
    /// `Vec3::ZERO` when the mesher emits world-space positions, as bd_voxel does); `center` is the
    /// chunk's world-space centre, used to mesh nearest-to-camera first.
    pub fn submit(
        &mut self,
        key: IVec3,
        translation: Vec3,
        center: Vec3,
        job: impl FnOnce() -> ChunkMesh + Send + Sync + 'static,
    ) {
        let generation = self.generations.entry(key).or_insert(0);
        *generation += 1;
        self.removals.remove(&key);
        self.pending.insert(key, Pending { origin: translation, center, generation: *generation, job: Box::new(job) });
    }

    /// Despawn chunk `key`'s render entity and drop any queued work for it.
    pub fn remove(&mut self, key: IVec3) {
        *self.generations.entry(key).or_insert(0) += 1;
        self.pending.remove(&key);
        self.removals.insert(key);
    }

    /// Nothing queued, running or waiting for upload.
    pub fn is_idle(&self) -> bool {
        self.pending.is_empty() && self.in_flight.is_empty() && self.ready.is_empty() && self.removals.is_empty()
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len() + self.in_flight.len() + self.ready.len()
    }
}

pub struct VoxelRenderPlugin;

impl Plugin for VoxelRenderPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, VOXEL_SHADER, "voxel.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<VoxelMaterial>::default())
            .init_resource::<RemeshQueue>()
            .add_systems(Startup, create_materials)
            .add_systems(PostUpdate, drive_remesh_queue)
            .add_systems(Update, sync_neon_boost);
    }
}

fn create_materials(mut commands: Commands, mut materials: ResMut<Assets<VoxelMaterial>>) {
    let opaque = materials.add(VoxelMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.92,
            metallic: 0.0,
            reflectance: 0.25,
            ..default()
        },
        extension: VoxelGlow { params: Vec4::new(0.0, NEON_GAIN, 0.0, 0.0) },
    });
    commands.insert_resource(VoxelMaterials { opaque });
}

/// The loaded map's neon boost → the terrain material (only when the map changes).
fn sync_neon_boost(arena: Option<Res<crate::arena::ArenaWorld>>, vm: Option<Res<VoxelMaterials>>, mut materials: ResMut<Assets<VoxelMaterial>>) {
    let (Some(arena), Some(vm)) = (arena, vm) else { return };
    if !arena.is_changed() {
        return;
    }
    let boost = arena.info.neon_boost.max(0.0);
    if materials.get(&vm.opaque).is_some_and(|m| m.extension.params.x != boost)
        && let Some(mut m) = materials.get_mut(&vm.opaque)
    {
        m.extension.params.x = boost;
    }
}

fn drive_remesh_queue(
    mut commands: Commands,
    mut queue: ResMut<RemeshQueue>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Option<Res<VoxelMaterials>>,
    mesh_handles: Query<&Mesh3d, With<VoxelChunk>>,
) {
    let Some(materials) = materials else { return };
    let queue = &mut *queue;

    // 1. Removals.
    for key in queue.removals.drain() {
        if let Some(e) = queue.entities.remove(&key) {
            if let Ok(h) = mesh_handles.get(e) {
                meshes.remove(&h.0);
            }
            commands.entity(e).despawn();
        }
    }

    // 2. Collect finished tasks (drop superseded ones).
    let mut i = 0;
    while i < queue.in_flight.len() {
        if let Some(result) = check_ready(&mut queue.in_flight[i].task) {
            let f = queue.in_flight.swap_remove(i);
            if queue.generations.get(&f.key).copied() == Some(f.generation) {
                queue.ready.push_back(Ready { key: f.key, origin: f.origin, generation: f.generation, result });
            }
        } else {
            i += 1;
        }
    }

    // 3. Start new tasks, nearest to the focus point first.
    let pool = AsyncComputeTaskPool::get();
    while queue.in_flight.len() < queue.max_in_flight && !queue.pending.is_empty() {
        let focus = queue.focus;
        let key = *queue
            .pending
            .iter()
            .min_by(|a, b| {
                a.1.center.distance_squared(focus).total_cmp(&b.1.center.distance_squared(focus))
            })
            .map(|(k, _)| k)
            .unwrap();
        let p = queue.pending.remove(&key).unwrap();
        let job = p.job;
        let task = pool.spawn(async move {
            let cm = job();
            if cm.is_empty() {
                return None;
            }
            let aabb = chunk_mesh_aabb(&cm)?;
            Some((chunk_mesh_into_bevy(cm), aabb))
        });
        queue.in_flight.push(InFlight { key, origin: p.origin, generation: p.generation, task });
    }

    // 4. Upload a bounded number of results.
    for _ in 0..queue.max_uploads_per_frame {
        let Some(r) = queue.ready.pop_front() else { break };
        if queue.generations.get(&r.key).copied() != Some(r.generation) {
            continue; // superseded or removed while waiting
        }
        queue.uploaded += 1;
        match (r.result, queue.entities.get(&r.key).copied()) {
            (None, Some(e)) => {
                if let Ok(h) = mesh_handles.get(e) {
                    meshes.remove(&h.0);
                }
                commands.entity(e).despawn();
                queue.entities.remove(&r.key);
            }
            (None, None) => {}
            (Some((mesh, aabb)), Some(e)) => {
                if let Ok(h) = mesh_handles.get(e) {
                    meshes.remove(&h.0);
                }
                commands.entity(e).insert((Mesh3d(meshes.add(mesh)), aabb, Transform::from_translation(r.origin)));
            }
            (Some((mesh, aabb)), None) => {
                let e = commands
                    .spawn((
                        VoxelChunk { key: r.key },
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(materials.opaque.clone()),
                        Transform::from_translation(r.origin),
                        aabb,
                        NoAutoAabb,
                    ))
                    .id();
                queue.entities.insert(r.key, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supersede_bumps_generation() {
        let mut q = RemeshQueue::default();
        q.submit(IVec3::ZERO, Vec3::ZERO, Vec3::ZERO, ChunkMesh::default);
        q.submit(IVec3::ZERO, Vec3::ZERO, Vec3::ZERO, ChunkMesh::default);
        assert_eq!(q.pending.len(), 1);
        assert_eq!(q.pending[&IVec3::ZERO].generation, 2);
        q.remove(IVec3::ZERO);
        assert!(q.pending.is_empty());
        assert!(!q.is_idle());
    }
}
