//! Backrooms map checks (feature `backrooms`): determinism, spawn validity + mutual reachability
//! (walk / fall / ≤ 2 m jumps with a 1 m hull), void pits only on the bottom level, routes clear,
//! sealed interiors present, and the triangle budget per storey.
#![cfg(feature = "backrooms")]

use std::collections::VecDeque;

use bd_core::voxel::AIR;
use bd_voxel::maps::backrooms::{self, GRID0, LEVELS, N, VOID_Y, floor_v, generate_backrooms, level_at, plan_for};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld};
use glam::{IVec3, Vec3};

/// Triangle budget per storey (air side of each face) and for the whole map.
const TRI_BUDGET_LEVEL: usize = 220_000;
const TRI_BUDGET_TOTAL: usize = 1_150_000;

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

/// 2 × 2 voxel footprint (1 m) with 4 voxels (2 m) of air from `y`.
fn clear(w: &VoxelWorld, x: i32, y: i32, z: i32, h: i32) -> bool {
    (0..2).all(|dz| (0..2).all(|dx| (y..y + h).all(|yy| w.get(IVec3::new(x + dx, yy, z + dz)) == AIR)))
}

fn standable(w: &VoxelWorld, x: i32, y: i32, z: i32) -> bool {
    y as f32 * 0.5 >= VOID_Y && clear(w, x, y, z, 4) && (0..2).any(|dz| (0..2).any(|dx| w.get(IVec3::new(x + dx, y - 1, z + dz)) != AIR))
}

/// Nodes reachable from `start` (feet voxel) by walking, stepping ≤ 0.5 m, jumping ≤ 2 m and falling.
fn reach(w: &VoxelWorld, g: &Grid, start: IVec3) -> Vec<bool> {
    let mut seen = vec![false; (g.size.x * g.size.y * g.size.z) as usize];
    let mut q = VecDeque::new();
    if let Some(i) = g.idx(start) {
        seen[i] = true;
        q.push_back(start);
    }
    while let Some(v) = q.pop_front() {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (v.x + dx, v.z + dz);
            // Up: any ledge within a 2 m jump, with head room over the take-off spot.
            for dy in 1..=4 {
                let ny = v.y + dy;
                if !clear(w, v.x, v.y, v.z, dy + 4) {
                    break;
                }
                if standable(w, nx, ny, nz) {
                    push(g, &mut seen, &mut q, IVec3::new(nx, ny, nz));
                }
            }
            // Level / down: fall until something is under the hull.
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

fn push(g: &Grid, seen: &mut [bool], q: &mut VecDeque<IVec3>, v: IVec3) {
    if let Some(i) = g.idx(v)
        && !seen[i]
    {
        seen[i] = true;
        q.push_back(v);
    }
}

/// The standable node for a spawn (feet position): the hull's min corner, searched nearby.
fn spawn_node(w: &VoxelWorld, s: Vec3) -> Option<IVec3> {
    let v = (s / 0.5).floor().as_ivec3();
    for (dx, dz) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
        if standable(w, v.x + dx, v.y, v.z + dz) {
            return Some(IVec3::new(v.x + dx, v.y, v.z + dz));
        }
    }
    None
}

#[test]
fn deterministic_per_seed() {
    let (a, ia) = generate_backrooms(3);
    let (b, ib) = generate_backrooms(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia.spawns, ib.spawns);
    assert_eq!(ia.routes, ib.routes);
    let (c, _) = generate_backrooms(4);
    assert_ne!(a.content_hash(), c.content_hash(), "different seeds should differ");
}

#[test]
fn spawns_valid_and_mutually_reachable() {
    // BR_SEEDS=a..b widens the sweep (default: 1, 2, 7).
    let seeds: Vec<u64> =
        match std::env::var("BR_SEEDS").ok().and_then(|s| s.split_once("..").map(|(a, b)| (a.parse().ok(), b.parse().ok()))) {
            Some((Some(a), Some(b))) => (a..b).collect(),
            _ => vec![1, 2, 7],
        };
    for seed in seeds {
        let (w, info) = generate_backrooms(seed);
        assert!((8..=10).contains(&info.spawns.len()), "seed {seed}: {} spawns", info.spawns.len());
        let mut per_level = [0; LEVELS];
        for s in &info.spawns {
            let v = (*s / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), AIR, "seed {seed}: spawn {s} inside a solid");
            assert_eq!(w.get(v + IVec3::Y * 3), AIR, "seed {seed}: spawn {s} without head room");
            assert_ne!(w.get(v - IVec3::Y), AIR, "seed {seed}: spawn {s} floating");
            per_level[level_at(s.y + 1.0).expect("spawn in a storey")] += 1;
        }
        assert!(per_level.iter().all(|&n| n >= 1), "seed {seed}: spawns per level {per_level:?}");
        let g = Grid::new(&w);
        let nodes: Vec<IVec3> =
            info.spawns.iter().map(|s| spawn_node(&w, *s).unwrap_or_else(|| panic!("seed {seed}: spawn {s} not standable"))).collect();
        let matrix: Vec<String> = nodes
            .iter()
            .map(|&from| {
                let seen = reach(&w, &g, from);
                nodes.iter().map(|&to| if seen[g.idx(to).unwrap()] { '1' } else { '.' }).collect()
            })
            .collect();
        if matrix.iter().any(|r| r.contains('.')) {
            eprintln!(
                "seed {seed} reachability (row = from):
{}",
                matrix.join(
                    "
"
                )
            );
        }
        for (i, &from) in nodes.iter().enumerate() {
            let seen = reach(&w, &g, from);
            for (j, &to) in nodes.iter().enumerate() {
                assert!(
                    seen[g.idx(to).unwrap()],
                    "seed {seed}: spawn {j} {:?} not reachable from spawn {i} {:?}",
                    info.spawns[j],
                    info.spawns[i]
                );
            }
        }
    }
}

#[test]
fn void_pits_only_on_the_bottom_level() {
    for seed in [1u64, 5] {
        let (w, _) = generate_backrooms(seed);
        let plan = plan_for(seed);
        assert!(!plan.pits.is_empty(), "seed {seed}: no void pits");
        let (x0, x1) = (GRID0 + 1, GRID0 + 8 * N - 1);
        let mut void_cols = 0;
        for z in x0..=x1 {
            for x in x0..=x1 {
                for l in 0..LEVELS {
                    let fy = floor_v(l);
                    if w.get(IVec3::new(x, fy - 1, z)) != AIR || w.get(IVec3::new(x, fy - 2, z)) != AIR {
                        continue;
                    }
                    // A hole in level l's floor: where does it lead?
                    let land = (0..fy - 2).rev().find(|&y| w.get(IVec3::new(x, y, z)) != AIR);
                    let to_void = land.is_none_or(|y| ((y + 1) as f32 * 0.5) < VOID_Y);
                    if to_void {
                        assert_eq!(l, 0, "seed {seed}: void hole at ({x}, {z}) in level {l}'s floor");
                        let (i, j) = ((x - GRID0).div_euclid(8), (z - GRID0).div_euclid(8));
                        assert!(plan.pits.iter().any(|p| p.contains(i, j)), "seed {seed}: void hole outside a pit at ({x}, {z})");
                        void_cols += 1;
                    }
                }
            }
        }
        assert!(void_cols > 0);
    }
}

#[test]
fn routes_clear_and_interiors_sealed() {
    let (w, info) = generate_backrooms(1);
    let bhop = info.route("hotel_bhop").expect("hotel bhop route");
    let (a, b) = (bhop.waypoints[0], bhop.waypoints[1]);
    assert!(a.distance(b) > 60.0, "bhop lane only {:.0} m", a.distance(b));
    for k in 0..=200 {
        let p = a.lerp(b, k as f32 / 200.0);
        for dz in [-1.5f32, 0.0, 1.5] {
            let v = ((p + Vec3::new(0.0, 0.1, dz)) / 0.5).floor().as_ivec3();
            for dy in 0..6 {
                assert_eq!(w.get(v + IVec3::Y * dy), AIR, "bhop lane blocked at {:?}", p + Vec3::new(0.0, dy as f32 * 0.5, dz));
            }
        }
    }
    let swing = info.route("sky_swing").expect("sky swing route");
    let anchor = swing.anchors[0];
    assert_ne!(w.get_at(anchor + Vec3::Y * 0.3), AIR, "swing anchor isn't solid ceiling");
    assert!(anchor.y - swing.waypoints[0].y > 12.0, "sky room too low for a swing");
    assert!(w.has_enclosed_air(), "no sealed interiors (drop-ceiling plenums / closed rooms)");
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_backrooms(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut bins = [0usize; LEVELS + 1];
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        total += mesh.triangle_count();
        for tri in mesh.indices.as_chunks::<3>().0 {
            let (a, n) = (mesh.positions[tri[0] as usize], mesh.normals[tri[0] as usize]);
            let (b, cc) = (mesh.positions[tri[1] as usize], mesh.positions[tri[2] as usize]);
            let p = Vec3::new(a[0] + b[0] + cc[0], a[1] + b[1] + cc[1], a[2] + b[2] + cc[2]) / 3.0 + Vec3::from_array(n) * 0.1;
            let inside = p.x.abs() < 38.0 && p.z.abs() < 38.0;
            bins[if inside { level_at(p.y).unwrap_or(LEVELS) } else { LEVELS }] += 1;
        }
    }
    for (l, &b) in bins.iter().enumerate().take(LEVELS) {
        assert!(b <= TRI_BUDGET_LEVEL, "level {l}: {b} triangles > {TRI_BUDGET_LEVEL}");
    }
    assert!(total <= TRI_BUDGET_TOTAL, "{total} triangles > {TRI_BUDGET_TOTAL}");
    let _ = backrooms::MAX_PROPS;
}

/// Debug: ASCII slice around `BR_AT=x,y,z` (metres) for `BR_SEED`.
#[test]
#[ignore]
fn debug_slice() {
    let seed: u64 = std::env::var("BR_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let at: Vec<f32> = std::env::var("BR_AT").unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect();
    let (w, _) = generate_backrooms(seed);
    let v = (Vec3::new(at[0], at[1], at[2]) / 0.5).floor().as_ivec3();
    for dy in [0, 1, 4, 9] {
        eprintln!("y = {} (+{dy})", v.y + dy);
        for dz in (-14..=14).rev() {
            let row: String = (-14..=14)
                .map(|dx| {
                    let q = IVec3::new(v.x + dx, v.y + dy, v.z + dz);
                    let m = w.get(q);
                    if dx == 0 && dz == 0 {
                        '@'
                    } else if m == AIR {
                        if w.get(q - IVec3::Y) == AIR { ' ' } else { '.' }
                    } else {
                        char::from(b'A' + (m % 26))
                    }
                })
                .collect();
            eprintln!("{row}");
        }
    }
}
