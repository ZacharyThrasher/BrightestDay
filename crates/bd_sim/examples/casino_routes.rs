//! Proves High Roller's movement lines with the real movement code: scripted "skilled bot" inputs
//! drive `bd_sim::movement::step_player` through `bd_voxel` `generate_casino` (+ its surf halo
//! colliders).
//!
//!     cargo run -p bd_sim --features casino --example casino_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **Floor bhop** — the 96 m aisle along the N balcony edge, bunny-hopping (must beat run speed).
//! 2. **Chandelier swing** — N balcony → S balcony across the hall over the roulette: jump the rail,
//!    tether the N chandelier, swing, release, tether the S chandelier, swing, land on the S balcony
//!    without touching the floor.
//! 3. **Roulette surf lap** — onto the surf halo from a chip-stack step on the felt, then one full
//!    lap around the wheel on the halo's inner face (strafe held into the face).
//!
//! Exits non-zero if a route fails. `TRACE=<route number>` prints the bot's state every 0.1 s.

use bd_core::DT;
use bd_core::Vec3;
use bd_core::input::PlayerInput;
use bd_core::voxel::VoxelQuery;
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;
use bd_voxel::MapInfo;
use bd_voxel::VoxelWorld;
use bd_voxel::maps::casino::{self, generate_casino};

fn horiz(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = horiz(to - from).normalize_or(Vec3::NEG_Z);
    (-d.x).atan2(-d.z)
}

/// Bunny-hop / strafe controller that steers toward a target (pursuit + zig-zag air-strafe).
struct Hopper {
    p: MoveParams,
    side: f32,
}

impl Hopper {
    fn new() -> Self {
        Self { p: MoveParams::default(), side: 1.0 }
    }
    fn hop_to(&mut self, s: &PlayerState, target: Vec3, lim_deg: f32) -> PlayerInput {
        let dir = horiz(target - s.pos).normalize_or(Vec3::Z);
        if s.horizontal_speed() < self.p.run_speed * 0.95 && s.grounded {
            return input(yaw_to(s.pos, target), 0.0, 0, 1, 0);
        }
        let h = horiz(s.vel);
        if h.length_squared() > 1e-4 {
            let e = dir.cross(h).y.atan2(dir.dot(h));
            let lim = lim_deg.to_radians();
            if e > lim {
                self.side = 1.0;
            } else if e < -lim {
                self.side = -1.0;
            }
        }
        let mx: i8 = if self.side > 0.0 { 1 } else { -1 };
        let err = dir.cross(h).y.atan2(dir.dot(h));
        if err.abs() > 25f32.to_radians() && !s.grounded {
            let perp = Vec3::new(-h.z, 0.0, h.x).normalize() * if err > 0.0 { 1.0 } else { -1.0 };
            return input(yaw_for_wish(perp, mx, 0), 0.0, mx, 0, JUMP);
        }
        input(optimal_strafe_yaw(s.vel, &self.p, DT, mx, 0, self.side), 0.0, mx, 0, JUMP)
    }
}

#[derive(Default, Debug)]
struct RunResult {
    ok: bool,
    secs: f32,
    peak: f32,
    peak_y: f32,
    avg: f32,
    hooks: u32,
    touchdowns: u32,
    fell: bool,
    end: Vec3,
    note: String,
}

#[allow(clippy::too_many_arguments)]
fn run(
    world: &VoxelWorld,
    info: &MapInfo,
    colliders: &[Collider],
    route: usize,
    mut s: PlayerState,
    max_secs: f32,
    mut ctrl: impl FnMut(&PlayerState, usize) -> PlayerInput,
    mut goal: impl FnMut(&PlayerState, usize) -> bool,
) -> RunResult {
    let cw = CollisionWorld::new(world, colliders);
    let p = MoveParams::default();
    let trace = std::env::var("TRACE").is_ok_and(|v| v == route.to_string());
    let mut r = RunResult { peak_y: s.pos.y, ..Default::default() };
    let start_y = s.pos.y;
    let mut dist = 0.0;
    let mut hooked = false;
    for t in 0..(max_secs / DT) as usize {
        let mut i = ctrl(&s, t);
        i.seq = t as u32;
        let prev = s;
        let e = step_player(&mut s, &i, &cw, &p, DT);
        dist += horiz(s.pos - prev.pos).length();
        r.peak = r.peak.max(s.horizontal_speed());
        r.peak_y = r.peak_y.max(s.pos.y);
        if e.tether_attached.iter().any(|&a| a) {
            r.hooks += 1;
            hooked = true;
        }
        if hooked && s.grounded && !prev.grounded {
            r.touchdowns += 1;
        }
        if trace && t % std::env::var("TRACE_EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(12usize) == 0 {
            println!(
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) |h|={:4.1} gr={} surf={} hooks={:?}/{:?} btn={:#x}",
                t as f32 * DT,
                s.pos.x,
                s.pos.y,
                s.pos.z,
                s.vel.x,
                s.vel.y,
                s.vel.z,
                s.horizontal_speed(),
                s.grounded as u8,
                (s.extra.surf_n != Vec3::ZERO) as u8,
                s.hooks[0].phase,
                s.hooks[1].phase,
                i.buttons
            );
        }
        r.secs = (t + 1) as f32 * DT;
        if s.pos.y < info.void_y {
            r.fell = true;
            break;
        }
        if goal(&s, t) {
            r.ok = true;
            break;
        }
    }
    r.avg = dist / r.secs.max(1e-3);
    r.peak_y -= start_y;
    r.end = s.pos;
    r
}

fn crosshair<V: VoxelQuery>(w: &V, eye: Vec3, target: Vec3) -> Option<Vec3> {
    bd_core::voxel::raycast(w, eye, target - eye, 120.0).map(|h| h.point)
}

/// 1. The floor aisle, bunny-hopped end to end.
fn floor_bhop(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("floor_bhop").expect("floor_bhop");
    let (a, end) = (r.waypoints[0], r.waypoints[1]);
    let p = MoveParams::default();
    let s = PlayerState::new(a, &p);
    let mut hop = Hopper::new();
    let dir = horiz(end - a).normalize();
    run(
        w,
        m,
        &[],
        1,
        s,
        30.0,
        move |s, _| {
            let along = (s.pos - a).dot(dir);
            let aim = a + dir * (along + 20.0);
            hop.hop_to(s, Vec3::new(aim.x, s.pos.y, aim.z), 8.0)
        },
        move |s, _| (s.pos - a).dot(dir) >= (end - a).dot(dir) - 2.0 && (s.pos.y - a.y).abs() < 1.5,
    )
}

/// 2. N balcony → two chandeliers → S balcony, never touching the floor.
fn chandelier_swing(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("chandelier_swing").expect("chandelier_swing");
    let (start, goal_p) = (r.waypoints[0], r.waypoints[1]);
    let hubs = [r.anchors[0], r.anchors[1]];
    let p = MoveParams::default();
    let s = PlayerState::new(start, &p);
    let mut phase = 0u8;
    let mut anchor = hubs[0];
    let mut idle = false;
    let mut res = run(
        w,
        m,
        &[],
        2,
        s,
        25.0,
        move |s, _| {
            let eye = s.eye(&p);
            let south = Vec3::new(start.x, s.pos.y, goal_p.z);
            match phase {
                // Sprint south, jump the rail at the balcony edge.
                0 => {
                    if !s.grounded && s.pos.z < 37.0 {
                        phase = 1;
                        anchor = crosshair(w, eye, hubs[0] + Vec3::Y * 0.4).unwrap_or(hubs[0]);
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 1, TETHER_L);
                    }
                    let mut i = input(yaw_to(s.pos, south), 0.0, 0, 1, 0);
                    if s.grounded && s.pos.z < 38.2 {
                        i.buttons |= JUMP;
                    }
                    i
                }
                // Swing on the first chandelier; release past it, rising.
                1 | 3 => {
                    let k = if phase == 1 { 0 } else { 1 };
                    let h = s.hooks[0];
                    if !h.attached() {
                        idle = !idle;
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 1, if idle && h.phase == HookPhase::Idle { 0 } else { TETHER_L });
                    }
                    let past = s.pos.z < hubs[k].z - 5.0;
                    if past && s.vel.y > -1.0 {
                        phase += 1;
                        return input(yaw_to(s.pos, south), 0.0, 0, 1, 0);
                    }
                    let (y, pc) = aim_at(eye, h.pos + Vec3::new(0.0, 0.0, -8.0));
                    // Keep the arc's low point off the felt / floor: reel while the rope is long.
                    let low = h.pos.y - h.rope_len;
                    let reel = if low < casino::FELT_Y + 3.0 || s.pos.z > hubs[k].z { REEL } else { 0 };
                    input(y, pc, 0, 1, TETHER_L | reel | if s.vel.y < 0.0 { BOOST } else { 0 })
                }
                // Airborne between chandeliers: fire at the second hub.
                2 => {
                    anchor = crosshair(w, eye, hubs[1] + Vec3::Y * 0.4).unwrap_or(hubs[1]);
                    phase = 3;
                    let (y, pc) = aim_at(eye, anchor);
                    input(y, pc, 0, 1, TETHER_L)
                }
                _ => input(yaw_to(s.pos, south), 0.0, 0, 1, 0),
            }
        },
        move |s, _| s.grounded && s.pos.z < -36.8 && (s.pos.y - casino::MEZZ_Y).abs() < 1.0,
    );
    if res.touchdowns > 1 {
        res.note = format!("touched down {} times", res.touchdowns);
    }
    res
}

/// 3. Onto the top of the halo spiral (as off the chandelier over its start), then the whole turn
/// down it: one lap around the wheel on its inner face.
fn roulette_surf(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let mut cols = Vec::new();
    push_surf_colliders(&m.surf_lines, &mut cols);
    let p = MoveParams::default();
    let l0 = m.surf_lines[1];
    let (st, ct) = l0.face_deg.to_radians().sin_cos();
    let ridge = (l0.a + l0.b) * 0.5;
    let d = (l0.b - l0.a).normalize();
    let radial = Vec3::new(ridge.x, 0.0, ridge.z).normalize();
    let n = -radial * st + Vec3::Y * ct;
    // 1.2 m down the inner face; the hull's face-side bottom corner just off it (0.8 m box).
    let point = ridge - radial * (1.2 * ct) - Vec3::Y * (1.2 * st);
    let corner = Vec3::new(0.4 * radial.x.signum(), 0.0, 0.4 * radial.z.signum());
    let mut s = PlayerState::new(point - corner + n * 0.05, &p);
    s.grounded = false;
    s.air_time = 0.5;
    s.vel = d * 12.0;
    let a0 = ridge.z.atan2(ridge.x);
    let mut travelled = 0.0f32;
    let mut prev_a = a0;
    let mut res = run(
        w,
        m,
        &cols,
        3,
        s,
        20.0,
        move |s, _| {
            // View along the spiral (CCW tangent); hold the strafe key that pushes into the face
            // (radially outward) — but not while drifting inward off it (that would only brake).
            let out = horiz(s.pos).normalize_or(Vec3::X);
            let d = Vec3::new(-out.z, 0.0, out.x);
            let yaw = (-d.x).atan2(-d.z);
            let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
            let hold = s.extra.surf_n != Vec3::ZERO || s.vel.dot(out) > -0.3;
            let mx = if !hold { 0 } else if out.dot(right) > 0.0 { 1 } else { -1 };
            input(yaw, 0.0, mx, 0, 0)
        },
        |s, _| {
            let a = s.pos.z.atan2(s.pos.x);
            let mut d = a - prev_a;
            if d > std::f32::consts::PI {
                d -= std::f32::consts::TAU;
            }
            if d < -std::f32::consts::PI {
                d += std::f32::consts::TAU;
            }
            travelled += d;
            prev_a = a;
            // Most of the turn (the start was 1/48 in; the end runs off onto the pockets).
            travelled >= 0.9 * std::f32::consts::TAU
        },
    );
    if !res.ok {
        res.note = format!("fell off after {:.0}° of 360°", travelled.to_degrees());
    } else {
        res.note = format!("{:.0}° on the halo", travelled.to_degrees());
    }
    res
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_casino(seed);
    let results = [("1 floor bhop", floor_bhop(&world, &info)), ("2 chandelier swing", chandelier_swing(&world, &info)), ("3 roulette surf lap", roulette_surf(&world, &info))];
    let run_speed = MoveParams::default().run_speed;
    println!("High Roller routes (seed {seed}); run speed {run_speed:.1} m/s");
    println!("{:<22} {:>7} {:>9} {:>9} {:>7} {:>6} {:>6}  result", "route", "time s", "avg m/s", "peak m/s", "rise m", "hooks", "touch");
    let mut all_ok = true;
    for (k, (name, r)) in results.iter().enumerate() {
        let fast = k != 0 || r.avg > run_speed * 1.1 || r.peak > run_speed * 1.4;
        let clean = k != 1 || r.touchdowns == 0;
        let ok = r.ok && fast && clean;
        all_ok &= ok;
        println!(
            "{:<22} {:>7.2} {:>9.1} {:>9.1} {:>7.1} {:>6} {:>6}  {}",
            name,
            r.secs,
            r.avg,
            r.peak,
            r.peak_y,
            r.hooks,
            r.touchdowns,
            if ok {
                "ok".to_string()
            } else if r.fell {
                "FELL".to_string()
            } else {
                format!("FAILED (ended at {:.1?}, fast {fast}, clean {clean}) {}", r.end, r.note)
            }
        );
    }
    if !all_ok {
        eprintln!("some High Roller routes failed");
        std::process::exit(1);
    }
}
