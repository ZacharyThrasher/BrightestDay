//! Proves the Lookout's key movement lines with the real movement code: scripted "skilled bot"
//! inputs drive `bd_sim::movement::step_player` through `bd_voxel` `generate_lookout`. Dropping
//! under the KO line fails a route.
//!
//!     cargo run -p bd_sim --features lookout --example lookout_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **Palace walk-through** — courtyard → terrace → the grand door → the hall → the north door →
//!    the north garden, on foot.
//! 2. **Time chamber** — the courtyard path → the plinth steps → the grand door → out the north door.
//! 3. **Wing stairs** — the east wing's outside stairs → its roof → the short flight → the hall roof.
//! 4. **Promenade** — bunny-hop half way round the rim ring (south → east → north).
//! 5. **Korin's stair** — from Korin's roof up the four floating rocks back onto the rim (jumps +
//!    air jumps).
//! 6. **Roof tether** — from the courtyard tether the south-east turret, reel up, let go over the
//!    hall roof and land beside the drum.
//! 7. **Rim recovery** — knocked off the east rim: tether the time chamber, reel back in over the
//!    kerb, land on the disc.
//!
//! Exits non-zero if a route fails. `TRACE=<route number>` prints the bot's state every 0.1 s.

use bd_core::DT;
use bd_core::Vec3;
use bd_core::input::PlayerInput;
use bd_core::voxel::VoxelQuery;
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;
use bd_voxel::maps::lookout::{self, generate_lookout};
use bd_voxel::{MapInfo, VoxelWorld};

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
    avg: f32,
    hooks: u32,
    /// Ground contacts after the first tether attached (0 = the swing never touched down).
    touchdowns: u32,
    fell: bool,
    end: Vec3,
}

fn run(
    cw: &CollisionWorld<'_, VoxelWorld>,
    info: &MapInfo,
    route: usize,
    mut s: PlayerState,
    max_secs: f32,
    mut ctrl: impl FnMut(&PlayerState, usize) -> PlayerInput,
    goal: impl Fn(&PlayerState) -> bool,
) -> RunResult {
    let p = MoveParams::default();
    let trace = std::env::var("TRACE").is_ok_and(|v| v == route.to_string());
    let mut r = RunResult::default();
    let mut dist = 0.0;
    let mut hooked = false;
    for t in 0..(max_secs / DT) as usize {
        let mut i = ctrl(&s, t);
        i.seq = t as u32;
        let prev = s;
        let e = step_player(&mut s, &i, cw, &p, DT);
        dist += horiz(s.pos - prev.pos).length();
        r.peak = r.peak.max(s.speed());
        if e.tether_attached.iter().any(|&a| a) {
            r.hooks += 1;
            hooked = true;
        }
        if hooked && s.grounded && !prev.grounded {
            r.touchdowns += 1;
        }
        if trace && t % 12 == 0 {
            println!(
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) gr={} aj={} hooks={:?} btn={:#x}",
                t as f32 * DT,
                s.pos.x,
                s.pos.y,
                s.pos.z,
                s.vel.x,
                s.vel.y,
                s.vel.z,
                s.grounded as u8,
                s.extra.air_jump_used as u8,
                s.hooks[0].phase,
                i.buttons
            );
        }
        r.secs = (t + 1) as f32 * DT;
        if s.pos.y < info.void_y {
            r.fell = true;
            break;
        }
        if goal(&s) {
            r.ok = true;
            break;
        }
    }
    r.avg = dist / r.secs.max(1e-3);
    r.end = s.pos;
    r
}

fn crosshair<V: VoxelQuery>(w: &V, eye: Vec3, target: Vec3) -> Option<Vec3> {
    bd_core::voxel::raycast(w, eye, target - eye, 120.0).map(|h| h.point)
}

/// Follow waypoints on foot (plain running: stairs are 0.5 m steps).
fn walk(cw: &CollisionWorld<'_, VoxelWorld>, m: &MapInfo, name: &str, n: usize) -> RunResult {
    let r = m.route(name).unwrap_or_else(|| panic!("route {name}"));
    let pts = r.waypoints.clone();
    let end = *pts.last().unwrap();
    let p = MoveParams::default();
    let s = PlayerState::new(pts[0], &p);
    let mut k = 1;
    run(
        cw,
        m,
        n,
        s,
        40.0,
        move |s, _| {
            while k < pts.len() - 1 && horiz(pts[k] - s.pos).length() < 1.5 {
                k += 1;
            }
            input(yaw_to(s.pos, pts[k]), 0.0, 0, 1, 0)
        },
        move |s| s.grounded && horiz(end - s.pos).length() < 2.0 && (s.pos.y - end.y).abs() < 1.5,
    )
}

/// Bunny-hop along waypoints.
fn hop(cw: &CollisionWorld<'_, VoxelWorld>, m: &MapInfo, name: &str, n: usize) -> RunResult {
    let r = m.route(name).unwrap_or_else(|| panic!("route {name}"));
    let pts = r.waypoints.clone();
    let end = *pts.last().unwrap();
    let p = MoveParams::default();
    let s = PlayerState::new(pts[0], &p);
    let mut hopper = Hopper::new();
    let mut k = 1;
    run(
        cw,
        m,
        n,
        s,
        30.0,
        move |s, _| {
            while k < pts.len() - 1 && horiz(pts[k] - s.pos).length() < 3.0 {
                k += 1;
            }
            hopper.hop_to(s, pts[k], 8.0)
        },
        move |s| s.grounded && horiz(end - s.pos).length() < 3.0 && (s.pos.y - end.y).abs() < 1.5,
    )
}

/// Climb a chain of platforms: from each top jump toward the next (air jump at the apex if short)
/// and land on it; the last waypoint is the goal.
fn climb(cw: &CollisionWorld<'_, VoxelWorld>, m: &MapInfo, name: &str, n: usize) -> RunResult {
    let r = m.route(name).unwrap_or_else(|| panic!("route {name}"));
    let pts = r.waypoints.clone();
    let top = *pts.last().unwrap();
    let p = MoveParams::default();
    let s = PlayerState::new(pts[0], &p);
    let mut k = 1usize;
    let mut prev_jump = false;
    run(
        cw,
        m,
        n,
        s,
        30.0,
        move |s, _| {
            // Advance once standing on (or above) the current target.
            if k < pts.len() - 1 && s.grounded && s.pos.y > pts[k].y - 0.3 && horiz(pts[k] - s.pos).length() < 2.4 {
                k += 1;
            }
            let tgt = pts[k];
            let to = horiz(tgt - s.pos);
            let mut b = 0;
            if s.grounded {
                // Jump from the edge nearest the next rock.
                if tgt.y > s.pos.y - 1.0 && to.length() < 3.6 && !prev_jump {
                    b = JUMP;
                }
            } else if !s.extra.air_jump_used && s.vel.y < 1.0 && s.pos.y < tgt.y + 0.6 && !prev_jump && s.air_time > 0.2 {
                b = JUMP;
            }
            prev_jump = b != 0;
            let mz = if to.length() > 0.35 { 1 } else { 0 };
            input(yaw_to(s.pos, tgt), 0.0, 0, mz, b)
        },
        move |s| s.grounded && s.pos.y >= top.y - 0.1 && horiz(top - s.pos).length() < 3.0,
    )
}

/// A tether hop: jump, fire at `target` (whatever the crosshair hits on the way), reel in until
/// `release(state)`, then let go and steer to `land` (air jump if sinking short of it).
fn tether_hop(
    cw: &CollisionWorld<'_, VoxelWorld>,
    w: &VoxelWorld,
    m: &MapInfo,
    n: usize,
    mut s: PlayerState,
    target: Vec3,
    land: Vec3,
    release: impl Fn(&PlayerState) -> bool,
    goal: impl Fn(&PlayerState) -> bool,
) -> RunResult {
    let p = MoveParams::default();
    s.air_time = s.air_time.max(0.0);
    let mut anchor = target;
    let mut phase = 0u8;
    run(
        cw,
        m,
        n,
        s,
        12.0,
        move |s, t| {
            let eye = s.eye(&p);
            match phase {
                0 => {
                    // On the ground: hop first so the rope doesn't drag us along the floor.
                    if s.grounded {
                        return input(yaw_to(s.pos, target), 0.0, 0, 0, if t % 2 == 0 { JUMP } else { 0 });
                    }
                    anchor = crosshair(w, eye, target).unwrap_or(target);
                    phase = 1;
                    let (y, pc) = aim_at(eye, anchor);
                    input(y, pc, 0, 0, TETHER_L)
                }
                1 => {
                    let h = s.hooks[0];
                    let (y, pc) = aim_at(eye, anchor);
                    if release(s) {
                        phase = 2;
                        return input(yaw_to(s.pos, land), 0.0, 0, 1, 0);
                    }
                    input(y, pc, 0, 0, TETHER_L | if h.attached() { REEL } else { 0 })
                }
                _ => {
                    let jump = s.vel.y < 0.0 && !s.extra.air_jump_used && s.pos.y < land.y + 0.8;
                    input(yaw_to(s.pos, land), 0.0, 0, 1, if jump { JUMP } else { 0 })
                }
            }
        },
        goal,
    )
}

/// 6. From the courtyard: tether the south-east turret, reel up past the hall's south wall, let go
/// over the roof and land on it beside the drum.
fn dome_tether(cw: &CollisionWorld<'_, VoxelWorld>, w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("dome_tether").expect("dome_tether");
    let p = MoveParams::default();
    let s = PlayerState::new(r.waypoints[0], &p);
    let land = r.waypoints[1];
    tether_hop(
        cw,
        w,
        m,
        6,
        s,
        r.anchors[0],
        land,
        |s| s.pos.y > lookout::HALL_ROOF + 2.0,
        move |s| s.grounded && (s.pos.y - lookout::HALL_ROOF).abs() < 0.6,
    )
}

/// 7. Knocked off the east rim (flying out, just above the floor): tether the time chamber, reel
/// back in over the kerb, let go over the disc and land.
fn rim_recovery(cw: &CollisionWorld<'_, VoxelWorld>, w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let p = MoveParams::default();
    let mut s = PlayerState::new(Vec3::new(lookout::DISC_R + 3.0, lookout::FLOOR_Y + 1.5, 3.0), &p);
    s.vel = Vec3::new(7.0, 2.0, 0.0);
    s.grounded = false;
    s.air_time = 0.6;
    let c = lookout::CHAMBER_C;
    tether_hop(
        cw,
        w,
        m,
        7,
        s,
        Vec3::new(c.x, lookout::CHAMBER_ROOF + 2.5, c.y),
        Vec3::new(lookout::DISC_R - 10.0, lookout::FLOOR_Y, 3.0),
        |s| horiz(s.pos).length() < lookout::DISC_R - 3.0 && s.pos.y > lookout::FLOOR_Y + 0.8,
        |s| s.grounded && (s.pos.y - lookout::FLOOR_Y).abs() < 0.6 && horiz(s.pos).length() < lookout::DISC_R - 0.5,
    )
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_lookout(seed);
    let cols = Vec::new();
    let cw = CollisionWorld::new(&world, &cols);
    let results = [
        ("1 palace walk-through", walk(&cw, &info, "palace_walk", 1)),
        ("2 time chamber", walk(&cw, &info, "chamber_walk", 2)),
        ("3 wing stairs to roof", walk(&cw, &info, "wing_climb", 3)),
        ("4 promenade bhop", hop(&cw, &info, "promenade", 4)),
        ("5 Korin's stair", climb(&cw, &info, "korin_stair", 5)),
        ("6 roof tether", dome_tether(&cw, &world, &info)),
        ("7 rim recovery", rim_recovery(&cw, &world, &info)),
    ];
    println!("the lookout routes (seed {seed})");
    println!("{:<24} {:>7} {:>9} {:>9} {:>6} {:>6}  result", "route", "time s", "avg m/s", "peak m/s", "hooks", "touch");
    let mut all_ok = true;
    for (k, (name, r)) in results.iter().enumerate() {
        let clean = match k {
            3 => r.avg > MoveParams::default().run_speed * 1.1,
            5 | 6 => r.hooks > 0,
            _ => true,
        };
        let ok = r.ok && clean && !r.fell;
        all_ok &= ok;
        println!(
            "{:<24} {:>7.2} {:>9.1} {:>9.1} {:>6} {:>6}  {}",
            name,
            r.secs,
            r.avg,
            r.peak,
            r.hooks,
            r.touchdowns,
            if ok {
                "ok".to_string()
            } else if r.fell {
                format!("VOID at {:.1?}", r.end)
            } else {
                format!("FAILED (ended at {:.1?}, clean {clean})", r.end)
            }
        );
    }
    if !all_ok {
        eprintln!("some Lookout routes failed");
        std::process::exit(1);
    }
}
