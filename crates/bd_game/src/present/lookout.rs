//! **The Lookout** presentation (DESIGN.md "Maps: The Lookout"): the sea of clouds far below the
//! floating temple — the cue that sells the altitude. Visual only, never collides. Active only while
//! the loaded map is `MapKind::Lookout`; everything spawned here carries [`LookoutEntity`] and is torn
//! down when another map loads.
//!
//! * **Cloud sea** — one polar-grid mesh (rings spaced geometrically out to [`SEA_R`], so it is dense
//!   under the disc and coarse at the horizon) at `bd_voxel::maps::lookout::CLOUD_Y`, its vertices
//!   lifted into billows by a small value-noise fbm and shaded once from the map's sun (bright tops,
//!   blue-grey troughs, baked into vertex colours; unlit + fog, so it melts into the horizon haze).
//!   Korin's tower rises out of it. The sky dome's below-horizon colour is set to the haze while the
//!   map is loaded (restored after), so there is no seam past the mesh edge.
//! * **Cumulus** — [`PUFFS`] clusters of baked-shaded spheres rising out of the sea at 180–1100 m,
//!   for parallax and scale.
//! * **Drift** — the whole sea and its cumulus turn very slowly about the map's axis (≈0.5 m/s under
//!   the disc; a third of that with the comfort preset). No flicker, no flashes.
//!
//! Cost: ~21 k triangles for the sea, one shared ico-sphere mesh + material for ~110 puff spheres
//! (batched), one transform write per entity per frame; no per-frame allocation.

use super::PresentSet;
use crate::arena::{ArenaWorld, MapKind};
use crate::render::Environment;
use crate::settings::ComfortSettings;
use bd_voxel::maps::lookout::CLOUD_Y;
use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Radius of the cloud sea (the camera-following sky dome is 1500 m, the far plane 2000 m).
pub const SEA_R: f32 = 1450.0;
const RINGS: usize = 76;
const SEGS: usize = 144;
/// Cumulus clusters round the horizon.
pub const PUFFS: usize = 28;
/// Drift (rad/s) of the whole cloud field about the map's vertical axis.
const DRIFT: f32 = 0.0012;

/// Everything this module spawns (despawned when another map loads).
#[derive(Component)]
pub struct LookoutEntity;

/// The drifting part of the scene (sea mesh + puffs), turned about +Y through the origin.
#[derive(Component)]
struct Drifts {
    /// Rest transform (at drift angle 0).
    rest: Transform,
}

#[derive(Resource, Default)]
struct LookoutState {
    active: bool,
    saved_ground: Option<LinearRgba>,
}

pub struct LookoutPlugin;

impl Plugin for LookoutPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookoutState>().add_systems(Update, (sync_lookout, drift_clouds).chain().in_set(PresentSet::Sync));
    }
}

fn is_lookout(arena: &Option<Res<ArenaWorld>>) -> bool {
    arena.as_ref().is_some_and(|a| a.info.kind == MapKind::Lookout)
}

/// Tiny deterministic hash → [0, 1) (layout only; no gameplay).
fn h01(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h >> 8) as f32 / 16_777_216.0
}

/// Smooth value noise in [-1, 1].
fn vnoise(x: f32, z: f32, seed: u32) -> f32 {
    let (xi, zi) = (x.floor(), z.floor());
    let (fx, fz) = (x - xi, z - zi);
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let c = |dx: i32, dz: i32| h01((xi as i32 + dx) as u32 ^ seed.wrapping_mul(0x27d4_eb2d), (zi as i32 + dz) as u32) * 2.0 - 1.0;
    let a = c(0, 0) + (c(1, 0) - c(0, 0)) * sx;
    let b = c(0, 1) + (c(1, 1) - c(0, 1)) * sx;
    a + (b - a) * sz
}

fn fbm(x: f32, z: f32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut f) = (0.0, 0.5, 1.0);
    for o in 0..4 {
        sum += amp * vnoise(x * f, z * f, seed + o);
        amp *= 0.5;
        f *= 2.03;
    }
    sum
}

/// Billow height above the sea level at (x, z): big rolling domes plus finer lumps.
fn billow(x: f32, z: f32) -> f32 {
    let big = fbm(x / 140.0, z / 140.0, 11);
    let fine = fbm(x / 38.0, z / 38.0, 29);
    9.0 * (big * 0.5 + 0.5) * (big * 0.5 + 0.5) + 2.5 * fine
}

/// Baked cloud shading: bright where the surface faces the sun and on the tops, blue-grey in the
/// troughs and on the far side.
fn cloud_color(n: Vec3, height01: f32, sun: Vec3) -> [f32; 4] {
    let lit = n.dot(sun).max(0.0);
    let t = (0.35 + 0.45 * lit + 0.35 * height01).clamp(0.0, 1.0);
    let shadow = Vec3::new(0.52, 0.60, 0.76);
    let top = Vec3::new(0.97, 0.97, 0.98);
    let c = shadow.lerp(top, t);
    [c.x, c.y, c.z, 1.0]
}

fn sea_mesh(sun: Vec3) -> Mesh {
    let growth = (SEA_R / 6.0).powf(1.0 / (RINGS - 1) as f32);
    let radius = |i: usize| 6.0 * growth.powi(i as i32);
    let h = |x: f32, z: f32| CLOUD_Y + billow(x, z);
    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(1 + RINGS * SEGS);
    let mut nrm: Vec<[f32; 3]> = Vec::with_capacity(pos.capacity());
    let mut col: Vec<[f32; 4]> = Vec::with_capacity(pos.capacity());
    let push = |x: f32, z: f32, pos: &mut Vec<[f32; 3]>, nrm: &mut Vec<[f32; 3]>, col: &mut Vec<[f32; 4]>| {
        let y = h(x, z);
        let e = 2.0;
        let n = Vec3::new(h(x - e, z) - h(x + e, z), 2.0 * e, h(x, z - e) - h(x, z + e)).normalize();
        // Fade the relief out toward the horizon (it is fogged there anyway; keeps the edge flat).
        let fade = 1.0 - ((x * x + z * z).sqrt() / SEA_R).clamp(0.0, 1.0);
        let y = CLOUD_Y + (y - CLOUD_Y) * (0.3 + 0.7 * fade);
        pos.push([x, y, z]);
        nrm.push(n.to_array());
        col.push(cloud_color(n, ((y - CLOUD_Y) / 11.0).clamp(0.0, 1.0), sun));
    };
    push(0.0, 0.0, &mut pos, &mut nrm, &mut col);
    for i in 0..RINGS {
        let r = radius(i);
        for s in 0..SEGS {
            // Stagger alternate rings by half a segment (nicer triangles).
            let a = std::f32::consts::TAU * (s as f32 + 0.5 * (i % 2) as f32) / SEGS as f32;
            push(a.cos() * r, a.sin() * r, &mut pos, &mut nrm, &mut col);
        }
    }
    let ring = |i: usize, s: usize| (1 + i * SEGS + s % SEGS) as u32;
    let mut idx: Vec<u32> = Vec::with_capacity(RINGS * SEGS * 6);
    for s in 0..SEGS {
        idx.extend_from_slice(&[0, ring(0, s + 1), ring(0, s)]);
    }
    for i in 0..RINGS - 1 {
        for s in 0..SEGS {
            let (a, b, c, d) = (ring(i, s), ring(i, s + 1), ring(i + 1, s), ring(i + 1, s + 1));
            // Wound so the faces look up (+Y).
            idx.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

/// A unit ico-sphere with the cloud shading baked into its vertex colours.
fn puff_mesh(sun: Vec3) -> Mesh {
    let mut m = Sphere::new(1.0).mesh().ico(3).expect("ico sphere");
    let normals: Vec<[f32; 3]> = match m.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(n)) => n.clone(),
        _ => Vec::new(),
    };
    let col: Vec<[f32; 4]> = normals.iter().map(|n| cloud_color(Vec3::from_array(*n), (n[1] * 0.5 + 0.5).clamp(0.0, 1.0), sun)).collect();
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    m
}

fn sync_lookout(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    mut state: ResMut<LookoutState>,
    mut env: ResMut<Environment>,
    existing: Query<Entity, With<LookoutEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let want = is_lookout(&arena);
    if want == state.active {
        return;
    }
    state.active = want;
    if !want {
        for e in &existing {
            commands.entity(e).despawn();
        }
        if let Some(g) = state.saved_ground.take() {
            env.ground = g;
        }
        return;
    }
    // Below the horizon the sky dome shows the haze the sea fades into.
    state.saved_ground = Some(env.ground);
    env.ground = env.fog_color;
    let sun = env.sun_dir.normalize_or(Vec3::Y);
    let mat = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, fog_enabled: true, ..default() });
    let sea = Transform::IDENTITY;
    commands.spawn((
        Name::new("LookoutCloudSea"),
        LookoutEntity,
        Drifts { rest: sea },
        Mesh3d(meshes.add(sea_mesh(sun))),
        MeshMaterial3d(mat.clone()),
        sea,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    // Cumulus: a big dome and a few shoulders per cluster, sitting in the sea.
    let puff = meshes.add(puff_mesh(sun));
    for i in 0..PUFFS as u32 {
        let a = std::f32::consts::TAU * (i as f32 + 0.6 * h01(i, 1)) / PUFFS as f32;
        let d = 180.0 + 920.0 * h01(i, 2);
        let base = Vec3::new(a.cos() * d, CLOUD_Y + 4.0, a.sin() * d);
        let size = 16.0 + 30.0 * h01(i, 3) * (0.5 + d / 1100.0);
        let parts = 3 + (h01(i, 4) * 3.0) as u32;
        for k in 0..parts {
            let (ox, oz) = if k == 0 {
                (0.0, 0.0)
            } else {
                let b = std::f32::consts::TAU * h01(i * 7 + k, 5);
                (b.cos() * size * 0.85, b.sin() * size * 0.85)
            };
            let r = if k == 0 { size } else { size * (0.45 + 0.3 * h01(i * 7 + k, 6)) };
            let t = Transform::from_translation(base + Vec3::new(ox, r * 0.35, oz)).with_scale(Vec3::new(r, r * 0.72, r));
            commands.spawn((
                Name::new("LookoutCumulus"),
                LookoutEntity,
                Drifts { rest: t },
                Mesh3d(puff.clone()),
                MeshMaterial3d(mat.clone()),
                t,
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

fn drift_clouds(time: Res<Time<Real>>, state: Res<LookoutState>, comfort: Res<ComfortSettings>, mut q: Query<(&Drifts, &mut Transform)>) {
    if !state.active {
        return;
    }
    let speed = if comfort.speed_fov { 1.0 } else { 0.33 };
    let rot = Quat::from_rotation_y(time.elapsed_secs() * DRIFT * speed);
    for (d, mut t) in &mut q {
        t.translation = rot * d.rest.translation;
        t.rotation = rot * d.rest.rotation;
    }
}
