//! Host core (no Bevy): the listen server around an authoritative [`ActiveWorld`].
//!
//! Per frame: [`HostCore::poll`] (joins / leaves / lobby / inputs / commands). Per tick:
//! [`HostCore::pre_tick`] in `SimSet::Input` (one input per remote player from its jitter queue +
//! lag-comp view tick) and [`HostCore::post_tick`] in `SimSet::Post` (terrain log, events, per-client
//! delta snapshots at 60 Hz, periodic terrain hash).
//!
//! Late joiners / resyncs get `WorldInit` (config + arena seed + construct manifest) and the
//! terrain log (every carve / extract / restore since the arena was generated) so they rebuild
//! bit-identical voxels; `TerrainCheck` hashes prove it.

use std::net::{IpAddr, Ipv4Addr};

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::{MAX_PLAYERS, PlayerId, TICK_HZ, Tick};
use bd_net::{
    DEFAULT_PORT, HostConfig, HostEvent, HostSession, InputQueue, LeaveReason, LocalPlayer, NetError, Tunnel, TunnelStatus,
    clamp_view_tick, now_secs,
};
use bd_sim::replicate::{DeltaEncoder, WorldSnapshot};
use bd_sim::settings::GameSettings;
use bd_sim::{GameEvent, World};

use super::proto::*;
use super::terrain_hash::TerrainHasher;
use crate::sim_bridge::{ActiveWorld, Cmd};

/// Snapshots every N ticks (2 → 60 Hz).
pub const SNAPSHOT_EVERY: u64 = 2;
/// Terrain hash every N ticks (incremental: only chunks edited since the last check are rehashed).
pub const TERRAIN_CHECK_EVERY: u64 = 2 * TICK_HZ as u64;
/// Lag compensation never rewinds further than this (ticks).
pub const MAX_REWIND_TICKS: f64 = 0.25 * TICK_HZ as f64;
/// Terrain log events per init message.
const LOG_CHUNK: usize = 1024;
/// Host jitter buffer: a starved tick (late input) and a trimmed input (queue too deep) each cost
/// the client a correction, so keep a little more slack than the minimum (+1 tick ≈ 8 ms) and
/// trim slowly.
fn input_queue() -> InputQueue<PlayerInput> {
    let mut q = InputQueue::new(3);
    q.slack = 2;
    q.window = 480;
    q
}
/// How long a dropped player's slot (score) is kept for a reconnect.
pub const REJOIN_SECS: f64 = 90.0;
/// A joiner that never sends its token enters the match anyway after this long.
const TOKEN_WAIT_SECS: f64 = 1.5;
/// Commands per second a client may issue (burst = 2×).
const CMD_RATE: f32 = 20.0;
/// Seconds the lobby votes on the next map once a match is over (the host can start it sooner).
pub const VOTE_SECS: f32 = 15.0;
/// Stats refresh cadence (ticks) while a match runs (the lobby carries them to clients).
const STATS_EVERY: u64 = 60;

/// The vote for the next match (open while the match is over; rules in `net::map_vote`).
pub use super::map_vote::VoteState;

/// Top-voted map; ties broken by `seed`; no votes = the current map again.
pub fn tally(votes: &[(PlayerId, NetMapKind)], current: NetMapKind, seed: u64) -> NetMapKind {
    let mut count = [0u32; NetMapKind::ALL.len()];
    for (_, k) in votes {
        count[k.index()] += 1;
    }
    let best = count.iter().copied().max().unwrap_or(0);
    if best == 0 {
        return current;
    }
    let tied: Vec<NetMapKind> = NetMapKind::ALL.into_iter().filter(|k| count[k.index()] == best).collect();
    // splitmix64: any seed spreads evenly over the tied maps.
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    tied[(z % tied.len() as u64) as usize]
}

/// Is the match over (FFA results / party podium)? The vote runs then.
pub fn match_over(w: &World) -> bool {
    match w.party() {
        Some(p) => p.phase == bd_sim::party::PartyPhase::PartyEnd,
        None => w.match_state.phase == bd_sim::MatchPhase::Ended,
    }
}

#[derive(Clone, Debug)]
pub struct HostStart {
    pub port: u16,
    pub tunnel: bool,
    /// Listen on all interfaces (LAN `ip:port`). Off = 127.0.0.1 only (tunnel only, no firewall prompt).
    pub lan: bool,
    pub name: String,
    pub corps: Corps,
    pub profile: NetProfile,
    pub settings: GameSettings,
    /// Lobby size including the host (server-side bots take the top ids).
    pub max_players: usize,
}

impl Default for HostStart {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            tunnel: true,
            lan: true,
            name: "Host".into(),
            corps: Corps::Willpower,
            profile: NetProfile::default(),
            settings: GameSettings::default(),
            max_players: MAX_PLAYERS,
        }
    }
}

struct Remote {
    id: PlayerId,
    queue: InputQueue<PlayerInput>,
    enc: DeltaEncoder,
    /// Has a player in the world (a match is running and it was initialised).
    in_world: bool,
    cmd_tokens: f32,
    last_cmd_refill: f64,
    last_keyframe: f64,
    last_resync: f64,
    ready: bool,
    profile: NetProfile,
    /// Casts sent to the sim, waiting for their spawn / failure event: (client cast seq, when).
    pending_casts: std::collections::VecDeque<(u32, f64)>,
    /// Reconnect token (None until the client sent it).
    token: Option<u64>,
    joined_at: f64,
    name: String,
    corps: Corps,
    /// World generation sent in the last `WorldInit` (inputs of older worlds are dropped).
    epoch: u32,
}

#[derive(Clone, Debug, Default)]
pub struct HostStats {
    pub snapshots_sent: u64,
    pub events_sent: u64,
    pub terrain_checks: u64,
    pub inits_sent: u64,
    pub keyframes: u64,
    pub resyncs: u64,
    pub cmds_dropped: u64,
    /// Reconnects that got their old player back.
    pub rejoins: u64,
    /// Sum over clients of host input-queue starvation (ticks).
    pub input_starved: u64,
    pub last_terrain_hash: u64,
    /// Main-thread time spent in net code (ms, smoothed).
    pub cost_ms: f32,
}

#[allow(dead_code)] // name / corps: for the UI
pub struct HostCore {
    pub session: HostSession<C2S, S2C>,
    pub tunnel: Option<Tunnel>,
    remotes: Vec<Remote>,
    pub me: PlayerId,
    pub name: String,
    pub corps: Corps,
    pub profile: NetProfile,
    pub ready: bool,
    pub settings: GameSettings,
    pub in_match: bool,
    /// The match map (set by [`HostCore::start_match`] from the world build).
    pub map: NetMap,
    lobby_dirty: bool,
    last_lobby_sent: f64,
    terrain_log: Vec<GameEvent>,
    ev_buf: Vec<GameEvent>,
    snap: WorldSnapshot,
    terrain: TerrainHasher,
    /// Dropped players kept for a reconnect: (token, player, when).
    departed: Vec<(u64, bd_sim::world::DepartedPlayer, f64)>,
    pub stats: HostStats,
    /// Terrain hash after every tick? (tests: prove convergence quickly).
    pub terrain_check_every: u64,
    pub port: u16,
    pub lan: bool,
    /// Shouts received from remote players (drained by `present::shout`, which relays them).
    pub shouts_in: Vec<(PlayerId, crate::present::shout::ShoutLine)>,
    /// Next-map vote (Some while the match is over).
    pub vote: Option<VoteState>,
    /// Which maps have been played / offered this session (fair next-map offers).
    pub rotation: super::map_vote::Rotation,
    /// Competitive stats of the match (observed every tick; sent in the lobby info).
    pub match_stats: bd_sim::match_stats::MatchStats,
    pub stats_lines: Vec<bd_sim::match_stats::StatLine>,
}

impl HostCore {
    pub fn start(s: HostStart) -> Result<Self, NetError> {
        let bind = if s.lan { IpAddr::V4(Ipv4Addr::UNSPECIFIED) } else { IpAddr::V4(Ipv4Addr::LOCALHOST) };
        let session = HostSession::start(HostConfig {
            bind,
            port: s.port,
            local_player: Some(LocalPlayer { name: s.name.clone(), corps: s.corps }),
            max_players: s.max_players.clamp(1, MAX_PLAYERS),
            ..Default::default()
        })?;
        let port = session.port();
        let tunnel = s.tunnel.then(|| Tunnel::start(port));
        let me = PlayerId(0);
        let (name, corps) = session.player(me).map(|p| (p.name.clone(), p.corps)).unwrap_or((s.name, s.corps));
        Ok(Self {
            session,
            tunnel,
            remotes: Vec::new(),
            me,
            name,
            corps,
            profile: s.profile.sanitized(),
            ready: true,
            settings: s.settings.validated(),
            in_match: false,
            map: NetMap::default(),
            lobby_dirty: true,
            last_lobby_sent: 0.0,
            terrain_log: Vec::with_capacity(1024),
            ev_buf: Vec::with_capacity(256),
            snap: WorldSnapshot::default(),
            terrain: TerrainHasher::default(),
            departed: Vec::new(),
            stats: HostStats::default(),
            terrain_check_every: TERRAIN_CHECK_EVERY,
            port,
            lan: s.lan,
            shouts_in: Vec::new(),
            vote: None,
            rotation: super::map_vote::Rotation::new(now_secs().to_bits() ^ u64::from(port)),
            match_stats: Default::default(),
            stats_lines: Vec::new(),
        })
    }

    pub fn tunnel_status(&self) -> Option<TunnelStatus> {
        self.tunnel.as_ref().map(|t| t.status())
    }

    pub fn lobby_code(&self) -> Option<String> {
        match self.tunnel_status()? {
            TunnelStatus::Ready { code, .. } => Some(code),
            _ => None,
        }
    }

    /// LAN addresses friends on the same network can type (`ip:port`).
    pub fn lan_addresses(&self) -> Vec<String> {
        if !self.lan {
            return Vec::new();
        }
        // Routing-table trick: a connected UDP socket reveals the primary local IP without sending.
        let ip = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("10.255.255.255:9").map(|_| s))
            .and_then(|s| s.local_addr())
            .ok()
            .map(|a| a.ip())
            .filter(|ip| !ip.is_unspecified() && !ip.is_loopback());
        let mut out: Vec<String> = ip.into_iter().map(|ip| format!("{ip}:{}", self.port)).collect();
        out.push(format!("127.0.0.1:{}", self.port));
        out
    }

    pub fn lobby(&self) -> LobbyInfo {
        let mut players = Vec::with_capacity(MAX_PLAYERS);
        for p in self.session.players() {
            let (ready, profile, is_host) = if p.id == self.me {
                (self.ready, self.profile.clone(), true)
            } else {
                match self.remotes.iter().find(|r| r.id == p.id) {
                    Some(r) => (r.ready, r.profile.clone(), false),
                    None => (false, NetProfile::default(), false),
                }
            };
            let rtt_ms = self.session.rtt(p.id).map_or(0, |r| (r * 1000.0).clamp(0.0, 9999.0) as u16);
            players.push(LobbyPlayer { id: p.id, name: p.name.clone(), corps: p.corps, ready, is_host, profile, rtt_ms });
        }
        let vote = self.vote.as_ref().map(|v| v.view_for(self.me, now_secs()));
        LobbyInfo { players, settings: self.settings.clone(), map: self.map, in_match: self.in_match, vote, stats: self.stats_lines.clone() }
    }

    /// `player` picks the map card `kind` for the next match (see [`HostCore::cast_pick`]).
    pub fn cast_vote(&mut self, player: PlayerId, kind: NetMapKind) -> bool {
        self.cast_pick(player, VotePick::Map(kind))
    }

    /// `player`'s next-map pick. Ignored unless a vote is open, the pick is on offer and (votes
    /// locked) the player hasn't voted yet. Returns whether it counted.
    pub fn cast_pick(&mut self, player: PlayerId, pick: VotePick) -> bool {
        let Some(v) = self.vote.as_mut() else { return false };
        let ok = v.cast(player, pick);
        self.lobby_dirty |= ok;
        ok
    }

    /// Host tool (dev auto-swap): the next match is on `kind`, now — no vote needed.
    pub fn force_next(&mut self, kind: NetMapKind) {
        if let Some(v) = self.vote.as_mut() {
            v.result = Some((kind, false));
            v.deadline = v.deadline.min(now_secs());
            self.lobby_dirty = true;
        }
    }

    /// The vote's time is up (or the host said go): build the next match now.
    pub fn vote_due(&self) -> bool {
        self.vote.as_ref().is_some_and(|v| now_secs() >= v.deadline)
    }

    /// Start the next match now (host click).
    pub fn vote_now(&mut self) {
        if let Some(v) = self.vote.as_mut() {
            v.deadline = v.deadline.min(now_secs());
            self.lobby_dirty = true;
        }
    }

    /// Keep at least `secs` on the clock (the host is changing the rules).
    pub fn hold_vote(&mut self, secs: f64) {
        if let Some(v) = self.vote.as_mut() {
            let at = now_secs() + secs;
            if v.deadline < at {
                v.deadline = at;
                self.lobby_dirty = true;
            }
        }
    }

    /// The map the next match is played on for the votes so far (`map_vote`: top pick, fair roll
    /// among ties, RANDOM rolled off the cards; nobody voted = a roll among the cards).
    pub fn next_map(&self, seed: u64) -> NetMapKind {
        self.vote.as_ref().map_or(self.map.kind, |v| v.decide(seed).0)
    }

    /// The vote is due: decide it once. `Some(map)` = build it now; `None` while RANDOM's roulette
    /// reveal plays (the countdown moves on; call again when due).
    pub fn settle_vote(&mut self, seed: u64) -> Option<NetMapKind> {
        let v = self.vote.as_mut()?;
        let first = v.result.is_none();
        let r = v.settle(seed, now_secs());
        self.lobby_dirty |= first;
        r
    }

    pub fn set_settings(&mut self, s: GameSettings) {
        self.settings = s.validated();
        self.lobby_dirty = true;
    }

    pub fn set_profile(&mut self, p: NetProfile) {
        self.profile = p.sanitized();
        self.lobby_dirty = true;
    }

    pub fn set_ready(&mut self, r: bool) {
        self.ready = r;
        self.lobby_dirty = true;
    }



    /// Build the world for a new match (call on a worker: ~150 ms+). Returns the world and the map
    /// with its pristine terrain hash filled in (hand it to [`HostCore::start_match`]).
    pub fn build_world(settings: &GameSettings, map: NetMap, sim_seed: u64) -> (World, NetMap, TerrainHasher) {
        // `assets/tuning/combat.ron` (clients get it in `WorldInit.cfg`, hot reloads via `set_tuning`).
        let base = bd_sim::SimConfig {
            arena: bd_sim::ArenaSource::Archipelago { seed: map.seed },
            seed: sim_seed,
            tuning: crate::combat_tuning::current(),
            ..Default::default()
        };
        let mut cfg = settings.sim_config(base);
        // The next match comes from the host's vote (a fresh world), not the sim's own timer.
        cfg.timings.auto_rematch = false;
        cfg.timings.ended_secs = VOTE_SECS;
        if map.kind != NetMapKind::Archipelago {
            // The sim can only regenerate archipelagos between rounds.
            cfg.rules.regen_islands_between_rounds = false;
        }
        let (voxels, info, seed) = build_terrain(map.kind, map.seed);
        let mut voxels = voxels;
        let pristine_hash = voxels.content_hash();
        let hasher = TerrainHasher::full(&mut voxels);
        (World::with_arena(cfg, voxels, info, seed), NetMap { pristine_hash, ..map }, hasher)
    }

    /// A match world is live: put everyone in it and initialise every client.
    ///
    /// Also starts every following match (the vote's pick, on a fresh world swapped into
    /// `active`): scores, stats and saved reconnect slots belong to the old match and are dropped;
    /// names, corps and costumes live in the session / lobby and carry over.
    pub fn start_match(&mut self, active: &mut ActiveWorld, map: NetMap, terrain: TerrainHasher) {
        self.in_match = true;
        self.terrain = terrain;
        self.map = map;
        self.rotation.played(map.kind);
        self.terrain_log.clear();
        self.ev_buf.clear();
        self.vote = None;
        self.departed.clear();
        self.match_stats = Default::default();
        self.stats_lines.clear();
        for r in &mut self.remotes {
            r.pending_casts.clear();
        }
        let players: Vec<(PlayerId, String, Corps)> = self.session.players().iter().map(|p| (p.id, p.name.clone(), p.corps)).collect();
        for (id, name, corps) in players {
            if active.world.player(id).is_none() {
                let _ = active.world.add_player(id, name, corps);
            }
        }
        for i in 0..self.remotes.len() {
            self.init_remote(i, active);
        }
        self.lobby_dirty = true;
    }

    /// Combat tuning hot reload: apply it to the match world (if any) and send it to every
    /// connected client (`S2C::Tuning`), so predicting clients run the host's numbers. Returns the
    /// number of clients it was sent to.
    pub fn set_tuning(&mut self, world: Option<&mut World>, t: &bd_sim::tuning::CombatTuning) -> usize {
        if let Some(w) = world {
            w.set_tuning(t.clone());
        }
        let msg = S2C::Tuning(Box::new(t.clone().validated()));
        let ids: Vec<PlayerId> = self.remotes.iter().map(|r| r.id).collect();
        for &id in &ids {
            self.session.send(id, &msg);
        }
        ids.len()
    }

    /// Back to the lobby (match torn down).
    pub fn end_match(&mut self) {
        self.in_match = false;
        self.vote = None;
        self.departed.clear();
        self.terrain_log.clear();
        self.ev_buf.clear();
        for r in &mut self.remotes {
            r.in_world = false;
            r.queue = input_queue();
            r.enc.reset();
        }
        self.session.broadcast(&S2C::BackToLobby);
        self.lobby_dirty = true;
    }

    fn init_remote(&mut self, i: usize, active: &mut ActiveWorld) {
        let w = &active.world;
        let id = self.remotes[i].id;
        let mut cfg = w.cfg.clone();
        if let bd_sim::ArenaSource::Archipelago { .. } = cfg.arena {
            cfg.arena = bd_sim::ArenaSource::Archipelago { seed: w.arena_seed() };
        }
        // Flush pending events first so the client's stream position matches the snapshot.
        self.flush_events(w.tick);
        // A new world generation for this client: inputs still in flight for the old one (the
        // client keeps playing it until this init arrives) carry the old epoch and are dropped.
        // Without it, their (high) sequence numbers locked the fresh queue out: after a map swap
        // the client's new inputs (numbered from 1 again) all counted as duplicates, so its player
        // stood repeating a stale input — "stuck", until a rejoin — for as long as the old match
        // had run.
        let epoch = self.remotes[i].epoch.wrapping_add(1);
        self.remotes[i].epoch = epoch;
        let init = WorldInit {
            cfg,
            map: self.map,
            arena_seed: w.arena_seed(),
            tick: w.tick,
            manifest: w.construct_manifest(),
            terrain_events: self.terrain_log.len() as u32,
            epoch,
        };
        self.session.send(id, &S2C::WorldInit(Box::new(init)));
        for chunk in self.terrain_log.chunks(LOG_CHUNK) {
            self.session.send(id, &S2C::TerrainLog(chunk.to_vec()));
        }
        self.session.send(id, &S2C::WorldReady);
        let r = &mut self.remotes[i];
        r.in_world = true;
        r.queue = input_queue();
        r.enc.reset();
        self.stats.inits_sent += 1;
    }

    /// Handle network traffic. Remote players' commands are appended to `cmds` (push them as
    /// `SimCommand::remote`). Never blocks.
    pub fn poll(&mut self, mut active: Option<&mut ActiveWorld>, cmds: &mut Vec<(PlayerId, Cmd)>) {
        let t0 = now_secs();
        while let Some(ev) = self.session.poll() {
            match ev {
                HostEvent::PlayerJoined(info) => {
                    let now = now_secs();
                    self.remotes.push(Remote {
                        id: info.id,
                        queue: input_queue(),
                        enc: DeltaEncoder::new(),
                        in_world: false,
                        cmd_tokens: CMD_RATE * 2.0,
                        last_cmd_refill: now,
                        last_keyframe: 0.0,
                        last_resync: 0.0,
                        ready: false,
                        profile: NetProfile::default(),
                        pending_casts: Default::default(),
                        token: None,
                        joined_at: now,
                        name: info.name.clone(),
                        corps: info.corps,
                        epoch: 0,
                    });
                    self.lobby_dirty = true;
                    // Mid-match joiners enter the world once their reconnect token arrives.
                }
                HostEvent::PlayerLeft { id, reason } => {
                    bd_net_log(|| format!("player {} left: {reason:?}", id.0));
                    let token = self.remotes.iter().find(|r| r.id == id).and_then(|r| r.token);
                    self.remotes.retain(|r| r.id != id);
                    if let Some(a) = active.as_deref_mut() {
                        match (&reason, token, self.in_match) {
                            (LeaveReason::ConnectionLost(_), Some(t), true) => {
                                if let Some(d) = a.world.remove_player_for_reconnect(id) {
                                    self.departed.retain(|(tk, _, _)| *tk != t);
                                    self.departed.push((t, d, now_secs()));
                                }
                            }
                            _ => {
                                a.world.remove_player(id);
                            }
                        }
                    }
                    self.lobby_dirty = true;
                }
                HostEvent::Message { from, msg } => self.on_message(from, msg, active.as_deref_mut(), cmds),
            }
        }
        let now = now_secs();
        self.departed.retain(|(_, _, t)| now - t < REJOIN_SECS);
        if self.in_match {
            if let Some(a) = active.as_deref_mut() {
                for i in 0..self.remotes.len() {
                    let r = &self.remotes[i];
                    if !r.in_world && (r.token.is_some() || now - r.joined_at > TOKEN_WAIT_SECS) {
                        self.enter_world(i, a);
                    }
                }
            }
        }
        if self.lobby_dirty && now - self.last_lobby_sent > 0.1 || now - self.last_lobby_sent > 1.0 {
            self.lobby_dirty = false;
            self.last_lobby_sent = now;
            let l = self.lobby();
            match self.vote.as_ref().filter(|v| v.hidden) {
                // Hidden votes: every client gets its own view (picks only once it voted).
                Some(v) => {
                    let ids: Vec<PlayerId> = self.session.players().iter().map(|p| p.id).filter(|id| *id != self.me).collect();
                    for id in ids {
                        let view = LobbyInfo { vote: Some(v.view_for(id, now)), ..l.clone() };
                        self.session.send(id, &S2C::Lobby(view));
                    }
                }
                None => self.session.broadcast(&S2C::Lobby(l)),
            }
        }
        self.cost(t0);
    }

    /// Put a remote into the running match: its old player back if it reconnected, else a new one.
    fn enter_world(&mut self, i: usize, a: &mut ActiveWorld) {
        let (id, token) = (self.remotes[i].id, self.remotes[i].token);
        let back = token.and_then(|t| self.departed.iter().position(|(tk, d, _)| *tk == t && d.id == id));
        let rejoined = back.map(|k| self.departed.remove(k).1).is_some_and(|d| a.world.rejoin_player(&d).is_ok());
        if rejoined {
            self.stats.rejoins += 1;
        } else if a.world.player(id).is_none() {
            let (name, corps) = (self.remotes[i].name.clone(), self.remotes[i].corps);
            let _ = a.world.add_player(id, name, corps);
        }
        self.init_remote(i, a);
    }

    fn on_message(&mut self, from: PlayerId, msg: C2S, active: Option<&mut ActiveWorld>, cmds: &mut Vec<(PlayerId, Cmd)>) {
        let Some(i) = self.remotes.iter().position(|r| r.id == from) else { return };
        let now = now_secs();
        match msg {
            C2S::Token(t) => {
                if self.remotes[i].token.is_none() {
                    self.remotes[i].token = Some(t);
                }
            }
            C2S::Profile(p) => {
                self.remotes[i].profile = p.sanitized();
                self.lobby_dirty = true;
            }
            C2S::Ready(r) => {
                self.remotes[i].ready = r;
                self.lobby_dirty = true;
            }
            C2S::Input { packet, snap_ack, epoch } => {
                let r = &mut self.remotes[i];
                if packet.inputs.len() > 256 {
                    self.session.kick(from, "input flood");
                    return;
                }
                if r.in_world && epoch == r.epoch {
                    r.queue.receive(packet);
                    r.enc.ack(snap_ack);
                }
            }
            C2S::Cmd(c) => {
                let r = &mut self.remotes[i];
                r.cmd_tokens = (r.cmd_tokens + (now - r.last_cmd_refill) as f32 * CMD_RATE).min(CMD_RATE * 2.0);
                r.last_cmd_refill = now;
                if r.cmd_tokens < 1.0 || !r.in_world {
                    self.stats.cmds_dropped += 1;
                    return;
                }
                r.cmd_tokens -= 1.0;
                if let Some(seq) = c.cast_seq() {
                    if r.pending_casts.len() >= 16 {
                        r.pending_casts.pop_front();
                    }
                    r.pending_casts.push_back((seq, now));
                }
                if let Some(cmd) = c.into_cmd() {
                    cmds.push((from, cmd));
                }
            }
            C2S::KeyframeReq => {
                let r = &mut self.remotes[i];
                if now - r.last_keyframe > 0.2 {
                    r.last_keyframe = now;
                    r.enc.reset();
                    self.stats.keyframes += 1;
                }
            }
            C2S::Shout(line) => {
                if self.remotes[i].in_world && self.shouts_in.len() < 32 {
                    self.shouts_in.push((from, line));
                }
            }
            C2S::MapVote(pick) => {
                self.cast_pick(from, pick);
            }
            C2S::Resync => {
                if now - self.remotes[i].last_resync > 2.0 {
                    self.remotes[i].last_resync = now;
                    if let (true, Some(a)) = (self.in_match, active) {
                        self.stats.resyncs += 1;
                        self.init_remote(i, a);
                    }
                }
            }
        }
    }

    /// `SimSet::Input`: one input per remote player from its jitter queue, plus its lag-comp view.
    pub fn pre_tick(&mut self, active: &mut ActiveWorld) {
        let t0 = now_secs();
        let tick = active.world.tick + 1;
        for r in &mut self.remotes {
            if !r.in_world {
                continue;
            }
            let before = r.queue.stats.starved;
            if let Some(q) = r.queue.next() {
                active.remote_inputs.push((r.id, q.input));
                let v = clamp_view_tick(q.view_tick, tick, MAX_REWIND_TICKS);
                active.world.set_view_tick(r.id, Some(v));
            }
            self.stats.input_starved += r.queue.stats.starved - before;
        }
        self.cost(t0);
    }

    /// `SimSet::Post`: forward this tick's events, send snapshots, check terrain.
    pub fn post_tick(&mut self, active: &mut ActiveWorld) {
        let t0 = now_secs();
        for e in &active.tick_events {
            if let GameEvent::ArenaReset { seed } = e {
                self.terrain_log.clear();
                // New terrain from `seed` (rematch / party round); its hash is unknown here.
                self.map = NetMap { seed: *seed, pristine_hash: 0, ..self.map };
                // Rare (a round change): one full rehash on this tick.
                self.terrain = TerrainHasher::full(&mut active.world.voxels);
            }
            if is_terrain_event(e) {
                self.terrain_log.push(e.clone());
            }
        }
        if self.remotes.iter().any(|r| !r.pending_casts.is_empty()) {
            self.match_casts(&active.tick_events);
        }
        // Competitive stats (an observer: never touches the world) + the next-map vote.
        self.match_stats.observe_events(&active.tick_events, &active.world);
        let over = match_over(&active.world);
        if active.world.tick % STATS_EVERY == 0 || over != self.vote.is_some() {
            self.stats_lines = self.match_stats.lines_for(&active.world);
            self.lobby_dirty |= over;
        }
        if over && self.vote.is_none() {
            self.vote = Some(VoteState::open(&self.settings.vote, &mut self.rotation, self.map.kind, now_secs() + f64::from(VOTE_SECS)));
            self.lobby_dirty = true;
        } else if !over && self.vote.is_some() {
            self.vote = None;
            self.lobby_dirty = true;
        }
        if self.remotes.iter().any(|r| r.in_world) {
            self.ev_buf.extend_from_slice(&active.tick_events);
        }
        let tick = active.world.tick;
        let snap_due = tick % SNAPSHOT_EVERY == 0;
        let check_due = self.terrain_check_every > 0 && tick % self.terrain_check_every == 0;
        if snap_due || check_due || self.ev_buf.len() > 512 {
            self.flush_events(tick);
        }
        let hash = check_due.then(|| self.terrain.update(&mut active.world.voxels));
        let w = &active.world;
        if let Some(hash) = hash.filter(|_| self.remotes.iter().any(|r| r.in_world)) {
            self.stats.last_terrain_hash = hash;
            self.stats.terrain_checks += 1;
            for r in self.remotes.iter().filter(|r| r.in_world) {
                self.session.send(r.id, &S2C::TerrainCheck { tick: w.tick, hash });
            }
        }
        if snap_due {
            for i in 0..self.remotes.len() {
                if !self.remotes[i].in_world {
                    continue;
                }
                let id = self.remotes[i].id;
                w.write_snapshot(Some(id), &mut self.snap);
                let delta = self.remotes[i].enc.encode(&self.snap);
                self.session.send_latest(id, &S2C::Snapshot(delta));
                self.stats.snapshots_sent += 1;
            }
        }
        self.session.set_tick(w.tick);
        self.cost(t0);
    }

    /// Tell clients what became of their casts, *before* the events that carry the spawn, so a
    /// predicted construct can be swapped for the real one without a pop.
    fn match_casts(&mut self, events: &[GameEvent]) {
        let now = now_secs();
        for r in &mut self.remotes {
            while r.pending_casts.front().is_some_and(|(_, t)| now - t > 3.0) {
                r.pending_casts.pop_front();
            }
        }
        for e in events {
            let (player, outcome) = match e {
                GameEvent::ConstructSpawned { id, owner, .. } => (*owner, CastOutcome::Spawned(*id)),
                GameEvent::Grafted { player, id, .. } => (*player, CastOutcome::Grafted(*id)),
                GameEvent::CastFailed { player, reason } => (*player, CastOutcome::Failed(*reason)),
                // A technique learned by saying it: the cast made a power, not a construct.
                GameEvent::Power(bd_sim::powers::PowerEvent::Gained { player, power, how: bd_sim::powers::GainHow::Learned }) => {
                    (*player, CastOutcome::Became(*power))
                }
                _ => continue,
            };
            let Some(r) = self.remotes.iter_mut().find(|r| r.id == player) else { continue };
            let Some((seq, _)) = r.pending_casts.pop_front() else { continue };
            self.session.send(player, &S2C::CastResult { seq, outcome });
        }
    }

    fn flush_events(&mut self, tick: Tick) {
        if self.ev_buf.is_empty() {
            return;
        }
        let events = std::mem::take(&mut self.ev_buf);
        self.stats.events_sent += events.len() as u64;
        let msg = S2C::Events { tick, events };
        for r in self.remotes.iter().filter(|r| r.in_world) {
            self.session.send(r.id, &msg);
        }
        if let S2C::Events { mut events, .. } = msg {
            events.clear();
            self.ev_buf = events;
        }
    }

    pub fn kick(&mut self, id: PlayerId, reason: &str, active: Option<&mut ActiveWorld>) {
        self.session.kick(id, reason);
        // `kick` queues a PlayerLeft that `poll` handles (world removal, lobby).
        let _ = active;
    }

    pub fn shutdown(&mut self) {
        self.session.shutdown();
        if let Some(t) = self.tunnel.as_mut() {
            t.stop();
        }
    }

    fn cost(&mut self, t0: f64) {
        let ms = ((now_secs() - t0) * 1000.0) as f32;
        self.stats.cost_ms += (ms - self.stats.cost_ms) * 0.05;
    }

    /// Diagnostics: per-remote input queue depth and stats.
    pub fn queue_report(&self) -> Vec<(PlayerId, usize, bd_net::InputQueueStats)> {
        self.remotes.iter().map(|r| (r.id, r.queue.depth(), r.queue.stats)).collect()
    }

}

impl Drop for HostCore {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn bd_net_log(msg: impl FnOnce() -> String) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("BD_NET_LOG").is_some()) {
        eprintln!("[net {:.3}] {}", now_secs(), msg());
    }
}
