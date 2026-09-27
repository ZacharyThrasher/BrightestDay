//! TEMPORARY diagnostic (alloc regression hunt) - delete before handoff.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use bd_sim::bots;
use bd_sim::world::{ArenaSource, SimConfig, World};

struct Counting;
static COUNT: AtomicUsize = AtomicUsize::new(0);
static ARMED: AtomicBool = AtomicBool::new(false);
static TRACES: Mutex<Vec<std::backtrace::Backtrace>> = Mutex::new(Vec::new());
thread_local!(static IN: Cell<bool> = const { Cell::new(false) });

fn note(size: usize) {
    if !ARMED.load(Ordering::Relaxed) { return; }
    COUNT.fetch_add(1, Ordering::Relaxed);
    if IN.with(|c| c.replace(true)) { return; }
    ARMED.store(false, Ordering::Relaxed);
    let bt = std::backtrace::Backtrace::force_capture();
    eprintln!("alloc size {size}");
    if let Ok(mut t) = TRACES.lock() { if t.len() < 40 { t.push(bt); } }
    ARMED.store(true, Ordering::Relaxed);
    IN.with(|c| c.set(false));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 { note(layout.size()); unsafe { System.alloc(layout) } }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { unsafe { System.dealloc(ptr, layout) } }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 { note(new_size); unsafe { System.realloc(ptr, layout, new_size) } }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn debug_allocs() {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 5, check_invariants: false, ..SimConfig::default() };
    cfg.timings.warmup_secs = 0.1;
    let mut w = World::new(cfg);
    if std::env::var("AIM").is_ok() { w.cfg.tuning.assist.aim = bd_sim::aim_assist::AimTuning::neutral(); }
    if std::env::var("LUNGE").is_ok() { let l = &mut w.cfg.tuning.assist.lunge; l.extra = 2.6; l.range_max = 5.5; l.cone_deg = 20.0; l.dash_speed = 26.0; l.dash_max_secs = 0.22; l.cooldown = 0.7; l.hit_resets_cooldown = false; l.aim_max_deg = 12.0; }
    let mut brains = bots::add_bots(&mut w, 0, 8, 0.9, 5);
    for b in &mut brains { b.casts_enabled = false; }
    let mut inputs = Vec::with_capacity(8);
    let mut ne = 0usize; let mut na = 0usize; let mut nt = 0usize;
    let mut hist: std::collections::BTreeMap<String, usize> = Default::default();
    for _ in 0..1200 {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        let ev = w.tick(&inputs);
        for e in ev.iter() { let s = format!("{e:?}"); let k: String = s.chars().take_while(|c| c.is_alphanumeric()).collect(); *hist.entry(format!("warm {k}")).or_default() += 1; }
    }
    for t in 0..1200 {
        inputs.clear();
        bots::drive(&mut w, &mut brains, &mut inputs);
        COUNT.store(0, Ordering::Relaxed);
        ARMED.store(true, Ordering::Relaxed);
        let ev = w.tick(&inputs);
        ARMED.store(false, Ordering::Relaxed);
        let c = COUNT.load(Ordering::Relaxed);
        let carved = ev.iter().any(|e| matches!(e, bd_sim::events::GameEvent::Carve { .. }));
        ne += ev.len(); if !carved { na += c; if c > 0 { nt += 1; } }
        if c > 0 { eprintln!("tick {t}: {c} allocs carved={carved}"); }
        for e in ev.iter() { let s = format!("{e:?}"); let k: String = s.chars().take_while(|c| c.is_alphanumeric()).collect(); *hist.entry(k).or_default() += 1; }
    }
    eprintln!("{hist:#?}"); eprintln!("SUMMARY events={ne} alloc_ticks={nt} allocs={na}");
    eprintln!("phase {:?} t={} ", w.match_state.phase, w.match_state.phase_time);
    for p in &w.players { eprintln!("player {:?} alive={} elim={} stocks={:?} pct={} kos={} deaths={} rt={} hp? launch={}", p.mv.pos, p.alive, p.eliminated, p.stocks, p.damage_pct, p.kos, p.deaths, p.respawn_timer, p.launch.is_some()); }
    let t = TRACES.lock().unwrap();
    for (i, bt) in t.iter().enumerate() { eprintln!("==== trace {i}\n{bt}"); }
}
