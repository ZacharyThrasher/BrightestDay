//! The look: bright, saturated, cheap.
//!
//! * HDR camera + bloom with a threshold (only emissive hard light and the sun glow bloom; sunlit
//!   terrain stays crisp), `KhronosPbrNeutral` tonemapping (keeps saturated base colours, whitens
//!   emissive cores) + a touch of post saturation. F4 cycles tonemappers for comparison.
//! * Gradient sky dome ([`sky`]) whose horizon colour is also the distance-fog colour.
//! * One directional sun, 4 shadow cascades out to ~200 m, soft sky-tinted ambient.
//! * MSAA 4× by default (FXAA / off selectable). **No TAA** (no smearing in a fast game).
//! * Voxel chunks: [`voxel_render`].

pub mod sky;
pub mod tethers;
pub mod voxel_render;

use crate::camera::PlayerCamera;
use crate::settings::{AntiAliasing, Settings};
use bevy::anti_alias::fxaa::Fxaa;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, GlobalAmbientLight};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use bevy::camera::Hdr;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, Msaa};

/// Shared environment parameters (sky, fog and sun agree on these).
#[derive(Resource, Debug, Clone)]
pub struct Environment {
    /// Unit vector pointing *toward* the sun.
    pub sun_dir: Vec3,
    pub sun_color: Color,
    pub sun_lux: f32,
    pub ambient_color: Color,
    pub ambient_brightness: f32,
    /// Linear HDR sky colours.
    pub zenith: LinearRgba,
    pub horizon: LinearRgba,
    pub ground: LinearRgba,
    pub fog_color: LinearRgba,
    /// Distance at which fog is ~fully opaque.
    pub fog_visibility: f32,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            sun_dir: Vec3::new(0.45, 0.78, 0.32).normalize(),
            sun_color: Color::srgb(1.0, 0.96, 0.88),
            sun_lux: 3200.0,
            ambient_color: Color::srgb(0.72, 0.82, 1.0),
            ambient_brightness: 520.0,
            zenith: LinearRgba::rgb(0.10, 0.30, 0.85),
            horizon: LinearRgba::rgb(0.62, 0.80, 1.00),
            ground: LinearRgba::rgb(0.30, 0.42, 0.62),
            fog_color: LinearRgba::rgb(0.55, 0.72, 0.95),
            fog_visibility: 520.0,
        }
    }
}

pub struct RenderSetupPlugin;

impl Plugin for RenderSetupPlugin {
    fn build(&self, app: &mut App) {
        let env = Environment::default();
        app.insert_resource(DirectionalLightShadowMap { size: 2048 })
            .insert_resource(GlobalAmbientLight {
                color: env.ambient_color,
                brightness: env.ambient_brightness,
                ..default()
            })
            .insert_resource(ClearColor(Color::LinearRgba(env.horizon)))
            .insert_resource(env)
            .add_plugins((sky::SkyPlugin, voxel_render::VoxelRenderPlugin, tethers::TetherRenderPlugin))
            .add_systems(Startup, spawn_sun)
            .add_systems(Update, (setup_camera_look, apply_aa_setting, cycle_tonemapper, apply_environment));
    }
}

#[derive(Component)]
pub struct Sun;

/// Re-applies [`Environment`] to the sun, ambient, clear colour and fog when it changes
/// (e.g. when the arena's `ArenaInfo` arrives). The sky dome updates itself.
fn apply_environment(
    env: Res<Environment>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut sun: Query<(&mut Transform, &mut DirectionalLight), With<Sun>>,
    mut fog: Query<&mut DistanceFog, With<PlayerCamera>>,
) {
    if !env.is_changed() || env.is_added() {
        return;
    }
    ambient.color = env.ambient_color;
    ambient.brightness = env.ambient_brightness;
    clear.0 = Color::LinearRgba(env.horizon);
    for (mut t, mut l) in &mut sun {
        *t = Transform::from_translation(env.sun_dir * 100.0).looking_at(Vec3::ZERO, Vec3::Y);
        l.color = env.sun_color;
        l.illuminance = env.sun_lux;
    }
    for mut f in &mut fog {
        f.color = Color::LinearRgba(env.fog_color);
        f.directional_light_color = env.sun_color.with_alpha(0.35);
        f.falloff = FogFalloff::from_visibility_squared(env.fog_visibility);
    }
}

fn spawn_sun(mut commands: Commands, env: Res<Environment>, cli: Res<crate::cli::CliArgs>) {
    commands.spawn((
        Name::new("Sun"),
        Sun,
        DirectionalLight {
            color: env.sun_color,
            illuminance: env.sun_lux,
            shadow_maps_enabled: !cli.no_shadows,
            ..default()
        },
        // Light shines along its -Z: look from the sun toward the origin.
        Transform::from_translation(env.sun_dir * 100.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            minimum_distance: 0.1,
            first_cascade_far_bound: 14.0,
            maximum_distance: 200.0,
            overlap_proportion: 0.25,
        }
        .build(),
    ));
}

/// Adds the render look to the gameplay camera once it exists.
fn setup_camera_look(
    mut commands: Commands,
    env: Res<Environment>,
    settings: Res<Settings>,
    cams: Query<Entity, Added<PlayerCamera>>,
) {
    for e in &cams {
        commands.entity(e).insert((
            Hdr,
            Tonemapping::KhronosPbrNeutral,
            DebandDither::Enabled,
            ColorGrading {
                global: ColorGradingGlobal { post_saturation: 1.04, ..default() },
                ..default()
            },
            Bloom {
                intensity: 0.22,
                low_frequency_boost: 0.6,
                prefilter: BloomPrefilter { threshold: 1.0, threshold_softness: 0.35 },
                ..Bloom::NATURAL
            },
            DistanceFog {
                color: Color::LinearRgba(env.fog_color),
                directional_light_color: env.sun_color.with_alpha(0.35),
                directional_light_exponent: 24.0,
                falloff: FogFalloff::from_visibility_squared(env.fog_visibility),
            },
            aa_components(settings.antialiasing),
        ));
    }
}

fn aa_components(aa: AntiAliasing) -> (Msaa, Fxaa) {
    match aa {
        AntiAliasing::Off => (Msaa::Off, Fxaa { enabled: false, ..default() }),
        AntiAliasing::Fxaa => (Msaa::Off, Fxaa { enabled: true, ..default() }),
        AntiAliasing::Msaa4 => (Msaa::Sample4, Fxaa { enabled: false, ..default() }),
    }
}

fn apply_aa_setting(
    mut commands: Commands,
    settings: Res<Settings>,
    cams: Query<Entity, With<PlayerCamera>>,
) {
    if !settings.is_changed() || settings.is_added() {
        return;
    }
    for e in &cams {
        commands.entity(e).insert(aa_components(settings.antialiasing));
    }
}

/// Dev: F4 cycles tonemappers (look comparison).
fn cycle_tonemapper(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut Tonemapping, With<PlayerCamera>>) {
    if !keys.just_pressed(KeyCode::F4) {
        return;
    }
    const ORDER: [Tonemapping; 5] = [
        Tonemapping::KhronosPbrNeutral,
        Tonemapping::TonyMcMapface,
        Tonemapping::AgX,
        Tonemapping::AcesFitted,
        Tonemapping::SomewhatBoringDisplayTransform,
    ];
    for mut t in &mut q {
        let i = ORDER.iter().position(|o| *o == *t).unwrap_or(0);
        *t = ORDER[(i + 1) % ORDER.len()];
        info!("tonemapping: {:?}", *t);
    }
}
