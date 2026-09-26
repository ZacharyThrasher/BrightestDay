//! Orbit's map gravity hook (DESIGN.md "Maps: Orbit"): normal gravity on the islands, fading to
//! ~zero-g above the station; applied identically by the host step and client prediction; blast
//! zones still KO (top = launched into space, bottom = re-entry). Maps without a profile are
//! untouched (see `party.rs` for the pinned FFA hash).

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::{PlayerId, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::smash::BlastZone;
use bd_sim::world::{SimConfig, World};
use bd_voxel::generate_orbit;
use bd_voxel::maps::orbit::{GRAVITY, VOID_Y};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
/// An open column of sky south of the station (nothing solid anywhere above the void here).
const X: f32 = 0.0;
const Z: f32 = -88.0;

fn orbit_world() -> World {
    let (voxels, info) = generate_orbit(1);
    let cfg = SimConfig { check_invariants: true, ..SimConfig::default() };
    let mut w = World::with_arena(cfg, voxels, info.arena.clone(), 1);
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

fn place(w: &mut World, id: PlayerId, pos: Vec3, vel: Vec3) {
    let p = w.player_mut(id).unwrap();
    p.mv.pos = pos;
    p.mv.vel = vel;
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

#[test]
fn profile_is_full_on_the_islands_and_fades_smoothly() {
    assert_eq!(GRAVITY.scale_at(40.0), 1.0);
    assert_eq!(GRAVITY.scale_at(GRAVITY.full_below), 1.0);
    assert!((GRAVITY.scale_at(GRAVITY.zero_above) - GRAVITY.min_scale).abs() < 1e-6);
    assert_eq!(GRAVITY.scale_at(500.0), GRAVITY.min_scale);
    assert_eq!(GRAVITY.scale_at(f32::NAN), 1.0);
    let mut prev = 1.0;
    for i in 0..=200 {
        let y = 70.0 + i as f32 * 0.2;
        let s = GRAVITY.scale_at(y);
        assert!(s <= prev + 1e-6 && s >= GRAVITY.min_scale - 1e-6, "monotonic at {y}: {s}");
        prev = s;
    }
    let (_, info) = generate_orbit(1);
    for s in &info.spawns {
        assert_eq!(info.gravity_scale_at(s.y + 3.0), 1.0, "spawn {s} must be in full gravity (not floaty)");
    }
}

#[test]
fn zero_g_band_drifts_while_the_islands_keep_full_gravity() {
    let mut w = orbit_world();
    place(&mut w, A, Vec3::new(X, 100.0, Z), Vec3::ZERO);
    place(&mut w, B, Vec3::new(X + 6.0, 60.0, Z), Vec3::ZERO);
    run(&mut w, 60);
    let (a, b) = (w.player(A).unwrap().mv, w.player(B).unwrap().mv);
    assert!(a.vel.y > -2.5 && a.pos.y > 99.3, "zero-g band: barely sinking ({:?})", a.vel);
    // Full gravity (25 m/s²) for 0.5 s ⇒ ≈ −12.5 m/s.
    assert!(b.vel.y < -11.5, "islands keep heavy gravity ({:?})", b.vel);
    assert!((b.mods.gravity_scale - 1.0).abs() < 1e-6);
}

#[test]
fn prediction_uses_the_same_gravity_as_the_host() {
    let mut w = orbit_world();
    for y in [60.0f32, 80.0, 90.0, 110.0] {
        place(&mut w, A, Vec3::new(X, y, Z), Vec3::new(3.0, 1.0, 0.0));
        let input = PlayerInput::default();
        let mut predicted = w.player(A).unwrap().mv;
        let mods = w.move_mods_for(A, &input, &predicted);
        assert_eq!(mods.gravity_scale, GRAVITY.scale_at(y), "at {y}");
        for _ in 0..30 {
            w.predict_player(A, &mut predicted, &input);
            w.tick(&[(A, input)]);
        }
        let host = w.player(A).unwrap().mv;
        assert!((host.pos - predicted.pos).length() < 1e-4, "at {y}: host {:?} vs predicted {:?}", host.pos, predicted.pos);
        assert!((host.vel - predicted.vel).length() < 1e-4);
    }
}

#[test]
fn launched_into_space_is_a_top_ko() {
    let mut w = orbit_world();
    place(&mut w, B, Vec3::new(X, 62.0, Z), Vec3::ZERO);
    w.launch_player(B, Vec3::new(0.0, 52.0, 0.0), Some(A));
    let ev = run(&mut w, 1200);
    let ko = ev.iter().find_map(|e| match e {
        GameEvent::Ko { victim, zone, killer, poof_delay, .. } if *victim == B => Some((*zone, *killer, *poof_delay)),
        _ => None,
    });
    let (zone, killer, delay) = ko.expect("launched out through the top");
    assert_eq!(zone, Some(BlastZone::Top));
    assert_eq!(killer, Some(A));
    assert!(delay > 0.0);
}

#[test]
fn a_modest_pop_up_comes_back_down() {
    // A normal jump / small knock from the station roof never drifts off into space.
    let mut w = orbit_world();
    place(&mut w, B, Vec3::new(X, 62.0, Z), Vec3::new(0.0, 16.0, 0.0));
    let ev = run(&mut w, 240);
    assert!(!ev.iter().any(|e| matches!(e, GameEvent::Ko { victim, .. } if *victim == B)));
    let b = w.player(B).unwrap().mv;
    assert!(b.vel.y < -10.0 && b.pos.y < 62.0, "fell back ({:?} {:?})", b.pos, b.vel);
}

#[test]
fn falling_below_the_islands_is_a_re_entry_ko() {
    let mut w = orbit_world();
    place(&mut w, B, Vec3::new(X, 20.0, Z), Vec3::ZERO);
    let ev = run(&mut w, 240);
    let ko = ev.iter().find_map(|e| match e {
        GameEvent::Ko { victim, zone, pos, .. } if *victim == B => Some((*zone, *pos)),
        _ => None,
    });
    let (zone, pos) = ko.expect("burned up on re-entry");
    assert_eq!(zone, Some(BlastZone::Bottom));
    assert!(pos.y < VOID_Y + 1.0);
}

#[test]
fn a_boulder_cast_into_the_band_drifts_but_one_on_the_islands_falls() {
    use bd_core::mechanics::{Element, Form};
    use bd_sim::world::Aim;
    let mut w = orbit_world();
    w.cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
    let fall = |w: &mut World, y: f32| {
        place(w, A, Vec3::new(X, y, Z), Vec3::ZERO);
        let eye = w.player(A).unwrap().eye(&w.cfg.move_params);
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let id = w.cast(A, bd_sim::bots::recipe("boulder", Form::Boulder, &[], Element::Light, 1.0), None, Aim::new(eye, dir)).unwrap();
        // Let it form, then measure 0.5 s of free flight (the player is held in place).
        for _ in 0..240 {
            place(w, A, Vec3::new(X, y, Z), Vec3::ZERO);
            w.tick(&[]);
        }
        let y0 = w.constructs.get(id).map(|c| c.pos.y);
        for _ in 0..60 {
            place(w, A, Vec3::new(X, y, Z), Vec3::ZERO);
            w.tick(&[]);
        }
        let y1 = w.constructs.get(id).map(|c| c.pos.y);
        (y0, y1)
    };
    let (a0, a1) = fall(&mut w, 100.0);
    let (b0, b1) = fall(&mut w, 64.0);
    if let (Some(a0), Some(a1)) = (a0, a1) {
        assert!(a0 - a1 < 1.5, "zero-g band: the boulder drifts ({a0} -> {a1})");
    }
    if let (Some(b0), Some(b1)) = (b0, b1) {
        assert!(b0 - b1 > 1.5 || b1 < 10.0, "full gravity: it falls ({b0} -> {b1})");
    }
    assert!(a0.is_some(), "the zero-g boulder is still around");
}