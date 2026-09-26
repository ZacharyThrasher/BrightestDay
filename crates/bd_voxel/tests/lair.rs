//! Turtle Lair map checks (feature `lair`): determinism, spawn validity + mutual reachability
//! (walk / fall / ≤ 2 m jumps with a 1 m hull — no tethers), void only through the chasm and the
//! block edges, routes clear, surf lines sane, props capped and clear of spawns, triangle budget.
#![cfg(feature = "lair")]

use std::collections::VecDeque;

use bd_core::voxel::AIR;
use bd_voxel::maps::lair::{self, CHASM, EDGE, FLOOR, STREET, VOID_Y, generate_lair, props_for};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld, material_def};
use glam::{IVec3, Vec3};

/// Whole-map triangle budget (DESIGN.md perf gates: ≤ ~1.0 M).
const TRI_BUDGET_TOTAL: usize = 1_000_000;

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
    let (a, ia) = generate_lair(3);
    let (b, ib) = generate_lair(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia.spawns, ib.spawns);
    assert_eq!(ia.routes, ib.routes);
    assert_eq!(ia.surf_lines, ib.surf_lines);
    let (c, _) = generate_lair(4);
    assert_ne!(a.content_hash(), c.content_hash(), "seeds should vary the details");
}

#[test]
fn spawns_valid_spread_and_mutually_reachable() {
    for seed in [1u64, 2] {
        let (w, info) = generate_lair(seed);
        assert!((8..=10).contains(&info.spawns.len()), "seed {seed}: {} spawns", info.spawns.len());
        let (mut under, mut surface) = (0, 0);
        for s in &info.spawns {
            let v = (*s / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), AIR, "seed {seed}: spawn {s} inside a solid");
            assert_eq!(w.get(v + IVec3::Y * 3), AIR, "seed {seed}: spawn {s} without head room");
            assert_ne!(w.get(v - IVec3::Y), AIR, "seed {seed}: spawn {s} floating");
            if s.y < STREET - 2.0 { under += 1 } else { surface += 1 }
        }
        assert!(under >= 6 && surface >= 2, "seed {seed}: {under} underground / {surface} surface spawns");
        for (i, a) in info.spawns.iter().enumerate() {
            for b in &info.spawns[i + 1..] {
                assert!(a.distance(*b) > 12.0, "seed {seed}: spawns {a} and {b} too close");
            }
        }
        let g = Grid::new(&w);
        let nodes: Vec<IVec3> =
            info.spawns.iter().map(|s| spawn_node(&w, *s).unwrap_or_else(|| panic!("seed {seed}: spawn {s} not standable"))).collect();
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
fn void_only_through_the_chasm_and_edges() {
    let (w, _) = generate_lair(1);
    let lo = (-EDGE / 0.5) as i32;
    let hi = (EDGE / 0.5) as i32 - 1;
    for z in lo..=hi {
        for x in lo..=hi {
            let mx = (x as f32 + 0.5) * 0.5;
            let bed = w.get(IVec3::new(x, 0, z));
            if mx >= CHASM.0 && mx < CHASM.1 {
                assert_eq!(bed, AIR, "chasm column ({x}, {z}) has a floor");
            } else {
                assert!(material_def(bed).indestructible, "column ({x}, {z}) has no indestructible bedrock");
            }
        }
    }
    // The sewer bridges span the chasm at walk level.
    for z in [lair::T_N_Z, 3.5, lair::T_S_Z] {
        let v = (Vec3::new((CHASM.0 + CHASM.1) * 0.5, FLOOR - 0.25, z) / 0.5).floor().as_ivec3();
        let _ = STREET;
        assert_ne!(w.get(v), AIR, "no bridge deck at z = {z}");
    }
}

#[test]
fn routes_clear_and_surf_lines_sane() {
    let (w, info) = generate_lair(1);
    for name in ["west_tunnel_bhop", "platform_bhop"] {
        let r = info.route(name).unwrap_or_else(|| panic!("route {name}"));
        let (a, b) = (r.waypoints[0], r.waypoints[1]);
        assert!(a.distance(b) > 60.0, "{name}: only {:.0} m", a.distance(b));
        for k in 0..=200 {
            let p = a.lerp(b, k as f32 / 200.0);
            for d in [-1.0f32, 0.0, 1.0] {
                let q = if (b - a).x.abs() > (b - a).z.abs() { p + Vec3::new(0.0, 0.1, d) } else { p + Vec3::new(d, 0.1, 0.0) };
                let v = (q / 0.5).floor().as_ivec3();
                for dy in 0..5 {
                    assert_eq!(w.get(v + IVec3::Y * dy), AIR, "{name} blocked at {:?}", q + Vec3::Y * dy as f32 * 0.5);
                }
                assert_ne!(w.get(v - IVec3::Y), AIR, "{name}: no floor at {q:?}");
            }
        }
    }
    let swing = info.route("lair_swing").expect("lair swing");
    assert_ne!(w.get_at(swing.anchors[0] + Vec3::Y * 0.6), AIR, "swing anchor isn't solid vault");
    assert!(swing.anchors[0].y - swing.waypoints[0].y > 14.0, "vault too low to swing");
    let climb = info.route("manhole_climb").expect("manhole climb");
    for a in &climb.anchors {
        let solid = (0..6).any(|k| w.get_at(*a + Vec3::new(0.0, 0.0, 0.0) + Vec3::new(k as f32 * 0.1 - 0.25, 0.0, 0.0)) != AIR);
        assert!(solid, "climb anchor {a} not on anything");
    }
    // Surf lines: 3, all inside the bounds; the chute descends into the skate hall.
    assert_eq!(info.surf_lines.len(), 3);
    for l in &info.surf_lines {
        for p in [l.a, l.b] {
            assert!(p.x.abs() <= EDGE && p.z.abs() <= EDGE && p.y > VOID_Y, "surf line end {p} out of the map");
        }
    }
    let chute = info.surf_lines[2];
    assert!(chute.a.y - chute.b.y > 15.0, "chute too shallow");
    // The air above the chute's ridge is open all the way down (a rider's hull fits).
    for k in 0..=60 {
        let p = chute.a.lerp(chute.b, k as f32 / 60.0);
        for dy in [0.6f32, 1.2, 2.0] {
            assert_eq!(w.get_at(p + Vec3::Y * dy), AIR, "chute blocked above the ridge at {:?}", p + Vec3::Y * dy);
        }
    }
    // Half-pipe decks behind the ridges, flat trough between them.
    for l in &info.surf_lines[..2] {
        let mid = (l.a + l.b) * 0.5;
        let behind = if mid.z > -20.0 { mid + Vec3::new(0.0, -0.25, 1.0) } else { mid + Vec3::new(0.0, -0.25, -1.0) };
        assert_ne!(w.get_at(behind), AIR, "no deck behind the half-pipe wall at {mid}");
    }
    assert_eq!(w.get_at(Vec3::new(0.0, FLOOR + 0.25, -21.0)), AIR, "half-pipe trough not clear");
}

#[test]
fn props_capped_and_clear_of_spawns() {
    let (_, info) = generate_lair(1);
    let placed = props_for(1);
    assert!(placed.len() >= 30, "only {} props placed", placed.len());
    assert!(placed.len() <= lair::props::MAX_PROPS);
    for p in &placed {
        for s in &info.spawns {
            let c = (p.min + p.max) * 0.5;
            let half = (p.max - p.min) * 0.5;
            let d = glam::Vec2::new((s.x - c.x).abs() - half.x, (s.z - c.z).abs() - half.z).max(glam::Vec2::ZERO).length();
            let near_y = s.y > p.min.y - 3.0 && s.y < p.max.y + 1.0;
            assert!(!(near_y && d < lair::props::SPAWN_CLEAR - 0.01), "{} too close to spawn {s}", p.name);
        }
    }
    for name in ["bean bag", "weapon rack", "arcade cabinet", "scrap contraption"] {
        assert!(placed.iter().any(|p| p.name == name), "no {name}");
    }
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_lair(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        total += mesh.triangle_count();
    }
    eprintln!("lair: {total} triangles");
    assert!(total <= TRI_BUDGET_TOTAL, "{total} triangles > {TRI_BUDGET_TOTAL}");
}

/// Debug: ASCII slice around `LAIR_AT=x,y,z` (metres).
#[test]
#[ignore]
fn debug_slice() {
    let at: Vec<f32> = std::env::var("LAIR_AT").unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect();
    let (w, _) = generate_lair(1);
    let v = (Vec3::new(at[0], at[1], at[2]) / 0.5).floor().as_ivec3();
    for dy in [-1, 0, 1, 4] {
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

/// Debug: which probe points are reachable from spawn 0.
#[test]
#[ignore]
fn debug_reach() {
    let (w, info) = generate_lair(1);
    let g = Grid::new(&w);
    let from = spawn_node(&w, info.spawns[0]).unwrap();
    let seen = reach(&w, &g, from);
    let probes = [
        ("T_N junction floor", Vec3::new(-44.0, 12.0, 31.0)),
        ("west street", Vec3::new(-42.0, 34.0, 20.0)),
        ("west street s", Vec3::new(-42.0, 34.0, -30.0)),
        ("avenue", Vec3::new(-20.0, 34.0, 5.0)),
        ("avenue sidewalk n", Vec3::new(-25.0, 34.5, 13.0)),
        ("east strip", Vec3::new(50.0, 34.0, 5.0)),
        ("platform", Vec3::new(50.0, 12.0, 0.0)),
        ("well L1", Vec3::new(50.0, 17.5, -9.0)),
        ("well L2", Vec3::new(50.0, 23.0, -23.0)),
        ("well L4", Vec3::new(50.0, 34.0, -23.0)),
        ("N1 fe landing 1", Vec3::new(-24.0, 40.5, 15.0)),
        ("N1 roof", Vec3::new(-26.0, 46.0, 30.0)),
    ];
    for (name, p) in probes {
        let v = (p / 0.5).floor().as_ivec3();
        let hit = (-3..=3).any(|dy| (-1..=1).any(|dx| (-1..=1).any(|dz| g.idx(v + IVec3::new(dx, dy, dz)).is_some_and(|i| seen[i]))));
        eprintln!("{name:>20}: {}", if hit { "reached" } else { "--" });
    }
}
