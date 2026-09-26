//! Ember in the sim (feature `ember`): the lava is the bottom blast zone (touching it is a KO through
//! the normal rules — no map-specific sim code), both surf lines are real surf faces that build speed
//! over the lava and land on the far island, their faces are clear of terrain past the run-off and
//! of every spawn, and eight bots brawl on the map with invariants on.
#![cfg(feature = "ember")]

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::voxel::VoxelQuery;
use bd_core::{DT, PlayerId, Vec3};
use bd_sim::bots;
use bd_sim::events::GameEvent;
use bd_sim::movement::testkit::input;
use bd_sim::movement::{Collider, surf_line, surfing};
use bd_sim::world::{SimConfig, World};
use bd_voxel::maps::ember::{self, generate_ember};

fn ember_world(seed: u64) -> World {
    let (v, info) = generate_ember(seed);
    World::with_arena(SimConfig { check_invariants: true, ..SimConfig::default() }, v, info.arena.clone(), seed)
}

fn surf_input(face: &Collider, d: Vec3) -> PlayerInput {
    let n = face.axes()[1];
    let yaw = (-d.x).atan2(-d.z);
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let into = Vec3::new(-n.x, 0.0, -n.z);
    input(yaw, 0.0, if into.dot(right) > 0.0 { 1 } else { -1 }, 0, 0)
}

fn on_face(face: &Collider, point: Vec3, gap: f32) -> Vec3 {
    let n = face.axes()[1];
    let corner = Vec3::new(-0.4 * n.x.signum(), 0.0, -0.4 * n.z.signum());
    point - corner + n * gap
}

#[test]
fn falling_into_the_lava_is_a_bottom_ko() {
    let mut w = ember_world(1);
    let a = PlayerId(1);
    w.add_player(a, "diver".into(), Corps::from_index(1)).unwrap();
    w.start_live();
    w.tick(&[]);
    // Over the open river west of the facility, 6 m up.
    let x = -30.0;
    let drop = Vec3::new(x, 6.0, ember::river_z(x));
    {
        let p = w.player_mut(a).unwrap();
        p.spawn_shield = 0.0;
        p.mv.pos = drop;
        p.mv.vel = Vec3::ZERO;
        p.mv.grounded = false;
    }
    let mut ko = None;
    let mut low = f32::INFINITY;
    for t in 0..240 {
        let ev = w.tick(&[]).to_vec();
        if let Some(p) = w.player(a)
            && p.alive
        {
            low = low.min(p.mv.pos.y);
        }
        if let Some(GameEvent::Ko { victim, void, pos, .. }) = ev.iter().find(|e| matches!(e, GameEvent::Ko { .. })) {
            ko = Some((t, *victim, *void, *pos));
            break;
        }
    }
    let (t, victim, void, pos) = ko.expect("no KO after dropping into the lava");
    eprintln!("lava KO after {:.2} s at {pos} (lowest alive feet {low:.2})", t as f32 * DT);
    assert_eq!(victim, a);
    assert!(void, "a lava KO is a bottom (void) KO");
    assert!(low >= ember::LAVA_TOP - 0.01, "the player sank into the lava sheet ({low})");
}

#[test]
fn surf_faces_clear_of_terrain_and_spawns() {
    let mut w = ember_world(1);
    w.tick(&[]);
    assert_eq!(w.arena.surf_lines.len(), 2);
    for (k, l) in w.arena.surf_lines.iter().enumerate() {
        let len = (l.b - l.a).length();
        let d = (l.b - l.a) / len;
        for c in surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0) {
            // Sample the face slab every 0.5 m; the first 22 % is the buried run-off start.
            let steps = |h: f32| ((2.0 * h) / 0.5).ceil() as i32;
            let (nx, ny, nz) = (steps(c.half.x), steps(c.half.y), steps(c.half.z));
            let mut bad = Vec::new();
            for kz in 0..=nz {
                for i in 0..=nx {
                    for j in 0..=ny {
                        let lp = Vec3::new(
                            -c.half.x + 2.0 * c.half.x * i as f32 / nx as f32,
                            -c.half.y + 2.0 * c.half.y * j as f32 / ny as f32,
                            -c.half.z + 2.0 * c.half.z * kz as f32 / nz as f32,
                        );
                        let p = c.to_world(lp);
                        if (p - l.a).dot(d) > len * 0.22 && w.voxels.is_solid(bd_core::voxel::world_to_voxel(p)) {
                            bad.push(p);
                        }
                    }
                }
            }
            assert!(bad.is_empty(), "line {k}: {} buried samples, e.g. {:?}", bad.len(), &bad[..bad.len().min(3)]);
            for s in &w.arena.spawns {
                let lp = c.to_local(*s + Vec3::Y * 0.9);
                let out = (lp.abs() - c.half).max(Vec3::ZERO).length();
                assert!(out > 2.0, "spawn {s} is {out:.2} m from surf line {k}");
            }
        }
    }
}

#[test]
fn scripted_surf_down_both_lines() {
    for (k, name, land_top) in [(0usize, "Ridge → Dunes", ember::ED_TOP), (1, "Crag → South Plain", ember::SP_TOP)] {
        let mut w = ember_world(1);
        w.add_player(PlayerId(1), "surfer".into(), Corps::from_index(1)).unwrap();
        w.start_live();
        w.tick(&[]);
        let l = w.arena.surf_lines[k];
        let faces = surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0);
        let d = (l.b - l.a).normalize();
        let f = faces[0].axes()[0];
        let start = on_face(&faces[0], l.a + (l.b - l.a) * 0.25 + f * 2.5, 0.05);
        let p = w.params().clone();
        {
            let pl = w.player_mut(PlayerId(1)).unwrap();
            pl.spawn_shield = 0.0;
            pl.mv.pos = start;
            pl.mv.vel = d * 10.0;
            pl.mv.grounded = false;
            pl.mv.air_time = 0.5;
        }
        let i = surf_input(&faces[0], d);
        let (mut on, mut exit, mut along, mut landed) = (0, 0.0f32, 0.0f32, None);
        for t in 0..(6.0 / DT) as usize {
            w.tick(&[(PlayerId(1), i)]);
            let pl = w.player(PlayerId(1)).unwrap();
            assert!(pl.alive, "{name}: died at tick {t} ({})", pl.mv.pos);
            if surfing(&pl.mv, &p) {
                on += 1;
                exit = pl.mv.speed();
                along = (pl.mv.pos - start).dot(d);
            } else if on > 0 && pl.mv.grounded && landed.is_none() {
                landed = Some((t, pl.mv.pos));
                break;
            }
        }
        let len = (l.b - l.a).length();
        eprintln!(
            "{name}: line {len:.0} m, drop {:.0} m; surfed {along:.0} m ({on} ticks), exit {exit:.1} m/s, landed {landed:?}",
            l.a.y - l.b.y
        );
        assert!(along > len * 0.75 - 6.0, "{name}: rode the line ({along:.1} of {len:.1} m)");
        assert!(exit > 20.0, "{name}: surf built speed ({exit:.1})");
        let (_, at) = landed.unwrap_or_else(|| panic!("{name}: never landed"));
        assert!(at.y > land_top - 1.5, "{name}: landed at {at}, below the island top {land_top}");
    }
}

#[test]
fn eight_bots_brawl_on_ember() {
    let mut w = ember_world(2);
    let mut brains = bots::add_bots(&mut w, 1, 8, 0.8, 11);
    w.start_live();
    let mut inputs = Vec::with_capacity(8);
    let (mut kos, mut lava) = (0, 0);
    for _ in 0..(40.0 / DT) as usize {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        for e in w.tick(&inputs) {
            if let GameEvent::Ko { void, .. } = e {
                kos += 1;
                lava += *void as u32;
            }
        }
    }
    eprintln!("40 s, 8 bots on Ember: {kos} KOs ({lava} into the lava)");
    let alive = w.players.iter().filter(|p| p.alive).count();
    assert!(alive >= 1, "everyone dead at the end");
}
