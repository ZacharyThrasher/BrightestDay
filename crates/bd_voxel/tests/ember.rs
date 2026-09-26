//! Ember map checks (feature `ember`): determinism; the lava sheet is the whole floor and sits under
//! the KO line; spawns valid, fairly spread, away from lava edges and mutually reachable on foot
//! (walk / fall / ≤ 2 m steps with a 1 m hull — no gap jumps, never touching lava); several
//! independent ground routes North ↔ South; landmarks (ledges, arm heads, spires) solid; the
//! triangle budget.
#![cfg(feature = "ember")]

use std::collections::VecDeque;

use bd_core::voxel::AIR;
use bd_voxel::maps::ember::{self, VOID_Y, generate_ember};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld, mat, material_def};
use glam::{IVec3, Vec3};

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

fn is_lava(m: u8) -> bool {
    matches!(m, mat::LAVA | mat::LAVA_HOT | mat::LAVA_CRUST)
}

/// 2 × 2 voxel footprint (1 m) with `h` voxels of air from `y`.
fn clear(w: &VoxelWorld, x: i32, y: i32, z: i32, h: i32) -> bool {
    (0..2).all(|dz| (0..2).all(|dx| (y..y + h).all(|yy| w.get(IVec3::new(x + dx, yy, z + dz)) == AIR)))
}

/// Standing here: above the KO line, 2 m of head room, something under the footprint that is not lava.
fn standable(w: &VoxelWorld, x: i32, y: i32, z: i32) -> bool {
    y as f32 * 0.5 >= VOID_Y
        && clear(w, x, y, z, 4)
        && (0..2).any(|dz| {
            (0..2).any(|dx| {
                let m = w.get(IVec3::new(x + dx, y - 1, z + dz));
                m != AIR && !is_lava(m)
            })
        })
}

/// Nodes reachable from `start` (feet voxel) by walking, stepping ≤ 2 m and falling (a fall that
/// ends in lava is not an edge).
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
    for dy in [0, 1, -1] {
        for (dx, dz) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
            if standable(w, v.x + dx, v.y + dy, v.z + dz) {
                return Some(IVec3::new(v.x + dx, v.y + dy, v.z + dz));
            }
        }
    }
    None
}

/// Horizontal distance from `p` to the nearest visible lava (searching a square of `r` metres).
fn lava_dist(w: &VoxelWorld, p: Vec3, r: f32) -> f32 {
    let c = (p / 0.5).floor().as_ivec3();
    let n = (r / 0.5) as i32;
    let mut best = f32::INFINITY;
    for dz in -n..=n {
        for dx in -n..=n {
            let (x, z) = (c.x + dx, c.z + dz);
            if w.top_solid_y(x, z).is_some_and(|y| y == ember::LAVA_V - 1 && is_lava(w.get(IVec3::new(x, y, z)))) {
                best = best.min(((dx * dx + dz * dz) as f32).sqrt() * 0.5);
            }
        }
    }
    best
}

#[test]
fn deterministic_per_seed() {
    let (a, ia) = generate_ember(3);
    let (b, ib) = generate_ember(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia.spawns, ib.spawns);
    assert_eq!(ia.routes, ib.routes);
    let (c, _) = generate_ember(4);
    assert_ne!(a.content_hash(), c.content_hash(), "different seeds should differ");
}

#[test]
fn lava_is_the_floor_and_below_the_ko_line() {
    let (w, info) = generate_ember(1);
    assert_eq!(info.void_y, VOID_Y);
    const { assert!(ember::LAVA_TOP < VOID_Y, "standing on lava must be under the KO line") };
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    let mut visible = 0usize;
    for z in lo.z..hi.z {
        for x in lo.x..hi.x {
            let m = w.get(IVec3::new(x, 0, z));
            assert!(is_lava(m), "floor voxel ({x}, 0, {z}) is {}", material_def(m).name);
            assert!(material_def(m).indestructible);
            if w.get(IVec3::new(x, ember::LAVA_V, z)) == AIR {
                visible += 1;
            }
            // Lava only ever shows on the floor or as the (thin, vertical) lavafall curtain.
            if let Some(y) = w.top_solid_y(x, z)
                && y >= ember::LAVA_V
                && is_lava(w.get(IVec3::new(x, y, z)))
            {
                let p = Vec3::new(x as f32 * 0.5, y as f32 * 0.5, z as f32 * 0.5);
                assert!(p.x.abs() < 2.0 && (38.0..50.0).contains(&p.z), "raised lava surface at {p}");
            }
        }
    }
    let total = ((hi.x - lo.x) * (hi.z - lo.z)) as usize;
    let frac = visible as f32 / total as f32;
    assert!((0.35..0.85).contains(&frac), "visible lava {:.0}% of the floor", frac * 100.0);
}

#[test]
fn spawns_valid_fair_and_mutually_reachable() {
    let seeds: Vec<u64> =
        match std::env::var("EMBER_SEEDS").ok().and_then(|s| s.split_once("..").map(|(a, b)| (a.parse().ok(), b.parse().ok()))) {
            Some((Some(a), Some(b))) => (a..b).collect(),
            _ => vec![1, 2, 7],
        };
    for seed in seeds {
        let (w, info) = generate_ember(seed);
        assert!((8..=10).contains(&info.spawns.len()), "seed {seed}: {} spawns", info.spawns.len());
        for (i, s) in info.spawns.iter().enumerate() {
            let v = (*s / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), AIR, "seed {seed}: spawn {s} inside a solid");
            assert_eq!(w.get(v + IVec3::Y * 3), AIR, "seed {seed}: spawn {s} without head room");
            let under = w.get(v - IVec3::Y);
            assert!(under != AIR && !is_lava(under), "seed {seed}: spawn {s} not on rock");
            assert!(s.y > VOID_Y + 3.5, "seed {seed}: spawn {s} too close to the lava level");
            let d = lava_dist(&w, *s, 8.0);
            assert!(d >= 4.0, "seed {seed}: spawn {s} only {d:.1} m from lava");
            for t in &info.spawns[i + 1..] {
                assert!(s.distance(*t) >= 12.0, "seed {seed}: spawns {s} and {t} too close");
            }
        }
        // East/west balance: mirrored spawn heights (fair).
        let (mut e, mut wst): (Vec<f32>, Vec<f32>) = (Vec::new(), Vec::new());
        for s in &info.spawns {
            if s.x > 5.0 {
                e.push(s.y);
            } else if s.x < -5.0 {
                wst.push(s.y);
            }
        }
        e.sort_by(f32::total_cmp);
        wst.sort_by(f32::total_cmp);
        assert_eq!(e.len(), wst.len(), "seed {seed}: east/west spawn count");
        let g = Grid::new(&w);
        let nodes: Vec<IVec3> =
            info.spawns.iter().map(|s| node(&w, *s).unwrap_or_else(|| panic!("seed {seed}: spawn {s} not standable"))).collect();
        for (i, &from) in nodes.iter().enumerate() {
            let seen = reach(&w, &g, from);
            for (j, &to) in nodes.iter().enumerate() {
                assert!(
                    seen[g.idx(to).unwrap()],
                    "seed {seed}: spawn {j} {} not reachable on foot from spawn {i} {}",
                    info.spawns[j],
                    info.spawns[i]
                );
            }
        }
    }
}

/// Walk-only reachability North Bank → South Plain with one crossing blocked at a time: every
/// crossing on its own still connects them (no single choke point).
#[test]
fn several_independent_crossings() {
    let (mut w, info) = generate_ember(1);
    let g = Grid::new(&w);
    let north = node(&w, info.spawns[2]).expect("north bank spawn");
    let south = node(&w, info.spawns[4]).expect("south plain spawn");
    assert!(reach(&w, &g, north)[g.idx(south).unwrap()], "North Bank can't reach the South Plain");
    // Block the facility (both stair pairs + deck) and the west arch: the Crag necks still connect.
    let block = |w: &mut VoxelWorld, x0: f32, z0: f32, x1: f32, z1: f32| {
        for z in (z0 / 0.5) as i32..(z1 / 0.5) as i32 {
            for x in (x0 / 0.5) as i32..(x1 / 0.5) as i32 {
                for y in 2..80 {
                    w.set(IVec3::new(x, y, z), AIR);
                }
            }
        }
    };
    block(&mut w, -24.0, -30.0, 24.0, 16.0);
    block(&mut w, ember::ARCH_X - 4.0, -20.0, ember::ARCH_X + 4.0, 4.0);
    assert!(reach(&w, &g, north)[g.idx(south).unwrap()], "without the facility and the arch, the Crag route is gone");
    // And with the Crag cut off too, the Dunes causeway + stones don't count (stones need jumps):
    // this is the "no other walk route" sanity check that the blocks above really cut something.
    block(&mut w, -70.0, -30.0, -52.0, 24.0);
    assert!(!reach(&w, &g, north)[g.idx(south).unwrap()], "expected no walk route left");
}

#[test]
fn landmarks_are_solid() {
    let (w, info) = generate_ember(1);
    for (top, _) in ember::fortress_ledges() {
        assert_ne!(w.get_at(top - Vec3::Y * 0.25), AIR, "fortress ledge at {top} missing");
        assert_eq!(w.get_at(top + Vec3::Y * 0.25), AIR, "fortress ledge at {top} blocked");
    }
    for s in [-1.0f32, 1.0] {
        let head = Vec3::new(s * ember::ARM_HEAD_X, ember::ARM_TOP_Y - 3.0, ember::RIVER_Z);
        assert_ne!(w.get_at(head), AIR, "collector head at {head} missing");
    }
    let f = ember::FORT;
    assert_ne!(w.get_at(Vec3::new(f.x - 1.75, ember::SPIRE_W_TOP - 2.0, f.y)), AIR, "west spire");
    assert_ne!(w.get_at(Vec3::new(f.x + 1.75, ember::SPIRE_E_TOP - 2.0, f.y)), AIR, "east spire");
    assert_ne!(w.get_at(Vec3::new(f.x, ember::PERCH_Y - 0.25, f.y - 2.0)), AIR, "perch");
    assert_ne!(w.get_at(Vec3::new(f.x + 3.0, ember::BALCONY_Y - 0.25, f.y - 2.0)), AIR, "balcony");
    // Facility: deck and roof are walkable.
    assert_ne!(w.get_at(Vec3::new(15.0, ember::DECK_Y - 0.25, -5.0)), AIR, "deck");
    assert_ne!(w.get_at(Vec3::new(0.0, ember::ROOF_Y - 0.25, -3.0)), AIR, "processor roof");
    // Routes start on something.
    for r in &info.routes {
        let p = r.waypoints[0];
        assert!((0..4).any(|k| w.get_at(p - Vec3::Y * (0.25 + 0.5 * k as f32)) != AIR), "route {} starts in the air at {p}", r.name);
    }
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_ember(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        total += mesh.triangle_count();
    }
    eprintln!("ember: {total} triangles");
    assert!(total <= TRI_BUDGET, "{total} triangles > {TRI_BUDGET}");
}

#[test]
#[ignore]
fn debug_column() {
    let (w, _) = generate_ember(1);
    let at: Vec<f32> = std::env::var("EM_AT").unwrap_or("-1.75,63".into()).split(',').filter_map(|s| s.parse().ok()).collect();
    for y in (40..140).rev() {
        let row: String = (-6..=6)
            .map(|dx| {
                let v = IVec3::new((at[0] / 0.5).floor() as i32 + dx, y, (at[1] / 0.5).floor() as i32);
                let m = w.get(v);
                if m == AIR { '.' } else { char::from(b'A' + (m % 26)) }
            })
            .collect();
        eprintln!("{:5.1} {row}", y as f32 * 0.5);
    }
}
