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

/// Combat tuning is host-authoritative: a joining client's replica / prediction world gets the
/// host's tuning in `WorldInit.cfg`, and a host hot reload (`HostCore::set_tuning` →
/// `S2C::Tuning`) reaches the client's world while it plays.
#[test]
fn combat_tuning_syncs_host_to_client() {
    use bd_core::mechanics::Form;
    use bd_sim::tuning::{CombatTuning, FormCost};
    let settings = bd_sim::settings::GameSettings::default();
    let mut host =
        HostCore::start(HostStart { port: 0, tunnel: false, lan: false, settings: settings.clone(), ..Default::default() }).expect("host");
    let port = host.port;
    let (world, map, hasher) = HostCore::build_world(&settings, NetMap { kind: NetMapKind::Archipelago, seed: 1, pristine_hash: 0 }, 7);
    // The host world starts from the shipped combat.ron (== defaults).
    assert_eq!(world.tuning().willpower.form_mult(Form::Gun), 2.6);
    let mut active = ActiveWorld::new(world, Some(host.me), SimRole::Host);
    // A host-side edit before anyone joins: travels in WorldInit.
    let mut at_join = CombatTuning::default();
    at_join.willpower.form_cost = vec![FormCost { form: Form::Gun, mult: 3.5 }];
    at_join.ring.blast_damage = 17.0;
    active.world.set_tuning(at_join.clone());
    host.start_match(&mut active, map, hasher);
    active.world.start_live();
    let mut bot = Bot::new(port, "tuned", 31);
    let mut reloaded = CombatTuning::default();
    reloaded.willpower.form_cost = vec![FormCost { form: Form::Gun, mult: 1.25 }, FormCost { form: Form::Staff, mult: 4.0 }];
    reloaded.charge.rate_per_sec *= 0.5;
    let t0 = now_secs();
    let mut next_tick = t0;
    let mut cmds = Vec::new();
    let mut joined_with = None;
    let mut sent_to = None;
    while now_secs() - t0 < 10.0 {
        host.poll(Some(&mut active), &mut cmds);
        cmds.clear();
        while now_secs() >= next_tick {
            host.pre_tick(&mut active);
            active.step(None);
            host.post_tick(&mut active);
            next_tick += DT as f64;
        }
        bot.frame(false);
        if let Some(w) = bot.world.as_ref() {
            if joined_with.is_none() {
                joined_with = Some(w.tuning().clone());
                // Hot reload on the host while the client plays.
                sent_to = Some(host.set_tuning(Some(&mut active.world), &reloaded));
            } else if w.tuning() == active.world.tuning() {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(joined_with.as_ref().map(|t| t.willpower.form_mult(Form::Gun)), Some(3.5), "join carries the host's tuning");
    assert_eq!(joined_with.as_ref().map(|t| t.ring.blast_damage), Some(17.0));
    assert_eq!(sent_to, Some(1));
    let w = bot.world.as_ref().expect("client joined");
    assert_eq!(active.world.tuning(), &reloaded.clone().validated());
    assert_eq!(w.tuning(), active.world.tuning(), "hot reload reached the client's world");
    assert_eq!(w.tuning().willpower.form_mult(Form::Staff), 4.0);
    assert_eq!(bot.core.stats.tuning_updates, 1);
}

/// Charge → release over the wire (owner: "it should attack on RELEASE, not require an extra
/// click"). A real client holds LMB with a sword for 0.5 s and lets go, never pressing again: the
/// host starts the heavy (`SwingStart` strength 2) on exactly the tick its input queue delivers the
/// release, and the client's presentation stream receives it.
#[test]
fn charged_heavy_releases_over_the_wire() {
    use bd_core::input::{PlayerInput, buttons};
    use bd_core::mechanics::{Element, Form};
    use bd_sim::GameEvent;
    use bd_sim::events::HeldFx;
    const HOLD: u32 = 60;
    let settings = bd_sim::settings::GameSettings::default();
    let mut host =
        HostCore::start(HostStart { port: 0, tunnel: false, lan: false, settings: settings.clone(), ..Default::default() }).expect("host");
    let port = host.port;
    let (world, map, hasher) = HostCore::build_world(&settings, NetMap { kind: NetMapKind::Archipelago, seed: 1, pristine_hash: 0 }, 7);
    let mut active = ActiveWorld::new(world, Some(host.me), SimRole::Host);
    host.start_match(&mut active, map, hasher);
    active.world.start_live();
    let mut core = ClientCore::connect(&format!("127.0.0.1:{port}"), "charger", Corps::Rage, Default::default()).expect("connect");
    let mut cw: Option<World> = None;
    let mut pacer: Option<TickPacer> = None;
    let mut last = now_secs();
    let mut armed_at: Option<f64> = None;
    let mut sent = 0u32; // client ticks predicted since the sword arrived
    let (mut host_prev, mut host_release, mut host_heavy, mut host_charge) = (false, None, Vec::new(), None);
    let mut client_heavy = 0;
    let mut cmds = Vec::new();
    let t0 = now_secs();
    let mut next_tick = t0;
    while now_secs() - t0 < 12.0 {
        host.poll(Some(&mut active), &mut cmds);
        cmds.clear();
        while now_secs() >= next_tick {
            host.pre_tick(&mut active);
            let tick = active.world.tick;
            let evs = active.step(None).to_vec();
            host.post_tick(&mut active);
            next_tick += DT as f64;
            let Some(me) = core.me else { continue };
            for e in &evs {
                if let GameEvent::HeldFx { owner, fx, strength, .. } = e
                    && *owner == me
                {
                    match fx {
                        HeldFx::SwingStart if *strength >= 1.9 => host_heavy.push(tick),
                        HeldFx::ChargeStart => host_charge = host_charge.or(Some(tick)),
                        _ => {}
                    }
                }
            }
            let held = active.world.player(me).is_some_and(|p| p.input.held(buttons::PRIMARY));
            if host_prev && !held && host_release.is_none() {
                host_release = Some(tick);
            }
            host_prev = held;
        }
        // Client frame.
        let now = now_secs();
        let out = core.update(cw.as_mut());
        if let Some(w) = out.new_world {
            cw = Some(w);
        }
        for e in core.outbox.drain(..) {
            if let GameEvent::HeldFx { owner, fx: HeldFx::SwingStart, strength, .. } = e
                && Some(owner) == core.me
                && strength >= 1.9
            {
                client_heavy += 1;
            }
        }
        let dt = now - last;
        last = now;
        if let (Some(w), ClientPhase::InGame, Some(me)) = (cw.as_mut(), &core.phase, core.me)
            && core.session.clock().is_synced()
        {
            if armed_at.is_none() && active.world.player(me).is_some_and(|p| p.alive) && w.player(me).is_some_and(|p| p.alive) {
                // Hand the client a sword (host-authoritative cast), let it form.
                let p = active.world.player(me).unwrap();
                let eye = p.eye(active.world.params());
                let r = bd_sim::bots::recipe("sword", Form::Sword, &[], Element::Light, bd_sim::constructs::form_spec(Form::Sword).default_scale);
                active.world.cast(me, r, None, bd_sim::world::Aim::new(eye, bd_core::Vec3::NEG_Z)).expect("cast");
                armed_at = Some(now + 0.6);
            }
            let target = core.session.clock().target_client_tick(now, super::client::LEAD_BUFFER_TICKS);
            let p = pacer.get_or_insert_with(|| TickPacer::new(TICK_HZ, target as u64));
            let st = p.advance(dt, Some(target));
            for _ in 0..st.count {
                let pressing = armed_at.is_some_and(|t| now >= t) && sent < HOLD;
                if armed_at.is_some_and(|t| now >= t) {
                    sent += 1;
                }
                let _ = core.predict(w, PlayerInput { buttons: if pressing { buttons::PRIMARY } else { 0 }, ..Default::default() });
            }
        }
        let _ = core.interp.update(now);
        core.release_events();
        core.send_inputs(false);
        if client_heavy > 0 && sent > HOLD + 120 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(host_charge.is_some(), "the hold coiled a charge on the host");
    let release = host_release.expect("host saw the release");
    assert!(host_charge.unwrap() < release);
    assert_eq!(host_heavy, vec![release], "one heavy, started on the release tick (no second press)");
    assert_eq!(client_heavy, 1, "the client received the heavy swing start");
}

/// The next-map vote picks the top map (random among ties) or keeps the current one.
#[test]
fn vote_tally() {
    use super::host::tally;
    let (a, b, c) = (PlayerId(0), PlayerId(1), PlayerId(2));
    assert_eq!(tally(&[], NetMapKind::Ember, 5), NetMapKind::Ember, "no votes: same map");
    assert_eq!(tally(&[(a, NetMapKind::Lair), (b, NetMapKind::Lair), (c, NetMapKind::Miami)], NetMapKind::Ember, 5), NetMapKind::Lair);
    // A tie: both sides get picked over many seeds, nothing else ever does.
    let votes = [(a, NetMapKind::Casino), (b, NetMapKind::Cybertron)];
    let picks: Vec<NetMapKind> = (0..64).map(|s| tally(&votes, NetMapKind::Ember, s)).collect();
    assert!(picks.contains(&NetMapKind::Casino) && picks.contains(&NetMapKind::Cybertron), "{picks:?}");
    assert!(picks.iter().all(|k| matches!(k, NetMapKind::Casino | NetMapKind::Cybertron)));
    // "Same map" is a valid choice.
    assert_eq!(tally(&[(a, NetMapKind::Ember)], NetMapKind::Ember, 1), NetMapKind::Ember);
}

/// Match swap over real sockets: a match ends, the lobby votes (a client picks Miami), a late
/// joiner arrives during the vote and a third client drops mid-generation; the host builds the
/// voted map on a worker and swaps it in; every remaining client regenerates the *new* map and
/// ends with the host's exact terrain. Names / corps survive, scores and stats reset, stats reach
/// the clients.
#[test]
fn match_swap_to_another_map() {
    use bd_sim::MatchPhase;
    let settings = bd_sim::settings::GameSettings::default();
    let mut host =
        HostCore::start(HostStart { port: 0, tunnel: false, lan: false, settings: settings.clone(), ..Default::default() }).expect("host");
    host.terrain_check_every = TICK_HZ as u64;
    let port = host.port;
    let (world, map, hasher) = HostCore::build_world(&settings, NetMap { kind: NetMapKind::Archipelago, seed: 1, pristine_hash: 0 }, 7);
    assert!(!world.cfg.timings.auto_rematch, "the host's vote drives the next match");
    let mut active = ActiveWorld::new(world, Some(host.me), SimRole::Host);
    host.start_match(&mut active, map, hasher);
    active.world.start_live();
    let mut bots = vec![Bot::new(port, "voter", 41), Bot::new(port, "quitter", 42)];
    let mut cmds = Vec::new();
    let t0 = now_secs();
    let mut next_tick = t0;
    let mut build: Option<std::thread::JoinHandle<(World, NetMap, super::terrain_hash::TerrainHasher)>> = None;
    let (mut ended_at, mut voted, mut swapped_at, mut stats_seen) = (None, false, None, false);
    let mut first_ids = Vec::new();
    let mut voter_before = None;
    while now_secs() - t0 < 40.0 {
        let t = now_secs() - t0;
        host.poll(Some(&mut active), &mut cmds);
        cmds.clear();
        while now_secs() >= next_tick {
            host.pre_tick(&mut active);
            active.step(None);
            host.post_tick(&mut active);
            next_tick += DT as f64;
        }
        let all_in = bots.iter().all(|b| b.core.phase == ClientPhase::InGame && b.world.is_some());
        // 1. Everyone in: score something, then end the match.
        if ended_at.is_none() && all_in && t > 2.0 {
            first_ids = bots.iter().filter_map(|b| b.core.me).collect();
            if let Some(p) = active.world.player_mut(first_ids[0]) {
                p.kos = 4;
                voter_before = Some((p.name.clone(), p.corps));
            }
            active.world.match_state.phase = MatchPhase::Ended;
            ended_at = Some(t);
        }
        // 2. The vote is open on the clients: vote, and a late joiner arrives.
        if ended_at.is_some() && !voted && bots[0].core.lobby.vote.is_some() {
            bots[0].core.vote_map(NetMapKind::Miami);
            bots.push(Bot::new(port, "late", 43));
            voted = true;
        }
        // 3. The vote is in (and the joiner in the old world): the host says go.
        if voted && build.is_none() && swapped_at.is_none() && host.vote.as_ref().is_some_and(|v| !v.votes.is_empty()) && bots[2].world.is_some() {
            host.vote_now();
        }
        if host.vote_due() && build.is_none() && swapped_at.is_none() {
            let nm = NetMap { kind: host.next_map(9), seed: crate::arena::ARENA_SEED, pristine_hash: 0 };
            assert_eq!(nm.kind, NetMapKind::Miami, "the voted map");
            let s = host.settings.clone();
            build = Some(std::thread::spawn(move || HostCore::build_world(&s, nm, 11)));
            // A client drops while the next map generates.
            bots[1].core.leave();
        }
        if build.as_ref().is_some_and(|b| b.is_finished()) {
            let (w, m, th) = build.take().unwrap().join().expect("build");
            super::vote::swap_world(&mut active, w);
            host.start_match(&mut active, m, th);
            swapped_at = Some(now_secs());
        }
        for (k, b) in bots.iter_mut().enumerate() {
            if k != 1 || swapped_at.is_none() {
                b.frame(false);
            }
        }
        if swapped_at.is_some() {
            stats_seen |= bots[0].core.lobby.stats.len() >= 2;
        }
        // 4. Done once the survivors run the new map and a few terrain checks passed.
        let done = swapped_at.is_some_and(|s| now_secs() - s > 4.0)
            && [0, 2].iter().all(|&k| bots[k].core.map.kind == NetMapKind::Miami && bots[k].core.phase == ClientPhase::InGame && bots[k].world.is_some());
        if done {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(swapped_at.is_some(), "the next match started");
    assert_eq!(host.map.kind, NetMapKind::Miami);
    assert!(host.vote.is_none(), "vote closed");
    let host_hash = active.world.voxels.content_hash();
    let host_spawns = active.world.arena.spawns.clone();
    for k in [0, 2] {
        let b = &bots[k];
        let w = b.world.as_ref().expect("in the new match");
        assert_eq!(b.core.map.kind, NetMapKind::Miami, "client {k} on the voted map");
        assert_eq!(w.voxels.content_hash(), host_hash, "client {k}: terrain identical to the host");
        assert_eq!(w.arena.spawns, host_spawns, "client {k}: same arena");
        assert_eq!(b.core.stats.desyncs, 0, "client {k}: no desync");
        assert!(b.core.stats.terrain_checks_ok >= 1, "client {k}: terrain checks ok");
    }
    // The dropped client is gone; the others (and the host) are in, names kept, scores reset.
    let quitter = bots[1].core.me.expect("quitter id");
    assert!(active.world.player(quitter).is_none(), "the client that dropped mid-generation is not in the new match");
    for id in [first_ids[0], bots[2].core.me.expect("late id")] {
        assert!(active.world.player(id).is_some(), "{id:?} in the new match");
    }
    let voter = active.world.player(first_ids[0]).expect("voter");
    assert_eq!(Some((voter.name.clone(), voter.corps)), voter_before, "name / corps carry over");
    assert_eq!(voter.kos, 0, "scores reset");
    assert_eq!(active.world.match_state.phase, MatchPhase::Warmup, "a fresh match");
    assert!(stats_seen, "stats reached the client");
}

/// Saying a technique on a client: the host answers the cast with `CastOutcome::Became` (a power,
/// no construct), so the client's provisional cast resolves at once instead of lingering as a
/// small orb until the 2 s timeout.
#[test]
fn learning_a_technique_on_a_client_resolves_the_cast_as_a_power() {
    use super::proto::CastOutcome;
    use bd_core::mechanics::{Element, Form};
    let settings = bd_sim::settings::GameSettings::default();
    let mut host =
        HostCore::start(HostStart { port: 0, tunnel: false, lan: false, settings: settings.clone(), ..Default::default() }).expect("host");
    let port = host.port;
    let (world, map, hasher) = HostCore::build_world(&settings, NetMap { kind: NetMapKind::Archipelago, seed: 1, pristine_hash: 0 }, 7);
    let mut active = ActiveWorld::new(world, Some(host.me), SimRole::Host);
    host.start_match(&mut active, map, hasher);
    active.world.start_live();
    let mut core = ClientCore::connect(&format!("127.0.0.1:{port}"), "saiyan", Corps::Rage, Default::default()).expect("connect");
    let ssj = bd_sim::powers::PowerTable::get().index_of("super_saiyan").expect("super_saiyan");
    let mut cw: Option<World> = None;
    let mut cmds = Vec::new();
    let mut sent: Option<u32> = None;
    let mut outcome = None;
    let t0 = now_secs();
    let mut next_tick = t0;
    while now_secs() - t0 < 10.0 && outcome.is_none() {
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
        let out = core.update(cw.as_mut());
        if let Some(w) = out.new_world {
            cw = Some(w);
        }
        core.outbox.clear();
        for (seq, o) in core.cast_results.drain(..) {
            if Some(seq) == sent {
                outcome = Some(o);
            }
        }
        if let (None, Some(w), ClientPhase::InGame, Some(me)) = (sent, cw.as_ref(), &core.phase, core.me)
            && core.session.clock().is_synced()
            && active.world.player(me).is_some_and(|p| p.alive)
            && now_secs() - t0 > 1.0
        {
            if let Some(p) = active.world.player_mut(me) {
                p.willpower = 100.0;
            }
            let p = w.player(me).expect("me");
            let aim = bd_sim::Aim::new(p.eye(w.params()), bd_core::Vec3::NEG_Z);
            let mut r = bd_sim::bots::recipe("super saiyan", Form::Ball, &[], Element::Light, 0.35);
            r.grants = Some("super_saiyan".into());
            sent = core.send_cmd(&Cmd::Cast { recipe: Box::new(r), shape: None, alt_shape: None, aim, incantation: false });
            assert!(sent.is_some(), "a cast has a seq");
        }
        core.release_events();
        core.send_inputs(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let me = core.me.expect("joined");
    assert!(active.world.player(me).is_some_and(|p| p.power.slot_of(ssj).is_some()), "the host granted the technique");
    assert_eq!(outcome, Some(Some(CastOutcome::Became(ssj))), "the client's cast became a power (not a timeout / failure)");
}
