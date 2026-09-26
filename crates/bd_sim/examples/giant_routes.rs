//! Proves the Giant's Room movement lines with the real movement code: scripted "skilled bot"
//! inputs drive `bd_sim::movement::step_player` through `bd_voxel` `generate_giant` (voxels + the
//! map's surf lines as colliders, exactly like the sim).
//!
//!     cargo run -p bd_sim --features giant --example giant_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **Track loop** — a full lap of the toy track, bunny-hopping round the corners and over the
//!    kicker gap (must beat run speed).
//! 2. **Desk ramp** — run up the launch ramp from the rug to the desk top.
//! 3. **Bookshelf climb** — from the floor to the top of the bookshelf (58 m) on one tether.
//! 4. **Curtain swing** — hook the curtain rod and swing across the open window, landing on the far
//!    side without touching down in between.
//! 5. **Blanket surf** — drop onto the blanket's surf line at the foot of the bed and ride it down.
//! 6. **Bank surf** — hit the NE banked corner at speed and ride its surf chords.
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
use bd_voxel::maps::giant::{self, generate_giant};

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
    surf_ticks: u32,
    fell: bool,
    end: Vec3,
}

#[allow(clippy::too_many_arguments)]
fn run(
    world: &VoxelWorld,
    cols: &[Collider],
    info: &MapInfo,
    route: usize,
    mut s: PlayerState,
    max_secs: f32,
    mut ctrl: impl FnMut(&PlayerState, usize) -> PlayerInput,
    goal: impl Fn(&PlayerState, usize) -> bool,
) -> RunResult {
    let cw = CollisionWorld::new(world, cols);
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
        r.surf_ticks += surfing(&s, &p) as u32;
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

/// 1. A full lap of the track, bunny-hopping: pure pursuit of a point 7 m ahead along the lane's
///    centre line (so the bot rides the kickers instead of cutting the corners).
fn track_loop(w: &VoxelWorld, cols: &[Collider], m: &MapInfo) -> RunResult {
    let wp = m.route("track_loop").expect("track_loop").waypoints.clone();
    let n = wp.len();
    // Cumulative arc length of the closed loop.
    let mut arc = vec![0.0f32; n + 1];
    for i in 0..n {
        arc[i + 1] = arc[i] + horiz(wp[(i + 1) % n] - wp[i]).length();
    }
    let total = arc[n];
    let at = |s: f32| -> Vec3 {
        let s = s.rem_euclid(total);
        let i = arc.partition_point(|&a| a <= s).saturating_sub(1).min(n - 1);
        let t = (s - arc[i]) / (arc[i + 1] - arc[i]).max(1e-3);
        wp[i].lerp(wp[(i + 1) % n], t)
    };
    let p = MoveParams::default();
    let s = PlayerState::new(wp[0], &p);
    let mut hop = Hopper::new();
    let mut progress = 0.0f32;
    let done = std::cell::Cell::new(false);
    run(
        w,
        cols,
        m,
        1,
        s,
        60.0,
        |s, _| {
            // Advance along the loop to the nearest centre-line point within 12 m ahead.
            let here = horiz(s.pos);
            let mut best = progress;
            let mut d = f32::INFINITY;
            let mut k = 0.0;
            while k <= 12.0 {
                let q = horiz(at(progress + k)).distance(here);
                if q < d {
                    d = q;
                    best = progress + k;
                }
                k += 0.5;
            }
            progress = best;
            if progress >= total - 3.0 {
                done.set(true);
            }
            let aim = at(progress + 7.0);
            // The jump on the west straight: land and run up the block ramp (airborne you'd hit its
            // 0.5 m steps), jump off the lip, then carry on hopping.
            let kicker = (s.pos.x - giant::TRACK_W).abs() < 3.0 && (9.5..23.0).contains(&s.pos.z);
            if kicker {
                let lip = s.pos.z < 10.8 && s.grounded;
                return input(yaw_to(s.pos, aim), 0.0, 0, 1, if lip { JUMP } else { 0 });
            }
            let mut i = hop.hop_to(s, Vec3::new(aim.x, s.pos.y, aim.z), 3.0);
            if s.grounded && s.horizontal_speed() < 2.0 {
                i.buttons |= JUMP;
            }
            i
        },
        |_, _| done.get(),
    )
}

/// 2. Run up the launch ramp to the desk.
fn desk_ramp(w: &VoxelWorld, cols: &[Collider], m: &MapInfo) -> RunResult {
    let wp = m.route("desk_ramp").expect("desk_ramp").waypoints.clone();
    let p = MoveParams::default();
    let s = PlayerState::new(wp[0], &p);
    let mut k = 1;
    run(
        w,
        cols,
        m,
        2,
        s,
        20.0,
        move |s, _| {
            if k + 1 < wp.len() && horiz(wp[k]).distance(horiz(s.pos)) < 2.0 {
                k += 1;
            }
            input(yaw_to(s.pos, wp[k]), 0.0, 0, 1, 0)
        },
        |s, _| s.grounded && s.pos.y >= giant::DESK_TOP - 0.05 && s.pos.z < giant::DESK.z1 - 2.0,
    )
}

/// 3. Floor → top of the bookshelf with the tethers: hook the ceiling straight above (in front of the
///    case), reel up past its top, then hook the far end of the top and reel onto it.
fn bookshelf_climb(w: &VoxelWorld, cols: &[Collider], m: &MapInfo) -> RunResult {
    let r = m.route("bookshelf_climb").expect("bookshelf_climb");
    let start = r.waypoints[0];
    let top = *r.waypoints.last().unwrap();
    let up = Vec3::new(start.x, giant::CEIL_Y, start.z);
    let back = Vec3::new(giant::BOOKSHELF.x0 + 1.5, giant::BOOKSHELF_TOP - 0.25, top.z);
    let deep = Vec3::new(giant::BOOKSHELF.center().x, top.y, top.z);
    let p = MoveParams::default();
    let s = PlayerState::new(start, &p);
    let mut phase = 0u8;
    let (mut a1, mut a2) = (up, back);
    let mut idle = false;
    run(
        w,
        cols,
        m,
        3,
        s,
        20.0,
        move |s, t| {
            let eye = s.eye(&p);
            match phase {
                0 => {
                    if t < 10 {
                        return input(0.0, 0.0, 0, 0, JUMP);
                    }
                    phase = 1;
                    a1 = crosshair(w, eye, up).unwrap_or(up);
                    let (y, pc) = aim_at(eye, a1);
                    input(y, pc, 0, 0, TETHER_L)
                }
                1 => {
                    let h = s.hooks[0];
                    if !h.attached() {
                        idle = !idle;
                        let (y, pc) = aim_at(eye, a1);
                        return input(y, pc, 0, 0, if idle && h.phase == HookPhase::Idle { 0 } else { TETHER_L });
                    }
                    if s.pos.y > giant::BOOKSHELF_TOP + 2.0 {
                        phase = 2;
                        a2 = crosshair(w, eye, back).unwrap_or(back);
                        let (y, pc) = aim_at(eye, a2);
                        return input(y, pc, 0, 0, TETHER_R);
                    }
                    let (y, pc) = aim_at(eye, a1);
                    input(y, pc, 0, 0, TETHER_L | REEL)
                }
                2 => {
                    let h = s.hooks[1];
                    if s.grounded {
                        phase = 3;
                    }
                    if !h.attached() {
                        idle = !idle;
                        let (y, pc) = aim_at(eye, a2);
                        return input(y, pc, 0, 0, if idle && h.phase == HookPhase::Idle { 0 } else { TETHER_R });
                    }
                    let (y, pc) = aim_at(eye, a2);
                    input(y, pc, 0, 1, TETHER_R | REEL)
                }
                _ => input(yaw_to(s.pos, deep), 0.0, 0, 1, 0),
            }
        },
        move |s, _| s.grounded && s.pos.y >= giant::BOOKSHELF_TOP - 0.05,
    )
}

/// 4. Curtain swing: sprint along the window wall, jump, hook the rod, swing across the window,
///    release on the upswing and land beyond it.
fn curtain_swing(w: &VoxelWorld, cols: &[Collider], m: &MapInfo) -> RunResult {
    let r = m.route("curtain_swing").expect("curtain_swing");
    let hint = r.anchors[0];
    let start = Vec3::new(giant::HX - 12.0, giant::FLOOR_Y, -34.0);
    let goal_p = Vec3::new(giant::HX - 12.0, giant::FLOOR_Y, 34.0);
    let p = MoveParams::default();
    let s = PlayerState::new(start, &p);
    let mut phase = 0u8;
    let mut anchor = hint;
    let mut idle = false;
    run(
        w,
        cols,
        m,
        4,
        s,
        20.0,
        move |s, _| {
            let eye = s.eye(&p);
            match phase {
                0 => {
                    let run_dist = horiz(s.pos - start).length();
                    if !s.grounded && run_dist > 5.0 {
                        phase = 1;
                        anchor = crosshair(w, eye, hint).unwrap_or(hint);
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    let mut i = input(yaw_to(s.pos, goal_p), 0.0, 0, 1, 0);
                    if s.grounded && run_dist > 4.5 {
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
                    // Reel while the arc would scrape the floor, boost on the downswing, let go past
                    // the anchor while rising.
                    if s.pos.z > anchor.z + 6.0 && s.vel.y > 0.0 {
                        phase = 2;
                        return input(yaw_to(s.pos, goal_p), 0.0, 0, 1, 0);
                    }
                    let low = h.pos.y - h.rope_len;
                    let reel = if low < giant::FLOOR_Y + 6.0 { REEL } else { 0 };
                    let (y, pc) = aim_at(eye, s.pos + Vec3::new(0.0, 0.0, 10.0));
                    input(y, pc, 0, 1, TETHER_L | reel | if s.vel.y < 0.0 { BOOST } else { 0 })
                }
                _ => input(yaw_to(s.pos, goal_p), 0.0, 0, 1, 0),
            }
        },
        move |s, _| s.grounded && s.pos.z > giant::WINDOW_Z.1 + 2.0,
    )
}

/// The right-hand face collider of surf line `k` and the line's unit direction.
fn surf_face(m: &MapInfo, k: usize) -> (Collider, Vec3, bd_voxel::SurfLine) {
    let l = m.surf_lines[k];
    (surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0)[0], (l.b - l.a).normalize(), l)
}

/// 5. Drop onto the blanket's surf line near the top and ride it to the floor, strafing into the face.
fn blanket_surf(w: &VoxelWorld, cols: &[Collider], m: &MapInfo) -> RunResult {
    let (face, d, l) = surf_face(m, 0);
    let (f, n) = (face.axes()[0], face.axes()[1]);
    let at = l.a + d * 2.5 + f * 1.8 + n * 0.1;
    let p = MoveParams::default();
    let mut s = PlayerState::new(at - Vec3::new(0.0, 0.0, 0.0), &p);
    s.air_time = 0.3;
    s.grounded = false;
    s.vel = d * 6.0;
    let yaw = (-d.x).atan2(-d.z);
    // The right face's outward normal points to the line's right: push left (A) into it.
    let end = l.b;
    run(w, cols, m, 5, s, 12.0, move |_, _| input(yaw, 0.0, -1, 0, 0), move |s, _| s.grounded && (s.pos - l.a).dot(d) >= (end - l.a).dot(d) - 4.0)
}

/// 6. Ride the NE banked corner: approach along the east straight at bhop speed, jump onto the bank
///    and hold into it round the chords.
fn bank_surf(w: &VoxelWorld, cols: &[Collider], m: &MapInfo) -> RunResult {
    let (face, d, l) = surf_face(m, 1);
    let (f, n) = (face.axes()[0], face.axes()[1]);
    // Chord 1 of the NE corner; its left face (index 1) looks into the corner. Enter on it.
    let left = surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0)[1];
    let (fl, nl) = (left.axes()[0], left.axes()[1]);
    let at = l.a + d * 1.5 + fl * 2.2 + nl * 0.1;
    let _ = (f, n);
    let p = MoveParams::default();
    let mut s = PlayerState::new(at, &p);
    s.air_time = 0.3;
    s.grounded = false;
    s.vel = d * 16.0;
    let last = m.surf_lines[3];
    run(
        w,
        cols,
        m,
        6,
        s,
        6.0,
        move |s, _| {
            // Look along the bank's current chord (the one nearest), strafe right into the bank.
            let k = (1..=3).min_by(|&a, &b| {
                let da = seg_dist(m.surf_lines[a], s.pos);
                let db = seg_dist(m.surf_lines[b], s.pos);
                da.total_cmp(&db)
            });
            let l = m.surf_lines[k.unwrap_or(1)];
            let dd = (l.b - l.a).normalize();
            input((-dd.x).atan2(-dd.z), 0.0, 1, 0, 0)
        },
        move |s, _| (s.pos - last.a).dot((last.b - last.a).normalize()) > (last.b - last.a).length() - 1.0 || s.grounded,
    )
}

fn seg_dist(l: bd_voxel::SurfLine, p: Vec3) -> f32 {
    let (a, b) = (horiz(l.a), horiz(l.b));
    let q = horiz(p);
    let t = ((q - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
    q.distance(a + (b - a) * t)
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_giant(seed);
    let mut cols = Vec::new();
    push_surf_colliders(&info.surf_lines, &mut cols);
    let results = [
        ("1 track loop bhop", track_loop(&world, &cols, &info)),
        ("2 desk launch ramp", desk_ramp(&world, &cols, &info)),
        ("3 bookshelf climb", bookshelf_climb(&world, &cols, &info)),
        ("4 curtain swing", curtain_swing(&world, &cols, &info)),
        ("5 blanket surf", blanket_surf(&world, &cols, &info)),
        ("6 bank surf (NE)", bank_surf(&world, &cols, &info)),
    ];
    let run_speed = MoveParams::default().run_speed;
    println!("giant's room routes (seed {seed}); run speed {run_speed:.1} m/s");
    println!("{:<22} {:>7} {:>8} {:>8} {:>7} {:>5} {:>5} {:>5}  result", "route", "time s", "avg m/s", "peak m/s", "rise m", "hooks", "touch", "surf");
    let mut all_ok = true;
    for (k, (name, r)) in results.iter().enumerate() {
        let (fast, clean) = match k {
            // The lap must beat plain running.
            0 => (r.avg > run_speed * 1.05, true),
            // The swing must be a real swing: up off the floor, then down beyond the window.
            3 => (true, r.touchdowns <= 1 && r.peak_y > 4.0),
            // Surf routes: actually surfed, and came off faster than running.
            4 | 5 => (r.peak > run_speed * 1.2, r.surf_ticks > 40),
            _ => (true, true),
        };
        let ok = r.ok && fast && clean;
        all_ok &= ok;
        println!(
            "{:<22} {:>7.2} {:>8.1} {:>8.1} {:>7.1} {:>5} {:>5} {:>5}  {}",
            name,
            r.secs,
            r.avg,
            r.peak,
            r.peak_y,
            r.hooks,
            r.touchdowns,
            r.surf_ticks,
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
        eprintln!("some Giant's Room routes failed");
        std::process::exit(1);
    }
}
