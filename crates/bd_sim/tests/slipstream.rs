//! SLIPSTREAM (Hyperline's wind, `bd_sim::slipstream`) on a flat test arena with a hand-made
//! footprint: no effect grounded, a gentle drag airborne over the "roof", a strong rearward +
//! outward sweep outside the footprint, scaled down on an attached tether, and client prediction
//! bit-identical to the host. Maps without a slipstream are untouched (the pinned FFA hash lives in
//! `party.rs`). The Hyperline map scenarios (knock-offs, tether saves) are in `hyperline.rs`.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::{PlayerId, Vec3};
use bd_sim::world::{SimConfig, World};
use bd_voxel::{Slipstream, generate_flat_test};
use glam::Vec2;

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
const GROUND: f32 = 8.0;

fn slip() -> Slipstream {
    Slipstream {
        min: Vec2::new(-20.0, -4.5),
        max: Vec2::new(20.0, 4.5),
        roof_y: GROUND - 0.5,
        drag: 1.5,
        sweep_back: 28.0,
        sweep_out: 14.0,
        ramp: 3.0,
        terminal_back: 60.0,
        terminal_out: 30.0,
        tether_scale: 0.35,
    }
}

fn world(with: bool) -> World {
    let (voxels, mut info) = generate_flat_test(96.0);
    info.slipstream = with.then(slip);
    let cfg = SimConfig { check_invariants: true, ..SimConfig::default() };
    let mut w = World::with_arena(cfg, voxels, info, 1);
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    // B out of the way (a corner far from the footprint, standing).
    let p = w.player_mut(B).unwrap();
    p.mv.pos = Vec3::new(-40.0, GROUND, 40.0);
    p.mv.vel = Vec3::ZERO;
    w
}

fn place(w: &mut World, pos: Vec3, vel: Vec3) {
    let p = w.player_mut(A).unwrap();
    p.mv.pos = pos;
    p.mv.vel = vel;
    p.mv.grounded = false;
}

fn run(w: &mut World, n: u32, inp: PlayerInput) {
    for _ in 0..n {
        w.tick(&[(A, inp)]);
        assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
    }
}

fn a(w: &World) -> bd_sim::movement::PlayerState {
    w.player(A).unwrap().mv
}

#[test]
fn standing_and_running_on_the_roof_is_untouched() {
    let (mut on, mut off) = (world(true), world(false));
    for w in [&mut on, &mut off] {
        place(w, Vec3::new(0.0, GROUND, 0.0), Vec3::ZERO);
        w.player_mut(A).unwrap().mv.grounded = true;
    }
    // Settle, then run toward the nose (+X: yaw −90° looks +X) for 2 s.
    let run_x = PlayerInput { yaw: -std::f32::consts::FRAC_PI_2, move_z: 1, ..Default::default() };
    for w in [&mut on, &mut off] {
        run(w, 30, PlayerInput::default());
        run(w, 240, run_x);
    }
    let (p, q) = (a(&on), a(&off));
    assert!(p.grounded && q.grounded);
    assert_eq!(p.pos, q.pos, "grounded movement must be bit-identical with the wind on");
    assert_eq!(p.vel, q.vel);
}

#[test]
fn airborne_over_the_roof_feels_a_gentle_drag() {
    let (mut on, mut off) = (world(true), world(false));
    for w in [&mut on, &mut off] {
        place(w, Vec3::new(0.0, GROUND + 20.0, 0.0), Vec3::new(0.0, 5.0, 0.0));
        run(w, 60, PlayerInput::default());
    }
    let (p, q) = (a(&on), a(&off));
    assert!(!p.grounded);
    // 0.5 s × 1.5 m/s² ≈ 0.75 m/s rearward, nothing sideways or vertical.
    let dvx = p.vel.x - q.vel.x;
    assert!((-0.8..=-0.7).contains(&dvx), "drag Δvx {dvx}");
    assert_eq!(p.vel.z, q.vel.z);
    assert_eq!(p.vel.y, q.vel.y);
}

#[test]
fn outside_the_footprint_the_sweep_carries_you_back_and_out() {
    let mut w = world(true);
    // Just off the right side, high enough to stay airborne for a second.
    place(&mut w, Vec3::new(0.0, GROUND + 30.0, 5.0), Vec3::new(0.0, 6.0, 3.0));
    run(&mut w, 150, PlayerInput::default());
    let p = a(&w);
    assert!(p.vel.x < -15.0, "rearward sweep: {:?}", p.vel);
    assert!(p.vel.z > 10.0, "outward sweep: {:?}", p.vel);
    assert!(p.pos.x < -8.0 && p.pos.z > 12.0, "carried back and out: {:?}", p.pos);
    // Mirror: off the left side goes left.
    let mut w = world(true);
    place(&mut w, Vec3::new(0.0, GROUND + 30.0, -5.0), Vec3::new(0.0, 0.0, -3.0));
    run(&mut w, 120, PlayerInput::default());
    assert!(a(&w).vel.z < -10.0);
    // Behind the tail: straight back.
    let mut w = world(true);
    place(&mut w, Vec3::new(-22.0, GROUND + 30.0, 0.0), Vec3::ZERO);
    run(&mut w, 60, PlayerInput::default());
    let p = a(&w);
    assert!(p.vel.x < -6.0 && p.vel.z.abs() < 1e-3, "astern: {:?}", p.vel);
}

#[test]
fn sheltered_below_the_roofline_inside_the_footprint() {
    let mut w = world(true);
    // Inside the footprint but below `roof_y`: a player falling through a gap / inside a car.
    place(&mut w, Vec3::new(0.0, GROUND - 0.6, 0.0), Vec3::ZERO);
    let s = slip();
    assert_eq!(s.accel(Vec3::new(0.0, GROUND - 0.6, 0.0), Vec3::ZERO), Vec3::ZERO);
}

#[test]
fn an_attached_tether_scales_the_sweep_down() {
    let mut w = world(true);
    // Hang off the side: hook the 24 m pillar (north side of the flat arena) … simpler: fire at the
    // ground below-behind and compare the push with / without the hook attached.
    place(&mut w, Vec3::new(0.0, GROUND + 6.0, 12.0), Vec3::ZERO);
    let aim = PlayerInput { pitch: -1.2, buttons: buttons::TETHER_L, ..Default::default() };
    let mut attached = false;
    for _ in 0..30 {
        w.tick(&[(A, aim)]);
        if a(&w).tethered() {
            attached = true;
            break;
        }
    }
    assert!(attached, "hook should reach the ground");
    let st = a(&w);
    let s = slip();
    let free = s.accel(st.pos, st.vel);
    let ev = bd_sim::movement::MoveEvents::default();
    let got = bd_sim::slipstream::accel(&s, &st, &ev);
    assert!(free.length() > 1.0, "{free:?}");
    assert!((got - free * s.tether_scale).length() < 1e-5, "tethered push {got:?} vs free {free:?}");
}

#[test]
fn prediction_matches_the_host_in_the_wind() {
    // Host and a client replica built from the same arena; the client predicts A's inputs.
    let mut host = world(true);
    place(&mut host, Vec3::new(0.0, GROUND + 3.0, 3.5), Vec3::new(2.0, 6.0, 8.0));
    host.tick(&[]);
    let (voxels, mut info) = generate_flat_test(96.0);
    info.slipstream = Some(slip());
    let mut client = World::with_arena(SimConfig::default(), voxels, info, 1);
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    let mut worst = (0.0f32, 0.0f32);
    for t in 0..360u32 {
        let inp = PlayerInput {
            seq: t + 1,
            yaw: (t as f32 * 0.02).sin(),
            move_z: if t % 90 < 60 { 1 } else { 0 },
            move_x: if t % 50 < 20 { -1 } else { 1 },
            buttons: if t % 70 == 5 { buttons::JUMP } else { 0 } | if (100..140).contains(&t) { buttons::DODGE } else { 0 },
            ..Default::default()
        };
        let mut s = client.player(A).unwrap().mv;
        client.predict_player(A, &mut s, &inp);
        let evs = host.tick(&[(A, inp)]).to_vec();
        for e in &evs {
            client.apply_event(e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        let h = host.player(A).unwrap().mv;
        worst.0 = worst.0.max((h.pos - s.pos).length());
        worst.1 = worst.1.max((h.vel - s.vel).length());
    }
    assert!(worst.0 < 1e-4 && worst.1 < 1e-4, "prediction diverged in the slipstream: pos {} vel {}", worst.0, worst.1);
    // It really was windy: A went out past the footprint at some point and got pushed back.
    assert!(host.player(A).unwrap().mv.pos.x < 0.0);
}
