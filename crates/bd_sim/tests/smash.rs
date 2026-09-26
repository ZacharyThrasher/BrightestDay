//! The Smash model (COMBAT.md §1a): damage %, launches, blast-zone KOs (side / bottom / star),
//! DI, tech, stock matches, punch-through.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::{IVec3, PlayerId, Vec3};
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::{GameEvent, KoCause};
use bd_sim::rules::{MatchPhase, Scoring};
use bd_sim::smash::BlastZone;
use bd_sim::world::{ArenaSource, SimConfig, World};
use bd_voxel::mat;

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const C: PlayerId = PlayerId(3);
const G: f32 = 8.0;

fn world_with(n: u8, f: impl FnOnce(&mut SimConfig)) -> World {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    f(&mut cfg);
    let mut w = World::new(cfg);
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

fn world(n: u8) -> World {
    world_with(n, |_| {})
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
        assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
    }
    all
}

/// Fill a box of voxels (world metres, inclusive-ish) with `m`.
fn wall(w: &mut World, lo: Vec3, hi: Vec3, m: u8) {
    let a = (lo / 0.5).floor().as_ivec3();
    let b = (hi / 0.5).ceil().as_ivec3() - IVec3::ONE;
    w.voxels.fill_box(a, b, m);
}

#[test]
fn damage_adds_percent_and_heavy_hits_launch_off_the_map() {
    let mut w = world(2);
    place(&mut w, B, Vec3::new(12.0, G, 0.0));
    w.apply_hit(HitInfo::new(B, Some(A), HitKind::Melee, 12.0, 5.0, Vec3::X, Vec3::ZERO));
    assert!((w.player(B).unwrap().damage_pct - 12.0).abs() < 1e-3);
    assert!(w.player(B).unwrap().alive, "no health KOs");
    // At 300 % a heavy blow sends B out of the side blast zone.
    run(&mut w, 60);
    place(&mut w, B, Vec3::new(12.0, G, 0.0));
    w.player_mut(B).unwrap().damage_pct = 300.0;
    w.apply_hit(HitInfo::new(B, Some(A), HitKind::HeavyMelee, 30.0, 28.0, Vec3::X, Vec3::ZERO));
    assert!(w.events().iter().any(|e| matches!(e, GameEvent::Launched { player, .. } if *player == B)));
    let ev = run(&mut w, 600);
    let ko = ev.iter().find_map(|e| match e {
        GameEvent::Ko { victim, killer, cause, zone, pct, .. } if *victim == B => Some((*killer, *cause, *zone, *pct)),
        _ => None,
    });
    let (killer, cause, zone, pct) = ko.expect("launched out");
    assert_eq!(killer, Some(A));
    assert!(matches!(cause, KoCause::Void | KoCause::BlastZone));
    assert!(zone.is_some() && pct >= 300.0);
    // Respawn at 0 %.
    run(&mut w, 400);
    assert_eq!(w.player(B).unwrap().damage_pct, 0.0);
}

#[test]
fn straight_up_is_a_star_ko() {
    let mut w = world(2);
    place(&mut w, B, Vec3::new(12.0, G + 2.0, 0.0));
    w.launch_player(B, Vec3::new(0.0, 70.0, 0.0), Some(A));
    let ev = run(&mut w, 400);
    let star = ev.iter().find_map(|e| match e {
        GameEvent::Ko { victim, zone: Some(BlastZone::Top), poof_delay, killer, .. } if *victim == B => Some((*poof_delay, *killer)),
        _ => None,
    });
    let (delay, killer) = star.expect("star KO");
    assert!(delay > 0.0);
    assert_eq!(killer, Some(A));
}

#[test]
fn di_bends_the_launch_up_to_the_limit() {
    let fly = |move_x: i8| {
        let mut w = world(2);
        place(&mut w, B, Vec3::new(0.0, G + 1.0, 20.0));
        w.launch_player(B, Vec3::new(0.0, 12.0, -40.0), Some(A));
        let inp = PlayerInput { yaw: 0.0, move_x, ..Default::default() };
        for _ in 0..30 {
            w.tick(&[(B, inp)]);
        }
        w.player(B).unwrap().mv.pos
    };
    let straight = fly(0);
    let right = fly(1);
    let left = fly(-1);
    assert!(right.x > straight.x + 1.0, "DI right: {right} vs {straight}");
    assert!(left.x < straight.x - 1.0, "DI left");
    // Bounded: ±15° of a ~40 m/s launch over 0.25 s is < 3 m sideways (plus a little air control).
    assert!((right.x - straight.x).abs() < 4.0);
}

#[test]
fn tech_cancels_the_wall_splat() {
    let go = |tech: bool| {
        let mut w = world(2);
        // The lab's east wall: x ∈ [24, 25], z ∈ [-6, 6], y ∈ [8, 12].
        place(&mut w, B, Vec3::new(21.5, G + 0.5, 0.0));
        w.launch_player(B, Vec3::new(20.0, 1.0, 0.0), Some(A));
        let mut ev = Vec::new();
        for k in 0..40 {
            let b = if tech && k == 1 { buttons::JUMP } else { 0 };
            ev.extend_from_slice(w.tick(&[(B, PlayerInput { buttons: b, ..Default::default() })]));
        }
        let splat = ev.iter().any(|e| matches!(e, GameEvent::WallSplat { player, .. } if *player == B));
        let teched = ev.iter().any(|e| matches!(e, GameEvent::Tech { player, .. } if *player == B));
        (splat, teched, w.player(B).unwrap().damage_pct)
    };
    let (splat, teched, pct) = go(false);
    assert!(splat && !teched && pct > 0.0, "no tech: splat adds %");
    let (splat, teched, pct) = go(true);
    assert!(!splat && teched && pct == 0.0, "tech: no splat");
}

#[test]
fn stock_match_ends_when_one_player_is_left() {
    let mut w = world_with(3, |c| c.rules.scoring = Scoring::Stock { lives: 2 });
    assert_eq!(w.player(B).unwrap().stocks, Some(2));
    for victim in [B, B, C] {
        w.apply_hit(HitInfo::new(victim, Some(A), HitKind::RingBlast, 1.0, 0.0, Vec3::X, Vec3::ZERO));
        w.player_mut(victim).unwrap().mv.pos.y = -100.0;
        run(&mut w, 1);
        if w.match_state.phase == MatchPhase::Live {
            run(&mut w, 400); // respawn
        }
    }
    assert!(w.player(B).unwrap().eliminated);
    assert!(!w.player(B).unwrap().alive, "eliminated players stay out");
    assert_eq!(w.match_state.phase, MatchPhase::Live, "C still has a stock");
    w.apply_hit(HitInfo::new(C, Some(A), HitKind::RingBlast, 1.0, 0.0, Vec3::X, Vec3::ZERO));
    w.player_mut(C).unwrap().mv.pos.y = -100.0;
    let ev = run(&mut w, 1);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Eliminated { player } if *player == C)));
    assert_eq!(w.match_state.phase, MatchPhase::Ended);
    assert_eq!(w.match_state.winner, Some(A));
}

/// Launch B along +Z at `speed` into whatever `setup` built; return (events, final z, final speed).
fn punch(speed: f32, setup: impl FnOnce(&mut World)) -> (Vec<GameEvent>, f32, f32) {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(-30.0, G, -30.0));
    setup(&mut w);
    place(&mut w, B, Vec3::new(12.0, G + 3.0, 0.0));
    w.launch_player(B, Vec3::new(0.0, 1.0, speed), Some(A));
    let mut ev = Vec::new();
    let mut max_speed_after_wall = 0.0f32;
    for _ in 0..90 {
        ev.extend_from_slice(w.tick(&[]));
        assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
        let p = w.player(B).unwrap();
        if p.mv.pos.z > 17.0 {
            max_speed_after_wall = max_speed_after_wall.max(p.mv.vel.length());
        }
    }
    let z = w.player(B).unwrap().mv.pos.z;
    (ev, z, max_speed_after_wall)
}

#[test]
fn punch_through_a_stone_wall_and_out_the_other_side_slower() {
    let (ev, z, after) = punch(60.0, |w| wall(w, Vec3::new(6.0, G, 10.0), Vec3::new(18.0, G + 12.0, 16.0), mat::STONE));
    let pt = ev.iter().find_map(|e| match e {
        GameEvent::PunchThrough { player, entry, exit, material } if *player == B => Some((*entry, *exit, *material)),
        _ => None,
    });
    let (entry, exit, material) = pt.expect("punched through");
    assert_eq!(material, mat::STONE);
    assert!(entry.z > 8.5 && entry.z < 11.0 && exit.z > 15.5, "{entry} → {exit}");
    assert!(z > 16.0, "came out the far side");
    assert!(after > 22.0 && after < 58.0, "slower but still flying: {after}");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Carve { .. })), "replicated carves");
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::WallSplat { .. })));
}

#[test]
fn punch_through_stops_inside_a_thick_block_and_splats() {
    let (ev, z, _) = punch(60.0, |w| wall(w, Vec3::new(6.0, G, 10.0), Vec3::new(18.0, G + 12.0, 40.0), mat::STONE));
    assert!(z < 30.0, "stopped inside the 30 m block at {z}");
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::PunchThrough { .. })), "never came out");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::WallSplat { player, .. } if *player == B)), "splat when it stops");
}

#[test]
fn indestructible_cores_always_stop_you() {
    let (ev, z, _) = punch(80.0, |w| wall(w, Vec3::new(6.0, G, 10.0), Vec3::new(18.0, G + 12.0, 12.0), mat::CORE));
    assert!(z < 10.0, "{z}");
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Carve { op } if matches!(op.shape, bd_voxel::CarveShape::Capsule { .. }))), "never carved");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::WallSplat { player, .. } if *player == B)));
}

#[test]
fn punch_through_is_deterministic_and_respects_the_carve_budget() {
    let run_once = || {
        let mut w = world_with(3, |c| c.tuning.smash.punch_max_carves_per_tick = 1);
        wall(&mut w, Vec3::new(4.0, G, 10.0), Vec3::new(20.0, G + 12.0, 16.0), mat::STONE);
        place(&mut w, B, Vec3::new(8.0, G + 3.0, 0.0));
        place(&mut w, C, Vec3::new(16.0, G + 3.0, 0.0));
        w.launch_player(B, Vec3::new(0.0, 1.0, 60.0), Some(A));
        w.launch_player(C, Vec3::new(0.0, 1.0, 60.0), Some(A));
        let mut worst = 0;
        let mut h = bd_sim::rng::Fnv64::default();
        for _ in 0..90 {
            let ev = w.tick(&[]);
            let capsules = ev.iter().filter(|e| matches!(e, GameEvent::Carve { op } if matches!(op.shape, bd_voxel::CarveShape::Capsule { .. }))).count();
            worst = worst.max(capsules);
            h.write_u64(w.state_hash());
        }
        (worst, h.0, w.voxels.content_hash())
    };
    let a = run_once();
    let b = run_once();
    assert!(a.0 <= 1, "punch carves per tick: {}", a.0);
    assert_eq!(a, b);
}
