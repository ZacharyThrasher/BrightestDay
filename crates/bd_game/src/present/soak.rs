//! `--soak-view --seed N`: bd_sim's `ChaosWorld` (bots casting random recipes from the whole
//! combination space + world-level chaos) rendered live, exactly as the headless soak runs it (same
//! seed ⇒ same world, including the arena seed), with
//! * an **auto-director** camera: every event is scored by drama (KOs, explosions, clashes, big
//!   hits, casts…) near each player; the camera cuts to the busiest player (min shot length,
//!   hysteresis) and follows them from a slow-orbiting three-quarter view;
//! * a tiny overlay: the last random casts' descriptions, tick time (last / p99 of the last
//!   second), invariant checks passed; on an invariant failure the world **freezes** and the seed,
//!   tick and message are shown.
//!
//! The brawl benchmark scenario (`--scenario brawl`) reuses the director camera.

use super::camera3p::ViewMode;
use super::{bv, PresentSet};
use crate::camera::{CameraUpdateSet, PlayerCamera};
use crate::practice::SimArgs;
use crate::sim_bridge::{ActiveWorld, GameEvents, SimRole, ViewKeys, WorldViews};
use crate::state::AppState;
use crate::tick::{SimSet, SimTick};
use bd_core::PlayerId;
use bd_sim::fuzz::{ChaosConfig, ChaosWorld};
use bd_sim::{ArenaSource, GameEvent, MatchRules, World};
use bevy::picking::Pickable;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
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

/// Director state.
#[derive(Resource, Default)]
pub struct Director {
    pub enabled: bool,
    target: Option<PlayerId>,
    shot_t: f32,
    orbit: f32,
    pos: Vec3,
    look_at: Vec3,
    heat: HashMap<PlayerId, f32>,
    cut: bool,
}

pub struct SoakPlugin;

impl Plugin for SoakPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Director>()
            .add_systems(OnEnter(AppState::InGame), start_soak.after(crate::player::SpawnPlayerSet))
            .add_systems(SimTick, soak_tick.in_set(SimSet::Simulate).run_if(resource_exists::<SoakDriver>))
            .add_systems(Update, (director_heat, soak_overlay.run_if(resource_exists::<SoakDriver>)).in_set(PresentSet::Sync))
            .add_systems(PostUpdate, place_director.after(CameraUpdateSet).before(TransformSystems::Propagate));
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
    ));
    info!("soak viewer: seed {seed}");
}

fn soak_tick(mut driver: ResMut<SoakDriver>, active: Option<ResMut<ActiveWorld>>, mut keys: ResMut<ViewKeys>) {
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
    for (p, desc) in report.casts {
        if d.casts.len() >= 6 {
            d.casts.pop_front();
        }
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

fn soak_overlay(driver: Res<SoakDriver>, mut q: Query<(&mut Text, &mut TextColor), With<SoakOverlay>>) {
    let Ok((mut t, mut c)) = q.single_mut() else { return };
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

/// Score drama near each player from this frame's events.
fn director_heat(time: Res<Time<Real>>, mut director: ResMut<Director>, mut events: MessageReader<GameEvents>, views: Res<WorldViews>) {
    if !director.enabled {
        events.clear();
        return;
    }
    let dt = time.delta_secs();
    for h in director.heat.values_mut() {
        *h *= (-dt / 1.5).exp();
    }
    let add = |p: Vec3, w: f32, heat: &mut HashMap<PlayerId, f32>| {
        for v in &views.players {
            if !v.alive() {
                continue;
            }
            let d = v.pos.distance(p);
            if d < 14.0 {
                *heat.entry(v.id).or_insert(0.0) += w * (1.0 - d / 14.0);
            }
        }
    };
    let mut heat = std::mem::take(&mut director.heat);
    for ev in events.read() {
        match &ev.event {
            GameEvent::Ko { pos, .. } => add(bv(*pos), 6.0, &mut heat),
            GameEvent::Explosion { pos, radius, .. } => add(bv(*pos), 1.5 + radius * 0.5, &mut heat),
            GameEvent::Clash { point, strength, .. } => add(bv(*point), 1.5 + strength, &mut heat),
            GameEvent::Hit { point, impact, .. } => add(bv(*point), 0.5 + impact.strength * 1.5, &mut heat),
            GameEvent::ConstructSpawned { pos, .. } => add(bv(*pos), 0.8, &mut heat),
            GameEvent::ConstructShattered { pos, .. } => add(bv(*pos), 1.2, &mut heat),
            GameEvent::WallSplat { point, .. } => add(bv(*point), 2.0, &mut heat),
            GameEvent::CreatureAttack { impact, id, .. } if *impact > 0.0 => {
                if let Some(c) = views.construct(*id) {
                    add(c.pos, 1.0 + impact, &mut heat);
                }
            }
            _ => {}
        }
    }
    director.heat = heat;
    director.shot_t += dt;
    let best = director.heat.iter().filter(|(id, _)| views.player(**id).is_some_and(|v| v.alive())).max_by(|a, b| a.1.total_cmp(b.1)).map(|(id, h)| (*id, *h));
    let current_heat = director.target.and_then(|t| director.heat.get(&t).copied()).unwrap_or(0.0);
    let target_alive = director.target.and_then(|t| views.player(t)).is_some_and(|v| v.alive());
    let want_cut = match best {
        Some((id, h)) => director.target != Some(id) && ((director.shot_t > 3.0 && h > current_heat * 1.6 + 0.5) || !target_alive && director.shot_t > 0.8),
        None => !target_alive && director.shot_t > 1.0,
    };
    if want_cut || director.target.is_none() {
        let pick = best.map(|b| b.0).or_else(|| views.players.iter().find(|v| v.alive()).map(|v| v.id));
        if pick.is_some() && pick != director.target {
            director.target = pick;
            director.shot_t = 0.0;
            director.cut = true;
            director.orbit += 1.9;
        }
    }
}

fn place_director(
    time: Res<Time<Real>>,
    mut director: ResMut<Director>,
    views: Res<WorldViews>,
    arena: Option<Res<crate::arena::ArenaWorld>>,
    mut cam: Query<&mut Transform, With<PlayerCamera>>,
) {
    if !director.enabled {
        return;
    }
    let Ok(mut tf) = cam.single_mut() else { return };
    let dt = time.delta_secs().min(0.1);
    director.orbit += dt * 0.12;
    let target = director.target.and_then(|t| views.player(t)).map(|v| v.pos + Vec3::Y * 1.2);
    let center = arena.as_ref().map(|a| a.info.center).unwrap_or(Vec3::ZERO);
    let focus = target.unwrap_or(center);
    let a = director.orbit;
    let dist = if target.is_some() { 13.0 } else { 55.0 };
    let want = focus + Vec3::new(a.cos() * dist, dist * 0.42, a.sin() * dist);
    if director.cut {
        director.cut = false;
        director.pos = want;
        director.look_at = focus;
    } else {
        let k = 1.0 - (-3.0 * dt).exp();
        director.pos = director.pos.lerp(want, k);
        director.look_at = director.look_at.lerp(focus, 1.0 - (-6.0 * dt).exp());
    }
    let pos = director.pos;
    let look = director.look_at;
    *tf = Transform::from_translation(pos).looking_at(look, Vec3::Y);
}
