//! Property tests for the pure netcode helpers (see docs/TESTING.md). Everything a remote peer
//! controls — sequence numbers, view ticks, snapshot ticks, clock samples — is fuzzed with
//! out-of-order, duplicate, huge-gap, wrapping and non-finite values. Invariants: no panic, bounded
//! memory, finite outputs, and the documented ordering guarantees.

use bd_core::input::PlayerInput;
use bd_net::{
    ClockSync, InputBuffer, InputPacket, InputQueue, InterpConfig, LagCompHistory, LatestLane, Reconcilable, ReconcileOutcome, Reconciler,
    Sequenced, SnapshotBuffer, TickPacer, clamp_view_tick, lobby_code_from_url, resolve_address,
};
use glam::Vec3;
use proptest::prelude::*;

/// Case count × `BD_PROP_SCALE` (default 1; `tools/check.ps1 -Deep` sets 10).
fn scaled(n: u32) -> u32 {
    std::env::var("BD_PROP_SCALE").ok().and_then(|s| s.parse::<u32>().ok()).map_or(n, |k| n.saturating_mul(k.max(1)))
}

fn wild_f64() -> impl Strategy<Value = f64> {
    prop_oneof![
        4 => -1e4f64..1e4,
        2 => any::<f64>(),
        1 => Just(f64::NAN),
        1 => Just(f64::INFINITY),
        1 => Just(f64::NEG_INFINITY),
        1 => Just(0.0f64),
        1 => Just(1e300f64),
        1 => Just(-1e300f64),
    ]
}

/// Sequence numbers a hostile client might send.
fn wild_seq() -> impl Strategy<Value = u32> {
    prop_oneof![
        4 => 0u32..200,
        1 => any::<u32>(),
        1 => Just(u32::MAX),
        1 => Just(u32::MAX - 1),
        1 => Just(0u32),
    ]
}

fn inp(seq: u32, x: i8) -> PlayerInput {
    PlayerInput { seq, move_x: x, reel_notches: 1, ..Default::default() }
}

// ---------------------------------------------------------------------------------- InputQueue

#[derive(Debug, Clone)]
enum QOp {
    /// A packet as a hostile/buggy client might send it (any seqs, any order, any view tick).
    Packet {
        view_tick: f64,
        seqs: Vec<u32>,
    },
    /// A well-formed contiguous run from `start` (typical honest packet, possibly with a gap).
    Run {
        view_tick: f64,
        start: u32,
        len: u8,
    },
    Next,
}

fn q_op() -> impl Strategy<Value = QOp> {
    prop_oneof![
        2 => (wild_f64(), prop::collection::vec(wild_seq(), 0..40)).prop_map(|(view_tick, seqs)| QOp::Packet { view_tick, seqs }),
        2 => (wild_f64(), wild_seq(), 0u8..20).prop_map(|(view_tick, start, len)| QOp::Run { view_tick, start, len }),
        4 => Just(QOp::Next),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(1024), ..ProptestConfig::default() })]

    #[test]
    fn input_queue_survives_hostile_packets(prime in 1usize..6, ops in prop::collection::vec(q_op(), 0..200)) {
        let mut q: InputQueue<PlayerInput> = InputQueue::new(prime);
        let mut last_seq: Option<u32> = None;
        for op in ops {
            match op {
                QOp::Packet { view_tick, seqs } => {
                    let inputs = seqs.iter().map(|s| inp(*s, 1)).collect();
                    q.receive(InputPacket { view_tick, inputs });
                }
                QOp::Run { view_tick, start, len } => {
                    let inputs = (0..len as u32).map(|i| inp(start.wrapping_add(i), 2)).collect();
                    q.receive(InputPacket { view_tick, inputs });
                }
                QOp::Next => {
                    if let Some(out) = q.next() {
                        // The ack reported to the client never goes backwards...
                        if let Some(prev) = last_seq {
                            prop_assert!(out.seq >= prev || prev == u32::MAX, "ack went backwards: {} -> {}", prev, out.seq);
                        }
                        last_seq = Some(out.seq);
                        // ...repeats never carry one-shot fields...
                        if out.repeated {
                            prop_assert_eq!(out.input.reel_notches, 0);
                        }
                        // ...and the lag-comp view tick is sane once clamped by the host.
                        let v = clamp_view_tick(out.view_tick, 10_000, 30.0);
                        prop_assert!(v.is_finite() && (9_970.0..=10_000.0).contains(&v), "clamped view tick {}", v);
                    }
                }
            }
        }
        let s = q.stats;
        prop_assert!(s.consumed + s.dropped <= s.received);
    }

    /// Honest client: ascending contiguous seqs, arbitrary batching and tick timing ⇒ every input is
    /// consumed at most once, in order, and nothing is invented.
    #[test]
    fn input_queue_honest_client_in_order(batches in prop::collection::vec((0u8..12, 0u8..6), 1..80), redundancy in 1usize..6) {
        let mut client = InputBuffer::<PlayerInput>::new(redundancy);
        let mut host = InputQueue::<PlayerInput>::new(2);
        let mut consumed = Vec::new();
        for (n_push, n_ticks) in batches {
            for _ in 0..n_push {
                client.push(inp(0, 0));
            }
            if let Some(p) = client.packet(0.0) {
                // Packets are ascending, contiguous and include every unsent input.
                prop_assert!(p.inputs.windows(2).all(|w| w[1].seq == w[0].seq + 1));
                prop_assert_eq!(p.inputs.last().map(|i| i.seq), Some(client.next_seq() - 1));
                host.receive(p);
            }
            for _ in 0..n_ticks {
                if let Some(q) = host.next() {
                    if !q.repeated {
                        consumed.push(q.seq);
                    }
                    client.ack(q.seq);
                }
            }
        }
        prop_assert!(consumed.windows(2).all(|w| w[0] < w[1]), "consumed out of order: {:?}", consumed);
        prop_assert!(consumed.iter().all(|&s| s >= 1 && s < client.next_seq()));
    }

    /// Client-side buffer under random pushes/acks/sends (including acks from the future and
    /// explicit seq jumps): bounded, ascending, redundancy honoured.
    #[test]
    fn input_buffer_invariants(ops in prop::collection::vec((0u8..4, any::<u32>()), 0..300), redundancy in 1usize..8, cap in 1usize..64) {
        let mut b = InputBuffer::<PlayerInput>::new(redundancy);
        b.capacity = cap;
        for (kind, x) in ops {
            match kind {
                0 => {
                    b.push(inp(0, 0));
                }
                1 => b.ack(x % b.next_seq()),
                2 => {
                    // push_at requires increasing seqs: jump ahead by a bounded amount.
                    let s = b.next_seq().saturating_add(x % 1000);
                    if s >= b.next_seq() {
                        b.push_at(s, inp(0, 0));
                    }
                }
                _ => {
                    if let Some(p) = b.packet(0.0) {
                        prop_assert!(!p.inputs.is_empty());
                        prop_assert!(p.inputs.windows(2).all(|w| w[0].seq < w[1].seq));
                        prop_assert!(p.inputs.len() >= redundancy.min(b.len()));
                        prop_assert!(p.inputs.iter().all(|i| i.seq > b.last_acked()));
                    }
                }
            }
            prop_assert!(b.len() <= cap);
            prop_assert!(b.unacked().all(|i| i.seq > b.last_acked()));
            let seqs: Vec<u32> = b.unacked().map(|i| i.seq).collect();
            prop_assert!(seqs.windows(2).all(|w| w[0] < w[1]));
        }
    }
}

#[test]
fn input_buffer_near_u32_wrap_does_not_panic() {
    let mut b = InputBuffer::<PlayerInput>::new(4);
    b.push_at(u32::MAX - 3, inp(0, 0));
    for _ in 0..8 {
        b.push(inp(0, 0));
        let _ = b.packet(0.0);
    }
    b.ack(u32::MAX);
    let mut q = InputQueue::<PlayerInput>::new(1);
    q.receive(InputPacket { view_tick: 0.0, inputs: vec![inp(u32::MAX - 1, 0), inp(u32::MAX, 0)] });
    for _ in 0..4 {
        let _ = q.next();
    }
}

// ---------------------------------------------------------------------------------- Reconciler

#[derive(Clone, Debug, PartialEq)]
struct S {
    pos: Vec3,
}
impl Reconcilable for S {
    fn diverges_from(&self, a: &Self) -> bool {
        (self.pos - a.pos).length() > 1e-3
    }
    fn render_position(&self) -> Vec3 {
        self.pos
    }
}
fn step(s: &mut S, i: &i8) {
    s.pos.x += *i as f32 * 0.25;
    s.pos.y = (s.pos.y + 1.0) % 7.0;
}

#[derive(Debug, Clone)]
enum ROp {
    Predict(i8),
    /// Ack `back` inputs behind the newest prediction (may be negative = from the future, or far past).
    Reconcile {
        back: i32,
        jitter: f32,
    },
    Smooth(f32),
    Reset,
}

fn r_op() -> impl Strategy<Value = ROp> {
    prop_oneof![
        5 => any::<i8>().prop_map(ROp::Predict),
        3 => (-5i32..400, prop_oneof![3 => Just(0.0f32), 1 => -3.0f32..3.0, 1 => -100.0f32..100.0])
            .prop_map(|(back, jitter)| ROp::Reconcile { back, jitter }),
        1 => prop_oneof![0.0f32..0.1, Just(f32::NAN), Just(f32::INFINITY), Just(-1.0f32)].prop_map(ROp::Smooth),
        1 => Just(ROp::Reset),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(1024), ..ProptestConfig::default() })]

    /// Differential test against a simple model: after any correction the predicted state equals
    /// "host state + replay of every retained newer input"; stale acks are ignored; the visual
    /// offset is bounded and finite.
    #[test]
    fn reconciler_matches_model(ops in prop::collection::vec(r_op(), 0..300), cap in 1usize..64) {
        let mut r: Reconciler<S, i8> = Reconciler::new(S { pos: Vec3::ZERO });
        r.capacity = cap;
        let mut next_seq: u32 = 1;
        // Model: retained (seq, input) and the host's own simulation.
        let mut hist: Vec<(u32, i8)> = Vec::new();
        let mut all: Vec<i8> = Vec::new();
        let mut last_ack: Option<u32> = None;
        for op in ops {
            match op {
                ROp::Predict(i) => {
                    r.predict(next_seq, i, step);
                    hist.push((next_seq, i));
                    if hist.len() > cap {
                        hist.remove(0);
                    }
                    all.push(i);
                    next_seq += 1;
                }
                ROp::Reconcile { back, jitter } => {
                    let ack = (next_seq as i64 - 1 - back as i64).clamp(0, u32::MAX as i64) as u32;
                    // Host truth for `ack`: every input up to ack, plus a disturbance.
                    let mut host = S { pos: Vec3::ZERO };
                    for i in all.iter().take(ack as usize) {
                        step(&mut host, i);
                    }
                    host.pos.z += jitter;
                    let out = r.reconcile(ack, host.clone(), step);
                    if last_ack.is_some_and(|a| ack < a) {
                        prop_assert_eq!(out, ReconcileOutcome::Stale);
                        continue;
                    }
                    last_ack = Some(ack);
                    hist.retain(|(s, _)| *s >= ack);
                    if hist.first().is_some_and(|(s, _)| *s == ack) {
                        hist.remove(0);
                    }
                    if let ReconcileOutcome::Corrected { error, .. } = out {
                        let mut want = host;
                        for (_, i) in &hist {
                            step(&mut want, i);
                        }
                        prop_assert!(!r.current().diverges_from(&want), "replay mismatch: {:?} vs {:?}", r.current(), want);
                        prop_assert!(error.is_finite() && error.length() <= r.snap_distance + 1e-4);
                    }
                    prop_assert_eq!(r.pending(), hist.len());
                }
                ROp::Smooth(dt) => r.update_smoothing(dt),
                ROp::Reset => {
                    r.reset(S { pos: Vec3::ZERO });
                    hist.clear();
                    all.clear();
                    next_seq = 1;
                    last_ack = None;
                    // A fresh reconciler is the documented way to restart the seq space.
                    r = Reconciler::new(S { pos: Vec3::ZERO });
                    r.capacity = cap;
                }
            }
            prop_assert!(r.pending() <= cap);
            prop_assert!(r.render_position().is_finite(), "render position {:?}", r.render_position());
            prop_assert!(r.visual_offset().length() <= r.snap_distance + 1e-4);
        }
    }
}

// ------------------------------------------------------------------------------ SnapshotBuffer

#[derive(Debug, Clone)]
enum SOp {
    Push { dt: u64 },
    PushAbs(u64),
    Update { dt: f64 },
}

fn s_op() -> impl Strategy<Value = SOp> {
    prop_oneof![
        4 => prop_oneof![Just(0u64), 1u64..4, 4u64..200, Just(1u64 << 40)].prop_map(|dt| SOp::Push { dt }),
        1 => prop_oneof![any::<u64>(), Just(u64::MAX), Just(0u64)].prop_map(SOp::PushAbs),
        5 => prop_oneof![0.0f64..0.05, 0.05f64..3.0, Just(0.0), Just(-1.0), Just(1e6)].prop_map(|dt| SOp::Update { dt }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(1024), ..ProptestConfig::default() })]

    #[test]
    fn snapshot_buffer_outputs_are_sane(ops in prop::collection::vec(s_op(), 0..300), cap in 1usize..80, extrap in 0.0f64..10.0) {
        let cfg = InterpConfig { capacity: cap, max_extrapolate_ticks: extrap, ..InterpConfig::default() };
        let max_delay = cfg.max_delay_ticks;
        let mut b: SnapshotBuffer<u64> = SnapshotBuffer::new(cfg);
        let mut tick: u64 = 1000;
        let mut now: f64 = 10.0;
        let mut newest: Option<u64> = None;
        for op in ops {
            match op {
                SOp::Push { dt } => {
                    tick = tick.saturating_add(dt);
                    b.push(tick, tick, now);
                    newest = Some(newest.map_or(tick, |n: u64| n.max(tick)));
                }
                SOp::PushAbs(t) => {
                    b.push(t, t, now);
                    newest = Some(newest.map_or(t, |n: u64| n.max(t)));
                }
                SOp::Update { dt } => {
                    now += dt;
                    if let Some(it) = b.update(now) {
                        prop_assert!(it.from_tick <= it.to_tick);
                        prop_assert_eq!(*it.from, it.from_tick);
                        prop_assert_eq!(*it.to, it.to_tick);
                        prop_assert!(it.alpha.is_finite() && it.alpha >= 0.0, "alpha {}", it.alpha);
                        let span = (it.to_tick - it.from_tick).max(1) as f32;
                        prop_assert!(it.alpha <= 1.0 + extrap as f32 / span + 1e-3, "alpha {} > limit", it.alpha);
                        prop_assert!(it.render_tick.is_finite());
                    }
                }
            }
            prop_assert!(b.len() <= cap);
            let d = b.delay_ticks();
            prop_assert!(d.is_finite() && d >= 0.0 && d <= max_delay + 1e-9, "delay {}", d);
            prop_assert_eq!(b.latest().map(|(t, _)| t), newest.filter(|_| !b.is_empty()));
        }
    }
}

// ------------------------------------------------------------------------- LagComp, clocks, lanes

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(1024), ..ProptestConfig::default() })]

    #[test]
    fn lag_comp_rewind_is_bounded(ticks in prop::collection::vec(prop_oneof![5 => 0u64..3, 1 => 3u64..100, 1 => Just(0u64)], 1..300),
                                  back in prop::collection::vec(any::<bool>(), 1..300),
                                  queries in prop::collection::vec(wild_f64(), 1..40),
                                  max_ms in 1u32..2000) {
        let mut h: LagCompHistory<Vec3> = LagCompHistory::new(max_ms, 120);
        let mut t: u64 = 500;
        for (i, d) in ticks.iter().enumerate() {
            if back.get(i).copied().unwrap_or(false) && t > 50 {
                t -= 50; // time goes backwards (map restart)
            } else {
                t += d;
            }
            h.record(t, Vec3::splat(t as f32));
            let (lo, hi) = (h.oldest_tick().unwrap(), h.newest_tick().unwrap());
            prop_assert!(lo <= hi);
            for &q in &queries {
                let r = h.rewind(q).unwrap();
                prop_assert!(r.alpha.is_finite() && (0.0..=1.0).contains(&r.alpha), "alpha {} for {}", r.alpha, q);
                prop_assert!(r.tick.is_finite() && r.tick >= lo as f64 && r.tick <= hi as f64, "tick {} for {}", r.tick, q);
                prop_assert!(h.sample(q).unwrap().is_finite());
            }
        }
    }

    #[test]
    fn clamp_view_tick_always_in_window(v in wild_f64(), now in any::<u64>(), max in wild_f64()) {
        let c = clamp_view_tick(v, now, max);
        prop_assert!(c.is_finite());
        prop_assert!(c <= now as f64);
    }

    #[test]
    fn clock_sync_stays_finite(samples in prop::collection::vec((wild_f64(), wild_f64(), 0.0f64..1e6), 0..64), q in 0.0f64..1e6) {
        let mut c = ClockSync::new(120);
        for (rtt, host_tick, recv) in samples {
            c.add_sample(rtt, host_tick, recv);
            prop_assert!(c.host_tick(q).is_finite(), "host_tick went non-finite");
            prop_assert!(c.rtt().srtt.is_finite() && c.rtt().srtt >= 0.0);
            prop_assert!(c.lead_ticks(2.0).is_finite());
        }
    }

    #[test]
    fn tick_pacer_is_bounded(steps in prop::collection::vec((wild_f64(), proptest::option::of(wild_f64())), 0..200), start in 0u64..1_000_000) {
        let mut p = TickPacer::new(120, start);
        for (dt, target) in steps {
            let before = p.tick();
            let st = p.advance(dt, target);
            prop_assert!(st.count <= p.max_ticks_per_frame);
            prop_assert!(p.alpha().is_finite() && (0.0..=1.0).contains(&p.alpha()), "alpha {}", p.alpha());
            if !st.snapped {
                prop_assert_eq!(st.first_tick, before);
                prop_assert_eq!(p.tick(), before.saturating_add(st.count as u64));
            }
        }
    }

    #[test]
    fn latest_lane_accounting(cap in 0usize..16, ops in prop::collection::vec(0u8..3, 0..200)) {
        let mut l = LatestLane::new(cap);
        let (mut pushed, mut taken) = (0u64, 0u64);
        for op in ops {
            match op {
                0 => { l.push(pushed); pushed += 1; }
                1 => { taken += l.drain().count() as u64; }
                _ => { taken += l.take_latest().is_some() as u64; }
            }
            prop_assert!(l.len() <= cap.max(1));
            prop_assert_eq!(pushed, taken + l.dropped + l.len() as u64);
        }
    }

    /// Whatever a player types or pastes into "join": never panics; accepted output is a ws(s) URL.
    #[test]
    fn address_parsing_never_panics(s in prop_oneof![any::<String>(), "[a-z0-9:/.\\[\\]-]{0,40}", "(wss?|https?)://[a-z.:0-9-]{0,20}"]) {
        if let Ok(url) = resolve_address(&s) {
            prop_assert!(url.starts_with("ws://") || url.starts_with("wss://"), "{}", url);
        }
        let _ = lobby_code_from_url(&s);
    }
}

#[test]
fn sequenced_repeat_clears_one_shots() {
    let i = inp(5, 1);
    let r = i.for_repeat();
    assert_eq!(r.reel_notches, 0);
    assert_eq!(r.seq(), 5);
}
