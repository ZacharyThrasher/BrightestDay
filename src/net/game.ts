/**
 * Online duel sync (docs/PVP_DESIGN.md "Netcode"): a host-authoritative listen server where each client
 * simulates its OWN lantern's movement. Pure game logic over an abstract link (no DOM, no sockets), so the
 * same code runs in the app (over `net`, src/net/client.ts) and in Node tests (over NetHost / NetClient).
 *
 *   HostSession   runs the real World. Every tick it applies the client's reported pose to that player's proxy
 *                 (world.externallyDriven), runs the client's typed commands through that player's own
 *                 Director, sends velocity corrections for knockback / launches that hit the proxy, and at
 *                 30 Hz broadcasts snapshots: players, construct poses (+ joint poses), entities, projectiles,
 *                 the match, world events and a hash of the discrete state (world.stateHash('sync')).
 *   ClientSession keeps a mirror World (same scene; world.mirror = true: it never decides damage or deaths).
 *                 Its own lantern moves locally (Player.update) whenever the host doesn't own it (dead, held,
 *                 riding); everything else is posed from snapshots rendered DELAY_MS in the past
 *                 (SnapshotBuffer), constructs are built from their validated spec as kinematic puppets, and
 *                 host events are replayed at render time for fx / juice / HUD.
 *
 * Wire messages (all under the transport's 64 KB cap; the host sends ~30 snapshots/s, the client ~60 states/s):
 *   host → client  start {scene, you, names}            the match begins (client loads the scene, answers ready)
 *                  cadd  {ht, id, o, v, spec, …, ps}     construct created / modified (validated spec + poses)
 *                  snap  {ht, k, tm, pl, c, cr, e, er, en, pr, m, ev, h, act, full}
 *                  corr  {dv, lg, fb, fu, teth}          knockback / launch / tether on the client's own lantern
 *                  cres  {id, r}                         the result of the client's cast
 *                  pause {on, why}
 *   client → host  ready | st {s, ak, f, p, v, y, pi, mx, mz, b, c, tl, tr, g, fl, sl} | cast {id, text, aim, combo}
 *                  hurt {amount, by} (wall slam after a throw) | cmd {op} | resync
 * Positions are millimetre integers, velocities cm/s, rotations "smallest three" quaternions (quant.ts).
 */
import { Quaternion, Vector3 } from 'three';
import type { World, GameEvent, Projectile } from '../sim/world.js';
import { NO_INPUT, emptyMods, type Player, type PlayerInput, type PlayerMods, type Tether } from '../sim/player.js';
import { Match, type MatchSnapshot } from '../sim/match.js';
import { Director } from '../lang/director.js';
import type { CommitResult } from '../lang/types.js';
import type { ConstructSpec } from '../sim/constructs/spec.js';
import { groups } from '../sim/physics.js';
import { SnapshotBuffer, slerpQuat, type Quat, type Vec3 } from './interp.js';
import { POS_STEP, VEL_STEP, dequantizeQuat, quantizeQuat } from './quant.js';
import { HOST_PEER, type NetMessage, type PeerEvent, type PeerId } from './protocol.js';

/** Remote things render this far in the past (ms). */
export const DELAY_MS = 100;
/** Snapshot every N host ticks (60 Hz sim → 30 Hz). */
export const SNAP_EVERY = 2;
/** Stay well under the transport's 64 KB message cap. */
export const MAX_MSG_BYTES = 60 * 1024;
/** Events per snapshot (the rest wait for the next one). */
const MAX_EVENTS = 48;
/** A client state older than this no longer drives the proxy (ms). */
const STALE_MS = 400;

/** What the sessions need from the transport (NetApi in the app, NetHost / NetClient in tests). */
export interface GameLink {
  send(to: PeerId | 'all', msg: NetMessage): void;
  /** Host clock in ms (client: estimated from ping/pong). */
  hostNow(): number;
}

/** A notice for the player (banner / toast): the app decides how to show it. */
export interface NetNotice { kind: 'pause' | 'resume' | 'left' | 'desync' | 'info'; text: string }

// ------------------------------------------------------------------------------------------------ encoding

const qp = (v: number) => Math.round(v / POS_STEP);
const qv = (v: number) => Math.round(v / VEL_STEP);
const qa = (v: number) => Math.round(v * 1e4);
const P3 = (v: { x: number; y: number; z: number }): number[] => [qp(v.x), qp(v.y), qp(v.z)];
const V3 = (v: { x: number; y: number; z: number }): number[] => [qv(v.x), qv(v.y), qv(v.z)];
const Q4 = (q: { x: number; y: number; z: number; w: number }): number[] => quantizeQuat([q.x, q.y, q.z, q.w]);
const dP = (a: unknown): Vec3 | null => (isNums(a, 3) ? [a[0] * POS_STEP, a[1] * POS_STEP, a[2] * POS_STEP] : null);
const dV = (a: unknown): Vec3 | null => (isNums(a, 3) ? [a[0] * VEL_STEP, a[1] * VEL_STEP, a[2] * VEL_STEP] : null);
const dQ = (a: unknown): Quat | null => (isNums(a, 4) ? dequantizeQuat(a as [number, number, number, number]) : null);
const num = (v: unknown, d = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : d);
const r3 = (v: number) => Math.round(v * 1000) / 1000;
function isNums(a: unknown, n: number): a is number[] {
  if (!Array.isArray(a) || a.length !== n) return false;
  for (const x of a) if (typeof x !== 'number' || !Number.isFinite(x)) return false;
  return true;
}
const vec = (a: Vec3) => new Vector3(a[0], a[1], a[2]);
/** JSON replacer for events: 3-decimal numbers, non-finite → null. */
const roundNums = (_k: string, v: unknown) => (typeof v === 'number' ? (Number.isFinite(v) ? Math.round(v * 1000) / 1000 : null) : v);
const size = (m: unknown) => JSON.stringify(m).length;

const BITS: (keyof PlayerInput)[] = ['jump', 'crouch', 'boost', 'hookL', 'hookR', 'reel', 'flyToggle', 'descend', 'use', 'primary', 'secondary'];
export function encodeBits(i: PlayerInput): number { let b = 0; BITS.forEach((k, n) => { if (i[k]) b |= 1 << n; }); return b; }
export function decodeBits(b: number): Partial<PlayerInput> { const o: any = {}; BITS.forEach((k, n) => { o[k] = !!(b & (1 << n)); }); return o; }

const modsDefault = (m: PlayerMods) => m.groundSpeedMul === 1 && m.frictionMul === 1 && m.airAccelMul === 1 && m.thrust.lengthSq() === 0 && m.thrustAlongLook === 0 && m.glide === 0 && !isFinite(m.maxFallSpeed);
const encodeMods = (m: PlayerMods) => [r3(m.groundSpeedMul), r3(m.frictionMul), r3(m.airAccelMul), r3(m.thrust.x), r3(m.thrust.y), r3(m.thrust.z), r3(m.thrustAlongLook), r3(m.glide), isFinite(m.maxFallSpeed) ? r3(m.maxFallSpeed) : -1];
function decodeMods(a: unknown, sources: string[] = []): PlayerMods {
  const m = emptyMods();
  if (!isNums(a, 9)) return m;
  m.groundSpeedMul = a[0]; m.frictionMul = a[1]; m.airAccelMul = a[2]; m.thrust.set(a[3], a[4], a[5]); m.thrustAlongLook = a[6]; m.glide = a[7]; m.maxFallSpeed = a[8] < 0 ? Infinity : a[8];
  m.sources = sources;
  return m;
}

/** The cast result a client needs (specs travel separately, in cadd). */
function trimResult(r: CommitResult): Partial<CommitResult> {
  return {
    text: r.text, summary: r.summary, confidence: r.confidence, source: r.source, approximations: r.approximations.slice(0, 8), unknownWords: r.unknownWords.slice(0, 8),
    abstain: r.abstain, needsEscalation: false, applied: r.applied, messages: r.messages.slice(0, 8), clamps: r.clamps.slice(0, 8), constructIds: r.constructIds, latencyMs: r.latencyMs, escalated: false,
    ops: r.ops.map((o) => (o.op === 'control' ? { op: o.op, action: o.action, name: o.name } : o.op === 'abstain' ? o : { op: o.op })) as any,
  };
}

/** Fake tether (display only) at a hook tip reported by the other side. */
function displayTether(side: 'L' | 'R', at: Vec3, now: number): Tether {
  const p = vec(at);
  return { side, state: 'attached', origin: p, target: p.clone(), dir: new Vector3(0, 1, 0), travel: 0, dist: 0, body: null, local: new Vector3(), length: 1, attachTime: now, constructId: null, netDisplay: true } as Tether;
}
const tetherTip = (t: Tether | null) => (t ? P3(t.origin) : 0);

// ------------------------------------------------------------------------------------------------ host

interface RemoteState {
  seq: number; ack: number; free: boolean; at: number;
  pos: Vector3; vel: Vector3; yaw: number; pitch: number; onGround: boolean; flying: boolean; sliding: boolean;
  input: PlayerInput; tl: Vec3 | null; tr: Vec3 | null;
}

interface Remote {
  peer: PeerId;
  player: Player;
  director: Director;
  state: RemoteState | null;
  edges: { use: boolean; cycle: number; primary: boolean; secondary: boolean };
  /** last host tick in which the host moved this lantern (dead / held / riding / no fresh client state) */
  hostOwnedTick: number;
  ready: boolean;
  lost: boolean;
  // per-peer replication caches (cleared for a full resync)
  sentC: Map<string, string>;
  knownC: Map<string, number>;
  sentE: Map<string, string>;
  lastMatch: string;
  full: boolean;
  castsThisSecond: number;
  castWindow: number;
  /** velocity written into the proxy before the tick (knockback = what the tick added) */
  vel0: Vector3 | null;
}

export interface HostSessionOptions {
  /** Called with a remote player's cast result (host HUD may show a line). */
  onRemoteCast?: (playerId: string, r: CommitResult) => void;
  onNotice?: (n: NetNotice) => void;
}

export class HostSession {
  private remotes = new Map<PeerId, Remote>();
  private events: GameEvent[] = [];
  private listener = (e: GameEvent) => { if (e.type !== 'entity_spawn' && e.type !== 'player_joined') this.events.push(e); if (this.events.length > 400) this.events.splice(0, 100); };
  private ticksSinceSnap = 0;
  readonly stats = { snaps: 0, bytes: 0, maxSnap: 0, corrections: 0, casts: 0 };

  constructor(readonly world: World, readonly link: GameLink, readonly opts: HostSessionOptions = {}) {
    world.listeners.push(this.listener);
  }

  /** The match waits (no ticks) until every client has loaded, and while one is reconnecting. */
  get paused(): boolean { for (const r of this.remotes.values()) if (!r.ready || r.lost) return true; return false; }
  get pauseReason(): string {
    for (const r of this.remotes.values()) {
      if (r.lost) return `${r.player.name} lost connection — waiting for them to come back…`;
      if (!r.ready) return `waiting for ${r.player.name} to load…`;
    }
    return '';
  }

  playerOf(peer: PeerId): Player | null { return this.remotes.get(peer)?.player ?? null; }

  /** A peer joined: give them a lantern ('p2' …) and tell them to load the scene. */
  addPeer(peer: PeerId, name: string, scene: string): Player {
    const w = this.world;
    const id = `p${w.players.length + 1}`;
    const player = w.addPlayer(id, name);
    const director = new Director(w, id);
    const r: Remote = {
      peer, player, director, state: null, edges: { use: false, cycle: 0, primary: false, secondary: false }, hostOwnedTick: w.tick, ready: false, lost: false,
      sentC: new Map(), knownC: new Map(), sentE: new Map(), lastMatch: '', full: true, castsThisSecond: 0, castWindow: 0, vel0: null,
    };
    this.remotes.set(peer, r);
    w.externallyDriven.add(id);
    this.link.send(peer, { t: 'start', scene, you: id, names: Object.fromEntries(w.players.map((p) => [p.id, p.name])) });
    return player;
  }

  /** Transport peer events: lost → pause, resume → full resync, leave → the opponent is gone. */
  onPeer(e: PeerEvent) {
    const r = this.remotes.get(e.peerId);
    if (!r) return;
    if (e.kind === 'lost') { r.lost = true; this.opts.onNotice?.({ kind: 'pause', text: `${r.player.name} lost connection — match paused` }); }
    else if (e.kind === 'resume') { r.lost = false; this.resync(r); this.opts.onNotice?.({ kind: 'resume', text: `${r.player.name} is back` }); }
    else if (e.kind === 'leave') {
      r.lost = true; // keep the lantern (frozen); the app decides what's next
      this.opts.onNotice?.({ kind: 'left', text: `${r.player.name} left the match` });
    }
  }

  private resync(r: Remote) {
    r.sentC.clear(); r.knownC.clear(); r.sentE.clear(); r.lastMatch = ''; r.full = true; r.state = null;
    r.hostOwnedTick = this.world.tick;
  }

  onMessage(from: PeerId, msg: NetMessage) {
    const r = this.remotes.get(from);
    if (!r) return;
    switch (msg.t) {
      case 'ready': r.ready = true; this.resync(r); break;
      case 'resync': this.resync(r); break;
      case 'st': this.onState(r, msg); break;
      case 'cast': this.onCast(r, msg); break;
      case 'hurt': {
        // the client's own lantern slammed into something after a throw / launch (Player.update's impact rule)
        const p = r.player, amount = num(msg.amount);
        const by = typeof msg.by === 'string' ? msg.by.slice(0, 40) : null;
        if (amount > 0 && amount <= 25 && p.alive) this.world.damagePlayer(p, amount, 'impact', by);
        break;
      }
      case 'cmd': {
        const w = this.world, id = r.player.id;
        if (msg.op === 'dismiss') { const f = w.constructs.focusOf(id); if (f) w.constructs.dismiss(f); }
        else if (msg.op === 'dismiss_all') { for (const c of [...w.constructs.list.values()]) if (c.owner === id) w.constructs.dismiss(c.id); }
        break;
      }
    }
  }

  private onState(r: Remote, m: NetMessage) {
    const pos = dP(m.p), vel = dV(m.v);
    if (!pos || !vel) return;
    const seq = num(m.s, -1);
    if (r.state && seq <= r.state.seq) return; // stale / reordered
    const b = num(m.b) | 0;
    const bits = decodeBits(b);
    const clampAxis = (x: unknown) => Math.max(-1, Math.min(1, num(x) / 100));
    const input: PlayerInput = { ...NO_INPUT, ...bits, moveX: clampAxis(m.mx), moveZ: clampAxis(m.mz), yaw: num(m.y) / 1e4, pitch: Math.max(-1.6, Math.min(1.6, num(m.pi) / 1e4)), flyToggle: false, cycle: 0 };
    r.edges.use ||= !!bits.use;
    r.edges.primary ||= !!bits.primary;
    r.edges.secondary ||= !!bits.secondary;
    r.edges.cycle = Math.max(-8, Math.min(8, r.edges.cycle + Math.round(num(m.c))));
    r.state = {
      seq, ack: num(m.ak, -1), free: m.f === 1, at: this.link.hostNow(),
      pos: vec(pos), vel: vec(vel), yaw: input.yaw, pitch: input.pitch, onGround: m.g === 1, flying: m.fl === 1, sliding: m.sl === 1,
      input, tl: dP(m.tl), tr: dP(m.tr),
    };
  }

  private onCast(r: Remote, m: NetMessage) {
    const w = this.world;
    const text = typeof m.text === 'string' ? m.text.slice(0, 240).trim() : '';
    const id = num(m.id);
    // a little flood guard: 8 casts per second is more than anyone can type
    const sec = Math.floor(w.time);
    if (sec !== r.castWindow) { r.castWindow = sec; r.castsThisSecond = 0; }
    if (!text || ++r.castsThisSecond > 8) { this.link.send(r.peer, { t: 'cres', id, r: null }); return; }
    const p = r.player;
    const aim = m.aim as any;
    // resolve "aim" from where the client was looking when they pressed Enter
    if (aim && typeof aim === 'object') {
      if (typeof aim.y === 'number' && Number.isFinite(aim.y)) p.yaw = aim.y / 1e4;
      if (typeof aim.pi === 'number' && Number.isFinite(aim.pi)) p.pitch = Math.max(-1.6, Math.min(1.6, aim.pi / 1e4));
      const at = dP(aim.p);
      if (at && w.externallyDriven.has(p.id) && p.alive && !p.heldBy && !p.riding) { p.pos.set(...at); p.body.setTranslation(p.pos, true); }
    }
    const insp = w.inspiration.of(p.id);
    insp.combo = Math.max(1, Math.min(4, num(m.combo, 1)));
    let res: CommitResult | null = null;
    try { res = r.director.commitSync(text); } catch (err) { console.warn('[net] remote cast failed', err); }
    insp.combo = 1;
    this.stats.casts++;
    if (res) this.opts.onRemoteCast?.(p.id, res);
    // the new construct's spec goes out before the result (ordered channel), so it exists when the result lands
    this.sendConstructs(r);
    this.link.send(r.peer, { t: 'cres', id, r: res ? trimResult(res) : null });
  }

  /** Before world.step(): write each client's reported pose into their proxy (when the client owns it). */
  beforeStep() {
    const w = this.world;
    const now = this.link.hostNow();
    for (const r of this.remotes.values()) {
      const p = r.player, s = r.state;
      const hostOwned = !p.alive || !!p.heldBy || !!p.riding;
      const fresh = !!s && s.free && now - s.at < STALE_MS && s.ack >= r.hostOwnedTick;
      const drive = !hostOwned && fresh;
      if (drive) {
        w.externallyDriven.add(p.id);
        p.pos.copy(s!.pos); p.vel.copy(s!.vel);
        p.yaw = s!.yaw; p.pitch = s!.pitch;
        p.onGround = s!.onGround; p.flying = s!.flying; p.sliding = s!.sliding;
        p.body.setTranslation(p.pos, true);
        // display tethers (the client simulates the real ones)
        for (const side of ['L', 'R'] as const) {
          const tip = side === 'L' ? s!.tl : s!.tr;
          const cur = p.tethers[side];
          if (cur && !(cur as any).netDisplay) continue; // a construct's tether: corrected below
          p.tethers[side] = tip ? displayTether(side, tip, w.time) : null;
        }
        p.launchGrace = 0; p.flungBy = null;
      } else {
        w.externallyDriven.delete(p.id);
        // only real host ownership (not merely waiting for a fresh state) invalidates older client states
        if (hostOwned) r.hostOwnedTick = w.tick;
        for (const side of ['L', 'R'] as const) if ((p.tethers[side] as any)?.netDisplay) p.tethers[side] = null;
      }
      const inp = s?.input ?? { ...NO_INPUT, yaw: p.yaw, pitch: p.pitch };
      // the host moves this lantern (just thrown, reconnecting…): momentum only, no stale steering
      const steer = drive || hostOwned;
      p.input = {
        ...(steer ? inp : { ...NO_INPUT, yaw: p.yaw, pitch: p.pitch }),
        use: r.edges.use, cycle: r.edges.cycle, primary: inp.primary || r.edges.primary, secondary: inp.secondary || r.edges.secondary, flyToggle: false,
      };
      r.edges = { use: false, cycle: 0, primary: false, secondary: false };
      r.vel0 = drive ? p.vel.clone() : null;
    }
  }

  /** After world.step(): corrections for the clients' own lanterns, and snapshots at 30 Hz. */
  afterStep() {
    const w = this.world;
    for (const r of this.remotes.values()) {
      const p = r.player;
      const v0 = r.vel0;
      if (!v0 || r.lost) continue;
      const corr: NetMessage = { t: 'corr', k: w.tick };
      let any = false;
      const dv = p.vel.clone().sub(v0);
      if (dv.lengthSq() > 1e-4 && p.alive && !p.heldBy && !p.riding) {
        corr.dv = V3(dv);
        corr.lg = r3(p.launchGrace);
        if (p.flungBy) { corr.fb = p.flungBy; corr.fu = r3(p.flungUntil); }
        any = true;
      }
      for (const side of ['L', 'R'] as const) {
        const t = p.tethers[side];
        if (t && !(t as any).netDisplay && t.constructId && !(t as any).netSent) {
          (t as any).netSent = true;
          corr.teth = { side, p: P3(t.target), cid: t.constructId, auto: !!t.autoReel };
          any = true;
        }
      }
      if (any) { this.link.send(r.peer, corr); this.stats.corrections++; }
    }
    if (++this.ticksSinceSnap >= SNAP_EVERY) { this.ticksSinceSnap = 0; this.broadcast(); }
  }

  /** One host tick: apply client states, step, correct, replicate. */
  step(input: PlayerInput) {
    this.beforeStep();
    this.world.step(input);
    this.afterStep();
  }

  private broadcast() {
    const events = this.events.splice(0, MAX_EVENTS);
    for (const r of this.remotes.values()) {
      if (!r.ready || r.lost) continue;
      this.sendConstructs(r);
      const snap = this.buildSnap(r, events);
      const n = size(snap);
      if (n > MAX_MSG_BYTES) {
        // never exceed the cap: drop the heavy optional parts and resend them next time
        delete snap.ev; delete snap.pr;
        r.sentE.clear(); r.sentC.clear();
        console.warn(`[net] snapshot of ${n} B trimmed`);
      }
      this.link.send(r.peer, snap);
      this.stats.snaps++; this.stats.bytes += n; this.stats.maxSnap = Math.max(this.stats.maxSnap, n);
      r.full = false;
    }
  }

  /** cadd for every construct this peer hasn't seen at its current version (ordered before the snapshot). */
  private sendConstructs(r: Remote) {
    if (!r.ready || r.lost) return;
    const w = this.world;
    const ht = this.link.hostNow();
    for (const c of w.constructs.list.values()) {
      if (r.knownC.get(c.id) === c.version) continue;
      const msg: NetMessage = {
        t: 'cadd', ht, id: c.id, o: c.owner, v: c.version, hp: r3(c.hp), mhp: r3(c.maxHp), born: c.born, exp: isFinite(c.expires) ? c.expires : -1,
        spec: c.spec as unknown as Record<string, unknown>, ps: c.insts.map((i) => [...P3(i.pos), ...Q4(i.quat)]),
      };
      if (size(msg) > MAX_MSG_BYTES) { console.warn(`[net] construct ${c.id} spec too large to send`); r.knownC.set(c.id, c.version); continue; }
      this.link.send(r.peer, msg);
      r.knownC.set(c.id, c.version);
      r.sentC.delete(c.id);
    }
  }

  private buildSnap(r: Remote, events: GameEvent[]): NetMessage {
    const w = this.world;
    const snap: NetMessage = { t: 'snap', ht: this.link.hostNow(), k: w.tick, tm: w.time };
    if (r.full) snap.full = 1;
    snap.pl = w.players.map((p) => {
      const rec: Record<string, unknown> = {
        id: p.id, p: P3(p.pos), v: V3(p.vel), y: qa(p.yaw), pi: qa(p.pitch), g: p.onGround ? 1 : 0, fl: p.flying ? 1 : 0, sl: p.sliding ? 1 : 0,
        hp: r3(p.hp), al: p.alive ? 1 : 0, ra: isFinite(p.respawnAt) ? p.respawnAt : -1, pu: isFinite(p.protectedUntil) ? p.protectedUntil : -1,
        en: r3(p.energy), ga: r3(p.gas), in: r3(w.inspiration.of(p.id).level), ha: isFinite(p.hurtAt) ? p.hurtAt : -1, hd: [r3(p.hurtDir.x), r3(p.hurtDir.y), r3(p.hurtDir.z)],
        da: isFinite(p.diedAt) ? p.diedAt : -1, hb: p.heldBy, rd: p.riding?.id ?? null, tl: tetherTip(p.tethers.L), tr: tetherTip(p.tethers.R),
      };
      if (p.id === r.player.id) {
        const mods = w.externallyDriven.has(p.id) ? p.netMods : p.mods;
        if (!modsDefault(mods)) { rec.mo = encodeMods(mods); rec.ms = mods.sources.slice(0, 4); }
        rec.act = w.actions.netState(p.id);
      }
      return rec;
    });
    // constructs: poses (+ joint poses, hp) of the ones that changed since the last snapshot to this peer
    const c: Record<string, unknown> = {};
    for (const con of w.constructs.list.values()) {
      if (r.knownC.get(con.id) !== con.version) continue; // spec not sent yet (too big)
      const rec: Record<string, unknown> = { h: r3(con.hp), i: con.insts.map((i) => [...P3(i.pos), ...Q4(i.quat)]) };
      if (con.insts.some((i) => i.jointPose?.size)) {
        rec.j = con.insts.map((i) => {
          const o: Record<string, number[]> = {};
          if (i.jointPose) for (const [g, ps] of i.jointPose) o[g] = [...P3(ps.pos), ...Q4(ps.quat)];
          return o;
        });
      }
      const s = JSON.stringify(rec);
      if (r.sentC.get(con.id) !== s) { c[con.id] = rec; r.sentC.set(con.id, s); }
    }
    const cr: string[] = [];
    for (const id of [...r.knownC.keys()]) if (!w.constructs.list.has(id)) { cr.push(id); r.knownC.delete(id); r.sentC.delete(id); }
    if (Object.keys(c).length) snap.c = c;
    if (cr.length) snap.cr = cr;
    // entities: pose / hp / flags when changed; creation info for ones this peer hasn't seen
    const e: Record<string, unknown> = {};
    const en: unknown[] = [];
    for (const ent of w.entities.values()) {
      const t = ent.body.translation(), q = ent.body.rotation();
      const rec = [...P3(t), ...Q4(q), r3(Math.min(ent.hp, 1e6)), (ent.disabled ? 1 : 0) | (ent.heldBy || ent.restrainedBy ? 2 : 0)];
      const s = rec.join(',');
      const prev = r.sentE.get(ent.id);
      if (prev === undefined) {
        en.push({ id: ent.id, ty: ent.type, va: ent.variant, lb: ent.label, ho: ent.hostile ? 1 : 0, sz: ent.size ? [ent.size.x, ent.size.y, ent.size.z] : null, co: ent.color, gr: ent.group, mhp: r3(Math.min(ent.maxHp, 1e6)) });
      }
      if (prev !== s) { e[ent.id] = rec; r.sentE.set(ent.id, s); }
    }
    const er: string[] = [];
    for (const id of [...r.sentE.keys()]) if (!w.entities.has(id)) { er.push(id); r.sentE.delete(id); }
    if (en.length) snap.en = en;
    if (Object.keys(e).length) snap.e = e;
    if (er.length) snap.er = er;
    // projectiles: all of them (short-lived, always moving)
    if (w.projectiles.size) snap.pr = [...w.projectiles.values()].slice(0, 200).map((p) => { const t = p.body.translation(), v = p.body.linvel(); return [p.id, ...P3(t), ...V3(v), p.reflected ? 1 : 0, p.ownerPlayer ?? 0]; });
    // match (when it changed) + the hash of the discrete state after this tick
    const m = w.match ? JSON.stringify(w.match.snapshot()) : 'null';
    if (m !== r.lastMatch) { snap.m = w.match ? w.match.snapshot() : null; r.lastMatch = m; }
    if (events.length) snap.ev = JSON.parse(JSON.stringify(events, roundNums));
    snap.h = w.stateHash('sync');
    return snap;
  }

  dispose() {
    const i = this.world.listeners.indexOf(this.listener);
    if (i >= 0) this.world.listeners.splice(i, 1);
  }
}

// ------------------------------------------------------------------------------------------------ client

interface PlayerView { p: Vec3; v: Vec3; y: number; pi: number; tl: Vec3 | null; tr: Vec3 | null; g: boolean; fl: boolean; sl: boolean }
interface ConView { i: number[][]; j?: Record<string, number[]>[] }
interface View { pl: Map<string, PlayerView>; c: Map<string, ConView>; e: Map<string, number[]>; pr: Map<number, number[]> }

const lerpArr = (a: number[], b: number[], t: number, quatAt = 3) => {
  const out = new Array(b.length);
  for (let k = 0; k < b.length; k++) out[k] = k < quatAt || k >= quatAt + 4 ? a[k] + (b[k] - a[k]) * t : 0;
  if (b.length >= quatAt + 4) {
    const q = slerpQuat(a.slice(quatAt, quatAt + 4) as Quat, b.slice(quatAt, quatAt + 4) as Quat, t);
    for (let k = 0; k < 4; k++) out[quatAt + k] = q[k];
  }
  return out;
};
const lerp3 = (a: Vec3, b: Vec3, t: number): Vec3 => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
const lerpAng = (a: number, b: number, t: number) => { let d = (b - a) % (Math.PI * 2); if (d > Math.PI) d -= Math.PI * 2; if (d < -Math.PI) d += Math.PI * 2; return a + d * t; };

function lerpView(a: View, b: View, t: number): View {
  const pl = new Map<string, PlayerView>();
  for (const [id, pb] of b.pl) {
    const pa = a.pl.get(id);
    pl.set(id, pa ? { ...pb, p: lerp3(pa.p, pb.p, t), v: lerp3(pa.v, pb.v, t), y: lerpAng(pa.y, pb.y, t), pi: pa.pi + (pb.pi - pa.pi) * t, tl: pa.tl && pb.tl ? lerp3(pa.tl, pb.tl, t) : pb.tl, tr: pa.tr && pb.tr ? lerp3(pa.tr, pb.tr, t) : pb.tr } : pb);
  }
  const c = new Map<string, ConView>();
  for (const [id, cb] of b.c) {
    const ca = a.c.get(id);
    if (!ca || ca.i.length !== cb.i.length) { c.set(id, cb); continue; }
    const j = cb.j && ca.j && ca.j.length === cb.j.length ? cb.j.map((jb, k) => { const o: Record<string, number[]> = {}; for (const g in jb) o[g] = ca.j![k][g] ? lerpArr(ca.j![k][g], jb[g], t) : jb[g]; return o; }) : cb.j;
    c.set(id, { i: cb.i.map((ib, k) => lerpArr(ca.i[k], ib, t)), j });
  }
  const e = new Map<string, number[]>();
  for (const [id, eb] of b.e) { const ea = a.e.get(id); e.set(id, ea ? lerpArr(ea, eb, t) : eb); }
  const pr = new Map<number, number[]>();
  for (const [id, pb] of b.pr) { const pa = a.pr.get(id); pr.set(id, pa ? pb.map((x, k) => (k < 3 ? pa[k] + (x - pa[k]) * t : x)) : pb); }
  return { pl, c, e, pr };
}

/** A discrete update waiting for its render time. */
type Item = { ht: number; msg: NetMessage };

export interface ClientSessionOptions {
  onNotice?: (n: NetNotice) => void;
  /** Local clock (ms) — tests may inject; defaults to the link's hostNow minus nothing. */
  delayMs?: number;
}

export class ClientSession {
  readonly me: Player;
  private buf = new SnapshotBuffer<View>(lerpView, 90, 120);
  private cur: View = { pl: new Map(), c: new Map(), e: new Map(), pr: new Map() };
  private queue: Item[] = [];
  private seq = 0;
  /** host tick of the last snapshot applied at render time (acked in every state message) */
  private applied = -1;
  private hostOwned = true;
  private castId = 0;
  private pendingCasts = new Map<number, (r: CommitResult | null) => void>();
  private timeSynced = false;
  private lastFrame: { ht: number; tm: number } | null = null;
  paused = false;
  pauseReason = '';
  started = false;
  readonly stats = { snaps: 0, desyncs: 0, hashChecks: 0, corrections: 0, lastHash: '' };
  readonly delayMs: number;

  constructor(readonly world: World, readonly link: GameLink, readonly playerId: string, names: Record<string, string> = {}, readonly opts: ClientSessionOptions = {}) {
    this.delayMs = opts.delayMs ?? DELAY_MS;
    const w = world;
    w.mirror = true;
    // every lantern the host has, in the same order (collision bits follow the index)
    const ids = Object.keys(names).length ? Object.keys(names).sort((a, b) => num(Number(a.slice(1))) - num(Number(b.slice(1)))) : ['p1', playerId];
    for (const id of ids) { const p = w.addPlayer(id); if (names[id]) p.name = names[id]; }
    const me = w.playerById(playerId) ?? w.addPlayer(playerId);
    this.me = me;
    w.player = me;
    w.localId = playerId;
    w.onMirrorDamage = (target, amount, cause, by) => {
      if (target === me && cause === 'impact') this.link.send(HOST_PEER, { t: 'hurt', amount: r3(Math.min(25, amount)), by });
    };
    // the host moves entities: make them puppets
    for (const e of w.entities.values()) this.puppetEntity(e.id);
    w.refreshQueries();
  }

  /** Tell the host we're loaded (it waits for this before the warmup runs). */
  ready() { this.link.send(HOST_PEER, { t: 'ready' }); this.started = true; }

  private puppetEntity(id: string) {
    const w = this.world, e = w.entities.get(id);
    if (!e) return;
    e.body.setBodyType(w.R.RigidBodyType.KinematicPositionBased, true);
    e.ai = null;
  }

  onPeer(e: PeerEvent) {
    if (e.peerId !== HOST_PEER) return;
    if (e.kind === 'lost') { this.paused = true; this.pauseReason = 'connection to the host lost — reconnecting…'; this.opts.onNotice?.({ kind: 'pause', text: this.pauseReason }); }
    else if (e.kind === 'resume') { this.paused = false; this.pauseReason = ''; this.link.send(HOST_PEER, { t: 'ready' }); this.opts.onNotice?.({ kind: 'resume', text: 'reconnected' }); }
    else if (e.kind === 'leave') { this.paused = true; this.pauseReason = `${e.name} left the match`; this.opts.onNotice?.({ kind: 'left', text: this.pauseReason }); }
  }

  onMessage(_from: PeerId, msg: NetMessage) {
    switch (msg.t) {
      case 'snap': this.onSnap(msg); break;
      case 'cadd': if (typeof msg.ht === 'number') this.queue.push({ ht: msg.ht, msg }); break;
      case 'corr': this.onCorr(msg); break;
      case 'cres': {
        const f = this.pendingCasts.get(num(msg.id));
        if (f) { this.pendingCasts.delete(num(msg.id)); f(msg.r ? this.fullResult(msg.r as Partial<CommitResult>) : null); }
        break;
      }
      case 'pause': {
        const on = !!msg.on;
        this.opts.onNotice?.({ kind: on ? 'pause' : 'resume', text: typeof msg.why === 'string' ? msg.why.slice(0, 120) : on ? 'paused' : 'resumed' });
        break;
      }
    }
  }

  private fullResult(r: Partial<CommitResult>): CommitResult {
    return {
      text: String(r.text ?? ''), ops: Array.isArray(r.ops) ? r.ops : [], summary: String(r.summary ?? ''), confidence: num(r.confidence), source: (r.source ?? 'rule') as any,
      approximations: Array.isArray(r.approximations) ? r.approximations.map(String) : [], unknownWords: Array.isArray(r.unknownWords) ? r.unknownWords.map(String) : [],
      abstain: typeof r.abstain === 'string' ? r.abstain : null, needsEscalation: false, timings: { parseMs: 0 }, applied: !!r.applied,
      messages: Array.isArray(r.messages) ? r.messages.map(String) : [], clamps: Array.isArray(r.clamps) ? r.clamps.map(String) : [],
      constructIds: Array.isArray(r.constructIds) ? r.constructIds.map(String) : [], latencyMs: num(r.latencyMs), escalated: false,
    };
  }

  /** Send a typed command to the host; resolves with its result (null on refusal / timeout). */
  cast(text: string, combo = 1): Promise<CommitResult | null> {
    const id = ++this.castId;
    const p = this.me;
    this.link.send(HOST_PEER, { t: 'cast', id, text: text.slice(0, 240), aim: { p: P3(p.pos), y: qa(p.yaw), pi: qa(p.pitch) }, combo: r3(combo) });
    return new Promise((resolve) => {
      this.pendingCasts.set(id, resolve);
      setTimeout(() => { if (this.pendingCasts.delete(id)) resolve(null); }, 5000);
    });
  }

  command(op: 'dismiss' | 'dismiss_all') { this.link.send(HOST_PEER, { t: 'cmd', op }); }

  private onSnap(m: NetMessage) {
    const ht = num(m.ht, NaN);
    if (!Number.isFinite(ht)) return;
    this.stats.snaps++;
    const cur = this.cur;
    if (m.full) { cur.c.clear(); cur.e.clear(); }
    // reconstruct the full view (deltas are against what the host sent before, on an ordered channel)
    const pl = new Map<string, PlayerView>();
    for (const rec of Array.isArray(m.pl) ? m.pl as any[] : []) {
      const p = dP(rec?.p), v = dV(rec?.v);
      if (!p || !v || typeof rec.id !== 'string') continue;
      pl.set(rec.id, { p, v, y: num(rec.y) / 1e4, pi: num(rec.pi) / 1e4, tl: dP(rec.tl), tr: dP(rec.tr), g: rec.g === 1, fl: rec.fl === 1, sl: rec.sl === 1 });
    }
    cur.pl = pl;
    const c = new Map(cur.c);
    if (m.c && typeof m.c === 'object') {
      for (const [id, rec] of Object.entries(m.c as Record<string, any>)) {
        if (!Array.isArray(rec?.i)) continue;
        const i = (rec.i as unknown[]).map((a) => (isNums(a, 7) ? [...dP(a.slice(0, 3))!, ...dQ(a.slice(3))!] : null)).filter((x): x is number[] => !!x);
        const j = Array.isArray(rec.j) ? (rec.j as any[]).map((o) => { const out: Record<string, number[]> = {}; if (o && typeof o === 'object') for (const g in o) if (isNums(o[g], 7)) out[g] = [...dP(o[g].slice(0, 3))!, ...dQ(o[g].slice(3))!]; return out; }) : undefined;
        c.set(id, { i, j });
      }
    }
    for (const id of Array.isArray(m.cr) ? m.cr as string[] : []) c.delete(id);
    cur.c = c;
    const e = new Map(cur.e);
    if (m.e && typeof m.e === 'object') for (const [id, a] of Object.entries(m.e as Record<string, any>)) if (isNums(a, 9)) e.set(id, [...dP(a.slice(0, 3))!, ...dQ(a.slice(3, 7))!, a[7], a[8]]);
    for (const id of Array.isArray(m.er) ? m.er as string[] : []) e.delete(id);
    cur.e = e;
    const pr = new Map<number, number[]>();
    for (const a of Array.isArray(m.pr) ? m.pr as any[] : []) if (Array.isArray(a) && a.length >= 7 && isNums(a.slice(0, 7), 7)) { pr.set(a[0], [...dP(a.slice(1, 4))!, ...dV(a.slice(4, 7))!, a[7] ? 1 : 0]); if (typeof a[8] === 'string') this.prOwner.set(a[0], a[8].slice(0, 16)); }
    cur.pr = pr;
    this.buf.push(ht, { pl, c, e, pr });
    this.queue.push({ ht, msg: m });
  }

  private onCorr(m: NetMessage) {
    const P = this.me;
    if (this.hostOwned) return; // the host moves us right now; its snapshots carry the result
    this.stats.corrections++;
    const dv = dV(m.dv);
    if (dv) {
      P.vel.add(vec(dv));
      if (dv[1] > 0.5) P.onGround = false;
      P.launchGrace = Math.max(P.launchGrace, num(m.lg));
      if (typeof m.fb === 'string') { P.flungBy = m.fb; P.flungUntil = num(m.fu, this.world.time + 1.2); }
    }
    const t = m.teth as any;
    if (t && (t.side === 'L' || t.side === 'R') && typeof t.cid === 'string') {
      const at = dP(t.p);
      const c = this.world.constructs.get(t.cid);
      if (at) { const tt = P.attachTether(t.side, vec(at), c?.insts[0]?.body ?? null, t.cid); tt.autoReel = !!t.auto; }
    }
  }

  /** Render time on the host's clock: remote things are shown this far in the past. */
  renderTime() { return this.link.hostNow() - this.delayMs; }

  /**
   * One client tick (replaces world.step): apply what's due, pose remote things, move our own lantern,
   * then report our state to the host.
   */
  step(input: PlayerInput) {
    const w = this.world;
    if (this.paused || !this.started) return;
    const rt = this.renderTime();
    // discrete updates whose time has come (in arrival order)
    let n = 0;
    while (n < this.queue.length && this.queue[n].ht <= rt) n++;
    if (n) for (const it of this.queue.splice(0, n)) this.apply(it.msg);
    this.buf.prune(rt - 1000);
    // world clock: follow the host's sim time at render time, smoothly
    if (this.lastFrame) {
      const target = this.lastFrame.tm + Math.max(0, Math.min(0.25, (rt - this.lastFrame.ht) / 1000));
      if (!this.timeSynced || Math.abs(target - w.time) > 0.5) { w.time = target; this.timeSynced = true; }
      else w.time += w.dt + Math.max(-w.dt / 2, Math.min(w.dt / 2, (target - (w.time + w.dt)) * 0.1));
    }
    const s = this.buf.sample(rt);
    if (s) this.pose(s.state);
    // our own lantern: simulated here unless the host owns it (dead, held, riding)
    const P = this.me;
    if (!this.hostOwned) { P.mods = this.modsHold; P.update(input, w.dt); }
    else { P.input = input; P.events.length = 0; P.body.setTranslation(P.pos, true); P.mods = emptyMods(); }
    w.pw.step();
    w.tick++;
    this.sendState(input);
  }

  private sendState(input: PlayerInput) {
    const P = this.me;
    this.link.send(HOST_PEER, {
      t: 'st', s: ++this.seq, ak: this.applied, f: this.hostOwned ? 0 : 1, p: P3(P.pos), v: V3(P.vel), y: qa(P.yaw), pi: qa(P.pitch),
      mx: Math.round(input.moveX * 100), mz: Math.round(input.moveZ * 100), b: encodeBits(input), c: Math.round(input.cycle ?? 0),
      tl: tetherTip(P.tethers.L), tr: tetherTip(P.tethers.R), g: P.onGround ? 1 : 0, fl: P.flying ? 1 : 0, sl: P.sliding ? 1 : 0,
    });
  }

  /** Continuous state at render time: remote lanterns, construct / entity / projectile poses. */
  private pose(v: View) {
    const w = this.world;
    for (const p of w.players) {
      const pv = v.pl.get(p.id);
      if (!pv) continue;
      if (p === this.me && !this.hostOwned) continue;
      p.pos.set(...pv.p); p.vel.set(...pv.v);
      p.yaw = pv.y; p.pitch = pv.pi;
      p.onGround = pv.g; p.flying = pv.fl; p.sliding = pv.sl;
      p.body.setTranslation(p.pos, true);
      if (p !== this.me) {
        p.tethers.L = pv.tl ? displayTether('L', pv.tl, w.time) : null;
        p.tethers.R = pv.tr ? displayTether('R', pv.tr, w.time) : null;
      }
    }
    const rt = w.constructs;
    const q = new Quaternion(), pp = new Vector3();
    for (const [id, cv] of v.c) {
      const c = rt.get(id);
      if (!c) continue;
      cv.i.forEach((a, k) => {
        pp.set(a[0], a[1], a[2]); q.set(a[3], a[4], a[5], a[6]);
        let joints: Map<string, { pos: Vector3; quat: Quaternion }> | undefined;
        const jr = cv.j?.[k];
        if (jr && Object.keys(jr).length) { joints = new Map(); for (const g in jr) { const b = jr[g]; joints.set(g, { pos: new Vector3(b[0], b[1], b[2]), quat: new Quaternion(b[3], b[4], b[5], b[6]) }); } }
        rt.posePuppet(c, k, pp, q, joints);
      });
    }
    for (const [id, a] of v.e) {
      const e = w.entities.get(id);
      if (!e) continue;
      e.body.setNextKinematicTranslation({ x: a[0], y: a[1], z: a[2] });
      e.body.setNextKinematicRotation({ x: a[3], y: a[4], z: a[5], w: a[6] });
    }
    // projectiles: create / move / drop puppets (renderer puffs where one vanishes)
    for (const [id, a] of v.pr) {
      let pr = w.projectiles.get(id);
      if (!pr) pr = this.addProjectile(id, a);
      pr.body.setTranslation({ x: a[0], y: a[1], z: a[2] }, true);
      pr.body.setLinvel({ x: a[3], y: a[4], z: a[5] }, true);
      pr.reflected = a[6] === 1;
    }
    for (const pr of [...w.projectiles.values()]) if (!v.pr.has(pr.id)) { w.removeProjectile(pr); this.prOwner.delete(pr.id); }
  }

  private addProjectile(id: number, a: number[]): Projectile {
    const w = this.world, R = w.R;
    const body = w.pw.createRigidBody(R.RigidBodyDesc.kinematicVelocityBased().setTranslation(a[0], a[1], a[2]));
    const collider = w.pw.createCollider(R.ColliderDesc.ball(0.2).setSensor(true).setCollisionGroups(groups(0, 0)), body);
    const p: Projectile = { id, body, collider, from: null, by: null, reflected: a[6] === 1, born: w.time, damage: 0, ownerPlayer: this.prOwner.get(id) ?? null };
    w.projectiles.set(id, p);
    return p;
  }

  /** A discrete update reached its render time. */
  private apply(m: NetMessage) {
    const w = this.world;
    if (m.t === 'cadd') {
      const spec = m.spec as unknown as ConstructSpec;
      if (!spec || typeof spec !== 'object' || !Array.isArray(spec.parts) || typeof m.id !== 'string') return;
      const poses = (Array.isArray(m.ps) ? m.ps as unknown[] : []).filter((a): a is number[] => isNums(a, 7)).map((a) => ({ pos: vec(dP(a.slice(0, 3))!), quat: new Quaternion(...dQ(a.slice(3))!) }));
      try {
        w.constructs.upsertPuppet(m.id, String(m.o), spec, { version: num(m.v, 1), hp: num(m.hp), maxHp: num(m.mhp), born: num(m.born), expires: num(m.exp, -1) < 0 ? Infinity : num(m.exp) }, poses);
      } catch (err) { console.warn('[net] could not build construct', m.id, err); }
      return;
    }
    if (m.t !== 'snap') return;
    this.lastFrame = { ht: num(m.ht), tm: num(m.tm) };
    // entities: new ones, removed ones, hp / flags
    for (const info of Array.isArray(m.en) ? m.en as any[] : []) {
      if (!info || typeof info.id !== 'string' || w.entities.has(info.id)) continue;
      const rec = this.cur.e.get(info.id);
      const pos = rec ? new Vector3(rec[0], rec[1], rec[2]) : new Vector3();
      try {
        w.addEntity(info.ty, pos, { id: info.id, hostile: !!info.ho, label: String(info.lb ?? info.ty), variant: info.va === 'heavy' ? 'heavy' : 'standard', size: isNums(info.sz, 3) ? new Vector3(...info.sz) : undefined, color: typeof info.co === 'number' ? info.co : undefined, group: typeof info.gr === 'string' ? info.gr : undefined, hp: num(info.mhp, 100) });
        this.puppetEntity(info.id);
      } catch (err) { console.warn('[net] could not mirror entity', info.id, err); }
    }
    for (const id of Array.isArray(m.er) ? m.er as string[] : []) w.removeEntity(id);
    if (m.full) { const live = new Set(this.cur.e.keys()); for (const id of [...w.entities.keys()]) if (!live.has(id)) w.removeEntity(id); }
    if (m.e && typeof m.e === 'object') for (const [id, a] of Object.entries(m.e as Record<string, any>)) {
      const e = w.entities.get(id);
      if (!e || !isNums(a, 9)) continue;
      e.hp = a[7];
      if ((a[8] & 1) && !e.disabled) { e.disabled = true; e.disabledAt = w.time; }
      e.heldBy = a[8] & 2 ? e.heldBy ?? 'net' : null;
    }
    // constructs the host dismissed
    for (const id of Array.isArray(m.cr) ? m.cr as string[] : []) w.constructs.removePuppet(id);
    if (m.full) { const live = new Set(this.cur.c.keys()); for (const id of [...w.constructs.list.keys()]) if (!live.has(id)) w.constructs.removePuppet(id); }
    if (m.c && typeof m.c === 'object') for (const [id, rec] of Object.entries(m.c as Record<string, any>)) { const c = w.constructs.get(id); if (c && typeof rec?.h === 'number') c.hp = rec.h; }
    // players: health, life, willpower, inspiration, who holds / carries them
    for (const rec of Array.isArray(m.pl) ? m.pl as any[] : []) this.applyPlayer(rec);
    // the match
    if (m.m !== undefined) {
      if (m.m === null) w.match = null;
      else if (w.match) w.match.restore(m.m as unknown as MatchSnapshot);
      else w.match = Match.mirror(w, m.m as unknown as MatchSnapshot);
    }
    // events → fx, juice, HUD (after the state they describe)
    for (const e of Array.isArray(m.ev) ? m.ev as GameEvent[] : []) if (e && typeof e.type === 'string') w.replay(e);
    this.applied = num(m.k, this.applied);
    // desync check: our discrete state must hash like the host's did after this tick
    if (typeof m.h === 'string') {
      this.stats.hashChecks++;
      const mine = w.stateHash('sync');
      this.stats.lastHash = mine;
      if (mine !== m.h) {
        this.stats.desyncs++;
        console.warn(`[net] desync at host tick ${m.k}: host ${m.h}, client ${mine}`, JSON.stringify(w.syncState()));
        this.opts.onNotice?.({ kind: 'desync', text: 'out of sync with the host — resyncing' });
        this.link.send(HOST_PEER, { t: 'resync' });
      }
    }
  }

  private applyPlayer(rec: any) {
    const w = this.world;
    const p = w.playerById(rec?.id);
    if (!p) return;
    const wasOwned = p === this.me ? this.hostOwned : false;
    const alive = rec.al === 1;
    if (alive !== p.alive) p.setSolid(alive);
    p.alive = alive;
    p.hp = num(rec.hp, p.hp);
    p.respawnAt = num(rec.ra, -1) < 0 ? Infinity : rec.ra;
    p.protectedUntil = num(rec.pu, -1) < 0 ? -Infinity : rec.pu;
    p.hurtAt = num(rec.ha, -1) < 0 ? -Infinity : rec.ha;
    p.diedAt = num(rec.da, -1) < 0 ? -Infinity : rec.da;
    if (isNums(rec.hd, 3)) p.hurtDir.set(rec.hd[0], rec.hd[1], rec.hd[2]);
    p.energy = num(rec.en, p.energy);
    w.inspiration.of(p.id).level = num(rec.in, w.inspiration.of(p.id).level);
    p.heldBy = typeof rec.hb === 'string' ? rec.hb : null;
    const rd = typeof rec.rd === 'string' ? rec.rd : null;
    if (rd !== (p.riding?.id ?? null)) {
      const c = rd ? w.constructs.get(rd) : null;
      p.riding = rd ? { id: rd, body: c?.insts[0]?.body ?? null, camBack: Math.max(1.2, Math.min(12, (c?.radius ?? 2) * 0.9)), camUp: Math.max(0.3, Math.min(4, (c?.extent.size.y ?? 2) * 0.3)) } : null;
    }
    if (p !== this.me) return;
    if (rec.act && typeof rec.act === 'object') w.actions.applyNetState(p.id, rec.act);
    const owned = !p.alive || !!p.heldBy || !!p.riding;
    if (owned) {
      p.gas = num(rec.ga, p.gas);
      p.releaseTether('L'); p.releaseTether('R');
    }
    if (wasOwned && !owned) {
      // the host hands our lantern back (respawned, released, thrown, got out): continue from its state then
      const pos = dP(rec.p), vel = dV(rec.v);
      if (pos && vel) { p.teleport(vec(pos), vec(vel)); p.onGround = false; }
      p.gas = num(rec.ga, p.gas);
      p.yaw = num(rec.y) / 1e4; p.pitch = num(rec.pi) / 1e4;
    }
    this.hostOwned = owned;
    // construct mods (jetpacks, wings, boots…) for our own movement until the next snapshot
    this.modsHold = rec.mo ? decodeMods(rec.mo, Array.isArray(rec.ms) ? rec.ms.map(String) : []) : emptyMods();
  }

  private modsHold: PlayerMods = emptyMods();
  private prOwner = new Map<number, string>();

  /** True while the host moves our lantern (dead, held, riding). */
  get hostControlled() { return this.hostOwned; }

  dispose() {
    for (const f of this.pendingCasts.values()) f(null);
    this.pendingCasts.clear();
  }
}
