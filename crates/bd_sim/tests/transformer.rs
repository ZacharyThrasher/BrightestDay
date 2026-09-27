//! TRANSFORMER COMBAT (COMBAT.md §5d-quater, `constructs/brawl.rs`): archetypes from the alt mode's
//! data, robot strings, transform attacks (each archetype's shape), the air slam, the perfect-dodge
//! counter, AI followers using transform attacks, host = prediction for the pilot, determinism.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Feature, Form, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::combat::ConstructHit;
use bd_sim::constructs::brawl::{XAttack, XPhase};
use bd_sim::events::{GameEvent, HeldFx};
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
fn starscream() -> Recipe {
    feats(recipe("Starscream", Form::Vehicle, &[Mechanic::Shoots, Mechanic::Flies, Mechanic::Fast, Mechanic::Transforms], Element::Light, 6.0), &[Feature::Jet, Feature::Cockpit, Feature::Fins], 3500.0)
}
fn bumblebee() -> Recipe {
    feats(recipe("Bumblebee", Form::Vehicle, &[Mechanic::Shoots, Mechanic::Fast, Mechanic::Transforms], Element::Light, 4.2), &[Feature::Wheels, Feature::Headlight], 1600.0)
}
fn tank() -> Recipe {
    feats(recipe("transforming tank", Form::Vehicle, &[Mechanic::Shoots, Mechanic::Charge, Mechanic::Heavy, Mechanic::Transforms], Element::Light, 4.0), &[Feature::Treads, Feature::Turret, Feature::Barrels, Feature::Armor], 1500.0)
}
fn heli() -> Recipe {
    feats(recipe("transforming helicopter", Form::Vehicle, &[Mechanic::Shoots, Mechanic::Flies, Mechanic::Charge, Mechanic::Spin, Mechanic::Transforms], Element::Light, 4.0), &[Feature::Rotor, Feature::Cockpit, Feature::Windows], 1500.0)
}
fn bike() -> Recipe {
    feats(recipe("motorcycle transformer", Form::Biped, &[Mechanic::Charge, Mechanic::Fast, Mechanic::Transforms], Element::Light, 4.5), &[Feature::Wheels, Feature::Exhaust, Feature::Headlight], 4000.0)
}
fn dozer() -> Recipe {
    feats(recipe("transforming bulldozer", Form::Vehicle, &[Mechanic::Charge, Mechanic::Heavy, Mechanic::Transforms], Element::Light, 4.0), &[Feature::Treads, Feature::Cab, Feature::Plow], 1500.0)
}

#[test]
fn archetypes_come_from_the_alt_mode_data() {
    let t = CombatTuning::default().transformer;
    let helicopter_robot = feats(recipe("helicopter robot", Form::Biped, &[Mechanic::Shoots], Element::Light, 1.8), &[Feature::Gears, Feature::Rotor, Feature::Antenna], 80.0);
    for (r, want) in [
        (starscream(), "jet"),
        (optimus(), "truck"),
        (bumblebee(), "car"),
        (tank(), "tank"),
        (helicopter_robot, "helicopter"),
        (bike(), "bike"),
        (dozer(), "construction"),
        (heli(), "helicopter"),
    ] {
        assert_eq!(t.archetype_of(&r).name, want, "{}", r.name);
    }
    // Every rule points at a row, and the shipped combat.ron equals the defaults (tests/tuning.rs).
    for rule in &t.rules {
        assert!(t.archetypes.iter().any(|a| a.name == rule.to), "rule → {}", rule.to);
    }
}

/// A seated pilot in `r`, in robot mode, with B standing `gap` m in front of it (+ its reach).
fn robot_duel(r: Recipe, gap: f32) -> (World, ConstructId) {
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
    for _ in 0..40 {
        w.tick(&[(A, PlayerInput::default())]);
    }
    if w.constructs.get(id).unwrap().held.vehicle {
        assert!(w.toggle_transform(A));
        for _ in 0..180 {
            w.tick(&[(A, PlayerInput::default())]);
        }
    }
    assert!(!w.constructs.get(id).unwrap().held.vehicle, "robot mode");
    put_b_in_front(&mut w, id, gap);
    (w, id)
}

fn put_b_in_front(w: &mut World, id: ConstructId, gap: f32) {
    let c = w.constructs.get(id).unwrap();
    let front = c.pos + Vec3::NEG_Z * (c.half.x.max(c.half.z) + gap);
    place(w, B, Vec3::new(front.x, GROUND, front.z));
    let p = w.player_mut(B).unwrap();
    p.damage_pct = 0.0;
}

fn tick_a(w: &mut World, inp: PlayerInput, n: u32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..n {
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

#[allow(non_snake_case)]
fn LOOK() -> PlayerInput {
    PlayerInput::default()
}

fn press(b: u16) -> PlayerInput {
    PlayerInput { buttons: b, ..LOOK() }
}

/// V2: land the whole string (B put back in front before every hit) until its ender CONNECTS and
/// opens the flash window — the moment Transform is the transform attack.
fn string_to_flash(w: &mut World, id: ConstructId) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..8 {
        let a = CombatTuning::default().transformer.archetype_of(&w.constructs.get(id).unwrap().recipe).clone();
        ev.extend(tap(w, id, 0));
        for _ in 0..(a.string_interval * 120.0) as u32 + 2 {
            if w.constructs.get(id).unwrap().brawl.flash > 0.0 {
                return ev;
            }
            ev.extend(tick_a(w, LOOK(), 1));
        }
    }
    panic!("the string's ender never opened the flash window");
}

/// One LMB tap (press 1 tick, release), then `after` idle ticks. B is put back in front first.
fn tap(w: &mut World, id: ConstructId, after: u32) -> Vec<GameEvent> {
    put_b_in_front(w, id, 1.0);
    let mut ev = tick_a(w, press(buttons::PRIMARY), 1);
    ev.extend(tick_a(w, LOOK(), after));
    ev
}

#[test]
fn a_robot_string_lands_every_hit_and_ends_stronger() {
    let (mut w, id) = robot_duel(optimus(), 1.0);
    let t = CombatTuning::default().transformer;
    let a = t.archetype_of(&optimus()).clone();
    let gap = (a.string_interval * 120.0) as u32 + 2;
    let mut kbs = Vec::new();
    for _ in 0..a.string_hits {
        let ev = tap(&mut w, id, gap);
        let h = hits_on_b(&ev);
        assert_eq!(h.len(), 1, "each string hit connects once");
        kbs.push(h[0]);
    }
    eprintln!("truck string knockbacks: {kbs:?}");
    assert_eq!(kbs.len(), a.string_hits as usize);
    let c = w.constructs.get(id).unwrap();
    assert!(c.brawl.string_done && c.brawl.open > 0.0, "the ender opens the combo window");
    assert!(kbs.last().unwrap() > &kbs[0], "the ender hits harder");
}

#[test]
fn a_transform_ram_mid_combo_hits_and_returns_to_robot() {
    let (mut w, id) = robot_duel(optimus(), 1.0);
    let ev = string_to_flash(&mut w, id);
    assert!(!hits_on_b(&ev).is_empty());
    // Transform in the flash window (the string's ender connected): the vehicle bursts through B.
    put_b_in_front(&mut w, id, 4.0);
    let z0 = w.constructs.get(id).unwrap().pos.z;
    assert!(w.toggle_transform(A));
    let mut ev = tick_a(&mut w, LOOK(), 2);
    {
        let c = w.constructs.get(id).unwrap();
        assert_eq!(c.brawl.xatk, XPhase::Burst, "a transform attack, not a plain transform");
        assert!(c.held.vehicle, "burst out as the vehicle");
    }
    ev.extend(tick_a(&mut w, LOOK(), 200));
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.xatk, XPhase::None);
    assert!(!c.held.vehicle, "back to robot");
    assert_eq!(c.brawl.last_xattack, Some(XAttack::Ram), "the truck's shape: a freight-train ram");
    assert!(c.brawl.last_xattack_hits >= 2, "ram + finisher hit: {}", c.brawl.last_xattack_hits);
    assert!(z0 - c.pos.z > 6.0, "rammed THROUGH along the travel: {}", z0 - c.pos.z);
    assert!(hits_on_b(&ev).len() >= 2);
    // Fast combat transforms: clients play the panel sequence faster (|strength| > 1).
    assert!(ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::Transform, strength, .. } if strength.abs() > 2.0)));
    for e in ev.iter().filter(|e| matches!(e, GameEvent::Transformed { .. } | GameEvent::HeldFx { fx: HeldFx::Transform, .. })) {
        eprintln!("{e:?}");
    }
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Transformed { .. })), "no double transform animation");
}

#[test]
fn air_string_then_air_slam() {
    let (mut w, id) = robot_duel(starscream(), 1.0);
    // Launch the robot into the air, then keep tapping: air hits float it, the 4th slams.
    w.constructs.get_mut(id).unwrap().vel.y = 12.0;
    w.constructs.get_mut(id).unwrap().grounded = false;
    tick_a(&mut w, LOOK(), 10);
    let mut ev = Vec::new();
    for _ in 0..4 {
        ev.extend(tick_a(&mut w, press(buttons::PRIMARY), 1));
        ev.extend(tick_a(&mut w, LOOK(), 30));
    }
    ev.extend(tick_a(&mut w, LOOK(), 240));
    assert!(has_fx(&ev, HeldFx::DiveStart), "the air slam dives");
    assert!(has_fx(&ev, HeldFx::TransformSlam), "and lands in a slam");
}

#[test]
fn a_perfect_dodge_opens_a_staggering_counter() {
    let (mut w, id) = robot_duel(optimus(), 1.0);
    let hit = |w: &mut World| {
        let c = w.constructs.get(id).unwrap();
        w.apply_construct_hit(ConstructHit { target: id, source: Some(B), damage: 10.0, impulse: Vec3::ZERO, blade: false, pierce: false, blunt: true, point: c.center() })
    };
    // Not dodging: the hit lands.
    let integ = w.constructs.get(id).unwrap().integrity;
    hit(&mut w);
    assert!(w.constructs.get(id).unwrap().integrity < integ, "not dodging: the hit lands");
    // Dodge, and get hit in the window: dodged, a counter opens, a chrome flash for the dodger.
    let mut ev = tick_a(&mut w, press(buttons::DODGE), 1);
    let integ = w.constructs.get(id).unwrap().integrity;
    hit(&mut w);
    ev.extend(tick_a(&mut w, LOOK(), 1));
    assert_eq!(w.constructs.get(id).unwrap().integrity, integ, "a perfect dodge: no damage");
    assert!(has_fx(&ev, HeldFx::Parry), "the chrome flash cue");
    assert!(w.constructs.get(id).unwrap().brawl.counter > 0.0, "counter window open");
    // Late in a dodge (outside the window): no dodge.
    let late = w.constructs.get(id).unwrap().brawl.iframes;
    assert!(late <= 0.15);
    // The counter: the next attack is a guaranteed heavy that staggers.
    tick_a(&mut w, LOOK(), 20);
    put_b_in_front(&mut w, id, 1.0);
    let ev = tick_a(&mut w, press(buttons::PRIMARY), 1);
    let kb = hits_on_b(&ev);
    assert_eq!(kb.len(), 1);
    assert!(w.player(B).unwrap().status.stunned(), "the counter staggers");
    assert!(has_fx(&ev, HeldFx::Uppercut));
    assert_eq!(w.constructs.get(id).unwrap().brawl.counter, 0.0, "used up");
    // A pilot hit inside a dodge window is dodged too.
    tick_a(&mut w, LOOK(), 80);
    tick_a(&mut w, press(buttons::DODGE), 1);
    let before = w.player(A).unwrap().damage_pct;
    w.apply_hit(bd_sim::combat::HitInfo::new(A, Some(B), bd_sim::combat::HitKind::Shot, 10.0, 5.0, Vec3::X, Vec3::ZERO));
    assert_eq!(w.player(A).unwrap().damage_pct, before, "the seated pilot is dodged with the robot");
}

/// Pilot `r` in robot mode, land one string hit, press Transform, run the whole attack.
fn transform_attack(r: Recipe) -> (World, ConstructId, Vec<GameEvent>, Vec3, f32) {
    let name = r.name.clone();
    let (mut w, id) = robot_duel(r, 1.0);
    let mut ev = string_to_flash(&mut w, id);
    put_b_in_front(&mut w, id, 3.0);
    let (p0, y0) = (w.constructs.get(id).unwrap().pos, w.constructs.get(id).unwrap().pos.y);
    assert!(w.toggle_transform(A));
    let mut top = y0;
    for k in 0..240 {
        ev.extend(tick_a(&mut w, LOOK(), 1));
        top = top.max(w.constructs.get(id).unwrap().pos.y);
    }
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.xatk, XPhase::None, "{name}: done");
    assert!(!c.held.vehicle, "{name}: back to robot (last {:?}, open {})", c.brawl.last_xattack, c.brawl.open);
    (w, id, ev, p0, top - y0)
}

fn shots(ev: &[GameEvent], id: ConstructId) -> usize {
    ev.iter().filter(|e| matches!(e, GameEvent::ShotFired { construct: Some(c), .. } if *c == id)).count()
}

#[test]
fn each_archetype_transform_attack_has_its_shape() {
    // Truck: a straight ram line (tested above). Car: a drift-ram arc.
    let (w, id, ev, p0, _) = transform_attack(bumblebee());
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.last_xattack, Some(XAttack::DriftRam));
    let d = c.pos - p0;
    assert!(d.z < -4.0 && d.x.abs() > 1.5, "an arc (forward and sideways): {d:?}");
    assert!(!hits_on_b(&ev).is_empty());
    // Jet: a strafing fly-by (climbs, fires a line of bolts), then the dive-kick.
    let (w, id, ev, _, climbed) = transform_attack(starscream());
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.last_xattack, Some(XAttack::FlyBy));
    assert!(climbed > 2.0, "flies over: +{climbed} m");
    assert!(shots(&ev, id) >= 6, "a gun run: {} bolts", shots(&ev, id));
    assert!(has_fx(&ev, HeldFx::DiveStart), "dive-kick");
    // Tank: a planted shell barrage, then the cannon arm.
    let (w, id, ev, p0, _) = transform_attack(tank());
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.last_xattack, Some(XAttack::Barrage));
    assert!(shots(&ev, id) >= 5, "4 shells + the cannon blast: {}", shots(&ev, id));
    assert!((c.pos - p0).length() < 4.0, "planted");
    // Helicopter: a hovering rotor sweep (multi-hit around it), then a drop slam.
    let (w, id, ev, _, climbed) = transform_attack(heli());
    let c = w.constructs.get(id).unwrap();
    assert_eq!(c.brawl.last_xattack, Some(XAttack::RotorSweep));
    assert!(climbed > 0.3, "hovers up");
    assert!(hits_on_b(&ev).len() >= 3, "the sweep hits again and again: {}", hits_on_b(&ev).len());
    assert!(has_fx(&ev, HeldFx::TransformSlam), "drop slam");
    // Bike: a spin-out dash; construction: a grind (repeated contact hits).
    let (w, id, _, _, _) = transform_attack(bike());
    assert_eq!(w.constructs.get(id).unwrap().brawl.last_xattack, Some(XAttack::SpinOut));
    let (w, id, ev, _, _) = transform_attack(dozer());
    assert_eq!(w.constructs.get(id).unwrap().brawl.last_xattack, Some(XAttack::Grind));
    assert!(hits_on_b(&ev).len() >= 3, "grinds: {}", hits_on_b(&ev).len());
}

#[test]
fn ai_followers_use_strings_and_transform_attacks() {
    let mut w = world();
    place(&mut w, A, Vec3::new(-30.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(-30.0, GROUND, 10.0));
    w.tick(&[]);
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, optimus(), None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0).normalize())).unwrap();
    assert!(w.constructs.get(id).unwrap().pilot.is_none(), "a follower");
    let mut ev = Vec::new();
    let mut xattacks = 0;
    for k in 0..2400 {
        // B stays put (tethered to its spot) so the follower keeps engaging.
        if k % 60 == 0 {
            place(&mut w, B, Vec3::new(-30.0, GROUND, 10.0));
            w.player_mut(B).unwrap().damage_pct = 0.0;
        }
        ev.extend_from_slice(w.tick(&[]));
        let Some(c) = w.constructs.get(id) else { break };
        xattacks = c.brawl.ai_xattacks;
        if xattacks >= 2 {
            break;
        }
    }
    eprintln!("AI: {xattacks} transform attacks, {} hits on B", hits_on_b(&ev).len());
    assert!(xattacks >= 1, "the follower extends its strings with transform attacks");
    assert!(hits_on_b(&ev).len() >= 3, "and lands hits");
}

#[test]
fn transformer_combat_is_deterministic_and_prediction_never_fights_the_seat() {
    let run = || {
        let (mut w, id) = robot_duel(optimus(), 1.0);
        let mut client = World::new(cfg());
        for e in w.construct_manifest() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&w.snapshot_for(Some(A)), Some(A));
        let mut worst = 0.0f32;
        let mut log = Vec::new();
        for k in 0..360u32 {
            let b = match k % 40 {
                0 | 12 | 24 => buttons::PRIMARY,
                30 => buttons::DODGE,
                _ => 0,
            };
            let inp = PlayerInput { buttons: b, move_z: if k % 80 < 20 { 1 } else { 0 }, ..LOOK() };
            if k == 26 || k == 200 {
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
        let _ = id;
        (log, w.state_hash(), worst)
    };
    let (l1, h1, worst) = run();
    let (l2, h2, _) = run();
    assert_eq!(l1, l2);
    assert_eq!(h1, h2);
    eprintln!("prediction drift while brawling: {worst:.5}");
    assert!(worst < 1e-6, "a cockpit is host-driven: prediction never moves the seated pilot on its own ({worst}); the seat comes from the snapshot");
}
