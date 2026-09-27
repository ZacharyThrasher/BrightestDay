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
}

pub fn build(app: &mut App) {
    let args: Vec<String> = std::env::args().collect();
    if !args.windows(2).any(|w| w[0] == "--scenario" && w[1] == "powerhud") {
        return;
    }
    info!("power hud scenario");
    app.init_resource::<PowerHudScenario>().add_systems(Update, place.run_if(resource_exists::<ActiveWorld>));
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
    if sc.placed || sc.t < 0.8 {
        return;
    }
    let Some(me) = active.local else { return };
    let Some((pos, yaw)) = active.local_player().filter(|p| p.alive).map(|p| (p.mv.pos, p.input.yaw)) else { return };
    active.bots.clear();
    let pool = active.world.rules().willpower_max();
    let bot = PlayerId(1);
    let w = &mut active.world;
    // You: stand (fired + cooling), logia, then an awakened technique (active, burning).
    grant_all(w, me, &["star_platinum", "mera_mera", "ultra_instinct"]);
    if let Some(p) = w.player_mut(me) {
        p.willpower = pool;
        p.spawn_shield = 0.0;
    }
    if let Some(k) = held_of(w, me, "star_platinum") {
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
    let fwd = bd_core::input::look_dir(yaw, 0.0);
    let fwd = bd_core::Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(bd_core::Vec3::NEG_Z);
    if let Some(p) = w.player_mut(bot) {
        p.mv.pos = pos + fwd * 4.5;
        p.mv.vel = bd_core::Vec3::ZERO;
        p.input.yaw = yaw + std::f32::consts::PI;
        p.spawn_shield = 0.0;
    }
    grant_all(w, bot, &["the_world", "super_saiyan", "gomu_gomu"]);
    if let Some(p) = w.player_mut(bot) {
        p.willpower = pool;
    }
    if let Some(k) = held_of(w, bot, "the_world") {
        let _ = w.power_move(bot, if k == 0 { move_sel::HOLD } else { move_sel::move_of(k, move_sel::HOLD) });
    }
    if let Some(k) = held_of(w, bot, "gomu_gomu") {
        let _ = w.power_move(bot, move_sel::select(k, 1));
    }
    look.yaw = yaw;
    look.pitch = -0.12;
    sc.placed = true;
    info!("power hud scenario placed");
}
