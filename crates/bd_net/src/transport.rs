//! WebSocket transport on a shared background tokio runtime.
//!
//! The game thread never blocks: it pushes outgoing frames into per-connection unbounded channels
//! and drains incoming events from a crossbeam channel. Every frame is a binary WebSocket message
//! whose first byte is a kind tag:
//!
//! | kind | payload |
//! |---|---|
//! | `0` DATA | bincode session message |
//! | `1` PING (client → host) | client send time `f64` + client's last RTT `f32` (seconds) |
//! | `2` PONG (host → client) | echoed client time `f64` + host tick `f64` at reply time |
//!
//! Pings are answered directly on the network thread so RTT does not include the host's frame time.
//! Outgoing messages are batched (one flush per burst) and latest-lane messages that queue up behind
//! a slow socket are coalesced so only the newest is written.

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use crossbeam_channel::{Receiver, Sender, unbounded};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{Connector, WebSocketStream};

use crate::{NetError, now_secs};

pub const K_DATA: u8 = 0;
pub const K_PING: u8 = 1;
pub const K_PONG: u8 = 2;

/// Identifies one accepted connection on the host (never reused within a session).
pub type ConnId = u64;

/// Close connections that send nothing (not even pings) for this long.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(10);

/// The shared network runtime (2 worker threads named `bd-net`).
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("bd-net")
            .enable_all()
            .build()
            .expect("bd_net: failed to start tokio runtime")
    })
}

/// Most bytes a connection may have queued for sending before the peer is considered dead (it has
/// stopped reading). Room for two maximum-size messages; beyond that the peer is dropped instead of
/// letting its backlog grow without bound.
pub const MAX_QUEUED_BYTES: usize = 2 * crate::codec::MAX_MESSAGE_BYTES;

/// Client side: accepts host messages up to [`crate::codec::MAX_MESSAGE_BYTES`] (keyframes).
fn ws_config() -> WebSocketConfig {
    WebSocketConfig::default().max_message_size(Some(crate::codec::MAX_MESSAGE_BYTES)).max_frame_size(Some(crate::codec::MAX_MESSAGE_BYTES))
}

/// Host side: clients only ever send small messages (inputs, casts, chat), so anything larger than
/// [`crate::codec::MAX_CLIENT_MESSAGE_BYTES`] drops the connection before it is even buffered.
fn ws_config_host() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(crate::codec::MAX_CLIENT_MESSAGE_BYTES))
        .max_frame_size(Some(crate::codec::MAX_CLIENT_MESSAGE_BYTES))
}

fn tls_connector() -> Connector {
    static CFG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    let cfg = CFG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("bd_net: rustls protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(cfg)
    });
    Connector::Rustls(cfg.clone())
}

/// Prefix a payload with the DATA kind byte.
pub fn data_frame(build: impl FnOnce(&mut Vec<u8>)) -> Bytes {
    let mut v = Vec::with_capacity(64);
    v.push(K_DATA);
    build(&mut v);
    Bytes::from(v)
}

pub(crate) enum Out {
    Reliable(Bytes),
    Latest(Bytes),
    Close,
}

impl Out {
    fn len(&self) -> usize {
        match self {
            Out::Reliable(b) | Out::Latest(b) => b.len(),
            Out::Close => 0,
        }
    }
}

/// Handle for sending frames to one peer. Cheap to clone; never blocks.
///
/// The queue is bounded by [`MAX_QUEUED_BYTES`]: a peer that stops reading (its socket backs up) is
/// disconnected instead of growing our memory forever.
#[derive(Clone)]
pub struct PeerTx {
    tx: mpsc::UnboundedSender<Out>,
    queued: Arc<AtomicUsize>,
    kill: Arc<watch::Sender<bool>>,
}

/// Receiving side of a [`PeerTx`], owned by the connection's tasks.
pub(crate) struct PeerRx {
    rx: mpsc::UnboundedReceiver<Out>,
    queued: Arc<AtomicUsize>,
    kill: watch::Receiver<bool>,
}

fn peer_channel() -> (PeerTx, PeerRx) {
    let (tx, rx) = mpsc::unbounded_channel();
    let queued = Arc::new(AtomicUsize::new(0));
    let (kill_tx, kill_rx) = watch::channel(false);
    (PeerTx { tx, queued: queued.clone(), kill: Arc::new(kill_tx) }, PeerRx { rx, queued, kill: kill_rx })
}

impl PeerTx {
    /// Queue a frame on the reliable ordered lane. Returns false if the connection is gone (or was
    /// just dropped because its send queue overflowed).
    pub fn send(&self, frame: Bytes) -> bool {
        self.enqueue(Out::Reliable(frame))
    }

    /// Queue a frame on the latest-only lane: if several are waiting for the socket, only the
    /// newest is written.
    pub fn send_latest(&self, frame: Bytes) -> bool {
        self.enqueue(Out::Latest(frame))
    }

    fn enqueue(&self, out: Out) -> bool {
        if *self.kill.borrow() {
            return false;
        }
        let n = out.len();
        if self.queued.fetch_add(n, Ordering::Relaxed) + n > MAX_QUEUED_BYTES {
            self.queued.fetch_sub(n, Ordering::Relaxed);
            crate::debug_log(|| format!("send queue overflow ({MAX_QUEUED_BYTES} bytes): dropping peer"));
            let _ = self.kill.send(true);
            return false;
        }
        self.tx.send(out).is_ok()
    }

    /// Bytes queued and not yet handed to the socket.
    pub fn queued_bytes(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }

    /// Flush what is queued, then close.
    pub fn close(&self) {
        let _ = self.tx.send(Out::Close);
    }
}

/// The host's simulation clock as seen by the network thread (for PONG replies).
pub struct HostClock {
    inner: Mutex<(u64, f64)>,
    explicit: AtomicBool,
    hz: f64,
}

impl HostClock {
    pub fn new(tick_hz: u32) -> Self {
        Self { inner: Mutex::new((0, now_secs())), explicit: AtomicBool::new(false), hz: tick_hz as f64 }
    }

    /// The host simulation just finished/started `tick`.
    pub fn set_tick(&self, tick: u64) {
        *self.inner.lock().unwrap() = (tick, now_secs());
        self.explicit.store(true, Ordering::Relaxed);
    }

    /// Current fractional host tick. Before the game calls `set_tick`, ticks are derived from
    /// wall time since the session started.
    pub fn tick_now(&self) -> f64 {
        let (t, at) = *self.inner.lock().unwrap();
        let ticks = (now_secs() - at) * self.hz;
        if self.explicit.load(Ordering::Relaxed) { t as f64 + ticks.clamp(0.0, 4.0) } else { t as f64 + ticks }
    }
}

// ---------------------------------------------------------------------------------------------
// Writer / reader tasks
// ---------------------------------------------------------------------------------------------

struct PingCfg {
    interval: Duration,
    last_rtt: Arc<AtomicU32>,
}

type WsSink<S> = futures_util::stream::SplitSink<WebSocketStream<S>, Message>;

/// Write a batch, skipping latest-lane messages superseded within the batch.
/// Returns Ok(true) if a Close was requested.
async fn write_batch<S>(sink: &mut WsSink<S>, batch: &mut Vec<Out>, queued: &AtomicUsize) -> Result<bool, ()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let last_latest = batch.iter().rposition(|m| matches!(m, Out::Latest(_)));
    let mut close = false;
    for (i, m) in batch.drain(..).enumerate() {
        queued.fetch_sub(m.len(), Ordering::Relaxed);
        let r = match m {
            Out::Reliable(b) => sink.feed(Message::Binary(b)).await,
            Out::Latest(b) if Some(i) == last_latest => sink.feed(Message::Binary(b)).await,
            Out::Latest(_) => Ok(()), // stale: a newer one is in this batch
            Out::Close => {
                close = true;
                break;
            }
        };
        r.map_err(|_| ())?;
    }
    sink.flush().await.map_err(|_| ())?;
    Ok(close)
}

fn drain_into(rx: &mut mpsc::UnboundedReceiver<Out>, batch: &mut Vec<Out>) {
    while batch.len() < 512 {
        match rx.try_recv() {
            Ok(m) => batch.push(m),
            Err(_) => break,
        }
    }
}

async fn writer<S>(mut sink: WsSink<S>, peer: PeerRx, mut shutdown: watch::Receiver<bool>, ping: Option<PingCfg>)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let PeerRx { mut rx, queued, mut kill } = peer;
    let mut ping_timer = tokio::time::interval(ping.as_ref().map(|p| p.interval).unwrap_or(Duration::from_secs(3600)));
    ping_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut batch: Vec<Out> = Vec::with_capacity(64);
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                // Graceful: flush whatever the game queued (goodbyes, kicks), then close.
                drain_into(&mut rx, &mut batch);
                let _ = tokio::time::timeout(Duration::from_millis(500), async {
                    let _ = write_batch(&mut sink, &mut batch, &queued).await;
                    let _ = sink.send(Message::Close(None)).await;
                }).await;
                return;
            }
            _ = kill.changed() => return,
            _ = ping_timer.tick(), if ping.is_some() => {
                let p = ping.as_ref().unwrap();
                let mut f = Vec::with_capacity(13);
                f.push(K_PING);
                f.extend_from_slice(&now_secs().to_le_bytes());
                f.extend_from_slice(&f32::from_bits(p.last_rtt.load(Ordering::Relaxed)).to_le_bytes());
                if sink.send(Message::Binary(Bytes::from(f))).await.is_err() {
                    return;
                }
            }
            first = rx.recv() => {
                let Some(first) = first else {
                    let _ = sink.send(Message::Close(None)).await;
                    return;
                };
                batch.push(first);
                drain_into(&mut rx, &mut batch);
                // A peer that stopped reading blocks this write forever: the kill switch (send-queue
                // overflow) must be able to interrupt it.
                let written = tokio::select! {
                    r = write_batch(&mut sink, &mut batch, &queued) => r,
                    _ = kill.changed() => return,
                };
                match written {
                    Ok(false) => {}
                    Ok(true) => {
                        let _ = sink.send(Message::Close(None)).await;
                        let _ = sink.close().await;
                        return;
                    }
                    Err(()) => return,
                }
            }
        }
    }
}

/// Reads frames until the connection ends; returns the reason.
async fn reader<S>(
    mut stream: futures_util::stream::SplitStream<WebSocketStream<S>>,
    mut shutdown: watch::Receiver<bool>,
    mut kill: watch::Receiver<bool>,
    mut on_frame: impl FnMut(Bytes) -> bool,
) -> String
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        tokio::select! {
            _ = kill.changed() => return "send queue overflow: peer stopped reading".into(),
            _ = shutdown.changed() => {
                // Keep reading (and discarding) until the peer acknowledges our Close. Closing a
                // socket with unread data makes Windows send RST, which can destroy our final
                // messages (goodbye / kick reason) before the peer reads them.
                let _ = tokio::time::timeout(Duration::from_secs(1), async {
                    while let Some(Ok(m)) = stream.next().await {
                        if matches!(m, Message::Close(_)) {
                            break;
                        }
                    }
                })
                .await;
                return "shut down".into();
            }
            r = tokio::time::timeout(IDLE_TIMEOUT, stream.next()) => match r {
                Err(_) => return "timed out".into(),
                Ok(None) => return "connection closed".into(),
                Ok(Some(Err(e))) => return format!("connection lost: {e}"),
                Ok(Some(Ok(Message::Binary(b)))) => {
                    if !b.is_empty() && !on_frame(b) {
                        return "local side closed".into();
                    }
                }
                Ok(Some(Ok(Message::Close(_)))) => return "closed by peer".into(),
                Ok(Some(Ok(_))) => {} // text / ws-level ping/pong: ignore
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Host
// ---------------------------------------------------------------------------------------------

pub enum HostTransportEvent {
    Connected {
        conn: ConnId,
        remote: String,
        tx: PeerTx,
    },
    Data {
        conn: ConnId,
        bytes: Bytes,
    },
    /// RTT (seconds) the client reported in its last ping.
    PeerRtt {
        conn: ConnId,
        rtt: f32,
    },
    Disconnected {
        conn: ConnId,
        reason: String,
    },
}

pub struct HostTransport {
    pub events: Receiver<HostTransportEvent>,
    pub clock: Arc<HostClock>,
    addr: SocketAddr,
    shutdown: watch::Sender<bool>,
}

impl HostTransport {
    /// Listen on `bind:port`. With `fallback`, tries the next 10 ports and then any free port.
    pub fn listen(bind: IpAddr, port: u16, fallback: bool, tick_hz: u32) -> Result<Self, NetError> {
        let mut candidates: Vec<u16> = vec![port];
        if fallback {
            candidates.extend((1..=10).filter_map(|i| port.checked_add(i)));
            candidates.push(0);
        }
        let mut last_err = String::new();
        let std_listener = candidates.iter().find_map(|p| match std::net::TcpListener::bind((bind, *p)) {
            Ok(l) => Some(l),
            Err(e) => {
                last_err = format!("{bind}:{p}: {e}");
                None
            }
        });
        let std_listener = std_listener.ok_or(NetError::Bind(last_err))?;
        std_listener.set_nonblocking(true).map_err(|e| NetError::Bind(e.to_string()))?;
        let addr = std_listener.local_addr().map_err(|e| NetError::Bind(e.to_string()))?;
        let rt = runtime();
        let listener = {
            let _g = rt.enter();
            TcpListener::from_std(std_listener).map_err(|e| NetError::Bind(e.to_string()))?
        };
        let (ev_tx, ev_rx) = unbounded();
        let (sd_tx, sd_rx) = watch::channel(false);
        let clock = Arc::new(HostClock::new(tick_hz));
        rt.spawn(accept_loop(listener, ev_tx, sd_rx, clock.clone()));
        Ok(Self { events: ev_rx, clock, addr, shutdown: sd_tx })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }
}

impl Drop for HostTransport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn accept_loop(
    listener: TcpListener,
    events: Sender<HostTransportEvent>,
    mut shutdown: watch::Receiver<bool>,
    clock: Arc<HostClock>,
) {
    let mut next_conn: ConnId = 1;
    loop {
        tokio::select! {
            _ = shutdown.changed() => return,
            r = listener.accept() => match r {
                Ok((stream, addr)) => {
                    let _ = stream.set_nodelay(true);
                    let conn = next_conn;
                    next_conn += 1;
                    tokio::spawn(host_conn(stream, addr, conn, events.clone(), shutdown.clone(), clock.clone()));
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    }
}

async fn host_conn(
    stream: tokio::net::TcpStream,
    addr: SocketAddr,
    conn: ConnId,
    events: Sender<HostTransportEvent>,
    shutdown: watch::Receiver<bool>,
    clock: Arc<HostClock>,
) {
    let mut forwarded: Option<String> = None;
    let cb = |req: &Request, resp: Response| {
        let h = req.headers();
        forwarded = h
            .get("cf-connecting-ip")
            .or_else(|| h.get("x-forwarded-for"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());
        Ok::<_, ErrorResponse>(resp)
    };
    let ws = match tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::accept_hdr_async_with_config(stream, cb, Some(ws_config_host())),
    )
    .await
    {
        Ok(Ok(ws)) => ws,
        _ => return, // not a websocket client / too slow: never surfaced to the game
    };
    let remote = forwarded.map(|f| format!("{f} (via tunnel)")).unwrap_or_else(|| addr.to_string());
    let (sink, stream) = ws.split();
    let (peer, peer_rx) = peer_channel();
    let kill = peer_rx.kill.clone();
    if events.send(HostTransportEvent::Connected { conn, remote, tx: peer.clone() }).is_err() {
        return;
    }
    tokio::spawn(writer(sink, peer_rx, shutdown.clone(), None));
    let reason = reader(stream, shutdown, kill, |b| match b[0] {
        K_DATA => events.send(HostTransportEvent::Data { conn, bytes: b.slice(1..) }).is_ok(),
        K_PING if b.len() >= 13 => {
            let mut f = Vec::with_capacity(17);
            f.push(K_PONG);
            f.extend_from_slice(&b[1..9]);
            f.extend_from_slice(&clock.tick_now().to_le_bytes());
            peer.send(Bytes::from(f));
            let rtt = f32::from_le_bytes(b[9..13].try_into().unwrap());
            // Client-reported: ignore NaN/inf/absurd values (they would poison `HostSession::rtt`).
            if rtt.is_finite() && rtt > 0.0 && rtt < 60.0 {
                let _ = events.send(HostTransportEvent::PeerRtt { conn, rtt });
            }
            true
        }
        _ => true,
    })
    .await;
    peer.close();
    let _ = events.send(HostTransportEvent::Disconnected { conn, reason });
}

// ---------------------------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------------------------

pub enum ClientTransportEvent {
    Connected,
    Data(Bytes),
    /// A ping round trip completed.
    Pong {
        rtt: f64,
        host_tick: f64,
        recv_at: f64,
    },
    Disconnected(String),
}

pub struct ClientTransport {
    pub events: Receiver<ClientTransportEvent>,
    pub tx: PeerTx,
    shutdown: watch::Sender<bool>,
}

impl ClientTransport {
    /// Start connecting to a `ws://` / `wss://` URL in the background. Retries (DNS for a fresh
    /// tunnel can take a few seconds to propagate) until `timeout` elapses.
    pub fn connect(url: String, timeout: Duration, ping_interval: Duration) -> Self {
        let (ev_tx, ev_rx) = unbounded();
        let (sd_tx, sd_rx) = watch::channel(false);
        let (tx, rx) = peer_channel();
        runtime().spawn(client_task(url, timeout, ping_interval, ev_tx, tx.clone(), rx, sd_rx));
        Self { events: ev_rx, tx, shutdown: sd_tx }
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }
}

impl Drop for ClientTransport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

type ClientWs = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// One connection attempt. Fresh quick-tunnel hostnames are resolved via public DNS directly
/// (see [`crate::dns`]) so a stale negative OS cache entry cannot block joining.
async fn connect_once(url: &str) -> Result<ClientWs, String> {
    let host = url.split("://").nth(1).unwrap_or("").split(['/', ':']).next().unwrap_or("").to_ascii_lowercase();
    let default_port = !url.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("").contains(':');
    if url.starts_with("wss://") && host.ends_with(".trycloudflare.com") && default_port {
        let answer = crate::dns::resolve_direct(&host).await;
        crate::debug_log(|| format!("direct DNS {host} -> {answer:?}"));
        match answer {
            crate::dns::DnsAnswer::Addrs(ips) => {
                for ip in ips {
                    let r = tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect((ip, 443))).await;
                    if let Ok(Ok(tcp)) = r {
                        let _ = tcp.set_nodelay(true);
                        return tokio_tungstenite::client_async_tls_with_config(url, tcp, Some(ws_config()), Some(tls_connector()))
                            .await
                            .map(|(ws, _)| ws)
                            .map_err(|e| e.to_string());
                    }
                }
            }
            // Not propagated yet: do NOT ask the OS resolver, it would cache the NXDOMAIN.
            crate::dns::DnsAnswer::NoSuchName => return Err(format!("lobby {host} not found (yet)")),
            crate::dns::DnsAnswer::NoResponse => {}
        }
    }
    tokio_tungstenite::connect_async_tls_with_config(url, Some(ws_config()), true, Some(tls_connector()))
        .await
        .map(|(ws, _)| ws)
        .map_err(|e| e.to_string())
}

async fn client_task(
    url: String,
    timeout: Duration,
    ping_interval: Duration,
    events: Sender<ClientTransportEvent>,
    self_tx: PeerTx,
    rx: PeerRx,
    mut shutdown: watch::Receiver<bool>,
) {
    let deadline = tokio::time::Instant::now() + timeout;
    let ws = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let attempt = tokio::time::timeout(remaining.max(Duration::from_millis(1)), connect_once(&url));
        let err = tokio::select! {
            _ = shutdown.changed() => return,
            r = attempt => match r {
                Ok(Ok(ws)) => break ws,
                Ok(Err(e)) => e,
                Err(_) => "timed out".to_string(),
            }
        };
        crate::debug_log(|| format!("connect {url} failed: {err}"));
        if tokio::time::Instant::now() + Duration::from_millis(500) >= deadline {
            let _ = events.send(ClientTransportEvent::Disconnected(format!("could not connect to {url}: {err}")));
            return;
        }
        tokio::select! {
            _ = shutdown.changed() => return,
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
    };
    if events.send(ClientTransportEvent::Connected).is_err() {
        return;
    }
    let (sink, stream) = ws.split();
    let last_rtt = Arc::new(AtomicU32::new(0f32.to_bits()));
    let kill = rx.kill.clone();
    tokio::spawn(writer(sink, rx, shutdown.clone(), Some(PingCfg { interval: ping_interval, last_rtt: last_rtt.clone() })));
    let reason = reader(stream, shutdown, kill, |b| match b[0] {
        K_DATA => events.send(ClientTransportEvent::Data(b.slice(1..))).is_ok(),
        K_PONG if b.len() >= 17 => {
            let now = now_secs();
            let sent = f64::from_le_bytes(b[1..9].try_into().unwrap());
            let host_tick = f64::from_le_bytes(b[9..17].try_into().unwrap());
            let rtt = (now - sent).max(0.0);
            last_rtt.store((rtt as f32).to_bits(), Ordering::Relaxed);
            events.send(ClientTransportEvent::Pong { rtt, host_tick, recv_at: now }).is_ok()
        }
        _ => true,
    })
    .await;
    self_tx.close();
    let _ = events.send(ClientTransportEvent::Disconnected(reason));
}
