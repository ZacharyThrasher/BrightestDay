//! Materials: hard-light construct surface, analytic particle cubes, portal swirl.

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;

use crate::gpu::FxBuffers;

pub const COMMON_SHADER: Handle<Shader> = uuid_handle!("b1d0f7a2-3c55-4e61-9a2e-0c7d4f1e5a01");
pub const CONSTRUCT_SHADER: Handle<Shader> = uuid_handle!("b1d0f7a2-3c55-4e61-9a2e-0c7d4f1e5a02");
pub const PARTICLE_SHADER: Handle<Shader> = uuid_handle!("b1d0f7a2-3c55-4e61-9a2e-0c7d4f1e5a03");
pub const PORTAL_SHADER: Handle<Shader> = uuid_handle!("b1d0f7a2-3c55-4e61-9a2e-0c7d4f1e5a04");

/// Pipeline variant of [`ConstructMaterial`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ConstructMaterialKey {
    pub translucent: bool,
    pub overlay: bool,
}

/// The hard-light construct surface. One instance per variant is shared by every construct part;
/// per-construct values come from the `constructs` storage buffer indexed by the entity's `MeshTag`.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(ConstructMaterialKey)]
pub struct ConstructMaterial {
    #[storage(0, read_only)]
    pub constructs: Handle<ShaderBuffer>,
    pub translucent: bool,
    /// Drawn in front of the world (depth remapped to the near range): screen-anchored text.
    pub overlay: bool,
}

impl From<&ConstructMaterial> for ConstructMaterialKey {
    fn from(m: &ConstructMaterial) -> Self {
        ConstructMaterialKey { translucent: m.translucent, overlay: m.overlay }
    }
}

impl Material for ConstructMaterial {
    fn vertex_shader() -> ShaderRef {
        CONSTRUCT_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        CONSTRUCT_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.translucent { AlphaMode::Premultiplied } else { AlphaMode::Opaque }
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
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = Some(Face::Back);
        if key.bind_group_data.overlay {
            descriptor.vertex.shader_defs.push("FX_OVERLAY".into());
        }
        if key.bind_group_data.translucent {
            // Depth-write the front shell so a part never shows its own far side (stable look while
            // other, farther parts still show through — sorted back to front per part entity).
            if let Some(ds) = descriptor.depth_stencil.as_mut() {
                ds.depth_write_enabled = Some(true);
            }
        }
        Ok(())
    }
}

/// Analytic particle cubes (opaque, unlit/emissive; smoke flag = sun-lit grey).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ParticleMaterial {
    #[storage(0, read_only)]
    pub particles: Handle<ShaderBuffer>,
    #[storage(1, read_only)]
    pub anchors: Handle<ShaderBuffer>,
}

impl Material for ParticleMaterial {
    fn vertex_shader() -> ShaderRef {
        PARTICLE_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        PARTICLE_SHADER.into()
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
}

/// Portal inner surface (swirl). Per-portal colour comes from the construct buffer via `MeshTag`.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct PortalMaterial {
    #[storage(0, read_only)]
    pub constructs: Handle<ShaderBuffer>,
}

impl Material for PortalMaterial {
    fn vertex_shader() -> ShaderRef {
        PORTAL_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        PORTAL_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
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
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Shared material handles.
#[derive(Resource, Clone)]
pub struct FxMaterials {
    pub construct_translucent: Handle<ConstructMaterial>,
    pub construct_solid: Handle<ConstructMaterial>,
    /// Solid, always in front of the world (performance words).
    pub construct_overlay: Handle<ConstructMaterial>,
    pub particles: Handle<ParticleMaterial>,
    pub portal: Handle<PortalMaterial>,
}

impl FxMaterials {
    pub fn construct(&self, translucent: bool) -> Handle<ConstructMaterial> {
        if translucent { self.construct_translucent.clone() } else { self.construct_solid.clone() }
    }
}

pub fn build(app: &mut App) {
    load_internal_asset!(app, COMMON_SHADER, "shaders/common.wgsl", Shader::from_wgsl);
    load_internal_asset!(app, CONSTRUCT_SHADER, "shaders/construct.wgsl", Shader::from_wgsl);
    load_internal_asset!(app, PARTICLE_SHADER, "shaders/particles.wgsl", Shader::from_wgsl);
    load_internal_asset!(app, PORTAL_SHADER, "shaders/portal.wgsl", Shader::from_wgsl);
    app.add_plugins((
        MaterialPlugin::<ConstructMaterial>::default(),
        MaterialPlugin::<ParticleMaterial>::default(),
        MaterialPlugin::<PortalMaterial>::default(),
    ));
}

/// Create the shared materials (needs [`FxBuffers`]; called from `particles::build` after gpu).
pub fn create(world: &mut World) -> FxMaterials {
    let bufs = world.resource::<FxBuffers>().clone();
    let (construct_translucent, construct_solid, construct_overlay) = {
        let mut mats = world.resource_mut::<Assets<ConstructMaterial>>();
        (
            mats.add(ConstructMaterial { constructs: bufs.constructs.clone(), translucent: true, overlay: false }),
            mats.add(ConstructMaterial { constructs: bufs.constructs.clone(), translucent: false, overlay: false }),
            mats.add(ConstructMaterial { constructs: bufs.constructs.clone(), translucent: false, overlay: true }),
        )
    };
    let particles = world
        .resource_mut::<Assets<ParticleMaterial>>()
        .add(ParticleMaterial { particles: bufs.particles.clone(), anchors: bufs.anchors.clone() });
    let portal = world.resource_mut::<Assets<PortalMaterial>>().add(PortalMaterial { constructs: bufs.constructs.clone() });
    FxMaterials { construct_translucent, construct_solid, construct_overlay, particles, portal }
}

