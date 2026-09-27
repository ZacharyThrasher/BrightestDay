//! WILLPOWER = LAUNCH (host match rule `MatchRules::willpower_launch`).

use bd_core::PlayerId;
use bd_sim::bots;
use bd_sim::world::{ArenaSource, SimConfig, World};

fn ffa_hash(seed: u64, ticks: u32, launch: Option<bool>) -> (u64, usize) {
    let mut cfg = SimConfig { arena: ArenaSource::Archipelago { seed }, seed, check_invariants: true, ..SimConfig::default() };
    cfg.timings.warmup_secs = 1.0;
    let _ = launch;
    let mut w = World::new(cfg);
    let mut brains = bots::add_bots(&mut w, 0, 4, 0.8, seed);
    let mut inputs = Vec::new();
    let mut hits = 0;
    for _ in 0..ticks {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        let evs = w.tick(&inputs);
        hits += evs.iter().filter(|e| matches!(e, bd_sim::events::GameEvent::Hit { .. })).count();
    }
    let _ = PlayerId(0);
    (w.state_hash(), hits)
}

#[test]
fn baseline_print() {
    let (h, hits) = ffa_hash(3, 120 * 60, None);
    println!("BASELINE hash={h:#x} hits={hits}");
}
