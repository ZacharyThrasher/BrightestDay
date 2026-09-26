//! End-to-end netcode through an in-process lag proxy (40 ms ± 15 ms each way, order preserved):
//! clock sync + tick pacing, redundant inputs → host jitter queue, prediction + reconciliation with
//! unpredictable host knockbacks, and interpolation of a host-driven bot.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use bd_core::input::PlayerInput;
use bd_core::{DT, PlayerId, TICK_HZ};
use bd_net::proxy::{LagProxy, LagSettings};
use bd_net::{
    ClientConfig, ClientEvent, ClientSession, HostConfig, HostEvent, HostSession, InputBuffer, InputPacket, InputQueue, InterpConfig,
    Reconcilable, ReconcileOutcome, Reconciler, SnapshotBuffer, TickPacer, now_secs,
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

#[test]
fn prediction_reconciliation_and_interpolation_under_lag() {
    let mut host: HostSession<Up, Snap> = HostSession::start(HostConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        ..Default::default()
    })
    .unwrap();
    let proxy = LagProxy::start(format!("127.0.0.1:{}", host.port()).parse().unwrap(), LagSettings { latency_ms: 40.0, jitter_ms: 15.0 }).unwrap();
    let mut client: ClientSession<Up, Snap> = ClientSession::connect(&proxy.url(), ClientConfig::default()).unwrap();

    // Host state.
    let mut h_tick: u64 = 10_000;
    let mut h_acc = 0.0f64;
    let mut h_player = P { pos: Vec3::ZERO, vel: Vec3::ZERO };
    let mut h_queue: InputQueue<PlayerInput> = InputQueue::new(2);
    let mut h_ack = 0u32;
    let mut me_id: Option<PlayerId> = None;
    let mut knockbacks = 0u32;
    let mut last_sent: Option<(u32, P)> = None;

    // Client state.
    let mut inputs: InputBuffer<PlayerInput> = InputBuffer::new(4);
    let mut rec: Reconciler<P, PlayerInput> = Reconciler::new(h_player.clone());
    let mut pacer: Option<TickPacer> = None;
    let mut interp: SnapshotBuffer<Vec3> = SnapshotBuffer::new(InterpConfig::default());
    let mut send_acc = 0.0f64;
    let mut corrections_after_warmup = 0u32;
    let mut max_interp_err = 0.0f32;
    let mut max_frame_step_dev = 0.0f64;
    let mut prev_render: Option<(f64, f64)> = None; // (render_tick, now)
    let mut snap_count_at_warmup = 0;
    let mut starved_at_warmup = 0;
    let mut max_offset = 0.0f32;
    let mut starved_measured = 0;

    let start = now_secs();
    let mut last = start;
    let run_secs = 6.0;
    let warmup = 1.5;
    let mut stop_inputs = false;
    let mut last_client_tick = 0u64;
    let mut q_starved_at_warmup = 0;
    let mut q_starved_at_stop = 0;
    loop {
        std::thread::sleep(Duration::from_millis(1));
        let now = now_secs();
        let dt = now - last;
        last = now;
        let t = now - start;
        if t > run_secs && !stop_inputs {
            stop_inputs = true;
            q_starved_at_stop = h_queue.stats.starved;
        }
        if t > run_secs + 1.0 {
            break;
        }

        // ---------------- host ----------------
        while let Some(e) = host.poll() {
            match e {
                HostEvent::PlayerJoined(p) => me_id = Some(p.id),
                HostEvent::Message { msg: Up::Input(p), .. } => h_queue.receive(p),
                HostEvent::PlayerLeft { .. } => panic!("client left"),
            }
        }
        h_acc += dt * TICK_HZ as f64;
        while h_acc >= 1.0 {
            h_acc -= 1.0;
            h_tick += 1;
            if let Some(q) = h_queue.next() {
                step(&mut h_player, &q.input);
                h_ack = q.seq;
            }
            // Unpredictable knockback every 1.5 s (after warmup).
            if t > warmup && h_tick % 180 == 0 && !stop_inputs {
                // (the knockback lands between inputs, so the client can never predict it)
                h_player.vel.y += 6.0;
                knockbacks += 1;
            }
            host.set_tick(h_tick);
            if h_tick % 2 == 0 && t < run_secs + 0.6 {
                if let Some(id) = me_id {
                    host.send_latest(id, &Snap { tick: h_tick, ack: h_ack, me: h_player.clone(), bot: bot(h_tick as f64) });
                    last_sent = Some((h_ack, h_player.clone()));
                }
            }
        }

        // ---------------- client ----------------
        while let Some(e) = client.poll() {
            if let ClientEvent::Disconnected(r) = e {
                panic!("disconnected: {r}");
            }
        }
        for (recv, s) in client.drain_latest() {
            inputs.ack(s.ack);
            let o = rec.reconcile(s.ack, s.me.clone(), step);
            if t > warmup && !stop_inputs && matches!(o, ReconcileOutcome::Corrected { .. }) {
                corrections_after_warmup += 1;
                if std::env::var("LAG_DEBUG").is_ok() {
                    eprintln!("t={t:.3} ack={} {o:?} starved={} depth={} pending={}", s.ack, h_queue.stats.starved, h_queue.depth(), rec.pending());
                }
            }
            interp.push(s.tick, s.bot, recv);
        }
        if !client.clock().is_synced() || client.clock().rtt().count < 3 {
            continue;
        }
        let target = client.clock().target_client_tick(now, 3.0);
        let p = pacer.get_or_insert_with(|| TickPacer::new(TICK_HZ, target as u64));
        if !stop_inputs {
            let st = p.advance(dt, Some(target));
            for k in 0..st.count as u64 {
                let tick = st.first_tick + k;
                last_client_tick = tick;
                let input = input_for(tick);
                let seq = inputs.push(input);
                rec.predict(seq, input, step);
            }
        }
        send_acc += dt;
        if send_acc >= 1.0 / 60.0 {
            send_acc = 0.0;
            if let Some(pkt) = inputs.packet(interp.render_tick()) {
                client.send(&Up::Input(pkt));
            }
        }
        rec.update_smoothing(dt as f32);
        max_offset = max_offset.max(rec.visual_offset().length());
        if let Some(it) = interp.update(now) {
            let shown = it.from.lerp(*it.to, it.alpha);
            let truth = bot(it.render_tick);
            if t > warmup && t < run_secs + 0.5 {
                max_interp_err = max_interp_err.max((shown - truth).length());
                if let Some((prt, pnow)) = prev_render {
                    let expected = (now - pnow) * TICK_HZ as f64;
                    let adv = it.render_tick - prt;
                    if expected > 0.5 {
                        max_frame_step_dev = max_frame_step_dev.max((adv - expected).abs() / expected);
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

    let rtt = client.clock().rtt();
    let lead = client.clock().lead_ticks(3.0);
    let host_now = h_tick as f64;
    let est = client.clock().host_tick(now_secs());
    eprintln!("rtt srtt {:.1} ms (min {:.1}, var {:.1}); lead {:.1} ticks; host tick est err {:.2} ticks", rtt.srtt * 1e3, rtt.min * 1e3, rtt.rttvar * 1e3, lead, est - host_now);
    eprintln!("host queue {:?}", h_queue.stats);
    eprintln!("knockbacks {knockbacks}, corrections after warmup {corrections_after_warmup}, total {}, max visual offset {max_offset:.3} m", rec.corrections);
    eprintln!(
        "interp delay {:.2} ticks, jitter {:.2}, interval {:.2}; max err {max_interp_err:.4} m; max render-rate deviation {:.1}%; snaps {} starved {}",
        interp.delay_ticks(),
        interp.jitter_ticks(),
        interp.interval_ticks(),
        max_frame_step_dev * 100.0,
        interp.snaps_count - snap_count_at_warmup,
        starved_measured
    );

    // Clock sync is accurate to a couple of ticks even with jitter.
    assert!(rtt.srtt > 0.075 && rtt.srtt < 0.16, "srtt {}", rtt.srtt);
    assert!((est - host_now).abs() < 3.0, "host tick estimate off by {}", est - host_now);
    // Inputs arrive in time: after the start-up the host essentially never starves.
    let starved_mid = q_starved_at_stop - q_starved_at_warmup;
    eprintln!("host queue starved {q_starved_at_warmup} during warmup, {starved_mid} after; depth now {}", h_queue.depth());
    assert!(starved_mid <= 6, "{:?}", h_queue.stats);
    // Each knockback causes one correction; nothing else mispredicts once warmed up.
    assert!(knockbacks >= 2);
    // Each knockback forces one correction, each host starvation (jitter spike) at most ~one per
    // snapshot while it lasts; nothing else may mispredict.
    let budget = knockbacks as u64 + 2 * starved_mid + 2;
    assert!(corrections_after_warmup as u64 <= budget, "corrections {corrections_after_warmup} for {knockbacks} knockbacks, {starved_mid} starved ticks");
    // Converged: every input acked and prediction identical to the host.
    assert_eq!(h_ack, inputs.next_seq() - 1, "all inputs consumed");
    // (After inputs stop the host keeps simulating the idle player; the client has re-based onto
    // the last snapshot it received, which must be the last one sent.)
    let (_, last_state) = last_sent.unwrap();
    assert!(!rec.current().diverges_from(&last_state), "predicted {:?} host {:?}", rec.current(), last_state);
    // Interpolation: accurate and smooth, no snaps or starvation after warmup.
    assert!(max_interp_err < 0.02, "interp error {max_interp_err}");
    assert!(max_frame_step_dev < 0.12, "render clock rate deviation {max_frame_step_dev}");
    assert_eq!(interp.snaps_count - snap_count_at_warmup, 0);
    assert!(starved_measured < 10);
    let _ = last_client_tick;
}
