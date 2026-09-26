//! Instanced debris rendering.
//!
//! One storage buffer holds [`DEBRIS_CAPACITY`] instances (48 B each) and is rewritten each frame
//! (`queue.write_buffer` into the same GPU buffer — no reallocation, no bind-group churn). The pool
//! is drawn by `DEBRIS_CAPACITY / BATCH` entities that share one mesh (a batch of `BATCH` unit
//! cubes) and one material; each entity's `MeshTag` is its first instance index, so Bevy's
//! automatic batching turns the whole pool into a single multi-draw. Batches past the live count
//! are hidden; batch 0 is always visible (keeps the pipeline warm and compiled at startup).

use super::{DebrisPool, Particle, DEBRIS_CAPACITY};
use crate::tick::FrameSet;
use bevy::asset::{load_internal_asset, uuid_handle, RenderAssetUsages};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{Indices, MeshTag, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use bytemuck::{Pod, Zeroable};

const DEBRIS_SHADER: Handle<Shader> = uuid_handle!("7a4c9f3e-1c1d-4f55-8b7e-5d6a2f9e3b10");
pub const BATCH: usize = 1024;
const _: () = assert!(DEBRIS_CAPACITY % BATCH == 0);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, PartialEq)]
pub struct DebrisInstance {
    /// xyz centre, w edge length.
    pub pos_size: [f32; 4],
    /// Quaternion xyzw.
    pub rot: [f32; 4],
    /// Linear rgb + emissive strength.
    pub color: [f32; 4],
}

impl DebrisInstance {
    pub const HIDDEN: Self = Self { pos_size: [0.0; 4], rot: [0.0, 0.0, 0.0, 1.0], color: [0.0; 4] };

    #[inline]
    pub(super) fn from_particle(p: &Particle, fade_secs: f32) -> Self {
        let remaining = (p.life - p.age).max(0.0);
        let fade_len = fade_secs.min(p.life * 0.35).max(1e-3);
        let t = (remaining / fade_len).min(1.0);
        let fade = t * t * (3.0 - 2.0 * t);
        // Brief white-hot flash when a glowing shard is born.
        let birth = 1.0 + 1.5 * (-p.age * 18.0).exp();
        Self {
            pos_size: [p.pos.x, p.pos.y, p.pos.z, p.size * fade],
            rot: p.rot.to_array(),
            color: [p.color[0], p.color[1], p.color[2], p.emissive * fade * birth],
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct DebrisMaterial {
    #[storage(100, read_only)]
    pub instances: Handle<ShaderBuffer>,
}

impl Material for DebrisMaterial {
    fn vertex_shader() -> ShaderRef {
        DEBRIS_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        DEBRIS_SHADER.into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        DEBRIS_SHADER.into()
    }
}

#[derive(Resource)]
pub struct DebrisRenderHandles {
    pub buffer: Handle<ShaderBuffer>,
    pub material: Handle<DebrisMaterial>,
    pub mesh: Handle<Mesh>,
}

#[derive(Component)]
pub struct DebrisBatch(pub usize);

pub struct DebrisRenderPlugin;

impl Plugin for DebrisRenderPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, DEBRIS_SHADER, "debris.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<DebrisMaterial>::default())
            .add_systems(Startup, setup_debris_render)
            .add_systems(Update, upload_debris.in_set(FrameSet::Present).after(super::simulate_debris));
    }
}

/// `BATCH` unit cubes in one mesh; UV.x = cube index within the batch.
fn batch_mesh() -> Mesh {
    const FACES: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        // normal, u axis, v axis
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let mut positions = Vec::with_capacity(BATCH * 24);
    let mut normals = Vec::with_capacity(BATCH * 24);
    let mut uvs = Vec::with_capacity(BATCH * 24);
    let mut indices = Vec::with_capacity(BATCH * 36);
    for cube in 0..BATCH {
        for (n, u, v) in FACES {
            let (n, u, v) = (Vec3::from(n), Vec3::from(u), Vec3::from(v));
            let base = positions.len() as u32;
            for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let p = (n + u * su + v * sv) * 0.5;
                positions.push(p.to_array());
                normals.push(n.to_array());
                uvs.push([cube as f32, 0.0]);
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

fn setup_debris_render(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<DebrisMaterial>>,
) {
    let initial = vec![DebrisInstance::HIDDEN; DEBRIS_CAPACITY];
    let mut sb = ShaderBuffer::new(bytemuck::cast_slice(&initial), RenderAssetUsages::RENDER_WORLD);
    sb.buffer_description.label = Some("debris_instances");
    let buffer = buffers.add(sb);
    let material = materials.add(DebrisMaterial { instances: buffer.clone() });
    let mesh = meshes.add(batch_mesh());
    for b in 0..DEBRIS_CAPACITY / BATCH {
        commands.spawn((
            Name::new(format!("DebrisBatch{b}")),
            DebrisBatch(b),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            MeshTag((b * BATCH) as u32),
            Transform::IDENTITY,
            NoFrustumCulling,
            if b == 0 { Visibility::Visible } else { Visibility::Hidden },
        ));
    }
    commands.insert_resource(DebrisRenderHandles { buffer, material, mesh });
}

fn upload_debris(
    pool: Res<DebrisPool>,
    handles: Res<DebrisRenderHandles>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut batches: Query<(&DebrisBatch, &mut Visibility)>,
    mut last_live: Local<usize>,
) {
    let live = pool.live();
    // Nothing alive now or last frame: the GPU buffer is already all-hidden.
    if live == 0 && *last_live == 0 {
        return;
    }
    *last_live = live;
    if let Some(mut buf) = buffers.get_mut(&handles.buffer) {
        buf.data = Some(bytemuck::cast_slice(pool.instances()).to_vec());
    }
    let visible_batches = live.div_ceil(BATCH).max(1);
    for (b, mut vis) in &mut batches {
        let want = if b.0 < visible_batches { Visibility::Visible } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}
