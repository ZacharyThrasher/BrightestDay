/**
 * JUICE core: the pure (DOM-, WebGL- and WebAudio-free, unit-tested) parts of game feel.
 *
 *   Trauma    — trauma-based screen shake (shake = trauma², linear decay).
 *   Spring    — damped springs for directional camera kicks and FOV punches.
 *   Hitstop   — render-side freeze scheduler: per-target holds of 40–120 ms scaled by energy, chains
 *               capped, a whole-view budget, then a smooth catch-up onto the live (sim) pose.
 *   mapEvent  — world / player event → Effect (what to shake, freeze, play, show). The sim is never
 *               touched: effects only change what is drawn and heard.
 */
import { JUICE } from './tuning.js';

export type V3 = [number, number, number];

export const clamp = (x: number, a: number, b: number) => (x < a ? a : x > b ? b : x);
export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
export const smoothstep = (t: number) => { const x = clamp(t, 0, 1); return x * x * (3 - 2 * x); };
/** Normalised impact size: value / ref, clamped to 0..1.5 (1 = a "big" hit). */
export const energy = (value: number | undefined, ref: number) => clamp((Number(value) || 0) / ref, 0, 1.5);

/** Distance falloff for world events: 1 inside `near`, smoothly to 0 at `far`. */
export function falloff(dist: number, near = JUICE.trauma.near, far = JUICE.trauma.far): number {
  if (!isFinite(dist)) return 0;
  if (dist <= near) return 1;
  if (dist >= far) return 0;
  return 1 - smoothstep((dist - near) / (far - near));
}

/** Smooth, deterministic 1D noise in −1..1 (sum of incommensurate sines; cheap and seamless). */
export function noise1(t: number, seed: number): number {
  return Math.sin(t * 1.0 + seed * 12.9898) * 0.5 + Math.sin(t * 2.31 + seed * 78.233) * 0.3 + Math.sin(t * 4.77 + seed * 37.719) * 0.2;
}

// ------------------------------------------------------------------------------------------ trauma

export class Trauma {
  value = 0;
  constructor(public decay = JUICE.trauma.decay, public max = JUICE.trauma.max, public exponent = JUICE.trauma.exponent) {}
  add(a: number) { if (a > 0 && isFinite(a)) this.value = Math.min(this.max, this.value + a); }
  update(dt: number) { this.value = Math.max(0, this.value - this.decay * Math.max(0, dt)); }
  /** 0..1 shake amount (trauma^exponent). */
  get shake() { return Math.pow(this.value, this.exponent); }
}

// ------------------------------------------------------------------------------------------ springs

/** Damped spring toward 0 (semi-implicit Euler, sub-stepped so large frame times stay stable). */
export class Spring {
  x = 0;
  v = 0;
  constructor(public k: number, public c: number) {}
  impulse(dv: number) { this.v += dv; }
  update(dt: number) {
    let t = Math.min(0.1, Math.max(0, dt));
    while (t > 1e-6) {
      const h = Math.min(t, 1 / 480);
      this.v += (-this.k * this.x - this.c * this.v) * h;
      this.x += this.v * h;
      t -= h;
    }
    if (Math.abs(this.x) < 5e-4 && Math.abs(this.v) < 5e-3) { this.x = 0; this.v = 0; }
  }
  get settled() { return this.x === 0 && this.v === 0; }
}

/**
 * Initial velocity that makes a spring (from rest) peak at `peak`. Lets the tuning table speak in
 * peak metres / degrees instead of impulses.
 */
export function impulseForPeak(peak: number, k: number, c: number): number {
  const w = Math.sqrt(k), z = c / (2 * w);
  if (z >= 1) return peak * w * Math.E; // critically damped approximation
  const wd = w * Math.sqrt(1 - z * z);
  const t = Math.atan2(wd, z * w) / wd;
  return (peak * wd) / (Math.exp(-z * w * t) * Math.sin(wd * t));
}

// ------------------------------------------------------------------------------------------ hitstop

interface Stop { chainStart: number; holdEnd: number; end: number; immuneUntil: number }

/**
 * Render-side hitstop. Keys name what is frozen: `c:<construct>`, `e:<entity>`, `p:<player>`, or `*`
 * (the whole view). The sim keeps running; the renderer blends a frozen pose back onto the live one
 * with `weight(key)` (1 = fully frozen, easing to 0 over the catch-up).
 */
export class Hitstop {
  now = 0; // ms
  private stops = new Map<string, Stop>();
  private globalLog: { at: number; ms: number }[] = [];
  /** stats */
  triggered = 0;
  refused = 0;
  totalHoldMs = 0;

  constructor(private cfg = JUICE.hitstop) {}

  /** Energy (0..1+) → hold length in ms. */
  msFor(e: number) { return Math.round(lerp(this.cfg.minMs, this.cfg.maxMs, clamp(e, 0, 1))); }

  /** Freeze `key` for `ms` (clamped to [min, max]). Returns the hold actually added (0 = refused). */
  trigger(key: string, ms: number): number {
    const c = this.cfg;
    ms = clamp(ms, c.minMs, c.maxMs);
    const s = this.stops.get(key);
    if (s && this.now < s.immuneUntil) { this.refused++; return 0; }
    if (key === '*') {
      this.globalLog = this.globalLog.filter((g) => this.now - g.at < 1000);
      const used = this.globalLog.reduce((a, g) => a + g.ms, 0);
      ms = Math.min(ms, c.globalBudgetMs - used);
      if (ms < c.minMs * 0.5) { this.refused++; return 0; }
    }
    let added: number;
    if (s && this.now < s.holdEnd) {
      // chain: extend, but never past chainStart + chainCap; hitting the cap grants immunity
      const cap = s.chainStart + c.chainCapMs;
      const holdEnd = Math.min(Math.max(s.holdEnd, this.now + ms), cap);
      added = holdEnd - s.holdEnd;
      s.holdEnd = holdEnd;
      s.end = holdEnd + c.catchUpMs;
      if (holdEnd >= cap) s.immuneUntil = s.end + c.immuneMs;
    } else {
      this.stops.set(key, { chainStart: this.now, holdEnd: this.now + ms, end: this.now + ms + c.catchUpMs, immuneUntil: s?.immuneUntil ?? 0 });
      added = ms;
    }
    if (added <= 0) { this.refused++; return 0; }
    if (key === '*') this.globalLog.push({ at: this.now, ms: added });
    this.triggered++;
    this.totalHoldMs += added;
    return added;
  }

  /** 1 while holding, smoothstep down to 0 during catch-up, 0 otherwise. */
  weight(key: string): number {
    const s = this.stops.get(key);
    if (!s || this.now >= s.end) return 0;
    if (this.now < s.holdEnd) return 1;
    return 1 - smoothstep((this.now - s.holdEnd) / this.cfg.catchUpMs);
  }

  /** True while `key` is in its hold phase (not catching up). */
  holding(key: string) { const s = this.stops.get(key); return !!s && this.now < s.holdEnd; }

  update(dtMs: number) {
    this.now += Math.max(0, dtMs);
    for (const [k, s] of this.stops) if (this.now >= s.end && this.now >= s.immuneUntil) this.stops.delete(k);
  }

  /** Keys currently frozen or catching up. */
  active(): string[] { const out: string[] = []; for (const [k, s] of this.stops) if (this.now < s.end) out.push(k); return out; }

  clear() { this.stops.clear(); this.globalLog = []; }
}

/** Kill-cam envelope 0..1: ease in over `easeIn`, hold, ease out over the last 40%. */
export function killCamWeight(t: number, dur = JUICE.camera.killCam.dur, easeIn = JUICE.camera.killCam.easeIn): number {
  if (t < 0 || t >= dur) return 0;
  if (t < easeIn) return smoothstep(t / easeIn);
  const out = dur * 0.6;
  return t < out ? 1 : 1 - smoothstep((t - out) / (dur - out));
}

// ------------------------------------------------------------------------------------------ event → effect

export type CalloutTone = 'hl' | 'gold' | 'dim' | 'bad';

export interface Effect {
  /** debug / counter label */
  kind: string;
  trauma?: number;
  hitstop?: { keys: string[]; ms: number };
  /** camera kick, as PEAK displacement: `mag` metres along `dir` (world, unit), `lift` metres up (−down), `pitch` degrees */
  kick?: { dir?: V3; mag: number; lift?: number; pitch?: number };
  fov?: number;
  sound?: { name: string; energy: number; pos?: V3 | null; kind?: string; novelty?: number };
  fx?: { pos: V3; energy: number; tone: 'construct' | 'white' | 'hurt'; constructId?: string };
  number?: { key: string; amount: number; pos?: V3 | null };
  marker?: 'hit' | 'kill' | 'pulse';
  /** a construct action charging (level 0..1) or released (null) — drives the wind-up hum */
  charge?: { constructId: string; level: number | null };
  callout?: { text: string; tone: CalloutTone; size?: number; sub?: string };
  flash?: { constructId: string; amount: number };
  squash?: { entityId: string; amount: number };
  killCam?: { key: string; pos?: V3 | null };
  hurt?: number;
}

export interface JuiceContext {
  /** local player id (`world.player`, 'p1') */
  local: string;
  /** owner (player id) of a construct, or null */
  ownerOf(constructId: string): string | null;
  /** distance from the camera to a world point (Infinity when unknown) */
  distance(pos: V3 | null | undefined): number;
  /** world position of a key (`c:`, `e:`, `p:`) or null */
  posOf(key: string): V3 | null;
  /** the local player's position */
  playerPos(): V3;
  /** resolve a bare id (entity / construct / player) to a `e:` / `c:` / `p:` key */
  keyOf?(id: string): string | null;
}

const asV3 = (p: any): V3 | null => (Array.isArray(p) && p.length >= 3 && p.every((x: any) => typeof x === 'number') ? [p[0], p[1], p[2]] : null);
const dirFrom = (from: V3 | null, to: V3 | null): V3 | undefined => {
  if (!from || !to) return undefined;
  const d: V3 = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
  const l = Math.hypot(d[0], d[1], d[2]);
  return l > 1e-4 ? [d[0] / l, d[1] / l, d[2] / l] : undefined;
};

export const CREATIVE_TEXT: Record<string, string> = {
  kill: 'CREATIVE KILL', hit: 'CREATIVE HIT', counter: 'CREATIVE COUNTER', block: 'CREATIVE BLOCK', defeat: 'CREATIVE TAKEDOWN', rescue: 'CREATIVE RESCUE', goal: 'CREATIVE ROUTE',
};

/** Novelty threshold for an "INSPIRED" callout (mirrors INSPIRED_AT in the sim; kept here so core stays sim-free). */
export const INSPIRED_NOVELTY = 0.4;

/**
 * Map one world event to its presentation effect (or null). Pure: everything it needs about the
 * world comes through `ctx`.
 */
export function mapEvent(e: { type: string; [k: string]: any }, ctx: JuiceContext): Effect | null {
  const S = JUICE.shake, R = JUICE.energyRef, HS = JUICE.hitstop;
  const local = ctx.local;
  /** did the local player (or one of their constructs) do this? */
  const mine = (by: any) => typeof by === 'string' && (by === local || ctx.ownerOf(by) === local);
  const pos = asV3(e.pos);
  const fall = (p: V3 | null) => falloff(ctx.distance(p));
  const hsMs = (en: number) => Math.round(lerp(HS.minMs, HS.maxMs, clamp(en, 0, 1)));

  switch (e.type) {
    case 'impact': {
      const en = energy(e.dv, R.dv);
      const m = mine(e.construct);
      const f = fall(pos);
      const trauma = (S.impact.base + S.impact.perEnergy * en) * (m ? lerp(0.4, 1, f) : f * 0.6);
      return {
        kind: 'impact', trauma,
        hitstop: m ? { keys: [`c:${e.construct}`, ...(e.entity ? [`e:${e.entity}`] : [])], ms: hsMs(en) } : undefined,
        kick: m && pos ? { dir: dirFrom(pos, ctx.playerPos()), mag: 0.04 + 0.12 * en } : undefined,
        sound: { name: 'impact', energy: en, pos },
        fx: m && pos ? { pos, energy: en, tone: 'construct', constructId: e.construct } : undefined,
        squash: e.entity ? { entityId: e.entity, amount: 0.4 + 0.6 * Math.min(1, en) } : undefined,
      };
    }
    case 'smash': {
      const en = energy(e.radius, R.radius) * 0.5 + 0.5;
      const m = mine(e.construct);
      return {
        kind: 'smash', trauma: (S.smash.base + S.smash.perEnergy * en) * (m ? lerp(0.5, 1, fall(pos)) : fall(pos)),
        hitstop: m ? { keys: [`c:${e.construct}`], ms: hsMs(en) } : undefined,
        sound: { name: 'impact', energy: Math.min(1.5, en + 0.3), pos },
        fx: m && pos ? { pos, energy: en, tone: 'construct', constructId: e.construct } : undefined,
      };
    }
    case 'explosion': {
      const en = energy(e.radius ?? 4, R.radius);
      const f = fall(pos);
      return {
        kind: 'explosion', trauma: (S.explosion.base + S.explosion.perEnergy * en) * f,
        kick: pos ? { dir: dirFrom(pos, ctx.playerPos()), mag: (0.08 + 0.25 * en) * f } : undefined,
        fov: 1.5 * f,
        sound: { name: 'explosion', energy: en, pos },
      };
    }
    case 'entity_damage': {
      if (!mine(e.by)) return null;
      const en = energy(e.amount, R.damage);
      return { kind: 'hit', number: { key: `e:${e.id}`, amount: Number(e.amount) || 0 }, marker: 'hit', sound: { name: 'hitConfirm', energy: en }, squash: { entityId: e.id, amount: 0.3 + 0.7 * Math.min(1, en) } };
    }
    case 'construct_damage': {
      const en = energy(e.amount, R.damage);
      const byMe = mine(e.by);
      const myConstruct = ctx.ownerOf(e.id) === local;
      if (!byMe && !myConstruct) return { kind: 'construct_hit', flash: { constructId: e.id, amount: JUICE.fx.flashOnHit * (0.5 + 0.5 * Math.min(1, en)) } };
      return {
        kind: byMe ? 'hit' : 'mine_hit',
        flash: { constructId: e.id, amount: JUICE.fx.flashOnHit * (0.5 + 0.5 * Math.min(1, en)) },
        number: byMe ? { key: `c:${e.id}`, amount: Number(e.amount) || 0 } : undefined,
        marker: byMe ? 'hit' : undefined,
        hitstop: byMe ? { keys: [`c:${e.id}`], ms: hsMs(en * 0.7) } : undefined,
        trauma: myConstruct ? 0.04 + 0.12 * en * fall(ctx.posOf(`c:${e.id}`)) : undefined,
        sound: { name: byMe ? 'hitConfirm' : 'impact', energy: en, pos: byMe ? null : ctx.posOf(`c:${e.id}`) },
      };
    }
    case 'player_hit': // PvE: a turret shot / robot punch landed on the local player
    case 'player_damage': {
      const target = e.type === 'player_hit' ? local : e.target;
      const amount = Number(e.amount ?? e.damage) || 0;
      const en = energy(amount, R.damage);
      if (target === local) {
        const src = e.type === 'player_hit' ? ctx.posOf(`e:${e.from}`) : typeof e.by === 'string' ? ctx.posOf(`c:${e.by}`) ?? ctx.posOf(`p:${e.by}`) : null;
        return {
          kind: 'hurt', trauma: S.hitReceived.base + S.hitReceived.perEnergy * en,
          kick: { dir: dirFrom(src, ctx.playerPos()), mag: JUICE.camera.hurtKick * (0.4 + 0.6 * Math.min(1, en)), pitch: 1 + 2 * Math.min(1, en) },
          fov: -1 - 2 * en,
          hurt: e.type === 'player_damage' ? 0.6 + 0.4 * Math.min(1, en) : undefined,
          sound: { name: 'hurt', energy: en },
        };
      }
      if (!mine(e.by)) return null;
      return { kind: 'hit', number: { key: `p:${target}`, amount }, marker: 'hit', hitstop: { keys: [`p:${target}`], ms: hsMs(en) }, sound: { name: 'hitConfirm', energy: en } };
    }
    case 'player_killed':
    case 'kill': {
      const target = e.target ?? e.victim;
      if (target === local) return { kind: 'killed', trauma: S.killed, hurt: 1, fov: -4, sound: { name: 'death', energy: 1 } };
      if (!mine(e.by ?? e.killer)) return null;
      return {
        kind: 'kill', trauma: S.kill, hitstop: { keys: ['*', `p:${target}`], ms: HS.killMs }, marker: 'kill',
        killCam: { key: `p:${target}`, pos: asV3(e.pos) }, sound: { name: 'kill', energy: 1 },
        callout: { text: 'KILL', tone: 'bad', size: 1.25 },
      };
    }
    case 'entity_disabled': {
      if (!mine(e.by)) return null;
      return {
        kind: 'ko', trauma: S.ko * lerp(0.5, 1, fall(pos)), hitstop: { keys: ['*', `e:${e.id}`], ms: HS.koMs }, marker: 'kill',
        sound: { name: 'ko', energy: 0.8, pos }, callout: { text: `${String(e.entityType ?? 'target').toUpperCase()} DOWN`, tone: 'hl', size: 0.8 },
      };
    }
    case 'entity_destroyed': {
      if (!mine(e.by)) return { kind: 'break', sound: { name: 'break', energy: 0.5, pos } };
      return { kind: 'break', trauma: S.entityDestroyed * lerp(0.5, 1, fall(pos)), sound: { name: 'break', energy: 0.8, pos } };
    }
    case 'construct_destroyed': {
      const p = ctx.posOf(`c:${e.id}`);
      if (mine(e.by)) return { kind: 'shatter', trauma: S.constructDestroyed, marker: 'kill', hitstop: { keys: ['*'], ms: HS.koMs }, sound: { name: 'shatter', energy: 1, pos: p }, callout: { text: 'BROKEN', tone: 'hl', size: 0.9, sub: e.label } };
      if (ctx.ownerOf(e.id) === local) return { kind: 'lost', trauma: S.constructDestroyed * 0.6, sound: { name: 'shatter', energy: 0.8, pos: p } };
      return { kind: 'shatter', sound: { name: 'shatter', energy: 0.6, pos: p } };
    }
    case 'blocked': case 'punch_blocked': {
      const cid = e.construct;
      if (!cid) return null;
      const p = pos ?? ctx.posOf(`c:${cid}`);
      const m = ctx.ownerOf(cid) === local;
      return {
        kind: 'block', trauma: m ? S.blocked * lerp(0.5, 1, fall(p)) : 0, hitstop: m ? { keys: [`c:${cid}`], ms: HS.minMs } : undefined,
        flash: { constructId: cid, amount: e.reflected ? 1 : 0.6 }, sound: { name: e.reflected ? 'reflect' : 'block', energy: 0.6, pos: p },
      };
    }
    case 'construct_formed': {
      const m = ctx.ownerOf(e.id) === local;
      return { kind: 'form', fov: m ? JUICE.fov.castPulse : 0, kick: m ? { mag: 0, lift: -0.03 } : undefined, sound: { name: 'form', energy: Math.min(1, (Number(e.cost) || 20) / 60), pos: m ? null : ctx.posOf(`c:${e.id}`) }};
    }
    case 'construct_modified': return { kind: 'reform', sound: { name: 'reform', energy: 0.5, pos: null }};
    case 'construct_rejected': return { kind: 'fail', sound: { name: 'castFail', energy: 0.6 } };
    case 'construct_dismissed': return e.reason === 'destroyed' ? null : { kind: 'dissolve', sound: { name: 'dissolve', energy: 0.5, pos: ctx.posOf(`c:${e.id}`) } };
    case 'construct_fire': {
      const cid = e.construct;
      const kind = String(e.kind ?? 'cannon');
      const m = ctx.ownerOf(cid) === local;
      const p = pos ?? ctx.posOf(`c:${cid}`);
      const rec = JUICE.camera.recoil[kind] ?? JUICE.camera.recoil.default;
      const d = asV3(e.dir);
      return {
        kind: 'fire', trauma: m ? S.recoil * (rec / 0.2) * lerp(0.5, 1, fall(p)) : 0,
        kick: m ? { dir: d ? [-d[0], -d[1], -d[2]] : undefined, mag: rec, pitch: rec * JUICE.camera.recoilRot } : undefined,
        sound: { name: 'weapon', kind, energy: Math.min(1.5, rec / 0.2), pos: p },
      };
    }
    case 'launch': return { kind: 'launch', trauma: S.launch, fov: JUICE.fov.launch, kick: { mag: 0, lift: -0.25 }, sound: { name: 'launch', energy: 0.8 } };
    case 'construct_throw': return { kind: 'throw', sound: { name: 'whoosh', energy: Math.min(1, (Number(e.speed) || 20) / 40), pos: ctx.posOf(`c:${e.id}`) } };
    case 'player_ram': {
      const en = energy(e.speed, 40);
      return { kind: 'ram', trauma: 0.1 + 0.3 * en, sound: { name: 'impact', energy: en } };
    }
    case 'inspiration': {
      if (e.player && e.player !== local) return null;
      const mul = `×${Number(e.mul ?? 1).toFixed(1)}`;
      if (e.stale) return { kind: 'stale', sound: { name: 'stale', energy: 0.5 }, callout: { text: `STALE ${mul}`, tone: 'dim', size: 0.75 } };
      const nov = Number(e.novelty) || 0;
      if (nov < INSPIRED_NOVELTY) return null;
      return { kind: 'inspired', sound: { name: 'chime', energy: nov, novelty: nov }, callout: { text: `INSPIRED ${mul}`, tone: 'hl', size: 0.85 + 0.45 * clamp((nov - INSPIRED_NOVELTY) / (1 - INSPIRED_NOVELTY), 0, 1), sub: e.fresh?.length ? `new: ${e.fresh.slice(0, 3).join(', ')}` : undefined } };
    }
    case 'creative_solution': {
      if (e.player && e.player !== local) return null;
      return { kind: 'creative', sound: { name: 'creative', energy: 1, novelty: 1 }, callout: { text: CREATIVE_TEXT[e.job] ?? 'CREATIVE', tone: 'gold', size: 1.15, sub: e.bonus ? `+${Math.round(e.bonus * 100)}% inspiration` : undefined } };
    }
    case 'match_state': case 'round_over': case 'round_end': case 'match_over': {
      const winner = e.winner ?? e.roundWinner;
      if (!winner || (e.type === 'match_state' && e.state && !/over|end|won|lost|finish/i.test(String(e.state)))) return null;
      const won = winner === local;
      const match = e.type === 'match_over' || /match/i.test(String(e.state ?? ''));
      return { kind: won ? 'win' : 'lose', sound: { name: won ? 'win' : 'lose', energy: 1 }, callout: { text: `${match ? 'MATCH' : 'ROUND'} ${won ? 'WON' : 'LOST'}`, tone: won ? 'gold' : 'bad', size: 1.3 } };
    }
    case 'race_finished': return { kind: 'win', sound: { name: 'win', energy: 1 } };
    case 'goal_reached': case 'civilian_saved': return { kind: 'good', sound: { name: 'chime', energy: 0.6, novelty: 0.6 } };
    case 'machine_jump': return { kind: 'mjump', sound: { name: 'jump', energy: 0.4, pos: ctx.posOf(`c:${e.construct}`) } };
    case 'machine_land': { const en = energy(e.speed, 20); return { kind: 'mland', trauma: 0.1 * en * fall(ctx.posOf(`c:${e.construct}`)), sound: { name: 'land', energy: en, pos: ctx.posOf(`c:${e.construct}`) } }; }
    // adaptive mouse actions on constructs (swing, charge-smash, grab/throw, shield bash, fire, hurl)
    case 'action_start': {
      if (ctx.ownerOf(e.construct) !== local) return null;
      return { kind: 'action_start', marker: 'pulse', sound: { name: 'actionStart', energy: 0.4 }, charge: { constructId: e.construct, level: 0 } };
    }
    case 'action_charge': {
      if (ctx.ownerOf(e.construct) !== local) return null;
      const lv = clamp(Number(e.level) || 0, 0, 1);
      return { kind: 'action_charge', charge: { constructId: e.construct, level: lv } };
    }
    case 'action_release': {
      if (ctx.ownerOf(e.construct) !== local) return null;
      const lv = clamp(Number(e.level) || 0, 0, 1);
      const p = ctx.posOf(`c:${e.construct}`);
      return { kind: 'action_release', charge: { constructId: e.construct, level: null }, fov: 1 + 2.5 * lv, trauma: 0.04 + 0.1 * lv * lv, sound: { name: 'whoosh', energy: 0.4 + 0.8 * lv, pos: p } };
    }
    case 'action_hit': {
      const m = ctx.ownerOf(e.construct) === local;
      const en = clamp(Number(e.energy) || 0, 0, 1.5);
      const tk = typeof e.target === 'string' ? (ctx.keyOf?.(e.target) ?? null) : null;
      const p = pos ?? (tk ? ctx.posOf(tk) : null) ?? ctx.posOf(`c:${e.construct}`);
      const f = fall(p);
      // charged hits land heavier than plain impacts: +40% shake, longer hold
      return {
        kind: 'action_hit', trauma: (S.impact.base + S.impact.perEnergy * 1.4 * en) * (m ? lerp(0.5, 1, f) : f * 0.6),
        hitstop: m ? { keys: [`c:${e.construct}`, ...(tk ? [tk] : [])], ms: hsMs(en * 1.25) } : undefined,
        kick: m && p ? { dir: dirFrom(p, ctx.playerPos()), mag: 0.05 + 0.15 * Math.min(1, en) } : undefined,
        fov: m ? 1.5 * en : 0,
        sound: { name: 'impact', energy: Math.min(1.5, en * 1.2), pos: p },
        fx: m && p ? { pos: p, energy: Math.min(1.5, en * 1.2), tone: 'construct', constructId: e.construct } : undefined,
        squash: tk?.startsWith('e:') ? { entityId: tk.slice(2), amount: 0.5 + 0.5 * Math.min(1, en) } : undefined,
      };
    }
    case 'ride': case 'dismount': return { kind: e.type, sound: { name: 'whoosh', energy: 0.4 } };
  }
  return null;
}

/** Local player movement events (Player.events strings) → effects. */
export function mapPlayerEvent(ev: string, info: { fallSpeed: number; anchor?: V3 | null; playerPos: V3 }): Effect | null {
  const C = JUICE.camera;
  if (ev === 'jump') return { kind: 'jump', kick: { mag: 0, lift: C.jumpLift }, sound: { name: 'jump', energy: 0.5 } };
  if (ev === 'land') {
    const en = energy(info.fallSpeed, JUICE.energyRef.fall);
    return { kind: 'land', kick: { mag: 0, lift: -C.landDip * (0.3 + en) }, sound: { name: 'land', energy: en } };
  }
  if (ev === 'land_hard') {
    const en = Math.max(0.6, energy(info.fallSpeed, JUICE.energyRef.fall));
    return { kind: 'land_hard', trauma: JUICE.shake.landHard * en, kick: { mag: 0, lift: -C.landHardDip * en, pitch: -1.5 * en }, fov: JUICE.fov.landHardDip * en, sound: { name: 'landHard', energy: en } };
  }
  if (ev.startsWith('tether_fire_')) return { kind: 'tether_fire', sound: { name: 'tetherFire', energy: 0.5 } };
  if (ev.startsWith('tether_attach_')) return { kind: 'tether_attach', kick: { dir: dirFrom(info.playerPos, info.anchor ?? null), mag: C.tetherSnap }, fov: JUICE.fov.tetherYank, sound: { name: 'tetherAttach', energy: 0.6 } };
  if (ev.startsWith('tether_release_')) return { kind: 'tether_release', sound: { name: 'tetherRelease', energy: 0.4 } };
  if (ev.startsWith('tether_miss_')) return { kind: 'tether_miss', sound: { name: 'tetherMiss', energy: 0.3 } };
  if (ev === 'fly_on' || ev === 'fly_off') return { kind: ev, sound: { name: ev === 'fly_on' ? 'flyOn' : 'flyOff', energy: 0.5 } };
  return null;
}

/** Speed-threshold FOV punches with hysteresis: returns the index of a threshold crossed upward, or -1. */
export class SpeedThresholds {
  private armed: boolean[];
  constructor(public thresholds = JUICE.fov.speedThresholds, public hysteresis = JUICE.fov.speedHysteresis) { this.armed = thresholds.map(() => true); }
  update(speed: number): number {
    let hit = -1;
    this.thresholds.forEach((t, i) => {
      if (this.armed[i] && speed >= t) { this.armed[i] = false; hit = i; }
      else if (!this.armed[i] && speed < t - this.hysteresis) this.armed[i] = true;
    });
    return hit;
  }
}
