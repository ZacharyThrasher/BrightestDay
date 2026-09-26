//! Proves Hyperline's key movement lines and the slipstream knock-off rules with the real sim
//! (`bd_sim::World` on `bd_voxel` `generate_hyperline`: movement, tethers, slipstream, blast zones).
//!
//!     cargo run -p bd_sim --features hyperline --example hyperline_routes --release [-- seed]
//!
//! Scenarios:
//! 1. **Roof run** — tail ramp to the loco, bunny-hopping down the roof centre line over all seven
//!    gaps (never below the roofline, faster than running).
//! 2. **Gap jump** — standing 8 m back from a gap: run, jump, land on the next car.
//! 3. **Swing in** — blown off beside a door: hook the far wall through the doorway, reel, land
//!    inside the car.
//! 4. **Tether back** — launched off the side (a KO without help): hook the roof, reel, back aboard.
//! 5. **Light hit** — knocked over the edge: steer back, air jump + dodge onto the roof, into a door
//!    or onto the running board under the doors (no tether).
//! 6. **Strong hit** — launched hard off the side: the slipstream sweeps you out → KO.
//! 7. **Astern** — knocked off the tail ramp: swept back → KO.
//!
//! Exits non-zero if a scenario fails. `TRACE=<n>` prints the player every 0.1 s.

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::{PlayerId, Vec3};
use bd_sim::events::GameEvent;
use bd_sim::world::{SimConfig, World};
use bd_voxel::maps::hyperline::{self, FLAT_HW, FLOOR_Y, HALF_W, ROOF_Y, Unit, generate_hyperline};

const A: PlayerId = PlayerId(1);
const B: PlayerId = PlayerId(2);
/// Light hit: sideways launch speed (m/s).
const LIGHT: f32 = 14.0;

fn yaw_pitch(d: Vec3) -> (f32, f32) {
    let d = d.normalize_or(Vec3::NEG_Z);
    let yaw = (-d.x).atan2(-d.z);
    (yaw, d.y.clamp(-1.0, 1.0).asin())
}

fn world(seed: u64) -> World {
    let (voxels, info) = generate_hyperline(seed);
    let cfg = SimConfig { check_invariants: true, ..SimConfig::default() };
    let spawns = info.arena.spawns.clone();
    let mut w = World::with_arena(cfg, voxels, info.arena, seed);
    for k in 1..=2 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for p in &mut w.players {
        p.spawn_shield = 0.0;
    }
    // B parks on the loco roof, out of the way.
    let p = w.player_mut(B).unwrap();
    p.mv.pos = spawns[0];
    p.mv.vel = Vec3::ZERO;
    w
}

fn place(w: &mut World, pos: Vec3, vel: Vec3, grounded: bool) {
    let p = w.player_mut(A).unwrap();
    p.mv.pos = pos;
    p.mv.vel = vel;
    p.mv.grounded = grounded;
}

struct Outcome {
    ko: bool,
    ticks: u32,
    /// Furthest outside the train's side (|z| − half width, m).
    max_out: f32,
    min_y: f32,
    end: bd_sim::movement::PlayerState,
}

/// Run up to `max` ticks with `policy(t, state)`; stops at A's KO or when `done` says so.
fn drive(w: &mut World, trace: bool, max: u32, mut policy: impl FnMut(u32, &bd_sim::movement::PlayerState) -> PlayerInput, done: impl Fn(&bd_sim::movement::PlayerState) -> bool) -> Outcome {
    let mut min_y = f32::INFINITY;
    let mut max_out = f32::NEG_INFINITY;
    for t in 0..max {
        let st = w.player(A).unwrap().mv;
        let inp = policy(t, &st);
        let evs = w.tick(&[(A, inp)]).to_vec();
        let st = w.player(A).unwrap().mv;
        min_y = min_y.min(st.pos.y);
        max_out = max_out.max(st.pos.z.abs() - HALF_W);
        if trace && t % 12 == 0 {
            println!("    t {:5.2} pos {:7.2} {:6.2} {:6.2} vel {:6.1} {:6.1} {:6.1} g {} teth {}", t as f32 / 120.0, st.pos.x, st.pos.y, st.pos.z, st.vel.x, st.vel.y, st.vel.z, st.grounded, st.tethered());
        }
        if evs.iter().any(|e| matches!(e, GameEvent::Ko { victim, .. } if *victim == A)) {
            return Outcome { ko: true, ticks: t, max_out, min_y, end: st };
        }
        if done(&st) {
            return Outcome { ko: false, ticks: t, max_out, min_y, end: st };
        }
    }
    Outcome { ko: false, ticks: max, max_out, min_y, end: w.player(A).unwrap().mv }
}

fn on_roof(st: &bd_sim::movement::PlayerState) -> bool {
    st.grounded && st.pos.y >= ROOF_Y - 0.1 && st.pos.z.abs() < HALF_W
}

/// Standing on a running board (a ledge under the doors: out of the wind, walk in).
fn on_board(st: &bd_sim::movement::PlayerState) -> bool {
    st.grounded && (st.pos.y - hyperline::FOOTBOARD_Y).abs() < 0.2
}

fn inside(st: &bd_sim::movement::PlayerState) -> bool {
    st.grounded && (st.pos.y - FLOOR_Y).abs() < 0.2 && st.pos.z.abs() < HALF_W - 0.4
}

/// Hook toward `target` (left tether) and reel in, from tick `at`.
fn hook_reel(t: u32, at: u32, st: &bd_sim::movement::PlayerState, target: Vec3) -> PlayerInput {
    let (yaw, pitch) = yaw_pitch(target - (st.pos + Vec3::Y * 1.6));
    if t < at {
        return PlayerInput { yaw, pitch, ..Default::default() };
    }
    PlayerInput { yaw, pitch, buttons: buttons::TETHER_L | buttons::REEL, move_z: 1, ..Default::default() }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed: u64 = args.first().and_then(|a| a.parse().ok()).unwrap_or(1);
    let trace: Option<u32> = std::env::var("TRACE").ok().and_then(|v| v.parse().ok());
    let tr = |n: u32| trace == Some(n);
    let us: Vec<Unit> = hyperline::units(seed);
    println!("hyperline routes (seed {seed})");
    let mut fails = 0;
    let mut report = |n: u32, name: &str, ok: bool, detail: String| {
        println!("  {n}. {name:<12} {}  {detail}", if ok { "OK  " } else { "FAIL" });
        if !ok {
            fails += 1;
        }
    };

    // 1. Roof run: hold forward + jump (auto-hop), a light zig-zag air strafe to keep speed.
    {
        let mut w = world(seed);
        let tail = &us[hyperline::UNITS - 1];
        let start = Vec3::new(tail.x0 + 7.0, ROOF_Y, 0.25);
        place(&mut w, start, Vec3::ZERO, true);
        let goal = us[0].level_roof().1 - 2.0;
        let o = drive(
            &mut w,
            tr(1),
            120 * 40,
            |_, st| {
                // Run up to speed, then hold jump (auto-hop) along the centre line.
                let steer = st.pos.z * 0.15;
                let yaw = -std::f32::consts::FRAC_PI_2 + steer.clamp(-0.3, 0.3);
                let hop = !st.grounded || st.horizontal_speed() > 9.5;
                PlayerInput { yaw, move_z: 1, buttons: if hop { buttons::JUMP } else { 0 }, ..Default::default() }
            },
            |st| st.pos.x >= goal,
        );
        let dist = o.end.pos.x - start.x;
        let secs = o.ticks as f32 / 120.0;
        let ok = !o.ko && o.end.pos.x >= goal && o.min_y > ROOF_Y - 2.0;
        report(1, "roof run", ok, format!("{dist:.0} m in {secs:.1} s ({:.1} m/s avg), lowest {:.1} m", dist / secs.max(0.01), o.min_y));
    }

    // 2. Gap jump from a standing start 8 m back: run, jump at the edge.
    {
        let mut w = world(seed);
        let (a, b) = (&us[3], &us[2]);
        place(&mut w, Vec3::new(a.x1 - 8.0, ROOF_Y, 0.25), Vec3::ZERO, true);
        let edge = a.x1 - 1.0;
        let o = drive(
            &mut w,
            tr(2),
            600,
            |_, st| PlayerInput { yaw: -std::f32::consts::FRAC_PI_2, move_z: 1, buttons: if st.pos.x > edge - 0.6 { buttons::JUMP } else { 0 }, ..Default::default() },
            |st| st.grounded && st.pos.x > b.x0 + 1.0,
        );
        report(2, "gap jump", !o.ko && on_roof(&o.end) && o.end.pos.x > b.x0, format!("landed at x {:.1} (car starts {:.1}) y {:.1}", o.end.pos.x, b.x0, o.end.pos.y));
    }

    // 3. Swing in: just off the side beside a door, hook the far inside wall through the doorway.
    {
        let mut w = world(seed);
        let u = &us[4];
        let d = u.x0 + u.doors[0];
        place(&mut w, Vec3::new(d, 9.0, HALF_W + 2.0), Vec3::new(0.0, 2.0, 3.0), false);
        let target = Vec3::new(d, 9.8, -(HALF_W - 0.75));
        // Hook the far wall through the doorway, reel in, let go once through.
        let o = drive(&mut w, tr(3), 480, |t, st| if st.pos.z < 2.5 { PlayerInput::default() } else { hook_reel(t, 6, st, target) }, inside);
        report(3, "swing in", !o.ko && inside(&o.end), format!("ended at {:.1},{:.1},{:.1} after {:.2} s", o.end.pos.x, o.end.pos.y, o.end.pos.z, o.ticks as f32 / 120.0));
    }

    // 4. Tether back: launched off the side hard enough to be a KO alone; hook the roof, reel.
    {
        let mut w = world(seed);
        let u = &us[3];
        place(&mut w, Vec3::new(u.cx(), ROOF_Y, 3.0), Vec3::ZERO, true);
        w.tick(&[]);
        w.launch_player(A, Vec3::new(0.0, 7.0, 16.0), Some(B));
        let anchor = Vec3::new(u.cx() - 2.0, ROOF_Y - 0.25, 0.25);
        let o = drive(&mut w, tr(4), 120 * 8, |t, st| hook_reel(t, 30, st, anchor), on_roof);
        report(4, "tether back", !o.ko && on_roof(&o.end), format!("back aboard at {:.1},{:.1},{:.1} after {:.2} s (lowest {:.1} m)", o.end.pos.x, o.end.pos.y, o.end.pos.z, o.ticks as f32 / 120.0, o.min_y));
        // Control: the same launch without the tether is a KO.
        let mut w = world(seed);
        place(&mut w, Vec3::new(u.cx(), ROOF_Y, 3.0), Vec3::ZERO, true);
        w.tick(&[]);
        w.launch_player(A, Vec3::new(0.0, 7.0, 16.0), Some(B));
        let o = drive(&mut w, false, 120 * 8, |_, _| PlayerInput::default(), on_roof);
        report(4, "  (no tether)", o.ko, format!("KO {} after {:.2} s", o.ko, o.ticks as f32 / 120.0));
    }

    // 5. Light hit carrying you just over the edge: steer back (DI + air control) while outside,
    //    air jump the moment the tumble ends, dodge in on the way up.
    {
        let mut w = world(seed);
        let u = &us[5];
        let d = u.x0 + u.doors[1];
        place(&mut w, Vec3::new(d, ROOF_Y, 2.8), Vec3::ZERO, true);
        w.tick(&[]);
        w.launch_player(A, Vec3::new(0.0, 4.0, LIGHT), Some(B));
        let mut jumped_at: Option<u32> = None;
        let o = drive(
            &mut w,
            tr(5),
            120 * 6,
            |t, st| {
                let out = st.pos.z > FLAT_HW - 0.5;
                let mut b = 0;
                if out && st.tumble <= 0.0 && jumped_at.is_none() {
                    jumped_at = Some(t);
                    b = buttons::JUMP;
                } else if jumped_at.is_some_and(|j| t == j + 8) && out {
                    b = buttons::DODGE;
                }
                PlayerInput { yaw: 0.0, move_z: if out { 1 } else { 0 }, buttons: b, ..Default::default() }
            },
            |st| on_roof(st) || inside(st) || on_board(st),
        );
        let ok = !o.ko && (on_roof(&o.end) || inside(&o.end) || on_board(&o.end)) && o.max_out > 0.5;
        report(5, "light hit", ok, format!("out to {:.1} m past the side, ended at {:.1},{:.1},{:.1} after {:.2} s", o.max_out, o.end.pos.x, o.end.pos.y, o.end.pos.z, o.ticks as f32 / 120.0));
    }

    // 6. Strong hit off the side: KO even when trying to recover by dodging back.
    {
        let mut w = world(seed);
        let u = &us[2];
        place(&mut w, Vec3::new(u.cx(), ROOF_Y, 1.0), Vec3::ZERO, true);
        w.tick(&[]);
        w.launch_player(A, Vec3::new(0.0, 12.0, 34.0), Some(B));
        let o = drive(&mut w, tr(6), 120 * 8, |t, _| PlayerInput { yaw: 0.0, move_z: 1, buttons: if t == 60 { buttons::DODGE } else if t == 80 { buttons::JUMP } else { 0 }, ..Default::default() }, on_roof);
        report(6, "strong hit", o.ko, format!("KO {} after {:.2} s, at {:.1},{:.1},{:.1}", o.ko, o.ticks as f32 / 120.0, o.end.pos.x, o.end.pos.y, o.end.pos.z));
    }

    // 7. Knocked off the tail ramp backwards: swept astern → KO.
    {
        let mut w = world(seed);
        let tail = &us[hyperline::UNITS - 1];
        place(&mut w, Vec3::new(tail.x0 + 8.0, ROOF_Y, 2.0), Vec3::ZERO, true);
        w.tick(&[]);
        w.launch_player(A, Vec3::new(-30.0, 10.0, 0.0), Some(B));
        let o = drive(&mut w, tr(7), 120 * 8, |_, _| PlayerInput { yaw: -std::f32::consts::FRAC_PI_2, move_z: 1, ..Default::default() }, on_roof);
        report(7, "astern", o.ko, format!("KO {} after {:.2} s, at {:.1},{:.1},{:.1}", o.ko, o.ticks as f32 / 120.0, o.end.pos.x, o.end.pos.y, o.end.pos.z));
    }

    if fails > 0 {
        eprintln!("{fails} Hyperline scenario(s) failed");
        std::process::exit(1);
    }
}
