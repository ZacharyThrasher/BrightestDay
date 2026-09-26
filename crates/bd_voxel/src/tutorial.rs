//! The movement tutorial: a compact loop of floating platforms whose stations each teach one
//! speed technique, then chain into a timed "peak run".
//!
//! Course (metres, feet heights; +X east, Y up). Stations run in a clockwise loop seen from above:
//!
//! | # | station          | geometry                                                                   |
//! |---|------------------|----------------------------------------------------------------------------|
//! | 1 | Bunny hop        | hub (start/finish ring) → 62 m runway at 40 m                              |
//! | 2 | Air-strafe corner| runway ends over void; the next lane starts 8 m right and 6 m lower: turn in the air |
//! | 3 | Slide            | 20 m flat, 24 m of 1-voxel stairs down 12 m, 28 m flat run-out at 22 m     |
//! | 4 | Tether swing     | 30 m gap under a stone arch (underside ≈ 42 m); hook it, swing, release    |
//! | 5 | Climb            | 32 m cliff; pillars + lintel (71 m) behind the lip: hook both, reel up     |
//! | 6 | Fling            | ledge edge → big gap west, one floating rock anchor; needs swing + boost    |
//! | 7 | Peak run         | everything above, then a 116 m return lane back through the ring           |
//!
//! Station data (starts, goals, gates, suggested anchors and racing line) is in [`TutorialInfo`]. Bot
//! ghosts proving every station is completable with the real movement code live in
//! `crates/bd_sim/examples/tutorial_ghosts.rs` (writes `assets/tutorial/ghosts/*.ron`).

use bd_core::VOXEL_SIZE;
use bd_core::voxel::{AIR, Aabb, Material};
use glam::{IVec3, Vec2, Vec3};

use crate::arena::{Gen, IslandSpec, Surface, vx};
use crate::material::mat;
use crate::noise::{PI, fbm2};
use crate::world::VoxelWorld;

/// One tutorial station.
#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    pub name: &'static str,
    /// 3–5 word on-screen hint.
    pub hint: &'static str,
    /// Feet position to spawn at (standing still) when practising this station alone.
    pub start: Vec3,
    /// View yaw at the start (bd_core convention: 0 looks down −Z, +π/2 looks down −X).
    pub facing_yaw: f32,
    /// Checkpoint spheres (centre, radius) to fly through, in order.
    pub gates: Vec<(Vec3, f32)>,
    /// The station is complete when the player's feet are inside this box.
    pub goal: Aabb,
    /// Suggested tether anchor points, in order of use (for guide markers).
    pub anchors: Vec<Vec3>,
    /// Suggested racing line (feet-height waypoints) for guide arrows and bots.
    pub route: Vec<Vec3>,
}

/// Peak-run medal times in seconds (start ring → finish ring).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Medals {
    pub gold: f32,
    pub silver: f32,
    pub bronze: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TutorialInfo {
    /// Stations 1–6 in order. The peak run is `stations` chained plus the return lane.
    pub stations: Vec<Station>,
    /// Peak run: spawn position and yaw (on the hub, behind the start ring).
    pub peak_start: Vec3,
    pub peak_yaw: f32,
    /// Peak run checkpoints in order; the last one is the finish ring (= the start ring).
    pub peak_gates: Vec<(Vec3, f32)>,
    /// Suggested racing line for the whole peak run.
    pub peak_route: Vec<Vec3>,
    /// The start/finish ring (centre, radius). The timer starts and stops crossing it heading +X.
    pub start_finish: (Vec3, f32),
    /// Medal times, set from the bot ghost (gold ≈ bot × 1.05, silver × 1.25, bronze × 1.6).
    pub medals: Medals,
    pub void_y: f32,
    pub bounds: Aabb,
}

/// Peak-run bot time in seconds, measured by `crates/bd_sim/examples/tutorial_ghosts.rs` with the
/// default `MoveParams`. Re-run that example (and update this) after changing the course or tuning.
pub const PEAK_BOT_TIME: f32 = 22.31;

// ----- course geometry (metres) -----------------------------------------------------------------
const H_HUB: f32 = 40.0;
const H_LANE: f32 = 34.0;
const H_LOW: f32 = 22.0;
const H_LEDGE: f32 = 54.0;
const H_LAND6: f32 = 40.0;
/// East edge of the station-6 landing platform (the fling gap is 12 − this wide).
const LAND6_EAST: f32 = -32.0;

const YAW_POS_X: f32 = -PI * 0.5;
const YAW_NEG_X: f32 = PI * 0.5;
const YAW_POS_Z: f32 = PI;

fn aabb(x0: f32, x1: f32, y0: f32, y1: f32, z0: f32, z1: f32) -> Aabb {
    Aabb { min: Vec3::new(x0, y0, z0), max: Vec3::new(x1, y1, z1) }
}

/// Rectangular platform with an exact, crisp walkable top (`top(x, z)`), grass/dirt/stone layers and
/// a jagged rocky keel underneath. Cores are placed at `cores` (x, z) points.
fn slab(g: &mut Gen, x0: f32, x1: f32, z0: f32, z1: f32, top: impl Fn(f32, f32) -> f32, depth: f32, cores: &[Vec2], crystals: Material) {
    let seed = g.seed();
    let half = ((x1 - x0).min(z1 - z0) * 0.5).max(1.0);
    for vz in vx(z0)..vx(z1) {
        for vxx in vx(x0)..vx(x1) {
            let (px, pz) = ((vxx as f32 + 0.5) * VOXEL_SIZE, (vz as f32 + 0.5) * VOXEL_SIZE);
            let t = top(px, pz);
            let ty = (t / VOXEL_SIZE).round() as i32;
            let edge = (px - x0).min(x1 - px).min(pz - z0).min(z1 - pz);
            let k = (edge / half).clamp(0.0, 1.0);
            let jag = 0.5 + 0.5 * fbm2(px * 0.11, pz * 0.11, seed, 2);
            let d = 1.5 + depth * k.sqrt() * (0.7 + 0.6 * jag);
            let by = vx(t - d);
            g.w.fill_column(vxx, vz, by, ty - 3, mat::STONE);
            g.w.fill_column(vxx, vz, ty - 3, ty - 1, mat::DIRT);
            g.w.fill_column(vxx, vz, ty - 1, ty, mat::GRASS);
            if fbm2(px * 0.16, pz * 0.16, seed ^ 4, 2) > 0.42 && k > 0.2 {
                g.w.fill_column(vxx, vz, by - 2, by + 2, crystals);
            }
        }
    }
    for c in cores {
        let t = top(c.x, c.y);
        let r = (depth * 0.15).clamp(0.75, 2.0);
        g.cores.push((Vec3::new(c.x, t - 1.5 - r, c.y), r));
    }
}

/// Box of voxels covering world-space `[lo, hi)`.
fn block(g: &mut Gen, lo: Vec3, hi: Vec3, m: Material) {
    let a = (lo / VOXEL_SIZE).round().as_ivec3();
    let b = (hi / VOXEL_SIZE).round().as_ivec3() - IVec3::ONE;
    g.w.fill_box(a, b, m);
}

/// Stone pillar with deep-stone bands (reads as climbable/anchorable).
fn pillar(g: &mut Gen, lo: Vec3, hi: Vec3) {
    let a = (lo / VOXEL_SIZE).round().as_ivec3();
    let b = (hi / VOXEL_SIZE).round().as_ivec3() - IVec3::ONE;
    g.w.fill_with(a, b, |v, _| Some(if (v.y / 4).rem_euclid(3) == 0 { mat::DEEP_STONE } else { mat::STONE }));
}

/// Vertical ring (torus in the YZ plane, normal X) of gold crystal: the start/finish gate.
fn gate_ring(g: &mut Gen, c: Vec3, major: f32, minor: f32) {
    let r = major + minor + 1.0;
    let lo = ((c - Vec3::new(minor + 1.0, r, r)) / VOXEL_SIZE).floor().as_ivec3();
    let hi = ((c + Vec3::new(minor + 1.0, r, r)) / VOXEL_SIZE).floor().as_ivec3();
    g.w.fill_with(lo, hi, |v, old| {
        let q = crate::arena::vcenter(v) - c;
        let rin = (q.y * q.y + q.z * q.z).sqrt() - major;
        let d = (rin * rin + q.x * q.x).sqrt();
        (d <= minor && old == AIR).then_some(mat::CRYSTAL_GOLD)
    });
}

/// Lowest solid point above `(x, from_y, z)` (the underside a hook fired from below would hit).
fn underside(w: &VoxelWorld, x: f32, z: f32, from_y: f32) -> Vec3 {
    let (cx, cz) = (vx(x), vx(z));
    let top = w.voxel_max().y;
    let y = (vx(from_y)..top).find(|&y| w.get(IVec3::new(cx, y, cz)) != AIR).unwrap_or(top);
    Vec3::new(x, y as f32 * VOXEL_SIZE, z)
}

/// Build the tutorial course.
pub fn generate_tutorial() -> (VoxelWorld, TutorialInfo) {
    let mut g = Gen::new(VoxelWorld::new(IVec3::new(-160, 0, -192), IVec3::new(288, 192, 400)), 0x7u64);

    // ---- 1: hub + bunny-hop runway ---------------------------------------------------------------
    slab(&mut g, -62.0, -42.0, -80.0, -60.0, |_, _| H_HUB, 12.0, &[Vec2::new(-52.0, -70.0)], mat::CRYSTAL_GOLD);
    slab(&mut g, -42.0, 20.0, -75.0, -65.0, |_, _| H_HUB, 7.0, &[Vec2::new(-20.0, -70.0), Vec2::new(8.0, -70.0)], mat::CRYSTAL_CYAN);
    let ring_c = Vec3::new(-42.0, H_HUB + 5.0, -70.0);
    gate_ring(&mut g, ring_c, 5.5, 0.6);

    // ---- 2 + 3: corner gap, then lane → stairs → run-out ----------------------------------------
    let stairs_top = -42.0;
    let stairs_bot = -18.0;
    let lane_top = move |_: f32, z: f32| -> f32 {
        if z < stairs_top {
            H_LANE
        } else if z < stairs_bot {
            H_LANE - 0.5 * ((z - stairs_top).floor() + 1.0)
        } else {
            H_LOW
        }
    };
    slab(
        &mut g,
        28.0,
        42.0,
        -62.0,
        10.0,
        lane_top,
        8.0,
        &[Vec2::new(35.0, -52.0), Vec2::new(35.0, -28.0), Vec2::new(35.0, 0.0)],
        mat::CRYSTAL_VIOLET,
    );

    // ---- 4: swing gap under an arch -------------------------------------------------------------
    let arch_z = 25.0;
    for x in [18.0f32, 52.0] {
        g.island(IslandSpec {
            depth: 6.0,
            hills: 0.2,
            surface: Surface::Rocky,
            crystals: mat::CRYSTAL_CYAN,
            ..IslandSpec::new(Vec2::new(x, arch_z), 20.0, 4.5)
        });
    }
    g.arch(Vec3::new(18.0, 17.5, arch_z), Vec3::new(52.0, 17.5, arch_z), 26.0, 1.6);
    slab(&mut g, 24.0, 46.0, 40.0, 67.0, |_, _| H_LOW, 10.0, &[Vec2::new(35.0, 52.0)], mat::CRYSTAL_CYAN);

    // ---- 5: climb frame (two pillars + lintel) and the high ledge -------------------------------
    slab(&mut g, 12.0, 46.0, 67.0, 88.0, |_, _| H_LEDGE, 12.0, &[Vec2::new(30.0, 78.0)], mat::CRYSTAL_GOLD);
    // Sheer cliff face from the lower platform up to the ledge lip: reeling slides you up it.
    pillar(&mut g, Vec3::new(12.0, 14.0, 67.0), Vec3::new(46.0, H_LEDGE - 1.0, 71.0));
    pillar(&mut g, Vec3::new(25.0, H_LEDGE - 2.0, 70.0), Vec3::new(28.0, 74.0, 73.0));
    pillar(&mut g, Vec3::new(42.0, H_LEDGE - 2.0, 70.0), Vec3::new(45.0, 74.0, 73.0));
    block(&mut g, Vec3::new(28.0, 71.0, 70.0), Vec3::new(42.0, 74.0, 73.0), mat::RUIN_BRICK);
    g.spire(43.0, 85.0, H_LEDGE + 4.0, 12.0, 1.3, Some(mat::CRYSTAL_GOLD));

    // ---- 6: fling anchor rock + landing ---------------------------------------------------------
    let rock = Vec2::new(-6.0, 78.0);
    g.island(IslandSpec {
        depth: 8.0,
        hills: 0.2,
        surface: Surface::Rocky,
        crystals: mat::CRYSTAL_VIOLET,
        ..IslandSpec::new(rock, 81.0, 4.5)
    });
    slab(&mut g, -66.0, LAND6_EAST, 56.0, 90.0, |_, _| H_LAND6, 12.0, &[Vec2::new(-46.0, 74.0)], mat::CRYSTAL_VIOLET);

    // ---- 7: return lane back to the hub ---------------------------------------------------------
    slab(
        &mut g,
        -57.0,
        -47.0,
        -60.0,
        56.0,
        |_, _| H_HUB,
        7.0,
        &[Vec2::new(-52.0, 40.0), Vec2::new(-52.0, 0.0), Vec2::new(-52.0, -40.0)],
        mat::CRYSTAL_CYAN,
    );
    // Side islets with trees along the return lane (anchors + scenery), kept clear of the lane.
    for (x, z, leaves) in [(-68.0, 30.0, mat::LEAVES_ORANGE), (-36.0, -12.0, mat::LEAVES_PINK), (-68.0, -40.0, mat::LEAVES_GOLD)] {
        g.island(IslandSpec { depth: 7.0, hills: 0.3, ..IslandSpec::new(Vec2::new(x, z), H_HUB - 3.0, 6.0) });
        g.tree(x, z, H_HUB + 4.0, 14.0, 0.9, leaves, 4.0);
    }
    g.tree(-59.0, -78.0, H_HUB + 4.0, 11.0, 0.8, mat::LEAVES_GREEN, 3.5);
    g.tree(-40.0, 88.0, H_LAND6 + 4.0, 12.0, 0.8, mat::LEAVES_RED, 3.8);

    g.place_cores();
    let mut w = g.w;
    w.prune_unanchored();
    w.mark_all_dirty();

    // ---- station data (anchors measured from the built world) -----------------------------------
    let arch_anchor = underside(&w, 35.0, arch_z, H_LOW);
    let lintel_l = underside(&w, 31.0, 71.5, H_LEDGE + 2.0);
    let lintel_r = underside(&w, 39.0, 71.5, H_LEDGE + 2.0);
    let rock_anchor = underside(&w, rock.x, rock.y, H_LEDGE);

    let stations = vec![
        Station {
            name: "bunny_hop",
            hint: "Hold jump. Strafe + turn.",
            start: Vec3::new(-54.0, H_HUB, -70.0),
            facing_yaw: YAW_POS_X,
            gates: vec![(ring_c, 5.0), (Vec3::new(-10.0, H_HUB + 1.5, -70.0), 5.0)],
            goal: aabb(8.0, 20.0, H_HUB - 0.1, H_HUB + 4.0, -75.0, -65.0),
            anchors: vec![],
            route: vec![Vec3::new(-54.0, H_HUB, -70.0), Vec3::new(18.0, H_HUB, -70.0)],
        },
        Station {
            name: "air_strafe",
            hint: "Jump the gap. Strafe right.",
            start: Vec3::new(-24.0, H_HUB, -70.0),
            facing_yaw: YAW_POS_X,
            gates: vec![(Vec3::new(25.0, H_LANE + 3.0, -63.0), 7.0)],
            goal: aabb(28.0, 42.0, H_LANE - 0.1, H_LANE + 3.0, -62.0, -44.0),
            anchors: vec![],
            route: vec![Vec3::new(18.0, H_HUB, -70.0), Vec3::new(35.0, H_LANE, -54.0)],
        },
        Station {
            name: "slide",
            hint: "Crouch at speed. Slide down.",
            start: Vec3::new(35.0, H_LANE, -60.0),
            facing_yaw: YAW_POS_Z,
            gates: vec![(Vec3::new(35.0, H_LOW + 7.0, -30.0), 5.0)],
            goal: aabb(28.0, 42.0, H_LOW - 0.1, H_LOW + 3.0, -2.0, 10.0),
            anchors: vec![],
            route: vec![Vec3::new(35.0, H_LANE, -60.0), Vec3::new(35.0, H_LOW, 10.0)],
        },
        Station {
            name: "tether_swing",
            hint: "Hook the arch. Release up.",
            start: Vec3::new(35.0, H_LOW, -12.0),
            facing_yaw: YAW_POS_Z,
            gates: vec![(Vec3::new(35.0, arch_anchor.y - 18.0, arch_z), 8.0)],
            goal: aabb(24.0, 46.0, H_LOW - 0.1, H_LOW + 3.0, 40.0, 64.0),
            anchors: vec![arch_anchor],
            route: vec![Vec3::new(35.0, H_LOW, 10.0), Vec3::new(35.0, H_LOW, 48.0)],
        },
        Station {
            name: "climb",
            hint: "Both hooks high. Reel in.",
            start: Vec3::new(35.0, H_LOW, 58.0),
            facing_yaw: YAW_POS_Z,
            gates: vec![(Vec3::new(35.0, H_LEDGE + 2.0, 67.0), 5.0)],
            goal: aabb(12.0, 46.0, H_LEDGE - 0.1, H_LEDGE + 3.0, 67.0, 88.0),
            anchors: vec![lintel_l, lintel_r],
            route: vec![Vec3::new(35.0, H_LOW, 58.0), Vec3::new(35.0, H_LEDGE, 76.0)],
        },
        Station {
            name: "fling",
            hint: "Boost the swing. Release.",
            start: Vec3::new(42.0, H_LEDGE, 78.0),
            facing_yaw: YAW_NEG_X,
            gates: vec![(Vec3::new(rock.x, rock_anchor.y - 16.0, rock.y), 10.0)],
            goal: aabb(-66.0, LAND6_EAST, H_LAND6 - 0.1, H_LAND6 + 3.0, 56.0, 90.0),
            anchors: vec![rock_anchor],
            route: vec![Vec3::new(42.0, H_LEDGE, 78.0), Vec3::new(12.0, H_LEDGE, 78.0), Vec3::new(-34.0, H_LAND6, 78.0)],
        },
    ];

    let mut peak_gates: Vec<(Vec3, f32)> = stations.iter().flat_map(|s| s.gates.iter().copied()).collect();
    peak_gates.remove(0); // the ring is the finish, not a mid-run gate
    peak_gates.push((Vec3::new(-52.0, H_HUB + 2.0, 0.0), 6.0));
    peak_gates.push((ring_c, 5.0));
    let mut peak_route: Vec<Vec3> = stations.iter().flat_map(|s| s.route.iter().copied()).collect();
    peak_route.extend([Vec3::new(-56.0, H_HUB, 64.0), Vec3::new(-52.0, H_HUB, -56.0), Vec3::new(-40.0, H_HUB, -70.0)]);

    let b = w.bounds();
    let info = TutorialInfo {
        stations,
        peak_start: Vec3::new(-56.0, H_HUB, -70.0),
        peak_yaw: YAW_POS_X,
        peak_gates,
        peak_route,
        start_finish: (ring_c, 5.0),
        medals: medals_from_bot(PEAK_BOT_TIME),
        void_y: 0.0,
        bounds: Aabb { min: Vec3::new(b.min.x, -4.0, b.min.z), max: Vec3::new(b.max.x, b.max.y + 30.0, b.max.z) },
    };
    (w, info)
}

/// Medal times from a bot time: gold × 1.05, silver × 1.25, bronze × 1.6.
pub fn medals_from_bot(bot: f32) -> Medals {
    Medals { gold: bot * 1.05, silver: bot * 1.25, bronze: bot * 1.6 }
}
