//! Multiplayer (NET): host / client modes around the `sim_bridge` contract.
//!
//! ## API for the UI (all non-blocking)
//! * `MessageWriter<NetRequest>`: [`NetRequest::Host`], [`NetRequest::Join`],
//!   [`NetRequest::UpdateProfile`] (name / corps / costume / loadout), [`NetRequest::SetSettings`]
//!   (host), [`NetRequest::SetReady`], [`NetRequest::StartMatch`] (host), [`NetRequest::Kick`] (host),
//!   [`NetRequest::Leave`].
//! * `Res<NetStatus>`: role, phase, lobby code + tunnel state, LAN addresses, lobby players (ready,
//!   costume, loadout, RTT), settings, a disconnect banner, live stats.
//! * When a match starts (host `StartMatch`, or a client receives the world) NET sets `GameMode`
//!   and `AppState::InGame` itself, and inserts `ActiveWorld` (role Host / Client).
//!
//! ## Frame / tick wiring
//! * `Update`, after `FrameSet::Input`, before `BridgeSet::Commands`: [`net_poll`] (host: joins,
//!   inputs → jitter queues, remote commands → `SimCommand::remote`; client: events → `apply_event`
//!   + `push_event`, snapshots → `apply_snapshot` + reconcile, local `SimCommand`s → host, tick
//!   pacing).
//! * `SimTick`: host `SimSet::Input` (remote inputs + lag-comp view ticks), `SimSet::Post`
//!   (events, 60 Hz delta snapshots, terrain hash); client `SimSet::Simulate` (predict the local
//!   player, write `PlayerSim` with ~100 ms error smoothing).
//! * `Update`, after `FrameSet::Sim`, before `BridgeSet::Views`: client input send + `ViewKeys`
//!   from the interpolation buffer (local player patched with its predicted pose).

pub mod cli;
pub mod client;
pub mod headless;
pub mod host;
pub mod proto;
pub mod terrain_hash;
pub mod timing;

#[cfg(test)]
mod tests;

use std::thread::JoinHandle;

use bd_core::corps::Corps;
use bd_core::{DT, PlayerId};
use bd_net::{TunnelStatus, now_secs};
use bd_sim::replicate::{QLook, QPos, QVel, WorldSnapshot};
use bd_sim::settings::GameSettings;
use bd_sim::{GameEvent, World};
use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::input::{LocalInputState, LookAngles};
use crate::player::{FrameMoveEvents, FramePositions, LocalPlayer, PlayerSim};
use crate::sim_bridge::{ActiveWorld, BridgeSet, SimCommand, SimRole, ViewKeys};
use crate::state::{AppState, GameMode};
use crate::tick::{FrameSet, SimSet, SimTick, TickClock};
use crate::util::v3;

pub use client::{ClientCore, ClientPhase};
pub use host::{HostCore, HostStart};
#[allow(unused_imports)]
pub use proto::{LobbyPlayer, NetCostume, NetProfile};

// ------------------------------------------------------------------------------------ public API

/// Who we are on the network (name + corps go in the handshake; the profile to the lobby).
#[derive(Resource, Clone, Debug)]
pub struct NetIdentity {
    pub name: String,
    pub corps: Corps,
    pub profile: NetProfile,
}

impl Default for NetIdentity {
    fn default() -> Self {
        Self { name: "Lantern".into(), corps: Corps::Willpower, profile: NetProfile::default() }
    }
}

/// UI → NET requests.
#[allow(dead_code)] // variants sent by the UI
#[derive(Message, Clone, Debug)]
pub enum NetRequest {
    /// Host a lobby. `port: None` = 27960 (falls back to a free one). `tunnel` = cloudflared.
    Host { port: Option<u16>, tunnel: bool },
    /// Lobby code, `ip:port`, host name or tunnel URL.
    Join { address: String },
    /// Name / corps (used for the next Host / Join) + costume / loadout (sent now if connected).
    UpdateProfile(NetIdentity),
    /// Host: lobby settings (broadcast to everyone).
    SetSettings(GameSettings),
    SetReady(bool),
    /// Host: build the world and go in game (late joiners go straight in).
    StartMatch,
    /// Host: remove a player.
    Kick(PlayerId),
    /// Leave / close the lobby and return to the menu.
    Leave,
}

/// Client-side predicted construct spawns (for presentation; client role only).
///
/// `Started` is written the frame the local cast is sent (spawn a provisional construct from the
/// recipe at the predicted placement and start forming). `Confirmed` arrives just *before* the
/// `GameEvents` that carry `ConstructSpawned { id }` (or `Grafted`): adopt the provisional visual
/// as construct `id` and blend its transform to the authoritative one over ~100 ms (no pop).
/// `Rejected` (host refused, e.g. willpower; `reason: None` = no answer in 2 s): dissolve it.
#[allow(dead_code)] // read by presentation
#[derive(Message, Clone, Debug)]
pub enum PredictedCast {
    Started { seq: u32, recipe: Box<bd_core::mechanics::Recipe>, aim: bd_sim::Aim },
    Confirmed { seq: u32, id: bd_core::ConstructId, grafted: bool },
    Rejected { seq: u32, reason: Option<bd_sim::events::CastError> },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum NetRole {
    #[default]
    Offline,
    Host,
    Client,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum NetPhase {
    #[default]
    Idle,
    /// Host: listening (+ tunnel starting). Client: connecting / handshaking.
    Connecting,
    Lobby,
    /// Building / receiving the world.
    Loading,
    InGame,
    Reconnecting { attempt: u32 },
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum TunnelInfo {
    #[default]
    Disabled,
    Starting,
    Ready { code: String, url: String },
    Failed(String),
}

#[derive(Clone, Debug, Default)]
pub struct NetStats {
    pub rtt_ms: f32,
    pub jitter_ms: f32,
    pub up_kbps: f32,
    pub down_kbps: f32,
    pub corrections: u64,
    pub visible_corrections: u64,
    pub max_error_m: f32,
    pub desyncs: u64,
    pub terrain_checks_ok: u64,
    pub interp_delay_ms: f32,
    pub interp_starved: u64,
    pub snapshots: u64,
    /// Main-thread cost of the net systems (ms per frame, smoothed).
    pub cost_ms: f32,
    /// Host: input ticks repeated because a client's input was late.
    pub input_starved: u64,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct NetStatus {
    pub role: NetRole,
    pub phase: NetPhase,
    /// The words friends type to join (`<words>.trycloudflare.com`).
    pub lobby_code: Option<String>,
    pub tunnel: TunnelInfo,
    /// `ip:port` for LAN friends.
    pub lan: Vec<String>,
    pub me: Option<PlayerId>,
    pub players: Vec<LobbyPlayer>,
    pub settings: GameSettings,
    pub in_match: bool,
    /// Short text for a centred banner (connection problems). `None` = nothing to show.
    pub banner: Option<String>,
    pub stats: NetStats,
}

// ------------------------------------------------------------------------------------ internals

#[derive(Resource, Default)]
pub struct NetState {
    pub host: Option<HostCore>,
    pub client: Option<ClientCore>,
    /// World built / received, waiting to become `ActiveWorld`.
    pending_world: Option<World>,
    build: Option<JoinHandle<(World, proto::NetMap, terrain_hash::TerrainHasher)>>,
    pending_hasher: Option<terrain_hash::TerrainHasher>,
    /// Map of the host world waiting in `pending_world`.
    pending_map: proto::NetMap,
    cmds: Vec<(PlayerId, crate::sim_bridge::Cmd)>,
    cli: cli::NetCli,
    last_bytes: (u64, u64, f64),
    cost_acc: f32,
}

impl NetState {
    fn role(&self) -> NetRole {
        if self.host.is_some() {
            NetRole::Host
        } else if self.client.is_some() {
            NetRole::Client
        } else {
            NetRole::Offline
        }
    }
}

#[derive(Component)]
struct NetBanner;
#[derive(Component)]
struct NetStatsText;

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        let cli = cli::NetCli::from_env();
        if cli.headless && cli.active() {
            // Window-less harness modes: never return to Bevy.
            let code = if cli.host { headless::run_host(&cli) } else { headless::run_client(&cli) };
            std::process::exit(code);
        }
        let mut ident = NetIdentity::default();
        if let Some(n) = &cli.name {
            ident.name = n.clone();
        }
        if let Some(c) = cli.corps {
            ident.corps = c;
        }
        app.add_message::<NetRequest>()
            .add_message::<PredictedCast>()
            .insert_resource(ident)
            .insert_resource(NetState { cli, ..Default::default() })
            .init_resource::<NetStatus>()
            .add_systems(Startup, (cli_start, spawn_net_ui))
            .add_systems(
                Update,
                (handle_requests, net_poll, emit_remote_cmds, match_flow).chain().after(FrameSet::Input).before(BridgeSet::Commands).before(FrameSet::Sim),
            )
            .add_systems(SimTick, host_pre_tick.in_set(SimSet::Input))
            .add_systems(SimTick, client_predict.in_set(SimSet::Simulate))
            .add_systems(SimTick, host_post_tick.in_set(SimSet::Post))
            .add_systems(Update, client_frame.after(FrameSet::Sim).before(BridgeSet::Flush))
            .add_systems(Update, (update_status, update_net_ui).chain().after(BridgeSet::Views))
            .add_systems(OnExit(AppState::InGame), on_exit_game);
    }
}

fn cli_start(net: Res<NetState>, mut req: MessageWriter<NetRequest>) {
    let c = &net.cli;
    if c.host {
        req.write(NetRequest::Host { port: c.port, tunnel: !c.no_tunnel });
        req.write(NetRequest::StartMatch);
    } else if let Some(a) = &c.join {
        req.write(NetRequest::Join { address: a.clone() });
    }
}

fn handle_requests(
    mut reqs: MessageReader<NetRequest>,
    mut net: ResMut<NetState>,
    mut ident: ResMut<NetIdentity>,
    mut status: ResMut<NetStatus>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut active: Option<ResMut<ActiveWorld>>,
    map: Res<crate::arena::CurrentMap>,
) {
    for r in reqs.read() {
        match r.clone() {
            NetRequest::Host { port, tunnel } => {
                net.client = None;
                net.host = None;
                let start = HostStart {
                    port: port.unwrap_or(bd_net::DEFAULT_PORT),
                    tunnel,
                    lan: true,
                    name: ident.name.clone(),
                    corps: ident.corps,
                    profile: ident.profile.clone(),
                    settings: status.settings.clone(),
                    ..Default::default()
                };
                match HostCore::start(start) {
                    Ok(h) => {
                        info!("net: hosting on port {} (tunnel: {tunnel})", h.port);
                        net.host = Some(h);
                        status.phase = NetPhase::Lobby;
                    }
                    Err(e) => status.phase = NetPhase::Failed(e.to_string()),
                }
            }
            NetRequest::Join { address } => {
                net.host = None;
                match ClientCore::connect(&address, &ident.name, ident.corps, ident.profile.clone()) {
                    Ok(c) => {
                        info!("net: joining {}", c.session.url());
                        net.client = Some(c);
                        status.phase = NetPhase::Connecting;
                    }
                    Err(e) => status.phase = NetPhase::Failed(e.to_string()),
                }
            }
            NetRequest::UpdateProfile(id) => {
                if let Some(h) = net.host.as_mut() {
                    h.set_profile(id.profile.clone());
                }
                if let Some(c) = net.client.as_mut() {
                    c.set_profile(id.profile.clone());
                }
                *ident = id;
            }
            NetRequest::SetSettings(s) => {
                status.settings = s.clone().validated();
                if let Some(h) = net.host.as_mut() {
                    h.set_settings(s);
                }
            }
            NetRequest::SetReady(r) => {
                if let Some(h) = net.host.as_mut() {
                    h.set_ready(r);
                }
                if let Some(c) = net.client.as_mut() {
                    c.set_ready(r);
                }
            }
            NetRequest::StartMatch => {
                if let Some(h) = net.host.as_ref() {
                    if h.in_match || net.build.is_some() || net.pending_world.is_some() {
                        continue;
                    }
                    let settings = h.settings.clone();
                    let seed = (now_secs().to_bits() ^ 0x9E37_79B9_7F4A_7C15).max(1);
                    let kind = match map.0 {
                        crate::arena::MapKind::Miami => proto::NetMapKind::Miami,
                        crate::arena::MapKind::Orbit => proto::NetMapKind::Orbit,
                        _ => proto::NetMapKind::Archipelago,
                    };
                    let nm = proto::NetMap { kind, seed: crate::arena::ARENA_SEED, pristine_hash: 0 };
                    net.build = std::thread::Builder::new()
                        .name("bd-net-world".into())
                        .spawn(move || HostCore::build_world(&settings, nm, seed))
                        .ok();
                    status.phase = NetPhase::Loading;
                }
            }
            NetRequest::Kick(id) => {
                if let Some(h) = net.host.as_mut() {
                    h.kick(id, "kicked by the host", active.as_deref_mut());
                }
            }
            NetRequest::Leave => {
                if let Some(mut c) = net.client.take() {
                    c.leave();
                }
                if let Some(mut h) = net.host.take() {
                    h.shutdown();
                }
                net.pending_world = None;
                net.build = None;
                *status = NetStatus { settings: status.settings.clone(), ..Default::default() };
                if *state.get() == AppState::InGame {
                    next.set(AppState::Menu);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn net_poll(
    mut net: ResMut<NetState>,
    mut active: Option<ResMut<ActiveWorld>>,
    mut sim_cmds: MessageReader<SimCommand>,
    mut predicted: MessageWriter<PredictedCast>,
    time: Res<Time<Real>>,
    mut clock: ResMut<TickClock>,
    mut hitstop: ResMut<crate::juice::HitStopState>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let t0 = now_secs();
    let net = &mut *net;
    if net.host.is_some() || net.client.is_some() {
        // A shared world never freezes for one player's hit-stop.
        if hitstop.freeze_sim {
            hitstop.freeze_sim = false;
        }
    }
    if let Some(h) = net.host.as_mut() {
        let a = active.as_deref_mut().filter(|a| a.role == SimRole::Host);
        h.poll(a, &mut net.cmds);
        sim_cmds.clear();
    }
    if let Some(c) = net.client.as_mut() {
        let is_client_world = active.as_deref().is_some_and(|a| a.role == SimRole::Client);
        let world = if is_client_world { active.as_deref_mut().map(|a| &mut a.world) } else { net.pending_world.as_mut() };
        let out = c.update(world);
        if let Some(w) = out.new_world {
            match active.as_deref_mut().filter(|a| a.role == SimRole::Client) {
                Some(a) => a.world = w,
                None => net.pending_world = Some(w),
            }
        }
        if out.back_to_lobby && *state.get() == AppState::InGame {
            next.set(AppState::Menu);
        }
        if let Some(a) = active.as_deref_mut().filter(|a| a.role == SimRole::Client) {
            for e in c.outbox.drain(..) {
                a.push_event(e);
            }
            // Forward local commands to the host (the bridge ignores them in Client role).
            for cmd in sim_cmds.read() {
                if let Some(seq) = c.send_cmd(&cmd.cmd) {
                    if let crate::sim_bridge::Cmd::Cast { recipe, aim, .. } | crate::sim_bridge::Cmd::Signature { recipe, aim, .. } = &cmd.cmd {
                        predicted.write(PredictedCast::Started { seq, recipe: recipe.clone(), aim: *aim });
                    }
                }
            }
            for (seq, o) in c.cast_results.drain(..) {
                predicted.write(match o {
                    Some(proto::CastOutcome::Spawned(id)) => PredictedCast::Confirmed { seq, id, grafted: false },
                    Some(proto::CastOutcome::Grafted(id)) => PredictedCast::Confirmed { seq, id, grafted: true },
                    Some(proto::CastOutcome::Failed(reason)) => PredictedCast::Rejected { seq, reason: Some(reason) },
                    None => PredictedCast::Rejected { seq, reason: None },
                });
            }
            // Pacing: nudge the tick accumulator so the client runs just ahead of the host.
            if let Some(err) = c.pacing_error(clock.accumulator / DT as f64) {
                let dt = time.delta_secs_f64().min(0.1);
                clock.accumulator = (clock.accumulator + (err * 0.02).clamp(-0.05, 0.05) * dt).max(0.0);
            }
        } else {
            sim_cmds.clear();
        }
        if let ClientPhase::Failed(_) = c.phase {
            if *state.get() == AppState::InGame {
                next.set(AppState::Menu);
            }
        }
    }
    net.cost_acc = ((now_secs() - t0) * 1000.0) as f32;
}

/// Host: remote players' commands → `SimCommand::remote` (a separate system: a reader and a writer
/// of the same message type can't share one).
fn emit_remote_cmds(mut net: ResMut<NetState>, mut out: MessageWriter<SimCommand>) {
    for (id, c) in net.cmds.drain(..) {
        out.write(SimCommand::remote(id, c));
    }
}

/// Builds finishing, worlds becoming `ActiveWorld`, state transitions.
fn match_flow(
    mut commands: Commands,
    mut net: ResMut<NetState>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut mode: ResMut<GameMode>,
    active: Option<Res<ActiveWorld>>,
    mut current_map: ResMut<crate::arena::CurrentMap>,
) {
    let net = &mut *net;
    if net.build.as_ref().is_some_and(|b| b.is_finished()) {
        if let Some(Ok((w, m, th))) = net.build.take().map(|b| b.join()) {
            net.pending_world = Some(w);
            net.pending_map = m;
            net.pending_hasher = Some(th);
        }
    }
    if net.pending_world.is_none() {
        return;
    }
    match state.get() {
        AppState::Loading => {}
        AppState::Menu => {
            if let Some(c) = net.client.as_ref() {
                let want = match c.map.kind {
                    proto::NetMapKind::Miami => crate::arena::MapKind::Miami,
                    proto::NetMapKind::Orbit => crate::arena::MapKind::Orbit,
                    proto::NetMapKind::Archipelago => crate::arena::MapKind::Arena,
                };
                if current_map.0 != want {
                    current_map.0 = want;
                }
            }
            *mode = if net.host.is_some() {
                GameMode::Host
            } else {
                GameMode::Join(net.client.as_ref().map(|c| c.address.clone()).unwrap_or_default())
            };
            next.set(AppState::InGame);
        }
        AppState::InGame => {
            let Some(w) = net.pending_world.take() else { return };
            if let Some(h) = net.host.as_mut() {
                let seed = w.arena_seed();
                let mut a = ActiveWorld::new(w, Some(h.me), SimRole::Host);
                h.start_match(&mut a, net.pending_map, net.pending_hasher.take().unwrap_or_default());
                a.push_event(GameEvent::ArenaReset { seed });
                commands.insert_resource(a);
            } else if let Some(c) = net.client.as_ref() {
                let a = ActiveWorld::new(w, c.me, SimRole::Client);
                commands.insert_resource(a);
            }
            let _ = active;
        }
    }
}

fn on_exit_game(mut net: ResMut<NetState>) {
    if let Some(h) = net.host.as_mut() {
        if h.in_match {
            h.end_match();
        }
    }
    if let Some(c) = net.client.as_mut() {
        c.forget_world();
    }
}

fn host_pre_tick(mut net: ResMut<NetState>, active: Option<ResMut<ActiveWorld>>) {
    let (Some(h), Some(mut a)) = (net.host.as_mut(), active) else { return };
    if a.role == SimRole::Host && h.in_match {
        h.pre_tick(&mut a);
    }
}

fn host_post_tick(mut net: ResMut<NetState>, active: Option<ResMut<ActiveWorld>>) {
    let (Some(h), Some(mut a)) = (net.host.as_mut(), active) else { return };
    if a.role == SimRole::Host && h.in_match {
        h.post_tick(&mut a);
    }
}

fn client_predict(
    mut net: ResMut<NetState>,
    active: Option<ResMut<ActiveWorld>>,
    input: Res<LocalInputState>,
    mut move_events: ResMut<FrameMoveEvents>,
    mut positions: ResMut<FramePositions>,
    mut local_q: Query<&mut PlayerSim, With<LocalPlayer>>,
) {
    let (Some(c), Some(mut a)) = (net.client.as_mut(), active) else { return };
    if a.role != SimRole::Client {
        return;
    }
    let predicted = c.predict(&mut a.world, input.current);
    let Ok(mut sim) = local_q.single_mut() else { return };
    match predicted {
        Some((_, after, ev)) => {
            let off = c.visual_offset();
            let mut shown = after;
            shown.pos += bd_core::Vec3::new(off.x, off.y, off.z);
            let prev = sim.state;
            sim.prev = if prev.pos.distance_squared(shown.pos) > 25.0 * 25.0 { shown } else { prev };
            sim.state = shown;
            move_events.0.push(ev);
            positions.0.push((v3(sim.prev.pos), v3(sim.state.pos)));
        }
        None => {
            // Dead / loading: show the host's state.
            if let Some(p) = a.local.and_then(|id| a.world.player(id)) {
                sim.prev = sim.state;
                sim.state = p.mv;
                if sim.prev.pos.distance_squared(sim.state.pos) > 25.0 * 25.0 {
                    sim.prev = sim.state;
                }
            }
        }
    }
}

fn client_frame(
    mut net: ResMut<NetState>,
    active: Option<ResMut<ActiveWorld>>,
    mut keys: ResMut<ViewKeys>,
    clock: Res<TickClock>,
    look: Res<LookAngles>,
    local_q: Query<&PlayerSim, With<LocalPlayer>>,
) {
    let t0 = now_secs();
    let net = &mut *net;
    let Some(c) = net.client.as_mut() else { return };
    let Some(mut a) = active.filter(|a| a.role == SimRole::Client) else { return };
    c.send_inputs(false);
    let now = now_secs();
    let keys = &mut *keys;
    if let Some(it) = c.interp.update(now) {
        keys.prev.clone_from(it.from);
        keys.curr.clone_from(it.to);
        keys.alpha = it.alpha;
        keys.external = true;
        keys.valid = true;
    }
    // Remote-entity events at the same render time as the entities (this frame's flush).
    c.release_events();
    for e in c.outbox.drain(..) {
        a.push_event(e);
    }
    // The local player is drawn where it is predicted, not ~100 ms in the past.
    if let (Some(me), Ok(sim)) = (a.local, local_q.single()) {
        if keys.valid && a.world.player(me).is_some_and(|p| p.alive) {
            let pos = sim.render_pos(clock.alpha);
            let st = sim.state;
            for snap in [&mut keys.prev, &mut keys.curr] {
                patch_local(snap, me, pos, &st, &look);
            }
        }
    }
    net.cost_acc += ((now_secs() - t0) * 1000.0) as f32;
}

fn patch_local(snap: &mut WorldSnapshot, me: PlayerId, pos: Vec3, st: &bd_sim::movement::PlayerState, look: &LookAngles) {
    let Ok(i) = snap.players.binary_search_by_key(&me.0, |p| p.id) else { return };
    let p = &mut snap.players[i];
    p.pos = QPos::new(bd_core::Vec3::new(pos.x, pos.y, pos.z));
    p.vel = QVel::new(st.vel);
    p.look = QLook::new(look.yaw, look.pitch);
    for h in 0..2 {
        p.hooks[h] = (st.hooks[h].phase != bd_sim::movement::HookPhase::Idle).then(|| QPos::new(st.hooks[h].pos));
    }
}

fn update_status(mut net: ResMut<NetState>, mut status: ResMut<NetStatus>, mut last: Local<f64>, active: Option<Res<ActiveWorld>>) {
    let now = now_secs();
    let cost = net.cost_acc;
    status.stats.cost_ms += (cost - status.stats.cost_ms) * 0.05;
    if now - *last < 0.2 {
        return;
    }
    *last = now;
    let role = net.role();
    let (sent, recv) = match (&net.host, &net.client) {
        (Some(h), _) => (h.session.bytes_sent(), h.session.bytes_received()),
        (_, Some(c)) => (c.session.bytes_sent(), c.session.bytes_received()),
        _ => (0, 0),
    };
    let (ps, pr, pt) = net.last_bytes;
    let dt = (now - pt).max(1e-3);
    if pt > 0.0 {
        status.stats.up_kbps = ((sent.saturating_sub(ps)) as f64 * 8.0 / 1000.0 / dt) as f32;
        status.stats.down_kbps = ((recv.saturating_sub(pr)) as f64 * 8.0 / 1000.0 / dt) as f32;
    }
    net.last_bytes = (sent, recv, now);
    status.role = role.clone();
    let in_game = active.is_some();
    match (&net.host, &net.client) {
        (Some(h), _) => {
            status.tunnel = match h.tunnel_status() {
                None => TunnelInfo::Disabled,
                Some(TunnelStatus::Starting) => TunnelInfo::Starting,
                Some(TunnelStatus::Ready { code, url }) => TunnelInfo::Ready { code, url },
                Some(TunnelStatus::Failed(e)) => TunnelInfo::Failed(e),
            };
            status.lobby_code = h.lobby_code();
            if status.lan.is_empty() {
                status.lan = h.lan_addresses();
            }
            status.me = Some(h.me);
            let l = h.lobby();
            status.players = l.players;
            status.settings = l.settings;
            status.in_match = h.in_match;
            status.phase = if h.in_match && in_game {
                NetPhase::InGame
            } else if net.build.is_some() || net.pending_world.is_some() {
                NetPhase::Loading
            } else {
                NetPhase::Lobby
            };
            status.banner = None;
            status.stats.input_starved = h.stats.input_starved;
        }
        (_, Some(c)) => {
            status.me = c.me;
            status.players = c.lobby.players.clone();
            status.settings = c.lobby.settings.clone();
            status.in_match = c.lobby.in_match;
            status.lobby_code = bd_net::lobby_code_from_url(c.session.url());
            status.phase = match &c.phase {
                ClientPhase::Connecting => NetPhase::Connecting,
                ClientPhase::Lobby => NetPhase::Lobby,
                ClientPhase::Loading => NetPhase::Loading,
                ClientPhase::InGame => NetPhase::InGame,
                ClientPhase::Reconnecting { attempt } => NetPhase::Reconnecting { attempt: *attempt },
                ClientPhase::Failed(e) => NetPhase::Failed(e.clone()),
            };
            status.banner = match &c.phase {
                ClientPhase::Reconnecting { attempt } => {
                    Some(format!("connection lost — reconnecting ({attempt}/{})", client::RECONNECT_ATTEMPTS))
                }
                ClientPhase::Loading if in_game => Some("resyncing world…".into()),
                ClientPhase::Failed(e) if in_game => Some(format!("disconnected: {e}")),
                _ => None,
            };
            let r = c.session.clock().rtt();
            let s = &c.stats;
            status.stats.rtt_ms = (r.srtt * 1000.0) as f32;
            status.stats.jitter_ms = (r.rttvar * 1000.0) as f32;
            status.stats.corrections = s.corrections;
            status.stats.visible_corrections = s.visible_corrections;
            status.stats.max_error_m = s.max_error_m;
            status.stats.desyncs = s.desyncs;
            status.stats.terrain_checks_ok = s.terrain_checks_ok;
            status.stats.snapshots = s.snapshots;
            status.stats.interp_delay_ms = (c.interp.delay_ticks() * DT as f64 * 1000.0) as f32;
            status.stats.interp_starved = c.interp.starved_frames;
        }
        _ => {
            status.phase = match &status.phase {
                NetPhase::Failed(e) => NetPhase::Failed(e.clone()),
                _ => NetPhase::Idle,
            };
            status.banner = None;
        }
    }
}

fn spawn_net_ui(mut commands: Commands) {
    commands.spawn((
        Name::new("NetBanner"),
        NetBanner,
        Text::new(""),
        TextFont::from_font_size(20.0),
        TextColor(Color::srgb(1.0, 0.85, 0.4)),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, top: percent(12), width: percent(100), justify_content: JustifyContent::Center, ..default() },
        TextLayout::justify(Justify::Center),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(70),
    ));
    commands.spawn((
        Name::new("NetStats"),
        NetStatsText,
        Text::new(""),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb(0.75, 0.9, 1.0)),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, left: px(10), top: px(48), ..default() },
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(60),
    ));
}

fn update_net_ui(
    status: Res<NetStatus>,
    overlay: Res<crate::hud::PerfOverlay>,
    mut banner: Query<(&mut Text, &mut Visibility), (With<NetBanner>, Without<NetStatsText>)>,
    mut stats: Query<(&mut Text, &mut Visibility), (With<NetStatsText>, Without<NetBanner>)>,
    mut last: Local<f64>,
) {
    if let Ok((mut t, mut v)) = banner.single_mut() {
        let want = if status.banner.is_some() { Visibility::Visible } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
        if let Some(b) = &status.banner {
            if t.0 != *b {
                t.0 = b.clone();
            }
        }
    }
    let Ok((mut t, mut v)) = stats.single_mut() else { return };
    let show = overlay.visible && status.role != NetRole::Offline;
    let want = if show { Visibility::Visible } else { Visibility::Hidden };
    if *v != want {
        *v = want;
    }
    let now = now_secs();
    if !show || now - *last < 0.25 {
        return;
    }
    *last = now;
    let s = &status.stats;
    t.0 = match status.role {
        NetRole::Host => format!(
            "net HOST  {} players  up {:.0} kbps  down {:.0} kbps  late inputs {}  net {:.3} ms/frame{}",
            status.players.len(),
            s.up_kbps,
            s.down_kbps,
            s.input_starved,
            s.cost_ms,
            status.lobby_code.as_deref().map(|c| format!("  code {c}")).unwrap_or_default()
        ),
        _ => format!(
            "net rtt {:.0} ms (jitter {:.0})  up {:.0} kbps  down {:.0} kbps  interp {:.0} ms  corrections {} ({} visible, max {:.2} m)  terrain ok {} desync {}  net {:.3} ms/frame",
            s.rtt_ms,
            s.jitter_ms,
            s.up_kbps,
            s.down_kbps,
            s.interp_delay_ms,
            s.corrections,
            s.visible_corrections,
            s.max_error_m,
            s.terrain_checks_ok,
            s.desyncs,
            s.cost_ms
        ),
    };
}
