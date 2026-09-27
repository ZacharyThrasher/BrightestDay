//! TRANSFORMERS FEEL (COMBAT.md §5d-quater "FEEL pass"): every kind of transformer fights —
//! archetypes for the new kinds (dino / beast / boat / train / colossus), beast modes that fight
//! (bites, tail sweeps, breath, pounces, roars), the robot bursting out of the beast into its
//! finisher, carrying rams, the combiner quake, small SUIT transformers (you are the robot: strings
//! and transform attacks on your own body), and voice orders mid-fight ("transform", "optimus prime
//! combo", "ion blaster", "energon axe"). Deterministic; a cockpit stays host-driven.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Feature, Form, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::constructs::ai::AttackKind;
use bd_sim::constructs::brawl::{Order, XAttack, XPhase, order_for_move};
use bd_sim::events::{GameEvent, HeldFx};
use bd_sim::powers::chain::ToolStep;
use bd_sim::powers::toolbox::{MoveIndex, MoveRef};
use bd_sim::powers::{PowerEvent, PowerTable};
use bd_sim::rules::WillpowerMode;
use bd_sim::tuning::CombatTuning;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn cfg() -> SimConfig {
    SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() }
}

fn world() -> World {
    let mut w = World::new(cfg());
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    w.cfg.rules.powers_enabled = true;
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

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
    p.mv.tumble = 0.0;
}

fn feats(mut r: Recipe, f: &[Feature], mass: f32) -> Recipe {
    r.features = f.to_vec();
    r.mass = mass;
    r.normalized()
}

fn optimus() -> Recipe {
    feats(recipe("Optimus Prime", Form::Vehicle, &[Mechanic::Blade, Mechanic::Shoots, Mechanic::Charge, Mechanic::Transforms], Element::Light, 6.0), &[Feature::Wheels, Feature::Smokestack, Feature::Cab, Feature::Headlight], 9000.0)
}
fn grimlock() -> Recipe {
    feats(recipe("Grimlock", Form::Kaiju, &[Mechanic::Blade, Mechanic::ArticulatedTail, Mechanic::Stomp, Mechanic::Breath, Mechanic::Heavy, Mechanic::Transforms], Element::Fire, 6.0), &[Feature::Spikes], 8000.0)
}
fn primal() -> Recipe {
    feats(recipe("Optimus Primal", Form::Quadruped, &[Mechanic::Blunt, Mechanic::Blade, Mechanic::Charge, Mechanic::Transforms], Element::Light, 3.6), &[Feature::Armor], 4000.0)
}
fn devastator() -> Recipe {
    feats(recipe("Devastator", Form::Kaiju, &[Mechanic::Blunt, Mechanic::Grab, Mechanic::Stomp, Mechanic::Heavy, Mechanic::Transforms], Element::Light, 8.0), &[Feature::Wheels, Feature::Treads, Feature::CraneArm, Feature::Plow], 60000.0)
}
fn unicron() -> Recipe {
    feats(recipe("Unicron", Form::Kaiju, &[Mechanic::Beam, Mechanic::Stomp, Mechanic::Heavy, Mechanic::Attract, Mechanic::Transforms], Element::Shadow, 22.0), &[Feature::Spikes, Feature::Armor], 60000.0)
}
fn train() -> Recipe {
    feats(recipe("transforming train", Form::Vehicle, &[Mechanic::Charge, Mechanic::Fast, Mechanic::Heavy, Mechanic::Transforms], Element::Light, 6.0), &[Feature::Wheels, Feature::Cab, Feature::Plow], 40000.0)
}
fn boat() -> Recipe {
    feats(recipe("transforming boat", Form::Vehicle, &[Mechanic::Charge, Mechanic::Transforms], Element::Water, 6.0), &[Feature::Propeller, Feature::Sails], 1500.0)
}
fn small_car() -> Recipe {
    feats(recipe("small transforming car", Form::Vehicle, &[Mechanic::Charge, Mechanic::Fast, Mechanic::Transforms], Element::Light, 2.4), &[Feature::Wheels, Feature::Windows, Feature::Headlight], 450.0)
}

#[test]
fn new_kinds_get_their_archetypes_from_data() {
    let t = CombatTuning::default().transformer;
    let dozer = feats(recipe("transforming bulldozer", Form::Vehicle, &[Mechanic::Charge, Mechanic::Heavy, Mechanic::Transforms], Element::Light, 4.0), &[Feature::Treads, Feature::Cab, Feature::Plow], 1500.0);
    let t_rex = feats(recipe("transforming t rex", Form::Kaiju, &[Mechanic::Grab, Mechanic::ArticulatedTail, Mechanic::Stomp, Mechanic::Transforms], Element::Light, 12.0), &[], 8000.0);
    let dragon = feats(recipe("transforming dragon", Form::Quadruped, &[Mechanic::ArticulatedTail, Mechanic::Flies, Mechanic::Breath, Mechanic::Transforms], Element::Fire, 4.0), &[], 6000.0);
    let ravage = feats(recipe("Ravage", Form::Quadruped, &[Mechanic::Pierce, Mechanic::Shoots, Mechanic::Fast, Mechanic::Transforms], Element::Light, 1.6), &[], 120.0);
    let helicopter_robot = feats(recipe("helicopter robot", Form::Vehicle, &[Mechanic::Shoots, Mechanic::Flies, Mechanic::Transforms], Element::Light, 5.0), &[Feature::Rotor], 2000.0);
    let bike = feats(recipe("motorcycle robot", Form::Vehicle, &[Mechanic::Fast, Mechanic::Transforms], Element::Light, 4.0), &[Feature::Wheels, Feature::Exhaust, Feature::Headlight], 400.0);
    for (r, want) in [
        (grimlock(), "dino"),
        (t_rex, "dino"),
        (dragon, "dino"),
        (primal(), "beast"),
        (ravage, "beast"),
        (devastator(), "colossus"),
        (unicron(), "colossus"),
        (train(), "train"),
        (boat(), "boat"),
        (dozer, "construction"),
        (helicopter_robot, "helicopter"),
        (bike, "bike"),
        (small_car(), "car"),
        (optimus(), "truck"),
    ] {
        assert_eq!(t.archetype_of(&r).name, want, "{}", r.name);
    }
    // Beast rows name their beast-mode row; every `alt` points at a row.
    for a in &t.archetypes {
        if !a.alt.is_empty() {
            assert!(t.named(&a.alt).is_some(), "{} → {}", a.name, a.alt);
        }
    }
    assert_eq!(t.archetype_of(&grimlock()).alt, "dino_beast");
    assert_eq!(t.archetype_of(&primal()).alt, "beast_alt");
}

/// A seated pilot in `r`, `robot` or alt mode, with B standing `gap` m in front of it.
fn duel(r: Recipe, robot: bool, gap: f32) -> (World, ConstructId) {
    let mut w = world();
    place(&mut w, A, Vec3::new(-20.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(40.0, GROUND, -40.0));
    w.tick(&[]);
    let mut r = r;
    r.mechanics.push(Mechanic::Piloted);
    let r = r.normalized();
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, r, None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0).normalize())).unwrap();
    assert_eq!(w.player(A).unwrap().seat.map(|s| s.construct), Some(id));
    settle(&mut w, id);
    if w.constructs.get(id).unwrap().held.vehicle == robot {
        assert!(w.toggle_transform(A));
        w.tick(&[(A, PlayerInput::default())]);
        settle(&mut w, id);
    }
    assert_eq!(!w.constructs.get(id).unwrap().held.vehicle, robot, "mode");
    put_b_in_front(&mut w, id, gap);
    (w, id)
}

/// Until the construct has formed and no transformation is running (big bodies form longer).
fn settle(w: &mut World, id: ConstructId) {
    for _ in 0..1200 {
        w.tick(&[(A, PlayerInput::default())]);
        let c = w.constructs.get(id).unwrap();
        if c.active() && c.held.transform_t <= 0.0 && c.age > 0.3 {
            break;
        }
    }
    for _ in 0..10 {
        w.tick(&[(A, PlayerInput::default())]);
    }
}

fn put_b_in_front(w: &mut World, id: ConstructId, gap: f32) {
    let c = w.constructs.get(id).unwrap();
    let f = c.forward();
    let f = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::NEG_Z);
    let front = c.center() + f * (c.half.x.max(c.half.z) + gap);
    place(w, B, Vec3::new(front.x, GROUND, front.z));
    w.player_mut(B).unwrap().damage_pct = 0.0;
}

/// A pilot input looking the way the construct faces.
fn look_at(w: &World, id: ConstructId, b: u16) -> PlayerInput {
    let c = w.constructs.get(id).unwrap();
    let f = c.forward();
    PlayerInput { buttons: b, yaw: (-f.x).atan2(-f.z), pitch: -0.1, ..PlayerInput::default() }
}

fn tick_a(w: &mut World, id: ConstructId, b: u16, n: u32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..n {
        let inp = look_at(w, id, b);
        ev.extend_from_slice(w.tick(&[(A, inp)]));
    }
    ev
}

fn hits_on_b(ev: &[GameEvent]) -> Vec<f32> {
    ev.iter()
        .filter_map(|e| match e {
            GameEvent::Hit { target, knockback, .. } if *target == B => Some(*knockback),
            _ => None,
        })
        .collect()
}

fn has_fx(ev: &[GameEvent], fx: HeldFx) -> bool {
    ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: f, .. } if *f == fx))
}

fn has_clip(ev: &[GameEvent], id: ConstructId, kind: AttackKind) -> bool {
    ev.iter().any(|e| matches!(e, GameEvent::CreatureAttack { id: c, kind: k, .. } if *c == id && *k == kind as u8))
}

#[test]
fn a_dino_fights_in_beast_mode_bite_tail_breath_then_bursts_out_into_the_robot() {
    let (mut w, id) = duel(grimlock(), false, 1.0);
    assert!(w.constructs.get(id).unwrap().held.vehicle, "T-rex mode");
    // LMB: a driving bite (the creature Bite clip plays on the beast).
    let mut ev = tick_a(&mut w, id, buttons::PRIMARY, 1);
    ev.extend(tick_a(&mut w, id, 0, 30));
    assert_eq!(hits_on_b(&ev).len(), 1, "the bite connects");
    assert!(has_clip(&ev, id, AttackKind::Bite), "bite clip");
    // Hold LMB: the tail sweep hits B even BEHIND the beast.
    let c = w.constructs.get(id).unwrap();
    let behind = c.center() - Vec3::new(c.forward().x, 0.0, c.forward().z).normalize() * (c.half.x.max(c.half.z) + 1.0);
    place(&mut w, B, Vec3::new(behind.x, GROUND, behind.z));
    let ev = tick_a(&mut w, id, buttons::PRIMARY, 60);
    assert!(!hits_on_b(&ev).is_empty(), "the tail sweeps round");
    assert!(has_fx(&ev, HeldFx::TailSweep) && has_clip(&ev, id, AttackKind::TailSweep));
    tick_a(&mut w, id, 0, 40);
    // Hold RMB: fire breath in a cone, tick after tick.
    put_b_in_front(&mut w, id, 3.0);
    let ev = tick_a(&mut w, id, buttons::SECONDARY, 60);
    assert!(hits_on_b(&ev).len() >= 3, "breath ticks: {}", hits_on_b(&ev).len());
    assert!(has_clip(&ev, id, AttackKind::Breath));
    tick_a(&mut w, id, 0, 30);
    // A bite, then Transform inside the combo window: the ROBOT bursts out into its finisher.
    put_b_in_front(&mut w, id, 1.0);
    let mut ev = tick_a(&mut w, id, buttons::PRIMARY, 1);
    ev.extend(tick_a(&mut w, id, 0, 5));
    assert!(w.toggle_transform(A));
    ev.extend(tick_a(&mut w, id, 0, 150));
    let c = w.constructs.get(id).unwrap();
    assert!(!c.held.vehicle, "burst out as the robot");
    assert_eq!(c.brawl.xatk, XPhase::None);
    assert!(hits_on_b(&ev).len() >= 2, "bite + the robot's finisher: {:?}", hits_on_b(&ev));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::Transform, strength, .. } if *strength > 2.0)), "a fast combat transform to robot");
    // The finisher is a heavy hit (clients: the impact frame).
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: bd_sim::combat::HitKind::HeavyMelee, .. } if *target == B)));
}

#[test]
fn a_dino_robot_stampedes_as_the_beast_carrying_its_victim_then_chops() {
    let (mut w, id) = duel(grimlock(), true, 1.0);
    let mut ev = tick_a(&mut w, id, buttons::PRIMARY, 1);
    ev.extend(tick_a(&mut w, id, 0, 10));
    assert_eq!(hits_on_b(&ev).len(), 1);
    put_b_in_front(&mut w, id, 2.0);
    let b0 = w.player(B).unwrap().mv.pos;
    assert!(w.toggle_transform(A));
    let mut ev = tick_a(&mut w, id, 0, 12);
    {
        let c = w.constructs.get(id).unwrap();
        assert_eq!(c.brawl.last_xattack, Some(XAttack::Stampede));
        assert!(c.held.vehicle, "the beast charges");
        assert_eq!(c.brawl.carry, Some(B), "B rides the jaws");
    }
    ev.extend(tick_a(&mut w, id, 0, 200));
    let c = w.constructs.get(id).unwrap();
    assert!(!c.held.vehicle && c.brawl.xatk == XPhase::None);
    let moved = (w.player(B).unwrap().mv.pos - b0).length();
    assert!(moved > 6.0, "carried and flung: {moved}");
    assert!(has_clip(&ev, id, AttackKind::Bite));
}

#[test]
fn a_beast_pounces_in_beast_mode_and_as_its_transform_attack() {
    // Beast mode: hold LMB = a pounce that lands on B.
    let (mut w, id) = duel(primal(), false, 5.0);
    let ev = tick_a(&mut w, id, buttons::PRIMARY, 50);
    let ev = [ev, tick_a(&mut w, id, 0, 60)].concat();
    assert!(has_fx(&ev, HeldFx::Pounce), "the pounce");
    assert!(!hits_on_b(&ev).is_empty(), "lands on B");
    // RMB = a roar (a short stagger, never a lock).
    put_b_in_front(&mut w, id, 2.0);
    let ev = tick_a(&mut w, id, buttons::SECONDARY, 1);
    assert!(has_fx(&ev, HeldFx::Roar));
    assert_eq!(hits_on_b(&ev).len(), 1);
    // Robot mode: the transform attack is the beast pouncing on them.
    let (mut w, id) = duel(primal(), true, 1.0);
    tick_a(&mut w, id, buttons::PRIMARY, 1);
    tick_a(&mut w, id, 0, 8);
    put_b_in_front(&mut w, id, 4.0);
    assert!(w.toggle_transform(A));
    let ev = tick_a(&mut w, id, 0, 240);
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.last_xattack, Some(XAttack::Pounce));
    assert!(!c.held.vehicle);
    assert!(c.brawl.last_xattack_hits >= 1, "the pounce / finisher connects");
    assert!(!hits_on_b(&ev).is_empty());
}

#[test]
fn a_colossus_quakes_without_splitting_and_stomps() {
    let (mut w, id) = duel(devastator(), true, 1.0);
    let c = w.constructs.get(id).unwrap();
    assert_eq!(CombatTuning::default().transformer.robot_row(c).name, "colossus");
    tick_a(&mut w, id, buttons::PRIMARY, 1);
    tick_a(&mut w, id, 0, 10);
    // B a few metres off to the side: the quake's ring still reaches.
    put_b_in_front(&mut w, id, 5.0);
    assert!(w.toggle_transform(A));
    let ev = tick_a(&mut w, id, 0, 200);
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.last_xattack, Some(XAttack::Quake));
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::Transform, strength, .. } if *strength < 0.0)), "a combiner never splits into its vehicle mid-fight");
    assert!(!hits_on_b(&ev).is_empty(), "the quake ring hits");
    // Hold LMB = a stomp.
    tick_a(&mut w, id, 0, 60);
    put_b_in_front(&mut w, id, 3.0);
    let ev = tick_a(&mut w, id, buttons::PRIMARY, 80);
    assert!(has_fx(&ev, HeldFx::GroundPound) && !hits_on_b(&ev).is_empty(), "stomp");
}

#[test]
fn a_train_carries_its_victim_on_the_plough() {
    let (mut w, id) = duel(train(), true, 1.0);
    tick_a(&mut w, id, buttons::PRIMARY, 1);
    tick_a(&mut w, id, 0, 10);
    put_b_in_front(&mut w, id, 3.0);
    let b0 = w.player(B).unwrap().mv.pos;
    assert!(w.toggle_transform(A));
    let mut carried = false;
    for _ in 0..240 {
        tick_a(&mut w, id, 0, 1);
        carried |= w.constructs.get(id).unwrap().brawl.carry == Some(B);
    }
    assert!(carried, "B rode the plough");
    assert!((w.player(B).unwrap().mv.pos - b0).length() > 8.0);
    assert_eq!(w.constructs.get(id).unwrap().brawl.last_xattack, Some(XAttack::Ram));
}

#[test]
fn a_small_transformer_suit_fights_with_your_own_body() {
    let mut w = world();
    place(&mut w, A, Vec3::new(-20.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(40.0, GROUND, -40.0));
    for _ in 0..30 {
        w.tick(&[]);
    }
    let mut r = small_car();
    r.mechanics.push(Mechanic::Piloted);
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, r.normalized(), None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0).normalize())).unwrap();
    let seat = w.player(A).unwrap().seat.unwrap();
    assert!(seat.suit && seat.construct == id, "a suit: you wear it");
    for _ in 0..30 {
        w.tick(&[(A, PlayerInput::default())]);
    }
    if w.constructs.get(id).unwrap().held.vehicle {
        w.toggle_transform(A);
        for _ in 0..150 {
            w.tick(&[(A, PlayerInput::default())]);
        }
    }
    assert!(!w.constructs.get(id).unwrap().held.vehicle, "robot mode");
    let inp = |b: u16| PlayerInput { buttons: b, ..PlayerInput::default() };
    let front = |w: &World| {
        let p = w.player(A).unwrap();
        p.mv.pos + Vec3::NEG_Z * 1.6
    };
    // A string on your own body.
    let mut ev = Vec::new();
    for _ in 0..3 {
        let f = front(&w);
        place(&mut w, B, Vec3::new(f.x, GROUND, f.z));
        w.player_mut(B).unwrap().damage_pct = 0.0;
        ev.extend_from_slice(w.tick(&[(A, inp(buttons::PRIMARY))]));
        for _ in 0..30 {
            ev.extend_from_slice(w.tick(&[(A, inp(0))]));
        }
    }
    assert!(hits_on_b(&ev).len() >= 3, "every string hit lands: {}", hits_on_b(&ev).len());
    // Transform inside the combo window: YOU burst out as the car through B, then the finisher.
    let f = front(&w) + Vec3::NEG_Z * 2.0;
    place(&mut w, B, Vec3::new(f.x, GROUND, f.z));
    let z0 = w.player(A).unwrap().mv.pos.z;
    ev.clear();
    ev.extend_from_slice(w.tick(&[(A, inp(buttons::PRIMARY))]));
    ev.extend_from_slice(w.tick(&[(A, inp(0))]));
    assert!(w.toggle_transform(A));
    let mut burst = false;
    for _ in 0..150 {
        ev.extend_from_slice(w.tick(&[(A, inp(0))]));
        burst |= w.constructs.get(id).unwrap().brawl.xatk == XPhase::Burst;
    }
    assert!(burst, "a transform attack, not a plain transform");
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.last_xattack, Some(XAttack::DriftRam));
    assert!(!c.held.vehicle, "back to robot");
    assert!(z0 - w.player(A).unwrap().mv.pos.z > 3.0, "the wearer was driven through: {}", z0 - w.player(A).unwrap().mv.pos.z);
    assert!(hits_on_b(&ev).len() >= 2);
}

fn kit_move(phrase: &str) -> MoveRef {
    MoveIndex::shipped().read(PowerTable::get(), None, phrase).move_ref().unwrap_or_else(|| panic!("{phrase:?} is not a move"))
}

#[test]
fn voice_orders_work_from_the_cockpit_mid_fight() {
    // Mapping by data: tags first, then ops.
    let t = PowerTable::get();
    let mv = |p: &str| kit_move(p).resolve(t).unwrap().1;
    assert_eq!(order_for_move(mv("optimus prime combo")), Order::Combo);
    assert_eq!(order_for_move(mv("ion blaster")), Order::Ranged);
    assert_eq!(order_for_move(mv("matrix of leadership")), Order::Super);
    assert_eq!(order_for_move(mv("optimus prime shield")), Order::Guard);
    assert_eq!(order_for_move(mv("optimus prime charge")), Order::Dash);

    // "transform" said from the cockpit switches the robot (it used to fizzle: pilots can't cast).
    let (mut w, id) = duel(optimus(), true, 1.0);
    let eye = w.player(A).unwrap().eye(w.params());
    let aim = Aim::new(eye, Vec3::NEG_Z);
    let mut say = Recipe::unknown("transform");
    say.name = "transform".into();
    assert_eq!(w.cast(A, say.clone(), None, aim), Ok(id));
    tick_a(&mut w, id, 0, 150);
    assert!(w.constructs.get(id).unwrap().held.vehicle, "transformed by voice");
    let mut roll = Recipe::unknown("roll out");
    roll.name = "roll out".into();
    assert_eq!(w.cast(A, roll, None, aim), Ok(id));
    tick_a(&mut w, id, 0, 150);
    assert!(!w.constructs.get(id).unwrap().held.vehicle, "rolled out back to robot");

    // "optimus prime combo": the whole combo by itself — string, transform attack, finisher.
    put_b_in_front(&mut w, id, 1.0);
    w.toolbox(A, vec![ToolStep::Move { mv: kit_move("optimus prime combo"), kit: true }], None).unwrap();
    let mut ev = Vec::new();
    for _ in 0..360 {
        ev.extend(tick_a(&mut w, id, 0, 1));
    }
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.last_xattack, Some(XAttack::Ram), "the combo ends in the transform attack");
    assert!(hits_on_b(&ev).len() >= 4, "string + ram + finisher: {:?}", hits_on_b(&ev));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Power(PowerEvent::Fired { .. }))), "the move's title / juice still plays");
    assert!(!c.held.vehicle && c.brawl.order == Order::None);

    // "ion blaster": the robot's blaster.
    tick_a(&mut w, id, 0, 200);
    put_b_in_front(&mut w, id, 6.0);
    w.toolbox(A, vec![ToolStep::Move { mv: kit_move("ion blaster"), kit: true }], None).unwrap();
    let ev = tick_a(&mut w, id, 0, 10);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ShotFired { construct: Some(c), .. } if *c == id)), "the robot fires");

    // "energon axe" (a held weapon, cast): the robot's heavy.
    tick_a(&mut w, id, 0, 60);
    put_b_in_front(&mut w, id, 1.0);
    let axe = recipe("Energon Axe", Form::Axe, &[Mechanic::Blade], Element::Electric, 1.2);
    assert_eq!(w.cast(A, axe, None, aim), Ok(id), "a weapon said from the cockpit is the robot's own");
    let ev = tick_a(&mut w, id, 0, 90);
    assert!(!hits_on_b(&ev).is_empty(), "the heavy lands");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Hit { target, kind: bd_sim::combat::HitKind::HeavyMelee, .. } if *target == B)));
}

#[test]
fn voice_orders_from_vehicle_mode_burst_straight_out() {
    let (mut w, id) = duel(optimus(), false, 6.0);
    assert!(w.constructs.get(id).unwrap().held.vehicle);
    w.toolbox(A, vec![ToolStep::Move { mv: kit_move("matrix of leadership"), kit: true }], None).unwrap();
    let mut burst = false;
    for _ in 0..240 {
        tick_a(&mut w, id, 0, 1);
        burst |= w.constructs.get(id).unwrap().brawl.xatk == XPhase::Burst;
    }
    assert!(burst, "the super bursts straight out of the truck");
    assert!(!w.constructs.get(id).unwrap().held.vehicle, "and finishes as the robot");
}

#[test]
fn beast_mode_is_deterministic_and_prediction_never_fights_the_seat() {
    let run = || {
        let (mut w, _) = duel(grimlock(), false, 1.0);
        let mut client = World::new(cfg());
        for e in w.construct_manifest() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&w.snapshot_for(Some(A)), Some(A));
        let mut worst = 0.0f32;
        let mut log = Vec::new();
        for k in 0..360u32 {
            let b = match k % 45 {
                0 | 14 => buttons::PRIMARY,
                20..=30 => buttons::SECONDARY,
                33 => buttons::DODGE,
                _ => 0,
            };
            let inp = PlayerInput { buttons: b, move_z: if k % 90 < 20 { 1 } else { 0 }, ..PlayerInput::default() };
            if k == 16 || k == 220 {
                w.toggle_transform(A);
            }
            let before = client.player(A).unwrap().mv;
            let mut s = before;
            client.predict_player(A, &mut s, &inp);
            if k > 3 {
                worst = worst.max((s.pos - before.pos).length() + (s.vel - before.vel).length());
            }
            let evs = w.tick(&[(A, inp)]).to_vec();
            for e in &evs {
                client.apply_event(e);
            }
            client.apply_snapshot(&w.snapshot_for(Some(A)), Some(A));
            log.push(format!("{evs:?}"));
        }
        (log, w.state_hash(), worst)
    };
    let (l1, h1, worst) = run();
    let (l2, h2, _) = run();
    assert_eq!(l1, l2);
    assert_eq!(h1, h2);
    assert!(worst < 1e-6, "a cockpit is host-driven ({worst})");
}

/// A follower of `r` hunting B (kept in place) for up to `secs`; returns its events.
fn follower(r: Recipe, secs: f32) -> (World, ConstructId, Vec<GameEvent>) {
    let mut w = world();
    place(&mut w, A, Vec3::new(-30.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(-30.0, GROUND, 6.0));
    w.tick(&[]);
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, r, None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0).normalize())).unwrap();
    assert!(w.constructs.get(id).unwrap().pilot.is_none(), "a follower");
    let mut ev = Vec::new();
    for k in 0..(secs * 120.0) as u32 {
        if k % 90 == 0 {
            place(&mut w, B, Vec3::new(-30.0, GROUND, 6.0));
            w.player_mut(B).unwrap().damage_pct = 0.0;
        }
        ev.extend_from_slice(w.tick(&[]));
        if w.constructs.get(id).is_none() {
            break;
        }
    }
    (w, id, ev)
}

#[test]
fn a_dino_follower_fights_as_the_beast_and_as_the_robot() {
    let (w, id, ev) = follower(grimlock(), 40.0);
    let beast = [AttackKind::Bite, AttackKind::TailSweep, AttackKind::Breath, AttackKind::Stomp].iter().filter(|k| has_clip(&ev, id, **k)).count();
    let robot_hits = ev.iter().filter(|e| matches!(e, GameEvent::HeldFx { id: c, fx: HeldFx::PilotAttack | HeldFx::GroundPound | HeldFx::Uppercut, .. } if *c == id)).count();
    let xattacks = w.constructs.get(id).map_or(0, |c| c.brawl.ai_xattacks);
    eprintln!("dino follower: beast attack kinds {beast}, robot strikes {robot_hits}, transform attacks {xattacks}, hits on B {}", hits_on_b(&ev).len());
    assert!(beast >= 1, "fights as the beast");
    assert!(robot_hits >= 2, "and as the robot");
    assert!(hits_on_b(&ev).len() >= 4);
}

#[test]
fn a_combined_giant_fights_as_the_colossus() {
    let mut w = world();
    place(&mut w, A, Vec3::new(-30.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(-30.0, GROUND, 0.0));
    w.tick(&[]);
    let eye = w.player(A).unwrap().eye(w.params());
    w.cfg.rules.limits = bd_sim::rules::ConstructLimits::chaos();
    let dozer = feats(recipe("transforming bulldozer", Form::Vehicle, &[Mechanic::Charge, Mechanic::Heavy, Mechanic::Transforms], Element::Light, 4.0), &[Feature::Treads, Feature::Cab, Feature::Plow], 1500.0);
    let mut ids = Vec::new();
    for k in 0..3 {
        let d = Vec3::new(-0.3 + 0.3 * k as f32, -0.3, -1.0).normalize();
        ids.push(w.cast(A, dozer.clone(), None, Aim::new(eye, d)).unwrap());
    }
    for _ in 0..240 {
        w.tick(&[]);
    }
    let mut say = Recipe::unknown("combine");
    say.name = "combine".into();
    assert_eq!(w.constructs.iter().filter(|c| c.owner == A && c.gone.is_none()).count(), 3, "{ids:?}");
    let _ = w.cast(A, say, None, Aim::new(eye, Vec3::Y));
    let mut ev = Vec::new();
    // The combined giant lives ~25 s: watch it while it fights.
    let mut giant = None;
    for k in 0..(24 * 120) {
        if k % 90 == 0 {
            place(&mut w, B, Vec3::new(-30.0, GROUND, 0.0));
            w.player_mut(B).unwrap().damage_pct = 0.0;
        }
        ev.extend_from_slice(w.tick(&[]));
        if let Some(c) = w.constructs.iter().find(|c| c.comp.xform.combined) {
            assert_eq!(CombatTuning::default().transformer.robot_row(c).name, "colossus");
            giant = Some((c.id, c.brawl.ai_xattacks, c.brawl.last_xattack));
        }
    }
    let Some((_gid, xattacks, last)) = giant else { panic!("combined") };
    eprintln!("combined giant: {xattacks} quakes, last {last:?}, hits on B {}", hits_on_b(&ev).len());
    assert!(hits_on_b(&ev).len() >= 3, "it fights");
    assert!(xattacks >= 1 && last == Some(XAttack::Quake), "and quakes");
}
