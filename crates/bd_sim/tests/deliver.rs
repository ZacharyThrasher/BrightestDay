//! Named-target delivery (FREEDOM HOOK, `bd_sim::freedom::deliver`): "drop a meteor on Ethan" lands on
//! Ethan — where the caster saw him — not in front of the caster. Two players on the flat lab arena
//! (ground top at y = 8); the caster always looks AWAY from the target, so only the name can aim.

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic, Recipe};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::events::GameEvent;
use bd_sim::freedom::Delivery;
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, TargetHint, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn duel() -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 128.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.add_player(A, "Caster".into(), Corps::from_index(1)).unwrap();
    w.add_player(B, "Ethan".into(), Corps::from_index(2)).unwrap();
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    w
}

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
}

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend_from_slice(w.tick(&[]));
    }
    all
}

fn eye(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().eye(w.params())
}

/// A looks along +X (away from B, who is down -Z) and names B.
fn named(w: &World, via: Delivery) -> Aim {
    let mut aim = Aim::new(eye(w, A), Vec3::X);
    aim.target_hint = Some(TargetHint::Named { player: B, via });
    aim
}

fn setup() -> World {
    let mut w = duel();
    place(&mut w, A, Vec3::new(0.0, 8.0, 0.0));
    place(&mut w, B, Vec3::new(6.0, 8.0, -30.0));
    run(&mut w, 4);
    w
}

fn flat_dist(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
}

fn meteor() -> Recipe {
    recipe("meteor", Form::Boulder, &[Mechanic::DropFromSky, Mechanic::Heavy], Element::Fire, 1.5)
}

fn hits_b(ev: &[GameEvent]) -> bool {
    ev.iter().any(|e| matches!(e, GameEvent::Hit { target, .. } if *target == B))
}

#[test]
fn drop_a_meteor_on_ethan_lands_on_ethan() {
    let mut w = setup();
    let b = w.player(B).unwrap().mv.pos;
    let id = w.cast(A, meteor(), None, named(&w, Delivery::Onto)).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert!(flat_dist(c.pos, b) < 1.5, "meteor formed at {:?}, Ethan at {b:?}", c.pos);
    assert!(c.pos.y > b.y + 6.0, "it comes from the sky ({:?})", c.pos);
    let ev = run(&mut w, 360);
    assert!(hits_b(&ev), "the meteor hits Ethan");
}

#[test]
fn unnamed_meteor_still_lands_at_the_crosshair() {
    let mut w = setup();
    let mut aim = Aim::new(eye(&w, A), Vec3::new(1.0, -0.4, 0.0).normalize());
    aim.target_hint = None;
    let id = w.cast(A, meteor(), None, aim).unwrap();
    let b = w.player(B).unwrap().mv.pos;
    let c = w.constructs.get(id).unwrap();
    assert!(flat_dist(c.pos, b) > 20.0, "no name: not delivered to Ethan ({:?})", c.pos);
    assert!(c.pos.x > 2.0, "along the crosshair (+X): {:?}", c.pos);
}

#[test]
fn anvil_on_top_of_ethan() {
    let mut w = setup();
    let b = w.player(B).unwrap().mv.pos;
    let anvil = recipe("anvil", Form::Anvil, &[Mechanic::DropFromSky, Mechanic::Heavy], Element::Light, 1.0);
    let id = w.cast(A, anvil, None, named(&w, Delivery::Onto)).unwrap();
    assert!(flat_dist(w.constructs.get(id).unwrap().pos, b) < 1.5);
    assert!(hits_b(&run(&mut w, 360)));
}

#[test]
fn a_car_on_ethan_falls_on_him() {
    let mut w = setup();
    let b = w.player(B).unwrap().mv.pos;
    let car = recipe("car", Form::Vehicle, &[Mechanic::Charge], Element::Light, 4.0);
    let id = w.cast(A, car, None, named(&w, Delivery::Onto)).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert!(c.recipe.has(Mechanic::DropFromSky), "an object delivered onto someone drops");
    assert!(flat_dist(c.pos, b) < 1.5 && c.pos.y > b.y + 6.0, "{:?}", c.pos);
}

#[test]
fn missile_at_ethan_flies_at_ethan() {
    let mut w = setup();
    let b = w.player(B).unwrap().mv.pos;
    let missile = recipe("missile", Form::Rocket, &[Mechanic::Explodes, Mechanic::Thrust, Mechanic::Homing], Element::Fire, 1.0);
    let id = w.cast(A, missile, None, named(&w, Delivery::At)).unwrap();
    // formed in the hand, then launched: its velocity heads for Ethan (the caster looks along +X)
    let mut launched = None;
    let mut ev = Vec::new();
    for _ in 0..240 {
        ev.extend_from_slice(w.tick(&[]));
        if let Some(c) = w.constructs.get(id)
            && c.vel.length() > 5.0
            && launched.is_none()
        {
            launched = Some((c.pos, c.vel));
        }
    }
    let (p, v) = launched.expect("the missile launched");
    let to_b = (b + Vec3::Y - p).normalize();
    assert!(v.normalize().dot(to_b) > 0.8, "launched toward Ethan: vel {v:?}, to Ethan {to_b:?}");
    assert!(hits_b(&ev) || ev.iter().any(|e| matches!(e, GameEvent::Explosion { .. })), "and it gets there");
}

#[test]
fn thrown_rock_at_ethan_arcs_to_him() {
    let mut w = setup();
    place(&mut w, B, Vec3::new(0.0, 8.0, -16.0));
    run(&mut w, 2);
    let rock = recipe("rock", Form::Boulder, &[Mechanic::Heavy], Element::Earth, 0.8);
    let _ = w.cast(A, rock, None, named(&w, Delivery::At)).unwrap();
    assert!(hits_b(&run(&mut w, 300)), "a rock thrown at Ethan (caster looking away) hits Ethan");
}

#[test]
fn bees_on_ethan_appear_next_to_him_and_sting_him() {
    let mut w = setup();
    let b = w.player(B).unwrap().mv.pos;
    let mut bees = recipe("bees", Form::Insect, &[Mechanic::Pierce, Mechanic::Flies, Mechanic::Swarm], Element::Poison, 0.3);
    bees.count = 10;
    let id = w.cast(A, bees, None, named(&w, Delivery::Near)).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert!(flat_dist(c.pos, b) < 6.0, "bees formed next to Ethan: {:?} vs {b:?}", c.pos);
    assert_eq!(c.comp.named, Some(B));
    assert!(hits_b(&run(&mut w, 600)), "and they go for him");
}

#[test]
fn wolves_after_ethan_lock_on_even_when_the_caster_aims_elsewhere() {
    let mut w = setup();
    let wolf = recipe("wolf", Form::Quadruped, &[], Element::Light, 1.2);
    let id = w.cast(A, wolf, None, named(&w, Delivery::Hunt)).unwrap();
    // the caster's own last target moves on (a third party would be picked up by `last_target`)
    w.player_mut(A).unwrap().last_target = None;
    assert_eq!(w.constructs.get(id).unwrap().comp.named, Some(B));
    assert!(hits_b(&run(&mut w, 900)), "the wolf hunts Ethan");
}

#[test]
fn dome_over_ethan_closes_around_him() {
    let mut w = setup();
    let b = w.player(B).unwrap().mv.pos;
    let dome = recipe("dome", Form::Dome, &[Mechanic::Barrier], Element::Light, 4.0);
    let id = w.cast(A, dome, None, named(&w, Delivery::Around)).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert!(flat_dist(c.pos, b) < 1.0, "the dome is centred on Ethan, not the caster: {:?}", c.pos);
    let cage = recipe("cage", Form::Cage, &[Mechanic::Trap], Element::Light, 2.5);
    let id = w.cast(A, cage, None, named(&w, Delivery::Around)).unwrap();
    assert!(flat_dist(w.constructs.get(id).unwrap().pos, b) < 1.0);
}

#[test]
fn rain_of_arrows_on_ethan_falls_around_him() {
    let mut w = setup();
    let b = w.player(B).unwrap().mv.pos;
    let mut arrows = recipe("arrows", Form::Spear, &[Mechanic::Pierce, Mechanic::Swarm, Mechanic::DropFromSky], Element::Light, 1.0);
    arrows.count = 12;
    let id = w.cast(A, arrows, None, named(&w, Delivery::Onto)).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert!(flat_dist(c.pos, b) < 4.0 && c.pos.y > b.y + 4.0, "the volley forms above Ethan: {:?}", c.pos);
    let ev = run(&mut w, 360);
    let near: usize =
        w.constructs.iter().filter(|c| c.owner == A && flat_dist(c.pos, b) < 6.0).count() + usize::from(hits_b(&ev)) * 99;
    assert!(near > 0, "arrows land around Ethan");
}

#[test]
fn delivery_uses_the_target_as_the_caster_saw_them() {
    let mut w = setup();
    // Ethan stands at P1 for a while (recorded in the hull history), then is at P2 now.
    let p1 = Vec3::new(6.0, 8.0, -30.0);
    let p2 = Vec3::new(-10.0, 8.0, -30.0);
    run(&mut w, 10);
    let seen = w.tick as f64 - 1.0;
    place(&mut w, B, p2);
    run(&mut w, 3);
    // The caster's view is 4 ticks old: it still sees Ethan at P1.
    w.set_view_tick(A, Some(seen));
    let id = w.cast(A, meteor(), None, named(&w, Delivery::Onto)).unwrap();
    let c = w.constructs.get(id).unwrap();
    assert!(flat_dist(c.pos, p1) < 1.5, "lag-compensated: lands where the caster saw Ethan ({:?})", c.pos);
    // No rewind: where Ethan is now.
    w.set_view_tick(A, None);
    let id = w.cast(A, meteor(), None, named(&w, Delivery::Onto)).unwrap();
    assert!(flat_dist(w.constructs.get(id).unwrap().pos, p2) < 1.5);
}

#[test]
fn host_validates_the_name() {
    let mut w = setup();
    // a dead / unknown / self target casts at the crosshair instead (the ring never says no)
    for bad in [PlayerId(9), A] {
        let mut aim = named(&w, Delivery::Onto);
        aim.target_hint = Some(TargetHint::Named { player: bad, via: Delivery::Onto });
        let id = w.cast(A, meteor(), None, aim).unwrap();
        assert!(w.constructs.get(id).unwrap().pos.x > 2.0, "{bad:?}: fell back to the crosshair");
    }
    w.player_mut(B).unwrap().alive = false;
    let id = w.cast(A, meteor(), None, named(&w, Delivery::Onto)).unwrap();
    assert!(w.constructs.get(id).unwrap().pos.x > 2.0, "dead target: crosshair");
    assert_eq!(w.constructs.get(id).unwrap().comp.named, None);
}

#[test]
fn far_targets_are_reached_up_to_the_named_range() {
    let mut w = setup();
    place(&mut w, B, Vec3::new(0.0, 8.0, -60.0));
    run(&mut w, 3);
    let b = w.player(B).unwrap().mv.pos;
    let id = w.cast(A, meteor(), None, named(&w, Delivery::Onto)).unwrap();
    assert!(flat_dist(w.constructs.get(id).unwrap().pos, b) < 1.5, "60 m is within the named range");
}
