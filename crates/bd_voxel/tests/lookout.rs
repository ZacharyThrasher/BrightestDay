//! The Lookout checks (feature `lookout`): determinism; the disc stands on an indestructible plate
//! (no void holes) with nothing to stand on under it but Korin's tower and the floating rocks; spawns
//! valid, mirrored, all at floor height round the outside and mutually reachable on foot (walk / fall
//! / ≤ 2 m steps with a 1 m hull — no jumps, never under the KO line); the palace roof and the time
//! chamber reachable on foot, by more than one way; the rim promenade clear all the way round;
//! landmarks (dome, turrets, Korin's roof, the floating stair, the spire) solid; the triangle budget.
#![cfg(feature = "lookout")]

use std::collections::VecDeque;

use bd_core::voxel::AIR;
use bd_voxel::maps::lookout::{self, FLOOR_Y, VOID_Y, generate_lookout};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld, mat, material_def};
use glam::{IVec3, Vec2, Vec3};

const TRI_BUDGET: usize = 1_000_000;

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

/// Standing here: above the KO line, 2 m of head room, something under the footprint.
fn standable(w: &VoxelWorld, x: i32, y: i32, z: i32) -> bool {
    y as f32 * 0.5 >= VOID_Y
        && clear(w, x, y, z, 4)
        && (0..2).any(|dz| (0..2).any(|dx| w.get(IVec3::new(x + dx, y - 1, z + dz)) != AIR))
}

/// Nodes reachable from `start` (feet voxel) by walking, stepping ≤ 2 m and falling (a fall below
/// the KO line is not an edge).
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

fn node(w: &VoxelWorld, s: Vec3) -> Option<IVec3> {
    let v = (s / 0.5).floor().as_ivec3();
    for dy in [0, 1, -1, 2] {
        for (dx, dz) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
            if standable(w, v.x + dx, v.y + dy, v.z + dz) {
                return Some(IVec3::new(v.x + dx, v.y + dy, v.z + dz));
            }
        }
    }
    None
}

fn reaches(w: &VoxelWorld, g: &Grid, seen: &[bool], p: Vec3) -> bool {
    node(w, p).is_some_and(|n| seen[g.idx(n).unwrap()])
}

#[test]
fn deterministic_per_seed() {
    let (a, ia) = generate_lookout(3);
    let (b, ib) = generate_lookout(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia.spawns, ib.spawns);
    assert_eq!(ia.routes, ib.routes);
    let (c, _) = generate_lookout(4);
    assert_ne!(a.content_hash(), c.content_hash(), "different seeds should differ");
}

#[test]
fn the_disc_stands_on_an_indestructible_plate() {
    let (w, info) = generate_lookout(1);
    assert_eq!(info.void_y, VOID_Y);
    assert!(info.pruned_voxels < 400, "{} voxels pruned (a piece came out detached)", info.pruned_voxels);
    let py = (lookout::PLATE_Y / 0.5) as i32 - 1;
    let ty = (FLOOR_Y / 0.5) as i32 - 1;
    let n = (lookout::DISC_R / 0.5) as i32;
    let mut cols = 0;
    for z in -n..n {
        for x in -n..n {
            let p = Vec2::new((x as f32 + 0.5) * 0.5, (z as f32 + 0.5) * 0.5);
            if p.length() > lookout::DISC_R - 1.5 {
                continue;
            }
            cols += 1;
            let m = w.get(IVec3::new(x, py, z));
            assert!(material_def(m).indestructible, "no plate at ({x}, {z}): {}", material_def(m).name);
            // The floor is breakable (craters), and there is floor over every plate column.
            let under_floor = (py + 1..=ty).all(|y| w.get(IVec3::new(x, y, z)) != AIR);
            let top = w.get(IVec3::new(x, ty, z));
            assert!(under_floor || top == AIR, "gap in the disc at ({x}, {z})");
        }
    }
    assert!(cols > 30_000, "{cols} plate columns");
    // Under the disc there is only its underside (curving up to the rim) and the spire.
    for (x, z) in [(0.0f32, 30.0f32), (40.0, 0.0), (-20.0, -40.0)] {
        let v = IVec3::new((x / 0.5).floor() as i32, 0, (z / 0.5).floor() as i32);
        let low = (0..py).find(|&y| w.get(IVec3::new(v.x, y, v.z)) != AIR).map(|y| y as f32 * 0.5).unwrap_or(f32::INFINITY);
        assert!(low >= lookout::underside_y((x * x + z * z).sqrt()) - 1.0, "something under the disc at ({x}, {z}): {low}");
    }
    // The spire tip hangs below the KO line.
    assert_ne!(w.get_at(Vec3::new(0.0, lookout::SPIRE_TIP + 1.0, 0.0)), AIR, "spire tip");
    assert!(lookout::SPIRE_TIP < VOID_Y);
}

#[test]
fn spawns_valid_fair_and_mutually_reachable() {
    let seeds: Vec<u64> =
        match std::env::var("LOOKOUT_SEEDS").ok().and_then(|s| s.split_once("..").map(|(a, b)| (a.parse().ok(), b.parse().ok()))) {
            Some((Some(a), Some(b))) => (a..b).collect(),
            _ => vec![1, 2, 7],
        };
    for seed in seeds {
        let (w, info) = generate_lookout(seed);
        assert_eq!(info.spawns.len(), 10, "seed {seed}: {} spawns", info.spawns.len());
        for (i, s) in info.spawns.iter().enumerate() {
            let v = (*s / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), AIR, "seed {seed}: spawn {s} inside a solid");
            assert_eq!(w.get(v + IVec3::Y * 3), AIR, "seed {seed}: spawn {s} without head room");
            let under = w.get(v - IVec3::Y);
            assert!(under != AIR && !material_def(under).indestructible, "seed {seed}: spawn {s} not on the floor");
            assert!((s.y - FLOOR_Y).abs() < 0.01, "seed {seed}: spawn {s} not at floor height");
            let r = Vec2::new(s.x, s.z).length();
            assert!((36.0..=50.0).contains(&r), "seed {seed}: spawn {s} at r {r:.1} (want the outer ring, off the rim)");
            for t in &info.spawns[i + 1..] {
                assert!(s.distance(*t) >= 12.0, "seed {seed}: spawns {s} and {t} too close");
            }
            // Mirrored east/west.
            assert!(
                info.spawns.iter().any(|t| (t.x + s.x).abs() < 2.0 && (t.z - s.z).abs() < 2.0),
                "seed {seed}: spawn {s} has no mirror"
            );
        }
        let g = Grid::new(&w);
        let nodes: Vec<IVec3> =
            info.spawns.iter().map(|s| node(&w, *s).unwrap_or_else(|| panic!("seed {seed}: spawn {s} not standable"))).collect();
        let seen = reach(&w, &g, nodes[0]);
        for (j, &to) in nodes.iter().enumerate() {
            assert!(seen[g.idx(to).unwrap()], "seed {seed}: spawn {j} {} not reachable on foot from spawn 0", info.spawns[j]);
        }
        // The walk graph is symmetric on a flat disc, but check one more origin to be sure.
        let seen = reach(&w, &g, nodes[5]);
        assert!(seen[g.idx(nodes[0]).unwrap()], "seed {seed}: spawn 0 not reachable from spawn 5");
    }
}

/// On foot: the hall, both wing roofs and the hall roof (outside stairs), the time chamber and its
/// roof; and more than one way into the hall (the south doors blocked, the north / wing doors still
/// lead in).
#[test]
fn palace_and_chamber_reachable_on_foot() {
    let (mut w, info) = generate_lookout(1);
    let g = Grid::new(&w);
    let start = node(&w, info.spawns[0]).unwrap();
    let seen = reach(&w, &g, start);
    let hall = Vec3::new(0.0, lookout::TERRACE_Y, 21.0);
    let targets = [
        ("hall", hall),
        ("east wing roof", Vec3::new(20.0, lookout::WING_ROOF, 25.0)),
        ("west wing roof", Vec3::new(-20.0, lookout::WING_ROOF, 25.0)),
        ("hall roof", Vec3::new(9.0, lookout::HALL_ROOF, 21.0)),
        ("chamber", Vec3::new(lookout::CHAMBER_C.x, lookout::CHAMBER_FLOOR, lookout::CHAMBER_C.y)),
        ("chamber roof", Vec3::new(lookout::CHAMBER_C.x, lookout::CHAMBER_ROOF, lookout::CHAMBER[3] - 1.5)),
        ("east wing", Vec3::new(18.0, lookout::TERRACE_Y, 21.0)),
    ];
    for (name, p) in targets {
        assert!(reaches(&w, &g, &seen, p), "{name} ({p}) not reachable on foot");
    }
    // Block the three south doorways: the hall is still reachable (north door, wings).
    for z in (lookout::HALL[1] / 0.5) as i32 - 1..=(lookout::HALL[1] / 0.5) as i32 + 2 {
        for x in -40..40 {
            for y in (lookout::TERRACE_Y / 0.5) as i32..(lookout::TERRACE_Y / 0.5) as i32 + 14 {
                w.set(IVec3::new(x, y, z), mat::LK_WALL);
            }
        }
    }
    let seen = reach(&w, &g, start);
    assert!(reaches(&w, &g, &seen, hall), "with the south doors shut there's no other way in");
}

#[test]
fn promenade_is_clear_all_the_way_round() {
    let (w, _) = generate_lookout(1);
    let ty = (FLOOR_Y / 0.5) as i32 - 1;
    for k in 0..720 {
        let a = std::f32::consts::TAU * k as f32 / 720.0;
        for r in [49.0f32, 51.0, 53.0] {
            let v = IVec3::new((a.cos() * r / 0.5).floor() as i32, ty, (a.sin() * r / 0.5).floor() as i32);
            assert_ne!(w.get(v), AIR, "promenade floor missing at r {r} angle {:.1}°", a.to_degrees());
            for dy in 1..=5 {
                assert_eq!(w.get(v + IVec3::Y * dy), AIR, "promenade blocked at r {r} angle {:.1}°", a.to_degrees());
            }
        }
    }
}

#[test]
fn landmarks_are_solid() {
    let (w, info) = generate_lookout(1);
    let d = lookout::DRUM;
    assert_ne!(w.get_at(Vec3::new(d.x, lookout::DOME_TOP - 0.25, d.y)), AIR, "dome top");
    assert_ne!(w.get_at(Vec3::new(d.x, lookout::FINIAL_TOP - 1.0, d.y)), AIR, "finial");
    assert_eq!(w.get_at(Vec3::new(d.x, lookout::HALL_ROOF + 3.0, d.y)), AIR, "the drum is hollow (atrium)");
    for (x, z) in [(-13.0f32, 12.0f32), (13.0, 12.0), (-13.0, 30.0), (13.0, 30.0)] {
        assert_ne!(w.get_at(Vec3::new(x, lookout::TURRET_TOP - 1.0, z)), AIR, "turret at ({x}, {z})");
        assert!(material_def(w.get_at(Vec3::new(x, 76.0, z))).indestructible, "turret core at ({x}, {z})");
    }
    let k = lookout::KORIN;
    assert_ne!(w.get_at(Vec3::new(k.x, lookout::KORIN_TOP - 0.25, k.y + 4.0)), AIR, "Korin's roof");
    assert_eq!(w.get_at(Vec3::new(k.x, lookout::KORIN_TOP + 0.25, k.y + 4.0)), AIR, "Korin's roof blocked");
    for (at, top, _) in lookout::KORIN_STAIR {
        assert_ne!(w.get_at(Vec3::new(at.x, top - 0.25, at.y)), AIR, "stair rock at {at}");
        assert_eq!(w.get_at(Vec3::new(at.x, top + 0.25, at.y)), AIR, "stair rock at {at} blocked");
    }
    for (p, _) in lookout::rocks() {
        assert_eq!(w.get_at(p - Vec3::Y * 0.25), mat::GRASS, "floating rock top at {p}");
    }
    // Routes start on something.
    for r in &info.routes {
        let p = r.waypoints[0];
        assert!((0..4).any(|k| w.get_at(p - Vec3::Y * (0.25 + 0.5 * k as f32)) != AIR), "route {} starts in the air at {p}", r.name);
    }
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_lookout(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        total += mesh.triangle_count();
    }
    eprintln!("lookout: {total} triangles");
    assert!(total <= TRI_BUDGET, "{total} triangles > {TRI_BUDGET}");
}

#[test]
#[ignore]
fn debug_column() {
    let (w, _) = generate_lookout(1);
    let at: Vec<f32> = std::env::var("LK_AT").unwrap_or("0,21".into()).split(',').filter_map(|s| s.parse().ok()).collect();
    for y in (100..210).rev() {
        let row: String = (-12..=12)
            .map(|dx| {
                let v = IVec3::new((at[0] / 0.5).floor() as i32 + dx, y, (at[1] / 0.5).floor() as i32);
                let m = w.get(v);
                if m == AIR { '.' } else { char::from(b'A' + (m % 26)) }
            })
            .collect();
        eprintln!("{:5.1} {row}", y as f32 * 0.5);
    }
}
