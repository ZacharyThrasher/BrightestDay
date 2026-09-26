//! Host tick stress bench: 8 bots on the Archipelago, every player kept at its construct limits
//! (held + worn + 3 structures + 2 objects + 2 creatures incl. a kaiju), frequent explosions that
//! carve terrain. Prints tick time avg / p50 / p99 / max (target p99 < 4 ms in release) and
//! snapshot sizes (full + per-recipient steady-state delta, bincode).
//!
//!     cargo run -p bd_sim --release --example stress -- [--ticks 7200] [--seed 1]

use std::time::Instant;

use bd_core::mechanics::{Element, Form, Mechanic};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::{self, recipe};
use bd_sim::combat::Explosion;
use bd_sim::replicate::{DeltaEncoder, WorldSnapshot};
use bd_sim::world::{Aim, SimConfig, World};

fn arg(name: &str, default: u64) -> u64 {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn bytes<T: serde::Serialize>(v: &T) -> usize {
    bincode::serde::encode_to_vec(v, bincode::config::standard()).map(|b| b.len()).unwrap_or(0)
}

fn main() {
    let ticks = arg("--ticks", 7200);
    let seed = arg("--seed", 1);
    // Constructs kept per player (9 = every slot at its limit).
    let per_player = arg("--per-player", 9) as usize;
    let t_gen = Instant::now();
    let mut cfg = SimConfig { seed, check_invariants: false, ..SimConfig::default() };
    cfg.rules.ko_target = 1000;
    cfg.rules.time_limit = 1e6;
    let mut w = World::new(cfg);
    println!("arena generated in {:.0} ms", t_gen.elapsed().as_secs_f64() * 1e3);
    let mut brains = bots::add_bots(&mut w, 0, 8, 0.8, seed);
    w.start_live();
    let kit = [
        recipe("hammer", Form::Hammer, &[Mechanic::Blunt], Element::Light, 1.2),
        recipe("wings", Form::Wings, &[Mechanic::Glide], Element::Light, 2.0),
        recipe("wall", Form::Wall, &[], Element::Light, 6.0),
        recipe("pillar", Form::Pillar, &[], Element::Light, 8.0),
        recipe("trampoline", Form::Trampoline, &[Mechanic::Bounce], Element::Light, 3.0),
        recipe("boulder", Form::Boulder, &[Mechanic::Heavy, Mechanic::Returning], Element::Earth, 1.6),
        recipe("ball", Form::Ball, &[Mechanic::Bounce, Mechanic::Returning], Element::Light, 0.8),
        recipe("wolf", Form::Quadruped, &[Mechanic::Charge], Element::Light, 1.2),
        recipe("godzilla", Form::Kaiju, &[Mechanic::Stomp], Element::Atomic, 14.0),
    ];
    let mut inputs = Vec::with_capacity(8);
    let mut times = Vec::with_capacity(ticks as usize);
    let mut rng = bd_sim::rng::SimRng::new(seed);
    let mut encoders: Vec<DeltaEncoder> = (0..8).map(|_| DeltaEncoder::new()).collect();
    let mut snap = WorldSnapshot::default();
    let (mut delta_total, mut delta_n, mut delta_max) = (0usize, 0usize, 0usize);
    let mut full_bytes = 0usize;
    let mut constructs_seen = 0usize;
    let mut constructs_sum = 0usize;
    let mut carves = 0usize;
    for t in 0..ticks {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        // Keep everyone at the construct limits (infinite-ish willpower for the bench).
        if t % 30 == 0 {
            for b in &brains {
                if let Some(p) = w.player_mut(b.id) {
                    p.willpower = 100.0;
                }
                let Some(p) = w.player(b.id).filter(|p| p.alive) else { continue };
                let eye = p.eye(w.params());
                let have = w.constructs.owned_by(b.id).count();
                if have < per_player {
                    let r = kit[(t as usize / 30 + b.id.0 as usize) % kit.len()].clone();
                    let dir = Vec3::new(rng.range(-1.0, 1.0), -0.3, rng.range(-1.0, 1.0)).normalize_or_zero();
                    let _ = w.cast(b.id, r, None, Aim::new(eye, dir));
                }
            }
        }
        // Frequent explosions that carve terrain.
        if t % 15 == 0 {
            if let Some(p) = w.players.get(rng.below(8) as usize).filter(|p| p.alive) {
                let at = p.mv.pos + Vec3::new(rng.range(-6.0, 6.0), -0.5, rng.range(-6.0, 6.0));
                w.explode(Explosion {
                    pos: at,
                    radius: 3.0,
                    damage: 5.0,
                    knockback: 8.0,
                    source: None,
                    construct: None,
                    element: Element::Fire,
                    carve_radius: rng.range(1.0, 2.5),
                    self_damage: false,
                });
            }
        }
        let t0 = Instant::now();
        let ev = w.tick(&inputs);
        let dt = t0.elapsed();
        carves += ev.iter().filter(|e| matches!(e, bd_sim::GameEvent::Carve { .. })).count();
        times.push(dt.as_secs_f64() * 1e3);
        constructs_seen = constructs_seen.max(w.constructs.len());
        if t > 600 {
            constructs_sum += w.constructs.len();
        }
        // Snapshots at 60 Hz for every recipient (delta vs last "acked" = previous send).
        if t % 2 == 0 {
            for (k, enc) in encoders.iter_mut().enumerate() {
                w.write_snapshot(Some(PlayerId(k as u8)), &mut snap);
                if t > 600 && k == 0 && full_bytes == 0 {
                    full_bytes = bytes(&snap);
                }
                let d = enc.encode(&snap);
                enc.ack(snap.tick);
                if t > 600 {
                    let n = bytes(&d);
                    delta_total += n;
                    delta_n += 1;
                    delta_max = delta_max.max(n);
                }
            }
        }
    }
    times.sort_by(|a, b| a.total_cmp(b));
    let avg = times.iter().sum::<f64>() / times.len() as f64;
    let pct = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    println!(
        "ticks {ticks}  constructs avg {:.0} max {constructs_seen}  carves {carves}  projectiles now {}",
        constructs_sum as f64 / (ticks.saturating_sub(601)).max(1) as f64,
        w.projectiles.len()
    );
    println!("tick ms: avg {avg:.3}  p50 {:.3}  p99 {:.3}  max {:.3}", pct(0.5), pct(0.99), times[times.len() - 1]);
    println!(
        "snapshot bytes: full {full_bytes}  delta avg {:.0}  max {delta_max}  (per recipient, 60 Hz → {:.1} KB/s)",
        delta_total as f64 / delta_n.max(1) as f64,
        delta_total as f64 / delta_n.max(1) as f64 * 60.0 / 1024.0
    );
    let rows = w.scoreboard();
    println!("scoreboard: {}", rows.iter().map(|r| format!("P{}:{}/{}", r.player.0, r.kos, r.deaths)).collect::<Vec<_>>().join(" "));
    if let Err(e) = w.check_invariants() {
        println!("INVARIANT FAILED: {e}");
        std::process::exit(1);
    }
}
