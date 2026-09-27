//! `--practice --bots 2 --scenario impactframes [--if-style black_flash,holy,..|all] [--if-view 1p|bystander]
//! [--if-real] [--if-reduce off|reduced|none] [--third-person]`: stages a hit in front of the camera every 1.5 s and fires an impact
//! frame for the sheet (`docs/impact/impact_frames_sheet.png`) — use with `--record <dir>` (a fixed
//! 30 fps step, so the 2–5 held frames land in the capture).
//!
//! * `1p` (default): you are the attacker, a bot stands 2.6 m ahead (full-screen frame; add
//!   `--third-person` for the 3P view).
//! * `bystander`: two bots trade the hit ~9 m ahead of you (the world-anchored bubble).
//! * `--if-real`: instead of a direct `ImpactFrameRequest`, the attacker actually shouts "black
//!   flash" (the real move: guaranteed crit → the Black Flash frame through the normal triggers).
//!
//! Each fire logs `impact frame scenario: fire <style> at <t>` (t = seconds in game).

use super::{ImpactFrameRequest, ImpactTrigger};
use crate::input::{InputOverride, LookAngles};
use crate::sim_bridge::ActiveWorld;
use bd_core::PlayerId;
use bevy::prelude::*;

const ALL: [&str; 6] = ["black_flash", "holy", "void", "ink", "comic", "glitch"];
const PERIOD: f32 = 1.5;
const FIRST: f32 = 2.2;

#[derive(Resource, Clone, Debug)]
pub struct ImpactScenario {
    styles: Vec<String>,
    bystander: bool,
    real: bool,
    t: f32,
    fired: usize,
    granted: bool,
}

pub fn build(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "impactframes") {
        return;
    }
    let arg = |k: &str| args.windows(2).find(|w| w[0] == k).map(|w| w[1].clone());
    let styles = match arg("--if-style").as_deref() {
        None | Some("all") => ALL.iter().map(|s| s.to_string()).collect(),
        Some(list) => list.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
    };
    let bystander = arg("--if-view").as_deref() == Some("bystander");
    let real = args.iter().any(|a| a == "--if-real");
    let reduce = match arg("--if-reduce").as_deref() {
        Some("reduced") => crate::settings::FlashReduction::Reduced,
        Some("none") => crate::settings::FlashReduction::NoFlashes,
        _ => crate::settings::FlashReduction::Off,
    };
    info!("impact frame scenario: {styles:?} bystander={bystander} real={real} reduce={reduce:?}");
    // The pref for this run only (the settings file is untouched).
    app.add_systems(Update, move |mut c: ResMut<crate::settings::ComfortSettings>| {
        if c.reduce_flashes != reduce {
            c.reduce_flashes = reduce;
        }
    });
    app.insert_resource(ImpactScenario { styles, bystander, real, t: 0.0, fired: 0, granted: false })
        .add_systems(Update, script.before(super::triggers).run_if(resource_exists::<ActiveWorld>));
}

fn ground(w: &bd_sim::World, x: f32, z: f32, near_y: f32) -> bd_core::Vec3 {
    let from = bd_core::Vec3::new(x, near_y + 4.0, z);
    let y = bd_core::voxel::raycast(&w.voxels, from, bd_core::Vec3::NEG_Y, 10.0).map_or(near_y, |h| h.point.y + 0.05);
    bd_core::Vec3::new(x, y, z)
}

fn place(w: &mut bd_sim::World, id: PlayerId, at: bd_core::Vec3, face: bd_core::Vec3) {
    let d = face - at;
    if let Some(p) = w.player_mut(id) {
        p.mv.pos = at;
        p.mv.vel = bd_core::Vec3::ZERO;
        p.mv.tumble = 0.0;
        p.spawn_shield = 0.0;
        p.damage_pct = 40.0;
        p.alive = true;
        p.willpower = 100.0;
        p.input.yaw = (-d.x).atan2(-d.z);
        p.input.pitch = 0.0;
    }
}

fn script(
    time: Res<Time<Real>>,
    mut sc: ResMut<ImpactScenario>,
    mut active: ResMut<ActiveWorld>,
    mut look: ResMut<LookAngles>,
    mut ov: ResMut<InputOverride>,
    mut out: MessageWriter<ImpactFrameRequest>,
) {
    sc.t += time.delta_secs();
    if sc.t < 0.8 {
        return;
    }
    let Some((me, me_pos)) = active.local_player().filter(|p| p.alive).map(|p| (p.id, p.mv.pos)) else { return };
    active.bots.clear();
    active.world.cfg.rules.willpower = bd_sim::WillpowerMode::Infinite;
    ov.active = true;
    ov.held = 0;
    ov.move_x = 0;
    ov.move_z = 0;
    let others: Vec<PlayerId> = active.world.players.iter().map(|p| p.id).filter(|id| *id != me).collect();
    let (Some(&b1), b2) = (others.first(), others.get(1).copied()) else { return };
    let fwd = bd_core::input::look_dir(look.yaw, 0.0);
    let side = bd_core::Vec3::new(-fwd.z, 0.0, fwd.x);
    // Phase in the cycle: keep the actors placed until the hit, then let the hit play out.
    let next = FIRST + sc.fired as f32 * PERIOD;
    let staging = sc.t < next && sc.t > next - PERIOD * 0.5;
    let (attacker, victim) = if sc.bystander { (b1, b2.unwrap_or(me)) } else { (me, b1) };
    if staging {
        let w = &mut active.world;
        if sc.bystander {
            let c = me_pos + fwd * 9.0;
            let a = ground(w, c.x - side.x * 1.2, c.z - side.z * 1.2, me_pos.y);
            let v = ground(w, c.x + side.x * 1.2, c.z + side.z * 1.2, me_pos.y);
            place(w, attacker, a, v);
            place(w, victim, v, a);
        } else {
            let v = ground(w, me_pos.x + fwd.x * 2.6, me_pos.z + fwd.z * 2.6, me_pos.y);
            place(w, victim, v, me_pos);
            if let Some(p) = w.player_mut(me) {
                p.mv.vel = bd_core::Vec3::ZERO;
                p.damage_pct = 0.0;
            }
        }
        // Park anyone else far away.
        for id in others.iter().skip(if sc.bystander { 2 } else { 1 }) {
            if let Some(p) = w.player_mut(*id) {
                p.alive = false;
                p.respawn_timer = 1e6;
            }
        }
    }
    // Look at the victim's chest (bystander: the midpoint).
    let w = &active.world;
    let eye = w.player(me).map(|p| p.eye(w.params())).unwrap_or(me_pos);
    let target = match (w.player(attacker), w.player(victim)) {
        (Some(a), Some(v)) if sc.bystander => (a.mv.pos + v.mv.pos) * 0.5 + bd_core::Vec3::Y * 1.0,
        (_, Some(v)) => v.hull(w.params()).center(),
        _ => eye + fwd,
    };
    if staging {
        let d = (target - eye).normalize_or_zero();
        look.yaw = (-d.x).atan2(-d.z);
        look.pitch = d.y.clamp(-1.0, 1.0).asin();
    }
    if sc.t < next {
        return;
    }
    let style = sc.styles[sc.fired % sc.styles.len().max(1)].clone();
    sc.fired += 1;
    info!("impact frame scenario: fire {style} at {:.3}", sc.t);
    if sc.real {
        let w = &mut active.world;
        if !sc.granted {
            sc.granted = w.grant_power(attacker, "black_flash");
        }
        if let Some(a) = w.player_mut(attacker) {
            a.power.move_cd = Default::default();
            // Face the victim.
            let (ap, vp) = (a.mv.pos, target);
            let d = vp - ap;
            a.input.yaw = (-d.x).atan2(-d.z);
        }
        let slot = bd_sim::powers::PowerTable::get()
            .index_of("black_flash")
            .and_then(|i| bd_sim::powers::PowerTable::get().def(i))
            .and_then(|d| d.moves.iter().position(|m| m.name == "black flash"))
            .unwrap_or(1) as u8;
        if let Err(e) = w.power_move(attacker, slot) {
            warn!("impact frame scenario: black flash failed: {e:?}");
        }
    } else {
        let p = super::bv(target);
        let dir = super::bv(target - w.player(attacker).map_or(eye, |a| a.mv.pos));
        out.write(ImpactFrameRequest::new(ImpactTrigger::Custom, p).between(Some(attacker), Some(victim)).weight(0.8).dir(dir).style(style));
    }
}
