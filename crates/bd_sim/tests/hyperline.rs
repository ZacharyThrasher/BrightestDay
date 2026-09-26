//! Hyperline on the real map (feature `hyperline`): the slipstream turns knock-offs into KOs, the
//! tether save still holds (reel back aboard against the wind), a light hit is survivable with the
//! ordinary tools, a hard hit or a trip off the tail is a KO, and prediction matches the host
//! through a knock-off + tether. The rule itself on a flat arena: `slipstream.rs`.
#![cfg(feature = "hyperline")]

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::{PlayerId, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::movement::PlayerState;
use bd_sim::smash::BlastZone;
use bd_sim::world::{SimConfig, World};
use bd_voxel::maps::hyperline::{self, FLAT_HW, FOOTBOARD_Y, HALF_W, ROOF_Y, generate_hyperline};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);

fn world() -> World {
    let (voxels, info) = generate_hyperline(1);
    let spawns = info.arena.spawns.clone();
    let cfg = SimConfig { check_invariants: true, ..SimConfig::default() };
    let mut w = World::with_arena(cfg, voxels, info.arena, 1);
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    let p = w.player_mut(B).unwrap();
    p.mv.pos = spawns[0];
    p.mv.vel = Vec3::ZERO;
    w
}

fn stand(w: &mut World, pos: Vec3) {
    let p = w.player_mut(A).unwrap();
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
    p.mv.grounded = true;
    w.tick(&[]);
}

fn yaw_pitch(d: Vec3) -> (f32, f32) {
    let d = d.normalize_or(Vec3::NEG_Z);
    ((-d.x).atan2(-d.z), d.y.clamp(-1.0, 1.0).asin())
}

/// Runs up to `max` ticks; `Some(zone)` if A was KO'd, `None` when `done` held first.
fn drive(w: &mut World, max: u32, mut policy: impl FnMut(u32, &PlayerState) -> PlayerInput, done: impl Fn(&PlayerState) -> bool) -> Result<Option<BlastZone>, PlayerState> {
    for t in 0..max {
        let st = w.player(A).unwrap().mv;
        let inp = policy(t, &st);
        let evs = w.tick(&[(A, inp)]).to_vec();
        assert!(w.last_invariant_error.is_none(), "{:?}", w.last_invariant_error);
        for e in &evs {
            if let GameEvent::Ko { victim, zone, .. } = e
                && *victim == A
            {
                return Ok(*zone);
            }
        }
        if done(&w.player(A).unwrap().mv) {
            return Ok(None);
        }
    }
    Err(w.player(A).unwrap().mv)
}

fn on_roof(st: &PlayerState) -> bool {
    st.grounded && st.pos.y >= ROOF_Y - 0.1 && st.pos.z.abs() < HALF_W
}

fn safe(st: &PlayerState) -> bool {
    on_roof(st) || (st.grounded && st.pos.y >= FOOTBOARD_Y - 0.1 && st.pos.y < ROOF_Y - 1.0)
}

fn hook_reel(t: u32, at: u32, st: &PlayerState, target: Vec3) -> PlayerInput {
    let (yaw, pitch) = yaw_pitch(target - (st.pos + Vec3::Y * 1.6));
    let b = if t >= at { buttons::TETHER_L | buttons::REEL } else { 0 };
    PlayerInput { yaw, pitch, buttons: b, move_z: 1, ..Default::default() }
}

#[test]
fn the_map_carries_its_slipstream() {
    let (_, info) = generate_hyperline(1);
    let s = info.arena.slipstream.expect("slipstream");
    assert!(s.sweep_back > 10.0 * s.drag, "the sweep must dwarf the roof drag");
    assert!(s.roof_y > ROOF_Y + 1.8, "ordinary hops stay in the calm layer");
}

#[test]
fn a_hard_hit_off_the_side_is_a_ko_but_the_tether_saves() {
    let u = &hyperline::units(1)[3];
    let launch = Vec3::new(0.0, 7.0, 16.0);
    // Without help: swept out and down → KO (bottom or side).
    let mut w = world();
    stand(&mut w, Vec3::new(u.cx(), ROOF_Y, 3.0));
    w.launch_player(A, launch, Some(B));
    let r = drive(&mut w, 120 * 8, |_, _| PlayerInput::default(), on_roof);
    assert!(matches!(r, Ok(Some(BlastZone::Bottom | BlastZone::Side))), "unassisted: {r:?}");
    // Hook the roof after 0.25 s and reel: back aboard, never KO'd.
    let mut w = world();
    stand(&mut w, Vec3::new(u.cx(), ROOF_Y, 3.0));
    w.launch_player(A, launch, Some(B));
    let anchor = Vec3::new(u.cx() - 2.0, ROOF_Y - 0.25, 0.25);
    let r = drive(&mut w, 120 * 8, |t, st| hook_reel(t, 30, st, anchor), on_roof);
    assert_eq!(r, Ok(None), "tether back aboard");
}

#[test]
fn the_tether_save_holds_while_hanging_in_the_sweep() {
    // Hook the roof edge, then just hang there (no reel) for 4 s: the rope holds you in the wind.
    let u = &hyperline::units(1)[4];
    let mut w = world();
    stand(&mut w, Vec3::new(u.cx(), ROOF_Y, 2.0));
    w.launch_player(A, Vec3::new(0.0, 6.0, 14.0), Some(B));
    let anchor = Vec3::new(u.cx(), ROOF_Y - 0.25, FLAT_HW - 0.25);
    let r = drive(
        &mut w,
        120 * 4,
        |t, st| {
            let (yaw, pitch) = yaw_pitch(anchor - (st.pos + Vec3::Y * 1.6));
            PlayerInput { yaw, pitch, buttons: if t >= 20 { buttons::TETHER_L } else { 0 }, ..Default::default() }
        },
        |_| false,
    );
    let st = w.player(A).unwrap().mv;
    assert!(r.is_err(), "no KO while hanging: {r:?}");
    assert!(st.tethered(), "still on the rope: {st:?}");
    assert!(st.pos.z > HALF_W && st.pos.y < ROOF_Y, "hanging off the side: {:?}", st.pos);
}

#[test]
fn a_light_hit_near_the_edge_is_survivable() {
    let us = hyperline::units(1);
    let u = &us[5];
    let mut w = world();
    stand(&mut w, Vec3::new(u.x0 + u.doors[1], ROOF_Y, 2.8));
    w.launch_player(A, Vec3::new(0.0, 4.0, 14.0), Some(B));
    let mut jumped: Option<u32> = None;
    let r = drive(
        &mut w,
        120 * 6,
        |t, st| {
            let out = st.pos.z > FLAT_HW - 0.5;
            let mut b = 0;
            if out && st.tumble <= 0.0 && jumped.is_none() {
                jumped = Some(t);
                b = buttons::JUMP;
            } else if jumped.is_some_and(|j| t == j + 8) && out {
                b = buttons::DODGE;
            }
            PlayerInput { yaw: 0.0, move_z: if out { 1 } else { 0 }, buttons: b, ..Default::default() }
        },
        safe,
    );
    assert_eq!(r, Ok(None), "light hit: steer back + air jump + dodge lands on the train");
}

#[test]
fn knocked_off_the_tail_is_swept_astern() {
    let tail = &hyperline::units(1)[hyperline::UNITS - 1];
    let mut w = world();
    stand(&mut w, Vec3::new(tail.x0 + 8.0, ROOF_Y, 2.0));
    w.launch_player(A, Vec3::new(-30.0, 10.0, 0.0), Some(B));
    let r = drive(&mut w, 120 * 8, |_, _| PlayerInput { yaw: -std::f32::consts::FRAC_PI_2, move_z: 1, ..Default::default() }, on_roof);
    assert!(matches!(r, Ok(Some(_))), "astern: {r:?}");
}

#[test]
fn prediction_matches_the_host_through_a_knock_off_and_tether() {
    let u = &hyperline::units(1)[2];
    let mut host = world();
    stand(&mut host, Vec3::new(u.cx(), ROOF_Y, 3.0));
    host.launch_player(A, Vec3::new(0.0, 7.0, 15.0), Some(B));
    host.tick(&[]);
    let (voxels, info) = generate_hyperline(1);
    let mut client = World::with_arena(SimConfig::default(), voxels, info.arena, 1);
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    let anchor = Vec3::new(u.cx() - 2.0, ROOF_Y - 0.25, 0.25);
    let mut worst = (0.0f32, 0.0f32);
    for t in 0..480u32 {
        let st = client.player(A).unwrap().mv;
        let mut inp = hook_reel(t, 40, &st, anchor);
        inp.seq = t + 1;
        let mut s = st;
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
    assert!(worst.0 < 1e-4 && worst.1 < 1e-4, "prediction diverged: pos {} vel {}", worst.0, worst.1);
}
