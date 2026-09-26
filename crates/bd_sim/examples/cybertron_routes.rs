//! Proves Cybertron's key movement lines with the real movement code: scripted "skilled bot" inputs
//! drive `bd_sim::movement::step_player` through `bd_voxel` `generate_cybertron` (surf lines and,
//! where noted, the extended transforming panels as colliders), plus the space bridge through a
//! real `World`.
//!
//!     cargo run -p bd_sim --features cybertron --example cybertron_routes --release [-- seed]
//!
//! 1. **Central lane** — Hall of Records → round the Well's north lip → the pit's west gate, bhop.
//! 2. **North expressway** — Iacon boulevard, up the ramp, along the deck, over the Rust Gap
//!    (kicker jump over the rust bay), down into Kaon: the vehicle loop's hardest stretch, bhop.
//! 3. **Well swing** — tether the crown ring and swing across the Well of AllSparks.
//! 4. **Surf south** — off the south expressway onto the surf line, ride it down onto the ring deck.
//! 5. **Well Span** — bhop straight across the Well on the extended span (a bonus route).
//! 6. **Space bridge** — walk into the Iacon gate, come out of the Kaon gate (World + portals).
//!
//! Exits non-zero if a route fails. `TRACE=<route number>` prints the bot's state every 0.1 s.

use bd_core::input::PlayerInput;
use bd_core::voxel::VoxelQuery;
use bd_core::{DT, PlayerId, Vec3};
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;
use bd_sim::world::map_mech::{MapMechState, MoverPhase, MoverState, push_mover_colliders};
use bd_sim::{SimConfig, World};
use bd_voxel::maps::cybertron::{self, CROWN_R, CROWN_Y, STREET, generate_cybertron};
use bd_voxel::{MapInfo, VoxelWorld};

fn horiz(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = horiz(to - from).normalize_or(Vec3::NEG_Z);
    (-d.x).atan2(-d.z)
}

/// Bunny-hop / strafe controller steering toward a target (pursuit + zig-zag air-strafe).
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
                "  t={:5.2} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) gr={} surf={} hooks={:?}",
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
    bd_core::voxel::raycast(w, eye, target - eye, 140.0).map(|h| h.point)
}

/// Point `ahead` metres further along a polyline from the closest point to `p`.
fn pursue(pts: &[Vec3], p: Vec3, ahead: f32) -> Vec3 {
    let (mut best, mut best_s, mut acc) = (f32::MAX, 0.0, 0.0);
    let mut lens = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let l = horiz(b - a).length();
        let t = (horiz(p - a).dot(horiz(b - a)) / (l * l)).clamp(0.0, 1.0);
        let d = horiz(a + (b - a) * t - p).length();
        if d < best {
            best = d;
            best_s = acc + t * l;
        }
        acc += l;
        lens.push(l);
    }
    let mut s = (best_s + ahead).min(acc);
    for (k, l) in lens.iter().enumerate() {
        if s <= *l {
            return pts[k] + (pts[k + 1] - pts[k]) * (s / l);
        }
        s -= l;
    }
    *pts.last().unwrap()
}

/// Bunny-hop along a route's waypoints; done within 3 m of the last one at its height.
fn hop_route(w: &VoxelWorld, m: &MapInfo, cols: &[Collider], name: &str, n: usize, max_secs: f32) -> RunResult {
    let r = m.route(name).unwrap_or_else(|| panic!("route {name}"));
    let pts = r.waypoints.clone();
    let p = MoveParams::default();
    let s = PlayerState::new(pts[0], &p);
    let end = *pts.last().unwrap();
    let mut hop = Hopper::new();
    run(
        w,
        m,
        cols,
        n,
        s,
        max_secs,
        move |s, _| {
            let aim = pursue(&pts, s.pos, 14.0);
            let mut i = hop.hop_to(s, Vec3::new(aim.x, s.pos.y, aim.z), 8.0);
            // Never walk off a lip: jump at it (the Rust Gap).
            if s.grounded {
                let dir = horiz(s.vel).normalize_or(horiz(aim - s.pos).normalize_or(Vec3::X));
                let ground = |d: f32| (0..6).any(|k| w.is_solid(bd_core::voxel::world_to_voxel(s.pos + dir * d - Vec3::Y * (0.25 + k as f32 * 0.5))));
                if !ground(1.0) {
                    i.buttons |= JUMP;
                }
            }
            i
        },
        move |s, _| horiz(s.pos - end).length() < 3.0 && (s.pos.y - end.y).abs() < 1.5,
    )
}

/// 3. Run at the Well from the west, jump, tether the crown ring's far side, swing across, let go
///    rising and land on the east side (lip or ring deck).
fn well_swing(w: &VoxelWorld, m: &MapInfo) -> RunResult {
    let start = Vec3::new(-17.0, STREET, 2.0);
    let anchor_hint = Vec3::new(CROWN_R, CROWN_Y - 0.9, 0.0);
    let p = MoveParams::default();
    let s = PlayerState::new(start, &p);
    let mut phase = 0u8;
    let mut anchor = anchor_hint;
    let mut idle = false;
    let goal_x = 14.0;
    run(
        w,
        m,
        &[],
        3,
        s,
        20.0,
        move |s, _| {
            let eye = s.eye(&p);
            match phase {
                0 => {
                    if !s.grounded && s.pos.x > -14.0 {
                        phase = 1;
                        anchor = crosshair(w, eye, anchor_hint).unwrap_or(anchor_hint);
                        let (y, pc) = aim_at(eye, anchor);
                        return input(y, pc, 0, 0, TETHER_L);
                    }
                    let mut i = input(yaw_to(s.pos, Vec3::new(40.0, STREET, 0.0)), 0.0, 0, 1, 0);
                    if s.grounded && s.pos.x > -14.5 {
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
                    if s.pos.x > anchor.x + 3.0 && s.vel.y > 0.0 {
                        phase = 2;
                        return input(yaw_to(s.pos, Vec3::new(40.0, STREET, 0.0)), 0.0, 0, 1, 0);
                    }
                    let (y, pc) = aim_at(eye, h.pos + Vec3::new(6.0, 0.0, 0.0));
                    let reel = if s.pos.x < anchor.x { REEL } else { 0 };
                    input(y, pc, 0, 1, TETHER_L | reel | if s.vel.y < 0.0 { BOOST } else { 0 })
                }
                _ => input(yaw_to(s.pos, Vec3::new(40.0, STREET, 0.0)), 0.0, 0, 1, 0),
            }
        },
        move |s, _| s.grounded && s.pos.x > goal_x && s.pos.y > cybertron::VOID_Y + 4.0,
    )
}

thread_local! { static SURF_KEY: i8 = std::env::var("SURF_KEY").ok().and_then(|v| v.parse().ok()).unwrap_or(1); }

/// 4. Run off the ring deck's outer edge onto the south surf line's face, hold into it, ride it out
///    over the street and land south of the Well.
fn surf_south(w: &VoxelWorld, m: &MapInfo, cols: &[Collider]) -> RunResult {
    let l = m.arena.surf_lines[0];
    let d = horiz(l.b - l.a).normalize();
    let p = MoveParams::default();
    let start = m.route("surf_south").expect("surf_south").waypoints[0];
    let mut s = PlayerState::new(start, &p);
    s.vel = d * 8.0;
    let yaw = (-d.x).atan2(-d.z);
    let key = SURF_KEY.with(|k| *k);
    run(
        w,
        m,
        cols,
        4,
        s,
        12.0,
        move |s, _| {
            if s.grounded && s.pos.y > STREET + 4.0 {
                // Still on the deck: run; on the ridge's lip: step off onto the −X face.
                let on_ridge = s.pos.z < l.a.z;
                return input(yaw, 0.0, if on_ridge { -key } else { 0 }, 1, 0);
            }
            // Hold into the face (the face we ride is on the −X side of the ridge).
            input(yaw, 0.0, key, 0, 0)
        },
        move |s, _| s.grounded && s.pos.z < -40.0 && (s.pos.y - STREET).abs() < 0.6,
    )
}

/// 6. The space bridge through a real `World` (portals are world-level).
fn space_bridge(world: VoxelWorld, info: &MapInfo) -> (bool, f32) {
    let mut w = World::with_arena(SimConfig::default(), world, info.arena.clone(), 1);
    let a = PlayerId(1);
    w.add_player(a, "bot".into(), bd_core::corps::Corps::from_index(1)).unwrap();
    w.start_live();
    w.tick(&[]);
    let gate = info.arena.mechanics.portals[0];
    {
        let p = w.player_mut(a).unwrap();
        p.spawn_shield = 0.0;
        p.mv.pos = Vec3::new(gate.a.x + 10.0, STREET, gate.a.z);
        p.mv.vel = Vec3::ZERO;
    }
    for t in 0..(4.0 / DT) as usize {
        let pos = w.player(a).unwrap().mv.pos;
        if horiz(pos - gate.b).length() < 8.0 && pos.x < gate.b.x {
            return (true, t as f32 * DT);
        }
        let i = input(yaw_to(pos, Vec3::new(gate.a.x - 5.0, STREET, gate.a.z)), 0.0, 0, 1, 0);
        w.tick(&[(a, i)]);
    }
    (false, 4.0)
}

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (world, info) = generate_cybertron(seed);
    let p = MoveParams::default();
    let mut surf_cols = Vec::new();
    push_surf_colliders(&info.arena.surf_lines, &mut surf_cols);
    // All transforming panels extended (for the bonus Well Span route).
    let state = MapMechState { movers: info.arena.mechanics.movers.iter().map(|_| MoverState { phase: MoverPhase::Extended, since: 0 }).collect() };
    let mut mover_cols = surf_cols.clone();
    push_mover_colliders(&info.arena, &state, &mut mover_cols);
    println!("cybertron routes (seed {seed}); run speed {:.1} m/s", p.run_speed);

    let span = {
        let pts = vec![Vec3::new(-34.0, STREET, 0.0), Vec3::new(34.0, STREET, 0.0)];
        let mut hop = Hopper::new();
        let s = PlayerState::new(pts[0], &p);
        run(
            &world,
            &info,
            &mover_cols,
            5,
            s,
            12.0,
            move |s, _| {
                let aim = pursue(&pts, s.pos, 14.0);
                hop.hop_to(s, Vec3::new(aim.x, s.pos.y, 0.0), 6.0)
            },
            |s, _| s.pos.x > 30.0 && (s.pos.y - STREET).abs() < 1.0,
        )
    };
    let results = [
        ("1 central lane (bhop)", hop_route(&world, &info, &surf_cols, "central_lane", 1, 30.0)),
        ("2 north expressway + Rust Gap", hop_route(&world, &info, &surf_cols, "north_expressway", 2, 40.0)),
        ("3 well swing (crown ring)", well_swing(&world, &info)),
        ("4 surf: ring deck → south", surf_south(&world, &info, &surf_cols)),
        ("5 Well Span (extended)", span),
    ];
    let mut all_ok = true;
    println!("{:<32} {:>4} {:>6} {:>6} {:>6} {:>6} {:>5} {:>5} {:>5}", "route", "ok", "secs", "avg", "peak", "rise", "hooks", "touch", "surf");
    for (name, r) in &results {
        let fast = r.avg > p.run_speed * 1.1 || r.peak > p.run_speed * 1.4 || name.starts_with('3') || name.starts_with('4');
        let clean = match name.chars().next() {
            Some('3') => r.touchdowns <= 1 && r.hooks >= 1,
            Some('4') => r.surf_ticks > 60,
            _ => !r.fell,
        };
        let ok = r.ok && fast && clean && !r.fell;
        all_ok &= ok;
        println!(
            "{name:<32} {:>4} {:>6.2} {:>6.1} {:>6.1} {:>6.1} {:>5} {:>5} {:>5}{}",
            if ok { "ok" } else { "FAIL" },
            r.secs,
            r.avg,
            r.peak,
            r.peak_y,
            r.hooks,
            r.touchdowns,
            r.surf_ticks,
            if r.ok { String::new() } else { format!("  (ended at {:.1}, {:.1}, {:.1}{})", r.end.x, r.end.y, r.end.z, if r.fell { ", fell" } else { "" }) }
        );
    }
    let (through, secs) = space_bridge(world, &info);
    println!("{:<32} {:>4} {:>6.2}", "6 space bridge (Iacon → Kaon)", if through { "ok" } else { "FAIL" }, secs);
    all_ok &= through;
    if !all_ok {
        eprintln!("some Cybertron routes failed");
        std::process::exit(1);
    }
}
