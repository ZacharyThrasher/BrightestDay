//! Map swaps with a host + two predicting clients (playtest bug: "when swapping to a new map my
//! friends spawned inside objects and needed to rejoin to get out").
//!
//! The host plays every map in turn (the end-of-match vote → `build_world` on a worker →
//! `swap_world` + `start_match`, exactly as `vote::host_vote_flow` / `match_flow` do). After each
//! swap, from the host's view *and* each client's view: nobody is inside solid terrain / static map
//! colliders, spawns stand on ground, and the clients' inputs actually drive their players (the
//! host consumes them and the client's prediction agrees with the host).

use bd_core::corps::Corps;
use bd_core::{DT, PlayerId, TICK_HZ, Vec3};
use bd_net::{TickPacer, now_secs};
use bd_sim::World;
use bd_sim::bots::BotBrain;
use bd_sim::world::unstick::UNSTICK_COOLDOWN;

use super::client::{ClientCore, ClientPhase};
use super::host::{HostCore, HostStart};
use super::proto::{NetMap, NetMapKind, NetProfile};
use crate::sim_bridge::{ActiveWorld, SimRole};

struct Peer {
    core: ClientCore,
    world: Option<World>,
    brain: Option<BotBrain>,
    pacer: Option<TickPacer>,
    last: f64,
    seed: u64,
    /// `GameEvent::Unstuck` of our own player seen on this client (arrival spots).
    unstuck: Vec<Vec3>,
}

impl Peer {
    fn new(port: u16, name: &str, seed: u64) -> Self {
        let core = ClientCore::connect(&format!("127.0.0.1:{port}"), name, Corps::Rage, NetProfile::default()).expect("connect");
        Self { core, world: None, brain: None, pacer: None, last: now_secs(), seed, unstuck: Vec::new() }
    }

    /// One client frame: network in, predicted ticks (a bot drives the input), inputs out.
    fn frame(&mut self) {
        let now = now_secs();
        let out = self.core.update(self.world.as_mut());
        if let Some(w) = out.new_world {
            self.world = Some(w);
        }
        for e in self.core.outbox.drain(..) {
            if let bd_sim::GameEvent::Unstuck { player, to, auto: false, .. } = e {
                if Some(player) == self.core.me {
                    self.unstuck.push(to);
                }
            }
        }
        self.core.cast_results.clear();
        let dt = now - self.last;
        self.last = now;
        let (Some(w), ClientPhase::InGame, Some(me)) = (self.world.as_mut(), &self.core.phase, self.core.me) else { return };
        let b = self.brain.get_or_insert_with(|| BotBrain::new(me, 0.7, self.seed));
        b.casts_enabled = false;
        if !self.core.session.clock().is_synced() {
            return;
        }
        let target = self.core.session.clock().target_client_tick(now, super::client::LEAD_BUFFER_TICKS);
        let p = self.pacer.get_or_insert_with(|| TickPacer::new(TICK_HZ, target as u64));
        let st = p.advance(dt, Some(target));
        for _ in 0..st.count {
            let act = b.think(w);
            let _ = self.core.predict(w, act.input);
        }
        let _ = self.core.interp.update(now);
        self.core.release_events();
        self.core.send_inputs(false);
    }

    fn in_map(&self, kind: NetMapKind) -> bool {
        self.core.map.kind == kind && self.core.phase == ClientPhase::InGame && self.world.is_some()
    }
}

/// Solid voxels / static map colliders inside a player's hull (the sim's own test).
fn embedded(w: &World, id: PlayerId) -> bool {
    w.player_embedded(id)
}

struct Harness {
    host: HostCore,
    active: ActiveWorld,
    peers: Vec<Peer>,
    next_tick: f64,
}

impl Harness {
    fn start(first: NetMapKind) -> Self {
        // Every map on the cards (the swap goes through all of them in a fixed order).
        let settings = bd_sim::settings::GameSettings { vote: bd_sim::settings::MapVoteRules::open_all(), ..Default::default() };
        let mut host =
            HostCore::start(HostStart { port: 0, tunnel: false, lan: false, settings: settings.clone(), ..Default::default() }).expect("host");
        let port = host.port;
        let (world, map, hasher) = HostCore::build_world(&settings, NetMap { kind: first, seed: crate::arena::ARENA_SEED, pristine_hash: 0 }, 7);
        let mut active = ActiveWorld::new(world, Some(host.me), SimRole::Host);
        host.start_match(&mut active, map, hasher);
        active.world.start_live();
        active.bots.push(BotBrain::new(host.me, 0.7, 3));
        let peers = vec![Peer::new(port, "alpha", 51), Peer::new(port, "bravo", 52)];
        Self { host, active, peers, next_tick: now_secs() }
    }

    /// One loop iteration: host net + due ticks, every client frame.
    fn pump(&mut self) {
        let mut cmds = Vec::new();
        self.host.poll(Some(&mut self.active), &mut cmds);
        for (id, c) in cmds {
            self.active.apply_command(id, c);
        }
        while now_secs() >= self.next_tick {
            self.host.pre_tick(&mut self.active);
            self.active.step(None);
            self.host.post_tick(&mut self.active);
            self.next_tick += DT as f64;
        }
        for p in &mut self.peers {
            p.frame();
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    fn pump_for(&mut self, secs: f64) {
        let t = now_secs();
        while now_secs() - t < secs {
            self.pump();
        }
    }

    fn pump_until(&mut self, secs: f64, mut f: impl FnMut(&Self) -> bool) -> bool {
        let t = now_secs();
        while now_secs() - t < secs {
            if f(self) {
                return true;
            }
            self.pump();
        }
        f(self)
    }

    /// End the match, vote for `kind`, build it on a worker while everyone keeps playing, swap.
    fn swap_to(&mut self, kind: NetMapKind) {
        self.active.world.match_state.phase = bd_sim::MatchPhase::Ended;
        assert!(self.pump_until(5.0, |h| h.host.vote.is_some()), "vote opened");
        let me = self.host.me;
        self.host.cast_vote(me, kind);
        self.host.vote_now();
        let nm = NetMap { kind: self.host.next_map(1), seed: crate::arena::ARENA_SEED, pristine_hash: 0 };
        assert_eq!(nm.kind, kind);
        let s = self.host.settings.clone();
        let build = std::thread::spawn(move || HostCore::build_world(&s, nm, 11));
        while !build.is_finished() {
            self.pump();
        }
        let (w, m, th) = build.join().expect("build");
        super::vote::swap_world(&mut self.active, w);
        self.host.start_match(&mut self.active, m, th);
    }
}

/// Every map in turn, then back the other way: after each swap nobody is embedded (host view and
/// client views), spawns stand on ground, and each client's inputs drive its player.
#[test]
fn swapping_through_every_map_never_embeds_anyone() {
    let mut order: Vec<NetMapKind> = vec![
        NetMapKind::Casino,
        NetMapKind::Lair,
        NetMapKind::Cybertron,
        NetMapKind::GiantsRoom,
        NetMapKind::Valley,
    ];
    for k in NetMapKind::ALL {
        if k != NetMapKind::Archipelago && !order.contains(&k) {
            order.push(k);
        }
    }
    // …and back the other way.
    order.extend([NetMapKind::Valley, NetMapKind::GiantsRoom, NetMapKind::Cybertron, NetMapKind::Lair, NetMapKind::Casino, NetMapKind::Archipelago]);
    let mut h = Harness::start(NetMapKind::Archipelago);
    assert!(h.pump_until(15.0, |h| h.peers.iter().all(|p| p.in_map(NetMapKind::Archipelago))), "clients joined");
    h.pump_for(2.0);
    let mut failures = Vec::new();
    for kind in order {
        h.swap_to(kind);
        // Right after the swap (tick 1 respawns): every spawn is clear and on ground.
        h.pump_until(1.0, |h| h.active.world.players.iter().all(|p| p.alive));
        for p in &h.active.world.players {
            if embedded(&h.active.world, p.id) {
                failures.push(format!("{kind:?}: host view: {:?} spawned embedded at {:?}", p.id, p.mv.pos));
            }
            if !h.active.world.spawn_clear(p.last_safe_pos) {
                failures.push(format!("{kind:?}: host view: {:?} spawn {:?} not clear / no ground", p.id, p.last_safe_pos));
            }
        }
        let joined = h.pump_until(20.0, |h| h.peers.iter().all(|p| p.in_map(kind)));
        assert!(joined, "{kind:?}: clients loaded the new map");
        // Play a while (bots run, jump, hook), checking every frame from every view.
        let t0 = now_secs();
        let mut seq0: Vec<u32> = Vec::new();
        while now_secs() - t0 < 3.0 {
            h.pump();
            let hw = &h.active.world;
            for p in hw.players.iter().filter(|p| p.alive) {
                if embedded(hw, p.id) && !failures.iter().any(|f| f.starts_with(&format!("{kind:?}: host"))) {
                    failures.push(format!("{kind:?}: host view: {:?} embedded at {:?}", p.id, p.mv.pos));
                }
            }
            for (k, peer) in h.peers.iter().enumerate() {
                let (Some(w), Some(me)) = (peer.world.as_ref(), peer.core.me) else { continue };
                for p in w.players.iter().filter(|p| p.alive) {
                    if embedded(w, p.id) && !failures.iter().any(|f| f.starts_with(&format!("{kind:?}: client {k}"))) {
                        let who = if p.id == me { "own" } else { "remote" };
                        failures.push(format!("{kind:?}: client {k} view: {who} {:?} embedded at {:?}", p.id, p.mv.pos));
                    }
                }
            }
            if seq0.is_empty() && now_secs() - t0 > 0.5 {
                seq0 = h.peers.iter().map(|p| p.core.me.and_then(|id| hw.player(id)).map_or(0, |p| p.last_input_seq)).collect();
            }
        }
        // The host consumed each client's new inputs (a stale sequence number from the old match
        // must not lock the fresh one out) and prediction agrees with the host.
        for (k, peer) in h.peers.iter().enumerate() {
            let me = peer.core.me.expect("id");
            let host_p = h.active.world.player(me).expect("in the host world");
            if host_p.last_input_seq <= seq0[k] || host_p.last_input_seq > peer.core.inputs.next_seq() {
                failures.push(format!(
                    "{kind:?}: client {k}: host not consuming its inputs (host seq {} → {}, client next {})",
                    seq0[k],
                    host_p.last_input_seq,
                    peer.core.inputs.next_seq()
                ));
            }
            let cw = peer.world.as_ref().expect("world");
            if let (Some(cp), true) = (cw.player(me), host_p.alive) {
                // The client runs ahead of the host by its lead (RTT + jitter buffer): allow that
                // much travel at the current speed. A stale / stuck start is metres off and stays.
                let d = (cp.mv.pos - host_p.mv.pos).length();
                if d > 1.0 + host_p.mv.vel.length().max(cp.mv.vel.length()) * 0.25 {
                    failures.push(format!("{kind:?}: client {k}: prediction {:?} is {d:.1} m from the host's {:?}", cp.mv.pos, host_p.mv.pos));
                }
            }
        }
        eprintln!("swap → {kind:?}: {} failure(s) so far", failures.len());
    }
    assert!(failures.is_empty(), "{} problem(s):\n{}", failures.len(), failures.join("\n"));
    let _ = Vec3::ZERO;
}

/// Every spawn of every map is clear (hull out of terrain / static colliders, ground under the
/// feet) — what `pick_spawn` / `safest_spawn` rely on for fresh starts, swaps and unsticks.
#[test]
fn every_map_spawn_is_clear() {
    let mut bad = Vec::new();
    for kind in NetMapKind::ALL {
        let (voxels, info, seed) = super::proto::build_terrain(kind, crate::arena::ARENA_SEED);
        let w = World::with_arena(bd_sim::SimConfig::default(), voxels, info, seed);
        assert!(!w.arena.spawns.is_empty(), "{kind:?} has spawns");
        for (k, s) in w.arena.spawns.iter().enumerate() {
            if !w.spawn_clear(*s) {
                bad.push(format!("{kind:?} spawn {k} at {s:?}"));
            }
        }
    }
    assert!(bad.is_empty(), "spawns not clear:\n{}", bad.join("\n"));
}

/// The UNSTICK button on a client: the command reaches the host, which moves the player to the
/// safest spawn (the client sees `Unstuck` and ends up there); pressed again at once it queues on
/// the host instead of being refused.
#[test]
fn a_client_unstick_request_goes_through_the_host() {
    let mut h = Harness::start(NetMapKind::Archipelago);
    assert!(h.pump_until(15.0, |h| h.peers.iter().all(|p| p.in_map(NetMapKind::Archipelago))), "clients joined");
    h.pump_for(1.0);
    let me = h.peers[0].core.me.expect("id");
    let pct = h.active.world.player(me).expect("in the host world").damage_pct;
    h.peers[0].core.send_cmd(&crate::sim_bridge::Cmd::Unstick);
    assert!(h.pump_until(3.0, |h| !h.peers[0].unstuck.is_empty()), "the host's Unstuck reached the client");
    let to = h.peers[0].unstuck[0];
    assert!(h.active.world.arena.spawns.iter().any(|s| (*s - to).length() < 1e-3), "a spawn of this map");
    let p = h.active.world.player(me).expect("me");
    assert!(p.unstick.cd > UNSTICK_COOLDOWN - 3.0, "cooldown running");
    assert_eq!(p.damage_pct, pct, "no % change");
    // Again straight away: queued on the host (never refused), nothing fires yet.
    h.peers[0].core.send_cmd(&crate::sim_bridge::Cmd::Unstick);
    assert!(h.pump_until(2.0, |h| h.active.world.player(me).is_some_and(|p| p.unstick.queued)), "queued");
    assert_eq!(h.peers[0].unstuck.len(), 1, "not before the cooldown");
}
