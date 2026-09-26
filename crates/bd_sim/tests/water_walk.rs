//! WATER-WALK (Valley of the End chakra walking; movement/water.rs): a map's water sheets are ground
//! for players only when the map turns `water_walk` on — walk, hop and land on the water there, sink
//! to the bed everywhere else; no other map pushes water colliders; hooks can't anchor to water;
//! host and client prediction agree tick for tick.

use bd_core::corps::Corps;
use bd_core::voxel::AIR;
use bd_core::{DT, IVec3, PlayerId, Vec3};
use bd_sim::movement::testkit::{JUMP, input};
use bd_sim::movement::{CollisionWorld, is_water, raycast_world};
use bd_sim::world::{ArenaSource, SimConfig, World};
use bd_voxel::WaterSheet;
use glam::Vec2;

const A: PlayerId = PlayerId(1);
/// The pond: x 32–44, z −18 … −6.5 (m), dug 4 m into the flat lab's 8 m ground, water at 8 m.
const POND_Y: f32 = 8.0;

fn pond_world(walk: bool) -> World {
    let (mut v, mut info) = bd_voxel::generate_flat_test(96.0);
    v.fill_box(IVec3::new(64, 8, -36), IVec3::new(87, 15, -14), AIR);
    info.water_sheets.push(WaterSheet { center: Vec3::new(38.0, POND_Y, -12.25), half: Vec2::new(6.0, 5.75), yaw: 0.0, depth: 4.0 });
    info.water_walk = walk;
    World::with_arena(SimConfig { check_invariants: true, ..SimConfig::default() }, v, info, 1)
}

fn place(w: &mut World, pos: Vec3) {
    w.add_player(A, "a".into(), Corps::from_index(1)).unwrap();
    w.start_live();
    w.tick(&[]);
    let p = w.player_mut(A).unwrap();
    p.spawn_shield = 0.0;
    p.mv.pos = pos;
    p.mv.vel = Vec3::ZERO;
}

/// Run +X from the pond's west shore across it (bunny-hopping — jump held — after `hop` ticks if
/// set): (lowest feet height over the pond, ticks grounded over the pond, final x).
fn cross(w: &mut World, hop: Option<u32>) -> (f32, u32, f32) {
    let east = -std::f32::consts::FRAC_PI_2;
    let (mut low, mut grounded) = (f32::INFINITY, 0);
    for t in 0..(3.0 / DT) as u32 {
        let mut i = input(east, 0.0, 0, 1, 0);
        if hop.is_some_and(|k| t >= k) {
            i.buttons |= JUMP;
        }
        w.tick(&[(A, i)]);
        let p = w.player(A).unwrap();
        if std::env::var("WW_TRACE").is_ok() && t % 12 == 0 {
            eprintln!("t {t} pos {:.2} vel {:.2} gr {} alive {}", p.mv.pos, p.mv.vel, p.mv.grounded, p.alive);
        }
        if (33.0..43.0).contains(&p.mv.pos.x) {
            low = low.min(p.mv.pos.y);
            grounded += p.mv.grounded as u32;
        }
    }
    (low, grounded, w.player(A).unwrap().mv.pos.x)
}

#[test]
fn runs_on_the_water_only_where_the_map_says_so() {
    let mut on = pond_world(true);
    place(&mut on, Vec3::new(30.0, POND_Y, -12.0));
    let (low, grounded, x) = cross(&mut on, None);
    eprintln!("water-walk on: lowest {low:.3} m, {grounded} grounded ticks over the pond, ended at x {x:.1}");
    assert!(low > POND_Y - 0.05, "sank into the water ({low})");
    assert!(grounded > 60, "ran on the water ({grounded} grounded ticks)");
    assert!(x > 44.0, "crossed the pond (x {x})");

    let mut off = pond_world(false);
    place(&mut off, Vec3::new(30.0, POND_Y, -12.0));
    let (low, _, _) = cross(&mut off, None);
    eprintln!("water-walk off: lowest {low:.3} m");
    assert!(low < POND_Y - 3.0, "without water-walk you sink to the bed ({low})");
}

#[test]
fn hops_and_landings_on_water_stay_on_top() {
    let mut w = pond_world(true);
    place(&mut w, Vec3::new(30.0, POND_Y, -12.0));
    let (low, grounded, x) = cross(&mut w, Some(30));
    assert!(low > POND_Y - 0.05, "a landing broke through ({low})");
    assert!(grounded >= 1 && x > 44.0, "hopped across, landing on the water ({grounded} grounded ticks, x {x})");
    // Drop onto the water from 6 m: lands on it.
    let p = w.player_mut(A).unwrap();
    p.mv.pos = Vec3::new(38.0, POND_Y + 6.0, -12.0);
    p.mv.vel = Vec3::ZERO;
    for _ in 0..120 {
        w.tick(&[(A, input(0.0, 0.0, 0, 0, 0))]);
    }
    let p = w.player(A).unwrap();
    assert!(p.mv.grounded && (p.mv.pos.y - POND_Y).abs() < 0.05, "landed on the water at {}", p.mv.pos);
}

#[test]
fn other_maps_push_no_water() {
    let mut maps = vec![
        World::new(SimConfig { arena: ArenaSource::Archipelago { seed: 3 }, ..SimConfig::default() }),
        World::new(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, ..SimConfig::default() }),
    ];
    for (v, info) in [bd_voxel::generate_miami(2), bd_voxel::generate_orbit(2)] {
        maps.push(World::with_arena(SimConfig::default(), v, info.arena.clone(), 2));
    }
    for mut w in maps {
        assert!(!w.arena.water_walk && w.arena.water_sheets.is_empty());
        w.tick(&[]);
        assert!(!w.colliders().iter().any(|c| is_water(c.id)), "water colliders on a map without water-walk");
    }
    // A sheet alone (flag off) is visual only.
    let mut w = pond_world(false);
    w.tick(&[]);
    assert!(!w.colliders().iter().any(|c| is_water(c.id)));
    let mut w = pond_world(true);
    w.tick(&[]);
    assert_eq!(w.colliders().iter().filter(|c| is_water(c.id)).count(), 1);
}

#[test]
fn hooks_do_not_anchor_to_water() {
    let mut w = pond_world(true);
    w.tick(&[]);
    let cw = CollisionWorld::new(&w.voxels, w.colliders());
    let hit = raycast_world(&cw, Vec3::new(38.0, 20.0, -12.0), Vec3::NEG_Y, 30.0, true).expect("the water stops the ray");
    assert!((hit.point.y - POND_Y).abs() < 1e-3, "hit the surface ({})", hit.point);
    assert!(!hit.anchorable, "water is not an anchor");
}

#[test]
fn prediction_matches_the_host_on_water() {
    let mut host = pond_world(true);
    place(&mut host, Vec3::new(34.0, POND_Y, -12.0));
    host.tick(&[]);
    let mut client = pond_world(true);
    client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
    let east = -std::f32::consts::FRAC_PI_2;
    let (mut bad, mut on_water) = (0, 0);
    for k in 0..300u32 {
        // Run east (hopping for a bit), then run back west, sliding at the end.
        let mut i = if k < 120 { input(east + (k as f32 * 0.05).sin() * 0.4, 0.0, 0, 1, 0) } else { input(-east, 0.0, 1, 1, 0) };
        if (40..70).contains(&k) {
            i.buttons |= JUMP;
        }
        if k > 250 {
            i.buttons |= bd_sim::movement::testkit::CROUCH;
        }
        i.seq = k + 1;
        let mut s = client.player(A).unwrap().mv;
        client.predict_player(A, &mut s, &i);
        let evs = host.tick(&[(A, i)]).to_vec();
        for e in &evs {
            client.apply_event(e);
        }
        client.apply_snapshot(&host.snapshot_for(Some(A)), Some(A));
        let h = host.player(A).unwrap();
        if h.mv.grounded && (h.mv.pos.y - POND_Y).abs() < 0.01 && (33.0..43.0).contains(&h.mv.pos.x) {
            on_water += 1;
        }
        if (h.mv.pos - s.pos).length() > 1e-4 || (h.mv.vel - s.vel).length() > 1e-3 || h.mv.grounded != s.grounded {
            bad += 1;
        }
    }
    assert!(on_water > 20, "spent {on_water} ticks on the water");
    assert_eq!(bad, 0, "mispredicted ticks");
}
