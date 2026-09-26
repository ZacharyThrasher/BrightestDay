//! Orbit map sanity: spawns (standable, spread, in full gravity), the spine bhop lane is continuous
//! with head room (through the hub doors), indestructible cores / mast / spine, anchoring,
//! determinism, size limits and generation timing.
//!
//! Timing: `cargo test -p bd_voxel --release --test orbit -- --nocapture`.

use std::time::Instant;

use bd_core::voxel::{AIR, VoxelQuery, world_to_voxel};
use bd_voxel::maps::orbit::*;
use bd_voxel::*;
use glam::{IVec3, Vec3};

#[test]
fn orbit_spawns_are_standable_spread_and_heavy() {
    let (w, info) = generate_orbit(1);
    assert!(info.spawns.len() >= 8, "spawns: {}", info.spawns.len());
    assert_eq!(info.gravity, Some(GRAVITY));
    assert_eq!(info.void_y, VOID_Y);
    for (i, a) in info.spawns.iter().enumerate() {
        let feet = world_to_voxel(*a);
        assert!(w.is_solid(feet - IVec3::Y), "spawn {i} floating at {a}");
        for dy in 0..4 {
            assert!(!w.is_solid(feet + IVec3::Y * dy), "spawn {i} buried at {a}");
        }
        assert_eq!(info.gravity_scale_at(a.y + 4.0), 1.0, "spawn {i} at {a} must have full gravity");
        for b in &info.spawns[i + 1..] {
            assert!(a.distance(*b) >= 12.0, "spawns too close: {a} {b}");
        }
    }
    for (sx, sz) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        assert!(info.spawns.iter().any(|s| s.x * sx > 0.0 && s.z * sz > 0.0), "no spawn in quadrant ({sx}, {sz})");
    }
    // Every walkable surface of consequence is below the zero-g band.
    for d in &info.districts {
        assert_eq!(info.gravity_scale_at(d.center.y + 2.0), 1.0, "district {} is floaty", d.name);
    }
}

#[test]
fn the_spine_lane_is_continuous_through_the_hub() {
    let (w, info) = generate_orbit(1);
    let r = info.route("spine_bhop").expect("spine route");
    let (x0, x1) = (r.waypoints.last().unwrap().x, r.waypoints[0].x);
    let mut x = x0;
    let mut worst_step = 0.0f32;
    let mut prev: Option<f32> = None;
    while x <= x1 {
        // Lane centre ± 1 m: a floor between 30 and 46 m with ≥ 3 m of head room.
        for z in [-1.0f32, 0.0, 1.0] {
            let col = |y: i32| w.get(IVec3::new((x / 0.5).floor() as i32, y, (z / 0.5).floor() as i32));
            let top = (60..=92).rev().find(|&y| col(y) != AIR && (1..=6).all(|d| col(y + d) == AIR));
            let Some(t) = top else { panic!("no floor on the spine lane at x = {x}, z = {z}") };
            let feet = (t + 1) as f32 * 0.5;
            assert!((30.0..=46.0).contains(&feet), "lane floor at x = {x}: {feet}");
            if z == 0.0 {
                if let Some(p) = prev {
                    worst_step = worst_step.max(feet - p);
                }
                prev = Some(feet);
            }
        }
        x += 0.5;
    }
    // Only 0.5 m step-ups along the lane (no walls to trip a bhop).
    assert!(worst_step <= 0.51, "a step of {worst_step} m on the spine lane");
}

#[test]
fn orbit_is_anchored_bounded_and_deterministic() {
    let (a, info) = generate_orbit(3);
    let (b, _) = generate_orbit(3);
    assert_eq!(a.content_hash(), b.content_hash(), "generation must be deterministic");
    let (c, _) = generate_orbit(4);
    assert_ne!(a.content_hash(), c.content_hash(), "seeds should vary details");
    assert!(info.pruned_voxels < 64, "kit pieces came out detached: {} pruned", info.pruned_voxels);
    let bw = a.bounds();
    assert!(bw.max.y <= 112.0 + 1e-3);
    let size = bw.max - bw.min;
    assert!((180.0..=200.0).contains(&size.x) && (180.0..=200.0).contains(&size.z));
    let top = a.voxel_max().y - 1;
    for z in a.voxel_min().z..a.voxel_max().z {
        for x in a.voxel_min().x..a.voxel_max().x {
            assert_eq!(a.get(IVec3::new(x, top, z)), AIR);
        }
    }
    // The antenna mast (the zero-g lifeline) and the hub's core columns survive a huge blast.
    let mut w = a;
    let mast = Vec3::new(0.0, 80.0, 0.0);
    w.apply_carve(&CarveOp { shape: CarveShape::Sphere { center: mast, radius: 4.0 }, power: 5.0, seed: 1 });
    assert_eq!(w.get(world_to_voxel(mast)), mat::TOWER_CORE, "the mast survives");
    let hub = Vec3::new(3.6, HUB_TOP + 8.0, 3.6);
    let out = w.apply_carve(&CarveOp { shape: CarveShape::Sphere { center: hub, radius: 6.0 }, power: 3.0, seed: 2 });
    assert!(out.removed.len() > 300, "the hub hull breaks");
    assert!(world_to_voxel(hub).y > 0 && w.is_solid(world_to_voxel(Vec3::new(3.54, HUB_TOP + 8.0, 3.54))), "a core column survives");
    // Rock cores are indestructible too.
    let core = Vec3::new(0.0, HUB_TOP - 9.0, 0.0);
    w.apply_carve(&CarveOp { shape: CarveShape::Sphere { center: core, radius: 5.0 }, power: 5.0, seed: 3 });
    assert!((0..6).any(|d| w.get(world_to_voxel(core) + IVec3::Y * d - IVec3::Y * 3) == mat::CORE), "the hub rock keeps its core");
}

#[test]
fn orbit_timing() {
    let mut gen_ms = f64::MAX;
    let mut world = None;
    for _ in 0..3 {
        let t = Instant::now();
        let (w, _) = generate_orbit(1);
        gen_ms = gen_ms.min(t.elapsed().as_secs_f64() * 1e3);
        world = Some(w);
    }
    let w = world.unwrap();
    let chunks: Vec<IVec3> = w.nonempty_chunks().collect();
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut tris = 0usize;
    let t = Instant::now();
    for &c in &chunks {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        tris += mesh.triangle_count();
    }
    println!("--- orbit ---");
    println!("gen: {gen_ms:.1} ms, {} solid voxels, {} chunks, {tris} tris, mesh {:.0} ms", w.solid_count(), chunks.len(), t.elapsed().as_secs_f64() * 1e3);
    if !cfg!(debug_assertions) {
        assert!(gen_ms < 1500.0, "generation too slow: {gen_ms:.0} ms");
    }
}
