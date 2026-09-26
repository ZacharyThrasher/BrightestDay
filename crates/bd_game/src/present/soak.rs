//! `--soak-view --seed N`: bd_sim's `ChaosWorld` (bots casting random recipes from the whole
//! combination space + world-level chaos) rendered live, exactly as the headless soak runs it (same
//! seed ⇒ same world, including the arena seed), with
//! * the **director** camera ([`super::director`]): interest-scored subjects, occlusion-safe
//!   3/4 framing, cuts on big events;
//! * with `--overlay`, a tiny overlay: the last random casts' descriptions, tick time (last / p99
//!   of the last second), invariant checks passed. On an invariant failure the world **freezes**
//!   and the seed, tick and message are shown (the overlay appears then regardless).
//!
//! The brawl benchmark scenario (`--scenario brawl`) reuses the director camera.

use super::camera3p::ViewMode;
use super::director::Director;
use super::PresentSet;
use crate::practice::SimArgs;
use crate::sim_bridge::{ActiveWorld, SimRole, ViewKeys};
use crate::state::AppState;
use crate::tick::{SimSet, SimTick};
use bd_sim::fuzz::{ChaosConfig, ChaosWorld};
use bd_sim::{ArenaSource, GameEvent, MatchRules, World};
use bevy::picking::Pickable;
use bevy::prelude::*;
use std::collections::VecDeque;

/// The chaos driver (its `world` field is a placeholder while `ActiveWorld` holds the real one).
#[derive(Resource)]
struct SoakDriver {
    chaos: ChaosWorld,
    seed: u64,
    ticks: u64,
    checks: u64,
    tick_us: VecDeque<u32>,
    casts: VecDeque<String>,
    failure: Option<String>,
}

#[derive(Component)]
struct SoakOverlay;

/// `--overlay`: show the soak text overlay.
fn overlay_requested() -> bool {
    std::env::args().any(|a| a == "--overlay")
}

pub struct SoakPlugin;

impl Plugin for SoakPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), start_soak.after(crate::player::SpawnPlayerSet))
            .add_systems(SimTick, soak_tick.in_set(SimSet::Simulate).run_if(resource_exists::<SoakDriver>))
            .add_systems(Update, soak_overlay.run_if(resource_exists::<SoakDriver>).in_set(PresentSet::Sync));
    }
}

fn start_soak(mut commands: Commands, args: Res<SimArgs>, mut director: ResMut<Director>, mut mode: ResMut<ViewMode>) {
    if args.brawl() {
        director.enabled = true;
        mode.director = true;
    }
    if !args.soak_view {
        return;
    }
    let seed = args.seed;
    let mut cfg = ChaosConfig::new(seed, MatchRules::default());
    cfg.arena = ArenaSource::Archipelago { seed };
    let mut chaos = ChaosWorld::with_config(cfg);
    // Move the real world into ActiveWorld; leave a tiny placeholder in the driver.
    let placeholder = World::new(bd_sim::SimConfig { arena: ArenaSource::Flat { size_m: 32.0 }, check_invariants: false, ..Default::default() });
    let world = std::mem::replace(&mut chaos.world, placeholder);
    let mut active = ActiveWorld::new(world, None, SimRole::Offline);
    // The bridge must not tick it: the chaos driver does.
    active.paused = true;
    // Rebuild the render mirror for this seed's arena.
    if seed != crate::arena::ARENA_SEED {
        active.push_event(GameEvent::ArenaReset { seed });
    }
    commands.insert_resource(active);
    commands.insert_resource(SoakDriver { chaos, seed, ticks: 0, checks: 0, tick_us: VecDeque::with_capacity(128), casts: VecDeque::with_capacity(8), failure: None });
    director.enabled = true;
    mode.director = true;
    commands.spawn((
        SoakOverlay,
        Text::new(""),
        TextFont::from_font_size(13.0),
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, left: px(12), bottom: px(10), max_width: px(620), ..default() },
        Pickable::IGNORE,
        GlobalZIndex(50),
        if overlay_requested() { Visibility::Inherited } else { Visibility::Hidden },
    ));
    info!("soak viewer: seed {seed}");
}

fn soak_tick(mut driver: ResMut<SoakDriver>, active: Option<ResMut<ActiveWorld>>, mut keys: ResMut<ViewKeys>, mut timing: ResMut<crate::sim_bridge::SimTiming>) {
    let Some(mut active) = active else { return };
    if driver.failure.is_some() {
        return;
    }
    let d = &mut *driver;
    std::mem::swap(&mut active.world, &mut d.chaos.world);
    let report = d.chaos.step();
    std::mem::swap(&mut active.world, &mut d.chaos.world);
    d.ticks += 1;
    d.checks += 1;
    if d.tick_us.len() >= 120 {
        d.tick_us.pop_front();
    }
    d.tick_us.push_back(report.tick_us);
    timing.record(report.tick_us as f32 / 1000.0);
    for (p, desc) in report.casts {
        if d.casts.len() >= 6 {
            d.casts.pop_front();
        }
        // The UI font is ASCII-only: '×' etc. would render as a box.
        let desc: String = desc.chars().map(|c| if c == '×' { 'x' } else if c.is_ascii() { c } else { '?' }).collect();
        d.casts.push_back(format!("{}: {desc}", p.0));
    }
    if let Some(e) = report.invariant_error {
        let msg = format!("INVARIANT FAILURE  seed {}  tick {}\n{e}", d.seed, report.tick);
        error!("{msg}");
        println!("SOAK FAILURE seed {} tick {}: {e}", d.seed, report.tick);
        d.failure = Some(msg);
        active.paused = true;
    }
    let evs: Vec<GameEvent> = active.world.events().to_vec();
    for e in evs {
        active.push_event(e);
    }
    let keys = &mut *keys;
    std::mem::swap(&mut keys.prev, &mut keys.curr);
    active.world.write_snapshot(None, &mut keys.curr);
    if !keys.valid {
        active.world.write_snapshot(None, &mut keys.prev);
        keys.valid = true;
    }
}

fn soak_overlay(driver: Res<SoakDriver>, mut q: Query<(&mut Text, &mut TextColor, &mut Visibility), With<SoakOverlay>>) {
    let Ok((mut t, mut c, mut vis)) = q.single_mut() else { return };
    if driver.failure.is_some() && *vis == Visibility::Hidden {
        *vis = Visibility::Inherited;
    }
    let last = driver.tick_us.back().copied().unwrap_or(0);
    let mut sorted: Vec<u32> = driver.tick_us.iter().copied().collect();
    sorted.sort_unstable();
    let p99 = sorted.get(((sorted.len() as f32 * 0.99) as usize).min(sorted.len().saturating_sub(1))).copied().unwrap_or(0);
    let mut s = String::with_capacity(512);
    use std::fmt::Write;
    for c in &driver.casts {
        let _ = writeln!(s, "{c}");
    }
    let _ = write!(s, "seed {}  tick {}  {:.2} ms (p99 {:.2})  invariants ok {}", driver.seed, driver.ticks, last as f32 / 1000.0, p99 as f32 / 1000.0, driver.checks);
    if let Some(f) = &driver.failure {
        let _ = write!(s, "\n{f}");
        c.0 = Color::srgb(1.0, 0.45, 0.4);
    }
    if t.0 != s {
        t.0 = s;
    }
}
