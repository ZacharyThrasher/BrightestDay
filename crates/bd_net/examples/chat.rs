//! End-to-end check of the whole path: host + cloudflared quick tunnel + lobby code + clients.
//!
//! ```text
//! cargo run -p bd_net --example chat -- --host [--name Zach] [--port 27960] [--bind 127.0.0.1] [--no-tunnel]
//! cargo run -p bd_net --example chat -- --join <lobby-code | ip:port | wss://…> [--name Friend]
//! ```
//! Type a line + Enter to chat. Commands: `/players`, `/ping`, `/kick <id>` (host), `/quit`.

use std::io::BufRead;
use std::net::IpAddr;
use std::time::Duration;

use bd_core::PlayerId;
use bd_core::corps::Corps;
use bd_net::{
    ClientConfig, ClientEvent, ClientSession, HostConfig, HostEvent, HostSession, LeaveReason, LocalPlayer, Tunnel, TunnelStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
enum Up {
    Chat(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum Down {
    Chat { from: PlayerId, name: String, text: String },
}

struct Args {
    host: bool,
    join: Option<String>,
    name: String,
    port: u16,
    bind: IpAddr,
    tunnel: bool,
}

fn parse_args() -> Args {
    let mut a =
        Args { host: false, join: None, name: String::new(), port: bd_net::DEFAULT_PORT, bind: "0.0.0.0".parse().unwrap(), tunnel: true };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--host" => a.host = true,
            "--join" => a.join = it.next(),
            "--name" => a.name = it.next().unwrap_or_default(),
            "--port" => a.port = it.next().and_then(|p| p.parse().ok()).expect("--port N"),
            "--bind" => a.bind = it.next().and_then(|p| p.parse().ok()).expect("--bind IP"),
            "--no-tunnel" => a.tunnel = false,
            _ => {
                eprintln!("usage: chat --host [--name N] [--port P] [--bind IP] [--no-tunnel] | --join <code|addr> [--name N]");
                std::process::exit(2);
            }
        }
    }
    if a.host == a.join.is_some() {
        eprintln!("pass exactly one of --host or --join <code>");
        std::process::exit(2);
    }
    if a.name.is_empty() {
        a.name = if a.host { "Host".into() } else { "Guest".into() };
    }
    a
}

/// Lines typed on stdin, delivered without ever blocking the main loop.
fn stdin_lines() -> crossbeam_channel::Receiver<String> {
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn main() {
    let args = parse_args();
    if args.host { run_host(args) } else { run_client(args) }
}

fn run_host(args: Args) {
    let mut host: HostSession<Up, Down> = HostSession::start(HostConfig {
        bind: args.bind,
        port: args.port,
        local_player: Some(LocalPlayer { name: args.name.clone(), corps: Corps::Willpower }),
        ..Default::default()
    })
    .unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let port = host.port();
    println!("[host] listening on {}:{port} (LAN: ws://<this-pc-ip>:{port})", args.bind);
    let tunnel = args.tunnel.then(|| Tunnel::start(port));
    let mut announced = false;
    let lines = stdin_lines();
    let me = PlayerId(0);
    loop {
        if let (Some(t), false) = (&tunnel, announced) {
            match t.status() {
                TunnelStatus::Ready { url, code } => {
                    println!("[host] tunnel up: {url}");
                    println!("[host] LOBBY CODE: {code}");
                    println!("[host] friends run: chat --join {code}");
                    announced = true;
                }
                TunnelStatus::Failed(e) => {
                    println!("[host] tunnel failed: {e} (LAN/direct still works)");
                    announced = true;
                }
                TunnelStatus::Starting => {}
            }
        }
        while let Some(ev) = host.poll() {
            match ev {
                HostEvent::PlayerJoined(p) => {
                    let via = host.remote_addr(p.id).unwrap_or("?").to_string();
                    println!("[host] {} joined as {:?} ({}) from {via}", p.name, p.id, p.corps.name());
                }
                HostEvent::PlayerLeft { id, reason } => println!("[host] {id:?} left: {}", reason_text(&reason)),
                HostEvent::Message { from, msg: Up::Chat(text) } => {
                    let name = host.player(from).map(|p| p.name.clone()).unwrap_or_default();
                    println!("<{name}> {text}");
                    host.broadcast_except(from, &Down::Chat { from, name, text });
                }
            }
        }
        while let Ok(line) = lines.try_recv() {
            let line = line.trim().to_string();
            match line.split_whitespace().collect::<Vec<_>>().as_slice() {
                [] => {}
                ["/quit"] => {
                    host.shutdown();
                    std::thread::sleep(Duration::from_millis(300));
                    return;
                }
                ["/players"] => {
                    for p in host.players() {
                        let rtt = host.rtt(p.id).map(|r| format!("{:.0} ms", r * 1000.0)).unwrap_or_else(|| "-".into());
                        println!("  {:?} {} ({}) rtt {rtt}", p.id, p.name, p.corps.name());
                    }
                }
                ["/kick", id] => match id.parse::<u8>() {
                    Ok(id) => host.kick(PlayerId(id), "kicked by host"),
                    Err(_) => println!("usage: /kick <id>"),
                },
                _ => host.broadcast(&Down::Chat { from: me, name: args.name.clone(), text: line }),
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn run_client(args: Args) {
    let target = args.join.clone().unwrap();
    let mut client: ClientSession<Up, Down> = ClientSession::connect(
        &target,
        ClientConfig { name: args.name.clone(), corps: Corps::Hope, connect_timeout: Duration::from_secs(30), ..Default::default() },
    )
    .unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    println!("[client] connecting to {} ...", client.url());
    let started = std::time::Instant::now();
    let lines = stdin_lines();
    loop {
        while let Some(ev) = client.poll() {
            match ev {
                ClientEvent::Welcome { you, players } => {
                    println!("[client] joined as {you:?} after {:.1}s; players:", started.elapsed().as_secs_f32());
                    for p in players {
                        println!("  {:?} {} ({})", p.id, p.name, p.corps.name());
                    }
                }
                ClientEvent::PlayerJoined(p) => println!("[client] {} joined ({})", p.name, p.corps.name()),
                ClientEvent::PlayerLeft { id, reason } => println!("[client] {id:?} left: {}", reason_text(&reason)),
                ClientEvent::Message(Down::Chat { name, text, .. }) => println!("<{name}> {text}"),
                ClientEvent::Disconnected(r) => {
                    println!("[client] disconnected: {r}");
                    return;
                }
            }
        }
        while let Ok(line) = lines.try_recv() {
            let line = line.trim().to_string();
            match line.as_str() {
                "" => {}
                "/quit" => {
                    client.disconnect();
                    std::thread::sleep(Duration::from_millis(300));
                    return;
                }
                "/ping" => {
                    let r = client.clock().rtt();
                    println!(
                        "[client] rtt {:.1} ms (min {:.1}, jitter {:.1}) over {} pings; host tick ≈ {:.0}",
                        r.srtt * 1e3,
                        r.min * 1e3,
                        r.rttvar * 1e3,
                        r.count,
                        client.host_tick_now()
                    );
                }
                "/players" => {
                    for p in client.players() {
                        println!("  {:?} {} ({})", p.id, p.name, p.corps.name());
                    }
                }
                _ => {
                    client.send(&Up::Chat(line));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn reason_text(r: &LeaveReason) -> String {
    match r {
        LeaveReason::Quit => "quit".into(),
        LeaveReason::Kicked(why) => format!("kicked ({why})"),
        LeaveReason::ConnectionLost(why) => format!("connection lost ({why})"),
    }
}
