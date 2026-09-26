//! Gradient sky dome that follows the camera.

use super::Environment;
use crate::camera::{CameraUpdateSet, PlayerCamera};
use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

const SKY_SHADER: Handle<Shader> = uuid_handle!("3c7f5f43-4a0e-4a53-9f0e-6f1f0b1d5a01");
const SKY_RADIUS: f32 = 1500.0;

#[derive(Clone, Copy, Debug, ShaderType)]
pub struct SkyParams {
    pub zenith: Vec4,
    pub horizon: Vec4,
    pub ground: Vec4,
    pub sun_dir: Vec4,
    pub sun_color: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct SkyMaterial {
    #[uniform(0)]
    pub params: SkyParams,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        SKY_SHADER.into()
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
        // Seen from inside.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Component)]
pub struct SkyDome;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SKY_SHADER, "sky.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<SkyMaterial>::default())
            .add_systems(Startup, spawn_sky)
            .add_systems(Update, update_sky_params)
            .add_systems(PostUpdate, follow_camera.after(CameraUpdateSet).before(bevy::transform::TransformSystems::Propagate));
    }
}

fn to_vec4(c: LinearRgba) -> Vec4 {
    Vec4::new(c.red, c.green, c.blue, 1.0)
}

fn sky_params(env: &Environment) -> SkyParams {
    let sun = env.sun_color.to_linear();
    SkyParams {
        zenith: to_vec4(env.zenith),
        horizon: to_vec4(env.horizon),
        ground: to_vec4(env.ground),
        sun_dir: env.sun_dir.extend(900.0),
        sun_color: Vec4::new(sun.red, sun.green, sun.blue, 0.35),
    }
}

fn update_sky_params(
    env: Res<Environment>,
    q: Query<&MeshMaterial3d<SkyMaterial>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    if !env.is_changed() || env.is_added() {
        return;
    }
    for h in &q {
        if let Some(mut m) = materials.get_mut(&h.0) {
            m.params = sky_params(&env);
        }
    }
}

fn spawn_sky(
    mut commands: Commands,
    env: Res<Environment>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    let params = sky_params(&env);
    commands.spawn((
        Name::new("SkyDome"),
        SkyDome,
        Mesh3d(meshes.add(Sphere::new(SKY_RADIUS).mesh().uv(48, 24))),
        MeshMaterial3d(materials.add(SkyMaterial { params })),
        Transform::IDENTITY,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn follow_camera(
    cam: Query<&Transform, (With<PlayerCamera>, Without<SkyDome>)>,
    mut sky: Query<&mut Transform, With<SkyDome>>,
) {
    let Ok(cam) = cam.single() else { return };
    for mut t in &mut sky {
        t.translation = cam.translation;
    }
}
