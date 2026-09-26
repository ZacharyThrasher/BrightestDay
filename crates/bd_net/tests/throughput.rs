//! 8 clients × 60 Hz input packets + host → each client 60 Hz ~2 KB snapshots, over loopback.
//! Everything (host + 8 clients) is polled from this one thread, like a worst-case frame loop.

mod common;

use std::time::{Duration, Instant};

use bd_core::input::PlayerInput;
use bd_net::{ClientEvent, HostEvent, InputBuffer, InputQueue, now_secs};
use common::*;

#[test]
fn eight_clients_60hz_inputs_and_2kb_snapshots() {
    let mut host = loopback_host(None);
    let addr = format!("127.0.0.1:{}", host.port());
    let mut clients: Vec<Client> = (0..8).map(|i| client(&addr, &format!("p{i}"))).collect();
    let mut hev = Vec::new();
    let mut cev: Vec<Vec<_>> = vec![Vec::new(); 8];
    assert!(pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(10), |h, cs, _, _| {
        h.players().len() == 8 && cs.iter().all(|c| c.is_connected())
    }));

    let mut bufs: Vec<InputBuffer<PlayerInput>> = (0..8).map(|_| InputBuffer::new(4)).collect();
    let mut queues: Vec<InputQueue<PlayerInput>> = (0..8).map(|_| InputQueue::new(2)).collect();
    let mut snaps_rx = vec![0u32; 8];
    let mut lat: Vec<f64> = Vec::new();
    let blob = vec![0xA5u8; 2000];
    let secs = 3.0;
    let start = Instant::now();
    let mut next_send = 0.0f64;
    let mut sends = 0u32;
    let mut tick = 0u64;
    let (h_sent0, h_recv0) = (host.bytes_sent(), host.bytes_received());
    while start.elapsed().as_secs_f64() < secs + 0.3 {
        let t = start.elapsed().as_secs_f64();
        if t >= next_send && t < secs {
            next_send += 1.0 / 60.0;
            sends += 1;
            tick += 2;
            host.set_tick(tick);
            let now = now_secs();
            for p in host.players().to_vec() {
                host.send_latest(p.id, &S2C::Blob { tick, sent_at: now, data: blob.clone() });
            }
            for (c, b) in clients.iter_mut().zip(bufs.iter_mut()) {
                b.push(PlayerInput { move_z: 1, yaw: t as f32, ..Default::default() });
                b.push(PlayerInput { move_z: 1, yaw: t as f32, ..Default::default() });
                let pkt = b.packet(tick as f64).unwrap();
                c.send(&C2S::Input(pkt));
            }
        }
        while let Some(e) = host.poll() {
            match e {
                HostEvent::Message { from, msg: C2S::Input(pkt) } => {
                    let q = &mut queues[from.0 as usize];
                    q.receive(pkt);
                    while q.depth() > 0 && q.next().is_some() {}
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        for (i, c) in clients.iter_mut().enumerate() {
            while let Some(e) = c.poll() {
                assert!(!matches!(e, ClientEvent::Disconnected(_)), "{e:?}");
            }
            let now = now_secs();
            for (_, m) in c.drain_latest() {
                if let S2C::Blob { sent_at, data, .. } = m {
                    assert_eq!(data.len(), 2000);
                    lat.push((now - sent_at) * 1000.0);
                    snaps_rx[i] += 1;
                }
            }
        }
        std::thread::sleep(Duration::from_micros(300));
    }
    let h_sent = (host.bytes_sent() - h_sent0) as f64 / secs;
    let h_recv = (host.bytes_received() - h_recv0) as f64 / secs;
    let mean = lat.iter().sum::<f64>() / lat.len() as f64;
    let p50 = percentile(&mut lat, 0.5);
    let p99 = percentile(&mut lat, 0.99);
    let max = percentile(&mut lat, 1.0);
    eprintln!("snapshot sends per client: {sends}, received: {snaps_rx:?}");
    eprintln!("snapshot latency ms: mean {mean:.3} p50 {p50:.3} p99 {p99:.3} max {max:.3}");
    eprintln!("host up {:.1} KB/s ({:.2} Mbit/s), host down {:.1} KB/s", h_sent / 1024.0, h_sent * 8.0 / 1e6, h_recv / 1024.0);
    for (i, q) in queues.iter().enumerate() {
        assert!(q.stats.received as u32 >= sends * 2 - 4, "client {i} inputs {:?}", q.stats);
    }
    for (i, n) in snaps_rx.iter().enumerate() {
        assert!(*n as f64 >= sends as f64 * 0.95, "client {i} got {n}/{sends} snapshots");
    }
    // Generous bounds: this machine may be busy compiling (typical: p50 ~0.8 ms, p99 ~3 ms).
    assert!(p50 < 15.0, "p50 latency {p50} ms");
    assert!(p99 < 60.0, "p99 latency {p99} ms");
}
