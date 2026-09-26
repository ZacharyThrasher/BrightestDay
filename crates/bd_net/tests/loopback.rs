//! Host + 7 clients over real loopback WebSockets: handshake, ids, corps, join/leave, messaging,
//! latest lane, lobby-full rejection, kick, quit, drop, host shutdown.

mod common;

use std::collections::HashSet;
use std::time::Duration;

use bd_core::PlayerId;
use bd_net::{ClientEvent, ClientState, HostEvent, LeaveReason};
use common::*;

#[test]
fn host_and_seven_clients() {
    let mut host = loopback_host(Some("Host"));
    let addr = format!("127.0.0.1:{}", host.port());
    let mut clients: Vec<Client> = (0..7).map(|i| client(&addr, if i == 3 { "host" } else { ["ann", "bo", "cy", "-", "  ", "ed", "flo"][i] })).collect();
    let mut hev = Vec::new();
    let mut cev: Vec<Vec<_>> = vec![Vec::new(); 7];

    // Everyone welcomed and everyone sees 8 players.
    let ok = pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(10), |h, cs, _, _| {
        h.players().len() == 8 && cs.iter().all(|c| c.is_connected() && c.players().len() == 8)
    });
    assert!(ok, "not all joined: host has {} players", host.players().len());
    let ids: HashSet<u8> = clients.iter().map(|c| c.my_id().unwrap().0).collect();
    assert_eq!(ids, (1..=7).collect::<HashSet<u8>>());
    let corps: HashSet<_> = host.players().iter().map(|p| p.corps).collect();
    assert_eq!(corps.len(), 8, "corps must be unique");
    let names: HashSet<_> = host.players().iter().map(|p| p.name.clone()).collect();
    assert_eq!(names.len(), 8);
    assert!(names.contains("host 2"), "duplicate name renamed: {names:?}");
    assert!(names.contains("Lantern"), "empty name defaulted: {names:?}");
    assert_eq!(hev.iter().filter(|e| matches!(e, HostEvent::PlayerJoined(_))).count(), 7);
    for (c, ev) in clients.iter().zip(&cev) {
        assert!(matches!(ev.first(), Some(ClientEvent::Welcome { you, .. }) if Some(*you) == c.my_id()));
        assert_eq!(c.players(), host.players());
    }

    // Chat relay: each client says hi, host rebroadcasts; every client sees all 7.
    for (i, c) in clients.iter_mut().enumerate() {
        assert!(c.send(&C2S::Chat(format!("hi {i}"))));
    }
    let chats = |hev: &[HostEvent<C2S>]| -> Vec<(PlayerId, String)> {
        hev.iter()
            .filter_map(|e| match e {
                HostEvent::Message { from, msg: C2S::Chat(t) } => Some((*from, t.clone())),
                _ => None,
            })
            .collect()
    };
    assert!(pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(5), |_, _, hev, _| chats(hev).len() == 7));
    for (from, t) in chats(&hev) {
        host.broadcast(&S2C::Chat(from, t));
    }
    let ok = pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(5), |_, _, _, cev| {
        cev.iter().all(|ev| ev.iter().filter(|e| matches!(e, ClientEvent::Message(S2C::Chat(..)))).count() == 7)
    });
    assert!(ok, "chat relay incomplete");

    // Latest lane: 20 snapshots queued at once; clients keep at most 8, newest last, in order.
    for tick in 0..20u64 {
        host.broadcast_latest(&S2C::Blob { tick, sent_at: 0.0, data: vec![0; 100] });
    }
    std::thread::sleep(Duration::from_millis(100));
    pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_millis(50), |_, _, _, _| false);
    for c in clients.iter_mut() {
        let got: Vec<u64> = c
            .drain_latest()
            .map(|(_, m)| match m {
                S2C::Blob { tick, .. } => tick,
                _ => panic!(),
            })
            .collect();
        assert_eq!(*got.last().unwrap(), 19, "newest snapshot delivered");
        assert!(got.len() <= 8, "latest lane bounded: {}", got.len());
        assert!(got.windows(2).all(|w| w[0] < w[1]), "in order");
    }

    // RTT via network-thread pings.
    std::thread::sleep(Duration::from_millis(400));
    pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_millis(300), |_, _, _, _| false);
    for c in &clients {
        let r = c.clock().rtt();
        assert!(r.count >= 3, "pings: {}", r.count);
        assert!(r.srtt < 0.010, "loopback srtt {} s", r.srtt);
        eprintln!("client {:?}: srtt {:.3} ms min {:.3} ms", c.my_id().unwrap(), r.srtt * 1e3, r.min * 1e3);
    }
    assert!(host.rtt(clients[0].my_id().unwrap()).is_some());
    assert!(host.remote_addr(clients[0].my_id().unwrap()).unwrap().starts_with("127.0.0.1"));

    // Lobby full: a 9th player is rejected.
    let mut extra = vec![client(&addr, "late")];
    let mut extra_ev = vec![Vec::new()];
    let ok = pump_until(&mut host, &mut extra, &mut hev, &mut extra_ev, Duration::from_secs(5), |_, _, _, ev| !ev[0].is_empty());
    assert!(ok);
    assert!(matches!(&extra_ev[0][0], ClientEvent::Disconnected(r) if r.contains("full")), "{:?}", extra_ev[0]);
    drop(extra);

    // Kick client 0, client 1 quits, client 2 is dropped (sends Bye from Drop).
    let kicked = clients[0].my_id().unwrap();
    let quit = clients[1].my_id().unwrap();
    let dropped = clients[2].my_id().unwrap();
    hev.clear();
    for e in cev.iter_mut() {
        e.clear();
    }
    host.kick(kicked, "too good");
    clients[1].disconnect();
    let c2 = clients.remove(2);
    cev.remove(2);
    drop(c2);
    let ok = pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(5), |h, _, hev, cev| {
        h.players().len() == 5 && hev.len() == 3 && cev[2..].iter().all(|e| e.len() >= 3)
    });
    assert!(ok, "leave events: host {hev:?}");
    assert!(hev.contains(&HostEvent::PlayerLeft { id: kicked, reason: LeaveReason::Kicked("too good".into()) }));
    assert!(hev.contains(&HostEvent::PlayerLeft { id: quit, reason: LeaveReason::Quit }));
    assert!(hev.contains(&HostEvent::PlayerLeft { id: dropped, reason: LeaveReason::Quit }), "{hev:?}");
    assert!(matches!(&cev[0][..], [ClientEvent::Disconnected(r)] if r.contains("kicked: too good")), "{:?}", cev[0]);
    assert!(cev[1].is_empty(), "local disconnect emits nothing: {:?}", cev[1]);
    for c in &clients[2..] {
        assert_eq!(c.players().len(), 5);
    }

    // A new player reuses the lowest free id.
    let mut again = vec![client(&addr, "again")];
    let mut again_ev = vec![Vec::new()];
    assert!(pump_until(&mut host, &mut again, &mut hev, &mut again_ev, Duration::from_secs(5), |_, c, _, _| c[0].is_connected()));
    assert_eq!(again[0].my_id(), Some(kicked.min(quit).min(dropped)));
    drop(again);

    // Host closes the lobby: everyone still connected is told.
    host.shutdown();
    let ok = pump_until(&mut host, &mut clients, &mut hev, &mut cev, Duration::from_secs(5), |_, cs, _, _| {
        cs[2..].iter().all(|c| matches!(c.state(), ClientState::Disconnected(_)))
    });
    assert!(ok);
    for c in &clients[2..] {
        assert!(matches!(c.state(), ClientState::Disconnected(r) if r.contains("host closed")), "{:?}", c.state());
    }
}

#[test]
fn port_fallback_and_bad_addresses() {
    let a = loopback_host(None);
    let b: Host = bd_net::HostSession::start(bd_net::HostConfig {
        bind: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        port: a.port(),
        ..Default::default()
    })
    .unwrap();
    assert_ne!(a.port(), b.port());
    let strict: Result<Host, _> = bd_net::HostSession::start(bd_net::HostConfig {
        bind: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        port: a.port(),
        port_fallback: false,
        ..Default::default()
    });
    assert!(strict.is_err());

    assert!(Client::connect("not a valid address", Default::default()).is_err());
    // Nothing listens here: fails with a message after the connect timeout.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let mut c = Client::connect(
        &format!("127.0.0.1:{port}"),
        bd_net::ClientConfig { connect_timeout: Duration::from_secs(1), ..Default::default() },
    )
    .unwrap();
    let t = std::time::Instant::now();
    let ev = loop {
        if let Some(e) = c.poll() {
            break e;
        }
        assert!(t.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(matches!(ev, ClientEvent::Disconnected(ref r) if r.contains("could not connect")), "{ev:?}");
}
