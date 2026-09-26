//! Client core (no Bevy): connection, replica world, prediction, reconciliation, interpolation.
//!
//! Per frame: [`ClientCore::update`] (network in: lobby, world init / terrain log on a worker,
//! events → `apply_event`, snapshots → `apply_snapshot` + reconcile + interpolation buffer,
//! desync checks, reconnects) and [`ClientCore::send_inputs`]. Per client tick:
//! [`ClientCore::predict`] (sanitised input → `InputBuffer` → `Reconciler::predict` with
//! `World::predict_player`, identical to the host's movement).

use std::collections::VecDeque;
use std::thread::JoinHandle;
use std::time::Duration;

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::{PlayerId, Tick};
use bd_net::{
    ClientConfig, ClientEvent, ClientSession, InputBuffer, InterpConfig, Reconcilable, ReconcileOutcome, Reconciler, SnapshotBuffer, now_secs,
};
use bd_sim::movement::{MoveEvents, PlayerState};
use bd_sim::replicate::{DeltaDecoder, WorldSnapshot};
use bd_sim::world::sanitize_input;
use bd_sim::{GameEvent, World};
use bd_voxel::{ArenaInfo, VoxelWorld};

use super::proto::*;
use super::terrain_hash::TerrainHasher;
use super::timing::EventDelay;

/// Position error that counts as a *visible* correction (smaller ones vanish in the ~100 ms blend).
pub const VISIBLE_ERROR_M: f32 = 0.10;
/// Reconnect attempts after a dropped connection (every [`RECONNECT_EVERY`] s).
pub const RECONNECT_ATTEMPTS: u32 = 15;
pub const RECONNECT_EVERY: f64 = 2.0;
/// Input packets per second at most (every client tick's input is sent, batched).
const SEND_HZ: f64 = 120.0;
/// Host jitter-buffer + batching allowance for the client lead (ticks).
pub const LEAD_BUFFER_TICKS: f64 = 3.0;

/// Predicted local movement state (newtype for `Reconcilable`).
#[derive(Clone, Debug)]
pub struct Pred(pub PlayerState);

impl Reconcilable for Pred {
    fn diverges_from(&self, a: &Self) -> bool {
        let (p, q) = (&self.0, &a.0);
        let f = |x: f32, y: f32| (x - y).abs() > 1e-3;
        (p.pos - q.pos).length() > 1e-3
            || (p.vel - q.vel).length() > 1e-3
            || p.grounded != q.grounded
            || p.crouched != q.crouched
            || p.sliding != q.sliding
            || p.boosting != q.boosting
            || f(p.gas, q.gas)
            || f(p.tumble, q.tumble)
            || p.hooks[0].phase != q.hooks[0].phase
            || p.hooks[1].phase != q.hooks[1].phase
            || (p.hooks[0].pos - q.hooks[0].pos).length() > 1e-3
            || (p.hooks[1].pos - q.hooks[1].pos).length() > 1e-3
    }
    fn render_position(&self) -> bd_core::Vec3 {
        let p = self.0.pos;
        bd_core::Vec3::new(p.x, p.y, p.z)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientPhase {
    Connecting,
    /// Connected, no match (or waiting for the host to start).
    Lobby,
    /// Receiving / building the world.
    Loading,
    InGame,
    Reconnecting { attempt: u32 },
    Failed(String),
}

#[derive(Clone, Debug, Default)]
pub struct ClientStats {
    pub snapshots: u64,
    pub events: u64,
    pub corrections: u64,
    /// Corrections with an error above [`VISIBLE_ERROR_M`] (and not snapped: respawns/teleports).
    pub visible_corrections: u64,
    pub snapped_corrections: u64,
    pub max_error_m: f32,
    pub keyframe_requests: u64,
    pub terrain_checks_ok: u64,
    pub desyncs: u64,
    pub inits: u64,
    pub reconnects: u64,
    pub last_terrain_hash: u64,
    pub world_build_ms: f32,
    /// `S2C::Tuning` received (host combat tuning hot reloads).
    pub tuning_updates: u64,
    /// Knockback hits / shoves / respawns on the local player (host-side impulses the client
    /// cannot predict: each one is an expected correction).
    pub hits_taken: u64,
    /// Visible corrections with no hit / respawn on us in the preceding 0.5 s.
    pub unexplained_visible: u64,
    /// Main-thread cost (ms, smoothed).
    pub cost_ms: f32,
}

enum Pending {
    Event(GameEvent),
    Check { tick: Tick, hash: u64 },
}

struct InitStream {
    init: WorldInit,
    log: Vec<GameEvent>,
    ready: bool,
    /// Terrain build: (voxels, info, seed, pristine content hash).
    job: Option<JoinHandle<(VoxelWorld, ArenaInfo, u64, u64, TerrainHasher)>>,
    started: f64,
}

/// What the Bevy side should do after [`ClientCore::update`].
#[derive(Default)]
pub struct ClientOut {
    /// A (new) replica world is ready: insert / replace `ActiveWorld` (role Client).
    pub new_world: Option<World>,
    /// Host went back to the lobby.
    pub back_to_lobby: bool,
}

pub struct ClientCore {
    pub address: String,
    cfg: ClientConfig,
    pub session: ClientSession<C2S, S2C>,
    pub phase: ClientPhase,
    pub me: Option<PlayerId>,
    pub lobby: LobbyInfo,
    /// Map of the current / last world init.
    pub map: NetMap,
    profile: NetProfile,
    ready: bool,
    init: Option<InitStream>,
    pending: VecDeque<Pending>,
    /// Events for presentation (`ActiveWorld::push_event`), in order.
    pub outbox: Vec<GameEvent>,
    /// Outcomes of our casts (seq, outcome); `None` = no answer in time (treat as rejected).
    pub cast_results: Vec<(u32, Option<CastOutcome>)>,
    pending_casts: VecDeque<(u32, f64)>,
    next_cast_seq: u32,
    decoder: DeltaDecoder,
    /// Host events held until the render clock reaches them (see `timing`).
    pub delay: EventDelay,
    terrain: TerrainHasher,
    latest: Option<WorldSnapshot>,
    pub interp: SnapshotBuffer<WorldSnapshot>,
    pub inputs: InputBuffer<PlayerInput>,
    pub rec: Option<Reconciler<Pred, PlayerInput>>,
    snap_ack: Tick,
    last_send: f64,
    last_keyframe_req: f64,
    reconnect_at: Option<f64>,
    reconnect_attempt: u32,
    /// World exists (`ActiveWorld` holds it).
    has_world: bool,
    pub stats: ClientStats,
    /// Pacing: client ticks simulated + offset to the host tick timeline.
    ticks_done: u64,
    /// Random per-process reconnect token (never derived from personal data).
    token: u64,
    last_hit_at: f64,
    pace_base: Option<f64>,
    /// Other players' shouts (drained every frame by `present::shout`).
    pub shouts: Vec<(PlayerId, crate::present::shout::ShoutLine)>,
}

impl ClientCore {
    pub fn connect(address: &str, name: &str, corps: Corps, profile: NetProfile) -> Result<Self, bd_net::NetError> {
        let cfg = ClientConfig { name: name.to_string(), corps, ..Default::default() };
        let session = ClientSession::connect(address, cfg.clone())?;
        Ok(Self {
            address: address.to_string(),
            cfg,
            session,
            phase: ClientPhase::Connecting,
            me: None,
            lobby: LobbyInfo::default(),
            map: NetMap::default(),
            profile: profile.sanitized(),
            ready: false,
            init: None,
            pending: VecDeque::new(),
            outbox: Vec::new(),
            cast_results: Vec::new(),
            pending_casts: VecDeque::new(),
            next_cast_seq: 1,
            decoder: DeltaDecoder::new(),
            delay: EventDelay::default(),
            terrain: TerrainHasher::default(),
            latest: None,
            interp: SnapshotBuffer::new(InterpConfig::default()),
            inputs: InputBuffer::new(4),
            rec: None,
            snap_ack: 0,
            last_send: 0.0,
            last_keyframe_req: 0.0,
            reconnect_at: None,
            reconnect_attempt: 0,
            has_world: false,
            stats: ClientStats::default(),
            ticks_done: 0,
            token: {
                use std::hash::{BuildHasher, Hasher};
                let mut h = std::collections::hash_map::RandomState::new().build_hasher();
                h.write_u64(now_secs().to_bits());
                h.finish() | 1
            },
            last_hit_at: 0.0,
            pace_base: None,
            shouts: Vec::new(),
        })
    }

    pub fn set_profile(&mut self, p: NetProfile) {
        self.profile = p.sanitized();
        self.session.send(&C2S::Profile(self.profile.clone()));
    }

    pub fn set_ready(&mut self, r: bool) {
        self.ready = r;
        self.session.send(&C2S::Ready(r));
    }

    /// Forward a bridge command to the host. Casts get a client cast sequence number (returned),
    /// echoed back in [`CastOutcome`] via [`ClientCore::cast_results`].
    pub fn send_cmd(&mut self, c: &crate::sim_bridge::Cmd) -> Option<u32> {
        if self.phase != ClientPhase::InGame {
            return None;
        }
        let is_cast = matches!(c, crate::sim_bridge::Cmd::Cast { .. } | crate::sim_bridge::Cmd::Signature { .. });
        let seq = self.next_cast_seq;
        let nc = NetCmd::from_cmd(c, seq)?;
        self.session.send(&C2S::Cmd(nc));
        if !is_cast {
            return None;
        }
        self.next_cast_seq = self.next_cast_seq.wrapping_add(1);
        self.pending_casts.push_back((seq, now_secs()));
        Some(seq)
    }

    pub fn leave(&mut self) {
        self.session.disconnect();
        self.phase = ClientPhase::Failed("left".into());
    }

    pub fn rtt_ms(&self) -> f32 {
        self.session.clock().rtt().ms()
    }

    /// The world was dropped by the game (left the match).
    pub fn forget_world(&mut self) {
        self.has_world = false;
        self.rec = None;
    }

    /// Presentation events whose render time has come (call each frame after `interp.update`).
    pub fn release_events(&mut self) {
        let rt = self.interp.render_tick();
        self.delay.release(rt, now_secs(), &mut self.outbox);
    }

    fn reset_replication(&mut self) {
        // Anything still held belongs to the old world: show it before the new init stream.
        self.delay.flush(&mut self.outbox);
        self.decoder = DeltaDecoder::new();
        self.latest = None;
        self.interp = SnapshotBuffer::new(InterpConfig::default());
        self.inputs = InputBuffer::new(4);
        self.rec = None;
        self.snap_ack = 0;
        self.pending.clear();
    }

    /// Network in. `world` = the replica currently in `ActiveWorld` (if any).
    pub fn update(&mut self, mut world: Option<&mut World>) -> ClientOut {
        let t0 = now_secs();
        let mut out = ClientOut::default();
        if world.is_none() {
            self.has_world = false;
        }
        self.try_reconnect();
        while let Some(ev) = self.session.poll() {
            match ev {
                ClientEvent::Welcome { you, .. } => {
                    self.me = Some(you);
                    if matches!(self.phase, ClientPhase::Reconnecting { .. }) {
                        self.stats.reconnects += 1;
                    }
                    self.phase = ClientPhase::Lobby;
                    self.reconnect_attempt = 0;
                    self.reconnect_at = None;
                    self.session.send(&C2S::Token(self.token));
                    self.session.send(&C2S::Profile(self.profile.clone()));
                    if self.ready {
                        self.session.send(&C2S::Ready(true));
                    }
                }
                ClientEvent::PlayerJoined(_) | ClientEvent::PlayerLeft { .. } => {}
                ClientEvent::Disconnected(reason) => {
                    let fatal = reason.starts_with("rejected") || reason.starts_with("kicked") || self.me.is_none() && self.stats.inits == 0;
                    if fatal || self.reconnect_attempt >= RECONNECT_ATTEMPTS {
                        self.phase = ClientPhase::Failed(reason);
                    } else {
                        super::host::bd_net_log(|| format!("connection lost ({reason}); reconnecting"));
                        self.reconnect_attempt += 1;
                        self.phase = ClientPhase::Reconnecting { attempt: self.reconnect_attempt };
                        self.reconnect_at = Some(now_secs() + if self.reconnect_attempt == 1 { 0.3 } else { RECONNECT_EVERY });
                    }
                }
                ClientEvent::Message(m) => self.on_message(m, world.as_deref_mut(), &mut out),
            }
        }
        // Casts the host never answered (lost on a reconnect / resync).
        let now = now_secs();
        while self.pending_casts.front().is_some_and(|(_, t)| now - t > 2.0) {
            if let Some((seq, _)) = self.pending_casts.pop_front() {
                self.cast_results.push((seq, None));
            }
        }
        // World build finished?
        if let Some(w) = self.finish_init() {
            out.new_world = Some(w);
            self.has_world = true;
            self.phase = ClientPhase::InGame;
            world = None; // the old one is being replaced
        }
        // Snapshots (latest lane).
        let mut newest: Option<WorldSnapshot> = None;
        let mut need_key = false;
        for (at, msg) in self.session.drain_latest() {
            let S2C::Snapshot(d) = msg else { continue };
            match self.decoder.decode(&d) {
                Some(full) => {
                    self.stats.snapshots += 1;
                    self.interp.push(full.tick, full.clone(), at);
                    newest = Some(full);
                }
                None => need_key = true,
            }
        }
        if need_key && now_secs() - self.last_keyframe_req > 0.25 {
            self.last_keyframe_req = now_secs();
            self.stats.keyframe_requests += 1;
            self.session.send(&C2S::KeyframeReq);
        }
        if let Some(s) = newest {
            self.snap_ack = s.tick;
            self.latest = Some(s);
        }
        if self.init.is_none() && self.has_world {
            if let (Some(w), Some(s)) = (world.as_deref_mut(), self.latest.take()) {
                self.apply_snapshot(w, &s);
            }
        }
        self.cost(t0);
        out
    }

    fn try_reconnect(&mut self) {
        let Some(at) = self.reconnect_at else { return };
        if now_secs() < at {
            return;
        }
        self.reconnect_at = None;
        let cfg = ClientConfig { connect_timeout: Duration::from_secs(4), ..self.cfg.clone() };
        match ClientSession::connect(&self.address, cfg) {
            Ok(s) => {
                self.session = s;
                self.reset_replication();
                self.init = None;
            }
            Err(e) => self.phase = ClientPhase::Failed(e.to_string()),
        }
    }

    fn on_message(&mut self, m: S2C, world: Option<&mut World>, out: &mut ClientOut) {
        match m {
            S2C::Lobby(l) => {
                self.lobby = l;
            }
            S2C::WorldInit(init) => {
                self.stats.inits += 1;
                self.reset_replication();
                let map = init.map;
                self.map = map;
                let job = std::thread::Builder::new()
                    .name("bd-net-arena".into())
                    .spawn(move || {
                        let (mut v, i, s) = build_terrain(map.kind, map.seed);
                        let h = v.content_hash();
                        let th = TerrainHasher::full(&mut v);
                        (v, i, s, h, th)
                    })
                    .ok();
                self.init = Some(InitStream { init: *init, log: Vec::new(), ready: false, job, started: now_secs() });
                self.phase = ClientPhase::Loading;
            }
            S2C::TerrainLog(mut evs) => {
                if let Some(i) = self.init.as_mut() {
                    i.log.append(&mut evs);
                }
            }
            S2C::WorldReady => {
                if let Some(i) = self.init.as_mut() {
                    i.ready = true;
                }
            }
            S2C::Events { events, tick } => {
                self.stats.events += events.len() as u64;
                match (self.init.is_none() && self.has_world, world) {
                    (true, Some(w)) => {
                        for e in events {
                            match &e {
                                GameEvent::Hit { target, knockback, .. } if Some(*target) == self.me && *knockback > 0.0 => {
                                    self.stats.hits_taken += 1;
                                    self.last_hit_at = now_secs();
                                }
                                GameEvent::Respawn { player, .. } if Some(*player) == self.me => {
                                    self.stats.hits_taken += 1;
                                    self.last_hit_at = now_secs();
                                }
                                _ => {}
                            }
                            if matches!(e, GameEvent::ArenaReset { .. }) {
                                // New terrain (rematch / party round): rebuild it off-thread from
                                // the host's init instead of regenerating here (a frame hitch, and
                                // the replica can't know the map). Later events wait for the init.
                                self.session.send(&C2S::Resync);
                                self.has_world = false;
                                break;
                            }
                            w.apply_event(&e);
                            self.delay.route(e, tick, now_secs(), self.me, w, &mut self.outbox);
                        }
                    }
                    _ => self.pending.extend(events.into_iter().map(Pending::Event)),
                }
            }
            S2C::TerrainCheck { tick, hash } => match (self.init.is_none() && self.has_world, world) {
                (true, Some(w)) => self.check_terrain(w, tick, hash),
                _ => self.pending.push_back(Pending::Check { tick, hash }),
            },
            S2C::BackToLobby => {
                self.init = None;
                self.reset_replication();
                self.has_world = false;
                self.phase = ClientPhase::Lobby;
                out.back_to_lobby = true;
            }
            S2C::CastResult { seq, outcome } => {
                if let Some(i) = self.pending_casts.iter().position(|(s, _)| *s == seq) {
                    self.pending_casts.remove(i);
                    self.cast_results.push((seq, Some(outcome)));
                }
            }
            S2C::Snapshot(_) => {}
            S2C::Shout { player, line } => {
                if self.shouts.len() < 32 {
                    self.shouts.push((player, line));
                }
            }
            S2C::Tuning(t) => {
                // The host's combat.ron changed: predict with the same numbers (never our own file).
                let t = t.validated();
                self.stats.tuning_updates += 1;
                if let Some(i) = self.init.as_mut() {
                    i.init.cfg.tuning = t.clone();
                }
                if let Some(w) = world {
                    w.set_tuning(t);
                }
                super::host::bd_net_log(|| "host combat tuning updated".into());
            }
        }
    }

    fn check_terrain(&mut self, w: &mut World, _tick: Tick, hash: u64) {
        let mine = self.terrain.update(&mut w.voxels);
        self.stats.last_terrain_hash = mine;
        if mine == hash {
            self.stats.terrain_checks_ok += 1;
        } else {
            self.stats.desyncs += 1;
            super::host::bd_net_log(|| format!("terrain desync: host {hash:016x} me {mine:016x}; requesting resync"));
            self.session.send(&C2S::Resync);
        }
    }

    fn finish_init(&mut self) -> Option<World> {
        let i = self.init.as_mut()?;
        if !i.ready || !i.job.as_ref().is_some_and(|j| j.is_finished()) {
            return None;
        }
        let mut i = self.init.take()?;
        let (voxels, info, seed, pristine, hasher) = i.job.take()?.join().ok()?;
        self.terrain = hasher;
        if i.init.map.pristine_hash != 0 && pristine != i.init.map.pristine_hash {
            // Different game build: its map generates other voxels. Refuse instead of desyncing.
            self.session.disconnect();
            self.phase = ClientPhase::Failed("map version mismatch with the host — update your game".into());
            return None;
        }
        let mut w = World::with_arena(i.init.cfg.clone(), voxels, info, seed);
        w.tick = i.init.tick;
        // Render mirror: rebuild the terrain from the seed, then the same edits.
        self.outbox.push(GameEvent::ArenaReset { seed });
        for e in &i.init.manifest {
            w.apply_event(e);
            self.outbox.push(e.clone());
        }
        for e in i.log.drain(..) {
            w.apply_event(&e);
            self.outbox.push(e);
        }
        while let Some(p) = self.pending.pop_front() {
            match p {
                Pending::Event(e) => {
                    w.apply_event(&e);
                    self.outbox.push(e);
                }
                Pending::Check { tick, hash } => self.check_terrain(&mut w, tick, hash),
            }
        }
        if let Some(s) = self.latest.take() {
            self.apply_snapshot(&mut w, &s);
        }
        self.stats.world_build_ms = ((now_secs() - i.started) * 1000.0) as f32;
        Some(w)
    }

    fn apply_snapshot(&mut self, w: &mut World, s: &WorldSnapshot) {
        let Some(me) = self.me else { return };
        w.apply_snapshot(s, Some(me));
        let Some(own) = s.own else {
            // Dead: nothing to predict until the respawn.
            self.rec = None;
            return;
        };
        self.inputs.ack(s.ack);
        let auth = Pred(own.to_state());
        let rec = match self.rec.as_mut() {
            Some(r) => r,
            None => {
                self.rec = Some(Reconciler::new(auth));
                if let Some(p) = w.player_mut(me) {
                    p.mv = own.to_state();
                }
                return;
            }
        };
        let wr: &World = w;
        let outcome = rec.reconcile(s.ack, auth, |st, i| {
            wr.predict_player(me, &mut st.0, i);
        });
        if let ReconcileOutcome::Corrected { error, .. } = outcome {
            self.stats.corrections += 1;
            let e = error.length();
            if error == bd_core::Vec3::ZERO {
                self.stats.snapped_corrections += 1;
            } else if e > VISIBLE_ERROR_M {
                self.stats.visible_corrections += 1;
                if now_secs() - self.last_hit_at > 0.5 {
                    self.stats.unexplained_visible += 1;
                    let (held, n) = (w.constructs.held_by(me).is_some(), w.constructs.owned_by(me).count());
                    let p = &own;
                    super::host::bd_net_log(|| format!("unexplained correction {e:.3} m ack {} tick {} held {held} owned {n} own.hooks {:?} grounded {}", s.ack, s.tick, p.hooks.map(|h| h.map(|h| h.phase)), p.bits & 1));
                }
            }
            if e.is_finite() {
                self.stats.max_error_m = self.stats.max_error_m.max(e);
            }
        }
        let cur = rec.current().0;
        if let Some(p) = w.player_mut(me) {
            p.mv = cur;
        }
    }

    /// One client tick: record + predict the local player. Returns (state before, state after,
    /// move events) for presentation, or `None` while there is nothing to predict (dead / loading).
    pub fn predict(&mut self, w: &mut World, raw: PlayerInput) -> Option<(PlayerState, PlayerState, MoveEvents)> {
        if self.phase != ClientPhase::InGame {
            return None;
        }
        self.ticks_done += 1;
        let me = self.me?;
        let input = sanitize_input(&raw);
        let seq = self.inputs.push(input);
        let input = PlayerInput { seq, ..input };
        let alive = w.player(me).is_some_and(|p| p.alive);
        let rec = self.rec.as_mut().filter(|_| alive)?;
        let before = rec.current().0;
        let mut ev = MoveEvents::default();
        let wr: &World = w;
        rec.predict(seq, input, |st, i| {
            ev = wr.predict_player(me, &mut st.0, i);
        });
        rec.update_smoothing(bd_core::DT);
        let after = rec.current().0;
        if let Some(p) = w.player_mut(me) {
            p.mv = after;
            p.input = input;
        }
        Some((before, after, ev))
    }

    /// Visual error offset to add to the rendered local player (decays over ~100 ms).
    pub fn visual_offset(&self) -> bd_core::Vec3 {
        self.rec.as_ref().map_or(bd_core::Vec3::ZERO, |r| r.visual_offset())
    }

    /// Send buffered inputs (+ snapshot ack) if due. Call once per frame after the ticks.
    pub fn send_inputs(&mut self, force: bool) {
        let now = now_secs();
        if self.phase != ClientPhase::InGame || (!force && now - self.last_send < 1.0 / SEND_HZ - 1e-4) {
            return;
        }
        if let Some(packet) = self.inputs.packet(self.interp.render_tick()) {
            self.last_send = now;
            self.session.send(&C2S::Input { packet, snap_ack: self.snap_ack });
        }
    }

    /// How far (ticks) the client timeline is behind where it should be (+ = run faster).
    /// `alpha` = fraction of the next tick already accumulated.
    pub fn pacing_error(&mut self, alpha: f64) -> Option<f64> {
        let clock = self.session.clock();
        if !clock.is_synced() || clock.rtt().count < 3 {
            return None;
        }
        let target = clock.target_client_tick(now_secs(), LEAD_BUFFER_TICKS);
        let local = self.ticks_done as f64 + alpha;
        let base = *self.pace_base.get_or_insert(target - local);
        let err = target - (base + local);
        if err.abs() > 30.0 {
            // Hitch / clock jump: re-base instead of racing.
            self.pace_base = Some(target - local);
            return Some(0.0);
        }
        Some(err)
    }

    fn cost(&mut self, t0: f64) {
        let ms = ((now_secs() - t0) * 1000.0) as f32;
        self.stats.cost_ms += (ms - self.stats.cost_ms) * 0.05;
    }
}
