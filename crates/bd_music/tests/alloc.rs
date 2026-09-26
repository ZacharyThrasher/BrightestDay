//! `MusicPlayer::render` / `render_stems` / `command` must never allocate (audio-thread rule).
//! A counting global allocator watches this thread while the player runs every code path:
//! start, intensity changes, tempo ramps, all stingers, a track switch, stop.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use bd_music::{MusicCommand, MusicLibrary, MusicPlayer, StingerKind};

struct Counting;

thread_local! {
    static WATCH: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if WATCH.with(|w| w.get()) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if WATCH.with(|w| w.get()) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if WATCH.with(|w| w.get()) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn render_and_commands_never_allocate() {
    let lib = Arc::new(MusicLibrary::load_dir(&bd_music::default_music_dir()).expect("load"));
    let menu = lib.track_id("menu").expect("menu");
    let combat = lib.track_id("archipelago_combat").expect("combat");
    let mut p = MusicPlayer::new(Arc::clone(&lib), 48_000);
    let mut buf = vec![0.0f32; 2 * 256];
    let mut stems = [vec![0.0f32; 2 * 256], vec![0.0f32; 2 * 256], vec![0.0f32; 2 * 256], vec![0.0f32; 2 * 256]];
    let script: Vec<(usize, MusicCommand)> = vec![
        (0, MusicCommand::Play { track: menu, skip_intro: false }),
        (100, MusicCommand::SetIntensity(1.0)),
        (300, MusicCommand::Stinger(StingerKind::Ko)),
        (500, MusicCommand::Stinger(StingerKind::Streak)),
        (700, MusicCommand::SetTempo { multiplier: 1.3, ramp_secs: 2.0 }),
        (900, MusicCommand::Play { track: combat, skip_intro: true }),
        (1200, MusicCommand::Stinger(StingerKind::MatchPoint)),
        (2200, MusicCommand::Stinger(StingerKind::LastStanding)),
        (3400, MusicCommand::SetIntensity(0.2)),
        (3600, MusicCommand::Stinger(StingerKind::Victory)),
        (4200, MusicCommand::Play { track: menu, skip_intro: true }),
        (4500, MusicCommand::Stinger(StingerKind::Defeat)),
        (5000, MusicCommand::Stop),
    ];
    WATCH.with(|w| w.set(true));
    for i in 0..5600 {
        for (at, cmd) in &script {
            if *at == i {
                p.command(*cmd);
            }
        }
        if i % 3 == 0 {
            let [a, b, c, d] = &mut stems;
            p.render_stems(a, b, c, d);
        } else {
            p.render(&mut buf);
        }
        let _ = p.status();
    }
    WATCH.with(|w| w.set(false));
    let n = COUNT.with(|c| c.get());
    assert_eq!(n, 0, "the player allocated {n} times on the audio path");
}
