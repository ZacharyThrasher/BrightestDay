//! CLI-driven automation: screenshots, timed exit with a PERF summary, stress benchmarks.
//! See `cli` for the flags.

use crate::arena::{crosshair_target, ArenaWorld, Explode};
use crate::camera::PlayerCamera;
use crate::cli::CliArgs;
use crate::debris::{DebrisPool, SpawnDebris, VfxRng, DEBRIS_CAPACITY};
use crate::hud::FrameTimes;
use crate::input::{InputOverride, LookAngles};
use bd_core::input::buttons;
use crate::juice::Impact;
use crate::state::AppState;
use crate::util::FrameStats;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Capturing, Screenshot};

#[derive(Resource, Default)]
struct AutoState {
    /// Real seconds since startup.
    t: f32,
    /// Real seconds since the first InGame frame (None before).
    t_game: Option<f32>,
    shots_taken: usize,
    screenshot_done_at: Option<f32>,
    exploded: bool,
    juice_next: f32,
    bench_next: f32,
    /// Frame times (ms) recorded for the summary.
    samples: Vec<f32>,
    debris_sum: f64,
    debris_peak: usize,
    exiting: bool,
    replay_started: bool,
}

/// Seconds after entering the game before stats are recorded (let the first explosions spawn).
const SETTLE_SECS: f32 = 1.0;

pub struct AutomationPlugin;

impl Plugin for AutomationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AutoState>().add_systems(Last, drive);
        app.add_systems(OnEnter(AppState::InGame), place_bench_camera.after(crate::player::SpawnPlayerSet));
    }
}

/// `--bench`: overlook the arena centre from a fixed vantage (deterministic view).
fn place_bench_camera(
    cli: Res<CliArgs>,
    arena: Option<Res<ArenaWorld>>,
    mut look: ResMut<LookAngles>,
    mut bodies: Query<&mut crate::player::PlayerSim, With<crate::player::LocalPlayer>>,
    mut frozen: ResMut<crate::player::PlayerFrozen>,
) {
    if cli.bench.is_none() || cli.pos.is_some() {
        return;
    }
    let Some(arena) = arena else { return };
    let c = arena.info.center;
    let eye = c + Vec3::new(0.0, 26.0, 58.0);
    frozen.0 = true;
    for mut b in &mut bodies {
        b.state.pos = crate::util::core_v3(eye - Vec3::Y * crate::player::EYE_HEIGHT);
        b.prev = b.state;
    }
    let d = (c - eye).normalize();
    look.yaw = (-d.x).atan2(-d.z);
    look.pitch = d.y.asin();
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    time: Res<Time<Real>>,
    cli: Res<CliArgs>,
    state: Res<State<AppState>>,
    ft: Res<FrameTimes>,
    pool: Res<DebrisPool>,
    arena: Option<Res<ArenaWorld>>,
    mut look_mut: ResMut<LookAngles>,
    mut ov: ResMut<InputOverride>,
    mut replay: ResMut<crate::tutorial::ReplayGhost>,
    cam: Query<&Transform, With<PlayerCamera>>,
    capturing: Query<(), With<Capturing>>,
    entities: Query<()>,
    mut rng: ResMut<VfxRng>,
    mut auto: ResMut<AutoState>,
    (mut explode, mut debris, mut impacts, mut exit): (
        MessageWriter<Explode>,
        MessageWriter<SpawnDebris>,
        MessageWriter<Impact>,
        MessageWriter<AppExit>,
    ),
) {
    let dt = time.delta_secs();
    auto.t += dt;
    let in_game = *state.get() == AppState::InGame;
    if in_game && auto.t_game.is_none() {
        auto.t_game = Some(0.0);
    } else if let Some(tg) = auto.t_game.as_mut() {
        *tg += dt;
    }
    let tg = auto.t_game.unwrap_or(-1.0);
    let cam_tf = cam.single().ok().copied();
    let look = *look_mut;

    // Record frame stats once in game and settled.
    if tg > SETTLE_SECS && !auto.exiting {
        if let Some(ms) = ft.recent.back() {
            auto.samples.push(*ms);
        }
        auto.debris_sum += pool.live() as f64;
        auto.debris_peak = auto.debris_peak.max(pool.live());
    }

    // One-shot explosion.
    if let (Some(at), Some(arena), Some(tf)) = (cli.explode_at, arena.as_deref(), cam_tf)
        && !auto.exploded
        && tg >= at
    {
        auto.exploded = true;
        explode.write(Explode::standard(crosshair_target(arena, tf.translation, look.forward())));
    }

    // Juice test: a medium hit on the local player every second.
    if cli.juice_test && tg >= 0.0 && tg >= auto.juice_next {
        auto.juice_next = tg + 1.0;
        let dir = rng.0.unit_vec3();
        impacts.write(Impact { impact: 0.7, dir, local_victim: true, color: Color::srgb(1.0, 0.2, 0.15) });
    }

    // Keep N debris alive: a fountain at the crosshair.
    if let (Some(n), Some(arena), Some(tf)) = (cli.bench_debris, arena.as_deref(), cam_tf)
        && tg >= 0.0
    {
        let n = n.min(DEBRIS_CAPACITY);
        let live = pool.live();
        if live < n {
            let at = crosshair_target(arena, tf.translation, look.forward());
            let r = &mut rng.0;
            for _ in 0..(n - live).min(4000) {
                let v = r.unit_vec3();
                let glow = r.f32() < 0.15;
                debris.write(SpawnDebris {
                    pos: at + Vec3::Y * 1.0 + v * 1.5,
                    vel: Vec3::new(v.x * 9.0, r.range(6.0, 18.0), v.z * 9.0),
                    color: if glow { [0.2, 1.0, 0.35] } else { [r.range(0.3, 0.6), r.range(0.25, 0.45), r.range(0.15, 0.3)] },
                    size: r.range(0.15, 0.45),
                    emissive: if glow { 4.0 } else { 0.0 },
                    lifetime: r.range(8.0, 14.0),
                });
            }
        }
    }

    // --bench: continuous G-style explosions across the view.
    if let (Some(_), Some(arena), Some(tf)) = (cli.bench, arena.as_deref(), cam_tf)
        && tg >= 0.0
        && tg >= auto.bench_next
    {
        auto.bench_next = tg + 0.2;
        let r = &mut rng.0;
        let jitter = Quat::from_euler(EulerRot::YXZ, r.range(-0.35, 0.35), r.range(-0.18, 0.12), 0.0);
        let dir = (tf.rotation * jitter) * Vec3::NEG_Z;
        let mut e = Explode::standard(crosshair_target(arena, tf.translation, dir));
        e.budget = 3500;
        e.shards = 400;
        explode.write(e);
    }

    // Scripted input demo.
    if cli.script.as_deref() == Some("swing")
        && let (Some(arena), Some(tf)) = (arena.as_deref(), cam_tf)
        && tg >= 0.0
    {
        run_swing_script(tg, arena, tf.translation, &mut look_mut, &mut ov);
    }

    // Ghost replay: `--script ghost:<station>` (tutorial map).
    if let Some(name) = cli.script.as_deref().and_then(|s| s.strip_prefix("ghost:"))
        && tg >= 0.5
        && !auto.replay_started
    {
        auto.replay_started = true;
        replay.0 = Some(name.to_string());
    }

    // Screenshots.
    let shots = cli.screenshots();
    let game_clock = cli.practice || cli.bench.is_some() || cli.script.is_some() || cli.map.is_some();
    if let Some((path, at)) = shots.get(auto.shots_taken)
        && (if game_clock { tg >= *at } else { auto.t >= *at && *state.get() != AppState::Loading })
    {
        auto.shots_taken += 1;
        info!("saving screenshot to {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
        auto.screenshot_done_at = None;
    }
    if !shots.is_empty() && auto.shots_taken == shots.len() && auto.screenshot_done_at.is_none() && capturing.is_empty() {
        // Give the observer a few frames to write the file before we allow exiting.
        auto.screenshot_done_at = Some(auto.t + 0.4);
    }

    // Exit conditions.
    let bench_end = cli.bench.map(|b| SETTLE_SECS + b);
    let quit_at = match (cli.quit_after, bench_end) {
        (Some(q), Some(b)) => Some(q.max(b)),
        (q, b) => q.or(b),
    };
    let Some(quit_at) = quit_at else { return };
    let clock = if game_clock { tg } else { auto.t };
    let shot_pending = cli.screenshot.is_some() && auto.screenshot_done_at.is_none_or(|d| auto.t < d);
    let hard_timeout = clock > quit_at + 5.0;
    if !auto.exiting && clock >= quit_at && (!shot_pending || hard_timeout) {
        auto.exiting = true;
        let mut scratch = Vec::new();
        let s = FrameStats::compute(auto.samples.iter().copied(), &mut scratch);
        let debris_avg = if auto.samples.is_empty() { 0.0 } else { auto.debris_sum / auto.samples.len() as f64 };
        let line = format!(
            "PERF frames={} avg_fps={:.1} avg_ms={:.3} p50_ms={:.3} p99_ms={:.3} max_ms={:.3} low1_fps={:.1} hitches={} debris_avg={:.0} debris_peak={} entities={}",
            s.frames, s.avg_fps, s.avg_ms, s.p50_ms, s.p99_ms, s.max_ms, s.low1_fps, s.hitches, debris_avg, auto.debris_peak, entities.iter().count()
        );
        println!("{line}");
        info!("{line}");
        if let Some(secs) = cli.bench {
            let path = cli.bench_out.clone().unwrap_or_else(|| "bench.json".into());
            let json = format!(
                "{{\n  \"bench_secs\": {secs},\n  \"frames\": {},\n  \"avg_ms\": {:.4},\n  \"p50_ms\": {:.4},\n  \"p99_ms\": {:.4},\n  \"max_ms\": {:.4},\n  \"hitches_16ms\": {},\n  \"avg_fps\": {:.2},\n  \"low1_fps\": {:.2},\n  \"debris_avg\": {:.0},\n  \"debris_peak\": {},\n  \"debris_capacity\": {}\n}}\n",
                s.frames, s.avg_ms, s.p50_ms, s.p99_ms, s.max_ms, s.hitches, s.avg_fps, s.low1_fps, debris_avg, auto.debris_peak, DEBRIS_CAPACITY
            );
            match std::fs::write(&path, json) {
                Ok(()) => println!("bench stats written to {}", path.display()),
                Err(e) => eprintln!("failed to write {}: {e}", path.display()),
            }
        }
        exit.write(AppExit::Success);
    }
}

/// `--script swing`: aim up at the centre landmark, tether + reel + boost, release, second tether.
fn run_swing_script(t: f32, arena: &ArenaWorld, eye: Vec3, look: &mut LookAngles, ov: &mut InputOverride) {
    let c = arena.info.center;
    // First anchor: the top of the tallest thing at the centre landmark (a solid voxel).
    let top = {
        let w = arena.read();
        let v = bd_core::voxel::world_to_voxel(crate::util::core_v3(c));
        (-3..=3)
            .flat_map(|dx| (-3..=3).map(move |dz| (dx, dz)))
            .filter_map(|(dx, dz)| w.top_solid_y(v.x + dx, v.z + dz).map(|y| Vec3::new((v.x + dx) as f32 + 0.5, y as f32 + 0.5, (v.z + dz) as f32 + 0.5) * bd_core::VOXEL_SIZE))
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .unwrap_or(c + Vec3::Y * 18.0)
    };
    let aim = |look: &mut LookAngles, target: Vec3| {
        let d = (target - eye).normalize_or(Vec3::NEG_Z);
        look.yaw = (-d.x).atan2(-d.z);
        look.pitch = d.y.asin();
    };
    ov.active = true;
    ov.move_x = 0;
    ov.move_z = 0;
    ov.held = 0;
    if t < 0.3 {
        return;
    }
    if t < 0.7 {
        aim(look, top - Vec3::Y * 0.3);
        return;
    }
    if t < 2.6 {
        ov.move_z = 1;
        ov.held |= buttons::TETHER_R;
        if t < 0.8 {
            ov.held |= buttons::JUMP;
        }
        if (0.9..1.7).contains(&t) {
            ov.held |= buttons::REEL | buttons::BOOST;
        }
        return;
    }
    if t < 3.0 {
        // Released: fly on momentum, look ahead at the next anchor.
        ov.move_z = 1;
        aim(look, c + Vec3::new(20.0, 12.0, -20.0));
        return;
    }
    if t < 4.5 {
        ov.move_z = 1;
        ov.held |= buttons::TETHER_L;
        if (3.2..3.8).contains(&t) {
            ov.held |= buttons::BOOST;
        }
        return;
    }
    ov.move_z = 1;
}
