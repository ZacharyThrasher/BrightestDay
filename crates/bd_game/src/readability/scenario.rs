//! `--practice --bots 3 --scenario readability --readability-shot <shot> [--readability-out <dir>]`:
//! stages one readability read for screenshots (the bots stand down; the scenario holds them). The
//! scenario saves its own PNGs at the right moments (`<dir>/<shot>_<k>.png`, default
//! `docs/readability`), so timing never depends on load time. Shots:
//! * `afford` — the crosshair gauge while typing an affordable 2-spark cast ("sword", 5.4 sparks held);
//! * `refuse` — typing a 5-spark cast ("meteor", Titan) with 2.5 sparks: red, shaking, ghost sparks;
//! * `low` — under one spark: dim and sputtering;
//! * `trio` — three enemies at 20 %, 80 %, 160 % in front (the 80 % one at the crosshair shows its
//!   number); `--third-person` for the 3P view;
//! * `smoke` — one enemy deep in KO range, close;
//! * `compass` — the three off to your sides (off screen): the compass pips in their heat colour;
//! * `break` — a real 2-spark cast from a full gauge; frames through the break-off (slowed ×4 for
//!   the strip: `gauge.break_secs` is raised for this shot only).

use bd_core::PlayerId;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::console::RingConsole;
use crate::input::LookAngles;
use crate::sim_bridge::ActiveWorld;

#[derive(Resource, Default)]
struct Sc {
    shot: String,
    out: std::path::PathBuf,
    t: f32,
    placed: Option<f32>,
    yaw: f32,
    /// (bot, spot, damage %)
    holds: Vec<(PlayerId, bd_core::Vec3, f32)>,
    /// Scenario times (s after placing) to capture at, and how many were taken.
    at: Vec<f32>,
    taken: usize,
    cast_at: Option<f32>,
}

pub fn build(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "readability") {
        return;
    }
    let arg = |k: &str| args.windows(2).find(|w| w[0] == k).map(|w| w[1].clone());
    let shot = arg("--readability-shot").unwrap_or_else(|| "trio".into());
    let out = arg("--readability-out").map(std::path::PathBuf::from).unwrap_or_else(|| "docs/readability".into());
    info!("readability scenario: {shot} → {}", out.display());
    let at = match shot.as_str() {
        "break" => vec![2.9, 3.12, 3.32, 3.55, 3.85],
        _ => vec![3.0],
    };
    let cast_at = (shot == "break").then_some(3.0);
    app.insert_resource(Sc { shot, out, at, cast_at, ..default() }).add_systems(Update, (place, hold).chain().run_if(resource_exists::<ActiveWorld>));
}

fn ground(w: &bd_sim::World, at: bd_core::Vec3) -> Option<bd_core::Vec3> {
    bd_core::voxel::raycast(&w.voxels, at + bd_core::Vec3::Y * 4.0, bd_core::Vec3::NEG_Y, 10.0).map(|h| h.point + bd_core::Vec3::Y * 0.05)
}

fn place(mut sc: ResMut<Sc>, time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>) {
    sc.t += time.delta_secs();
    if sc.placed.is_some() || sc.t < 1.0 {
        return;
    }
    let Some(me) = active.local else { return };
    let Some((pos, yaw0)) = active.local_player().filter(|p| p.alive).map(|p| (p.mv.pos, p.input.yaw)) else { return };
    let others: Vec<PlayerId> = active.world.players.iter().map(|p| p.id).filter(|id| *id != me).collect();
    if others.len() < 3 {
        warn!("readability scenario: needs --bots 3");
    }
    let shot = sc.shot.clone();
    // Spots: in front (trio / smoke) or behind (compass), on ground, for some view direction.
    let layout: Vec<(f32, f32, f32)> = match shot.as_str() {
        // (metres ahead, metres right, damage %)
        "smoke" => vec![(6.0, 0.6, 170.0)],
        "compass" | "break" => vec![(6.0, -10.0, 20.0), (4.0, 10.0, 80.0), (2.0, -13.0, 160.0)],
        _ => vec![(9.0, -3.2, 20.0), (9.5, 0.0, 80.0), (9.0, 3.2, 160.0)],
    };
    let mut pick = None;
    for step in 0..24 {
        let yaw = yaw0 + step as f32 * std::f32::consts::TAU / 24.0;
        let d = bd_core::input::look_dir(yaw, 0.0);
        let f = bd_core::Vec3::new(d.x, 0.0, d.z).normalize_or(bd_core::Vec3::NEG_Z);
        let r = bd_core::Vec3::new(-f.z, 0.0, f.x);
        let spots: Vec<Option<bd_core::Vec3>> = layout.iter().map(|(a, b, _)| ground(&active.world, pos + f * *a + r * *b)).collect();
        if spots.iter().all(|s| s.is_some_and(|s| (s.y - pos.y).abs() < 2.5)) {
            pick = Some((yaw, spots.into_iter().flatten().collect::<Vec<_>>()));
            break;
        }
    }
    let Some((yaw, spots)) = pick else { return };
    active.bots.clear();
    sc.holds = others.iter().zip(spots.iter().zip(layout.iter())).map(|(id, (s, l))| (*id, *s, l.2)).collect();
    // Everyone not in the shot stays down.
    for id in others.iter().skip(sc.holds.len()) {
        if let Some(p) = active.world.player_mut(*id) {
            p.alive = false;
            p.respawn_timer = 1e6;
        }
    }
    if let Some(p) = active.world.player_mut(me) {
        p.input.yaw = yaw;
        p.spawn_shield = 0.0;
    }
    sc.yaw = yaw;
    look.yaw = yaw;
    look.pitch = -0.04;
    sc.placed = Some(sc.t);
    info!("readability scenario placed ({shot}) at {pos:?}, yaw {yaw:.2}");
}

#[allow(clippy::too_many_arguments)]
fn hold(
    mut commands: Commands,
    mut sc: ResMut<Sc>,
    mut active: ResMut<ActiveWorld>,
    mut look: ResMut<LookAngles>,
    console: Option<ResMut<RingConsole>>,
    lex: Option<Res<crate::console::RingLexicon>>,
    tuning: Option<ResMut<super::Readability>>,
) {
    let Some(t0) = sc.placed else { return };
    let t = sc.t - t0;
    let Some(me) = active.local else { return };
    let spark = active.world.rules().segment_size();
    // Hold the bots where the shot wants them, at their damage, facing you.
    let face = sc.yaw + std::f32::consts::PI;
    for &(id, spot, pct) in &sc.holds {
        if let Some(p) = active.world.player_mut(id) {
            p.alive = true;
            p.mv.pos = spot;
            p.mv.vel = bd_core::Vec3::ZERO;
            p.damage_pct = pct;
            p.spawn_shield = 0.0;
            p.input = bd_core::input::PlayerInput { seq: p.input.seq, yaw: face, ..Default::default() };
        }
    }
    let shot = sc.shot.clone();
    let wp = match shot.as_str() {
        "afford" => Some(spark * 5.4),
        "refuse" => Some(spark * 2.5),
        "low" => Some(spark * 0.6),
        "break" if sc.cast_at.is_some() => Some(active.world.rules().willpower_max()),
        _ => None,
    };
    if let (Some(wp), Some(p)) = (wp, active.world.player_mut(me)) {
        p.willpower = wp;
        p.damage_pct = if shot == "low" { 95.0 } else { 0.0 };
    }
    if let Some(mut c) = console {
        let text = match shot.as_str() {
            "afford" => Some("sword"),
            "refuse" => Some("meteor"),
            _ => None,
        };
        if let Some(text) = text
            && (!c.open || c.buf != text)
        {
            c.dev_type(text);
        }
    }
    if shot == "break"
        && let Some(mut r) = tuning
        && r.0.gauge.break_secs < 1.0
    {
        r.0.gauge.break_secs = 1.1;
    }
    look.yaw = sc.yaw;
    look.pitch = match shot.as_str() {
        "smoke" => 0.02,
        "break" => -0.3,
        _ => -0.04,
    };
    // The cast (a real one: the sim prices it in whole sparks).
    if let Some(at) = sc.cast_at
        && t >= at
        && let Some(lex) = lex
    {
        sc.cast_at = None;
        let recipe = lex.0.resolve("car").recipe;
        if let Some(p) = active.world.player(me) {
            let eye = p.eye(active.world.params());
            // Off to the left and down: the summon's own flash stays out of the gauge's way.
            let dir = bd_core::input::look_dir(sc.yaw, -0.3);
            let before = p.willpower;
            let res = active.world.cast(me, recipe, None, bd_sim::world::Aim::new(eye, dir));
            let after = active.world.player(me).map_or(0.0, |p| p.willpower);
            info!("readability scenario: cast car {res:?}: {before:.1} → {after:.1} ({:.2} sparks)", (before - after) / spark);
        }
    }
    // Captures.
    while sc.taken < sc.at.len() && t >= sc.at[sc.taken] {
        let k = sc.taken;
        sc.taken += 1;
        let _ = std::fs::create_dir_all(&sc.out);
        let name = if sc.at.len() > 1 { format!("{shot}_{k}.png") } else { format!("{shot}.png") };
        let path = sc.out.join(name);
        info!("readability scenario: capture {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}
