//! End-to-end netcode under lag: clock sync + tick pacing, redundant inputs → host jitter queue,
//! prediction + reconciliation with unpredictable host knockbacks, and interpolation of a host-driven
//! bot.
//!
//! The main test is **deterministic**: a virtual clock and a simulated order-preserving channel
//! (40 ms + 0..15 ms jitter each way, like `LagProxy`) replace wall-clock sockets, so the result never
//! depends on machine load. It runs several seeds with jittery frame times and asserts tight bounds.
//! Messages still go through the real wire codec. A small real-socket smoke test (generous
//! tolerances) keeps the transport + `LagProxy` path covered.

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use bd_core::input::PlayerInput;
use bd_core::{DT, PlayerId, TICK_HZ};
use bd_net::proxy::{LagProxy, LagSettings};
use bd_net::{
    ClientConfig, ClientEvent, ClientSession, ClockSync, HostConfig, HostEvent, HostSession, InputBuffer, InputPacket, InputQueue,
    InterpConfig, Reconcilable, ReconcileOutcome, Reconciler, SnapshotBuffer, TickPacer, codec,
};
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct P {
    pos: Vec3,
    vel: Vec3,
}

impl Reconcilable for P {
    fn diverges_from(&self, a: &Self) -> bool {
        (self.pos - a.pos).length() > 1e-4 || (self.vel - a.vel).length() > 1e-4
    }
    fn render_position(&self) -> Vec3 {
        self.pos
    }
}

/// Deterministic movement shared by host and client.
fn step(s: &mut P, i: &PlayerInput) {
    let want = i.move_x as f32 * 8.0;
    s.vel.x += (want - s.vel.x) * 0.2;
    s.vel.y *= 0.98;
    s.pos += s.vel * DT;
}

fn input_for(tick: u64) -> PlayerInput {
    let phase = (tick / 45) % 3;
    PlayerInput { move_x: phase as i8 - 1, ..Default::default() }
}

fn bot(tick: f64) -> Vec3 {
    let t = tick / TICK_HZ as f64;
    Vec3::new((t.cos() * 5.0) as f32, 0.0, (t.sin() * 5.0) as f32)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Up {
    Input(InputPacket<PlayerInput>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snap {
    tick: u64,
    ack: u32,
    me: P,
    bot: Vec3,
}

// ------------------------------------------------------------------------------ simulated network

/// xorshift64*: deterministic jitter.
struct Rng(u64);
impl Rng {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// One direction of a TCP-like path: `latency + U(0, jitter)` delay, order preserved. Payloads are
/// encoded with the real wire codec (what goes in must survive bincode).
struct Pipe {
    queue: VecDeque<(f64, Vec<u8>)>,
    last: f64,
    latency: f64,
    jitter: f64,
}

impl Pipe {
    fn new(latency_ms: f64, jitter_ms: f64) -> Self {
        Self { queue: VecDeque::new(), last: 0.0, latency: latency_ms / 1000.0, jitter: jitter_ms / 1000.0 }
    }
    fn send<T: Serialize>(&mut self, now: f64, msg: &T, rng: &mut Rng) {
        let at = (now + self.latency + rng.unit() * self.jitter).max(self.last);
        self.last = at;
        self.queue.push_back((at, codec::encode(msg)));
    }
    /// Everything that has arrived by `now`, with its arrival time.
    fn recv<T: serde::de::DeserializeOwned>(&mut self, now: f64) -> Vec<(f64, T)> {
        let mut out = Vec::new();
        while self.queue.front().is_some_and(|(at, _)| *at <= now) {
            let Some((at, bytes)) = self.queue.pop_front() else { break };
            out.push((at, codec::decode(&bytes).expect("wire roundtrip")));
        }
        out
    }
}

#[derive(Debug)]
struct Report {
    srtt: f64,
    clock_err: f64,
    starved_mid: u64,
    knockbacks: u32,
    corrections_after_warmup: u32,
    max_interp_err: f32,
    max_frame_step_dev: f64,
    interp_snaps: u64,
    interp_starved: u64,
}

/// One full session on a virtual clock. Host and client share a "frame" loop whose frame times
/// jitter around 144 fps (like the real test's 1 ms sleep loop); the network is simulated.
fn run_session(seed: u64) -> Report {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let (mut up, mut down) = (Pipe::new(40.0, 15.0), Pipe::new(40.0, 15.0));
    let mut ping_up = Pipe::new(40.0, 15.0);
    let mut ping_down = Pipe::new(40.0, 15.0);
    // (Pings travel on their own simulated pipes; on the real stream they share ordering with data,
    // which only matters for head-of-line blocking the simulation doesn't need.)
    let hz = TICK_HZ as f64;

    // Host.
    let t0 = 0.0;
    let start_tick: u64 = 10_000;
    let mut h_tick = start_tick;
    let mut h_acc = 0.0f64;
    let mut h_player = P { pos: Vec3::ZERO, vel: Vec3::ZERO };
    let mut h_queue: InputQueue<PlayerInput> = InputQueue::new(2);
    let mut h_ack = 0u32;
    let mut knockbacks = 0u32;
    let mut last_sent: Option<(u32, P)> = None;
    let host_tick_at = |t: f64| start_tick as f64 + (t - t0) * hz;

    // Client.
    let mut clock = ClockSync::new(TICK_HZ);
    let mut inputs: InputBuffer<PlayerInput> = InputBuffer::new(4);
    let mut rec: Reconciler<P, PlayerInput> = Reconciler::new(h_player.clone());
    let mut pacer: Option<TickPacer> = None;
    let mut interp: SnapshotBuffer<Vec3> = SnapshotBuffer::new(InterpConfig::default());
    let mut send_acc = 0.0f64;
    let mut ping_acc = 1.0f64;
    let mut corrections_after_warmup = 0u32;
    let mut max_interp_err = 0.0f32;
    let mut max_frame_step_dev = 0.0f64;
    let mut prev_render: Option<(f64, f64)> = None;
    let (mut snap_count_at_warmup, mut starved_at_warmup, mut starved_measured) = (0, 0, 0);

    let run_secs = 6.0;
    let warmup = 1.5;
    let mut stop_inputs = false;
    let (mut q_starved_at_warmup, mut q_starved_at_stop) = (0, 0);
    let mut now = t0;
    loop {
        let dt = (1.0 / 144.0) * (0.6 + 0.8 * rng.unit());
        now += dt;
        let t = now - t0;
        if t > run_secs && !stop_inputs {
            stop_inputs = true;
            q_starved_at_stop = h_queue.stats.starved;
        }
        if t > run_secs + 1.0 {
            break;
        }

        // ---------------- network thread: pings answered on arrival ----------------
        for (arrive, sent_at) in ping_up.recv::<f64>(now) {
            let reply = (sent_at, host_tick_at(arrive));
            // Reply leaves the host at `arrive`.
            ping_down.send(arrive, &reply, &mut rng);
        }
        for (arrive, (sent_at, host_tick)) in ping_down.recv::<(f64, f64)>(now) {
            clock.add_sample(arrive - sent_at, host_tick, arrive);
        }

        // ---------------- host ----------------
        for (_, Up::Input(p)) in up.recv::<Up>(now) {
            h_queue.receive(p);
        }
        h_acc += dt * hz;
        while h_acc >= 1.0 {
            h_acc -= 1.0;
            h_tick += 1;
            if let Some(q) = h_queue.next() {
                step(&mut h_player, &q.input);
                h_ack = q.seq;
            }
            // Unpredictable knockback every 1.5 s (after warmup), between inputs.
            if t > warmup && h_tick % 180 == 0 && !stop_inputs {
                h_player.vel.y += 6.0;
                knockbacks += 1;
            }
            if h_tick % 2 == 0 && t < run_secs + 0.6 {
                let snap = Snap { tick: h_tick, ack: h_ack, me: h_player.clone(), bot: bot(h_tick as f64) };
                down.send(now, &snap, &mut rng);
                last_sent = Some((h_ack, h_player.clone()));
            }
        }

        // ---------------- client ----------------
        ping_acc += dt;
        if ping_acc >= 0.1 {
            ping_acc = 0.0;
            ping_up.send(now, &now, &mut rng);
        }
        for (_, s) in down.recv::<Snap>(now) {
            inputs.ack(s.ack);
            let o = rec.reconcile(s.ack, s.me.clone(), step);
            if t > warmup && !stop_inputs && matches!(o, ReconcileOutcome::Corrected { .. }) {
                corrections_after_warmup += 1;
            }
            interp.push(s.tick, s.bot, now);
        }
        if !clock.is_synced() || clock.rtt().count < 3 {
            continue;
        }
        let target = clock.target_client_tick(now, 3.0);
        let p = pacer.get_or_insert_with(|| TickPacer::new(TICK_HZ, target as u64));
        if !stop_inputs {
            let st = p.advance(dt, Some(target));
            for k in 0..st.count as u64 {
                let input = input_for(st.first_tick + k);
                let seq = inputs.push(input);
                rec.predict(seq, input, step);
            }
        }
        send_acc += dt;
        if send_acc >= 1.0 / 60.0 {
            send_acc = 0.0;
            if let Some(pkt) = inputs.packet(interp.render_tick()) {
                up.send(now, &Up::Input(pkt), &mut rng);
            }
        }
        rec.update_smoothing(dt as f32);
        if let Some(it) = interp.update(now) {
            let shown = it.from.lerp(*it.to, it.alpha);
            let truth = bot(it.render_tick);
            if t > warmup && t < run_secs + 0.5 {
                max_interp_err = max_interp_err.max((shown - truth).length());
                if let Some((prt, pnow)) = prev_render {
                    let expected = (now - pnow) * hz;
                    if expected > 0.5 {
                        max_frame_step_dev = max_frame_step_dev.max((it.render_tick - prt - expected).abs() / expected);
                    }
                }
            }
            prev_render = Some((it.render_tick, now));
        }
        if t > warmup && t < run_secs + 0.5 {
            starved_measured = interp.starved_frames - starved_at_warmup;
        }
        if t <= warmup {
            snap_count_at_warmup = interp.snaps_count;
            starved_at_warmup = interp.starved_frames;
            q_starved_at_warmup = h_queue.stats.starved;
        }
    }

    // Converged: every input acked and prediction identical to the last state the host sent.
    assert_eq!(h_ack, inputs.next_seq() - 1, "seed {seed}: all inputs consumed");
    let (_, last_state) = last_sent.expect("snapshots sent");
    assert!(!rec.current().diverges_from(&last_state), "seed {seed}: predicted {:?} host {:?}", rec.current(), last_state);

    Report {
        srtt: clock.rtt().srtt,
        clock_err: clock.host_tick(now) - host_tick_at(now),
        starved_mid: q_starved_at_stop - q_starved_at_warmup,
        knockbacks,
        corrections_after_warmup,
        max_interp_err,
        max_frame_step_dev,
        interp_snaps: interp.snaps_count - snap_count_at_warmup,
        interp_starved: starved_measured,
    }
}

#[test]
fn prediction_reconciliation_and_interpolation_under_lag() {
    for seed in 1..=8u64 {
        let r = run_session(seed);
        eprintln!("seed {seed}: {r:?}");
        // RTT = 2 × (40 + U(0,15)) ms ≈ 80..110 ms.
        assert!(r.srtt > 0.078 && r.srtt < 0.115, "seed {seed}: srtt {}", r.srtt);
        assert!(r.clock_err.abs() < 2.0, "seed {seed}: host tick estimate off by {}", r.clock_err);
        // Inputs arrive in time: after start-up the host essentially never starves.
        assert!(r.starved_mid <= 6, "seed {seed}: host starved {} ticks", r.starved_mid);
        // Each knockback forces one correction; each starved tick at most ~two; nothing else.
        assert!(r.knockbacks >= 2);
        let budget = r.knockbacks as u64 + 2 * r.starved_mid + 2;
        assert!(r.corrections_after_warmup as u64 <= budget, "seed {seed}: {r:?}");
        // Interpolation: accurate and smooth, no snaps, (almost) no starvation after warmup.
        assert!(r.max_interp_err < 0.02, "seed {seed}: interp error {}", r.max_interp_err);
        assert!(r.max_frame_step_dev < 0.12, "seed {seed}: render clock rate deviation {}", r.max_frame_step_dev);
        assert_eq!(r.interp_snaps, 0, "seed {seed}");
        assert!(r.interp_starved < 5, "seed {seed}: starved {}", r.interp_starved);
    }
}

#[test]
fn simulated_session_is_deterministic() {
    let a = format!("{:?}", run_session(42));
    let b = format!("{:?}", run_session(42));
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------- real-socket smoke

/// The real transport through `LagProxy`: connect, inputs flow host-ward in order, snapshots flow
/// back, RTT reflects the injected latency. Tolerances are deliberately generous (wall clock).
#[test]
fn real_sockets_through_lag_proxy_smoke() {
    let mut host: HostSession<Up, Snap> =
        HostSession::start(HostConfig { bind: IpAddr::V4(Ipv4Addr::LOCALHOST), port: 0, ..Default::default() }).expect("host");
    let proxy =
        LagProxy::start(format!("127.0.0.1:{}", host.port()).parse().expect("addr"), LagSettings { latency_ms: 40.0, jitter_ms: 15.0 })
            .expect("proxy");
    let mut client: ClientSession<Up, Snap> = ClientSession::connect(&proxy.url(), ClientConfig::default()).expect("client");
    let mut me: Option<PlayerId> = None;
    let mut inputs: InputBuffer<PlayerInput> = InputBuffer::new(4);
    let mut queue: InputQueue<PlayerInput> = InputQueue::new(2);
    let (mut snaps, mut tick) = (0u32, 0u64);
    let start = Instant::now();
    let mut last_send = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(2));
        while let Some(e) = host.poll() {
            match e {
                HostEvent::PlayerJoined(p) => me = Some(p.id),
                HostEvent::Message { msg: Up::Input(p), .. } => queue.receive(p),
                HostEvent::PlayerLeft { .. } => panic!("client left"),
            }
        }
        while let Some(q) = queue.next() {
            if q.repeated {
                break;
            }
            tick = q.seq as u64;
        }
        if let Some(id) = me {
            host.send_latest(id, &Snap { tick, ack: queue.last_consumed(), me: P { pos: Vec3::ZERO, vel: Vec3::ZERO }, bot: Vec3::ZERO });
        }
        while let Some(e) = client.poll() {
            if let ClientEvent::Disconnected(r) = e {
                panic!("disconnected: {r}");
            }
        }
        for (_, s) in client.drain_latest() {
            inputs.ack(s.ack);
            snaps += 1;
        }
        if client.is_connected() && last_send.elapsed() >= Duration::from_millis(16) {
            last_send = Instant::now();
            for _ in 0..2 {
                inputs.push(PlayerInput::default());
            }
            if let Some(p) = inputs.packet(0.0) {
                client.send(&Up::Input(p));
            }
        }
        if snaps > 30 && queue.last_consumed() > 100 && client.clock().rtt().count >= 5 {
            break;
        }
    }
    let rtt = client.clock().rtt().srtt;
    assert!(snaps > 30, "snapshots received: {snaps}");
    assert!(queue.last_consumed() > 100, "inputs consumed: {}", queue.last_consumed());
    assert_eq!(queue.stats.skipped, 0, "inputs arrive in order without gaps");
    // Injected ≥ 80 ms round trip; allow a lot of slack for a loaded machine.
    assert!(rtt > 0.07 && rtt < 1.0, "srtt {rtt}");
}
