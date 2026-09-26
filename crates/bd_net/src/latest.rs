//! "Latest-only" lane: a small bounded queue that silently discards the oldest entries when the
//! consumer falls behind. Used for snapshots on the receiving side; the host's socket writer also
//! coalesces queued latest-lane messages so a slow link never builds up a backlog of stale state.

use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct LatestLane<T> {
    items: VecDeque<T>,
    capacity: usize,
    /// Items discarded because the consumer fell behind.
    pub dropped: u64,
}

impl<T> LatestLane<T> {
    /// `capacity` of 1 means "only ever the newest".
    pub fn new(capacity: usize) -> Self {
        Self { items: VecDeque::with_capacity(capacity.max(1)), capacity: capacity.max(1), dropped: 0 }
    }

    pub fn push(&mut self, item: T) {
        if self.items.len() == self.capacity {
            self.items.pop_front();
            self.dropped += 1;
        }
        self.items.push_back(item);
    }

    /// Only the newest item; everything older is discarded.
    pub fn take_latest(&mut self) -> Option<T> {
        let last = self.items.pop_back();
        self.dropped += self.items.len() as u64;
        self.items.clear();
        last
    }

    /// All buffered items, oldest first (e.g. to feed an interpolation buffer).
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.items.drain(..)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_stale() {
        let mut l = LatestLane::new(3);
        for i in 0..5 {
            l.push(i);
        }
        assert_eq!(l.dropped, 2);
        assert_eq!(l.drain().collect::<Vec<_>>(), vec![2, 3, 4]);
        for i in 0..3 {
            l.push(i);
        }
        assert_eq!(l.take_latest(), Some(2));
        assert!(l.is_empty());
        assert_eq!(l.dropped, 4);
    }
}
