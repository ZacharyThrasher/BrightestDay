//! TRAINING FIELD practice dummy (`World::set_dummy` / `set_dummy_stationary` / `reset_dummy`):
//! STATIONARY takes damage / hitstun but never moves; LAUNCH is a normal player; it always respawns
//! at its own anchor, never a normal spawn point.

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::world::{ArenaSource, SimConfig, World};

const ATTACKER: PlayerId = PlayerId(1);
const DUMMY: PlayerId = PlayerId(2);

fn world() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.add_player(ATTACKER, "attacker".into(), Corps::from_index(1)).unwrap();
    w.add_player(DUMMY, "dummy".into(), Corps::from_index(2)).unwrap();
    w.start_live();
    w.tick(&[]);
    // No spawn shield in the way of these hits.
    w.player_mut(ATTACKER).unwrap().spawn_shield = 0.0;
    w.player_mut(DUMMY).unwrap().spawn_shield = 0.0;
    w
}

/// A hit hard enough to both damage and launch a normal player.
fn big_hit(target: PlayerId) -> HitInfo {
    HitInfo::new(target, Some(ATTACKER), HitKind::Melee, 40.0, 30.0, Vec3::new(1.0, 0.2, 0.0), Vec3::ZERO)
}

#[test]
fn stationary_takes_damage_but_never_moves() {
    let mut w = world();
    w.set_dummy(DUMMY, true);
    let anchor = w.player(DUMMY).unwrap().mv.pos;
    let r = w.apply_hit(big_hit(DUMMY));
    assert!(r.applied && r.damage > 0.0 && r.knockback > 0.0, "still a real hit: damage lands and `knockback` reports what it would have been");
    let p = w.player(DUMMY).unwrap();
    assert!(p.damage_pct > 0.0, "damage % applies normally");
    assert_eq!(p.mv.pos, anchor, "STATIONARY: zero displacement");
    assert_eq!(p.mv.vel, Vec3::ZERO, "STATIONARY: zero knockback velocity");
    assert!(p.launch.is_none(), "STATIONARY: never actually launched");
}

#[test]
fn launch_mode_gets_launched_like_a_real_player() {
    let mut w = world();
    w.set_dummy(DUMMY, false);
    w.apply_hit(big_hit(DUMMY));
    let p = w.player(DUMMY).unwrap();
    assert!(p.mv.vel.length() > 0.1, "LAUNCH: knockback applies a real velocity");
    assert!(p.launch.is_some(), "LAUNCH: a normal player, flagged launched");
}

#[test]
fn toggle_switches_behaviour_live() {
    let mut w = world();
    w.set_dummy(DUMMY, true);
    w.apply_hit(big_hit(DUMMY));
    assert_eq!(w.player(DUMMY).unwrap().mv.vel, Vec3::ZERO, "still stationary");
    w.set_dummy_stationary(DUMMY, false);
    w.apply_hit(big_hit(DUMMY));
    assert!(w.player(DUMMY).unwrap().mv.vel.length() > 0.1, "toggled to LAUNCH");
}

#[test]
fn reset_returns_to_zero_percent_at_its_anchor() {
    let mut w = world();
    w.set_dummy(DUMMY, false);
    let anchor = w.player(DUMMY).unwrap().mv.pos;
    w.apply_hit(big_hit(DUMMY));
    assert!(w.player(DUMMY).unwrap().damage_pct > 0.0);
    w.reset_dummy(DUMMY);
    let p = w.player(DUMMY).unwrap();
    assert_eq!(p.damage_pct, 0.0);
    assert_eq!(p.mv.pos, anchor);
    assert_eq!(p.mv.vel, Vec3::ZERO);
    assert!(p.alive);
}

#[test]
fn respawns_at_its_anchor_after_a_void_ko() {
    let mut w = world();
    w.set_dummy(DUMMY, false); // LAUNCH: it can actually go over the edge
    let anchor = w.player(DUMMY).unwrap().mv.pos;
    // As if a big launch had carried it out of bounds.
    let void_y = w.arena.void_y;
    w.player_mut(DUMMY).unwrap().mv.pos.y = void_y - 5.0;
    for _ in 0..600 {
        w.tick(&[]);
        let p = w.player(DUMMY).unwrap();
        if p.alive && p.mv.pos == anchor {
            return;
        }
    }
    panic!("dummy never came back to its anchor after falling out of bounds");
}

#[test]
fn set_dummy_is_a_pure_sim_flag_not_a_new_player_kind() {
    // No dummy flag at all: a completely normal player, unaffected.
    let mut w = world();
    let pos_before = w.player(ATTACKER).unwrap().mv.pos;
    w.apply_hit(big_hit(ATTACKER));
    assert!(w.player(ATTACKER).unwrap().mv.vel.length() > 0.1 || w.player(ATTACKER).unwrap().mv.pos != pos_before);
}
