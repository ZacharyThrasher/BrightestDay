//! Integration tests: raycasts, carve determinism, hardness, collapse, meshing, arena sanity, timing.
//!
//! Timing numbers: `cargo test -p bd_voxel --release --test voxel timing -- --nocapture`.

use std::time::Instant;

use bd_core::VOXEL_SIZE;
use bd_core::voxel::{AIR, VoxelQuery, raycast, world_to_voxel};
use bd_voxel::*;
use glam::{IVec3, Vec3};

fn flat() -> (VoxelWorld, ArenaInfo) {
    generate_flat_test(128.0)
}

/// A free-standing 2×2-voxel stone pillar on the flat lab's ground, 20 m tall, at (x, z) metres.
fn build_pillar(w: &mut VoxelWorld, x: f32, z: f32) -> (IVec3, IVec3) {
    let base = world_to_voxel(Vec3::new(x, 8.0, z));
    let top = base + IVec3::new(1, 39, 1);
    w.fill_box(base, top, mat::STONE);
    (base, top)
}

#[test]
fn raycast_hits_ground_and_walls() {
    let (w, _) = flat();
    let hit = raycast(&w, Vec3::new(1.3, 30.0, 2.2), Vec3::NEG_Y, 100.0).unwrap();
    assert_eq!(hit.normal, IVec3::Y);
    assert!((hit.point.y - 8.0).abs() < 1e-3, "ground top is at 8 m, got {}", hit.point.y);
    assert_eq!(w.get(hit.voxel), mat::GRASS);

    let mut w = w;
    build_pillar(&mut w, 5.0, 5.0);
    let hit = raycast(&w, Vec3::new(0.0, 12.0, 5.2), Vec3::X, 50.0).unwrap();
    assert_eq!(hit.normal, IVec3::NEG_X);
    assert!((hit.point.x - 5.0).abs() < 1e-3);
    // Shooting into the sky misses.
    assert!(raycast(&w, Vec3::new(0.0, 12.0, 0.0), Vec3::Y, 200.0).is_none());
}

#[test]
fn carve_is_deterministic() {
    let (mut a, _) = generate_arena(7);
    let (mut b, _) = generate_arena(7);
    assert_eq!(a.content_hash(), b.content_hash(), "generation must be deterministic");
    let ops = [
        CarveOp { shape: CarveShape::Sphere { center: Vec3::new(3.3, 40.2, -7.9), radius: 4.2 }, power: 1.0, seed: 11 },
        CarveOp {
            shape: CarveShape::Capsule { a: Vec3::new(-10.0, 39.0, 5.0), b: Vec3::new(10.0, 41.0, 9.0), radius: 2.5 },
            power: 1.3,
            seed: 12,
        },
        CarveOp { shape: CarveShape::Box { min: Vec3::new(60.0, 30.0, -4.0), max: Vec3::new(66.0, 38.0, 4.0) }, power: 0.6, seed: 13 },
    ];
    for op in &ops {
        let ra = a.apply_carve(op);
        let rb = b.apply_carve(op);
        assert!(!ra.removed.is_empty());
        assert_eq!(ra, rb);
        assert_eq!(a.content_hash(), b.content_hash());
    }
    // A different seed only changes the noisy rim.
    let (mut c, _) = generate_arena(7);
    let mut op = ops[0];
    let r1 = c.carve(&op);
    let (mut d, _) = generate_arena(7);
    op.seed = 999;
    let r2 = d.carve(&op);
    assert_ne!(r1, r2);
    let ratio = r1.len() as f32 / r2.len() as f32;
    assert!((0.8..1.25).contains(&ratio), "seed should only affect the edge ({} vs {})", r1.len(), r2.len());
}

#[test]
fn different_seeds_make_different_arenas() {
    let (a, _) = generate_arena(1);
    let (b, _) = generate_arena(2);
    assert_ne!(a.content_hash(), b.content_hash());
}

#[test]
fn hardness_and_indestructible() {
    let (mut w, _) = flat();
    // Weak carve on stone bites less than a full-power one.
    let (mut w2, _) = flat();
    let c = Vec3::new(20.0, 4.0, 20.0); // inside the stone layer
    let weak = w.carve_sphere(c, 3.0, 0.4, 1).len();
    let strong = w2.carve_sphere(c, 3.0, 1.0, 1).len();
    assert!(weak > 0 && weak * 2 < strong, "weak {weak} strong {strong}");
    // Far too weak: nothing.
    assert!(w.carve_sphere(Vec3::new(-20.0, 4.0, -20.0), 3.0, 0.05, 1).is_empty());
    // The bedrock (core) layer at y = 0 survives any carve.
    let removed = w.carve_sphere(Vec3::new(0.0, 0.5, 30.0), 3.0, 100.0, 2);
    assert!(!removed.is_empty());
    assert!(removed.iter().all(|r| r.material != mat::CORE));
    assert_eq!(w.get(world_to_voxel(Vec3::new(0.0, 0.25, 30.0))), mat::CORE);
    // Leaves are chewed a bit beyond the nominal radius.
    let mut lw = VoxelWorld::centered(IVec3::new(64, 64, 64));
    lw.fill_box(IVec3::new(-20, 0, -20), IVec3::new(19, 40, 19), mat::LEAVES_GREEN);
    let mut sw = VoxelWorld::centered(IVec3::new(64, 64, 64));
    sw.fill_box(IVec3::new(-20, 0, -20), IVec3::new(19, 40, 19), mat::STONE);
    let (cc, r) = (Vec3::new(0.0, 10.0, 0.0), 4.0);
    assert!(lw.carve_sphere(cc, r, 1.0, 3).len() > sw.carve_sphere(cc, r, 1.0, 3).len());
}

#[test]
fn carve_marks_chunks_dirty() {
    let (mut w, _) = flat();
    w.drain_dirty();
    let removed = w.carve_sphere(Vec3::new(0.0, 8.0, 0.0), 3.0, 1.0, 5);
    assert!(!removed.is_empty());
    let dirty = w.drain_dirty();
    for r in &removed {
        assert!(dirty.contains(&chunk_of(r.pos)));
    }
}

#[test]
fn cut_pillar_top_falls() {
    let (mut w, _) = flat();
    let (base, top) = build_pillar(&mut w, 30.0, -12.0);
    let pillar_voxels = ((top - base + IVec3::ONE).x * (top - base + IVec3::ONE).y * (top - base + IVec3::ONE).z) as usize;
    // Blast a slice out of the pillar 3 m above the ground.
    let cut_y = 11.0;
    let out = w.apply_carve(&CarveOp {
        shape: CarveShape::Box {
            min: Vec3::new(29.0, cut_y, -13.0),
            max: Vec3::new(32.0, cut_y + 1.0, -10.0),
        },
        power: 1.0,
        seed: 1,
    });
    assert_eq!(out.falling.len(), 1, "exactly the top should fall");
    let f = &out.falling[0];
    assert!(f.voxels.iter().all(|(v, m)| v.y as f32 * VOXEL_SIZE >= cut_y && *m == mat::STONE));
    assert_eq!(f.len() + out.removed.len() + (((cut_y - 8.0) / VOXEL_SIZE) as usize * 4), pillar_voxels);
    // Removed from the world; the stump stays.
    for (v, _) in &f.voxels {
        assert_eq!(w.get(*v), AIR);
    }
    assert_eq!(w.get(base), mat::STONE);
    let com = f.center_of_mass();
    assert!(com.y > cut_y && (com.x - 30.5).abs() < 0.6);

    // Carving a crater in open ground drops nothing.
    let out = w.apply_carve(&CarveOp { shape: CarveShape::Sphere { center: Vec3::new(-20.0, 8.0, 20.0), radius: 3.0 }, power: 1.0, seed: 2 });
    assert!(!out.removed.is_empty());
    assert!(out.falling.is_empty());
}

#[test]
fn chopping_a_tree_drops_the_canopy() {
    let (mut w, _) = flat();
    // The lab's tree stands near (-40, 40) for size 128.
    let (tx, tz) = (-40.0, 40.0);
    // Find its trunk 3 m up.
    let mut trunk = None;
    'find: for dz in -8..=8 {
        for dx in -8..=8 {
            let v = world_to_voxel(Vec3::new(tx, 11.0, tz)) + IVec3::new(dx, 0, dz);
            if matches!(w.get(v), mat::WOOD | mat::DARK_WOOD) {
                trunk = Some(v);
                break 'find;
            }
        }
    }
    let trunk = trunk.expect("tree trunk");
    let c = bd_core::voxel::voxel_center(trunk);
    let out = w.apply_carve(&CarveOp {
        shape: CarveShape::Capsule { a: c - Vec3::X * 6.0, b: c + Vec3::X * 6.0, radius: 2.2 },
        power: 1.0,
        seed: 3,
    });
    let leaves: usize = out
        .falling
        .iter()
        .flat_map(|f| f.voxels.iter())
        .filter(|(_, m)| material_def(*m).debris == DebrisClass::Leaves)
        .count();
    assert!(leaves > 500, "canopy should fall, got {leaves} leaves in {} chunks", out.falling.len());
}

#[test]
fn mesher_sanity() {
    let (w, _) = flat();
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        let n = mesh.vertex_count();
        assert_eq!(mesh.normals.len(), n);
        assert_eq!(mesh.colors.len(), n);
        assert_eq!(mesh.indices.len() % 6, 0);
        assert!(mesh.indices.iter().all(|&i| (i as usize) < n));
        let lo = chunk_origin(c).as_vec3() * VOXEL_SIZE;
        let hi = lo + Vec3::splat(CHUNK_SIZE as f32 * VOXEL_SIZE);
        for p in &mesh.positions {
            let p = Vec3::from(*p);
            assert!(p.cmpge(lo - 1e-4).all() && p.cmple(hi + 1e-4).all());
        }
        for col in &mesh.colors {
            assert!(col[0] >= 0.0 && col[1] >= 0.0 && col[2] >= 0.0 && col[3] == 1.0);
        }
        total += mesh.triangle_count();
    }
    assert!(total > 1000);
    // Fully buried chunk (deep stone below the surface, surrounded by stone) → no faces.
    let buried = IVec3::new(0, 0, 0); // voxels y 0..32 = 0..16 m: ground top is 8 m, so not buried
    mesher.mesh_chunk_into(&w, buried, &mut mesh);
    let mut solid = VoxelWorld::centered(IVec3::new(96, 96, 96));
    solid.fill_box(IVec3::new(-48, 0, -48), IVec3::new(47, 95, 47), mat::STONE);
    mesher.mesh_chunk_into(&solid, IVec3::new(0, 1, 0), &mut mesh);
    assert!(mesh.is_empty(), "interior chunk must produce no faces");
}

#[test]
fn arena_is_sane() {
    let (w, info) = generate_arena(1);
    assert!(info.spawns.len() >= 8, "spawns: {}", info.spawns.len());
    for (i, a) in info.spawns.iter().enumerate() {
        // Standing on solid ground with head room.
        let feet = world_to_voxel(*a);
        assert!(w.is_solid(feet - IVec3::Y), "spawn {i} floating");
        for dy in 0..4 {
            assert!(!w.is_solid(feet + IVec3::Y * dy), "spawn {i} buried");
        }
        assert!(a.y > info.void_y + 10.0);
        for b in &info.spawns[i + 1..] {
            assert!(a.distance(*b) >= 12.0, "spawns too close: {a} {b}");
        }
    }
    // Spread: spawns cover all four quadrants.
    for (sx, sz) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        assert!(info.spawns.iter().any(|s| s.x * sx > 0.0 && s.z * sz > 0.0));
    }
    // ~150–200 m across.
    let b = w.bounds();
    assert!((150.0..=200.0).contains(&(b.max.x - b.min.x)));
    // Every solid voxel is connected to an indestructible core (nothing collapses spontaneously
    // the first time something nearby is carved).
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let size = hi - lo;
    let idx = |v: IVec3| {
        let r = v - lo;
        (r.x + size.x * (r.z + size.z * r.y)) as usize
    };
    let mut seen = vec![false; (size.x * size.y * size.z) as usize];
    let mut stack: Vec<IVec3> = Vec::new();
    for c in w.nonempty_chunks() {
        let o = chunk_origin(c);
        for i in 0..CHUNK_VOLUME as i32 {
            let v = o + IVec3::new(i & 31, i >> 10, (i >> 5) & 31);
            if material_def(w.get(v)).indestructible {
                seen[idx(v)] = true;
                stack.push(v);
            }
        }
    }
    assert!(stack.len() > 100, "cores present");
    while let Some(v) = stack.pop() {
        for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
            let n = v + d;
            if w.in_bounds(n) && w.get(n) != AIR && !seen[idx(n)] {
                seen[idx(n)] = true;
                stack.push(n);
            }
        }
    }
    let mut orphans = Vec::new();
    for c in w.nonempty_chunks() {
        let o = chunk_origin(c);
        for i in 0..CHUNK_VOLUME as i32 {
            let v = o + IVec3::new(i & 31, i >> 10, (i >> 5) & 31);
            if w.get(v) != AIR && !seen[idx(v)] {
                orphans.push(v);
            }
        }
    }
    assert!(orphans.len() < 50, "{} unanchored voxels, e.g. {:?}", orphans.len(), &orphans[..orphans.len().min(5)]);
}

#[test]
fn timing() {
    let t = Instant::now();
    let (mut w, _) = generate_arena(1);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;

    let chunks: Vec<IVec3> = w.nonempty_chunks().collect();
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let t = Instant::now();
    let mut tris = 0usize;
    let mut worst = 0.0f64;
    for &c in &chunks {
        let tc = Instant::now();
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        worst = worst.max(tc.elapsed().as_secs_f64() * 1e3);
        tris += mesh.triangle_count();
    }
    let mesh_ms = t.elapsed().as_secs_f64() * 1e3;

    // Parallel full-world mesh (what the client does on worker threads).
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let t = Instant::now();
    std::thread::scope(|s| {
        for part in chunks.chunks(chunks.len().div_ceil(threads)) {
            let w = &w;
            s.spawn(move || {
                let mut m = Mesher::new();
                let mut out = ChunkMesh::default();
                for &c in part {
                    m.mesh_chunk_into(w, c, &mut out);
                }
            });
        }
    });
    let par_ms = t.elapsed().as_secs_f64() * 1e3;

    // Typical gameplay carve: 3 m crater in the Heart's floor (+ collapse check).
    let mut carve_ms = Vec::new();
    for i in 0..10 {
        let op = CarveOp {
            shape: CarveShape::Sphere { center: Vec3::new(-8.0 + i as f32 * 2.0, 40.0, 8.0), radius: 3.0 },
            power: 1.0,
            seed: i,
        };
        let t = Instant::now();
        w.apply_carve(&op);
        carve_ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    carve_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Worst-ish case: sever the lone tree on the south stepping stone (exhaustive flood of its
    // whole canopy, which then falls). Grove trees have interlocking canopies that hold each other up.
    let (mut w2, _) = generate_arena(1);
    let mut fell = 0;
    let mut tree_ms = 0.0;
    'outer: for x in -40..40 {
        for z in -190..-120 {
            let v = IVec3::new(x, 84, z);
            if matches!(w2.get(v), mat::WOOD | mat::DARK_WOOD) {
                // Aim at the trunk's centroid in this layer.
                let mut sum = Vec3::ZERO;
                let mut n = 0.0;
                for dz in -6..=6 {
                    for dx in -6..=6 {
                        let u = v + IVec3::new(dx, 0, dz);
                        if matches!(w2.get(u), mat::WOOD | mat::DARK_WOOD) {
                            sum += bd_core::voxel::voxel_center(u);
                            n += 1.0;
                        }
                    }
                }
                let c = sum / n;
                let t = Instant::now();
                let out = w2.apply_carve(&CarveOp {
                    shape: CarveShape::Capsule { a: c - Vec3::X * 5.0, b: c + Vec3::X * 5.0, radius: 3.0 },
                    power: 1.5,
                    seed: 9,
                });
                tree_ms = t.elapsed().as_secs_f64() * 1e3;
                fell = out.falling.iter().map(|f| f.len()).sum();
                break 'outer;
            }
        }
    }

    println!("--- bd_voxel timing ({} build) ---", if cfg!(debug_assertions) { "debug" } else { "release" });
    println!("arena gen:            {gen_ms:.1} ms ({} solid voxels)", w.solid_count());
    println!("full mesh (1 thread): {mesh_ms:.1} ms for {} chunks, {tris} tris; worst chunk {worst:.2} ms, avg {:.3} ms", chunks.len(), mesh_ms / chunks.len() as f64);
    println!("full mesh ({threads} threads): {par_ms:.1} ms");
    println!("carve r=3 m + collapse: median {:.3} ms, max {:.3} ms", carve_ms[5], carve_ms[9]);
    println!("tree felling carve + collapse: {tree_ms:.2} ms ({fell} voxels fell)");
    assert!(fell > 1000, "the south tree should fall");
}

#[test]
fn tutorial_is_sane() {
    let (w, info) = generate_tutorial();
    let (w2, _) = generate_tutorial();
    assert_eq!(w.content_hash(), w2.content_hash(), "tutorial generation must be deterministic");
    assert_eq!(info.stations.len(), 6);
    for st in &info.stations {
        // Starts: standing on ground with head room.
        let feet = world_to_voxel(st.start);
        assert!(w.is_solid(feet - IVec3::Y), "{} start floating", st.name);
        for dy in 0..4 {
            assert!(!w.is_solid(feet + IVec3::Y * dy), "{} start buried", st.name);
        }
        // Goals: ground right under the goal box's centre.
        let c = st.goal.center();
        let below = world_to_voxel(Vec3::new(c.x, st.goal.min.y + 0.05, c.z)) - IVec3::Y;
        assert!(w.is_solid(below), "{} goal has no floor", st.name);
        // Anchors sit on a solid underside.
        for a in &st.anchors {
            assert!(w.is_solid(world_to_voxel(*a + Vec3::Y * 0.05)), "{} anchor {a} not on solid", st.name);
            assert!(!w.is_solid(world_to_voxel(*a - Vec3::Y * 0.05)), "{} anchor {a} not an underside", st.name);
        }
        assert!((3..=5).contains(&st.hint.split_whitespace().count()), "hint length: {}", st.hint);
    }
    assert_eq!(info.peak_gates.last().copied(), Some(info.start_finish));
    let m = info.medals;
    assert!(m.gold > 0.0 && m.gold < m.silver && m.silver < m.bronze);
}
