//! **Mirrorfold** presentation (DESIGN.md "Maps: Mirrorfold"): the folded city beyond the playable
//! curl. Visual only, never collides. Active only while the loaded map is `MapKind::Mirrorfold`;
//! everything spawned here carries [`MirrorEntity`] and is torn down when another map loads.
//!
//! * **The fold** — the city keeps curling past the arena's lip: ~4k low-detail buildings stand on
//!   the inside of a huge tube around the X axis (radius [`FOLD_R`], axis [`FOLD_AXIS_Y`] up), rising
//!   from below the arena on both sides, through the horizon and on overhead, where the two edges
//!   almost meet: a bright **rift** slit runs along the top (the star-KO gap). Toward ±X the tube
//!   narrows to a point where floor and ceiling meet in a second glare. One merged, unlit,
//!   vertex-coloured mesh per material (windows from a 4×4 tile), haze baked into the colours.
//! * **Hanging towers** — the six voxel towers continue above the world top as simple boxes that
//!   fade into the rift light.
//! * **Mirrored void** — under the floating city a dark glass sheet reflects the fold (the same mesh,
//!   mirrored, dimmed) with slow glinting shards between: what you see down the fractures.
//! * **Motion** — the fold breathes a few degrees about its axis, the shard cloud turns and the rift
//!   pulses; with the comfort preset (speed FOV off) everything is static.
//!
//! Cost: ~2 × 45k triangles in 4 draws for the fold + reflection, a few quads for the rift and sheet,
//! one mesh for ~240 shards; zero per-frame allocation (a handful of transforms and one colour).

use super::PresentSet;
use crate::arena::{ArenaWorld, MapKind};
use crate::settings::ComfortSettings;
use bd_voxel::maps::mirror::{TOP_Y, hanging_tower_footprints};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Tube radius of the fold (m) and its axis height.
pub const FOLD_R: f32 = 300.0;
pub const FOLD_AXIS_Y: f32 = 150.0;
/// Buildings stand between these angles from the bottom of the tube (the gap above is the rift;
/// below the start the tube would dip under the mirror plane).
const THETA_MIN: f32 = 50.0;
const THETA_MAX: f32 = 162.0;
/// Half length along X; the tube narrows past `TAPER_FROM` to a point at the end.
const HALF_LEN: f32 = 1150.0;
const TAPER_FROM: f32 = 380.0;
/// The mirror sheet under the floating city.
const MIRROR_Y: f32 = -50.0;

/// Everything this module spawns (despawned when another map loads).
#[derive(Component)]
pub struct MirrorEntity;

#[derive(Component)]
struct Fold {
    mirrored: bool,
}

#[derive(Component)]
struct ShardCloud;

#[derive(Resource, Default)]
struct MirrorState {
    active: bool,
    rift: Option<Handle<StandardMaterial>>,
}

pub struct MirrorPlugin;

impl Plugin for MirrorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MirrorState>().add_systems(Update, (sync_mirror, animate_mirror).chain().in_set(PresentSet::Sync));
    }
}

fn is_mirror(arena: &Option<Res<ArenaWorld>>) -> bool {
    arena.as_ref().is_some_and(|a| a.info.kind == MapKind::Mirrorfold)
}

/// Tiny deterministic hash → [0, 1) (layout only; no gameplay).
fn h01(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 / 16_777_216.0
}

fn srgb(r: f32, g: f32, b: f32) -> Vec3 {
    let c = Color::srgb(r, g, b).to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

/// A 4×4 facade tile (two bays × two floors): piers / spandrels white (vertex colour shows through),
/// windows dark blue — or warm and bright for the lit variant.
fn window_tile(lit: bool) -> Image {
    let mut data = Vec::with_capacity(64);
    for y in 0..4 {
        for x in 0..4 {
            let win = y % 2 == 1 && x % 2 == 1;
            let c: [u8; 4] = match (win, lit) {
                (true, false) => [112, 122, 152, 255],
                (true, true) => [255, 214, 150, 255],
                (false, false) => [255, 255, 255, 255],
                (false, true) => [170, 162, 154, 255],
            };
            data.extend_from_slice(&c);
        }
    }
    let mut img = Image::new(
        Extent3d { width: 4, height: 4, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    img
}

#[derive(Default)]
struct MeshBuf {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl MeshBuf {
    fn quad(&mut self, p: [Vec3; 4], n: Vec3, uv: [Vec2; 4], c: [Vec3; 4]) {
        let b = self.pos.len() as u32;
        for k in 0..4 {
            self.pos.push(p[k].to_array());
            self.nrm.push(n.to_array());
            self.uv.push(uv[k].to_array());
            self.col.push([c[k].x, c[k].y, c[k].z, 1.0]);
        }
        self.idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }

    /// A box standing on a surface: base centre `o`, `up` (height axis), `u` / `v` (footprint axes,
    /// half sizes `hu`, `hv`), `h` tall. Five faces (no base). `shade` = base colour at the foot,
    /// `top` = colour at the roof line; UVs tile the 4×4 window texture every 6 m × 8 m.
    #[allow(clippy::too_many_arguments)]
    fn tower(&mut self, o: Vec3, up: Vec3, u: Vec3, v: Vec3, hu: f32, hv: f32, h: f32, foot: Vec3, top: Vec3) {
        let t = up * h;
        let corners = [o - u * hu - v * hv, o + u * hu - v * hv, o + u * hu + v * hv, o - u * hu + v * hv];
        // Sides: (a, b) along the footprint ring, facing outward.
        let sides = [(0usize, 1usize, -v, hu), (1, 2, u, hv), (2, 3, v, hu), (3, 0, -u, hv)];
        for (i, (a, b, n, half)) in sides.iter().enumerate() {
            let k = if i % 2 == 0 { 0.82 } else { 0.64 };
            let (ua, ub) = (0.0, half * 2.0 / 6.0);
            let vt = h / 8.0;
            self.quad(
                [corners[*a], corners[*b], corners[*b] + t, corners[*a] + t],
                *n,
                [Vec2::new(ua, 0.0), Vec2::new(ub, 0.0), Vec2::new(ub, vt), Vec2::new(ua, vt)],
                [foot * k, foot * k, top * k, top * k],
            );
        }
        // Roof: a pier texel (plain).
        let r = Vec2::splat(0.05);
        let roof = top * 1.05;
        self.quad([corners[0] + t, corners[1] + t, corners[2] + t, corners[3] + t], up, [r; 4], [roof; 4]);
    }

    fn into_mesh(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
            .with_inserted_indices(Indices::U32(self.idx))
    }
}

/// Tube radius at `x` (narrows to a point at the ends).
fn radius_at(x: f32) -> f32 {
    let t = ((x.abs() - TAPER_FROM) / (HALF_LEN - TAPER_FROM)).clamp(0.0, 1.0);
    FOLD_R * (1.0 - t * t * (3.0 - 2.0 * t) * 0.97)
}

/// The fold's buildings, in axis-relative coordinates (the entity sits on the axis). Returns the
/// plain and the lit-window meshes.
fn build_fold(haze: Vec3, glow: Vec3) -> (Mesh, Mesh) {
    let (mut plain, mut lit) = (MeshBuf::default(), MeshBuf::default());
    let palette = [srgb(0.62, 0.58, 0.52), srgb(0.50, 0.49, 0.55), srgb(0.30, 0.34, 0.46), srgb(0.70, 0.66, 0.60), srgb(0.40, 0.36, 0.40)];
    let cell = 26.0;
    let nx = (HALF_LEN * 2.0 / cell) as i32;
    for side in [-1.0f32, 1.0] {
        let mut i = 0u32;
        for ix in 0..nx {
            let x = -HALF_LEN + (ix as f32 + 0.5) * cell;
            let r = radius_at(x);
            let scale = r / FOLD_R;
            let arc = (THETA_MAX - THETA_MIN).to_radians() * r;
            let n_arc = ((arc / (cell * scale.max(0.25))) as i32).max(2);
            for ia in 0..n_arc {
                i += 1;
                let key = i ^ if side > 0.0 { 0x5555 } else { 0 };
                let th = (THETA_MIN + (THETA_MAX - THETA_MIN) * (ia as f32 + 0.5) / n_arc as f32).to_radians() + (h01(key, 1) - 0.5) * 0.01;
                let (s, c) = th.sin_cos();
                // Tube point (axis-relative), inward normal and the along-arc tangent.
                let p = Vec3::new(x + (h01(key, 2) - 0.5) * cell * 0.3, -r * c, side * r * s);
                let up = Vec3::new(0.0, c, -side * s);
                let v = Vec3::new(0.0, s, side * c);
                let size = cell * scale.max(0.2);
                let hu = size * (0.28 + 0.16 * h01(key, 3));
                let hv = size * (0.28 + 0.16 * h01(key, 4));
                let tall = h01(key, 5);
                let h = size * (0.6 + 2.6 * tall * tall) * if th.to_degrees() > 120.0 { 0.8 } else { 1.0 };
                let base = palette[(h01(key, 6) * palette.len() as f32) as usize % palette.len()];
                // Haze by distance from the arena, glow toward the rift (top) and the far ends.
                let world = p + Vec3::Y * FOLD_AXIS_Y;
                let d = world.length();
                let fogk = 1.0 - (-(d / 750.0) * (d / 750.0)).exp();
                let riftk = ((th.to_degrees() - 135.0) / 27.0).clamp(0.0, 1.0) * 0.55 + ((x.abs() - 600.0) / 550.0).clamp(0.0, 1.0) * 0.6;
                // Always a little haze: the fold stays a backdrop behind the players, never busier.
                let tint = base.lerp(haze, 0.22 + 0.62 * fogk).lerp(glow, riftk.min(0.85));
                let foot = tint * 0.55;
                let top = tint;
                let buf = if h01(key, 7) < 0.08 { &mut lit } else { &mut plain };
                buf.tower(p, up, Vec3::X, v, hu, hv, h, foot, top);
            }
        }
    }
    (plain.into_mesh(), lit.into_mesh())
}

/// The hanging towers' continuation above the world top (fading into the rift light).
fn build_tower_tops(stone: Vec3, glow: Vec3) -> Mesh {
    let mut m = MeshBuf::default();
    for (c, half) in hanging_tower_footprints() {
        let o = Vec3::new(c.x, TOP_Y - 0.05, c.y);
        m.tower(o, Vec3::Y, Vec3::X, Vec3::Z, half.x, half.y, 150.0, stone, stone.lerp(glow, 0.9));
    }
    m.into_mesh()
}

/// Thin glinting shards scattered in the void under (and around) the city.
fn build_shards() -> Mesh {
    let mut m = MeshBuf::default();
    for i in 0..240u32 {
        let a = h01(i, 11) * std::f32::consts::TAU;
        let rr = 20.0 + h01(i, 12) * 170.0;
        let p = Vec3::new(a.cos() * rr * 1.2, -8.0 - h01(i, 13) * 40.0, a.sin() * rr);
        let len = 2.0 + h01(i, 14) * 7.0;
        let w = 0.4 + h01(i, 15) * 1.6;
        let d1 = Vec3::new(h01(i, 16) - 0.5, h01(i, 17) - 0.3, h01(i, 18) - 0.5).normalize_or(Vec3::Y);
        let d2 = d1.cross(Vec3::new(h01(i, 19) - 0.5, 1.0, h01(i, 20) - 0.5)).normalize_or(Vec3::X);
        let k = 0.5 + h01(i, 21);
        let c = Vec3::new(0.55, 0.85, 1.0) * k;
        let (p0, p1, p2, p3) = (p - d1 * len * 0.5, p + d2 * w, p + d1 * len * 0.5, p - d2 * w * 0.4);
        m.quad([p0, p1, p2, p3], d1.cross(d2), [Vec2::ZERO; 4], [c, c * 0.6, c * 1.3, c * 0.5]);
    }
    m.into_mesh()
}

fn rift_strip(len: f32, width: f32) -> Mesh {
    let mut m = MeshBuf::default();
    let hw = width * 0.5;
    let c = Vec3::ONE;
    // Along X, facing down (and up — double-sided material).
    m.quad(
        [Vec3::new(-len, 0.0, -hw), Vec3::new(len, 0.0, -hw), Vec3::new(len, 0.0, hw), Vec3::new(-len, 0.0, hw)],
        Vec3::NEG_Y,
        [Vec2::ZERO; 4],
        [c; 4],
    );
    m.into_mesh()
}

/// Spawn / tear down the Mirrorfold backdrop when the map changes.
#[allow(clippy::too_many_arguments)]
fn sync_mirror(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    mut state: ResMut<MirrorState>,
    existing: Query<Entity, With<MirrorEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let want = is_mirror(&arena);
    if want == state.active {
        return;
    }
    state.active = want;
    if !want {
        for e in &existing {
            commands.entity(e).despawn();
        }
        state.rift = None;
        return;
    }
    let fog = arena.as_ref().and_then(|a| a.info.arena.as_ref()).map_or([0.58, 0.58, 0.68], |i| i.fog_color);
    let haze = srgb(fog[0], fog[1], fog[2]);
    let glow = srgb(1.0, 0.97, 0.9) * 1.6;
    let (plain, lit) = build_fold(haze, glow);
    let (plain, lit) = (meshes.add(plain), meshes.add(lit));
    let tile = images.add(window_tile(false));
    let tile_lit = images.add(window_tile(true));
    let unlit = |tex: Handle<Image>, tint: Color| StandardMaterial {
        base_color: tint,
        base_color_texture: Some(tex),
        unlit: true,
        fog_enabled: false,
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    let fold_mat = materials.add(unlit(tile.clone(), Color::WHITE));
    let fold_lit_mat = materials.add(unlit(tile_lit.clone(), Color::WHITE));
    // The reflection: the same meshes mirrored under the glass, dimmed and cooled.
    let refl_mat = materials.add(unlit(tile, Color::srgb(0.30, 0.32, 0.44)));
    let refl_lit_mat = materials.add(unlit(tile_lit, Color::srgb(0.34, 0.32, 0.40)));
    for mirrored in [false, true] {
        let (m0, m1) = if mirrored { (refl_mat.clone(), refl_lit_mat.clone()) } else { (fold_mat.clone(), fold_lit_mat.clone()) };
        commands
            .spawn((
                Name::new(if mirrored { "MirrorFoldReflection" } else { "MirrorFold" }),
                MirrorEntity,
                Fold { mirrored },
                fold_transform(0.0, mirrored),
                Visibility::default(),
            ))
            .with_children(|c| {
                c.spawn((Mesh3d(plain.clone()), MeshMaterial3d(m0), Transform::IDENTITY, NotShadowCaster, NotShadowReceiver));
                c.spawn((Mesh3d(lit.clone()), MeshMaterial3d(m1), Transform::IDENTITY, NotShadowCaster, NotShadowReceiver));
            });
    }

    // Hanging towers continue up into the light.
    let stone = srgb(0.46, 0.44, 0.50);
    let tops = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, fog_enabled: false, ..default() });
    commands.spawn((
        Name::new("MirrorTowerTops"),
        MirrorEntity,
        Mesh3d(meshes.add(build_tower_tops(stone, glow))),
        MeshMaterial3d(tops),
        Transform::IDENTITY,
        NotShadowCaster,
        NotShadowReceiver,
    ));

    // The rift: a blazing slit along the top of the fold plus a soft additive bloom sheet, and the
    // two far glares where the tube closes.
    let rift = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::rgb(9.0, 9.2, 9.6)),
        unlit: true,
        fog_enabled: false,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    state.rift = Some(rift.clone());
    let halo = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(1.4, 1.35, 1.25, 0.35)),
        unlit: true,
        fog_enabled: false,
        double_sided: true,
        cull_mode: None,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    let top_y = FOLD_AXIS_Y + FOLD_R * 0.97;
    let slit_w = 2.0 * FOLD_R * (180.0 - THETA_MAX).to_radians().sin();
    commands.spawn((
        Name::new("MirrorRift"),
        MirrorEntity,
        Mesh3d(meshes.add(rift_strip(HALF_LEN, slit_w * 0.55))),
        MeshMaterial3d(rift),
        Transform::from_xyz(0.0, top_y, 0.0),
        NotShadowCaster,
        NotShadowReceiver,
    ));
    commands.spawn((
        Name::new("MirrorRiftHalo"),
        MirrorEntity,
        Mesh3d(meshes.add(rift_strip(HALF_LEN, slit_w * 1.6))),
        MeshMaterial3d(halo.clone()),
        Transform::from_xyz(0.0, top_y - 25.0, 0.0),
        NotShadowCaster,
        NotShadowReceiver,
    ));
    let end_disc = meshes.add(Circle::new(110.0).mesh().resolution(40).build());
    for sx in [-1.0f32, 1.0] {
        commands.spawn((
            Name::new("MirrorRiftEnd"),
            MirrorEntity,
            Mesh3d(end_disc.clone()),
            MeshMaterial3d(halo.clone()),
            Transform::from_xyz(sx * (HALF_LEN - 20.0), FOLD_AXIS_Y, 0.0)
                .with_rotation(Quat::from_rotation_y(-sx * std::f32::consts::FRAC_PI_2)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }

    // Mirror glass under the floating city (tinted, translucent over the reflection) + shards.
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgba(0.03, 0.03, 0.07, 0.62),
        unlit: true,
        fog_enabled: false,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Name::new("MirrorGlass"),
        MirrorEntity,
        Mesh3d(meshes.add(Circle::new(1400.0).mesh().resolution(64).build())),
        MeshMaterial3d(glass),
        Transform::from_xyz(0.0, MIRROR_Y, 0.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        NotShadowCaster,
        NotShadowReceiver,
    ));
    let shard_mat = materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::rgb(1.6, 2.2, 2.6)),
        unlit: true,
        fog_enabled: false,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Name::new("MirrorShards"),
        MirrorEntity,
        ShardCloud,
        Mesh3d(meshes.add(build_shards())),
        MeshMaterial3d(shard_mat),
        Transform::IDENTITY,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// The fold's transform for a breathing angle `a` (the reflection mirrors it under the glass).
fn fold_transform(a: f32, mirrored: bool) -> Transform {
    if mirrored {
        Transform {
            translation: Vec3::new(0.0, 2.0 * MIRROR_Y - FOLD_AXIS_Y, 0.0),
            rotation: Quat::from_rotation_x(-a),
            scale: Vec3::new(1.0, -1.0, 1.0),
        }
    } else {
        Transform { translation: Vec3::new(0.0, FOLD_AXIS_Y, 0.0), rotation: Quat::from_rotation_x(a), scale: Vec3::ONE }
    }
}

/// Slow kaleidoscope motion (static under the comfort preset).
fn animate_mirror(
    time: Res<Time<Real>>,
    state: Res<MirrorState>,
    comfort: Res<ComfortSettings>,
    mut folds: Query<(&Fold, &mut Transform), Without<ShardCloud>>,
    mut shards: Query<&mut Transform, With<ShardCloud>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !state.active {
        return;
    }
    let moving = comfort.speed_fov;
    let t = if moving { time.elapsed_secs() } else { 0.0 };
    let a = 0.045 * (t * std::f32::consts::TAU / 95.0).sin();
    for (f, mut tf) in &mut folds {
        *tf = fold_transform(a, f.mirrored);
    }
    for mut tf in &mut shards {
        tf.rotation = Quat::from_rotation_y(t * 0.012);
    }
    if moving
        && let Some(h) = state.rift.as_ref()
        && let Some(mut m) = materials.get_mut(h)
    {
        let k = 1.0 + 0.15 * (t * std::f32::consts::TAU / 7.0).sin();
        m.base_color = Color::LinearRgba(LinearRgba::rgb(9.0 * k, 9.2 * k, 9.6 * k));
    }
}
