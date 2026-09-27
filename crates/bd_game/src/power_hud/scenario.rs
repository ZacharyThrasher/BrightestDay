//! `--scenario powerhud` (practice, `--bots 1`): the power HUD proof shot. You hold three powers
//! (an awakened technique burning, a stand whose big move was just fired and is cooling, a logia),
//! carry an Awakening pickup and burn; bot 1 stands in front with three powers of its own (one
//! cooling, one in a form) so its motes show. Everything goes through the real sim calls (grants,
//! power moves, statuses) — the HUD reads the replicated state like in any match.

use crate::input::LookAngles;
use crate::sim_bridge::ActiveWorld;
use bd_core::PlayerId;
use bd_sim::pickups::{GateKind, Pickup, PickupPhase};
use bd_sim::powers::move_sel;
use bd_sim::status::StatusKind;
use bevy::prelude::*;

#[derive(Resource, Default)]
struct PowerHudScenario {
    t: f32,
    placed: bool,
    /// The bot's cooling move is fired this late (s) so it is mid-cooldown in the shot.
    fire_at: Option<(f32, PlayerId, u8, f32)>,
    /// `--powerhud-shot tp`: no stands (their companions would hide the bot), bot straight ahead.
    tp: bool,
}

pub fn build(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "powerhud") {
        return;
    }
    let tp = args.windows(2).any(|w| w[0] == "--powerhud-shot" && w[1] == "tp");
    info!("power hud scenario (tp: {tp})");
    app.insert_resource(PowerHudScenario { tp, ..default() }).add_systems(Update, place.run_if(resource_exists::<ActiveWorld>));
}

fn grant_all(w: &mut bd_sim::World, id: PlayerId, keys: &[&str]) {
    for k in keys {
        if !w.grant_power(id, k) {
            warn!("power hud scenario: couldn't grant {k} to {id:?}");
        }
    }
}

/// Held index of power `key` for player `id`.
fn held_of(w: &bd_sim::World, id: PlayerId, key: &str) -> Option<usize> {
    let pid = bd_sim::powers::PowerTable::get().index_of(key)?;
    w.player(id)?.power.slot_of(pid)
}

fn place(mut sc: ResMut<PowerHudScenario>, time: Res<Time<Real>>, mut active: ResMut<ActiveWorld>, mut look: ResMut<LookAngles>) {
    sc.t += time.delta_secs();
    if let Some((at, bot, slot, yaw)) = sc.fire_at
        && sc.t >= at
    {
        sc.fire_at = None;
        if let Some(p) = active.world.player_mut(bot) {
            p.input.yaw = yaw + std::f32::consts::FRAC_PI_2;
        }
        if let Err(e) = active.world.power_move(bot, slot) {
            warn!("power hud scenario: bot move failed {e:?}");
        }
        if let Some(p) = active.world.player_mut(bot) {
            p.input.yaw = yaw + std::f32::consts::PI;
        }
        info!("power hud scenario: bot fired at {:.2}", sc.t);
    }
    if sc.placed || sc.t < 0.8 {
        return;
    }
    let Some(me) = active.local else { return };
    let Some((pos, yaw0)) = active.local_player().filter(|p| p.alive).map(|p| (p.mv.pos, p.input.yaw)) else { return };
    // A view with open ground where the bot will stand (never over the void).
    let ground = |w: &bd_sim::World, at: bd_core::Vec3| {
        bd_core::voxel::raycast(&w.voxels, at + bd_core::Vec3::Y * 3.0, bd_core::Vec3::NEG_Y, 6.0).map(|h| h.point + bd_core::Vec3::Y * 0.05)
    };
    let mut pick = None;
    for step in 0..24 {
        let yaw = yaw0 + step as f32 * std::f32::consts::TAU / 24.0;
        let d = bd_core::input::look_dir(yaw, 0.0);
        let f = bd_core::Vec3::new(d.x, 0.0, d.z).normalize_or(bd_core::Vec3::NEG_Z);
        let l = bd_core::Vec3::new(f.z, 0.0, -f.x);
        let spot = pos + f * if sc.tp { 6.5 } else { 5.0 } + l * if sc.tp { -2.4 } else { 2.2 };
        let beyond = pos + f * 9.0;
        if let (Some(g), Some(_)) = (ground(&active.world, spot), ground(&active.world, beyond)) {
            pick = Some((yaw, g));
            break;
        }
    }
    let Some((yaw, bot_at)) = pick else { return };
    if let Some(p) = active.world.player_mut(me) {
        p.input.yaw = yaw;
    }
    active.bots.clear();
    let pool = active.world.rules().willpower_max();
    let bot = PlayerId(1);
    let w = &mut active.world;
    // You: stand (fired + cooling), logia, then an awakened technique (active, burning).
    let (cooling, mine) = if sc.tp { ("spider_bite", ["spider_bite", "mera_mera", "ultra_instinct"]) } else { ("star_platinum", ["star_platinum", "mera_mera", "ultra_instinct"]) };
    grant_all(w, me, &mine);
    if let Some(p) = w.player_mut(me) {
        p.willpower = pool;
        p.spawn_shield = 0.0;
    }
    if let Some(k) = held_of(w, me, cooling).filter(|_| !sc.tp) {
        let slot = if k == 0 { move_sel::HOLD } else { move_sel::move_of(k, move_sel::HOLD) };
        if let Err(e) = w.power_move(me, slot) {
            warn!("power hud scenario: move failed {e:?}");
        }
    }
    if let Some(p) = w.player_mut(me) {
        p.status.apply(StatusKind::Burn, 12.0, None);
        p.willpower = pool * 0.62;
    }
    // A carried Awakening pickup (the mode on, so it stays).
    w.cfg.rules.powerups = true;
    let until = w.tick + 120 * 40;
    w.pickups.items.push(Pickup { id: 900, kind: GateKind::Awakening, phase: PickupPhase::Held, pos, holder: Some(me), until });
    // Bot 1: in front of you, facing you, three powers (one cooling, one in a form).
    if let Some(p) = w.player_mut(bot) {
        p.mv.pos = bot_at;
        p.mv.vel = bd_core::Vec3::ZERO;
        // Sideways while it fires (nothing comes at you), then it turns to face you.
        p.input.yaw = yaw + std::f32::consts::FRAC_PI_2;
        p.spawn_shield = 0.0;
    }
    let (bot_cooling, theirs) = if sc.tp { ("kryptonian", ["kryptonian", "spider_bite", "super_saiyan"]) } else { ("the_world", ["the_world", "super_saiyan", "gomu_gomu"]) };
    grant_all(w, bot, &theirs);
    if let Some(p) = w.player_mut(bot) {
        p.willpower = pool;
    }
    if let Some(k) = held_of(w, bot, bot_cooling) {
        let m = if sc.tp { move_sel::TAP } else { move_sel::HOLD };
        let slot = if k == 0 { m } else { move_sel::move_of(k, m) };
        if sc.tp {
            sc.fire_at = Some((sc.t + 2.2, bot, slot, yaw));
        } else if let Err(e) = w.power_move(bot, slot) {
            warn!("power hud scenario: bot move failed {e:?}");
        }
    }
    if let Some(p) = w.player_mut(bot) {
        p.input.yaw = yaw + std::f32::consts::PI - if sc.tp { 0.0 } else { 0.4 };
    }
    if let Some(k) = held_of(w, bot, "gomu_gomu") {
        let _ = w.power_move(bot, move_sel::select(k, 1));
    }
    look.yaw = yaw;
    look.pitch = -0.12;
    sc.placed = true;
    info!("power hud scenario placed");
}
