//! Steady state with powers held (stands following their users, a Logia, a Zoan in beast form,
//! uneaten fruits lying around, a rush queue draining): `World::tick` performs no heap allocation.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use bd_core::corps::Corps;
use bd_core::input::{PlayerInput, buttons};
use bd_core::mechanics::{Element, Form};
use bd_core::{PlayerId, Vec3};
use bd_sim::bots::recipe;
use bd_sim::powers::PowerTable;
use bd_sim::world::{Aim, ArenaSource, SimConfig, World};

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
fn steady_state_tick_with_powers_does_not_allocate() {
    let _ = PowerTable::get(); // parse the table outside the measured window
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 5, check_invariants: false, ..SimConfig::default() };
    cfg.timings.warmup_secs = 0.1;
    cfg.rules.willpower = bd_sim::rules::WillpowerMode::Infinite;
    let mut w = World::new(cfg);
    let keys = ["star_platinum", "the_world", "mera_mera", "neko_leopard"];
    for k in 0..keys.len() {
        let id = PlayerId(k as u8 + 1);
        w.add_player(id, format!("p{k}"), Corps::from_index(k as u8)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    for (k, key) in keys.iter().enumerate() {
        let id = PlayerId(k as u8 + 1);
        let p = w.player_mut(id).unwrap();
        p.mv.pos = Vec3::new(-24.0 + 16.0 * k as f32, 8.0, 0.0);
        assert!(w.grant_power(id, key));
    }
    w.power_move(PlayerId(4), 1).expect("leopard form");
    // Two fruits nobody can reach.
    for (k, key) in ["gomu_gomu", "hie_hie"].iter().enumerate() {
        let mut r = recipe("fruit", Form::Ball, &[], Element::Light, 0.35);
        r.grants = Some(key.to_string());
        let id = PlayerId(1 + k as u8);
        let p = w.player(id).unwrap();
        let aim = Aim::new(p.eye(w.params()), Vec3::new(0.3, -0.2, -1.0).normalize());
        w.cast(id, r, None, aim).expect("fruit");
    }
    let typing = PlayerInput { buttons: buttons::TYPING, ..Default::default() };
    let inputs = [(PlayerId(1), typing), (PlayerId(2), typing)];
    let run = |w: &mut World, measure: bool| {
        let mut ticks_with_allocs = 0;
        for t in 0..900 {
            if t % 30 == 0 {
                // Rush tokens into empty air (queued, punched over the next ticks).
                w.stand_rush(PlayerId(1), 3);
            }
            COUNT.store(0, Ordering::Relaxed);
            ARMED.store(measure, Ordering::Relaxed);
            w.tick(&inputs);
            ARMED.store(false, Ordering::Relaxed);
            if COUNT.load(Ordering::Relaxed) > 0 {
                ticks_with_allocs += 1;
            }
        }
        ticks_with_allocs
    };
    run(&mut w, false);
    let n = run(&mut w, true);
    assert_eq!(w.powers.fruits.len(), 2, "fruits are still lying there");
    assert!(w.player(PlayerId(1)).unwrap().power.companion.is_some());
    assert!(n <= 2, "{n} ticks allocated");
}
