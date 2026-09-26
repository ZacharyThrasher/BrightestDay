//! The audio callback path must never allocate or free: a counting global allocator trips if
//! [`Mixer::process`] touches the heap while the guard is armed — including while it applies
//! every kind of command (plays, updates, stops, mix/reverb/duck/settings/config, bank swap,
//! music swap), steals voices, virtualises and resumes them.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use bd_audio::cue::{CueSpec, CueTable};
use bd_audio::mixer::RtConfig;
use bd_audio::*;
use glam::Vec3;

struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    /// Per-thread count (tests run in parallel threads).
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

/// Heap operations seen on this thread while armed.
fn allocs() -> usize {
    COUNT.with(|c| c.get())
}

fn bump() {
    // `try_with`: the allocator can run during thread-local teardown.
    let _ = ARMED.try_with(|a| {
        if a.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    });
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        bump();
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        bump();
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn armed<R>(f: impl FnOnce() -> R) -> R {
    ARMED.with(|a| a.set(true));
    let r = f();
    ARMED.with(|a| a.set(false));
    r
}

struct Tone(f32);
impl MusicSource for Tone {
    fn render(&mut self, out: &mut [f32]) {
        for (i, x) in out.iter_mut().enumerate() {
            self.0 += 0.01;
            *x = (self.0 + i as f32 * 0.0).sin() * 0.1;
        }
    }
    fn command(&mut self, _c: MusicCommand) {}
}

fn bank() -> Arc<SoundBank> {
    let mut b = SoundBank::empty(48000);
    let n = |secs: f32, f: f32| (0..(secs * 48000.0) as usize).map(|i| (i as f32 * f).sin() * 0.4).collect::<Vec<f32>>();
    b.push_mono("hit", "combat", n(0.5, 0.05), false);
    b.push_mono("step", "movement", n(0.1, 0.2), false);
    b.push_mono("bed", "ambience", n(1.0, 0.01), true);
    b.push_mono("confirm", "ui", n(0.2, 0.9), false);
    Arc::new(b)
}

#[test]
fn audio_callback_never_allocates() {
    let mut cues = CueTable::default();
    cues.sounds.insert("confirm".into(), CueSpec { bus: Some(Bus::Feedback), hdr: Some(false), ..Default::default() });
    cues.sounds.insert("step".into(), CueSpec { bus: Some(Bus::Foley), max_instances: Some(3), ..Default::default() });
    let mut cfg = MixConfig::default();
    cfg.voices.max_real = 12;
    cfg.voices.max_total = 40;
    let (mut e, mut m) = AudioEngine::new(bank(), &cues, cfg.clone(), 48000, 1);
    let mut out = vec![0.0f32; 480 * 2];
    // Warm up (first-use paths) before arming.
    m.process(&mut out, 2);

    for round in 0..40 {
        // Game thread (allowed to allocate): queue every kind of command.
        e.set_listener(Vec3::new(round as f32, 1.6, 0.0), Vec3::NEG_Z, Vec3::Y);
        let mut handles = Vec::new();
        for i in 0..30 {
            let p = Vec3::new(i as f32 - 15.0, 0.0, -(i as f32));
            handles.push(e.play(if i % 3 == 0 { "step" } else { "hit" }, PlayOpts::at(p).occlusion((i % 4) as f32 / 4.0)));
        }
        e.play("bed", PlayOpts::ui());
        e.play("confirm", PlayOpts::ui());
        if let Some(Some(h)) = handles.first() {
            e.update(*h, Vec3::new(1.0, 0.0, -1.0), -3.0, 1.2, 0.5);
            e.stop(*h, 0.05);
        }
        e.duck_named("heavy_hit");
        let mut mix = cfg.base_params();
        mix.world_lpf_hz = 2000.0 + round as f32 * 100.0;
        e.set_mix(mix);
        e.set_reverb(ReverbParams { size: 0.4 + (round % 5) as f32 * 0.3, slap_ms: 80.0, slap_db: -10.0, ..Default::default() });
        e.set_settings(AudioSettings { night_mode: round % 2 == 0, mono: round % 3 == 0, ..Default::default() });
        if round == 10 {
            e.reload(cfg.clone(), None);
            e.set_music(Box::new(Tone(0.0)));
        }
        if round == 20 {
            // Swap the bank: the old one must travel back to the game thread to be freed.
            e.set_bank(bank(), &cues);
            e.set_music(Box::new(Tone(1.0)));
        }
        e.music(MusicCommand::Intensity(0.8));
        let _ = RtConfig::from_mix(&cfg);

        // Audio thread: must not touch the heap.
        let before = allocs();
        armed(|| {
            for _ in 0..8 {
                m.process(&mut out, 2);
            }
        });
        let after = allocs();
        assert_eq!(after, before, "audio callback allocated/freed in round {round}");
        assert!(out.iter().all(|x| x.is_finite()));
        e.advance(0.016); // frees garbage on the game thread
    }
}

#[test]
fn guard_detects_allocation() {
    let before = allocs();
    let v = armed(|| std::hint::black_box(vec![1u8; 64]));
    assert!(allocs() > before, "the counting allocator must see this");
    drop(v);
}
