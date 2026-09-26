//! Pose cost of 30 animated constructs with role behaviours (the client's per-frame work in bd_fx
//! `pose_constructs` + bd_game `construct_anim`): clip layers alone vs. clip layers + role
//! behaviours (`ConstructAnimator::update` + `pose_layered_with`) + follow-through, and the
//! allocation count per frame after warm-up (must be 0).
//!
//! cargo run -p bd_shapes --release --example anim_perf

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use bd_core::mechanics::{Feature, Form, Mechanic};
use bd_shapes::behaviours::{AnimDrive, AnimEvent, ConstructAnimator};
use bd_shapes::*;
use glam::{Affine3A, Vec3};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;

fn recipe(form: Form, scale: f32, mech: &[Mechanic], feats: &[Feature], seed: u32) -> bd_core::mechanics::Recipe {
    let mut r = demo_recipe(form, seed, mech);
    r.scale = scale;
    r.features = feats.to_vec();
    r.normalized()
}

fn main() {
    use Feature::*;
    use Mechanic::*;
    let kinds = [
        recipe(Form::Vehicle, 7.0, &[Piloted], &[Treads, Turret], 2),
        recipe(Form::Vehicle, 9.0, &[Flies], &[Rotor, Exhaust], 1),
        recipe(Form::Vehicle, 5.0, &[], &[Wheels, Cab, Exhaust], 1),
        recipe(Form::Biped, 12.0, &[Piloted], &[], 1),
        recipe(Form::Quadruped, 3.0, &[Flies, Breath], &[], 3),
        recipe(Form::Kaiju, 20.0, &[Breath], &[], 1),
        recipe(Form::Fish, 3.0, &[], &[], 4),
        recipe(Form::Sword, 1.2, &[], &[], 1),
        recipe(Form::Vehicle, 12.0, &[Charge], &[Boiler, Cab, Pistons, Plow, Smokestack, Wheels], 7),
        recipe(Form::Dome, 4.0, &[Shoots, Walks], &[], 1),
    ];
    let mut models = Vec::new();
    for k in 0..30 {
        let r = &kinds[k % kinds.len()];
        let mut m = build_model(r).1;
        m.weight = weight_of(r);
        models.push(m);
    }
    let parts: usize = models.iter().map(|m| m.parts.len()).sum();
    let mut anims: Vec<ConstructAnimator> = models.iter().enumerate().map(|(i, m)| ConstructAnimator::new(m, i as u32)).collect();
    let mut secs: Vec<SecondaryMotion> = models.iter().map(SecondaryMotion::new).collect();
    let mut poses: Vec<Vec<Affine3A>> = models.iter().map(|m| vec![Affine3A::IDENTITY; m.parts.len()]).collect();
    for a in anims.iter_mut() {
        a.event(AnimEvent::Summon);
    }
    let frames = 3000;
    let dt = 1.0 / 144.0;
    let drive = |f: usize, i: usize| {
        let t = f as f32 * dt;
        AnimDrive {
            time: t,
            speed: 4.0 + 3.0 * (t * 0.7 + i as f32).sin(),
            turn: 0.6 * (t * 0.3 + i as f32).sin(),
            airborne: false,
            aim: Some(Vec3::new((t + i as f32).sin(), 0.1, -1.0).normalize()),
            look: Some(Vec3::new(3.0 * (t * 0.5).sin(), 1.5, -6.0)),
            pilot_near: (f / 400) % 2 == 1,
            piloted: (f / 700) % 3 == 2,
            charge: 0.0,
            ring: Some(Vec3::new(2.0, 1.5, 3.0)),
        }
    };
    let layers = |f: usize, i: usize| {
        [(AnimState::Locomotion { speed: 4.0, distance: f as f32 * dt * 4.0 + i as f32, airborne: false }, f as f32 * dt)]
    };
    // Baseline: clip layers only.
    let t0 = Instant::now();
    for f in 0..frames {
        for (i, m) in models.iter().enumerate() {
            pose_layered(m, &layers(f, i), &mut poses[i]);
        }
    }
    let base = t0.elapsed().as_secs_f64() * 1e6 / frames as f64;
    // With role behaviours + overlay + follow-through.
    let mut worst: f64 = 0.0;
    let mut allocs_after_warmup = 0;
    let t0 = Instant::now();
    for f in 0..frames {
        let tf = Instant::now();
        if f == 100 {
            ALLOCS.store(0, Ordering::Relaxed);
        }
        for (i, m) in models.iter().enumerate() {
            if f % 97 == (i % 97) {
                anims[i].event(AnimEvent::Fire { strength: 1.0 });
            }
            anims[i].update(&drive(f, i), dt);
            pose_layered_with(m, &layers(f, i), Some(anims[i].locals()), &mut poses[i]);
            secs[i].update(Affine3A::from_translation(Vec3::new(f as f32 * 0.03, 0.0, i as f32)), dt);
            secs[i].apply(m, &mut poses[i]);
        }
        if f >= 100 {
            worst = worst.max(tf.elapsed().as_secs_f64() * 1e6);
        }
        if f == frames - 1 {
            allocs_after_warmup = ALLOCS.load(Ordering::Relaxed);
        }
    }
    let full = t0.elapsed().as_secs_f64() * 1e6 / frames as f64;
    println!("30 constructs, {parts} parts, {frames} frames @144 Hz");
    println!("  clip layers only:            {base:8.1} us/frame");
    println!("  + role behaviours + overlay + follow-through: {full:8.1} us/frame  (worst frame {worst:.1} us)");
    println!("  behaviour overhead:          {:8.1} us/frame ({:.2} us per construct)", full - base, (full - base) / 30.0);
    println!("  allocations after warm-up:   {allocs_after_warmup}");
    assert_eq!(allocs_after_warmup, 0, "per-frame allocation");
}
