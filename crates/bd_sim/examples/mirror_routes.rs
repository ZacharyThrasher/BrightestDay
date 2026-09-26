//! Proves Mirrorfold's key movement lines with the real movement code: scripted "skilled bot" inputs
//! drive `bd_sim::movement::step_player` through `bd_voxel` `generate_mirror` (voxels + the map's
//! hard-light surf lines as colliders, exactly like the host).
//!
//!     cargo run -p bd_sim --features mirror --example mirror_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **Avenue launch** — bunny-hop from the plaza down the east avenue, across the ring road and up
//!    the curl's ramp onto the surf band.
//! 2. **Curl surf loop** — a full lap around the arena on the curl's surf faces (straights + every
//!    corner segment), holding the band by strafing into the face, never touching the street.
//! 3. **Hanging-tower swing chain** — from the bank roof, spire to spire around four hanging towers
//!    without touching down, dropping onto the far side of the city.
//! 4. **Fragment hop** — bank roof → two floating cornices → the street-island spawn.
//! 5. **Clock hop** — brownstone roof → two cornices → the clock-face spawn.
//!
//! Exits non-zero if any route fails. `TRACE=<route number>` prints the bot's state every 0.1 s.

use bd_core::DT;
use bd_core::input::PlayerInput;
use bd_core::voxel::{AIR, VoxelQuery};
use bd_core::{IVec3, Vec3};
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;
use bd_voxel::maps::mirror::{self, CURL_R, STREET_Y, curl_polar, generate_mirror};
use bd_voxel::{MapInfo, VoxelWorld};

fn horiz(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = horiz(to - from).normalize_or(Vec3::NEG_Z);
    (-d.x).atan2(-d.z)
}

#[derive(Default, Debug)]
struct RunResult {
    ok: bool,
    secs: f32,
    peak: f32,
    avg: f32,
    min_speed: f32,
    /// Ground contacts (voxel or walkable hard light) after the route "started" (first tether /
    /// first surf contact); swing and surf routes want 0.
    touchdowns: u32,
    hooks: u32,
    surf_ticks: u32,
    fell: bool,
    end: Vec3,
    note: String,
}

struct Ctx<'a> {
    world: &'a VoxelWorld,
    cols: Vec<Collider>,
    void_y: f32,
}

/// Run a controller. `started(&s)` flips once the route proper begins (touchdowns counted after).
#[allow(clippy::too_many_arguments)]
fn run(
    cx: &Ctx,
    route: usize,
    start: Vec3,
    vel0: Vec3,
    yaw: f32,
    max_secs: f32,
    mut ctrl: impl FnMut(&PlayerState, usize) -> PlayerInput,
    started: impl Fn(&PlayerState) -> bool,
    mut goal: impl FnMut(&PlayerState) -> bool,
) -> RunResult {
    let cw = CollisionWorld::new(cx.world, &cx.cols);
    let p = MoveParams::default();
    let mut s = PlayerState::new(start, &p);
    s.vel = vel0;
    if vel0 != Vec3::ZERO {
        s.air_time = 0.3;
    }
    let trace = std::env::var("TRACE").is_ok_and(|v| v == route.to_string());
    let mut r = RunResult { min_speed: f32::INFINITY, ..Default::default() };
    let mut dist = 0.0;
    let mut begun = false;
    for t in 0..(max_secs / DT) as usize {
        let mut i = if t < 2 && vel0 == Vec3::ZERO { input(yaw, 0.0, 0, 0, 0) } else { ctrl(&s, t) };
        i.seq = t as u32;
        let prev = s;
        let e = step_player(&mut s, &i, &cw, &p, DT);
        dist += horiz(s.pos - prev.pos).length();
        r.peak = r.peak.max(s.speed());
        if e.tether_attached.iter().any(|&a| a) {
            r.hooks += 1;
        }
        if surfing(&s, &p) {
            r.surf_ticks += 1;
        }
        if !begun && started(&s) {
            begun = true;
        }
        if begun {
            r.min_speed = r.min_speed.min(s.speed());
            if s.grounded && !prev.grounded {
                r.touchdowns += 1;
            }
        }
        if trace && t % 12 == 0 {
            let pr = mirror::perim(s.pos.x, s.pos.z);
            let (_, th) = curl_polar(pr.s, s.pos.y);
            println!(
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) |v|={:4.1} gr={} surf={} s={:5.1} th={:5.1} hooks={:?}/{:?}",
                t as f32 * DT,
                s.pos.x,
                s.pos.y,
                s.pos.z,
                s.vel.x,
                s.vel.y,
                s.vel.z,
                s.speed(),
                s.grounded as u8,
                surfing(&s, &p) as u8,
                pr.s,
                th.to_degrees(),
                s.hooks[0].phase,
                s.hooks[1].phase,
            );
        }
        if s.pos.y < cx.void_y {
            r.fell = true;
            r.secs = t as f32 * DT;
            break;
        }
        r.secs = (t + 1) as f32 * DT;
        if goal(&s) {
            r.ok = true;
            break;
        }
    }
    r.avg = dist / r.secs.max(1e-3);
    r.end = s.pos;
    if !r.min_speed.is_finite() {
        r.min_speed = 0.0;
    }
    r
}

/// Bunny-hop toward `target` (pursuit + optimal air strafe, zig-zag), holding jump.
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
        input(optimal_strafe_yaw(s.vel, &self.p, DT, mx, 0, self.side), 0.0, mx, 0, JUMP)
    }
}

// ------------------------------------------------------------------------------------ routes

/// 1. Plaza → east avenue → ring road → up the curl ramp onto the surf band.
fn avenue_launch(cx: &Ctx, m: &MapInfo) -> RunResult {
    let r = m.route("avenue_launch").unwrap();
    let (a, b) = (r.waypoints[0], r.waypoints[1]);
    let mut hop = Hopper::new();
    run(
        cx,
        1,
        a,
        Vec3::ZERO,
        yaw_to(a, b),
        12.0,
        |s, _| hop.hop_to(s, Vec3::new(s.pos.x + 25.0, a.y, 0.0), 8.0),
        |_| true,
        |s| mirror::perim(s.pos.x, s.pos.z).s > 12.0 && s.pos.y > STREET_Y + 6.0,
    )
}

/// 2. A full counter-clockwise lap of the curl's surf band.
fn curl_loop(cx: &Ctx, _m: &MapInfo) -> RunResult {
    let p = MoveParams::default();
    // Drop onto the east straight's middle surf face heading north at a bhop-exit speed.
    let start = mirror::curl_world(glam::Vec2::new(mirror::HALF_X, -18.0), glam::Vec2::X, 60.0, 1.4);
    let target_th = 62f32.to_radians();
    let turn_eps: f32 = std::env::var("TURN_EPS").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let mut turned = 0.0f32;
    let mut last_ang = start.z.atan2(start.x);
    let mut off_band = 0u32;
    let mut r = run(
        cx,
        2,
        start,
        Vec3::new(0.0, 0.0, 16.0),
        0.0,
        45.0,
        |s, _| {
            let pr = mirror::perim(s.pos.x, s.pos.z);
            let (_, th) = curl_polar(pr.s, s.pos.y);
            let out = Vec3::new(pr.normal.x, 0.0, pr.normal.y);
            let tangent = Vec3::new(-out.z, 0.0, out.x); // counter-clockwise
            let h = horiz(s.vel);
            // Travelling CCW the face is on the left: strafe left (turn_sign −1) into it to climb /
            // hold, right to ease down; steer back along the tangent if the heading drifts inward.
            let err = tangent.cross(h.normalize_or(tangent)).y; // > 0: heading points outward-left
            // Corners: turn with the curl (strafe right, which also gains speed) before the next
            // segment's face has to clip us round; on the straights lean into the face.
            let sign = if th < target_th - 0.12 {
                -1.0
            } else if th > target_th + 0.10 || err > turn_eps {
                1.0
            } else {
                -1.0
            };
            let mx: i8 = if sign < 0.0 { -1 } else { 1 };
            let yaw = optimal_strafe_yaw(s.vel, &p, DT, mx, 0, sign);
            let b = if s.grounded { JUMP } else { 0 };
            input(yaw, 0.0, mx, 0, b)
        },
        |_| true,
        |s| {
            let ang = s.pos.z.atan2(s.pos.x);
            let mut d = ang - last_ang;
            if d > std::f32::consts::PI {
                d -= std::f32::consts::TAU;
            } else if d < -std::f32::consts::PI {
                d += std::f32::consts::TAU;
            }
            turned += d;
            last_ang = ang;
            if mirror::perim(s.pos.x, s.pos.z).s < 4.0 {
                off_band += 1;
            }
            turned >= std::f32::consts::TAU
        },
    );
    r.note =
        format!("lap {:.0}% ({:.0} deg), {} ticks off the band", turned / std::f32::consts::TAU * 100.0, turned.to_degrees(), off_band);
    r.ok &= r.touchdowns == 0 && off_band == 0;
    r
}

/// Look along the swing, gas-boost on the downswing, reel if the arc's low point would scrape
/// `floor_y` (+ margin).
fn swing_input(s: &PlayerState, p: &MoveParams, hand: u16, floor_y: f32, boost: bool) -> PlayerInput {
    let Some(h) = s.hooks.iter().find(|h| h.attached()) else {
        return input(0.0, 0.0, 0, 0, hand);
    };
    let (yaw, pitch) = aim_at(Vec3::ZERO, s.vel);
    let mut b = hand;
    let feet_low = h.pos.y - h.rope_len - p.stand_height * 0.6;
    if feet_low < floor_y + 3.0 {
        b |= REEL;
    }
    let boosting = boost && s.vel.y < 0.0;
    if boosting {
        b |= BOOST;
    }
    input(yaw, pitch, 0, boosting as i8, b)
}

/// 3. Bank roof → spire to spire around four hanging towers → the brownstones' side, never
///    touching down in between.
fn tower_swing(cx: &Ctx, m: &MapInfo) -> RunResult {
    let r = m.route("tower_swing").unwrap();
    let start = r.waypoints[0];
    let anchors: Vec<Vec3> = r.anchors.iter().map(|a| *a + Vec3::Y * 0.8).collect();
    let p = MoveParams::default();
    let floor = 40.0; // stay above every roof on the way
    let mut next = 0usize;
    let mut phase = 0u8;
    let mut idle = false;
    let released_last = std::cell::Cell::new(false);
    let mut last_release = 0usize;
    let first = anchors[0];
    let mut res = run(
        cx,
        3,
        start,
        Vec3::ZERO,
        yaw_to(start, first),
        40.0,
        |s, t| {
            let hooked = s.hooks[0].attached() || s.hooks[0].phase == HookPhase::Flying;
            match phase {
                0 => {
                    // Run at the first spire, jump, fire.
                    if !s.grounded && t > 30 {
                        phase = 1;
                    }
                    let mut i = input(yaw_to(s.pos, first), 0.0, 0, 1, 0);
                    if t > 25 {
                        i.buttons |= JUMP;
                    }
                    i
                }
                1 => {
                    let a = anchors[next];
                    if !hooked {
                        idle = !idle;
                        let (y, pc) = aim_at(s.eye(&p), a);
                        return input(y, pc, 0, 0, if idle { 0 } else { TETHER_L });
                    }
                    if !s.hooks[0].attached() {
                        let (y, pc) = aim_at(s.eye(&p), a);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    let hp = s.hooks[0].pos;
                    let target = if next + 1 < anchors.len() { anchors[next + 1] } else { r.waypoints[1] };
                    let dir = horiz(target - hp).normalize_or(Vec3::X);
                    let past = horiz(s.pos - hp).dot(dir);
                    if past > 3.0 && s.vel.y > 0.25 * s.speed() {
                        phase = 2;
                        next += 1;
                        last_release = t;
                        if next >= anchors.len() {
                            released_last.set(true);
                        }
                        return input(yaw_to(s.pos, target), 0.0, 0, 1, 0);
                    }
                    // Steer the swing plane toward the next spire: strafe toward it.
                    let mut i = swing_input(s, &p, TETHER_L, floor, true);
                    let side = horiz(s.vel).normalize_or(dir).cross(dir).y;
                    i.move_x = if side > 0.15 {
                        -1
                    } else if side < -0.15 {
                        1
                    } else {
                        0
                    };
                    i
                }
                _ => {
                    if next >= anchors.len() {
                        return input(yaw_to(s.pos, r.waypoints[1]), 0.0, 0, 1, 0);
                    }
                    let a = anchors[next];
                    let d = (a - s.pos).length();
                    if (s.vel.y < 2.0 && d < 48.0 && t > last_release + 10) || s.pos.y < floor - 4.0 {
                        phase = 1;
                        let (y, pc) = aim_at(s.eye(&p), a);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    input(yaw_to(s.pos, a), 0.0, 0, 1, 0)
                }
            }
        },
        |s| !s.grounded,
        |s| released_last.get() && (s.grounded || surfing(s, &MoveParams::default())) && s.pos.x < -15.0,
    );
    res.note = format!("{} of {} spires, landed at x {:.0} z {:.0}", next.min(anchors.len()), anchors.len(), res.end.x, res.end.z);
    // Touchdowns before the last release are failures; the landing itself is the goal.
    res.ok &= next >= anchors.len() && res.touchdowns <= 1;
    res
}

/// Step-by-step ledge hopper through `waypoints` (run at each, jump at the edge, land, next).
fn hop_chain(cx: &Ctx, route: usize, waypoints: Vec<Vec3>) -> RunResult {
    let start = waypoints[0];
    let last = *waypoints.last().unwrap();
    let mut k = 1usize;
    let world = cx.world;
    run(
        cx,
        route,
        start,
        Vec3::ZERO,
        yaw_to(start, waypoints[1]),
        30.0,
        |s, _| {
            let wp = waypoints[k.min(waypoints.len() - 1)];
            if s.grounded && horiz(wp - s.pos).length() < 2.0 && s.pos.y > wp.y - 0.6 && k + 1 < waypoints.len() {
                k += 1;
            }
            let wp = waypoints[k.min(waypoints.len() - 1)];
            let yaw = yaw_to(s.pos, wp);
            let dir = horiz(wp - s.pos).normalize_or(Vec3::Z);
            // Jump when the ground 0.9 m ahead drops away, or a ledge / parapet blocks the way.
            let ahead = s.pos + dir * 0.9;
            let v = (ahead / 0.5).floor().as_ivec3();
            let drop = (1..4).all(|d| world.material(v - IVec3::Y * d) == AIR);
            let wall = world.material(v) != AIR || world.material(v + IVec3::Y) != AIR;
            let higher = wp.y > s.pos.y + 0.4 && horiz(wp - s.pos).length() < 5.5;
            let b = if s.grounded && (drop || wall || higher) { JUMP } else { 0 };
            input(yaw, 0.0, 0, 1, b)
        },
        |_| true,
        move |s| s.grounded && horiz(last - s.pos).length() < 3.0 && s.pos.y > last.y - 0.6,
    )
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_mirror(seed);
    let mut cols = Vec::new();
    push_surf_colliders(&info.surf_lines, &mut cols);
    let cx = Ctx { world: &world, cols, void_y: info.void_y };
    let _ = (CURL_R, AIR, IVec3::ZERO, world.material(IVec3::ZERO));
    let frag = info.route("fragment_hop").unwrap().waypoints.clone();
    let clock = info.route("clock_hop").unwrap().waypoints.clone();
    let results = [
        ("1 avenue launch", avenue_launch(&cx, &info)),
        ("2 curl surf loop", curl_loop(&cx, &info)),
        ("3 hanging-tower swing", tower_swing(&cx, &info)),
        ("4 fragment hop (south)", hop_chain(&cx, 4, frag)),
        ("5 clock hop (north)", hop_chain(&cx, 5, clock)),
    ];
    println!("mirrorfold routes (seed {seed})");
    println!(
        "{:<26} {:>7} {:>8} {:>8} {:>8} {:>6} {:>6} {:>6}  result",
        "route", "time s", "avg m/s", "peak", "min", "hooks", "touch", "surf"
    );
    let mut all_ok = true;
    for (name, r) in &results {
        all_ok &= r.ok;
        println!(
            "{:<26} {:>7.2} {:>8.1} {:>8.1} {:>8.1} {:>6} {:>6} {:>6}  {} (end {:.1?}) {}",
            name,
            r.secs,
            r.avg,
            r.peak,
            r.min_speed,
            r.hooks,
            r.touchdowns,
            r.surf_ticks,
            if r.ok {
                "ok"
            } else if r.fell {
                "FELL INTO THE VOID"
            } else {
                "FAILED"
            },
            r.end,
            r.note
        );
    }
    if !all_ok {
        eprintln!("some Mirrorfold routes failed");
        std::process::exit(1);
    }
}
