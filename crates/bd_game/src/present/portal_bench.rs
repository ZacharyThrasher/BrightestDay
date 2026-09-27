//! `--scenario portals` (practice): portal perf + look checks. `BD_PORTAL_MODE` picks:
//! * `fill` (default) — a gate 3 m in front of you (the window fills most of the screen), its exit
//!   standing 30 m to the side, looking out over the arena;
//! * `none` — same pose, no portal (the baseline);
//! * `appear` — the pair opens in front of you 3 s in; prints the frame times around the first live
//!   portal-view frame (`PORTAL APPEAR ...`) — it must not spike;
//! * `door` — the same, as a doorman's doorway;
//! * `tracer` — slow bolts fired through the gate (tail split across both ends);
//! * `ab` — `fill`, with the live views switched off / on every second in the same run (so load
//!   from anything else hits both halves alike); prints `PORTAL AB` (frame p50 / p99 with and
//!   without, and the frames right after each switch-on: first appearance, textures re-created);
//! * `step3p` — (with `--third-person`) walk through the gate; `BD_PORTAL_SHOT=<png>` saves a
//!   screenshot just after the pass (camera still behind you, looking through the entry).
//!
//! `cargo build --profile fastrel -p bd_game`, then e.g.
//! `brightest_day --practice --bots 0 --scenario portals --bench 10 --windowed --size 1600x900`.

use super::portal_cross::PortalCross;
use super::portal_view::PortalViews;
use crate::input::{InputOverride, LookAngles};
use crate::sim_bridge::ActiveWorld;
use crate::state::AppState;
use bd_core::PlayerId;
use bd_sim::space::{PORTAL_RADIUS, PortalEnd, fit_portal};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Fill,
    None,
    Appear,
    Door,
    Tracer,
    Step3p,
    Ab,
}

#[derive(Resource)]
struct Scenario {
    mode: Mode,
    t: f32,
    placed: bool,
    opened: bool,
    /// (seconds since opened, frame ms) around the opening.
    frames: Vec<(f32, f32)>,
    first_live: Option<f32>,
    reported: bool,
    eye: Vec3,
    entry: Vec3,
    next_shot: f32,
    shot_at: Option<f32>,
    shot_done: bool,
    /// A/B: (frame ms, views on, frames since the last switch).
    ab: Vec<(f32, bool, u32)>,
    ab_since: u32,
}

pub fn build(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "portals") {
        return;
    }
    let mode = match std::env::var("BD_PORTAL_MODE").unwrap_or_default().as_str() {
        "none" => Mode::None,
        "appear" => Mode::Appear,
        "door" => Mode::Door,
        "tracer" => Mode::Tracer,
        "step3p" => Mode::Step3p,
        "ab" => Mode::Ab,
        _ => Mode::Fill,
    };
    info!("portal scenario: {mode:?}");
    app.insert_resource(Scenario {
        mode,
        t: 0.0,
        placed: false,
        opened: false,
        frames: Vec::with_capacity(1024),
        first_live: None,
        reported: false,
        eye: Vec3::ZERO,
        entry: Vec3::ZERO,
        next_shot: 0.0,
        shot_at: None,
        shot_done: false,
        ab: Vec::with_capacity(20_000),
        ab_since: 0,
    })
    .add_systems(Update, drive.run_if(in_state(AppState::InGame)).run_if(resource_exists::<ActiveWorld>));
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut sc: ResMut<Scenario>,
    mut active: ResMut<ActiveWorld>,
    mut look: ResMut<LookAngles>,
    mut ov: ResMut<InputOverride>,
    views: Option<Res<PortalViews>>,
    cross: Res<PortalCross>,
    mut off: ResMut<super::portal_view::PortalViewsOff>,
) {
    let dt = time.delta_secs();
    sc.t += dt;
    let Some(me) = active.local else { return };
    // ---- place: stand still facing a spot 3 m ahead (after the spawn settles)
    if !sc.placed {
        if sc.t < 1.0 {
            return;
        }
        let Some(p) = active.world.player(me).filter(|p| p.alive) else { return };
        let feet = super::bv(p.mv.pos);
        let fwd = Vec3::new(-look.yaw.sin(), 0.0, -look.yaw.cos());
        sc.eye = feet + Vec3::Y * 1.6;
        sc.entry = feet + fwd * if sc.mode == Mode::Step3p { 6.0 } else { 3.2 };
        active.bots.clear();
        look.pitch = if sc.mode == Mode::Step3p { -0.12 } else { 0.08 };
        sc.placed = true;
        sc.t = 0.0;
        return;
    }
    let open_at = match sc.mode {
        Mode::None => f32::INFINITY,
        Mode::Appear => 3.0,
        _ => 0.2,
    };
    if !sc.opened && sc.t >= open_at {
        sc.opened = true;
        sc.t = 0.0;
        let fwd = Vec3::new(-look.yaw.sin(), 0.0, -look.yaw.cos());
        let right = Vec3::new(-fwd.z, 0.0, fwd.x);
        let door = sc.mode == Mode::Door;
        let r = if door { 1.7 } else { PORTAL_RADIUS };
        let entry = sc.entry + Vec3::Y * if door { r } else { 0.4 };
        // Exit: 30 m to the right, 4 m up, looking back across the arena (toward the player's side).
        let exit_pos = sc.entry + right * 30.0 + Vec3::Y * 4.0;
        let exit_n = (-right * 0.6 + fwd * 0.8).normalize();
        let style = |e: PortalEnd| if door { e.door() } else { e };
        let w = &mut active.world;
        let a = fit_portal(&w.voxels, style(PortalEnd::new(super::gv(entry), super::gv(-fwd), bd_core::Vec3::Y, r)));
        let b = fit_portal(&w.voxels, style(PortalEnd::new(super::gv(exit_pos), super::gv(exit_n), bd_core::Vec3::Y, r)));
        w.create_portal(PlayerId(me.0), a, b, 120.0, None);
        info!("portal scenario: opened (entry {:?} r {:.2}, exit {:?} r {:.2})", a.pos, a.radius, b.pos, b.radius);
    }
    // ---- frame times around the first live view (appear)
    if sc.opened {
        let ms = dt * 1000.0;
        let live = views.as_ref().is_some_and(|v| v.live() > 0);
        if live && sc.first_live.is_none() {
            sc.first_live = Some(sc.t);
        }
        if sc.frames.len() < sc.frames.capacity() {
            let t = sc.t;
            sc.frames.push((t, ms));
        }
    } else if sc.mode == Mode::Appear && sc.t > open_at - 1.0 && sc.frames.len() < 512 {
        let t = sc.t - open_at;
        sc.frames.push((t, dt * 1000.0));
    }
    if sc.mode == Mode::Appear && !sc.reported && sc.opened && sc.t > 1.5 {
        sc.reported = true;
        let first = sc.first_live.unwrap_or(0.0);
        let mut before: Vec<f32> = sc.frames.iter().filter(|f| f.0 < 0.0).map(|f| f.1).collect();
        before.sort_by(|a, b| a.total_cmp(b));
        let med = before.get(before.len() / 2).copied().unwrap_or(0.0);
        let p99 = before.get(before.len() * 99 / 100).copied().unwrap_or(0.0);
        let after: Vec<f32> = sc.frames.iter().filter(|f| f.0 >= first).take(8).map(|f| f.1).collect();
        let max_after = sc.frames.iter().filter(|f| f.0 >= 0.0 && f.0 < first + 1.0).map(|f| f.1).fold(0.0f32, f32::max);
        let line = format!(
            "PORTAL APPEAR before: median {med:.2} ms p99 {p99:.2} ms | first live view {first:.3}s after opening, next 8 frames {after:.2?} ms | max within 1 s {max_after:.2} ms"
        );
        println!("{line}");
        info!("{line}");
    }
    // ---- mode actions
    match sc.mode {
        Mode::Ab if sc.opened => {
            let on = (sc.t as u32) % 2 == 1;
            if off.0 == on {
                off.0 = !on;
                sc.ab_since = 0;
            } else {
                sc.ab_since += 1;
            }
            // The measured frame is the one that just ended (it rendered the previous state).
            if sc.t > 1.0 && sc.ab.len() < sc.ab.capacity() {
                let (since, prev_on) = (sc.ab_since, if sc.ab_since == 0 { !on } else { on });
                sc.ab.push((dt * 1000.0, prev_on, since));
            }
            if sc.t > 13.0 && !sc.reported {
                sc.reported = true;
                let stat = |f: &dyn Fn(&(f32, bool, u32)) -> bool| {
                    let mut v: Vec<f32> = sc.ab.iter().filter(|x| f(x)).map(|x| x.0).collect();
                    v.sort_by(|a, b| a.total_cmp(b));
                    let n = v.len().max(1);
                    (v.len(), v.get(n / 2).copied().unwrap_or(0.0), v.get(n * 99 / 100).copied().unwrap_or(0.0), v.last().copied().unwrap_or(0.0))
                };
                let (n_on, p50_on, p99_on, max_on) = stat(&|x| x.1 && x.2 >= 4);
                let (n_off, p50_off, p99_off, max_off) = stat(&|x| !x.1 && x.2 >= 4);
                // First frames after the views come back (appearance: pipelines warm, textures new).
                let firsts: Vec<f32> = sc.ab.iter().filter(|x| x.1 && (1..=3).contains(&x.2)).map(|x| x.0).collect();
                let first_max = firsts.iter().copied().fold(0.0f32, f32::max);
                let first_mean = firsts.iter().sum::<f32>() / firsts.len().max(1) as f32;
                let line = format!(
                    "PORTAL AB views on: {n_on} frames p50 {p50_on:.2} p99 {p99_on:.2} max {max_on:.2} ms | off: {n_off} frames p50 {p50_off:.2} p99 {p99_off:.2} max {max_off:.2} ms | first 3 frames after each switch-on ({} frames): mean {first_mean:.2} max {first_max:.2} ms",
                    firsts.len()
                );
                println!("{line}");
                info!("{line}");
            }
        }
        Mode::Tracer if sc.opened => {
            sc.next_shot -= dt;
            if sc.next_shot <= 0.0 {
                sc.next_shot = 0.12;
                let target = sc.entry + Vec3::Y * 0.4;
                let from = sc.eye + Vec3::new(0.0, -0.3, 0.0);
                let dir = (target - from).normalize();
                let mut s = bd_sim::projectile::ProjectileSpawn::ring_blast(me, super::gv(from + dir * 0.6), super::gv(dir));
                s.vel = super::gv(dir * 45.0);
                s.life = 3.0;
                s.damage = 0.0;
                let _ = active.world.fire_projectile(s, None, None);
            }
        }
        Mode::Step3p if sc.opened => {
            ov.active = sc.t > 0.4 && !sc.shot_done;
            ov.move_z = 1;
            ov.move_x = 0;
            ov.held = 0;
            if cross.passes > 0 && sc.shot_at.is_none() {
                sc.shot_at = Some(sc.t + 0.03);
            }
            if let (Some(at), false) = (sc.shot_at, sc.shot_done)
                && sc.t >= at
            {
                sc.shot_done = true;
                if let Ok(path) = std::env::var("BD_PORTAL_SHOT") {
                    info!("portal scenario: step-through screenshot → {path} (folded: {:?})", cross.folded);
                    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
                }
            }
        }
        _ => {}
    }
}
