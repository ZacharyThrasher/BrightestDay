//! Practice mode: a local authoritative `bd_sim::World` on the chosen map with N bots.
//!
//! * `OnEnter(AppState::InGame)` with `GameMode::Practice` on a sim map (Archipelago / Miami / Orbit — the
//!   tutorial keeps its standalone movement path) builds the world from the sim-side terrain copy
//!   the arena loader prepared ([`crate::arena::SimArenaCopy`]), adds the local player (id 0) + bots
//!   and inserts [`ActiveWorld`] (`SimRole::Offline`). The bridge ticks it at 120 Hz.
//! * Flags (parsed here so the shared `cli` stays untouched; `cli` prints "ignoring" for them):
//!   `--bots N` (default 5), `--difficulty 0..1` (default 0.5), `--scenario brawl` (8 bots, spectator
//!   camera over the centre, constructs everywhere; combine with `--bench N` for the perf gate),
//!   `--soak-view --seed N` (see `present::soak`), `--record <dir> --record-secs N [--record-size WxH]`.
//! * The match never really ends in practice: long clock, high KO target, auto-rematch.

use crate::arena::{CurrentMap, MapKind, SimArenaCopy};
use crate::sim_bridge::{ActiveWorld, SimRole};
use crate::state::{AfterLoading, AppState, GameMode};
use bd_core::corps::Corps;
use bd_core::PlayerId;
use bd_sim::{ArenaSource, MatchPhase, SimConfig, World};
use bevy::prelude::*;
use std::path::PathBuf;

/// SIM-owned command-line flags.
#[derive(Resource, Clone, Debug)]
pub struct SimArgs {
    pub bots: u8,
    pub difficulty: f32,
    pub scenario: Option<String>,
    pub soak_view: bool,
    pub seed: u64,
    pub record: Option<PathBuf>,
    pub record_secs: f32,
    pub record_size: Option<UVec2>,
    /// `--third-person`: force the over-the-shoulder camera (testing / recordings).
    pub third_person: bool,
}

impl Default for SimArgs {
    fn default() -> Self {
        Self { bots: 5, difficulty: 0.5, scenario: None, soak_view: false, seed: 1, record: None, record_secs: 20.0, record_size: None, third_person: false }
    }
}

impl SimArgs {
    pub fn parse_from(args: impl IntoIterator<Item = String>) -> Self {
        let mut out = SimArgs::default();
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--bots" => out.bots = it.next().and_then(|v| v.parse().ok()).unwrap_or(out.bots).min(7),
                "--difficulty" => out.difficulty = it.next().and_then(|v| v.parse().ok()).unwrap_or(out.difficulty),
                "--scenario" => out.scenario = it.next(),
                "--soak-view" => out.soak_view = true,
                "--seed" => out.seed = it.next().and_then(|v| v.parse().ok()).unwrap_or(1),
                "--record" => out.record = it.next().map(PathBuf::from),
                "--record-secs" => out.record_secs = it.next().and_then(|v| v.parse().ok()).unwrap_or(out.record_secs),
                "--record-size" => {
                    out.record_size = it.next().and_then(|v| {
                        let mut p = v.split(['x', 'X', ',']);
                        Some(UVec2::new(p.next()?.parse().ok()?, p.next()?.parse().ok()?))
                    })
                }
                "--third-person" => out.third_person = true,
                _ => {}
            }
        }
        if out.scenario.as_deref() == Some("brawl") {
            out.bots = 8;
        }
        out
    }

    pub fn brawl(&self) -> bool {
        self.scenario.as_deref() == Some("brawl")
    }

    /// `--scenario showcase` (`crate::showcase` builds its own world).
    pub fn showcase(&self) -> bool {
        self.scenario.as_deref() == Some("showcase")
    }
}

/// Practice lobby knobs (the UI may change them before entering).
#[derive(Resource, Clone, Debug)]
pub struct PracticeSettings {
    pub bots: u8,
    pub difficulty: f32,
}

/// Seconds after `MatchPhase::Ended` before practice rematches.
const REMATCH_SECS: f32 = 6.0;

pub struct PracticePlugin;

impl Plugin for PracticePlugin {
    fn build(&self, app: &mut App) {
        let args = SimArgs::parse_from(std::env::args().skip(1));
        if args.soak_view || args.brawl() {
            // Straight into the game (the shared `cli` only knows its own flags).
            app.insert_resource(AfterLoading(AppState::InGame));
        }
        app.insert_resource(PracticeSettings { bots: args.bots, difficulty: args.difficulty })
            .insert_resource(args)
            .add_systems(OnEnter(AppState::InGame), start_practice.after(crate::player::SpawnPlayerSet))
            .add_systems(Update, auto_rematch.run_if(resource_exists::<ActiveWorld>))
            .add_systems(Update, report_sim_perf);
    }
}

/// The sim config every practice world uses.
pub fn practice_config(seed: u64, arena_seed: u64) -> SimConfig {
    // `assets/tuning/combat.ron` (hot-reloaded into the running world by `combat_tuning`).
    let tuning = crate::combat_tuning::current();
    let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed: arena_seed }, seed, check_invariants: false, tuning, ..Default::default() };
    cfg.rules.ko_target = 50;
    cfg.rules.time_limit = 60.0 * 60.0;
    cfg.rules.regen_islands_between_rounds = false;
    cfg.timings.warmup_secs = 0.0;
    cfg
}

/// Build a world on the prepared terrain copy (or generate one synchronously as a fallback).
pub fn build_world(copy: &mut SimArenaCopy, cfg: SimConfig) -> World {
    match copy.take() {
        Some((voxels, info, seed)) => World::with_arena(cfg, voxels, info, seed),
        None => {
            warn!("practice: no prepared terrain copy; generating synchronously");
            World::new(cfg)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn start_practice(
    mut commands: Commands,
    mode: Res<GameMode>,
    map: Res<CurrentMap>,
    args: Res<SimArgs>,
    settings: Res<PracticeSettings>,
    mut copy: ResMut<SimArenaCopy>,
    existing: Option<Res<ActiveWorld>>,
    prefs: Option<Res<crate::lobby::prefs::UiPrefs>>,
    cli: Res<crate::cli::CliArgs>,
) {
    if *mode != GameMode::Practice || map.0 == MapKind::Tutorial || existing.is_some() || args.soak_view || args.showcase() {
        return;
    }
    let seed = args.seed;
    let mut cfg = practice_config(seed, crate::arena::ARENA_SEED);
    // The player's lobby rules (powers / party / scale / willpower toggles…).
    if let Some(p) = prefs.as_ref() {
        let timings = cfg.timings.clone();
        cfg = p.game.sim_config(cfg);
        cfg.timings = timings;
        cfg.check_invariants = false;
    }
    // Practice keeps its terrain on rematch (the render mirror regenerates only via ArenaReset,
    // and city maps can't be rebuilt from an Archipelago seed).
    cfg.rules.regen_islands_between_rounds = false;
    let mut world = build_world(&mut copy, cfg);
    let brawl = args.brawl();
    let local = if brawl { None } else { Some(PlayerId(0)) };
    if local.is_some() {
        let (name, corps) = prefs.as_ref().map_or(("you".to_string(), Corps::Willpower), |p| (p.name.clone(), p.corps));
        let _ = world.add_player(PlayerId(0), name, corps);
    }
    let first = if brawl { 0 } else { 1 };
    let n = if brawl { 8 } else { settings.bots.min(7) };
    let difficulty = if brawl { 0.8 } else { settings.difficulty };
    let bots = bd_sim::bots::add_bots(&mut world, first, n, difficulty, seed);
    world.start_live();
    // `--pos x,y,z` (feet): start the local player there (screenshots / feel checks of a spot).
    if let (Some(id), Some(p)) = (local, cli.pos)
        && let Some(pl) = world.player_mut(id)
    {
        pl.mv.pos = bd_core::Vec3::new(p.x, p.y, p.z);
        pl.mv.vel = bd_core::Vec3::ZERO;
    }
    let mut active = ActiveWorld::new(world, local, SimRole::Offline);
    active.bots = bots;
    info!("practice: {} bots (difficulty {:.2}), local {:?}", active.bots.len(), difficulty, local);
    commands.insert_resource(active);
}

fn auto_rematch(time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut ended_for: Local<f32>) {
    if active.role != SimRole::Offline {
        return;
    }
    if active.world.match_state.phase == MatchPhase::Ended {
        *ended_for += time.delta_secs();
        if *ended_for > REMATCH_SECS {
            *ended_for = 0.0;
            active.world.rematch();
        }
    } else {
        *ended_for = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sim_flags() {
        let a = SimArgs::parse_from("--soak-view --seed 42 --record out --record-secs 30 --record-size 1280x720".split(' ').map(String::from));
        assert!(a.soak_view);
        assert_eq!(a.seed, 42);
        assert_eq!(a.record_secs, 30.0);
        assert_eq!(a.record_size, Some(UVec2::new(1280, 720)));
        let b = SimArgs::parse_from("--scenario brawl".split(' ').map(String::from));
        assert!(b.brawl());
        assert_eq!(b.bots, 8);
    }
}

/// Every 5 s with `--bench` / a scenario / `--soak-view`: print the authoritative tick cost.
fn report_sim_perf(time: Res<Time<Real>>, args: Res<SimArgs>, cli: Res<crate::cli::CliArgs>, timing: Res<crate::sim_bridge::SimTiming>, mut next: Local<f32>) {
    if !(args.scenario.is_some() || args.soak_view || cli.bench.is_some()) {
        return;
    }
    let t = time.elapsed_secs();
    if t < *next {
        return;
    }
    *next = t + 5.0;
    let (avg, p99, max) = timing.stats();
    println!("SIMPERF ticks={} avg_ms={avg:.3} p99_ms={p99:.3} max_ms={max:.3}", timing.samples.len());
}
