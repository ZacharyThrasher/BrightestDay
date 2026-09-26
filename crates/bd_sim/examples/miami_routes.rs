//! Proves the Miami map's key movement routes are traversable with the real movement code, by driving
//! `bd_sim::movement::step_player` with scripted "skilled bot" inputs through `bd_voxel::generate_miami`,
//! and prints times and speeds.
//!
//!     cargo run -p bd_sim --example miami_routes --release [-- seed]
//!
//! Routes (see `MapInfo::routes`):
//! 1. **Ocean Drive bhop** — 180 m straight down the boulevard, bunny-hopping between the parked cars.
//! 2. **Brickell canyon swing chain** — from the stadium end up Brickell Ave, swinging from skybridge
//!    to skybridge (gas-boosting the downswings) without touching the street.
//! 3. **North causeway** — Brickell → canal bridge → quay → the arch span over the bay → Ocean Drive.
//! 4. **Beach → port** — from the sand across the park, Ocean Drive and the gateway alley to the bay
//!    wall, then a tether onto the cruise ship (the central landmark), landing on a deck.
//!
//! Exits non-zero if any route fails, so it doubles as a regression test for the layout and tuning.
//! `TRACE=<route number>` prints the bot's state every 0.1 s.

use bd_core::DT;
use bd_core::input::PlayerInput;
use bd_core::voxel::VoxelQuery;
use bd_core::{IVec3, Vec3};
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;
use bd_voxel::{MapInfo, VoxelWorld, generate_miami};

// ------------------------------------------------------------------------------------ helpers

fn horiz(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = horiz(to - from).normalize_or(Vec3::NEG_Z);
    (-d.x).atan2(-d.z)
}

/// Where a ballistic body (no air control) crosses height `h` on the way down.
fn predict_landing(pos: Vec3, vel: Vec3, h: f32, g: f32) -> Option<Vec3> {
    let disc = vel.y * vel.y + 2.0 * g * (pos.y - h);
    if disc < 0.0 {
        return None;
    }
    let t = (vel.y + disc.sqrt()) / g;
    Some(pos + horiz(vel) * t)
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

    /// Run until at run speed, then perfect air-strafe with held jump toward `target`.
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

// ------------------------------------------------------------------------------------ runner

#[derive(Default, Debug)]
struct RunResult {
    ok: bool,
    secs: f32,
    peak: f32,
    avg: f32,
    /// Ground contacts after the first tether attached (swing routes: 0 = never touched down).
    touchdowns: u32,
    hooks: u32,
    fell: bool,
    end: Vec3,
    note: String,
}

#[allow(clippy::too_many_arguments)]
fn run(
    world: &VoxelWorld,
    route: usize,
    start: Vec3,
    yaw: f32,
    max_secs: f32,
    mut ctrl: impl FnMut(&PlayerState, usize) -> PlayerInput,
    goal: impl Fn(&PlayerState) -> bool,
    void_y: f32,
) -> RunResult {
    let cw = CollisionWorld::new(world, &[]);
    let p = MoveParams::default();
    let mut s = PlayerState::new(start, &p);
    let trace = std::env::var("TRACE").is_ok_and(|v| v == route.to_string());
    let mut r = RunResult::default();
    let mut dist = 0.0;
    let mut hooked = false;
    let max_ticks = (max_secs / DT) as usize;
    for t in 0..max_ticks {
        let mut i = if t < 2 { input(yaw, 0.0, 0, 0, 0) } else { ctrl(&s, t) };
        i.seq = t as u32;
        let prev = s;
        let e = step_player(&mut s, &i, &cw, &p, DT);
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
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) |v|={:4.1} gr={} hooks={:?}/{:?} btn={:#x}",
                t as f32 * DT,
                s.pos.x,
                s.pos.y,
                s.pos.z,
                s.vel.x,
                s.vel.y,
                s.vel.z,
                s.speed(),
                s.grounded as u8,
                s.hooks[0].phase,
                s.hooks[1].phase,
                i.buttons
            );
        }
        if s.pos.y < void_y {
            r.fell = true;
            r.secs = t as f32 * DT;
            break;
        }
        if goal(&s) {
            r.ok = true;
            r.secs = (t + 1) as f32 * DT;
            break;
        }
        r.secs = (t + 1) as f32 * DT;
    }
    r.avg = dist / r.secs.max(1e-3);
    r.end = s.pos;
    r
}

/// Aim helper: the first solid point along the eye ray toward `target` (what the crosshair shows).
fn crosshair<V: VoxelQuery>(w: &V, eye: Vec3, target: Vec3) -> Option<Vec3> {
    bd_core::voxel::raycast(w, eye, target - eye, 120.0).map(|h| h.point)
}

// ------------------------------------------------------------------------------------ routes

/// 1. Ocean Drive, south → north, bunny-hopping the carriageway between the parked cars.
fn ocean_drive(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("ocean_drive_bhop").unwrap();
    let (a, b) = (r.waypoints[0], r.waypoints[1]);
    let mut hop = Hopper::new();
    run(
        w,
        1,
        a,
        yaw_to(a, b),
        20.0,
        |s, _| {
            // Lane keeping: pursue a point 22 m ahead on the centre line.
            let target = Vec3::new(a.x, a.y, s.pos.z + 22.0);
            hop.hop_to(s, target, 10.0)
        },
        |s| s.pos.z >= b.z - 4.0,
        m.void_y,
    )
}

/// 2. Brickell canyon: run up the avenue, then skybridge → skybridge swings, never touching down.
fn canyon(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("brickell_canyon").unwrap();
    let start = r.waypoints[0];
    let end = *r.waypoints.last().unwrap();
    let anchors = r.anchors.clone();
    let floor = start.y;
    let mut hop = Hopper::new();
    let p = MoveParams::default();
    let mut next = 0usize; // anchor index
    let mut phase = 0u8; // 0 approach, 1 firing/swinging, 2 released (between anchors)
    let mut release_tick = 0usize;
    let mut idle_tick = false;
    run(
        w,
        2,
        start,
        0.0,
        30.0,
        move |s, t| {
            let lane = |z: f32| Vec3::new(start.x, floor, z);
            let hooked = s.hooks[0].attached() || s.hooks[0].phase == HookPhase::Flying;
            match phase {
                0 => {
                    // Build speed, then jump and fire at the first skybridge once it's ~24 m ahead.
                    let a = anchors[next];
                    if s.pos.z > a.z - 24.0 && !s.grounded && s.vel.y < 1.0 {
                        phase = 1;
                        let (y, pc) = aim_at(s.eye(&p), a);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    hop.hop_to(s, lane(s.pos.z + 20.0), 10.0)
                }
                1 => {
                    let a = anchors[next];
                    if !hooked {
                        // Missed or not yet fired: release a tick, then fire again.
                        idle_tick = !idle_tick;
                        let (y, pc) = aim_at(s.eye(&p), a);
                        return input(y, pc, 0, 0, if idle_tick { 0 } else { TETHER_L });
                    }
                    if !s.hooks[0].attached() {
                        let (y, pc) = aim_at(s.eye(&p), a);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    // Release on the upswing past the anchor once climbing steeply enough.
                    let hp = s.hooks[0].pos;
                    if s.pos.z > hp.z + 2.0 && s.vel.y > 0.3 * s.speed() {
                        phase = 2;
                        release_tick = t;
                        next += 1;
                        return input(yaw_to(s.pos, lane(s.pos.z + 30.0)), 0.0, 0, 1, 0);
                    }
                    swing_input(s, &p, TETHER_L, floor, true)
                }
                _ => {
                    // Flying between anchors: fire at the next one as the arc tops out.
                    if next >= anchors.len() {
                        return input(yaw_to(s.pos, end), 0.0, 0, 1, 0);
                    }
                    let a = anchors[next];
                    let near = a.z - s.pos.z < 34.0;
                    // Only take the next anchor if the current arc wouldn't carry us past it anyway.
                    let land = predict_landing(s.pos, s.vel, floor, p.gravity);
                    let reaches_goal = land.is_some_and(|l| l.z > end.z - 6.0);
                    let short = land.is_none_or(|l| l.z < a.z + 8.0);
                    if reaches_goal {
                        return input(yaw_to(s.pos, end), 0.0, 0, 1, 0);
                    }
                    if (short && s.vel.y < 2.0 && near && t > release_tick + 12) || s.pos.y < floor + 4.0 {
                        phase = 1;
                        let (y, pc) = aim_at(s.eye(&p), a);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    input(yaw_to(s.pos, lane(s.pos.z + 30.0)), 0.0, 0, 1, 0)
                }
            }
        },
        move |s| s.pos.z >= end.z - 8.0,
        m.void_y,
    )
}

/// 3. North causeway: Brickell → canal bridge → quay → arch span → Ocean Drive, hopping.
fn causeway(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("north_causeway").unwrap();
    let a = r.waypoints[0];
    let b = *r.waypoints.last().unwrap();
    let mut hop = Hopper::new();
    run(
        w,
        3,
        a,
        yaw_to(a, b),
        20.0,
        |s, _| hop.hop_to(s, Vec3::new(s.pos.x + 22.0, a.y, a.z), 10.0),
        |s| s.pos.x >= b.x - 5.0 && s.pos.y > m.water_y + 1.0,
        m.void_y,
    )
}

/// 4. Beach → cruise ship: run/hop west through the park, across Ocean Drive and the gateway alley
///    to the bay wall, jump, tether the crane boom over the ship, reel up and drop onto a deck.
fn beach_to_ship(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let r = m.route("beach_to_ship").unwrap();
    let start = r.waypoints[0];
    let wall = r.waypoints[2];
    let crane = m.route("crane_swing").unwrap().anchors[0];
    let deck_top = r.waypoints[3];
    let p = MoveParams::default();
    let mut hop = Hopper::new();
    let mut phase = 0u8;
    let mut anchor = Vec3::ZERO;
    let ship = m.district("cruise_ship").unwrap().area;
    run(
        w,
        4,
        start,
        yaw_to(start, wall),
        25.0,
        move |s, _| {
            match phase {
                0 => {
                    // Along z = 0 to the bay wall; jump off it.
                    if s.pos.x < wall.x + 1.5 {
                        phase = 1;
                        // Aim at the crane boom (sill beam underside) above the ship.
                        let target = Vec3::new(2.0, crane.y + 0.05, crane.z + 3.5);
                        anchor = crosshair(w, s.eye(&p), target).unwrap_or(target);
                        let (y, pc) = aim_at(s.eye(&p), anchor);
                        return input(y, pc, 0, 0, TETHER_L | JUMP);
                    }
                    let mut i = hop.hop_to(s, Vec3::new(s.pos.x - 20.0, wall.y, 0.0), 8.0);
                    if s.pos.x < wall.x + 5.0 && s.grounded {
                        i.buttons |= JUMP;
                    }
                    i
                }
                1 => {
                    let h = s.hooks[0];
                    if h.phase == HookPhase::Idle {
                        let (y, pc) = aim_at(s.eye(&p), anchor);
                        return input(y, pc, 0, 0, 0); // re-arm, fire next tick
                    }
                    if !h.attached() {
                        let (y, pc) = aim_at(s.eye(&p), anchor);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    // Reel up; release once above the sun deck and over the ship.
                    let over = s.pos.x < ship[2] - 3.0;
                    if (over && s.pos.y > deck_top.y + 1.5) || (h.pos - s.chest(&p)).length() < 3.0 {
                        phase = 2;
                    }
                    let (y, pc) = aim_at(s.eye(&p), h.pos);
                    input(y, pc, 0, 1, TETHER_L | REEL)
                }
                _ => {
                    // Drift toward the middle of the sun deck.
                    let land = predict_landing(s.pos, s.vel, deck_top.y, p.gravity);
                    let _ = land;
                    input(yaw_to(s.pos, Vec3::new(-2.5, 0.0, -4.0)), 0.0, 0, 1, 0)
                }
            }
        },
        move |s| s.grounded && s.pos.y >= 11.5 && s.pos.x > ship[0] && s.pos.x < ship[2] && s.pos.z > ship[1] && s.pos.z < ship[3],
        m.void_y,
    )
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_miami(seed);
    let _ = IVec3::ZERO;
    let results = [
        ("1 ocean drive bhop", ocean_drive(&world, &info)),
        ("2 brickell swing chain", canyon(&world, &info)),
        ("3 north causeway", causeway(&world, &info)),
        ("4 beach -> cruise ship", beach_to_ship(&world, &info)),
    ];
    println!("miami routes (seed {seed})");
    println!("{:<24} {:>7} {:>9} {:>9} {:>6} {:>6}  result", "route", "time s", "avg m/s", "peak m/s", "hooks", "touch");
    let mut all_ok = true;
    for (name, r) in &results {
        all_ok &= r.ok;
        println!(
            "{:<24} {:>7.2} {:>9.1} {:>9.1} {:>6} {:>6}  {}{}",
            name,
            r.secs,
            r.avg,
            r.peak,
            r.hooks,
            r.touchdowns,
            if r.ok {
                "ok"
            } else if r.fell {
                "FELL IN THE WATER"
            } else {
                "FAILED"
            },
            if r.ok { String::new() } else { format!(" (ended at {:.1?}) {}", r.end, r.note) }
        );
    }
    if !all_ok {
        eprintln!("some Miami routes failed");
        std::process::exit(1);
    }
}
