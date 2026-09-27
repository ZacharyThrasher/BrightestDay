//! Practice mode: a local authoritative `bd_sim::World` on the chosen map with N bots.
//!
//! * `OnEnter(AppState::InGame)` with `GameMode::Practice` on a sim map (Archipelago / Miami / Orbit — the
//!   tutorial keeps its standalone movement path) builds the world from the sim-side terrain copy
//!   the arena loader prepared ([`crate::arena::SimArenaCopy`]), adds the local player (id 0) + bots
//!   and inserts [`ActiveWorld`] (`SimRole::Offline`). The bridge ticks it at 120 Hz.
//! * Flags (parsed here so the shared `cli` stays untouched; `cli` prints "ignoring" for them):
//!   `--bots N` (default 5), `--difficulty 0..1` (default 0.5), `--scenario brawl` (8 bots, spectator
//!   camera over the centre, constructs everywhere; combine with `--bench N` for the perf gate),
//!   `--soak-view --seed N` (see `present::soak`), `--record <dir> --record-secs N [--record-size WxH]`,
//!   `--dummy` (spawn the TRAINING FIELD practice dummy on any practice map, not just Training Field).
//! * The match never really ends in practice: long clock, high KO target, auto-rematch.
//!
//! ## TRAINING FIELD dummy
//! On [`MapKind::Training`] (or any practice map with `--dummy`) [`start_practice`] adds one extra
//! player a few metres in front of the local player and flags it a dummy (`World::set_dummy`): no
//! [`bd_sim::bots::BotBrain`] drives it (no AI at all — [`drive_dummy_face`] just keeps its `input.yaw`
//! pointed at the local player every tick, movement axes and buttons always zero), so it never moves
//! or attacks, but every hit/launch code path still runs on it exactly as on a real player. F6 toggles
//! LAUNCH / STATIONARY (also a Pause-menu row, [`DummyToggleRequest`]); F7 resets it; F8 cycles a
//! damage % preset. [`DummyReadout`] shows the last hit's numbers in the corner (dev-only HUD).

use crate::arena::{CurrentMap, MapKind, SimArenaCopy};
use crate::input::UiFocus;
use crate::sim_bridge::{ActiveWorld, GameEvents, SimRole};
use crate::state::{AfterLoading, AppState, GameMode};
use crate::tick::SimSet;
use bd_core::corps::Corps;
use bd_core::PlayerId;
use bd_sim::{ArenaSource, GameEvent, MatchPhase, SimConfig, World};
use bevy::picking::Pickable;
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
    /// `--dummy`: spawn the TRAINING FIELD practice dummy even off Training Field (always on there).
    pub dummy: bool,
    /// `--bots N` was given explicitly (Training Field defaults to no bots otherwise).
    pub bots_explicit: bool,
}

impl Default for SimArgs {
    fn default() -> Self {
        Self { bots: 5, difficulty: 0.5, scenario: None, soak_view: false, seed: 1, record: None, record_secs: 20.0, record_size: None, third_person: false, dummy: false, bots_explicit: false }
    }
}

impl SimArgs {
    pub fn parse_from(args: impl IntoIterator<Item = String>) -> Self {
        let mut out = SimArgs::default();
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--bots" => {
                    out.bots = it.next().and_then(|v| v.parse().ok()).unwrap_or(out.bots).min(7);
                    out.bots_explicit = true;
                }
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
                "--dummy" => out.dummy = true,
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

/// TRAINING FIELD: present while this practice match has a dummy (`start_practice`; removed on
/// `OnExit(AppState::InGame)`).
#[derive(Resource, Clone, Copy, Debug)]
pub struct PracticeDummy {
    pub id: PlayerId,
}

/// The Pause menu's "TOGGLE DUMMY MODE" row (menu.rs) fired this.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct DummyToggleRequest;

/// TRAINING FIELD dev readout (corner HUD): the dummy's last hit + a rolling combo count. Dev-only
/// numbers — not the competitive HUD.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct DummyReadout {
    pub last_damage: f32,
    pub last_knockback: f32,
    pub combo: u32,
    /// Real seconds left before the next hit starts a fresh combo instead of extending this one.
    combo_window: f32,
}

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
            .add_message::<DummyToggleRequest>()
            .init_resource::<DummyReadout>()
            .add_systems(OnEnter(AppState::InGame), start_practice.after(crate::player::SpawnPlayerSet))
            .add_systems(OnExit(AppState::InGame), |mut c: Commands| c.remove_resource::<PracticeDummy>())
            .add_systems(Update, auto_rematch.run_if(resource_exists::<ActiveWorld>))
            .add_systems(Update, report_sim_perf)
            .add_systems(
                Update,
                (drive_dummy_face.in_set(SimSet::Input), dummy_controls, dummy_readout_events).run_if(resource_exists::<PracticeDummy>),
            )
            .add_systems(Startup, spawn_dummy_readout_ui)
            .add_systems(Update, update_dummy_readout_ui);
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
    // WILLPOWER = LAUNCH dev / smoke override (A/B without touching lobby prefs).
    if std::env::var_os("BD_WILLPOWER_LAUNCH").is_some() {
        cfg.rules.willpower_launch = true;
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
    // TRAINING FIELD is a quiet test range: just you and the dummy unless `--bots N` asks for company.
    let n = if brawl {
        8
    } else if map.0 == MapKind::Training && !args.bots_explicit {
        0
    } else {
        settings.bots.min(7)
    };
    let difficulty = if brawl { 0.8 } else { settings.difficulty };
    let bots = bd_sim::bots::add_bots(&mut world, first, n, difficulty, seed);
    // TRAINING FIELD: one dummy, no BotBrain (`bots::drive` never sees it, so nothing ever drives it
    // — see the module docs). Training Field always gets one; any other practice map only with
    // `--dummy`. Never in `--scenario brawl` (no local player to face).
    let want_dummy = !brawl && local.is_some() && (map.0 == MapKind::Training || args.dummy);
    let dummy_id = want_dummy.then(|| PlayerId(first.saturating_add(n)));
    if let Some(id) = dummy_id {
        let _ = world.add_player(id, "dummy".to_string(), Corps::from_index(id.0));
    }
    world.start_live();
    // `--pos x,y,z` (feet): start the local player there (screenshots / feel checks of a spot).
    if let (Some(id), Some(p)) = (local, cli.pos)
        && let Some(pl) = world.player_mut(id)
    {
        pl.mv.pos = bd_core::Vec3::new(p.x, p.y, p.z);
        pl.mv.vel = bd_core::Vec3::ZERO;
    }
    // TRAINING FIELD: place the dummy a few metres in front of wherever the local player spawned,
    // facing back at them, and anchor it there (`World::set_dummy`: KOs / resets return it to this
    // exact spot). Runs after `start_live()` so `local`'s spawn position is final.
    if let (Some(id), Some(local_id)) = (dummy_id, local)
        && let Some(lp) = world.player(local_id)
    {
        let local_pos = lp.mv.pos;
        let fwd = lp.input.look_dir();
        let pos = local_pos + bd_core::Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(bd_core::Vec3::NEG_Z) * 3.5;
        let stationary = prefs.as_ref().is_none_or(|p| p.dummy_stationary);
        if let Some(dp) = world.player_mut(id) {
            dp.mv.pos = pos;
            dp.mv.vel = bd_core::Vec3::ZERO;
            let to = local_pos - pos;
            dp.input.yaw = (-to.x).atan2(-to.z); // face the local player immediately
        }
        world.set_dummy(id, stationary);
        commands.insert_resource(PracticeDummy { id });
    }
    let mut active = ActiveWorld::new(world, local, SimRole::Offline);
    active.bots = bots;
    info!("practice: {} bots (difficulty {:.2}), local {:?}, dummy {:?}", active.bots.len(), difficulty, local, dummy_id);
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

// ------------------------------------------------------------------------------------ TRAINING FIELD dummy

/// SimSet::Input, before the bridge ticks: no AI, just keep the dummy turned to face the local
/// player (movement axes / buttons always zero — see the module docs).
fn drive_dummy_face(dummy: Res<PracticeDummy>, active: Option<ResMut<ActiveWorld>>) {
    let Some(mut active) = active else { return };
    let Some(local_id) = active.local else { return };
    let Some(target) = active.world.player(local_id).map(|p| p.mv.pos) else { return };
    let Some(dp) = active.world.player_mut(dummy.id) else { return };
    let to = target - dp.mv.pos;
    if to.x * to.x + to.z * to.z > 1e-6 {
        dp.input.yaw = (-to.x).atan2(-to.z);
    }
    dp.input.move_x = 0;
    dp.input.move_z = 0;
    dp.input.buttons = 0;
}

/// F6 toggle LAUNCH / STATIONARY (mirrored by the Pause menu's [`DummyToggleRequest`]), F7 reset,
/// F8 cycle a damage % preset. Never while typing (the ring console / oath).
fn dummy_controls(
    dummy: Res<PracticeDummy>,
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<UiFocus>,
    mut toggles: MessageReader<DummyToggleRequest>,
    active: Option<ResMut<ActiveWorld>>,
    prefs: Option<ResMut<crate::lobby::prefs::UiPrefs>>,
) {
    let Some(mut active) = active else {
        toggles.clear();
        return;
    };
    let menu_toggle = toggles.read().count() > 0;
    if menu_toggle || (!focus.typing && keys.just_pressed(KeyCode::F6)) {
        let stationary_now = active.world.player(dummy.id).and_then(|p| p.dummy).is_none_or(|d| d.stationary);
        let stationary = !stationary_now;
        active.world.set_dummy_stationary(dummy.id, stationary);
        if let Some(mut p) = prefs {
            p.dummy_stationary = stationary;
        }
    }
    if !focus.typing && keys.just_pressed(KeyCode::F7) {
        active.world.reset_dummy(dummy.id);
    }
    if !focus.typing && keys.just_pressed(KeyCode::F8)
        && let Some(p) = active.world.player_mut(dummy.id)
    {
        // 0 % → 50 % → 100 % → 150 % → 0 % …
        p.damage_pct = match p.damage_pct as i32 {
            ..25 => 50.0,
            25..75 => 100.0,
            75..125 => 150.0,
            _ => 0.0,
        };
    }
}

/// Tracks the dummy's last `Hit` (damage / knockback) and a rolling combo count for the dev readout.
fn dummy_readout_events(dummy: Res<PracticeDummy>, mut events: MessageReader<GameEvents>, mut readout: ResMut<DummyReadout>, time: Res<Time<Real>>) {
    const COMBO_WINDOW: f32 = 1.2;
    readout.combo_window = (readout.combo_window - time.delta_secs()).max(0.0);
    for ev in events.read() {
        if let GameEvent::Hit { target, damage, knockback, .. } = &ev.event
            && *target == dummy.id
        {
            readout.last_damage = *damage;
            readout.last_knockback = *knockback;
            readout.combo = if readout.combo_window > 0.0 { readout.combo + 1 } else { 1 };
            readout.combo_window = COMBO_WINDOW;
        }
    }
}

#[derive(Component)]
struct DummyReadoutText;

/// Dev-only corner readout (hidden unless [`PracticeDummy`] is present) — not the competitive HUD.
fn spawn_dummy_readout_ui(mut commands: Commands) {
    commands.spawn((
        Name::new("DummyReadout"),
        DummyReadoutText,
        Text::new(""),
        TextFont::from_font_size(13.0),
        TextColor(Color::srgb(1.0, 0.85, 0.35)),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, right: px(10), top: px(8), ..default() },
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(60),
    ));
}

fn update_dummy_readout_ui(
    dummy: Option<Res<PracticeDummy>>,
    readout: Res<DummyReadout>,
    active: Option<Res<ActiveWorld>>,
    mut text: Query<(&mut Text, &mut Visibility), With<DummyReadoutText>>,
) {
    let Ok((mut text, mut vis)) = text.single_mut() else { return };
    let Some(dummy) = dummy else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Visible;
    let p = active.as_ref().and_then(|a| a.world.player(dummy.id));
    let stationary = p.and_then(|p| p.dummy).is_none_or(|d| d.stationary);
    let mode = if stationary { "STATIONARY" } else { "LAUNCH" };
    let pct = p.map_or(0.0, |p| p.damage_pct);
    let combo = if readout.combo == 0 { "—".to_string() } else { format!("x{}", readout.combo) };
    text.0 = format!(
        "DUMMY [{mode}] {pct:.0}%\nlast hit {:.1}% / {:.1} kb  combo {combo}\nF6 toggle mode · F7 reset · F8 cycle %",
        readout.last_damage, readout.last_knockback
    );
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
