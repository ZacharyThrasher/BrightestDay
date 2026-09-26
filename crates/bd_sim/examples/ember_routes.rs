//! Proves Ember's key movement lines with the real movement code: scripted "skilled bot" inputs drive
//! `bd_sim::movement::step_player` through `bd_voxel` `generate_ember` (voxels + the map's surf-line
//! colliders). Touching the lava (feet below `void_y`) fails a route.
//!
//!     cargo run -p bd_sim --features ember --example ember_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **Facility crossing** — North Bank → catwalk stairs → deck → stairs → South Plain, on foot.
//! 2. **West arch** — North Bank → over the rock arch → South Plain, bunny-hopping.
//! 3. **Surf east** — onto the Ridge → Dunes line over the lava, ride it, land on the Dunes.
//! 4. **Surf west** — the Crag → South Plain line.
//! 5. **Arm swing** — sprint off the facility deck, tether the east collector head, swing across
//!    the river and land on the far side without touching down in between.
//! 6. **Fortress climb** — up the keep's south ledge ladder (jumps + air jumps) onto the balcony.
//!
//! Exits non-zero if a route fails. `TRACE=<route number>` prints the bot's state every 0.1 s.

use bd_core::DT;
use bd_core::Vec3;
use bd_core::input::PlayerInput;
use bd_core::voxel::VoxelQuery;
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;
use bd_voxel::maps::ember::{self, generate_ember};
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
    surf_ticks: u32,
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
        if surfing(&s, &p) {
            r.surf_ticks += 1;
        }
        if e.tether_attached.iter().any(|&a| a) {
            r.hooks += 1;
            hooked = true;
        }
        if hooked && s.grounded && !prev.grounded {
            r.touchdowns += 1;
        }
        if trace && t % 12 == 0 {
            println!(
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) gr={} surf={} aj={} hooks={:?} btn={:#x}",
                t as f32 * DT,
                s.pos.x,
                s.pos.y,
                s.pos.z,
                s.vel.x,
                s.vel.y,
                s.vel.z,
                s.grounded as u8,
                surfing(&s, &p) as u8,
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

/// Ride a surf line: drop onto its right face a quarter of the way down at 10 m/s, strafe into the
/// face along the line, then air-steer to `land` and touch down on it.
fn surf(cw: &CollisionWorld<'_, VoxelWorld>, m: &MapInfo, line: bd_voxel::SurfLine, land: Vec3, n: usize) -> RunResult {
    let p = MoveParams::default();
    let faces = surf_line(line.a, line.b, line.face_deg, line.width, 1.5, 0);
    let d = (line.b - line.a).normalize();
    let f = faces[0].axes()[0];
    let nrm = faces[0].axes()[1];
    let point = line.a + (line.b - line.a) * 0.25 + f * 2.5;
    let corner = Vec3::new(-0.4 * nrm.x.signum(), 0.0, -0.4 * nrm.z.signum());
    let mut s = PlayerState::new(point - corner + nrm * 0.05, &p);
    s.vel = d * 10.0;
    s.grounded = false;
    s.air_time = 0.5;
    let yaw = (-d.x).atan2(-d.z);
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let into = Vec3::new(-nrm.x, 0.0, -nrm.z);
    let hold = input(yaw, 0.0, if into.dot(right) > 0.0 { 1 } else { -1 }, 0, 0);
    let mut left_line = false;
    run(
        cw,
        m,
        n,
        s,
        12.0,
        move |s, _| {
            let along = (s.pos - line.a).dot(d) / (line.b - line.a).length();
            if !left_line && (along > 0.97 || (along > 0.5 && !surfing(s, &p) && !s.grounded && s.vel.y < -2.0)) {
                left_line = true;
            }
            if left_line { input(yaw_to(s.pos, land), 0.0, 0, 1, 0) } else { hold }
        },
        move |s| s.grounded && horiz(land - s.pos).length() < 14.0 && s.pos.y > land.y - 2.0,
    )
}

/// 5. Deck → tether the east collector head → swing over the river → land on the far side.
fn arm_swing(cw: &CollisionWorld<'_, VoxelWorld>, w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("arm_swing").expect("arm_swing");
    let head = r.anchors[0];
    let p = MoveParams::default();
    let start = Vec3::new(6.5, ember::DECK_Y, -2.0);
    let s = PlayerState::new(start, &p);
    let mut phase = 0u8;
    let mut anchor = head;
    let edge = ember::DECK[2];
    run(
        cw,
        m,
        5,
        s,
        20.0,
        move |s, _| {
            let eye = s.eye(&p);
            match phase {
                0 => {
                    if !s.grounded && s.pos.x > edge - 2.0 {
                        phase = 1;
                        anchor = crosshair(w, eye, head).unwrap_or(head);
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    let mut i = input(yaw_to(s.pos, Vec3::new(40.0, 0.0, -2.0)), 0.0, 0, 1, 0);
                    if s.grounded && s.pos.x > edge - 3.0 {
                        i.buttons |= JUMP;
                    }
                    i
                }
                1 => {
                    let h = s.hooks[0];
                    if h.phase == HookPhase::Idle {
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, 0);
                    }
                    if !h.attached() {
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    // Swing through under the head, let go rising on the far side.
                    if s.pos.x > anchor.x + 3.0 && s.vel.y > 0.0 {
                        phase = 2;
                        return input(yaw_to(s.pos, Vec3::new(44.0, 0.0, 4.0)), 0.0, 0, 1, 0);
                    }
                    let (y, pc) = aim_at(eye, h.pos + Vec3::new(6.0, 0.0, 0.0));
                    let feet_low = h.pos.y - h.rope_len - 1.2;
                    let reel = if feet_low < 4.0 { REEL } else { 0 };
                    input(y, pc, 0, 1, TETHER_L | reel | if s.vel.y < 0.0 { BOOST } else { 0 })
                }
                _ => input(yaw_to(s.pos, Vec3::new(44.0, 0.0, 4.0)), 0.0, 0, 1, 0),
            }
        },
        move |s| s.grounded && s.pos.x > 33.0 && s.pos.y > ember::VOID_Y + 5.0,
    )
}

/// 6. Up the south ledge ladder: from each ledge jump toward the next (air jump if short) and land.
fn fortress_climb(cw: &CollisionWorld<'_, VoxelWorld>, m: &MapInfo) -> RunResult {
    let r = m.route("fortress_climb").expect("fortress_climb");
    let pts = r.waypoints.clone();
    let top = *pts.last().unwrap();
    let p = MoveParams::default();
    let s = PlayerState::new(pts[0], &p);
    let mut k = 1usize;
    let mut prev_jump = false;
    run(
        cw,
        m,
        6,
        s,
        30.0,
        move |s, _| {
            // Advance once standing on (or above) the current target.
            if k < pts.len() - 1 && s.grounded && s.pos.y > pts[k].y - 0.3 {
                k += 1;
            }
            let tgt = pts[k];
            let to = horiz(tgt - s.pos);
            let mut b = 0;
            if s.grounded {
                // Hop when lined up under the next step.
                if tgt.y > s.pos.y + 0.6 && to.length() < 3.4 && !prev_jump {
                    b = JUMP;
                }
            } else if !s.extra.air_jump_used && s.vel.y < 1.5 && s.pos.y < tgt.y + 0.4 && !prev_jump && s.air_time > 0.15 {
                b = JUMP;
            }
            prev_jump = b != 0;
            let mz = if to.length() > 0.35 { 1 } else { 0 };
            input(yaw_to(s.pos, tgt), 0.0, 0, mz, b)
        },
        move |s| s.grounded && s.pos.y >= top.y - 0.1,
    )
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_ember(seed);
    let mut cols = Vec::new();
    push_surf_colliders(&info.surf_lines, &mut cols);
    let cw = CollisionWorld::new(&world, &cols);
    let results = [
        ("1 facility crossing", walk(&cw, &info, "facility_cross", 1)),
        ("2 west arch bhop", hop(&cw, &info, "west_arch", 2)),
        ("3 surf east (ridge)", surf(&cw, &info, ember::SURF_EAST, Vec3::new(70.0, ember::ED_TOP + 1.0, -12.0), 3)),
        ("4 surf west (crag)", surf(&cw, &info, ember::SURF_WEST, Vec3::new(-34.0, ember::SP_TOP, -48.0), 4)),
        ("5 collector arm swing", arm_swing(&cw, &world, &info)),
        ("6 fortress climb", fortress_climb(&cw, &info)),
    ];
    println!("ember routes (seed {seed})");
    println!("{:<24} {:>7} {:>9} {:>9} {:>6} {:>6} {:>6}  result", "route", "time s", "avg m/s", "peak m/s", "surf", "hooks", "touch");
    let mut all_ok = true;
    for (k, (name, r)) in results.iter().enumerate() {
        let clean = match k {
            2 | 3 => r.surf_ticks > 120 && r.peak > 18.0,
            4 => r.touchdowns <= 1 && r.hooks > 0,
            _ => true,
        };
        let ok = r.ok && clean && !r.fell;
        all_ok &= ok;
        println!(
            "{:<24} {:>7.2} {:>9.1} {:>9.1} {:>6} {:>6} {:>6}  {}",
            name,
            r.secs,
            r.avg,
            r.peak,
            r.surf_ticks,
            r.hooks,
            r.touchdowns,
            if ok {
                "ok".to_string()
            } else if r.fell {
                format!("LAVA at {:.1?}", r.end)
            } else {
                format!("FAILED (ended at {:.1?}, clean {clean})", r.end)
            }
        );
    }
    if !all_ok {
        eprintln!("some Ember routes failed");
        std::process::exit(1);
    }
}
