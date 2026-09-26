//! Local player: bd_sim's movement controller on the 120 Hz tick, rendered with interpolation.
//!
//! * `SimTick`/`SimSet::Simulate`: `prev = state; events = step_player(state, input, world)`.
//!   Every tick's [`MoveEvents`] are collected in [`FrameMoveEvents`] for audio / camera juice.
//! * Frame: eye = `lerp(prev.pos, pos, alpha)` + smoothed eye height (crouch) + step-up smoothing +
//!   landing dip → [`ViewAnchor`]. Rotation always comes from this frame's mouse (`camera`).
//! * [`AnchorPreview`]: `query_anchor` from the eye along the crosshair every frame (the crosshair
//!   shows whether a tether would stick).
//! * [`MoveTuning`] hot-reloads `assets/tuning/move.ron` (written with the defaults if missing).
//! * Falling below the arena's `void_y` respawns at a spawn point.

use crate::arena::ArenaWorld;
use crate::assets_dir;
use crate::camera::ViewAnchor;
use crate::cli::CliArgs;
use crate::input::{LocalInputState, LookAngles};
use crate::juice::{CameraShakeEvent, FovPunch};
use crate::state::AppState;
use crate::tick::{FrameSet, SimSet, SimTick, TickClock};
use crate::util::{core_v3, v3};
use bd_core::DT;
use bd_sim::movement::{query_anchor, step_player, CollisionWorld, MoveEvents, MoveParams, PlayerState};
use bevy::prelude::*;
use std::path::PathBuf;
use std::time::SystemTime;

pub const EYE_HEIGHT: f32 = 1.6;

/// The locally controlled player.
#[derive(Component)]
pub struct LocalPlayer;

/// Simulated movement state (tick space) + the previous tick for interpolation.
#[derive(Component, Clone, Copy, Debug)]
pub struct PlayerSim {
    pub state: PlayerState,
    pub prev: PlayerState,
    pub spawn_index: usize,
}

impl PlayerSim {
    /// Interpolated feet position.
    pub fn render_pos(&self, alpha: f32) -> Vec3 {
        v3(self.prev.pos).lerp(v3(self.state.pos), alpha)
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct MoveTuning(pub MoveParams);

/// Skip movement simulation (benchmarks / scripted cameras).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PlayerFrozen(pub bool);

/// Every tick's events from this frame (cleared at the start of each frame).
#[derive(Resource, Default, Debug)]
pub struct FrameMoveEvents(pub Vec<MoveEvents>);

/// Every tick's feet movement `(from, to)` this frame (for gate/ring crossing tests).
#[derive(Resource, Default, Debug)]
pub struct FramePositions(pub Vec<(Vec3, Vec3)>);

/// Where void falls respawn (feet, yaw). `None` = cycle through the map's spawn points.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct RespawnPoint(pub Option<(Vec3, f32)>);

/// The local player fell into the void and was respawned this frame.
#[derive(Message, Clone, Copy, Debug)]
pub struct PlayerRespawned;

/// Crosshair anchor query result for this frame.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct AnchorPreview {
    pub valid: bool,
    pub assisted: bool,
    pub point: Vec3,
    pub dist: f32,
}

/// Render-side eye smoothing.
#[derive(Resource, Default, Debug)]
struct EyeSmoothing {
    height: f32,
    step_offset: f32,
    dip: f32,
    dip_vel: f32,
}

/// `OnEnter(AppState::InGame)` set that spawns the local player (order after it to adjust it).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpawnPlayerSet;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameMoveEvents>()
            .init_resource::<FramePositions>()
            .init_resource::<RespawnPoint>()
            .add_message::<PlayerRespawned>()
            .init_resource::<AnchorPreview>()
            .init_resource::<EyeSmoothing>()
            .init_resource::<PlayerFrozen>()
            .insert_resource(MoveTuning(load_or_write_tuning()))
            .add_systems(OnEnter(AppState::InGame), spawn_local_player.in_set(SpawnPlayerSet))
            .add_systems(OnExit(AppState::InGame), despawn_local_player)
            .add_systems(Update, (clear_frame_events.in_set(FrameSet::Input), hot_reload_tuning))
            .add_systems(SimTick, movement_tick.in_set(SimSet::Simulate))
            .add_systems(
                Update,
                (write_view_anchor, movement_juice, update_anchor_preview)
                    .chain()
                    .after(FrameSet::Sim)
                    .before(FrameSet::Present),
            );
    }
}

fn tuning_path() -> PathBuf {
    assets_dir().join("tuning").join("move.ron")
}

fn load_or_write_tuning() -> MoveParams {
    let path = tuning_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => ron::from_str::<MoveParams>(&text).unwrap_or_else(|e| {
            eprintln!("move tuning: failed to parse {}: {e}; using defaults", path.display());
            MoveParams::default()
        }),
        Err(_) => {
            let p = MoveParams::default();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(text) = ron::ser::to_string_pretty(&p, ron::ser::PrettyConfig::default()) {
                let header = "// Movement tuning (bd_sim::movement::MoveParams). Hot-reloaded while the game runs.\n// Missing fields use the built-in defaults.\n";
                let _ = std::fs::write(&path, format!("{header}{text}\n"));
            }
            p
        }
    }
}

fn hot_reload_tuning(time: Res<Time<Real>>, mut tuning: ResMut<MoveTuning>, mut last: Local<(f32, Option<SystemTime>)>) {
    last.0 += time.delta_secs();
    if last.0 < 0.5 {
        return;
    }
    last.0 = 0.0;
    let Ok(modified) = std::fs::metadata(tuning_path()).and_then(|m| m.modified()) else { return };
    match last.1 {
        None => last.1 = Some(modified),
        Some(prev) if prev != modified => {
            last.1 = Some(modified);
            if let Ok(text) = std::fs::read_to_string(tuning_path()) {
                match ron::from_str::<MoveParams>(&text) {
                    Ok(p) => {
                        info!("move tuning reloaded");
                        tuning.0 = p;
                    }
                    Err(e) => warn!("move tuning: parse error, keeping previous: {e}"),
                }
            }
        }
        _ => {}
    }
}

/// Spawn `index` of the map: feet position + what to look at (explicit yaw or a point to face).
fn spawn_point(arena: Option<&ArenaWorld>, index: usize) -> (Vec3, Facing) {
    match arena {
        Some(a) if !a.info.spawns.is_empty() => {
            let s = a.info.spawns[index % a.info.spawns.len()];
            let facing = match a.info.spawn_yaw {
                Some(y) => Facing::Yaw(y),
                None => Facing::Point(a.info.center),
            };
            (s, facing)
        }
        _ => (Vec3::new(0.0, 30.0, 0.0), Facing::Yaw(0.0)),
    }
}

#[derive(Clone, Copy, Debug)]
enum Facing {
    Yaw(f32),
    Point(Vec3),
}

fn apply_facing(look: &mut LookAngles, eye: Vec3, facing: Facing) {
    match facing {
        Facing::Yaw(y) => {
            look.yaw = y;
            look.pitch = -0.08;
        }
        Facing::Point(p) => face(look, eye, p),
    }
}

fn face(look: &mut LookAngles, eye: Vec3, target: Vec3) {
    let d = (target - eye).normalize_or(Vec3::NEG_Z);
    look.yaw = (-d.x).atan2(-d.z);
    look.pitch = d.y.asin().clamp(-0.5, 0.2);
}

fn spawn_local_player(
    mut commands: Commands,
    cli: Res<CliArgs>,
    arena: Option<Res<ArenaWorld>>,
    tuning: Res<MoveTuning>,
    mut look: ResMut<LookAngles>,
    mut eye: ResMut<EyeSmoothing>,
) {
    let (spawn, facing) = spawn_point(arena.as_deref(), 0);
    let pos = cli.pos.unwrap_or(spawn);
    match cli.look {
        Some(l) => look.set_degrees(l.x, l.y),
        None => apply_facing(&mut look, pos + Vec3::Y * EYE_HEIGHT, facing),
    }
    let state = PlayerState::new(core_v3(pos), &tuning.0);
    *eye = EyeSmoothing { height: tuning.0.eye_stand, ..default() };
    commands.spawn((Name::new("LocalPlayer"), LocalPlayer, PlayerSim { state, prev: state, spawn_index: 0 }));
}

fn despawn_local_player(mut commands: Commands, q: Query<Entity, With<LocalPlayer>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn clear_frame_events(mut ev: ResMut<FrameMoveEvents>, mut pos: ResMut<FramePositions>) {
    ev.0.clear();
    pos.0.clear();
}

fn movement_tick(
    inputs: Res<LocalInputState>,
    tuning: Res<MoveTuning>,
    arena: Option<Res<ArenaWorld>>,
    mut events: ResMut<FrameMoveEvents>,
    mut positions: ResMut<FramePositions>,
    respawn: Res<RespawnPoint>,
    mut respawned: MessageWriter<PlayerRespawned>,
    mut look: ResMut<LookAngles>,
    frozen: Res<PlayerFrozen>,
    active: Option<Res<crate::sim_bridge::ActiveWorld>>,
    mut q: Query<&mut PlayerSim, With<LocalPlayer>>,
) {
    // A bd_sim World drives the local player (practice / host / client prediction): see sim_bridge.
    if active.is_some() {
        return;
    }
    let Some(arena) = arena else { return };
    if frozen.0 {
        for mut sim in &mut q {
            sim.prev = sim.state;
        }
        return;
    }
    let p = &tuning.0;
    let world = arena.read();
    let cw = CollisionWorld::new(&*world, &[]);
    for mut sim in &mut q {
        let sim = &mut *sim;
        sim.prev = sim.state;
        let input = inputs.current;
        let ev = step_player(&mut sim.state, &input, &cw, p, DT);
        events.0.push(ev);
        positions.0.push((v3(sim.prev.pos), v3(sim.state.pos)));
        // Hanging on an attached tether survives below the void (sanity floor 40 m under it).
        let hanging = sim.state.tethered() && sim.state.pos.y > arena.info.void_y - 40.0;
        if sim.state.pos.y < arena.info.void_y && !hanging {
            // Void KO (practice): respawn at the station/spawn point.
            let (pos, facing) = match respawn.0 {
                Some((pos, yaw)) => (pos, Facing::Yaw(yaw)),
                None => {
                    sim.spawn_index += 1;
                    spawn_point(Some(&arena), sim.spawn_index)
                }
            };
            sim.state = PlayerState::new(core_v3(pos), p);
            sim.prev = sim.state;
            apply_facing(&mut look, pos + Vec3::Y * EYE_HEIGHT, facing);
            respawned.write(PlayerRespawned);
        }
    }
}

fn write_view_anchor(
    time: Res<Time<Real>>,
    clock: Res<TickClock>,
    tuning: Res<MoveTuning>,
    events: Res<FrameMoveEvents>,
    mut smooth: ResMut<EyeSmoothing>,
    comfort: Res<crate::settings::ComfortSettings>,
    q: Query<&PlayerSim, With<LocalPlayer>>,
    mut anchor: ResMut<ViewAnchor>,
) {
    let Ok(sim) = q.single() else { return };
    let p = &tuning.0;
    let dt = time.delta_secs().min(0.05);
    // Crouch: ease the eye instead of snapping it.
    let target_h = if sim.state.crouched { p.eye_crouch } else { p.eye_stand };
    smooth.height += (target_h - smooth.height) * (1.0 - (-18.0 * dt).exp());
    // Step-ups: the hull teleports up a step; hide it by offsetting the eye and decaying.
    for ev in &events.0 {
        smooth.step_offset -= ev.stepped;
        if let Some(l) = ev.landed {
            // Landing dip, proportional to impact speed.
            smooth.dip_vel -= (l.impact_speed * 0.045).min(0.9) * comfort.landing_dip;
        }
        if ev.air_jumped {
            // AIR JUMP: a small push-off kick off the hard-light step (comfort-gated like the dip).
            smooth.dip_vel -= 0.3 * comfort.landing_dip;
        }
    }
    smooth.step_offset = smooth.step_offset.clamp(-1.0, 1.0) * (-dt / 0.045).exp();
    // Dip spring (slightly underdamped: a quick knee-bend and recover).
    let (k, c) = (160.0, 2.0 * 160f32.sqrt() * 0.8);
    smooth.dip_vel += (-k * smooth.dip - c * smooth.dip_vel) * dt;
    smooth.dip = (smooth.dip + smooth.dip_vel * dt).clamp(-0.5, 0.2);

    let feet = sim.render_pos(clock.alpha);
    anchor.eye = feet + Vec3::Y * (smooth.height + smooth.step_offset + smooth.dip);
    anchor.velocity = v3(sim.state.vel);
}

fn movement_juice(
    events: Res<FrameMoveEvents>,
    mut shake: MessageWriter<CameraShakeEvent>,
    mut fov: MessageWriter<FovPunch>,
) {
    for ev in &events.0 {
        if ev.tether_attached.iter().any(|a| *a) {
            fov.write(FovPunch { degrees: 2.2 });
        }
        if ev.boost_burst {
            fov.write(FovPunch { degrees: 1.6 });
        }
        if ev.slide_burst {
            fov.write(FovPunch { degrees: 1.2 });
        }
        if let Some(l) = ev.landed
            && l.impact_speed > 16.0
        {
            shake.write(CameraShakeEvent { trauma: ((l.impact_speed - 16.0) * 0.03).min(0.5), dir: Vec3::NEG_Y });
        }
        if let Some(w) = ev.wall_hit
            && w.speed > 12.0
        {
            shake.write(CameraShakeEvent { trauma: ((w.speed - 12.0) * 0.03).min(0.45), dir: -v3(w.normal) });
        }
    }
}

fn update_anchor_preview(
    arena: Option<Res<ArenaWorld>>,
    tuning: Res<MoveTuning>,
    look: Res<LookAngles>,
    anchor: Res<ViewAnchor>,
    players: Query<(), With<LocalPlayer>>,
    mut preview: ResMut<AnchorPreview>,
) {
    let Some(arena) = arena.filter(|_| !players.is_empty()) else {
        *preview = AnchorPreview::default();
        return;
    };
    let world = arena.read();
    let cw = CollisionWorld::new(&*world, &[]);
    *preview = match query_anchor(&cw, core_v3(anchor.eye), core_v3(look.forward()), &tuning.0) {
        Some(q) => AnchorPreview { valid: true, assisted: q.assisted, point: v3(q.point), dist: q.dist },
        None => AnchorPreview::default(),
    };
}
