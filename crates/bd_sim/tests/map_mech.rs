//! MAP MECH (world/map_mech.rs): transforming panels + the permanent space bridge.
//!
//! - Deterministic: identical worlds + inputs → identical mover event streams.
//! - Telegraphed: every move is preceded by ≥ `WARN_SECS` of warning, and lasts `MOVE_SECS`.
//! - Never crushes / shoves / drops: a panel never solidifies onto a player, never retracts from
//!   under one, and a player standing on or next to it is never moved by it.
//! - Host/prediction parity: a client mirroring the replicated events builds the same mover
//!   colliders as the host and predicts a run across the extended panel exactly.
//! - The space bridge is permanent (survives rounds, explosions, invariant checks) and carries
//!   players across.

use bd_core::corps::Corps;
use bd_core::{PlayerId, Quat, TICK_HZ, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::movement::testkit::input;
use bd_sim::world::map_mech::{MAP_OWNER, MoverPhase, is_mover};
use bd_sim::{SimConfig, World};
use bd_voxel::mechanics::{MOVE_SECS, WARN_SECS};
use bd_voxel::{MapMover, MapPortal, MoverPart};

const A: PlayerId = PlayerId(1);

fn ticks(s: f32) -> i64 {
    (s * TICK_HZ as f32).round() as i64
}

/// Flat 96 m lab with one mover: a 10 × 6 m slab whose top is flush 1 m above the floor at x = 10
/// (a low step — walkable onto with a hop), folded by swinging it up on a hinge at its west edge.
fn make(with_portal: bool) -> (World, f32) {
    let (v, mut info) = bd_voxel::generate_flat_test(96.0);
    let floor = info.spawns[0].y;
    info.mechanics.movers.push(MapMover {
        name: "test_slab",
        parts: vec![MoverPart {
            center: Vec3::new(10.0, floor + 0.6, 0.0),
            half: Vec3::new(5.0, 0.4, 3.0),
            rot: Quat::IDENTITY,
            hinge: Vec3::new(5.0, floor + 1.0, 0.0),
            hinge_axis: Vec3::Z,
            fold_angle: 1.4,
            fold_offset: Vec3::ZERO,
        }],
        folded_secs: 2.0,
        extended_secs: 3.0,
        offset_secs: 0.5,
    });
    if with_portal {
        info.mechanics.portals.push(MapPortal {
            a: Vec3::new(-20.0, floor + 3.0, 20.0),
            a_normal: Vec3::X,
            b: Vec3::new(20.0, floor + 3.0, -20.0),
            b_normal: Vec3::NEG_X,
            radius: 2.5,
        });
    }
    (World::with_arena(SimConfig { check_invariants: true, ..SimConfig::default() }, v, info, 1), floor)
}

fn live(w: &mut World) {
    w.add_player(A, "a".into(), Corps::from_index(1)).unwrap();
    w.start_live();
    w.tick(&[]);
    w.player_mut(A).unwrap().spawn_shield = 0.0;
}

fn park(w: &mut World, pos: Vec3) {
    let p = w.player_mut(A).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
}

fn mover_events(evs: &[GameEvent]) -> Vec<(u8, MoverPhase, i64)> {
    evs.iter().filter_map(|e| if let GameEvent::MapMover { mover, phase, since } = e { Some((*mover, *phase, *since)) } else { None }).collect()
}

#[test]
fn movers_cycle_deterministically_and_always_telegraph() {
    let run = || {
        let (mut w, floor) = make(false);
        live(&mut w);
        let mut log = Vec::new();
        for k in 0..40 * TICK_HZ {
            // Wander back and forth across the slab (sometimes on it, sometimes not).
            let t = k as f32 / TICK_HZ as f32;
            let yaw = if (t * 0.25).fract() < 0.5 { -std::f32::consts::FRAC_PI_2 } else { std::f32::consts::FRAC_PI_2 };
            let i = input(yaw, 0.0, 0, 1, if k % 97 == 0 { bd_sim::movement::testkit::JUMP } else { 0 });
            let evs = w.tick(&[(A, i)]).to_vec();
            log.extend(mover_events(&evs));
            let _ = floor;
        }
        log
    };
    let (a, b) = (run(), run());
    assert_eq!(a, b, "mover events must be deterministic");
    assert!(a.iter().filter(|e| e.1 == MoverPhase::Extended).count() >= 3, "the slab should extend repeatedly: {a:?}");
    // Order + durations: WarnExtend → Extending (≥ warn) → Extended (≥ move) → WarnRetract →
    // Retracting (≥ warn) → Folded (≥ move).
    let order = [MoverPhase::Folded, MoverPhase::WarnExtend, MoverPhase::Extending, MoverPhase::Extended, MoverPhase::WarnRetract, MoverPhase::Retracting];
    for w in a.windows(2) {
        let (p0, p1) = (w[0].1, w[1].1);
        let i0 = order.iter().position(|&p| p == p0).unwrap();
        assert_eq!(order[(i0 + 1) % order.len()], p1, "phase order {p0:?} → {p1:?}");
        let dt = w[1].2 - w[0].2;
        match p1 {
            MoverPhase::Extending | MoverPhase::Retracting => assert!(dt >= ticks(WARN_SECS), "{p1:?} after only {dt} ticks of warning"),
            MoverPhase::Extended | MoverPhase::Folded => assert!(dt >= ticks(MOVE_SECS), "{p1:?} after a {dt}-tick move"),
            _ => {}
        }
    }
}

#[test]
fn a_panel_never_solidifies_onto_a_player() {
    let (mut w, floor) = make(false);
    live(&mut w);
    // Stand inside where the slab will appear (its volume is floor+0.2 .. floor+1.0).
    let inside = Vec3::new(10.0, floor, 0.0);
    let mut blocked_ticks = 0;
    for _ in 0..(10 * TICK_HZ) {
        park(&mut w, inside);
        w.tick(&[(A, input(0.0, 0.0, 0, 0, 0))]);
        let ph = w.map_mech.movers[0].phase;
        assert!(!ph.solid(), "the slab solidified onto a player ({ph:?})");
        assert!(!w.colliders().iter().any(|c| is_mover(c.id)), "a mover collider appeared on a player");
        if ph == MoverPhase::Extending {
            blocked_ticks += 1;
        }
    }
    assert!(blocked_ticks > ticks(MOVE_SECS) as u32, "the slab should have been waiting to lock");
    // Step away: it locks in right after.
    park(&mut w, Vec3::new(30.0, floor, 20.0));
    for _ in 0..3 {
        w.tick(&[(A, input(0.0, 0.0, 0, 0, 0))]);
    }
    assert_eq!(w.map_mech.movers[0].phase, MoverPhase::Extended);
}

#[test]
fn a_panel_never_retracts_from_under_a_player_and_never_moves_them() {
    let (mut w, floor) = make(false);
    live(&mut w);
    park(&mut w, Vec3::new(30.0, floor, 20.0));
    // Wait for the lock.
    for _ in 0..(8 * TICK_HZ) {
        w.tick(&[(A, input(0.0, 0.0, 0, 0, 0))]);
        if w.map_mech.movers[0].phase == MoverPhase::Extended {
            break;
        }
    }
    assert_eq!(w.map_mech.movers[0].phase, MoverPhase::Extended);
    // Stand on top of the slab, idle, for far longer than its hold + warning.
    let top = Vec3::new(10.0, floor + 1.0, 0.0);
    park(&mut w, top);
    for _ in 0..30 {
        w.tick(&[(A, input(0.0, 0.0, 0, 0, 0))]);
    }
    let settled = w.player(A).unwrap().mv.pos;
    assert!((settled - top).length() < 0.05, "standing on the slab: {settled}");
    for _ in 0..(12 * TICK_HZ) {
        w.tick(&[(A, input(0.0, 0.0, 0, 0, 0))]);
        let ph = w.map_mech.movers[0].phase;
        assert!(ph.solid(), "retracted from under a player ({ph:?})");
        let p = w.player(A).unwrap().mv.pos;
        assert!((p - settled).length() < 1e-3, "the slab moved its rider: {p}");
    }
    assert_eq!(w.map_mech.movers[0].phase, MoverPhase::WarnRetract, "held in its warning state while occupied");
    // Walk off; it retracts after that.
    park(&mut w, Vec3::new(30.0, floor, 20.0));
    for _ in 0..3 {
        w.tick(&[(A, input(0.0, 0.0, 0, 0, 0))]);
    }
    assert_eq!(w.map_mech.movers[0].phase, MoverPhase::Retracting);
}

#[test]
fn prediction_matches_the_host_across_a_transforming_panel() {
    let (mut host, floor) = make(false);
    live(&mut host);
    let (mut client, _) = make(false);
    let mut evs = host.tick(&[]).to_vec();
    for e in &evs {
        client.apply_event(e);
    }
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    let (mut bad, mut on_slab, mut checked_cols) = (0, 0, 0);
    for k in 0..30 * TICK_HZ {
        // Run east–west over the slab, hopping.
        let t = k as f32 / TICK_HZ as f32;
        let yaw = if (t * 0.2).fract() < 0.5 { -std::f32::consts::FRAC_PI_2 } else { std::f32::consts::FRAC_PI_2 };
        let i = input(yaw, 0.0, 0, 1, if k % 60 == 0 { bd_sim::movement::testkit::JUMP } else { 0 });
        let mut s = client.player(A).unwrap().mv;
        client.predict_player(A, &mut s, &i);
        evs = host.tick(&[(A, i)]).to_vec();
        for e in &evs {
            client.apply_event(e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        let h = host.player(A).unwrap().mv;
        if (h.pos - s.pos).length() > 1e-3 {
            bad += 1;
        }
        if (h.pos.y - (floor + 1.0)).abs() < 0.05 && h.pos.x > 5.0 && h.pos.x < 15.0 {
            on_slab += 1;
        }
        // The client's mover colliders (from replicated events) equal the host's.
        let hc: Vec<_> = host.colliders().iter().filter(|c| is_mover(c.id)).map(|c| (c.id, c.center, c.half, c.rot)).collect();
        let cc: Vec<_> = client.colliders().iter().filter(|c| is_mover(c.id)).map(|c| (c.id, c.center, c.half, c.rot)).collect();
        assert_eq!(hc, cc, "mover colliders differ at tick {}", host.tick);
        checked_cols += hc.len();
        assert_eq!(client.map_mech, host.map_mech, "mover state differs at tick {}", host.tick);
    }
    assert!(on_slab > 20, "the run should cross the extended slab ({on_slab} ticks on it)");
    assert!(checked_cols > 0);
    assert_eq!(bad, 0, "{bad} ticks of prediction mismatch");
}

#[test]
fn the_space_bridge_is_permanent_and_carries_players() {
    let (mut w, floor) = make(true);
    live(&mut w);
    let pair = |w: &World| w.portals.iter().find(|p| p.owner == MAP_OWNER).cloned();
    assert!(pair(&w).is_some(), "space bridge installed");
    // Survives time, a round reset and explosions (invariants on: debug-asserts every tick).
    for _ in 0..(20 * TICK_HZ) {
        w.tick(&[]);
    }
    w.start_live();
    w.tick(&[]);
    let p = pair(&w).expect("space bridge survives a round reset");
    assert_eq!(w.portals.iter().filter(|p| p.owner == MAP_OWNER).count(), 1);
    // Walk into gate A from its front: come out of gate B.
    w.player_mut(A).unwrap().spawn_shield = 0.0;
    let a = p.ends[0];
    park(&mut w, Vec3::new(a.pos.x + 2.0, floor, a.pos.z));
    let mut through = false;
    for _ in 0..TICK_HZ {
        w.tick(&[(A, input(std::f32::consts::FRAC_PI_2, 0.0, 0, 1, 0))]);
        let pos = w.player(A).unwrap().mv.pos;
        if (pos - p.ends[1].pos).length() < 6.0 {
            through = true;
            break;
        }
    }
    assert!(through, "walking into the Iacon gate should come out of the Kaon gate");
}

#[test]
fn every_cybertron_mover_cycles_with_players_around() {
    let (v, m) = bd_voxel::maps::generate_cybertron(1);
    let mut w = World::with_arena(SimConfig { check_invariants: true, ..SimConfig::default() }, v, m.arena, 1);
    for k in 0..4u8 {
        w.add_player(PlayerId(k + 1), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    let n = w.arena.mechanics.movers.len();
    assert_eq!(n, 5);
    assert_eq!(w.portals.iter().filter(|p| p.owner == MAP_OWNER).count(), 1);
    let mut extended = vec![0u32; n];
    for _ in 0..(90 * TICK_HZ) {
        let evs = w.tick(&[]).to_vec();
        for (i, ph, _) in mover_events(&evs) {
            if ph == MoverPhase::Extended {
                extended[i as usize] += 1;
            }
        }
    }
    for (i, m) in w.arena.mechanics.movers.iter().enumerate() {
        assert!(extended[i] >= 2, "{} extended only {} times in 90 s", m.name, extended[i]);
    }
}
