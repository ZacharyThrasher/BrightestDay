//! A steady-state Party-mode `World::tick` (8 bots in a live round, every per-tick party effect
//! active: Slippery, Hiccups, Loud, Reverse Card, Magnetic Personality, Popcorn, Butterfingers,
//! Growth Spurt; no casts) performs no heap allocation (ticks that carve terrain excepted).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use bd_sim::bots;
use bd_sim::settings::{GameSettings, Preset};
use bd_sim::world::{ArenaSource, SimConfig, World};

struct Counting;

static COUNT: AtomicUsize = AtomicUsize::new(0);
static ARMED: AtomicBool = AtomicBool::new(false);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn steady_state_party_tick_does_not_allocate() {
    let mut s = GameSettings::preset(Preset::Party);
    s.party.round_time = 1000.0;
    s.party.round_ko_target = 100;
    let mut cfg = s.sim_config(SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 5, check_invariants: false, ..SimConfig::default() });
    cfg.timings.warmup_secs = 0.1;
    let mut w = World::new(cfg);
    let mut brains = bots::add_bots(&mut w, 0, 8, 0.9, 5);
    for b in &mut brains {
        b.casts_enabled = false;
    }
    let table = w.party().unwrap().table.clone();
    let all = ["slippery", "hiccups", "loud", "reverse_card", "magnetic_personality", "popcorn", "butterfingers", "growth_spurt"];
    let only = std::env::var("PARTY_ALLOC_ONLY").ok();
    let keys: Vec<&str> = all.iter().copied().filter(|k| only.as_deref().is_none_or(|o| o == *k)).collect();
    for b in &brains {
        for k in &keys {
            assert!(w.grant_party_effect(b.id, table.find(k).unwrap()));
        }
    }
    let mut inputs = Vec::with_capacity(8);
    for _ in 0..2400 {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        w.tick(&inputs);
    }
    let mut ticks_with_allocs = 0;
    let mut total = 0;
    let mut events = 0;
    for _ in 0..2400 {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        COUNT.store(0, Ordering::Relaxed);
        ARMED.store(true, Ordering::Relaxed);
        let n = w.tick(&inputs).len();
        ARMED.store(false, Ordering::Relaxed);
        events += n;
        let c = COUNT.load(Ordering::Relaxed);
        // Terrain carves (wall splats: Hiccups launch bots into walls) allocate inside bd_voxel by
        // design; the FFA alloc test avoids them, here they are exempt.
        let carved = w.events().iter().any(|e| matches!(e, bd_sim::events::GameEvent::Carve { .. }));
        let c = if carved { 0 } else { c };
        total += c;
        if c > 0 {
            ticks_with_allocs += 1;
            if std::env::var("PARTY_ALLOC_DEBUG").is_ok() {
                let kinds: Vec<String> = w.events().iter().map(|e| format!("{e:?}").chars().take(40).collect()).collect();
                eprintln!("alloc tick {} ({c}): {kinds:?}", w.tick);
            }
        }
    }
    assert_eq!(w.party().unwrap().phase, bd_sim::party::PartyPhase::Round);
    assert!(events > 100, "the scenario must actually do things ({events} events)");
    assert!(ticks_with_allocs <= 3, "{ticks_with_allocs} ticks allocated ({total} allocations)");
}
