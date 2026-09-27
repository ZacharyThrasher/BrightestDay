//! **High Roller** presentation (DESIGN.md "Maps: High Roller"): the gambling toys the sim runs
//! (`bd_sim::world::interactables`) made visible and audible. Generic over any map whose
//! `ArenaInfo::interactables` declares machines / a wheel; everything spawned carries
//! [`CasinoEntity`] and is torn down when another map loads. Driven only by `GameEvent::Casino`, so
//! it works the same in practice, host and client.
//!
//! * **Slot machines** — three reels per machine (a procedural symbol strip, scrolled while the
//!   sim spins them), a gold lever arm + red knob on the side mount that yanks down on a pull and
//!   springs back (a jiggle on a clunk), and a marquee bar that glows on a jackpot. After a payout
//!   the reels stop one by one: three of a kind for an effect, 7-7-7 for the jackpot, a mismatch for
//!   a bust. The odds are never shown anywhere (mystery tenet) — the result is what happens.
//! * **Busts** — confetti bursting in the puller's face, a cream pie splatting across *your* view
//!   for a moment (only yours, fades, never flashes), or a springy hop sound.
//! * **Roulette house spin** — a ball rolls the banked track, spirals in and drops into the winning
//!   pocket while a light chases it; the winning pockets then glow and breathe gold while they pay.
//! * **Glitter** — a capped handful of tiny sparkles twinkling around the chandeliers and the wheel
//!   (slow, small; comfort-safe).
//!
//! Cost: 12 reel quads, 4 levers, 4 marquee bars, 38 flat pocket wedges, one ball, ≤ 96 sparkles;
//! no per-frame allocation.

use std::sync::Arc;

use super::sound::{Sfx3d, SfxLoopEmitter};
use super::{bv, PresentSet};
use crate::arena::ArenaWorld;
use crate::settings::ComfortSettings;
use crate::sim_bridge::{ActiveWorld, GameEvents};
use bd_fx::{BurstFx, BurstKind};
use bd_sim::world::interactables::{BustKind, CasinoEvent, SlotOutcome};
use bd_sim::GameEvent;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine2;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bd_voxel::{RouletteWheel, SlotMachine};

/// Symbols on a reel strip (top to bottom): cherry, lemon, bell, BAR, diamond, star, horseshoe, 7.
const SYMBOLS: usize = 8;
const SEVEN: usize = 7;
/// Seconds between the three reels stopping after a payout.
const REEL_STAGGER: f32 = 0.28;
const GLITTER: usize = 96;

/// Everything this module spawns (despawned when another map loads).
#[derive(Component)]
pub struct CasinoEntity;

#[derive(Component)]
struct Reel;

/// One part of a lever (the arm or the knob), posed in world space every frame: it swings about
/// `axis` (the machine's right) through `pivot`; `rest` is its transform relative to the pivot.
#[derive(Component)]
struct Lever {
    machine: usize,
    axis: Vec3,
    pivot: Vec3,
    rest: Transform,
}

#[derive(Component)]
struct Marquee;

#[derive(Component)]
struct Wedge(usize);

#[derive(Component)]
struct Ball;

#[derive(Component)]
struct Sparkle {
    center: Vec3,
    radius: f32,
    angle: f32,
    speed: f32,
    phase: f32,
    size: f32,
}

#[derive(Component)]
struct PieOverlay;

#[derive(Component)]
struct SpinHum(usize);

#[derive(Clone, Debug, Default)]
struct MachineFx {
    /// Seconds since the lever went down (None = at rest); `clunk` = a small jiggle.
    lever_t: Option<f32>,
    clunk: bool,
    /// Reels spinning (scroll offsets in symbols) and, per reel, the time left until it lands on
    /// its target (None = free spinning / at rest).
    spinning: bool,
    offset: [f32; 3],
    speed: [f32; 3],
    target: [usize; 3],
    stop_in: [Option<f32>; 3],
    /// Outcome waiting for the last reel (sound + burst then).
    pending: Option<(SlotOutcome, bd_core::PlayerId)>,
    /// Marquee glow left (s).
    glow: f32,
    /// The spin hum loop is attached.
    hum: bool,
}

#[derive(Clone, Debug, Default)]
struct HouseFx {
    /// Seconds into the current spin (None = idle).
    t: Option<f32>,
    pocket: usize,
    half_width: usize,
    sweep: f32,
    lit: f32,
    start_angle: f32,
    landed: bool,
}

#[derive(Resource, Default)]
struct CasinoFx {
    /// The arena these entities were built for (pointer identity of its `MapInfo`).
    built_for: usize,
    active: bool,
    slots: Vec<SlotMachine>,
    wheel: Option<RouletteWheel>,
    machines: Vec<MachineFx>,
    house: HouseFx,
    /// Pie splat on the local view (1 → 0).
    pie: f32,
    pie_hold: f32,
    reel_mats: Vec<Handle<StandardMaterial>>,
    marquee_mats: Vec<Handle<StandardMaterial>>,
    sweep_mat: Option<Handle<StandardMaterial>>,
    lit_mat: Option<Handle<StandardMaterial>>,
}

pub struct CasinoPlugin;

impl Plugin for CasinoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CasinoFx>()
            .add_systems(Update, (sync_scene, casino_events).chain().in_set(PresentSet::Events))
            .add_systems(Update, (animate_slots, animate_house, animate_glitter, animate_pie).in_set(PresentSet::Sync));
        if std::env::var_os("BD_CASINO_DEMO").is_some() {
            app.add_systems(Update, demo.in_set(PresentSet::Events).before(casino_events));
        }
    }
}

/// `BD_CASINO_DEMO=<outcome>[:<machine>]` (practice / host only; dev screenshots, never set in play):
/// ~2.5 s into the match the local player pulls machine `machine` (default 0) with the odds forced to
/// `jackpot`, `bust`, `effect` (or `any` = the real odds), and the roulette house spin starts at ~1 s.
/// `BD_CASINO_DEMO_REPEAT=1` pulls again every 3.2 s.
fn demo(time: Res<Time>, fx: Res<CasinoFx>, active: Option<ResMut<ActiveWorld>>, mut t: Local<f32>, mut done: Local<u8>) {
    let Some(mut active) = active else { return };
    if !fx.active || active.role == crate::sim_bridge::SimRole::Client {
        return;
    }
    *t += time.delta_secs();
    let spec = std::env::var("BD_CASINO_DEMO").unwrap_or_default();
    let (what, machine) = spec.split_once(':').map_or((spec.as_str(), 0usize), |(a, b)| (a, b.parse().unwrap_or(0)));
    let w = &mut active.world;
    if *done == 0 && *t > 1.0 {
        *done = 1;
        w.interact.house.next = 0.05;
    }
    // Pull at 2.5 s, then again every 3.2 s (`BD_CASINO_DEMO_REPEAT=1`: busts come in three kinds).
    let repeat = std::env::var_os("BD_CASINO_DEMO_REPEAT").is_some();
    if *done >= 1 && *t > 2.5 + 3.2 * (*done - 1) as f32 && (*done == 1 || repeat) {
        *done += 1;
        let c = &mut w.cfg.tuning.casino;
        match what {
            "jackpot" => (c.p_jackpot, c.p_bust) = (1.0, 0.0),
            "bust" => (c.p_jackpot, c.p_bust) = (0.0, 1.0),
            "effect" => (c.p_jackpot, c.p_bust) = (0.0, 0.0),
            _ => {}
        }
        if let Some(me) = active.local {
            let w = &mut active.world;
            w.interact.machines.iter_mut().for_each(|m| m.cooldown = 0.0);
            bd_sim::world::interactables::pull(w, machine, me, bd_sim::world::interactables::PullBy::Melee);
        }
    }
}

fn unlit(color: Color, tex: Option<Handle<Image>>) -> StandardMaterial {
    StandardMaterial { base_color: color, base_color_texture: tex, unlit: true, fog_enabled: true, ..default() }
}

/// Pocket light: a translucent film (no additive bloom wash over the hall).
fn wedge_mat(color: LinearRgba, alpha: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba { alpha, ..color }),
        emissive: color * 0.35,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

fn glow_mat(color: LinearRgba, alpha: bool) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::LinearRgba(color),
        emissive: color * 2.0,
        unlit: true,
        alpha_mode: if alpha { AlphaMode::Add } else { AlphaMode::Opaque },
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

// ------------------------------------------------------------------------------------ textures

fn put(data: &mut [u8], w: usize, x: i32, y: i32, c: [u8; 4]) {
    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < data.len() / 4 / w {
        let i = (y as usize * w + x as usize) * 4;
        data[i..i + 4].copy_from_slice(&c);
    }
}

/// The reel strip: 8 symbols of 32 × 32 px stacked vertically on an ivory ground.
fn reel_strip() -> Image {
    const S: usize = 32;
    let (w, h) = (S, S * SYMBOLS);
    let mut d = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let edge = y % S == 0 || y % S == S - 1;
            put(&mut d, w, x as i32, y as i32, if edge { [200, 190, 170, 255] } else { [250, 244, 228, 255] });
        }
    }
    let disc = |d: &mut Vec<u8>, cx: f32, cy: f32, r: f32, c: [u8; 4]| {
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if dx * dx + dy * dy <= r * r {
                    put(d, w, x as i32, y as i32, c);
                }
            }
        }
    };
    let rect = |d: &mut Vec<u8>, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 4]| {
        for y in y0..y1 {
            for x in x0..x1 {
                put(d, w, x, y, c);
            }
        }
    };
    let red = [210, 20, 36, 255];
    let gold = [240, 180, 30, 255];
    let green = [40, 150, 60, 255];
    let dark = [30, 24, 30, 255];
    let blue = [40, 110, 230, 255];
    let o = |k: usize| (k * S) as f32;
    // 0 cherry: two red discs + a green stem.
    disc(&mut d, 11.0, o(0) + 21.0, 6.0, red);
    disc(&mut d, 21.0, o(0) + 22.0, 6.0, red);
    for t in 0..10 {
        put(&mut d, w, 12 + t / 3, (o(0) as i32) + 14 - t, green);
        put(&mut d, w, 20 - t / 4, (o(0) as i32) + 15 - t, green);
    }
    // 1 lemon.
    for y in 0..S {
        for x in 0..S {
            let (dx, dy) = ((x as f32 - 16.0) / 11.0, (y as f32 - 16.0) / 7.5);
            if dx * dx + dy * dy <= 1.0 {
                put(&mut d, w, x as i32, (o(1) as usize + y) as i32, [245, 220, 40, 255]);
            }
        }
    }
    // 2 bell.
    for y in 0..20 {
        let half = 3 + y * 9 / 20;
        rect(&mut d, 16 - half as i32, o(2) as i32 + 6 + y as i32, 16 + half as i32, o(2) as i32 + 7 + y as i32, gold);
    }
    disc(&mut d, 16.0, o(2) + 27.0, 2.5, gold);
    // 3 BAR: a black bar with an ivory band.
    rect(&mut d, 4, o(3) as i32 + 10, 28, o(3) as i32 + 22, dark);
    rect(&mut d, 6, o(3) as i32 + 14, 26, o(3) as i32 + 18, [250, 244, 228, 255]);
    // 4 diamond.
    for y in 0..20i32 {
        let half = 10 - (y - 10).abs();
        rect(&mut d, 16 - half, o(4) as i32 + 6 + y, 16 + half, o(4) as i32 + 7 + y, blue);
    }
    // 5 star.
    for y in 0..S as i32 {
        for x in 0..S as i32 {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 16.5);
            let a = dy.atan2(dx);
            let r = (dx * dx + dy * dy).sqrt();
            let lim = 6.0 + 6.0 * (0.5 + 0.5 * (a * 5.0 + 1.57).cos()).powf(3.0);
            if r < lim {
                put(&mut d, w, x, o(5) as i32 + y, gold);
            }
        }
    }
    // 6 horseshoe.
    for y in 0..S as i32 {
        for x in 0..S as i32 {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 14.0);
            let r = (dx * dx + dy * dy).sqrt();
            if (7.0..11.0).contains(&r) && dy > -6.0 {
                put(&mut d, w, x, o(6) as i32 + y, [150, 150, 165, 255]);
            }
        }
    }
    // 7: a big red 7.
    rect(&mut d, 7, o(7) as i32 + 5, 26, o(7) as i32 + 10, red);
    for t in 0..18i32 {
        rect(&mut d, 20 - t * 7 / 18, o(7) as i32 + 10 + t, 26 - t * 7 / 18, o(7) as i32 + 11 + t, red);
    }
    let mut img = Image::new(Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, TextureDimension::D2, d, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_v: ImageAddressMode::Repeat, ..ImageSamplerDescriptor::nearest() });
    img
}

/// A cream pie splat with drips (alpha-blended over the whole view).
fn pie_splat() -> Image {
    const N: usize = 256;
    let mut d = vec![0u8; N * N * 4];
    let blobs = [(128.0f32, 118.0f32, 92.0f32), (70.0, 90.0, 40.0), (190.0, 80.0, 44.0), (86.0, 170.0, 38.0), (180.0, 170.0, 46.0), (128.0, 40.0, 30.0)];
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32, y as f32);
            let mut inside = blobs.iter().any(|(cx, cy, r)| (fx - cx).powi(2) + (fy - cy).powi(2) < r * r);
            // Drips running down.
            for k in 0..9 {
                let dx = 40.0 + k as f32 * 22.0;
                let len = 150.0 + ((k * 37) % 70) as f32;
                if (fx - dx).abs() < 5.0 + (k % 3) as f32 && fy > 100.0 && fy < len + 60.0 {
                    inside = true;
                }
            }
            if inside {
                let crumb = ((x * 7 + y * 13) % 29) == 0;
                let c = if crumb { [196, 140, 80, 235] } else { [252, 246, 232, 238] };
                d[(y * N + x) * 4..(y * N + x) * 4 + 4].copy_from_slice(&c);
            }
        }
    }
    Image::new(Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 }, TextureDimension::D2, d, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// A flat annular sector (y up), `a0..a1` radians like `atan2(z, x)`.
fn wedge_mesh(r0: f32, r1: f32, a0: f32, a1: f32) -> Mesh {
    let n = 4;
    let mut pos = Vec::new();
    let mut idx = Vec::new();
    for i in 0..=n {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        let (s, c) = a.sin_cos();
        pos.push([c * r0, 0.0, s * r0]);
        pos.push([c * r1, 0.0, s * r1]);
    }
    for i in 0..n as u32 {
        let b = i * 2;
        idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
    }
    let nrm = vec![[0.0, 1.0, 0.0]; pos.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_indices(Indices::U32(idx))
}

fn h01(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    (x >> 8) as f32 / 16_777_216.0
}

fn right_of(m: &SlotMachine) -> Vec3 {
    Vec3::new(m.facing.z, 0.0, -m.facing.x)
}

// ------------------------------------------------------------------------------------ scene

#[allow(clippy::too_many_arguments)]
fn sync_scene(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    mut fx: ResMut<CasinoFx>,
    existing: Query<Entity, With<CasinoEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (key, it) = match arena.as_ref() {
        Some(a) => (Arc::as_ptr(&a.info) as usize, a.info.arena.as_ref().map(|i| i.interactables.clone()).unwrap_or_default()),
        None => (0, Default::default()),
    };
    if key == fx.built_for {
        return;
    }
    fx.built_for = key;
    for e in &existing {
        commands.entity(e).despawn();
    }
    *fx = CasinoFx { built_for: key, ..default() };
    if it.is_empty() {
        return;
    }
    fx.active = true;
    fx.slots = it.slots.clone();
    fx.wheel = it.roulette.clone();
    fx.machines = vec![MachineFx::default(); it.slots.len()];

    let strip = images.add(reel_strip());
    let reel_mesh = meshes.add(Rectangle::new(1.0, 1.35));
    let arm = meshes.add(Cylinder::new(0.07, 1.0));
    let knob = meshes.add(Sphere::new(0.24));
    let gold = materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.74, 0.28), metallic: 0.9, perceptual_roughness: 0.3, ..default() });
    let red = materials.add(glow_mat(LinearRgba::rgb(1.4, 0.05, 0.08), false));
    for (i, m) in it.slots.iter().enumerate() {
        let right = right_of(m);
        let face = Transform::from_translation(bv(m.reels) + bv(m.facing) * 0.03).looking_to(-bv(m.facing), Vec3::Y);
        for k in 0..3 {
            let mat = materials.add(StandardMaterial { uv_transform: Affine2::from_scale(Vec2::new(1.0, 1.25 / SYMBOLS as f32)), ..unlit(Color::WHITE, Some(strip.clone())) });
            fx.reel_mats.push(mat.clone());
            let mut t = face;
            t.translation += right * ((k as f32 - 1.0) * 1.15);
            // `looking_to(-facing)`: the quad's front (+Z) faces the player.
            commands.spawn((CasinoEntity, Reel, Mesh3d(reel_mesh.clone()), MeshMaterial3d(mat), t, NotShadowCaster, NotShadowReceiver));
        }
        // Lever: pivot 1 m under the knob, just outside the mount plate.
        let pivot = bv(m.lever) - Vec3::Y * 1.0 - right * 0.2;
        let tip = bv(m.lever) - pivot;
        let arm_t = Transform::from_translation(tip * 0.5).with_rotation(Quat::from_rotation_arc(Vec3::Y, tip.normalize())).with_scale(Vec3::new(1.0, tip.length(), 1.0));
        let knob_t = Transform::from_translation(tip);
        for (mesh, mat, rest) in [(arm.clone(), gold.clone(), arm_t), (knob.clone(), red.clone(), knob_t)] {
            let mut world = rest;
            world.translation += pivot;
            commands.spawn((CasinoEntity, Lever { machine: i, axis: right, pivot, rest }, Mesh3d(mesh), MeshMaterial3d(mat), world, NotShadowCaster));
        }
        // Marquee light bar over the reels.
        let mm = materials.add(glow_mat(LinearRgba::rgb(0.9, 0.55, 0.12), false));
        fx.marquee_mats.push(mm.clone());
        let bar = meshes.add(Cuboid::new(3.6, 0.3, 0.1));
        let t = Transform::from_translation(bv(m.reels) + Vec3::Y * 1.05 + bv(m.facing) * 0.08).looking_to(-bv(m.facing), Vec3::Y);
        commands.spawn((CasinoEntity, Marquee, Mesh3d(bar), MeshMaterial3d(mm), t, NotShadowCaster, NotShadowReceiver));
        commands.spawn((CasinoEntity, SpinHum(i), Transform::from_translation(bv(m.reels)), Visibility::default()));
    }
    if let Some(w) = &it.roulette {
        let sweep = materials.add(wedge_mat(LinearRgba::rgb(1.0, 0.92, 0.7), 0.55));
        let lit = materials.add(wedge_mat(LinearRgba::rgb(1.0, 0.72, 0.2), 0.5));
        fx.sweep_mat = Some(sweep);
        fx.lit_mat = Some(lit.clone());
        let n = w.pockets();
        let step = std::f32::consts::TAU / n as f32;
        for k in 0..n {
            let a0 = w.zero_angle + k as f32 * step;
            let mesh = meshes.add(wedge_mesh(w.r_inner, w.r_outer, a0 + 0.004, a0 + step - 0.004));
            // Just over the frets, so the whole pocket glows (a thin film at ankle height).
            let t = Transform::from_translation(bv(w.center) + Vec3::Y * 0.56);
            commands.spawn((CasinoEntity, Wedge(k), Mesh3d(mesh), MeshMaterial3d(lit.clone()), t, Visibility::Hidden, NotShadowCaster, NotShadowReceiver));
        }
        let ball = materials.add(StandardMaterial { base_color: Color::srgb(0.97, 0.96, 0.92), emissive: LinearRgba::rgb(0.5, 0.5, 0.48), perceptual_roughness: 0.15, ..default() });
        commands.spawn((CasinoEntity, Ball, Mesh3d(meshes.add(Sphere::new(0.8))), MeshMaterial3d(ball), Transform::from_translation(bv(w.center)), Visibility::Hidden));
    }
    // Glitter: sparkles around the chandeliers (the hub ring over the wheel) and above the felt.
    let sparkle_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let sparkle_mats = [
        materials.add(glow_mat(LinearRgba::rgb(1.0, 0.85, 0.5), false)),
        materials.add(glow_mat(LinearRgba::rgb(1.0, 1.0, 1.0), false)),
        materials.add(glow_mat(LinearRgba::rgb(1.0, 0.5, 0.8), false)),
    ];
    let c = it.roulette.as_ref().map_or(Vec3::ZERO, |w| bv(w.center));
    for k in 0..GLITTER {
        let (a, b, e) = (h01(k as u32, 1), h01(k as u32, 2), h01(k as u32, 3));
        let ring = k % 3 != 0;
        let center = if ring {
            let ang = (k % 6) as f32 * std::f32::consts::TAU / 6.0;
            c + Vec3::new(ang.cos() * 24.0, 13.0 + e * 3.0, ang.sin() * 24.0)
        } else {
            c + Vec3::new(0.0, 7.0 + e * 6.0, 0.0)
        };
        let s = Sparkle {
            center,
            radius: if ring { 1.5 + a * 2.5 } else { 6.0 + a * 16.0 },
            angle: b * std::f32::consts::TAU,
            speed: (0.05 + 0.12 * e) * if k % 2 == 0 { 1.0 } else { -1.0 },
            phase: a * 50.0,
            size: 0.05 + 0.05 * b,
        };
        commands.spawn((CasinoEntity, Mesh3d(sparkle_mesh.clone()), MeshMaterial3d(sparkle_mats[k % 3].clone()), Transform::from_translation(center).with_scale(Vec3::ZERO), s, NotShadowCaster, NotShadowReceiver));
    }
    // The pie overlay (hidden until a pie bust lands on you).
    let pie = images.add(pie_splat());
    commands.spawn((
        CasinoEntity,
        PieOverlay,
        Node { position_type: PositionType::Absolute, left: percent(0), top: percent(0), width: percent(100), height: percent(100), ..default() },
        ImageNode { image: pie, color: Color::srgba(1.0, 1.0, 1.0, 0.0), ..default() },
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(30),
    ));
}

// ------------------------------------------------------------------------------------ events

fn symbol_for(outcome: SlotOutcome, salt: u32) -> [usize; 3] {
    match outcome {
        SlotOutcome::Jackpot => [SEVEN; 3],
        SlotOutcome::Effect(e) => [(e as usize) % 7; 3],
        SlotOutcome::Bust(_) => {
            let a = (salt as usize) % 7;
            let b = (a + 1 + (salt as usize / 7) % 5) % 7;
            let c = (b + 2 + (salt as usize / 3) % 3) % 7;
            let c = if c == a { (c + 3) % 7 } else { c };
            [a, b, c]
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn casino_events(
    mut events: MessageReader<GameEvents>,
    mut fx: ResMut<CasinoFx>,
    active: Option<Res<ActiveWorld>>,
    mut sfx: MessageWriter<Sfx3d>,
    mut burst: MessageWriter<BurstFx>,
    hums: Query<(Entity, &SpinHum)>,
    mut commands: Commands,
) {
    if !fx.active {
        events.clear();
        return;
    }
    let local = active.as_ref().and_then(|a| a.local);
    let head = |p: bd_core::PlayerId| active.as_ref().and_then(|a| a.world.player(p)).map(|p| bv(p.mv.pos) + Vec3::Y * 1.6);
    for ev in events.read() {
        let GameEvent::Casino(e) = &ev.event else { continue };
        match *e {
            CasinoEvent::Pull { machine, secs, .. } => {
                let m = machine as usize;
                let Some(pos) = fx.slots.get(m).map(|s| bv(s.lever)) else { continue };
                let mf = &mut fx.machines[m];
                mf.lever_t = Some(0.0);
                mf.clunk = false;
                mf.spinning = true;
                mf.stop_in = [None; 3];
                mf.pending = None;
                for k in 0..3 {
                    mf.speed[k] = 14.0 + k as f32 * 2.5;
                }
                let _ = secs;
                sfx.write(Sfx3d::at("hatch_close", pos).pitch(1.25));
                for (ent, h) in &hums {
                    if h.0 == m {
                        commands.entity(ent).insert(SfxLoopEmitter { name: "rotor_loop", gain: 0.5, pitch: 2.2 });
                    }
                }
                fx.machines[m].hum = true;
            }
            CasinoEvent::Clunk { machine, .. } => {
                let m = machine as usize;
                let Some(pos) = fx.slots.get(m).map(|s| bv(s.lever)) else { continue };
                fx.machines[m].lever_t = Some(0.0);
                fx.machines[m].clunk = true;
                sfx.write(Sfx3d::at("transform_panel_click", pos).pitch(0.7).gain(0.8));
            }
            CasinoEvent::Payout { machine, player, outcome } => {
                let m = machine as usize;
                if m >= fx.machines.len() {
                    continue;
                }
                let salt = (ev.tick as u32).wrapping_mul(31) ^ machine as u32;
                let target = symbol_for(outcome, salt);
                let mf = &mut fx.machines[m];
                mf.target = target;
                for k in 0..3 {
                    mf.stop_in[k] = Some(REEL_STAGGER * k as f32 + 0.05);
                }
                mf.pending = Some((outcome, player));
            }
            CasinoEvent::EffectEnded { player, .. } => {
                if Some(player) == local
                    && let Some(p) = head(player)
                {
                    sfx.write(Sfx3d::at("shout_fizzle", p).gain(0.5));
                }
            }
            CasinoEvent::HouseSpin { pocket, half_width, sweep, lit } => {
                let start = h01(ev.tick as u32, 9) * std::f32::consts::TAU;
                fx.house = HouseFx { t: Some(0.0), pocket: pocket as usize, half_width: half_width as usize, sweep, lit, start_angle: start, landed: false };
                if let Some(w) = &fx.wheel {
                    sfx.write(Sfx3d::at("rotor_loop", bv(w.center) + Vec3::Y * 2.0).pitch(0.55).gain(0.9));
                    sfx.write(Sfx3d::at("assemble_click", bv(w.center) + Vec3::Y * 2.0).pitch(0.8));
                }
            }
            CasinoEvent::HouseBonus { player, .. } => {
                if let Some(p) = head(player) {
                    sfx.write(Sfx3d::at("charge_full", p).gain(if Some(player) == local { 0.7 } else { 0.35 }).pitch(1.3));
                    burst.write(BurstFx { kind: BurstKind::Shards, pos: p - Vec3::Y * 1.2, dir: Vec3::Y, scale: 0.6, color: Some([1.0, 0.75, 0.2]) });
                }
            }
        }
    }
    // Hums stop with the reels.
    for (ent, h) in &hums {
        if let Some(m) = fx.machines.get_mut(h.0)
            && m.hum
            && !m.spinning
        {
            m.hum = false;
            commands.entity(ent).remove::<SfxLoopEmitter>();
        }
    }
}

// ------------------------------------------------------------------------------------ animation

#[allow(clippy::too_many_arguments)]
fn animate_slots(
    time: Res<Time>,
    mut fx: ResMut<CasinoFx>,
    active: Option<Res<ActiveWorld>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut levers: Query<(&Lever, &mut Transform)>,
    mut sfx: MessageWriter<Sfx3d>,
    mut burst: MessageWriter<BurstFx>,
) {
    if !fx.active {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let local = active.as_ref().and_then(|a| a.local);
    let head = |p: bd_core::PlayerId| active.as_ref().and_then(|a| a.world.player(p)).map(|p| bv(p.mv.pos) + Vec3::Y * 1.6);
    let mut pie_now = false;
    for i in 0..fx.slots.len() {
        let m = fx.slots[i];
        let mut stop_sounds = 0;
        let mut finished = None;
        {
            let mf = &mut fx.machines[i];
            if let Some(t) = mf.lever_t.as_mut() {
                *t += dt;
                if *t > 1.2 {
                    mf.lever_t = None;
                }
            }
            mf.glow = (mf.glow - dt).max(0.0);
            for k in 0..3 {
                match mf.stop_in[k] {
                    Some(left) if left - dt <= 0.0 => {
                        mf.stop_in[k] = None;
                        mf.offset[k] = mf.target[k] as f32;
                        mf.speed[k] = 0.0;
                        stop_sounds += 1;
                        if k == 2 {
                            mf.spinning = false;
                            finished = mf.pending.take();
                        }
                    }
                    Some(left) => {
                        mf.stop_in[k] = Some(left - dt);
                        mf.offset[k] = (mf.offset[k] + mf.speed[k] * dt).rem_euclid(SYMBOLS as f32);
                    }
                    None if mf.spinning => {
                        mf.offset[k] = (mf.offset[k] + mf.speed[k] * dt).rem_euclid(SYMBOLS as f32);
                    }
                    None => {}
                }
            }
        }
        let reel_pos = bv(m.reels);
        for _ in 0..stop_sounds {
            sfx.write(Sfx3d::at("assemble_click", reel_pos).pitch(1.4).gain(0.8));
        }
        if let Some((outcome, player)) = finished {
            let front = reel_pos + bv(m.facing) * 1.2;
            match outcome {
                SlotOutcome::Jackpot => {
                    fx.machines[i].glow = 4.0;
                    sfx.write(Sfx3d::at("legendary_sting", reel_pos).gain(1.2));
                    sfx.write(Sfx3d::at("charge_full", reel_pos).pitch(1.5));
                    for k in 0..3 {
                        burst.write(BurstFx { kind: BurstKind::Shards, pos: front + Vec3::Y * (k as f32 * 0.6), dir: bv(m.facing) + Vec3::Y, scale: 2.2, color: Some([1.0, 0.78, 0.18]) });
                    }
                }
                SlotOutcome::Effect(_) => {
                    fx.machines[i].glow = 1.2;
                    sfx.write(Sfx3d::at("oath_complete", reel_pos).gain(0.9).pitch(1.2));
                    burst.write(BurstFx { kind: BurstKind::Shards, pos: front, dir: bv(m.facing) + Vec3::Y, scale: 1.0, color: Some([0.6, 1.0, 0.9]) });
                }
                SlotOutcome::Bust(kind) => {
                    // Wah-wah: a falling pair of fizzles (the sad trombone is on the SFX wishlist).
                    sfx.write(Sfx3d::at("sputter", reel_pos).pitch(0.8));
                    sfx.write(Sfx3d::at("shout_fizzle", reel_pos).pitch(0.6).gain(0.9));
                    let at = head(player).unwrap_or(front);
                    match kind {
                        BustKind::Confetti => {
                            for (k, c) in [[1.0, 0.2, 0.4], [0.2, 0.8, 1.0], [1.0, 0.85, 0.2], [0.5, 1.0, 0.3]].iter().enumerate() {
                                let dir = Vec3::new((k as f32 - 1.5) * 0.4, 1.0, 0.0);
                                burst.write(BurstFx { kind: BurstKind::Shards, pos: at, dir, scale: 1.3, color: Some(*c) });
                            }
                            sfx.write(Sfx3d::at("shout_release", at).pitch(1.6).gain(0.7));
                        }
                        BustKind::Pie => {
                            burst.write(BurstFx { kind: BurstKind::Shards, pos: at, dir: Vec3::Y, scale: 1.0, color: Some([1.0, 0.97, 0.9]) });
                            sfx.write(Sfx3d::at("land_heavy", at).pitch(1.6).gain(0.7));
                            if Some(player) == local {
                                pie_now = true;
                            }
                        }
                        BustKind::Hop => {
                            sfx.write(Sfx3d::at("air_jump", at).pitch(0.65));
                        }
                    }
                }
            }
        }
    }
    if pie_now {
        fx.pie = 1.0;
        fx.pie_hold = 0.7;
    }
    // Reel UVs + marquee glow.
    let now = time.elapsed_secs();
    for (i, mf) in fx.machines.iter().enumerate() {
        for k in 0..3 {
            if let Some(h) = fx.reel_mats.get(i * 3 + k)
                && let Some(mut mat) = materials.get_mut(h)
            {
                // Symbol `s` centred in the window; the strip scrolls downward while spinning.
                let v = (mf.offset[k] - 0.125) / SYMBOLS as f32;
                mat.uv_transform = Affine2::from_scale_angle_translation(Vec2::new(1.0, 1.25 / SYMBOLS as f32), 0.0, Vec2::new(0.0, v));
            }
        }
        if let Some(h) = fx.marquee_mats.get(i)
            && let Some(mut mat) = materials.get_mut(h)
        {
            let base = LinearRgba::rgb(0.9, 0.55, 0.12);
            let k = if mf.glow > 0.0 { 1.0 + 2.5 * (0.5 + 0.5 * (now * 9.0).sin()) * (mf.glow / 4.0).min(1.0) } else if mf.spinning { 1.4 } else { 1.0 };
            mat.emissive = base * 2.0 * k;
        }
    }
    // Levers: yank down toward the player, spring back (a jiggle on a clunk).
    for (l, mut t) in &mut levers {
        let mf = &fx.machines[l.machine];
        let ang = match mf.lever_t {
            None => 0.0,
            Some(t) if mf.clunk => 0.25 * (-t * 8.0).exp() * (t * 40.0).sin(),
            Some(t) if t < 0.12 => 1.6 * (t / 0.12),
            Some(t) => {
                let s = t - 0.12;
                1.6 * (-s * 5.0).exp() * (s * 11.0).cos()
            }
        };
        // Positive rotation about the right axis tips the knob toward the player (+facing).
        let q = Quat::from_axis_angle(l.axis, ang);
        t.translation = l.pivot + q * l.rest.translation;
        t.rotation = q * l.rest.rotation;
    }
}

fn animate_house(time: Res<Time>, mut fx: ResMut<CasinoFx>, mut wedges: Query<(&Wedge, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>)>, mut ball: Query<(&mut Transform, &mut Visibility), (With<Ball>, Without<Wedge>)>, mut materials: ResMut<Assets<StandardMaterial>>, mut sfx: MessageWriter<Sfx3d>) {
    if !fx.active {
        return;
    }
    let Some(w) = fx.wheel.as_ref() else { return };
    let (center, r_inner, r_outer, zero_angle, n) = (w.center, w.r_inner, w.r_outer, w.zero_angle, w.pockets().max(1));
    let target_a = w.pocket_angle(fx.house.pocket);
    let dt = time.delta_secs().min(0.1);
    let step = std::f32::consts::TAU / n as f32;
    let h = &mut fx.house;
    let Some(t) = h.t.as_mut() else {
        for (_, mut v, _) in &mut wedges {
            *v = Visibility::Hidden;
        }
        for (_, mut v) in &mut ball {
            *v = Visibility::Hidden;
        }
        return;
    };
    *t += dt;
    let t = *t;
    let (sweep, lit) = (h.sweep.max(0.1), h.lit);
    // Ball: laps the track decelerating, spirals in over the last 35 %, drops into the pocket.
    let u = (t / sweep).min(1.0);
    let ease = 1.0 - (1.0 - u).powi(3);
    let laps = 3.0;
    let mut delta = (target_a - h.start_angle).rem_euclid(std::f32::consts::TAU);
    delta += laps * std::f32::consts::TAU;
    let a = h.start_angle + delta * ease;
    let track_r = r_outer + 3.0;
    let pocket_r = (r_inner + r_outer) * 0.5;
    let inward = ((u - 0.65) / 0.35).clamp(0.0, 1.0);
    let r = track_r + (pocket_r - track_r) * inward * inward;
    let y_track = center.y + 1.6;
    let y = y_track + (center.y + 0.55 - y_track) * inward;
    let bounce = if inward > 0.0 && u < 1.0 { (u * 60.0).sin().abs() * 0.3 * (1.0 - u) } else { 0.0 };
    let ball_pos = bv(center) + Vec3::new(a.cos() * r, y - center.y + bounce, a.sin() * r);
    let ball_visible = t < sweep + lit;
    for (mut tr, mut v) in &mut ball {
        tr.translation = ball_pos;
        *v = if ball_visible { Visibility::Visible } else { Visibility::Hidden };
    }
    if u >= 1.0 && !h.landed {
        h.landed = true;
        sfx.write(Sfx3d::at("assemble_lock", ball_pos).pitch(1.3));
        sfx.write(Sfx3d::at("oath_complete", ball_pos + Vec3::Y * 2.0).pitch(1.4).gain(0.8));
    }
    // Lights: during the sweep one wedge chases the ball; then the winning region breathes.
    let under = (((a - zero_angle).rem_euclid(std::f32::consts::TAU) / step).floor() as usize).min(n - 1);
    let (sweep_mat, lit_mat) = (fx.sweep_mat.clone(), fx.lit_mat.clone());
    let (pocket, hw) = (fx.house.pocket, fx.house.half_width);
    for (wd, mut v, mut mm) in &mut wedges {
        let d = (wd.0 + n - pocket) % n;
        let in_region = d.min(n - d) <= hw;
        let show = if t < sweep { wd.0 == under } else { t < sweep + lit && in_region };
        *v = if show { Visibility::Visible } else { Visibility::Hidden };
        let want = if t < sweep { sweep_mat.clone() } else { lit_mat.clone() };
        if let Some(h) = want
            && mm.0 != h
        {
            mm.0 = h;
        }
    }
    if let Some(h) = lit_mat
        && let Some(mut m) = materials.get_mut(&h)
    {
        // Breathe slowly (no flashing).
        let k = 0.75 + 0.25 * (t * 2.2).sin();
        m.base_color = Color::LinearRgba(LinearRgba { alpha: 0.35 + 0.2 * k, ..LinearRgba::rgb(1.0, 0.72, 0.2) });
        m.emissive = LinearRgba::rgb(1.0, 0.72, 0.2) * (0.25 * k);
    }
    if t > sweep + lit + 0.5 {
        fx.house.t = None;
    }
}

fn animate_glitter(time: Res<Time>, fx: Res<CasinoFx>, comfort: Option<Res<ComfortSettings>>, mut q: Query<(&mut Sparkle, &mut Transform)>) {
    if !fx.active {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs();
    // Comfort preset: fewer, steadier sparkles.
    let calm = comfort.as_ref().is_some_and(|c| c.flashes <= 0.0);
    for (mut s, mut t) in &mut q {
        s.angle += s.speed * dt;
        let (sn, cs) = s.angle.sin_cos();
        t.translation = s.center + Vec3::new(cs * s.radius, (now * 0.3 + s.phase).sin() * 0.4, sn * s.radius);
        let tw = (now * 1.7 + s.phase).sin().max(0.0).powi(if calm { 2 } else { 8 });
        t.scale = Vec3::splat(s.size * (0.25 + 0.75 * tw));
        t.rotation = Quat::from_rotation_y(now * 0.8 + s.phase);
    }
}

fn animate_pie(time: Res<Time>, mut fx: ResMut<CasinoFx>, mut q: Query<(&mut ImageNode, &mut Visibility), With<PieOverlay>>) {
    if !fx.active {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    if fx.pie_hold > 0.0 {
        fx.pie_hold -= dt;
    } else {
        fx.pie = (fx.pie - dt / 1.6).max(0.0);
    }
    for (mut img, mut v) in &mut q {
        *v = if fx.pie > 0.0 { Visibility::Visible } else { Visibility::Hidden };
        img.color = Color::srgba(1.0, 1.0, 1.0, fx.pie.min(1.0) * 0.92);
    }
}
