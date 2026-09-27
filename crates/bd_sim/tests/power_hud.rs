//! POWER HUD: readiness / forms / own timers derived from sim state and replicated to clients.

use bd_core::corps::Corps;
use bd_core::{PlayerId, Vec3};
use bd_sim::powers::{move_sel, PowerTable};
use bd_sim::readiness::{OwnHud, ReadySnap, GUARD_TIMER, SIZE_TIMER};
use bd_sim::replicate::{SnapshotDelta, WorldSnapshot};
use bd_sim::status::StatusKind;
use bd_sim::world::{ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn duel() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for (k, p) in w.players.iter_mut().enumerate() {
        p.spawn_shield = 0.0;
        p.mv.pos = Vec3::new(k as f32 * 6.0, 8.0, 0.0);
    }
    w
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

fn ready_of(w: &World, id: PlayerId) -> ReadySnap {
    w.snapshot_for(Some(id)).players.iter().find(|p| p.id == id.0).unwrap().ready
}

fn full(w: &mut World, id: PlayerId) {
    let pool = w.rules().willpower_max();
    w.player_mut(id).unwrap().willpower = pool;
}

#[test]
fn cooldown_fraction_counts_down_to_a_ready_edge() {
    let mut w = duel();
    assert_eq!(ready_of(&w, A), ReadySnap::default(), "holding nothing: all zero");
    assert!(w.grant_power(A, "star_platinum"));
    full(&mut w, A);
    assert!(ready_of(&w, A).is_ready(0));
    // Star finger: 1.5 s.
    w.power_move(A, move_sel::TAP).unwrap();
    let r = ready_of(&w, A);
    assert!(!r.is_ready(0));
    assert!(r.ready(0) < 0.02, "just fired: {}", r.ready(0));
    run(&mut w, 90); // 0.75 s
    let r = ready_of(&w, A);
    assert!((r.ready(0) - 0.5).abs() < 0.03, "half way: {}", r.ready(0));
    run(&mut w, 89);
    assert!(!ready_of(&w, A).is_ready(0), "rounded up: never ready early");
    run(&mut w, 2);
    assert!(ready_of(&w, A).is_ready(0));
}

#[test]
fn slowest_move_and_every_held_power_count() {
    let mut w = duel();
    for k in ["star_platinum", "mera_mera", "hie_hie"] {
        assert!(w.grant_power(A, k), "{k}");
    }
    full(&mut w, A);
    let held: Vec<u16> = w.player(A).unwrap().power.held().map(|h| h.id).collect();
    assert_eq!(held.len(), 3);
    let sp = PowerTable::get().index_of("star_platinum").unwrap();
    let k = held.iter().position(|h| *h == sp).unwrap();
    // Fire a move of a power that isn't active: its own readiness drops, the others stay ready.
    w.power_move(A, move_sel::move_of(k, move_sel::HOLD)).unwrap();
    let r = ready_of(&w, A);
    for j in 0..3 {
        assert_eq!(r.is_ready(j), j != k, "slot {j}: {r:?}");
    }
    // The time stop (20 s) dominates over the short tap.
    run(&mut w, 240);
    assert!(r.ready(k) < ready_of(&w, A).ready(k) && ready_of(&w, A).ready(k) < 0.15);
}

#[test]
fn time_stop_drains_as_a_form_and_techniques_burn() {
    let mut w = duel();
    assert!(w.grant_power(A, "the_world"));
    full(&mut w, A);
    assert_eq!(ready_of(&w, A).form(0), None);
    w.power_move(A, move_sel::HOLD).unwrap(); // za warudo, 1.5 s
    let f0 = ready_of(&w, A).form(0).expect("timed form");
    assert!(f0 > 0.95, "{f0}");
    run(&mut w, 90);
    let f1 = ready_of(&w, A).form(0).expect("still on");
    assert!((f1 - 0.5).abs() < 0.05, "{f1}");
    run(&mut w, 100);
    assert_eq!(ready_of(&w, A).form(0), None, "over");
    // An awakened technique is a sustained form.
    assert!(w.grant_power(B, "ultra_instinct"));
    assert_eq!(ready_of(&w, B).form(0), Some(1.0));
}

#[test]
fn own_timers_for_the_recipient_only() {
    let mut w = duel();
    assert!(w.snapshot_for(Some(A)).own.unwrap().hud.is_none(), "nothing running: no bytes");
    {
        let p = w.player_mut(A).unwrap();
        p.status.apply(StatusKind::Chill, 1.5, None);
        p.size.apply(bd_sim::space::SHRINK_SCALE, 6.0);
        p.juggle.immune = 0.8;
    }
    let hud: OwnHud = w.snapshot_for(Some(A)).own.unwrap().hud.expect("timers");
    assert!((hud.secs(StatusKind::Chill as usize) - 1.5).abs() < 0.11);
    assert!((hud.secs(SIZE_TIMER) - 6.0).abs() < 0.11);
    assert!((hud.secs(GUARD_TIMER) - 0.8).abs() < 0.11);
    assert_eq!(hud.secs(StatusKind::Burn as usize), 0.0);
    // B's snapshot carries B's own (nothing), never A's.
    assert!(w.snapshot_for(Some(B)).own.unwrap().hud.is_none());
}

#[test]
fn readiness_replicates_through_the_delta_codec() {
    let mut w = duel();
    assert!(w.grant_power(A, "star_platinum"));
    assert!(w.grant_power(A, "ultra_instinct"));
    full(&mut w, A);
    let base: WorldSnapshot = w.snapshot_for(Some(B));
    let sp = PowerTable::get().index_of("star_platinum").unwrap();
    let k = w.player(A).unwrap().power.slot_of(sp).unwrap();
    w.power_move(A, move_sel::move_of(k, move_sel::HOLD)).unwrap();
    w.player_mut(B).unwrap().status.apply(StatusKind::Burn, 2.5, None);
    run(&mut w, 12);
    let cur = w.snapshot_for(Some(B));
    let d = SnapshotDelta::encode(Some(&base), &cur);
    let got = d.apply(Some(&base)).expect("decodes");
    let a_cur = cur.players.iter().find(|p| p.id == A.0).unwrap();
    let a_got = got.players.iter().find(|p| p.id == A.0).unwrap();
    assert_eq!(a_got.ready, a_cur.ready, "B sees A's readiness exactly");
    assert!(!a_got.ready.is_ready(k) && a_got.ready.form(0).is_some());
    assert_eq!(got.own.unwrap().hud, cur.own.unwrap().hud);
    assert!(got.own.unwrap().hud.unwrap().secs(StatusKind::Burn as usize) > 2.0);
    // A client mirror adopts it (and a later host snapshot keeps it in step).
    let mut client = duel();
    client.apply_snapshot(&got, Some(B));
    assert_eq!(client.snapshot_for(Some(B)).players.iter().find(|p| p.id == A.0).unwrap().power, a_cur.power);
}
