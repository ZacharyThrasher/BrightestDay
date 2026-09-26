//! Proves the Valley of the End's key movement lines with the real movement code: scripted "skilled
//! bot" inputs drive `bd_sim::movement::step_player` through `bd_voxel` `generate_valley`, colliding
//! with the map's surf lines and walkable water exactly like the sim does.
//!
//!     cargo run -p bd_sim --features valley --example valley_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **Canyon surf** — drop onto the west surf line and ride it down along the curved gorge wall,
//!    holding the face, then land on the gorge floor.
//! 2. **Falls climb** — from the bank in the corner of the falls and the east buttress, chain wall
//!    kicks (every touch of the falls or the buttress gives the air jump back) up 43 m to the pool.
//! 3. **Statue swing** — off the west statue's shoulder, tether the east statue's head, reel across
//!    the plunge hole and land on the east statue (shoulder, arm or head).
//! 4. **River run** — bunny-hop down the river *on the water* (water-walk), statue bay to the lip.
//!
//! Exits non-zero if a route fails. `TRACE=<route number>` prints the bot's state every 0.1 s.

use bd_core::DT;
use bd_core::Vec3;
use bd_core::input::PlayerInput;
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;
use bd_voxel::MapInfo;
use bd_voxel::VoxelWorld;
use bd_voxel::maps::valley::{self, generate_valley, statue_specs};

fn horiz(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = horiz(to - from).normalize_or(Vec3::NEG_Z);
    (-d.x).atan2(-d.z)
}

/// The map's colliders as the sim builds them (surf lines + walkable water).
fn map_colliders(info: &MapInfo) -> Vec<Collider> {
    let mut cols = Vec::new();
    push_surf_colliders(&info.surf_lines, &mut cols);
    push_water_colliders(&info.arena, &mut cols);
    cols
}

#[derive(Default, Debug)]
struct RunResult {
    ok: bool,
    secs: f32,
    peak: f32,
    rise: f32,
    avg: f32,
    hooks: u32,
    refreshes: u32,
    surf_ticks: u32,
    fell: bool,
    end: Vec3,
}

#[allow(clippy::too_many_arguments)]
fn run(
    world: &VoxelWorld,
    info: &MapInfo,
    cols: &[Collider],
    route: usize,
    mut s: PlayerState,
    max_secs: f32,
    mut ctrl: impl FnMut(&PlayerState, usize) -> PlayerInput,
    goal: impl Fn(&PlayerState, usize) -> bool,
) -> RunResult {
    let cw = CollisionWorld::new(world, cols);
    let p = MoveParams::default();
    let trace = std::env::var("TRACE").is_ok_and(|v| v == route.to_string());
    let mut r = RunResult::default();
    let start_y = s.pos.y;
    let mut top = s.pos.y;
    let mut dist = 0.0;
    for t in 0..(max_secs / DT) as usize {
        let mut i = ctrl(&s, t);
        i.seq = t as u32;
        let prev = s;
        let e = step_player(&mut s, &i, &cw, &p, DT);
        dist += horiz(s.pos - prev.pos).length();
        r.peak = r.peak.max(s.horizontal_speed());
        top = top.max(s.pos.y);
        r.hooks += e.tether_attached.iter().filter(|a| **a).count() as u32;
        r.refreshes += e.wall_refresh.is_some() as u32;
        r.surf_ticks += surfing(&s, &p) as u32;
        if trace && t % 12 == 0 {
            println!(
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) gr={} aj={} hooks={:?}/{:?}",
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
    r.rise = top - start_y;
    r.end = s.pos;
    r
}

/// 1. Canyon surf: onto the west line a quarter of the way down, hold the face, ride it out.
fn canyon_surf(w: &VoxelWorld, m: &MapInfo, cols: &[Collider]) -> RunResult {
    let l = m.surf_lines[0];
    let faces = surf_line(l.a, l.b, l.face_deg, l.width, 1.5, 0);
    // The inner face (toward the gorge centre) is the one we ride.
    let f = faces.iter().copied().max_by(|a, b| (a.axes()[1].x * -l.a.x.signum()).total_cmp(&(b.axes()[1].x * -l.a.x.signum()))).unwrap();
    let d = (l.b - l.a).normalize();
    let n = f.axes()[1];
    let down = f.axes()[0];
    let start = l.a + (l.b - l.a) * 0.2 + down * 2.5 + n * 0.9 + Vec3::new(0.0, 0.05, 0.0);
    let p = MoveParams::default();
    let mut s = PlayerState::new(start - Vec3::Y * 0.9, &p);
    s.vel = d * 10.0;
    s.air_time = 0.5;
    let yaw = (-d.x).atan2(-d.z);
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let into = Vec3::new(-n.x, 0.0, -n.z);
    let mx: i8 = if into.dot(right) > 0.0 { 1 } else { -1 };
    let len = (l.b - l.a).length();
    run(
        w,
        m,
        cols,
        1,
        s,
        8.0,
        move |s, _| if (s.pos - l.a).dot(d) < len * 0.97 { input(yaw, 0.0, mx, 0, 0) } else { input(yaw, 0.0, 0, 1, 0) },
        move |s, _| s.grounded && (s.pos - l.a).dot(d) > len * 0.6,
    )
}

/// 2. Falls climb: kick between the falls (facing −Z) and the buttress (facing −X), air-jumping every
/// time a touch gives it back.
fn falls_climb(w: &VoxelWorld, m: &MapInfo, cols: &[Collider]) -> RunResult {
    let r = m.route("falls_climb").expect("falls_climb");
    let (start, goal_p) = (r.waypoints[0], r.waypoints[1]);
    let p = MoveParams::default();
    let s = PlayerState::new(start, &p);
    let mut kick = 0u32;
    let mut was_used = false;
    run(
        w,
        m,
        cols,
        2,
        s,
        30.0,
        move |s, t| {
            // Near the top: go over the lip onto the pool.
            if s.pos.y > valley::POOL_Y + 0.8 {
                return input(yaw_to(s.pos, goal_p), 0.0, 0, 1, 0);
            }
            // Face north (into the falls); alternate pushing into the falls (W) and into the
            // buttress (D) between kicks.
            let into_falls = kick % 2 == 0;
            // Facing north (+Z, yaw π) the strafe-right axis points −X, so strafe *left* pushes +X.
            let (mx, mz) = if into_falls { (0, 1) } else { (-1, 0) };
            let mut i = input(std::f32::consts::PI, 0.0, mx, mz, 0);
            if s.grounded && t > 6 {
                i.buttons |= JUMP;
            } else if !s.grounded && was_used && !s.extra.air_jump_used {
                // Refreshed by a touch: jump again at once.
                i.buttons |= JUMP;
                kick += 1;
            } else if !s.grounded && !s.extra.air_jump_used && s.vel.y < 1.0 {
                i.buttons |= JUMP;
                kick += 1;
            }
            was_used = s.extra.air_jump_used;
            i
        },
        move |s, _| s.grounded && s.pos.y > valley::POOL_Y - 0.6,
    )
}

/// 3. Statue swing: jump off the west statue's north shoulder, tether the east statue's head, reel
/// across and land on the east statue.
fn statue_swing(w: &VoxelWorld, m: &MapInfo, cols: &[Collider]) -> RunResult {
    let r = m.route("statue_swing").expect("statue_swing");
    let (start, land) = (r.waypoints[0], r.waypoints[1]);
    let anchor_hint = r.anchors[0];
    let [_, east] = statue_specs();
    let p = MoveParams::default();
    let s = PlayerState::new(start + Vec3::Y * 0.1, &p);
    let mut phase = 0u8;
    let mut anchor = anchor_hint;
    let mut idle = false;
    run(
        w,
        m,
        cols,
        3,
        s,
        20.0,
        move |s, t| {
            let eye = s.eye(&p);
            match phase {
                0 => {
                    let mut i = input(yaw_to(s.pos, land), 0.0, 0, 1, 0);
                    if s.grounded && t > 30 {
                        i.buttons |= JUMP;
                    }
                    if !s.grounded && t > 36 {
                        phase = 1;
                        anchor = bd_core::voxel::raycast(w, eye, anchor_hint - eye, 120.0).map_or(anchor_hint, |h| h.point);
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, TETHER_L);
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
                    // Reel in; let go once over the east statue and drop onto it.
                    let over = s.pos.x > east.base.x - 7.0;
                    if over {
                        phase = 2;
                        return input(yaw_to(s.pos, land), 0.0, 0, 1, 0);
                    }
                    let (y, pc) = aim_at(eye, h.pos);
                    input(y, pc, 0, 1, TETHER_L | REEL | if s.vel.y < -2.0 { BOOST } else { 0 })
                }
                _ => input(yaw_to(s.pos, land), 0.0, 0, 1, 0),
            }
        },
        move |s, _| s.grounded && s.pos.x > east.base.x - 10.0 && s.pos.y > east.base.y + 30.0,
    )
}

/// 4. River run: bunny-hop down the river on the water.
fn river_run(w: &VoxelWorld, m: &MapInfo, cols: &[Collider]) -> RunResult {
    let r = m.route("river_run").expect("river_run");
    let pts = r.waypoints.clone();
    let p = MoveParams::default();
    let s = PlayerState::new(pts[0] + Vec3::Y * 0.02, &p);
    let end = *pts.last().unwrap();
    let mut side = 1.0f32;
    run(
        w,
        m,
        cols,
        4,
        s,
        40.0,
        move |s, _| {
            // Pursue the route line (it weaves round the rock stacks) 14 m ahead.
            let z = s.pos.z - 14.0;
            let k = pts.iter().position(|q| q.z < z).unwrap_or(pts.len() - 1).max(1);
            let (a, b) = (pts[k - 1], pts[k]);
            let t = ((z - a.z) / (b.z - a.z)).clamp(0.0, 1.0);
            let aim = Vec3::new(a.x + (b.x - a.x) * t, s.pos.y, z);
            let dir = horiz(aim - s.pos).normalize_or(Vec3::NEG_Z);
            if s.horizontal_speed() < p.run_speed * 0.95 && s.grounded {
                return input(yaw_to(s.pos, aim), 0.0, 0, 1, 0);
            }
            let h = horiz(s.vel);
            let e = dir.cross(h).y.atan2(dir.dot(h));
            if e > 0.14 {
                side = 1.0;
            } else if e < -0.14 {
                side = -1.0;
            }
            let mx: i8 = if side > 0.0 { 1 } else { -1 };
            input(optimal_strafe_yaw(s.vel, &p, DT, mx, 0, side), 0.0, mx, 0, JUMP)
        },
        move |s, _| s.pos.z < end.z + 2.0 && (s.pos.y - valley::RIVER_Y).abs() < 1.2,
    )
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_valley(seed);
    let cols = map_colliders(&info);
    let results = [
        ("1 canyon surf", canyon_surf(&world, &info, &cols)),
        ("2 falls climb", falls_climb(&world, &info, &cols)),
        ("3 statue swing", statue_swing(&world, &info, &cols)),
        ("4 river run (water)", river_run(&world, &info, &cols)),
    ];
    let run_speed = MoveParams::default().run_speed;
    println!("valley routes (seed {seed}); run speed {run_speed:.1} m/s");
    println!(
        "{:<22} {:>7} {:>8} {:>9} {:>7} {:>5} {:>6} {:>6}  result",
        "route", "time s", "avg m/s", "peak m/s", "rise m", "hooks", "kicks", "surf"
    );
    let mut all_ok = true;
    for (k, (name, r)) in results.iter().enumerate() {
        let extra = match k {
            0 => r.surf_ticks > 60 && r.peak > 18.0,
            1 => r.refreshes >= 4 && r.rise > 40.0,
            2 => r.hooks >= 1,
            _ => r.avg > run_speed * 1.05,
        };
        let ok = r.ok && extra;
        all_ok &= ok;
        println!(
            "{:<22} {:>7.2} {:>8.1} {:>9.1} {:>7.1} {:>5} {:>6} {:>6}  {}",
            name,
            r.secs,
            r.avg,
            r.peak,
            r.rise,
            r.hooks,
            r.refreshes,
            r.surf_ticks,
            if ok {
                "ok".to_string()
            } else if r.fell {
                "FELL".to_string()
            } else {
                format!("FAILED (ended at {:.1?}, goal {}, check {extra})", r.end, r.ok)
            }
        );
    }
    if !all_ok {
        eprintln!("some Valley routes failed");
        std::process::exit(1);
    }
}
