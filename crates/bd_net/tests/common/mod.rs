#![allow(dead_code)]

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use bd_core::PlayerId;
use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_net::{ClientConfig, ClientEvent, ClientSession, HostConfig, HostEvent, HostSession, InputPacket, LocalPlayer};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum C2S {
    Chat(String),
    Input(InputPacket<PlayerInput>),
    Echo(f64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum S2C {
    Chat(PlayerId, String),
    Blob { tick: u64, sent_at: f64, data: Vec<u8> },
    Echo(f64),
}

pub type Host = HostSession<C2S, S2C>;
pub type Client = ClientSession<C2S, S2C>;

pub fn loopback_host(local: Option<&str>) -> Host {
    HostSession::start(HostConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        local_player: local.map(|n| LocalPlayer { name: n.into(), corps: Corps::Willpower }),
        ..Default::default()
    })
    .expect("host start")
}

pub fn client(addr: &str, name: &str) -> Client {
    ClientSession::connect(addr, ClientConfig { name: name.into(), corps: Corps::Willpower, ..Default::default() }).expect("client connect")
}

/// Poll everything until `done` returns true or the timeout passes.
pub fn pump_until(
    host: &mut Host,
    clients: &mut [Client],
    host_events: &mut Vec<HostEvent<C2S>>,
    client_events: &mut [Vec<ClientEvent<S2C>>],
    timeout: Duration,
    mut done: impl FnMut(&Host, &[Client], &[HostEvent<C2S>], &[Vec<ClientEvent<S2C>>]) -> bool,
) -> bool {
    let end = Instant::now() + timeout;
    loop {
        while let Some(e) = host.poll() {
            host_events.push(e);
        }
        for (c, ev) in clients.iter_mut().zip(client_events.iter_mut()) {
            while let Some(e) = c.poll() {
                ev.push(e);
            }
        }
        if done(host, clients, host_events, client_events) {
            return true;
        }
        if Instant::now() > end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn percentile(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f64 * p).round() as usize]
}
