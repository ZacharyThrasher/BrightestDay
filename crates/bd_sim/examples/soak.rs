//! Chaos soak: headless worlds full of bots casting random recipes from the full combination space
//! (every form × mechanics × elements × payloads × features × temperaments × scales incl. no-limits
//! × counts), random grafts / terrain animation, portals, links, fields, resizes, piloting, oath,
//! joins and leaves. Asserts no panic, invariants every tick, a tick-time budget, and determinism
//! (world 0 is re-run and its state hashes compared). Failures print the seed and tick.
//!
//!     cargo run --release -p bd_sim --example soak -- --seed 1 --ticks 200000
//!         [--worlds 1] [--stats out.json] [--budget-ms 4] [--rules random|normal] [--arena arch|flat]
//!         [--det-ticks 20000] [--party 4]
//!
//! `--party N`: Party mode (short rounds, picks between them) with N random party effects of every
//! rarity stacked on each bot from the start (COMBAT.md §5g).
//!
//! Exit code 1 on any failure.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

use bd_sim::fuzz::{ChaosConfig, ChaosStats, ChaosWorld, random_rules};
use bd_sim::rng::SimRng;
use bd_sim::rules::MatchRules;
use bd_sim::world::ArenaSource;

fn arg_str(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn arg(name: &str, default: u64) -> u64 {
    arg_str(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn config(seed: u64, random: bool, flat: bool, party: Option<u8>) -> ChaosConfig {
    let rules = if random { random_rules(&mut SimRng::new(seed ^ 0x5EED)) } else { MatchRules::default() };
    let mut cfg = ChaosConfig::new(seed, rules);
    if flat {
        cfg.arena = ArenaSource::Flat { size_m: 128.0 };
    }
    cfg.party = party;
    cfg
}

/// Run one world; returns (stats, final state hash, voxel hash, failed).
fn run_world(cfg: ChaosConfig, ticks: u64, budget_us: u32, verbose: bool) -> (ChaosStats, u64, u64, bool) {
    let seed = cfg.seed;
    let mut cw = ChaosWorld::with_config(cfg);
    let mut failed = false;
    let mut over_budget = 0u64;
    let t0 = Instant::now();
    for t in 0..ticks {
        let r = catch_unwind(AssertUnwindSafe(|| cw.step()));
        match r {
            Ok(rep) => {
                if let Some(e) = rep.invariant_error {
                    eprintln!("INVARIANT FAILED seed={seed} tick={}: {e}", rep.tick);
                    failed = true;
                    break;
                }
                if rep.tick_us > budget_us {
                    over_budget += 1;
                    if std::env::var_os("SOAK_TRACE").is_some() {
                        let mut kinds = std::collections::BTreeMap::<String, u32>::new();
                        for e in cw.world.events() {
                            let k = format!("{e:?}");
                            let k = k.split([' ', '{', '(']).next().unwrap_or("?").to_string();
                            *kinds.entry(k).or_default() += 1;
                        }
                        eprintln!("  slow tick {} ({} µs): {kinds:?} casts {:?}", rep.tick, rep.tick_us, rep.casts);
                    }
                }
            }
            Err(p) => {
                let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                eprintln!("PANIC seed={seed} tick={}: {msg}", cw.world.tick);
                cw.stats.failures.push((seed, cw.world.tick, format!("panic: {msg}")));
                failed = true;
                break;
            }
        }
        if verbose && t > 0 && t % 20_000 == 0 {
            let (p50, p99, max) = cw.stats.percentiles();
            eprintln!(
                "  seed {seed}: {t} ticks, {:.1}s, casts {}/{}, constructs {}, p50 {p50}µs p99 {p99}µs max {max}µs",
                t0.elapsed().as_secs_f64(),
                cw.stats.casts_ok,
                cw.stats.casts,
                cw.world.constructs.len()
            );
        }
    }
    if over_budget > 0 {
        eprintln!("  seed {seed}: {over_budget} ticks over the {budget_us} µs budget");
    }
    let sh = cw.world.state_hash();
    let vh = cw.world.voxels.content_hash();
    (cw.stats, sh, vh, failed)
}

fn main() {
    let seed = arg("--seed", 1);
    let ticks = arg("--ticks", 200_000);
    let worlds = arg("--worlds", 1).max(1);
    let budget_ms = arg_str("--budget-ms").and_then(|v| v.parse::<f64>().ok()).unwrap_or(4.0);
    let det_ticks = arg("--det-ticks", 20_000).min(ticks);
    let random = arg_str("--rules").is_none_or(|r| r != "normal");
    let flat = arg_str("--arena").is_some_and(|a| a == "flat");
    let party = arg_str("--party").and_then(|v| v.parse::<u8>().ok());
    let budget_us = (budget_ms * 1000.0) as u32;
    let mut all = ChaosStats::default();
    let mut failed = false;
    let t0 = Instant::now();
    for k in 0..worlds {
        let s = seed.wrapping_add(k);
        eprintln!("world {k}: seed {s}, {ticks} ticks");
        let (st, _, _, f) = run_world(config(s, random, flat, party), ticks, budget_us, true);
        failed |= f;
        all.merge(&st);
    }
    // Determinism: world 0 twice for `det_ticks`.
    let (_, h1, v1, f1) = run_world(config(seed, random, flat, party), det_ticks, u32::MAX, false);
    let (_, h2, v2, f2) = run_world(config(seed, random, flat, party), det_ticks, u32::MAX, false);
    let det = !f1 && !f2 && h1 == h2 && v1 == v2;
    all.determinism = Some(det);
    if !det {
        eprintln!("DETERMINISM FAILED seed={seed} ({det_ticks} ticks): {h1:x}/{v1:x} vs {h2:x}/{v2:x}");
        failed = true;
    }
    let (p50, p99, max) = all.percentiles();
    println!(
        "soak: {} worlds, {} ticks in {:.1}s | casts {} ok {} | events {} | tick p50 {p50}µs p99 {p99}µs max {max}µs | uncovered mechanic pairs {} | determinism {} | failures {}",
        all.worlds,
        all.ticks,
        t0.elapsed().as_secs_f64(),
        all.casts,
        all.casts_ok,
        all.events,
        all.uncovered_pairs(),
        if det { "ok" } else { "FAILED" },
        all.failures.len()
    );
    if p99 > budget_us {
        eprintln!("TICK BUDGET FAILED: p99 {p99} µs > {budget_us} µs");
        failed = true;
    }
    if let Some(path) = arg_str("--stats") {
        if let Err(e) = std::fs::write(&path, all.to_json()) {
            eprintln!("could not write {path}: {e}");
        }
    }
    if failed {
        std::process::exit(1);
    }
}
