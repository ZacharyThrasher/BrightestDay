//! A steady-state `World::tick` (8 bots moving, ring-blasting, getting hit, dying and respawning;
//! no casts) performs no heap allocation: every per-tick buffer is reused. Terrain carves (a Smash
//! wall-splat crater now and then) allocate inside bd_voxel's edit path and are excluded.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use bd_sim::bots;
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
fn steady_state_tick_does_not_allocate() {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 5, check_invariants: false, ..SimConfig::default() };
    cfg.timings.warmup_secs = 0.1;
    let mut w = World::new(cfg);
    w.cfg.tuning.assist.aim = bd_sim::aim_assist::AimTuning::neutral();
    { let l = &mut w.cfg.tuning.assist.lunge; l.extra = 2.6; l.range_max = 5.5; l.cone_deg = 20.0; l.dash_speed = 26.0; l.dash_max_secs = 0.22; l.cooldown = 0.7; l.hit_resets_cooldown = false; l.aim_max_deg = 12.0; }
    let mut brains = bots::add_bots(&mut w, 0, 8, 0.9, 5);
    for b in &mut brains {
        b.casts_enabled = false;
    }
    let mut inputs = Vec::with_capacity(8);
    // Warm up: buffers reach their working capacity.
    for _ in 0..1200 {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        w.tick(&inputs);
    }
    let mut ticks_with_allocs = 0;
    let mut total = 0;
    let mut events = 0;
    let mut carve_ticks = 0;
    for _ in 0..1200 {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        COUNT.store(0, Ordering::Relaxed);
        ARMED.store(true, Ordering::Relaxed);
        let ev = w.tick(&inputs);
        ARMED.store(false, Ordering::Relaxed);
        let n = ev.len();
        let carved = ev.iter().any(|e| matches!(e, bd_sim::events::GameEvent::Carve { .. }));
        events += n;
        if carved {
            carve_ticks += 1;
            continue;
        }
        let c = COUNT.load(Ordering::Relaxed);
        total += c;
        if c > 0 {
            ticks_with_allocs += 1;
        }
    }
    assert!(events > 100, "the scenario must actually do things ({events} events)");
    assert!(carve_ticks < 60, "carves are rare here ({carve_ticks})");
    // Allow a handful of one-off buffer growths (a burst of events beyond the warm-up peak).
    eprintln!("ALLOCS {ticks_with_allocs} {total}");
    assert!(ticks_with_allocs <= 3, "{ticks_with_allocs} ticks allocated ({total} allocations)");
}
