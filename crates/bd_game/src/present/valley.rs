//! **Valley of the End** presentation (bd_voxel `maps::valley`; DESIGN.md "Maps"). Active while the
//! loaded map has water sheets (the Valley); everything spawned here carries [`ValleyEntity`] and
//! is torn down when another map loads.
//!
//! * **Water sheets** — the river and the upper pool: flat animated water (the sea's wave normals,
//!   scrolling downstream on the river), drawn at the sheet tops the sim lets players run on.
//! * **Falls** — a scrolling streak veil over the white-water voxels (and the little falls at the
//!   downstream lip), so the water moves.
//! * **Mist** — a capped pool of soft billboards rising out of the plunge hole (and the lip), plus a
//!   faint rainbow arc in the spray, opposite the afternoon sun.
//! * **Water-walk feedback** — ripple rings, droplets and a splashy footstep layer wherever someone
//!   runs, lands or slides on the water (chakra walking).
//!
//! Dev screenshots (never set in play): `BD_VALLEY_POSE=x,y,z,yaw,pitch` (feet, degrees) pins the
//! local player there and holds the view from 0.8 s on (practice respawn facing otherwise wins
//! over `--look`); pair with `BD_HIDDEN_WINDOW=1 --screenshot … --quit-after …`.

use super::sound::Sfx3d;
use super::PresentSet;
use crate::arena::ArenaWorld;
use crate::camera::PlayerCamera;
use crate::debris::{SpawnDebris, VfxRng};
use crate::sim_bridge::WorldViews;
use bd_core::PlayerId;
use bd_sim::replicate::pflags;
use bd_voxel::maps::valley as vmap;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine2;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::sync::Arc;

/// Mist puffs at the plunge hole + at the downstream lip.
const MIST_PLUNGE: usize = 36;
const MIST_LIP: usize = 12;
/// Ripple rings in flight at once.
const RIPPLES: usize = 24;
/// A splash step every this many metres of travel on the water.
const STRIDE: f32 = 1.8;

#[derive(Component)]
pub struct ValleyEntity;

#[derive(Component)]
struct FlowingWater {
    material: Handle<StandardMaterial>,
    /// UV scroll per second (local x, local z).
    flow: Vec2,
}

#[derive(Component)]
struct Mist {
    material: Handle<StandardMaterial>,
    /// Where it rises from, how far and how fast; phase offset.
    origin: Vec3,
    rise: f32,
    period: f32,
    phase: f32,
    size: f32,
}

#[derive(Component)]
struct Ripple {
    material: Handle<StandardMaterial>,
    age: f32,
    life: f32,
    size: f32,
}

/// What's spawned for which map (by `ArenaWorld::info` pointer).
#[derive(Resource, Default)]
struct Scene {
    for_map: Option<usize>,
    sheets: Vec<bd_voxel::WaterSheet>,
    walk: bool,
    next_ripple: usize,
}

/// Per player: last feet position on the water and distance since the last splash step.
#[derive(Resource, Default)]
struct Wading {
    players: HashMap<PlayerId, (Vec3, f32, bool)>,
}

pub struct ValleyPlugin;

impl Plugin for ValleyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Scene>()
            .init_resource::<Wading>()
            .add_systems(Update, (spawn_scene, animate_water, animate_mist, water_walk_fx, animate_ripples).chain().in_set(PresentSet::Sync));
        if let Some(pose) = std::env::var("BD_VALLEY_POSE").ok().map(|s| s.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 5) {
            app.insert_resource(Pose(pose, 0.0)).add_systems(
                Update,
                hold_pose.before(crate::tick::FrameSet::Input).run_if(resource_exists::<crate::sim_bridge::ActiveWorld>),
            );
        }
    }
}

#[derive(Resource)]
struct Pose(Vec<f32>, f32);

fn hold_pose(time: Res<Time<Real>>, mut pose: ResMut<Pose>, mut active: ResMut<crate::sim_bridge::ActiveWorld>, mut look: ResMut<crate::input::LookAngles>) {
    pose.1 += time.delta_secs();
    if pose.1 < 0.8 {
        return;
    }
    let v = &pose.0;
    if let Some(me) = active.local
        && let Some(p) = active.world.player_mut(me)
    {
        p.mv.pos = bd_core::Vec3::new(v[0], v[1], v[2]);
        p.mv.vel = bd_core::Vec3::ZERO;
        p.spawn_shield = 0.0;
    }
    look.set_degrees(v[3], v[4]);
}

fn repeat_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    })
}

/// 128² tangent-space wave normals (same idea as the sea's).
fn wave_normals() -> Image {
    const N: usize = 128;
    let mut data = Vec::with_capacity(N * N * 4);
    let tau = std::f32::consts::TAU;
    let waves: [(f32, f32, f32); 4] = [(1.0, 0.0, 0.25), (0.0, 3.0, 0.35), (3.0, 2.0, 0.12), (-2.0, 5.0, 0.1)];
    for y in 0..N {
        for x in 0..N {
            let (u, v) = (x as f32 / N as f32, y as f32 / N as f32);
            let (mut dx, mut dy) = (0.0, 0.0);
            for (kx, ky, a) in waves {
                let ph = tau * (kx * u + ky * v);
                dx += a * kx * ph.cos();
                dy += a * ky * ph.cos();
            }
            let n = Vec3::new(-dx * 0.25, -dy * 0.25, 1.0).normalize();
            data.extend_from_slice(&[((n.x * 0.5 + 0.5) * 255.0) as u8, ((n.y * 0.5 + 0.5) * 255.0) as u8, ((n.z * 0.5 + 0.5) * 255.0) as u8, 255]);
        }
    }
    let mut img = Image::new(Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8Unorm, RenderAssetUsages::RENDER_WORLD);
    img.sampler = repeat_sampler();
    img
}

fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65535.0
}

/// Falling-water veil: vertical streaks of white with alpha, tileable (scrolls down).
fn streaks() -> Image {
    const W: usize = 64;
    const H: usize = 256;
    let mut data = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        for x in 0..W {
            let col = hash(x as u32 / 2);
            let wobble = (y as f32 / H as f32 * std::f32::consts::TAU * 3.0 + col * 20.0).sin() * 0.5 + 0.5;
            let s = (col * 0.7 + wobble * 0.3).powf(2.0);
            let a = (0.12 + 0.7 * s) * (0.8 + 0.2 * hash((x * 977 + y) as u32));
            let c = (220.0 + 35.0 * s) as u8;
            data.extend_from_slice(&[c, c, 255, (a * 255.0) as u8]);
        }
    }
    let mut img = Image::new(Extent3d { width: W as u32, height: H as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    img.sampler = repeat_sampler();
    img
}

/// Soft round puff (radial alpha).
fn puff() -> Image {
    const N: usize = 64;
    let mut data = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let d = Vec2::new(x as f32 + 0.5 - N as f32 * 0.5, y as f32 + 0.5 - N as f32 * 0.5).length() / (N as f32 * 0.5);
            let a = (1.0 - d).clamp(0.0, 1.0).powf(1.6);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// A flat rectangle mesh (XZ) with UVs tiled every `tile` metres.
fn sheet_mesh(w: f32, d: f32, tile: f32) -> Mesh {
    let mut m = Plane3d::default().mesh().size(w, d).build();
    if let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) = m.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        for uv in uvs.iter_mut() {
            uv[0] *= w / tile;
            uv[1] *= d / tile;
        }
    }
    let _ = m.generate_tangents();
    m
}

/// A vertical veil `w` × `h` (facing −Z), UVs tiled so streaks keep their size.
fn veil_mesh(w: f32, h: f32) -> Mesh {
    let mut m = Rectangle::new(w, h).mesh().build();
    if let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) = m.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        for uv in uvs.iter_mut() {
            uv[0] *= w / 12.0;
            uv[1] *= h / 24.0;
        }
    }
    m
}

/// Half-annulus rainbow in the XY plane (seven bands, vertex colours).
fn rainbow_mesh(r0: f32, r1: f32) -> Mesh {
    let bands: [[f32; 3]; 7] =
        [[0.55, 0.1, 0.9], [0.25, 0.2, 1.0], [0.1, 0.55, 1.0], [0.1, 0.9, 0.3], [1.0, 0.95, 0.1], [1.0, 0.55, 0.05], [1.0, 0.1, 0.1]];
    let seg = 48;
    let (mut pos, mut col, mut nrm, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (b, c) in bands.iter().enumerate() {
        let (ra, rb) = (r0 + (r1 - r0) * b as f32 / 7.0, r0 + (r1 - r0) * (b + 1) as f32 / 7.0);
        let base = pos.len() as u32;
        for s in 0..=seg {
            let a = std::f32::consts::PI * s as f32 / seg as f32;
            let (sn, cs) = a.sin_cos();
            // Fade the feet of the arc into the spray.
            let fade = sn.clamp(0.0, 1.0).powf(0.6);
            for r in [ra, rb] {
                pos.push([cs * r, sn * r, 0.0]);
                col.push([c[0] * fade, c[1] * fade, c[2] * fade, 1.0]);
                nrm.push([0.0, 0.0, -1.0]);
            }
            if s < seg {
                let i = base + s * 2;
                idx.extend_from_slice(&[i, i + 1, i + 2, i + 1, i + 3, i + 2]);
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

#[allow(clippy::too_many_arguments)]
fn spawn_scene(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    mut scene: ResMut<Scene>,
    existing: Query<Entity, With<ValleyEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let want = arena.as_ref().and_then(|a| {
        let info = a.info.arena.as_ref()?;
        (!info.water_sheets.is_empty()).then(|| (Arc::as_ptr(&a.info) as usize, info.water_sheets.clone(), info.water_walk, a.info.water_color))
    });
    let key = want.as_ref().map(|w| w.0);
    if key == scene.for_map {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    scene.for_map = key;
    scene.sheets.clear();
    scene.next_ripple = 0;
    let Some((_, sheets, walk, colors)) = want else { return };
    scene.sheets = sheets.clone();
    scene.walk = walk;
    let (shallow, _deep) = colors.unwrap_or(([0.30, 0.60, 0.62], [0.08, 0.26, 0.30]));
    let normals = images.add(wave_normals());
    let water_mat = |materials: &mut Assets<StandardMaterial>| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(shallow[0], shallow[1], shallow[2]),
            perceptual_roughness: 0.08,
            reflectance: 0.6,
            normal_map_texture: Some(normals.clone()),
            ..default()
        })
    };
    // Water sheets: the river segments share one flowing material, the pool its own slow one.
    let river = water_mat(&mut materials);
    let pool = water_mat(&mut materials);
    for s in &sheets {
        let is_river = (s.center.y - vmap::RIVER_Y).abs() < 0.01;
        let material = if is_river { river.clone() } else { pool.clone() };
        commands.spawn((
            Name::new("Valley water"),
            ValleyEntity,
            Mesh3d(meshes.add(sheet_mesh(s.half.x * 2.0, s.half.y * 2.0, 8.0))),
            MeshMaterial3d(material),
            Transform::from_translation(Vec3::new(s.center.x, s.center.y - 0.02, s.center.z)).with_rotation(Quat::from_rotation_y(s.yaw)),
            NotShadowCaster,
        ));
    }
    commands.spawn((ValleyEntity, FlowingWater { material: river, flow: Vec2::new(0.0, 0.35) }));
    commands.spawn((ValleyEntity, FlowingWater { material: pool, flow: Vec2::new(0.012, 0.03) }));
    // Falls veils (a hair in front of the white-water voxels).
    let streak_img = images.add(streaks());
    let veil = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.85),
        base_color_texture: Some(streak_img),
        emissive: LinearRgba::rgb(0.35, 0.4, 0.45),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    let top = vmap::POOL_Y;
    let (fw, fh) = (vmap::FALLS_HALF * 2.0, top - vmap::WATER_Y);
    commands.spawn((
        Name::new("Falls veil"),
        ValleyEntity,
        Mesh3d(meshes.add(veil_mesh(fw, fh))),
        MeshMaterial3d(veil.clone()),
        Transform::from_translation(Vec3::new(0.0, vmap::WATER_Y + fh * 0.5, vmap::FALLS_Z - 0.04)),
        NotShadowCaster,
        NotShadowReceiver,
    ));
    let (lx, lw) = (vmap::centre_x(vmap::LIP_Z), vmap::river_half(vmap::LIP_Z) * 2.0);
    let lh = vmap::RIVER_Y - vmap::WATER_Y;
    commands.spawn((
        Name::new("Lip veil"),
        ValleyEntity,
        Mesh3d(meshes.add(veil_mesh(lw, lh))),
        MeshMaterial3d(veil.clone()),
        Transform::from_translation(Vec3::new(lx, vmap::WATER_Y + lh * 0.5, vmap::LIP_Z - 1.54)),
        NotShadowCaster,
        NotShadowReceiver,
    ));
    commands.spawn((ValleyEntity, FlowingWater { material: veil, flow: Vec2::new(0.0, -1.1) }));
    // Mist.
    let puff_img = images.add(puff());
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let mut spawn_mist = |commands: &mut Commands, k: usize, origin: Vec3, spread: Vec3, rise: f32, size: f32| {
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 1.0, 0.0),
            base_color_texture: Some(puff_img.clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        let j = |s: u32| hash(k as u32 * 7 + s) * 2.0 - 1.0;
        commands.spawn((
            ValleyEntity,
            Mist { material: material.clone(), origin: origin + spread * Vec3::new(j(1), j(2).abs(), j(3)), rise, period: 4.0 + 3.0 * hash(k as u32 * 7 + 4), phase: hash(k as u32 * 7 + 5), size },
            Mesh3d(quad.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    };
    let pc = vmap::PLUNGE_C;
    for k in 0..MIST_PLUNGE {
        spawn_mist(&mut commands, k, Vec3::new(pc.x, vmap::WATER_Y + 1.0, pc.y + 4.0), Vec3::new(11.0, 4.0, 6.0), 22.0, 9.0);
    }
    for k in 0..MIST_LIP {
        spawn_mist(&mut commands, 100 + k, Vec3::new(lx, vmap::WATER_Y + 1.0, vmap::LIP_Z - 4.0), Vec3::new(lw * 0.5, 2.0, 3.0), 12.0, 6.0);
    }
    // Rainbow in the spray (opposite the south-west sun: seen looking north up the gorge).
    let bow = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.16),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Name::new("Rainbow"),
        ValleyEntity,
        Mesh3d(meshes.add(rainbow_mesh(20.0, 23.5))),
        MeshMaterial3d(bow),
        Transform::from_translation(Vec3::new(pc.x, vmap::WATER_Y + 2.0, pc.y - 4.0)).with_rotation(Quat::from_rotation_y(0.25)),
        NotShadowCaster,
        NotShadowReceiver,
    ));
    // Ripple ring pool (hidden until used).
    let ring = meshes.add(Annulus::new(0.8, 1.0).mesh().resolution(28).build());
    for _ in 0..RIPPLES {
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.9, 0.97, 1.0, 0.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            ValleyEntity,
            Ripple { material: material.clone(), age: 1.0, life: 1.0, size: 1.0 },
            Mesh3d(ring.clone()),
            MeshMaterial3d(material),
            Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::ZERO),
            Visibility::Hidden,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

fn animate_water(time: Res<Time<Real>>, q: Query<&FlowingWater>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let t = time.elapsed_secs();
    for w in &q {
        if let Some(mut m) = materials.get_mut(&w.material) {
            m.uv_transform = Affine2::from_translation(w.flow * t);
        }
    }
}

fn animate_mist(
    time: Res<Time<Real>>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut q: Query<(&Mist, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if q.is_empty() {
        return;
    }
    let t = time.elapsed_secs();
    let eye = cam.iter().next().map_or(Vec3::ZERO, |c| c.translation());
    for (m, mut tf) in &mut q {
        let u = (t / m.period + m.phase).fract();
        let pos = m.origin + Vec3::new((t * 0.3 + m.phase * 9.0).sin() * 1.5, m.rise * u, (t * 0.23 + m.phase * 5.0).cos() * 1.2);
        tf.translation = pos;
        let to_eye = (eye - pos).normalize_or(Vec3::Z);
        tf.rotation = Quat::from_rotation_arc(Vec3::Z, to_eye);
        tf.scale = Vec3::splat(m.size * (0.5 + u));
        if let Some(mut mat) = materials.get_mut(&m.material) {
            let a = (u * 4.0).min(1.0) * (1.0 - u) * 0.34;
            mat.base_color = Color::srgba(1.0, 1.0, 1.0, a);
        }
    }
}

/// Splash steps, landings and slides on the water.
#[allow(clippy::too_many_arguments)]
fn water_walk_fx(
    views: Res<WorldViews>,
    mut scene: ResMut<Scene>,
    mut wading: ResMut<Wading>,
    mut rng: ResMut<VfxRng>,
    mut ripples: Query<(&mut Ripple, &mut Transform, &mut Visibility)>,
    mut debris: MessageWriter<SpawnDebris>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    if scene.sheets.is_empty() || !scene.walk {
        return;
    }
    let r = &mut rng.0;
    for v in &views.players {
        let on = v.alive()
            && v.has(pflags::GROUNDED)
            && scene.sheets.iter().any(|s| (v.pos.y - s.center.y).abs() < 0.08 && s.covers(bd_voxel_v3(v.pos), 0.0));
        let entry = wading.players.entry(v.id).or_insert((v.pos, 0.0, false));
        let (last, dist, was_on) = *entry;
        let mut splash: Option<f32> = None;
        if on {
            let d = Vec2::new(v.pos.x - last.x, v.pos.z - last.z).length().min(3.0);
            let mut acc = dist + d;
            if !was_on {
                // Landing on / stepping onto the water.
                splash = Some(1.0 + (-v.vel.y).max(0.0) * 0.08);
                acc = 0.0;
            } else if acc >= STRIDE {
                splash = Some(if v.has(pflags::SLIDING) { 0.7 } else { 0.45 });
                acc = 0.0;
            }
            *entry = (v.pos, acc, true);
        } else {
            *entry = (v.pos, 0.0, false);
        }
        let Some(big) = splash else { continue };
        let p = v.pos;
        // Ripple ring.
        let n = ripples.iter().count();
        if n > 0 {
            let k = scene.next_ripple % n;
            scene.next_ripple += 1;
            if let Some((mut rp, mut tf, mut vis)) = ripples.iter_mut().nth(k) {
                rp.age = 0.0;
                rp.life = 0.7 + 0.4 * big;
                rp.size = 1.2 + 1.6 * big;
                tf.translation = Vec3::new(p.x, p.y + 0.03, p.z);
                tf.scale = Vec3::splat(0.2);
                *vis = Visibility::Visible;
            }
        }
        // Droplets.
        for _ in 0..(6.0 + 14.0 * big) as usize {
            let d = r.unit_vec3();
            debris.write(SpawnDebris {
                pos: p + Vec3::new(d.x * 0.3, 0.05, d.z * 0.3),
                vel: Vec3::new(d.x * 1.6, r.range(1.5, 3.5) * big.sqrt() + 1.0, d.z * 1.6) + v.vel * 0.25,
                color: [0.6, 0.85, 0.95],
                size: r.range(0.05, 0.12),
                emissive: 0.3,
                lifetime: r.range(0.35, 0.7),
            });
        }
        // Splashy footstep layer (the normal footstep plays too).
        let g = if v.is_local { 0.55 } else { 0.8 };
        sfx.write(Sfx3d::at("debris_patter", p).pitch(1.45 + r.range(-0.1, 0.1)).gain(g * (0.35 + 0.3 * big)));
        if big >= 0.9 {
            sfx.write(Sfx3d::at("land_light", p).pitch(0.75).gain(g * 0.7));
        }
    }
    if wading.players.len() > 64 {
        wading.players.clear();
    }
}

fn bd_voxel_v3(p: Vec3) -> bd_core::Vec3 {
    bd_core::Vec3::new(p.x, p.y, p.z)
}

fn animate_ripples(
    time: Res<Time<Real>>,
    mut q: Query<(&mut Ripple, &mut Transform, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    for (mut rp, mut tf, mut vis) in &mut q {
        if rp.age >= rp.life {
            continue;
        }
        rp.age += dt;
        let u = (rp.age / rp.life).min(1.0);
        tf.scale = Vec3::splat(0.2 + rp.size * (1.0 - (1.0 - u) * (1.0 - u)));
        if let Some(mut m) = materials.get_mut(&rp.material) {
            m.base_color = Color::srgba(0.9, 0.97, 1.0, 0.55 * (1.0 - u));
        }
        if u >= 1.0 {
            *vis = Visibility::Hidden;
        }
    }
}
