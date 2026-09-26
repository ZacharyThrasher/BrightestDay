//! Offline, deterministic render of the game's audio — so the mix can be *measured* and *listened
//! to* without running the game.
//!
//!     cargo run --release -p bd_audio --example render_mix -- --scenario brawl [--mode both|after|before]
//!         [--out target/audio] [--secs 45] [--seed 1]
//!
//! Scenarios: `brawl` (headless bd_sim 8-bot FFA on Archipelago, listener = bot 0), `archipelago_idle`,
//! `miami_street`, `interior` (a sealed Miami room: gunfight outside, shots inside, then the wall is
//! blown open), `stress` (200 voices for CPU).
//!
//! AFTER drives the real engine (bd_audio: SimAudio mapping → AudioEngine → Mixer with buses,
//! reverb, HDR, ducking, ambience, acoustics). BEFORE replays the *same recorded plays* through an
//! approximation of the previous client path (every voice summed flat, no buses/reverb/ambience,
//! rodio's inverse-square spatial model, the old 0.28 KO duck, the old per-frame voice caps, the old
//! assets from `target/audio/sfx_before`). Writes `<scenario>_<mode>.wav|png|json`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bd_audio::acoustics::ListenerSpace;
use bd_audio::ambience::{AmbienceDirector, EmitterMap};
use bd_audio::analysis;
use bd_audio::bank::SoundBank;
use bd_audio::cue::{CueTable, NUM_BUSES};
use bd_audio::engine::Recorded;
use bd_audio::game::{SimAudio, SimCtx, corps_key};
use bd_audio::*;
use bd_core::PlayerId;
use bd_core::mechanics::Element;
use bd_core::voxel::{VoxelQuery, raycast};
use bd_sim::GameEvent;
use bd_sim::combat::{HitKind, ImpactFx};
use bd_sim::weapons::GunArchetype;
use bd_voxel::{CarveOp, CarveShape, VoxelWorld};
use glam::Vec3;
use serde_json::json;

const SR: u32 = 48000;
/// Game frame = 12 blocks = 768 samples = 16 ms.
const FRAME_BLOCKS: usize = 12;
const FRAME_DT: f32 = (FRAME_BLOCKS * BLOCK) as f32 / SR as f32;
const LOCAL: PlayerId = PlayerId(0);

fn arg<T: std::str::FromStr>(name: &str, default: T) -> T {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

// =============================================================================================
// Scenario plumbing
// =============================================================================================

/// What the listener is doing this frame.
struct Frame {
    ear: Vec3,
    fwd: Vec3,
    vel: Vec3,
    inputs: MixInputs,
}

/// Everything audio on the game side (the Phase B client owns the same set).
struct Rig {
    engine: AudioEngine,
    director: MixDirector,
    amb: AmbienceDirector,
    space: ListenerSpace,
    sim: SimAudio,
    cfg: MixConfig,
    wind: Option<VoiceHandle>,
    events: usize,
}

impl Rig {
    fn ctx<'a, W: VoxelQuery + ?Sized>(
        voxels: &'a W,
        ear: Vec3,
        pos_of: &'a dyn Fn(PlayerId) -> Option<Vec3>,
        corps_of: &'a dyn Fn(PlayerId) -> Option<&'static str>,
    ) -> SimCtx<'a, W> {
        SimCtx { voxels, ear, local: Some(LOCAL), pos_of, corps_of, local_move_predicted: false, match_point: false }
    }
    fn event<W: VoxelQuery + ?Sized>(&mut self, e: &GameEvent, ctx: &SimCtx<W>) {
        self.events += 1;
        self.sim.event(e, ctx, &mut self.engine, &mut self.director, Some(&mut self.amb));
    }
}

trait Scenario {
    fn map(&self) -> &'static str;
    fn world(&self) -> &VoxelWorld;
    /// Advance to time `t`, fire this frame's events through the rig.
    fn frame(&mut self, t: f32, rig: &mut Rig) -> Frame;
    fn scan(&self) -> EmitterMap;
    /// Microseconds of this frame spent in the simulation itself (excluded from the audio budget).
    fn sim_us(&self) -> f32 {
        0.0
    }
}

fn look(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(-yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos())
}

fn in_game(damage: f32, alive: bool) -> MixInputs {
    MixInputs { scene: Scene::InGame, damage_pct: damage, alive, ..Default::default() }
}

fn ground_below(w: &VoxelWorld, p: Vec3) -> Option<Vec3> {
    raycast(w, p + Vec3::Y * 2.0, Vec3::NEG_Y, 200.0).map(|h| h.point)
}

fn hit_event(target: PlayerId, source: Option<PlayerId>, point: Vec3, strength: f32, crit: bool, lethal: bool) -> GameEvent {
    GameEvent::Hit {
        target,
        source,
        construct: None,
        kind: HitKind::Shot,
        element: Element::Light,
        damage: 10.0 * strength,
        knockback: 10.0 * strength,
        point,
        bonus: 0.0,
        tumble: false,
        lethal,
        crit,
        impact: ImpactFx { strength, heavy: strength > 1.0, ..Default::default() },
    }
}

fn shot(owner: PlayerId, arch: GunArchetype, muzzle: Vec3, dir: Vec3) -> GameEvent {
    GameEvent::ShotFired { owner, construct: None, archetype: arch, muzzle, dir, recoil: 1.0, pellets: 1 }
}

fn trace(owner: PlayerId, from: Vec3, to: Vec3, normal: Vec3) -> GameEvent {
    GameEvent::ShotTrace { owner, archetype: GunArchetype::HandCannon, from, to, normal, hit_player: None }
}

/// Fires each (time, event) once when the clock passes it.
struct Timeline {
    items: Vec<(f32, GameEvent)>,
    next: usize,
}

impl Timeline {
    fn new(mut items: Vec<(f32, GameEvent)>) -> Self {
        items.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self { items, next: 0 }
    }
    fn due(&mut self, t: f32) -> Vec<GameEvent> {
        let mut out = Vec::new();
        while self.next < self.items.len() && self.items[self.next].0 <= t {
            out.push(self.items[self.next].1.clone());
            self.next += 1;
        }
        out
    }
}

// =============================================================================================
// brawl: headless bd_sim bots
// =============================================================================================

struct Brawl {
    w: bd_sim::World,
    brains: Vec<bd_sim::bots::BotBrain>,
    inputs: Vec<(PlayerId, bd_core::input::PlayerInput)>,
    ticks: u64,
    t0_ticks: u64,
    prev_pos: Vec<Vec3>,
    last_sim_us: f32,
}

impl Brawl {
    fn new(seed: u64, warmup_s: f32) -> Self {
        let mut cfg = bd_sim::SimConfig { seed, ..Default::default() };
        cfg.rules.time_limit = 600.0;
        cfg.check_invariants = false;
        let mut w = bd_sim::World::new(cfg);
        let mut brains = bd_sim::bots::add_bots(&mut w, 0, 8, 0.85, seed);
        let mut inputs = Vec::new();
        let warm = (warmup_s * 120.0) as u64;
        for _ in 0..warm {
            inputs.clear();
            bd_sim::bots::drive(&mut w, &mut brains, &mut inputs);
            w.tick(&inputs);
        }
        let prev_pos = w.players.iter().map(|p| p.mv.pos).collect();
        Self { w, brains, inputs, ticks: warm, t0_ticks: warm, prev_pos, last_sim_us: 0.0 }
    }
}

impl Scenario for Brawl {
    fn map(&self) -> &'static str {
        "archipelago"
    }
    fn world(&self) -> &VoxelWorld {
        &self.w.voxels
    }
    fn sim_us(&self) -> f32 {
        self.last_sim_us
    }
    fn scan(&self) -> EmitterMap {
        let b = self.w.arena.bounds;
        EmitterMap::scan(&self.w.voxels, b.min, b.max, None, 4.0)
    }
    fn frame(&mut self, t: f32, rig: &mut Rig) -> Frame {
        let target = self.t0_ticks + (t * 120.0) as u64;
        let mut evs: Vec<GameEvent> = Vec::new();
        let s0 = Instant::now();
        while self.ticks < target {
            self.inputs.clear();
            bd_sim::bots::drive(&mut self.w, &mut self.brains, &mut self.inputs);
            evs.extend(self.w.tick(&self.inputs).iter().cloned());
            self.ticks += 1;
        }
        self.last_sim_us = s0.elapsed().as_secs_f32() * 1e6;
        let w = &self.w;
        let me = w.player(LOCAL);
        let (feet, yaw, pitch, vel, dmg, alive) = me
            .map(|p| (p.mv.pos, p.input.yaw, p.input.pitch, p.mv.vel, p.damage_pct, p.alive))
            .unwrap_or((Vec3::ZERO, 0.0, 0.0, Vec3::ZERO, 0.0, true));
        let ear = feet + Vec3::Y * 1.6;
        let pos_of = |id: PlayerId| w.player(id).map(|p| p.mv.pos + Vec3::Y * 0.9);
        let corps_of = |id: PlayerId| w.player(id).map(|p| corps_key(p.corps));
        let ctx = Rig::ctx(&w.voxels, ear, &pos_of, &corps_of);
        for e in &evs {
            rig.event(e, &ctx);
        }
        // Footsteps for everyone (the limiter keeps the nearest few).
        for (i, p) in w.players.iter().enumerate() {
            let prev = self.prev_pos.get(i).copied().unwrap_or(p.mv.pos);
            let d = p.mv.pos - prev;
            let moved = Vec3::new(d.x, 0.0, d.z).length();
            rig.sim.footsteps(
                &ctx,
                p.id,
                p.mv.pos,
                if moved < 3.0 { moved } else { 0.0 },
                p.mv.grounded && p.alive && !p.mv.sliding,
                &mut rig.engine,
            );
        }
        self.prev_pos = w.players.iter().map(|p| p.mv.pos).collect();
        Frame { ear, fwd: look(yaw, pitch), vel, inputs: in_game(dmg, alive) }
    }
}

// =============================================================================================
// archipelago_idle
// =============================================================================================

struct Idle {
    world: VoxelWorld,
    info: bd_voxel::ArenaInfo,
    start: Vec3,
    tl: Timeline,
    pos: Vec3,
    prev: Vec3,
    swing_from: Vec3,
    swing_to: Vec3,
}

impl Idle {
    fn new() -> Self {
        let (world, info) = bd_voxel::generate_arena(1);
        let start = info.spawns[0];
        let to_center = (info.center - start).with_y(0.0).normalize_or_zero();
        let boom = start + to_center * 30.0 + Vec3::Y * 1.0;
        let far = start + Vec3::new(70.0, 0.0, 40.0);
        let swing_from = start + to_center * 17.5;
        let swing_to = swing_from + Vec3::new(to_center.z, 0.0, -to_center.x) * 90.0;
        let mut items = vec![(13.0, GameEvent::Explosion { pos: boom, radius: 5.0, element: Element::Fire, source: None })];
        for k in 0..8 {
            let t = 32.0 + k as f32 * 0.5;
            items.push((
                t,
                hit_event(PlayerId(3), Some(PlayerId(4)), far + Vec3::Y * (k as f32 * 0.2), 0.5 + (k % 3) as f32 * 0.3, false, false),
            ));
        }
        items.push((20.0, GameEvent::Move { player: LOCAL, flags: bd_sim::events::move_flags::TETHER_FIRE, speed: 0.0 }));
        items.push((20.3, GameEvent::Move { player: LOCAL, flags: bd_sim::events::move_flags::TETHER_ATTACH, speed: 0.0 }));
        items.push((26.5, GameEvent::Move { player: LOCAL, flags: bd_sim::events::move_flags::TETHER_RELEASE, speed: 0.0 }));
        items.push((28.6, GameEvent::Move { player: LOCAL, flags: bd_sim::events::move_flags::LAND_HARD, speed: 20.0 }));
        Self { world, info, start, tl: Timeline::new(items), pos: start, prev: start, swing_from, swing_to }
    }
}

impl Scenario for Idle {
    fn map(&self) -> &'static str {
        "archipelago"
    }
    fn world(&self) -> &VoxelWorld {
        &self.world
    }
    fn scan(&self) -> EmitterMap {
        EmitterMap::scan(&self.world, self.info.bounds.min, self.info.bounds.max, None, 4.0)
    }
    fn frame(&mut self, t: f32, rig: &mut Rig) -> Frame {
        let to_center = (self.info.center - self.start).with_y(0.0).normalize_or_zero();
        let (feet, grounded) = if t < 6.0 {
            (self.start, true)
        } else if t < 11.0 {
            (self.start + to_center * (3.5 * (t - 6.0)), true)
        } else if t < 20.0 {
            (self.swing_from, true)
        } else if t < 28.6 {
            // Swing arc: up and across, speeding up to ~30 m/s.
            let u = ((t - 20.0) / 8.6).clamp(0.0, 1.0);
            let p = self.swing_from.lerp(self.swing_to, u * u);
            (p + Vec3::Y * (4.0 * u * (1.0 - u) * 50.0), false)
        } else {
            (self.swing_to, true)
        };
        let feet = if grounded { ground_below(&self.world, feet + Vec3::Y * 30.0).unwrap_or(feet) } else { feet };
        self.prev = self.pos;
        self.pos = feet;
        let vel = (self.pos - self.prev) / FRAME_DT;
        let ear = feet + Vec3::Y * 1.6;
        let yaw = 0.15 * t;
        let none = |_| None;
        let corps = |_| Some("willpower");
        let ctx = Rig::ctx(&self.world, ear, &none, &corps);
        let own = |_: PlayerId| Some(ear);
        let ctx_own = Rig::ctx(&self.world, ear, &own, &corps);
        for e in self.tl.due(t) {
            let c = if matches!(e, GameEvent::Move { .. }) { &ctx_own } else { &ctx };
            rig.event(&e, c);
        }
        let moved = (self.pos - self.prev).with_y(0.0).length();
        rig.sim.footsteps(&ctx, LOCAL, feet, if moved < 2.0 { moved } else { 0.0 }, grounded, &mut rig.engine);
        Frame { ear, fwd: look(yaw, 0.0), vel: if vel.is_finite() { vel } else { Vec3::ZERO }, inputs: in_game(20.0, true) }
    }
}

// =============================================================================================
// miami_street
// =============================================================================================

struct Street {
    world: VoxelWorld,
    info: bd_voxel::MapInfo,
    a: Vec3,
    b: Vec3,
    tl: Timeline,
    pos: Vec3,
    prev: Vec3,
}

impl Street {
    fn new() -> Self {
        let (world, info) = bd_voxel::generate_miami(1);
        let route = info.routes.iter().find(|r| r.name == "ocean_drive_bhop").cloned();
        let (a, b) = route
            .map(|r| (r.waypoints[0], r.waypoints[r.waypoints.len() - 1]))
            .unwrap_or((Vec3::new(54.0, 3.0, -92.0), Vec3::new(54.0, 3.0, 92.0)));
        let dir = (b - a).normalize_or_zero();
        let start = a + dir * 30.0;
        let at = |t: f32| start + dir * (4.0 * t);
        let mut items = Vec::new();
        // An enemy up the street shooting at you (threat), rounds smacking the asphalt near you.
        for k in 0..14 {
            let t = 12.0 + k as f32 * 0.45;
            let me = at(t) + Vec3::Y * 1.6;
            let enemy = me + dir * 34.0 + Vec3::new(3.0, 0.0, 0.0);
            let aim = (me - enemy).normalize_or_zero();
            items.push((t, shot(PlayerId(2), GunArchetype::HandCannon, enemy, aim)));
            let hit = at(t) + Vec3::new(if k % 2 == 0 { 1.5 } else { -2.0 }, 0.0, 2.0 + (k % 3) as f32);
            items.push((t + 0.02, trace(PlayerId(2), enemy, hit, Vec3::Y)));
        }
        let boom = at(21.0) + dir * 20.0 + Vec3::new(6.0, 0.0, 0.0);
        items.push((21.0, GameEvent::Explosion { pos: boom, radius: 4.5, element: Element::Fire, source: Some(PlayerId(2)) }));
        items.push((
            21.02,
            GameEvent::Carve { op: CarveOp { shape: CarveShape::Sphere { center: boom, radius: 3.0 }, power: 1.0, seed: 3 } },
        ));
        // You fire back: 6 shots, 4 hits, one crit, then the KO.
        for k in 0..6 {
            let t = 24.0 + k as f32 * 0.55;
            let me = at(t) + Vec3::Y * 1.6;
            let enemy = me + dir * 30.0;
            items.push((t, shot(LOCAL, GunArchetype::HandCannon, me, dir)));
            if k % 3 != 1 {
                items.push((t + 0.03, hit_event(PlayerId(2), Some(LOCAL), enemy, 0.7 + k as f32 * 0.1, k == 3, k == 5)));
            }
        }
        let ko_pos = at(27.3) + dir * 30.0;
        items.push((
            27.35,
            GameEvent::Ko {
                victim: PlayerId(2),
                killer: Some(LOCAL),
                cause: bd_sim::events::KoCause::BlastZone,
                victim_corps: bd_core::corps::Corps::Rage,
                streak: 2,
                double: false,
                void: false,
                shutdown: false,
                pos: ko_pos,
                vel: Vec3::ZERO,
                yaw: 0.0,
                zone: None,
                poof_delay: 0.0,
                killer_kos: 2,
                pct: 140.0,
                stocks_left: None,
            },
        ));
        Self { world, info, a: start, b, tl: Timeline::new(items), pos: start, prev: start }
    }
}

impl Scenario for Street {
    fn map(&self) -> &'static str {
        "miami"
    }
    fn world(&self) -> &VoxelWorld {
        &self.world
    }
    fn scan(&self) -> EmitterMap {
        let b = self.info.arena.bounds;
        EmitterMap::scan(&self.world, b.min, b.max, Some(self.info.water_y), 4.0)
    }
    fn frame(&mut self, t: f32, rig: &mut Rig) -> Frame {
        let dir = (self.b - self.a).normalize_or_zero();
        let feet = self.a + dir * (4.0 * t);
        let feet = ground_below(&self.world, feet + Vec3::Y * 4.0).unwrap_or(feet);
        self.prev = self.pos;
        self.pos = feet;
        let ear = feet + Vec3::Y * 1.6;
        let yaw = (-dir.x).atan2(-dir.z) + 0.5 * (t * 0.2).sin();
        let own = |_: PlayerId| Some(ear);
        let corps = |_: PlayerId| Some("willpower");
        let ctx = Rig::ctx(&self.world, ear, &own, &corps);
        for e in self.tl.due(t) {
            rig.event(&e, &ctx);
        }
        let moved = (self.pos - self.prev).with_y(0.0).length();
        rig.sim.footsteps(&ctx, LOCAL, feet, if moved < 2.0 { moved } else { 0.0 }, true, &mut rig.engine);
        Frame { ear, fwd: look(yaw, 0.0), vel: dir * 4.0, inputs: in_game(if t > 20.0 { 120.0 } else { 40.0 }, true) }
    }
}

// =============================================================================================
// interior: a sealed Miami room, fighting outside, then the wall is blown open
// =============================================================================================

struct Interior {
    world: VoxelWorld,
    info: bd_voxel::MapInfo,
    ear: Vec3,
    wall: Vec3,
    outside: Vec3,
    tl: Timeline,
    blown: bool,
}

/// Find an enclosed air pocket (a room) by probing the listener-space estimate on a grid.
fn find_room(w: &VoxelWorld, area: [f32; 4]) -> Option<(Vec3, Vec3)> {
    let mut best: Option<(f32, Vec3)> = None;
    let mut z = area[1] + 4.0;
    while z < area[3] - 4.0 {
        let mut x = area[0] + 4.0;
        while x < area[2] - 4.0 {
            for y in [8.0f32, 14.0, 20.0, 26.0, 32.0, 40.0] {
                let p = Vec3::new(x, y, z);
                if w.is_solid(bd_core::voxel::world_to_voxel(p)) || w.is_solid(bd_core::voxel::world_to_voxel(p - Vec3::Y)) {
                    continue;
                }
                let mut s = ListenerSpace::new(Default::default());
                s.step(w, p, 16);
                let e = s.estimate_now();
                if e.ceiling
                    && e.enclosure > 0.9
                    && e.mean_free_path > 3.0
                    && e.mean_free_path < 12.0
                    && best.is_none_or(|b| e.enclosure > b.0)
                {
                    best = Some((e.enclosure, p));
                }
            }
            x += 3.0;
        }
        z += 3.0;
    }
    let p = best?.1;
    // Nearest destructible wall horizontally (tower cores are indestructible).
    let mut wall = None;
    for i in 0..32 {
        let a = i as f32 / 32.0 * std::f32::consts::TAU;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        if let Some(h) = raycast(w, p, d, 30.0)
            && !bd_voxel::material::is_indestructible(w.material(h.voxel))
            && wall.is_none_or(|(t, _): (f32, Vec3)| h.t < t)
        {
            wall = Some((h.t, h.point));
        }
    }
    Some((p, wall?.1))
}

impl Interior {
    fn new() -> Self {
        let (world, info) = bd_voxel::generate_miami(1);
        let area = info.districts.iter().find(|d| d.name == "brickell").map(|d| d.area).unwrap_or([-96.0, -67.0, -36.0, 96.0]);
        let (ear, wall) = find_room(&world, area).unwrap_or((Vec3::new(-66.0, 10.0, 20.0), Vec3::new(-60.0, 10.0, 20.0)));
        let out_dir = (wall - ear).with_y(0.0).normalize_or_zero();
        let outside = wall + out_dir * 26.0;
        let mut items = Vec::new();
        for k in 0..12 {
            let t = 2.0 + k as f32 * 0.5;
            let muzzle = outside + Vec3::new(0.0, 0.0, (k % 3) as f32 * 3.0);
            items.push((t, shot(PlayerId(3), GunArchetype::HandCannon, muzzle, out_dir)));
        }
        items.push((9.0, GameEvent::Explosion { pos: outside + out_dir * 4.0, radius: 4.5, element: Element::Fire, source: None }));
        for k in 0..6 {
            let t = 11.0 + k as f32 * 0.6;
            items.push((t, shot(LOCAL, GunArchetype::HandCannon, ear, out_dir)));
            items.push((t + 0.01, trace(LOCAL, ear, wall, -out_dir)));
        }
        // t = 18: a rocket blows the wall open (the room literally opens up).
        items.push((
            18.0,
            GameEvent::Explosion { pos: wall + out_dir * 0.5, radius: 4.0, element: Element::Fire, source: Some(PlayerId(3)) },
        ));
        for k in 0..6 {
            let t = 22.0 + k as f32 * 0.6;
            items.push((t, shot(LOCAL, GunArchetype::HandCannon, ear, out_dir)));
            items.push((t + 0.4, shot(PlayerId(3), GunArchetype::HandCannon, outside, -out_dir)));
        }
        Self { world, info, ear, wall, outside, tl: Timeline::new(items), blown: false }
    }
}

impl Scenario for Interior {
    fn map(&self) -> &'static str {
        "miami"
    }
    fn world(&self) -> &VoxelWorld {
        &self.world
    }
    fn scan(&self) -> EmitterMap {
        let b = self.info.arena.bounds;
        EmitterMap::scan(&self.world, b.min, b.max, Some(self.info.water_y), 4.0)
    }
    fn frame(&mut self, t: f32, rig: &mut Rig) -> Frame {
        if t >= 18.0 && !self.blown {
            self.blown = true;
            let op = CarveOp { shape: CarveShape::Sphere { center: self.wall, radius: 6.0 }, power: 6.0, seed: 9 };
            let out = self.world.apply_carve(&op);
            eprintln!("interior: wall blown open ({} voxels)", out.removed.len());
        }
        let ear = self.ear;
        let fwd = (self.outside - ear).with_y(0.0).normalize_or_zero();
        let own = |_: PlayerId| Some(ear);
        let corps = |_: PlayerId| Some("willpower");
        let ctx = Rig::ctx(&self.world, ear, &own, &corps);
        for e in self.tl.due(t) {
            rig.event(&e, &ctx);
        }
        Frame { ear, fwd, vel: Vec3::ZERO, inputs: in_game(30.0, true) }
    }
}

// =============================================================================================
// stress: 200 looping voices for the CPU budget
// =============================================================================================

struct Stress {
    world: VoxelWorld,
    started: bool,
}

impl Scenario for Stress {
    fn map(&self) -> &'static str {
        "archipelago"
    }
    fn world(&self) -> &VoxelWorld {
        &self.world
    }
    fn scan(&self) -> EmitterMap {
        EmitterMap::default()
    }
    fn frame(&mut self, _t: f32, rig: &mut Rig) -> Frame {
        if !self.started {
            self.started = true;
            // Worst case for the CPU: no HDR culling, no per-sound limits → the full real-voice budget.
            let mut cfg = rig.engine.config().clone();
            cfg.hdr.enabled = false;
            let cues = CueTable {
                defaults: bd_audio::cue::CueSpec { max_instances: Some(64), hpf_hz: Some(120.0), hdr: Some(false), ..Default::default() },
                ..Default::default()
            };
            rig.engine.reload(cfg, Some(&cues));
            let names =
                ["gun_minigun_loop", "beam_loop", "amb_neon_hum", "boost_loop", "vehicle_engine_loop", "tether_reel_loop", "slide_loop"];
            let mut r = bd_audio::dsp::Rng::new(4);
            for i in 0..200 {
                let p = Vec3::new(r.range(-60.0, 60.0), r.range(0.0, 10.0), r.range(-60.0, 60.0));
                rig.engine.play(names[i % names.len()], PlayOpts::at(p).occlusion(r.range(0.0, 0.6)));
            }
        }
        Frame { ear: Vec3::new(0.0, 1.6, 0.0), fwd: Vec3::NEG_Z, vel: Vec3::ZERO, inputs: in_game(0.0, true) }
    }
}

// =============================================================================================
// AFTER: the real engine
// =============================================================================================

#[derive(Default)]
struct Capture {
    out: Vec<f32>,
    /// Mono stems per bus + world group + reverb.
    bus: Vec<Vec<f32>>,
    world: Vec<f32>,
    reverb: Vec<f32>,
    real: Vec<u16>,
    virt: Vec<u16>,
    hdr_top: Vec<f32>,
    hdr_gain: Vec<f32>,
    combat: Vec<f32>,
    block_us: Vec<f32>,
    game_us: Vec<f32>,
}

/// Listener track: (time, ear, forward) per frame.
type Track = Vec<(f64, Vec3, Vec3)>;

fn render_after(scen: &mut dyn Scenario, secs: f32, seed: u64) -> (Capture, Vec<Recorded>, Track, serde_json::Value) {
    let t_load = Instant::now();
    let a = AudioAssets::load(&assets()).expect("assets");
    for w in &a.warnings {
        eprintln!("warning: {w}");
    }
    let load_ms = t_load.elapsed().as_secs_f32() * 1000.0;
    let (mut engine, mut mixer) = AudioEngine::new(a.bank.clone(), &a.cues, a.mix.clone(), SR, seed);
    engine.set_recording(true);
    mixer.enable_stems();
    let map = a.maps.get(scen.map());
    let t_scan = Instant::now();
    let emitters = scen.scan();
    let scan_ms = t_scan.elapsed().as_secs_f32() * 1000.0;
    let zones: serde_json::Map<String, serde_json::Value> =
        emitters.zones.iter().map(|(z, v)| (format!("{z:?}"), json!(v.len()))).collect();
    let space = ListenerSpace::new(map.space);
    let amb = AmbienceDirector::new(map, emitters, seed);
    let mut rig = Rig { engine, director: MixDirector::new(), amb, space, sim: SimAudio::new(), cfg: a.mix.clone(), wind: None, events: 0 };
    // `--music`: the soundtrack on the Music bus (bd_music), driven like the client does.
    let mut music = if std::env::args().any(|x| x == "--music") {
        let lib = std::sync::Arc::new(bd_music::MusicLibrary::load_dir(&assets().join("music")).expect("music"));
        rig.engine.set_music(Box::new(bd_audio::music::ChipMusic::new(lib.clone(), SR)));
        Some(bd_audio::music::MusicDirector::new(lib, bd_audio::music::MusicStyle::Classic, seed))
    } else {
        None
    };
    let map_key = scen.map();
    let mut cap = Capture { bus: vec![Vec::new(); NUM_BUSES], ..Default::default() };
    let mut track = Vec::new();
    let frames = (secs / FRAME_DT) as usize;
    let mut block = [0.0f32; BLOCK * 2];
    let mut space_log = Vec::new();
    for fi in 0..frames {
        let t = fi as f32 * FRAME_DT;
        let g0 = Instant::now();
        rig.sim.begin_frame(FRAME_DT, &mut rig.engine);
        let f = scen.frame(t, &mut rig);
        rig.engine.set_listener(f.ear, f.fwd, Vec3::Y);
        track.push((rig.engine.time(), f.ear, f.fwd));
        rig.space.step(scen.world(), f.ear, 4);
        let rv = rig.space.reverb(FRAME_DT);
        rig.engine.set_reverb(rv);
        let est = rig.space.estimate_now();
        rig.amb.enclosure = est.enclosure;
        rig.amb.update(FRAME_DT, f.ear, f.vel, &mut rig.engine, Some(scen.world()));
        // Own speed wind (as the old client did, now on the Foley bus).
        let speed = f.vel.length();
        let k = ((speed - 6.0) / 30.0).clamp(0.0, 1.0);
        let wind_db = if k > 0.0 { 20.0 * (k * k * 1.2).log10() } else { -80.0 };
        match rig.wind {
            None => rig.wind = rig.engine.play("wind_loop", PlayOpts::own().gain_db(wind_db)),
            Some(h) => rig.engine.update(h, f.ear, wind_db, 0.8 + k * 0.6, 0.0),
        }
        rig.director.update(FRAME_DT, &f.inputs, &rig.cfg, &mut rig.engine);
        if let Some(m) = music.as_mut() {
            let mi = bd_audio::music::MusicInputs { scene: Scene::InGame, map: map_key, combat: rig.director.weights().combat, ..Default::default() };
            m.update(FRAME_DT, &mi, &mut rig.engine);
        }
        rig.engine.advance(FRAME_DT);
        cap.game_us.push((g0.elapsed().as_secs_f32() * 1e6 - scen.sim_us()).max(0.0));
        if fi % 30 == 0 {
            space_log.push(json!({"t": t, "enclosure": est.enclosure, "mfp": est.mean_free_path, "ceiling": est.ceiling, "slap_ms": rv.slap_ms, "wet_db": rv.wet_db, "rt60": rv.rt60, "fauna_scared": rig.amb.scared()}));
        }
        for _ in 0..FRAME_BLOCKS {
            let b0 = Instant::now();
            mixer.render_block_into(&mut block);
            cap.block_us.push(b0.elapsed().as_secs_f32() * 1e6);
            cap.out.extend_from_slice(&block);
            let st = mixer.stems().expect("stems");
            for b in 0..NUM_BUSES {
                cap.bus[b].extend(st.bus[b].chunks(2).map(|c| 0.5 * (c[0] + c[1])));
            }
            cap.world.extend(st.world_out.chunks(2).map(|c| 0.5 * (c[0] + c[1])));
            cap.reverb.extend(st.reverb.chunks(2).map(|c| 0.5 * (c[0] + c[1])));
            let tel = mixer.telemetry();
            cap.real.push(tel.real);
            cap.virt.push(tel.virt);
            cap.hdr_top.push(tel.hdr_top_db);
            cap.hdr_gain.push(tel.hdr_gain_db);
            cap.combat.push(rig.director.weights().combat);
        }
    }
    let rec = rig.engine.take_recording();
    let extra = json!({
        "load_ms": load_ms,
        "emitter_scan_ms": scan_ms,
        "emitter_zones": zones,
        "sim_events": rig.events,
        "plays_recorded": rec.iter().filter(|r| matches!(r, Recorded::Play{..})).count(),
        "engine_cmds_dropped": rig.engine.dropped,
        "cooldown_drops": rig.engine.cooled,
        "rejected_plays": mixer.telemetry().rejected,
        "stolen_voices": mixer.telemetry().stolen,
        "bank_mb": a.bank.bytes() as f32 / 1e6,
        "space_track": space_log,
    });
    (cap, rec, track, extra)
}

// =============================================================================================
// BEFORE: approximation of the previous client path
// =============================================================================================

const OLD_HALF_DIST: f32 = 22.0;
const OLD_MAX_DIST: f32 = 160.0;

struct OldVoice {
    sound: usize,
    var: usize,
    head: f64,
    rate: f64,
    gain: f32,
    pos: Option<Vec3>,
    looping: bool,
    handle: u32,
    bus: Bus,
    base: f32,
}

fn old_name(n: &str) -> Option<&str> {
    if n.starts_with("amb_")
        || matches!(n, "heartbeat" | "tinnitus" | "final_blow_suck" | "mix_whoosh_in" | "mix_whoosh_out" | "break_hardlight")
    {
        return None; // the old client had no ambience / danger / suck layers
    }
    Some(match n {
        "break_metal" => "break_stone",
        "break_glass" => "break_crystal",
        "bullet_impact_metal" | "bullet_impact_glass" | "bullet_impact_wood" | "bullet_impact_dirt" => "bullet_impact_stone",
        other => other,
    })
}

fn exempt(n: &str) -> bool {
    n.starts_with("ko_streak") || n.starts_with("ko_accent") || n == "ko" || n == "hit_crit"
}

fn render_before(rec: &[Recorded], track: &[(f64, Vec3, Vec3)], secs: f32, cues: &CueTable, new_bank: &SoundBank, seed: u64) -> Capture {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/audio/before_root");
    let mut warn = Vec::new();
    let bank = SoundBank::load_manifest(&root, &root.join("sfx/manifest.ron"), &mut warn)
        .expect("old bank: copy the old assets/sfx to target/audio/before_root/sfx");
    let mut rng = bd_audio::dsp::Rng::new(seed ^ 0x01d);
    let mut voices: Vec<OldVoice> = Vec::new();
    let mut cap = Capture { bus: vec![Vec::new(); NUM_BUSES], ..Default::default() };
    let frames = (secs / FRAME_DT) as usize;
    let mut ri = 0;
    let mut duck_until = -1.0f64;
    let mut budget = 0.0f32;
    let right_of = |fwd: Vec3| fwd.cross(Vec3::Y).normalize_or(Vec3::X);
    for fi in 0..frames {
        let (t, ear, fwd) = track.get(fi).copied().unwrap_or((fi as f64 * FRAME_DT as f64, Vec3::ZERO, Vec3::NEG_Z));
        let t_end = t + FRAME_DT as f64;
        budget = (budget + FRAME_DT * 28.0).min(14.0);
        // This frame's plays: old per-frame culling (one per name, loudest first, 4 world voices/frame).
        let mut picks: Vec<(usize, OldVoice, bool, f32)> = Vec::new();
        while ri < rec.len() {
            let rt = match rec[ri] {
                Recorded::Play { t, .. } | Recorded::Update { t, .. } | Recorded::Stop { t, .. } => t,
            };
            if rt >= t_end {
                break;
            }
            match rec[ri] {
                Recorded::Play { sound, opts, handle, .. } => {
                    let name = new_bank.get(sound).map(|s| s.name.as_str()).unwrap_or("");
                    if let Some(on) = old_name(name)
                        && let Some(id) = bank.id(on)
                        && let Some(s) = bank.get(id)
                        && !s.vars.is_empty()
                    {
                        let var = match opts.step {
                            Some(k) => (k.max(1) as usize - 1).min(s.vars.len() - 1),
                            None => rng.below(s.vars.len()),
                        };
                        let pitch = if s.sequence { 1.0 } else { rng.range(s.pitch.0, s.pitch.1) } * opts.pitch;
                        let mut gain = s.volume * bd_audio::dsp::db_to_lin(opts.gain_db.min(12.0));
                        let ex = exempt(name);
                        let pos = match opts.emit {
                            Emit::At(p) => Some(p),
                            _ => None,
                        };
                        if !ex && t < duck_until && pos.is_some() {
                            gain *= 0.28;
                        }
                        let mut keep = true;
                        if let Some(p) = pos {
                            let d = p.distance(ear);
                            if d > OLD_MAX_DIST {
                                keep = false;
                            }
                            gain *= 1.0 / (1.0 + d / OLD_HALF_DIST);
                            if gain < 0.04 && !ex {
                                keep = false;
                            }
                        }
                        if name.starts_with("ko_streak") && matches!(opts.emit, Emit::Ui) {
                            duck_until = t + 0.25;
                        }
                        let bus = cues.resolve(name, &new_bank.get(sound).map(|s| s.category.clone()).unwrap_or_default()).bus;
                        if keep {
                            let v = OldVoice {
                                sound: id.0 as usize,
                                var,
                                head: 0.0,
                                rate: pitch as f64,
                                gain,
                                pos,
                                looping: s.looping,
                                handle,
                                bus,
                                base: s.volume,
                            };
                            picks.push((id.0 as usize, v, ex || pos.is_none(), gain));
                        }
                    }
                }
                Recorded::Update { handle, pos, gain_db, pitch, .. } => {
                    if let Some(v) = voices.iter_mut().find(|v| v.handle == handle) {
                        v.gain = v.base * bd_audio::dsp::db_to_lin(gain_db.min(12.0));
                        if v.pos.is_some() {
                            v.pos = Some(pos);
                            v.gain /= 1.0 + pos.distance(ear) / OLD_HALF_DIST;
                        }
                        v.rate = pitch.clamp(0.25, 4.0) as f64;
                    }
                }
                Recorded::Stop { handle, .. } => voices.retain(|v| v.handle != handle),
            }
            ri += 1;
        }
        // Old Sfx3d culling applied to world (positional) voices; 2D PlaySfx paths were uncapped.
        picks.sort_by(|a, b| (b.2, b.3).partial_cmp(&(a.2, a.3)).unwrap_or(std::cmp::Ordering::Equal));
        let mut seen: Vec<usize> = Vec::new();
        let mut n_world = 0;
        for (sid, v, free, _) in picks {
            if !free {
                if seen.contains(&sid) || n_world >= 4 || budget < 1.0 {
                    continue;
                }
                seen.push(sid);
                n_world += 1;
                budget -= 1.0;
            }
            voices.push(v);
        }
        // Render the frame.
        let right = right_of(fwd);
        let (le, re) = (ear - right * 0.15, ear + right * 0.15);
        for _ in 0..FRAME_BLOCKS * BLOCK {
            let (mut l, mut r) = (0.0f32, 0.0f32);
            let mut bl = [0.0f32; NUM_BUSES];
            for v in voices.iter_mut() {
                let s = &bank.sounds[v.sound];
                let pcm = &s.vars[v.var];
                let i0 = v.head as usize;
                if i0 >= pcm.frames {
                    if v.looping {
                        v.head -= pcm.frames as f64;
                        continue;
                    }
                    v.gain = -1.0; // finished
                    continue;
                }
                let ch = pcm.channels as usize;
                let x = pcm.sample(i0 * ch);
                let (gl, gr) = match v.pos {
                    None => (v.gain, v.gain),
                    Some(p) => {
                        // rodio Spatial: inverse-square per ear (clamped at 1 m) + a ±0.25 inter-ear modifier.
                        let (dl, dr) = (p.distance(le), p.distance(re));
                        let md = 0.3f32;
                        let lm = (((dl - dr) / md + 1.0) / 4.0 + 0.5).min(1.0);
                        let rm = (((dr - dl) / md + 1.0) / 4.0 + 0.5).min(1.0);
                        (v.gain * lm * (1.0 / (dl * dl)).min(1.0), v.gain * rm * (1.0 / (dr * dr)).min(1.0))
                    }
                };
                l += x * gl;
                r += x * gr;
                bl[v.bus.index()] += 0.5 * x * (gl + gr);
                v.head += v.rate;
            }
            voices.retain(|v| v.gain >= 0.0);
            // The device clips.
            cap.out.push(l.clamp(-1.0, 1.0));
            cap.out.push(r.clamp(-1.0, 1.0));
            for (stem, v) in cap.bus.iter_mut().zip(bl) {
                stem.push(v);
            }
            let w: f32 = bl[..bd_audio::cue::WORLD_BUSES].iter().sum();
            cap.world.push(w);
        }
        for _ in 0..FRAME_BLOCKS {
            cap.real.push(voices.len() as u16);
        }
    }
    cap
}

// =============================================================================================
// Reports
// =============================================================================================

const BANDS: [(&str, f32, f32); 7] = [
    ("sub_20_90", 20.0, 90.0),
    ("low_90_300", 90.0, 300.0),
    ("lowmid_300_800", 300.0, 800.0),
    ("mid_800_3k", 800.0, 3000.0),
    ("himid_3k_5k", 3000.0, 5000.0),
    ("hi_5k_12k", 5000.0, 12000.0),
    ("air_12k_20k", 12000.0, 20000.0),
];

fn db(x: f64) -> f64 {
    10.0 * (x + 1e-20).log10()
}

fn pct(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s[((s.len() - 1) as f32 * p) as usize]
}

/// Feedback vs World in 5-12 kHz, only where feedback is sounding (100 ms windows).
fn carve_check(feedback: &[f32], world: &[f32]) -> serde_json::Value {
    let win = 4800;
    let mut fb_e = 0.0f64;
    let mut w_e = 0.0f64;
    let mut windows = 0;
    let mut ratios = Vec::new();
    let mut s = 0;
    while s + win <= feedback.len().min(world.len()) {
        let f = &feedback[s..s + win];
        let rms = (f.iter().map(|x| (x * x) as f64).sum::<f64>() / win as f64).sqrt();
        if rms > 1e-3 {
            let pad = |x: &[f32]| {
                let mut v = x.to_vec();
                v.resize(8192, 0.0);
                v
            };
            let ef = analysis::band_energies(&pad(f), SR, &[(5000.0, 12000.0)])[0];
            let ew = analysis::band_energies(&pad(&world[s..s + win]), SR, &[(5000.0, 12000.0)])[0];
            // Only windows where the cue actually has 5-12 kHz content (not the heartbeat).
            if db(ef) < -10.0 {
                s += win;
                continue;
            }
            fb_e += ef;
            w_e += ew;
            windows += 1;
            ratios.push((db(ef) - db(ew)) as f32);
        }
        s += win;
    }
    json!({
        "feedback_windows": windows,
        "feedback_share_5k_12k": if fb_e + w_e > 0.0 { fb_e / (fb_e + w_e) } else { 0.0 },
        "feedback_minus_world_db_median": pct(&ratios, 0.5),
        "feedback_minus_world_db_p10": pct(&ratios, 0.1),
    })
}

fn report(name: &str, mode: &str, cap: &Capture, extra: serde_json::Value, out_dir: &Path) -> serde_json::Value {
    let x = &cap.out;
    let lufs = analysis::integrated_lufs(x, 2, SR);
    let tp = analysis::true_peak_db(x, 2);
    let clipped = x.iter().filter(|v| v.abs() >= 0.9999).count();
    let st = analysis::short_term_series(x, 2, SR, 0.5);
    let st_valid: Vec<f32> = st.iter().copied().filter(|v| *v > -60.0).collect();
    let mut buses = serde_json::Map::new();
    let bands: Vec<(f32, f32)> = BANDS.iter().map(|b| (b.1, b.2)).collect();
    for b in 0..NUM_BUSES {
        let e = analysis::band_energies(&cap.bus[b], SR, &bands);
        let total: f64 = e.iter().sum();
        if total <= 1e-12 {
            continue;
        }
        let m: serde_json::Map<String, serde_json::Value> =
            BANDS.iter().zip(e.iter()).map(|(bd, v)| (bd.0.to_string(), json!(((db(*v) - db(total)) * 10.0).round() / 10.0))).collect();
        buses.insert(Bus::from_index(b).name().to_string(), json!({"total_db": (db(total) * 10.0).round() / 10.0, "band_share_db": m}));
    }
    let mono: Vec<f32> = x.chunks(2).map(|c| 0.5 * (c[0] + c[1])).collect();
    let me = analysis::band_energies(&mono, SR, &bands);
    let mt: f64 = me.iter().sum();
    let mix_bands: serde_json::Map<String, serde_json::Value> =
        BANDS.iter().zip(me.iter()).map(|(bd, v)| (bd.0.to_string(), json!(((db(*v) - db(mt)) * 10.0).round() / 10.0))).collect();
    let real: Vec<f32> = cap.real.iter().map(|&v| v as f32).collect();
    let virt: Vec<f32> = cap.virt.iter().map(|&v| v as f32).collect();
    let per10 = |v: &[f32]| v.iter().map(|x| x * (480.0 / BLOCK as f32)).collect::<Vec<f32>>();
    let blk = per10(&cap.block_us);
    let r = json!({
        "scenario": name,
        "mode": mode,
        "seconds": x.len() as f32 / 2.0 / SR as f32,
        "integrated_lufs": lufs,
        "true_peak_dbtp": tp,
        "clipped_samples": clipped,
        "short_term_lufs": {"max": pct(&st_valid, 1.0), "p95": pct(&st_valid, 0.95), "p50": pct(&st_valid, 0.5), "p10": pct(&st_valid, 0.1), "range_p95_p10": pct(&st_valid, 0.95) - pct(&st_valid, 0.1)},
        "mix_band_share_db": mix_bands,
        "buses": buses,
        "carve_check_5k_12k": carve_check(&cap.bus[Bus::Feedback.index()], &cap.world),
        "voices": {"real_max": pct(&real, 1.0), "real_mean": real.iter().sum::<f32>() / real.len().max(1) as f32, "virtual_max": pct(&virt, 1.0), "virtual_mean": virt.iter().sum::<f32>() / virt.len().max(1) as f32},
        "hdr": if cap.hdr_top.is_empty() { json!(null) } else { json!({"top_max_db": pct(&cap.hdr_top, 1.0), "gain_min_db": pct(&cap.hdr_gain, 0.0), "time_attenuated_3db_pct": 100.0 * cap.hdr_gain.iter().filter(|g| **g < -3.0).count() as f32 / cap.hdr_gain.len() as f32}) },
        "combat_weight_mean": if cap.combat.is_empty() { 0.0 } else { cap.combat.iter().sum::<f32>() / cap.combat.len() as f32 },
        "cpu_audio_thread_us_per_10ms": if blk.is_empty() { json!(null) } else { json!({"mean": blk.iter().sum::<f32>() / blk.len() as f32, "p99": pct(&blk, 0.99), "max": pct(&blk, 1.0), "realtime_pct_mean": blk.iter().sum::<f32>() / blk.len() as f32 / 100.0}) },
        "cpu_game_thread_us_per_frame": if cap.game_us.is_empty() { json!(null) } else { json!({"mean": cap.game_us.iter().sum::<f32>() / cap.game_us.len() as f32, "p99": pct(&cap.game_us, 0.99), "max": pct(&cap.game_us, 1.0)}) },
        "extra": extra,
    });
    let base = out_dir.join(format!("{name}_{mode}"));
    std::fs::write(base.with_extension("json"), serde_json::to_string_pretty(&r).unwrap()).unwrap();
    write_wav(&base.with_extension("wav"), x);
    write_png(&base.with_extension("png"), &mono, &cap.real, &cap.hdr_gain);
    r
}

fn write_wav(path: &Path, x: &[f32]) {
    let spec = hound::WavSpec { channels: 2, sample_rate: SR, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    let mut r = bd_audio::dsp::Rng::new(77);
    for &s in x {
        let d = (r.f32() - r.f32()) / 32768.0;
        w.write_sample(((s + d).clamp(-1.0, 1.0) * 32767.0).round() as i16).unwrap();
    }
    w.finalize().unwrap();
}

/// Log-frequency spectrogram (30 Hz-20 kHz) + a strip of real voices (green) and HDR gain (red).
fn write_png(path: &Path, mono: &[f32], real: &[u16], hdr: &[f32]) {
    let n_fft = 2048;
    let hop = 512;
    let spec = analysis::spectrogram(mono, n_fft, hop);
    let w = spec.len().clamp(1, 1600);
    let h_spec = 360;
    let h_strip = 60;
    let h = h_spec + h_strip;
    let mut img = vec![18u8; w * h * 3];
    let bin_hz = SR as f32 / n_fft as f32;
    // Normalise to the loudest 0.1 % of cells, 70 dB of range.
    let mut all: Vec<f32> = spec.iter().flat_map(|f| f.iter().step_by(8).copied()).collect();
    all.sort_by(f32::total_cmp);
    let top = all.get(all.len().saturating_sub(1 + all.len() / 1000)).copied().unwrap_or(0.0);
    let cmap = |v: f32| -> [u8; 3] {
        let t = ((v - (top - 70.0)) / 70.0).clamp(0.0, 1.0);
        let stops = [[0.0, 0.0, 4.0], [60.0, 15.0, 110.0], [180.0, 55.0, 120.0], [250.0, 140.0, 40.0], [252.0, 250.0, 180.0]];
        let x = t * 4.0;
        let i = (x as usize).min(3);
        let f = x - i as f32;
        let a = stops[i];
        let b = stops[i + 1];
        [(a[0] + (b[0] - a[0]) * f) as u8, (a[1] + (b[1] - a[1]) * f) as u8, (a[2] + (b[2] - a[2]) * f) as u8]
    };
    for px in 0..w {
        let f0 = px * spec.len() / w;
        let f1 = ((px + 1) * spec.len() / w).max(f0 + 1).min(spec.len());
        for py in 0..h_spec {
            let frac = 1.0 - py as f32 / (h_spec - 1) as f32;
            let hz = 30.0 * (20000.0f32 / 30.0).powf(frac);
            let k = ((hz / bin_hz) as usize).clamp(1, n_fft / 2 - 1);
            // Mean power over the frames in this pixel (max-pooling paints transients as a bed).
            let mut acc = 0.0f64;
            for fr in &spec[f0..f1] {
                acc += 10f64.powf(fr[k] as f64 / 10.0);
            }
            let v = (10.0 * (acc / (f1 - f0) as f64 + 1e-20).log10()) as f32;
            let c = cmap(v);
            let o = (py * w + px) * 3;
            img[o..o + 3].copy_from_slice(&c);
        }
    }
    // Strip: real voices (0..64) green bars, HDR gain (0..-15 dB) red line.
    let nb = real.len().max(1);
    for px in 0..w {
        let b = px * nb / w;
        let rv = real.get(b).copied().unwrap_or(0) as f32;
        let bar = ((rv / 64.0).min(1.0) * (h_strip - 4) as f32) as usize;
        for y in 0..bar {
            let o = ((h - 1 - y) * w + px) * 3;
            img[o..o + 3].copy_from_slice(&[40, 170, 90]);
        }
        if let Some(g) = hdr.get(b) {
            let y = ((-g / 15.0).clamp(0.0, 1.0) * (h_strip - 4) as f32) as usize;
            let o = ((h_spec + 2 + y) * w + px) * 3;
            img[o..o + 3].copy_from_slice(&[230, 60, 60]);
        }
    }
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&img).unwrap();
}

fn main() {
    let scenario: String = arg("--scenario", "brawl".to_string());
    let mode: String = arg("--mode", "both".to_string());
    let out_dir: PathBuf = PathBuf::from(arg("--out", "target/audio".to_string()));
    let seed: u64 = arg("--seed", 1);
    let default_secs = match scenario.as_str() {
        "brawl" => 45.0,
        "interior" => 30.0,
        "stress" => 10.0,
        _ => 40.0,
    };
    let secs: f32 = arg("--secs", default_secs);
    std::fs::create_dir_all(&out_dir).unwrap();
    let t0 = Instant::now();
    let mut scen: Box<dyn Scenario> = match scenario.as_str() {
        "brawl" => Box::new(Brawl::new(seed, 20.0)),
        "archipelago_idle" => Box::new(Idle::new()),
        "miami_street" => Box::new(Street::new()),
        "interior" => Box::new(Interior::new()),
        "stress" => Box::new(Stress { world: bd_voxel::generate_flat_test(96.0).0, started: false }),
        other => panic!("unknown scenario {other}"),
    };
    eprintln!("scenario {scenario} ready in {:.1}s", t0.elapsed().as_secs_f32());
    let (cap, rec, track, extra) = render_after(scen.as_mut(), secs, seed);
    let mut summary = HashMap::new();
    if mode != "before" {
        let label = if std::env::args().any(|x| x == "--music") { format!("{scenario}_music") } else { scenario.clone() };
        let r = report(&label, "after", &cap, extra, &out_dir);
        summary.insert("after", r);
    }
    if mode != "after" {
        let a = AudioAssets::load(&assets()).expect("assets");
        let old = render_before(&rec, &track, secs, &a.cues, &a.bank, seed);
        let r =
            report(&scenario, "before", &old, json!({"note": "approximation of the previous client path; see render_mix.rs"}), &out_dir);
        summary.insert("before", r);
    }
    for (m, r) in &summary {
        println!(
            "{scenario:>16} {m:>6}: {:6.1} LUFS  TP {:5.1} dBTP  clipped {:>6}  ST range {:4.1} LU  real voices max {:>3} mean {:5.1}  virtual max {:>3}  feedback 5-12k share {:.2}",
            r["integrated_lufs"].as_f64().unwrap_or(0.0),
            r["true_peak_dbtp"].as_f64().unwrap_or(0.0),
            r["clipped_samples"].as_u64().unwrap_or(0),
            r["short_term_lufs"]["range_p95_p10"].as_f64().unwrap_or(0.0),
            r["voices"]["real_max"].as_f64().unwrap_or(0.0),
            r["voices"]["real_mean"].as_f64().unwrap_or(0.0),
            r["voices"]["virtual_max"].as_f64().unwrap_or(0.0),
            r["carve_check_5k_12k"]["feedback_share_5k_12k"].as_f64().unwrap_or(0.0),
        );
        if let Some(c) = r.get("cpu_audio_thread_us_per_10ms").filter(|c| !c.is_null()) {
            println!(
                "                  audio thread per 10 ms: mean {:.0} us, p99 {:.0} us, max {:.0} us",
                c["mean"].as_f64().unwrap_or(0.0),
                c["p99"].as_f64().unwrap_or(0.0),
                c["max"].as_f64().unwrap_or(0.0)
            );
        }
        if let Some(c) = r.get("cpu_game_thread_us_per_frame").filter(|c| !c.is_null()) {
            println!(
                "                  game thread per frame: mean {:.0} us, p99 {:.0} us, max {:.0} us",
                c["mean"].as_f64().unwrap_or(0.0),
                c["p99"].as_f64().unwrap_or(0.0),
                c["max"].as_f64().unwrap_or(0.0)
            );
        }
    }
    eprintln!("done in {:.1}s -> {}", t0.elapsed().as_secs_f32(), out_dir.display());
}
