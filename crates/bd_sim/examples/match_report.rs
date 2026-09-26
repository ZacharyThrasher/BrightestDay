//! Run a full 8-bot FFA match and print what happened (hits by kind, casts, KOs by cause, oath
//! recitals, scoreboard). A quick way to eyeball balance and bot behaviour.
//!
//!     cargo run -p bd_sim --release --example match_report -- [--seed 1] [--minutes 3] [--difficulty 80]

use std::collections::BTreeMap;

use bd_sim::bots;
use bd_sim::events::GameEvent;
use bd_sim::world::{SimConfig, World};

fn arg(name: &str, default: u64) -> u64 {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn main() {
    let seed = arg("--seed", 1);
    let minutes = arg("--minutes", 3) as f32;
    let diff = arg("--difficulty", 80) as f32 / 100.0;
    let mut cfg = SimConfig { seed, ..SimConfig::default() };
    cfg.rules.time_limit = minutes * 60.0;
    cfg.check_invariants = true;
    let mut w = World::new(cfg);
    let mut brains = bots::add_bots(&mut w, 0, 8, diff, seed);
    let mut inputs = Vec::new();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut ended_at = None;
    for _ in 0..((minutes * 60.0 + 30.0) * 120.0) as u64 {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        let now = w.tick + 1;
        for e in w.tick(&inputs) {
            let key = match e {
                GameEvent::Hit { kind, .. } => format!("hit {kind:?}"),
                GameEvent::Ko { cause, killer, .. } => format!("ko {cause:?}{}", if killer.is_some() { " (credited)" } else { "" }),
                GameEvent::ConstructSpawned { recipe, .. } => format!("cast {:?}", recipe.form.category()),
                GameEvent::CastFailed { reason, .. } => format!("cast failed {:?}", std::mem::discriminant(reason)),
                GameEvent::ConstructGone { reason, .. } => format!("gone {reason:?}"),
                GameEvent::Move { .. } | GameEvent::ProjectileFired { .. } | GameEvent::ProjectileEnded { .. } => continue,
                other => format!("{other:?}").split([' ', '{', '(']).next().unwrap_or("?").to_string(),
            };
            *counts.entry(key).or_default() += 1;
            if let GameEvent::MatchPhase { phase: bd_sim::MatchPhase::Ended, .. } = e {
                ended_at.get_or_insert(now);
            }
        }
        if ended_at.is_some() {
            break;
        }
    }
    for (k, v) in &counts {
        println!("{v:>7}  {k}");
    }
    println!("ended at tick {ended_at:?}");
    for r in w.scoreboard() {
        let p = w.player(r.player).map(|p| p.name.clone()).unwrap_or_default();
        println!("{p:>8}  KOs {:>3}  deaths {:>3}", r.kos, r.deaths);
    }
}
