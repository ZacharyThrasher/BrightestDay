//! **Hyperline** presentation (DESIGN.md "Maps: Hyperline"): "the train stands still, the world
//! moves". Active while the loaded map carries a slipstream (`ArenaInfo::slipstream`); everything
//! spawned here carries [`HyperEntity`] and is torn down when another map loads. Visual only —
//! nothing here touches the sim.
//!
//! * **City shaft** — two walls of towers (a few vertex-coloured mesh variants, window bands that
//!   glow), tall vertical light rails and scaffold bridges spanning the shaft far above and below the
//!   track, all streaming back *and down* (the train is climbing) and wrapping in a big box domain
//!   whose seams sit deep in the fog.
//! * **Ring gates** — huge concentric rings around the track (inner radius ≥ 36 m: always far clear of
//!   the train and anyone on it), counter-spinning, sweeping over the train every few seconds; each
//!   plays a doppler whoosh as it passes the camera.
//! * **Guideway** — the glowing rail runs on to the horizon ahead and behind; marker bars and pylons
//!   stream past under and beside the train.
//! * **Speed streaks** — thin light streaks rushing past in the air around the camera, and faint
//!   **wind lines** at the screen edges (stronger in the slipstream); **wind trails** on anyone the
//!   sweep is carrying off; **sparks** spraying from the bogies near the camera.
//! * **Comfort** — with the comfort preset (speed FOV off / FOV kicks 0) the backdrop scrolls at a
//!   third of the speed and the streaks / wind lines are off (no peripheral vection).
//!
//! Cost: ~650 entities of a dozen shared meshes (auto-batched), transform writes only, no shadows,
//! no lights, a capped trickle of debris particles.

use super::sound::Sfx3d;
use super::PresentSet;
use crate::arena::ArenaWorld;
use crate::camera::PlayerCamera;
use crate::debris::{SpawnDebris, VfxRng};
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, WorldViews};
use crate::util::Rng;
use bd_voxel::maps::hyperline;
use bd_voxel::{SlipZone, Slipstream};
use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// Visual train speed (m/s, ≈ 500 km/h).
const SPEED: f32 = 140.0;
/// The city streams back and *down* at this angle (the train is climbing the shaft).
const CLIMB_DEG: f32 = 13.0;
/// City domain (m): x span, y range; both walls at |z| in `WALL_Z`.
const CITY_X: f32 = 1800.0;
const CITY_Y: (f32, f32) = (-560.0, 440.0);
const WALL_Z: (f32, f32) = (62.0, 300.0);
const TOWERS: usize = 420;
const RAILS: usize = 48;
const BRIDGES: usize = 30;
/// Ring gates: count, spacing along x, outer radius, centre height.
const RINGS: usize = 4;
const RING_SPACING: f32 = 560.0;
const RING_R: f32 = 46.0;
const RING_Y: f32 = 12.0;
const STREAKS: usize = 140;
const STREAK_X: f32 = 260.0;
const WIND_LINES: usize = 24;
const MARKERS: usize = 64;
const MARKER_SPACING: f32 = 18.0;
const PYLONS: usize = 24;
const PYLON_SPACING: f32 = 60.0;

/// Everything this module spawns (despawned when another map loads).
#[derive(Component)]
pub struct HyperEntity;

/// Streams diagonally with the city (wraps in the city domain).
#[derive(Component)]
struct CityPiece;

/// Streams along −X with the track, wrapping in `[-span/2, span/2)` around `centre`.
#[derive(Component)]
struct TrackPiece {
    span: f32,
    /// Hidden while over the voxel train (the voxel guideway is there).
    hide_over_train: bool,
}

#[derive(Component)]
struct RingGate {
    spin: f32,
    parts: [Entity; 2],
}

#[derive(Component)]
struct Streak {
    x: f32,
    y: f32,
    z: f32,
    len: f32,
    speed: f32,
}

#[derive(Component)]
struct WindLine(usize);

/// A ring gate's spinning part (positioned by its [`RingGate`] every frame).
#[derive(Component)]
struct RingPart;

#[derive(Resource, Default)]
struct HyperState {
    active: bool,
    slip: Option<Slipstream>,
    streak_mat: Option<Handle<StandardMaterial>>,
    line_mat: Option<Handle<StandardMaterial>>,
    /// Bogie spark emitters: (x, z side) at the wheel band.
    bogies: Vec<Vec3>,
    spark_acc: f32,
    trail_acc: f32,
    /// Current comfort factor (0 = comfort preset, 1 = full speed feel).
    comfort_k: f32,
    time: f32,
}

pub struct HyperlinePlugin;

impl Plugin for HyperlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Sfx3d>()
            .add_message::<SpawnDebris>()
            .init_resource::<HyperState>()
            .add_systems(Update, (sync_hyper, sparks_and_trails).chain().in_set(PresentSet::Sync))
            .add_systems(Update, dev_hyper_test.before(crate::tick::FrameSet::Sim))
            .add_systems(PostUpdate, scroll_backdrop.after(crate::camera::CameraUpdateSet).before(TransformSystems::Propagate));
    }
}

/// Every backdrop entity: torn down with the map, never in shadows.
fn tag() -> (HyperEntity, NotShadowCaster, NotShadowReceiver) {
    (HyperEntity, NotShadowCaster, NotShadowReceiver)
}

fn slip_of(arena: &Option<Res<ArenaWorld>>) -> Option<Slipstream> {
    arena.as_ref().and_then(|a| a.info.arena.as_ref()).and_then(|i| i.slipstream)
}

#[inline]
fn wrap(v: f32, lo: f32, span: f32) -> f32 {
    lo + (v - lo).rem_euclid(span)
}

fn climb() -> Vec3 {
    let a = CLIMB_DEG.to_radians();
    Vec3::new(-a.cos(), -a.sin(), 0.0)
}

// ------------------------------------------------------------------------------------ meshes

/// Vertex-coloured mesh builder (unlit: the colour *is* the look; > 1 blooms).
#[derive(Default)]
struct Vc {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Vc {
    fn quad(&mut self, o: Vec3, u: Vec3, v: Vec3, c: [f32; 3]) {
        let n = u.cross(v).normalize_or_zero();
        let b = self.pos.len() as u32;
        for p in [o, o + u, o + u + v, o + v] {
            self.pos.push(p.to_array());
            self.nrm.push(n.to_array());
            self.col.push([c[0], c[1], c[2], 1.0]);
        }
        self.idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    /// Axis box `min..max` with a per-face colour function (face normal index 0..6).
    fn cuboid(&mut self, min: Vec3, max: Vec3, c: [f32; 3]) {
        let d = max - min;
        let (x, y, z) = (Vec3::X * d.x, Vec3::Y * d.y, Vec3::Z * d.z);
        self.quad(min, z, y, c); // −X
        self.quad(min + x, y, z, c); // +X
        self.quad(min, x, z, c); // −Y
        self.quad(min + y, z, x, c); // +Y
        self.quad(min, y, x, c); // −Z
        self.quad(min + z, x, y, c); // +Z
    }
    fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
            .with_inserted_indices(Indices::U32(self.idx))
    }
}

fn mul(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

/// A tower of unit footprint (1 × 1, scaled per instance) and `h` metres: painterly blue body,
/// glowing window bands every 8 m, an optional vertical light stripe per face.
fn tower_mesh(h: f32, body: [f32; 3], window: [f32; 3], stripe: bool) -> Mesh {
    let mut m = Vc::default();
    let floors = (h / 8.0) as i32;
    let faces: [(Vec3, Vec3, f32); 4] = [
        (Vec3::new(-0.5, 0.0, 0.5), Vec3::X, 1.0),
        (Vec3::new(0.5, 0.0, -0.5), Vec3::NEG_X, 0.8),
        (Vec3::new(0.5, 0.0, 0.5), Vec3::NEG_Z, 0.9),
        (Vec3::new(-0.5, 0.0, -0.5), Vec3::Z, 0.75),
    ];
    for (o, u, shade) in faces {
        for f in 0..floors {
            let y0 = f as f32 * 8.0;
            let lit = (f * 7 + (o.x * 3.0) as i32).rem_euclid(5) != 0;
            m.quad(o + Vec3::Y * y0, u, Vec3::Y * 5.5, mul(body, shade * (0.92 + 0.08 * (f % 3) as f32)));
            let w = if lit { window } else { mul(body, 1.3) };
            m.quad(o + Vec3::Y * (y0 + 5.5), u, Vec3::Y * 2.5, mul(w, shade.max(0.85)));
        }
        if stripe {
            let s = o + u * 0.45;
            let n = u.cross(Vec3::Y);
            m.quad(s + n * 0.01, u * 0.1, Vec3::Y * (floors as f32 * 8.0), [1.2, 2.6, 3.0]);
        }
    }
    let top = floors as f32 * 8.0;
    m.quad(Vec3::new(-0.5, top, -0.5), Vec3::Z, Vec3::X, mul(body, 1.2));
    m.build()
}

/// A scaffold bridge across the shaft (along Z, 600 m): a truss body with a light strip.
fn bridge_mesh() -> Mesh {
    let mut m = Vc::default();
    let body = [0.30, 0.50, 0.66];
    m.cuboid(Vec3::new(-2.0, -1.5, -300.0), Vec3::new(2.0, 1.5, 300.0), body);
    m.cuboid(Vec3::new(-2.2, 1.5, -300.0), Vec3::new(-1.6, 2.1, 300.0), [1.3, 2.8, 3.2]);
    let mut z = -300.0;
    while z < 300.0 {
        m.cuboid(Vec3::new(-2.6, -3.5, z), Vec3::new(2.6, -1.5, z + 1.2), mul(body, 0.8));
        z += 12.0;
    }
    m.build()
}

/// A vertical light rail (a tall glowing line on the shaft walls).
fn rail_mesh() -> Mesh {
    let mut m = Vc::default();
    m.cuboid(Vec3::new(-0.6, 0.0, -0.6), Vec3::new(0.6, 420.0, 0.6), [1.6, 3.0, 3.4]);
    m.cuboid(Vec3::new(-2.0, 0.0, -2.0), Vec3::new(2.0, 420.0, -0.6), [0.28, 0.46, 0.62]);
    m.build()
}

/// Ring-gate panel clamp (a chunky block on the rim) with a glowing edge.
fn clamp_mesh() -> Mesh {
    let mut m = Vc::default();
    m.cuboid(Vec3::new(-4.0, -3.0, -5.0), Vec3::new(4.0, 3.0, 5.0), [0.86, 0.93, 0.98]);
    m.cuboid(Vec3::new(-4.2, 2.9, -5.2), Vec3::new(4.2, 3.4, 5.2), [0.55, 1.9, 1.5]);
    m.build()
}

// ------------------------------------------------------------------------------------ spawn / tear down

#[allow(clippy::too_many_arguments)]
fn sync_hyper(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    mut state: ResMut<HyperState>,
    existing: Query<Entity, With<HyperEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let slip = slip_of(&arena);
    let want = slip.is_some();
    if want == state.active {
        return;
    }
    state.active = want;
    state.slip = slip;
    if !want {
        for e in &existing {
            commands.entity(e).despawn();
        }
        state.bogies.clear();
        return;
    }
    let mut rng = Rng::new(0x6879_7065_726c_696e);
    let vc = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() });
    let unlit = |materials: &mut Assets<StandardMaterial>, c: LinearRgba| materials.add(StandardMaterial { base_color: Color::LinearRgba(c), unlit: true, ..default() });
    let ring_white = unlit(&mut materials, LinearRgba::rgb(0.80, 0.90, 0.98));
    let ring_glow = unlit(&mut materials, LinearRgba::rgb(0.9, 3.2, 2.4));
    let orange = unlit(&mut materials, LinearRgba::rgb(9.0, 2.6, 0.45));
    let dark = unlit(&mut materials, LinearRgba::rgb(0.05, 0.07, 0.10));
    let add = |c: LinearRgba| StandardMaterial { base_color: Color::LinearRgba(c), unlit: true, fog_enabled: false, alpha_mode: AlphaMode::Add, ..default() };
    let streak_mat = materials.add(add(LinearRgba::new(0.85, 0.95, 1.0, 0.0)));
    let line_mat = materials.add(add(LinearRgba::new(0.9, 0.97, 1.0, 0.0)));
    state.streak_mat = Some(streak_mat.clone());
    state.line_mat = Some(line_mat.clone());

    // City towers: six variants (height, palette), instanced with per-instance footprint scale.
    let palettes: [([f32; 3], [f32; 3], bool); 6] = [
        ([0.18, 0.38, 0.58], [0.80, 1.35, 1.60], false),
        ([0.24, 0.48, 0.68], [1.00, 1.60, 1.80], true),
        ([0.14, 0.30, 0.50], [0.70, 1.20, 1.55], false),
        ([0.30, 0.56, 0.74], [1.20, 1.80, 1.90], false),
        ([0.20, 0.44, 0.60], [1.60, 1.30, 0.90], true),
        ([0.26, 0.52, 0.72], [0.90, 1.50, 1.70], false),
    ];
    let heights = [120.0, 200.0, 160.0, 260.0, 96.0, 180.0];
    let towers: Vec<Handle<Mesh>> = palettes.iter().zip(heights).map(|(&(b, w, s), h)| meshes.add(tower_mesh(h, b, w, s))).collect();
    for i in 0..TOWERS {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        // Denser near the track (the shaft's inner wall), thinning out behind.
        let z = side * (WALL_Z.0 + (WALL_Z.1 - WALL_Z.0) * rng.f32().powf(1.6));
        let x = rng.range(-CITY_X * 0.5, CITY_X * 0.5);
        let y = rng.range(CITY_Y.0, CITY_Y.1);
        let (w, d) = (rng.range(14.0, 42.0), rng.range(14.0, 42.0));
        let v = (rng.next_u32() as usize) % towers.len();
        commands.spawn((
            tag(),
            CityPiece,
            Mesh3d(towers[v].clone()),
            MeshMaterial3d(vc.clone()),
            Transform::from_xyz(x, y, z).with_scale(Vec3::new(w, 1.0, d)),
        ));
    }
    let rail = meshes.add(rail_mesh());
    for i in 0..RAILS {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let (x, y, z) = (rng.range(-CITY_X * 0.5, CITY_X * 0.5), rng.range(CITY_Y.0, CITY_Y.1), side * rng.range(WALL_Z.0 - 4.0, 140.0));
        commands.spawn((tag(), CityPiece, Mesh3d(rail.clone()), MeshMaterial3d(vc.clone()), Transform::from_xyz(x, y, z)));
    }
    let bridge = meshes.add(bridge_mesh());
    for i in 0..BRIDGES {
        // Far above the star-KO line or below the void: never near a player.
        let y = if i % 2 == 0 { rng.range(110.0, CITY_Y.1) } else { rng.range(CITY_Y.0, -50.0) };
        let x = rng.range(-CITY_X * 0.5, CITY_X * 0.5);
        commands.spawn((tag(), CityPiece, Mesh3d(bridge.clone()), MeshMaterial3d(vc.clone()), Transform::from_xyz(x, y, 0.0).with_rotation(Quat::from_rotation_y(rng.range(-0.25, 0.25)))));
    }

    // Ring gates (axis along X): an outer panelled ring with clamps, an inner glowing ring,
    // orange nodes. Inner radius ≥ 36 m round a centre at the roofs: the train and anyone near it
    // are always inside the clear bore.
    let outer = meshes.add(Torus::new(RING_R - 3.5, RING_R).mesh().major_resolution(64).minor_resolution(10).build());
    let inner = meshes.add(Torus::new(RING_R - 8.8, RING_R - 7.6).mesh().major_resolution(64).minor_resolution(6).build());
    let clamp = meshes.add(clamp_mesh());
    let node = meshes.add(Sphere::new(1.1).mesh().ico(1).unwrap_or_else(|_| Sphere::new(1.1).mesh().build()));
    let axis = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2); // torus axis Y → X
    for r in 0..RINGS {
        let x = -(RINGS as f32) * RING_SPACING * 0.5 + r as f32 * RING_SPACING + 180.0;
        let spin_part = |c: &mut Commands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>| c.spawn((tag(), RingPart, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_rotation(axis))).id();
        let a = spin_part(&mut commands, &outer, &ring_white);
        let b = spin_part(&mut commands, &inner, &ring_glow);
        // Clamps + nodes on the outer ring (children of `a`, placed in its local XZ plane).
        let k = 8;
        commands.entity(a).with_children(|c| {
            for j in 0..k {
                let t = j as f32 / k as f32 * std::f32::consts::TAU;
                let p = Vec3::new(t.cos(), 0.0, t.sin()) * (RING_R - 1.7);
                c.spawn((NotShadowCaster, NotShadowReceiver, Mesh3d(clamp.clone()), MeshMaterial3d(vc.clone()), Transform::from_translation(p).looking_to(p.normalize(), Vec3::Y)));
                let q = Vec3::new((t + 0.39).cos(), 0.0, (t + 0.39).sin()) * (RING_R - 1.7);
                c.spawn((NotShadowCaster, NotShadowReceiver, Mesh3d(node.clone()), MeshMaterial3d(orange.clone()), Transform::from_translation(q + Vec3::Y * 2.8)));
            }
        });
        commands.spawn((tag(), RingGate { spin: if r % 2 == 0 { 0.12 } else { -0.09 }, parts: [a, b] }, TrackPiece { span: RINGS as f32 * RING_SPACING, hide_over_train: false }, Transform::from_xyz(x, RING_Y, 0.0), Visibility::default()));
    }

    // The guideway on to the horizon (static glowing beam either side of the train), streaming
    // marker bars on it and pylons under it.
    let (x0, x1) = slip.map_or((-120.0, 120.0), |s| (s.min.x, s.max.x));
    let beam = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    for (a, b) in [(-3000.0, x0), (x1, 3000.0)] {
        let c = (a + b) * 0.5;
        let len = b - a;
        commands.spawn((tag(), Mesh3d(beam.clone()), MeshMaterial3d(orange.clone()), Transform::from_xyz(c, 4.75, 0.0).with_scale(Vec3::new(len, 0.5, 2.0))));
        commands.spawn((tag(), Mesh3d(beam.clone()), MeshMaterial3d(dark.clone()), Transform::from_xyz(c, 4.0, 0.0).with_scale(Vec3::new(len, 1.0, 1.9))));
    }
    let marker = meshes.add(Cuboid::new(0.6, 0.12, 2.6));
    let white = unlit(&mut materials, LinearRgba::rgb(2.5, 2.8, 3.0));
    for i in 0..MARKERS {
        let x = (i as f32 - MARKERS as f32 * 0.5) * MARKER_SPACING;
        commands.spawn((tag(), TrackPiece { span: MARKERS as f32 * MARKER_SPACING, hide_over_train: true }, Mesh3d(marker.clone()), MeshMaterial3d(white.clone()), Transform::from_xyz(x, 5.06, 0.0)));
    }
    let pylon = meshes.add(Cuboid::new(3.0, 1.0, 3.0));
    for i in 0..PYLONS {
        let x = (i as f32 - PYLONS as f32 * 0.5) * PYLON_SPACING;
        commands.spawn((tag(), TrackPiece { span: PYLONS as f32 * PYLON_SPACING, hide_over_train: false }, Mesh3d(pylon.clone()), MeshMaterial3d(dark.clone()), Transform::from_xyz(x, -200.0, 0.0).with_scale(Vec3::new(1.0, 406.0, 1.0))));
    }

    // Speed streaks around the track and wind lines around the camera (hidden until placed).
    let streak = meshes.add(Cuboid::new(0.07, 0.07, 1.0));
    for _ in 0..STREAKS {
        let a = rng.range(0.0, std::f32::consts::TAU);
        let r = rng.range(9.0, 70.0);
        commands.spawn((
            tag(),
            Streak { x: rng.range(-STREAK_X, STREAK_X), y: RING_Y + a.sin() * r, z: a.cos() * r, len: rng.range(6.0, 28.0), speed: rng.range(1.2, 1.7) },
            Mesh3d(streak.clone()),
            MeshMaterial3d(streak_mat.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
    let line = meshes.add(Cuboid::new(0.02, 0.02, 1.0));
    for k in 0..WIND_LINES {
        commands.spawn((tag(), WindLine(k), Mesh3d(line.clone()), MeshMaterial3d(line_mat.clone()), Transform::default(), Visibility::Hidden));
    }

    // Spark emitters at the bogie wheel bands (both sides, both ends of every unit).
    state.bogies.clear();
    for u in hyperline::units(1) {
        for lx in [3.5f32, u.len() - 3.5] {
            for s in [-1.0f32, 1.0] {
                state.bogies.push(Vec3::new(u.x0 + lx, 6.25, s * 3.05));
            }
        }
    }
}

// ------------------------------------------------------------------------------------ per frame

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn scroll_backdrop(
    time: Res<Time<Real>>,
    comfort: Res<ComfortSettings>,
    mut state: ResMut<HyperState>,
    active: Option<Res<ActiveWorld>>,
    views: Res<WorldViews>,
    cam: Query<&Transform, (With<PlayerCamera>, Without<HyperEntity>)>,
    mut city: Query<&mut Transform, (With<CityPiece>, Without<PlayerCamera>, Without<TrackPiece>, Without<Streak>, Without<WindLine>)>,
    mut track: Query<(&TrackPiece, &mut Transform, Option<&mut Visibility>, Option<&RingGate>), (Without<CityPiece>, Without<PlayerCamera>, Without<Streak>, Without<WindLine>)>,
    mut parts: Query<&mut Transform, (With<RingPart>, Without<TrackPiece>, Without<CityPiece>, Without<PlayerCamera>, Without<Streak>, Without<WindLine>)>,
    mut streaks: Query<(&mut Streak, &mut Transform, &mut Visibility), (Without<CityPiece>, Without<TrackPiece>, Without<PlayerCamera>, Without<WindLine>)>,
    mut lines: Query<(&WindLine, &mut Transform, &mut Visibility), (Without<CityPiece>, Without<TrackPiece>, Without<PlayerCamera>, Without<Streak>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut sfx: MessageWriter<Sfx3d>,
) {
    if !state.active {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    state.time += dt;
    let comfort_k = if comfort.speed_fov { comfort.fov_kick.clamp(0.0, 1.0) } else { 0.0 };
    state.comfort_k = comfort_k;
    let speed = SPEED * (0.33 + 0.67 * comfort_k);
    let Ok(cam) = cam.single() else { return };
    let eye = cam.translation;

    // City: diagonal stream, wrapped in the domain.
    let d = climb() * speed * dt;
    let span_y = CITY_Y.1 - CITY_Y.0;
    for mut t in &mut city {
        t.translation += d;
        t.translation.x = wrap(t.translation.x, -CITY_X * 0.5, CITY_X);
        t.translation.y = wrap(t.translation.y, CITY_Y.0, span_y);
    }
    // Track pieces: along −X; rings whoosh as they pass the camera.
    let (tx0, tx1) = state.slip.map_or((0.0, 0.0), |s| (s.min.x, s.max.x));
    for (tp, mut t, vis, ring) in &mut track {
        let before = t.translation.x;
        let x = wrap(before - speed * dt, -tp.span * 0.5, tp.span);
        t.translation.x = x;
        if let Some(mut v) = vis
            && tp.hide_over_train
        {
            *v = if (tx0 - 1.0..tx1 + 1.0).contains(&x) { Visibility::Hidden } else { Visibility::Inherited };
        }
        if let Some(r) = ring {
            // Crossing the camera from ahead (no wrap jump).
            if before > eye.x && x <= eye.x && before - x < tp.span * 0.5 {
                sfx.write(Sfx3d::at("train_ring_whoosh", Vec3::new(eye.x, eye.y + 10.0, eye.z)).gain(0.9));
            }
            let ang = state.time * r.spin;
            for (k, e) in r.parts.iter().enumerate() {
                if let Ok(mut pt) = parts.get_mut(*e) {
                    pt.translation = t.translation;
                    let s = if k == 0 { ang } else { -ang * 1.6 };
                    pt.rotation = Quat::from_rotation_x(s) * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
                }
            }
        }
    }

    // Local player's wind: calm / drag / sweep → wind line strength.
    let me = active.as_ref().and_then(|a| a.local).and_then(|id| views.player(id)).filter(|v| v.alive());
    let zone_k = match (me, state.slip) {
        (Some(v), Some(s)) => match s.zone(bd_core::Vec3::from_array(v.pos.to_array())) {
            SlipZone::Sheltered => {
                if v.pos.y > s.roof_y - 4.0 {
                    0.3
                } else {
                    0.0
                }
            }
            SlipZone::Roof => 0.6,
            SlipZone::Sweep(k) => 0.6 + 0.4 * k,
        },
        _ => 0.3,
    };
    let streak_a = 0.22 * comfort_k;
    let line_a = 0.16 * comfort_k * zone_k;
    if let Some(h) = state.streak_mat.as_ref()
        && let Some(mut m) = materials.get_mut(h)
    {
        m.base_color = Color::LinearRgba(LinearRgba::new(0.85, 0.95, 1.0, streak_a));
    }
    if let Some(h) = state.line_mat.as_ref()
        && let Some(mut m) = materials.get_mut(h)
    {
        m.base_color = Color::LinearRgba(LinearRgba::new(0.9, 0.97, 1.0, line_a));
    }
    // Streaks: a camera-centred band of air, streaming back fast.
    for (mut s, mut t, mut vis) in &mut streaks {
        if streak_a <= 0.01 {
            *vis = Visibility::Hidden;
            continue;
        }
        s.x = wrap(s.x - speed * s.speed * dt, -STREAK_X, 2.0 * STREAK_X);
        *vis = Visibility::Inherited;
        t.translation = Vec3::new(eye.x + s.x, s.y, s.z);
        t.rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        t.scale = Vec3::new(1.0, 1.0, s.len);
    }
    // Wind lines: a loose ring around the eye, kept out of the centre of view.
    let fwd = cam.rotation * Vec3::NEG_Z;
    let tt = state.time;
    for (l, mut t, mut vis) in &mut lines {
        if line_a <= 0.005 {
            *vis = Visibility::Hidden;
            continue;
        }
        let i = l.0 as f32;
        let h = (i * 0.618_034).fract();
        let ang = i / WIND_LINES as f32 * std::f32::consts::TAU + h;
        let rad = 2.4 + 2.4 * (i * 0.371).fract();
        let phase = (h + tt * (2.2 + h)).fract();
        let along = 12.0 - 26.0 * phase;
        let p = eye + Vec3::new(along, ang.sin() * rad, ang.cos() * rad);
        let to = (p - eye).normalize_or_zero();
        if to.dot(fwd) > 0.8 {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        t.translation = p;
        t.rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        t.scale = Vec3::new(1.0, 1.0, 1.5 + 3.0 * zone_k);
    }
}


/// Sparks from the bogies near the camera; wind trails on players the sweep is carrying off.
fn sparks_and_trails(time: Res<Time<Real>>, mut state: ResMut<HyperState>, views: Res<WorldViews>, cam: Query<&Transform, With<PlayerCamera>>, mut debris: MessageWriter<SpawnDebris>, mut rng: ResMut<VfxRng>) {
    if !state.active {
        return;
    }
    let dt = time.delta_secs().min(0.05);
    let Ok(cam) = cam.single() else { return };
    let eye = cam.translation;
    let r = &mut rng.0;
    // Sparks: ~24/s per bogie within 70 m (≈ 4-8 bogies in view), orange-white, sprayed backward.
    let is_near = |b: &&Vec3| b.distance(eye) < 70.0;
    let n_near = state.bogies.iter().filter(is_near).count();
    state.spark_acc += dt * 24.0 * n_near as f32;
    while state.spark_acc >= 1.0 && n_near > 0 {
        state.spark_acc -= 1.0;
        let Some(&b) = state.bogies.iter().filter(is_near).nth((r.next_u32() as usize) % n_near) else { break };
        let hot = r.f32();
        debris.write(SpawnDebris {
            pos: b + Vec3::new(r.range(-1.2, 1.2), r.range(-0.2, 0.2), 0.0),
            vel: Vec3::new(-r.range(18.0, 42.0), r.range(0.5, 6.0), b.z.signum() * r.range(1.0, 6.0)),
            color: [1.0, 0.55 + 0.4 * hot, 0.15 + 0.5 * hot * hot],
            size: r.range(0.05, 0.12),
            emissive: 6.0 + 6.0 * hot,
            lifetime: r.range(0.15, 0.4),
        });
    }
    if state.spark_acc > 4.0 {
        state.spark_acc = 0.0;
    }
    // Wind trails: anyone out in the sweep streams white air behind them.
    let Some(s) = state.slip else { return };
    let mut n_trail = state.trail_acc;
    for v in &views.players {
        if !v.alive() {
            continue;
        }
        if let SlipZone::Sweep(k) = s.zone(bd_core::Vec3::from_array(v.pos.to_array()))
            && k > 0.15
        {
            n_trail += 90.0 * k * dt;
            let mut m = n_trail;
            while m >= 1.0 {
                m -= 1.0;
                let side = (v.pos.z - (s.min.y + s.max.y) * 0.5).signum();
                debris.write(SpawnDebris {
                    pos: v.pos + Vec3::Y * 0.9 + r.unit_vec3() * r.range(0.3, 1.0),
                    vel: Vec3::new(-r.range(40.0, 70.0), r.range(-1.0, 1.0), side * r.range(4.0, 12.0)) + v.vel * 0.3,
                    color: [0.85, 0.95, 1.0],
                    size: r.range(0.04, 0.09),
                    emissive: 2.5,
                    lifetime: r.range(0.12, 0.3),
                });
            }
            n_trail = m;
        }
    }
    state.trail_acc = n_trail;
}

/// Dev / verification hook (off unless set): `BD_HYPER_TEST=knock@3` puts the local practice player
/// on car 3's roof edge 3 s in and launches them off the side (a slipstream knock-off: swept back
/// and out → KO); `tail@3` launches them off the tail ramp. Offline practice only.
fn dev_hyper_test(time: Res<Time<Real>>, state: Res<HyperState>, active: Option<ResMut<ActiveWorld>>, mut started: Local<Option<f32>>, mut done: Local<bool>) {
    static SPEC: std::sync::OnceLock<Option<(bool, f32)>> = std::sync::OnceLock::new();
    let spec = *SPEC.get_or_init(|| {
        let v = std::env::var("BD_HYPER_TEST").ok()?;
        let (kind, at) = v.split_once('@').unwrap_or((v.as_str(), "3"));
        Some((kind == "tail", at.parse().unwrap_or(3.0)))
    });
    let (Some((tail, at)), false, true) = (spec, *done, state.active) else { return };
    let Some(mut active) = active else { return };
    if active.role != crate::sim_bridge::SimRole::Offline {
        return;
    }
    let t0 = *started.get_or_insert(time.elapsed_secs());
    if time.elapsed_secs() - t0 < at {
        return;
    }
    *done = true;
    let Some(id) = active.local else { return };
    let us = hyperline::units(1);
    let (pos, vel) = if tail {
        let u = &us[hyperline::UNITS - 1];
        (bd_core::Vec3::new(u.x0 + 8.0, hyperline::ROOF_Y, 2.0), bd_core::Vec3::new(-30.0, 10.0, 0.0))
    } else {
        let u = &us[3];
        (bd_core::Vec3::new(u.cx(), hyperline::ROOF_Y, 3.0), bd_core::Vec3::new(0.0, 7.0, 16.0))
    };
    if let Some(p) = active.world.player_mut(id) {
        p.mv.pos = pos;
        p.mv.vel = bd_core::Vec3::ZERO;
        p.mv.grounded = true;
        p.spawn_shield = 0.0;
    }
    active.world.launch_player(id, vel, None);
}
