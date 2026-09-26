//! Lock-free single-producer / single-consumer ring buffer.
//!
//! The game thread pushes [`crate::mixer::Cmd`]s, the audio thread pops them (and the reverse for
//! telemetry / garbage). `push` and `pop` never allocate, lock or block: a full ring rejects the
//! value (the caller counts the drop). Capacity is a power of two fixed at construction.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Inner<T> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    mask: usize,
    /// Next slot to read (owned by the consumer).
    head: AtomicUsize,
    /// Next slot to write (owned by the producer).
    tail: AtomicUsize,
}

// SAFETY: slots are only touched by the single producer (between tail and head+cap) or the single
// consumer (between head and tail); the Release/Acquire pair on head/tail publishes the writes.
unsafe impl<T: Send> Sync for Inner<T> {}
unsafe impl<T: Send> Send for Inner<T> {}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        let head = *self.head.get_mut();
        let tail = *self.tail.get_mut();
        let mut i = head;
        while i != tail {
            // SAFETY: slots in head..tail are initialised and not yet consumed.
            unsafe { (*self.slots[i & self.mask].get()).assume_init_drop() };
            i = i.wrapping_add(1);
        }
    }
}

/// Writing half.
pub struct Producer<T> {
    inner: Arc<Inner<T>>,
}

/// Reading half.
pub struct Consumer<T> {
    inner: Arc<Inner<T>>,
}

/// A ring holding at least `capacity` items (rounded up to a power of two, minimum 2).
pub fn ring<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    let cap = capacity.max(2).next_power_of_two();
    let slots: Box<[UnsafeCell<MaybeUninit<T>>]> = (0..cap).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect();
    let inner = Arc::new(Inner { slots, mask: cap - 1, head: AtomicUsize::new(0), tail: AtomicUsize::new(0) });
    (Producer { inner: inner.clone() }, Consumer { inner })
}

impl<T> Producer<T> {
    /// Push, or hand the value back when the ring is full.
    pub fn push(&mut self, v: T) -> Result<(), T> {
        let inner = &*self.inner;
        let tail = inner.tail.load(Ordering::Relaxed);
        let head = inner.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) > inner.mask {
            return Err(v);
        }
        // SAFETY: the slot at `tail` is free (the consumer is behind `head` ≤ tail - cap + 1).
        unsafe { (*inner.slots[tail & inner.mask].get()).write(v) };
        inner.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }
    /// Free slots right now.
    pub fn free(&self) -> usize {
        let inner = &*self.inner;
        inner.mask + 1 - inner.tail.load(Ordering::Relaxed).wrapping_sub(inner.head.load(Ordering::Acquire))
    }
    pub fn capacity(&self) -> usize {
        self.inner.mask + 1
    }
}

impl<T> Consumer<T> {
    pub fn pop(&mut self) -> Option<T> {
        let inner = &*self.inner;
        let head = inner.head.load(Ordering::Relaxed);
        let tail = inner.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        // SAFETY: head < tail, so the slot was initialised by the producer and published by the
        // Release store of `tail` (observed by our Acquire load).
        let v = unsafe { (*inner.slots[head & inner.mask].get()).assume_init_read() };
        inner.head.store(head.wrapping_add(1), Ordering::Release);
        Some(v)
    }
    pub fn len(&self) -> usize {
        let inner = &*self.inner;
        inner.tail.load(Ordering::Acquire).wrapping_sub(inner.head.load(Ordering::Relaxed))
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_and_full() {
        let (mut p, mut c) = ring::<u32>(4);
        for i in 0..4 {
            p.push(i).unwrap();
        }
        assert_eq!(p.push(9), Err(9));
        for i in 0..4 {
            assert_eq!(c.pop(), Some(i));
        }
        assert_eq!(c.pop(), None);
    }

    #[test]
    fn threads_keep_order() {
        let (mut p, mut c) = ring::<u64>(64);
        let n = 200_000u64;
        let t = std::thread::spawn(move || {
            let mut i = 0;
            while i < n {
                if p.push(i).is_ok() {
                    i += 1;
                } else {
                    std::hint::spin_loop();
                }
            }
        });
        let mut expect = 0;
        while expect < n {
            if let Some(v) = c.pop() {
                assert_eq!(v, expect);
                expect += 1;
            }
        }
        t.join().unwrap();
    }

    #[test]
    fn drops_leftovers() {
        let marker = Arc::new(());
        {
            let (mut p, _c) = ring::<Arc<()>>(8);
            p.push(marker.clone()).unwrap();
            p.push(marker.clone()).unwrap();
            assert_eq!(Arc::strong_count(&marker), 3);
        }
        assert_eq!(Arc::strong_count(&marker), 1);
    }
}
