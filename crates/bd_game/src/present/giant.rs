//! **Giant's Room** presentation (DESIGN.md "Maps: Giant's Room"): the afternoon sunbeam through the
//! open window and the big dust motes drifting in it — the two cues (with the haze) that sell the
//! scale of the room. Visual only, never collides. Active only while the loaded map is
//! `MapKind::GiantsRoom`; everything spawned here carries [`GiantEntity`] and is torn down when
//! another map loads.
//!
//! * **Sunbeam** — the window rectangle swept along the sun direction down to the floor
//!   (`bd_voxel::maps::giant::sun_dir`, the same vector the key light uses, so the shaft lands on
//!   the lit patch): three nested, double-sided, additive, unlit shells whose vertex alpha fades
//!   from the window to the floor. Fog off (additive + fog would glow twice).
//! * **Dust motes** — [`MOTES`] tiny warm specks living in beam coordinates (across, up, along), each
//!   wandering slowly on its own sines; they only exist inside the shaft, so they read as "lit
//!   dust". Slow, no flicker (comfort-safe); with the comfort preset (speed FOV off) they drift at a
//!   third of the speed.
//!
//! Cost: 3 × 8 triangles for the beam, one shared cube mesh + material for the motes (batched),
//! [`MOTES`] transform writes a frame; zero per-frame allocation.

use super::PresentSet;
use crate::arena::{ArenaWorld, MapKind};
use crate::settings::ComfortSettings;
use bd_voxel::maps::giant::{FLOOR_Y, HX, WINDOW_Y, WINDOW_Z, sun_dir};
use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Dust motes in the beam (cap).
pub const MOTES: usize = 220;

/// Everything this module spawns (despawned when another map loads).
#[derive(Component)]
pub struct GiantEntity;

#[derive(Component)]
struct Mote {
    /// Beam coordinates: across the window (0..1), up the window (0..1), along the shaft (0..1).
    base: Vec3,
    /// Wander amplitudes and angular rates, phase.
    amp: Vec3,
    rate: Vec3,
    phase: f32,
    size: f32,
}

#[derive(Resource, Default)]
struct GiantState {
    active: bool,
}

pub struct GiantPlugin;

impl Plugin for GiantPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GiantState>().add_systems(Update, (sync_giant, drift_motes).chain().in_set(PresentSet::Sync));
    }
}

fn is_giant(arena: &Option<Res<ArenaWorld>>) -> bool {
    arena.as_ref().is_some_and(|a| a.info.kind == MapKind::GiantsRoom)
}

fn v3(v: bd_core::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// A point of the shaft from beam coordinates: `u` across the window (z), `v` up it (y), `w` from the
/// window (0) to where that ray meets the floor (1).
fn beam_point(u: f32, v: f32, w: f32, sun: Vec3) -> Vec3 {
    let z = WINDOW_Z.0 + (WINDOW_Z.1 - WINDOW_Z.0) * u;
    let y = WINDOW_Y.0 + (WINDOW_Y.1 - WINDOW_Y.0) * v;
    let p = Vec3::new(HX, y, z);
    let t = (y - (FLOOR_Y + 0.5)) / sun.y.max(0.05);
    p - sun * (t * w)
}

/// One shell of the shaft: the four long sides, shrunk towards the middle by `inset` (0..0.5 in
/// window fractions). Vertex alpha fades from `a0` at the window to `a1` at the floor.
fn beam_mesh(sun: Vec3, inset: f32, a0: f32, a1: f32) -> Mesh {
    let (lo, hi) = (inset, 1.0 - inset);
    let corners = [(lo, lo), (hi, lo), (hi, hi), (lo, hi)];
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nrm: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        let quad = [beam_point(a.0, a.1, 0.0, sun), beam_point(b.0, b.1, 0.0, sun), beam_point(b.0, b.1, 1.0, sun), beam_point(a.0, a.1, 1.0, sun)];
        let n = (quad[1] - quad[0]).cross(quad[3] - quad[0]).normalize_or_zero();
        let base = pos.len() as u32;
        for (i, p) in quad.iter().enumerate() {
            pos.push(p.to_array());
            nrm.push(n.to_array());
            let a = if i < 2 { a0 } else { a1 };
            col.push([1.0, 1.0, 1.0, a]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

/// Tiny deterministic hash → [0, 1) (layout only; no gameplay).
fn h01(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h >> 8) as f32 / 16_777_216.0
}

fn sync_giant(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    mut state: ResMut<GiantState>,
    existing: Query<Entity, With<GiantEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let want = is_giant(&arena);
    if want == state.active {
        return;
    }
    state.active = want;
    if !want {
        for e in &existing {
            commands.entity(e).despawn();
        }
        return;
    }
    let sun = v3(sun_dir());
    // Warm afternoon light; three nested shells give the shaft a soft edge and a brighter core.
    for (k, (inset, a0, a1, glow)) in [(0.0f32, 0.055f32, 0.012f32, 1.0f32), (0.12, 0.05, 0.012, 1.1), (0.26, 0.06, 0.015, 1.25)].iter().enumerate() {
        let mat = materials.add(StandardMaterial {
            base_color: Color::LinearRgba(LinearRgba::new(1.0 * glow, 0.80 * glow, 0.52 * glow, 1.0)),
            unlit: true,
            fog_enabled: false,
            alpha_mode: AlphaMode::Add,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            Name::new(format!("GiantSunbeam{k}")),
            GiantEntity,
            Mesh3d(meshes.add(beam_mesh(sun, *inset, *a0, *a1))),
            MeshMaterial3d(mat),
            Transform::IDENTITY,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    // Dust motes: one mesh + material for all of them (batched).
    let mote_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let mote_mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(2.6, 2.2, 1.6, 0.85)),
        unlit: true,
        fog_enabled: false,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    for i in 0..MOTES as u32 {
        let base = Vec3::new(h01(i, 1), h01(i, 2), 0.08 + 0.9 * h01(i, 3).powf(0.8));
        let amp = Vec3::new(0.02 + 0.03 * h01(i, 4), 0.02 + 0.03 * h01(i, 5), 0.01 + 0.02 * h01(i, 6));
        let rate = Vec3::new(0.05 + 0.1 * h01(i, 7), 0.04 + 0.08 * h01(i, 8), 0.03 + 0.06 * h01(i, 9));
        let size = 0.05 + 0.09 * h01(i, 10) * h01(i, 11);
        commands.spawn((
            Name::new("GiantDustMote"),
            GiantEntity,
            Mote { base, amp, rate, phase: h01(i, 12) * std::f32::consts::TAU, size },
            Mesh3d(mote_mesh.clone()),
            MeshMaterial3d(mote_mat.clone()),
            Transform::from_translation(beam_point(base.x, base.y, base.z, sun)).with_scale(Vec3::splat(size)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

fn drift_motes(time: Res<Time<Real>>, state: Res<GiantState>, comfort: Res<ComfortSettings>, mut motes: Query<(&Mote, &mut Transform)>) {
    if !state.active {
        return;
    }
    let speed = if comfort.speed_fov { 1.0 } else { 0.33 };
    let t = time.elapsed_secs() * speed;
    let sun = v3(sun_dir());
    for (m, mut tf) in &mut motes {
        let ph = m.phase;
        let u = (m.base.x + m.amp.x * (t * m.rate.x + ph).sin()).clamp(0.02, 0.98);
        let v = (m.base.y + m.amp.y * (t * m.rate.y + ph * 1.7).sin()).clamp(0.02, 0.98);
        let w = (m.base.z + m.amp.z * (t * m.rate.z + ph * 2.3).sin()).clamp(0.02, 0.98);
        tf.translation = beam_point(u, v, w, sun);
        // A slow tumble so the specks catch the light (no flicker).
        tf.rotation = Quat::from_euler(EulerRot::XYZ, t * 0.3 + ph, t * 0.21 + ph * 0.5, 0.0);
        tf.scale = Vec3::splat(m.size);
    }
}
