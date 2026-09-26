//! `--features alloc-count`: a counting global allocator for perf checks ("no per-frame
//! allocations in hot paths", docs/DESIGN.md). Every allocation bumps two relaxed atomics; the
//! [`AllocCountPlugin`] samples them once per frame in game and prints an `ALLOCS` line every 5 s
//! with `--bench` / a scenario (allocations and bytes per frame: avg / p99 / max).
//! Without the feature this module is empty and the system allocator is untouched.

#[cfg(feature = "alloc-count")]
mod imp {
    use bevy::prelude::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct Counting;

    pub static ALLOCS: AtomicU64 = AtomicU64::new(0);
    pub static BYTES: AtomicU64 = AtomicU64::new(0);

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;

    #[derive(Default)]
    struct Window {
        last: (u64, u64),
        allocs: Vec<u64>,
        bytes: Vec<u64>,
        next: f32,
    }

    pub struct AllocCountPlugin;

    impl Plugin for AllocCountPlugin {
        fn build(&self, app: &mut App) {
            app.add_systems(Last, sample);
        }
    }

    fn sample(time: Res<Time<Real>>, state: Res<State<crate::state::AppState>>, mut w: Local<Window>) {
        let now = (ALLOCS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
        let (da, db) = (now.0 - w.last.0, now.1 - w.last.1);
        w.last = now;
        if *state.get() != crate::state::AppState::InGame {
            return;
        }
        if w.allocs.capacity() == 0 {
            w.allocs.reserve(4096);
            w.bytes.reserve(4096);
            w.next = time.elapsed_secs() + 5.0;
        }
        if w.allocs.len() < w.allocs.capacity() {
            w.allocs.push(da);
            w.bytes.push(db);
        }
        if time.elapsed_secs() < w.next {
            return;
        }
        w.next = time.elapsed_secs() + 5.0;
        let n = w.allocs.len().max(1) as f64;
        let stat = |v: &mut Vec<u64>| {
            let avg = v.iter().sum::<u64>() as f64 / n;
            v.sort_unstable();
            let p99 = v.get(((v.len() as f64 * 0.99) as usize).min(v.len().saturating_sub(1))).copied().unwrap_or(0);
            (avg, p99, v.last().copied().unwrap_or(0))
        };
        let (aa, ap, am) = stat(&mut w.allocs);
        let (ba, bp, bm) = stat(&mut w.bytes);
        println!("ALLOCS frames={} per_frame_avg={aa:.1} p99={ap} max={am} bytes_avg={ba:.0} bytes_p99={bp} bytes_max={bm}", w.allocs.len());
        w.allocs.clear();
        w.bytes.clear();
    }
}

#[cfg(feature = "alloc-count")]
pub use imp::AllocCountPlugin;

/// No-op without the `alloc-count` feature.
#[cfg(not(feature = "alloc-count"))]
pub struct AllocCountPlugin;

#[cfg(not(feature = "alloc-count"))]
impl bevy::prelude::Plugin for AllocCountPlugin {
    fn build(&self, _app: &mut bevy::prelude::App) {}
}
