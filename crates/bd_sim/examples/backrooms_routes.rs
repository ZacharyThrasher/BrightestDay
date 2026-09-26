//! Proves the Backrooms' key movement lines with the real movement code: scripted "skilled bot"
//! inputs drive `bd_sim::movement::step_player` through `bd_voxel` `generate_backrooms`.
//!
//!     cargo run -p bd_sim --features backrooms --example backrooms_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **Hotel bhop** — the 76 m level-3 hotel spine, end to end, bunny-hopping (must beat run speed).
//! 2. **Sky swing** — across the 28 m sky room: sprint, jump, tether the painted-sky ceiling, swing
//!    through, release and land on the far side without touching down in between.
//! 3. **Office corridor** — the level-1 office corridor under the drop ceiling, bunny-hopping.
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
use bd_voxel::maps::backrooms::generate_backrooms;

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
}

fn run(
    world: &VoxelWorld,
    info: &MapInfo,
    route: usize,
    mut s: PlayerState,
    max_secs: f32,
    mut ctrl: impl FnMut(&PlayerState, usize) -> PlayerInput,
    goal: impl Fn(&PlayerState, usize) -> bool,
) -> RunResult {
    let cw = CollisionWorld::new(world, &[]);
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
        if trace && t % 12 == 0 {
            println!(
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) gr={} hooks={:?}/{:?}",
                t as f32 * DT,
                s.pos.x,
                s.pos.y,
                s.pos.z,
                s.vel.x,
                s.vel.y,
                s.vel.z,
                s.grounded as u8,
                s.hooks[0].phase,
                s.hooks[1].phase
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

/// Bunny-hop a straight corridor route (lane-keeping on its centre line).
fn corridor(w: &VoxelWorld, m: &MapInfo, name: &str, n: usize) -> RunResult {
    let r = m.route(name).unwrap_or_else(|| panic!("route {name}"));
    let (a, end) = (r.waypoints[0], *r.waypoints.last().unwrap());
    let p = MoveParams::default();
    let mut s = PlayerState::new(a, &p);
    s.vel = Vec3::ZERO;
    let mut hop = Hopper::new();
    let dir = horiz(end - a).normalize();
    run(
        w,
        m,
        n,
        s,
        30.0,
        move |s, _| {
            // Pursue a point 20 m ahead on the centre line.
            let along = (s.pos - a).dot(dir);
            let aim = a + dir * (along + 20.0);
            hop.hop_to(s, Vec3::new(aim.x, s.pos.y, aim.z), 8.0)
        },
        move |s, _| (s.pos - a).dot(dir) >= (end - a).dot(dir) - 2.0 && (s.pos.y - a.y).abs() < 1.5,
    )
}

/// 2. Sky room: run, jump, tether the ceiling, swing across, land on the far side.
fn sky_swing(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("sky_swing").expect("sky_swing");
    let (start, goal_p) = (r.waypoints[0], r.waypoints[1]);
    let anchor_hint = r.anchors[0];
    let floor = start.y;
    let p = MoveParams::default();
    let s = PlayerState::new(start, &p);
    let mut phase = 0u8;
    let mut anchor = anchor_hint;
    let mut idle = false;
    run(
        w,
        m,
        2,
        s,
        20.0,
        move |s, _| {
            let eye = s.eye(&p);
            match phase {
                0 => {
                    // Sprint at the far side; jump after a few metres; fire at the ceiling ahead.
                    let run_dist = horiz(s.pos - start).length();
                    if !s.grounded && run_dist > 4.0 {
                        phase = 1;
                        anchor = crosshair(w, eye, anchor_hint).unwrap_or(anchor_hint);
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    let mut i = input(yaw_to(s.pos, goal_p), 0.0, 0, 1, 0);
                    if s.grounded && run_dist > 3.5 {
                        i.buttons |= JUMP;
                    }
                    i
                }
                1 => {
                    let h = s.hooks[0];
                    if !h.attached() {
                        idle = !idle;
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, if idle && h.phase == HookPhase::Idle { 0 } else { TETHER_L });
                    }
                    // Reel up into the swing (boost through the bottom of the arc), let go past the
                    // anchor while rising and fly on to the far side.
                    let past = s.pos.x > anchor.x + 4.0;
                    if past && s.vel.y > 0.0 {
                        phase = 2;
                        return input(yaw_to(s.pos, goal_p), 0.0, 0, 1, 0);
                    }
                    let (y, pc) = aim_at(eye, h.pos + Vec3::new(6.0, 0.0, 0.0));
                    let reel = if s.pos.x < anchor.x { REEL } else { 0 };
                    input(y, pc, 0, 1, TETHER_L | reel | if s.vel.y < 0.0 { BOOST } else { 0 })
                }
                _ => input(yaw_to(s.pos, goal_p), 0.0, 0, 1, 0),
            }
        },
        move |s, _| s.grounded && s.pos.x > goal_p.x - 3.0 && (s.pos.y - floor).abs() < 1.0,
    )
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_backrooms(seed);
    let results = [
        ("1 hotel spine bhop", corridor(&world, &info, "hotel_bhop", 1)),
        ("2 sky room swing", sky_swing(&world, &info)),
        ("3 office corridor bhop", corridor(&world, &info, "office_corridor", 3)),
    ];
    let run_speed = MoveParams::default().run_speed;
    println!("backrooms routes (seed {seed}); run speed {run_speed:.1} m/s");
    println!("{:<24} {:>7} {:>9} {:>9} {:>7} {:>6} {:>6}  result", "route", "time s", "avg m/s", "peak m/s", "rise m", "hooks", "touch");
    let mut all_ok = true;
    for (k, (name, r)) in results.iter().enumerate() {
        // Bhop lines must beat plain running; the swing must not touch down before the landing.
        let fast = k == 1 || r.avg > run_speed * 1.1 || r.peak > run_speed * 1.4;
        // The swing must be a real swing: up off the floor, then down onto the far side.
        let clean = k != 1 || (r.touchdowns <= 1 && r.peak_y > 4.0);
        let ok = r.ok && fast && clean;
        all_ok &= ok;
        println!(
            "{:<24} {:>7.2} {:>9.1} {:>9.1} {:>7.1} {:>6} {:>6}  {}",
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
                format!("FAILED (ended at {:.1?}, fast {fast}, clean {clean})", r.end)
            }
        );
    }
    if !all_ok {
        eprintln!("some Backrooms routes failed");
        std::process::exit(1);
    }
}
