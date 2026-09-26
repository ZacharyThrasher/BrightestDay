//! Real Cloudflare quick tunnel (needs cloudflared + internet). Ignored by default:
//! `cargo test -p bd_net --test tunnel -- --ignored --nocapture`

mod common;

use std::time::{Duration, Instant};

use bd_net::{ClientConfig, ClientEvent, ClientSession, HostEvent, Tunnel, now_secs};
use common::*;

#[test]
#[ignore]
fn real_cloudflared_tunnel_rtt() {
    assert!(Tunnel::find_cloudflared().is_some(), "cloudflared not installed");
    let mut host = loopback_host(Some("Host"));
    let t0 = Instant::now();
    let tunnel = Tunnel::start(host.port());
    let (url, code) = tunnel.wait_ready(Duration::from_secs(60)).unwrap_or_else(|e| panic!("tunnel: {e}\n{}", tunnel.log_tail().join("\n")));
    let t_ready = t0.elapsed();
    eprintln!("tunnel ready after {:.1}s: {url} (code {code})", t_ready.as_secs_f32());

    let t1 = Instant::now();
    let mut clients = vec![ClientSession::connect(&code, ClientConfig { name: "remote".into(), connect_timeout: Duration::from_secs(45), ..Default::default() }).unwrap()];
    let mut hev = Vec::new();
    let mut cev = vec![Vec::new()];
    let ok = pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(50), |_, cs, _, cev| {
        cs[0].is_connected() || cev[0].iter().any(|e| matches!(e, ClientEvent::Disconnected(_)))
    });
    assert!(ok && clients[0].is_connected(), "client failed: {:?}\n{}", cev[0], tunnel.log_tail().join("\n"));
    let t_join = t1.elapsed();
    let me = clients[0].my_id().unwrap();
    eprintln!("client joined via lobby code after {:.1}s; host sees remote {:?}", t_join.as_secs_f32(), host.remote_addr(me));

    // 6 s: app-level echo at 20 Hz + 60 Hz 1 KB latest-lane snapshots (one-way latency is exact
    // because host and client share this process's clock).
    let mut echo_rtts = Vec::new();
    let mut one_way = Vec::new();
    let mut next_echo = 0.0;
    let mut next_snap = 0.0;
    let start = Instant::now();
    let mut tick = 0u64;
    while start.elapsed() < Duration::from_secs(6) {
        let t = start.elapsed().as_secs_f64();
        if t >= next_echo {
            next_echo += 0.05;
            clients[0].send(&C2S::Echo(now_secs()));
        }
        if t >= next_snap {
            next_snap += 1.0 / 60.0;
            tick += 2;
            host.set_tick(tick);
            host.send_latest(me, &S2C::Blob { tick, sent_at: now_secs(), data: vec![7; 1000] });
        }
        while let Some(e) = host.poll() {
            if let HostEvent::Message { from, msg: C2S::Echo(t) } = e {
                host.send(from, &S2C::Echo(t));
            }
        }
        while let Some(e) = clients[0].poll() {
            match e {
                ClientEvent::Message(S2C::Echo(t)) => echo_rtts.push((now_secs() - t) * 1000.0),
                ClientEvent::Disconnected(r) => panic!("dropped: {r}"),
                _ => {}
            }
        }
        let now = now_secs();
        for (_, m) in clients[0].drain_latest() {
            if let S2C::Blob { sent_at, .. } = m {
                one_way.push((now - sent_at) * 1000.0);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let ping = clients[0].clock().rtt();
    let n_echo = echo_rtts.len();
    let n_snap = one_way.len();
    eprintln!(
        "transport ping (net thread): srtt {:.1} ms, min {:.1} ms, jitter {:.1} ms over {} pings",
        ping.srtt * 1e3,
        ping.min * 1e3,
        ping.rttvar * 1e3,
        ping.count
    );
    eprintln!(
        "app echo RTT over {n_echo}: p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms",
        percentile(&mut echo_rtts, 0.5),
        percentile(&mut echo_rtts, 0.95),
        percentile(&mut echo_rtts, 1.0)
    );
    eprintln!(
        "host→client 1 KB snapshots: {n_snap}/{} received, one-way p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms",
        tick / 2,
        percentile(&mut one_way, 0.5),
        percentile(&mut one_way, 0.95),
        percentile(&mut one_way, 1.0)
    );
    eprintln!("host clock estimate error: {:.2} ticks (host tick only set at 60 Hz here)", clients[0].host_tick_now() - tick as f64);
    assert!(n_echo > 100);
    assert!(ping.srtt < 0.5);
    drop(clients);
    host.shutdown();
    drop(tunnel);
}
