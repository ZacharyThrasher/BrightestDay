//! Property tests for bd_voxel (see docs/TESTING.md): random worlds + random (including absurd) carve
//! sequences must never panic and must preserve the world's invariants; the mesher must emit
//! well-formed geometry for any world.

use std::collections::{HashSet, VecDeque};

use bd_core::VOXEL_SIZE;
use bd_core::voxel::{AIR, Material};
use bd_voxel::world::local_index;
use bd_voxel::*;
use glam::{IVec3, Vec3};
use proptest::prelude::*;

/// Case count × `BD_PROP_SCALE` (default 1; `tools/check.ps1 -Deep` sets 10).
fn scaled(n: u32) -> u32 {
    std::env::var("BD_PROP_SCALE").ok().and_then(|s| s.parse::<u32>().ok()).map_or(n, |k| n.saturating_mul(k.max(1)))
}

// ------------------------------------------------------------------------------------ strategies

/// A world description that can be rebuilt bit-identically (for determinism checks).
#[derive(Clone, Debug)]
struct WorldSpec {
    min: IVec3,
    size: IVec3,
    fills: Vec<Fill>,
}

#[derive(Clone, Debug)]
enum Fill {
    Box { a: IVec3, b: IVec3, m: Material },
    Sphere { c: Vec3, r: f32, m: Material },
    Voxel { v: IVec3, m: Material },
}

fn material() -> impl Strategy<Value = Material> {
    prop_oneof![
        6 => 1u8..=20,           // real materials
        2 => Just(mat::CORE),    // indestructible anchors
        1 => any::<u8>(),        // unknown ids (> table) must still behave
    ]
}

fn world_spec() -> impl Strategy<Value = WorldSpec> {
    (
        (-40i32..40, -8i32..8, -40i32..40),
        (1i32..64, 1i32..48, 1i32..64),
    )
        .prop_flat_map(|(min, size)| {
            let min = IVec3::new(min.0, min.1, min.2);
            let size = IVec3::new(size.0, size.1, size.2);
            let lo = min - IVec3::splat(4);
            let hi = min + size + IVec3::splat(4);
            let coord = move || (lo.x..hi.x, lo.y..hi.y, lo.z..hi.z).prop_map(|(x, y, z)| IVec3::new(x, y, z));
            let fill = prop_oneof![
                3 => (coord(), coord(), material()).prop_map(|(a, b, m)| Fill::Box { a: a.min(b), b: a.max(b), m }),
                2 => (coord(), 0.0f32..6.0, material())
                    .prop_map(|(c, r, m)| Fill::Sphere { c: c.as_vec3() * VOXEL_SIZE, r, m }),
                2 => (coord(), material()).prop_map(|(v, m)| Fill::Voxel { v, m }),
                1 => (coord(), coord()).prop_map(|(a, b)| Fill::Box { a: a.min(b), b: a.max(b), m: AIR }),
            ];
            prop::collection::vec(fill, 0..12).prop_map(move |fills| WorldSpec { min, size, fills })
        })
}

fn build(spec: &WorldSpec) -> VoxelWorld {
    let mut w = VoxelWorld::new(spec.min, spec.size);
    for f in &spec.fills {
        match *f {
            Fill::Box { a, b, m } => {
                // Keep box fills small so tests stay fast.
                let b = b.min(a + IVec3::splat(24));
                w.fill_box(a, b, m)
            }
            Fill::Sphere { c, r, m } => w.fill_sphere(c, r, m, false),
            Fill::Voxel { v, m } => {
                w.set(v, m);
            }
        }
    }
    // Register a few anchors as hints, like the arena generator does.
    let mut hints = Vec::new();
    for c in w.nonempty_chunks().collect::<Vec<_>>() {
        if let Some(d) = w.chunk_data(c) {
            if let Some(i) = d.iter().position(|m| material_def(*m).indestructible) {
                let i = i as i32;
                hints.push(chunk_origin(c) + IVec3::new(i & 31, i >> 10, (i >> 5) & 31));
            }
        }
    }
    for h in hints {
        w.add_anchor_hint(h);
    }
    w.drain_dirty();
    w
}

/// Any f32 with nasty values over-represented.
fn wild_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        2 => any::<f32>(),
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
        1 => Just(f32::NEG_INFINITY),
        1 => Just(0.0f32),
        1 => Just(-1.0f32),
        1 => Just(1e9f32),
        1 => Just(-1e9f32),
        1 => Just(f32::MAX),
        1 => Just(1e-30f32),
    ]
}

/// Mostly sensible numbers near the test worlds, sometimes absurd ones.
fn num(lo: f32, hi: f32) -> impl Strategy<Value = f32> {
    prop_oneof![8 => lo..hi, 1 => wild_f32()]
}

fn point() -> impl Strategy<Value = Vec3> {
    (num(-24.0, 36.0), num(-8.0, 28.0), num(-24.0, 36.0)).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

fn carve_op() -> impl Strategy<Value = CarveOp> {
    let shape = prop_oneof![
        (point(), num(0.0, 8.0)).prop_map(|(center, radius)| CarveShape::Sphere { center, radius }),
        (point(), point()).prop_map(|(min, max)| CarveShape::Box { min, max }),
        (point(), point(), num(0.0, 4.0)).prop_map(|(a, b, radius)| CarveShape::Capsule { a, b, radius }),
    ];
    (shape, num(0.0, 3.0), any::<u32>()).prop_map(|(shape, power, seed)| CarveOp { shape, power, seed })
}

// ------------------------------------------------------------------------------------ invariants

fn for_each_voxel(w: &VoxelWorld, mut f: impl FnMut(IVec3, Material)) {
    let (lo, hi) = (w.voxel_min(), w.voxel_max());
    for y in lo.y..hi.y {
        for z in lo.z..hi.z {
            for x in lo.x..hi.x {
                let v = IVec3::new(x, y, z);
                f(v, w.get(v));
            }
        }
    }
}

struct Census {
    solid: u64,
    indestructible: u64,
}

fn census(w: &VoxelWorld) -> Census {
    let mut c = Census { solid: 0, indestructible: 0 };
    for_each_voxel(w, |_, m| {
        if m != AIR {
            c.solid += 1;
            if material_def(m).indestructible {
                c.indestructible += 1;
            }
        }
    });
    c
}

const N6: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

fn check_outcome(w: &VoxelWorld, before: &Census, out: &CarveOutcome) -> Result<(), TestCaseError> {
    let after = census(w);
    prop_assert_eq!(after.solid, w.solid_count(), "chunk solid counters drifted from contents");
    prop_assert_eq!(after.indestructible, before.indestructible, "an indestructible voxel was removed");
    let removed_total = out.removed.len() as u64 + out.falling.iter().map(|f| f.len() as u64).sum::<u64>();
    prop_assert_eq!(before.solid - after.solid, removed_total, "removed-voxel accounting mismatch");

    let mut seen = HashSet::new();
    for r in &out.removed {
        prop_assert!(w.in_bounds(r.pos), "removed voxel out of bounds: {:?}", r.pos);
        prop_assert!(r.material != AIR);
        prop_assert!(!material_def(r.material).indestructible);
        prop_assert_eq!(w.get(r.pos), AIR);
        prop_assert!(seen.insert(r.pos), "voxel removed twice");
    }
    for chunk in &out.falling {
        prop_assert!(!chunk.is_empty());
        for &(v, m) in &chunk.voxels {
            prop_assert!(w.in_bounds(v));
            prop_assert!(m != AIR && !material_def(m).indestructible, "falling chunk carries an anchor");
            prop_assert_eq!(w.get(v), AIR);
            prop_assert!(seen.insert(v), "voxel removed twice");
            // A falling piece was a separate island: nothing solid may remain next to it (otherwise it
            // was connected to terrain that stayed, i.e. possibly supported).
            for d in N6 {
                prop_assert_eq!(w.get(v + d), AIR, "falling voxel {:?} still touches solid terrain", v);
            }
        }
        prop_assert!(chunk.center_of_mass().is_finite());
        let (lo, hi) = chunk.voxel_bounds();
        prop_assert!(lo.cmple(hi).all());
    }
    Ok(())
}

/// Every solid voxel reachable (6-connected through solids) from an indestructible one.
fn anchored_set(w: &VoxelWorld) -> HashSet<IVec3> {
    let mut seen = HashSet::new();
    let mut q = VecDeque::new();
    for_each_voxel(w, |v, m| {
        if m != AIR && material_def(m).indestructible {
            seen.insert(v);
            q.push_back(v);
        }
    });
    while let Some(v) = q.pop_front() {
        for d in N6 {
            let n = v + d;
            if w.get(n) != AIR && seen.insert(n) {
                q.push_back(n);
            }
        }
    }
    seen
}

// ----------------------------------------------------------------------------------------- tests

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(192), ..ProptestConfig::default() })]

    /// Random worlds × random carve sequences (including NaN/inf/huge/negative parameters): no
    /// panics, exact bookkeeping, anchors never removed, falling pieces are true islands, dirty
    /// chunks cover every edit.
    #[test]
    fn carve_sequences_keep_invariants(spec in world_spec(), ops in prop::collection::vec(carve_op(), 1..8)) {
        let mut w = build(&spec);
        for op in &ops {
            let before = census(&w);
            let hash_before = w.content_hash();
            let out = w.apply_carve(op);
            check_outcome(&w, &before, &out)?;
            let dirty: HashSet<IVec3> = w.drain_dirty().into_iter().collect();
            for v in out.removed.iter().map(|r| r.pos).chain(out.falling.iter().flat_map(|f| f.voxels.iter().map(|x| x.0))) {
                prop_assert!(dirty.contains(&chunk_of(v)), "edited chunk {:?} not marked dirty", chunk_of(v));
            }
            let changed = !out.removed.is_empty() || !out.falling.is_empty();
            prop_assert_eq!(w.content_hash() != hash_before, changed, "content hash must track edits");
            if !op_is_finite(op) {
                prop_assert!(!changed, "a carve with non-finite geometry/power must be a no-op: {:?}", op);
            }
        }
    }

    /// Same world + same ops ⇒ bit-identical results on every "client".
    #[test]
    fn carve_is_deterministic(spec in world_spec(), ops in prop::collection::vec(carve_op(), 1..6)) {
        let mut a = build(&spec);
        let mut b = build(&spec);
        prop_assert_eq!(a.content_hash(), b.content_hash());
        for op in &ops {
            let oa = a.apply_carve(op);
            let ob = b.apply_carve(op);
            prop_assert_eq!(oa, ob);
            prop_assert_eq!(a.content_hash(), b.content_hash());
            prop_assert_eq!(a.drain_dirty(), b.drain_dirty());
        }
    }

    /// With unlimited collapse limits nothing unanchored is left next to a carve.
    #[test]
    fn unlimited_collapse_is_complete(spec in world_spec(), op in carve_op()) {
        let mut w = build(&spec);
        let before = census(&w);
        let removed = w.carve(&op);
        let limits = CollapseLimits { max_component: usize::MAX, max_total: usize::MAX };
        let falling = w.detect_collapse(&removed, limits);
        check_outcome(&w, &before, &CarveOutcome { removed: removed.clone(), falling })?;
        let anchored = anchored_set(&w);
        for r in &removed {
            for d in N6 {
                let n = r.pos + d;
                if w.get(n) != AIR {
                    prop_assert!(anchored.contains(&n), "unanchored voxel {:?} left floating after collapse", n);
                }
            }
        }
    }

    /// Reads and writes anywhere (including i32 extremes) never panic; nothing is stored outside the
    /// world box.
    #[test]
    fn access_anywhere_is_safe(spec in world_spec(), writes in prop::collection::vec((any::<i32>(), any::<i32>(), any::<i32>(), material()), 0..64)) {
        let mut w = build(&spec);
        for (x, y, z, m) in writes {
            let v = IVec3::new(x, y, z);
            let prev = w.set(v, m);
            if !w.in_bounds(v) {
                prop_assert_eq!(prev, AIR);
                prop_assert_eq!(w.get(v), AIR);
            } else {
                prop_assert_eq!(w.get(v), m);
            }
            let _ = w.get_at(Vec3::new(x as f32, y as f32, z as f32));
        }
        prop_assert_eq!(census(&w).solid, w.solid_count());
        // Bounds cover what was asked for.
        prop_assert!(w.voxel_min().cmple(spec.min).all());
        prop_assert!(w.voxel_max().cmpge(spec.min + spec.size).all());
    }

    /// Mesher output is well-formed for any world, and matches a brute-force exposed-face count.
    #[test]
    fn mesher_output_is_well_formed(spec in world_spec(), ops in prop::collection::vec(carve_op(), 0..3)) {
        let mut w = build(&spec);
        for op in &ops {
            w.apply_carve(op);
        }
        let mut mesher = Mesher::new();
        let mut mesh = ChunkMesh::default();
        let chunks: Vec<IVec3> = w.all_chunks().collect();
        for c in chunks {
            mesher.mesh_chunk_into(&w, c, &mut mesh);
            check_mesh(&w, c, &mesh)?;
        }
        // Chunks outside the world mesh to nothing.
        for c in [w.min_chunk() - IVec3::ONE, w.min_chunk() + w.chunk_dims(), IVec3::new(1000, -1000, 7)] {
            mesher.mesh_chunk_into(&w, c, &mut mesh);
            prop_assert!(mesh.is_empty());
        }
    }
}

fn op_is_finite(op: &CarveOp) -> bool {
    let shape_ok = match op.shape {
        CarveShape::Sphere { center, radius } => center.is_finite() && radius.is_finite(),
        CarveShape::Box { min, max } => min.is_finite() && max.is_finite() && (max - min).is_finite(),
        CarveShape::Capsule { a, b, radius } => a.is_finite() && b.is_finite() && radius.is_finite() && (b - a).is_finite(),
    };
    shape_ok && op.power.is_finite()
}

fn check_mesh(w: &VoxelWorld, c: IVec3, m: &ChunkMesh) -> Result<(), TestCaseError> {
    let nv = m.positions.len();
    prop_assert_eq!(m.normals.len(), nv);
    prop_assert_eq!(m.colors.len(), nv);
    prop_assert_eq!(nv % 4, 0);
    prop_assert_eq!(m.indices.len() % 3, 0);
    prop_assert_eq!(m.indices.len(), nv / 4 * 6);
    prop_assert!(m.indices.iter().all(|&i| (i as usize) < nv), "index out of range");
    let o = chunk_origin(c).as_vec3() * VOXEL_SIZE;
    let e = (chunk_origin(c) + IVec3::splat(CHUNK_SIZE)).as_vec3() * VOXEL_SIZE;
    for i in 0..nv {
        let p = Vec3::from(m.positions[i]);
        prop_assert!(p.is_finite());
        prop_assert!(p.cmpge(o - 1e-4).all() && p.cmple(e + 1e-4).all(), "vertex {:?} outside chunk {:?}", p, c);
        let n = Vec3::from(m.normals[i]);
        prop_assert!(n.abs().element_sum() == 1.0 && n.abs().max_element() == 1.0, "normal {:?}", n);
        let col = m.colors[i];
        prop_assert!(col.iter().all(|x| x.is_finite() && *x >= 0.0 && *x <= 4.0), "colour {:?}", col);
        prop_assert_eq!(col[3], 1.0);
    }
    for t in m.indices.chunks_exact(3) {
        let p: Vec<Vec3> = t.iter().map(|&i| Vec3::from(m.positions[i as usize])).collect();
        let n = Vec3::from(m.normals[t[0] as usize]);
        prop_assert!((p[1] - p[0]).cross(p[2] - p[0]).dot(n) > 0.0, "triangle winding must face its normal");
    }
    // Brute-force: 4 vertices per solid voxel face that borders air.
    let mut faces = 0usize;
    if let Some(data) = w.chunk_data(c) {
        let base = chunk_origin(c);
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let l = IVec3::new(x, y, z);
                    if data[local_index(l)] == AIR {
                        continue;
                    }
                    faces += N6.iter().filter(|d| w.get(base + l + **d) == AIR).count();
                }
            }
        }
    }
    prop_assert_eq!(nv, faces * 4, "mesher face count != exposed faces");
    Ok(())
}

// ------------------------------------------------------------------------ extreme single-op cases

#[test]
fn extreme_carve_parameters_are_safe() {
    let specials = [0.0, -0.0, -1.0, 1e-30, 0.25, 1e9, -1e9, 1e30, f32::MAX, f32::MIN, f32::INFINITY, f32::NEG_INFINITY, f32::NAN];
    let (mut w, info) = generate_flat_test(64.0);
    let c = info.center;
    let mut n = 0;
    for &r in &specials {
        for &p in &specials {
            for shape in [
                CarveShape::Sphere { center: c, radius: r },
                CarveShape::Sphere { center: Vec3::splat(r), radius: 2.0 },
                CarveShape::Box { min: c - Vec3::splat(r), max: c + Vec3::splat(1.0) },
                CarveShape::Box { min: c + Vec3::splat(2.0), max: c - Vec3::splat(2.0) }, // inverted
                CarveShape::Capsule { a: c, b: c, radius: r },
                CarveShape::Capsule { a: c, b: Vec3::new(r, c.y, c.z), radius: 1.0 },
            ] {
                let op = CarveOp { shape, power: p, seed: n };
                let before = census(&w);
                let out = w.apply_carve(&op);
                n += 1;
                if !op_is_finite(&op) {
                    assert!(out.removed.is_empty() && out.falling.is_empty(), "non-finite op carved: {op:?}");
                }
                assert_eq!(census(&w).indestructible, before.indestructible, "{op:?}");
                for rv in &out.removed {
                    assert!(w.in_bounds(rv.pos));
                }
                w.drain_dirty();
            }
        }
    }
    // The bedrock survived everything, and the world is still consistent.
    assert_eq!(census(&w).solid, w.solid_count());
}

#[test]
fn arena_generation_is_deterministic() {
    for seed in [0u64, 0xdead_beef] {
        let (a, ia) = generate_arena(seed);
        let (b, ib) = generate_arena(seed);
        assert_eq!(a.content_hash(), b.content_hash(), "seed {seed}");
        assert_eq!(ia.spawns, ib.spawns);
    }
}
