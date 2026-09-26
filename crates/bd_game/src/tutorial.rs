//! Movement tutorial course (`--map tutorial` or Title → TUTORIAL).
//!
//! * Keys **1–6** teleport to station N's start (standing, facing along the course); **7** = the
//!   peak-run start. **Hold R** (≈0.45 s, while not tethered) or fall into the void → back to the
//!   current station's start.
//! * **Peak run timer**: starts when the feet cross the start/finish ring heading +X, stops after
//!   every `peak_gates` checkpoint was passed in order (the last one is the ring again). Timed on
//!   the 120 Hz sim clock (per tick), shown against the medal times.
//! * Tiny speedometer (horizontal m/s) and a one-line hint per station.
//! * Ghost: the bot's recorded run for the current station (assets/tutorial/ghosts/*.ron) plays as a
//!   translucent capsule once you start moving.

use crate::arena::{ArenaWorld, MapKind};
use crate::assets_dir;
use crate::input::LookAngles;
use crate::player::{FramePositions, LocalPlayer, MoveTuning, PlayerRespawned, PlayerSim, RespawnPoint};
use crate::state::AppState;
use crate::tick::FrameSet;
use crate::util::{core_v3, v3};
use bd_core::DT;
use bd_sim::movement::PlayerState;
use bevy::picking::Pickable;
use bevy::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
struct GhostFile {
    station: String,
    #[allow(dead_code)]
    tick_hz: u32,
    #[allow(dead_code)]
    bot_time: f32,
    /// One entry per 120 Hz tick: (yaw, pitch, move_x, move_z, buttons, reel_notches).
    #[serde(default)]
    inputs: Vec<(f32, f32, i8, i8, u16, i8)>,
    /// Feet positions at 30 Hz from the spawn.
    positions: Vec<(f32, f32, f32)>,
}

/// Replay a bot ghost's recorded inputs on the local player (automation: `--script ghost:<name>`).
#[derive(Resource, Default, Debug)]
pub struct ReplayGhost(pub Option<String>);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Run {
    /// Waiting to cross the start ring.
    Armed,
    Running { start_tick: u64, next_gate: usize },
    Finished { time: f32 },
    /// Station practice (no timer).
    Off,
}

#[derive(Resource, Debug)]
struct TutorialState {
    /// 0..6 = stations 1–6, 6 = peak run.
    station: usize,
    run: Run,
    best: Option<f32>,
    r_hold: f32,
    hint_t: f32,
    /// Ghost clock (seconds since the player started moving), None = not started.
    ghost_t: Option<f32>,
    /// Sim ticks counted since the tutorial started (for the timer).
    ticks: u64,
}

impl Default for TutorialState {
    fn default() -> Self {
        Self { station: 6, run: Run::Armed, best: None, r_hold: 0.0, hint_t: 0.0, ghost_t: None, ticks: 0 }
    }
}

#[derive(Resource, Default)]
struct Ghosts(Vec<GhostFile>);

#[derive(Component)]
struct TutorialUi;
#[derive(Component)]
struct SpeedText;
#[derive(Component)]
struct TimerText;
#[derive(Component)]
struct HintText;
#[derive(Component)]
struct GhostBody;

pub struct TutorialPlugin;

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TutorialState>()
            .init_resource::<ReplayGhost>()
            .init_resource::<Ghosts>()
            .add_systems(Startup, load_ghosts)
            .add_systems(OnEnter(AppState::InGame), enter_tutorial.after(crate::player::SpawnPlayerSet))
            .add_systems(OnExit(AppState::InGame), exit_tutorial)
            .add_systems(
                Update,
                (station_keys, start_replay, peak_timer, update_ghost, update_ui)
                    .chain()
                    .run_if(is_tutorial)
                    .after(FrameSet::Sim)
                    .before(FrameSet::Present),
            );
    }
}

fn is_tutorial(state: Res<State<AppState>>, arena: Option<Res<ArenaWorld>>) -> bool {
    *state.get() == AppState::InGame && arena.is_some_and(|a| a.info.kind == MapKind::Tutorial)
}

fn load_ghosts(mut ghosts: ResMut<Ghosts>) {
    let dir = assets_dir().join("tutorial").join("ghosts");
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "ron")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            match ron::from_str::<GhostFile>(&text) {
                Ok(g) => ghosts.0.push(g),
                Err(err) => warn!("ghost {}: {err}", path.display()),
            }
        }
    }
    info!("tutorial: {} ghosts loaded", ghosts.0.len());
}

/// Station `i` (0..6) start, or the peak start for 6.
fn station_start(arena: &ArenaWorld, i: usize) -> Option<(Vec3, f32, &'static str, &'static str)> {
    let t = arena.info.tutorial.as_ref()?;
    if i < t.stations.len() {
        let s = &t.stations[i];
        Some((v3(s.start), s.facing_yaw, s.name, s.hint))
    } else {
        Some((v3(t.peak_start), t.peak_yaw, "peak_run", "Chain it all. Beat gold."))
    }
}

#[allow(clippy::too_many_arguments)]
fn teleport(
    i: usize,
    arena: &ArenaWorld,
    tuning: &MoveTuning,
    sim: &mut PlayerSim,
    look: &mut LookAngles,
    respawn: &mut RespawnPoint,
    tut: &mut TutorialState,
) {
    let Some((pos, yaw, _, _)) = station_start(arena, i) else { return };
    sim.state = PlayerState::new(core_v3(pos), &tuning.0);
    sim.prev = sim.state;
    look.yaw = yaw;
    look.pitch = -0.08;
    respawn.0 = Some((pos, yaw));
    tut.station = i;
    tut.run = if i == 6 { Run::Armed } else { Run::Off };
    tut.hint_t = 0.0;
    tut.ghost_t = None;
}

#[allow(clippy::too_many_arguments)]
fn enter_tutorial(
    mut commands: Commands,
    arena: Option<Res<ArenaWorld>>,
    tuning: Res<MoveTuning>,
    mut look: ResMut<LookAngles>,
    mut respawn: ResMut<RespawnPoint>,
    mut tut: ResMut<TutorialState>,
    mut players: Query<&mut PlayerSim, With<LocalPlayer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(arena) = arena.filter(|a| a.info.kind == MapKind::Tutorial) else { return };
    *tut = TutorialState::default();
    if let Ok(mut sim) = players.single_mut() {
        teleport(6, &arena, &tuning, &mut sim, &mut look, &mut respawn, &mut tut);
    }
    // HUD: speed (bottom centre), timer + medals (top centre), hint (under the timer).
    let small = |t: &str, size: f32| (Text::new(t), TextFont::from_font_size(size), TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)), TextShadow::default());
    commands
        .spawn((
            TutorialUi,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(18),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4),
                ..default()
            },
            Pickable::IGNORE,
            GlobalZIndex(30),
        ))
        .with_children(|p| {
            p.spawn((TimerText, small("", 30.0)));
            p.spawn((HintText, small("", 16.0)));
        });
    commands.spawn((
        TutorialUi,
        SpeedText,
        small("", 18.0),
        Node { position_type: PositionType::Absolute, left: percent(50), bottom: px(28), margin: UiRect::left(px(-20)), ..default() },
        Pickable::IGNORE,
        GlobalZIndex(30),
    ));
    // Ghost capsule (translucent; the alpha-blend StandardMaterial pipeline is pre-warmed).
    commands.spawn((
        TutorialUi,
        GhostBody,
        Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.7, 0.9, 1.0, 0.28),
            emissive: LinearRgba::rgb(0.25, 0.45, 0.6),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::default(),
        Visibility::Hidden,
        bevy::light::NotShadowCaster,
    ));
}

fn exit_tutorial(mut commands: Commands, q: Query<Entity, With<TutorialUi>>, mut respawn: ResMut<RespawnPoint>) {
    for e in &q {
        commands.entity(e).despawn();
    }
    respawn.0 = None;
}

#[allow(clippy::too_many_arguments)]
fn station_keys(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    arena: Res<ArenaWorld>,
    tuning: Res<MoveTuning>,
    menu: Res<crate::state::InGameMenu>,
    focus: Res<crate::input::UiFocus>,
    mut respawned: MessageReader<PlayerRespawned>,
    mut look: ResMut<LookAngles>,
    mut respawn: ResMut<RespawnPoint>,
    mut tut: ResMut<TutorialState>,
    mut players: Query<&mut PlayerSim, With<LocalPlayer>>,
) {
    let Ok(mut sim) = players.single_mut() else { return };
    let tut = &mut *tut;
    // Void respawn happened in the sim: restart this station's run/ghost.
    if respawned.read().count() > 0 {
        let st = tut.station;
        teleport(st, &arena, &tuning, &mut sim, &mut look, &mut respawn, tut);
    }
    if menu.0 || focus.typing {
        tut.r_hold = 0.0;
        return;
    }
    const DIGITS: [KeyCode; 7] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
    ];
    for (i, k) in DIGITS.iter().enumerate() {
        if keys.just_pressed(*k) {
            teleport(i, &arena, &tuning, &mut sim, &mut look, &mut respawn, tut);
        }
    }
    // Hold R to restart the station (R also reels, so only when not tethered).
    if keys.pressed(KeyCode::KeyR) && !sim.state.tethered() {
        tut.r_hold += time.delta_secs();
        if tut.r_hold > 0.45 {
            tut.r_hold = -10.0; // once per hold
            let st = tut.station;
            teleport(st, &arena, &tuning, &mut sim, &mut look, &mut respawn, tut);
        }
    } else if !keys.pressed(KeyCode::KeyR) {
        tut.r_hold = 0.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn start_replay(
    mut req: ResMut<ReplayGhost>,
    ghosts: Res<Ghosts>,
    arena: Res<ArenaWorld>,
    tuning: Res<MoveTuning>,
    mut queue: ResMut<crate::input::TickInputQueue>,
    mut look: ResMut<LookAngles>,
    mut respawn: ResMut<RespawnPoint>,
    mut tut: ResMut<TutorialState>,
    mut players: Query<&mut PlayerSim, With<LocalPlayer>>,
) {
    let Some(name) = req.0.take() else { return };
    let Some(g) = ghosts.0.iter().find(|g| g.station == name) else {
        warn!("no ghost named {name}");
        return;
    };
    let station = (0..7).find(|i| station_start(&arena, *i).is_some_and(|s| s.2 == name)).unwrap_or(6);
    let Ok(mut sim) = players.single_mut() else { return };
    teleport(station, &arena, &tuning, &mut sim, &mut look, &mut respawn, &mut tut);
    queue.0 = g
        .inputs
        .iter()
        .map(|&(yaw, pitch, move_x, move_z, buttons, reel_notches)| bd_core::input::PlayerInput {
            seq: 0,
            yaw,
            pitch,
            move_x,
            move_z,
            buttons,
            reel_notches,
        })
        .collect();
    info!("replaying ghost {name}: {} ticks", queue.0.len());
}

/// Closest distance from `c` to segment `a..b`.
fn seg_dist(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() > 1e-8 { ((c - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t).distance(c)
}

fn peak_timer(arena: Res<ArenaWorld>, positions: Res<FramePositions>, mut tut: ResMut<TutorialState>) {
    let Some(t) = arena.info.tutorial.as_ref() else { return };
    let (ring_c, ring_r) = (v3(t.start_finish.0), t.start_finish.1);
    let gates: Vec<(Vec3, f32)> = t.peak_gates.iter().map(|(c, r)| (v3(*c), *r)).collect();
    // One segment per sim tick: (from, to) of the chest point.
    let chest = Vec3::Y * 0.9;
    for (from, to) in positions.0.iter().copied() {
        tut.ticks += 1;
        let (a, b) = (from + chest, to + chest);
        match tut.run {
            Run::Armed | Run::Finished { .. } if tut.station == 6 => {
                // Start: crossing the ring plane (x = ring.x) heading +X, inside the ring.
                if a.x < ring_c.x && b.x >= ring_c.x {
                    let f = (ring_c.x - a.x) / (b.x - a.x);
                    let p = a.lerp(b, f);
                    if Vec2::new(p.y - ring_c.y, p.z - ring_c.z).length() <= ring_r {
                        tut.run = Run::Running { start_tick: tut.ticks, next_gate: 0 };
                        tut.ghost_t = Some(0.0);
                    }
                }
            }
            Run::Running { start_tick, next_gate } => {
                if let Some((c, r)) = gates.get(next_gate).copied()
                    && seg_dist(a, b, c) <= r
                {
                    if next_gate + 1 == gates.len() {
                        let time = (tut.ticks - start_tick) as f32 * DT;
                        tut.run = Run::Finished { time };
                        tut.best = Some(tut.best.map_or(time, |b| b.min(time)));
                        info!("peak run finished: {time:.3} s");
                    } else {
                        tut.run = Run::Running { start_tick, next_gate: next_gate + 1 };
                    }
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_ghost(
    time: Res<Time<Real>>,
    ghosts: Res<Ghosts>,
    arena: Res<ArenaWorld>,
    mut tut: ResMut<TutorialState>,
    anchor: Res<crate::camera::ViewAnchor>,
    players: Query<&PlayerSim, With<LocalPlayer>>,
    mut body: Query<(&mut Transform, &mut Visibility), With<GhostBody>>,
) {
    let Ok((mut tf, mut vis)) = body.single_mut() else { return };
    let name = station_start(&arena, tut.station).map(|s| s.2).unwrap_or("");
    let ghost = ghosts.0.iter().find(|g| g.station == name);
    // Stations: the ghost starts when you do (first movement). Peak: when the timer starts
    // (its recording starts at the spawn, so offset by the time the bot took to reach the ring).
    if tut.ghost_t.is_none()
        && tut.station != 6
        && let Ok(sim) = players.single()
        && v3(sim.state.vel).length() > 0.5
    {
        tut.ghost_t = Some(0.0);
    }
    let (Some(g), Some(t)) = (ghost, tut.ghost_t) else {
        *vis = Visibility::Hidden;
        return;
    };
    let t = t + time.delta_secs();
    tut.ghost_t = Some(t);
    let offset = if tut.station == 6 { ghost_ring_offset(g, &arena) } else { 0.0 };
    let f = (t + offset) * 30.0;
    let i = f.floor() as usize;
    if i + 1 >= g.positions.len() {
        *vis = Visibility::Hidden;
        return;
    }
    let (p0, p1) = (Vec3::from(g.positions[i]), Vec3::from(g.positions[i + 1]));
    let p = p0.lerp(p1, f.fract());
    tf.translation = p + Vec3::Y * 0.9;
    // Don't let the ghost fill the screen when it overlaps the camera.
    let want = if tf.translation.distance(anchor.eye) < 3.0 { Visibility::Hidden } else { Visibility::Visible };
    if *vis != want {
        *vis = want;
    }
}

/// Seconds from the peak ghost's spawn until it crossed the start ring.
fn ghost_ring_offset(g: &GhostFile, arena: &ArenaWorld) -> f32 {
    let Some(t) = arena.info.tutorial.as_ref() else { return 0.0 };
    let rx = t.start_finish.0.x;
    g.positions
        .windows(2)
        .position(|w| w[0].0 < rx && w[1].0 >= rx)
        .map(|i| (i + 1) as f32 / 30.0)
        .unwrap_or(0.0)
}

#[allow(clippy::type_complexity)]
fn update_ui(
    time: Res<Time<Real>>,
    arena: Res<ArenaWorld>,
    mut tut: ResMut<TutorialState>,
    players: Query<&PlayerSim, With<LocalPlayer>>,
    mut speed: Query<&mut Text, (With<SpeedText>, Without<TimerText>, Without<HintText>)>,
    mut timer: Query<(&mut Text, &mut TextColor), (With<TimerText>, Without<SpeedText>, Without<HintText>)>,
    mut hint: Query<(&mut Text, &mut TextColor, &mut Visibility), (With<HintText>, Without<SpeedText>, Without<TimerText>)>,
) {
    let Ok(sim) = players.single() else { return };
    let v = v3(sim.state.vel);
    if let Ok(mut s) = speed.single_mut() {
        let txt = format!("{:.0}", Vec2::new(v.x, v.z).length());
        if s.0 != txt {
            s.0 = txt;
        }
    }
    let medals = arena.info.tutorial.as_ref().map(|t| t.medals);
    if let Ok((mut t, mut c)) = timer.single_mut() {
        let (txt, color) = match (tut.run, medals) {
            (Run::Running { start_tick, .. }, _) => (format!("{:.2}", (tut.ticks - start_tick) as f32 * DT), Color::WHITE),
            (Run::Finished { time }, Some(m)) => {
                let (medal, col) = if time <= m.gold {
                    ("GOLD", Color::srgb(1.0, 0.85, 0.25))
                } else if time <= m.silver {
                    ("SILVER", Color::srgb(0.85, 0.9, 1.0))
                } else if time <= m.bronze {
                    ("BRONZE", Color::srgb(0.95, 0.6, 0.35))
                } else {
                    ("", Color::WHITE)
                };
                (format!("{time:.2}  {medal}"), col)
            }
            (Run::Armed, Some(m)) => (format!("gold {:.2}", m.gold), Color::srgba(1.0, 1.0, 1.0, 0.6)),
            _ => (String::new(), Color::WHITE),
        };
        if t.0 != txt {
            t.0 = txt;
        }
        if c.0 != color {
            c.0 = color;
        }
    }
    tut.hint_t += time.delta_secs();
    if let Ok((mut h, mut c, mut vis)) = hint.single_mut() {
        let (_, _, name, text) = station_start(&arena, tut.station).unwrap_or((Vec3::ZERO, 0.0, "", ""));
        let n = tut.station + 1;
        let txt = format!("{n}  {}   {text}", name.replace('_', " ").to_uppercase());
        if h.0 != txt {
            h.0 = txt;
        }
        // Visible for 5 s after a (re)start, then fades.
        let a = (1.0 - (tut.hint_t - 5.0).max(0.0)).clamp(0.0, 1.0) * 0.85;
        c.0 = Color::srgba(1.0, 1.0, 1.0, a);
        let want = if a > 0.01 { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}
