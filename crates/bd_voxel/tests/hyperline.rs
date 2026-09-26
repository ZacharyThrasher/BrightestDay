//! Hyperline map checks (feature `hyperline`): determinism, spawn validity (no solids, not at the
//! edge) + mutual reachability across the cars (walk / step / ≤ 2 m jumps / falls / running gap
//! jumps ≤ 4 m, 1 m hull), the frame is indestructible and nothing was pruned, the slipstream
//! footprint covers the train, surf lines sit on the shoulders, and the triangle budget.
#![cfg(feature = "hyperline")]

use std::collections::VecDeque;

use bd_core::voxel::AIR;
use bd_voxel::maps::hyperline::{self, FLAT_HW, FLOOR_Y, HALF_W, NOSE_X, ROOF_Y, TAIL_X, UNITS, VOID_Y, generate_hyperline};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld, material_def};
use glam::{IVec3, Vec3};

/// The train is compact: keep it cheap.
const TRI_BUDGET: usize = 170_000;

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
    y as f32 * 0.5 >= VOID_Y + 1.0 && clear(w, x, y, z, 4) && (0..2).any(|dz| (0..2).any(|dx| w.get(IVec3::new(x + dx, y - 1, z + dz)) != AIR))
}

fn push(g: &Grid, seen: &mut [bool], q: &mut VecDeque<IVec3>, v: IVec3) {
    if let Some(i) = g.idx(v)
        && !seen[i]
    {
        seen[i] = true;
        q.push_back(v);
    }
}

/// Nodes reachable from `start` (feet voxel): walk, step ≤ 0.5 m, jump ≤ 2 m, fall, and running
/// jumps straight across a gap of ≤ 4 m (8 voxels) landing at most 0.5 m higher.
fn reach(w: &VoxelWorld, g: &Grid, start: IVec3) -> Vec<bool> {
    let mut seen = vec![false; (g.size.x * g.size.y * g.size.z) as usize];
    let mut q = VecDeque::new();
    push(g, &mut seen, &mut q, start);
    while let Some(v) = q.pop_front() {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (v.x + dx, v.z + dz);
            // Step up 0.5 m (walking, no jump: only the new spot needs head room).
            if standable(w, nx, v.y + 1, nz) {
                push(g, &mut seen, &mut q, IVec3::new(nx, v.y + 1, nz));
            }
            for dy in 1..=4 {
                let ny = v.y + dy;
                if !clear(w, v.x, v.y, v.z, dy + 4) {
                    break;
                }
                if standable(w, nx, ny, nz) {
                    push(g, &mut seen, &mut q, IVec3::new(nx, ny, nz));
                }
            }
            if clear(w, nx, v.y, nz, 4) {
                let mut ny = v.y;
                while ny as f32 * 0.5 >= VOID_Y + 1.0 && !standable(w, nx, ny, nz) {
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
            // Running gap jump: 2 m of clear air over the whole flight, landing 1..8 voxels away.
            for k in 2..=8 {
                let (jx, jz) = (v.x + dx * k, v.z + dz * k);
                if !(1..k).all(|i| clear(w, v.x + dx * i, v.y, v.z + dz * i, 5)) {
                    break;
                }
                let mut landed = false;
                for ly in [v.y + 1, v.y, v.y - 1, v.y - 2] {
                    if standable(w, jx, ly, jz) {
                        push(g, &mut seen, &mut q, IVec3::new(jx, ly, jz));
                        landed = true;
                        break;
                    }
                }
                if landed {
                    break;
                }
            }
        }
    }
    seen
}

fn feet(s: Vec3) -> IVec3 {
    IVec3::new((s.x / 0.5).floor() as i32 - 1, (s.y / 0.5).round() as i32, (s.z / 0.5).floor() as i32 - 1)
}

fn node_near(w: &VoxelWorld, s: Vec3) -> Option<IVec3> {
    let f = feet(s);
    for dz in 0..=1 {
        for dx in 0..=1 {
            let v = f + IVec3::new(dx, 0, dz);
            if standable(w, v.x, v.y, v.z) {
                return Some(v);
            }
        }
    }
    None
}

#[test]
fn deterministic_per_seed() {
    let (a, ia) = generate_hyperline(7);
    let (b, ib) = generate_hyperline(7);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia, ib);
    // Seeds vary gangways / pods, never the spawns or the footprint.
    let (_, ic) = generate_hyperline(8);
    assert_eq!(ia.spawns, ic.spawns);
    assert_eq!(ia.slipstream, ic.slipstream);
}

#[test]
fn spawns_valid_and_mutually_reachable() {
    for seed in [1u64, 2, 3] {
        let (w, info) = generate_hyperline(seed);
        assert!(info.spawns.len() >= 8, "seed {seed}: {} spawns", info.spawns.len());
        assert_eq!(info.pruned_voxels, 0, "seed {seed}: detached voxels");
        let roof = info.spawns.iter().filter(|s| s.y == ROOF_Y).count();
        let inside = info.spawns.iter().filter(|s| s.y == FLOOR_Y).count();
        assert!(roof >= 8 && inside >= 2, "seed {seed}: roof {roof} inside {inside}");
        let us = hyperline::units(seed);
        for s in &info.spawns {
            // Hull (0.8 × 1.8 m) fully in air, standing on solid.
            for y in 0..4 {
                for (dx, dz) in [(-0.35f32, -0.35f32), (0.35, -0.35), (-0.35, 0.35), (0.35, 0.35), (0.0, 0.0)] {
                    let p = Vec3::new(s.x + dx, s.y + 0.25 + y as f32 * 0.5, s.z + dz);
                    assert_eq!(w.get((p / 0.5).floor().as_ivec3()), AIR, "seed {seed}: spawn {s} in a solid at {p}");
                }
            }
            assert_ne!(w.get(((*s - Vec3::Y * 0.25) / 0.5).floor().as_ivec3()), AIR, "seed {seed}: spawn {s} floats");
            // Never at the edge: on the centre line, well inside a unit.
            assert!(s.z.abs() <= 1.0, "seed {seed}: spawn {s} near the side");
            assert!(us.iter().any(|u| s.x > u.x0 + 3.0 && s.x < u.x1 - 3.0), "seed {seed}: spawn {s} near a gap");
            // Spread out.
            for o in &info.spawns {
                // 8 m apart on the same deck; roof vs inside are separated by the roof itself.
                let min = if o.y == s.y { 8.0 } else { 4.5 };
                if o != s {
                    assert!(o.distance(*s) >= min, "seed {seed}: spawns {s} / {o} too close");
                }
            }
        }
        // Every spawn reaches every other (roof ↔ roof over the gaps, inside → roof by the hatch stairs).
        let g = Grid::new(&w);
        let nodes: Vec<IVec3> = info.spawns.iter().map(|s| node_near(&w, *s).unwrap_or_else(|| panic!("seed {seed}: spawn {s} not standable"))).collect();
        for (i, a) in nodes.iter().enumerate() {
            let seen = reach(&w, &g, *a);
            for (j, b) in nodes.iter().enumerate() {
                let ok = g.idx(*b).is_some_and(|k| seen[k]);
                assert!(ok, "seed {seed}: spawn {} can't reach spawn {}", info.spawns[i], info.spawns[j]);
            }
        }
    }
}

#[test]
fn frame_is_indestructible() {
    let (w, _) = generate_hyperline(1);
    for u in hyperline::units(1) {
        // Underframe under each unit, the guideway under each gap.
        for lx in [2.0f32, u.len() * 0.5, u.len() - 2.0] {
            let v = (Vec3::new(u.x0 + lx, 7.25, 0.25) / 0.5).floor().as_ivec3();
            assert!(material_def(w.get(v)).indestructible, "{}: underframe at lx {lx}", u.name());
        }
        let g = (Vec3::new(u.x0 - 1.5, 4.75, 0.25) / 0.5).floor().as_ivec3();
        if u.index < UNITS - 1 {
            assert!(material_def(w.get(g)).indestructible, "{}: guideway under the gap", u.name());
        }
        // The roof top panels break (holes into the cars).
        let r = (Vec3::new(u.x0 + 10.25, ROOF_Y - 0.25, 1.25) / 0.5).floor().as_ivec3();
        let m = w.get(r);
        assert!(m != AIR && !material_def(m).indestructible, "{}: roof panel at {r} is {m}", u.name());
    }
}

#[test]
fn slipstream_footprint_and_surf_lines() {
    let (w, info) = generate_hyperline(1);
    let s = info.slipstream.expect("hyperline has a slipstream");
    assert_eq!((s.min.x, s.max.x), (TAIL_X, NOSE_X));
    assert_eq!((s.min.y, s.max.y), (-HALF_W, HALF_W));
    // Every train voxel is inside the footprint.
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    for x in lo.x..hi.x {
        for z in lo.z..hi.z {
            if w.top_solid_y(x, z).is_some() {
                let p = Vec3::new((x as f32 + 0.5) * 0.5, 10.0, (z as f32 + 0.5) * 0.5);
                assert_eq!(s.outside(p), 0.0, "voxel column at {p} is outside the footprint");
            }
        }
    }
    // Blast zones: side zones well beyond the train, the rear one behind the tail.
    let b = w.bounds();
    assert!(b.min.z < -HALF_W - 8.0 && b.max.z > HALF_W + 8.0);
    assert!(b.min.x < TAIL_X - 8.0);
    // Surf lines: one per shoulder of every unit, the ridge on the flat roof's edge, faces clear of voxels.
    assert_eq!(info.surf_lines.len(), 2 * UNITS);
    for l in &info.surf_lines {
        assert!((l.a.z.abs() - FLAT_HW).abs() < 1e-4 && (l.a.y - ROOF_Y).abs() < 0.05);
        let side = l.a.z.signum();
        let (st, ct) = l.face_deg.to_radians().sin_cos();
        let mut x = l.a.x;
        while x < l.b.x {
            for k in 1..8 {
                let t = l.width * k as f32 / 8.0;
                // Just above the face.
                let p = Vec3::new(x, l.a.y - t * st + 0.2, l.a.z + side * t * ct);
                assert_eq!(w.get((p / 0.5).floor().as_ivec3()), AIR, "voxel pokes through surf face at {p}");
            }
            x += 0.5;
        }
    }
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_hyperline(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut tris = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        tris += mesh.triangle_count();
    }
    assert!(tris <= TRI_BUDGET, "{tris} triangles > {TRI_BUDGET}");
}

#[test]
#[ignore]
fn debug_stairs() {
    let (w, _) = generate_hyperline(1);
    let u = &hyperline::units(1)[2];
    for zc in [2.25f32, 1.75] {
        println!("z = {zc}");
        for y in (14..30).rev() {
            let mut line = String::new();
            for x in ((u.x0 + 14.0) / 0.5) as i32..((u.x1 + 0.5) / 0.5) as i32 {
                let m = w.get(IVec3::new(x, y, (zc / 0.5).floor() as i32));
                line.push(if m == AIR { '.' } else if material_def(m).indestructible { '#' } else { 'o' });
            }
            println!("{:5.1} {line}", y as f32 * 0.5);
        }
    }
}
