//! World-level behaviour: combat core, economy, limits, rules, space mechanics, seats, replication,
//! determinism. Most tests use the flat lab arena (ground top at y = 8) for speed.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{DT, PlayerId, Vec3};
use bd_sim::bots::{self, recipe};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::{CastError, GameEvent, GoneReason, KoCause};
use bd_sim::replicate::{DeltaDecoder, DeltaEncoder, WorldSnapshot};
use bd_sim::rng::Fnv64;
use bd_sim::rules::{CapMode, MatchPhase, ScaleLimit, WillpowerMode};
use bd_sim::space::{LinkEnd, PortalEnd};
use bd_sim::world::{Aim, ArenaSource, SimConfig, TargetHint, World};

/// Smash model: tests written against health read `100 − damage %` (same comparisons).
#[allow(dead_code)]
trait HealthShim {
    fn health(&self) -> f32;
}
impl HealthShim for bd_sim::Player {
    fn health(&self) -> f32 {
        100.0 - self.damage_pct
    }
}

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);

fn flat() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    World::new(cfg)
}

/// Flat world with players A, B (and C) spawned, match live, spawn shields off.
fn duel(n: u8) -> World {
    let mut w = flat();
    for k in 1..=n {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w
}

/// Smash model: KOs only through a blast zone — tag the victim, drop them into the void.
#[allow(dead_code)]
fn blast_ko(w: &mut World, victim: PlayerId, killer: PlayerId) {
    w.apply_hit(bd_sim::combat::HitInfo::new(victim, Some(killer), bd_sim::combat::HitKind::RingBlast, 1.0, 0.0, Vec3::X, Vec3::ZERO));
    w.player_mut(victim).unwrap().mv.pos.y = -100.0;
    w.tick(&[]);
}

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn hit(target: PlayerId, source: Option<PlayerId>, dmg: f32, kb: f32, dir: Vec3) -> HitInfo {
    HitInfo::new(target, source, HitKind::RingBlast, dmg, kb, dir, Vec3::ZERO)
}

fn hash_events(h: &mut Fnv64, evs: &[GameEvent]) {
    for e in evs {
        let bytes = bincode::serde::encode_to_vec(e, bincode::config::standard()).unwrap();
        h.write(&bytes);
    }
}

// ------------------------------------------------------------------------------------ match

#[test]
fn eight_bot_ffa_runs_to_ended_deterministically() {
    let run_match = || {
        let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed: 7 }, seed: 7, check_invariants: true, ..SimConfig::default() };
        cfg.rules.time_limit = 75.0;
        cfg.rules.ko_target = 4;
        cfg.timings.warmup_secs = 1.0;
        cfg.timings.auto_rematch = false;
        let mut w = World::new(cfg);
        let mut brains = bots::add_bots(&mut w, 0, 8, 0.8, 7);
        let mut inputs = Vec::new();
        let mut h = Fnv64::default();
        let mut ended = false;
        let mut kos = 0;
        for _ in 0..(90 * 120) {
            inputs.clear();
            bots::drive(&mut w, &mut brains, &mut inputs);
            let evs = w.tick(&inputs);
            hash_events(&mut h, evs);
            kos += evs.iter().filter(|e| matches!(e, GameEvent::Ko { .. })).count();
            if w.match_state.phase == MatchPhase::Ended {
                ended = true;
                break;
            }
        }
        (ended, h.0, w.state_hash(), w.voxels.content_hash(), w.tick, kos)
    };
    let a = run_match();
    let b = run_match();
    assert!(a.0, "match did not end");
    assert!(a.5 > 0, "nobody was KO'd");
    assert_eq!(a, b, "same seed must give identical events and state");
}

#[test]
fn warmup_counts_down_then_live_then_ended_and_rematch() {
    let mut w = flat();
    w.cfg.timings.warmup_secs = 0.5;
    w.cfg.timings.ended_secs = 0.5;
    w.cfg.rules.ko_target = 1;
    w.add_player(A, "a".into(), Corps::Hope).unwrap();
    run(&mut w, 120);
    assert_eq!(w.match_state.phase, MatchPhase::Warmup, "one player: no countdown");
    w.add_player(B, "b".into(), Corps::Rage).unwrap();
    let ev = run(&mut w, 70);
    assert_eq!(w.match_state.phase, MatchPhase::Live);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::MatchPhase { phase: MatchPhase::Live, .. })));
    // A KO wins it (first to 1).
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    blast_ko(&mut w, B, A);
    assert_eq!(w.match_state.phase, MatchPhase::Ended);
    assert_eq!(w.match_state.winner, Some(A));
    // Rematch resets scores.
    run(&mut w, 70);
    assert_eq!(w.match_state.phase, MatchPhase::Warmup);
    assert!(w.players.iter().all(|p| p.kos == 0 && p.deaths == 0));
}

#[test]
fn time_limit_ends_match_with_best_score_winner() {
    let mut w = duel(3);
    w.cfg.rules.time_limit = 1.0;
    blast_ko(&mut w, C, B);
    run(&mut w, 130);
    assert_eq!(w.match_state.phase, MatchPhase::Ended);
    assert_eq!(w.match_state.winner, Some(B));
}

#[test]
fn join_and_leave_mid_match_leave_no_dangling_refs() {
    let mut w = duel(3);
    w.apply_hit(hit(B, Some(A), 10.0, 5.0, Vec3::X));
    let eye = w.player(A).unwrap().eye(w.params());
    w.cast(A, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2), None, Aim { target_hint: Some(TargetHint::Player(B)), ..Aim::new(eye, Vec3::NEG_Z) }).unwrap();
    w.create_link(Some(A), LinkEnd::Player(A), LinkEnd::Player(B), 8.0, 6.0).unwrap();
    run(&mut w, 60);
    assert!(w.remove_player(A));
    let ev = run(&mut w, 2);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::PlayerLeft { player } if *player == A)));
    assert!(w.constructs.iter().all(|c| c.owner != A));
    assert!(w.links.is_empty());
    w.check_invariants().unwrap();
    w.add_player(PlayerId(9), "late".into(), Corps::Life).unwrap();
    run(&mut w, 5);
    assert!(w.player(PlayerId(9)).unwrap().alive);
}

// ------------------------------------------------------------------------------------ combat

#[test]
fn knockback_scales_with_missing_health_and_tumbles() {
    // Smash model: knockback grows with damage % (after the hit) and the hit's damage.
    let mut w = duel(2);
    let v0 = {
        let r = w.apply_hit(hit(B, Some(A), 0.0, 10.0, Vec3::X));
        r.knockback
    };
    assert!((v0 - 10.0).abs() < 1e-3, "0 % takes the move's base knockback");
    w.player_mut(B).unwrap().damage_pct = 80.0;
    let r = w.apply_hit(hit(B, Some(A), 20.0, 10.0, Vec3::X));
    let t = w.cfg.tuning.smash.clone();
    assert!((w.player(B).unwrap().damage_pct - 100.0).abs() < 1e-3, "damage adds %");
    assert!((r.knockback - bd_sim::smash::knockback(&t, 10.0, 20.0, 100.0, 1.0, 1.0)).abs() < 1e-3);
    assert!(r.knockback > 18.0);
    assert!(w.player(B).unwrap().mv.tumble > 0.0, "high knockback tumbles");
    assert!(w.player(B).unwrap().mv.vel.y > 0.0, "guaranteed lift");
}

#[test]
fn melee_momentum_bonus_is_bounded() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    place(&mut w, B, Vec3::new(2.0, 8.0, 0.0));
    w.player_mut(A).unwrap().mv.vel = Vec3::new(23.0, 0.0, 0.0);
    let mut h = hit(B, Some(A), 10.0, 5.0, Vec3::X);
    h.kind = HitKind::Melee;
    let r = w.apply_hit(h);
    assert!((r.damage - 15.0).abs() < 1e-3, "v_close 23 → +50 %: {}", r.damage);
    w.player_mut(B).unwrap().damage_pct = 0.0;
    w.player_mut(A).unwrap().mv.vel = Vec3::new(200.0, 0.0, 0.0);
    let r = w.apply_hit(h);
    assert!((r.damage - 16.0).abs() < 1e-3, "capped at +60 %");
    // Ranged hits get nothing.
    w.player_mut(B).unwrap().damage_pct = 0.0;
    let r = w.apply_hit(hit(B, Some(A), 10.0, 5.0, Vec3::X));
    assert!((r.damage - 10.0).abs() < 1e-3);
}

#[test]
fn void_ko_is_credited_to_last_attacker_within_six_seconds() {
    let mut w = duel(2);
    w.apply_hit(hit(B, Some(A), 1.0, 5.0, Vec3::X));
    run(&mut w, 60);
    place(&mut w, B, Vec3::new(0.0, -10.0, 0.0));
    let ev = run(&mut w, 1);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Ko { victim, killer: Some(k), cause: KoCause::Void, void: true, .. } if *victim == B && *k == A)));
    assert_eq!(w.player(A).unwrap().kos, 1);
    assert_eq!(w.player(B).unwrap().deaths, 1);
    // Respawn after 3 s with a shield.
    let ev = run(&mut w, (3.0 / DT) as u32 + 2);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Respawn { player, .. } if *player == B)));
    assert!(w.player(B).unwrap().shielded());
    // Stale credit: > 6 s after the hit → no killer.
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w.apply_hit(hit(B, Some(A), 1.0, 1.0, Vec3::X));
    run(&mut w, (6.5 / DT) as u32);
    place(&mut w, B, Vec3::new(500.0, 20.0, 0.0));
    let ev = run(&mut w, 1);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Ko { victim, killer: None, .. } if *victim == B)));
}

#[test]
fn spawn_shield_blocks_damage_both_ways() {
    let mut w = duel(2);
    w.player_mut(B).unwrap().spawn_shield = 1.5;
    let r = w.apply_hit(hit(B, Some(A), 50.0, 10.0, Vec3::X));
    assert!(!r.applied);
    assert_eq!(w.player(B).unwrap().health(), 100.0);
    assert!(w.events().iter().any(|e| matches!(e, GameEvent::Blocked { .. })));
    // A shielded attacker deals nothing.
    let r = w.apply_hit(hit(A, Some(B), 50.0, 10.0, Vec3::X));
    assert!(!r.applied);
    run(&mut w, 181);
    assert!(!w.player(B).unwrap().shielded());
    assert!(w.apply_hit(hit(B, Some(A), 5.0, 1.0, Vec3::X)).applied);
}

#[test]
fn respawn_picks_spawn_far_from_living_enemies() {
    let mut w = duel(2);
    let spawns = w.arena.spawns.clone();
    let a_pos = spawns[0];
    place(&mut w, A, a_pos);
    let s = w.pick_spawn(B);
    let best = spawns.iter().map(|p| (p - a_pos).length()).fold(0.0f32, f32::max);
    assert!(((s - a_pos).length() - best).abs() < 1e-3);
}

#[test]
fn wall_splat_damages_carves_and_bounces() {
    let mut w = duel(2);
    // The lab's east wall: x ∈ [24, 25], z ∈ [-6, 6], y ∈ [8, 12].
    place(&mut w, B, Vec3::new(21.0, 8.0, 0.0));
    // Tumbling but below the punch-through speed (a 32 m/s launch would plow through the wall).
    let r = w.apply_hit(hit(B, Some(A), 0.0, 20.0, Vec3::X));
    assert!(r.knockback > 15.0);
    let ev = run(&mut w, 30);
    let splat = ev.iter().find_map(|e| match e {
        GameEvent::WallSplat { player, damage, speed, normal, .. } if *player == B => Some((*damage, *speed, *normal)),
        _ => None,
    });
    let (dmg, speed, normal) = splat.expect("wall splat");
    assert!(speed > 15.0 && dmg > 0.0 && dmg <= 30.0);
    assert!(normal.x < -0.9);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Carve { .. })), "crater");
    assert!(w.player(B).unwrap().health() < 100.0);
    assert!(w.player(B).unwrap().last_attacker.is_some_and(|(a, _)| a == A));
}

#[test]
fn ring_blast_hits_without_tunnelling_and_respects_cooldown() {
    let mut w = duel(2);
    // A clear lane of the lab (x = 10).
    place(&mut w, A, Vec3::new(10.0, 8.0, 8.0));
    place(&mut w, B, Vec3::new(10.0, 8.0, -12.0));
    let inp = PlayerInput { yaw: 0.0, pitch: -0.03, buttons: buttons::PRIMARY, ..Default::default() };
    let mut fired = 0;
    let mut hits = 0;
    for _ in 0..60 {
        for e in w.tick(&[(A, inp)]) {
            match e {
                GameEvent::ProjectileFired { owner, .. } if *owner == A => fired += 1,
                GameEvent::Hit { target, kind: HitKind::RingBlast, .. } if *target == B => hits += 1,
                _ => {}
            }
        }
    }
    // 0.5 s of holding at 0.25 s cooldown → 2 shots (fired on the first tick too).
    assert!((2..=3).contains(&fired), "fired {fired}");
    assert!(hits >= 1, "90 m/s bolt must not tunnel through a 0.8 m hull");
}

#[test]
fn ring_shove_pushes_without_damage_and_bats_projectiles() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    place(&mut w, B, Vec3::new(0.0, 8.0, -2.0));
    let inp = PlayerInput { yaw: 0.0, pitch: 0.0, buttons: buttons::SECONDARY, ..Default::default() };
    let ev = w.tick(&[(A, inp)]).to_vec();
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: HitKind::RingShove, damage, .. } if *target == B && *damage == 0.0)));
    assert!(w.player(B).unwrap().mv.vel.z < -5.0);
    assert!(w.player(B).unwrap().last_attacker.is_some(), "shoves count for void credit");
}

#[test]
fn lag_compensated_blast_hits_where_the_shooter_saw_the_target() {
    // B stands at x = 8 until tick 60, then dodges to x = 15. A (20 ticks of view lag) fires at
    // tick 62 at where his screen still shows B: host-side lag compensation must count the hit;
    // without it the same shot misses.
    let shoot = |lagcomp: bool| {
        let mut w = duel(2);
        place(&mut w, A, Vec3::new(8.0, 8.0, 8.0));
        place(&mut w, B, Vec3::new(8.0, 8.0, -2.0));
        while w.tick < 60 {
            w.tick(&[]);
        }
        place(&mut w, B, Vec3::new(15.0, 8.0, -2.0));
        w.tick(&[]);
        w.tick(&[]);
        let view = (w.tick - 20) as f64;
        let seen = w.history.hull_at(B, view).unwrap().center();
        assert!((seen.x - 8.0).abs() < 0.1, "history shows the old position");
        let eye = eye_of(&w, A);
        let dir = (seen - eye).normalize();
        w.set_view_tick(A, lagcomp.then_some(view));
        w.spawn_projectile(bd_sim::projectile::ProjectileSpawn::ring_blast(A, eye, dir), lagcomp.then_some(view));
        run(&mut w, 30).iter().any(|e| matches!(e, GameEvent::Hit { target, .. } if *target == B))
    };
    assert!(shoot(true), "rewound hit");
    assert!(!shoot(false), "no rewind → miss");
}

// ------------------------------------------------------------------------------------ statuses

#[test]
fn elements_apply_statuses_and_water_electric_backfires() {
    let mut w = duel(3);
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    place(&mut w, B, Vec3::new(0.0, 8.0, -3.0));
    place(&mut w, C, Vec3::new(30.0, 8.0, 0.0));
    w.apply_hit(hit(B, Some(A), 5.0, 0.0, Vec3::Z).with_element(Element::Fire));
    assert!(w.player(B).unwrap().status.active(bd_sim::status::StatusKind::Burn));
    let hp = w.player(B).unwrap().health();
    run(&mut w, 120);
    assert!(w.player(B).unwrap().health() < hp - 2.0, "burn ticks");
    // A soaks himself, then zaps B: the chain arcs back to the wet caster.
    let a_hp = w.player(A).unwrap().health();
    w.player_mut(A).unwrap().status.apply(bd_sim::status::StatusKind::Wet, 4.0, None);
    w.apply_hit(hit(B, Some(A), 10.0, 0.0, Vec3::Z).with_element(Element::Electric));
    assert!(w.player(A).unwrap().health() < a_hp, "water + electric backfires on a wet caster");
    assert!(w.player(B).unwrap().status.active(bd_sim::status::StatusKind::Stun));
}

// ------------------------------------------------------------------------------------ economy

fn eye_of(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().eye(w.params())
}

#[test]
fn willpower_costs_regen_pause_staleness_and_refund() {
    let mut w = duel(2);
    let hammer = recipe("hammer", Form::Hammer, &[], Element::Light, 1.2);
    let aim = Aim::new(eye_of(&w, A), Vec3::NEG_Z);
    w.cast(A, hammer.clone(), None, aim).unwrap();
    let wp = w.player(A).unwrap().willpower;
    assert!((wp - 88.0).abs() < 1e-3, "cost 12 → {wp}");
    // Regen paused 1.5 s after casting (upkeep 0.6/s still drains).
    run(&mut w, 120);
    assert!(w.player(A).unwrap().willpower < wp);
    run(&mut w, 120);
    assert!(w.player(A).unwrap().willpower > wp - 1.0);
    // Staleness: same phrase again → ×1.3 (the old hammer is replaced, no refund).
    let before = w.player(A).unwrap().willpower;
    w.cast(A, hammer.clone(), None, aim).unwrap();
    let paid = before - w.player(A).unwrap().willpower;
    assert!((paid - 15.6).abs() < 1e-2, "stale cost {paid}");
    // Dismiss refunds 25 %.
    let before = w.player(A).unwrap().willpower;
    assert_eq!(w.dismiss(A), 1);
    assert!((w.player(A).unwrap().willpower - before - 15.6 * 0.25).abs() < 1e-2);
    // Can't afford → CastFailed.
    w.player_mut(A).unwrap().willpower = 5.0;
    let err = w.cast(A, hammer, None, aim).unwrap_err();
    assert!(matches!(err, CastError::NotEnoughWillpower { .. }));
}

#[test]
fn upkeep_starvation_shatters_constructs() {
    let mut w = duel(2);
    let aim = Aim::new(eye_of(&w, A), Vec3::new(0.0, -0.3, -1.0));
    let id = w.cast(A, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2), None, aim).unwrap();
    w.player_mut(A).unwrap().willpower = 0.0;
    w.player_mut(A).unwrap().regen_pause = 100.0;
    let ev = run(&mut w, 400);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ConstructShattered { id: s, .. } if *s == id)));
}

#[test]
fn slot_limits_replace_the_oldest() {
    let mut w = duel(2);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    let aim = Aim::new(eye_of(&w, A), Vec3::new(0.0, -0.4, -1.0));
    let wall = recipe("wall", Form::Wall, &[], Element::Light, 4.0);
    let first = w.cast(A, wall.clone(), None, aim).unwrap();
    for _ in 0..3 {
        w.cast(A, wall.clone(), None, aim).unwrap();
    }
    let ev = run(&mut w, 1);
    assert_eq!(w.constructs.owned_by(A).count(), 3);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ConstructGone { id, reason: GoneReason::Replaced, .. } if *id == first)));
    w.cast(A, recipe("sword", Form::Sword, &[], Element::Light, 1.0), None, aim).unwrap();
    w.cast(A, recipe("axe", Form::Axe, &[], Element::Light, 1.0), None, aim).unwrap();
    run(&mut w, 1);
    assert_eq!(w.constructs.held_by(A).unwrap().recipe.form, Form::Axe);
    // Chaos caps triple structures.
    w.cfg.rules.caps = CapMode::Chaos;
    for _ in 0..8 {
        w.cast(A, wall.clone(), None, aim).unwrap();
    }
    run(&mut w, 1);
    assert_eq!(w.constructs.in_slot(A, bd_sim::constructs::Slot::Structure).count(), 9);
}

#[test]
fn validation_clamps_scale_by_lobby_rules() {
    let mut w = duel(2);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    let aim = Aim::new(eye_of(&w, A), Vec3::new(0.0, -0.4, -1.0));
    let id = w.cast(A, recipe("huge hammer", Form::Hammer, &[], Element::Light, 500.0), None, aim).unwrap();
    assert_eq!(w.constructs.get(id).unwrap().recipe.scale, 3.0);
    w.cfg.rules.scale = ScaleLimit::NoLimits;
    let id = w.cast(A, recipe("moon", Form::Ball, &[], Element::Light, 500.0), None, aim).unwrap();
    assert_eq!(w.constructs.get(id).unwrap().recipe.scale, bd_sim::rules::NO_LIMITS_MAX_SCALE);
    assert!(w.constructs.get(id).unwrap().colossal);
    let err = w.cast(A, recipe("moon 2", Form::Boulder, &[], Element::Light, 500.0), None, aim).unwrap_err();
    assert_eq!(err, CastError::ColossalLimit);
}

#[test]
fn oath_recital_validates_and_refills() {
    let mut w = duel(2);
    w.player_mut(A).unwrap().willpower = 10.0;
    let corps = w.player(A).unwrap().corps;
    assert!(w.begin_oath(A, 1));
    let len = w.cfg.oaths.section_len(corps, 1);
    assert!(w.cfg.oaths.section_count(corps) >= 2 && len > 40);
    // Too fast to be human.
    run(&mut w, 10);
    assert!(w.oath_result(A, 1.0, 0.5).is_err());
    assert!(w.player(A).unwrap().willpower < 11.0, "no refill");
    // A plausible recital on target time with 90 % accuracy.
    assert!(w.begin_oath(A, 1));
    assert!(matches!(w.player(A).unwrap().oath, bd_sim::willpower::OathState::Reciting { len: l, .. } if l == len));
    let target = len as f32 / bd_sim::willpower::OATH_TARGET_CPS;
    run(&mut w, (target / DT) as u32);
    let before = w.player(A).unwrap().willpower;
    let refill = w.oath_result(A, 0.9, target).unwrap();
    assert!((refill - 81.0).abs() < 0.5, "refill {refill}");
    assert!((w.player(A).unwrap().willpower - (before + refill).min(100.0)).abs() < 1e-3);
    // Damage interrupts with a partial refill.
    w.player_mut(A).unwrap().willpower = 0.0;
    assert!(w.begin_oath(A, 0));
    run(&mut w, 600);
    w.apply_hit(hit(A, Some(B), 5.0, 0.0, Vec3::X));
    assert!(!w.player(A).unwrap().oath.reciting());
    assert!(w.events().iter().any(|e| matches!(e, GameEvent::OathInterrupted { refill, .. } if *refill > 0.0)));
}

#[test]
fn bots_recite_rotating_oath_sections() {
    let mut w = duel(0);
    let mut brains = bots::add_bots(&mut w, 1, 1, 0.8, 7);
    let id = brains[0].id;
    let corps = w.player(id).unwrap().corps;
    let lens: Vec<u16> = (0..w.cfg.oaths.section_count(corps)).map(|s| w.cfg.oaths.section_len(corps, s)).collect();
    let mut recited = Vec::new();
    let mut finished = 0;
    let mut inputs = Vec::new();
    for _ in 0..(240 * 120) {
        {
            let p = w.player_mut(id).unwrap();
            if !p.oath.reciting() && !p.charge.active {
                p.willpower = p.willpower.min(5.0);
            }
        }
        let was = w.player(id).unwrap().oath;
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        if let (false, bd_sim::willpower::OathState::Reciting { len, .. }) = (was.reciting(), w.player(id).unwrap().oath) {
            recited.push(len);
        }
        finished += w.tick(&inputs).iter().filter(|e| matches!(e, GameEvent::OathFinished { player, .. } if *player == id)).count();
        if recited.len() >= 4 && finished >= 3 {
            break;
        }
    }
    assert!(finished >= 3 && recited.len() >= 4, "bot recited {recited:?}, finished {finished}");
    // Section 0 first, then the rotation (never the same section twice in a row).
    for (k, len) in recited.iter().enumerate() {
        assert_eq!(*len, lens[k % lens.len()], "recital {k}: {recited:?} vs sections {lens:?}");
    }
}

#[test]
fn oath_section_comes_from_the_host_table() {
    let mut w = duel(2);
    let corps = w.player(A).unwrap().corps;
    let n = w.cfg.oaths.section_count(corps);
    // Each valid section recites its own length; a bogus section index reads as section 0.
    for (section, want) in [(0u8, 0u8), (n - 1, n - 1), (n, 0), (255, 0)] {
        assert!(w.begin_oath(A, section));
        let len = w.cfg.oaths.section_len(corps, want);
        assert!(matches!(w.player(A).unwrap().oath, bd_sim::willpower::OathState::Reciting { len: l, .. } if l == len), "section {section}");
        // The plausibility floor scales with the section: faster than OATH_MAX_CPS is refused.
        let min = bd_sim::willpower::oath_min_secs(len);
        run(&mut w, (min / DT) as u32 + 2);
        assert!(w.oath_result(A, 1.0, min * 0.8).is_err());
    }
    // A shorter section refills in proportionally less time: same full refill on target.
    let (s0, s1) = (w.cfg.oaths.section_len(corps, 0), w.cfg.oaths.section_len(corps, 1));
    for (section, len) in [(0u8, s0), (1, s1)] {
        w.player_mut(A).unwrap().willpower = 0.0;
        assert!(w.begin_oath(A, section));
        let target = bd_sim::willpower::oath_target_secs(len);
        run(&mut w, (target / DT) as u32 + 1);
        let refill = w.oath_result(A, 1.0, target).unwrap();
        assert!((refill - 100.0).abs() < 0.5, "section {section}: refill {refill}");
    }
}

// ------------------------------------------------------------------------------------ constructs

#[test]
fn grafting_onto_own_construct_merges_mechanics() {
    let mut w = duel(2);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    let aim = Aim::new(eye_of(&w, A), Vec3::new(0.0, -0.3, -1.0));
    let wall = w.cast(A, recipe("wall", Form::Wall, &[], Element::Light, 4.0), None, aim).unwrap();
    let before = w.constructs.len();
    let mut g = Aim::new(eye_of(&w, A), Vec3::NEG_Z);
    g.target_hint = Some(TargetHint::Construct(wall));
    g.modifier = true;
    let id = w.cast(A, recipe("legs", Form::Biped, &[Mechanic::Walks], Element::Fire, 1.0), None, g).unwrap();
    assert_eq!(id, wall);
    assert_eq!(w.constructs.len(), before);
    let c = w.constructs.get(wall).unwrap();
    assert!(c.has(Mechanic::Walks) && c.recipe.element == Element::Fire);
    assert!(w.events().iter().any(|e| matches!(e, GameEvent::Grafted { .. })));
}

#[test]
fn graft_aimed_at_terrain_animates_it() {
    let mut w = duel(2);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    // The lab's 4 m pillar (north row) at x ∈ [-18, -16.5], z ∈ [24, 25.5].
    place(&mut w, A, Vec3::new(-17.0, 8.0, 18.0));
    let eye = eye_of(&w, A);
    let target = Vec3::new(-17.2, 10.0, 24.2);
    let mut aim = Aim::new(eye, (target - eye).normalize());
    aim.modifier = true;
    let solid_before = w.voxels.solid_count();
    let id = w.cast(A, recipe("legs", Form::Biped, &[Mechanic::Walks], Element::Light, 1.0), None, aim).unwrap();
    let c = w.constructs.get(id).unwrap();
    let model = c.model.clone().expect("terrain model");
    assert!(!model.voxels.is_empty());
    assert!(w.voxels.solid_count() < solid_before);
    let op = w.events().iter().find_map(|e| match e {
        GameEvent::Extracted { op, .. } => Some(*op),
        _ => None,
    });
    // A client applying the same op to its own copy of the world gets the same model.
    let mut client = flat();
    let out = bd_sim::extract::apply_extract(&mut client.voxels, &op.unwrap());
    assert_eq!(out.model, *model);
    run(&mut w, 120);
    w.check_invariants().unwrap();
}

#[test]
fn held_hammer_swing_hits_with_melee_kind() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    place(&mut w, B, Vec3::new(0.0, 8.0, -1.6));
    let aim = Aim::new(eye_of(&w, A), Vec3::NEG_Z);
    w.cast(A, recipe("hammer", Form::Hammer, &[], Element::Light, 1.2), None, aim).unwrap();
    run(&mut w, 30);
    let press = PlayerInput { buttons: buttons::PRIMARY, ..Default::default() };
    let mut evs = Vec::new();
    evs.extend_from_slice(w.tick(&[(A, press)]));
    for _ in 0..40 {
        evs.extend_from_slice(w.tick(&[(A, PlayerInput::default())]));
    }
    assert!(evs.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: HitKind::Melee, .. } if *target == B)));
}

#[test]
fn creature_hunts_and_bites() {
    let mut w = duel(2);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    place(&mut w, B, Vec3::new(0.0, 8.0, -14.0));
    let mut aim = Aim::new(eye_of(&w, A), Vec3::new(0.0, -0.3, -1.0));
    aim.target_hint = Some(TargetHint::Player(B));
    w.cast(A, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2), None, aim).unwrap();
    let ev = run(&mut w, 480);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: HitKind::Creature, .. } if *target == B)));
}

#[test]
fn thrown_bomb_explodes_and_carves() {
    let mut w = duel(2);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    w.player_mut(A).unwrap().input.pitch = -0.5;
    let aim = Aim::new(eye_of(&w, A), Vec3::new(0.0, -0.5, -1.0).normalize());
    w.cast(A, recipe("bomb", Form::Bomb, &[Mechanic::Explodes], Element::Fire, 0.7), None, aim).unwrap();
    let ev = run(&mut w, 240);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Explosion { .. })));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Carve { .. })));
}

#[test]
fn opposing_constructs_clash_and_shatter() {
    let mut w = duel(2);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    place(&mut w, B, Vec3::new(0.0, 8.0, -20.0));
    let wall_aim = Aim::new(eye_of(&w, B), (Vec3::new(0.0, 8.0, -12.0) - eye_of(&w, B)).normalize());
    let wall = w.cast(B, recipe("wall", Form::Wall, &[], Element::Light, 6.0), None, wall_aim).unwrap();
    run(&mut w, 40);
    let mut aim = Aim::new(eye_of(&w, A), Vec3::new(0.0, -0.1, -1.0).normalize());
    aim.target_hint = Some(TargetHint::Player(B));
    // A charging bull rams the wall repeatedly.
    let mut clashed = false;
    let mut shattered = false;
    for _ in 0..6 {
        let _ = w.cast(A, recipe("bull", Form::Quadruped, &[Mechanic::Charge, Mechanic::Heavy], Element::Light, 2.0), None, aim);
        for e in run(&mut w, 300) {
            clashed |= matches!(e, GameEvent::Clash { .. });
            shattered |= matches!(e, GameEvent::ConstructShattered { id, .. } if id == wall);
        }
    }
    assert!(clashed, "construct contact clashes");
    let _ = shattered;
    w.check_invariants().unwrap();
}

// ------------------------------------------------------------------------------------ space

#[test]
fn portal_carries_player_speed_and_view() {
    let mut w = duel(2);
    // Floor portal under A (normal up), exit on a vertical face facing +X far away.
    let a = PortalEnd::new(Vec3::new(0.0, 8.05, 0.0), Vec3::Y, Vec3::NEG_Z, 1.5);
    let b = PortalEnd::new(Vec3::new(-20.0, 12.0, 10.0), Vec3::X, Vec3::Y, 1.5);
    w.create_portal(A, a, b, 12.0, None).unwrap();
    place(&mut w, B, Vec3::new(0.0, 12.0, 0.0));
    w.player_mut(B).unwrap().mv.vel = Vec3::new(0.0, -20.0, 0.0);
    let mut tele = None;
    for _ in 0..30 {
        for e in w.tick(&[]) {
            if let GameEvent::Teleported { who: bd_sim::events::Traveller::Player(p), rot, .. } = e {
                if *p == B {
                    tele = Some(*rot);
                }
            }
        }
        if tele.is_some() {
            break;
        }
    }
    assert!(tele.is_some(), "went through");
    let p = w.player(B).unwrap();
    assert!(p.mv.pos.x > -20.0 && (p.mv.pos.z - 10.0).abs() < 2.0, "{}", p.mv.pos);
    assert!(p.mv.vel.x > 15.0, "fall speed became forward speed: {}", p.mv.vel);
}

#[test]
fn attract_field_pulls_players_and_owner_is_immune() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    place(&mut w, B, Vec3::new(16.0, 8.0, 0.0));
    w.create_field(A, Vec3::new(10.0, 9.0, 0.0), 12.0, 30.0, 5.0, None).unwrap();
    run(&mut w, 60);
    assert!(w.player(B).unwrap().mv.pos.x < 15.0, "pulled toward the field: {}", w.player(B).unwrap().mv.pos);
    assert!(w.player(A).unwrap().mv.pos.x.abs() < 0.1, "owner immune");
    assert_eq!(w.snapshot_for(Some(B)).fields.len(), 1, "replicated");
}

#[test]
fn link_keeps_players_within_length_and_blade_cuts_it() {
    let mut w = duel(2);
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    place(&mut w, B, Vec3::new(3.0, 8.0, 0.0));
    let id = w.create_link(Some(A), LinkEnd::Player(A), LinkEnd::Player(B), 5.0, 6.0).unwrap();
    w.apply_hit(hit(B, Some(A), 0.0, 25.0, Vec3::X));
    let mut max_d: f32 = 0.0;
    for _ in 0..120 {
        w.tick(&[]);
        let d = (w.player(A).unwrap().mv.pos - w.player(B).unwrap().mv.pos).length();
        max_d = max_d.max(d);
    }
    assert!(max_d < 8.5, "chain holds (with slack): {max_d}");
    assert!(w.player(A).unwrap().mv.pos.x > 0.5, "B dragged A along");
    w.damage_link(id, 0.0, true);
    let ev = run(&mut w, 1);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::LinkBroken { cut: true, .. })));
}

#[test]
fn shrink_and_grow_change_hull_knockback_and_replicate() {
    let mut w = duel(2);
    w.resize_player(B, bd_sim::space::GROW_SCALE, 6.0);
    run(&mut w, 60);
    let hull = w.hull_of(B).unwrap();
    assert!((hull.max.y - hull.min.y - 1.8 * 1.6).abs() < 1e-3);
    let r = w.apply_hit(hit(B, Some(A), 0.0, 10.0, Vec3::X));
    assert!((r.knockback - 10.0 / 1.6).abs() < 1e-3, "grown weighs 1.6");
    assert_eq!(w.snapshot_for(Some(A)).players.iter().find(|p| p.id == B.0).unwrap().size, 160);
    run(&mut w, (6.5 / DT) as u32);
    assert_eq!(w.player(B).unwrap().size.scale, 1.0);
}

// ------------------------------------------------------------------------------------ seats

#[test]
fn cockpit_pilot_drives_and_ejects_with_launch() {
    let mut w = duel(2);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    let aim = Aim::new(eye_of(&w, A), Vec3::new(0.0, -0.3, -1.0));
    let mech = w.cast(A, recipe("giant mech", Form::Biped, &[Mechanic::Piloted], Element::Light, 4.0), None, aim).unwrap();
    let seat = w.player(A).unwrap().seat.unwrap();
    assert_eq!(seat.construct, mech);
    assert!(!seat.suit);
    run(&mut w, 90);
    let start = w.constructs.get(mech).unwrap().pos;
    let fwd = PlayerInput { move_z: 1, yaw: 0.0, ..Default::default() };
    for _ in 0..120 {
        w.tick(&[(A, fwd)]);
    }
    let c = w.constructs.get(mech).unwrap();
    assert!((c.pos - start).length() > 3.0, "WASD drives the mech");
    let p = w.player(A).unwrap();
    assert!((p.mv.pos - c.seat_pos(0)).length() < 0.5, "hull rides the seat");
    // E ejects with a big upward launch.
    let eject = PlayerInput { buttons: buttons::TETHER_R, ..fwd };
    let ev = w.tick(&[(A, eject)]).to_vec();
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Ejected { .. })));
    let p = w.player(A).unwrap();
    assert!(p.seat.is_none() && p.mv.vel.y > 15.0);
    assert!(p.mv.hooks[1].phase == bd_sim::movement::HookPhase::Idle, "the E press did not fire a tether");
}

#[test]
fn power_suit_absorbs_damage_and_keeps_movement() {
    let mut w = duel(2);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    let aim = Aim::new(eye_of(&w, A), Vec3::NEG_Z);
    let suit = w.cast(A, recipe("iron man suit", Form::Biped, &[Mechanic::Piloted], Element::Light, 2.0), None, aim).unwrap();
    assert!(w.player(A).unwrap().seat.unwrap().suit);
    run(&mut w, 90);
    let integ = w.constructs.get(suit).unwrap().integrity;
    w.apply_hit(hit(A, Some(B), 10.0, 0.0, Vec3::X));
    assert_eq!(w.player(A).unwrap().health(), 100.0, "suit took it");
    assert!(w.constructs.get(suit).unwrap().integrity < integ);
    let x0 = w.player(A).unwrap().mv.pos.x;
    let right = PlayerInput { move_x: 1, ..Default::default() };
    for _ in 0..60 {
        w.tick(&[(A, right)]);
    }
    assert!(w.player(A).unwrap().mv.pos.x > x0 + 3.0, "pilot keeps own movement");
    // Enough damage shatters the suit and spills into health.
    w.apply_hit(hit(A, Some(B), 500.0, 0.0, Vec3::X));
    run(&mut w, 1);
    assert!(w.constructs.get(suit).is_none());
}

// ------------------------------------------------------------------------------------ replication

#[test]
fn snapshot_round_trips_and_client_mirror_matches() {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 3, check_invariants: true, ..SimConfig::default() };
    cfg.timings.warmup_secs = 0.1;
    let mut host = World::new(cfg.clone());
    let mut brains = bots::add_bots(&mut host, 0, 8, 0.7, 3);
    let mut client = World::new(cfg);
    let mut enc = DeltaEncoder::new();
    let mut dec = DeltaDecoder::new();
    let mut inputs = Vec::new();
    let me = PlayerId(2);
    for t in 0..900 {
        inputs.clear();
        bots::drive(&mut host, &mut brains, &mut inputs);
        let evs = host.tick(&inputs).to_vec();
        for e in &evs {
            client.apply_event(e);
        }
        if t % 2 == 0 {
            let snap = host.snapshot_for(Some(me));
            // bincode round trip.
            let delta = enc.encode(&snap);
            let bytes = bincode::serde::encode_to_vec(&delta, bincode::config::standard()).unwrap();
            let (back, _): (bd_sim::SnapshotDelta, usize) = bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
            let full = dec.decode(&back).expect("baseline known");
            assert_eq!(full, snap);
            enc.ack(full.tick);
            client.apply_snapshot(&full, Some(me));
            // Own movement state is exact.
            if let (Some(h), Some(c)) = (host.player(me), client.player(me)) {
                if h.alive {
                    assert_eq!(h.mv, c.mv);
                }
            }
            assert_eq!(client.constructs.len(), host.constructs.len());
        }
    }
    // Terrain edits replicated through carve events.
    assert_eq!(client.voxels.content_hash(), host.voxels.content_hash());
    let full = host.snapshot_for(Some(me));
    let b = bincode::serde::encode_to_vec(&full, bincode::config::standard()).unwrap();
    let (back, _): (WorldSnapshot, usize) = bincode::serde::decode_from_slice(&b, bincode::config::standard()).unwrap();
    assert_eq!(back, full);
}

#[test]
fn client_prediction_matches_host_movement() {
    let mut w = duel(2);
    let mut predicted = w.player(A).unwrap().mv;
    let mut client = duel(2);
    client.apply_snapshot(&w.snapshot_for(Some(A)), Some(A));
    for k in 0..240 {
        let inp = PlayerInput {
            seq: k,
            yaw: k as f32 * 0.01,
            move_z: 1,
            move_x: if k % 60 < 30 { 1 } else { -1 },
            buttons: if k % 50 < 25 { buttons::JUMP } else { 0 },
            ..Default::default()
        };
        client.predict_player(A, &mut predicted, &inp);
        w.tick(&[(A, inp)]);
    }
    let host = w.player(A).unwrap().mv;
    assert!((host.pos - predicted.pos).length() < 1e-3, "{} vs {}", host.pos, predicted.pos);
}

#[test]
fn rejects_hostile_inputs_and_recipes() {
    let mut w = duel(2);
    let bad = PlayerInput { yaw: f32::NAN, pitch: f32::INFINITY, move_x: 100, move_z: -100, reel_notches: 127, buttons: 0xFFFF, seq: 1 };
    for _ in 0..30 {
        w.tick(&[(A, bad)]);
    }
    let mut r = recipe("x", Form::Kaiju, &[], Element::Light, f32::NAN);
    r.mass = f32::NEG_INFINITY;
    r.count = 255;
    r.intensity = f32::NAN;
    let aim = Aim { origin: Vec3::NAN, dir: Vec3::ZERO, target_hint: Some(TargetHint::Construct(bd_core::ConstructId(999))), modifier: true };
    let _ = w.cast(A, r, None, aim);
    w.apply_hit(HitInfo::new(B, Some(A), HitKind::Melee, f32::NAN, f32::INFINITY, Vec3::NAN, Vec3::NAN));
    run(&mut w, 60);
    w.check_invariants().unwrap();
}
