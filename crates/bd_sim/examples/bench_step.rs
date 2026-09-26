//! Micro-benchmark: cost of one `step_player` tick in typical situations, and of a 30-tick
//! prediction replay. Run with `--release` for representative numbers:
//!
//!     cargo run -p bd_sim --release --example bench_step

use std::hint::black_box;
use std::time::Instant;

use bd_core::{DT, IVec3, Quat, Vec3};
use bd_sim::movement::testkit::scenarios::{arena, lattice_anchor};
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;

fn bench<V: bd_core::voxel::VoxelQuery + ?Sized>(
    name: &str,
    cw: &CollisionWorld<V>,
    p: &MoveParams,
    start: PlayerState,
    script: impl Fn(usize, &PlayerState) -> bd_core::input::PlayerInput,
    episode: usize,
) {
    let total = 300_000usize;
    // Pre-record inputs per episode tick from one run so the timed loop measures only step_player.
    let mut inputs = Vec::with_capacity(episode);
    let mut s = start;
    for t in 0..episode {
        let i = script(t, &s);
        step_player(&mut s, &i, cw, p, DT);
        inputs.push(i);
    }
    let t0 = Instant::now();
    let mut s = start;
    for k in 0..total {
        let t = k % episode;
        if t == 0 {
            s = start;
        }
        black_box(step_player(&mut s, black_box(&inputs[t]), cw, p, DT));
    }
    let ns = t0.elapsed().as_nanos() as f64 / total as f64;
    println!("{name:<48} {ns:>8.0} ns/tick   30-tick replay {:>6.1} µs", ns * 30.0 / 1000.0);
}

fn main() {
    let p = MoveParams::default();

    // Dense grid world (like the real chunk store: an array lookup per voxel).
    let mut g = TestGrid::new(IVec3::new(-200, -4, -200), IVec3::new(200, 40, 200));
    g.fill(IVec3::new(-200, -2, -200), IVec3::new(199, -1, 199), 1);
    // Bumpy terrain: 1-voxel steps in a checker of 4×4 patches.
    for x in (-200..200).step_by(8) {
        for z in (-200..200).step_by(8) {
            g.fill(IVec3::new(x, 0, z), IVec3::new(x + 3, 0, z + 3), 1);
        }
    }
    let cw = CollisionWorld::new(&g, &[]);
    let mut start = PlayerState::new(Vec3::new(0.0, 1.0, 0.0), &p);
    for _ in 0..30 {
        step_player(&mut start, &input(0.0, 0.0, 0, 0, 0), &cw, &p, DT);
    }
    start.vel = Vec3::new(12.0, 0.0, 3.0);
    bench("bhop + strafe over bumpy voxel terrain", &cw, &p, start, |_, s| {
        input(optimal_strafe_yaw(s.vel, &p, DT, 1, 0, 1.0), 0.0, 1, 0, JUMP)
    }, 600);
    bench("run over bumpy voxel terrain (step up/down)", &cw, &p, start, |_, _| input(-1.3, 0.0, 0, 1, 0), 600);

    // Tether swing in the lattice arena.
    let w = arena();
    let cwa = CollisionWorld::new(&w, &[]);
    let mut s0 = PlayerState::new(Vec3::new(-22.0, 0.01, 1.0), &p);
    for _ in 0..5 {
        step_player(&mut s0, &input(0.0, 0.0, 0, 0, 0), &cwa, &p, DT);
    }
    s0.vel = Vec3::new(14.0, 0.0, 0.0);
    let anchor = lattice_anchor(Vec3::ZERO);
    bench("hop → hook → swing (rope + hook flight)", &cwa, &p, s0, |t, s| {
        if t < 3 {
            input(-1.57, 0.0, 0, 0, JUMP)
        } else if !s.tethered() {
            let (y, pi) = aim_at(s.eye(&p), anchor);
            input(y, pi, 0, 0, TETHER_L)
        } else {
            scenarios_swing(s, &p)
        }
    }, 240);

    // 60 construct boxes scattered around + 8 player hulls (most far away).
    let mut cols = Vec::new();
    for i in 0..60 {
        let a = i as f32 * 0.7;
        cols.push(Collider::new(
            Vec3::new(a.cos() * (5.0 + i as f32), 1.0, a.sin() * (5.0 + i as f32)),
            Vec3::new(1.0, 1.0, 0.25),
            Quat::from_rotation_y(a),
            ColliderId::Other(i),
        ));
    }
    let cwc = CollisionWorld::new(&g, &cols);
    bench("bhop among 60 construct OBBs", &cwc, &p, start, |_, s| {
        input(optimal_strafe_yaw(s.vel, &p, DT, 1, 0, 1.0), 0.0, 1, 0, JUMP)
    }, 600);

    // Crosshair query (client, once per rendered frame).
    let t0 = Instant::now();
    let n = 100_000;
    for k in 0..n {
        let a = (k % 360) as f32 * 0.0174;
        black_box(query_anchor(&cwa, Vec3::new(-10.0, 2.0, 1.0), Vec3::new(a.cos(), 0.6, a.sin()).normalize(), &p));
    }
    println!("{:<48} {:>8.0} ns/query", "query_anchor (crosshair, 120 m)", t0.elapsed().as_nanos() as f64 / n as f64);
}

fn scenarios_swing(s: &PlayerState, p: &MoveParams) -> bd_core::input::PlayerInput {
    bd_sim::movement::testkit::scenarios::swing_input(s, p, TETHER_L, s.gas > 50.0)
}
