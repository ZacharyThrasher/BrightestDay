//! Hostile-peer integration test (see docs/TESTING.md): while two well-behaved clients play, a set
//! of attackers throws garbage at the host over real loopback sockets — raw TCP junk, WebSocket
//! garbage before the handshake, bad magic/protocol, giant names, malformed/NaN pings, unknown frame
//! kinds, random bytes after a valid handshake, and a pile of connections that never handshake.
//!
//! Required: the host never panics, every attacker is rejected or kicked, and the good clients stay
//! connected and can still talk to the host and each other.

mod common;

use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;

use bd_core::PlayerId;
use bd_core::corps::Corps;
use bd_net::transport::{K_DATA, K_PING};
use bd_net::{ClientEvent, HostConfig, HostEvent, HostSession, LeaveReason, PROTOCOL_MAGIC, PROTOCOL_VERSION, PlayerInfo, codec};
use common::*;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

/// Same serde shape as the private session envelopes ⇒ same bytes on the wire.
#[derive(Serialize, Deserialize)]
enum ClientWire<C> {
    Hello { magic: u32, protocol: u32, name: String, corps: Corps },
    App(C),
    Bye,
}

#[derive(Serialize, Deserialize, Debug)]
enum HostWire<S> {
    Welcome { you: PlayerId, players: Vec<PlayerInfo>, tick_hz: u32 },
    Reject(String),
    Joined(PlayerInfo),
    Left(PlayerId, LeaveReason),
    Kicked(String),
    App(S),
    Latest(S),
}

fn data(msg: &impl Serialize) -> Message {
    let mut v = vec![K_DATA];
    v.extend_from_slice(&codec::encode(msg));
    Message::Binary(v.into())
}

fn hello(magic: u32, protocol: u32, name: &str) -> Message {
    data(&ClientWire::<C2S>::Hello { magic, protocol, name: name.into(), corps: Corps::Rage })
}

/// Tiny deterministic RNG (xorshift) so failures reproduce.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Read until the host closes us (or a timeout). Returns every decodable host message plus how the
/// stream ended.
async fn drain(ws: &mut Ws, secs: u64) -> (Vec<HostWire<S2C>>, &'static str) {
    let mut got = Vec::new();
    let end = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        match tokio::time::timeout_at(end, ws.next()).await {
            Err(_) => return (got, "timeout"),
            Ok(None) => return (got, "closed"),
            Ok(Some(Err(_))) => return (got, "error"),
            Ok(Some(Ok(Message::Close(_)))) => return (got, "closed"),
            Ok(Some(Ok(Message::Binary(b)))) => {
                if b.first() == Some(&K_DATA) {
                    if let Ok(m) = codec::decode::<HostWire<S2C>>(&b[1..]) {
                        got.push(m);
                    }
                }
            }
            Ok(Some(Ok(_))) => {}
        }
    }
}

async fn connect(url: &str) -> Ws {
    tokio_tungstenite::connect_async(url).await.expect("attacker ws connect").0
}

/// Every attack, run sequentially on its own runtime. Returns a log of outcomes for assertions.
async fn attacks(port: u16, seed: u64) -> Vec<(String, Vec<HostWire<S2C>>, &'static str)> {
    let url = format!("ws://127.0.0.1:{port}");
    let mut rng = Rng(seed | 1);
    let mut log = Vec::new();

    // 1. WebSocket garbage before the handshake (random, empty, text, bogus kinds).
    for i in 0..6 {
        let mut ws = connect(&url).await;
        let mut junk = vec![K_DATA];
        let n = 1 + (rng.next() % 300) as usize;
        junk.extend(rng.bytes(n));
        let _ = ws.send(Message::Binary(Vec::new().into())).await;
        let _ = ws.send(Message::Text("GET / HTTP/1.1".into())).await;
        let _ = ws.send(Message::Binary(vec![9, 9, 9].into())).await;
        let _ = ws.send(Message::Binary(vec![K_PING].into())).await;
        let _ = ws.send(Message::Binary(junk.into())).await;
        let (got, how) = drain(&mut ws, 5).await;
        log.push((format!("pre-handshake garbage #{i}"), got, how));
    }

    // 2. Wrong magic / wrong protocol / absurd name.
    for (label, msg) in
        [("bad magic", hello(0xDEAD_BEEF, PROTOCOL_VERSION, "x")), ("bad protocol", hello(PROTOCOL_MAGIC, PROTOCOL_VERSION + 99, "x"))]
    {
        let mut ws = connect(&url).await;
        let _ = ws.send(msg).await;
        let (got, how) = drain(&mut ws, 5).await;
        log.push((label.to_string(), got, how));
    }

    // 3. Valid handshake with a hostile name, then malformed pings, an inf-RTT ping, a marker
    //    message, then garbage ⇒ must be kicked.
    {
        let mut ws = connect(&url).await;
        let name = format!("{}\u{0}\u{7}\n{}", "A".repeat(30_000), "\u{202e}evil");
        let _ = ws.send(hello(PROTOCOL_MAGIC, PROTOCOL_VERSION, &name)).await;
        // Short ping, NaN ping, inf-RTT ping, unknown kind, empty frame.
        let mut nan_ping = vec![K_PING];
        nan_ping.extend_from_slice(&f64::NAN.to_le_bytes());
        nan_ping.extend_from_slice(&f32::NAN.to_le_bytes());
        let mut inf_ping = vec![K_PING];
        inf_ping.extend_from_slice(&0f64.to_le_bytes());
        inf_ping.extend_from_slice(&f32::INFINITY.to_le_bytes());
        for f in [vec![K_PING, 1, 2], nan_ping, inf_ping, vec![200, 1], vec![]] {
            let _ = ws.send(Message::Binary(f.into())).await;
        }
        // Marker: a valid app message, so the host test can check RTT sanitisation at this point.
        let _ = ws.send(data(&ClientWire::App(C2S::Echo(4242.0)))).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        // A second Hello after the handshake must be ignored; then garbage with an invalid variant.
        let _ = ws.send(hello(PROTOCOL_MAGIC, PROTOCOL_VERSION, "again")).await;
        let mut garbage = vec![K_DATA, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
        garbage.extend(rng.bytes(64));
        let _ = ws.send(Message::Binary(garbage.into())).await;
        let (got, how) = drain(&mut ws, 5).await;
        log.push(("handshake then garbage".to_string(), got, how));
    }

    // 4. Valid handshakes followed by purely random data frames (whatever they decode to, the host
    //    must survive; most will be protocol errors ⇒ kick).
    for i in 0..4 {
        let mut ws = connect(&url).await;
        let _ = ws.send(hello(PROTOCOL_MAGIC, PROTOCOL_VERSION, &format!("rand{i}"))).await;
        for _ in 0..20 {
            let mut f = vec![K_DATA];
            let n = (rng.next() % 64) as usize;
            f.extend(rng.bytes(n));
            if ws.send(Message::Binary(f.into())).await.is_err() {
                break;
            }
        }
        let (got, how) = drain(&mut ws, 3).await;
        log.push((format!("random frames #{i}"), got, how));
    }

    // 5. Connections that never handshake (must be dropped after the handshake timeout).
    let mut idle = Vec::new();
    for _ in 0..10 {
        idle.push(connect(&url).await);
    }
    for (i, mut ws) in idle.into_iter().enumerate() {
        let (got, how) = drain(&mut ws, 6).await;
        log.push((format!("idle #{i}"), got, how));
    }
    log
}

#[test]
fn hostile_peers_cannot_hurt_the_host_or_other_players() {
    let mut host: Host = HostSession::start(HostConfig {
        bind: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        port: 0,
        local_player: Some(bd_net::LocalPlayer { name: "Host".into(), corps: Corps::Willpower }),
        handshake_timeout: Duration::from_secs(2),
        ..Default::default()
    })
    .expect("host");
    let port = host.port();
    let addr = format!("127.0.0.1:{port}");
    let mut clients = vec![client(&addr, "good1"), client(&addr, "good2")];
    let mut hev = Vec::new();
    let mut cev: Vec<Vec<ClientEvent<S2C>>> = vec![Vec::new(), Vec::new()];
    assert!(
        pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(10), |h, cs, _, _| {
            h.players().len() == 3 && cs.iter().all(|c| c.is_connected())
        }),
        "good clients failed to join"
    );
    let good_ids: Vec<PlayerId> = clients.iter().map(|c| c.my_id().unwrap()).collect();

    // Raw TCP junk (not even a WebSocket): never surfaces to the game.
    for i in 0..5u8 {
        if let Ok(mut s) = TcpStream::connect(&addr) {
            let _ = s.write_all(&[i; 64]);
            let _ = s.write_all(b"\r\n\r\nGET /?a=\x00\xff HTTP/9.9\r\n\r\n");
        }
    }

    // Attackers on their own thread + runtime while the host keeps ticking.
    let attacker = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("rt");
        rt.block_on(attacks(port, 0x5EED_CAFE))
    });
    let mut rtt_checked = false;
    let mut hostile_name_len = None;
    while !attacker.is_finished() {
        while let Some(e) = host.poll() {
            if let HostEvent::Message { from, msg: C2S::Echo(x) } = &e {
                if *x == 4242.0 {
                    // All pings sent before the marker have been processed by now.
                    let rtt = host.rtt(*from);
                    assert!(rtt.is_none_or(|r| r.is_finite() && r > 0.0), "client-reported RTT not sanitised: {rtt:?}");
                    rtt_checked = true;
                }
            }
            if let HostEvent::PlayerJoined(p) = &e {
                if p.name.starts_with("AAAA") {
                    hostile_name_len = Some(p.name.chars().count());
                    assert!(!p.name.chars().any(|c| c.is_control()), "control chars in name");
                }
            }
            hev.push(e);
        }
        for (c, ev) in clients.iter_mut().zip(cev.iter_mut()) {
            while let Some(e) = c.poll() {
                ev.push(e);
            }
        }
        // Keep the good clients chatting the whole time.
        for c in clients.iter_mut() {
            c.send(&C2S::Echo(1.0));
        }
        host.broadcast(&S2C::Echo(2.0));
        std::thread::sleep(Duration::from_millis(5));
    }
    let log = attacker.join().expect("attacker thread panicked");

    // Every attacker was turned away.
    for (label, got, how) in &log {
        assert_ne!(*how, "timeout", "{label}: host neither rejected nor closed the connection ({got:?})");
        let welcomed = got.iter().any(|m| matches!(m, HostWire::Welcome { .. }));
        if label.starts_with("pre-handshake") || label.starts_with("bad") {
            assert!(!welcomed, "{label}: welcomed");
            assert!(got.iter().any(|m| matches!(m, HostWire::Reject(_))), "{label}: no Reject sent ({got:?})");
        }
        if label == "handshake then garbage" {
            assert!(welcomed, "hostile-name handshake should still be accepted (sanitised)");
            assert!(got.iter().any(|m| matches!(m, HostWire::Kicked(r) if r.contains("protocol error"))), "not kicked: {got:?}");
        }
    }
    assert!(rtt_checked, "marker message never arrived");
    assert!(hostile_name_len.is_some_and(|n| n <= bd_net::session::MAX_NAME_CHARS), "name not truncated: {hostile_name_len:?}");
    // Every attacker that got in was removed again.
    assert!(hev.iter().any(|e| matches!(e, HostEvent::PlayerLeft { reason: LeaveReason::Kicked(r), .. } if r.contains("protocol error"))));

    // The good clients never noticed anything beyond join/leave chatter.
    for (c, ev) in clients.iter().zip(&cev) {
        assert!(c.is_connected(), "good client dropped: {:?}", ev.iter().rev().take(3).collect::<Vec<_>>());
        assert!(!ev.iter().any(|e| matches!(e, ClientEvent::Disconnected(_))));
        assert!(ev.iter().filter(|e| matches!(e, ClientEvent::Message(S2C::Echo(_)))).count() > 10);
    }
    let from_good = hev.iter().filter(|e| matches!(e, HostEvent::Message { from, msg: C2S::Echo(_) } if good_ids.contains(from))).count();
    assert!(from_good > 10, "host stopped hearing the good clients ({from_good})");
    // Only the host + the two good players remain.
    let ok = pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(5), |h, _, _, _| h.players().len() == 3);
    assert!(ok, "attackers left behind in the lobby: {:?}", host.players());

    // And the session still works end to end.
    clients[0].send(&C2S::Chat("still here".into()));
    let ok = pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(5), |_, _, hev, _| {
        hev.iter().any(|e| matches!(e, HostEvent::Message { msg: C2S::Chat(t), .. } if t == "still here"))
    });
    assert!(ok, "post-attack chat lost");
}

/// Host with one good client, plus an attacker connection that completed the handshake.
fn host_with_good_client() -> (Host, Vec<Client>, Vec<HostEvent<C2S>>, Vec<Vec<ClientEvent<S2C>>>) {
    let mut host = loopback_host(Some("Host"));
    let addr = format!("127.0.0.1:{}", host.port());
    let mut clients = vec![client(&addr, "good")];
    let mut hev = Vec::new();
    let mut cev = vec![Vec::new()];
    assert!(pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(10), |_, cs, _, _| cs[0].is_connected()));
    (host, clients, hev, cev)
}

/// Pump host + clients until `done`, while `each` runs every iteration (e.g. to keep broadcasting).
fn pump_while(
    host: &mut Host,
    clients: &mut [Client],
    hev: &mut Vec<HostEvent<C2S>>,
    cev: &mut [Vec<ClientEvent<S2C>>],
    secs: u64,
    mut each: impl FnMut(&mut Host),
    mut done: impl FnMut(&[HostEvent<C2S>]) -> bool,
) -> bool {
    let end = std::time::Instant::now() + Duration::from_secs(secs);
    while std::time::Instant::now() < end {
        each(host);
        while let Some(e) = host.poll() {
            hev.push(e);
        }
        for (c, ev) in clients.iter_mut().zip(cev.iter_mut()) {
            while let Some(e) = c.poll() {
                ev.push(e);
            }
        }
        if done(hev) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    false
}

/// A client that completes the handshake and then never reads again must be dropped once its send
/// queue passes `MAX_QUEUED_BYTES`, instead of growing the host's memory forever. Other players are
/// unaffected.
#[test]
fn client_that_stops_reading_is_dropped() {
    let (mut host, mut clients, mut hev, mut cev) = host_with_good_client();
    let port = host.port();
    // The attacker: handshake, then hold the socket without ever polling it.
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("rt");
    let mut stalled = rt.block_on(async move {
        let mut ws = connect(&format!("ws://127.0.0.1:{port}")).await;
        let _ = ws.send(hello(PROTOCOL_MAGIC, PROTOCOL_VERSION, "sloth")).await;
        ws
    });
    let joined = pump_while(
        &mut host,
        &mut clients,
        &mut hev,
        &mut cev,
        10,
        |_| {},
        |hev| hev.iter().any(|e| matches!(e, HostEvent::PlayerJoined(p) if p.name == "sloth")),
    );
    assert!(joined, "attacker never joined");
    // Keep pinging from the attacker's side so it isn't dropped for idleness, and keep broadcasting
    // 256 KiB snapshots that the good client drains and the attacker doesn't.
    let blob = vec![7u8; 256 * 1024];
    let mut tick = 0u64;
    let dropped = pump_while(
        &mut host,
        &mut clients,
        &mut hev,
        &mut cev,
        60,
        |h| {
            tick += 1;
            h.broadcast(&S2C::Blob { tick, sent_at: 0.0, data: blob.clone() });
            if tick % 50 == 0 {
                let mut ping = vec![K_PING];
                ping.extend_from_slice(&0f64.to_le_bytes());
                ping.extend_from_slice(&0.05f32.to_le_bytes());
                let _ = rt.block_on(stalled.send(Message::Binary(ping.into())));
            }
        },
        |hev| {
            hev.iter().any(
                |e| matches!(e, HostEvent::PlayerLeft { reason: LeaveReason::ConnectionLost(r), .. } if r.contains("send queue overflow")),
            )
        },
    );
    assert!(dropped, "stalled reader was never dropped; events: {:?}", hev.iter().rev().take(3).collect::<Vec<_>>());
    assert!(tick as usize * blob.len() > bd_net::transport::MAX_QUEUED_BYTES, "overflow needs at least the queue limit");
    // The good client kept receiving and is still connected.
    assert!(clients[0].is_connected());
    let got = cev[0].iter().filter(|e| matches!(e, ClientEvent::Message(S2C::Blob { .. }))).count();
    assert!(got > 50, "good client received only {got} blobs");
    assert_eq!(host.players().len(), 2);
}

/// Client → host messages are capped at `MAX_CLIENT_MESSAGE_BYTES`: a bigger one drops that client
/// at the WebSocket layer (never buffered or decoded); one just under the cap is fine.
#[test]
fn oversized_client_message_is_dropped() {
    let (mut host, mut clients, mut hev, mut cev) = host_with_good_client();
    let cap = bd_net::codec::MAX_CLIENT_MESSAGE_BYTES;
    // Just under the cap: delivered.
    let ok_text = "x".repeat(cap - 64);
    clients[0].send(&C2S::Chat(ok_text.clone()));
    let got = pump_while(
        &mut host,
        &mut clients,
        &mut hev,
        &mut cev,
        10,
        |_| {},
        |hev| hev.iter().any(|e| matches!(e, HostEvent::Message { msg: C2S::Chat(t), .. } if t.len() == ok_text.len())),
    );
    assert!(got, "message under the cap was not delivered");
    // An attacker sends a message over the cap.
    let port = host.port();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("rt");
    let attacker = std::thread::spawn(move || {
        rt.block_on(async move {
            let mut ws = connect(&format!("ws://127.0.0.1:{port}")).await;
            let _ = ws.send(hello(PROTOCOL_MAGIC, PROTOCOL_VERSION, "big")).await;
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = ws.send(data(&ClientWire::App(C2S::Chat("y".repeat(cap * 2))))).await;
            drain(&mut ws, 10).await
        })
    });
    let left = pump_while(
        &mut host,
        &mut clients,
        &mut hev,
        &mut cev,
        15,
        |_| {},
        |hev| hev.iter().any(|e| matches!(e, HostEvent::PlayerLeft { .. })),
    );
    let (_, how) = attacker.join().expect("attacker thread");
    assert!(left, "oversized sender was not dropped");
    assert_ne!(how, "timeout");
    assert!(!hev.iter().any(|e| matches!(e, HostEvent::Message { msg: C2S::Chat(t), .. } if t.len() > cap)), "oversized message delivered");
    assert!(clients[0].is_connected());
    assert_eq!(host.players().len(), 2);
}
