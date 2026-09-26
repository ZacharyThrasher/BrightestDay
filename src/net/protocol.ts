/**
 * Transport-level protocol shared by the Electron main process (sockets) and the renderer (game).
 * Pure module: no Node, no DOM.
 *
 * Wire messages are plain JSON objects with a string `t` (type) field. The transport itself owns a few
 * reserved types (hello / welcome / ping / pong / bye / error); everything else is game traffic and is
 * relayed untouched (after validation) to the renderer.
 *
 * Handshake:  client → host  {t:'hello', v, name, resume?}
 *             host → client  {t:'welcome', v, peerId, hostTick, hostTime, name, token}
 *             version mismatch → host sends {t:'error', code:'version', message} and closes (4001).
 * Peer id 0 is always the host.
 */

/** Bump when the wire format changes incompatibly (host and client must match exactly). */
export const PROTOCOL_VERSION = 1;

export type PeerId = number;
export const HOST_PEER: PeerId = 0;

/** Any wire message: a plain object with a short string type tag. */
export interface NetMessage { t: string; [k: string]: unknown }

export interface HelloMsg extends NetMessage { t: 'hello'; v: number; name: string; resume?: string }
export interface WelcomeMsg extends NetMessage { t: 'welcome'; v: number; peerId: PeerId; hostTick: number; hostTime: number; name: string; token: string }
export interface PingMsg extends NetMessage { t: 'ping'; id: number; ts: number }
export interface PongMsg extends NetMessage { t: 'pong'; id: number; ts: number; now: number }
export interface ByeMsg extends NetMessage { t: 'bye'; reason?: string }
export interface ErrorMsg extends NetMessage { t: 'error'; code: NetErrorCode; message: string }

export type NetErrorCode = 'version' | 'full' | 'handshake' | 'rate' | 'bad-message' | 'kicked' | 'closed';

/** Types the transport handles itself; the game must not use them. */
export const RESERVED_TYPES: ReadonlySet<string> = new Set(['hello', 'welcome', 'ping', 'pong', 'bye', 'error']);

/** WebSocket close codes (4000–4999 are application-defined). */
export const CLOSE = {
  normal: 1000,
  tooBig: 1009,
  version: 4001,
  full: 4002,
  handshake: 4003,
  abuse: 4004,
  kicked: 4005,
  hostStopped: 4006,
} as const;
/** Close codes after which a client must not try to reconnect. */
export const FATAL_CLOSE_CODES: ReadonlySet<number> = new Set([CLOSE.version, CLOSE.full, CLOSE.abuse, CLOSE.kicked, CLOSE.hostStopped, CLOSE.normal]);

export interface NetLimits {
  /** Max encoded size of one message (bytes). Larger frames close the connection (1009). */
  maxMessageBytes: number;
  /** Token bucket: sustained messages/s and burst per peer. */
  msgPerSec: number;
  msgBurst: number;
  /** Token bucket: sustained bytes/s and burst per peer. */
  bytesPerSec: number;
  bytesBurst: number;
  /** Max object nesting depth of a message. */
  maxDepth: number;
  /** Max length of the `t` field. */
  maxTypeLength: number;
  /** Dropped (invalid / over-rate) messages tolerated before the peer is kicked. */
  maxViolations: number;
  /** A client must say hello within this many ms. */
  handshakeTimeoutMs: number;
  /** No traffic at all for this long → connection considered dead. */
  idleTimeoutMs: number;
  /** Ping interval (ms). */
  pingIntervalMs: number;
}

export const DEFAULT_LIMITS: NetLimits = {
  maxMessageBytes: 64 * 1024,
  msgPerSec: 240,
  msgBurst: 480,
  bytesPerSec: 512 * 1024,
  bytesBurst: 1024 * 1024,
  maxDepth: 24,
  maxTypeLength: 32,
  maxViolations: 50,
  handshakeTimeoutMs: 5000,
  idleTimeoutMs: 10000,
  pingIntervalMs: 1000,
};

/** Pluggable wire encoding (JSON now; a binary codec can replace it later without touching callers). */
export interface Codec {
  readonly name: string;
  encode(msg: NetMessage): string | Uint8Array;
  /** Returns the decoded value or throws; validation happens separately (see validateMessage). */
  decode(data: string | Uint8Array): unknown;
}

export const JsonCodec: Codec = {
  name: 'json',
  encode: (msg) => JSON.stringify(msg),
  decode: (data) => JSON.parse(typeof data === 'string' ? data : new TextDecoder('utf-8', { fatal: true }).decode(data)),
};

/** True for `{...}` created by JSON.parse / object literals (not arrays, null, class instances). */
export function isPlainObject(v: unknown): v is Record<string, unknown> {
  if (v === null || typeof v !== 'object' || Array.isArray(v)) return false;
  const p = Object.getPrototypeOf(v);
  return p === Object.prototype || p === null;
}

/** Iterative depth check (no recursion, so hostile nesting can't blow the stack). */
export function depthWithin(v: unknown, maxDepth: number): boolean {
  const stack: [unknown, number][] = [[v, 1]];
  while (stack.length) {
    const [x, d] = stack.pop()!;
    if (x === null || typeof x !== 'object') continue;
    if (d > maxDepth) return false;
    for (const k in x as object) {
      if (!Object.prototype.hasOwnProperty.call(x, k)) continue;
      const c = (x as any)[k];
      if (c !== null && typeof c === 'object') stack.push([c, d + 1]);
    }
  }
  return true;
}

/** Validate a decoded inbound message. Returns an error string or null when valid. */
export function validateMessage(v: unknown, limits: Pick<NetLimits, 'maxDepth' | 'maxTypeLength'> = DEFAULT_LIMITS): string | null {
  if (!isPlainObject(v)) return 'not a plain object';
  const t = v.t;
  if (typeof t !== 'string' || !t.length) return 'missing string field t';
  if (t.length > limits.maxTypeLength) return 'type too long';
  if (!depthWithin(v, limits.maxDepth)) return 'nested too deeply';
  return null;
}

/** Decode + validate in one step. */
export function decodeMessage(codec: Codec, data: string | Uint8Array, limits: NetLimits = DEFAULT_LIMITS): { ok: true; msg: NetMessage } | { ok: false; error: string } {
  let v: unknown;
  try { v = codec.decode(data); } catch { return { ok: false, error: 'undecodable' }; }
  const err = validateMessage(v, limits);
  return err ? { ok: false, error: err } : { ok: true, msg: v as NetMessage };
}

/** Simple token bucket (per peer: messages and bytes). */
export class TokenBucket {
  private tokens: number;
  private last: number;
  constructor(readonly rate: number, readonly burst: number, now: number) { this.tokens = burst; this.last = now; }
  take(n: number, now: number): boolean {
    this.tokens = Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate);
    this.last = now;
    if (this.tokens < n) return false;
    this.tokens -= n;
    return true;
  }
}

// ------------------------------------------------------------------------------------ room codes

const QUICK_TUNNEL_RE = /https:\/\/([a-z0-9]+(?:-[a-z0-9]+)+)\.trycloudflare\.com/i;
const CODE_RE = /^[a-z0-9]+(?:-[a-z0-9]+)+$/;

/** Extract the `https://<words>.trycloudflare.com` URL from a line of cloudflared output. */
export function findQuickTunnelUrl(text: string): string | null {
  const m = QUICK_TUNNEL_RE.exec(text);
  return m && m[1].toLowerCase() !== 'api' ? `https://${m[1].toLowerCase()}.trycloudflare.com` : null;
}

/** "apple-banana-cherry-delta" from "https://apple-banana-cherry-delta.trycloudflare.com". */
export function roomCodeFromUrl(url: string): string | null {
  const m = QUICK_TUNNEL_RE.exec(url);
  return m ? m[1].toLowerCase() : null;
}

/**
 * Resolve what the player typed into a WebSocket URL:
 *   room code "apple-banana-cherry-delta"     → wss://apple-banana-cherry-delta.trycloudflare.com
 *   https://<words>.trycloudflare.com[/...]   → wss://<words>.trycloudflare.com
 *   <words>.trycloudflare.com                 → wss://…
 *   ws://ip:port / wss://host                  → as is
 *   http(s)://host[:port]                      → ws(s)://host[:port]
 *   ip:port / localhost:port                   → ws://ip:port
 * Returns null for anything else.
 */
export function parseJoinTarget(input: string): string | null {
  let s = (input ?? '').trim();
  if (!s || s.length > 300) return null;
  const lower = s.toLowerCase();
  if (CODE_RE.test(lower) && lower.length <= 63 && !/^\d+(-\d+)*$/.test(lower)) return `wss://${lower}.trycloudflare.com`;
  if (/^[a-z0-9-]+\.trycloudflare\.com\/?$/i.test(s)) s = 'https://' + s;
  if (!/^[a-z]+:\/\//i.test(s)) {
    if (/^[a-z0-9.-]+:\d{1,5}$/i.test(s) || /^\[[0-9a-f:]+\]:\d{1,5}$/i.test(s)) s = 'ws://' + s;
    else return null;
  }
  let u: URL;
  try { u = new URL(s); } catch { return null; }
  const proto = u.protocol.toLowerCase();
  if (proto === 'http:') u.protocol = 'ws:';
  else if (proto === 'https:') u.protocol = 'wss:';
  else if (proto !== 'ws:' && proto !== 'wss:') return null;
  if (!u.hostname || u.username || u.password) return null;
  if (u.port && (Number(u.port) < 1 || Number(u.port) > 65535)) return null;
  const path = u.hostname.endsWith('.trycloudflare.com') ? '' : u.pathname === '/' ? '' : u.pathname;
  return `${u.protocol}//${u.host}${path}`;
}

// ------------------------------------------------------------------------------------ renderer <-> main port protocol

/** Options for hosting a match. */
export interface HostOptions {
  /** Expose the server through a Cloudflare quick tunnel (internet play). */
  tunnel: boolean;
  /** Bind on all interfaces (LAN play) instead of 127.0.0.1. Ignored (forced off) when tunnel is true. */
  lan?: boolean;
  /** Fixed port (default: random free port). */
  port?: number;
  /** Max simultaneous peers (default 1 — duels). */
  maxPeers?: number;
  /** Display name sent in welcome. */
  name?: string;
}

export interface HostInfo {
  port: number;
  /** Room code (tunnel words) — null in LAN mode. */
  code: string | null;
  /** Public https URL (tunnel) — null in LAN mode. */
  url: string | null;
  /** Direct addresses (ws://ip:port) usable on this machine / LAN. */
  addresses: string[];
}

export interface JoinInfo { peerId: PeerId; hostTick: number; hostName: string; url: string }

export type NetState =
  | 'idle'
  | 'starting'         // host: opening the server
  | 'tunnel-starting'  // host: waiting for cloudflared to register
  | 'ready'            // host: accepting peers
  | 'connecting'       // client
  | 'connected'        // client: welcomed
  | 'reconnecting'     // client: lost connection, retrying
  | 'closed'
  | 'failed';

export interface NetStatus {
  role: 'idle' | 'host' | 'client';
  state: NetState;
  /** Human-readable reason / progress detail. */
  detail?: string;
  code?: string | null;
  url?: string | null;
}

export type PeerEventKind = 'join' | 'leave' | 'lost' | 'resume';
/** join: welcomed. lost: connection dropped, may resume within the grace period. resume: back. leave: gone. */
export interface PeerEvent { kind: PeerEventKind; peerId: PeerId; name: string; reason?: string }

export interface PeerStats { peerId: PeerId; name: string; rtt: number; rttAvg: number; clockOffset: number; inBps: number; outBps: number; connected: boolean; dropped: number }
export interface NetStats { role: 'idle' | 'host' | 'client'; peers: PeerStats[]; inBps: number; outBps: number; /** Client only: estimated host clock offset (host Date.now() ≈ Date.now() + clockOffset). */ clockOffset: number; rtt: number }

export const EMPTY_STATS: NetStats = { role: 'idle', peers: [], inBps: 0, outBps: 0, clockOffset: 0, rtt: 0 };

/** Renderer → main. */
export type PortRequest =
  | { op: 'host'; req: number; opts: HostOptions }
  | { op: 'join'; req: number; target: string; name: string }
  | { op: 'stop'; req: number }
  | { op: 'send'; to: PeerId | 'all'; msg: NetMessage }
  | { op: 'tick'; tick: number };

/** Main → renderer. */
export type PortEvent =
  | { ev: 'reply'; req: number; ok: true; data: unknown }
  | { ev: 'reply'; req: number; ok: false; error: string }
  | { ev: 'status'; status: NetStatus }
  | { ev: 'peer'; peer: PeerEvent }
  | { ev: 'msg'; from: PeerId; msg: NetMessage }
  | { ev: 'stats'; stats: NetStats };
