/**
 * Online transport core (Node only — no Electron import, so tests run it directly).
 *
 *   NetHost    WebSocket server (listen server): handshake, peer ids, ping/RTT, limits, resume grace.
 *   NetClient  WebSocket client: handshake, ping/clock sync, automatic reconnect with resume.
 *   NetService one host-or-client session driven by PortRequest objects, emitting PortEvent objects
 *              (net-main.ts pipes these over a MessagePort to the renderer).
 *
 * Every inbound frame is size-capped (ws maxPayload), rate-limited (token buckets for messages and
 * bytes), decoded with the pluggable codec and validated (plain object, string `t`, bounded depth).
 * Nothing received from a peer is ever evaluated; messages are only parsed and relayed.
 */
import { WebSocketServer, WebSocket, type RawData } from 'ws';
import type { IncomingMessage } from 'node:http';
import type { Agent } from 'node:http';
import { networkInterfaces } from 'node:os';
import { randomBytes } from 'node:crypto';
import {
  CLOSE, DEFAULT_LIMITS, FATAL_CLOSE_CODES, HOST_PEER, JsonCodec, PROTOCOL_VERSION, RESERVED_TYPES, TokenBucket, decodeMessage, isPlainObject, parseJoinTarget,
  type Codec, type HostInfo, type HostOptions, type JoinInfo, type NetLimits, type NetMessage, type NetStats, type NetStatus, type PeerEvent, type PeerId,
  type PeerStats, type PortEvent, type PortRequest,
} from '../net/protocol.js';
import { ClockSync } from '../net/clock.js';
import type { QuickTunnel } from './tunnel.js';

/** Sub-millisecond wall-clock-ish time (monotonic within the process). */
const now = () => performance.timeOrigin + performance.now();

export function sanitizeName(v: unknown, fallback = 'Lantern'): string {
  const s = typeof v === 'string' ? v.replace(/[\u0000-\u001f\u007f-\u009f​-‏‪-‮]/g, '').trim().slice(0, 24).trim() : '';
  return s || fallback;
}

function rawToBytes(data: RawData): Uint8Array {
  if (Array.isArray(data)) return Buffer.concat(data);
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  return data;
}

/** Traffic + rate accounting for one direction pair. */
class Meter {
  inBytes = 0; outBytes = 0;
  inBps = 0; outBps = 0;
  private lastIn = 0; private lastOut = 0; private lastT = now();
  sample() {
    const t = now(), dt = Math.max(1, t - this.lastT);
    this.inBps = ((this.inBytes - this.lastIn) * 1000) / dt;
    this.outBps = ((this.outBytes - this.lastOut) * 1000) / dt;
    this.lastIn = this.inBytes; this.lastOut = this.outBytes; this.lastT = t;
  }
}

/** Shared per-connection inbound guard: size, rate, decode, validate, violation count. */
class InboundGuard {
  private msgBucket: TokenBucket;
  private byteBucket: TokenBucket;
  violations = 0;
  dropped = 0;
  constructor(private limits: NetLimits, private codec: Codec) {
    this.msgBucket = new TokenBucket(limits.msgPerSec, limits.msgBurst, now());
    this.byteBucket = new TokenBucket(limits.bytesPerSec, limits.bytesBurst, now());
  }
  /** Returns the message, or null (dropped). `abuse` becomes true when the peer should be kicked. */
  accept(data: RawData, isBinary: boolean): { msg: NetMessage | null; abuse: boolean; error?: string } {
    const bytes = rawToBytes(data);
    const t = now();
    let error: string | undefined;
    if (bytes.byteLength > this.limits.maxMessageBytes) error = 'too big';
    else if (!this.msgBucket.take(1, t) || !this.byteBucket.take(bytes.byteLength, t)) error = 'rate limit';
    let msg: NetMessage | null = null;
    if (!error) {
      const r = decodeMessage(this.codec, isBinary ? bytes : Buffer.from(bytes).toString('utf8'), this.limits);
      if (r.ok) msg = r.msg; else error = r.error;
    }
    if (error) { this.dropped++; this.violations++; }
    return { msg, abuse: this.violations > this.limits.maxViolations, error };
  }
}

function sendRaw(ws: WebSocket, codec: Codec, msg: NetMessage): number {
  if (ws.readyState !== WebSocket.OPEN) return 0;
  const data = codec.encode(msg);
  ws.send(data);
  return typeof data === 'string' ? Buffer.byteLength(data) : data.byteLength;
}

function lanAddresses(port: number): string[] {
  const out: string[] = [];
  for (const list of Object.values(networkInterfaces())) for (const a of list ?? []) if (a.family === 'IPv4' && !a.internal) out.push(`ws://${a.address}:${port}`);
  return out;
}

// ================================================================================================ host

export interface NetHostOptions {
  bind?: string;
  port?: number;
  maxPeers?: number;
  name?: string;
  limits?: Partial<NetLimits>;
  codec?: Codec;
  /** Keep a dropped peer's slot/id this long for a reconnect (0 = leave immediately). */
  resumeGraceMs?: number;
  /** Current host sim tick, reported in welcome. */
  getTick?: () => number;
}

interface HostPeer {
  id: PeerId;
  name: string;
  token: string;
  ws: WebSocket | null;
  state: 'active' | 'lost';
  lastSeen: number;
  clock: ClockSync;
  guard: InboundGuard;
  meter: Meter;
  lostTimer?: NodeJS.Timeout;
  pingId: number;
}

export class NetHost {
  readonly limits: NetLimits;
  readonly codec: Codec;
  readonly maxPeers: number;
  readonly name: string;
  private wss: WebSocketServer | null = null;
  private peers = new Map<PeerId, HostPeer>();
  private nextId = 1;
  private timer?: NodeJS.Timeout;
  private meter = new Meter();
  port = 0;
  onPeer: (e: PeerEvent) => void = () => {};
  onMessage: (from: PeerId, msg: NetMessage) => void = () => {};
  /** Diagnostics (rejections, drops). */
  onLog: (line: string) => void = () => {};

  constructor(private opts: NetHostOptions = {}) {
    this.limits = { ...DEFAULT_LIMITS, ...opts.limits };
    this.codec = opts.codec ?? JsonCodec;
    this.maxPeers = Math.max(1, Math.floor(opts.maxPeers ?? 1));
    this.name = sanitizeName(opts.name, 'Host');
  }

  start(): Promise<number> {
    return new Promise((resolve, reject) => {
      const wss = new WebSocketServer({
        host: this.opts.bind ?? '127.0.0.1',
        port: this.opts.port ?? 0,
        maxPayload: this.limits.maxMessageBytes,
        perMessageDeflate: false,
        clientTracking: true,
        // Browsers always send Origin; our clients (Node ws in the Electron main process) never do.
        // Refusing Origin-bearing upgrades stops any web page the host has open from connecting to localhost.
        verifyClient: (info: { origin?: string; req: IncomingMessage }) => !info.origin && !info.req.headers.origin,
      });
      this.wss = wss;
      wss.once('error', reject);
      wss.once('listening', () => {
        wss.off('error', reject);
        wss.on('error', (e) => this.onLog(`server error: ${e.message}`));
        const addr = wss.address();
        this.port = typeof addr === 'object' && addr ? addr.port : 0;
        this.timer = setInterval(() => this.tick(), this.limits.pingIntervalMs);
        resolve(this.port);
      });
      wss.on('connection', (ws) => this.accept(ws));
    });
  }

  get addresses(): string[] {
    const bind = this.opts.bind ?? '127.0.0.1';
    const local = `ws://127.0.0.1:${this.port}`;
    return bind === '0.0.0.0' || bind === '::' ? [local, ...lanAddresses(this.port)] : [local];
  }

  private accept(ws: WebSocket) {
    const guard = new InboundGuard(this.limits, this.codec);
    let peer: HostPeer | null = null;
    const hsTimer = setTimeout(() => { if (!peer) this.reject(ws, 'handshake', CLOSE.handshake, 'no hello received'); }, this.limits.handshakeTimeoutMs);
    ws.on('message', (data, isBinary) => {
      if (ws.readyState !== WebSocket.OPEN) return; // rejected / closing
      const g = peer ? peer.guard : guard;
      const r = g.accept(data, isBinary);
      if (peer) { peer.lastSeen = now(); peer.meter.inBytes += rawToBytes(data).byteLength; }
      this.meter.inBytes += rawToBytes(data).byteLength;
      if (r.abuse) { this.onLog(`kicking ${peer ? `peer ${peer.id}` : 'connection'}: too many bad messages (${r.error})`); this.reject(ws, 'rate', CLOSE.abuse, 'too many invalid or excessive messages'); return; }
      if (!r.msg) return;
      const msg = r.msg;
      if (!peer) {
        if (msg.t !== 'hello') { this.reject(ws, 'handshake', CLOSE.handshake, 'expected hello'); return; }
        clearTimeout(hsTimer);
        peer = this.handshake(ws, msg, guard);
        return;
      }
      this.handle(peer, msg);
    });
    // a socket error on the server side means a protocol violation (oversize frame, invalid frame/UTF-8): no resume
    let violation = '';
    ws.on('close', (code, reason) => {
      clearTimeout(hsTimer);
      if (peer && peer.ws === ws) this.dropped(peer, violation ? CLOSE.abuse : code, violation || reason.toString('utf8'));
    });
    ws.on('error', (e) => { violation = e.message.slice(0, 120); this.onLog(`socket error: ${e.message}`); });
  }

  private reject(ws: WebSocket, code: 'version' | 'full' | 'handshake' | 'rate' | 'kicked', closeCode: number, message: string) {
    try { sendRaw(ws, this.codec, { t: 'error', code, message }); } catch { /* ignore */ }
    try { ws.close(closeCode, message.slice(0, 120)); } catch { /* ignore */ }
    setTimeout(() => { try { ws.terminate(); } catch { /* ignore */ } }, 2000).unref?.();
  }

  private handshake(ws: WebSocket, msg: NetMessage, guard: InboundGuard): HostPeer | null {
    if (msg.v !== PROTOCOL_VERSION) {
      this.reject(ws, 'version', CLOSE.version, `Version mismatch: host runs protocol v${PROTOCOL_VERSION}, you run v${typeof msg.v === 'number' ? msg.v : '?'} — both players need the same game version.`);
      return null;
    }
    const name = sanitizeName(msg.name);
    // resume a dropped peer?
    if (typeof msg.resume === 'string') {
      for (const p of this.peers.values()) {
        if (p.token === msg.resume && p.state === 'lost') {
          clearTimeout(p.lostTimer);
          p.ws = ws; p.state = 'active'; p.lastSeen = now(); p.guard = guard;
          this.welcome(p);
          this.onPeer({ kind: 'resume', peerId: p.id, name: p.name });
          return p;
        }
      }
    }
    if (this.peers.size >= this.maxPeers) { this.reject(ws, 'full', CLOSE.full, 'The match is full.'); return null; }
    const p: HostPeer = { id: this.nextId++, name, token: randomBytes(12).toString('hex'), ws, state: 'active', lastSeen: now(), clock: new ClockSync(), guard, meter: new Meter(), pingId: 0 };
    this.peers.set(p.id, p);
    this.welcome(p);
    this.onPeer({ kind: 'join', peerId: p.id, name: p.name });
    this.ping(p);
    return p;
  }

  private welcome(p: HostPeer) {
    this.out(p, { t: 'welcome', v: PROTOCOL_VERSION, peerId: p.id, hostTick: this.opts.getTick?.() ?? 0, hostTime: now(), name: this.name, token: p.token });
  }

  private handle(p: HostPeer, msg: NetMessage) {
    switch (msg.t) {
      case 'ping':
        if (typeof msg.id === 'number' && typeof msg.ts === 'number') this.out(p, { t: 'pong', id: msg.id, ts: msg.ts, now: now() });
        return;
      case 'pong':
        if (typeof msg.ts === 'number' && typeof msg.now === 'number') p.clock.add(msg.ts, msg.now, now());
        return;
      case 'bye':
        this.leave(p, typeof msg.reason === 'string' ? msg.reason.slice(0, 80) : 'left');
        try { p.ws?.close(CLOSE.normal, 'bye'); } catch { /* ignore */ }
        return;
      default:
        if (RESERVED_TYPES.has(msg.t)) { p.guard.violations++; p.guard.dropped++; return; }
        this.onMessage(p.id, msg);
    }
  }

  private dropped(p: HostPeer, code: number, reason: string) {
    p.ws = null;
    if (!this.peers.has(p.id)) return; // already left (bye / kick)
    const grace = this.opts.resumeGraceMs ?? 10000;
    if (grace <= 0 || code === CLOSE.abuse || code === CLOSE.kicked || code === CLOSE.tooBig) { this.leave(p, reason || `closed (${code})`); return; }
    p.state = 'lost';
    this.onPeer({ kind: 'lost', peerId: p.id, name: p.name, reason: reason || `connection lost (${code})` });
    p.lostTimer = setTimeout(() => this.leave(p, 'did not reconnect'), grace);
  }

  private leave(p: HostPeer, reason: string) {
    if (!this.peers.delete(p.id)) return;
    clearTimeout(p.lostTimer);
    this.onPeer({ kind: 'leave', peerId: p.id, name: p.name, reason });
  }

  private out(p: HostPeer, msg: NetMessage): boolean {
    if (!p.ws) return false;
    const n = sendRaw(p.ws, this.codec, msg);
    p.meter.outBytes += n; this.meter.outBytes += n;
    return n > 0;
  }

  private ping(p: HostPeer) { this.out(p, { t: 'ping', id: ++p.pingId, ts: now() }); }

  private tick() {
    const t = now();
    for (const p of this.peers.values()) {
      if (p.state !== 'active' || !p.ws) continue;
      if (t - p.lastSeen > this.limits.idleTimeoutMs) { this.onLog(`peer ${p.id} timed out`); p.ws.terminate(); continue; }
      this.ping(p);
    }
  }

  /** Send a game message to one peer or all active peers. Returns false if it could not be sent. */
  send(to: PeerId | 'all', msg: NetMessage): boolean {
    if (!isPlainObject(msg) || typeof msg.t !== 'string' || RESERVED_TYPES.has(msg.t)) return false;
    const data = this.codec.encode(msg);
    const size = typeof data === 'string' ? Buffer.byteLength(data) : data.byteLength;
    if (size > this.limits.maxMessageBytes) return false;
    let ok = false;
    for (const p of this.peers.values()) {
      if ((to !== 'all' && p.id !== to) || p.state !== 'active' || !p.ws || p.ws.readyState !== WebSocket.OPEN) continue;
      p.ws.send(data);
      p.meter.outBytes += size; this.meter.outBytes += size;
      ok = true;
    }
    return ok;
  }

  kick(peerId: PeerId, reason = 'kicked by host') {
    const p = this.peers.get(peerId);
    if (!p) return;
    const ws = p.ws;
    this.leave(p, reason);
    if (ws) this.reject(ws, 'kicked', CLOSE.kicked, reason);
  }

  stats(): NetStats {
    this.meter.sample();
    const peers: PeerStats[] = [...this.peers.values()].map((p) => {
      p.meter.sample();
      return { peerId: p.id, name: p.name, rtt: p.clock.rtt, rttAvg: p.clock.rttAvg, clockOffset: p.clock.offset, inBps: p.meter.inBps, outBps: p.meter.outBps, connected: p.state === 'active', dropped: p.guard.dropped };
    });
    return { role: 'host', peers, inBps: this.meter.inBps, outBps: this.meter.outBps, clockOffset: 0, rtt: peers[0]?.rttAvg ?? 0 };
  }

  get peerCount() { return this.peers.size; }

  async stop(reason = 'host stopped') {
    clearInterval(this.timer);
    for (const p of this.peers.values()) {
      clearTimeout(p.lostTimer);
      if (p.ws) { try { sendRaw(p.ws, this.codec, { t: 'bye', reason }); p.ws.close(CLOSE.hostStopped, reason); } catch { /* ignore */ } }
    }
    this.peers.clear();
    const wss = this.wss;
    this.wss = null;
    if (!wss) return;
    for (const c of wss.clients) setTimeout(() => c.terminate(), 500).unref?.();
    await new Promise<void>((r) => wss.close(() => r()));
  }
}

// ================================================================================================ client

export interface NetClientOptions {
  name?: string;
  limits?: Partial<NetLimits>;
  codec?: Codec;
  /** Reconnect attempts after an unexpected drop (0 = none). */
  maxReconnects?: number;
  /** HTTP(S) agent (e.g. a proxy agent) for the WebSocket upgrade. */
  agent?: Agent;
  connectTimeoutMs?: number;
}

export class NetClient {
  readonly limits: NetLimits;
  readonly codec: Codec;
  readonly clock = new ClockSync();
  private ws: WebSocket | null = null;
  private url = '';
  private name: string;
  private token: string | null = null;
  private timer?: NodeJS.Timeout;
  private reconnectTimer?: NodeJS.Timeout;
  private attempts = 0;
  private stopped = false;
  private lastSeen = 0;
  private pingId = 0;
  private meter = new Meter();
  private guard: InboundGuard;
  private hostName = 'Host';
  private lastError = '';
  peerId: PeerId = -1;
  state: 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'closed' | 'failed' = 'idle';
  onStatus: (s: NetStatus) => void = () => {};
  onPeer: (e: PeerEvent) => void = () => {};
  onMessage: (from: PeerId, msg: NetMessage) => void = () => {};

  constructor(private opts: NetClientOptions = {}) {
    this.limits = { ...DEFAULT_LIMITS, ...opts.limits };
    this.codec = opts.codec ?? JsonCodec;
    this.name = sanitizeName(opts.name, 'Challenger');
    this.guard = new InboundGuard(this.limits, this.codec);
  }

  /** Connect to a room code / URL; resolves on welcome, rejects with a player-readable reason. */
  connect(target: string): Promise<JoinInfo> {
    const url = parseJoinTarget(target);
    if (!url) return Promise.reject(new Error('Not a room code or address. Paste the code the host sees (e.g. apple-banana-cherry-delta) or ws://ip:port.'));
    this.url = url;
    this.stopped = false;
    this.setState('connecting', `connecting to ${url}`);
    return new Promise((resolve, reject) => this.open(resolve, reject));
  }

  private open(resolve?: (i: JoinInfo) => void, reject?: (e: Error) => void) {
    let welcomed = false;
    const ws = new WebSocket(this.url, { maxPayload: this.limits.maxMessageBytes, perMessageDeflate: false, handshakeTimeout: this.opts.connectTimeoutMs ?? 15000, agent: this.opts.agent });
    this.ws = ws;
    this.guard = new InboundGuard(this.limits, this.codec);
    ws.on('open', () => {
      this.lastSeen = now();
      this.out({ t: 'hello', v: PROTOCOL_VERSION, name: this.name, ...(this.token ? { resume: this.token } : {}) });
    });
    ws.on('message', (data, isBinary) => {
      if (ws !== this.ws) return;
      this.lastSeen = now();
      this.meter.inBytes += rawToBytes(data).byteLength;
      const r = this.guard.accept(data, isBinary);
      if (r.abuse) { ws.close(CLOSE.abuse, 'host sent too many invalid messages'); return; }
      const msg = r.msg;
      if (!msg) return;
      if (!welcomed) {
        if (msg.t === 'welcome' && typeof msg.peerId === 'number') {
          welcomed = true;
          const resumed = this.token !== null && msg.token === this.token;
          this.peerId = msg.peerId;
          this.token = typeof msg.token === 'string' ? msg.token : null;
          this.hostName = sanitizeName(msg.name, 'Host');
          this.attempts = 0;
          this.setState('connected', `connected to ${this.hostName}`);
          this.onPeer({ kind: resumed ? 'resume' : 'join', peerId: HOST_PEER, name: this.hostName });
          clearInterval(this.timer);
          this.timer = setInterval(() => this.tick(), this.limits.pingIntervalMs);
          this.ping();
          resolve?.({ peerId: this.peerId, hostTick: typeof msg.hostTick === 'number' ? msg.hostTick : 0, hostName: this.hostName, url: this.url });
        } else if (msg.t === 'error') {
          this.lastError = typeof msg.message === 'string' ? msg.message.slice(0, 300) : 'rejected by host';
        }
        return;
      }
      switch (msg.t) {
        case 'ping': if (typeof msg.id === 'number' && typeof msg.ts === 'number') this.out({ t: 'pong', id: msg.id, ts: msg.ts, now: now() }); return;
        case 'pong': if (typeof msg.ts === 'number' && typeof msg.now === 'number') this.clock.add(msg.ts, msg.now, now()); return;
        case 'error': this.lastError = typeof msg.message === 'string' ? msg.message.slice(0, 300) : 'error from host'; return;
        case 'bye': this.lastError = `host closed the match${typeof msg.reason === 'string' ? ` (${msg.reason.slice(0, 80)})` : ''}`; return;
        default:
          if (RESERVED_TYPES.has(msg.t)) return;
          this.onMessage(HOST_PEER, msg);
      }
    });
    ws.on('error', (e) => { this.lastError ||= e.message; });
    ws.on('close', (code, reasonBuf) => {
      if (ws !== this.ws) return;
      clearInterval(this.timer);
      this.ws = null;
      const reason = this.lastError || reasonBuf.toString('utf8') || (code === CLOSE.tooBig ? 'message too big' : `connection closed (${code})`);
      this.lastError = '';
      if (!welcomed && resolve) {
        // initial connect failed
        this.setState('failed', reason);
        reject?.(new Error(reason));
        return;
      }
      const wasConnected = welcomed;
      if (this.stopped) { this.setState('closed', 'left the match'); return; }
      const maxRe = this.opts.maxReconnects ?? 6;
      if (FATAL_CLOSE_CODES.has(code) || this.attempts >= maxRe) {
        this.setState(FATAL_CLOSE_CODES.has(code) ? 'closed' : 'failed', reason);
        this.onPeer({ kind: 'leave', peerId: HOST_PEER, name: this.hostName, reason });
        return;
      }
      if (wasConnected) this.onPeer({ kind: 'lost', peerId: HOST_PEER, name: this.hostName, reason });
      const delay = Math.min(4000, 500 * 2 ** this.attempts);
      this.attempts++;
      this.setState('reconnecting', `${reason} — reconnecting (${this.attempts}/${maxRe})…`);
      this.reconnectTimer = setTimeout(() => this.open(), delay);
    });
  }

  private setState(state: NetClient['state'], detail: string) {
    this.state = state;
    this.onStatus({ role: 'client', state, detail });
  }

  private out(msg: NetMessage): number {
    if (!this.ws) return 0;
    const n = sendRaw(this.ws, this.codec, msg);
    this.meter.outBytes += n;
    return n;
  }

  private ping() { this.out({ t: 'ping', id: ++this.pingId, ts: now() }); }

  private tick() {
    if (!this.ws || this.state !== 'connected') return;
    if (now() - this.lastSeen > this.limits.idleTimeoutMs) { this.lastError = 'host stopped responding'; this.ws.terminate(); return; }
    this.ping();
  }

  send(msg: NetMessage): boolean {
    if (!this.ws || this.state !== 'connected' || !isPlainObject(msg) || typeof msg.t !== 'string' || RESERVED_TYPES.has(msg.t)) return false;
    const data = this.codec.encode(msg);
    const size = typeof data === 'string' ? Buffer.byteLength(data) : data.byteLength;
    if (size > this.limits.maxMessageBytes) return false;
    this.ws.send(data);
    this.meter.outBytes += size;
    return true;
  }

  /** Test hook: send raw bytes/text (bypasses validation). */
  sendRawForTest(data: string | Buffer) { this.ws?.send(data); }

  stats(): NetStats {
    this.meter.sample();
    const peer: PeerStats = { peerId: HOST_PEER, name: this.hostName, rtt: this.clock.rtt, rttAvg: this.clock.rttAvg, clockOffset: this.clock.offset, inBps: this.meter.inBps, outBps: this.meter.outBps, connected: this.state === 'connected', dropped: this.guard.dropped };
    return { role: 'client', peers: this.state === 'idle' ? [] : [peer], inBps: this.meter.inBps, outBps: this.meter.outBps, clockOffset: this.clock.offset, rtt: this.clock.rttAvg };
  }

  async stop(reason = 'left') {
    this.stopped = true;
    clearInterval(this.timer);
    clearTimeout(this.reconnectTimer);
    const ws = this.ws;
    if (!ws) { this.setState('closed', 'left the match'); return; }
    await new Promise<void>((r) => {
      const t = setTimeout(() => { try { ws.terminate(); } catch { /* ignore */ } r(); }, 1000);
      ws.once('close', () => { clearTimeout(t); r(); });
      try { this.out({ t: 'bye', reason }); ws.close(CLOSE.normal, reason); } catch { clearTimeout(t); r(); }
    });
  }
}

// ================================================================================================ service

export interface NetServiceOptions {
  /** Starts a quick tunnel to 127.0.0.1:port, or throws/returns null when cloudflared is unavailable. */
  startTunnel?: (port: number) => QuickTunnel;
  statsIntervalMs?: number;
  limits?: Partial<NetLimits>;
  clientAgent?: Agent;
}

/** One session (host or client) driven by PortRequests; emits PortEvents. */
export class NetService {
  private host: NetHost | null = null;
  private client: NetClient | null = null;
  private tunnel: QuickTunnel | null = null;
  private statsTimer?: NodeJS.Timeout;
  private tick = 0;
  private status: NetStatus = { role: 'idle', state: 'idle' };

  constructor(private emit: (e: PortEvent) => void, private opts: NetServiceOptions = {}) {}

  private setStatus(s: NetStatus) { this.status = s; this.emit({ ev: 'status', status: s }); }

  /** Handle one request from the renderer (shape is validated here: the port is a trust boundary too). */
  async handle(raw: unknown): Promise<void> {
    if (!isPlainObject(raw) || typeof raw.op !== 'string') return;
    const r = raw as PortRequest;
    const reply = (req: unknown, ok: boolean, data: unknown) => {
      if (typeof req !== 'number') return;
      this.emit(ok ? { ev: 'reply', req, ok: true, data } : { ev: 'reply', req, ok: false, error: String(data) });
    };
    switch (r.op) {
      case 'send': {
        if (!isPlainObject(r.msg)) return;
        const to = r.to === 'all' ? 'all' : typeof r.to === 'number' ? r.to : null;
        if (to === null) return;
        if (this.host) this.host.send(to, r.msg);
        else if (this.client) this.client.send(r.msg);
        return;
      }
      case 'tick': if (typeof r.tick === 'number' && Number.isFinite(r.tick)) this.tick = r.tick; return;
      case 'stop': await this.stop(); reply(r.req, true, null); return;
      case 'host':
        try { reply(r.req, true, await this.startHost(isPlainObject(r.opts) ? r.opts : { tunnel: false })); }
        catch (e) { reply(r.req, false, (e as Error).message); }
        return;
      case 'join':
        try { reply(r.req, true, await this.join(String(r.target ?? ''), sanitizeName(r.name, 'Challenger'))); }
        catch (e) { reply(r.req, false, (e as Error).message); }
        return;
    }
  }

  async startHost(o: HostOptions): Promise<HostInfo> {
    await this.stop();
    const tunnel = o.tunnel === true;
    this.setStatus({ role: 'host', state: 'starting', detail: 'opening server…' });
    const host = new NetHost({
      bind: !tunnel && o.lan ? '0.0.0.0' : '127.0.0.1',
      port: typeof o.port === 'number' ? o.port : 0,
      maxPeers: typeof o.maxPeers === 'number' ? Math.min(16, Math.max(1, o.maxPeers)) : 1,
      name: sanitizeName(o.name, 'Host'),
      limits: this.opts.limits,
      getTick: () => this.tick,
    });
    host.onPeer = (peer) => this.emit({ ev: 'peer', peer });
    host.onMessage = (from, msg) => this.emit({ ev: 'msg', from, msg });
    host.onLog = (l) => console.log('[net]', l);
    this.host = host;
    let port: number;
    try { port = await host.start(); } catch (e) {
      this.host = null;
      const reason = `could not open a server: ${(e as Error).message}`;
      this.setStatus({ role: 'host', state: 'failed', detail: reason });
      throw new Error(reason);
    }
    this.startStats();
    const info: HostInfo = { port, code: null, url: null, addresses: host.addresses };
    if (!tunnel) {
      this.setStatus({ role: 'host', state: 'ready', detail: `waiting for opponent on ${info.addresses.join(', ')}` });
      return info;
    }
    this.setStatus({ role: 'host', state: 'tunnel-starting', detail: 'starting tunnel…' });
    try {
      if (!this.opts.startTunnel) throw new Error('tunnel support is not available in this build');
      const t = this.opts.startTunnel(port);
      this.tunnel = t;
      const ti = await t.ready;
      if (this.host !== host) throw new Error('stopped');
      t.onExit((reason) => { if (this.tunnel === t) this.setStatus({ role: 'host', state: 'failed', detail: `tunnel closed: ${reason}`, code: ti.code, url: ti.url }); });
      info.code = ti.code; info.url = ti.url;
      this.setStatus({ role: 'host', state: 'ready', detail: 'waiting for opponent', code: ti.code, url: ti.url });
      return info;
    } catch (e) {
      const reason = (e as Error).message;
      if (this.host === host) { await this.stop(); this.setStatus({ role: 'host', state: 'failed', detail: `tunnel failed: ${reason}` }); }
      throw new Error(`tunnel failed: ${reason}`);
    }
  }

  async join(target: string, name: string): Promise<JoinInfo> {
    await this.stop();
    const c = new NetClient({ name, limits: this.opts.limits, agent: this.opts.clientAgent });
    c.onStatus = (s) => this.setStatus(s);
    c.onPeer = (peer) => this.emit({ ev: 'peer', peer });
    c.onMessage = (from, msg) => this.emit({ ev: 'msg', from, msg });
    this.client = c;
    this.startStats();
    try { return await c.connect(target); } catch (e) {
      if (this.client === c) { this.client = null; this.stopStats(); }
      throw e;
    }
  }

  private startStats() {
    this.stopStats();
    this.statsTimer = setInterval(() => this.emit({ ev: 'stats', stats: this.stats() }), this.opts.statsIntervalMs ?? 500);
  }
  private stopStats() { clearInterval(this.statsTimer); this.statsTimer = undefined; }

  stats(): NetStats {
    return this.host?.stats() ?? this.client?.stats() ?? { role: 'idle', peers: [], inBps: 0, outBps: 0, clockOffset: 0, rtt: 0 };
  }

  get currentStatus() { return this.status; }

  /** Synchronous last-resort cleanup (process exit): kill cloudflared. */
  killTunnel() { try { this.tunnel?.stop(); } catch { /* ignore */ } }

  async stop() {
    this.stopStats();
    const { host, client, tunnel } = this;
    this.host = null; this.client = null; this.tunnel = null;
    tunnel?.stop();
    if (host) await host.stop();
    if (client) await client.stop();
    if (host || client) this.setStatus({ role: 'idle', state: 'idle', detail: 'stopped' });
  }
}
