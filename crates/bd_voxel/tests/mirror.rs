//! Mirrorfold map checks (feature `mirror`): determinism, spawn validity + mutual reachability
//! (walk / fall / jumps incl. gap jumps, with a 1 m hull), the curl's hard light (angles, nothing
//! voxel poking through it, continuous loop), fractures open to the void, hanging-tower anchors,
//! sealed interiors, pruning and the triangle budget.
#![cfg(feature = "mirror")]

use std::collections::{HashMap, VecDeque};

use bd_core::voxel::AIR;
use bd_voxel::maps::mirror::{self, CURL_BREAKS, HALF_X, STREET_Y, VOID_Y, WALK_Y, generate_mirror};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld, material_def};
use glam::{IVec3, Vec2, Vec3};

const TRI_BUDGET_TOTAL: usize = 1_100_000;

/// 2 × 2 voxel footprint (1 m) with `h` voxels of air from `y`.
fn clear(w: &VoxelWorld, x: i32, y: i32, z: i32, h: i32) -> bool {
    (0..2).all(|dz| (0..2).all(|dx| (y..y + h).all(|yy| w.get(IVec3::new(x + dx, yy, z + dz)) == AIR)))
}

fn standable(w: &VoxelWorld, x: i32, y: i32, z: i32) -> bool {
    y as f32 * 0.5 >= VOID_Y && clear(w, x, y, z, 4) && (0..2).any(|dz| (0..2).any(|dx| w.get(IVec3::new(x + dx, y - 1, z + dz)) != AIR))
}

/// Per column (x, z): the standable feet heights, precomputed once.
struct Stand {
    lo: IVec3,
    size: IVec3,
    cols: Vec<Vec<i32>>,
}

impl Stand {
    fn new(w: &VoxelWorld) -> Self {
        let (lo, hi) = (w.voxel_min(), w.voxel_max());
        let size = hi - lo;
        let mut cols = vec![Vec::new(); (size.x * size.z) as usize];
        for z in lo.z..hi.z - 1 {
            for x in lo.x..hi.x - 1 {
                let c = &mut cols[((x - lo.x) + size.x * (z - lo.z)) as usize];
                for y in (10..hi.y - 4).rev() {
                    if standable(w, x, y, z) {
                        c.push(y);
                    }
                }
            }
        }
        Self { lo, size, cols }
    }
    fn col(&self, x: i32, z: i32) -> &[i32] {
        let (ix, iz) = (x - self.lo.x, z - self.lo.z);
        if ix < 0 || iz < 0 || ix >= self.size.x || iz >= self.size.z {
            return &[];
        }
        &self.cols[(ix + self.size.x * iz) as usize]
    }
    fn is(&self, v: IVec3) -> bool {
        self.col(v.x, v.z).contains(&v.y)
    }
}

/// Nodes reachable from `start`: walk to a neighbour (step / jump up ≤ 2 m with head room, or fall),
/// plus gap jumps from ledges: any standable spot within 4 m horizontally, ≤ 1.5 m higher or lower.
fn reach(w: &VoxelWorld, st: &Stand, start: IVec3) -> HashMap<IVec3, ()> {
    let mut seen: HashMap<IVec3, ()> = HashMap::new();
    let mut q = VecDeque::new();
    seen.insert(start, ());
    q.push_back(start);
    while let Some(v) = q.pop_front() {
        let mut ledge = false;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (v.x + dx, v.z + dz);
            for dy in 1..=4 {
                if !clear(w, v.x, v.y, v.z, dy + 4) {
                    break;
                }
                let n = IVec3::new(nx, v.y + dy, nz);
                if st.is(n) && seen.insert(n, ()).is_none() {
                    q.push_back(n);
                }
            }
            if !clear(w, nx, v.y, nz, 4) {
                continue;
            }
            // Level or down: the first standable height at or below.
            if let Some(&y) = st.col(nx, nz).iter().find(|&&y| y <= v.y) {
                if y < v.y {
                    ledge = true;
                }
                if (v.y - y) > 1 || !clear(w, nx, y, nz, v.y - y + 4) {
                    // Falling: must be a clear drop.
                    if !clear(w, nx, y, nz, v.y - y + 4) {
                        continue;
                    }
                }
                let n = IVec3::new(nx, y, nz);
                if seen.insert(n, ()).is_none() {
                    q.push_back(n);
                }
            } else {
                ledge = true;
            }
        }
        if ledge {
            for dz in -8..=8i32 {
                for dx in -8..=8i32 {
                    if dx * dx + dz * dz > 64 {
                        continue;
                    }
                    for &y in st.col(v.x + dx, v.z + dz) {
                        if y <= v.y + 3 && y >= v.y - 3 {
                            let n = IVec3::new(v.x + dx, y, v.z + dz);
                            if seen.insert(n, ()).is_none() {
                                q.push_back(n);
                            }
                        }
                    }
                }
            }
        }
    }
    seen
}

fn spawn_node(st: &Stand, s: Vec3) -> Option<IVec3> {
    let v = (s / 0.5).floor().as_ivec3();
    [(0, 0), (-1, 0), (0, -1), (-1, -1)].iter().map(|&(dx, dz)| IVec3::new(v.x + dx, v.y, v.z + dz)).find(|n| st.is(*n))
}

#[test]
fn deterministic_per_seed() {
    let (a, ia) = generate_mirror(3);
    let (b, ib) = generate_mirror(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia.spawns, ib.spawns);
    assert_eq!(ia.routes, ib.routes);
    assert_eq!(ia.surf_lines, ib.surf_lines);
    let (c, _) = generate_mirror(4);
    assert_ne!(a.content_hash(), c.content_hash(), "different seeds should differ in the details");
}

#[test]
fn spawns_valid_spread_and_mutually_reachable() {
    for seed in [1u64, 2] {
        let (w, info) = generate_mirror(seed);
        assert!((8..=10).contains(&info.spawns.len()), "seed {seed}: {} spawns", info.spawns.len());
        for (i, s) in info.spawns.iter().enumerate() {
            let v = (*s / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), AIR, "seed {seed}: spawn {i} {s} inside a solid");
            assert_eq!(w.get(v + IVec3::Y * 3), AIR, "seed {seed}: spawn {i} {s} without head room");
            assert_ne!(w.get(v - IVec3::Y), AIR, "seed {seed}: spawn {i} {s} floating");
            let pr = mirror::perim(s.x, s.z);
            assert!(pr.s < -1.0, "seed {seed}: spawn {i} {s} on the curl");
            assert!(mirror::fracture_dist(Vec2::new(s.x, s.z)) > 3.5, "seed {seed}: spawn {i} {s} next to a fracture");
            for (j, t) in info.spawns.iter().enumerate().skip(i + 1) {
                assert!(s.distance(*t) >= 10.0, "seed {seed}: spawns {i} and {j} too close");
            }
        }
        // Heights: street, roofs and floating islands all represented.
        assert!(info.spawns.iter().any(|s| s.y > 28.0), "a floating-island spawn");
        assert!(info.spawns.iter().filter(|s| s.y < WALK_Y + 0.6).count() >= 4, "ground spawns");
        let st = Stand::new(&w);
        let nodes: Vec<IVec3> = info.spawns.iter().map(|s| spawn_node(&st, *s).unwrap_or_else(|| panic!("seed {seed}: spawn {s} not standable"))).collect();
        for (i, &from) in nodes.iter().enumerate() {
            let seen = reach(&w, &st, from);
            for (j, to) in nodes.iter().enumerate() {
                assert!(seen.contains_key(to), "seed {seed}: spawn {j} {:?} not reachable from spawn {i} {:?}", info.spawns[j], info.spawns[i]);
            }
        }
    }
}

#[test]
fn curl_hard_light_is_clear_continuous_and_surfable() {
    let (w, info) = generate_mirror(1);
    let lines = mirror::curl_surf_lines();
    assert!(lines.len() >= 100, "{} curl lines", lines.len());
    for l in &lines {
        assert!(l.a.is_finite() && l.b.is_finite());
        assert!((l.a.y - l.b.y).abs() < 1e-4, "curl ridges are level");
        // One walkable ramp, the rest in the surf band (45.6°–80°).
        let walk = l.face_deg < 45.0;
        assert!(walk || (46.0..=79.0).contains(&l.face_deg), "face {:.1}° outside the surf band", l.face_deg);
        // Nothing voxel on or just in front of the plaza-facing face (sampled).
        let d = (l.b - l.a).normalize();
        let r = d.cross(Vec3::Y).normalize();
        let up = r.cross(d).normalize();
        let (st, ct) = l.face_deg.to_radians().sin_cos();
        let n = r * st + up * ct;
        let f = r * ct - up * st;
        let len = (l.b - l.a).length();
        for i in 1..8 {
            for j in 0..5 {
                let p = l.a + d * (len * i as f32 / 8.0) + f * ((l.width - 0.6) * j as f32 / 4.0) + n * 0.12;
                if p.y < STREET_Y + 0.3 {
                    continue; // the walkable ramp's foot is buried in the street on purpose
                }
                assert_eq!(w.get_at(p), AIR, "voxel pokes through the hard light at {p} (line {l:?})");
            }
        }
    }
    // The map carries them (plus the two chutes).
    assert_eq!(info.surf_lines.len(), lines.len() + 2);
    // Continuity: consecutive corner / straight ridges of the same chord meet (≤ 0.35 m apart).
    let per_chord = lines.len() / (CURL_BREAKS.len() - 1);
    for chunk in lines.chunks(per_chord) {
        for a in chunk {
            let near = chunk.iter().filter(|b| *b != a).map(|b| a.b.distance(b.a).min(a.b.distance(b.b))).fold(f32::INFINITY, f32::min);
            assert!(near < 0.8, "gap after ridge {a:?}: {near:.2} m");
        }
    }
    // The ring the loop route follows stays on the band and goes all the way round.
    let ring = &info.route("curl_loop").expect("curl loop route").waypoints;
    assert!(ring.len() > 40);
    let mut turned = 0.0f32;
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        let da = b.z.atan2(b.x) - a.z.atan2(a.x);
        turned += (da + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        assert!(b.y > STREET_Y + 5.0 && b.y < STREET_Y + 16.0);
    }
    assert!((turned.abs() - std::f32::consts::TAU).abs() < 0.1, "loop turns {turned}");
}

#[test]
fn fractures_open_to_the_void_and_blast_zones() {
    let (w, info) = generate_mirror(1);
    for (pts, _) in mirror::fractures() {
        for s in pts.windows(2) {
            let m = (s[0] + s[1]) * 0.5;
            let v = (Vec3::new(m.x, 0.0, m.y) / 0.5).floor().as_ivec3();
            let solid = (0..(WALK_Y / 0.5) as i32 + 2).find(|&y| w.get(IVec3::new(v.x, y, v.z)) != AIR);
            assert!(solid.is_none(), "fracture at {m} is floored at voxel y {solid:?}");
        }
    }
    assert!(info.void_y < STREET_Y - 6.0 && info.void_y > 0.0);
    // Side blast zones sit just behind the curl wall.
    assert!(info.bounds.max.x > HALF_X + mirror::CURL_R + 1.0 && info.bounds.max.x < HALF_X + mirror::CURL_R + 6.0);
    assert!(info.bounds.max.y > mirror::TOP_Y + 20.0, "star KO well above the hanging towers");
}

#[test]
fn hanging_towers_are_anchored_and_in_tether_range() {
    let (w, info) = generate_mirror(1);
    let tips = mirror::tower_tips();
    assert_eq!(tips.len(), 6);
    for t in &tips {
        let m = w.get_at(*t + Vec3::Y * 0.3);
        assert!(material_def(m).indestructible, "spire tip at {t} is {}", material_def(m).name);
        // From the plaza (beside the obelisk) the spire is a clean tether shot (< 120 m, first hit
        // is the tower).
        let eye = Vec3::new(-8.0, WALK_Y + 1.6, 6.0);
        let to = *t + Vec3::Y * 0.8;
        let hit = bd_core::voxel::raycast(&w, eye, to - eye, 120.0).expect("tower in tether range");
        assert!(hit.point.distance(to) < 12.0, "line to spire {t} blocked at {}", hit.point);
        assert!(t.y > STREET_Y + 40.0 && t.y < STREET_Y + 52.0, "tip height {}", t.y);
    }
    assert!(w.has_enclosed_air(), "no sealed floors");
    assert!(info.pruned_voxels < 3000, "{} voxels pruned (a kit piece came out detached?)", info.pruned_voxels);
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_mirror(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let mut total = 0;
    for c in w.nonempty_chunks() {
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        total += mesh.triangle_count();
    }
    eprintln!("Mirrorfold: {total} triangles");
    assert!(total <= TRI_BUDGET_TOTAL, "{total} triangles > {TRI_BUDGET_TOTAL}");
}
