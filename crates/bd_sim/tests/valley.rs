//! Valley of the End in the sim (feature `valley`): the surf lines are clear of terrain past their
//! run-off and away from the spawns, the river and the upper pool carry players (water-walk) while
//! the plunge hole is a void KO, and a bot match runs clean on the map.
#![cfg(feature = "valley")]

use bd_core::corps::Corps;
use bd_core::voxel::VoxelQuery;
use bd_core::{DT, PlayerId, Vec3};
use bd_sim::movement::testkit::input;
use bd_sim::movement::{ColliderId, SURF_ID_BASE, is_water, surf_line};
use bd_sim::world::{SimConfig, World};
use bd_voxel::maps::valley::{self, generate_valley};

fn valley_world(seed: u64) -> World {
    let (v, info) = generate_valley(seed);
    World::with_arena(SimConfig { check_invariants: true, ..SimConfig::default() }, v, info.arena.clone(), seed)
}

#[test]
fn surf_lines_clear_of_terrain_and_spawns() {
    let mut w = valley_world(1);
    w.tick(&[]);
    assert_eq!(w.arena.surf_lines.len(), 2);
    let surf: Vec<_> = w.colliders().iter().filter(|c| matches!(c.id, ColliderId::Other(i) if (SURF_ID_BASE..SURF_ID_BASE + 0x100).contains(&i))).copied().collect();
    assert_eq!(surf.len(), 4);
    for s in &w.arena.spawns {
        for c in &surf {
            let l = c.to_local(*s + Vec3::Y * 0.9);
            let out = (l.abs() - c.half).max(Vec3::ZERO).length();
            assert!(out > 2.0, "spawn {s} is {out:.2} m from a surf face");
        }
    }
    for (k, l) in w.arena.surf_lines.iter().enumerate() {
        let len = (l.b - l.a).length();
        let d = (l.b - l.a) / len;
        for c in surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0) {
            let steps = |h: f32| ((2.0 * h) / 0.5).ceil() as i32;
            let (nx, ny, nz) = (steps(c.half.x), steps(c.half.y), steps(c.half.z));
            for i in 0..=nx {
                for j in 0..=ny {
                    for kk in 0..=nz {
                        let lp = Vec3::new(
                            -c.half.x + 2.0 * c.half.x * i as f32 / nx as f32,
                            -c.half.y + 2.0 * c.half.y * j as f32 / ny as f32,
                            -c.half.z + 2.0 * c.half.z * kk as f32 / nz as f32,
                        );
                        let p = c.to_world(lp);
                        if (p - l.a).dot(d) > len * 0.1 {
                            assert!(!w.voxels.is_solid(bd_core::voxel::world_to_voxel(p)), "surf line {k} buried at {p}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn river_and_pool_carry_players_the_plunge_does_not() {
    let a = PlayerId(1);
    let mut w = valley_world(1);
    w.add_player(a, "a".into(), Corps::from_index(1)).unwrap();
    w.start_live();
    w.tick(&[]);
    assert!(w.colliders().iter().any(|c| is_water(c.id)), "the Valley pushes its water");
    // Stand still on the river, then on the pool: stays on top.
    for spot in [Vec3::new(valley::centre_x(-20.0) - 4.5, valley::RIVER_Y + 0.3, -20.0), Vec3::new(0.0, valley::POOL_Y + 0.3, 80.0)] {
        let p = w.player_mut(a).unwrap();
        p.spawn_shield = 0.0;
        p.mv.pos = spot;
        p.mv.vel = Vec3::ZERO;
        for _ in 0..(1.0 / DT) as usize {
            w.tick(&[(a, input(0.0, 0.0, 0, 0, 0))]);
        }
        let p = w.player(a).unwrap();
        assert!(p.alive && p.mv.grounded, "stands on the water at {spot}");
        assert!((p.mv.pos.y - (spot.y - 0.3)).abs() < 0.05, "on the surface: {} vs {}", p.mv.pos.y, spot.y - 0.3);
    }
    // Step off the lip of the falls into the plunge hole: gone (void KO).
    let p = w.player_mut(a).unwrap();
    p.mv.pos = Vec3::new(0.0, valley::POOL_Y + 0.1, valley::FALLS_Z + 0.3);
    p.mv.vel = Vec3::new(0.0, 0.0, -6.0);
    let deaths_before = w.player(a).unwrap().deaths;
    let mut ko = false;
    for _ in 0..(6.0 / DT) as usize {
        w.tick(&[(a, input(std::f32::consts::PI, 0.0, 0, -1, 0))]);
        let p = w.player(a).unwrap();
        if !p.alive || p.deaths > deaths_before {
            ko = true;
            break;
        }
    }
    assert!(ko, "falling down the falls into the plunge hole is a KO");
}

#[test]
fn bots_play_the_valley() {
    let mut w = valley_world(2);
    let mut brains = bd_sim::bots::add_bots(&mut w, 0, 7, 0.9, 5);
    w.start_live();
    let mut inputs = Vec::new();
    for _ in 0..(30.0 / DT) as usize {
        inputs.clear();
        bd_sim::bots::drive(&mut w, &mut brains, &mut inputs);
        w.tick(&inputs);
    }
    let deaths: u32 = w.players.iter().map(|p| p.deaths as u32).sum();
    eprintln!("after 30 s: {deaths} KOs among {} bots", w.players.len());
    assert_eq!(w.players.len(), 7);
}
