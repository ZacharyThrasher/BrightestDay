//! Creatures in steady state (walking, flying, swimming, biting, breathing, kiting, guarding, holding)
//! perform no heap allocation per tick: every creature decision reads fixed-size state (`c.ai`) and
//! writes into the world's reused output buffers.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use bd_core::corps::Corps;
use bd_core::mechanics::{Element, Form, Mechanic, Temperament};
use bd_core::{PlayerId, Quat, Vec3};
use bd_sim::bots::recipe;
use bd_sim::constructs::SpawnParams;
use bd_sim::rules::WillpowerMode;
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
fn creature_ticks_do_not_allocate() {
    let mut cfg = SimConfig { arena: ArenaSource::Flat { size_m: 96.0 }, seed: 9, check_invariants: false, ..SimConfig::default() };
    cfg.rules.willpower = WillpowerMode::Infinite;
    let mut w = World::new(cfg);
    for k in 1..=2u8 {
        w.add_player(PlayerId(k), format!("p{k}"), Corps::from_index(k)).unwrap();
    }
    w.start_live();
    w.tick(&[]);
    let a = PlayerId(1);
    let b = PlayerId(2);
    let pa = Vec3::new(-8.0, 8.0, 0.0);
    let pb = Vec3::new(8.0, 8.0, 0.0);
    // No carves (they queue replicated ops) and no payloads (they spawn constructs): pure behaviour.
    let bodies: [(Form, &[Mechanic], f32, Temperament); 9] = [
        (Form::Quadruped, &[], 1.2, Temperament::Calm),
        (Form::Biped, &[], 1.8, Temperament::Aggressive),
        (Form::Bird, &[], 1.0, Temperament::Sneaky),
        (Form::Fish, &[], 1.2, Temperament::Playful),
        (Form::Serpent, &[Mechanic::Constrict], 1.5, Temperament::Calm),
        (Form::Quadruped, &[Mechanic::Breath], 1.5, Temperament::Fearful),
        (Form::Biped, &[], 1.8, Temperament::Protective),
        (Form::Quadruped, &[Mechanic::Grab], 1.2, Temperament::Lazy),
        (Form::Arachnid, &[], 1.0, Temperament::Chaotic),
    ];
    for (i, (form, mechs, scale, t)) in bodies.iter().enumerate() {
        let mut r = recipe(&format!("c{i}"), *form, mechs, Element::Fire, *scale);
        r.temperament = *t;
        r.intensity = 0.8;
        let owner = if i % 2 == 0 { a } else { b };
        let tick = w.tick;
        w.constructs.spawn(SpawnParams {
            owner,
            recipe: r,
            shape: None,
            pos: Vec3::new(-6.0 + i as f32 * 1.5, 8.0, 6.0),
            rot: Quat::IDENTITY,
            vel: Vec3::ZERO,
            target: None,
            cost: 10.0,
            integrity: 1.0e6,
            forming: 0.05,
            life: 1.0e6,
            tick,
            colossal: false,
            model: None,
        });
    }
    let hold = |w: &mut World| {
        for (id, p) in [(a, pa), (b, pb)] {
            let pl = w.player_mut(id).unwrap();
            pl.mv.pos = p;
            pl.mv.vel = Vec3::ZERO;
            pl.health = 100.0;
            pl.alive = true;
            pl.spawn_shield = 0.0;
        }
    };
    for _ in 0..1200 {
        hold(&mut w);
        w.tick(&[]);
    }
    let mut ticks_with_allocs = 0;
    let mut events = 0;
    for _ in 0..1200 {
        hold(&mut w);
        COUNT.store(0, Ordering::Relaxed);
        ARMED.store(true, Ordering::Relaxed);
        events += w.tick(&[]).len();
        ARMED.store(false, Ordering::Relaxed);
        if COUNT.load(Ordering::Relaxed) > 0 {
            ticks_with_allocs += 1;
        }
    }
    assert!(events > 100, "the creatures must actually do things ({events} events)");
    assert!(ticks_with_allocs <= 3, "{ticks_with_allocs} creature ticks allocated");
}
