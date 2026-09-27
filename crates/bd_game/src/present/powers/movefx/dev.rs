//! Dev harness for the move grammar frame sheets (never set in play; docs/toolbox/presentation_sheet.png).
//!
//! `BD_MOVEFX_TEST=<scene>` on the practice map (`--practice --bots 1`):
//!
//! | scene | power · move | shows |
//! |---|---|---|
//! | `blink_smoke` | teleport_bamf · bamf | Blink in the Smoke body: BAMF puffs at both ends, implosion, trace |
//! | `dash_speed` | viltrumite · plow through | Impulse in the Speedforce body (forced): air tear, streak ribbons, afterimages, skid |
//! | `shove` | kryptonian · super breath | Shove (Ice, from the element): cone shockwave, ground crack, debris |
//! | `slam` | gamma · hulk smash | SlamOnLand: dread ring tightening under you, crater ring, dust wall, cracks |
//! | `rubber_pistol` | gomu_gomu · pistol | Cast + Rubber: the arm stretches from the shoulder to the fist, wobbles, band-snaps back |
//! | `rubber_rocket` | gomu_gomu · rocket | Reel + Rubber: the arm shoots to the anchor, yanks you, snaps |
//! | `rigid_pistol` | gomu_gomu · pistol (forced `rigid`) | the SAME cast in the Rigid body: a telescoping banded pole |
//!
//! * `BD_MOVEFX_VIEW=fp` (you fire it; first person unless `--third-person`) or `obs` (a bot fires
//!   it across your view, 9 m ahead: how everyone else reads it).
//! * `BD_MOVEFX_STYLE=<key>`: force every beat's style (any scene).
//! * `BD_MOVEFX_SHOTS=<dir>`: screenshots after the move fires → `<dir>/<scene>_<view>_<NN>_<tag>.png`.
//! * `hud` scene: synthetic borrowed moves + a 4-link chain for the strip's chips / pips
//!   (pair with `BD_TAB_TEST=1` for the names).
//! * `BD_MOVEFX_SPAM=<n>`: perf — every bot (and you) fires the scene's move every 0.4 s from t = 3 s,
//!   `n` rounds; `MoveFxStats` (live beats, peak, step µs) is logged each second.

use super::{MoveChainFx, MoveFxFired, MoveFxStats};
use crate::sim_bridge::{ActiveWorld, Cmd, SimCommand};
use crate::state::AppState;
use bd_core::PlayerId;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

fn scene() -> Option<&'static str> {
    static V: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    V.get_or_init(|| env("BD_MOVEFX_TEST")).as_deref()
}

/// A dev-forced style key (any scene).
pub fn forced_style() -> Option<&'static str> {
    static V: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    V.get_or_init(|| env("BD_MOVEFX_STYLE").or_else(|| (scene() == Some("rigid_pistol")).then(|| "rigid".to_string())).or_else(|| (scene() == Some("dash_speed")).then(|| "speedforce".to_string())))
        .as_deref()
}

/// `BD_MOVEFX_OFF=1`: no move grammar at all (A/B frames: what the moves looked like before).
pub fn off() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| env("BD_MOVEFX_OFF").is_some())
}

fn observer() -> bool {
    env("BD_MOVEFX_VIEW").is_some_and(|v| v == "obs")
}

fn spam() -> u32 {
    env("BD_MOVEFX_SPAM").and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// (power key, slot, look pitch°, shot offsets after the move fires).
fn spec(s: &str) -> (&'static str, u8, f32, &'static [f32]) {
    match s {
        "blink_smoke" => ("teleport_bamf", 0, -4.0, &[0.02, 0.07, 0.15, 0.3, 0.6]),
        "dash_speed" => ("viltrumite", 1, -3.0, &[0.05, 0.12, 0.22, 0.35, 0.55]),
        "shove" => ("kryptonian", 1, -6.0, &[0.03, 0.08, 0.15, 0.28, 0.5]),
        "slam" => ("gamma", 0, -8.0, &[0.2, 0.45, 0.8, 1.05, 1.3, 1.6]),
        "rubber_pistol" | "rigid_pistol" => ("gomu_gomu", 0, -2.0, &[0.04, 0.1, 0.18, 0.3, 0.5, 0.75]),
        "rubber_rocket" => ("gomu_gomu", 1, -5.0, &[0.04, 0.1, 0.2, 0.35, 0.55]),
        _ => ("teleport_bamf", 0, 0.0, &[0.05, 0.15, 0.3]),
    }
}

#[derive(Resource, Default)]
struct Dev {
    t: f32,
    phase: u8,
    base: Option<(f32, bd_core::Vec3)>,
    actor: Option<PlayerId>,
    queue: Vec<SimCommand>,
    shots: Vec<(f32, String)>,
    taken: u32,
    fired_at: Option<f32>,
    rounds: u32,
    next_spam: f32,
    log_t: f32,
}

pub fn build(app: &mut App) {
    if scene().is_none() {
        return;
    }
    app.init_resource::<Dev>()
        .add_systems(Update, (dev_world, dev_plan).chain().run_if(in_state(AppState::InGame)))
        .add_systems(Update, dev_shots.after(super::super::super::PresentSet::Sync).run_if(in_state(AppState::InGame)))
        .add_systems(Update, dev_hud.before(super::super::super::PresentSet::Sync).run_if(in_state(AppState::InGame)));
}

/// `hud`: borrowed moves of four powers + a chain's first three links, then a shot.
fn dev_hud(active: Option<Res<ActiveWorld>>, mut dev: ResMut<Dev>, mut fired: MessageWriter<MoveFxFired>, mut chain: MessageWriter<MoveChainFx>) {
    if scene() != Some("hud") {
        return;
    }
    let Some(me) = active.as_ref().and_then(|a| a.local) else { return };
    const PLAN: [(f32, &str, u8); 5] = [(2.0, "teleport_bamf", 0), (2.35, "gamma", 0), (2.7, "gomu_gomu", 0), (3.05, "kryptonian", 1), (3.4, "teleport_bamf", 0)];
    let t = dev.t;
    for (k, (at, key, slot)) in PLAN.iter().enumerate() {
        if dev.rounds as usize == k && t >= *at {
            dev.rounds += 1;
            let table = bd_sim::powers::PowerTable::get();
            let Some(def) = table.index_of(key).and_then(|i| table.def(i)) else { continue };
            let Some(mv) = def.moves.get(*slot as usize) else { continue };
            fired.write(MoveFxFired::synthetic(me, mv, Some(def), true));
            if k < 3 {
                chain.write(MoveChainFx { player: me, index: k as u8, len: 4 });
            }
            if k == 4 {
                dev.shots.push((t + 0.12, "chips".to_string()));
                dev.shots.push((t + 0.9, "sweep".to_string()));
            }
        }
    }
}

fn dev_world(time: Res<Time<Real>>, active: Option<ResMut<ActiveWorld>>, mut look: ResMut<crate::input::LookAngles>, mut dev: ResMut<Dev>, mut cmds: MessageWriter<SimCommand>) {
    for c in dev.queue.drain(..) {
        cmds.write(c);
    }
    let Some(mut active) = active else { return };
    dev.t += time.delta_secs();
    let Some(me) = active.local_player().map(|p| (p.id, p.mv.pos, p.alive)) else { return };
    if !me.2 || dev.t < 0.6 {
        return;
    }
    let s = scene().unwrap_or("");
    let (_, _, pitch, _) = spec(s);
    let (base_yaw, origin) = *dev.base.get_or_insert((look.yaw, me.1));
    if spam() == 0 {
        active.bots.clear();
    }
    // Hold the look (you are the camera).
    look.yaw = base_yaw;
    look.pitch = if observer() { -6f32.to_radians() } else { pitch.to_radians() };
    let fwd = bd_core::input::look_dir(base_yaw, 0.0);
    let fwd = bd_core::Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(bd_core::Vec3::NEG_Z);
    let side = bd_core::Vec3::new(-fwd.z, 0.0, fwd.x);
    // The observed actor (bot 1) stands 9 m ahead, 5 m to the left, facing across your view;
    // the target dummy (the other bot / bot 1 in first person) stands 8 m ahead.
    let others: Vec<PlayerId> = active.world.players.iter().filter(|p| p.id != me.0 && p.alive).map(|p| p.id).collect();
    let pin_until = if spam() > 0 { 0.0 } else { dev.fired_at.map_or(f32::MAX, |f| f + 0.02) };
    for (k, id) in others.iter().enumerate() {
        let (want, face) = if observer() && k == 0 { (origin + fwd * 9.0 - side * 5.0, side) } else { (origin + fwd * (17.0 + 3.0 * k as f32) + side * (3.0 + 2.0 * k as f32), -fwd) };
        let ground = bd_core::voxel::raycast(&active.world.voxels, want + bd_core::Vec3::Y * 12.0, bd_core::Vec3::NEG_Y, 40.0).map(|h| h.point.y);
        if let Some(p) = active.world.player_mut(*id) {
            if dev.t < pin_until {
                p.mv.pos = bd_core::Vec3::new(want.x, ground.map_or(p.mv.pos.y, |g| g + 0.05), want.z);
                p.mv.vel = bd_core::Vec3::ZERO;
                p.input.yaw = (-face.x).atan2(-face.z);
                p.input.pitch = 0.0;
            }
            p.spawn_shield = 0.0;
            p.willpower = p.willpower.max(100.0);
        }
    }
    if let Some(p) = active.world.player_mut(me.0) {
        p.willpower = p.willpower.max(100.0);
    }
    dev.actor = Some(if observer() { others.first().copied().unwrap_or(me.0) } else { me.0 });
    if let (Some(f), Some(a)) = (dev.fired_at, dev.actor)
        && dev.t - f < 1.0
        && ((dev.t - f) * 4.0).floor() != ((dev.t - f - time.delta_secs()) * 4.0).floor()
        && let Some(p) = active.world.player(a)
    {
        info!("movefx test: actor at {:?} (t+{:.2})", p.mv.pos, dev.t - f);
    }
}

fn dev_plan(active: Option<Res<ActiveWorld>>, mut dev: ResMut<Dev>, stats: Res<MoveFxStats>) {
    let Some(active) = active else { return };
    let Some(actor) = dev.actor else { return };
    let s = scene().unwrap_or("");
    let (key, slot, _, _) = spec(s);
    let me = active.local;
    let send = |id: PlayerId, c: Cmd| if Some(id) == me { SimCommand::local(c) } else { SimCommand::remote(id, c) };
    let aim_of = |id: PlayerId| {
        if Some(id) == me {
            return None;
        }
        active.world.player(id).map(|p| {
            let eye = p.eye(active.world.params());
            let dir = bd_core::input::look_dir(p.input.yaw, if s == "rubber_rocket" { -0.12 } else { -0.05 });
            bd_sim::Aim::new(eye, dir)
        })
    };
    if dev.phase == 0 && dev.t > 1.0 {
        dev.phase = 1;
        info!("movefx test {s}: granting {key}");
        dev.queue.push(send(actor, Cmd::GrantPower(key.to_string())));
        if spam() > 0 {
            for p in active.world.players.iter().filter(|p| p.id != actor) {
                dev.queue.push(send(p.id, Cmd::GrantPower(key.to_string())));
            }
        }
    }
    if dev.phase == 1 && dev.t > 2.6 {
        dev.phase = 2;
        info!("movefx test {s}: firing slot {slot}");
        let aim = aim_of(actor);
        dev.queue.push(send(actor, Cmd::PowerMove(slot, aim)));
        dev.next_spam = dev.t + 0.4;
    }
    if spam() > 0 && dev.phase == 2 && dev.t >= dev.next_spam && dev.rounds < spam() {
        dev.rounds += 1;
        dev.next_spam = dev.t + 0.4;
        let ids: Vec<PlayerId> = active.world.players.iter().filter(|p| p.alive).map(|p| p.id).collect();
        for id in ids {
            let aim = aim_of(id);
            dev.queue.push(send(id, Cmd::PowerMove(slot, aim)));
        }
    }
    if spam() > 0 && dev.t - dev.log_t >= 1.0 {
        dev.log_t = dev.t;
        info!("movefx stats t={:.1}: live {} peak {} started {} step {:.0} us", dev.t, stats.live, stats.peak, stats.started, stats.step_us);
    }
}

fn dev_shots(mut fired: MessageReader<MoveFxFired>, mut dev: ResMut<Dev>, mut commands: Commands) {
    let s = scene().unwrap_or("");
    let t = dev.t;
    for f in fired.read() {
        if Some(f.player) == dev.actor && dev.fired_at.is_none() {
            dev.fired_at = Some(t);
            info!("movefx test {s}: fired {:?} at t={t:.2}", f.mv.name);
            for (k, o) in spec(s).3.iter().enumerate() {
                dev.shots.push((t + o, format!("{k}_{:03}ms", (o * 1000.0) as u32)));
            }
        }
    }
    let Some(dir) = env("BD_MOVEFX_SHOTS") else {
        dev.shots.retain(|x| x.0 > t);
        return;
    };
    dev.shots.sort_by(|a, b| a.0.total_cmp(&b.0));
    if dev.shots.first().is_some_and(|x| x.0 <= t) {
        let (_, tag) = dev.shots.remove(0);
        dev.taken += 1;
        let _ = std::fs::create_dir_all(&dir);
        let view = if observer() { "obs" } else { "fp" };
        let path = std::path::Path::new(&dir).join(format!("{s}_{view}_{:02}_{tag}.png", dev.taken));
        info!("movefx test {s}: shot {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}
