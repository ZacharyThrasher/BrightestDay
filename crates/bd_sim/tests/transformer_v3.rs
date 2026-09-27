//! TRANSFORMERS V3 (COMBAT.md §5d-sexies): a piloted robot moves on the PILOT'S kit (bhop, air jump,
//! tethers), a forward boost-step steers at the chase target, vehicle mode is the sprint (launch
//! speed carry, flash-transform pop-out into melee range), alt modes carry weapons from their data
//! (tank turret on the crosshair, jet / heli guns, car boost-ram), and a scripted CHASE measures the
//! time to close 30 m to melee range on foot vs piloted robot vs vehicle (`chase_*`).

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Feature, Form, Mechanic, Recipe};
use bd_core::{ConstructId, PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::constructs::kit::{Armament, V3Tuning, armament};
use bd_sim::events::{GameEvent, HeldFx};
use bd_sim::movement::HookPhase;
use bd_sim::rules::WillpowerMode;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn world(size: f32) -> World {
    let mut w = World::new(SimConfig { arena: ArenaSource::Flat { size_m: size }, check_invariants: true, ..SimConfig::default() });
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
fn tank() -> Recipe {
    feats(recipe("transforming tank", Form::Vehicle, &[Mechanic::Shoots, Mechanic::Heavy, Mechanic::Transforms], Element::Fire, 5.0), &[Feature::Treads, Feature::Turret, Feature::Barrels], 20000.0)
}
fn jet() -> Recipe {
    feats(recipe("transforming jet", Form::Vehicle, &[Mechanic::Flies, Mechanic::Fast, Mechanic::Transforms], Element::Light, 5.0), &[Feature::Jet], 6000.0)
}
fn heli() -> Recipe {
    feats(recipe("transforming helicopter", Form::Vehicle, &[Mechanic::Flies, Mechanic::Transforms], Element::Light, 5.0), &[Feature::Rotor], 5000.0)
}
fn car(gun: bool) -> Recipe {
    let f: &[Feature] = if gun { &[Feature::Wheels, Feature::Barrels] } else { &[Feature::Wheels, Feature::Windows] };
    feats(recipe("transforming car", Form::Vehicle, &[Mechanic::Charge, Mechanic::Fast, Mechanic::Transforms], Element::Light, 4.5), f, 1200.0)
}

/// The V2 model (before V3): no kit, the slow throttle, no launch / pop-out / dash seek.
fn legacy() -> V3Tuning {
    V3Tuning { kit: false, vehicle_accel: 2.5, vehicle_launch: 0.0, vehicle_carry_max: 0.0, pop_range: -100.0, dash_seek_range: 0.0, ..V3Tuning::default() }
}

/// Cast `r` piloted by A at `at` (B far away). `robot` = switch to robot mode first.
fn piloted(w: &mut World, r: Recipe, at: Vec3, robot: bool) -> ConstructId {
    place(w, A, at + Vec3::Y * 1.0);
    w.tick(&[]);
    let mut r = r;
    r.mechanics.push(Mechanic::Piloted);
    let r = r.normalized();
    let eye = w.player(A).unwrap().eye(w.params());
    let id = w.cast(A, r, None, Aim::new(eye, Vec3::new(0.0, -0.3, -1.0).normalize())).unwrap();
    w.constructs.get_mut(id).unwrap().life = 1.0e6;
    for _ in 0..60 {
        w.tick(&[(A, PlayerInput::default())]);
    }
    let v = w.constructs.get(id).unwrap().held.vehicle;
    if v == robot {
        assert!(w.toggle_transform(A));
        for _ in 0..160 {
            w.tick(&[(A, PlayerInput::default())]);
        }
    }
    assert_eq!(w.constructs.get(id).unwrap().held.vehicle, !robot);
    assert_eq!(w.player(A).unwrap().seat.map(|s| s.construct), Some(id), "A pilots it");
    id
}

fn hspeed(v: Vec3) -> f32 {
    Vec3::new(v.x, 0.0, v.z).length()
}

fn yaw_to(d: Vec3) -> f32 {
    (-d.x).atan2(-d.z)
}

// ------------------------------------------------------------------------------------ 1. the kit

#[test]
fn robot_bhops_and_air_strafes_past_the_old_walker() {
    let mut top = [0.0f32; 2];
    for (k, legacy_model) in [false, true].into_iter().enumerate() {
        let mut w = world(256.0);
        if legacy_model {
            w.cfg.tuning.transformer.v3 = legacy();
        }
        let id = piloted(&mut w, optimus(), Vec3::new(-100.0, GROUND, 0.0), true);
        let h = w.constructs.get(id).unwrap().half.y * 2.0;
        assert!(h <= 5.6, "hero scale robot ({h} m)");
        // W + held jump (auto-hop) while air-strafing: the view sweeps 1.2 rad/s side to side with A/D.
        let mut yaw = -std::f32::consts::FRAC_PI_2; // facing +X
        for t in 0..(6 * 120) {
            let side = if (t / 60) % 2 == 0 { 1 } else { -1 };
            yaw -= side as f32 * 1.2 / 120.0;
            let inp = PlayerInput { yaw, move_z: 1, move_x: side, buttons: buttons::JUMP, ..PlayerInput::default() };
            w.tick(&[(A, inp)]);
            top[k] = top[k].max(hspeed(w.constructs.get(id).unwrap().vel));
        }
        if !legacy_model {
            assert!(w.constructs.get(id).unwrap().kit.is_some(), "robot mode runs the kit");
        }
    }
    eprintln!("robot top speed bhop+strafe: V3 {:.1} m/s, V2 {:.1} m/s", top[0], top[1]);
    assert!(top[0] >= 10.5, "V3 robot keeps a hop chain above run speed ({:.1})", top[0]);
    assert!(top[0] > top[1] * 1.3, "V3 {:.1} vs V2 {:.1}", top[0], top[1]);
}

#[test]
fn robot_has_the_air_jump() {
    let mut w = world(128.0);
    let id = piloted(&mut w, optimus(), Vec3::new(0.0, GROUND, 0.0), true);
    let jump = PlayerInput { buttons: buttons::JUMP, ..PlayerInput::default() };
    w.tick(&[(A, jump)]);
    let mut peak_vy = 0.0f32;
    for _ in 0..30 {
        w.tick(&[(A, PlayerInput::default())]);
    }
    let before = w.constructs.get(id).unwrap().vel.y;
    assert!(!w.constructs.get(id).unwrap().grounded, "airborne");
    for _ in 0..3 {
        w.tick(&[(A, jump)]);
        peak_vy = peak_vy.max(w.constructs.get(id).unwrap().vel.y);
    }
    assert!(peak_vy > before + 4.0 && peak_vy > 4.0, "an air jump in robot mode: vy {before:.1} → {peak_vy:.1}");
}

#[test]
fn robot_tethers_hook_terrain_and_reel_to_an_enemy() {
    let mut w = world(128.0);
    let id = piloted(&mut w, optimus(), Vec3::new(-30.0, GROUND, 0.0), true);
    // Hook the ground ahead (aim down at it).
    let inp = PlayerInput { yaw: yaw_to(Vec3::X), pitch: -0.5, buttons: buttons::TETHER_L, ..PlayerInput::default() };
    let mut attached = false;
    for _ in 0..60 {
        w.tick(&[(A, inp)]);
        let k = w.constructs.get(id).unwrap().kit.unwrap();
        attached |= k.hooks[0].phase == HookPhase::Attached;
    }
    assert!(attached, "the robot's tether anchors");
    assert!(w.player(A).unwrap().mv.hooks[0].phase == HookPhase::Attached, "the rope shows (mirrored on the pilot)");
    for _ in 0..10 {
        w.tick(&[(A, PlayerInput::default())]);
    }
    // Reel to an enemy: B stands 22 m ahead; aim at them, fire + reel.
    let c = w.constructs.get(id).unwrap();
    let from = c.center();
    place(&mut w, B, Vec3::new(from.x + 22.0, GROUND, from.z));
    w.tick(&[(A, PlayerInput::default())]);
    let d0 = hspeed(w.player(B).unwrap().mv.pos - w.constructs.get(id).unwrap().center());
    let mut on_b = false;
    for _ in 0..(2 * 120) {
        let c = w.constructs.get(id).unwrap();
        let d = w.player(B).unwrap().hull(w.params()).center() - (c.center() + Vec3::Y * c.half.y * 0.2);
        let pitch = (d.y / hspeed(d).max(0.1)).atan();
        let inp = PlayerInput { yaw: yaw_to(d), pitch, buttons: buttons::TETHER_L | buttons::REEL, ..PlayerInput::default() };
        w.tick(&[(A, inp)]);
        let k = w.constructs.get(id).unwrap().kit.unwrap();
        on_b |= matches!(k.hooks[0].target, Some(bd_sim::movement::AnchorTarget::Collider(bd_sim::movement::ColliderId::Player(p))) if p == B);
    }
    let d1 = hspeed(w.player(B).unwrap().mv.pos - w.constructs.get(id).unwrap().center());
    assert!(on_b, "the tether hooked B");
    assert!(d1 < d0 - 8.0, "reeled toward B: {d0:.1} → {d1:.1} m");
}

// ------------------------------------------------------------------------------------ 2. chase tools

#[test]
fn forward_boost_step_steers_at_the_target() {
    let mut w = world(128.0);
    let id = piloted(&mut w, optimus(), Vec3::new(-30.0, GROUND, 0.0), true);
    let c = w.constructs.get(id).unwrap().center();
    // B 18 m ahead at +X; the pilot looks at B but holds W + D (wish 45° off).
    place(&mut w, B, Vec3::new(c.x + 18.0, GROUND, c.z));
    w.tick(&[(A, PlayerInput::default())]);
    let d = w.player(B).unwrap().hull(w.params()).center() - w.constructs.get(id).unwrap().center();
    let inp = PlayerInput { yaw: yaw_to(d), move_z: 1, move_x: 1, buttons: buttons::DODGE, ..PlayerInput::default() };
    w.tick(&[(A, inp)]);
    let dir = w.constructs.get(id).unwrap().brawl.dodge_dir;
    let to_b = Vec3::new(d.x, 0.0, d.z).normalize();
    assert!(dir.dot(to_b) > 0.98, "dash steered at B: {dir:?} vs {to_b:?}");
    // Backward / sideways dodges stay what they were (a defensive step).
    for _ in 0..80 {
        w.tick(&[(A, PlayerInput { yaw: yaw_to(d), ..PlayerInput::default() })]);
    }
    w.tick(&[(A, PlayerInput { yaw: yaw_to(d), move_z: -1, buttons: buttons::DODGE, ..PlayerInput::default() })]);
    let dir = w.constructs.get(id).unwrap().brawl.dodge_dir;
    assert!(dir.dot(to_b) < -0.9, "a back-step stays a back-step");
}

#[test]
fn transforming_out_of_a_dash_launches_the_vehicle() {
    let mut w = world(128.0);
    let id = piloted(&mut w, optimus(), Vec3::new(-30.0, GROUND, 0.0), true);
    let inp = PlayerInput { yaw: yaw_to(Vec3::X), move_z: 1, buttons: buttons::DODGE, ..PlayerInput::default() };
    w.tick(&[(A, inp)]);
    w.tick(&[(A, PlayerInput { yaw: yaw_to(Vec3::X), move_z: 1, ..PlayerInput::default() })]);
    let before = hspeed(w.constructs.get(id).unwrap().vel);
    assert!(w.toggle_transform(A));
    w.tick(&[(A, PlayerInput { yaw: yaw_to(Vec3::X), move_z: 1, ..PlayerInput::default() })]);
    let c = w.constructs.get(id).unwrap();
    assert!(c.held.vehicle);
    let after = hspeed(c.vel);
    assert!(after >= before.min(24.0) - 0.5 && after >= 14.0, "speed carried into the vehicle: {before:.1} → {after:.1}");
}

#[test]
fn flash_transform_pops_out_into_melee_range() {
    let mut w = world(128.0);
    let id = piloted(&mut w, optimus(), Vec3::new(-30.0, GROUND, 0.0), false);
    let c = w.constructs.get(id).unwrap().center();
    place(&mut w, B, Vec3::new(c.x + 13.0, GROUND, c.z));
    let yaw = yaw_to(Vec3::X);
    for _ in 0..30 {
        w.tick(&[(A, PlayerInput { yaw, ..PlayerInput::default() })]);
    }
    let target = {
        let c = w.constructs.get(id).unwrap();
        let f = Vec3::new(c.forward().x, 0.0, c.forward().z).normalize();
        c.center() + f * 13.0
    };
    place(&mut w, B, Vec3::new(target.x, GROUND, target.z));
    assert!(w.toggle_transform(A));
    let mut fast = false;
    for _ in 0..48 {
        let ev = w.tick(&[(A, PlayerInput { yaw, ..PlayerInput::default() })]).to_vec();
        fast |= ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::Transform, strength, .. } if *strength > 1.5));
    }
    let c = w.constructs.get(id).unwrap();
    assert!(!c.held.vehicle && c.held.transform_t <= 0.0, "robot, ready to hit within 0.4 s");
    let b = w.player(B).unwrap().mv.pos;
    let gap = hspeed(b - c.center()) - c.half.x.max(c.half.z);
    assert!(fast, "a fast (flash) transform");
    assert!(gap <= 3.0, "popped out into melee range (gap {gap:.1} m)");
}

// ------------------------------------------------------------------------------------ 3. vehicle weapons

fn shots(ev: &[GameEvent], id: ConstructId) -> Vec<(Vec3, f32)> {
    ev.iter()
        .filter_map(|e| match e {
            GameEvent::HeldFx { id: cid, fx: HeldFx::VehicleFire, dir, strength, .. } if *cid == id => Some((*dir, *strength)),
            _ => None,
        })
        .collect()
}

#[test]
fn alt_modes_carry_weapons_from_their_data() {
    assert_eq!(armament(&dummy(tank())), Armament::Tank);
    for (name, r, lmb, rmb) in [("tank", tank(), 2.0, 1.0), ("jet", jet(), 3.0, 4.0), ("heli", heli(), 5.0, 6.0), ("car+gun", car(true), 7.0, 8.0), ("car", car(false), -1.0, 8.0)] {
        let mut w = world(128.0);
        let id = piloted(&mut w, r, Vec3::new(0.0, GROUND, 0.0), false);
        let c = w.constructs.get(id).unwrap();
        assert!(c.held.vehicle, "{name}: vehicle mode");
        let yaw = yaw_to(c.forward());
        let mut ev = Vec::new();
        for _ in 0..30 {
            ev.extend(w.tick(&[(A, PlayerInput { yaw, move_z: 1, buttons: buttons::PRIMARY, ..PlayerInput::default() })]).iter().cloned());
        }
        let s = shots(&ev, id);
        if lmb > 0.0 {
            assert!(s.iter().any(|x| x.1 == lmb), "{name}: LMB gun fires ({s:?})");
            assert!(ev.iter().any(|e| matches!(e, GameEvent::ShotFired { construct: Some(c), .. } if *c == id)), "{name}: real shots");
        } else {
            assert!(s.is_empty(), "{name}: no gun without Barrels / Turret");
        }
        w.tick(&[(A, PlayerInput { yaw, ..PlayerInput::default() })]);
        let ev = w.tick(&[(A, PlayerInput { yaw, buttons: buttons::SECONDARY, ..PlayerInput::default() })]).to_vec();
        assert!(shots(&ev, id).iter().any(|x| x.1 == rmb), "{name}: RMB weapon ({:?})", shots(&ev, id));
    }
}

fn dummy(r: Recipe) -> bd_sim::constructs::Construct {
    let mut w = world(64.0);
    let id = piloted(&mut w, r, Vec3::new(0.0, GROUND, 0.0), false);
    w.constructs.get(id).unwrap().clone()
}

#[test]
fn tank_turret_aims_at_the_crosshair_not_the_hull() {
    let mut w = world(128.0);
    let id = piloted(&mut w, tank(), Vec3::new(0.0, GROUND, 0.0), false);
    // Face the hull at +X, then look at -Z (90° off) without driving: the hull turns slowly.
    for _ in 0..240 {
        w.tick(&[(A, PlayerInput { yaw: yaw_to(Vec3::X), ..PlayerInput::default() })]);
    }
    let hull = w.constructs.get(id).unwrap().forward();
    let look = Vec3::NEG_Z;
    let ev = w.tick(&[(A, PlayerInput { yaw: yaw_to(look), buttons: buttons::SECONDARY, ..PlayerInput::default() })]).to_vec();
    let s = shots(&ev, id);
    let shell = s.iter().find(|x| x.1 == 1.0).expect("tank shell fired");
    assert!(hull.dot(look).abs() < 0.5, "hull still sideways ({hull:?})");
    assert!(shell.0.dot(look) > 0.95, "the turret fires where you look: {:?}", shell.0);
}

// ------------------------------------------------------------------------------------ 4. the chase

#[derive(Clone, Copy, Debug, PartialEq)]
enum Chaser {
    OnFoot,
    Robot,
    Vehicle,
}

/// B: a bhopping, strafing runner fleeing A (zig-zags ±40° off straight away every 0.6 s, W + A/D
/// into the turn, jump held). A starts 30 m behind. Returns seconds until the hull gap ≤ 2 m (melee),
/// capped at 15 s.
fn chase(who: Chaser, v2: bool) -> f32 {
    let mut w = world(256.0);
    if v2 {
        w.cfg.tuning.transformer.v3 = legacy();
    }
    let start = Vec3::new(-110.0, GROUND, 0.0);
    let id = match who {
        Chaser::OnFoot => None,
        Chaser::Robot => Some(piloted(&mut w, optimus(), start, true)),
        Chaser::Vehicle => Some(piloted(&mut w, optimus(), start, false)),
    };
    if id.is_none() {
        place(&mut w, A, start);
    }
    let a_pos = |w: &World| match id {
        Some(id) => w.constructs.get(id).unwrap().center(),
        None => w.player(A).unwrap().hull(w.params()).center(),
    };
    let a_half = |w: &World| match id {
        Some(id) => {
            let c = w.constructs.get(id).unwrap();
            c.half.x.min(c.half.z)
        }
        None => w.params().hull_half_width,
    };
    let p = a_pos(&w);
    let off = 30.0 + a_half(&w) + w.params().hull_half_width;
    place(&mut w, B, Vec3::new(p.x + off, GROUND, p.z));
    w.player_mut(B).unwrap().spawn_shield = 0.0;
    let mut popped = false;
    for t in 0..(15 * 120) {
        let a = a_pos(&w);
        let b = w.player(B).unwrap().hull(w.params()).center();
        let d = b - a;
        let gap = hspeed(d) - a_half(&w) - w.params().hull_half_width;
        if gap <= 2.0 {
            return t as f32 / 120.0;
        }
        // The runner.
        let away = Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::X);
        let side = if (t / 72) % 2 == 0 { 1 } else { -1 };
        let zig = side as f32 * 0.7;
        let dir = bd_core::Quat::from_rotation_y(zig) * away;
        let ib = PlayerInput { yaw: yaw_to(dir), move_z: 1, move_x: -side, buttons: buttons::JUMP, ..PlayerInput::default() };
        // The chaser: look at B, W, hop, boost-step / dodge every 0.5 s.
        let dodge = if t % 60 == 0 { buttons::DODGE } else { 0 };
        let pitch = (d.y / hspeed(d).max(0.1)).atan().clamp(-1.0, 1.0);
        let mut ia = PlayerInput { yaw: yaw_to(d), pitch, move_z: 1, buttons: buttons::JUMP | dodge, ..PlayerInput::default() };
        if who == Chaser::Vehicle {
            ia.buttons = 0;
            if !popped && gap < 12.0 {
                popped = true;
                w.toggle_transform(A);
            }
        }
        w.tick(&[(A, ia), (B, ib)]);
    }
    15.0
}

#[test]
fn chase_piloted_closes_as_fast_as_on_foot() {
    let foot = chase(Chaser::OnFoot, false);
    let robot = chase(Chaser::Robot, false);
    let vehicle = chase(Chaser::Vehicle, false);
    let robot_v2 = chase(Chaser::Robot, true);
    let vehicle_v2 = chase(Chaser::Vehicle, true);
    eprintln!("CHASE 30 m → melee vs a bhopping strafer: on foot {foot:.2} s | robot V3 {robot:.2} s (V2 {robot_v2:.2} s) | vehicle V3 {vehicle:.2} s (V2 {vehicle_v2:.2} s)");
    assert!(robot <= foot + 0.05, "piloted robot ({robot:.2} s) ≥ on foot ({foot:.2} s)");
    assert!(vehicle <= foot, "vehicle ({vehicle:.2} s) is the sprint (on foot {foot:.2} s)");
    assert!(robot < robot_v2, "V3 robot {robot:.2} s beats V2 {robot_v2:.2} s");
}
