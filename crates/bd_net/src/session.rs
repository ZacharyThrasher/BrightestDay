//! Host / client sessions, generic over the game's message types.
//!
//! `C` = client → host message type, `S` = host → client message type. Both only need serde.
//! Each has two lanes:
//! - **reliable** (`send`, `broadcast`): ordered, never dropped — events, casts, chat, carve ops.
//! - **latest** (`send_latest`, `broadcast_latest`): for snapshots. If a slow link backs up, only the
//!   newest queued message is written, and the client keeps a small [`LatestLane`] that discards
//!   the oldest when the game falls behind.
//!
//! Neither side ever blocks: call `poll()` in a `while let` loop once per frame.

use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use bd_core::corps::Corps;
use bd_core::{MAX_PLAYERS, PlayerId, TICK_HZ};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::clock::ClockSync;
use crate::latest::LatestLane;
use crate::transport::{ClientTransport, ClientTransportEvent, ConnId, HostTransport, HostTransportEvent, PeerTx, data_frame};
use crate::{DEFAULT_PORT, NetError, PROTOCOL_MAGIC, PROTOCOL_VERSION, codec, now_secs};

/// Longest player name (characters).
pub const MAX_NAME_CHARS: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: PlayerId,
    pub name: String,
    pub corps: Corps,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaveReason {
    /// Left on purpose.
    Quit,
    Kicked(String),
    /// Connection dropped / timed out.
    ConnectionLost(String),
}

#[derive(Serialize, Deserialize)]
enum ClientWire<C> {
    Hello { magic: u32, protocol: u32, name: String, corps: Corps },
    App(C),
    Bye,
}

#[derive(Serialize, Deserialize)]
enum HostWire<S> {
    Welcome { you: PlayerId, players: Vec<PlayerInfo>, tick_hz: u32 },
    Reject(String),
    Joined(PlayerInfo),
    Left(PlayerId, LeaveReason),
    Kicked(String),
    App(S),
    Latest(S),
}

fn frame<T: Serialize>(msg: &T) -> bytes::Bytes {
    data_frame(|v| codec::encode_into(msg, v))
}

fn sanitize_name(raw: &str) -> String {
    let s: String = raw.chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
    let s = s.trim();
    if s.is_empty() { "Lantern".to_string() } else { s.to_string() }
}

// =============================================================================================
// Host
// =============================================================================================

/// The listen-server's own player (it never goes through the network).
#[derive(Clone, Debug)]
pub struct LocalPlayer {
    pub name: String,
    pub corps: Corps,
}

#[derive(Clone, Debug)]
pub struct HostConfig {
    /// Address to listen on. `0.0.0.0` for LAN + tunnel. `127.0.0.1` is enough when only the
    /// cloudflared tunnel is used, and avoids the Windows firewall prompt.
    pub bind: IpAddr,
    pub port: u16,
    /// If `port` is taken, try the next few and then any free port.
    pub port_fallback: bool,
    /// Including the local player.
    pub max_players: usize,
    /// The host's own player; gets `PlayerId(0)`.
    pub local_player: Option<LocalPlayer>,
    /// Give each player a distinct corps (reassigns duplicates to the first free one).
    pub unique_corps: bool,
    /// Connections that do not complete the handshake in this time are dropped.
    pub handshake_timeout: Duration,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: DEFAULT_PORT,
            port_fallback: true,
            max_players: MAX_PLAYERS,
            local_player: None,
            unique_corps: true,
            handshake_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent<C> {
    PlayerJoined(PlayerInfo),
    PlayerLeft { id: PlayerId, reason: LeaveReason },
    Message { from: PlayerId, msg: C },
}

struct Conn {
    tx: PeerTx,
    player: Option<PlayerId>,
    since: f64,
    remote: String,
    rtt: f32,
}

pub struct HostSession<C, S> {
    transport: HostTransport,
    cfg: HostConfig,
    conns: HashMap<ConnId, Conn>,
    players: Vec<PlayerInfo>,
    conn_of: [Option<ConnId>; MAX_PLAYERS],
    pending: VecDeque<HostEvent<C>>,
    last_timeout_check: f64,
    bytes_sent: u64,
    bytes_received: u64,
    _types: PhantomData<fn(C) -> S>,
}

impl<C, S> HostSession<C, S>
where
    C: DeserializeOwned,
    S: Serialize,
{
    /// Start listening. Non-blocking afterwards; see [`port`](Self::port) for the actual port.
    pub fn start(cfg: HostConfig) -> Result<Self, NetError> {
        let transport = HostTransport::listen(cfg.bind, cfg.port, cfg.port_fallback, TICK_HZ)?;
        let mut players = Vec::new();
        if let Some(lp) = &cfg.local_player {
            players.push(PlayerInfo { id: PlayerId(0), name: sanitize_name(&lp.name), corps: lp.corps });
        }
        Ok(Self {
            transport,
            cfg: HostConfig { max_players: cfg.max_players.clamp(1, MAX_PLAYERS), ..cfg },
            conns: HashMap::new(),
            players,
            conn_of: [None; MAX_PLAYERS],
            pending: VecDeque::new(),
            last_timeout_check: now_secs(),
            bytes_sent: 0,
            bytes_received: 0,
            _types: PhantomData,
        })
    }

    /// The port actually listened on (pass this to [`crate::Tunnel::start`]).
    pub fn port(&self) -> u16 {
        self.transport.local_addr().port()
    }

    /// Tell the network thread the host's current simulation tick (call every tick). Pings are
    /// answered with this so clients can sync their clocks to the simulation.
    pub fn set_tick(&self, tick: u64) {
        self.transport.clock.set_tick(tick);
    }

    /// All players including the local one, sorted by id.
    pub fn players(&self) -> &[PlayerInfo] {
        &self.players
    }

    pub fn player(&self, id: PlayerId) -> Option<&PlayerInfo> {
        self.players.iter().find(|p| p.id == id)
    }

    /// RTT (seconds) reported by a remote player; `None` for the local player / unknown ids.
    pub fn rtt(&self, id: PlayerId) -> Option<f32> {
        let c = self.conns.get(&self.conn_of.get(id.0 as usize).copied().flatten()?)?;
        (c.rtt > 0.0).then_some(c.rtt)
    }

    /// Remote address (the real client IP when it came through the Cloudflare tunnel).
    pub fn remote_addr(&self, id: PlayerId) -> Option<&str> {
        let c = self.conns.get(&self.conn_of.get(id.0 as usize).copied().flatten()?)?;
        Some(&c.remote)
    }

    /// Total payload bytes queued for sending / received (for bandwidth stats).
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent
    }
    pub fn bytes_received(&self) -> u64 {
        self.bytes_received
    }

    fn tx_of(&self, id: PlayerId) -> Option<&PeerTx> {
        let conn = self.conn_of.get(id.0 as usize).copied().flatten()?;
        self.conns.get(&conn).map(|c| &c.tx)
    }

    /// Send on the reliable lane. Returns false for the local player / unknown ids.
    pub fn send(&mut self, to: PlayerId, msg: &S) -> bool {
        let f = frame(&HostWire::App(msg));
        self.send_frame(to, f, false)
    }

    /// Send on the latest-only lane (snapshots).
    pub fn send_latest(&mut self, to: PlayerId, msg: &S) -> bool {
        let f = frame(&HostWire::Latest(msg));
        self.send_frame(to, f, true)
    }

    fn send_frame(&mut self, to: PlayerId, f: bytes::Bytes, latest: bool) -> bool {
        let len = f.len() as u64;
        let Some(tx) = self.tx_of(to) else { return false };
        let ok = if latest { tx.send_latest(f) } else { tx.send(f) };
        if ok {
            self.bytes_sent += len;
        }
        ok
    }

    /// Reliable message to every remote player (encoded once).
    pub fn broadcast(&mut self, msg: &S) {
        let f = frame(&HostWire::App(msg));
        self.broadcast_frame(f, None, false);
    }

    /// Reliable message to every remote player except one.
    pub fn broadcast_except(&mut self, except: PlayerId, msg: &S) {
        let f = frame(&HostWire::App(msg));
        self.broadcast_frame(f, Some(except), false);
    }

    /// Latest-lane message to every remote player (encoded once).
    pub fn broadcast_latest(&mut self, msg: &S) {
        let f = frame(&HostWire::Latest(msg));
        self.broadcast_frame(f, None, true);
    }

    fn broadcast_frame(&mut self, f: bytes::Bytes, except: Option<PlayerId>, latest: bool) {
        for p in &self.players {
            if Some(p.id) == except {
                continue;
            }
            if let Some(conn) = self.conn_of[p.id.0 as usize] {
                if let Some(c) = self.conns.get(&conn) {
                    let ok = if latest { c.tx.send_latest(f.clone()) } else { c.tx.send(f.clone()) };
                    if ok {
                        self.bytes_sent += f.len() as u64;
                    }
                }
            }
        }
    }

    /// Remove a remote player. They receive the reason; everyone else gets `PlayerLeft`.
    /// A `PlayerLeft` event is also queued locally.
    pub fn kick(&mut self, id: PlayerId, reason: &str) {
        let Some(conn) = self.conn_of.get(id.0 as usize).copied().flatten() else { return };
        if let Some(c) = self.conns.remove(&conn) {
            c.tx.send(frame(&HostWire::<()>::Kicked(reason.to_string())));
            c.tx.close();
        }
        self.remove_player(id, LeaveReason::Kicked(reason.to_string()));
    }

    fn remove_player(&mut self, id: PlayerId, reason: LeaveReason) {
        self.conn_of[id.0 as usize] = None;
        self.players.retain(|p| p.id != id);
        let f = frame(&HostWire::<()>::Left(id, reason.clone()));
        self.broadcast_frame(f, None, false);
        self.pending.push_back(HostEvent::PlayerLeft { id, reason });
    }

    /// Next event, or `None` when there is nothing more this frame. Never blocks.
    pub fn poll(&mut self) -> Option<HostEvent<C>> {
        let now = now_secs();
        if now - self.last_timeout_check > 0.25 {
            self.last_timeout_check = now;
            let limit = self.cfg.handshake_timeout.as_secs_f64();
            self.conns.retain(|_, c| {
                let keep = c.player.is_some() || now - c.since < limit;
                if !keep {
                    c.tx.close();
                }
                keep
            });
        }
        loop {
            if let Some(e) = self.pending.pop_front() {
                return Some(e);
            }
            let ev = self.transport.events.try_recv().ok()?;
            self.handle(ev);
        }
    }

    fn handle(&mut self, ev: HostTransportEvent) {
        match ev {
            HostTransportEvent::Connected { conn, remote, tx } => {
                self.conns.insert(conn, Conn { tx, player: None, since: now_secs(), remote, rtt: 0.0 });
            }
            HostTransportEvent::PeerRtt { conn, rtt } => {
                if let Some(c) = self.conns.get_mut(&conn) {
                    c.rtt = if c.rtt == 0.0 { rtt } else { c.rtt + (rtt - c.rtt) * 0.2 };
                }
            }
            HostTransportEvent::Disconnected { conn, reason } => {
                if let Some(c) = self.conns.remove(&conn) {
                    if let Some(id) = c.player {
                        self.remove_player(id, LeaveReason::ConnectionLost(reason));
                    }
                }
            }
            HostTransportEvent::Data { conn, bytes } => {
                self.bytes_received += bytes.len() as u64 + 1;
                let Some(c) = self.conns.get(&conn) else { return };
                let player = c.player;
                match (player, codec::decode::<ClientWire<C>>(&bytes)) {
                    (Some(from), Ok(ClientWire::App(msg))) => self.pending.push_back(HostEvent::Message { from, msg }),
                    (Some(from), Ok(ClientWire::Bye)) => {
                        if let Some(c) = self.conns.remove(&conn) {
                            c.tx.close();
                        }
                        self.remove_player(from, LeaveReason::Quit);
                    }
                    (Some(_), Ok(ClientWire::Hello { .. })) => {}
                    (Some(from), Err(e)) => self.kick(from, &format!("protocol error: {e}")),
                    (None, Ok(ClientWire::Hello { magic, protocol, name, corps })) => self.handshake(conn, magic, protocol, name, corps),
                    (None, _) => self.reject(conn, "expected handshake"),
                }
            }
        }
    }

    fn reject(&mut self, conn: ConnId, reason: &str) {
        if let Some(c) = self.conns.remove(&conn) {
            c.tx.send(frame(&HostWire::<()>::Reject(reason.to_string())));
            c.tx.close();
        }
    }

    fn handshake(&mut self, conn: ConnId, magic: u32, protocol: u32, name: String, corps: Corps) {
        if magic != PROTOCOL_MAGIC {
            return self.reject(conn, "not a Brightest Day client");
        }
        if protocol != PROTOCOL_VERSION {
            return self.reject(
                conn,
                &format!("version mismatch: host has protocol v{PROTOCOL_VERSION}, you have v{protocol} — update your game"),
            );
        }
        let Some(id) = (0..self.cfg.max_players as u8).map(PlayerId).find(|id| self.players.iter().all(|p| p.id != *id)) else {
            return self.reject(conn, &format!("lobby is full ({} players)", self.cfg.max_players));
        };
        // Unique, sanitized name.
        let base = sanitize_name(&name);
        let mut name = base.clone();
        let mut n = 2;
        while self.players.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
            name = format!("{base} {n}");
            n += 1;
        }
        let mut corps = corps;
        if self.cfg.unique_corps && self.players.iter().any(|p| p.corps == corps) {
            let start = corps.index();
            if let Some(free) = (0..8u8).map(|i| Corps::from_index(start + i)).find(|c| self.players.iter().all(|p| p.corps != *c)) {
                corps = free;
            }
        }
        let info = PlayerInfo { id, name, corps };
        // Tell everyone else first, then add and welcome the newcomer with the full list.
        let joined = frame(&HostWire::<()>::Joined(info.clone()));
        self.broadcast_frame(joined, None, false);
        self.players.push(info.clone());
        self.players.sort_by_key(|p| p.id);
        self.conn_of[id.0 as usize] = Some(conn);
        let welcome = frame(&HostWire::<()>::Welcome { you: id, players: self.players.clone(), tick_hz: TICK_HZ });
        let c = self.conns.get_mut(&conn).unwrap();
        c.player = Some(id);
        c.tx.send(welcome);
        self.pending.push_back(HostEvent::PlayerJoined(info));
    }

    /// Close every connection and stop listening.
    pub fn shutdown(&mut self) {
        for c in self.conns.values() {
            c.tx.send(frame(&HostWire::<()>::Kicked("host closed the lobby".into())));
            c.tx.close();
        }
        self.conns.clear();
        // Writers flush what is queued (the goodbye) before closing.
        self.transport.shutdown();
    }
}

impl<C, S> Drop for HostSession<C, S> {
    fn drop(&mut self) {
        for c in self.conns.values() {
            c.tx.close();
        }
    }
}

// =============================================================================================
// Client
// =============================================================================================

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub name: String,
    pub corps: Corps,
    /// Keep retrying the connection for this long (fresh tunnels need a few seconds of DNS).
    pub connect_timeout: Duration,
    /// Ping rate for RTT / clock sync.
    pub ping_interval: Duration,
    /// Latest-lane messages kept when the game falls behind.
    pub latest_capacity: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            name: "Lantern".into(),
            corps: Corps::Willpower,
            connect_timeout: Duration::from_secs(15),
            ping_interval: Duration::from_millis(100),
            latest_capacity: 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientState {
    Connecting,
    Handshaking,
    Connected,
    Disconnected(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientEvent<S> {
    /// Handshake done. `players` includes you.
    Welcome {
        you: PlayerId,
        players: Vec<PlayerInfo>,
    },
    PlayerJoined(PlayerInfo),
    PlayerLeft {
        id: PlayerId,
        reason: LeaveReason,
    },
    /// Reliable-lane message. (Latest-lane messages go to [`ClientSession::drain_latest`].)
    Message(S),
    /// Connection failed, was rejected, kicked, or dropped. Final.
    Disconnected(String),
}

pub struct ClientSession<C, S> {
    transport: ClientTransport,
    url: String,
    state: ClientState,
    me: Option<PlayerId>,
    players: Vec<PlayerInfo>,
    clock: ClockSync,
    latest: LatestLane<(f64, S)>,
    bytes_sent: u64,
    bytes_received: u64,
    _types: PhantomData<fn(C)>,
}

impl<C, S> ClientSession<C, S>
where
    C: Serialize,
    S: DeserializeOwned,
{
    /// Start connecting to an address or lobby code (see [`crate::resolve_address`]).
    /// Only fails immediately for unparseable addresses; connection errors arrive as
    /// [`ClientEvent::Disconnected`].
    pub fn connect(address: &str, cfg: ClientConfig) -> Result<Self, NetError> {
        let url = crate::resolve_address(address)?;
        let transport = ClientTransport::connect(url.clone(), cfg.connect_timeout, cfg.ping_interval);
        let hello =
            frame(&ClientWire::<()>::Hello { magic: PROTOCOL_MAGIC, protocol: PROTOCOL_VERSION, name: cfg.name.clone(), corps: cfg.corps });
        let bytes_sent = hello.len() as u64;
        transport.tx.send(hello);
        Ok(Self {
            transport,
            url,
            state: ClientState::Connecting,
            me: None,
            players: Vec::new(),
            clock: ClockSync::new(TICK_HZ),
            latest: LatestLane::new(cfg.latest_capacity),
            bytes_sent,
            bytes_received: 0,
            _types: PhantomData,
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn state(&self) -> &ClientState {
        &self.state
    }

    pub fn is_connected(&self) -> bool {
        self.state == ClientState::Connected
    }

    /// Our player id once welcomed.
    pub fn my_id(&self) -> Option<PlayerId> {
        self.me
    }

    pub fn players(&self) -> &[PlayerInfo] {
        &self.players
    }

    /// RTT statistics and host clock estimate.
    pub fn clock(&self) -> &ClockSync {
        &self.clock
    }

    /// Estimated current host tick (fractional).
    pub fn host_tick_now(&self) -> f64 {
        self.clock.host_tick(now_secs())
    }

    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent
    }
    pub fn bytes_received(&self) -> u64 {
        self.bytes_received
    }

    /// Send on the reliable lane. Messages sent before the handshake completes are queued.
    pub fn send(&mut self, msg: &C) -> bool {
        if matches!(self.state, ClientState::Disconnected(_)) {
            return false;
        }
        let f = frame(&ClientWire::App(msg));
        self.bytes_sent += f.len() as u64;
        self.transport.tx.send(f)
    }

    /// Say goodbye and close. A final `Disconnected` event is not emitted for a local disconnect.
    pub fn disconnect(&mut self) {
        if !matches!(self.state, ClientState::Disconnected(_)) {
            self.transport.tx.send(frame(&ClientWire::<()>::Bye));
            self.transport.tx.close();
            self.state = ClientState::Disconnected("disconnected".into());
        }
    }

    /// Latest-lane messages received so far, oldest first, with local receive time (seconds,
    /// [`now_secs`] clock). At most `latest_capacity` are kept.
    pub fn drain_latest(&mut self) -> impl Iterator<Item = (f64, S)> + '_ {
        self.latest.drain()
    }

    /// Only the newest latest-lane message; older ones are discarded.
    pub fn take_latest(&mut self) -> Option<(f64, S)> {
        self.latest.take_latest()
    }

    /// Latest-lane messages dropped because the game did not drain them in time.
    pub fn latest_dropped(&self) -> u64 {
        self.latest.dropped
    }

    /// Next event, or `None` when there is nothing more this frame. Never blocks.
    pub fn poll(&mut self) -> Option<ClientEvent<S>> {
        loop {
            let ev = self.transport.events.try_recv().ok()?;
            if let Some(e) = self.handle(ev) {
                return Some(e);
            }
        }
    }

    fn fail(&mut self, reason: String) -> Option<ClientEvent<S>> {
        if matches!(self.state, ClientState::Disconnected(_)) {
            return None;
        }
        self.state = ClientState::Disconnected(reason.clone());
        self.transport.tx.close();
        Some(ClientEvent::Disconnected(reason))
    }

    fn handle(&mut self, ev: ClientTransportEvent) -> Option<ClientEvent<S>> {
        match ev {
            ClientTransportEvent::Connected => {
                if self.state == ClientState::Connecting {
                    self.state = ClientState::Handshaking;
                }
                None
            }
            ClientTransportEvent::Pong { rtt, host_tick, recv_at } => {
                self.clock.add_sample(rtt, host_tick, recv_at);
                None
            }
            ClientTransportEvent::Disconnected(reason) => self.fail(reason),
            ClientTransportEvent::Data(bytes) => {
                if matches!(self.state, ClientState::Disconnected(_)) {
                    return None;
                }
                self.bytes_received += bytes.len() as u64 + 1;
                let msg = match codec::decode::<HostWire<S>>(&bytes) {
                    Ok(m) => m,
                    Err(e) => return self.fail(format!("protocol error: {e}")),
                };
                match msg {
                    HostWire::Welcome { you, players, tick_hz } => {
                        if tick_hz != TICK_HZ {
                            return self.fail(format!("host runs at {tick_hz} Hz, we run at {TICK_HZ} Hz"));
                        }
                        self.state = ClientState::Connected;
                        self.me = Some(you);
                        self.players = players.clone();
                        Some(ClientEvent::Welcome { you, players })
                    }
                    HostWire::Reject(reason) => self.fail(format!("rejected: {reason}")),
                    HostWire::Kicked(reason) => self.fail(format!("kicked: {reason}")),
                    HostWire::Joined(info) => {
                        self.players.retain(|p| p.id != info.id);
                        self.players.push(info.clone());
                        self.players.sort_by_key(|p| p.id);
                        Some(ClientEvent::PlayerJoined(info))
                    }
                    HostWire::Left(id, reason) => {
                        self.players.retain(|p| p.id != id);
                        Some(ClientEvent::PlayerLeft { id, reason })
                    }
                    HostWire::App(m) => Some(ClientEvent::Message(m)),
                    HostWire::Latest(m) => {
                        self.latest.push((now_secs(), m));
                        None
                    }
                }
            }
        }
    }
}

impl<C, S> Drop for ClientSession<C, S> {
    fn drop(&mut self) {
        if !matches!(self.state, ClientState::Disconnected(_)) {
            self.transport.tx.send(data_frame(|v| codec::encode_into(&ClientWire::<()>::Bye, v)));
        }
        // The writer flushes the goodbye before closing when the transport shuts down.
        self.transport.tx.close();
    }
}
