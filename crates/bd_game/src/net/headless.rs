//! Window-less host / client loops for the loopback harness (`tools/net_smoke.ps1`).
//!
//! Both run the *same* net cores as the game ([`HostCore`] around an [`ActiveWorld`] with the
//! bridge's `step` / `apply_command`; [`ClientCore`] with prediction + reconciliation +
//! interpolation). The client's inputs and casts come from a `bd_sim` bot brain thinking on the
//! **replica** world, so they look like a real player's. On exit both print one
//! `NETSTATS {json}` line (terrain hash, RTT, bandwidth, corrections, desyncs…).

use std::time::Duration;

use bd_core::corps::Corps;
use bd_core::{DT, PlayerId, TICK_HZ};
use bd_net::{TickPacer, TunnelStatus, now_secs};
use bd_sim::bots::BotBrain;
use bd_sim::combat::CarveRequest;
use bd_sim::rng::SimRng;
use bd_sim::World;

use super::cli::NetCli;
use super::client::{ClientCore, ClientPhase};
use super::host::{HostCore, HostStart};
use super::proto::{CastOutcome, NetMap, NetMapKind, NetProfile};
use crate::sim_bridge::{ActiveWorld, Cmd, SimRole};

/// Seconds at the end of a run with no new terrain edits, so every final terrain hash is comparable.
const QUIET_TAIL: f64 = 8.0;

fn sleep_until(t: f64) {
    let d = t - now_secs();
    if d > 0.0015 {
        std::thread::sleep(Duration::from_secs_f64(d - 0.001));
    }
    while now_secs() < t {
        std::hint::spin_loop();
    }
}

fn percentile(v: &mut [f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f32 * p).round() as usize]
}

/// `--host --headless`: authoritative server with bots, periodic terrain destruction.
pub fn run_host(cli: &NetCli) -> i32 {
    let bots_n = cli.bots.min(6);
    let settings = bd_sim::settings::GameSettings::default();
    let start = HostStart {
        port: cli.port.unwrap_or(bd_net::DEFAULT_PORT),
        tunnel: !cli.no_tunnel,
        lan: true,
        name: cli.name.clone().unwrap_or_else(|| "HeadlessHost".into()),
        corps: cli.corps.unwrap_or(Corps::Willpower),
        profile: NetProfile::default(),
        settings: settings.clone(),
        max_players: 8 - bots_n as usize,
    };
    let mut host = match HostCore::start(start) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("NETERROR host: {e}");
            return 2;
        }
    };
    if let Some(s) = cli.check_secs {
        host.terrain_check_every = (s.max(0.1) * TICK_HZ as f32) as u64;
    }
    println!("NETHOST port={} lan={:?}", host.port, host.lan_addresses());
    let t_start = now_secs();
    // Wait for the tunnel (if any) before announcing the code.
    if host.tunnel.is_some() {
        loop {
            match host.tunnel_status() {
                Some(TunnelStatus::Ready { code, url }) => {
                    println!("NETCODE code={code} url={url} after={:.1}s", now_secs() - t_start);
                    break;
                }
                Some(TunnelStatus::Failed(e)) => {
                    println!("NETCODE failed: {e}");
                    break;
                }
                _ => std::thread::sleep(Duration::from_millis(50)),
            }
            if now_secs() - t_start > 40.0 {
                println!("NETCODE failed: timeout");
                break;
            }
        }
    }
    let kind = match cli.map.as_deref() {
        Some("miami") => NetMapKind::Miami,
        Some("orbit") => NetMapKind::Orbit,
        _ => NetMapKind::Archipelago,
    };
    let (world, map, hasher) = HostCore::build_world(&settings, NetMap { kind, seed: crate::arena::ARENA_SEED, pristine_hash: 0 }, cli.seed);
    let mut active = ActiveWorld::new(world, Some(host.me), SimRole::Host);
    let mut bots: Vec<BotBrain> = Vec::new();
    host.start_match(&mut active, map, hasher);
    active.world.start_live();
    bots.push(BotBrain::new(host.me, 0.6, cli.seed));
    for k in 0..bots_n {
        let id = PlayerId(7 - k);
        if active.world.add_player(id, format!("bot{}", id.0), Corps::from_index(id.0)).is_ok() {
            bots.push(BotBrain::new(id, 0.6, cli.seed.wrapping_add(k as u64 + 1)));
        }
    }
    active.bots = bots;
    let run_for = cli.quit_after.unwrap_or(60.0) as f64 + 10.0; // clients connect later; outlive them
    let mut rng = SimRng::new(cli.seed ^ 0xC0FFEE);
    let mut next_tick = now_secs();
    let mut next_boom = now_secs() + 2.0;
    let mut cmds = Vec::new();
    let mut tick_ms: Vec<f32> = Vec::with_capacity(20_000);
    let t0 = now_secs();
    let mut quiet_since: Option<f64> = None;
    let mut last_report = t0;
    loop {
        let now = now_secs();
        if now - t0 > run_for {
            break;
        }
        // Stop terrain edits for the tail (and whenever every client has left after playing).
        let tail = now - t0 > run_for - QUIET_TAIL - 10.0;
        if tail && quiet_since.is_none() {
            quiet_since = Some(now);
            for b in active.bots.iter_mut() {
                b.casts_enabled = false;
            }
            let ids: Vec<PlayerId> = active.world.players.iter().map(|p| p.id).collect();
            for id in ids {
                active.world.dismiss(id);
            }
        }
        host.poll(Some(&mut active), &mut cmds);
        for (id, c) in cmds.drain(..) {
            active.apply_command(id, c);
        }
        if now >= next_boom && !tail {
            next_boom = now + 1.5;
            let spawns = &active.world.arena.spawns;
            if !spawns.is_empty() {
                let s = spawns[rng.below(spawns.len() as u32) as usize];
                // Well away from the spawn itself (players spawning into a crater are stuck).
                let off = bd_core::Vec3::new(rng.range(8.0, 20.0) * if rng.chance(0.5) { 1.0 } else { -1.0 }, rng.range(-3.0, 0.0), rng.range(8.0, 20.0) * if rng.chance(0.5) { 1.0 } else { -1.0 });
                active.world.request_carve(CarveRequest { center: s + off, radius: rng.range(1.5, 4.5), power: 1.5 });
            }
        }
        let mut n = 0;
        while now_secs() >= next_tick && n < 8 {
            let t = now_secs();
            host.pre_tick(&mut active);
            active.step(None);
            host.post_tick(&mut active);
            tick_ms.push(((now_secs() - t) * 1000.0) as f32);
            next_tick += DT as f64;
            n += 1;
        }
        if now_secs() - next_tick > 0.25 {
            next_tick = now_secs();
        }
        if now - last_report > 5.0 {
            last_report = now;
            for (id, depth, st) in host.queue_report() {
                super::host::bd_net_log(|| format!("host: p{} depth {depth} {st:?} rtt {:?}", id.0, host.session.rtt(id)));
            }
        }
        sleep_until(next_tick.min(now_secs() + 0.002));
    }
    let hash = active.world.voxels.content_hash();
    let secs = now_secs() - t0;
    let mut tm = tick_ms.clone();
    let st = &host.stats;
    let q: Vec<String> = host
        .queue_report()
        .iter()
        .map(|(id, d, s)| format!("{{\"id\":{},\"depth\":{d},\"starved\":{},\"dropped\":{},\"consumed\":{}}}", id.0, s.starved, s.dropped, s.consumed))
        .collect();
    println!(
        "NETSTATS {{\"role\":\"host\",\"terrain_hash\":\"{hash:016x}\",\"secs\":{secs:.1},\"tx_kbps\":{:.1},\"rx_kbps\":{:.1},\"snapshots\":{},\"events\":{},\"terrain_checks\":{},\"inits\":{},\"resyncs\":{},\"keyframes\":{},\"input_starved\":{},\"tick_p50_ms\":{:.3},\"tick_p99_ms\":{:.3},\"net_cost_ms\":{:.4},\"queues\":[{}]}}",
        host.session.bytes_sent() as f64 * 8.0 / 1000.0 / secs,
        host.session.bytes_received() as f64 * 8.0 / 1000.0 / secs,
        st.snapshots_sent,
        st.events_sent,
        st.terrain_checks,
        st.inits_sent,
        st.resyncs,
        st.keyframes,
        st.input_starved,
        percentile(&mut tm, 0.5),
        percentile(&mut tm, 0.99),
        st.cost_ms,
        q.join(",")
    );
    host.shutdown();
    0
}

/// `--join X --headless`: a scripted bot client (prediction + reconciliation + interpolation).
pub fn run_client(cli: &NetCli) -> i32 {
    let Some(addr) = cli.join.clone() else { return 2 };
    // Optional lag in front of the host (ip:port only).
    let _proxy;
    let addr = if cli.lag_ms > 0.0 || cli.jitter_ms > 0.0 {
        let Some(target) = bd_net::resolve_address(&addr).ok().and_then(|u| u.strip_prefix("ws://").map(str::to_string)).and_then(|h| {
            use std::net::ToSocketAddrs;
            h.to_socket_addrs().ok()?.next()
        }) else {
            eprintln!("NETERROR --lag-ms needs an ip:port address");
            return 2;
        };
        let p = match bd_net::LagProxy::start(target, bd_net::LagSettings { latency_ms: cli.lag_ms, jitter_ms: cli.jitter_ms }) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("NETERROR lag proxy: {e}");
                return 2;
            }
        };
        let url = p.url();
        _proxy = Some(p);
        url
    } else {
        _proxy = None;
        addr
    };
    let name = cli.name.clone().unwrap_or_else(|| format!("bot{}", cli.seed));
    let mut c = match ClientCore::connect(&addr, &name, cli.corps.unwrap_or(Corps::Rage), NetProfile::default()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("NETERROR connect: {e}");
            return 2;
        }
    };
    let t0 = now_secs();
    let run_for = cli.quit_after.unwrap_or(60.0) as f64;
    let mut world: Option<World> = None;
    let mut brain: Option<BotBrain> = None;
    let mut pacer: Option<TickPacer> = None;
    let mut last = now_secs();
    let mut ingame_at: Option<f64> = None;
    let mut rtts: Vec<f32> = Vec::new();
    let mut last_rtt_sample = t0;
    let mut interp_starved0 = 0;
    let (mut casts_ok, mut casts_failed) = (0u32, 0u32);
    loop {
        let now = now_secs();
        if now - t0 > run_for {
            break;
        }
        let out = c.update(world.as_mut());
        if let Some(w) = out.new_world {
            world = Some(w);
            if ingame_at.is_none() {
                ingame_at = Some(now);
                println!("NETJOINED after={:.2}s build_ms={:.0}", now - t0, c.stats.world_build_ms);
            }
        }
        c.outbox.clear();
        for (_, o) in c.cast_results.drain(..) {
            match o {
                Some(CastOutcome::Spawned(_) | CastOutcome::Grafted(_)) => casts_ok += 1,
                _ => casts_failed += 1,
            }
        }
        if let ClientPhase::Failed(r) = &c.phase {
            eprintln!("NETERROR failed: {r}");
            break;
        }
        let dt = now - last;
        last = now;
        if let (Some(w), ClientPhase::InGame, Some(me)) = (world.as_mut(), &c.phase, c.me) {
            let b = brain.get_or_insert_with(|| BotBrain::new(me, 0.6, cli.seed));
            let quiet = now - t0 > run_for - QUIET_TAIL;
            b.casts_enabled = !quiet;
            if let Some(target) = c.session.clock().is_synced().then(|| c.session.clock().target_client_tick(now, super::client::LEAD_BUFFER_TICKS)) {
                let p = pacer.get_or_insert_with(|| TickPacer::new(TICK_HZ, target as u64));
                let st = p.advance(dt, Some(target));
                for _ in 0..st.count {
                    let act = b.think(w);
                    if let Some((r, aim)) = act.cast {
                        c.send_cmd(&Cmd::Cast { recipe: Box::new(r), shape: None, alt_shape: None, aim, incantation: false });
                    }
                    if let Some(section) = act.begin_oath {
                        c.send_cmd(&Cmd::OathBegin { section });
                    }
                    if let Some((accuracy, seconds)) = act.finish_oath {
                        c.send_cmd(&Cmd::OathResult { accuracy, seconds, voice: false });
                    }
                    if act.dismiss {
                        c.send_cmd(&Cmd::Dismiss);
                    }
                    let _ = c.predict(w, act.input);
                }
            }
            let _ = c.interp.update(now);
            c.release_events();
            c.outbox.clear();
            c.send_inputs(false);
            if now - last_rtt_sample > 0.5 {
                last_rtt_sample = now;
                rtts.push(c.rtt_ms());
            }
            if ingame_at.is_some_and(|t| now - t < 2.0) {
                interp_starved0 = c.interp.starved_frames;
            }
        }
        sleep_until(now + 0.001);
    }
    let secs = now_secs() - t0;
    let hash = world.as_ref().map(|w| w.voxels.content_hash()).unwrap_or(0);
    let s = &c.stats;
    let r = c.session.clock().rtt();
    println!(
        "NETSTATS {{\"role\":\"client\",\"name\":\"{name}\",\"terrain_hash\":\"{hash:016x}\",\"secs\":{secs:.1},\"joined\":{},\"rtt_ms\":{:.1},\"rtt_min_ms\":{:.1},\"rtt_p95_ms\":{:.1},\"jitter_ms\":{:.1},\"tx_kbps\":{:.1},\"rx_kbps\":{:.1},\"snapshots\":{},\"events\":{},\"corrections\":{},\"visible_corrections\":{},\"snapped\":{},\"max_error_m\":{:.3},\"terrain_checks_ok\":{},\"desyncs\":{},\"inits\":{},\"reconnects\":{},\"keyframe_requests\":{},\"interp_delay_ticks\":{:.2},\"interp_starved\":{},\"latest_dropped\":{},\"net_cost_ms\":{:.4},\"build_ms\":{:.0},\"hits_taken\":{},\"unexplained_visible\":{},\"casts_ok\":{casts_ok},\"casts_failed\":{casts_failed}}}",
        ingame_at.is_some(),
        r.srtt * 1000.0,
        r.min * 1000.0,
        percentile(&mut rtts, 0.95),
        r.rttvar * 1000.0,
        c.session.bytes_sent() as f64 * 8.0 / 1000.0 / secs,
        c.session.bytes_received() as f64 * 8.0 / 1000.0 / secs,
        s.snapshots,
        s.events,
        s.corrections,
        s.visible_corrections,
        s.snapped_corrections,
        s.max_error_m,
        s.terrain_checks_ok,
        s.desyncs,
        s.inits,
        s.reconnects,
        s.keyframe_requests,
        c.interp.delay_ticks(),
        c.interp.starved_frames.saturating_sub(interp_starved0),
        c.session.latest_dropped(),
        s.cost_ms,
        s.world_build_ms,
        s.hits_taken,
        s.unexplained_visible,
    );
    let desyncs = c.stats.desyncs;
    c.leave();
    std::thread::sleep(Duration::from_millis(100));
    if ingame_at.is_some() && desyncs == 0 { 0 } else { 1 }
}
