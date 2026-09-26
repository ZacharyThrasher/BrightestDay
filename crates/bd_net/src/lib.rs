//! bd_net — networking for Brightest Day (see docs/DESIGN.md, "Netcode").
//!
//! Layers, bottom to top:
//! - [`transport`]: WebSocket (binary frames) on a background tokio runtime. The game thread only ever
//!   touches non-blocking channels. TCP_NODELAY everywhere. Ping/pong is answered on the network
//!   thread so RTT samples are not quantised by the game's frame rate.
//! - [`tunnel`]: launches `cloudflared tunnel --url http://127.0.0.1:PORT` and parses the
//!   `https://<words>.trycloudflare.com` address (the `<words>` part is the lobby code).
//! - [`session`]: [`HostSession`] / [`ClientSession`], generic over the game's serde message types:
//!   handshake, player ids, join/leave, kick, reliable lane + latest-only lane, clock sync.
//! - Pure netcode helpers (no I/O, unit-tested): [`ClockSync`], [`TickPacer`], [`InputBuffer`],
//!   [`InputQueue`], [`Reconciler`], [`SnapshotBuffer`], [`LagCompHistory`], [`LatestLane`].
//! - [`proxy::LagProxy`]: in-process TCP proxy that adds latency/jitter, for tests and a
//!   "simulate lag" dev option.
//!
//! All time values handed to the pure helpers are `f64` seconds from any monotonic clock; use
//! [`now_secs`] for the process-wide one the sessions use.

pub mod address;
pub mod clock;
pub mod codec;
pub mod dns;
pub mod input;
pub mod interp;
pub mod lagcomp;
pub mod latest;
pub mod proxy;
pub mod reconcile;
pub mod session;
pub mod transport;
pub mod tunnel;

pub use address::{lobby_code_from_url, resolve_address};
pub use clock::{ClockSync, PacerStep, RttStats, TickPacer};
pub use input::{InputBuffer, InputPacket, InputQueue, InputQueueStats, QueuedInput, Sequenced};
pub use interp::{Interp, InterpConfig, SnapshotBuffer};
pub use lagcomp::{LagCompHistory, Lerp, Rewind, clamp_view_tick};
pub use latest::LatestLane;
pub use proxy::{LagProxy, LagSettings};
pub use reconcile::{Reconcilable, ReconcileOutcome, Reconciler};
pub use session::{
    ClientConfig, ClientEvent, ClientSession, ClientState, HostConfig, HostEvent, HostSession, LeaveReason, LocalPlayer, PlayerInfo,
};
pub use tunnel::{Tunnel, TunnelStatus};

use std::sync::OnceLock;
use std::time::Instant;

/// Default host port (the classic Quake 3 port). The host falls back to a free port if taken.
pub const DEFAULT_PORT: u16 = 27960;

/// Wire protocol version. Bump whenever the session envelope or any game message type changes
/// incompatibly; mismatching clients are rejected during the handshake with a clear message.
pub const PROTOCOL_VERSION: u32 = 15; // v15: NetMapKind::Valley (Valley of the End) + ArenaInfo water sheets / water-walk (map-generated, same on host and clients); v14: NetMapKind::Casino + GameEvent::Casino + CombatTuning.casino (High Roller slot machines / roulette house spin); v13: NetMapKind::Mirrorfold (bd_game, Mirrorfold); v12: CombatTuning.juggle (juggle protection; WorldInit.cfg / S2C::Tuning) + guard bit 7 in PlayerSnap.status; v11: NetMapKind::Ember (bd_game, Ember); v10: NetMapKind::Sakura (bd_game, Sakura Keep); v9: NetCmd::PowerMove carries the crosshair aim (3P convergence, host-validated); v8: NetMapKind::Backrooms; v7: S2C::Tuning (host combat.ron hot reload -> clients; join uses WorldInit.cfg.tuning); v6: MatchRules.allow_third_person + ADS/DODGE/CHARGE bits; v2: C2S::Shout / S2C::Shout (bd_game shout callouts); v3: oath sections (NetCmd::OathBegin { section }, OathTable); v4: NetCmd::Signature + CombatTuning.signature; v5: OathResult.voice + tuning.willpower.oath_voice_slack

/// Magic number at the start of the handshake so random HTTP clients are rejected fast.
pub const PROTOCOL_MAGIC: u32 = 0x4244_4E54; // "BDNT"

/// Seconds since the first call in this process (monotonic). Used for all network timing.
pub fn now_secs() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// Diagnostic logging to stderr, enabled with the  environment variable.
pub(crate) fn debug_log(msg: impl FnOnce() -> String) {
    static ON: OnceLock<bool> = OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("BD_NET_LOG").is_some()) {
        eprintln!("[bd_net {:.3}] {}", now_secs(), msg());
    }
}

/// Errors surfaced to the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The address / lobby code could not be understood.
    BadAddress(String),
    /// Could not bind the listen socket.
    Bind(String),
    /// Something else went wrong at the I/O level.
    Io(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::BadAddress(s) => write!(f, "bad address: {s}"),
            NetError::Bind(s) => write!(f, "could not listen: {s}"),
            NetError::Io(s) => write!(f, "network error: {s}"),
        }
    }
}

impl std::error::Error for NetError {}
