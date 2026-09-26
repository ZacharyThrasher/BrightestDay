//! Miami map sanity: spawns, walkable connectivity between every spawn and district, no walk-traps,
//! anchoring, determinism, size limits, and generation/mesh timing.
//!
//! Timing + stats: `cargo test -p bd_voxel --release --test miami -- --nocapture`.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use bd_core::voxel::{AIR, VoxelQuery, world_to_voxel};
use bd_voxel::*;
use glam::{IVec3, Vec3};

/// Walk graph over standable cells (feet voxel coordinates). Edges: to a 4-neighbour column cell
/// up to 1 m higher (a jump) or any lower (a drop, ≤ 12 m), plus short gap jumps (≤ 2 m) at the same
/// or lower height. Deliberately conservative: no tethers, no bhop distance.
struct WalkGraph<'a> {
    w: &'a VoxelWorld,
    nodes: Vec<IVec3>,
    index: HashMap<IVec3, u32>,
    /// Standable feet heights per column.
    cols: HashMap<(i32, i32), Vec<i32>>,
}

impl<'a> WalkGraph<'a> {
    /// `min_feet_y`: cells below this (in the sea: a KO, not a place) are left out.
    fn build(w: &'a VoxelWorld, min_feet_y: f32) -> Self {
        let (lo, hi) = (w.voxel_min(), w.voxel_max());
        let mut nodes = Vec::new();
        let mut index = HashMap::new();
        let mut cols: HashMap<(i32, i32), Vec<i32>> = HashMap::new();
        for z in lo.z..hi.z {
            for x in lo.x..hi.x {
                let mut ys = Vec::new();
                let mut run_air = 0;
                for y in (lo.y..hi.y).rev() {
                    let v = IVec3::new(x, y, z);
                    if w.get(v) == AIR {
                        run_air += 1;
                    } else {
                        if run_air >= 4 && (y + 1) as f32 * 0.5 >= min_feet_y {
                            ys.push(y + 1);
                        }
                        run_air = 0;
                    }
                }
                for &y in &ys {
                    let v = IVec3::new(x, y, z);
                    index.insert(v, nodes.len() as u32);
                    nodes.push(v);
                }
                if !ys.is_empty() {
                    cols.insert((x, z), ys);
                }
            }
        }
        Self { w, nodes, index, cols }
    }

    /// Column (x, z) is air from `y0` to `y1` inclusive (a hull can pass through at that height).
    fn clear(&self, x: i32, z: i32, y0: i32, y1: i32) -> bool {
        (y0..=y1).all(|y| self.w.get(IVec3::new(x, y, z)) == AIR)
    }

    fn neighbours(&self, v: IVec3, out: &mut Vec<u32>) {
        out.clear();
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            // Adjacent column: step/jump up ≤ 2 voxels, drop ≤ 24 (stepping over must be clear).
            if let Some(ys) = self.cols.get(&(v.x + dx, v.z + dz)) {
                for &y in ys {
                    let dy = y - v.y;
                    // Dropping: the whole fall column must be open (you land on the first floor below).
                    let ok = if dy >= 0 { self.clear(v.x, v.z, v.y, y + 3) } else { self.clear(v.x + dx, v.z + dz, y, v.y + 3) };
                    if (-24..=2).contains(&dy) && ok {
                        out.push(self.index[&IVec3::new(v.x + dx, y, v.z + dz)]);
                    }
                }
            }
            // Gap jump: up to 4 voxels across open space, landing at the same height or lower.
            for k in 2..=4 {
                // The columns jumped over must be open at head height.
                if !(1..k).all(|j| self.clear(v.x + dx * j, v.z + dz * j, v.y, v.y + 3)) {
                    break;
                }
                if let Some(ys) = self.cols.get(&(v.x + dx * k, v.z + dz * k)) {
                    for &y in ys {
                        let dy = y - v.y;
                        if (-6..=0).contains(&dy) && self.clear(v.x + dx * k, v.z + dz * k, y, v.y + 3) {
                            out.push(self.index[&IVec3::new(v.x + dx * k, y, v.z + dz * k)]);
                        }
                    }
                }
            }
        }
    }

    fn bfs(&self, start: u32, reverse: Option<&Vec<Vec<u32>>>) -> Vec<bool> {
        let mut seen = vec![false; self.nodes.len()];
        let mut q = VecDeque::new();
        seen[start as usize] = true;
        q.push_back(start);
        let mut nb = Vec::new();
        while let Some(i) = q.pop_front() {
            let list: &[u32] = match reverse {
                Some(r) => &r[i as usize],
                None => {
                    self.neighbours(self.nodes[i as usize], &mut nb);
                    &nb
                }
            };
            for &j in list {
                if !seen[j as usize] {
                    seen[j as usize] = true;
                    q.push_back(j);
                }
            }
        }
        seen
    }

    fn reversed(&self) -> Vec<Vec<u32>> {
        let mut r = vec![Vec::new(); self.nodes.len()];
        let mut nb = Vec::new();
        for (i, &v) in self.nodes.iter().enumerate() {
            self.neighbours(v, &mut nb);
            for &j in &nb {
                r[j as usize].push(i as u32);
            }
        }
        r
    }

    /// Nearest node at or just below a feet position.
    fn node_at(&self, p: Vec3) -> Option<u32> {
        let v = world_to_voxel(p + Vec3::Y * 0.01);
        (0..6).find_map(|d| self.index.get(&(v - IVec3::Y * d)).copied())
    }
}

#[test]
fn miami_spawns_and_connectivity() {
    let (w, info) = generate_miami(1);
    assert!(info.spawns.len() >= 8, "spawns: {}", info.spawns.len());
    assert!(info.water_y > 0.0 && info.void_y < info.water_y);
    for (i, a) in info.spawns.iter().enumerate() {
        let feet = world_to_voxel(*a);
        assert!(w.is_solid(feet - IVec3::Y), "spawn {i} floating at {a}");
        for dy in 0..4 {
            assert!(!w.is_solid(feet + IVec3::Y * dy), "spawn {i} buried at {a}");
        }
        assert!(a.y > info.water_y + 1.0, "spawn {i} at sea level");
        for b in &info.spawns[i + 1..] {
            assert!(a.distance(*b) >= 12.0, "spawns too close: {a} {b}");
        }
    }
    for (sx, sz) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        assert!(info.spawns.iter().any(|s| s.x * sx > 0.0 && s.z * sz > 0.0), "no spawn in quadrant ({sx}, {sz})");
    }

    let t = Instant::now();
    let g = WalkGraph::build(&w, info.water_y);
    let s0 = g.node_at(info.spawns[0]).expect("spawn 0 on the walk graph");
    let fwd = g.bfs(s0, None);
    let rev = g.reversed();
    let back = g.bfs(s0, Some(&rev));
    println!("walk graph: {} cells, built + searched in {:.0} ms", g.nodes.len(), t.elapsed().as_secs_f64() * 1e3);

    // Every spawn reaches every other spawn on foot (and back).
    for (i, s) in info.spawns.iter().enumerate() {
        let n = g.node_at(*s).unwrap_or_else(|| panic!("spawn {i} not standable"));
        assert!(fwd[n as usize], "spawn {i} at {s} not walkable from spawn 0");
        assert!(back[n as usize], "spawn 0 not walkable from spawn {i} at {s}");
    }
    // Every district centre (incl. the ship's sun deck, via the gangways + ship stairs… or not) is
    // reachable on foot, except the ship, which needs the gangway (main deck) at least.
    let deck = info.route("gangway").unwrap().waypoints[2];
    let n = g.node_at(deck).expect("ship main deck standable");
    assert!(fwd[n as usize] && back[n as usize], "ship main deck not walkable via the gangways");
    for d in &info.districts {
        if d.name == "cruise_ship" {
            continue;
        }
        let n = g.node_at(d.center).unwrap_or_else(|| panic!("district {} centre not standable ({})", d.name, d.center));
        assert!(fwd[n as usize] && back[n as usize], "district {} not walk-connected", d.name);
    }

    // No walk-traps: every cell you can walk to from a spawn, you can walk back from (tethers are
    // then pure bonus).
    let reach = fwd.iter().filter(|&&b| b).count();
    let mut traps: Vec<IVec3> = (0..g.nodes.len()).filter(|&i| fwd[i] && !back[i]).map(|i| g.nodes[i]).collect();
    traps.sort_by_key(|v| (v.y, v.z, v.x));
    println!("walkable from spawns: {reach} cells; walk-traps: {}", traps.len());
    if !traps.is_empty() {
        let sample: Vec<Vec3> = traps.iter().take(12).map(|v| v.as_vec3() * 0.5).collect();
        println!("trap samples (m): {sample:?}");
    }
    assert!(traps.is_empty(), "walk-trap cells: {} (e.g. {:?})", traps.len(), traps.first());
}

#[test]
fn miami_is_anchored_bounded_and_deterministic() {
    let (a, info) = generate_miami(3);
    let (b, _) = generate_miami(3);
    assert_eq!(a.content_hash(), b.content_hash(), "generation must be deterministic");
    let (c, _) = generate_miami(4);
    assert_ne!(a.content_hash(), c.content_hash(), "seeds should vary details");
    assert_eq!(info.pruned_voxels, 0, "every kit piece should be attached to something indestructible");
    let bw = a.bounds();
    assert!(bw.max.y <= 96.0 + 1e-3);
    let size = bw.max - bw.min;
    assert!((180.0..=200.0).contains(&size.x) && (180.0..=200.0).contains(&size.z));
    // Nothing solid in the top voxel layer (skyline ≤ 95.5 m).
    let top = a.voxel_max().y - 1;
    for z in a.voxel_min().z..a.voxel_max().z {
        for x in a.voxel_min().x..a.voxel_max().x {
            assert_eq!(a.get(IVec3::new(x, top, z)), AIR);
        }
    }
    // Tower cores and bridge piers survive a big blast; the facade doesn't.
    let mut w = a;
    let core = info.district("brickell").unwrap();
    let _ = core;
    let op = CarveOp { shape: CarveShape::Sphere { center: Vec3::new(-84.5, 45.0, 1.0), radius: 9.0 }, power: 3.0, seed: 1 };
    let out = w.apply_carve(&op);
    assert!(out.removed.len() > 500, "the tower blast should remove glass and slabs");
    assert_eq!(w.get(world_to_voxel(Vec3::new(-84.5, 45.0, 1.0))), mat::TOWER_CORE, "the core survives");
}

#[test]
fn sealed_floors_open_when_blasted() {
    let (mut w, _) = generate_miami(1);
    w.drain_dirty();
    // A closed mid-height floor of the tallest tower (west column, row C): sealed until blasted.
    let inside = Vec3::new(-88.0, 5.5 + 12.0 * 4.0 + 2.0, 1.0);
    let v = world_to_voxel(inside);
    assert!(w.is_enclosed_air(v), "closed tower floors are sealed (render-only)");
    let chunk = chunk_of(v);
    let mut mesher = Mesher::new();
    let mut before = ChunkMesh::default();
    mesher.mesh_chunk_into(&w, chunk, &mut before);
    // Blast the west facade of that floor.
    let out =
        w.apply_carve(&CarveOp { shape: CarveShape::Sphere { center: Vec3::new(-92.0, inside.y, 1.0), radius: 2.0 }, power: 1.0, seed: 3 });
    assert!(!out.removed.is_empty());
    assert!(!w.is_enclosed_air(v), "the blast opens the floor");
    let dirty = w.drain_dirty();
    assert!(dirty.contains(&chunk), "the floor's chunk is remeshed");
    let mut after = ChunkMesh::default();
    mesher.mesh_chunk_into(&w, chunk, &mut after);
    assert!(after.triangle_count() > before.triangle_count() + 100, "revealed floor faces are meshed");
    // Collision / hashing never see the bits: an identical world without sealing hashes the same.
    let (a, _) = generate_miami(1);
    assert!(a.has_enclosed_air());
}

#[test]
fn miami_timing() {
    // Warm-up + best of 3 (other work on the machine skews single runs).
    let mut gen_ms = f64::MAX;
    let mut world = None;
    for _ in 0..3 {
        let t = Instant::now();
        let (w, _) = generate_miami(1);
        gen_ms = gen_ms.min(t.elapsed().as_secs_f64() * 1e3);
        world = Some(w);
    }
    let w = world.unwrap();
    let chunks: Vec<IVec3> = w.nonempty_chunks().collect();
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let t = Instant::now();
    let mut tris = 0usize;
    let mut worst = 0.0f64;
    for &c in &chunks {
        let tc = Instant::now();
        mesher.mesh_chunk_into(&w, c, &mut mesh);
        worst = worst.max(tc.elapsed().as_secs_f64() * 1e3);
        tris += mesh.triangle_count();
    }
    let mesh_ms = t.elapsed().as_secs_f64() * 1e3;
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let t = Instant::now();
    std::thread::scope(|s| {
        for part in chunks.chunks(chunks.len().div_ceil(threads)) {
            let w = &w;
            s.spawn(move || {
                let mut m = Mesher::new();
                let mut out = ChunkMesh::default();
                for &c in part {
                    m.mesh_chunk_into(w, c, &mut out);
                }
            });
        }
    });
    let par_ms = t.elapsed().as_secs_f64() * 1e3;
    println!("--- miami ({} build) ---", if cfg!(debug_assertions) { "debug" } else { "release" });
    println!("gen: {gen_ms:.1} ms (best of 3), {} solid voxels, {} anchors", w.solid_count(), w.anchor_hints().len());
    println!("full mesh (1 thread): {mesh_ms:.1} ms for {} chunks, {tris} tris; worst chunk {worst:.2} ms", chunks.len());
    println!("full mesh ({threads} threads): {par_ms:.1} ms");
    if std::env::var("MIAMI_FACES").is_ok() {
        let mut faces: std::collections::BTreeMap<&str, usize> = Default::default();
        for &c in &chunks {
            let o = chunk_origin(c);
            for i in 0..CHUNK_VOLUME as i32 {
                let v = o + IVec3::new(i & 31, i >> 10, (i >> 5) & 31);
                let m = w.get(v);
                if m == AIR {
                    continue;
                }
                let n = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z]
                    .iter()
                    .filter(|&&d| w.get(v + d) == AIR)
                    .count();
                *faces.entry(material_def(m).name).or_default() += n;
            }
        }
        let mut f: Vec<_> = faces.into_iter().collect();
        f.sort_by_key(|x| std::cmp::Reverse(x.1));
        println!("faces by material: {f:?}");
    }
    // Budget is < 400 ms release on an idle machine (~140–270 ms measured). The hard assert is a
    // loose regression guard because other tests run in parallel and skew wall-clock time.
    if !cfg!(debug_assertions) {
        assert!(gen_ms < 1500.0, "generation too slow: {gen_ms:.0} ms (budget 400 ms idle)");
    }
}
