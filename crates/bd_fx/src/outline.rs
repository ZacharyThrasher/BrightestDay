//! Player outlines (readability in chaos): an inverted-hull pass for bd_fx voxel part meshes.
//!
//! * [`OutlineMaterial`] (WGSL `outline.wgsl`): each voxel face is inflated around its own voxel by
//!   a constant *pixel* width (computed per vertex from the view depth), back faces only, opaque,
//!   depth-tested (line of sight only — never through walls). One material shared by every hull;
//!   per-slot colours / widths come from one storage buffer indexed by the entity's `MeshTag`
//!   (`slot << 1 | layer`), so every hull batches into the same pipeline + bind group.
//! * Two hulls per part: [`OutlineLayer::Inner`] (thin dark line hugging the silhouette — hard-light
//!   constructs never have it, so a body reads apart from same-colour glow) and
//!   [`OutlineLayer::Outer`] (corps colour, readable against dark backgrounds). Colours stay below
//!   the camera's bloom threshold: the line never smears.
//! * The game fills [`OutlineUpload::slots`] each frame (reused `Vec`, only the used range is
//!   uploaded with `RenderQueue::write_buffer`, like `gpu.rs`) and spawns hull children with
//!   [`OutlineAssets::hull`].
//!
//! Add [`OutlinePlugin`] next to `BdFxPlugin`.

use bevy::asset::{RenderAssetUsages, load_internal_asset, uuid_handle};
use bevy::light::NotShadowCaster;
use bevy::mesh::{MeshTag, MeshVertexBufferLayoutRef};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::render::renderer::RenderQueue;
use bevy::render::storage::{GpuShaderBuffer, ShaderBuffer};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::shader::ShaderRef;
use bytemuck::{Pod, Zeroable};

pub const OUTLINE_SHADER: Handle<Shader> = uuid_handle!("b1d0f7a2-3c55-4e61-9a2e-0c7d4f1e5a31");

/// Outline param slots (one per player id).
pub const MAX_OUTLINE_SLOTS: usize = 256;

/// Per-slot shader params (std430, 48 B).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, PartialEq)]
pub struct OutlineGpu {
    /// Outer line colour (linear) + outer edge distance from the silhouette (px).
    pub outer: [f32; 4],
    /// Inner line colour (linear) + inner line width (px).
    pub inner: [f32; 4],
    /// Dash period (px, 0 = solid), duty (lit fraction), dash angle (rad), fog amount (0..1).
    pub pattern: [f32; 4],
}

/// Which hull of a part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutlineLayer {
    Inner = 0,
    Outer = 1,
}

/// `MeshTag` of a hull: slot + layer.
#[inline]
pub fn outline_tag(slot: u32, layer: OutlineLayer) -> u32 {
    (slot.min(MAX_OUTLINE_SLOTS as u32 - 1) << 1) | layer as u32
}

/// The hull material (one shared instance).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct OutlineMaterial {
    #[storage(0, read_only)]
    pub outlines: Handle<ShaderBuffer>,
}

impl Material for OutlineMaterial {
    fn vertex_shader() -> ShaderRef {
        OUTLINE_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        OUTLINE_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Inverted hull: draw the far side only.
        descriptor.primitive.cull_mode = Some(Face::Front);
        Ok(())
    }
}

/// Shared handles.
#[derive(Resource, Clone)]
pub struct OutlineAssets {
    pub material: Handle<OutlineMaterial>,
    pub buffer: Handle<ShaderBuffer>,
}

impl OutlineAssets {
    /// Components of one hull entity for `mesh` (spawn it as a child of the part entity, identity
    /// transform, so it follows the part's pose and visibility).
    pub fn hull(&self, mesh: Handle<Mesh>, slot: u32, layer: OutlineLayer) -> impl Bundle {
        (Mesh3d(mesh), MeshMaterial3d(self.material.clone()), MeshTag(outline_tag(slot, layer)), Transform::IDENTITY, NotShadowCaster)
    }
}

/// Main-world staging: `slots[i]` = params of slot `i` (the game resizes/fills it every frame).
#[derive(Resource, Clone, Default)]
pub struct OutlineUpload {
    pub slots: Vec<OutlineGpu>,
}

impl OutlineUpload {
    pub fn set(&mut self, slot: u32, data: OutlineGpu) {
        let i = (slot as usize).min(MAX_OUTLINE_SLOTS - 1);
        if self.slots.len() <= i {
            self.slots.resize(i + 1, OutlineGpu::default());
        }
        self.slots[i] = data;
    }
}

#[derive(Resource, Clone, Default)]
struct RenderOutlineUpload(Vec<OutlineGpu>);

pub struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, OUTLINE_SHADER, "shaders/outline.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<OutlineMaterial>::default());
        let buffer = {
            let mut sb = ShaderBuffer::with_size(MAX_OUTLINE_SLOTS * size_of::<OutlineGpu>(), RenderAssetUsages::RENDER_WORLD);
            sb.buffer_description.label = Some("bd_fx_outlines");
            app.world_mut().resource_mut::<Assets<ShaderBuffer>>().add(sb)
        };
        let material = app.world_mut().resource_mut::<Assets<OutlineMaterial>>().add(OutlineMaterial { outlines: buffer.clone() });
        app.insert_resource(OutlineAssets { material, buffer }).init_resource::<OutlineUpload>();
    }

    fn finish(&self, app: &mut App) {
        let assets = app.world().resource::<OutlineAssets>().clone();
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .insert_resource(assets)
            .init_resource::<RenderOutlineUpload>()
            .add_systems(ExtractSchedule, extract_outlines)
            .add_systems(Render, write_outlines.in_set(RenderSystems::PrepareResources));
    }
}

fn extract_outlines(src: Extract<Res<OutlineUpload>>, mut dst: ResMut<RenderOutlineUpload>) {
    dst.0.clone_from(&src.slots);
}

fn write_outlines(
    up: Res<RenderOutlineUpload>,
    assets: Res<OutlineAssets>,
    gpu: Res<RenderAssets<GpuShaderBuffer>>,
    queue: Res<RenderQueue>,
) {
    if up.0.is_empty() {
        return;
    }
    if let Some(b) = gpu.get(&assets.buffer) {
        let n = up.0.len().min(MAX_OUTLINE_SLOTS);
        queue.write_buffer(&b.buffer, 0, bytemuck::cast_slice(&up.0[..n]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_layout_is_std430() {
        assert_eq!(size_of::<OutlineGpu>(), 48);
    }

    #[test]
    fn tags_pack_slot_and_layer() {
        assert_eq!(outline_tag(0, OutlineLayer::Inner), 0);
        assert_eq!(outline_tag(3, OutlineLayer::Outer), 7);
        assert_eq!(outline_tag(10_000, OutlineLayer::Outer) >> 1, MAX_OUTLINE_SLOTS as u32 - 1);
        let mut up = OutlineUpload::default();
        up.set(5, OutlineGpu { outer: [1.0; 4], ..default() });
        assert_eq!(up.slots.len(), 6);
        assert_eq!(up.slots[5].outer, [1.0; 4]);
    }
}
