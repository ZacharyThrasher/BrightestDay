//! Proves the Turtle Lair's key movement lines with the real movement code: scripted "skilled bot"
//! inputs drive `bd_sim::movement::step_player` through `bd_voxel` `generate_lair` (with the map's
//! hard-light surf lines as colliders, exactly as host and clients build them).
//!
//!     cargo run -p bd_sim --features lair --example lair_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **West tunnel bhop** — the dry 62 m storm tunnel, bunny-hopping (must beat run speed).
//! 2. **Platform bhop** — the station platform along the chasm, past the stair well.
//! 3. **Lair swing** — across the hub: sprint, jump, tether the vault rib, swing over the pit,
//!    land in the workshop without touching down in between.
//! 4. **Drain chute** — surf the storm-drain line from the street culvert down into the skate hall.
//! 5. **Half-pipe** — drop in off the north deck, ride down the hard-light wall, across the trough
//!    and up the far wall.
//! 6. **Manhole climb** — up the junction's spiral stair to the west street, then tether the
//!    tenement parapet and pull up onto the roof.
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
use bd_voxel::maps::lair::{self, FLOOR, STREET, generate_lair};

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
    cw: &CollisionWorld<VoxelWorld>,
    info: &MapInfo,
    route: usize,
    mut s: PlayerState,
    max_secs: f32,
    mut ctrl: impl FnMut(&PlayerState, usize) -> PlayerInput,
    goal: impl Fn(&PlayerState, usize) -> bool,
) -> RunResult {
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
        let e = step_player(&mut s, &i, cw, &p, DT);
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
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) gr={} surf={} hooks={:?}/{:?}",
                t as f32 * DT,
                s.pos.x,
                s.pos.y,
                s.pos.z,
                s.vel.x,
                s.vel.y,
                s.vel.z,
                s.grounded as u8,
                surfing(&s, &p) as u8,
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

/// Bunny-hop a straight route (lane-keeping on its centre line).
fn corridor(cw: &CollisionWorld<VoxelWorld>, m: &MapInfo, name: &str, n: usize) -> RunResult {
    let r = m.route(name).unwrap_or_else(|| panic!("route {name}"));
    let (a, end) = (r.waypoints[0], *r.waypoints.last().unwrap());
    let p = MoveParams::default();
    let s = PlayerState::new(a, &p);
    let mut hop = Hopper::new();
    let dir = horiz(end - a).normalize();
    run(
        cw,
        m,
        n,
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

/// 3. Hub swing: run east, jump, tether the vault rib, swing over the pit, land in the workshop.
fn lair_swing(w: &VoxelWorld, cw: &CollisionWorld<VoxelWorld>, m: &MapInfo) -> RunResult {
    let r = m.route("lair_swing").expect("lair_swing");
    let (start, goal_p) = (r.waypoints[0], r.waypoints[1]);
    let anchor_hint = r.anchors[0];
    let floor = start.y;
    let p = MoveParams::default();
    let s = PlayerState::new(start, &p);
    let mut phase = 0u8;
    let mut anchor = anchor_hint;
    let mut idle = false;
    run(
        cw,
        m,
        3,
        s,
        20.0,
        move |s, _| {
            let eye = s.eye(&p);
            match phase {
                0 => {
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
        move |s, _| s.grounded && s.pos.x > goal_p.x - 4.0 && (s.pos.y - floor).abs() < 1.0,
    )
}

/// 4. Drain chute: start on the +z face near the top, hold strafe into the ridge, look down the line.
fn chute(cw: &CollisionWorld<VoxelWorld>, m: &MapInfo) -> RunResult {
    let line = m.surf_lines[2];
    let p = MoveParams::default();
    let d = (line.b - line.a).normalize();
    let a = line.a + d * 3.0;
    let slope = line.face_deg.to_radians().tan();
    // Start 1.8 m out on the +z face (past the ridge caps' walkable lip).
    let mut s = PlayerState::new(Vec3::new(a.x, a.y - 1.8 * slope + 0.3, a.z + 1.8), &p);
    s.vel = horiz(d) * 4.0;
    s.grounded = false;
    let end = line.b;
    let yaw = yaw_to(line.a, line.b);
    run(
        cw,
        m,
        4,
        s,
        12.0,
        move |s, _| {
            if s.pos.x < end.x - 0.5 {
                // Hold a line ~1.3 m down the +z face: strafe into the ramp (−z) only when gravity
                // has pulled you further out than that — never over the ridge.
                let mx: i8 = if s.pos.z > lair::CHUTE_Z + 1.8 { -1 } else { 0 };
                input(yaw, 0.0, mx, 0, 0)
            } else {
                input(yaw, 0.0, 0, 1, 0)
            }
        },
        move |s, _| s.grounded && s.pos.x > end.x + 4.0 && (s.pos.y - FLOOR).abs() < 1.0,
    )
}

/// 5. Half-pipe: come out of the west flat end at speed, hop onto the north hard-light wall and
///    carve it end to end (surf keeps horizontal speed), dropping back into the trough at the east end.
fn halfpipe(cw: &CollisionWorld<VoxelWorld>, m: &MapInfo) -> RunResult {
    let r = m.route("halfpipe").expect("halfpipe");
    let (start, end) = (r.waypoints[0], r.waypoints[1]);
    let p = MoveParams::default();
    // Hopping in along the trough at bhop speed, just short of the wall.
    let mut s = PlayerState::new(Vec3::new(-lair::PIPE_X - 2.0, FLOOR + 1.0, lair::RIDGE_N - 3.0), &p);
    s.vel = Vec3::new(13.0, 2.0, 2.0);
    s.grounded = false;
    let yaw = yaw_to(start, Vec3::new(end.x, start.y, start.z));
    let wall_z = lair::RIDGE_N - 1.3;
    let min_y_on_wall = std::cell::Cell::new(f32::MAX);
    run(
        cw,
        m,
        5,
        s,
        8.0,
        move |s, _| {
            // Strafe into the wall (north = right when looking +x) whenever gravity pulls you below
            // the line; otherwise just glide.
            let mx: i8 = if s.pos.z < wall_z { 1 } else { 0 };
            input(yaw, 0.0, mx, 0, 0)
        },
        move |s, _| {
            if s.pos.x > -lair::PIPE_X + 1.0 && s.pos.x < lair::PIPE_X - 1.0 {
                min_y_on_wall.set(min_y_on_wall.get().min(s.pos.y));
            }
            // Carved the whole wall without dropping to the trough floor, still carrying speed.
            s.pos.x > lair::PIPE_X - 1.0 && min_y_on_wall.get() > FLOOR + 0.6 && s.horizontal_speed() > 8.0
        },
    )
}

/// 6. Manhole climb: follow the spiral stair up the junction shaft, then tether the parapet.
fn manhole_climb(w: &VoxelWorld, cw: &CollisionWorld<VoxelWorld>, m: &MapInfo) -> RunResult {
    let r = m.route("manhole_climb").expect("manhole_climb");
    let start = r.waypoints[0];
    let roof = r.waypoints[1];
    let parapet = r.anchors[1];
    let j = lair::JUNCTIONS[0];
    let p = MoveParams::default();
    // Start at the foot of the stair (west side, first step).
    let s = PlayerState::new(Vec3::new(j.x - 1.75, FLOOR, j.y - 2.4), &p);
    let _ = start;
    let path = 1.75f32;
    let per_rev = 8.0 * path;
    // Point on the stair's centre path at perimeter coordinate `s` (west → north → east → south).
    let at = move |s: f32| -> Vec3 {
        let s = s.rem_euclid(per_rev);
        let (x, z) = if s < 2.0 * path {
            (-path, s - path)
        } else if s < 4.0 * path {
            (s - 3.0 * path, path)
        } else if s < 6.0 * path {
            (path, 5.0 * path - s)
        } else {
            (7.0 * path - s, -path)
        };
        Vec3::new(j.x + x, 0.0, j.y + z)
    };
    let param = move |q: Vec3| -> f32 {
        let (dx, dz) = (q.x - j.x, q.z - j.y);
        if dx.abs() >= dz.abs() {
            if dx < 0.0 { dz.clamp(-path, path) + path } else { 4.0 * path + (path - dz.clamp(-path, path)) }
        } else if dz > 0.0 {
            2.0 * path + (dx.clamp(-path, path) + path)
        } else {
            6.0 * path + (path - dx.clamp(-path, path))
        }
    };
    let mut phase = 0u8;
    let mut anchor = parapet;
    let mut idle = false;
    run(
        cw,
        m,
        6,
        s,
        40.0,
        move |s, _| {
            let eye = s.eye(&p);
            match phase {
                0 => {
                    if s.pos.y >= STREET - 0.05 && s.grounded {
                        phase = 1;
                    }
                    let sp = param(s.pos);
                    let tgt = at(sp + 1.6);
                    input(yaw_to(s.pos, tgt), 0.0, 0, 1, 0)
                }
                1 | 4 => {
                    // Step off the stair onto the street (west), round the shaft's south side,
                    // then jump and fire at the parapet.
                    let (w1, w2) = (Vec3::new(j.x - 4.0, s.pos.y, j.y - 4.0), Vec3::new(j.x, s.pos.y, j.y - 4.5));
                    if phase == 1 {
                        if horiz(s.pos - w1).length() < 1.0 {
                            phase = 4;
                        }
                        return input(yaw_to(s.pos, w1), 0.0, 0, 1, 0);
                    }
                    let toward = w2;
                    if horiz(s.pos - w2).length() < 1.0 && s.grounded {
                        phase = 2;
                        anchor = crosshair(w, eye, parapet).unwrap_or(parapet);
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 1, JUMP | TETHER_L);
                    }
                    input(yaw_to(s.pos, toward), 0.0, 0, 1, 0)
                }
                2 => {
                    let h = s.hooks[0];
                    if !h.attached() {
                        idle = !idle;
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, if idle && h.phase == HookPhase::Idle { 0 } else { TETHER_L });
                    }
                    // Let go once swung up over the parapet line: momentum carries you onto the roof.
                    if s.pos.y > roof.y + 1.2 && s.pos.x > -36.5 {
                        phase = 3;
                        return input(yaw_to(s.pos, roof), 0.0, 0, 1, JUMP);
                    }
                    let (y, pc) = aim_at(eye, anchor + Vec3::Y * 2.0);
                    input(y, pc, 0, 1, TETHER_L | REEL | BOOST)
                }
                _ => input(yaw_to(s.pos, roof), 0.0, 0, 1, if s.grounded { 0 } else { JUMP }),
            }
        },
        move |s, _| s.grounded && s.pos.y > roof.y - 0.6 && s.pos.x > -36.0 && s.pos.x < -16.0,
    )
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_lair(seed);
    let mut colliders = Vec::new();
    push_surf_colliders(&info.surf_lines, &mut colliders);
    let cw = CollisionWorld::new(&world, &colliders);
    let results = [
        ("1 west tunnel bhop", corridor(&cw, &info, "west_tunnel_bhop", 1)),
        ("2 platform bhop", corridor(&cw, &info, "platform_bhop", 2)),
        ("3 lair swing", lair_swing(&world, &cw, &info)),
        ("4 drain chute surf", chute(&cw, &info)),
        ("5 half-pipe", halfpipe(&cw, &info)),
        ("6 manhole climb", manhole_climb(&world, &cw, &info)),
    ];
    let run_speed = MoveParams::default().run_speed;
    println!("turtle lair routes (seed {seed}); run speed {run_speed:.1} m/s");
    println!("{:<22} {:>7} {:>9} {:>9} {:>7} {:>6} {:>6}  result", "route", "time s", "avg m/s", "peak m/s", "rise m", "hooks", "touch");
    let mut all_ok = true;
    for (k, (name, r)) in results.iter().enumerate() {
        // Bhop lines must beat plain running; the chute must be faster than running too.
        let fast = !matches!(k, 0 | 1 | 3) || r.avg > run_speed * 1.1 || r.peak > run_speed * 1.4;
        // The swing must be a real swing: up off the floor, then down onto the far side.
        let clean = k != 2 || (r.touchdowns <= 1 && r.peak_y > 4.0);
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
                format!("FAILED (ended at {:.1?}, fast {fast}, clean {clean})", r.end)
            }
        );
    }
    if !all_ok {
        eprintln!("some Turtle Lair routes failed");
        std::process::exit(1);
    }
}
