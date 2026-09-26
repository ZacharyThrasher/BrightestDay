//! Valley of the End checks (feature `valley`): determinism, spawns valid + mutually reachable
//! (walk / fall / ≤ 2 m jumps with a 1 m hull), water sheets only over a bed (never over the void),
//! the KO drops exist, statues have indestructible cores, and the triangle budget.
#![cfg(feature = "valley")]

use std::collections::VecDeque;

use bd_core::voxel::AIR;
use bd_voxel::maps::valley::{self, LIP_Z, PLUNGE_C, RIVER_Y, VOID_Y, generate_valley, statue_specs};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld, mat, material_def};
use glam::{IVec3, Vec3};

const TRI_BUDGET: usize = 1_050_000;

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

/// Nodes reachable from `start` (feet voxel) by walking, stepping, jumping ≤ 2 m and falling.
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
    let (a, ia) = generate_valley(3);
    let (b, ib) = generate_valley(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia.spawns, ib.spawns);
    assert_eq!(ia.water_sheets, ib.water_sheets);
    assert_eq!(ia.routes, ib.routes);
    let (c, _) = generate_valley(4);
    assert_ne!(a.content_hash(), c.content_hash(), "different seeds should differ");
}

#[test]
fn spawns_valid_spread_and_mutually_reachable() {
    for seed in [1u64, 2] {
        let (w, info) = generate_valley(seed);
        assert!((8..=10).contains(&info.spawns.len()), "seed {seed}: {} spawns", info.spawns.len());
        for s in &info.spawns {
            let v = (*s / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), AIR, "seed {seed}: spawn {s} inside a solid");
            assert_eq!(w.get(v + IVec3::Y * 3), AIR, "seed {seed}: spawn {s} without head room");
            assert_ne!(w.get(v - IVec3::Y), AIR, "seed {seed}: spawn {s} floating");
            assert!(!info.water_sheets.iter().any(|ws| ws.covers(*s, 0.0) && ws.center.y > s.y), "seed {seed}: spawn {s} under water");
        }
        // Spread: every pair ≥ 12 m apart, heights from the gorge floor to the cliff tops.
        for (i, a) in info.spawns.iter().enumerate() {
            for b in &info.spawns[i + 1..] {
                assert!(a.distance(*b) >= 12.0, "seed {seed}: spawns {a} and {b} too close");
            }
        }
        let low = info.spawns.iter().filter(|s| s.y < 25.0).count();
        let high = info.spawns.iter().filter(|s| s.y > 55.0).count();
        assert!(low >= 3 && high >= 4, "seed {seed}: {low} low / {high} high spawns");
        let g = Grid::new(&w);
        let nodes: Vec<IVec3> =
            info.spawns.iter().map(|s| spawn_node(&w, *s).unwrap_or_else(|| panic!("seed {seed}: spawn {s} not standable"))).collect();
        for (i, &from) in nodes.iter().enumerate() {
            let seen = reach(&w, &g, from);
            for (j, &to) in nodes.iter().enumerate() {
                assert!(
                    seen[g.idx(to).unwrap()],
                    "seed {seed}: spawn {j} {:?} not reachable on foot from spawn {i} {:?}",
                    info.spawns[j],
                    info.spawns[i]
                );
            }
        }
    }
}

#[test]
fn water_sheets_sit_on_a_bed_and_the_void_drops_exist() {
    let (w, info) = generate_valley(1);
    assert!(info.water_walk, "the Valley walks on water");
    assert!(info.water_sheets.len() >= 3);
    // Every covered column: either land above the water (banks hide the sheet's edges) or a bed
    // within the sheet's depth — never water over the void.
    for x in (-160..160).step_by(2) {
        for z in (-224..224).step_by(2) {
            let p = Vec3::new(x as f32 * 0.5 + 0.25, 0.0, z as f32 * 0.5 + 0.25);
            for s in info.water_sheets.iter().filter(|s| s.covers(p, 0.0)) {
                let top = valley::top_at(&w, p.x, p.z).unwrap_or(0.0);
                if top >= s.center.y {
                    continue;
                }
                assert!(top >= s.center.y - s.depth - 0.6, "water over a drop at {p} (sheet y {}, ground {top})", s.center.y);
            }
        }
    }
    // The river is walkable water all the way down the gorge.
    for z in [30.0f32, 0.0, -20.0, -55.0, -90.0] {
        let p = Vec3::new(valley::centre_x(z), 0.0, z);
        assert!(info.water_sheets.iter().any(|s| s.covers(p, 0.0) && s.center.y == RIVER_Y), "no river sheet at z {z}");
        let top = valley::top_at(&w, p.x, p.z).unwrap();
        assert!(top < RIVER_Y && top > RIVER_Y - 2.5, "river bed at z {z}: {top}");
    }
    // KO drops: the plunge hole and past the downstream lip go all the way down to the bedrock.
    for p in [Vec3::new(PLUNGE_C.x, 0.0, PLUNGE_C.y), Vec3::new(valley::centre_x(LIP_Z - 8.0), 0.0, LIP_Z - 8.0)] {
        let top = valley::top_at(&w, p.x, p.z).unwrap_or(0.0);
        assert!(top < VOID_Y, "no drop to the void at {p}: ground at {top}");
    }
    // The falls face is indestructible white water (a wall that refreshes the air jump).
    let v = (Vec3::new(0.0, 36.0, valley::FALLS_Z + 0.25) / 0.5).floor().as_ivec3();
    assert!(matches!(w.get(v), mat::WATERFALL | mat::WATERFALL_BLUE));
    assert!(material_def(w.get(v)).indestructible);
}

#[test]
fn statues_stand_with_cores_and_perches() {
    let (w, _) = generate_valley(1);
    for s in statue_specs() {
        // Core through the legs and torso: indestructible.
        for (a, y, c) in [(0.0, 4.0, 3.2), (0.0, 16.0, -3.2), (-0.4, 30.0, 0.0)] {
            let p = s.world(a, y, c);
            let v = (p / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), mat::STATUE_CORE, "{:?}: no core at {p}", s.hair);
        }
        // Head, shoulders and the raised seal are solid, breakable stone.
        for p in [s.head_top() - Vec3::Y * 2.0, s.pauldron_top(1.0) - Vec3::Y * 0.6, s.pauldron_top(-1.0) - Vec3::Y * 0.6, s.seal_hand() - Vec3::Y * 0.8] {
            let m = w.get((p / 0.5).floor().as_ivec3());
            assert!(m == mat::STATUE_STONE || m == mat::STATUE_SHADE, "{:?}: {p} is {}", s.hair, material_def(m).name);
        }
        // The shoulder-guard tops are perches (air above).
        let top = s.pauldron_top(1.0) + Vec3::Y * 0.6;
        assert_eq!(w.get((top / 0.5).floor().as_ivec3()), AIR);
        assert!(s.head_top().y > 70.0 && s.head_top().y < 85.0, "statue height {:?}", s.head_top());
    }
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_valley(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        total += mesh.triangle_count();
    }
    assert!(total <= TRI_BUDGET, "{total} triangles > {TRI_BUDGET}");
}
