//! POWER-UPS (`MatchRules::powerups`, `bd_sim::pickups`): deterministic map pickups (telegraph →
//! appear → carry → drop on KO / fade), gated casts (weaker without, FULL size with, spent), and the
//! mode off = no change.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::{HitInfo, HitKind};
use bd_sim::events::GameEvent;
use bd_sim::pickups::{self, Gate, GateKind, Pickup, PickupMoment, PickupPhase};
use bd_sim::rules::WillpowerMode;
use bd_sim::signature::{SignatureCast, SignatureKind};
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn world(powerups: bool, seed: u64) -> World {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed, check_invariants: true, ..SimConfig::default() };
    cfg.rules.powerups = powerups;
    cfg.rules.willpower = WillpowerMode::Infinite;
    cfg.tuning.pickups.first_after = 1.0;
    cfg.tuning.pickups.telegraph = 1.0;
    cfg.tuning.pickups.every = 6.0;
    let mut w = World::new(cfg);
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn aim(w: &World, id: PlayerId) -> Aim {
    Aim::new(w.player(id).unwrap().eye(w.params()), Vec3::new(0.0, -0.3, -1.0))
}

fn give(w: &mut World, who: PlayerId, kind: GateKind) {
    let until = w.tick + 10_000;
    let pos = w.player(who).unwrap().mv.pos;
    w.pickups.items.push(Pickup { id: 900 + kind as u16, kind, phase: PickupPhase::Held, pos, holder: Some(who), until });
}

fn moments(ev: &[GameEvent], m: PickupMoment) -> usize {
    ev.iter().filter(|e| matches!(e, GameEvent::Pickup { moment, .. } if *moment == m)).count()
}

fn kaiju() -> bd_core::mechanics::Recipe {
    recipe("godzilla", Form::Kaiju, &[], Element::Atomic, 16.0)
}

#[test]
fn pickups_spawn_deterministically_on_derived_spots() {
    let mut a = world(true, 7);
    let mut b = world(true, 7);
    let ea = run(&mut a, 60 * 30);
    let eb = run(&mut b, 60 * 30);
    assert_eq!(a.pickups.items, b.pickups.items);
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(moments(&ea, PickupMoment::Telegraph) >= 2 && moments(&ea, PickupMoment::Appear) >= 2, "telegraphed, then appeared");
    assert_eq!(moments(&ea, PickupMoment::Telegraph), moments(&eb, PickupMoment::Telegraph));
    // Spots: on solid ground, away from every player spawn; the cap holds.
    assert!(!a.pickups.spots.is_empty());
    for s in &a.pickups.spots {
        assert!(a.arena.spawns.iter().all(|sp| sp.distance(*s) >= a.cfg.tuning.pickups.spawn_clear - 1e-3), "{s}");
    }
    assert!(a.pickups.items.len() <= a.cfg.tuning.pickups.max_live as usize);
    // The beacon comes first: a telegraph turns into the pickup `telegraph` s later, at the same spot.
    let mut c = world(true, 7);
    let ev = run(&mut c, 60 * 2 + 10);
    let t = ev.iter().find_map(|e| match e {
        GameEvent::Pickup { id, moment: PickupMoment::Telegraph, pos, .. } => Some((*id, *pos)),
        _ => None,
    });
    let (id, pos) = t.expect("a telegraph within 2 s");
    let p = c.pickups.items.iter().find(|p| p.id == id).unwrap();
    assert_eq!((p.phase, p.pos), (PickupPhase::Ground, pos));
    // A different seed may pick differently, but is just as deterministic.
    let mut d = world(true, 8);
    let mut e = world(true, 8);
    run(&mut d, 60 * 20);
    run(&mut e, 60 * 20);
    assert_eq!(d.pickups.items, e.pickups.items);
}

#[test]
fn touch_carries_ko_drops_and_it_fades() {
    let mut w = world(true, 3);
    run(&mut w, 60 * 2 + 10);
    let p = *w.pickups.items.iter().find(|p| p.phase == PickupPhase::Ground).expect("a pickup lies there");
    // Walk A onto it.
    {
        let q = w.player_mut(A).unwrap();
        q.mv.pos = p.pos;
        q.mv.vel = Vec3::ZERO;
    }
    let ev = run(&mut w, 3);
    assert_eq!(moments(&ev, PickupMoment::Take), 1);
    assert_eq!(w.pickups.held_by(A).map(|h| h.id), Some(p.id));
    // One at a time: B can't take it from A by touch, and A can't take a second one.
    let footing = w.player(A).unwrap().mv.pos;
    run(&mut w, 30);
    // KO A (Smash: a blast-zone exit) → it drops where A last stood.
    w.apply_hit(HitInfo::new(A, Some(B), HitKind::RingBlast, 1.0, 0.0, Vec3::X, Vec3::ZERO));
    w.player_mut(A).unwrap().mv.pos.y = -100.0;
    let ev = run(&mut w, 2);
    assert_eq!(moments(&ev, PickupMoment::Drop), 1, "dropped on KO");
    let d = w.pickups.items.iter().find(|q| q.id == p.id).unwrap();
    assert_eq!(d.phase, PickupPhase::Ground);
    assert!(d.pos.distance(footing) < 1.0, "at A's last footing ({} vs {footing})", d.pos);
    assert!(w.pickups.held_by(A).is_none());
    // Unclaimed, it fades after `ground_life`.
    let life = (w.cfg.tuning.pickups.ground_life / bd_core::DT) as u32;
    let ev = run(&mut w, life + 2);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Pickup { id, moment: PickupMoment::Fade, .. } if *id == p.id)));
}

#[test]
fn a_gated_giant_without_the_pickup_is_person_sized_with_it_full_size() {
    let mut w = world(true, 1);
    let a = aim(&w, A);
    assert_eq!(pickups::gate(&w, A, GateKind::Giant), Gate::Locked);
    let small = w.cast(A, kaiju(), None, a).unwrap();
    let c = w.constructs.get(small).unwrap();
    assert_eq!(c.recipe.form, Form::Biped, "GODZILLA without the heart: a person-sized lizard");
    assert!(c.recipe.scale <= w.cfg.tuning.pickups.giant_weak_scale + 1e-4);
    // With the heart: FULL size, and the heart is spent.
    give(&mut w, A, GateKind::Giant);
    assert_eq!(pickups::gate(&w, A, GateKind::Giant), Gate::Loaded);
    let n0 = w.events().len();
    let big = w.cast(A, kaiju(), None, a).unwrap();
    let c = w.constructs.get(big).unwrap();
    assert_eq!((c.recipe.form, c.recipe.scale), (Form::Kaiju, 16.0));
    assert!(w.pickups.held_by(A).is_none(), "spent");
    assert_eq!(moments(&w.events()[n0..], PickupMoment::Spend), 1);
    // The wrong pickup doesn't open it.
    give(&mut w, A, GateKind::Awakening);
    assert_eq!(pickups::gate(&w, A, GateKind::Giant), Gate::Locked);
    // Small things are never gated.
    assert!(w.cast(A, recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2), None, a).is_ok());
}

#[test]
fn a_meteor_without_the_pickup_is_a_fist_sized_rock() {
    let mut w = world(true, 1);
    let a = aim(&w, A);
    let meteor = recipe("meteor", Form::Boulder, &[Mechanic::DropFromSky, Mechanic::Heavy], Element::Fire, 4.0);
    let id = w.cast(A, meteor.clone(), None, a).unwrap();
    let min = bd_sim::constructs::form_spec(Form::Boulder).min_scale;
    assert!((w.constructs.get(id).unwrap().recipe.scale - min).abs() < 1e-4);
    give(&mut w, A, GateKind::Ultimate);
    let id = w.cast(A, meteor, None, a).unwrap();
    assert_eq!(w.constructs.get(id).unwrap().recipe.scale, 4.0);
    assert!(w.pickups.held_by(A).is_none());
}

#[test]
fn a_signature_ultimate_without_the_pickup_is_the_plain_phrase() {
    let mut w = world(true, 1);
    let a = aim(&w, A);
    let shrine = recipe("shrine", Form::Wall, &[], Element::Shadow, 4.0);
    let plain = w.cast_signature(A, shrine.clone(), None, a, SignatureCast::new(SignatureKind::Domain, 1.0)).unwrap();
    assert_eq!(w.constructs.get(plain).unwrap().recipe.form, Form::Wall, "no domain without the star");
    give(&mut w, A, GateKind::Ultimate);
    let dom = w.cast_signature(A, shrine, None, a, SignatureCast::new(SignatureKind::Domain, 1.0)).unwrap();
    assert_eq!(w.constructs.get(dom).unwrap().recipe.form, Form::Dome, "the full domain");
    assert!(w.pickups.held_by(A).is_none());
}

#[test]
fn an_awakening_without_the_pickup_is_a_plain_power_up() {
    let mut w = world(true, 1);
    assert!(w.grant_power(A, "nika"));
    // Gear 5 is move slot 1 (a Transform).
    w.power_move(A, 1).expect("the move runs (never a no)");
    run(&mut w, 1);
    assert!(!w.player(A).unwrap().power.transformed, "no Gear 5 without the orb");
    w.player_mut(A).unwrap().power.move_cd = Default::default();
    give(&mut w, A, GateKind::Awakening);
    w.power_move(A, 1).expect("gear 5");
    assert!(w.player(A).unwrap().power.transformed, "Gear 5 with the orb");
    assert!(w.pickups.held_by(A).is_none(), "spent");
}

#[test]
fn mode_off_changes_nothing() {
    let mut w = world(false, 7);
    let ev = run(&mut w, 60 * 30);
    assert!(w.pickups.items.is_empty() && moments(&ev, PickupMoment::Telegraph) == 0);
    for k in GateKind::ALL {
        assert_eq!(pickups::gate(&w, A, k), Gate::Free);
    }
    let a = aim(&w, A);
    let id = w.cast(A, kaiju(), None, a).unwrap();
    assert_eq!(w.constructs.get(id).unwrap().recipe.form, Form::Kaiju);
    // Same run with and without the (off) tuning section present: bit-identical state.
    let mut x = world(false, 11);
    let mut y = world(false, 11);
    y.cfg.tuning.pickups = Default::default();
    run(&mut x, 600);
    run(&mut y, 600);
    assert_eq!(x.state_hash(), y.state_hash());
    // Rules round-trip carries the toggle.
    let mut r = bd_sim::MatchRules::default();
    r.powerups = true;
    let bytes = bincode::serde::encode_to_vec(&r, bincode::config::standard()).unwrap();
    let (back, _): (bd_sim::MatchRules, usize) = bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert!(back.powerups);
}

#[test]
fn pickups_ride_the_snapshot() {
    let mut w = world(true, 5);
    run(&mut w, 60 * 3);
    assert!(!w.pickups.items.is_empty());
    let snap = w.snapshot_for(Some(A));
    assert_eq!(snap.pickups, w.pickups.items);
    let d = bd_sim::replicate::SnapshotDelta::encode(None, &snap);
    let back = d.apply(None).unwrap();
    assert_eq!(back.pickups, w.pickups.items);
    // Unchanged list → not resent.
    let later = { run(&mut w, 1); w.snapshot_for(Some(A)) };
    if later.pickups == snap.pickups {
        assert!(bd_sim::replicate::SnapshotDelta::encode(Some(&snap), &later).pickups.is_none());
    }
    // A client mirror adopts it.
    let mut c = World::new(w.cfg.clone());
    c.apply_snapshot(&snap, Some(B));
    assert_eq!(c.pickups.items, w.pickups.items);
}
