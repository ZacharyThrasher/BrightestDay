//! Proves Orbit's key movement lines work with the real movement code *and* the map's gravity
//! profile, by driving `bd_sim::movement::step_player` with scripted "skilled bot" inputs through
//! `bd_voxel::generate_orbit` (gravity applied exactly like the world does: `mods.gravity_scale =
//! ArenaInfo::gravity_scale_at(pos.y)` before every step). Prints times and speeds.
//!
//!     cargo run -p bd_sim --example orbit_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **Spine bhop** — ~160 m from the east end of the Regolith Flats, up the rock bridge, straight
//!    through the station hub's doors, along the west catwalk onto the Dockyard, bunny-hopping.
//! 2. **Derelict swing chain** — Crash Site (SE, 26 m) → across the swing-only 270° gap on the
//!    derelict module → a second tether on the Comms tower → land on the Comms Array (SW, 50 m),
//!    never touching down in between.
//! 3. **Zero-g recovery** — knocked straight up off the hub roof hard enough to drift out into space
//!    (proved first: the same launch with no input is a top KO), the bot tethers the antenna mast
//!    from the zero-g band, reels in and drops back onto the station.
//!
//! Exits non-zero if any route fails. `TRACE=<route number>` prints the bot's state every 0.1 s.

use bd_core::DT;
use bd_core::input::PlayerInput;
use bd_core::voxel::VoxelQuery;
use bd_core::Vec3;
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;
use bd_voxel::maps::orbit::{HUB_ROOF, MAST_TOP, RING_TOP};
use bd_voxel::{MapInfo, VoxelWorld, generate_orbit};

// ------------------------------------------------------------------------------------ helpers

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

    fn steer_side(&mut self, vel: Vec3, route: Vec3, lim_deg: f32) {
        let h = horiz(vel);
        if h.length_squared() < 1e-4 {
            return;
        }
        let e = route.cross(h).y.atan2(route.dot(h));
        let lim = lim_deg.to_radians();
        if e > lim {
            self.side = 1.0;
        } else if e < -lim {
            self.side = -1.0;
        }
    }

    fn hop_to(&mut self, s: &PlayerState, target: Vec3, lim_deg: f32) -> PlayerInput {
        let dir = horiz(target - s.pos).normalize_or(Vec3::Z);
        if s.horizontal_speed() < self.p.run_speed * 0.95 && s.grounded {
            return input(yaw_to(s.pos, target), 0.0, 0, 1, 0);
        }
        self.steer_side(s.vel, dir, lim_deg);
        let mx: i8 = if self.side > 0.0 { 1 } else { -1 };
        let h = horiz(s.vel);
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
    peak_y: f32,
    touchdowns: u32,
    hooks: u32,
    fell: bool,
    space: bool,
    end: Vec3,
}

/// Step the bot with the map's gravity profile. Fails on falling below the void or leaving through
/// the top blast zone (`top_y`).
#[allow(clippy::too_many_arguments)]
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
    let top_y = world.bounds().max.y + 40.0; // SmashTuning::blast_margin_top default
    let trace = std::env::var("TRACE").is_ok_and(|v| v == route.to_string());
    let mut r = RunResult { peak_y: s.pos.y, ..Default::default() };
    let mut dist = 0.0;
    let mut hooked = false;
    for t in 0..(max_secs / DT) as usize {
        let mut i = ctrl(&s, t);
        i.seq = t as u32;
        let prev = s;
        s.mods.gravity_scale = info.gravity_scale_at(s.pos.y); // the world's MAP HOOK
        let e = step_player(&mut s, &i, &cw, &p, DT);
        dist += horiz(s.pos - prev.pos).length();
        r.peak = r.peak.max(s.speed());
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
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) g={:.2} gr={} hooks={:?}/{:?} btn={:#x}",
                t as f32 * DT,
                s.pos.x,
                s.pos.y,
                s.pos.z,
                s.vel.x,
                s.vel.y,
                s.vel.z,
                s.mods.gravity_scale,
                s.grounded as u8,
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
        if s.pos.y > top_y {
            r.space = true;
            break;
        }
        if goal(&s, t) {
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

// ------------------------------------------------------------------------------------ routes

/// 1. Flats → hub doors → catwalk → Dockyard, bunny-hopping the whole spine.
fn spine(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("spine_bhop").unwrap();
    let a = r.waypoints[0];
    let end = *r.waypoints.last().unwrap();
    let p = MoveParams::default();
    let mut s = PlayerState::new(a, &p);
    s.vel = Vec3::ZERO;
    let mut hop = Hopper::new();
    let mut first = true;
    run(
        w,
        m,
        1,
        s,
        25.0,
        move |s, _| {
            if first {
                first = false;
                return input(yaw_to(s.pos, end), 0.0, 0, 0, 0);
            }
            // Lane keeping: pursue a point 20 m ahead on the centre line (z = 0).
            hop.hop_to(s, Vec3::new(s.pos.x - 20.0, s.pos.y, 0.0), 8.0)
        },
        move |s, _| s.pos.x <= end.x + 2.0 && s.pos.y > 40.0,
    )
}

/// 2. Crash Site → derelict (270° gap) → Comms tower → land on the Comms Array.
fn derelict_chain(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("derelict_swing").unwrap();
    let (start, goal_p) = (r.waypoints[0], r.waypoints[1]);
    let (derelict, tower) = (r.anchors[0], r.anchors[1]);
    let comms = m.district("comms").unwrap().center;
    let p = MoveParams::default();
    let s = PlayerState::new(start, &p);
    let mut phase = 0u8;
    let mut anchor = Vec3::ZERO;
    let mut idle = false;
    let mut t_phase = 0usize;
    run(
        w,
        m,
        2,
        s,
        25.0,
        move |s, t| {
            let eye = s.eye(&p);
            match phase {
                0 => {
                    // Sprint toward the gap; jump off the rim and fire at the derelict's belly.
                    let to = horiz(derelict - s.pos);
                    if !s.grounded && s.vel.y < 2.0 && to.length() < 26.0 {
                        phase = 1;
                        t_phase = t;
                        anchor = crosshair(w, eye, derelict).unwrap_or(derelict);
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    let mut i = input(yaw_to(s.pos, derelict), 0.0, 0, 1, 0);
                    if s.grounded && to.length() < 30.0 && s.horizontal_speed() > p.run_speed * 0.9 {
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
                    // Reel + boost toward the far side; let go past the anchor, rising.
                    let past = s.pos.x < anchor.x - 2.0;
                    if (past && s.vel.y > -2.0) || (h.pos - s.chest(&p)).length() < 4.0 {
                        phase = 2;
                        t_phase = t;
                        return input(yaw_to(s.pos, comms), 0.0, 0, 1, 0);
                    }
                    let (y, pc) = aim_at(eye, h.pos + Vec3::new(-6.0, 0.0, 0.0));
                    input(y, pc, 0, 1, TETHER_L | REEL | if s.vel.y < 0.0 { BOOST } else { 0 })
                }
                2 => {
                    // Flying free: second tether on the Comms tower.
                    if t > t_phase + 6 {
                        phase = 3;
                        t_phase = t;
                        anchor = crosshair(w, eye, tower).unwrap_or(tower);
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, TETHER_R);
                    }
                    input(yaw_to(s.pos, comms), 0.0, 0, 1, 0)
                }
                3 => {
                    let h = s.hooks[1];
                    if !h.attached() {
                        idle = !idle;
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, if idle && h.phase == HookPhase::Idle { 0 } else { TETHER_R });
                    }
                    // Reel up over the island, then let go and drop onto it.
                    let over = horiz(s.pos - comms).length() < 11.0;
                    if (over && s.pos.y > comms.y + 2.0) || (h.pos - s.chest(&p)).length() < 3.5 {
                        phase = 4;
                    }
                    let (y, pc) = aim_at(eye, h.pos);
                    input(y, pc, 0, 1, TETHER_R | REEL)
                }
                _ => input(yaw_to(s.pos, goal_p), 0.0, 0, 1, 0),
            }
        },
        move |s, _| s.grounded && s.pos.y > comms.y - 3.0 && horiz(s.pos - comms).length() < 16.0,
    )
}

/// 3. Launched straight up off the hub roof: `recover = false` proves the launch is lethal (top
/// blast zone), `true` tethers the mast from the zero-g band and lands back on the station.
fn zero_g(w: &VoxelWorld, m: &MapInfo, recover: bool) -> RunResult {
    let r = m.route("zero_g_recovery").unwrap();
    let start = r.waypoints[0];
    let mast = r.anchors[0];
    let p = MoveParams::default();
    let mut s = PlayerState::new(start, &p);
    s.vel = Vec3::new(0.0, 44.0, 1.5);
    s.grounded = false;
    s.tumble = 1.2; // a launch: reduced air control
    let mut phase = 0u8;
    let mut idle = false;
    run(
        w,
        m,
        if recover { 3 } else { 30 },
        s,
        20.0,
        move |s, _| {
            if !recover {
                return input(0.0, 0.0, 0, 0, 0);
            }
            let eye = s.eye(&p);
            match phase {
                0 => {
                    // Drift off the mast's axis while rising (air control), then — deep in the band —
                    // fire at the mast's side below the beacon.
                    if s.pos.y > 92.0 && s.tumble <= 0.0 && horiz(s.pos).length() > 3.0 {
                        phase = 1;
                    }
                    input(yaw_to(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0)), 0.0, 0, 1, 0)
                }
                1 => {
                    let h = s.hooks[0];
                    if !h.attached() {
                        idle = !idle;
                        let (y, pc) = aim_at(eye, mast);
                        return input(y, pc, 0, 0, if idle && h.phase == HookPhase::Idle { 0 } else { TETHER_L });
                    }
                    // Reel down to the mast; let go once we're heading down under full gravity.
                    if s.pos.y < 80.0 || s.grounded || (h.pos - s.chest(&p)).length() < 3.5 {
                        phase = 2;
                    }
                    let (y, pc) = aim_at(eye, h.pos);
                    input(y, pc, 0, 0, TETHER_L | REEL)
                }
                _ => input(yaw_to(s.pos, Vec3::new(0.0, 0.0, 4.0)), 0.0, 0, 1, 0),
            }
        },
        move |s, _| recover && s.grounded && (RING_TOP - 1.0..=HUB_ROOF + 2.0).contains(&s.pos.y),
    )
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_orbit(seed);
    let lethal = zero_g(&world, &info, false);
    let results = [
        ("1 spine bhop", spine(&world, &info)),
        ("2 derelict swing chain", derelict_chain(&world, &info)),
        ("3 zero-g mast recovery", zero_g(&world, &info, true)),
    ];
    println!("orbit routes (seed {seed}); mast top {MAST_TOP} m");
    println!("{:<24} {:>7} {:>9} {:>9} {:>7} {:>6} {:>6}  result", "route", "time s", "avg m/s", "peak m/s", "peak y", "hooks", "touch");
    let mut all_ok = true;
    for (name, r) in &results {
        all_ok &= r.ok;
        println!(
            "{:<24} {:>7.2} {:>9.1} {:>9.1} {:>7.1} {:>6} {:>6}  {}{}",
            name,
            r.secs,
            r.avg,
            r.peak,
            r.peak_y,
            r.hooks,
            r.touchdowns,
            if r.ok {
                "ok"
            } else if r.fell {
                "FELL (re-entry)"
            } else if r.space {
                "LOST IN SPACE"
            } else {
                "FAILED"
            },
            if r.ok { String::new() } else { format!(" (ended at {:.1?})", r.end) }
        );
    }
    println!(
        "  same launch, no input: {} after {:.1} s (peak y {:.0})",
        if lethal.space { "launched into space (top KO)" } else { "NOT a KO" },
        lethal.secs,
        lethal.peak_y
    );
    // Route 2 must never touch down between the two tethers.
    if results[1].1.ok && results[1].1.touchdowns > 1 {
        eprintln!("derelict chain touched down {} times", results[1].1.touchdowns);
        all_ok = false;
    }
    if !lethal.space {
        eprintln!("the recovery test launch is not lethal without the tether");
        all_ok = false;
    }
    if !all_ok {
        eprintln!("some Orbit routes failed");
        std::process::exit(1);
    }
}
