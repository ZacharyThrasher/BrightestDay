//! PILOTING (COMBAT.md §5d-ter): boarding an own construct by command ("get in" → a recipe named
//! "board") and by the Transform key on foot, driving, transforming while seated, ejecting (E) into a
//! follower that fights on its own, enemies never boarding, default-piloted spawns, arcade flight
//! (jets / helicopters / flying transformers), and host = prediction for the pilot.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Feature, Form, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::constructs::{Controller, controller_of};
use bd_sim::events::{CastError, GameEvent, HeldFx};
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, TargetHint, World};

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

fn run(w: &mut World, ticks: u32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..ticks {
        ev.extend_from_slice(w.tick(&[]));
    }
    ev
}

fn drive(w: &mut World, id: PlayerId, inp: PlayerInput, ticks: u32) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..ticks {
        ev.extend_from_slice(w.tick(&[(id, inp)]));
    }
    ev
}

fn has_fx(ev: &[GameEvent], fx: HeldFx) -> bool {
    ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: f, .. } if *f == fx))
}

fn with_feats(mut r: Recipe, f: &[Feature]) -> Recipe {
    r.features = f.to_vec();
    r.normalized()
}

/// Optimus-like: a 6 m truck that transforms (a follower unless Piloted).
fn optimus(piloted: bool) -> Recipe {
    let mut m = vec![Mechanic::Blade, Mechanic::Shoots, Mechanic::Charge, Mechanic::Transforms];
    if piloted {
        m.push(Mechanic::Piloted);
    }
    with_feats(recipe("Optimus Prime", Form::Vehicle, &m, Element::Light, 6.0), &[Feature::Wheels, Feature::Cab])
}

fn starscream() -> Recipe {
    let m = [Mechanic::Shoots, Mechanic::Flies, Mechanic::Fast, Mechanic::Transforms, Mechanic::Piloted];
    with_feats(recipe("Starscream", Form::Vehicle, &m, Element::Light, 6.0), &[Feature::Jet, Feature::Cockpit, Feature::Fins])
}

/// The lexicon's board command ("get in", "ride it", "take the wheel"): `bd_lexicon::BOARD_COMMAND`.
fn board_cmd() -> Recipe {
    let mut r = Recipe::unknown("board");
    r.mechanics = vec![Mechanic::Piloted];
    r.normalized()
}

fn eye(w: &World, id: PlayerId) -> Vec3 {
    w.player(id).unwrap().eye(w.params())
}

/// A cast straight ahead (−Z) from `id`'s eye, optionally aimed at a construct.
fn cast(w: &mut World, id: PlayerId, r: Recipe, hint: Option<TargetHint>) -> Result<ConstructId, CastError> {
    let e = eye(w, id);
    let mut aim = Aim::new(e, Vec3::new(0.0, -0.25, -1.0).normalize());
    aim.target_hint = hint;
    w.cast(id, r, None, aim)
}

fn con(w: &World, id: ConstructId) -> &bd_sim::constructs::Construct {
    w.constructs.get(id).expect("construct alive")
}

fn seated_in(w: &World, id: PlayerId) -> Option<ConstructId> {
    w.player(id).and_then(|p| p.seat).map(|s| s.construct)
}

/// Yaw / pitch that make `id` look at `at`.
fn look_at(w: &World, id: PlayerId, at: Vec3) -> (f32, f32) {
    let d = at - eye(w, id);
    let yaw = (-d.x).atan2(-d.z);
    let pitch = (d.y / d.length().max(1e-3)).asin();
    (yaw, pitch)
}

fn set_look(w: &mut World, id: PlayerId, yaw: f32, pitch: f32) {
    let p = w.player_mut(id).unwrap();
    p.input.yaw = yaw;
    p.input.pitch = pitch;
}

/// A follower Optimus parked in front of A (A at the origin lane, far from B).
fn parked_follower() -> (World, ConstructId) {
    let mut w = world();
    place(&mut w, A, Vec3::new(-30.0, GROUND, 30.0));
    place(&mut w, B, Vec3::new(40.0, GROUND, -40.0));
    run(&mut w, 10);
    let id = cast(&mut w, A, optimus(false), None).unwrap();
    run(&mut w, 60);
    assert_eq!(seated_in(&w, A), None, "a follower cast doesn't seat");
    assert_ne!(controller_of(con(&w, id)), Controller::Piloted);
    // Park it (it would roam toward B otherwise): B far away, and zero its motion.
    (w, id)
}

#[test]
fn board_by_command_then_eject_then_board_by_key() {
    let (mut w, id) = parked_follower();
    // "get in" aimed at it: board.
    let got = cast(&mut w, A, board_cmd(), Some(TargetHint::Construct(id)));
    assert_eq!(got, Ok(id), "the board command seats the owner");
    assert_eq!(seated_in(&w, A), Some(id));
    assert!(con(&w, id).recipe.has(Mechanic::Piloted), "boarding grafts Piloted (replicated)");
    assert_eq!(con(&w, id).pilot, Some(A));
    let ev = run(&mut w, 2);
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::CastFailed { .. })));
    assert_eq!(controller_of(con(&w, id)), Controller::Piloted);
    assert!(con(&w, id).held.third_person);
    // Drive forward.
    let p0 = con(&w, id).pos;
    let yaw = w.player(A).unwrap().input.yaw;
    drive(&mut w, A, PlayerInput { move_z: 1, yaw, ..Default::default() }, 90);
    let moved = (con(&w, id).pos - p0).length();
    assert!(moved > 8.0, "drove {moved} m");
    // E: eject with the launch → follower.
    let ev = drive(&mut w, A, PlayerInput { buttons: buttons::TETHER_R, yaw, ..Default::default() }, 1);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::Ejected { player, .. } if *player == A)));
    assert_eq!(seated_in(&w, A), None);
    assert!(w.player(A).unwrap().mv.vel.y > 10.0, "the eject launch");
    assert_ne!(controller_of(con(&w, id)), Controller::Piloted, "hopped out: its own AI drives it");
    run(&mut w, 90);
    // Walk up to it and press the Transform key (no pilot → boards).
    let c = con(&w, id);
    let side = c.pos + Vec3::new(c.half.x + 1.5, 0.0, 0.0);
    let center = c.center();
    place(&mut w, A, Vec3::new(side.x, GROUND, side.z));
    run(&mut w, 5);
    let (yaw, pitch) = look_at(&w, A, center);
    set_look(&mut w, A, yaw, pitch);
    assert_eq!(w.board_candidate(A), Some(id), "the glow and the key agree");
    assert!(w.toggle_transform(A), "the Transform key boards on foot");
    assert_eq!(seated_in(&w, A), Some(id));
    assert!(w.board_candidate(A).is_none(), "seated: nothing more to board");
}

#[test]
fn transform_while_seated_mid_jump_then_keep_piloting() {
    let (mut w, id) = parked_follower();
    cast(&mut w, A, board_cmd(), Some(TargetHint::Construct(id))).unwrap();
    run(&mut w, 5);
    assert!(con(&w, id).held.vehicle, "boarded a truck: vehicle mode");
    // Off a jump: throw it up, press Transform mid-air → robot, landing slam.
    w.constructs.get_mut(id).unwrap().vel.y = 14.0;
    w.constructs.get_mut(id).unwrap().grounded = false;
    drive(&mut w, A, PlayerInput::default(), 4);
    assert!(w.toggle_transform(A), "transform key while seated transforms");
    let mut ev = Vec::new();
    for _ in 0..200 {
        ev.extend_from_slice(w.tick(&[(A, PlayerInput::default())]));
    }
    assert!(has_fx(&ev, HeldFx::Transform));
    assert!(has_fx(&ev, HeldFx::TransformSlam), "mid-jump transform lands in a slam");
    let c = con(&w, id);
    assert!(!c.held.vehicle, "robot mode");
    assert_eq!(c.comp.xform.mode, bd_sim::constructs::components::XMode::Robot, "the follower machine agrees (no double event)");
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Transformed { .. })), "one transform animation: the pilot's");
    assert_eq!(seated_in(&w, A), Some(id), "still piloting");
    // Walk the robot.
    let p0 = con(&w, id).pos;
    let yaw = w.player(A).unwrap().input.yaw;
    drive(&mut w, A, PlayerInput { move_z: 1, yaw, ..Default::default() }, 60);
    assert!((con(&w, id).pos - p0).length() > 2.0, "the robot walks");
    // Hop out in robot mode: the follower carries on as a robot…
    drive(&mut w, A, PlayerInput { buttons: buttons::TETHER_R, yaw, ..Default::default() }, 1);
    assert_eq!(con(&w, id).comp.xform.mode, bd_sim::constructs::components::XMode::Robot);
    assert_eq!(con(&w, id).pilot, None);
    // …and boarding it again right away keeps the robot mode.
    cast(&mut w, A, board_cmd(), Some(TargetHint::Construct(id))).unwrap();
    assert!(!con(&w, id).held.vehicle, "boarded in robot mode");
    // With no enemy around, a hopped-out transformer rolls out as a vehicle to roam (its own AI).
    drive(&mut w, A, PlayerInput { yaw, ..Default::default() }, 80); // the eject cooldown
    drive(&mut w, A, PlayerInput { buttons: buttons::TETHER_R, yaw, ..Default::default() }, 1);
    assert_eq!(seated_in(&w, A), None);
    run(&mut w, 600);
    assert_eq!(con(&w, id).comp.xform.mode, bd_sim::constructs::components::XMode::Alt, "the follower decides its own mode");
    cast(&mut w, A, board_cmd(), Some(TargetHint::Construct(id))).unwrap();
    assert!(con(&w, id).held.vehicle, "boarded in vehicle mode");
}

#[test]
fn enemies_cannot_board_and_the_owner_must_be_close_and_alive() {
    let (mut w, id) = parked_follower();
    // B right next to A's construct.
    let c = con(&w, id);
    let side = c.pos + Vec3::new(-(c.half.x + 1.5), 0.0, 0.0);
    let center = c.center();
    place(&mut w, B, Vec3::new(side.x, GROUND, side.z));
    run(&mut w, 5);
    let (yaw, pitch) = look_at(&w, B, center);
    set_look(&mut w, B, yaw, pitch);
    assert!(w.board_candidate(B).is_none());
    assert!(!w.toggle_transform(B), "an enemy's Transform key doesn't board your construct");
    assert!(w.board(B, id, 10.0).is_err());
    assert!(w.board_command(B, Some(id)).is_err(), "an enemy's \"get in\" fizzles");
    assert_eq!(con(&w, id).pilot, None);
    // The owner too far away: the key does nothing, the command range is bigger.
    place(&mut w, A, center + Vec3::new(0.0, 0.0, 60.0));
    run(&mut w, 3);
    assert!(w.board_candidate(A).is_none());
    assert_eq!(w.board_command(A, Some(id)), Err(CastError::Fizzled), "out of range");
    // Dead owners don't board.
    place(&mut w, A, center + Vec3::new(8.0, 0.0, 0.0));
    run(&mut w, 3);
    w.player_mut(A).unwrap().alive = false;
    assert!(w.board_command(A, Some(id)).is_err());
}

#[test]
fn default_piloted_cast_seats_and_hop_out_makes_a_follower_that_hunts() {
    let mut w = world();
    place(&mut w, A, Vec3::new(-20.0, GROUND, 20.0));
    place(&mut w, B, Vec3::new(-20.0, GROUND, -25.0));
    run(&mut w, 10);
    // A plain "car" from the lexicon carries Piloted by default.
    let car = with_feats(recipe("car", Form::Vehicle, &[Mechanic::Charge, Mechanic::Fast, Mechanic::Piloted], Element::Light, 4.0), &[Feature::Wheels]);
    let id = cast(&mut w, A, car, None).unwrap();
    assert_eq!(seated_in(&w, A), Some(id), "spawned seated");
    assert!(!w.player(A).unwrap().seat.unwrap().suit, "a car is a cockpit, not a suit");
    // Seated at spawn: no fall / stuck glitch — the pilot rides the seat while it forms.
    let mut lowest = f32::MAX;
    for _ in 0..90 {
        w.tick(&[(A, PlayerInput::default())]);
        lowest = lowest.min(w.player(A).unwrap().mv.pos.y);
    }
    assert!(lowest > GROUND - 0.5, "the seated pilot never falls through ({lowest})");
    // Hop out → it hunts B.
    let yaw = w.player(A).unwrap().input.yaw;
    drive(&mut w, A, PlayerInput { buttons: buttons::TETHER_R, yaw, ..Default::default() }, 1);
    let d0 = (con(&w, id).pos - w.player(B).unwrap().mv.pos).length();
    let mut closest = d0;
    for _ in 0..240 {
        w.tick(&[]);
        if let Some(c) = w.constructs.get(id) {
            closest = closest.min((c.pos - w.player(B).unwrap().mv.pos).length());
        }
    }
    assert!(closest < d0 - 10.0, "the follower car drives at the enemy: {d0} → {closest}");
    // A horse is ridden (a cockpit walker), not worn.
    let horse = recipe("horse", Form::Quadruped, &[Mechanic::Charge, Mechanic::Fast, Mechanic::Piloted], Element::Light, 1.8);
    run(&mut w, 30);
    let hid = cast(&mut w, A, horse, None).unwrap();
    assert_eq!(seated_in(&w, A), Some(hid));
    assert!(!w.player(A).unwrap().seat.unwrap().suit, "you ride a horse, you don't wear it");
}

#[test]
fn a_summon_while_seated_in_a_suit_is_a_follower() {
    let mut w = world();
    place(&mut w, A, Vec3::new(-20.0, GROUND, 20.0));
    run(&mut w, 10);
    let suit = cast(&mut w, A, recipe("iron man suit", Form::Biped, &[Mechanic::Piloted], Element::Light, 2.0), None).unwrap();
    assert!(w.player(A).unwrap().seat.is_some_and(|s| s.suit && s.construct == suit));
    run(&mut w, 30);
    let car = with_feats(recipe("car", Form::Vehicle, &[Mechanic::Charge, Mechanic::Piloted], Element::Light, 4.0), &[Feature::Wheels]);
    let id = cast(&mut w, A, car, None).unwrap();
    assert_eq!(seated_in(&w, A), Some(suit), "still in the suit");
    assert_eq!(con(&w, id).pilot, None, "the car runs as a follower");
}

// ------------------------------------------------------------------------------------ flight

fn pilot_in(r: Recipe, at: Vec3) -> (World, ConstructId) {
    let mut w = world();
    place(&mut w, A, at);
    place(&mut w, B, Vec3::new(40.0, GROUND, 40.0));
    run(&mut w, 10);
    let id = cast(&mut w, A, r, None).unwrap();
    assert_eq!(seated_in(&w, A), Some(id));
    run(&mut w, 60); // forming
    (w, id)
}

#[test]
fn piloted_starscream_jet_mode_takes_off_and_flies_forward() {
    let (mut w, id) = pilot_in(starscream(), Vec3::new(-30.0, GROUND, 40.0));
    assert!(con(&w, id).held.vehicle, "starts as the jet");
    let p0 = con(&w, id).pos;
    // Throttle up, aim a little above the horizon, toward −Z.
    let inp = PlayerInput { move_z: 1, yaw: 0.0, pitch: 0.35, ..Default::default() };
    drive(&mut w, A, inp, 150);
    let c = con(&w, id);
    let climbed = c.pos.y - p0.y;
    let forward = p0.z - c.pos.z;
    eprintln!("starscream: climbed {climbed:.1} m, forward {forward:.1} m, speed {:.1} m/s", c.vel.length());
    assert!(climbed > 6.0, "gains altitude: {climbed}");
    assert!(forward > 25.0, "flies forward: {forward}");
    assert!(c.vel.length() > 15.0, "jet speed");
    // Pilot rides with it.
    assert!((w.player(A).unwrap().mv.pos - c.seat_pos(0)).length() < 0.5);
    // Transform to robot mid-air → falls → slam on landing.
    assert!(w.toggle_transform(A));
    let mut ev = Vec::new();
    for _ in 0..400 {
        ev.extend_from_slice(w.tick(&[(A, PlayerInput::default())]));
        if has_fx(&ev, HeldFx::TransformSlam) {
            break;
        }
    }
    assert!(!con(&w, id).held.vehicle, "robot");
    assert!(has_fx(&ev, HeldFx::TransformSlam), "robot falls and slams");
}

#[test]
fn robot_to_jet_mid_air_takes_off() {
    let (mut w, id) = pilot_in(starscream(), Vec3::new(-30.0, GROUND, 40.0));
    // Robot on the ground, jump + hover-boost, then transform to the jet mid-air.
    assert!(w.toggle_transform(A));
    run(&mut w, 70);
    assert!(!con(&w, id).held.vehicle);
    let y0 = con(&w, id).pos.y;
    drive(&mut w, A, PlayerInput { buttons: buttons::JUMP, ..Default::default() }, 120);
    assert!(con(&w, id).pos.y > y0 + 2.0, "robot hovers on held Jump: {}", con(&w, id).pos.y - y0);
    assert!(w.toggle_transform(A));
    let inp = PlayerInput { move_z: 1, pitch: 0.2, ..Default::default() };
    drive(&mut w, A, inp, 120);
    let c = con(&w, id);
    assert!(c.held.vehicle && c.vel.length() > 12.0 && c.pos.y > y0 + 3.0, "the jet flies off: speed {} y {}", c.vel.length(), c.pos.y - y0);
}

#[test]
fn fighter_jet_flies_and_helicopter_hovers() {
    let jet = with_feats(recipe("Fighter Jet", Form::Vehicle, &[Mechanic::Shoots, Mechanic::Thrust, Mechanic::Flies, Mechanic::Fast, Mechanic::Piloted], Element::Light, 7.0), &[Feature::Jet, Feature::Cockpit]);
    let (mut w, id) = pilot_in(jet, Vec3::new(-30.0, GROUND, 40.0));
    let p0 = con(&w, id).pos;
    drive(&mut w, A, PlayerInput { move_z: 1, pitch: 0.3, ..Default::default() }, 150);
    let c = con(&w, id);
    assert!(c.pos.y > p0.y + 6.0 && p0.z - c.pos.z > 25.0, "fighter jet flies: {:?} → {:?}", p0, c.pos);
    // A jet with no input keeps flying (minimum speed), never stalls into the ground.
    drive(&mut w, A, PlayerInput { pitch: 0.0, ..Default::default() }, 60);
    assert!(con(&w, id).vel.length() > 8.0, "keeps a minimum airspeed");

    let heli = with_feats(recipe("helicopter", Form::Vehicle, &[Mechanic::Shoots, Mechanic::Flies, Mechanic::Spin, Mechanic::Piloted], Element::Light, 4.0), &[Feature::Rotor, Feature::Cockpit]);
    let (mut w, id) = pilot_in(heli, Vec3::new(-30.0, GROUND, 40.0));
    drive(&mut w, A, PlayerInput { buttons: buttons::JUMP, ..Default::default() }, 120);
    let y1 = con(&w, id).pos.y;
    assert!(y1 > GROUND + 4.0, "Space climbs: {y1}");
    drive(&mut w, A, PlayerInput::default(), 120); // settle
    let y2 = con(&w, id).pos.y;
    assert!(y2 - y1 < 3.0, "climb stops soon after Space is released: {}", y2 - y1);
    drive(&mut w, A, PlayerInput::default(), 240);
    let (y3, v3) = (con(&w, id).pos.y, con(&w, id).vel.length());
    assert!((y3 - y2).abs() < 0.3 && v3 < 0.3, "hovers in place: dy {} v {}", y3 - y2, v3);
    let y2 = y3;
    drive(&mut w, A, PlayerInput { buttons: buttons::CROUCH, ..Default::default() }, 60);
    assert!(con(&w, id).pos.y < y2 - 1.0, "Ctrl descends");
}

#[test]
fn tanks_and_cars_still_roll() {
    let tank = with_feats(recipe("tank", Form::Vehicle, &[Mechanic::Shoots, Mechanic::Charge, Mechanic::Heavy, Mechanic::Piloted], Element::Light, 4.0), &[Feature::Treads, Feature::Turret]);
    let (mut w, id) = pilot_in(tank, Vec3::new(-30.0, GROUND, 40.0));
    run(&mut w, 30);
    let p0 = con(&w, id).pos;
    let c = con(&w, id);
    assert!((c.bounds().min.y - GROUND).abs() < 0.3, "a piloted tank sits ON the ground: bottom {}", c.bounds().min.y);
    drive(&mut w, A, PlayerInput { move_z: 1, buttons: buttons::JUMP, pitch: 0.5, ..Default::default() }, 120);
    let c = con(&w, id);
    assert!(p0.z - c.pos.z > 8.0, "the tank drives");
    assert!((c.pos.y - p0.y).abs() < 0.6, "and stays on the ground: {}", c.pos.y - p0.y);
}

// ------------------------------------------------------------------------------------ prediction

struct Pair {
    host: World,
    client: World,
}

impl Pair {
    fn sync(&mut self, evs: &[GameEvent]) {
        for e in evs {
            self.client.apply_event(e);
        }
        self.client.apply_snapshot(&self.host.snapshot_for(Some(A)), Some(A));
    }
    fn step(&mut self, inp: PlayerInput) -> (f32, f32) {
        let mut s = self.client.player(A).unwrap().mv;
        self.client.predict_player(A, &mut s, &inp);
        let evs = self.host.tick(&[(A, inp)]).to_vec();
        self.sync(&evs);
        let h = self.host.player(A).unwrap().mv;
        ((h.pos - s.pos).length(), (h.vel - s.vel).length())
    }
}

#[test]
fn boarding_is_replicated_and_prediction_agrees_with_the_host() {
    // A titan-style ride (a big walker: the pilot's own state drives it, predicted) and a truck.
    let mech = with_feats(recipe("mech", Form::Kaiju, &[Mechanic::Shoots, Mechanic::Stomp, Mechanic::Heavy], Element::Light, 8.0), &[Feature::Cockpit, Feature::Pistons]);
    for (r, titan) in [(mech, true), (optimus(false), false)] {
        let mut host = world();
        place(&mut host, A, Vec3::new(-30.0, GROUND, 30.0));
        place(&mut host, B, Vec3::new(40.0, GROUND, -40.0));
        run(&mut host, 10);
        let id = cast(&mut host, A, r, None).unwrap();
        run(&mut host, 60);
        let mut client = World::new(cfg());
        for e in host.construct_manifest() {
            client.apply_event(&e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        let mut p = Pair { host, client };
        // Board by command (host), replicate.
        let got = cast(&mut p.host, A, board_cmd(), Some(TargetHint::Construct(id)));
        assert_eq!(got, Ok(id));
        let evs = p.host.tick(&[]).to_vec(); // the command's events (Grafted, Seated) ride this tick
        assert!(evs.iter().any(|e| matches!(e, GameEvent::Seated { player, construct, .. } if *player == A && *construct == id)));
        p.sync(&evs);
        assert_eq!(p.client.player(A).unwrap().seat.map(|s| s.construct), Some(id), "the seat replicates");
        assert!(p.client.constructs.get(id).is_some_and(|c| c.recipe.has(Mechanic::Piloted)), "Piloted graft replicates");
        assert_eq!(bd_sim::constructs::titan::walks(p.client.constructs.get(id).unwrap()), titan);
        let (mut worst_dp, mut worst_dv) = (0.0f32, 0.0f32);
        for k in 0..120u32 {
            let inp = PlayerInput { move_z: if k < 80 { 1 } else { 0 }, move_x: if k % 40 < 20 { 1 } else { 0 }, yaw: 0.3, ..Default::default() };
            let (dp, dv) = p.step(inp);
            if k > 5 {
                worst_dp = worst_dp.max(dp);
                worst_dv = worst_dv.max(dv);
            }
        }
        eprintln!("board parity (titan {titan}): worst {worst_dp:.4} m, {worst_dv:.4} m/s");
        if titan {
            assert!(worst_dp < 0.02 && worst_dv < 0.05, "host = prediction while piloting a boarded walker");
        } else {
            // A cockpit vehicle is host-driven: prediction must not move the pilot on its own
            // (the seat comes from the snapshot), so it can only lag by what the host moved this tick.
            assert!(worst_dp < 1.5, "prediction never fights the seat: {worst_dp}");
        }
    }
}

#[test]
fn piloting_scenarios_are_deterministic() {
    let once = || {
        let (mut w, id) = parked_follower();
        cast(&mut w, A, board_cmd(), Some(TargetHint::Construct(id))).unwrap();
        let mut log = Vec::new();
        for k in 0..240u32 {
            let inp = PlayerInput { move_z: 1, yaw: (k as f32 * 0.01).sin(), buttons: if k == 60 || k == 200 { buttons::TETHER_R } else { 0 }, ..Default::default() };
            if k == 120 {
                w.toggle_transform(A);
            }
            log.push(format!("{:?}", w.tick(&[(A, inp)])));
        }
        (log, w.state_hash())
    };
    assert_eq!(once(), once());
}
