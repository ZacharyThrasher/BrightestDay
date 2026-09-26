//! Cybertron map checks (feature `cybertron`): determinism, spawns (standable, head room, spread,
//! balanced between the cities, clear of the transforming pieces / gates / surf lines), mutual
//! reachability on foot (walk, fall, ≤ 2 m jumps — no transform, portal or tether needed),
//! transforming pieces extend into empty space, the space-bridge discs are open, KO geometry
//! (the Well and the rim drop to the void), indestructible frames, and the triangle budget.
//!
//! `cargo test -p bd_voxel --features cybertron --release --test cybertron -- --nocapture`
#![cfg(feature = "cybertron")]

use std::collections::VecDeque;

use bd_core::voxel::{AIR, VoxelQuery, world_to_voxel};
use bd_voxel::maps::cybertron::{self, STREET, VOID_Y, WELL_R, generate_cybertron};
use bd_voxel::{CarveOp, CarveShape, ChunkMesh, Mesher, VoxelWorld, mat};
use glam::{IVec3, Vec3};

const TRI_BUDGET: usize = 1_100_000;

struct Grid {
    lo: IVec3,
    size: IVec3,
}

impl Grid {
    fn new(w: &VoxelWorld) -> Self {
        Self { lo: w.voxel_min(), size: w.voxel_max() - w.voxel_min() }
    }
    fn idx(&self, v: IVec3) -> Option<usize> {
        let r = v - self.lo;
        if r.cmplt(IVec3::ZERO).any() || r.cmpge(self.size).any() {
            return None;
        }
        Some((r.x + self.size.x * (r.z + self.size.z * r.y)) as usize)
    }
}

fn clear(w: &VoxelWorld, x: i32, y: i32, z: i32, h: i32) -> bool {
    (0..2).all(|dz| (0..2).all(|dx| (y..y + h).all(|yy| w.get(IVec3::new(x + dx, yy, z + dz)) == AIR)))
}

fn standable(w: &VoxelWorld, x: i32, y: i32, z: i32) -> bool {
    y as f32 * 0.5 >= VOID_Y && clear(w, x, y, z, 4) && (0..2).any(|dz| (0..2).any(|dx| w.get(IVec3::new(x + dx, y - 1, z + dz)) != AIR))
}

fn push(g: &Grid, seen: &mut [bool], q: &mut VecDeque<IVec3>, v: IVec3) {
    if let Some(i) = g.idx(v)
        && !seen[i]
    {
        seen[i] = true;
        q.push_back(v);
    }
}

/// Nodes reachable from `start` by walking, stepping ≤ 0.5 m, jumping ≤ 2 m and falling.
fn reach(w: &VoxelWorld, g: &Grid, start: IVec3) -> Vec<bool> {
    let mut seen = vec![false; (g.size.x * g.size.y * g.size.z) as usize];
    let mut q = VecDeque::new();
    push(g, &mut seen, &mut q, start);
    while let Some(v) = q.pop_front() {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (v.x + dx, v.z + dz);
            for dy in 1..=4 {
                if !clear(w, v.x, v.y, v.z, dy + 4) {
                    break;
                }
                if standable(w, nx, v.y + dy, nz) {
                    push(g, &mut seen, &mut q, IVec3::new(nx, v.y + dy, nz));
                }
            }
            if !clear(w, nx, v.y, nz, 4) {
                continue;
            }
            let mut ny = v.y;
            while ny as f32 * 0.5 >= VOID_Y && !standable(w, nx, ny, nz) {
                ny -= 1;
                if !clear(w, nx, ny, nz, 1) {
                    ny = i32::MIN;
                    break;
                }
            }
            if ny != i32::MIN && standable(w, nx, ny, nz) {
                push(g, &mut seen, &mut q, IVec3::new(nx, ny, nz));
            }
        }
    }
    seen
}

fn spawn_node(w: &VoxelWorld, s: Vec3) -> Option<IVec3> {
    let v = (s / 0.5).floor().as_ivec3();
    [(0, 0), (-1, 0), (0, -1), (-1, -1)].into_iter().map(|(dx, dz)| IVec3::new(v.x + dx, v.y, v.z + dz)).find(|n| standable(w, n.x, n.y, n.z))
}

#[test]
fn deterministic_per_seed() {
    let (a, ia) = generate_cybertron(3);
    let (b, ib) = generate_cybertron(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia, ib);
    let (c, _) = generate_cybertron(4);
    assert_ne!(a.content_hash(), c.content_hash(), "seeds vary details");
}

#[test]
fn spawns_are_standable_spread_balanced_and_clear() {
    let (w, info) = generate_cybertron(1);
    assert!((8..=10).contains(&info.spawns.len()), "spawns: {}", info.spawns.len());
    let (mut west, mut east) = (0, 0);
    for (i, s) in info.spawns.iter().enumerate() {
        let feet = world_to_voxel(*s);
        assert!(w.is_solid(feet - IVec3::Y), "spawn {i} floating at {s}");
        for dy in 0..5 {
            for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                assert!(!w.is_solid(feet + IVec3::new(dx, dy, dz)), "spawn {i} buried at {s}");
            }
        }
        assert!((s.y - STREET).abs() < 0.01, "spawn {i} off street level: {s}");
        for b in &info.spawns[i + 1..] {
            assert!(s.distance(*b) >= 14.0, "spawns too close: {s} {b}");
        }
        if s.x < 0.0 { west += 1 } else { east += 1 }
        // Not in a choke: open street in a 3 m radius (no wall within reach of the hull).
        for a in 0..16 {
            let d = Vec3::new((a as f32 * std::f32::consts::FRAC_PI_8).cos(), 0.0, (a as f32 * std::f32::consts::FRAC_PI_8).sin()) * 3.0;
            assert!(!w.is_solid(world_to_voxel(*s + d + Vec3::Y)), "spawn {i} at {s} is hemmed in");
        }
        // Clear of the transforming pieces (extended and folded), gates and surf lines.
        for m in &info.arena.mechanics.movers {
            for p in &m.parts {
                for t in [0.0, 1.0] {
                    let (c, q) = p.pose(t);
                    // Distance from the spawn's hull (feet … head) to the panel box.
                    let d = (0..4)
                        .map(|k| {
                            let l = q.inverse() * (*s + Vec3::Y * (k as f32 * 0.6) - c);
                            (l.abs() - p.half).max(Vec3::ZERO).length()
                        })
                        .fold(f32::MAX, f32::min);
                    assert!(d > 6.0, "spawn {i} within {d:.1} m of mover {}", m.name);
                }
            }
        }
        for g in &info.arena.mechanics.portals {
            assert!(s.distance(g.a) > 14.0 && s.distance(g.b) > 14.0, "spawn {i} next to a gate");
        }
    }
    assert_eq!(west, east, "spawns split evenly between Iacon and Kaon");
    // Balanced: mean distance to the Well within 15 %.
    let mean = |f: &dyn Fn(&Vec3) -> bool| {
        let v: Vec<f32> = info.spawns.iter().filter(|s| f(s)).map(|s| Vec3::new(s.x, 0.0, s.z).length()).collect();
        v.iter().sum::<f32>() / v.len() as f32
    };
    let (mw, me) = (mean(&|s| s.x < 0.0), mean(&|s| s.x >= 0.0));
    assert!((mw - me).abs() / mw.max(me) < 0.15, "unbalanced spawn distances: Iacon {mw:.0} m, Kaon {me:.0} m");
}

#[test]
fn every_spawn_reaches_every_other_on_foot() {
    let (w, info) = generate_cybertron(1);
    let g = Grid::new(&w);
    let nodes: Vec<IVec3> = info.spawns.iter().map(|s| spawn_node(&w, *s).unwrap_or_else(|| panic!("no standable node at spawn {s}"))).collect();
    // Plus the key places: the pit floor, the Well's ring deck, both expressways, the rust bay.
    let places = [
        Vec3::new(cybertron::PIT.x, cybertron::PIT_FLOOR_Y, 4.0),
        Vec3::new(0.0, cybertron::RING_TOP, (cybertron::RING_R0 + cybertron::RING_R1) * 0.5),
        Vec3::new(-40.0, cybertron::EXPRESS_TOP, cybertron::express_z(40.0)),
        Vec3::new(40.0, cybertron::EXPRESS_TOP, -cybertron::express_z(40.0)),
        Vec3::new(0.0, STREET, -45.0),
    ];
    let seen = reach(&w, &g, nodes[0]);
    for (i, n) in nodes.iter().enumerate() {
        assert!(seen[g.idx(*n).unwrap()], "spawn {i} ({}) not reachable from spawn 0", info.spawns[i]);
    }
    for p in places {
        let n = (0..6).find_map(|dy| spawn_node(&w, p + Vec3::Y * dy as f32 * 0.5)).unwrap_or_else(|| panic!("no standable node at {p}"));
        assert!(seen[g.idx(n).unwrap()], "{p} not reachable on foot");
        // …and back.
        let back = reach(&w, &g, n);
        assert!(back[g.idx(nodes[0]).unwrap()], "can't get back from {p}");
    }
    // The rust bay: fall in, climb back out up the drifts.
    let bay = (0..40).find_map(|dy| spawn_node(&w, Vec3::new(-2.0, 10.0 + dy as f32 * 0.5, 58.0))).expect("bay floor");
    let back = reach(&w, &g, bay);
    assert!(back[g.idx(nodes[0]).unwrap()], "no way out of the rust bay");
}

#[test]
fn transforming_pieces_extend_into_empty_space_and_gates_are_open() {
    let (w, info) = generate_cybertron(1);
    assert_eq!(info.arena.mechanics.movers.len(), 5);
    for m in &info.arena.mechanics.movers {
        for p in &m.parts {
            let (c, q) = p.pose(1.0);
            let h = p.half - Vec3::splat(0.35);
            let n = 8;
            let mut solid = 0;
            for i in 0..=n {
                for j in 0..=n {
                    for k in 0..=2 {
                        let l = Vec3::new(h.x * (i as f32 / n as f32 * 2.0 - 1.0), h.y * (k as f32 - 1.0), h.z * (j as f32 / n as f32 * 2.0 - 1.0));
                        if w.is_solid(world_to_voxel(c + q * l)) {
                            solid += 1;
                            if std::env::var("CYB_DEBUG").is_ok() {
                                eprintln!("{} solid at {} ({})", m.name, c + q * l, bd_voxel::material_def(w.get(world_to_voxel(c + q * l))).name);
                            }
                        }
                    }
                }
            }
            assert!(solid <= 6, "mover {} overlaps terrain at {solid} samples", m.name);
            assert!(m.folded_secs >= 10.0 && m.extended_secs >= 10.0, "mover {} changes too often", m.name);
        }
    }
    for g in &info.arena.mechanics.portals {
        for (c, n) in [(g.a, g.a_normal), (g.b, g.b_normal)] {
            for a in 0..12 {
                let r = g.radius * 0.8;
                let ang = a as f32 * std::f32::consts::FRAC_PI_6;
                let p = c + Vec3::new(0.0, ang.sin() * r, ang.cos() * r) + n * 0.5;
                assert!(!w.is_solid(world_to_voxel(p)), "gate disc blocked at {p}");
            }
            // An open approach in front of each gate.
            for d in 1..10 {
                assert!(!w.is_solid(world_to_voxel(c + n * d as f32 - Vec3::Y * 4.0)), "gate approach blocked");
            }
        }
    }
}

#[test]
fn ko_geometry_and_indestructible_frames() {
    let (mut w, info) = generate_cybertron(1);
    // The Well of AllSparks is open all the way down past the void line.
    for y in (0..(STREET * 2.0) as i32).rev() {
        if (y as f32) * 0.5 < 9.0 {
            break;
        }
        assert_eq!(w.get(IVec3::new(0, y, 3)), AIR, "the Well shaft is blocked at y = {}", y as f32 * 0.5);
    }
    // Off the plate's rim: nothing to land on down to the void.
    for (x, z) in [(124.0f32, 0.0f32), (-124.0, 10.0), (0.0, -86.0)] {
        for y in (VOID_Y * 2.0) as i32..(STREET * 2.0) as i32 {
            assert_eq!(w.get(IVec3::new((x * 2.0) as i32, y, (z * 2.0) as i32)), AIR, "rim at ({x}, {z}) catches fallers");
        }
    }
    // Star KO: the top blast zone is well above everything.
    assert!(info.arena.bounds.max.y > 120.0 && info.arena.gravity.is_none());
    // The Well lip, the frame slab and the gates survive huge blasts; streets break.
    let lip = Vec3::new((WELL_R + 0.6) * 0.866, STREET - 0.25, (WELL_R + 0.6) * 0.5);
    let out = w.apply_carve(&CarveOp { shape: CarveShape::Sphere { center: lip, radius: 4.0 }, power: 5.0, seed: 1 });
    assert!(out.removed.len() > 100, "streets break");
    assert!([mat::ENERGON_FRAME].contains(&w.get(world_to_voxel(lip))), "the Well lip survives");
    let deep = Vec3::new(-50.0, 12.25, 30.0);
    w.apply_carve(&CarveOp { shape: CarveShape::Sphere { center: deep + Vec3::Y * 2.0, radius: 6.0 }, power: 8.0, seed: 2 });
    assert_eq!(w.get(world_to_voxel(deep)), mat::CYBER_FRAME, "the frame slab holds");
    for g in &info.arena.mechanics.portals {
        let rim = g.a + Vec3::Y * (g.radius + 1.0);
        w.apply_carve(&CarveOp { shape: CarveShape::Sphere { center: rim, radius: 3.0 }, power: 8.0, seed: 3 });
        assert!(w.is_solid(world_to_voxel(rim)), "the gate frame is indestructible");
    }
    assert!(info.pruned_voxels < 64, "detached kit pieces: {}", info.pruned_voxels);
}

#[test]
fn triangle_budget_and_timing() {
    let t = std::time::Instant::now();
    let (w, _) = generate_cybertron(1);
    let gen_ms = t.elapsed().as_secs_f64() * 1e3;
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut tris = 0;
    for c in w.nonempty_chunks().collect::<Vec<_>>() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        tris += mesh.triangle_count();
    }
    println!("cybertron: gen {gen_ms:.0} ms, {} solid voxels, {tris} tris", w.solid_count());
    assert!(tris <= TRI_BUDGET, "{tris} triangles (budget {TRI_BUDGET})");
}
