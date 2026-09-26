//! **Orbit** presentation (DESIGN.md "Maps: Orbit"): selling scale cheaply and making the two space
//! blast zones memorable. Active only while the loaded map is `MapKind::Orbit`; everything spawned
//! here carries [`SpaceEntity`] and is torn down when another map loads.
//!
//! * **Sky** — a camera-following dome with `space.wgsl` (the default gradient `SkyDome` is hidden):
//!   three star layers at finite distances (parallax as you move), a nebula band, the planet far
//!   below (oceans / continents / scrolling clouds / day-night terminator with city lights /
//!   fresnel atmosphere rim + halo over the limb), the moon drifting slowly past, the sun.
//! * **Far silhouettes** — a big ring station, a freighter and a satellite, unlit and unfogged at
//!   0.7–1.2 km, drifting, with blinking beacons.
//! * **Zero-g cues** — floating dust motes around the camera in the zero-g band; debris uses the map
//!   gravity profile (`DebrisSettings::gravity_profile`), so shattered voxels drift up there.
//! * **Launched into space (top KO)** — the victim keeps flying up as a glowing, tumbling figure
//!   with a corps trail; if it's *you*, a short spectator shot follows yourself from beside the
//!   launch line (never rolls, eases in from your view), stars streak, the mix muffles, and a
//!   delayed poof flashes far away.
//! * **Re-entry (bottom KO)** — anyone falling fast near the bottom trails fire that grows as they
//!   drop; the KO is a fireball.
//! * **Speed lines** — thin streaks rushing past when you're launched / very fast (plus star
//!   streaks); comfort-aware: off with the comfort preset (speed FOV off / FOV kicks at 0), scaled
//!   by the FOV-kick amount, kept to the periphery.

use super::sound::{MixDuck, Sfx3d};
use super::{bv, corps_rgb, PresentSet};
use crate::arena::{ArenaWorld, MapKind};
use crate::camera::PlayerCamera;
use crate::debris::{DebrisSettings, SpawnDebris, VfxRng};
use crate::render::sky::SkyDome;
use crate::render::Environment;
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents, WorldViews};
use bd_core::mechanics::Element;
use bd_fx::{BurstFx, BurstKind};
use bd_sim::replicate::pflags;
use bd_sim::smash::BlastZone;
use bd_sim::GameEvent;
use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use bevy::transform::TransformSystems;

const SPACE_SHADER: Handle<Shader> = uuid_handle!("7a1e6f2c-5d3b-4c8e-9f10-0b5e3d2c4a71");
const DOME_RADIUS: f32 = 1500.0;
/// Fire trails start this far above the bottom blast zone.
const HEAT_BAND: f32 = 20.0;
const SPEED_LINES: usize = 20;

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct SpaceParams {
    pub sun_dir: Vec4,
    /// rgb = sun colour, w = time.
    pub sun_color: Vec4,
    /// xyz = camera, w = moon angle.
    pub cam: Vec4,
    /// xyz = streak direction, w = amount.
    pub streak: Vec4,
    /// x = sky style: 0 Orbit (blue planet, one moon), 1 Cybertron (the metal planet below, amber
    /// seams and fires, a blue-violet nebula, two moons).
    pub style: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct SpaceMaterial {
    #[uniform(0)]
    pub params: SpaceParams,
}

impl Material for SpaceMaterial {
    fn fragment_shader() -> ShaderRef {
        SPACE_SHADER.into()
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

/// Everything this module spawns for Orbit (despawned when another map loads).
#[derive(Component)]
pub struct SpaceEntity;

#[derive(Component)]
struct SpaceDome(Handle<SpaceMaterial>);

/// A far silhouette on a slow orbit around the arena.
#[derive(Component)]
struct FarSilhouette {
    radius: f32,
    angle: f32,
    speed: f32,
    height: f32,
    base: Quat,
    spin_axis: Vec3,
    spin: f32,
}

#[derive(Component)]
struct SpeedLine(usize);

/// A player launched into space: flies on, trailing their corps colour, poofs far away.
#[derive(Component)]
pub struct SpaceKo {
    pub start: Vec3,
    pub pos: Vec3,
    pub vel: Vec3,
    t: f32,
    delay: f32,
    color: [f32; 3],
    poofed: bool,
    spin_axis: Vec3,
}

#[derive(Resource, Default)]
struct SpaceState {
    active: bool,
    blink: Option<Handle<StandardMaterial>>,
    lines: Option<Handle<StandardMaterial>>,
    dust: f32,
    /// Star streak (xyz dir, w amount) for the dome.
    streak: Vec4,
    /// Sky style of the active space map (see `SpaceParams::style`).
    style: f32,
}

/// The local player's space-KO spectator shot.
#[derive(Resource, Default)]
pub struct SpaceShot {
    pub target: Option<Entity>,
    t: f32,
    dur: f32,
    side: Vec3,
    from: Option<Transform>,
}

pub struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SPACE_SHADER, "space.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<SpaceMaterial>::default())
            .init_resource::<SpaceState>()
            .init_resource::<SpaceShot>()
            .add_systems(Update, space_kos.in_set(PresentSet::Events))
            .add_systems(Update, dev_orbit_test.before(crate::tick::FrameSet::Sim))
            .add_systems(Update, (sync_space, drift_silhouettes, zero_g_dust, reentry_heat, animate_space_kos, speed_lines).chain().in_set(PresentSet::Sync))
            .add_systems(
                PostUpdate,
                (ko_camera.after(super::camera3p::place_camera), dome_follow)
                    .chain()
                    .after(crate::camera::CameraUpdateSet)
                    .before(TransformSystems::Propagate),
            );
    }
}

fn is_orbit(arena: &Option<Res<ArenaWorld>>) -> bool {
    arena.as_ref().is_some_and(|a| a.info.kind == MapKind::Orbit)
}

/// Maps under open space and their sky style (the KO presentation stays Orbit-only).
fn space_style(arena: &Option<Res<ArenaWorld>>) -> Option<f32> {
    match arena.as_ref().map(|a| a.info.kind) {
        Some(MapKind::Orbit) => Some(0.0),
        // CYBERTRON (switched to `MapKind::Cybertron` at registration).
        Some(k) if format!("{k:?}") == "Cybertron" => Some(1.0),
        _ => None,
    }
}

fn dark(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, fog_enabled: false, ..default() })
}

/// Spawn / tear down the Orbit backdrop when the map changes; toggles the default sky and the
/// debris gravity profile.
#[allow(clippy::too_many_arguments)]
fn sync_space(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    mut state: ResMut<SpaceState>,
    existing: Query<Entity, With<SpaceEntity>>,
    mut sky: Query<&mut Visibility, With<SkyDome>>,
    mut debris: ResMut<DebrisSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut space_mats: ResMut<Assets<SpaceMaterial>>,
    mut shot: ResMut<SpaceShot>,
) {
    let style = space_style(&arena);
    let want = style.is_some();
    if want == state.active && style.unwrap_or(0.0) == state.style {
        return;
    }
    state.active = want;
    state.style = style.unwrap_or(0.0);
    for mut v in &mut sky {
        *v = if want { Visibility::Hidden } else { Visibility::Inherited };
    }
    if !want {
        for e in &existing {
            commands.entity(e).despawn();
        }
        debris.gravity_profile = None;
        shot.target = None;
        state.streak = Vec4::ZERO;
        return;
    }
    // Switching straight between two space maps: rebuild.
    for e in &existing {
        commands.entity(e).despawn();
    }
    debris.gravity_profile = arena.as_ref().and_then(|a| a.info.arena.as_ref()).and_then(|i| i.gravity);

    // Sky dome.
    let mat = space_mats.add(SpaceMaterial { params: SpaceParams::default() });
    commands.spawn((
        Name::new("SpaceDome"),
        SpaceEntity,
        SpaceDome(mat.clone()),
        Mesh3d(meshes.add(Sphere::new(DOME_RADIUS).mesh().uv(48, 24))),
        MeshMaterial3d(mat),
        Transform::IDENTITY,
        NotShadowCaster,
        NotShadowReceiver,
    ));

    // Far silhouettes.
    let hull = dark(&mut materials, Color::srgb(0.045, 0.05, 0.07));
    let panel = dark(&mut materials, Color::srgb(0.03, 0.05, 0.12));
    let blink = materials.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(40.0, 4.0, 3.0)), unlit: true, fog_enabled: false, ..default() });
    let glow = materials.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(6.0, 12.0, 30.0)), unlit: true, fog_enabled: false, ..default() });
    state.blink = Some(blink.clone());
    let mut part = |c: &mut ChildSpawnerCommands, mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform| {
        c.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t, NotShadowCaster, NotShadowReceiver));
    };
    // 1. A big ring station, tilted, turning slowly.
    commands
        .spawn((
            Name::new("FarStation"),
            SpaceEntity,
            FarSilhouette { radius: 980.0, angle: 2.35, speed: 0.0012, height: 230.0, base: Quat::from_euler(EulerRot::XYZ, 0.9, 0.3, 0.2), spin_axis: Vec3::Y, spin: 0.05 },
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|c| {
            part(c, Torus::new(54.0, 62.0).mesh().major_resolution(48).minor_resolution(8).build(), &hull, Transform::IDENTITY);
            part(c, Cylinder::new(9.0, 60.0).mesh().resolution(16).build(), &hull, Transform::IDENTITY);
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::FRAC_PI_2;
                part(c, Cuboid::new(50.0, 3.0, 3.0).mesh().build(), &hull, Transform::from_rotation(Quat::from_rotation_y(a)).with_translation(Quat::from_rotation_y(a) * Vec3::X * 30.0));
            }
            for s in [-1.0f32, 1.0] {
                part(c, Cuboid::new(24.0, 1.0, 70.0).mesh().build(), &panel, Transform::from_xyz(0.0, s * 42.0, 0.0));
            }
            for (i, p) in [Vec3::new(0.0, 31.0, 0.0), Vec3::new(62.0, 0.0, 0.0), Vec3::new(-62.0, 0.0, 0.0)].into_iter().enumerate() {
                let m = if i == 0 { &glow } else { &blink };
                part(c, Sphere::new(2.4).mesh().ico(1).unwrap_or_else(|_| Sphere::new(2.4).mesh().build()), m, Transform::from_translation(p));
            }
        });
    // 2. A freighter crossing the sky.
    commands
        .spawn((
            Name::new("FarFreighter"),
            SpaceEntity,
            FarSilhouette { radius: 1150.0, angle: -0.6, speed: 0.006, height: 170.0, base: Quat::from_rotation_z(0.08), spin_axis: Vec3::Y, spin: 0.0 },
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|c| {
            part(c, Cuboid::new(16.0, 12.0, 95.0).mesh().build(), &hull, Transform::IDENTITY);
            part(c, Cuboid::new(10.0, 14.0, 16.0).mesh().build(), &hull, Transform::from_xyz(0.0, 10.0, -36.0));
            part(c, Cuboid::new(34.0, 2.0, 12.0).mesh().build(), &hull, Transform::from_xyz(0.0, 0.0, -40.0));
            part(c, Sphere::new(5.0).mesh().build(), &glow, Transform::from_xyz(0.0, 0.0, -50.0));
            part(c, Sphere::new(1.8).mesh().build(), &blink, Transform::from_xyz(0.0, 18.0, -36.0));
        });
    // 3. A small satellite, higher and closer.
    commands
        .spawn((
            Name::new("FarSatellite"),
            SpaceEntity,
            FarSilhouette { radius: 720.0, angle: 0.9, speed: -0.01, height: 320.0, base: Quat::from_rotation_x(0.4), spin_axis: Vec3::Z, spin: 0.15 },
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|c| {
            part(c, Cuboid::new(5.0, 5.0, 8.0).mesh().build(), &hull, Transform::IDENTITY);
            part(c, Cuboid::new(30.0, 0.6, 7.0).mesh().build(), &panel, Transform::IDENTITY);
            part(c, Sphere::new(0.9).mesh().build(), &blink, Transform::from_xyz(0.0, 3.2, 0.0));
        });

    // Speed lines (hidden until launched).
    let lines = materials.add(StandardMaterial {
        base_color: Color::srgba(0.85, 0.92, 1.0, 0.0),
        unlit: true,
        fog_enabled: false,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    state.lines = Some(lines.clone());
    let line_mesh = meshes.add(Cuboid::new(0.03, 0.03, 1.0));
    for k in 0..SPEED_LINES {
        commands.spawn((SpaceEntity, SpeedLine(k), Mesh3d(line_mesh.clone()), MeshMaterial3d(lines.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster, NotShadowReceiver));
    }
}

fn drift_silhouettes(time: Res<Time<Real>>, state: Res<SpaceState>, mut q: Query<(&mut FarSilhouette, &mut Transform)>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (mut s, mut tf) in &mut q {
        s.angle += s.speed * dt;
        tf.translation = Vec3::new(s.angle.cos() * s.radius, s.height, s.angle.sin() * s.radius);
        // Face along the orbit (freighter), then the slow spin.
        let heading = Quat::from_rotation_y(-s.angle);
        tf.rotation = heading * s.base * Quat::from_axis_angle(s.spin_axis, s.spin * t);
    }
    if let Some(h) = state.blink.as_ref()
        && let Some(mut m) = materials.get_mut(h)
    {
        let on = (t * 0.8).fract() < 0.1;
        // Unlit: the base colour is the (HDR) light.
        m.base_color = Color::LinearRgba(if on { LinearRgba::rgb(60.0, 6.0, 4.0) } else { LinearRgba::rgb(0.6, 0.05, 0.04) });
    }
}

/// Floating dust motes around the camera while it's up in the zero-g band.
fn zero_g_dust(
    time: Res<Time<Real>>,
    arena: Option<Res<ArenaWorld>>,
    mut state: ResMut<SpaceState>,
    cam: Query<&Transform, With<PlayerCamera>>,
    mut debris: MessageWriter<SpawnDebris>,
    mut rng: ResMut<VfxRng>,
) {
    let Some(g) = arena.as_ref().and_then(|a| a.info.arena.as_ref()).and_then(|i| i.gravity) else { return };
    let Ok(cam) = cam.single() else { return };
    let eye = cam.translation;
    if eye.y < g.full_below - 26.0 {
        state.dust = 0.0;
        return;
    }
    state.dust += time.delta_secs().min(0.1) * 160.0;
    let r = &mut rng.0;
    while state.dust >= 1.0 {
        state.dust -= 1.0;
        let p = eye + r.unit_vec3() * r.range(2.0, 26.0);
        if p.y < g.full_below - 4.0 {
            continue;
        }
        let bright = r.range(0.5, 1.0);
        debris.write(SpawnDebris {
            pos: p,
            vel: r.unit_vec3() * r.range(0.05, 0.4),
            color: [0.72 * bright, 0.8 * bright, 0.95 * bright],
            size: r.range(0.04, 0.1),
            emissive: 0.8,
            lifetime: r.range(4.0, 7.0),
        });
    }
}

/// Fire trails on anyone falling fast toward the bottom blast zone (re-entry).
fn reentry_heat(
    time: Res<Time<Real>>,
    arena: Option<Res<ArenaWorld>>,
    views: Res<WorldViews>,
    mut debris: MessageWriter<SpawnDebris>,
    mut rng: ResMut<VfxRng>,
) {
    if !is_orbit(&arena) {
        return;
    }
    let Some(void_y) = arena.as_ref().map(|a| a.info.void_y) else { return };
    let dt = time.delta_secs().min(0.05);
    let r = &mut rng.0;
    for v in &views.players {
        if !v.alive() || v.vel.y > -6.0 || v.pos.y > void_y + HEAT_BAND {
            continue;
        }
        // Hanging on an attached tether survives below the void: no burn-up trail.
        if v.has(pflags::HOOK_L_ATTACHED | pflags::HOOK_R_ATTACHED) {
            continue;
        }
        let k = (1.0 - (v.pos.y - void_y) / HEAT_BAND).clamp(0.0, 1.0) * ((-v.vel.y - 6.0) / 14.0).clamp(0.2, 1.0);
        let n = (k * 420.0 * dt).ceil() as usize;
        let down = v.vel.normalize_or(Vec3::NEG_Y);
        // Your own trail wraps below you (the leading edge), never across your view.
        let centre = if v.is_local { v.pos + down * 1.2 } else { v.pos + Vec3::Y * 0.9 };
        for _ in 0..n {
            let t = r.f32();
            let color = [1.0, 0.85 - 0.55 * t, 0.35 - 0.3 * t];
            debris.write(SpawnDebris {
                pos: centre + r.unit_vec3() * r.range(0.2, 0.9 + k * 0.6),
                vel: v.vel * r.range(0.55, 0.85) + r.unit_vec3() * 2.0,
                color,
                size: r.range(0.12, 0.3) * (0.6 + k),
                emissive: 8.0 + 10.0 * k,
                lifetime: r.range(0.2, 0.45),
            });
        }
    }
}

/// Orbit's top / bottom KOs (impact.rs skips its star twinkle here).
#[allow(clippy::too_many_arguments)]
fn space_kos(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    active: Option<Res<ActiveWorld>>,
    mut events: MessageReader<GameEvents>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shot: ResMut<SpaceShot>,
    mut duck: ResMut<MixDuck>,
    fx: (MessageWriter<Sfx3d>, MessageWriter<BurstFx>, MessageWriter<SpawnDebris>),
    mut rng: ResMut<VfxRng>,
    cam: Query<&Transform, With<PlayerCamera>>,
) {
    let (mut sfx, mut burst, mut debris) = fx;
    let (true, Some(active)) = (is_orbit(&arena), active) else {
        events.clear();
        return;
    };
    for ev in events.read() {
        let GameEvent::Ko { victim, victim_corps, pos, vel, zone, poof_delay, .. } = &ev.event else { continue };
        let p = bv(*pos);
        let v = bv(*vel);
        let col = corps_rgb(*victim_corps);
        let local = active.local == Some(*victim);
        match zone {
            Some(BlastZone::Top) => {
                let dir = (v.normalize_or(Vec3::Y) + Vec3::Y * 0.6).normalize();
                let delay = (poof_delay + 1.2).clamp(1.8, 2.6);
                let r = &mut rng.0;
                let mat = materials.add(StandardMaterial {
                    base_color: Color::LinearRgba(LinearRgba::rgb(col[0] * 6.0 + 1.0, col[1] * 6.0 + 1.0, col[2] * 6.0 + 1.0)),
                    unlit: true,
                    ..default()
                });
                let e = commands
                    .spawn((
                        Name::new("SpaceKo"),
                        SpaceEntity,
                        SpaceKo { start: p, pos: p, vel: dir * v.length().clamp(26.0, 40.0), t: 0.0, delay, color: col, poofed: false, spin_axis: r.unit_vec3() },
                        Mesh3d(meshes.add(Cuboid::new(0.7, 1.7, 0.45))),
                        MeshMaterial3d(mat),
                        Transform::from_translation(p),
                        NotShadowCaster,
                    ))
                    .id();
                if local {
                    let right = cam.single().map(|t| t.rotation * Vec3::X).unwrap_or(Vec3::X);
                    let side = Vec3::new(right.x, 0.0, right.z).normalize_or(Vec3::X);
                    *shot = SpaceShot { target: Some(e), t: 0.0, dur: delay + 0.35, side, from: None };
                    duck.duck(delay + 0.3);
                    sfx.write(Sfx3d::ui("ko_accent_void").pitch(0.55).gain(0.9).exempt());
                } else {
                    sfx.write(Sfx3d::at("ko_accent_void", p).pitch(1.3).gain(0.8));
                }
            }
            Some(BlastZone::Bottom) => {
                // Re-entry burn-up: a fireball.
                burst.write(BurstFx { kind: BurstKind::Element(Element::Fire), pos: p, dir: Vec3::Y, scale: 3.2, color: None });
                burst.write(BurstFx { kind: BurstKind::Flash, pos: p, dir: Vec3::Y, scale: 3.5, color: Some([1.0, 0.55, 0.15]) });
                let r = &mut rng.0;
                for _ in 0..140 {
                    let d = r.unit_vec3();
                    let t = r.f32();
                    debris.write(SpawnDebris {
                        pos: p + d * r.range(0.2, 1.2),
                        vel: d * r.range(4.0, 16.0) + Vec3::Y * r.range(2.0, 8.0),
                        color: [1.0, 0.8 - 0.5 * t, 0.3 - 0.25 * t],
                        size: r.range(0.15, 0.4),
                        emissive: r.range(8.0, 16.0),
                        lifetime: r.range(0.4, 1.0),
                    });
                }
                for _ in 0..30 {
                    // A few embers of the victim's colour.
                    let d = r.unit_vec3();
                    debris.write(SpawnDebris { pos: p, vel: d * r.range(3.0, 9.0), color: col, size: r.range(0.1, 0.25), emissive: 6.0, lifetime: r.range(0.6, 1.2) });
                }
                sfx.write(Sfx3d::at("explosion_big", p).pitch(0.72).gain(0.9));
            }
            _ => {}
        }
    }
}

fn animate_space_kos(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut q: Query<(Entity, &mut SpaceKo, &mut Transform, &mut Visibility)>,
    mut debris: MessageWriter<SpawnDebris>,
    mut burst: MessageWriter<BurstFx>,
    mut sfx: MessageWriter<Sfx3d>,
    mut rng: ResMut<VfxRng>,
) {
    let dt = time.delta_secs().min(0.05);
    let r = &mut rng.0;
    for (e, mut ko, mut tf, mut vis) in &mut q {
        ko.t += dt;
        if !ko.poofed {
            // Drifting out: it keeps going (a little faster: nothing up there slows you down).
            let v = ko.vel * (1.0 + 0.35 * dt);
            ko.vel = v;
            ko.pos += v * dt;
            tf.translation = ko.pos;
            // The body tumbles (the camera never does).
            tf.rotation = Quat::from_axis_angle(ko.spin_axis, 6.5 * dt) * tf.rotation;
            let shrink = (1.0 - ko.t / ko.delay * 0.5).max(0.4);
            tf.scale = Vec3::splat(shrink);
            for _ in 0..3 {
                debris.write(SpawnDebris {
                    pos: ko.pos + r.unit_vec3() * 0.4,
                    vel: r.unit_vec3() * 0.5,
                    color: ko.color,
                    size: r.range(0.12, 0.28),
                    emissive: 7.0,
                    lifetime: r.range(0.8, 1.4),
                });
            }
            if ko.t >= ko.delay {
                ko.poofed = true;
                *vis = Visibility::Hidden;
                burst.write(BurstFx { kind: BurstKind::Flash, pos: ko.pos, dir: Vec3::Y, scale: 7.0, color: Some(ko.color) });
                burst.write(BurstFx { kind: BurstKind::Shards, pos: ko.pos, dir: Vec3::ZERO, scale: 5.0, color: Some(ko.color) });
                // Far away and muffled.
                sfx.write(Sfx3d::ui("ko_shatter").pitch(0.55).gain(0.45).exempt());
            }
        }
        if ko.t > ko.delay + 0.9 {
            commands.entity(e).despawn();
        }
    }
}

/// The local space-KO spectator shot: beside the launch line, looking at yourself flying away.
/// Eases in from the first-person view; `looking_at` with +Y up and a ≥ 11 m horizontal offset so
/// the camera never rolls or flips.
fn ko_camera(time: Res<Time<Real>>, mut shot: ResMut<SpaceShot>, kos: Query<&SpaceKo>, mut cam: Query<&mut Transform, With<PlayerCamera>>) {
    let Some(target) = shot.target else { return };
    let Ok(mut tf) = cam.single_mut() else { return };
    shot.t += time.delta_secs().min(0.05);
    let Ok(ko) = kos.get(target) else {
        shot.target = None;
        return;
    };
    if shot.t > shot.dur {
        shot.target = None;
        return;
    }
    if shot.from.is_none() {
        shot.from = Some(*tf);
    }
    let rise = ko.pos - ko.start;
    let eye = ko.start + shot.side * 11.0 - Vec3::Y * 2.5 + rise * 0.42;
    let want = Transform::from_translation(eye).looking_at(ko.pos, Vec3::Y);
    let k = {
        let x = (shot.t / 0.35).clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    let from = shot.from.unwrap_or(want);
    tf.translation = from.translation.lerp(want.translation, k);
    tf.rotation = from.rotation.slerp(want.rotation, k);
}

/// Speed lines + star streaks while you're launched / very fast (comfort-aware).
#[allow(clippy::too_many_arguments)]
fn speed_lines(
    time: Res<Time<Real>>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    comfort: Res<ComfortSettings>,
    shot: Res<SpaceShot>,
    kos: Query<&SpaceKo>,
    mut state: ResMut<SpaceState>,
    cam: Query<&Transform, (With<PlayerCamera>, Without<SpeedLine>)>,
    mut lines: Query<(&SpeedLine, &mut Transform, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !state.active {
        return;
    }
    let me = active.as_ref().and_then(|a| a.local).and_then(|id| views.player(id));
    let (vel, launched) = me.filter(|v| v.alive()).map_or((Vec3::ZERO, false), |v| (v.vel, v.has(pflags::LAUNCHED)));
    let speed = vel.length();
    let comfort_k = if comfort.speed_fov { comfort.fov_kick.clamp(0.0, 1.0) } else { 0.0 };
    let mut k = if launched { ((speed - 18.0) / 25.0).clamp(0.0, 1.0) } else { ((speed - 34.0) / 25.0).clamp(0.0, 1.0) } * comfort_k;
    let mut dir = vel.normalize_or_zero();
    // The space-KO shot streaks the stars along the flight (the view itself stays calm).
    if let Some(ko) = shot.target.and_then(|e| kos.get(e).ok()) {
        dir = ko.vel.normalize_or(Vec3::Y);
        k = ((shot.t / 0.6).min(1.0) * 0.9).max(k);
    }
    let target = dir.extend(k);
    let s = &mut state.streak;
    *s += (target - *s) * (1.0 - (-8.0 * time.delta_secs()).exp());
    if s.w < 0.01 {
        s.w = 0.0;
    }
    // Lines: only for your own launch (not during the KO shot), in the periphery.
    let line_k = if shot.target.is_some() { 0.0 } else { k };
    if let Some(h) = state.lines.as_ref()
        && let Some(mut m) = materials.get_mut(h)
    {
        m.base_color = Color::srgba(0.85, 0.92, 1.0, 0.28 * line_k);
    }
    let Ok(cam) = cam.single() else { return };
    let t = time.elapsed_secs();
    let fwd = if dir == Vec3::ZERO { cam.rotation * Vec3::NEG_Z } else { dir };
    let a = if fwd.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let u = fwd.cross(a).normalize();
    let w = fwd.cross(u);
    for (l, mut tf, mut vis) in &mut lines {
        if line_k <= 0.02 {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let i = l.0 as f32;
        let h = (i * 0.618_034).fract();
        let ang = i / SPEED_LINES as f32 * std::f32::consts::TAU + h;
        let rad = 2.8 + 2.2 * (i * 0.371).fract();
        let phase = (h + t * (1.6 + speed * 0.03)).fract();
        let along = 16.0 - 26.0 * phase;
        let len = 1.5 + 3.5 * line_k;
        tf.translation = cam.translation + fwd * along + (u * ang.cos() + w * ang.sin()) * rad;
        tf.rotation = Quat::from_rotation_arc(Vec3::Z, fwd);
        tf.scale = Vec3::new(1.0, 1.0, len);
    }
}

/// The dome follows the camera; its uniforms (sun, time, camera, moon, streak) update per frame.
fn dome_follow(
    time: Res<Time<Real>>,
    env: Res<Environment>,
    state: Res<SpaceState>,
    cam: Query<&Transform, (With<PlayerCamera>, Without<SpaceDome>)>,
    mut domes: Query<(&SpaceDome, &mut Transform)>,
    mut mats: ResMut<Assets<SpaceMaterial>>,
) {
    let Ok(cam) = cam.single() else { return };
    let t = time.elapsed_secs();
    let sun = env.sun_color.to_linear();
    for (d, mut tf) in &mut domes {
        tf.translation = cam.translation;
        if let Some(mut m) = mats.get_mut(&d.0) {
            m.params = SpaceParams {
                sun_dir: env.sun_dir.extend(0.0),
                sun_color: Vec4::new(sun.red, sun.green, sun.blue, t),
                cam: cam.translation.extend(1.9 + t * 0.0025),
                streak: state.streak,
                style: Vec4::new(state.style, 0.0, 0.0, 0.0),
            };
        }
    }
}

/// Dev / verification hook (off unless set): `BD_ORBIT_TEST=top@3` launches the local practice
/// player straight up off the hub roof 3 s into the game (a space KO); `bottom@3` drops them into
/// the re-entry band. Offline practice only.
fn dev_orbit_test(time: Res<Time<Real>>, arena: Option<Res<ArenaWorld>>, active: Option<ResMut<ActiveWorld>>, mut started: Local<Option<f32>>, mut done: Local<bool>) {
    static SPEC: std::sync::OnceLock<Option<(bool, f32)>> = std::sync::OnceLock::new();
    let spec = *SPEC.get_or_init(|| {
        let v = std::env::var("BD_ORBIT_TEST").ok()?;
        let (kind, at) = v.split_once('@').unwrap_or((v.as_str(), "3"));
        Some((kind == "top", at.parse().unwrap_or(3.0)))
    });
    let (Some((top, at)), false) = (spec, *done) else { return };
    let (true, Some(mut active)) = (is_orbit(&arena), active) else { return };
    if active.role != crate::sim_bridge::SimRole::Offline {
        return;
    }
    let t0 = *started.get_or_insert(time.elapsed_secs());
    if time.elapsed_secs() - t0 < at {
        return;
    }
    *done = true;
    let Some(id) = active.local else { return };
    let (pos, vel) = if top {
        (bd_core::Vec3::new(2.0, bd_voxel::maps::orbit::HUB_ROOF + 2.0, 3.0), bd_core::Vec3::new(0.0, 58.0, 2.0))
    } else {
        (bd_core::Vec3::new(0.0, 24.0, -88.0), bd_core::Vec3::new(0.0, -18.0, 0.0))
    };
    if let Some(p) = active.world.player_mut(id) {
        p.mv.pos = pos;
        p.mv.vel = vel;
        p.mv.grounded = false;
    }
    if top {
        active.world.launch_player(id, vel, None);
    }
}