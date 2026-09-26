//! In-process loopback: a real `HostCore` (authoritative world, bots, terrain destruction) and two
//! real `ClientCore`s over localhost WebSockets — one joins at the start, one mid-match. Asserts
//! both replicas end with the host's exact terrain, terrain checks pass, prediction converges.

use bd_core::corps::Corps;
use bd_core::{DT, PlayerId, TICK_HZ};
use bd_net::{TickPacer, now_secs};
use bd_sim::bots::BotBrain;
use bd_sim::combat::CarveRequest;
use bd_sim::rng::SimRng;
use bd_sim::World;

use super::client::{ClientCore, ClientPhase};
use super::host::{HostCore, HostStart};
use super::proto::{NetMap, NetMapKind, NetProfile};
use crate::sim_bridge::{ActiveWorld, Cmd, SimRole};

struct Bot {
    core: ClientCore,
    world: Option<World>,
    brain: Option<BotBrain>,
    pacer: Option<TickPacer>,
    last: f64,
    seed: u64,
}

impl Bot {
    fn new(port: u16, name: &str, seed: u64) -> Self {
        let core = ClientCore::connect(&format!("127.0.0.1:{port}"), name, Corps::Rage, NetProfile::default()).expect("connect");
        Self { core, world: None, brain: None, pacer: None, last: now_secs(), seed }
    }

    fn frame(&mut self, casts: bool) {
        let now = now_secs();
        let out = self.core.update(self.world.as_mut());
        if let Some(w) = out.new_world {
            self.world = Some(w);
        }
        self.core.outbox.clear();
        self.core.cast_results.clear();
        let dt = now - self.last;
        self.last = now;
        let (Some(w), ClientPhase::InGame, Some(me)) = (self.world.as_mut(), &self.core.phase, self.core.me) else { return };
        let b = self.brain.get_or_insert_with(|| BotBrain::new(me, 0.7, self.seed));
        b.casts_enabled = casts;
        if !self.core.session.clock().is_synced() {
            return;
        }
        let target = self.core.session.clock().target_client_tick(now, super::client::LEAD_BUFFER_TICKS);
        let p = self.pacer.get_or_insert_with(|| TickPacer::new(TICK_HZ, target as u64));
        let st = p.advance(dt, Some(target));
        for _ in 0..st.count {
            let act = b.think(w);
            if let Some((r, aim)) = act.cast {
                self.core.send_cmd(&Cmd::Cast { recipe: Box::new(r), shape: None, alt_shape: None, aim, incantation: false });
            }
            let _ = self.core.predict(w, act.input);
        }
        let _ = self.core.interp.update(now);
        self.core.release_events();
        self.core.send_inputs(false);
    }
}

#[test]
fn loopback_host_two_clients_terrain_and_prediction() {
    let settings = bd_sim::settings::GameSettings::default();
    let mut host =
        HostCore::start(HostStart { port: 0, tunnel: false, lan: false, settings: settings.clone(), ..Default::default() }).expect("host");
    host.terrain_check_every = TICK_HZ as u64; // 1 s
    let port = host.port;
    let (world, map, hasher) = HostCore::build_world(&settings, NetMap { kind: NetMapKind::Archipelago, seed: 1, pristine_hash: 0 }, 7);
    let mut active = ActiveWorld::new(world, Some(host.me), SimRole::Host);
    host.start_match(&mut active, map, hasher);
    active.world.start_live();
    active.bots.push(BotBrain::new(host.me, 0.7, 3));

    let mut bots = vec![Bot::new(port, "early", 11)];
    let mut rng = SimRng::new(5);
    let t0 = now_secs();
    let mut next_tick = t0;
    let mut next_boom = t0 + 0.5;
    let mut cmds = Vec::new();
    let total = 14.0;
    let quiet_at = 9.0;
    while now_secs() - t0 < total {
        let t = now_secs() - t0;
        if t > 2.5 && bots.len() == 1 {
            bots.push(Bot::new(port, "late", 12)); // mid-match join after terrain has been carved
        }
        let quiet = t > quiet_at;
        if quiet {
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
        if now_secs() >= next_boom && !quiet {
            next_boom += 0.4;
            let s = active.world.arena.spawns[rng.below(active.world.arena.spawns.len() as u32) as usize];
            let off = bd_core::Vec3::new(rng.range(-5.0, 5.0), rng.range(-3.0, 0.0), rng.range(-5.0, 5.0));
            active.world.request_carve(CarveRequest { center: s + off, radius: rng.range(1.5, 4.0), power: 1.5 });
        }
        let mut n = 0;
        while now_secs() >= next_tick && n < 8 {
            host.pre_tick(&mut active);
            active.step(None);
            host.post_tick(&mut active);
            next_tick += DT as f64;
            n += 1;
        }
        for b in bots.iter_mut() {
            b.frame(!quiet);
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let host_hash = active.world.voxels.content_hash();
    assert!(host.stats.terrain_checks >= 8, "terrain checks {}", host.stats.terrain_checks);
    for b in &bots {
        let s = &b.core.stats;
        eprintln!("{}: {:?} rtt {:.1} ms", b.core.session.players().len(), s, b.core.rtt_ms());
        let w = b.world.as_ref().expect("client joined");
        assert_eq!(w.voxels.content_hash(), host_hash, "terrain identical to the host");
        assert_eq!(s.desyncs, 0, "no terrain desync");
        assert!(s.terrain_checks_ok >= 4, "terrain checks ok {}", s.terrain_checks_ok);
        assert!(s.snapshots > 200, "snapshots {}", s.snapshots);
        // Localhost: prediction should almost always match the host.
        assert!(s.visible_corrections <= 12, "visible corrections {}", s.visible_corrections);
    }
    assert!(host.stats.inits_sent >= 2);
}

/// Prints the cost of the expensive operations the net code schedules (run with --nocapture).
#[test]
fn measure_costs() {
    let t = now_secs();
    let settings = bd_sim::settings::GameSettings::default();
    let (mut w, _, _) = HostCore::build_world(&settings, NetMap { kind: NetMapKind::Archipelago, seed: 1, pristine_hash: 0 }, 1);
    let build = now_secs() - t;
    let t = now_secs();
    let mut h = 0;
    for _ in 0..5 {
        h ^= w.voxels.content_hash();
    }
    let hash = (now_secs() - t) / 5.0;
    let mut th = super::terrain_hash::TerrainHasher::full(&mut w.voxels);
    let op = bd_voxel::CarveOp { shape: bd_voxel::CarveShape::Sphere { center: w.arena.center, radius: 4.0 }, power: 2.0, seed: 1 };
    let _ = bd_sim::world::apply_carve_op(&mut w.voxels, &op);
    let t = now_secs();
    th.update(&mut w.voxels);
    let inc = (now_secs() - t) * 1000.0;
    let _ = w.add_player(PlayerId(0), "a".into(), Corps::Rage);
    let _ = w.add_player(PlayerId(1), "b".into(), Corps::Hope);
    for _ in 0..200 {
        w.tick(&[]);
    }
    let mut snap = bd_sim::WorldSnapshot::default();
    let mut enc = bd_sim::DeltaEncoder::new();
    let t = now_secs();
    for _ in 0..1000 {
        w.write_snapshot(Some(PlayerId(1)), &mut snap);
        let d = enc.encode(&snap);
        std::hint::black_box(bd_net::codec::encode(&d));
    }
    let snap_us = (now_secs() - t) * 1000.0;
    eprintln!("COSTS build_world {:.1} ms, content_hash {:.2} ms (h {h:x}), incremental hash after a carve {inc:.3} ms, snapshot+delta+encode {:.1} us", build * 1000.0, hash * 1000.0, snap_us);
}

/// A client whose connection dies mid-match reconnects on its own and gets its player back.
#[test]
fn reconnect_keeps_the_player() {
    let settings = bd_sim::settings::GameSettings::default();
    let mut host =
        HostCore::start(HostStart { port: 0, tunnel: false, lan: false, settings: settings.clone(), ..Default::default() }).expect("host");
    let port = host.port;
    let (world, map, hasher) = HostCore::build_world(&settings, NetMap { kind: NetMapKind::Archipelago, seed: 1, pristine_hash: 0 }, 7);
    let mut active = ActiveWorld::new(world, Some(host.me), SimRole::Host);
    host.start_match(&mut active, map, hasher);
    active.world.start_live();
    let proxy = bd_net::LagProxy::start(format!("127.0.0.1:{port}").parse().expect("addr"), bd_net::LagSettings::default()).expect("proxy");
    let mut bot = Bot::new(port, "flaky", 21);
    bot.core = ClientCore::connect(&proxy.url(), "flaky", Corps::Rage, NetProfile::default()).expect("connect");
    let mut proxy = Some(proxy);
    let t0 = now_secs();
    let mut next_tick = t0;
    let mut cmds = Vec::new();
    let mut first_id = None;
    while now_secs() - t0 < 8.0 {
        let t = now_secs() - t0;
        if t > 2.5 && proxy.is_some() {
            first_id = bot.core.me;
            assert_eq!(bot.core.phase, ClientPhase::InGame);
            // Give the player a score, then cut the wire (both sides see a lost connection).
            if let Some(p) = first_id.and_then(|id| active.world.player_mut(id)) {
                p.kos = 3;
            }
            bot.core.address = format!("127.0.0.1:{port}");
            proxy = None;
        }
        host.poll(Some(&mut active), &mut cmds);
        for (id, c) in cmds.drain(..) {
            active.apply_command(id, c);
        }
        while now_secs() >= next_tick {
            host.pre_tick(&mut active);
            active.step(None);
            host.post_tick(&mut active);
            next_tick += DT as f64;
        }
        bot.frame(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(bot.core.stats.reconnects, 1, "{:?}", bot.core.phase);
    assert_eq!(bot.core.phase, ClientPhase::InGame);
    assert_eq!(host.stats.rejoins, 1);
    let id = bot.core.me.expect("id");
    assert_eq!(Some(id), first_id, "same player id after the reconnect");
    assert_eq!(active.world.player(id).map(|p| p.kos), Some(3), "score kept");
}
