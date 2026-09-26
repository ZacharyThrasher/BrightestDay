/**
 * Creativity economy — "a Lantern is exactly as strong as their imagination". Per player.
 *
 * Every cast (a construct created or changed by a typed command) gets a NOVELTY score from its
 * signature: the ideas it is made of (what it is, extra parts, adjectives, materials, size, count and
 * arrangement, behaviours, joint motions, placement, what it acts on, the verbs used) and, weighted
 * most, the PAIRS of those ideas ("tank" is ordinary; "tank + wings" is a new combination).
 * Features the player used recently are worth little; features they have never used are worth extra.
 *
 * Novel casts raise that player's INSPIRATION meter; repeating yourself (and idle time) lowers it.
 * Inspiration multiplies power: willpower max and regen, construct strength (speeds / forces / damage,
 * lifetime) and energy cost. A stale cast costs more and is weaker, and the player is told why. In a duel,
 * hurting / taking down the rival, breaking their constructs or blocking their shots with something
 * unusual for the job (a flying piano rather than a cannon) pays a bonus to the construct's owner.
 *
 * The validator stays the authority: power scaling happens AFTER validation and never exceeds LIMITS.
 * Everything here is deterministic given the sequence of casts and world time (no wall clock, no RNG),
 * so a host can run it authoritatively for every player.
 *
 * Integration surface (kept tiny on purpose):
 *   world.inspiration     → CreativityEconomy; `.of(playerId)` is one player's Inspiration
 *   runtime.spawn/modify  → price() before validation, boost.shape(spec) after, boost.done(c) at the end
 *   runtime energy        → energyMax / regenMul (the local player's until the runtime has per-player pools:
 *                           then use `.of(owner).energyMax / .regenMul`)
 *   Director.commit       → beginCast(verbs, playerId) / endCast()   (casts outside a Director commit are neutral)
 *   world events          → onEvent() attributes outcomes to constructs → owners (see CreativityEconomy)
 * Emits world events:
 *   'inspiration'       { player, construct, label, novelty, mul, stale, reasons[], fresh[], level, gain, combo, costMul, streak }
 *   'creative_solution' { player, against, construct, label, job, unusual, bonus, typical, reason }
 */
import type { ConstructSpec, Ref, Behavior } from './constructs/spec.js';
import { LIMITS } from './constructs/validate.js';
import { FORMS, buildForm } from './constructs/forms.js';
import type { World, GameEvent } from './world.js';
import type { Construct } from './constructs/runtime.js';

/** Tuning (see docs/EXPERIMENTS.md §8 for how these were chosen and what was measured). */
export const INSPIRATION = {
  start: 0.4, // meter at the start of a session (0..1)
  rest: 0.3, // idle time relaxes the meter down toward this
  restTau: 75, // seconds (sim time) for the relaxation
  neutral: 0.3, // novelty at which a cast neither raises nor lowers the meter
  gain: 0.3, // meter change per unit of (novelty - neutral)
  staleBelow: 0.18, // novelty under this = stale cast (a 2nd identical cast scores ~0.2: quiet; the 3rd is stale)
  castDecay: 0.85, // every cast, older usage counts shrink by this
  timeTau: 120, // seconds (sim time): usage fades with time too
  freshK: 1.2, // freshness of a feature = exp(-freshK * recentUses)
  firstTime: 1.3, // never-used-before features are worth this much more
  mulMin: 0.5,
  mulMax: 2,
  costMulMin: 0.5,
  costMulMax: 1.6,
  comboStep: 0.15, // positive gains ×(1 + step·(combo-1))
  comboMax: 1.6,
  // creative-solution bonuses (meter) per job; ×unusualness (typical tool ×0.35, halves per reuse)
  creative: { kill: 0.15, hit: 0.04, counter: 0.08, block: 0.05, defeat: 0.12, rescue: 0.15, goal: 0.08, energy: 12, typical: 0.35, window: 8 },
};

// ------------------------------------------------------------------------------------------ features

export type FeatureKind = 'n' | 'c' | 'x' | 'a' | 'm' | 's' | 'k' | 'b' | 'j' | 'p' | 't' | 'v' | 'pair';

export interface Feature { key: string; kind: FeatureKind; name: string; weight: number }

/** Kinds that combine into pairs (the "ideas"); size/placement/joints/concept ids are modifiers only. */
const PAIRED = new Set<FeatureKind>(['n', 'x', 'a', 'm', 'k', 'b', 't', 'v']);
const WEIGHT: Record<FeatureKind, number> = { n: 1.5, c: 0.25, x: 1.2, a: 0.6, m: 0.5, s: 0.35, k: 0.45, b: 0.7, j: 0.4, p: 0.2, t: 0.5, v: 0.5, pair: 0 };
/** Total weight of all pairs of a cast saturates at this (combinations matter most, but big specs can't run away). */
const PAIR_WEIGHT = 4;
/** Go-to constructs everybody reaches for first: they count, but less. */
const COMMON_NOUNS = new Set(['wall', 'shield', 'platform', 'ramp', 'bridge', 'trampoline', 'dome', 'bubble', 'fist', 'hand', 'hammer', 'block', 'box', 'cube', 'ball', 'rope', 'stairs', 'ladder', 'cannon', 'gun', 'sword']);
const LABEL_STOP = new Set(['a', 'an', 'the', 'of', 'and', 'with', 'on', 'in', 'to', 'at', 'my', 'some', 'that', 'this', 'x', '×']);
const GENERIC_ROLES = new Set(['body', 'base', 'collider', 'mesh', 'deck', 'pad', 'handle', 'spine', 'flame']);
/** Plain creation/editing verbs are not ideas. */
const PLAIN_VERBS = new Set(['create', 'modify', 'add', 'remove', 'save', 'recall', 'duplicate', 'undo', 'dismiss', 'stop', 'handle']);

/** Words of a construct label: "3 × giant spiky wrecking ball with rockets" → head "ball", adjectives [giant, spiky, wrecking]. */
export function labelWords(label: string): { head: string | null; adjs: string[] } {
  const core = label.toLowerCase().replace(/^\s*\d+\s*[×x]\s*/, '').split(/\s+with\s+/)[0];
  const words = core.split(/[^a-z'-]+/).filter((w) => w && !LABEL_STOP.has(w));
  if (!words.length) return { head: null, adjs: [] };
  return { head: singular(words[words.length - 1]), adjs: words.slice(0, -1) };
}

function singular(w: string) { return w.length > 3 && w.endsWith('s') && !w.endsWith('ss') ? w.slice(0, -1) : w; }

let formSeqs: string[][] | null = null;
const roleOf = (p: { role?: string }) => (p.role ?? 'body').toLowerCase();
/**
 * Roles that belong to the construct's base shape. Add-on parts are appended after the base form's
 * parts, so the longest built-in form whose role sequence is a prefix of the spec's parts is the base;
 * whatever follows was grafted on (wings, rockets, blades…). Unknown base shape: nothing counts as extra.
 */
function baseRoles(parts: { role?: string }[]): Set<string> {
  formSeqs ??= Object.keys(FORMS).map((id) => buildForm(id).map(roleOf));
  const seq = parts.map(roleOf);
  let best = 0;
  for (const f of formSeqs) if (f.length > best && f.length <= seq.length && f.every((r, i) => seq[i] === r)) best = f.length;
  return new Set(best ? seq.slice(0, best) : seq);
}

function behaviourKey(b: Behavior): string {
  const a = b as any;
  switch (b.type) {
    case 'field': return `field:${a.mode}`;
    case 'launch': return `launch:${a.dir}`;
    case 'mount': return `mount:${a.mode}`;
    case 'weapon': return `weapon:${a.kind}`;
    case 'brain': return `brain:${a.goal}`;
    case 'drive': return `drive:${a.mode}`;
    default: return b.type;
  }
}

const BEH_NAME: Record<string, string> = {
  'launch:normal': 'bounce', 'launch:up': 'launch', 'launch:forward': 'launch', 'launch:arc': 'catapult', 'mount:ride': 'riding', 'mount:wear': 'wearing',
  'field:attract': 'pull field', 'field:repel': 'push field', 'field:wind': 'wind', 'field:lift': 'levitation', shoot: 'shooting', strike: 'strike',
  thrust: 'thrust', spin: 'spin', crush: 'crush', grab: 'grab', restrain: 'restrain', explode: 'explosion', home: 'homing', reflect: 'reflection',
  conveyor: 'conveyor', cushion: 'cushion', tether: 'tether', sweep: 'sweep', orbit: 'orbit', follow: 'follow', lift: 'lift',
};

function refTargets(b: Behavior): Ref[] {
  const a = b as any;
  return [a.target, a.around].filter((r): r is Ref => !!r && typeof r === 'object' && 'kind' in r);
}

export interface SignatureContext {
  /** Entity type for an entity id ("drone", "turret", …). */
  entityType?: (id: string) => string | null;
  /** What the crosshair is on right now (resolves 'aim' refs). */
  aimType?: string | null;
  /** Verb classes of the typed command (strike, grab, throw…). */
  verbs?: string[];
}

/** The novelty signature of a construct spec. Deterministic; order-independent keys. */
export function signature(spec: ConstructSpec, ctx: SignatureContext = {}): Feature[] {
  const out = new Map<string, Feature>();
  const add = (kind: FeatureKind, id: string, name = id, weight = WEIGHT[kind]) => {
    const key = `${kind}:${id}`;
    if (!out.has(key)) out.set(key, { key, kind, name, weight });
  };
  const { head, adjs } = labelWords(spec.label ?? '');
  const concepts = String(spec.concept ?? '').toLowerCase().split('+').map((s) => s.trim()).filter(Boolean);
  const noun = head ?? concepts[0] ?? 'thing';
  add('n', noun, noun, WEIGHT.n * (COMMON_NOUNS.has(noun) ? 0.7 : 1));
  for (const c of concepts) if (c !== noun) add('c', c);
  // extra parts: roles the base shape doesn't have (add-ons, grafted parts)
  const base = baseRoles(spec.parts ?? []);
  for (const p of spec.parts ?? []) {
    const r = roleOf(p);
    if (!GENERIC_ROLES.has(r) && !base.has(r) && r !== noun) add('x', singular(r));
  }
  // materials (from physics, so they count however they were phrased)
  if (spec.soft) add('m', 'soft');
  if ((spec.restitution ?? 0) >= 0.7) add('m', 'bouncy');
  if ((spec.friction ?? 0.6) <= 0.1) add('m', 'slippery');
  if ((spec.density ?? 300) >= 2000) add('m', 'heavy');
  if ((spec.density ?? 300) <= 60) add('m', 'light');
  // size
  const sc = spec.scale ?? 1;
  const size = sc < 0.5 ? 'tiny' : sc < 0.85 ? 'small' : sc <= 1.6 ? null : sc <= 4 ? 'big' : sc <= 9 ? 'huge' : 'colossal';
  if (size) add('s', size);
  const st = spec.stretch;
  if (st && Math.max(...st) / Math.max(0.01, Math.min(...st)) > 1.8) add('s', 'stretched');
  // count / arrangement
  const n = spec.count ?? 1;
  if (n > 1) add('k', n >= 5 ? 'many' : 'few', n >= 5 ? 'a swarm' : `${n}×`);
  if (spec.arrangement && spec.arrangement !== 'single' && spec.arrangement !== 'line') add('k', spec.arrangement);
  // behaviours and what they act on
  for (const b of spec.behaviors ?? []) {
    if (b.type === 'hold') continue; // "in my hand" is a placement, not an idea
    const bk = behaviourKey(b);
    add('b', bk, BEH_NAME[bk] ?? bk.replace(':', ' '));
    for (const r of refTargets(b)) {
      const t = refType(r, ctx);
      if (t) add('t', t);
    }
  }
  for (const j of spec.joints ?? []) for (const m of j.motion ?? []) add('j', m.kind);
  // label adjectives the physics didn't already capture ("giant", "wrecking", "boxing")
  const named = new Set([...out.values()].map((f) => f.name));
  for (const a of adjs) if (!named.has(a) && !named.has(singular(a))) add('a', a);
  if (spec.placement?.at) add('p', spec.placement.at);
  if (spec.placement?.target) { const t = refType(spec.placement.target, ctx); if (t) add('t', t); }
  for (const v of ctx.verbs ?? []) if (!PLAIN_VERBS.has(v)) add('v', v);
  // pairs: every combination of two ideas, weighted as a block
  const singles = [...out.values()].filter((f) => PAIRED.has(f.kind));
  const pairs: Feature[] = [];
  for (let i = 0; i < singles.length; i++) {
    for (let k = i + 1; k < singles.length; k++) {
      const [a, b] = [singles[i], singles[k]].sort((x, y) => (x.key < y.key ? -1 : 1));
      if (a.kind === 't' && b.kind === 'v') continue; // "strike the drone" is one thought, not a combination
      if (a.name === b.name) continue; // a spinning part + spin behaviour
      pairs.push({ key: `pair:${a.key}|${b.key}`, kind: 'pair', name: `${a.name} + ${b.name}`, weight: 0 });
    }
  }
  if (pairs.length) {
    const total = PAIR_WEIGHT * (1 - Math.exp(-pairs.length / 4));
    for (const p of pairs) { p.weight = total / pairs.length; out.set(p.key, p); }
  }
  return [...out.values()];
}

function refType(r: Ref, ctx: SignatureContext): string | null {
  switch (r.kind) {
    case 'entity': return ctx.entityType?.(r.id) ?? 'thing';
    case 'entities': return (r.ids[0] && ctx.entityType?.(r.ids[0])) || 'group';
    case 'aim': return ctx.aimType ?? null;
    case 'self': return 'me';
    case 'nearest_enemy': case 'threat': return 'enemy';
    case 'all_enemies': return 'all enemies';
    case 'construct': return 'construct';
    default: return (r as { kind: string }).kind === 'player' ? 'opponent' : null;
  }
}

// ------------------------------------------------------------------------------------------ scoring

export interface NoveltyScore {
  novelty: number; // 0..1
  ratio: number; // share of the idea's weight that is fresh
  richness: number; // 0..1: how many ideas went into it
  fresh: Feature[]; // most novel features first
}

/** Pure scoring against recent usage counts and the set of features ever used. */
export function scoreNovelty(features: Feature[], usage: ReadonlyMap<string, number>, ever: ReadonlySet<string>): NoveltyScore {
  let max = 0;
  let got = 0;
  const contrib: { f: Feature; v: number }[] = [];
  for (const f of features) {
    const u = usage.get(f.key) ?? 0;
    const fresh = Math.exp(-INSPIRATION.freshK * u) * (ever.has(f.key) ? 1 : INSPIRATION.firstTime);
    max += f.weight;
    got += f.weight * fresh;
    if (fresh > 0.55) contrib.push({ f, v: f.weight * fresh });
  }
  if (max <= 0) return { novelty: 0, ratio: 0, richness: 0, fresh: [] };
  const ratio = Math.min(1, got / max);
  const richness = 1 - Math.exp(-max / 5);
  const novelty = ratio * (0.55 + 0.45 * richness);
  contrib.sort((a, b) => b.v - a.v || (a.f.key < b.f.key ? -1 : 1));
  return { novelty, ratio, richness, fresh: contrib.map((c) => c.f) };
}

const SHOWN = new Set<FeatureKind>(['n', 'x', 'a', 'm', 's', 'k', 'b', 't', 'v']);
/**
 * What to tell the player was new: the most novel idea, shown as a combination with the construct
 * when that pairing is new ("wing + tank"), then up to two more fresh ideas.
 */
export function describeFresh(fresh: Feature[], noun: string): string[] {
  const singles = fresh.filter((f) => SHOWN.has(f.kind));
  const out: string[] = [];
  const top = singles[0];
  if (!top) return fresh.filter((f) => f.kind === 'pair').slice(0, 2).map((f) => f.name);
  const combo = top.kind !== 'n' && fresh.some((f) => f.kind === 'pair' && f.key.includes(`n:${noun}`) && f.key.includes(top.key));
  out.push(combo ? `${top.name} + ${noun}` : top.name);
  for (const f of singles.slice(1)) {
    if (out.length >= 3) break;
    if (!out.some((o) => o.split(' + ').includes(f.name))) out.push(f.name);
  }
  return out;
}

/** Casts at least this novel are announced as inspired. */
export const INSPIRED_AT = 0.4;

export function levelMul(level: number) { return 0.8 + 0.8 * clamp01(level); }

/** Power multiplier of one cast given the meter (after the cast's own gain) and its novelty. */
export function castMul(level: number, novelty: number): { mul: number; stale: boolean } {
  const T = INSPIRATION;
  const stale = novelty < T.staleBelow;
  // a stale cast never gets the meter's bonus: it always reads (and is) weaker than ×1
  const f = stale ? 0.7 + 0.15 * (novelty / T.staleBelow) : 1 + 0.25 * (novelty - T.neutral);
  const lm = stale ? Math.min(1, levelMul(level)) : levelMul(level);
  return { mul: clamp(lm * f, T.mulMin, T.mulMax), stale };
}

export function comboMul(combo: number) { return Math.min(INSPIRATION.comboMax, 1 + INSPIRATION.comboStep * Math.max(0, combo - 1)); }

// ------------------------------------------------------------------------------------------ power

/** [behaviour type, field, LIMITS key]. A field whose cap is unknown may only be scaled DOWN. */
const POWER_FIELDS: [string, string, string][] = [
  ['strike', 'speed', 'strikeSpeed'], ['throw', 'speed', 'throwSpeed'], ['home', 'speed', 'homeSpeed'],
  ['shoot', 'speed', 'shootSpeed'], ['field', 'strength', 'fieldStrength'], ['launch', 'speed', 'launchSpeed'],
  ['thrust', 'accel', 'thrust'], ['explode', 'force', 'explodeForce'],
  ['weapon', 'damage', 'weaponDamage'], ['weapon', 'speed', 'weaponSpeed'], ['drive', 'speed', 'driveSpeed'],
];

/**
 * Scale a VALIDATED spec's strength by `mul`, never past the validator's limits. A spec edited from
 * an already-scaled construct carries `_insp` (its previous multiplier), which is undone first so
 * repeated edits don't compound.
 */
export function scaleWithinLimits(spec: ConstructSpec, mul: number) {
  const prev = Number((spec as any)._insp) || 1;
  const L = LIMITS as unknown as Record<string, number | undefined>;
  for (const b of spec.behaviors) {
    for (const [type, field, capKey] of POWER_FIELDS) {
      if (b.type !== type) continue;
      const v = (b as any)[field];
      if (typeof v !== 'number' || !isFinite(v) || v <= 0) continue;
      const cap = L[capKey];
      // known cap: up to the validator's limit; unknown cap: never above the unscaled value
      if (typeof cap === 'number') (b as any)[field] = Math.min((v / prev) * mul, Math.max(cap, 0));
      else (b as any)[field] = (v / Math.min(prev, 1)) * Math.min(mul, 1);
    }
  }
  const life = (m: number) => Math.min(m, 1.5);
  spec.lifetime = clamp((spec.lifetime / life(prev)) * life(mul), 1, LIMITS.lifetime);
  (spec as any)._insp = Math.round(mul * 1000) / 1000;
}

// ------------------------------------------------------------------------------------------ runtime hook

export interface CastBoost {
  /** Multiply the validator's cost by this (and divide the available energy by it before validating). */
  costMul: number;
  /** Scale the validated spec's strength (within LIMITS). */
  shape(spec: ConstructSpec): void;
  /** The construct exists: commit the meter change and emit the 'inspiration' event. */
  done(c: Construct): void;
}

/** Player id used when nothing else says who is casting (single-player, tests, the eval harness). */
export const LOCAL_PLAYER = 'p1';

// ------------------------------------------------------------------------------------------ one player's meter

export interface PlayerSnapshot { level: number; usage: [string, number][]; ever: string[]; jobUses: [string, number][] }

/** One lantern's imagination: their meter, what they have used recently / ever, their streaks. */
export class Inspiration {
  /** The meter, 0..1. */
  level = INSPIRATION.start;
  /** Typing combo (set by whoever feeds this player's commands); 1 = none. Only amplifies positive gains. */
  combo = 1;
  /** Last cast's result (for the HUD). */
  last: { novelty: number; mul: number; stale: boolean; t: number } | null = null;
  stats = { casts: 0, inspired: 0, stale: 0, creative: 0 };
  usage = new Map<string, number>();
  ever = new Set<string>();
  jobUses = new Map<string, number>();
  streak = { noun: '', n: 0 };
  private decayedAt = 0;

  constructor(readonly id: string, private w: World) {}

  /** Willpower cap: 85 (uninspired) … 125 (fully inspired); 101 at the starting level. */
  get energyMax() { return LIMITS.energyMax * (0.85 + 0.4 * this.level); }
  /** Willpower regeneration multiplier: 0.7× … 1.6×. */
  get regenMul() { return 0.7 + 0.9 * this.level; }
  /** Multiplier a perfectly ordinary cast would get right now. */
  get mul() { return levelMul(this.level); }

  /** Price one construct this player creates / changes. */
  price(kind: 'create' | 'modify', proposal: ConstructSpec, prev: ConstructSpec | null, ctx: SignatureContext, remember: (c: Construct, owner: string) => void): CastBoost {
    const neutral: CastBoost = { costMul: 1, shape: () => {}, done: (c) => remember(c, this.id) };
    this.decay();
    const feats = signature(proposal, ctx);
    if (kind === 'modify' && prev) {
      const before = new Set(signature(prev, ctx).map((f) => f.key));
      // an edit that adds no new idea (undo, "smaller", "stop") is neither creative nor stale
      if (feats.every((f) => before.has(f.key))) return neutral;
    }
    const sc = scoreNovelty(feats, this.usage, this.ever);
    const T = INSPIRATION;
    let gain = (sc.novelty - T.neutral) * T.gain;
    if (gain > 0) gain *= comboMul(this.combo);
    const level = clamp01(this.level + gain);
    const { mul, stale } = castMul(level, sc.novelty);
    const costMul = clamp(1 / mul, T.costMulMin, T.costMulMax);
    const noun = feats.find((f) => f.kind === 'n')?.name ?? 'thing';
    return {
      costMul,
      shape: (spec) => scaleWithinLimits(spec, mul),
      done: (c) => {
        this.level = level;
        for (const [k, v] of this.usage) this.usage.set(k, v * T.castDecay);
        for (const f of feats) { this.usage.set(f.key, (this.usage.get(f.key) ?? 0) + 1); this.ever.add(f.key); }
        if (this.usage.size > 600) for (const [k, v] of [...this.usage]) if (v < 0.05) this.usage.delete(k);
        this.streak = this.streak.noun === noun ? { noun, n: this.streak.n + 1 } : { noun, n: 1 };
        this.stats.casts++;
        if (stale) this.stats.stale++;
        else if (sc.novelty >= INSPIRED_AT) this.stats.inspired++;
        this.last = { novelty: sc.novelty, mul, stale, t: this.w.time };
        remember(c, this.id);
        const fresh = describeFresh(sc.fresh, noun);
        const reasons: string[] = [];
        if (stale) reasons.push(this.streak.n >= 2 ? `you've made ${this.streak.n} ${plural(noun)} in a row` : `nothing new — ${this.mostUsed(feats)} again`);
        else if (fresh.length) reasons.push(`new: ${fresh.join(', ')}`);
        if (this.combo > 1 && gain > 0) reasons.push(`combo ×${this.combo}`);
        this.w.emit('inspiration', {
          player: this.id, construct: c.id, label: c.spec.label, kind, novelty: r2(sc.novelty), mul: r2(mul), stale, reasons, fresh,
          level: r2(this.level), gain: r2(gain), combo: this.combo, costMul: r2(costMul), streak: this.streak.n,
        });
      },
    };
  }

  /** Idle time relaxes the meter toward rest. */
  tick(dt: number) { if (this.level > INSPIRATION.rest) this.level -= (this.level - INSPIRATION.rest) * (dt / INSPIRATION.restTau); }

  private mostUsed(feats: Feature[]): string {
    let best: Feature | null = null;
    let bu = -1;
    for (const f of feats) { const u = (this.usage.get(f.key) ?? 0) * f.weight; if (f.kind !== 'pair' && u > bu) { bu = u; best = f; } }
    return best?.name ?? 'the same idea';
  }

  private decay() {
    const dt = this.w.time - this.decayedAt;
    if (dt <= 0) return;
    this.decayedAt = this.w.time;
    const k = Math.exp(-dt / INSPIRATION.timeTau);
    for (const [key, v] of this.usage) this.usage.set(key, v * k);
  }

  snapshot(): PlayerSnapshot { return { level: this.level, usage: [...this.usage], ever: [...this.ever], jobUses: [...this.jobUses] }; }
  restore(s: PlayerSnapshot) { this.level = clamp01(s.level); this.usage = new Map(s.usage); this.ever = new Set(s.ever); this.jobUses = new Map(s.jobUses); }
}

// ------------------------------------------------------------------------------------------ creative solutions

/** What a construct was, for judging how unusual it is for a job. */
interface Remembered { id: string; owner: string; label: string; noun: string; behaviours: Set<string>; born: number }

export type Job = 'kill' | 'hit' | 'counter' | 'block' | 'defeat' | 'rescue' | 'goal';

const WEAPON_NOUNS = new Set(['gun', 'cannon', 'missile', 'rocket', 'bomb', 'sword', 'fist', 'glove', 'hammer', 'bat', 'spear', 'axe', 'laser', 'turret', 'tank', 'blaster', 'mace', 'club', 'dagger', 'knife', 'arrow', 'bow', 'crossbow', 'grenade', 'mallet', 'punch', 'foot', 'boot', 'whip', 'flail', 'katana', 'lance', 'pusher']);
const BARRIER_NOUNS = new Set(['wall', 'shield', 'dome', 'bubble', 'panel', 'mirror', 'barrier', 'door', 'armor', 'umbrella', 'cage', 'box', 'block', 'fence', 'slab']);
const RESCUE_NOUNS = new Set(['trampoline', 'net', 'cushion', 'mattress', 'bubble', 'platform', 'hand', 'claw', 'pillow', 'airbag', 'slide', 'bed', 'parachute', 'basket', 'bucket', 'catcher']);
const GOAL_NOUNS = new Set(['ramp', 'bridge', 'stairs', 'staircase', 'ladder', 'platform', 'elevator', 'spring', 'catapult', 'rope', 'zipline', 'pad', 'wing', 'jetpack', 'trampoline', 'launcher', 'pillar', 'tower', 'step', 'grappling', 'hook']);
const ATTACK = { nouns: WEAPON_NOUNS, beh: new Set(['shoot', 'weapon', 'home', 'explode']) };
const JOB_TYPICAL: Record<Job, { nouns: Set<string>; beh: Set<string> }> = {
  kill: ATTACK, hit: ATTACK, counter: ATTACK, defeat: ATTACK,
  block: { nouns: BARRIER_NOUNS, beh: new Set(['reflect']) },
  rescue: { nouns: RESCUE_NOUNS, beh: new Set(['cushion']) },
  goal: { nouns: GOAL_NOUNS, beh: new Set(['launch', 'tether', 'lift']) },
};
const JOB_TEXT: Record<Job, string> = { kill: 'taken down', hit: 'hit', counter: 'broken', block: 'stopped', defeat: 'downed', rescue: 'saved', goal: 'reached' };

// ------------------------------------------------------------------------------------------ the economy (all players)

export interface InspirationSnapshot { players: [string, PlayerSnapshot][] }

/**
 * Every lantern's inspiration, keyed by player id, plus attribution of outcomes to the constructs (and so
 * the players) that caused them. One per World; the runtime and the Director talk to this.
 *
 * Outcome events it understands (emitted by whoever implements them; unknown ones are ignored):
 *   PvP   'player_damage' {target, amount, cause, by?}   'player_killed' {target, by?}
 *         'construct_destroyed' {id, by?}               'blocked' {construct, reflected, from?}
 *         (`by` = construct id or player id; without it the last construct that touched the target counts)
 *   PvE   'entity_disabled' {id, cause}  'civilian_safe' {id}  'goal_reached' {label}
 *   touch 'impact' / 'grabbed' / 'launch_entity' / 'construct_hit' {construct, entity|player|target}
 *         'construct_fire' / 'explosion' / 'launch' {construct}
 */
export class CreativityEconomy {
  readonly players = new Map<string, Inspiration>();
  private casters: { player: string; verbs: string[] }[] = [];
  private remembered = new Map<string, Remembered>();
  private touch = new Map<string, { construct: string; t: number }>();
  private shooter = new Map<string, { construct: string; t: number }>(); // per owner
  private blast: { construct: string; t: number } | null = null;
  private launch: { construct: string; t: number } | null = null;
  private rewardedPairs = new Set<string>();
  private pending: GameEvent[] = [];

  constructor(private w: World) {
    w.listeners.push((e) => this.onEvent(e));
    w.tickHooks.push(() => this.afterTick());
  }

  /** A player's meter (created on first use). */
  of(player: string = LOCAL_PLAYER): Inspiration {
    let p = this.players.get(player);
    if (!p) { p = new Inspiration(player, this.w); this.players.set(player, p); }
    return p;
  }

  /** Label of a construct the economy remembers (still known after it dissolved: kill feed). */
  labelOf(constructId: string): string | null { return this.remembered.get(constructId)?.label ?? null; }

  /** Owner of a construct as far as the economy knows (the player whose cast made it). */
  ownerOf(constructId: string): string | null {
    const c = this.w.constructs?.get(constructId) as (Construct & { owner?: string }) | null;
    return c?.owner ?? this.remembered.get(constructId)?.owner ?? null;
  }

  // ---- compatibility for the single-pool runtime: the local player's numbers
  get level() { return this.of().level; }
  get mul() { return this.of().mul; }
  get energyMax() { return this.of().energyMax; }
  get regenMul() { return this.of().regenMul; }

  // ---------------------------------------------------------------- casts (called by the Director)

  beginCast(verbs: string[] = [], player: string = LOCAL_PLAYER) { this.casters.push({ player, verbs: verbs.slice(0, 4) }); }
  endCast() { this.casters.pop(); }
  get casting() { return this.casters.length > 0; }
  /** Who is casting right now (null outside a Director commit). */
  get caster() { return this.casters[this.casters.length - 1]?.player ?? null; }

  /** Runtime hook: price a proposal before validation. `owner` overrides the current caster. */
  price(kind: 'create' | 'modify', proposal: ConstructSpec, prev: ConstructSpec | null, owner?: string): CastBoost {
    const cur = this.casters[this.casters.length - 1];
    const who = owner ?? cur?.player ?? LOCAL_PLAYER;
    const remember = (c: Construct, o: string) => this.remember(c, o);
    if (!cur) return { costMul: 1, shape: () => {}, done: (c) => remember(c, who) };
    return this.of(who).price(kind, proposal, prev, this.sigContext(cur.verbs), remember);
  }

  private sigContext(verbs: string[]): SignatureContext {
    const P = this.w.playerById(this.caster) ?? this.w.player;
    let aimType: string | null = null;
    try {
      const own = this.w.ownerOf(P.aim(300).collider) as any;
      aimType = own?.kind === 'entity' ? this.w.entities.get(own.id)?.type ?? null : own?.kind === 'player' && own.id ? 'opponent' : null;
    } catch { /* no physics yet */ }
    return { entityType: (id) => this.w.entities.get(id)?.type ?? null, aimType, verbs };
  }

  private remember(c: Construct, owner: string) {
    const { head } = labelWords(c.spec.label ?? '');
    this.remembered.set(c.id, { id: c.id, owner: (c as any).owner ?? owner, label: c.spec.label, noun: head ?? c.spec.concept, behaviours: new Set(c.spec.behaviors.map((b) => b.type)), born: this.w.time });
    if (this.remembered.size > 128) this.remembered.delete(this.remembered.keys().next().value!);
  }

  // ---------------------------------------------------------------- outcomes → creative solutions

  onEvent(e: GameEvent) {
    const now = this.w.time;
    switch (e.type) {
      case 'impact': case 'grabbed': case 'launch_entity': case 'construct_hit': {
        const target = e.entity ?? e.player ?? e.target;
        if (target && e.construct) this.touch.set(target, { construct: e.construct, t: now });
        break;
      }
      case 'construct_fire': this.shooter.set(this.ownerOf(e.construct) ?? '?', { construct: e.construct, t: now }); break;
      case 'explosion': this.blast = { construct: e.construct, t: now }; break;
      case 'launch': if (e.construct) this.launch = { construct: e.construct, t: now }; break;
      case 'blocked':
        // a reflected shot now belongs to whoever reflected it
        if (e.reflected) this.shooter.set(this.ownerOf(e.construct) ?? '?', { construct: e.construct, t: now });
        this.pending.push(e);
        break;
      case 'player_damage': case 'player_killed': case 'construct_destroyed':
      case 'entity_disabled': case 'civilian_safe': case 'goal_reached':
        this.pending.push(e);
        break;
    }
  }

  /** Resolve outcomes at the end of the tick (the construct that caused them may be reported later in the same tick). */
  private afterTick() {
    for (const p of this.players.values()) p.tick(this.w.dt);
    if (!this.pending.length) return;
    const evs = this.pending;
    this.pending = [];
    const W = INSPIRATION.creative.window;
    const now = this.w.time;
    const recent = (x: { construct: string; t: number } | null | undefined, win = W) => (x && now - x.t <= win ? x.construct : null);
    const asConstruct = (by: unknown) => (typeof by === 'string' && this.remembered.has(by) ? by : null);
    /** Most recent construct shot (or reflected shot) by anyone other than the target, within 4 s. */
    const lastShot = (target: string) => {
      let best: { construct: string; t: number } | null = null;
      for (const [owner, s] of this.shooter) if (owner !== target && now - s.t <= 4 && (!best || s.t > best.t)) best = s;
      return best?.construct ?? null;
    };
    const byCause = (target: string, cause: string | undefined, by: unknown) =>
      asConstruct(by)
      ?? (cause === 'explosion' ? recent(this.blast, 0.5) : null)
      ?? (cause === 'projectile' ? lastShot(target) : null)
      ?? (cause === 'ram' ? null : recent(this.touch.get(target)));
    for (const e of evs) {
      switch (e.type) {
        case 'player_killed': { const c = byCause(e.target, e.cause, e.by); if (c) this.award(c, 'kill', 'rival', e.target); break; }
        case 'player_damage': {
          const c = byCause(e.target, e.cause, e.by);
          // one "hit" reward per construct per opponent (kills are where the real bonus is)
          if (c && !this.rewardedPairs.has(`${c}>${e.target}`)) { this.rewardedPairs.add(`${c}>${e.target}`); this.award(c, 'hit', 'rival', e.target); }
          break;
        }
        case 'construct_destroyed': {
          const victim = this.remembered.get(e.id);
          const c = asConstruct(e.by) ?? recent(this.touch.get(e.id));
          if (c && victim) this.award(c, 'counter', `their ${victim.noun}`, victim.owner);
          break;
        }
        case 'blocked': {
          // only blocking an opponent's shot counts, once per blocking construct
          const shooterOwner = typeof e.from === 'string' ? this.ownerOf(e.from) ?? (this.players.has(e.from) ? e.from : null) : null;
          if (shooterOwner && !this.rewardedPairs.has(`block:${e.construct}`)) { this.rewardedPairs.add(`block:${e.construct}`); this.award(e.construct, 'block', 'their shot', shooterOwner); }
          break;
        }
        case 'entity_disabled': {
          const ent = this.w.entities.get(e.id);
          if (ent && !ent.hostile) break;
          const c = byCause(e.id, e.cause, e.by);
          if (c) this.award(c, 'defeat', e.entityType ?? 'enemy', null);
          break;
        }
        case 'civilian_safe': { const c = recent(this.touch.get(e.id), 10) ?? this.newest(12); if (c) this.award(c, 'rescue', 'civilian', null); break; }
        case 'goal_reached': { const c = recent(this.launch) ?? this.tetheredTo() ?? this.newest(20, 25); if (c) this.award(c, 'goal', e.label ?? 'goal', null); break; }
      }
    }
    if (this.rewardedPairs.size > 512) this.rewardedPairs.clear();
  }

  private tetheredTo(): string | null {
    const t = this.w.player.tethers;
    return t.L?.constructId ?? t.R?.constructId ?? null;
  }

  /** Most recently created construct still alive within `age` seconds (and `near` metres of the player). */
  private newest(age: number, near = Infinity): string | null {
    let best: Construct | null = null;
    for (const c of this.w.constructs.list.values()) {
      if (this.w.time - c.born > age) continue;
      if (near < Infinity && (!c.insts[0] || c.insts[0].pos.distanceTo(this.w.player.pos) > near)) continue;
      if (!best || c.born >= best.born) best = c;
    }
    return best?.id ?? null;
  }

  /** Pay the construct's owner for solving `job` — more for an unusual tool, less each time they reuse it. */
  private award(constructId: string, job: Job, what: string, against: string | null) {
    const r = this.remembered.get(constructId);
    if (!r) return;
    if (against && against === r.owner) return; // hurting / breaking / blocking yourself is not a solution
    const typ = JOB_TYPICAL[job];
    const typical = typ.nouns.has(r.noun) || [...r.behaviours].some((b) => typ.beh.has(b));
    const p = this.of(r.owner);
    const jobKey = `${job}:${r.noun}`;
    const uses = p.jobUses.get(jobKey) ?? 0;
    p.jobUses.set(jobKey, uses + 1);
    const C = INSPIRATION.creative;
    const unusual = (typical ? C.typical : 1) / (1 + uses);
    const bonus = C[job] * unusual;
    p.level = clamp01(p.level + bonus);
    const rt = this.w.constructs;
    if (rt && this.w.playerById(r.owner)) rt.setEnergy(r.owner, Math.min(p.energyMax, rt.energyOf(r.owner) + C.energy * unusual));
    p.stats.creative++;
    const reason = `${what} ${JOB_TEXT[job]} with ${article(r.noun)}`;
    this.w.emit('creative_solution', { player: r.owner, against, construct: constructId, label: r.label, job, unusual: r2(unusual), bonus: r2(bonus), typical, reason });
  }

  // ---------------------------------------------------------------- persistence across scene reloads

  snapshot(): InspirationSnapshot { return { players: [...this.players].map(([id, p]) => [id, p.snapshot()]) }; }
  restore(s: InspirationSnapshot) { for (const [id, ps] of s.players) this.of(id).restore(ps); }
}

function clamp(v: number, lo: number, hi: number) { return Math.min(hi, Math.max(lo, v)); }
function clamp01(v: number) { return clamp(v, 0, 1); }
function r2(v: number) { return Math.round(v * 100) / 100; }
function plural(w: string) { return /(s|x|ch|sh)$/.test(w) ? w + 'es' : w.endsWith('y') && !/[aeiou]y$/.test(w) ? w.slice(0, -1) + 'ies' : w + 's'; }
function article(w: string) { return (/^[aeiou]/.test(w) ? 'an ' : 'a ') + w; }
