//! Proves every movement-tutorial station (and the peak run) is completable with the real movement
//! code, by driving `step_player` with scripted "skilled bot" inputs through `bd_voxel::generate_tutorial`.
//! Writes one ghost per station to `assets/tutorial/ghosts/<station>.ron` (per-tick inputs for exact
//! replay + 30 Hz feet positions for cheap playback) and prints bot times, peak speeds and medal times.
//!
//!     cargo run -p bd_sim --example tutorial_ghosts --release [-- params.ron]
//!
//! An optional RON file (partial `MoveParams`) overrides the defaults for a dry run (tuning experiments);
//! ghosts are only written with the default tuning, since that is what the game replays them with.
//!
//! Exits non-zero if any station's goal (or gate) isn't reached, so it doubles as a regression test for
//! both the course layout and movement tuning.

use bd_core::input::PlayerInput;
use bd_core::voxel::Aabb;
use bd_core::{DT, TICK_HZ, Vec3};
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;
use bd_voxel::{Station, TutorialInfo, VoxelWorld, generate_tutorial, medals_from_bot};
use serde::Serialize;

// ------------------------------------------------------------------------------------ ghost file

#[derive(Serialize)]
struct Ghost {
    station: String,
    hint: String,
    tick_hz: u32,
    start: (f32, f32, f32),
    start_yaw: f32,
    /// Seconds from spawn to goal (peak run: start ring → finish ring).
    bot_time: f32,
    peak_speed: f32,
    /// Peak run only: (gold, silver, bronze) seconds.
    medals: Option<(f32, f32, f32)>,
    /// One entry per 120 Hz tick from spawn: (yaw, pitch, move_x, move_z, buttons, reel_notches).
    /// Feeding these to `step_player` from `start` reproduces the run exactly.
    inputs: Vec<(f32, f32, i8, i8, u16, i8)>,
    /// Feet positions at 30 Hz (every 4th tick, starting with the spawn), rounded to 1 cm.
    positions: Vec<(f32, f32, f32)>,
}

// ------------------------------------------------------------------------------------ helpers

/// Movement tuning for this run: the defaults, or a partial RON override from the first argument.
fn params() -> &'static MoveParams {
    static P: std::sync::OnceLock<MoveParams> = std::sync::OnceLock::new();
    P.get_or_init(|| match std::env::args().nth(1) {
        Some(path) => ron::from_str(&std::fs::read_to_string(path).expect("read params")).expect("parse params"),
        None => MoveParams::default(),
    })
}

fn default_params() -> bool {
    *params() == MoveParams::default()
}

fn inside(b: &Aabb, p: Vec3) -> bool {
    p.cmpge(b.min).all() && p.cmple(b.max).all()
}

fn horiz(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
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

/// Zig-zag / pursuit side selection with a configurable heading tolerance.
fn steer_side(vel: Vec3, route: Vec3, side: f32, lim_deg: f32) -> f32 {
    let h = horiz(vel);
    if h.length_squared() < 1e-4 {
        return side;
    }
    let e = route.cross(h).y.atan2(route.dot(h));
    let lim = lim_deg.to_radians();
    if e > lim {
        1.0
    } else if e < -lim {
        -1.0
    } else {
        side
    }
}

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = horiz(to - from).normalize_or(Vec3::NEG_Z);
    (-d.x).atan2(-d.z)
}

// ------------------------------------------------------------------------------------ the bot

#[derive(Clone, Copy, Debug, PartialEq)]
enum Leg {
    Station(usize),
    Return,
}

struct Bot<'a> {
    info: &'a TutorialInfo,
    p: MoveParams,
    side: f32,
    phase: u8,
    /// Gas-boost during swings (the fling station must fail without it).
    boost: bool,
    /// Return-lane waypoint index.
    wp: usize,
}

impl<'a> Bot<'a> {
    fn new(info: &'a TutorialInfo) -> Self {
        Self { info, p: params().clone(), side: 1.0, phase: 0, boost: true, wp: 0 }
    }

    fn st(&self, i: usize) -> &'a Station {
        &self.info.stations[i]
    }

    /// Bunny-hop toward `target`: run until at run speed, then perfect air-strafe (held jump).
    fn hop_to(&mut self, s: &PlayerState, target: Vec3, jump: bool, lim_deg: f32) -> PlayerInput {
        let dir = horiz(target - s.pos).normalize_or(Vec3::X);
        let j = if jump { JUMP } else { 0 };
        if s.horizontal_speed() < self.p.run_speed * 0.95 {
            // Run up to speed first: hopping from a standstill goes nowhere.
            return input(yaw_to(s.pos, target), 0.0, 0, 1, if s.grounded { 0 } else { j });
        }
        self.side = steer_side(s.vel, dir, self.side, lim_deg);
        let mx: i8 = if self.side > 0.0 { 1 } else { -1 };
        // Far off course: hard turn (wish perpendicular to velocity turns fastest, keeps speed).
        let h = horiz(s.vel);
        let err = dir.cross(h).y.atan2(dir.dot(h));
        if err.abs() > 25f32.to_radians() && !s.grounded {
            let perp = Vec3::new(-h.z, 0.0, h.x).normalize() * if err > 0.0 { 1.0 } else { -1.0 };
            return input(yaw_for_wish(perp, mx, 0), 0.0, mx, 0, j);
        }
        input(optimal_strafe_yaw(s.vel, &self.p, DT, mx, 0, self.side), 0.0, mx, 0, j)
    }

    /// Swinging on `hand`: look along the swing, gas-boost on the downswing.
    fn swing(&self, s: &PlayerState, hands: u16) -> PlayerInput {
        let (yaw, pitch) = aim_at(Vec3::ZERO, s.vel);
        let boosting = self.boost && s.vel.y < 0.0;
        input(yaw, pitch, 0, boosting as i8, hands | if boosting { BOOST } else { 0 })
    }

    /// Drift toward the goal centre in the air (forward only: W + mouse, which the W-strafe assist turns).
    fn drift(&self, s: &PlayerState, goal: &Aabb) -> PlayerInput {
        self.drift_to(s, goal.center())
    }

    fn drift_to(&self, s: &PlayerState, target: Vec3) -> PlayerInput {
        input(yaw_to(s.pos, target), 0.0, 0, 1, 0)
    }

    fn goal_reached(&self, leg: Leg, s: &PlayerState) -> bool {
        match leg {
            Leg::Station(i) => {
                let g = &self.st(i).goal;
                // Stations 1 (runway) and 3 (slide) count as soon as the feet pass through; the rest
                // need a landing.
                inside(g, s.pos) && (s.grounded || i == 0 || i == 2)
            }
            Leg::Return => {
                let (c, r) = self.info.start_finish;
                s.pos.x >= c.x && (s.chest(&self.p) - c).length() < r + 1.0
            }
        }
    }

    fn control(&mut self, leg: Leg, s: &PlayerState) -> PlayerInput {
        let Leg::Station(i) = leg else {
            // Return lane: down the lane, then hook right through the ring.
            let (c, _) = self.info.start_finish;
            let wps = [Vec3::new(-54.0, 40.0, 52.0), Vec3::new(-52.0, 40.0, -58.0), c + Vec3::new(4.0, 0.0, 0.0)];
            while self.wp < 2 && (horiz(wps[self.wp] - s.pos).length() < 6.0 || s.pos.z < wps[self.wp].z + 2.0) {
                self.wp += 1;
            }
            let lim = if self.wp == 2 { 6.0 } else { 20.0 };
            return self.hop_to(s, wps[self.wp], true, lim);
        };
        let st = self.st(i);
        let g = self.p.gravity;
        match i {
            // 1: bunny hop down the runway.
            0 => self.hop_to(s, *st.route.last().unwrap(), true, 20.0),
            // 2: hop off the runway end and air-strafe the right-hand corner onto the lane.
            1 => {
                if self.phase == 0 && s.pos.x > 8.0 {
                    self.phase = 1;
                }
                if self.phase == 1 && s.pos.x > st.route[1].x - 10.0 {
                    self.phase = 2; // over the lane: line up with it
                }
                let t = match self.phase {
                    0 => st.route[0],
                    1 => st.route[1],
                    _ => st.route[1] + Vec3::Z * 30.0,
                };
                let lim = if self.phase == 0 { 20.0 } else { 5.0 };
                self.hop_to(s, t, true, lim)
            }
            // 3: slide the stairs (build speed first when starting from rest).
            2 => {
                let end = *st.route.last().unwrap();
                let dir = horiz(end - s.pos).normalize_or(Vec3::Z);
                let h = horiz(s.vel);
                let off_course = h.length() > 3.0 && dir.cross(h).y.atan2(dir.dot(h)).abs() > 10f32.to_radians();
                if s.grounded && off_course {
                    // Slides barely steer: hop and air-turn onto the line first.
                    self.hop_to(s, end, true, 5.0)
                } else if s.grounded {
                    let crouch = if s.horizontal_speed() >= 9.0 || s.sliding { CROUCH } else { 0 };
                    input(yaw_to(s.pos, end), 0.0, 0, 1, crouch)
                } else {
                    self.hop_to(s, end, false, 10.0)
                }
            }
            // 4: jump the gap, hook the arch, swing, release when the ballistic landing is on the far side.
            3 => {
                let anchor = st.anchors[0];
                match self.phase {
                    0 => {
                        let edge = st.route[0];
                        if s.pos.z > edge.z - 1.5 && !s.grounded || s.pos.z > edge.z {
                            self.phase = 1;
                            let (y, pch) = aim_at(s.eye(&self.p), anchor);
                            return input(y, pch, 0, 0, TETHER_L);
                        }
                        let near_edge = s.pos.z > edge.z - 4.0;
                        let mut i = self.hop_to(s, edge + Vec3::Z * 5.0, near_edge || s.horizontal_speed() > 12.0, 10.0);
                        if near_edge && s.grounded {
                            i.buttons |= JUMP;
                        }
                        i
                    }
                    1 => {
                        if !s.hooks[0].attached() {
                            let (y, pch) = aim_at(s.eye(&self.p), anchor);
                            return input(y, pch, 0, 0, TETHER_L);
                        }
                        let land = predict_landing(s.pos, s.vel, st.goal.min.y, g);
                        let good = land.is_some_and(|l| l.z > st.goal.min.z + 3.0 && l.z < st.goal.max.z - 6.0);
                        if s.pos.z > anchor.z && s.vel.y > 0.0 && good {
                            self.phase = 2;
                            return self.drift(s, &st.goal);
                        }
                        let mut i = self.swing(s, TETHER_L);
                        i.buttons &= !BOOST; // not needed here
                        i.move_z = 0;
                        i
                    }
                    _ => self.drift(s, &st.goal),
                }
            }
            // 5: hook both sides of the lintel, reel up, release when the arc lands on the ledge.
            4 => {
                let (l, r) = (st.anchors[0], st.anchors[1]);
                let eye = s.eye(&self.p);
                match self.phase {
                    0 => {
                        // Arriving hot from the swing: let friction stop us first (the climb is vertical).
                        if !s.grounded || s.horizontal_speed() > 1.0 {
                            return input(yaw_to(s.pos, st.route[1]), 0.0, 0, 0, 0);
                        }
                        self.phase = 1;
                        let (y, pch) = aim_at(eye, l);
                        input(y, pch, 0, 0, TETHER_L)
                    }
                    1 => {
                        if s.hooks[0].attached() || s.hooks[0].phase == HookPhase::Flying {
                            self.phase = 2;
                        }
                        let (y, pch) = aim_at(eye, r);
                        input(y, pch, 0, 0, TETHER_L | TETHER_R)
                    }
                    2 => {
                        let both = s.hooks[0].attached() && s.hooks[1].attached();
                        let ledge = st.goal.min.y;
                        let land = predict_landing(s.pos, s.vel, ledge, g);
                        let good = land.is_some_and(|p| p.z > st.goal.min.z + 1.5 && p.z < st.goal.max.z - 2.0);
                        if both && s.pos.y > ledge + 0.5 && s.vel.y > 0.0 && good {
                            self.phase = 3;
                            return self.drift(s, &st.goal);
                        }
                        let (y, _) = aim_at(eye, st.route[1]);
                        input(y, 0.3, 0, 0, TETHER_L | TETHER_R | if both { REEL } else { 0 })
                    }
                    _ => input(yaw_to(s.pos, st.route[1] + Vec3::Z * 6.0), 0.0, 0, 1, 0),
                }
            }
            // 6: run off the ledge, hook the rock, boost the downswing, release on target.
            5 => {
                let anchor = st.anchors[0];
                let edge = st.route[1];
                // Land toward the platform's north end: the peak run turns south into the narrow
                // return lane from there at ~40 m/s and needs the room.
                let land_at = Vec3::new(st.goal.center().x, st.goal.min.y, st.goal.max.z - 6.0);
                match self.phase {
                    0 => {
                        if s.pos.x < edge.x + 2.0 && !s.grounded || s.pos.x < edge.x {
                            self.phase = 1;
                            let (y, pch) = aim_at(s.eye(&self.p), anchor);
                            return input(y, pch, 0, 0, TETHER_L);
                        }
                        let near_edge = s.pos.x < edge.x + 4.0;
                        // Coming off the climb: pass between the pillars before turning down the ledge.
                        let target = if s.pos.z < edge.z - 3.0 && s.pos.x > 29.0 {
                            Vec3::new(33.0, edge.y, edge.z)
                        } else {
                            edge - Vec3::X * 5.0
                        };
                        let mut i = self.hop_to(s, target, s.horizontal_speed() > 12.0, 10.0);
                        if near_edge && s.grounded {
                            i.buttons |= JUMP;
                        }
                        i
                    }
                    1 => {
                        if !s.hooks[0].attached() {
                            let (y, pch) = aim_at(s.eye(&self.p), anchor);
                            return input(y, pch, 0, 0, TETHER_L);
                        }
                        let land = predict_landing(s.pos, s.vel, st.goal.min.y, g);
                        let good = land.is_some_and(|l| l.x < st.goal.max.x - 3.0 && l.x > st.goal.min.x + 8.0);
                        if s.pos.x < anchor.x && s.vel.y > 0.0 && good {
                            self.phase = 2;
                            return self.drift_to(s, land_at);
                        }
                        // Out of swing: let go at the top of the arc regardless (it will fall short).
                        if s.pos.x < anchor.x && s.vel.y > 0.0 && s.vel.y < 1.0 {
                            self.phase = 2;
                            return self.drift_to(s, land_at);
                        }
                        self.swing(s, TETHER_L)
                    }
                    _ => self.drift_to(s, land_at),
                }
            }
            _ => unreachable!(),
        }
    }
}

// ------------------------------------------------------------------------------------ runs

struct RunResult {
    ok: bool,
    ticks: usize,
    /// Tick the timer started (peak run: first start-ring crossing).
    t0: usize,
    peak_speed: f32,
    gates_hit: usize,
    gates_total: usize,
    inputs: Vec<PlayerInput>,
    positions: Vec<Vec3>,
    end: Vec3,
    /// Smallest x reached while at or above the goal height (fling reach diagnostics).
    best_x_at_goal_height: f32,
}

fn run(world: &VoxelWorld, info: &TutorialInfo, legs: &[Leg], start: Vec3, yaw: f32, gates: &[(Vec3, f32)], boost: bool, max_secs: f32) -> RunResult {
    let cw = CollisionWorld::new(world, &[]);
    let mut bot = Bot::new(info);
    bot.boost = boost;
    let p = bot.p.clone();
    let mut s = PlayerState::new(start, &p);
    let mut leg = 0;
    let mut gi = 0;
    let peak = legs.len() > 1;
    let mut t0 = if peak { usize::MAX } else { 0 };
    let (ring_c, ring_r) = info.start_finish;
    let mut r = RunResult {
        ok: false,
        ticks: 0,
        t0: 0,
        peak_speed: 0.0,
        gates_hit: 0,
        gates_total: gates.len(),
        inputs: Vec::new(),
        positions: vec![start],
        end: start,
        best_x_at_goal_height: f32::MAX,
    };
    let goal_y = match legs[0] {
        Leg::Station(k) => info.stations[k].goal.min.y,
        Leg::Return => f32::MIN,
    };
    let max_ticks = (max_secs / DT) as usize;
    let trace = std::env::var("TRACE").is_ok_and(|v| legs.iter().any(|l| format!("{l:?}") == v));
    for t in 0..max_ticks {
        let mut i = if t < 2 { input(yaw, 0.0, 0, 0, 0) } else { bot.control(legs[leg], &s) };
        i.seq = t as u32;
        let prev = s.pos;
        step_player(&mut s, &i, &cw, &p, DT);
        if trace && t % 12 == 0 {
            println!(
                "  t={:5.2} leg={:?} ph={} pos=({:6.1},{:5.1},{:6.1}) vel=({:5.1},{:5.1},{:5.1}) gr={} sl={} hooks={:.1?}/{:.1?} in=({:.2},{:.2},{},{},{:#x})",
                t as f32 * DT, legs[leg], bot.phase, s.pos.x, s.pos.y, s.pos.z, s.vel.x, s.vel.y, s.vel.z, s.grounded as u8, s.sliding as u8,
                s.hooks[0].pos, s.hooks[1].pos, i.yaw, i.pitch, i.move_x, i.move_z, i.buttons
            );
        }
        r.inputs.push(i);
        if (t + 1) % 4 == 0 {
            r.positions.push(s.pos);
        }
        // Timer start for the peak run: crossing the ring plane heading +X near its centre.
        if peak && t0 == usize::MAX && prev.x < ring_c.x && s.pos.x >= ring_c.x && (s.chest(&p) - ring_c).length() < ring_r + 1.0 {
            t0 = t;
        }
        if s.pos.y >= goal_y {
            r.best_x_at_goal_height = r.best_x_at_goal_height.min(s.pos.x);
        }
        if t0 != usize::MAX {
            r.peak_speed = r.peak_speed.max(s.speed());
        }
        if gi < gates.len() && (s.chest(&p) - gates[gi].0).length() <= gates[gi].1 && (!peak || t0 != usize::MAX) {
            gi += 1;
        }
        if s.pos.y < info.void_y {
            break;
        }
        // Peak: the return leg only completes after the station legs (timer must be running).
        if bot.goal_reached(legs[leg], &s) && !(peak && legs[leg] == Leg::Return && gi + 1 < gates.len()) {
            leg += 1;
            bot.phase = 0;
            if leg == legs.len() {
                r.ok = true;
                r.ticks = t + 1;
                break;
            }
        }
    }
    r.t0 = if t0 == usize::MAX { 0 } else { t0 };
    r.gates_hit = gi;
    r.end = s.pos;
    r
}

fn write_ghost(name: &str, hint: &str, start: Vec3, yaw: f32, r: &RunResult, bot_time: f32, medals: Option<(f32, f32, f32)>) {
    let round = |v: f32| (v * 100.0).round() / 100.0;
    let g = Ghost {
        station: name.to_string(),
        hint: hint.to_string(),
        tick_hz: TICK_HZ,
        start: (start.x, start.y, start.z),
        start_yaw: yaw,
        bot_time,
        peak_speed: r.peak_speed,
        medals,
        inputs: r.inputs.iter().map(|i| (i.yaw, i.pitch, i.move_x, i.move_z, i.buttons, i.reel_notches)).collect(),
        positions: r.positions.iter().map(|p| (round(p.x), round(p.y), round(p.z))).collect(),
    };
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/tutorial/ghosts");
    std::fs::create_dir_all(&dir).expect("create ghost dir");
    let cfg = ron::ser::PrettyConfig::default().compact_arrays(true);
    let text = ron::ser::to_string_pretty(&g, cfg).expect("serialize ghost");
    std::fs::write(dir.join(format!("{name}.ron")), text).expect("write ghost");
}

/// Replays recorded inputs and checks the end state matches bit-for-bit (determinism).
fn replay_matches(world: &VoxelWorld, start: Vec3, r: &RunResult) -> bool {
    let cw = CollisionWorld::new(world, &[]);
    let p = params();
    let mut s = PlayerState::new(start, p);
    for i in &r.inputs {
        step_player(&mut s, i, &cw, p, DT);
    }
    s.pos == r.end
}

fn main() {
    let (world, info) = generate_tutorial();
    let mut all_ok = true;
    println!("{:<14} {:>7} {:>9} {:>6}  {}", "station", "time s", "peak m/s", "gates", "result");
    for (k, st) in info.stations.iter().enumerate() {
        let r = run(&world, &info, &[Leg::Station(k)], st.start, st.facing_yaw, &st.gates, true, 20.0);
        let time = r.ticks as f32 * DT;
        let ok = r.ok && r.gates_hit == r.gates_total && replay_matches(&world, st.start, &r);
        all_ok &= ok;
        println!(
            "{:<14} {:>7.2} {:>9.1} {:>3}/{:<2}  {}{}",
            st.name,
            time,
            r.peak_speed,
            r.gates_hit,
            r.gates_total,
            if ok { "ok" } else { "FAILED" },
            if ok { String::new() } else { format!(" (ended at {:.1?})", r.end) }
        );
        if ok && default_params() {
            write_ghost(st.name, st.hint, st.start, st.facing_yaw, &r, time, None);
        }
        if st.name == "fling" {
            let nb = run(&world, &info, &[Leg::Station(k)], st.start, st.facing_yaw, &st.gates, false, 20.0);
            let proven = !nb.ok;
            all_ok &= proven;
            println!(
                "{:<14} {:>7} {:>9.1} {:>6}  {}",
                "  (no boost)",
                "-",
                nb.peak_speed,
                "",
                if proven { "falls short, as intended" } else { "FAILED: reachable without boost" }
            );
            println!(
                "{:<14} boost run ended at x {:.1}; best no-boost x at landing height {:.1} (platform edge {:.1})",
                "",
                r.end.x,
                nb.best_x_at_goal_height,
                st.goal.max.x
            );
        }
    }

    let mut legs: Vec<Leg> = (0..info.stations.len()).map(Leg::Station).collect();
    legs.push(Leg::Return);
    let r = run(&world, &info, &legs, info.peak_start, info.peak_yaw, &info.peak_gates, true, 60.0);
    let time = r.ticks.saturating_sub(r.t0) as f32 * DT;
    let ok = r.ok && r.gates_hit == r.gates_total && replay_matches(&world, info.peak_start, &r);
    all_ok &= ok;
    let m = medals_from_bot(time);
    println!(
        "{:<14} {:>7.2} {:>9.1} {:>3}/{:<2}  {}{}",
        "peak_run",
        time,
        r.peak_speed,
        r.gates_hit,
        r.gates_total,
        if ok { "ok" } else { "FAILED" },
        if ok { String::new() } else { format!(" (ended at {:.1?})", r.end) }
    );
    if ok {
        println!("medals: gold {:.2} s, silver {:.2} s, bronze {:.2} s", m.gold, m.silver, m.bronze);
    }
    if ok && default_params() {
        write_ghost("peak_run", "Chain it all. Beat gold.", info.peak_start, info.peak_yaw, &r, time, Some((m.gold, m.silver, m.bronze)));
    }
    if !all_ok {
        eprintln!("some tutorial routes failed");
        std::process::exit(1);
    }
}
