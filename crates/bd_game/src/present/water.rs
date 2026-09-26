//! City maps with a sea (Miami): a cheap animated water plane at `water_y` out to the horizon (teal,
//! PBR fresnel from a low-roughness surface, a procedurally generated tileable wave normal map
//! scrolled every frame — no reflections) and big splashes (VFX + sound) when players or
//! constructs hit the water; falling below it is the bottom blast zone.

use super::sound::Sfx3d;
use super::PresentSet;
use crate::arena::ArenaWorld;
use crate::debris::{SpawnDebris, VfxRng};
use crate::sim_bridge::WorldViews;
use bd_core::mechanics::Element;
use bd_core::PlayerId;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::math::Affine2;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bd_fx::{BurstFx, BurstKind, SpawnOnomatopoeia};

#[derive(Component)]
struct WaterPlane {
    material: Handle<StandardMaterial>,
}

/// Last height of everything that can splash.
#[derive(Resource, Default)]
struct Wet {
    players: HashMap<PlayerId, f32>,
    constructs: HashMap<bd_core::ConstructId, f32>,
}

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wet>().add_systems(Update, (spawn_water, animate_water, splashes).chain().in_set(PresentSet::Sync));
    }
}

/// 128² tangent-space normal map from a few tileable sine waves.
fn wave_normals() -> Image {
    const N: usize = 128;
    let mut data = Vec::with_capacity(N * N * 4);
    let tau = std::f32::consts::TAU;
    let waves: [(f32, f32, f32); 4] = [(1.0, 0.0, 0.35), (0.0, 2.0, 0.25), (3.0, 2.0, 0.12), (-2.0, 5.0, 0.08)];
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
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    img
}

fn spawn_water(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    existing: Query<(Entity, &Transform), With<WaterPlane>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let want = arena.as_ref().and_then(|a| a.info.water_y.map(|y| (y, a.info.water_color, a.info.center)));
    match (want, existing.single()) {
        (None, Ok((e, _))) => commands.entity(e).despawn(),
        (Some((y, _, _)), Ok((e, t))) if (t.translation.y - y).abs() > 0.01 => commands.entity(e).despawn(),
        (Some((y, colors, center)), Err(_)) => {
            let (shallow, _deep) = colors.unwrap_or(([0.1, 0.55, 0.6], [0.02, 0.2, 0.3]));
            let normal = images.add(wave_normals());
            let material = materials.add(StandardMaterial {
                base_color: Color::srgb(shallow[0], shallow[1], shallow[2]),
                perceptual_roughness: 0.07,
                metallic: 0.0,
                reflectance: 0.7,
                normal_map_texture: Some(normal),
                uv_transform: Affine2::IDENTITY,
                ..default()
            });
            let size = 4000.0;
            let mut mesh = Plane3d::default().mesh().size(size, size).build();
            // Tile the wave map every 16 m.
            if let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
                for uv in uvs.iter_mut() {
                    uv[0] *= size / 16.0;
                    uv[1] *= size / 16.0;
                }
            }
            let _ = mesh.generate_tangents();
            commands.spawn((
                Name::new("Water"),
                WaterPlane { material: material.clone() },
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::from_translation(Vec3::new(center.x, y, center.z)),
                NotShadowCaster,
            ));
        }
        _ => {}
    }
}

fn animate_water(time: Res<Time<Real>>, q: Query<&WaterPlane>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let t = time.elapsed_secs();
    for w in &q {
        if let Some(mut m) = materials.get_mut(&w.material) {
            m.uv_transform = Affine2::from_translation(Vec2::new(t * 0.013, t * 0.021));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn splashes(
    arena: Option<Res<ArenaWorld>>,
    views: Res<WorldViews>,
    mut wet: ResMut<Wet>,
    mut rng: ResMut<VfxRng>,
    mut burst: MessageWriter<BurstFx>,
    mut debris: MessageWriter<SpawnDebris>,
    mut sfx: MessageWriter<Sfx3d>,
    mut ono: MessageWriter<SpawnOnomatopoeia>,
) {
    let Some(y) = arena.and_then(|a| a.info.water_y) else { return };
    let r = &mut rng.0;
    let mut splash = |p: Vec3, big: f32, word: bool| {
        let p = Vec3::new(p.x, y, p.z);
        burst.write(BurstFx { kind: BurstKind::Element(Element::Water), pos: p, dir: Vec3::Y, scale: 1.2 * big, color: None });
        let n = (40.0 * big) as usize;
        for _ in 0..n {
            let d = r.unit_vec3();
            debris.write(SpawnDebris {
                pos: p + Vec3::new(d.x, 0.1, d.z) * 0.6,
                vel: Vec3::new(d.x * 3.0, r.range(5.0, 11.0) * big.sqrt(), d.z * 3.0),
                color: [0.55, 0.85, 0.95],
                size: r.range(0.08, 0.18),
                emissive: 0.4,
                lifetime: r.range(0.7, 1.3),
            });
        }
        sfx.write(Sfx3d::at("land_heavy", p).pitch(0.7).gain(big.min(1.2)));
        sfx.write(Sfx3d::at("debris_patter", p).gain(0.6));
        if word {
            ono.write(SpawnOnomatopoeia::key("water", p + Vec3::Y, big, [0.3, 0.6, 1.0]));
        }
    };
    for v in &views.players {
        let prev = wet.players.insert(v.id, v.pos.y).unwrap_or(v.pos.y);
        if prev > y + 0.2 && v.pos.y <= y + 0.2 {
            splash(v.pos, 1.6, true);
        }
    }
    for c in &views.constructs {
        if c.attached() {
            continue;
        }
        let prev = wet.constructs.insert(c.id, c.pos.y).unwrap_or(c.pos.y);
        if prev > y && c.pos.y <= y {
            splash(c.pos, 1.0 + (c.size - 1.0).max(0.0), false);
        }
    }
    if wet.constructs.len() > 512 {
        wet.constructs.clear();
    }
}
