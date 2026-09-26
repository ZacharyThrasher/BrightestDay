//! High Roller map checks (feature `casino`): determinism, spawns (valid, spread, mutually reachable
//! by walking / falling / ≤ 2 m jumps with a 1 m hull), the triangle budget, destructibility rules,
//! interactables (levers + reel windows clear, pockets standable), the surf halo (clear of voxels,
//! walkable clearance under it), routes (bhop lane clear, chandelier anchors indestructible), and no
//! void holes through the hall floor.
#![cfg(feature = "casino")]

use std::collections::VecDeque;

use bd_core::voxel::AIR;
use bd_voxel::maps::casino::{self, CHANDELIER_R, FLOOR_Y, HUB_Y, POCKET_Y, VOID_Y, generate_casino};
use bd_voxel::material::{is_indestructible, material_def};
use bd_voxel::{ChunkMesh, Mesher, VoxelWorld, mat};
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

fn clear(w: &VoxelWorld, x: i32, y: i32, z: i32, h: i32) -> bool {
    (0..2).all(|dz| (0..2).all(|dx| (y..y + h).all(|yy| w.get(IVec3::new(x + dx, yy, z + dz)) == AIR)))
}

fn standable(w: &VoxelWorld, x: i32, y: i32, z: i32) -> bool {
    y as f32 * 0.5 >= VOID_Y && clear(w, x, y, z, 4) && (0..2).any(|dz| (0..2).any(|dx| w.get(IVec3::new(x + dx, y - 1, z + dz)) != AIR))
}

/// Nodes reachable from `start` by walking, stepping, jumping ≤ 2 m and falling (no tethers).
fn reach(w: &VoxelWorld, g: &Grid, start: IVec3) -> Vec<bool> {
    let mut seen = vec![false; (g.size.x * g.size.y * g.size.z) as usize];
    let mut q = VecDeque::new();
    if let Some(i) = g.idx(start) {
        seen[i] = true;
        q.push_back(start);
    }
    let push = |seen: &mut Vec<bool>, q: &mut VecDeque<IVec3>, v: IVec3| {
        if let Some(i) = g.idx(v)
            && !seen[i]
        {
            seen[i] = true;
            q.push_back(v);
        }
    };
    while let Some(v) = q.pop_front() {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (v.x + dx, v.z + dz);
            for dy in 1..=4 {
                let ny = v.y + dy;
                if !clear(w, v.x, v.y, v.z, dy + 4) {
                    break;
                }
                if standable(w, nx, ny, nz) {
                    push(&mut seen, &mut q, IVec3::new(nx, ny, nz));
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
                push(&mut seen, &mut q, IVec3::new(nx, ny, nz));
            }
        }
    }
    seen
}

fn spawn_node(w: &VoxelWorld, s: Vec3) -> Option<IVec3> {
    let v = (s / 0.5).floor().as_ivec3();
    [(0, 0), (-1, 0), (0, -1), (-1, -1)].iter().map(|&(dx, dz)| IVec3::new(v.x + dx, v.y, v.z + dz)).find(|n| standable(w, n.x, n.y, n.z))
}

#[test]
fn deterministic_per_seed() {
    let (a, ia) = generate_casino(3);
    let (b, ib) = generate_casino(3);
    assert_eq!(a.content_hash(), b.content_hash());
    assert_eq!(ia.spawns, ib.spawns);
    assert_eq!(ia.routes, ib.routes);
    assert_eq!(ia.interactables, ib.interactables);
    let (c, _) = generate_casino(4);
    assert_ne!(a.content_hash(), c.content_hash(), "seeds should vary the details");
}

#[test]
fn spawns_valid_spread_and_mutually_reachable() {
    for seed in [1u64, 2] {
        let (w, info) = generate_casino(seed);
        assert!((8..=10).contains(&info.spawns.len()), "seed {seed}: {} spawns", info.spawns.len());
        for (i, s) in info.spawns.iter().enumerate() {
            let v = (*s / 0.5).floor().as_ivec3();
            assert_eq!(w.get(v), AIR, "spawn {i} {s} inside a solid");
            assert_eq!(w.get(v + IVec3::Y * 3), AIR, "spawn {i} {s} without head room");
            assert_ne!(w.get(v - IVec3::Y), AIR, "spawn {i} {s} floating");
            for (j, t) in info.spawns.iter().enumerate().skip(i + 1) {
                assert!(s.distance(*t) >= 10.0, "spawns {i} and {j} too close");
            }
        }
        // Heights: floor, table, balconies, roof all used.
        for (y, what) in [(FLOOR_Y, "floor"), (casino::FELT_Y, "table"), (casino::MEZZ_Y, "balcony"), (casino::ROOF_Y, "roof")] {
            assert!(info.spawns.iter().filter(|s| (s.y - y).abs() < 0.6).count() >= 2, "seed {seed}: < 2 spawns on the {what}");
        }
        let g = Grid::new(&w);
        let nodes: Vec<IVec3> = info.spawns.iter().map(|s| spawn_node(&w, *s).unwrap_or_else(|| panic!("spawn {s} not standable"))).collect();
        for (i, &from) in nodes.iter().enumerate() {
            let seen = reach(&w, &g, from);
            for (j, &to) in nodes.iter().enumerate() {
                assert!(seen[g.idx(to).unwrap()], "seed {seed}: spawn {j} {:?} not reachable from spawn {i} {:?}", info.spawns[j], info.spawns[i]);
            }
        }
    }
}

#[test]
fn triangle_budget() {
    let (w, _) = generate_casino(1);
    let mut mesher = Mesher::new();
    let mut mesh = ChunkMesh::default();
    let total: usize = w
        .nonempty_chunks()
        .map(|c| {
            mesher.mesh_chunk_into(&w, c, &mut mesh);
            mesh.triangle_count()
        })
        .collect::<Vec<_>>()
        .iter()
        .sum();
    assert!(total <= TRI_BUDGET, "{total} triangles > {TRI_BUDGET}");
}

#[test]
fn destruction_rules() {
    let (w, info) = generate_casino(1);
    let at = |p: Vec3| w.get_at(p);
    // Walls (between windows), columns, the spindle, chandelier hubs, the big slot cabinets: never break.
    assert!(is_indestructible(at(Vec3::new(51.5, 48.5, 0.25))), "E wall frame band");
    assert!(is_indestructible(at(Vec3::new(0.0, 45.0, 36.5))), "N column");
    assert!(is_indestructible(at(Vec3::new(0.0, POCKET_Y + 6.0, 0.0))), "spindle");
    assert!(is_indestructible(at(Vec3::new(CHANDELIER_R, HUB_Y, 0.0))), "chandelier hub");
    for m in &info.interactables.slots {
        let inside = m.reels - m.facing * 1.25;
        assert!(is_indestructible(at(inside)), "big slot cabinet at {inside}");
    }
    // Carpet, felt, glass, chips break.
    assert!(!is_indestructible(at(Vec3::new(40.0, FLOOR_Y - 0.25, 30.0))), "carpet");
    assert!(!is_indestructible(at(Vec3::new(0.0, casino::FELT_Y - 0.25, 25.5))), "felt");
    let window = (0..200).map(|k| Vec3::new(51.75, 44.0, -45.0 + k as f32 * 0.5)).find(|p| material_def(at(*p)).name.contains("glass"));
    assert!(window.is_some(), "no breakable windows on the E facade");
    // No void holes through the hall floor: an indestructible plate under all of it.
    for z in (-44..44).step_by(4) {
        for x in (-50..50).step_by(4) {
            assert!(is_indestructible(at(Vec3::new(x as f32 + 0.25, 36.75, z as f32 + 0.25))), "no base plate under ({x}, {z})");
        }
    }
}

#[test]
fn interactables_are_clear_and_standable() {
    let (w, info) = generate_casino(1);
    let it = &info.interactables;
    assert_eq!(it.slots.len(), 4);
    for (i, m) in it.slots.iter().enumerate() {
        assert_eq!(w.get_at(m.lever), AIR, "slot {i}: lever knob inside a solid");
        // Something solid (the mount plate) inside the trigger for a tether to hook.
        let hookable = (0..27).any(|k| {
            let d = Vec3::new((k % 3) as f32 - 1.0, ((k / 3) % 3) as f32 - 1.0, (k / 9) as f32 - 1.0) * (m.radius * 0.6);
            w.get_at(m.lever + d) != AIR
        });
        assert!(hookable, "slot {i}: nothing to hook near the lever");
        assert_eq!(w.get_at(m.reels + m.facing * 0.25), AIR, "slot {i}: reel window not recessed");
        assert_ne!(w.get_at(m.reels - m.facing * 0.25), AIR, "slot {i}: no reel backing");
        // The pull spot in front of the machine is standable.
        let front = m.reels + m.facing * 3.0;
        let feet = (0..8).map(|k| front - Vec3::Y * (1.0 + k as f32 * 0.5)).find(|p| w.get_at(*p) != AIR);
        assert!(feet.is_some(), "slot {i}: nothing to stand on in front");
    }
    let wheel = it.roulette.as_ref().expect("roulette");
    assert_eq!(wheel.pockets(), 38);
    for k in 0..wheel.pockets() {
        let a = wheel.pocket_angle(k);
        let p = Vec3::new(a.cos() * 13.0, POCKET_Y + 0.1, a.sin() * 13.0);
        assert_eq!(wheel.pocket_at(p), Some(k));
        assert!(wheel.in_region(p, k, 0), "pocket {k}: feet height outside the band");
        assert_ne!(w.get_at(p - Vec3::Y * 0.3), AIR, "pocket {k}: no floor");
        assert_eq!(w.get_at(p + Vec3::Y * 0.5), AIR, "pocket {k}: blocked");
        // Colour matches the label.
        let m = w.get_at(Vec3::new(a.cos() * 10.4, POCKET_Y - 0.25, a.sin() * 10.4));
        let want = match wheel.colors[k] {
            0 => mat::ROULETTE_GREEN,
            1 => mat::ROULETTE_RED,
            _ => mat::ROULETTE_BLACK,
        };
        assert!(m == want || m == mat::CHIP_WHITE, "pocket {k} ({}): floor {}", wheel.labels[k], material_def(m).name);
    }
}

#[test]
fn surf_halo_floats_clear() {
    let (w, info) = generate_casino(1);
    assert_eq!(info.surf_lines.len(), casino::HALO_SEGMENTS);
    // One descending turn: the first segment starts high and out, the last ends low and in.
    let (first, last) = (info.surf_lines[0], info.surf_lines[casino::HALO_SEGMENTS - 1]);
    assert!(first.a.y - last.b.y > casino::HALO_DROP - 0.1, "halo doesn't descend");
    for (k, l) in info.surf_lines.iter().enumerate() {
        let mid = (l.a + l.b) * 0.5;
        assert!(l.a.y >= l.b.y, "segment {k} climbs");
        assert!((45.6..=80.0).contains(&l.face_deg), "halo faces must be surfable");
        let (sn, cs) = l.face_deg.to_radians().sin_cos();
        // The prism's volume is air (hard light over the wheel, never inside voxels).
        let out = Vec3::new(mid.x, 0.0, mid.z).normalize();
        for s in [-1.0f32, 1.0] {
            for j in 0..=6 {
                let t = j as f32 / 6.0 * l.width;
                let p = mid + out * (s * t * cs) - Vec3::Y * (t * sn + 0.4);
                assert_eq!(w.get_at(p), AIR, "halo segment {k} intersects terrain at {p}");
            }
        }
        // Walkable clearance under it: whatever is below + a standing hull (1.8 m) stays under the
        // faces' lowest edge (incl. the 1.5 m collider thickness).
        let bottom = mid.y - l.width * sn - 1.5 * cs;
        for dr in [-l.width * cs, 0.0, l.width * cs] {
            let p = Vec3::new(mid.x, 0.0, mid.z) + out * dr;
            let top = (0..60).map(|j| mid.y - 1.0 - j as f32 * 0.5).find(|y| w.get_at(Vec3::new(p.x, *y, p.z)) != AIR).expect("floor under the halo");
            assert!(top + 0.5 + 1.8 < bottom + 0.2, "segment {k}: no head room at {p}: floor {top} vs halo {bottom}");
        }
    }
}

#[test]
fn routes_clear_and_anchors_solid() {
    let (w, info) = generate_casino(1);
    let bhop = info.route("floor_bhop").expect("floor bhop");
    let (a, b) = (bhop.waypoints[0], bhop.waypoints[1]);
    assert!(a.distance(b) > 80.0);
    for k in 0..=240 {
        let p = a.lerp(b, k as f32 / 240.0);
        for dz in [-1.5f32, 0.0, 1.5] {
            for dy in 0..6 {
                let q = p + Vec3::new(0.0, 0.1 + dy as f32 * 0.5, dz);
                assert_eq!(w.get_at(q), AIR, "bhop lane blocked at {q}");
            }
        }
    }
    let swing = info.route("chandelier_swing").expect("chandelier swing");
    for an in &swing.anchors {
        assert!(is_indestructible(w.get_at(*an)), "chandelier hub at {an} not an indestructible anchor");
    }
    assert!(info.route("roulette_surf").is_some());
}
