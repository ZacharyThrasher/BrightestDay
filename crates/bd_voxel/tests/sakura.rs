//! Sakura Keep checks (feature `sakura`): determinism, spawn validity + mutual reachability (walk /
//! fall / ≤ 2 m jumps with a 1 m hull), spawns off the chokepoints, the void edges and the ravine,
//! routes clear, the keep hall tall enough to swing in, and the triangle budget.
#![cfg(feature = "sakura")]

use std::collections::VecDeque;

use bd_core::voxel::AIR;
use bd_voxel::maps::sakura::{self, VOID_Y, generate_sakura, in_playable, layout, px_to_m};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld, material_def};
use glam::{IVec3, Vec2, Vec3};

/// Whole-map triangle budget (air side of each face).
const TRI_BUDGET_TOTAL: usize = 950_000;

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

/// 2 × 2 voxel footprint (1 m) with `h` voxels of air from `y`.
fn clear(w: &VoxelWorld, x: i32, y: i32, z: i32, h: i32) -> bool {
    (0..2).all(|dz| (0..2).all(|dx| (y..y + h).all(|yy| w.get(IVec3::new(x + dx, yy, z + dz)) == AIR)))
}

fn standable(w: &VoxelWorld, x: i32, y: i32, z: i32) -> bool {
    y as f32 * 0.5 >= VOID_Y && clear(w, x, y, z, 4) && (0..2).any(|dz| (0..2).any(|dx| w.get(IVec3::new(x + dx, y - 1, z + dz)) != AIR))
}

/// Nodes reachable from `start` by walking, stepping ≤ 0.5 m, jumping ≤ 2 m and falling.
fn reach(w: &VoxelWorld, g: &Grid, start: IVec3) -> Vec<bool> {
    let mut seen = vec![false; (g.size.x * g.size.y * g.size.z) as usize];
    let mut q = VecDeque::new();
    if let Some(i) = g.idx(start) {
        seen[i] = true;
        q.push_back(start);
    }
    let floor_y = w.voxel_min().y;
    while let Some(v) = q.pop_front() {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (v.x + dx, v.z + dz);
            for dy in 1..=4 {
                let ny = v.y + dy;
                if !clear(w, v.x, v.y, v.z, dy + 4) {
                    break;
                }
                if standable(w, nx, ny, nz) {
                    push(g, &mut seen, &mut q, IVec3::new(nx, ny, nz));
                }
            }
            if !clear(w, nx, v.y, nz, 4) {
                continue;
            }
            let mut ny = v.y;
            while ny > floor_y && !standable(w, nx, ny, nz) {
                ny -= 1;
                if !clear(w, nx, ny, nz, 1) {
                    ny = i32::MIN;
                    break;
                }
            }
            if ny != i32::MIN && ny > floor_y && standable(w, nx, ny, nz) {
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
    let (a, ia) = generate_sakura(3);
    let (b, ib) = generate_sakura(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia.spawns, ib.spawns);
    assert_eq!(ia.routes, ib.routes);
    assert_eq!(ia.surf_lines, ib.surf_lines);
    let (c, _) = generate_sakura(4);
    assert_ne!(a.content_hash(), c.content_hash(), "different seeds should differ (trees, koi, roofline)");
}

#[test]
fn spawns_valid_spread_and_mutually_reachable() {
    for seed in [1u64, 2, 7] {
        let (w, info) = generate_sakura(seed);
        assert!((8..=10).contains(&info.spawns.len()), "seed {seed}: {} spawns", info.spawns.len());
        for (i, s) in info.spawns.iter().enumerate() {
            let v = (*s / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), AIR, "seed {seed}: spawn {i} {s} inside a solid");
            assert_eq!(w.get(v + IVec3::Y * 3), AIR, "seed {seed}: spawn {i} {s} without head room");
            assert_ne!(w.get(v - IVec3::Y), AIR, "seed {seed}: spawn {i} {s} floating");
            assert!(!material_def(w.get(v - IVec3::Y)).indestructible, "seed {seed}: spawn {i} on indestructible ground");
            assert!(in_playable(Vec2::new(s.x, s.z)), "seed {seed}: spawn {i} {s} outside the playable outline");
            for (j, t) in info.spawns.iter().enumerate().skip(i + 1) {
                assert!(s.distance(*t) >= 12.0, "seed {seed}: spawns {i} and {j} only {:.1} m apart", s.distance(*t));
            }
            // Never in a chokepoint: no stair / gate steps under a spawn, and a clear 3 m around it.
            for st in layout::STAIRS {
                let p = sakura::Poly::from_px(st.poly);
                assert!(!p.contains(Vec2::new(s.x, s.z)), "seed {seed}: spawn {i} on the {} (a chokepoint)", st.name);
            }
            let mut open = 0;
            for k in 0..16 {
                let a = k as f32 * std::f32::consts::TAU / 16.0;
                let p = *s + Vec3::new(a.cos() * 3.0, 0.6, a.sin() * 3.0);
                if w.get((p / 0.5).floor().as_ivec3()) == AIR {
                    open += 1;
                }
            }
            assert!(open >= 12, "seed {seed}: spawn {i} {s} is boxed in ({open}/16 directions open at 3 m)");
        }
        let g = Grid::new(&w);
        let nodes: Vec<IVec3> =
            info.spawns.iter().map(|s| spawn_node(&w, *s).unwrap_or_else(|| panic!("seed {seed}: spawn {s} not standable"))).collect();
        let mut matrix = Vec::new();
        let mut ok = true;
        for &from in &nodes {
            let seen = reach(&w, &g, from);
            let row: String = nodes.iter().map(|&to| if seen[g.idx(to).unwrap()] { '1' } else { '.' }).collect();
            ok &= !row.contains('.');
            matrix.push(row);
        }
        assert!(ok, "seed {seed}: spawns not mutually reachable (row = from):\n{}\nspawns: {:?}", matrix.join("\n"), info.spawns);
    }
}

#[test]
fn void_edges_ravine_and_blast_zones() {
    let (w, info) = generate_sakura(1);
    // Blast zones: sides 24 m past the world, top 40 m over it, void below the plateau.
    let wb = w.bounds();
    assert!(info.bounds.max.y >= wb.max.y + 39.0);
    assert!(info.void_y < 0.0 && info.bounds.min.y == info.void_y);
    // The ravine between the A district and the garden is a hole to the void.
    let rav = px_to_m((362.0, 440.0));
    let (x, z) = ((rav.x / 0.5).floor() as i32, (rav.y / 0.5).floor() as i32);
    assert!(w.top_solid_y(x, z).is_none(), "the ravine at {rav} has a floor");
    // Beyond the plateau: nothing (the side of the map is the void).
    for p in [Vec2::new(-100.0, 0.0), Vec2::new(100.0, 0.0), Vec2::new(0.0, -100.0), Vec2::new(0.0, 100.0)] {
        let (x, z) = ((p.x / 0.5).floor() as i32, (p.y / 0.5).floor() as i32);
        assert!(w.top_solid_y(x, z).is_none(), "solid at the map edge {p}");
    }
    // Castle base core and bedrock are indestructible; the town is not.
    let keep = sakura::CastleFrame::new().world(Vec2::ZERO);
    let (kx, kz) = ((keep.x / 0.5).floor() as i32, (keep.y / 0.5).floor() as i32);
    assert!(material_def(w.get(IVec3::new(kx, 0, kz))).indestructible, "no bedrock under the keep");
    let plaza = px_to_m((280.0, 527.0));
    let top = w.top_solid_y((plaza.x / 0.5) as i32, (plaza.y / 0.5) as i32).expect("plaza block");
    assert!(!material_def(w.get(IVec3::new((plaza.x / 0.5) as i32, top, (plaza.y / 0.5) as i32))).indestructible);
}

#[test]
fn routes_clear_hall_tall_surf_lines_clear() {
    let (w, info) = generate_sakura(1);
    let street = info.route("street_bhop").expect("street bhop route");
    let (a, b) = (street.waypoints[0], street.waypoints[1]);
    assert!(a.distance(b) > 50.0, "bhop lane only {:.0} m", a.distance(b));
    for k in 0..=200 {
        let p = a.lerp(b, k as f32 / 200.0);
        for dz in [-1.0f32, 0.0, 1.0] {
            let v = ((p + Vec3::new(0.0, 0.1, dz)) / 0.5).floor().as_ivec3();
            for dy in 0..6 {
                assert_eq!(w.get(v + IVec3::Y * dy), AIR, "street lane blocked at {:?}", p + Vec3::new(0.0, dy as f32 * 0.5, dz));
            }
            assert_ne!(w.get(v - IVec3::Y), AIR, "street lane has a hole at {p}");
        }
    }
    let swing = info.route("keep_swing").expect("keep swing");
    let anchor = swing.anchors[0];
    let up = bd_core::voxel::raycast(&w, anchor - Vec3::Y * 6.0, Vec3::Y, 30.0).expect("keep hall has a ceiling");
    assert!(up.point.y - swing.waypoints[0].y > 16.0, "keep hall only {:.1} m tall", up.point.y - swing.waypoints[0].y);
    assert_eq!(info.surf_lines.len(), 2);
    for (k, l) in info.surf_lines.iter().enumerate() {
        assert!(l.a.y - l.b.y > 5.0, "surf line {k} barely descends");
        // The ridge line itself is clear past the run-off start.
        let len = (l.b - l.a).length();
        for s in 0..=100 {
            let t = s as f32 / 100.0;
            if t * len < len * 0.25 {
                continue;
            }
            let p = l.a.lerp(l.b, t);
            assert_eq!(w.get((p / 0.5).floor().as_ivec3()), AIR, "surf line {k} buried at {p}");
        }
    }
    // Spawns are clear of the surf faces.
    for s in &info.spawns {
        for l in &info.surf_lines {
            let (a, b) = (l.a, l.b);
            let ab = b - a;
            let t = ((*s - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            assert!((a + ab * t).distance(*s) > 6.0, "spawn {s} under a surf line");
        }
    }
    assert!(w.has_enclosed_air(), "no sealed interiors (blocks / upper storeys)");
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_sakura(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        total += mesh.triangle_count();
    }
    assert!(total <= TRI_BUDGET_TOTAL, "{total} triangles > {TRI_BUDGET_TOTAL}");
}

/// Debug: ASCII slice around `SK_AT=x,y,z` (metres).
#[test]
#[ignore]
fn debug_slice() {
    let at: Vec<f32> = std::env::var("SK_AT").unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect();
    let (w, _) = generate_sakura(1);
    let v = (Vec3::new(at[0], at[1], at[2]) / 0.5).floor().as_ivec3();
    for dy in [0, 1, 4, 9] {
        eprintln!("y = {} (+{dy})", v.y + dy);
        for dz in (-20..=20).rev() {
            let row: String = (-30..=30)
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
