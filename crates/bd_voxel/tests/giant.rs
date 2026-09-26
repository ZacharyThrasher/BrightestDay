//! Giant's Room checks (feature `giant`): determinism, spawn validity + mutual reachability
//! (walk / fall / ≤ 2 m jumps with a 1 m hull), void pits exactly where designed, the window and
//! ceiling blast zones, nothing floating, surf lines clear of spawns, and the triangle budget.
#![cfg(feature = "giant")]

use std::collections::VecDeque;

use bd_core::voxel::AIR;
use bd_voxel::maps::giant::{self, FLOOR_Y, HX, HZ, VOID_Y, generate_giant};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld, material_def};
use glam::{IVec3, Vec2, Vec3};

const TRI_BUDGET: usize = 1_000_000;

/// Flat solid bitmap of the world (fast lookups for the reachability search).
struct Grid {
    lo: IVec3,
    size: IVec3,
    solid: Vec<bool>,
}

impl Grid {
    fn new(w: &VoxelWorld) -> Self {
        let (lo, size) = (w.voxel_min(), w.voxel_max() - w.voxel_min());
        let mut solid = vec![false; (size.x * size.y * size.z) as usize];
        for y in 0..size.y {
            for z in 0..size.z {
                for x in 0..size.x {
                    solid[(x + size.x * (z + size.z * y)) as usize] = w.get(lo + IVec3::new(x, y, z)) != AIR;
                }
            }
        }
        Self { lo, size, solid }
    }
    fn idx(&self, v: IVec3) -> Option<usize> {
        let r = v - self.lo;
        if r.cmplt(IVec3::ZERO).any() || r.cmpge(self.size).any() {
            return None;
        }
        Some((r.x + self.size.x * (r.z + self.size.z * r.y)) as usize)
    }
    fn air(&self, v: IVec3) -> bool {
        self.idx(v).is_none_or(|i| !self.solid[i])
    }
    /// 2 × 2 voxel footprint (1 m) with `h` voxels of air from `y`.
    fn clear(&self, x: i32, y: i32, z: i32, h: i32) -> bool {
        (0..2).all(|dz| (0..2).all(|dx| (y..y + h).all(|yy| self.air(IVec3::new(x + dx, yy, z + dz)))))
    }
    fn standable(&self, x: i32, y: i32, z: i32) -> bool {
        y as f32 * 0.5 >= VOID_Y && self.clear(x, y, z, 4) && (0..2).any(|dz| (0..2).any(|dx| !self.air(IVec3::new(x + dx, y - 1, z + dz))))
    }
}

/// Is `to` reachable from `from` (feet voxels) by walking, stepping, jumping ≤ 2 m and falling?
fn reaches(g: &Grid, from: IVec3, to: IVec3) -> bool {
    let mut seen = vec![false; g.solid.len()];
    let mut q = VecDeque::new();
    let push = |seen: &mut Vec<bool>, q: &mut VecDeque<IVec3>, v: IVec3| {
        if let Some(i) = g.idx(v)
            && !seen[i]
        {
            seen[i] = true;
            q.push_back(v);
        }
    };
    push(&mut seen, &mut q, from);
    while let Some(v) = q.pop_front() {
        if v == to {
            return true;
        }
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (v.x + dx, v.z + dz);
            for dy in 1..=4 {
                if !g.clear(v.x, v.y, v.z, dy + 4) {
                    break;
                }
                if g.standable(nx, v.y + dy, nz) {
                    push(&mut seen, &mut q, IVec3::new(nx, v.y + dy, nz));
                }
            }
            if !g.clear(nx, v.y, nz, 4) {
                continue;
            }
            let mut ny = v.y;
            while ny as f32 * 0.5 >= VOID_Y && !g.standable(nx, ny, nz) {
                ny -= 1;
                if !g.clear(nx, ny, nz, 1) {
                    ny = i32::MIN;
                    break;
                }
            }
            if ny != i32::MIN && g.standable(nx, ny, nz) {
                push(&mut seen, &mut q, IVec3::new(nx, ny, nz));
            }
        }
    }
    false
}

fn spawn_node(g: &Grid, s: Vec3) -> Option<IVec3> {
    let v = (s / 0.5).floor().as_ivec3();
    [(0, 0), (-1, 0), (0, -1), (-1, -1)].iter().map(|&(dx, dz)| IVec3::new(v.x + dx, v.y, v.z + dz)).find(|n| g.standable(n.x, n.y, n.z))
}

#[test]
fn deterministic_per_seed() {
    let (a, ia) = generate_giant(3);
    let (b, ib) = generate_giant(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia.spawns, ib.spawns);
    assert_eq!(ia.routes, ib.routes);
    let (c, _) = generate_giant(4);
    assert_ne!(a.content_hash(), c.content_hash(), "different seeds should differ (books, folds, tufts)");
}

#[test]
fn spawns_valid_spread_and_mutually_reachable() {
    for seed in [1u64, 2, 9] {
        let (w, info) = generate_giant(seed);
        assert!((8..=10).contains(&info.spawns.len()), "seed {seed}: {} spawns", info.spawns.len());
        for (i, s) in info.spawns.iter().enumerate() {
            let v = (*s / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), AIR, "seed {seed}: spawn {s} inside a solid");
            assert_eq!(w.get(v + IVec3::Y * 3), AIR, "seed {seed}: spawn {s} without head room");
            assert_ne!(w.get(v - IVec3::Y), AIR, "seed {seed}: spawn {s} floating");
            for (j, t) in info.spawns.iter().enumerate().skip(i + 1) {
                assert!(s.distance(*t) >= 12.0, "seed {seed}: spawns {i} and {j} only {:.1} m apart", s.distance(*t));
            }
        }
        // Spread over heights: floor, low furniture, bed height and desk height all represented.
        let tiers = [(FLOOR_Y - 0.1, FLOOR_Y + 1.1), (9.0, 17.0), (22.0, 26.0), (29.0, 32.0)];
        for (lo, hi) in tiers {
            assert!(info.spawns.iter().any(|s| s.y >= lo && s.y <= hi), "seed {seed}: no spawn at {lo}..{hi} m");
        }
        let g = Grid::new(&w);
        let nodes: Vec<IVec3> =
            info.spawns.iter().map(|s| spawn_node(&g, *s).unwrap_or_else(|| panic!("seed {seed}: spawn {s} not standable"))).collect();
        // Mutual reachability ⇔ every spawn reaches spawn 0 and spawn 0 reaches every spawn.
        for (i, &n) in nodes.iter().enumerate().skip(1) {
            assert!(
                reaches(&g, nodes[0], n),
                "seed {seed}: spawn {i} {:?} not reachable from spawn 0 {:?}",
                info.spawns[i],
                info.spawns[0]
            );
            assert!(
                reaches(&g, n, nodes[0]),
                "seed {seed}: spawn 0 {:?} not reachable from spawn {i} {:?}",
                info.spawns[0],
                info.spawns[i]
            );
        }
    }
}

#[test]
fn void_pits_exactly_where_designed() {
    let (w, _) = generate_giant(1);
    let pits = [giant::VENT, giant::UNDER_BED_PIT, giant::TOY_BOX_PIT];
    let (fy, vy) = ((FLOOR_Y / 0.5) as i32, (VOID_Y / 0.5) as i32);
    let mut void_cols = [0usize; 3];
    for z in (-HZ / 0.5) as i32..(HZ / 0.5) as i32 {
        for x in (-HX / 0.5) as i32..(HX / 0.5) as i32 {
            // A column open from the floor level down past the void line.
            let open = (vy - 1..fy).all(|y| w.get(IVec3::new(x, y, z)) == AIR);
            if !open {
                continue;
            }
            let (mx, mz) = (x as f32 * 0.5 + 0.25, z as f32 * 0.5 + 0.25);
            let k = pits.iter().position(|p| p.contains(mx, mz));
            assert!(k.is_some(), "void hole outside the designed pits at ({mx}, {mz})");
            void_cols[k.unwrap()] += 1;
        }
    }
    for (k, n) in void_cols.iter().enumerate() {
        let a = pits[k];
        let expect = ((a.x1 - a.x0) * (a.z1 - a.z0) / 0.25) as usize;
        assert_eq!(*n, expect, "pit {k} ({a:?}) open columns");
    }
}

#[test]
fn blast_zones_window_and_ceiling() {
    let (w, info) = generate_giant(1);
    let b = w.bounds();
    // The room's walls are the world's extent (side blast zone = world edge + margin).
    assert!((b.max.x - (HX + 4.0)).abs() < 0.01 && (b.min.x + HX + 4.0).abs() < 0.01, "world bounds {b:?}");
    // The open window: clear air from inside the room out past the world's edge.
    for (y, z) in [(40.0f32, -10.0f32), (58.0, 12.0)] {
        for x in [HX - 1.0, HX + 1.5, HX + 3.75] {
            assert_eq!(w.get_at(Vec3::new(x, y, z)), AIR, "window blocked at ({x}, {y}, {z})");
        }
    }
    // Walls elsewhere are indestructible; the ceiling is breakable plaster (punch through → star KO).
    assert!(material_def(w.get_at(Vec3::new(HX + 1.0, 20.0, 30.0))).indestructible);
    let ceil = material_def(w.get_at(Vec3::new(-20.0, giant::CEIL_Y + 0.25, -20.0)));
    assert!(!ceil.indestructible && ceil.hardness <= 0.5, "ceiling {ceil:?}");
    assert!(w.voxel_max().y as f32 * 0.5 <= giant::CEIL_Y + 2.0, "nothing above the ceiling");
    assert!(info.void_y > 0.0 && info.void_y < FLOOR_Y - 3.0);
}

#[test]
fn nothing_floating_and_toys_placed() {
    // A stray voxel or two of a baked toy is fine (pruned); whole pieces coming out detached isn't.
    let o = giant::debug_orphans(1);
    assert!(o.len() <= 16, "{} voxels left floating, e.g. {:?}", o.len(), o.first());
    let (_, info) = generate_giant(1);
    assert_eq!(info.pruned_voxels, o.len());
}

#[test]
fn surf_lines_clear_of_spawns_and_routes_named() {
    let (_, info) = generate_giant(1);
    assert_eq!(info.surf_lines.len(), 7);
    for l in &info.surf_lines {
        for s in &info.spawns {
            // Horizontal distance from the ridge segment.
            let (a, b) = (Vec2::new(l.a.x, l.a.z), Vec2::new(l.b.x, l.b.z));
            let p = Vec2::new(s.x, s.z);
            let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            let d = p.distance(a + (b - a) * t);
            assert!(d > l.width + 1.0, "spawn {s} within {d:.1} m of surf line {l:?}");
        }
    }
    for r in ["track_loop", "desk_ramp", "bookshelf_climb", "curtain_swing", "blanket_surf", "bed_to_desk_swing"] {
        assert!(info.route(r).is_some(), "route {r}");
    }
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_giant(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        total += mesh.triangle_count();
        // Closed shell: no face looks out of the world.
        let wb = w.bounds();
        for tri in mesh.indices.as_chunks::<3>().0 {
            let p = tri.iter().map(|&i| Vec3::from_array(mesh.positions[i as usize])).sum::<Vec3>() / 3.0;
            let out = p + Vec3::from_array(mesh.normals[tri[0] as usize]) * 0.25;
            assert!(
                out.x > wb.min.x && out.x < wb.max.x && out.z > wb.min.z && out.z < wb.max.z && out.y < wb.max.y,
                "face looking out of the world at {p:?}"
            );
        }
    }
    assert!(total <= TRI_BUDGET, "{total} triangles > {TRI_BUDGET}");
}

/// Debug: print the column at `GR_AT=x,z` (metres).
#[test]
#[ignore]
fn debug_column() {
    let at: Vec<f32> = std::env::var("GR_AT").unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect();
    let (w, _) = generate_giant(1);
    let v = (Vec3::new(at[0], 0.0, at[1]) / 0.5).floor().as_ivec3();
    for y in (0..160).rev() {
        let m = w.get(IVec3::new(v.x, y, v.z));
        if m != AIR {
            eprintln!("y {:5.1}: {} ({})", y as f32 * 0.5, m, material_def(m).name);
        }
    }
}
