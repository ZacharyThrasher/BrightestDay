//! GPU-side data: three storage buffers owned by bd_fx (construct params, anchors, particles) and the
//! allocation-free upload path.
//!
//! The buffers are `ShaderBuffer` assets created once with a fixed size and **no CPU data**, so the
//! material bind groups never change. Each frame the main world fills [`FxUpload`] (reused `Vec`s);
//! it is copied to the render world in `ExtractSchedule` (`clone_from`, capacity reused) and written
//! with `RenderQueue::write_buffer` — only the used construct/anchor range and the particles spawned
//! this frame (one or two contiguous ring ranges) are uploaded.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::renderer::RenderQueue;
use bevy::render::storage::{GpuShaderBuffer, ShaderBuffer};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bytemuck::{Pod, Zeroable};

/// Construct-param slots (one per construct / chain / portal).
pub const MAX_SLOTS: usize = 2048;
/// Anchor slots (moving reference frames for particles). Slot 0 = world identity.
pub const MAX_ANCHORS: usize = 2048;
/// Particle cubes per draw batch entity.
pub const BATCH: usize = 1024;
/// Immediate region: rewritten every frame (arcs, lightning, onomatopoeia letters).
pub const IMM_CAP: usize = 8 * BATCH;
/// Analytic ring buffer.
pub const RING_CAP: usize = 64 * BATCH;
pub const PARTICLE_CAP: usize = IMM_CAP + RING_CAP;

/// Per-construct shader params (std430, 112 B).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, PartialEq)]
pub struct ConstructGpu {
    /// Palette rgb (linear) + unused.
    pub primary: [f32; 4],
    pub accent: [f32; 4],
    pub dark: [f32; 4],
    pub core: [f32; 4],
    /// integrity (0..1), reveal (0..1), flash (0..1), dim (0..1)
    pub state: [f32; 4],
    /// voxel size (m), seed, opacity (0..1), mode (0 forming, 1 dissolving)
    pub misc: [f32; 4],
    /// Forming origin (world) + 1/radius.
    pub origin: [f32; 4],
}

/// Anchor: world transform columns + the owning construct's ring position (stream source).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct AnchorGpu {
    pub x: [f32; 4],
    pub y: [f32; 4],
    pub z: [f32; 4],
    pub t: [f32; 4],
    pub ring: [f32; 4],
}

impl Default for AnchorGpu {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl AnchorGpu {
    pub const IDENTITY: AnchorGpu = AnchorGpu {
        x: [1.0, 0.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0, 0.0],
        z: [0.0, 0.0, 1.0, 0.0],
        t: [0.0, 0.0, 0.0, 1.0],
        ring: [0.0; 4],
    };
    pub fn from_affine(a: &bevy::math::Affine3A, ring: Vec3) -> Self {
        let m = a.matrix3;
        let t = a.translation;
        AnchorGpu {
            x: [m.x_axis.x, m.x_axis.y, m.x_axis.z, 0.0],
            y: [m.y_axis.x, m.y_axis.y, m.y_axis.z, 0.0],
            z: [m.z_axis.x, m.z_axis.y, m.z_axis.z, 0.0],
            t: [t.x, t.y, t.z, 1.0],
            ring: [ring.x, ring.y, ring.z, 1.0],
        }
    }
}

/// One analytic particle (80 B). See `particles.wgsl` for the meaning per kind.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, PartialEq)]
pub struct ParticleGpu {
    /// xyz position/offset, w = size (edge, m; 0 = dead)
    pub p0: [f32; 4],
    /// xyz velocity / target / params, w = spawn time (Time::elapsed_secs_wrapped)
    pub v0: [f32; 4],
    /// rgb linear, a = emissive strength
    pub color: [f32; 4],
    /// life, gravity (or rise), drag, spin (or stretch)
    pub a: [f32; 4],
    /// kind | flags << 8, anchor, seed, aux (f32 bits)
    pub b: [u32; 4],
}

/// Main-world upload staging (reused every frame).
#[derive(Resource, Clone, Default)]
pub struct FxUpload {
    pub constructs: Vec<ConstructGpu>,
    pub anchors: Vec<AnchorGpu>,
    /// Immediate region contents (index 0.. in the particle buffer); length = max(used this frame,
    /// used last frame) so stale entries are overwritten with dead particles.
    pub immediate: Vec<ParticleGpu>,
    /// Ring particles spawned this frame, contiguous from ring index `ring_start` (wrapping).
    pub ring: Vec<ParticleGpu>,
    pub ring_start: u32,
}

#[derive(Resource, Clone, Default)]
struct RenderFxUpload(FxUpload);

/// Handles of the three bd_fx storage buffers.
#[derive(Resource, Clone)]
pub struct FxBuffers {
    pub constructs: Handle<ShaderBuffer>,
    pub anchors: Handle<ShaderBuffer>,
    pub particles: Handle<ShaderBuffer>,
}

pub fn build(app: &mut App) {
    let mut buffers = app.world_mut().resource_mut::<Assets<ShaderBuffer>>();
    let mk = |bytes: usize, label: &'static str| {
        let mut sb = ShaderBuffer::with_size(bytes, RenderAssetUsages::RENDER_WORLD);
        sb.buffer_description.label = Some(label);
        sb
    };
    let constructs = buffers.add(mk(MAX_SLOTS * size_of::<ConstructGpu>(), "bd_fx_constructs"));
    let anchors = buffers.add(mk(MAX_ANCHORS * size_of::<AnchorGpu>(), "bd_fx_anchors"));
    let particles = buffers.add(mk(PARTICLE_CAP * size_of::<ParticleGpu>(), "bd_fx_particles"));
    let mut up = FxUpload::default();
    up.anchors.push(AnchorGpu::IDENTITY);
    app.insert_resource(FxBuffers { constructs, anchors, particles }).insert_resource(up).add_systems(First, clear_ring_upload);
}

pub fn finish(app: &mut App) {
    let handles = app.world().resource::<FxBuffers>().clone();
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .insert_resource(handles)
        .init_resource::<RenderFxUpload>()
        .add_systems(ExtractSchedule, extract_upload)
        .add_systems(Render, write_buffers.in_set(RenderSystems::PrepareResources));
}

/// The ring upload is consumed by the extract at the end of the previous frame.
fn clear_ring_upload(mut up: ResMut<FxUpload>) {
    up.ring.clear();
}

fn extract_upload(src: Extract<Res<FxUpload>>, mut dst: ResMut<RenderFxUpload>) {
    let s: &FxUpload = &src;
    let d = &mut dst.0;
    d.constructs.clone_from(&s.constructs);
    d.anchors.clone_from(&s.anchors);
    d.immediate.clone_from(&s.immediate);
    d.ring.clone_from(&s.ring);
    d.ring_start = s.ring_start;
}

fn write_buffers(
    up: Res<RenderFxUpload>,
    handles: Res<FxBuffers>,
    gpu: Res<RenderAssets<GpuShaderBuffer>>,
    queue: Res<RenderQueue>,
) {
    let up = &up.0;
    if let Some(b) = gpu.get(&handles.constructs)
        && !up.constructs.is_empty()
    {
        let n = up.constructs.len().min(MAX_SLOTS);
        queue.write_buffer(&b.buffer, 0, bytemuck::cast_slice(&up.constructs[..n]));
    }
    if let Some(b) = gpu.get(&handles.anchors)
        && !up.anchors.is_empty()
    {
        let n = up.anchors.len().min(MAX_ANCHORS);
        queue.write_buffer(&b.buffer, 0, bytemuck::cast_slice(&up.anchors[..n]));
    }
    if let Some(b) = gpu.get(&handles.particles) {
        let psz = size_of::<ParticleGpu>() as u64;
        if !up.immediate.is_empty() {
            let n = up.immediate.len().min(IMM_CAP);
            queue.write_buffer(&b.buffer, 0, bytemuck::cast_slice(&up.immediate[..n]));
        }
        if !up.ring.is_empty() {
            let n = up.ring.len().min(RING_CAP);
            let start = up.ring_start as usize % RING_CAP;
            let first = n.min(RING_CAP - start);
            queue.write_buffer(
                &b.buffer,
                (IMM_CAP + start) as u64 * psz,
                bytemuck::cast_slice(&up.ring[..first]),
            );
            if first < n {
                queue.write_buffer(&b.buffer, IMM_CAP as u64 * psz, bytemuck::cast_slice(&up.ring[first..n]));
            }
        }
    }
}
