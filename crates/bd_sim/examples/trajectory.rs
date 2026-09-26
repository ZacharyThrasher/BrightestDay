//! Scripted "skilled route" through the reference arena: run → bhop (perfect strafe) → tether swing →
//! release → hook the next anchor at the apex → swing + gas boost → release → air-strafe → land & hop.
//! Prints the speed-over-time table and the measured speed tiers.
//!
//!     cargo run -p bd_sim --example trajectory [--release] [-- params.ron | -- --dump-params]
//!
//! An optional RON file (partial `MoveParams`) overrides the defaults — handy for tuning.
//! `--dump-params` prints the default tuning as RON.

use bd_core::{DT, Vec3};
use bd_sim::movement::testkit::scenarios::{self, arena, lattice_anchor};
use bd_sim::movement::testkit::*;
use bd_sim::movement::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Run,
    Hop,
    Swing(u8),
    Air,
}

fn main() {
    let p: MoveParams = match std::env::args().nth(1).as_deref() {
        Some("--dump-params") => {
            // Starting point for a hot-reloadable tuning file.
            println!("{}", ron::ser::to_string_pretty(&MoveParams::default(), Default::default()).unwrap());
            return;
        }
        Some(path) => ron::from_str(&std::fs::read_to_string(path).expect("read params")).expect("parse params"),
        None => MoveParams::default(),
    };

    // --------------------------------------------------------------------------------- tiers
    println!("== speed tiers (m/s) ==");
    let run = scenarios::run(&p);
    let (nv_avg, nv_end) = scenarios::naive_bhop(&p, 5.0);
    let (bh_avg, bh_end) = scenarios::bhop(&p, 5.0, false);
    let (bp_avg, bp_end) = scenarios::bhop(&p, 5.0, true);
    let (sl_peak, sl_05) = scenarios::slide_flat(&p);
    let sl_ramp = scenarios::slide_ramp(&p);
    let sw = scenarios::swing(&p, false);
    let swb = scenarios::swing(&p, true);
    let flight = scenarios::flight(&p);
    println!("{:<44} {:>6.1}", "run (steady)", run);
    for deg in [90.0f32, 150.0, 250.0] {
        let (r, secs) = (deg.to_radians(), 3.0);
        let sync = scenarios::turn_gain(&p, r, secs, 12.0, true).0 / secs;
        let (w_gain, w_lag) = scenarios::turn_gain(&p, r, secs, 12.0, false);
        let w_only = w_gain / secs;
        println!(
            "{:<44} {:>6.2}  (W only {:.2} = {:.0}%, view-velocity lag {:.0}°)",
            format!("air-strafe gain at {deg:.0}°/s, D+sync (m/s²)"),
            sync,
            w_only,
            100.0 * w_only / sync,
            w_lag.to_degrees()
        );
    }
    println!("{:<44} {:>6.1}", "flight construct cruise (reference)", flight);
    println!("{:<44} {:>6.1}  (end {:.1})", "naive bhop 5 s, W + held jump, view sweeps", nv_avg, nv_end);
    println!("{:<44} {:>6.1}  (end {:.1})", "bhop 5 s, held jump, perfect strafe: last-1s avg", bh_avg, bh_end);
    println!("{:<44} {:>6.1}  (end {:.1})", "bhop 5 s, timed hops, perfect strafe", bp_avg, bp_end);
    println!("{:<44} {:>6.1}  (after 0.5 s {:.1})", "slide entry burst from run", sl_peak, sl_05);
    println!("{:<44} {:>6.1}", "slide down 20° ramp (20 m) from 14 m/s", sl_ramp);
    println!(
        "{:<44} {:>6.1}  (release {:.1}, +0.5 s {:.1}, dist {:.0} m)",
        "tether swing from a 14 m/s hop: peak", sw.peak, sw.release, sw.after, sw.distance
    );
    println!(
        "{:<44} {:>6.1}  (release {:.1}, +0.5 s {:.1}, dist {:.0} m)",
        "swing + boost + release: peak", swb.peak, swb.release, swb.after, swb.distance
    );

    // ---------------------------------------------------------------------------- the route
    println!("\n== skilled route ==");
    let w = arena();
    let cw = CollisionWorld::new(&w, &[]);
    let mut s = PlayerState::new(Vec3::new(-75.0, 0.01, 1.0), &p);
    let mut phase = Phase::Run;
    let mut side = 1.0f32;
    let mut rising = false;
    let mut swings = 0u8;
    let mut anchor = Vec3::ZERO;
    let mut max_speed = 0.0f32;
    println!("{:>5} {:>9} {:>6} {:>6} {:>6} {:>5} {:>7}  events", "t", "phase", "speed", "horiz", "y", "gas", "x");
    let ticks = (11.0 / DT) as usize;
    let mut events_line = String::new();
    for t in 0..ticks {
        let time = t as f32 * DT;
        let vel_dir = Vec3::new(s.vel.x, 0.0, s.vel.z).normalize_or(Vec3::X);
        side = scenarios::zigzag_side(s.vel, Vec3::X, side);
        let apex = rising && s.vel.y <= 0.0;
        rising = s.vel.y > 0.0;
        let mx: i8 = if side > 0.0 { 1 } else { -1 };
        let strafe_yaw = optimal_strafe_yaw(s.vel, &p, DT, mx, 0, side);
        let perfect_jump = (s.grounded || (s.vel.y < 0.0 && s.pos.y < 0.5)) && t % 2 == 0;
        // Phase transitions.
        match phase {
            Phase::Run if time >= 0.3 => phase = Phase::Hop,
            Phase::Hop | Phase::Air if apex && time >= 3.0 && swings < 2 && s.pos.y > 1.0 => {
                anchor = lattice_anchor(s.pos + vel_dir * 28.0);
                if (anchor - s.pos).dot(vel_dir) > 8.0 {
                    phase = Phase::Swing(swings);
                    swings += 1;
                }
            }
            Phase::Swing(k) => {
                let past = (s.pos - anchor).dot(vel_dir) > 0.0 && s.vel.y > 0.4 * s.speed();
                if past && s.hooks[k as usize % 2].attached() {
                    phase = Phase::Air;
                }
            }
            Phase::Air if s.grounded => phase = Phase::Hop,
            _ => {}
        }
        let i = match phase {
            Phase::Run => input(-std::f32::consts::FRAC_PI_2, 0.0, 0, 1, 0),
            Phase::Hop => input(strafe_yaw, 0.0, mx, 0, if perfect_jump { JUMP } else { 0 }),
            Phase::Swing(k) => {
                let hook = if k % 2 == 0 { TETHER_L } else { TETHER_R };
                let attached = s.hooks.iter().any(|h| h.attached());
                if !attached {
                    // Aim at the anchor until the hook lands.
                    let (yaw, pitch) = aim_at(s.eye(&p), anchor);
                    input(yaw, pitch, 0, 0, hook)
                } else {
                    // Look along the swing, reel only to clear the ground; the second swing boosts.
                    scenarios::swing_input(&s, &p, hook, k == 1)
                }
            }
            Phase::Air => input(strafe_yaw, 0.0, mx, 0, if perfect_jump { JUMP } else { 0 }),
        };
        let e = step_player(&mut s, &i, &cw, &p, DT);
        max_speed = max_speed.max(s.speed());
        for (flag, name) in [
            (e.jumped, "jump"),
            (e.perfect_hop, "perfect"),
            (e.landed.is_some(), "land"),
            (e.slide_start, "slide"),
            (e.tether_fired[0] || e.tether_fired[1], "fire"),
            (e.tether_attached[0] || e.tether_attached[1], "attach"),
            (e.tether_released[0] || e.tether_released[1], "release"),
            (e.boost_start, "boost"),
            (e.wall_hit.is_some(), "WALL"),
        ] {
            if flag && !events_line.contains(name) {
                events_line.push_str(name);
                events_line.push(' ');
            }
        }
        if e.jumped && e.hop_chain > 0 {
            events_line.push_str(&format!("chain{} ", e.hop_chain));
        }
        if t % 12 == 0 {
            println!(
                "{:>5.2} {:>9} {:>6.1} {:>6.1} {:>6.1} {:>5.0} {:>7.1}  {}",
                time,
                match phase {
                    Phase::Run => "run".to_string(),
                    Phase::Hop => "bhop".to_string(),
                    Phase::Swing(k) => format!("swing{}", k + 1),
                    Phase::Air => "air".to_string(),
                },
                s.speed(),
                s.horizontal_speed(),
                s.pos.y,
                s.gas,
                s.pos.x,
                events_line
            );
            events_line.clear();
        }
    }
    println!("max speed on route: {max_speed:.1} m/s");
}
