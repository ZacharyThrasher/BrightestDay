//! Stabilisation B: cross-feature correctness — host-enforced "Allow third person", power moves along
//! the converged aim, projectile muzzle parallax, never-refuse power pickups, lingering thrown items.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::events::{GameEvent, HeldFx};
use bd_sim::rules::WillpowerMode;
use bd_sim::weapons::GunArchetype;
use bd_sim::world::{Aim, ArenaSource, FIRST_PERSON_AIM_SLACK, SimConfig, World};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn world(n: u8) -> World {
    let cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, check_invariants: true, ..SimConfig::default() };
    let mut w = World::new(cfg);
    w.cfg.rules.willpower = WillpowerMode::Infinite;
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

fn place(w: &mut World, id: PlayerId, pos: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = false;
    p.mv.tumble = 0.0;
}

fn run(w: &mut World, ticks: u32, inp: &[(PlayerId, PlayerInput)]) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..ticks {
        ev.extend_from_slice(w.tick(inp));
    }
    ev
}

fn idx(w: &World, id: PlayerId) -> usize {
    w.players.iter().position(|p| p.id == id).unwrap()
}

// ------------------------------------------------------------------------------------ 1. rule

/// "Allow third person" off: the host takes nothing camera-derived — origin = the eye, direction
/// clamped to the replicated look. On: the client's converged ray is honoured as before.
#[test]
fn host_enforces_first_person_aim_when_third_person_is_disallowed() {
    let mut w = world(1);
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    run(&mut w, 10, &[(A, PlayerInput { yaw: 0.0, pitch: 0.0, ..Default::default() })]);
    let i = idx(&w, A);
    let eye = w.player(A).unwrap().eye(w.params());
    let look = w.player(A).unwrap().look();
    // A converged over-the-shoulder ray: from a point off the eye, 40° to the side.
    let side = Vec3::new(-(40f32.to_radians()).sin(), 0.0, -(40f32.to_radians()).cos());
    let aim = Aim::new(eye + Vec3::new(0.8, 0.4, 2.5), side);

    w.cfg.rules.allow_third_person = true;
    let (o, d) = w.resolve_aim(i, &aim);
    assert!(o.distance(aim.origin) < 1e-5 && d.distance(side) < 1e-5, "rule on: the converged ray stands");

    w.cfg.rules.allow_third_person = false;
    let (o, d) = w.resolve_aim(i, &aim);
    assert!(o.distance(eye) < 1e-5, "rule off: casts start at the host's eye ({o} vs {eye})");
    let ang = d.dot(look).clamp(-1.0, 1.0).acos();
    assert!((ang - FIRST_PERSON_AIM_SLACK).abs() < 1e-3, "clamped to the slack: {ang}");
    assert!((d.length() - 1.0).abs() < 1e-4);
    // A first-person aim (within the slack) is untouched.
    let small = (look + Vec3::X * 0.1).normalize();
    let (_, d) = w.resolve_aim(i, &Aim::new(eye, small));
    assert!(d.distance(small) < 1e-5);

    // End to end: a cast along the forbidden ray lands inside the slack cone, not 40° off.
    w.cfg.rules.willpower = WillpowerMode::Infinite;
    let r = recipe("wall", Form::Wall, &[], Element::Light, 1.0);
    let cid = w.cast(A, r, None, aim).unwrap();
    let c = w.constructs.get(cid).unwrap();
    let to = (c.pos - eye) * Vec3::new(1.0, 0.0, 1.0);
    let a = to.normalize().dot(Vec3::NEG_Z).acos();
    assert!(a < FIRST_PERSON_AIM_SLACK + 0.05, "placed {:.1}° off the look", a.to_degrees());
}

// ------------------------------------------------------------------------------------ 2. power moves

/// Power moves (G) use the validated client aim (3P: the converged crosshair ray), not the raw look.
#[test]
fn power_move_follows_the_aim_it_was_given() {
    let blink_to = |aim_dir: Option<Vec3>| {
        let mut w = world(1);
        place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
        run(&mut w, 20, &[(A, PlayerInput { yaw: 0.0, pitch: 0.0, ..Default::default() })]);
        assert!(w.grant_power(A, "goro_goro"));
        w.player_mut(A).unwrap().willpower = 100.0;
        let start = w.player(A).unwrap().mv.pos;
        let eye = w.player(A).unwrap().eye(w.params());
        let aim = aim_dir.map(|d| Aim::new(eye, d.normalize()));
        w.power_move_aimed(A, 0, aim).expect("lightning blink");
        run(&mut w, 2, &[(A, PlayerInput { yaw: 0.0, pitch: 0.0, ..Default::default() })]);
        w.player(A).unwrap().mv.pos - start
    };
    let plain = blink_to(None);
    let aimed = blink_to(Some(Vec3::new(1.0, 0.0, -1.0)));
    assert!(plain.z < -5.0 && plain.x.abs() < 1.0, "look: straight ahead {plain}");
    assert!(aimed.x > 3.0 && aimed.z < -3.0, "aimed: along the given ray {aimed}");
    // The rule still clamps it.
    let mut w = world(1);
    w.cfg.rules.allow_third_person = false;
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    run(&mut w, 20, &[(A, PlayerInput::default())]);
    assert!(w.grant_power(A, "goro_goro"));
    let start = w.player(A).unwrap().mv.pos;
    let eye = w.player(A).unwrap().eye(w.params());
    w.power_move_aimed(A, 0, Some(Aim::new(eye, Vec3::X))).unwrap();
    run(&mut w, 2, &[(A, PlayerInput::default())]);
    let d = w.player(A).unwrap().mv.pos - start;
    assert!(d.z < -3.0 && d.x.abs() < d.z.abs() * 0.5, "rule off: clamped near the look {d}");
}

// ------------------------------------------------------------------------------------ 3. parallax

/// Projectile muzzle parallax: a point-blank rocket leaves the (offset) muzzle toward what the eye
/// ray hits, so its line passes through the crosshair point instead of ~0.3 m beside it.
#[test]
fn projectiles_converge_from_the_muzzle_on_the_crosshair() {
    let mut w = world(2);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 2.5));
    let eye0 = w.player(A).unwrap().eye(w.params());
    let r = recipe("rocket launcher", Form::Gun, &[Mechanic::Explodes], Element::Light, 0.6);
    w.cast(A, r, None, Aim::new(eye0, Vec3::NEG_Z)).unwrap();
    run(&mut w, 30, &[]);
    place(&mut w, A, Vec3::new(0.0, GROUND, 6.0));
    place(&mut w, B, Vec3::new(0.0, GROUND, 2.5));
    run(&mut w, 2, &[]);
    let eye = w.player(A).unwrap().eye(w.params());
    let target = w.hull_of(B).unwrap().center();
    let d = (target - eye).normalize();
    let (yaw, pitch) = ((-d.x).atan2(-d.z), d.y.asin());
    let ev = run(&mut w, 1, &[(A, PlayerInput { yaw, pitch, buttons: buttons::PRIMARY, ..Default::default() })]);
    let (origin, vel) = ev
        .iter()
        .find_map(|e| match e {
            GameEvent::ProjectileFired { owner, origin, vel, .. } if *owner == A => Some((*origin, *vel)),
            _ => None,
        })
        .expect("a rocket");
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ShotFired { archetype: GunArchetype::Rocket, .. })));
    // The muzzle is off the eye ray (the hand offset) ...
    let off = (origin - eye) - d * (origin - eye).dot(d);
    assert!(off.length() > 0.15, "the muzzle is offset from the eye ray ({})", off.length());
    // ... yet the rocket's line passes through the target's centre line (≤ 8 cm), not beside it.
    let v = vel.normalize();
    let to = target - origin;
    let miss = (to - v * to.dot(v)).length();
    assert!(miss < 0.08, "rocket passes {miss:.3} m from the crosshair target (offset {:.3})", off.length());
}

// ------------------------------------------------------------------------------------ 9. pickups

/// Never refuse: a devil-fruit phrase you can't afford casts as the ordinary construct it
/// describes (sputtering), not an error; the power stays a real purchase.
#[test]
fn unaffordable_power_pickup_casts_a_construct_instead_of_refusing() {
    let mut w = world(1);
    w.cfg.rules.willpower = WillpowerMode::Normal;
    place(&mut w, A, Vec3::new(0.0, GROUND, 0.0));
    run(&mut w, 10, &[]);
    w.player_mut(A).unwrap().willpower = 1.0;
    let eye = w.player(A).unwrap().eye(w.params());
    let mut r = recipe("gomu gomu fruit", Form::Ball, &[], Element::Light, 0.35);
    r.grants = Some("gomu_gomu".into());
    let res = w.cast(A, r, None, Aim::new(eye, Vec3::NEG_Z));
    assert!(res.is_ok(), "never refused: {res:?}");
    assert!(w.player(A).unwrap().power.id.is_none(), "no free power");
    let cid = res.unwrap();
    assert!(bd_sim::freedom::is_sputtering(&w, cid), "it sputters");
    // Affordable: the fruit pops out as before (a pickup, not a plain construct).
    w.player_mut(A).unwrap().willpower = 100.0;
    let mut r = recipe("gomu gomu fruit", Form::Ball, &[], Element::Light, 0.35);
    r.grants = Some("gomu_gomu".into());
    let cid2 = w.cast(A, r, None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
    assert!(!bd_sim::freedom::is_sputtering(&w, cid2));
    assert!(w.player(A).unwrap().willpower < 100.0, "the fruit was paid for");
}

// ------------------------------------------------------------------------------------ 10. linger

/// A thrown (non-returning) axe that lands lies there ~4 s (not spent on landing), the ring is free
/// meanwhile, walking over it picks it back up; left alone it dissolves.
#[test]
fn thrown_axe_lingers_where_it_lands() {
    let setup = || {
        let mut w = world(1);
        place(&mut w, A, Vec3::new(0.0, GROUND, 10.0));
        let eye = w.player(A).unwrap().eye(w.params());
        let scale = bd_sim::constructs::form_spec(Form::Axe).default_scale;
        let cid = w.cast(A, recipe("axe", Form::Axe, &[], Element::Light, scale), None, Aim::new(eye, Vec3::NEG_Z)).unwrap();
        run(&mut w, 30, &[]);
        // Throw it down at the ground ~4 m ahead.
        let aim = PlayerInput { yaw: 0.0, pitch: -0.5, ..Default::default() };
        run(&mut w, 1, &[(A, PlayerInput { buttons: buttons::SECONDARY, ..aim })]);
        run(&mut w, 60, &[(A, aim)]);
        (w, cid)
    };
    let (mut w, cid) = setup();
    let c = w.constructs.get(cid).expect("still there after landing");
    assert!(c.gone.is_none() && c.held.thrown && c.held.landed, "lying on the ground");
    let at = c.pos;
    // The hands are free: the ring blasts while the axe lies there.
    let ev = run(&mut w, 2, &[(A, PlayerInput { yaw: 0.0, pitch: 0.2, buttons: buttons::PRIMARY, ..Default::default() })]);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ShotFired { owner, archetype: GunArchetype::Ring, .. } if *owner == A)), "ring free");
    run(&mut w, 60, &[]);
    assert!(w.constructs.get(cid).is_some_and(|c| c.gone.is_none() && c.pos.distance(at) < 0.05), "still lying at 1.5 s");
    run(&mut w, (bd_sim::constructs::held::LINGER_SECS * 120.0) as u32, &[]);
    assert!(w.constructs.get(cid).is_none_or(|c| c.gone.is_some()), "dissolves after lingering");

    // Walking over it picks it up.
    let (mut w, cid) = setup();
    let at = w.constructs.get(cid).unwrap().pos;
    place(&mut w, A, Vec3::new(at.x, GROUND, at.z + 0.3));
    let ev = run(&mut w, 30, &[]);
    assert!(ev.iter().any(|e| matches!(e, GameEvent::HeldFx { fx: HeldFx::Catch, .. })), "caught");
    let c = w.constructs.get(cid).unwrap();
    assert!(!c.held.thrown && c.attached(), "back in the hand");
}
