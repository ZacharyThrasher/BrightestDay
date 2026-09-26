//! Sequenced inputs: client-side send buffer with redundancy, host-side per-client jitter queue.
//!
//! Flow: every client tick, `InputBuffer::push` assigns the next sequence number. At the send rate
//! (e.g. 60 Hz) the client sends `InputBuffer::packet`, which carries every input not yet sent plus
//! older unacknowledged ones up to the redundancy count. The host feeds packets into that client's
//! [`InputQueue`] and pops exactly one input per host tick. When the queue is empty it repeats the
//! last input (starvation), when it stays too deep it drops one (latency creep). Each snapshot
//! tells the client the last sequence the host consumed so it can reconcile and trim its buffer.

use std::collections::VecDeque;

use bd_core::input::PlayerInput;
use serde::{Deserialize, Serialize};

/// An input with a per-client monotonically increasing sequence number.
pub trait Sequenced: Clone {
    fn seq(&self) -> u32;
    fn set_seq(&mut self, seq: u32);
    /// The input the host should use when it has to repeat this one because the next is late.
    /// Default: identical. Override to clear one-shot fields.
    fn for_repeat(&self) -> Self {
        self.clone()
    }
}

impl Sequenced for PlayerInput {
    fn seq(&self) -> u32 {
        self.seq
    }
    fn set_seq(&mut self, seq: u32) {
        self.seq = seq;
    }
    fn for_repeat(&self) -> Self {
        // Reel notches are per-tick deltas; everything else is "held" state and safe to repeat.
        PlayerInput { reel_notches: 0, ..*self }
    }
}

/// What a client sends: a contiguous run of inputs, ascending by seq.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputPacket<T> {
    /// The client's interpolation render tick (see [`crate::SnapshotBuffer::render_tick`]) when the
    /// newest input in this packet was sampled. The host rewinds to this for lag compensation.
    pub view_tick: f64,
    pub inputs: Vec<T>,
}

/// Client-side ring of sequenced inputs awaiting acknowledgement.
#[derive(Clone, Debug)]
pub struct InputBuffer<T> {
    next_seq: u32,
    last_acked: u32,
    last_sent: u32,
    pending: VecDeque<T>,
    /// Minimum number of (newest, unacked) inputs repeated in every packet.
    pub redundancy: usize,
    /// Upper bound on buffered inputs (oldest are discarded).
    pub capacity: usize,
}

impl<T: Sequenced> InputBuffer<T> {
    /// Sequence numbers start at 1; an ack of 0 means "nothing processed yet".
    pub fn new(redundancy: usize) -> Self {
        Self {
            next_seq: 1,
            last_acked: 0,
            last_sent: 0,
            pending: VecDeque::with_capacity(256),
            redundancy: redundancy.max(1),
            capacity: 1024,
        }
    }

    /// Record an input; its seq field is overwritten with the assigned sequence number.
    pub fn push(&mut self, mut input: T) -> u32 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        input.set_seq(seq);
        self.pending.push_back(input);
        while self.pending.len() > self.capacity {
            self.pending.pop_front();
        }
        seq
    }

    /// Like [`push`](Self::push) but with an explicit (strictly increasing) sequence number, e.g.
    /// when sequence numbers are client tick numbers and the pacer skipped ticks.
    pub fn push_at(&mut self, seq: u32, input: T) -> u32 {
        debug_assert!(seq >= self.next_seq, "push_at seq must increase");
        self.next_seq = seq.max(self.next_seq);
        self.push(input)
    }

    /// The host has consumed everything up to and including `seq`.
    pub fn ack(&mut self, seq: u32) {
        if seq <= self.last_acked {
            return;
        }
        self.last_acked = seq;
        while self.pending.front().is_some_and(|i| i.seq() <= seq) {
            self.pending.pop_front();
        }
    }

    /// Build the next packet: all inputs not sent yet, padded with older unacked ones up to
    /// `redundancy`. Returns `None` when there is nothing new to send.
    pub fn packet(&mut self, view_tick: f64) -> Option<InputPacket<T>> {
        let newest = self.pending.back()?.seq();
        if newest <= self.last_sent {
            return None;
        }
        let unsent = self.pending.iter().rev().take_while(|i| i.seq() > self.last_sent).count();
        let n = unsent.max(self.redundancy).min(self.pending.len());
        let inputs: Vec<T> = self.pending.iter().skip(self.pending.len() - n).cloned().collect();
        self.last_sent = newest;
        Some(InputPacket { view_tick, inputs })
    }

    pub fn last_acked(&self) -> u32 {
        self.last_acked
    }

    /// Sequence number the next pushed input will get.
    pub fn next_seq(&self) -> u32 {
        self.next_seq
    }

    /// Unacknowledged inputs, oldest first.
    pub fn unacked(&self) -> impl Iterator<Item = &T> {
        self.pending.iter()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// One input handed to the host simulation.
#[derive(Clone, Debug, PartialEq)]
pub struct QueuedInput<T> {
    pub input: T,
    /// Last sequence consumed — report it back to the client as `last_processed_input_seq`.
    /// For a repeated input this is unchanged from the previous tick.
    pub seq: u32,
    /// The next input was not here in time; this is a repeat of the previous one (the client's
    /// timeline is held back one tick, i.e. the jitter buffer grew).
    pub repeated: bool,
    /// Client's render tick when this input was sampled (for lag compensation).
    pub view_tick: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputQueueStats {
    pub received: u64,
    /// Ticks that consumed a real input.
    pub consumed: u64,
    /// Ticks where the next input was missing and the last one was repeated.
    pub starved: u64,
    /// Inputs dropped to shrink an over-deep queue (latency creep / after a stall).
    pub dropped: u64,
    /// Redundant copies of inputs already received.
    pub duplicates: u64,
    /// Sequence numbers the client skipped (e.g. its pacer snapped forward).
    pub skipped: u64,
}

/// Host-side per-client input queue: a self-tuning jitter buffer.
///
/// - Consumes one input per host tick, in sequence order.
/// - **Starved** (next input not here yet): repeats the previous input *without* consuming a
///   sequence number. The client's timeline is held back one tick, i.e. the buffer grows by one
///   tick exactly when jitter demands it. (The client sees a small misprediction and reconciles.)
/// - **Over-buffered**: if the queue never dropped below `slack` extra inputs during a whole
///   `window`, one input is dropped (latency creeps back down). Far too deep (a stalled client
///   bursting its backlog) drops straight down to `prime_depth`.
///
/// The buffer therefore settles at the smallest depth that absorbs the observed jitter.
/// The client must send inputs in sequence order (TCP guarantees delivery order).
#[derive(Clone, Debug)]
pub struct InputQueue<T> {
    buf: VecDeque<(T, f64)>,
    /// Next sequence to consume (0 = nothing received yet).
    next_seq: u32,
    /// Highest sequence ever received (inputs arrive in order, so anything <= is a duplicate).
    highest: u32,
    last: Option<(T, f64)>,
    primed: bool,
    window_min: usize,
    window_ticks: u32,
    /// Inputs buffered before the first one is consumed.
    pub prime_depth: usize,
    /// Minimum spare inputs kept at the worst moment of a window before trimming.
    pub slack: usize,
    /// Ticks per trimming window.
    pub window: u32,
    /// Beyond this depth, trim immediately down to `prime_depth`.
    pub max_depth: usize,
    pub stats: InputQueueStats,
}

impl<T: Sequenced> InputQueue<T> {
    /// `prime_depth` ≈ 2 is right for 60 Hz input packets at a 120 Hz tick.
    pub fn new(prime_depth: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(64),
            next_seq: 0,
            highest: 0,
            last: None,
            primed: false,
            window_min: usize::MAX,
            window_ticks: 0,
            prime_depth: prime_depth.max(1),
            slack: 1,
            window: 240,
            max_depth: 16,
            stats: InputQueueStats::default(),
        }
    }

    /// Currently buffered inputs.
    pub fn depth(&self) -> usize {
        self.buf.len()
    }

    /// Last sequence handed to the simulation (0 if none) — the ack to send to the client.
    pub fn last_consumed(&self) -> u32 {
        self.next_seq.saturating_sub(1)
    }

    /// Feed a packet from the client.
    pub fn receive(&mut self, packet: InputPacket<T>) {
        let Some(newest) = packet.inputs.last().map(|i| i.seq()) else { return };
        for input in packet.inputs {
            let seq = input.seq();
            if seq <= self.highest {
                self.stats.duplicates += 1;
                continue;
            }
            self.highest = seq;
            if self.next_seq == 0 {
                self.next_seq = seq;
            }
            // Untrusted input: a hostile packet may be out of order (newest < seq). Non-finite view
            // ticks pass through; `clamp_view_tick` neutralises them.
            let view = packet.view_tick - newest.saturating_sub(seq) as f64;
            self.buf.push_back((input, view));
            self.stats.received += 1;
        }
    }

    /// Pop the input for this host tick. `None` only before the first input ever arrived (or
    /// while the initial buffer fills), in which case the player should stand still.
    pub fn next(&mut self) -> Option<QueuedInput<T>> {
        if !self.primed {
            if self.buf.len() >= self.prime_depth {
                self.primed = true;
            } else {
                return None;
            }
        }
        let Some((input, view)) = self.buf.pop_front() else {
            // Starved: hold the client's timeline and repeat; the buffer grows by one tick.
            let (last, view) = self.last.as_ref()?;
            let input = last.for_repeat();
            let view = view + 1.0;
            self.last = Some((input.clone(), view));
            self.stats.starved += 1;
            self.window_min = 0;
            return Some(QueuedInput { seq: self.last_consumed(), input, repeated: true, view_tick: view });
        };
        let seq = input.seq();
        if seq > self.next_seq {
            // The client skipped sequence numbers; nothing to wait for (delivery is ordered).
            self.stats.skipped += (seq - self.next_seq) as u64;
        }
        self.next_seq = seq.wrapping_add(1);
        self.last = Some((input.clone(), view));
        self.stats.consumed += 1;

        let depth = self.buf.len();
        if depth > self.max_depth {
            // Far too deep (client stalled, then burst its backlog): catch up at once.
            while self.buf.len() > self.prime_depth {
                self.buf.pop_front();
                self.stats.dropped += 1;
            }
            if let Some((i, _)) = self.buf.front() {
                self.next_seq = i.seq();
            }
            self.window_min = usize::MAX;
            self.window_ticks = 0;
        } else {
            self.window_min = self.window_min.min(depth);
            self.window_ticks += 1;
            if self.window_ticks >= self.window {
                if self.window_min > self.slack {
                    if let Some((i, _)) = self.buf.pop_front() {
                        self.next_seq = i.seq().wrapping_add(1);
                        self.stats.dropped += 1;
                    }
                }
                self.window_min = usize::MAX;
                self.window_ticks = 0;
            }
        }
        Some(QueuedInput { seq, input, repeated: false, view_tick: view })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inp(x: i8) -> PlayerInput {
        PlayerInput { move_x: x, reel_notches: 1, ..Default::default() }
    }

    #[test]
    fn buffer_redundancy_and_ack() {
        let mut b = InputBuffer::new(4);
        assert!(b.packet(0.0).is_none());
        for i in 0..2 {
            b.push(inp(i));
        }
        let p = b.packet(10.0).unwrap();
        assert_eq!(p.inputs.iter().map(|i| i.seq).collect::<Vec<_>>(), vec![1, 2]);
        assert!(b.packet(10.0).is_none(), "nothing new");
        for i in 0..2 {
            b.push(inp(i));
        }
        let p = b.packet(12.0).unwrap();
        assert_eq!(p.inputs.iter().map(|i| i.seq).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
        b.ack(3);
        b.push(inp(0));
        let p = b.packet(13.0).unwrap();
        assert_eq!(p.inputs.iter().map(|i| i.seq).collect::<Vec<_>>(), vec![4, 5]);
        // A burst larger than the redundancy is still sent in full.
        for _ in 0..10 {
            b.push(inp(0));
        }
        let p = b.packet(14.0).unwrap();
        assert_eq!(p.inputs.first().unwrap().seq, 6);
        assert_eq!(p.inputs.len(), 10);
    }

    #[test]
    fn queue_jitter_starve_and_dedupe() {
        let mut client = InputBuffer::new(4);
        let mut host = InputQueue::new(2);
        assert!(host.next().is_none());
        client.push(inp(1));
        host.receive(client.packet(100.0).unwrap());
        assert!(host.next().is_none(), "priming");
        client.push(inp(2));
        host.receive(client.packet(101.0).unwrap());
        let a = host.next().unwrap();
        assert_eq!((a.seq, a.repeated, a.input.move_x), (1, false, 1));
        assert!((a.view_tick - 100.0).abs() < 1e-9);
        let b = host.next().unwrap();
        assert_eq!((b.seq, b.repeated), (2, false));
        // Starve: repeats last with cleared one-shots, ack unchanged.
        let c = host.next().unwrap();
        assert_eq!((c.seq, c.repeated, c.input.move_x, c.input.reel_notches), (2, true, 2, 0));
        // Seq 3 arrives late: it is still used (timeline was held), nothing lost.
        client.push(inp(3));
        client.push(inp(4));
        host.receive(client.packet(104.0).unwrap());
        let d = host.next().unwrap();
        assert_eq!((d.seq, d.repeated, d.input.move_x), (3, false, 3));
        assert_eq!(host.stats.starved, 1);
        assert!(host.stats.duplicates >= 1, "redundant copies ignored");
        assert_eq!(host.last_consumed(), 3);
    }

    #[test]
    fn queue_shrinks_when_too_deep_and_catches_up_after_stall() {
        let mut client = InputBuffer::new(2);
        let mut host = InputQueue::new(2);
        host.window = 10;
        for _ in 0..6 {
            client.push(inp(0));
        }
        host.receive(client.packet(0.0).unwrap());
        // Exactly one in, one out per tick: depth ~5 > slack → one dropped per window.
        for _ in 0..60 {
            client.push(inp(0));
            host.receive(client.packet(0.0).unwrap());
            assert!(!host.next().unwrap().repeated);
        }
        assert!(host.stats.dropped >= 3, "dropped {}", host.stats.dropped);
        assert!(host.depth() <= 2, "depth {}", host.depth());
        // Client stalls for 0.5 s: host holds and repeats.
        for _ in 0..60 {
            host.next().unwrap();
        }
        assert!(host.stats.starved >= 55);
        // Client catches up with a burst of 60 inputs: host drops the backlog at once.
        for _ in 0..60 {
            client.push(inp(5));
        }
        host.receive(client.packet(0.0).unwrap());
        let q = host.next().unwrap();
        assert!(!q.repeated);
        assert!(host.depth() <= host.prime_depth, "depth {}", host.depth());
        assert_eq!(q.input.move_x, 5);
    }
}
