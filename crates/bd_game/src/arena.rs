//! The world: bd_voxel's seeded Archipelago arena or the movement tutorial ([`MapKind`]).
//!
//! * Generated on a worker during loading; lighting/sky/fog come from its [`ArenaInfo`].
//! * [`ArenaWorld::world`] is `Arc<RwLock<VoxelWorld>>`: mutated (carved) on the main thread,
//!   read concurrently by the chunk-meshing workers and the debris collider.
//! * Every nonempty chunk is meshed at load; afterwards dirty chunks are re-submitted to the
//!   [`RemeshQueue`] every frame (the queue budgets starts/uploads, nearest-to-camera first).
//! * [`Explode`] carves a sphere, turns removed voxels + unsupported (`falling`) pieces into debris,
//!   throws hard-light shards and fires juice. `G` explodes at the crosshair; `Shift+G` fills the
//!   debris pool (20k benchmark).

use crate::camera::PlayerCamera;
use crate::audio::PlaySfx;
use crate::debris::{shatter, DebrisGround, DebrisImpulse, LockedVoxelCollider, ShatterParams, SpawnDebris, VfxRng, DEBRIS_CAPACITY};
use crate::input::LookAngles;
use crate::juice::{CameraShakeEvent, FovPunch, HitStop};
use crate::render::voxel_render::{ChunkMesh, RemeshQueue};
use crate::render::Environment;
use crate::state::{gameplay_active, AppState};
use crate::tick::FrameSet;
use crate::util::{core_iv3, core_v3, hash3, iv3, v3};
use crate::warmup::LoadingBlockers;
use bd_core::corps::Corps;
use bd_core::voxel::{raycast, voxel_center};
use bd_core::VOXEL_SIZE;
use bd_voxel::{
    chunk_origin, generate_arena, material_def, ArenaInfo, CarveOp, CarveShape, Mesher, VoxelWorld, CHUNK_SIZE,
};
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use std::cell::RefCell;
use std::sync::{Arc, RwLock};

pub const ARENA_SEED: u64 = 1;

#[derive(Resource, Clone)]
pub struct ArenaWorld {
    pub world: Arc<RwLock<VoxelWorld>>,
    pub info: Arc<MapInfo>,
}

impl ArenaWorld {
    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, VoxelWorld> {
        self.world.read().unwrap_or_else(|e| e.into_inner())
    }
    pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, VoxelWorld> {
        self.world.write().unwrap_or_else(|e| e.into_inner())
    }
}

/// Blow a hole in the world.
#[derive(Message, Clone, Copy, Debug)]
pub struct Explode {
    pub center: Vec3,
    pub radius: f32,
    /// Carve power relative to material hardness (1 = removes stone).
    pub power: f32,
    /// Hard-light shard colour (linear) — the caster's corps colour.
    pub color: [f32; 3],
    /// Extra glowing shards on top of the terrain debris.
    pub shards: usize,
    /// Debris cube budget for this explosion (terrain + falling pieces).
    pub budget: usize,
}

impl Explode {
    pub fn standard(center: Vec3) -> Self {
        Self { center, radius: 3.0, power: 1.2, color: Corps::Willpower.color(), shards: 260, budget: 5000 }
    }
}

/// Which world to play in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapKind {
    /// The seeded Archipelago FFA arena.
    #[default]
    Arena,
    /// The movement tutorial course (stations + timed peak run).
    Tutorial,
    /// Vice-City-sunset city arena (bd_voxel `generate_miami`).
    Miami,
    /// Asteroid ring + wrecked station in low orbit (bd_voxel `generate_orbit`; gravity by altitude).
    Orbit,
    /// Five stacked liminal levels, all breakable (bd_voxel `generate_backrooms`).
    Backrooms,
    /// Japanese castle town on a plateau over the void (bd_voxel `generate_sakura`).
    Sakura,
    /// Volcanic basin: lava rivers (= the void), mining facility, twin-spired fortress (bd_voxel `generate_ember`).
    Ember,
    /// The folded city: streets curl into surf half-pipes, towers hang from the sky (bd_voxel `generate_mirror`).
    Mirrorfold,
    /// High Roller: a Vegas casino atop a tower, giant roulette arena, optional slot machines (bd_voxel `generate_casino`).
    Casino,
    /// Valley of the End: river gorge, falls, two colossal statues; walk on water (bd_voxel `generate_valley`).
    Valley,
    /// Cybertron: Iacon vs Kaon on a raised city plate under open space, the Well of AllSparks, transforming
    /// panels + a space bridge (bd_voxel `generate_cybertron`; private-use Transformers fan map).
    Cybertron,
    /// Turtle Lair: vaulted sewer lair + tunnels under NYC rooftops, subway chasm (bd_voxel
    /// `generate_lair`; private-use TMNT map).
    Lair,
    /// Giant's Room: a kid's bedroom at toy scale (×30) — bed, bookshelf, desk, toy track round the
    /// rug, open-window side KO, soft ceiling star KO (bd_voxel `generate_giant`).
    GiantsRoom,
    /// Hyperline: an endless high-speed train chase (a static voxel train, the world streams past;
    /// the SLIPSTREAM wind; bd_voxel `generate_hyperline`, backdrop `present::hyperline`).
    Hyperline,
}

impl MapKind {
    pub fn from_cli(s: &str) -> Option<Self> {
        match s {
            "arena" | "archipelago" => Some(MapKind::Arena),
            "tutorial" | "course" => Some(MapKind::Tutorial),
            "miami" => Some(MapKind::Miami),
            "orbit" | "space" => Some(MapKind::Orbit),
            "backrooms" => Some(MapKind::Backrooms),
            "sakura" | "sakura_keep" => Some(MapKind::Sakura),
            "ember" => Some(MapKind::Ember),
            "mirror" | "mirrorfold" => Some(MapKind::Mirrorfold),
            "casino" | "high_roller" | "highroller" => Some(MapKind::Casino),
            "cybertron" | "iacon" | "kaon" => Some(MapKind::Cybertron),
            "valley" | "valley_of_the_end" => Some(MapKind::Valley),
            "lair" | "turtle_lair" | "tmnt" => Some(MapKind::Lair),
            "giant" | "giants_room" | "bedroom" => Some(MapKind::GiantsRoom),
            "train" | "hyperline" => Some(MapKind::Hyperline),
            _ => None,
        }
    }
}

/// Map-independent info the client needs (spawns, void, landmark) + the tutorial data if any.
#[derive(Clone, Debug)]
pub struct MapInfo {
    pub kind: MapKind,
    /// Feet positions.
    pub spawns: Vec<Vec3>,
    /// Explicit spawn view yaw; `None` = face `center`.
    pub spawn_yaw: Option<f32>,
    pub center: Vec3,
    pub void_y: f32,
    pub tutorial: Option<bd_voxel::TutorialInfo>,
    /// The sim's view of the map (FFA maps only; `None` for the tutorial).
    pub arena: Option<ArenaInfo>,
    /// Sea level (Miami): water surface / splash height.
    pub water_y: Option<f32>,
    /// City maps: water colours (sRGB), sun colour (sRGB), fog end distance (m), neon boost.
    pub water_color: Option<([f32; 3], [f32; 3])>,
    pub sun_color: Option<[f32; 3]>,
    pub fog_end: Option<f32>,
    pub neon_boost: f32,
}

/// A second, untouched copy of the current FFA map's voxels for a `bd_sim::World` (the rendered
/// `ArenaWorld` is the client-side mirror). Built on the loader thread next to the render copy;
/// practice / NET host take it with [`SimArenaCopy::take`]. Taking it marks the render copy for
/// regeneration when the game is left (it will have been carved).
#[derive(Resource, Default)]
pub struct SimArenaCopy {
    slot: Option<(VoxelWorld, ArenaInfo, u64)>,
    pub taken: bool,
}

impl SimArenaCopy {
    /// `(voxels, info, arena seed)`.
    pub fn take(&mut self) -> Option<(VoxelWorld, ArenaInfo, u64)> {
        let s = self.slot.take();
        if s.is_some() {
            self.taken = true;
        }
        s
    }
    pub fn is_ready(&self) -> bool {
        self.slot.is_some()
    }
}

/// Generate `kind` with `seed` (the render copy; call twice for a sim copy).
pub fn generate_map(kind: MapKind, seed: u64) -> (VoxelWorld, MapInfo, Option<ArenaInfo>) {
    match kind {
        MapKind::Arena => {
            let (world, info) = generate_arena(seed);
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                water_y: None,
                water_color: None,
                sun_color: None,
                fog_end: None,
                neon_boost: 0.0,
            };
            (world, map, Some(info))
        }
        MapKind::Miami => {
            let (world, m) = bd_voxel::generate_miami(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                water_y: Some(m.water_y),
                water_color: Some((m.water_color, m.water_deep)),
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Orbit => {
            let (world, m) = bd_voxel::generate_orbit(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // No sea: the planet is drawn by `present::space`.
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Backrooms => {
            let (world, m) = bd_voxel::generate_backrooms(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // No sea: the pools are voxels.
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Sakura => {
            let (world, m) = bd_voxel::generate_sakura(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // No sea: the koi pond is voxels, below the plateau is the void.
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Ember => {
            let (world, m) = bd_voxel::maps::generate_ember(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // No sea: the lava is voxels on the floor, just under the KO line.
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Mirrorfold => {
            let (world, m) = bd_voxel::generate_mirror(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // No sea: the fractures open onto the mirrored void (drawn by `present::mirror`).
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Valley => {
            let (world, m) = bd_voxel::generate_valley(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // The KO water under the plunge hole / past the lip; the river and pool are water
                // sheets drawn by `present::valley`.
                water_y: Some(m.water_y),
                water_color: Some((m.water_color, m.water_deep)),
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Casino => {
            let (world, m) = bd_voxel::generate_casino(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // No sea: the city glows far below the void line (the roof pool is voxels).
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Cybertron => {
            let (world, m) = bd_voxel::maps::generate_cybertron(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // No sea: the metal planet below is drawn by `present::space` (Cybertron style).
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Hyperline => {
            let (world, m) = bd_voxel::generate_hyperline(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // No sea: the city shaft, rings and guideway are drawn by `present::hyperline`.
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::GiantsRoom => {
            let (world, m) = bd_voxel::generate_giant(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // Indoors: no sea; the sunbeam + dust are drawn by `present::giant`.
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Lair => {
            let (world, m) = bd_voxel::generate_lair(seed);
            let info = m.arena.clone();
            let map = MapInfo {
                kind,
                spawns: info.spawns.iter().map(|s| v3(*s)).collect(),
                spawn_yaw: None,
                center: v3(info.center),
                void_y: info.void_y,
                tutorial: None,
                arena: Some(info.clone()),
                // No sea: the sewer channels are voxels, the subway chasm is void.
                water_y: None,
                water_color: None,
                sun_color: Some(m.sun_color),
                fog_end: Some(m.fog_end),
                neon_boost: m.neon_boost,
            };
            (world, map, Some(info))
        }
        MapKind::Tutorial => {
            let (world, info) = bd_voxel::generate_tutorial();
            let map = MapInfo {
                kind,
                spawns: vec![v3(info.peak_start)],
                spawn_yaw: Some(info.peak_yaw),
                center: v3(info.start_finish.0),
                void_y: info.void_y,
                tutorial: Some(info),
                arena: None,
                water_y: None,
                water_color: None,
                sun_color: None,
                fog_end: None,
                neon_boost: 0.0,
            };
            (world, map, None)
        }
    }
}

/// The map to load/play. Change it (and enter `AppState::Loading`) to switch maps.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrentMap(pub MapKind);

type GenResult = (VoxelWorld, MapInfo, Option<ArenaInfo>, Option<VoxelWorld>);

#[derive(Resource)]
struct ArenaGenTask(Task<GenResult>, MapKind);

/// Which map the current `ArenaWorld` holds.
#[derive(Resource, Default)]
struct LoadedMap(Option<MapKind>);

pub struct ArenaPlugin;

impl Plugin for ArenaPlugin {
    fn build(&self, app: &mut App) {
        let map = app
            .world()
            .resource::<crate::cli::CliArgs>()
            .map
            .as_deref()
            .and_then(MapKind::from_cli)
            .unwrap_or_default();
        app.add_message::<Explode>()
            .insert_resource(CurrentMap(map))
            .init_resource::<LoadedMap>()
            .init_resource::<SimArenaCopy>()
            .add_systems(OnExit(AppState::InGame), regen_after_play)
            .add_systems(Update, (start_generation, finish_generation, remesh_dirty_chunks).chain().before(FrameSet::Sim))
            .add_systems(Update, explode_at_crosshair.run_if(gameplay_active).in_set(FrameSet::Input))
            .add_systems(Update, handle_explosions.in_set(FrameSet::Present).before(crate::debris::DebrisSimSet))
            .add_systems(Update, fade_explosion_lights.in_set(FrameSet::Present));
    }
}

/// Starts generating `CurrentMap` whenever it differs from what is loaded (at startup and when the
/// menu switches maps). Tears down the old world's chunks first.
#[allow(clippy::too_many_arguments)]
fn start_generation(
    mut commands: Commands,
    current: Res<CurrentMap>,
    mut loaded: ResMut<LoadedMap>,
    task: Option<Res<ArenaGenTask>>,
    arena: Option<Res<ArenaWorld>>,
    mut queue: ResMut<RemeshQueue>,
    mut ground: ResMut<DebrisGround>,
    mut pool: ResMut<crate::debris::DebrisPool>,
    mut blockers: ResMut<LoadingBlockers>,
) {
    let kind = current.0;
    if loaded.0 == Some(kind) || task.as_ref().is_some_and(|t| t.1 == kind) {
        return;
    }
    // Tear down the previous map.
    if arena.is_some() {
        commands.remove_resource::<ArenaWorld>();
        let keys: Vec<IVec3> = queue.entities.keys().copied().collect();
        for k in keys {
            queue.remove(k);
        }
        ground.0 = None;
        pool.clear();
    }
    loaded.0 = None;
    blockers.arena_pending = true;
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let t = std::time::Instant::now();
        let (world, map, info) = generate_map(kind, ARENA_SEED);
        // The sim's own copy (bd_voxel worlds are not Clone): generated again, same seed.
        let sim = info.is_some().then(|| generate_map(kind, ARENA_SEED).0);
        info!("{kind:?} generated in {:.0} ms", t.elapsed().as_secs_f32() * 1000.0);
        (world, map, info, sim)
    });
    commands.insert_resource(ArenaGenTask(task, kind));
}

/// Leaving the game after a sim world took the terrain copy: the render mirror was carved, so
/// regenerate both copies (behind the menu).
fn regen_after_play(sim_copy: Res<SimArenaCopy>, mut loaded: ResMut<LoadedMap>) {
    if sim_copy.taken {
        loaded.0 = None;
    }
}

fn srgb3(c: [f32; 3]) -> LinearRgba {
    Color::srgb(c[0], c[1], c[2]).to_linear()
}

#[allow(clippy::too_many_arguments)]
fn finish_generation(
    mut commands: Commands,
    task: Option<ResMut<ArenaGenTask>>,
    mut env: ResMut<Environment>,
    mut queue: ResMut<RemeshQueue>,
    mut ground: ResMut<DebrisGround>,
    mut blockers: ResMut<LoadingBlockers>,
    mut loaded: ResMut<LoadedMap>,
    mut sim_copy: ResMut<SimArenaCopy>,
) {
    let Some(mut task) = task else { return };
    let kind = task.1;
    let Some((mut world, map, arena_info, sim_world)) = check_ready(&mut task.0) else { return };
    sim_copy.slot = match (sim_world, arena_info.clone()) {
        (Some(w), Some(i)) => Some((w, i, ARENA_SEED)),
        _ => None,
    };
    sim_copy.taken = false;
    commands.remove_resource::<ArenaGenTask>();

    // Environment from the map (the tutorial keeps the current sky).
    if let Some(info) = arena_info {
        env.sun_dir = v3(info.sun_dir).normalize();
        env.zenith = srgb3(info.sky_zenith);
        env.horizon = srgb3(info.sky_horizon);
        env.fog_color = srgb3(info.fog_color);
        env.ambient_brightness = 500.0 + info.ambient * 1400.0;
    }
    // City maps: sunset key light + fog distance.
    if let Some(c) = map.sun_color {
        env.sun_color = Color::srgb(c[0], c[1], c[2]);
    }
    if let Some(f) = map.fog_end {
        env.fog_visibility = f.clamp(40.0, 2000.0);
    }

    world.drain_dirty();
    let keys: Vec<IVec3> = world.nonempty_chunks().map(iv3).collect();
    let arena = ArenaWorld { world: Arc::new(RwLock::new(world)), info: Arc::new(map) };
    for key in &keys {
        submit_chunk(&mut queue, &arena, *key);
    }
    info!("{kind:?}: {} nonempty chunks queued for meshing", keys.len());
    ground.0 = Some(Arc::new(LockedVoxelCollider(arena.world.clone())));
    commands.insert_resource(arena);
    loaded.0 = Some(kind);
    blockers.arena_pending = false;
}

fn chunk_center(key: IVec3) -> Vec3 {
    (iv3(chunk_origin(core_iv3(key))).as_vec3() + Vec3::splat(CHUNK_SIZE as f32 * 0.5)) * VOXEL_SIZE
}

fn submit_chunk(queue: &mut RemeshQueue, arena: &ArenaWorld, key: IVec3) {
    let world = arena.world.clone();
    queue.submit(key, Vec3::ZERO, chunk_center(key), move || {
        thread_local! {
            static MESHER: RefCell<Mesher> = RefCell::new(Mesher::new());
        }
        let mut out = ChunkMesh::default();
        let w = world.read().unwrap_or_else(|e| e.into_inner());
        MESHER.with(|m| m.borrow_mut().mesh_chunk_into(&w, core_iv3(key), &mut out));
        out
    });
}

fn remesh_dirty_chunks(
    arena: Option<Res<ArenaWorld>>,
    mut queue: ResMut<RemeshQueue>,
    cam: Query<&Transform, With<PlayerCamera>>,
    state: Res<State<AppState>>,
    mut scratch: Local<Vec<bd_core::IVec3>>,
) {
    if let Ok(t) = cam.single() {
        queue.focus = t.translation;
    }
    // Upload everything as fast as possible while the loading screen is up.
    queue.max_uploads_per_frame = if *state.get() == AppState::Loading { 256 } else { 12 };
    let Some(arena) = arena else { return };
    if !arena.read().has_dirty() {
        return;
    }
    arena.write().drain_dirty_into(&mut scratch);
    for key in scratch.drain(..) {
        submit_chunk(&mut queue, &arena, iv3(key));
    }
}

/// `BD_DEBUG_KEYS=1`: developer hotkeys that would otherwise collide with gameplay binds.
fn debug_keys() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("BD_DEBUG_KEYS").is_ok_and(|v| v != "0"))
}

fn explode_at_crosshair(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<crate::input::UiFocus>,
    arena: Option<Res<ArenaWorld>>,
    look: Res<LookAngles>,
    cam: Query<&Transform, With<PlayerCamera>>,
    mut out: MessageWriter<Explode>,
    active: Option<Res<crate::sim_bridge::ActiveWorld>>,
) {
    // G is the power key (and Shift the boost) while a sim world runs: the debug explosion is
    // Shift+G there only with `BD_DEBUG_KEYS=1`, and never while typing (a capital G is text).
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if !keys.just_pressed(KeyCode::KeyG) || focus.typing || (active.is_some() && (!shift || !debug_keys())) {
        return;
    }
    let (Some(arena), Ok(cam)) = (arena, cam.single()) else { return };
    let dir = look.forward();
    let center = crosshair_target(&arena, cam.translation, dir);
    let mut e = Explode::standard(center);
    if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        // Benchmark: fill the whole pool.
        e.radius = 5.0;
        e.budget = DEBRIS_CAPACITY;
        e.shards = 1200;
    }
    out.write(e);
}

/// First solid hit along the view ray (or a point 40 m out when looking at the sky).
pub fn crosshair_target(arena: &ArenaWorld, eye: Vec3, dir: Vec3) -> Vec3 {
    match raycast(&*arena.read(), core_v3(eye), core_v3(dir), 300.0) {
        Some(hit) => v3(hit.point),
        None => eye + dir * 40.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_explosions(
    mut commands: Commands,
    mut events: MessageReader<Explode>,
    arena: Option<Res<ArenaWorld>>,
    cam: Query<&Transform, With<PlayerCamera>>,
    mut rng: ResMut<VfxRng>,
    mut debris: MessageWriter<SpawnDebris>,
    mut impulses: MessageWriter<DebrisImpulse>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut fov: MessageWriter<FovPunch>,
    mut hitstop: MessageWriter<HitStop>,
    mut sfx: MessageWriter<PlaySfx>,
    mut seed: Local<u32>,
) {
    let Some(arena) = arena else {
        events.clear();
        return;
    };
    let eye = cam.single().map(|t| t.translation).unwrap_or(Vec3::ZERO);
    for e in events.read() {
        *seed = seed.wrapping_add(1);
        let op = CarveOp { shape: CarveShape::Sphere { center: core_v3(e.center), radius: e.radius }, power: e.power, seed: *seed };
        let t0 = std::time::Instant::now();
        let outcome = arena.write().apply_carve(&op);
        let carve_ms = t0.elapsed().as_secs_f32() * 1000.0;
        if carve_ms > 1.0 {
            warn!("carve took {carve_ms:.2} ms ({} removed, {} falling pieces)", outcome.removed.len(), outcome.falling.len());
        }
        let rng = &mut rng.0;

        // Sound: blast + the dominant material breaking, attenuated by distance.
        let d = eye.distance(e.center);
        let gain = (1.0 / (1.0 + d / 25.0)).clamp(0.08, 1.0);
        sfx.write(PlaySfx::new(if e.radius >= 4.0 { "explosion_big" } else { "explosion_small" }).gain(gain));
        let mut counts = [0usize; 7];
        for r in outcome.removed.iter().take(512) {
            counts[material_def(r.material).debris as usize] += 1;
        }
        if let Some((class, _)) = counts.iter().enumerate().max_by_key(|(_, c)| **c).filter(|(_, c)| **c > 0) {
            let name = match class {
                0 => "break_stone",
                1 | 5 | 6 => "break_dirt",
                2 => "break_wood",
                3 => "break_leaves",
                _ => "break_crystal",
            };
            sfx.write(PlaySfx::new(name).gain(gain));
        }
        if !outcome.falling.is_empty() {
            sfx.write(PlaySfx::new("collapse_rumble").gain(gain));
        }

        // Terrain debris: whole voxels if many, 2×2×2 sub-cubes if few; sampled down to the budget.
        let falling_voxels: usize = outcome.falling.iter().map(|f| f.len()).sum();
        let terrain_budget = e.budget.saturating_sub(falling_voxels.min(e.budget / 3)).max(1);
        let n = outcome.removed.len();
        let subdivide = if n * 8 <= terrain_budget { 2 } else { 1 };
        let keep = (terrain_budget as f32 / (n * (subdivide * subdivide * subdivide) as usize).max(1) as f32).min(1.0);
        let mut plain = Vec::with_capacity(n.min(terrain_budget));
        let mut glowing = Vec::new();
        for r in &outcome.removed {
            if keep < 1.0 && rng.f32() > keep {
                continue;
            }
            let def = material_def(r.material);
            let c = bd_voxel::material::linear_color(r.material);
            let j = 1.0 + (hash3(r.pos.x, r.pos.y, r.pos.z) - 0.5) * 2.0 * def.jitter;
            let color = [c[0] * j, c[1] * j, c[2] * j];
            if def.emissive > 0.0 {
                glowing.push((v3(voxel_center(r.pos)), color, def.emissive));
            } else {
                plain.push((v3(voxel_center(r.pos)), color));
            }
        }
        let params = ShatterParams {
            origin: e.center,
            strength: 16.0 * e.power.sqrt(),
            voxel_size: VOXEL_SIZE,
            subdivide: subdivide as u32,
            lifetime: (3.5, 7.0),
            lift: 6.0,
            ..default()
        };
        shatter(&mut debris, rng, plain, &params);
        for (pos, color, glow) in glowing {
            shatter(&mut debris, rng, [(pos, color)], &ShatterParams { emissive: 3.0 * glow, ..params });
        }

        // Unsupported pieces: fall as clumps (shared velocity + a little spread), sampled to budget.
        let falling_budget = e.budget / 3;
        let fkeep = (falling_budget as f32 / falling_voxels.max(1) as f32).min(1.0);
        for piece in &outcome.falling {
            let com = v3(piece.center_of_mass());
            let away = (com - e.center).normalize_or_zero();
            let common = away * rng.range(2.0, 5.0) + Vec3::Y * rng.range(0.5, 2.5);
            for (v, m) in &piece.voxels {
                if fkeep < 1.0 && rng.f32() > fkeep {
                    continue;
                }
                let c = bd_voxel::material::linear_color(*m);
                let def = material_def(*m);
                debris.write(SpawnDebris {
                    pos: v3(voxel_center(*v)),
                    vel: common + rng.unit_vec3() * 0.6,
                    color: c,
                    size: VOXEL_SIZE * 0.96,
                    emissive: def.emissive * 3.0,
                    lifetime: rng.range(6.0, 9.0),
                });
            }
        }

        // Hard-light shards in the caster's colour.
        for _ in 0..e.shards {
            let dir = (rng.unit_vec3() + Vec3::Y * 0.35).normalize();
            debris.write(SpawnDebris {
                pos: e.center + dir * 0.3,
                vel: dir * rng.range(14.0, 34.0),
                color: e.color,
                size: rng.range(0.08, 0.2),
                emissive: rng.range(4.0, 9.0),
                lifetime: rng.range(0.5, 1.3),
            });
        }
        impulses.write(DebrisImpulse { center: e.center, radius: e.radius * 3.0, strength: 10.0 });

        // A short, hot flash lighting up the crater and everything nearby.
        let c = e.color;
        commands.spawn((
            ExplosionLight { t: 0.0, life: 0.22, peak: 2.5e6 * e.radius / 3.0 },
            PointLight {
                color: Color::srgb(0.6 + c[0] * 0.4, 0.6 + c[1] * 0.4, 0.6 + c[2] * 0.4),
                intensity: 0.0,
                range: e.radius * 8.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(e.center + Vec3::Y * 0.8),
        ));

        // Camera: shake + punch scaled by distance.
        let k = (1.0 - d / (e.radius * 18.0)).clamp(0.0, 1.0);
        if k > 0.0 {
            shake.write(CameraShakeEvent { trauma: 0.25 + 0.55 * k, dir: (e.center - eye).normalize_or_zero() });
            fov.write(FovPunch { degrees: 2.5 * k });
            if k > 0.6 {
                hitstop.write(HitStop { secs: 0.04 });
            }
        }
    }
}

#[derive(Component)]
struct ExplosionLight {
    t: f32,
    life: f32,
    peak: f32,
}

fn fade_explosion_lights(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut q: Query<(Entity, &mut ExplosionLight, &mut PointLight)>,
) {
    for (e, mut l, mut light) in &mut q {
        l.t += time.delta_secs();
        let k = 1.0 - l.t / l.life;
        if k <= 0.0 {
            commands.entity(e).despawn();
        } else {
            light.intensity = l.peak * k * k;
        }
    }
}
