/**
 * Typing as a skill: the pure (DOM-free, unit-tested) parts of the ring console.
 *
 *   FocusMeter  — OFFLINE PRACTICE ONLY (off by default; never in an online duel, where both players share
 *                 one clock): while the console is open the world slows (fewer fixed 60 Hz ticks per real
 *                 second, never a different dt); a meter drains in real time and, once empty, time runs at
 *                 full speed until it has recharged. Fast typists get more done per slowed second.
 *   CastClock   — console open → commit time, characters per second, personal bests.
 *   Combo       — successful casts in quick succession (real time) build a combo that amplifies
 *                 inspiration gains (src/sim/inspiration.ts).
 *   Completer   — Tab-completion of the word being typed from history / saved designs / the lexicon.
 *   QuickSlots  — the 1–9 quick-cast bar: saved designs first, then the latest distinct typed commands.
 */
import { conceptByName, addonByName, CONCEPTS, ADDONS } from '../lang/lexicon.js';
import { VERBS, ENTITY_NOUNS, SIZE_ADJ, MATERIAL_ADJ, SPEED_ADJ } from '../lang/words.js';

export const TYPING = {
  focusScale: 0.35, // world speed while typing when practice focus time is on (settings default: off)
  focusSeconds: 5, // real seconds of slow time in a full meter
  rechargeSeconds: 10, // real seconds to refill an empty meter (console closed)
  resumeAt: 0.25, // after running dry, slow time needs this much meter back
  easeSeconds: 0.12, // time scale eases in/out over this long (no hard jolt)
  comboWindow: 6, // real seconds between successful casts to keep a combo going
  comboMax: 9,
};

// ------------------------------------------------------------------------------------------ focus time

export class FocusMeter {
  meter = 1;
  exhausted = false;
  /** Current (eased) time scale; multiply real frame time by this before feeding the fixed-step sim. */
  scale = 1;

  constructor(public target = TYPING.focusScale, public seconds = TYPING.focusSeconds, public rechargeSeconds = TYPING.rechargeSeconds) {}

  get enabled() { return this.target < 0.999; }
  /** True while time is actually being slowed. */
  get active() { return this.scale < 0.999; }

  /** Advance by `realDt` seconds; `typing` = the console is open. Returns the time scale to use. */
  update(realDt: number, typing: boolean): number {
    const dt = Math.max(0, Math.min(realDt, 0.25));
    let want = 1;
    if (typing && this.enabled && !this.exhausted) {
      this.meter = Math.max(0, this.meter - dt / this.seconds);
      if (this.meter <= 0) this.exhausted = true;
      else want = this.target;
    } else if (!typing) {
      this.meter = Math.min(1, this.meter + dt / this.rechargeSeconds);
      if (this.exhausted && this.meter >= TYPING.resumeAt) this.exhausted = false;
    }
    const k = TYPING.easeSeconds > 0 ? 1 - Math.exp(-dt / TYPING.easeSeconds) : 1;
    this.scale += (want - this.scale) * k;
    if (Math.abs(this.scale - want) < 0.002) this.scale = want;
    return this.scale;
  }
}

// ------------------------------------------------------------------------------------------ cast timing

export interface CastTiming { sec: number; cps: number; chars: number }

export class CastClock {
  private openedAt: number | null = null;
  private firstKeyAt: number | null = null;

  open(now: number) { this.openedAt = now; this.firstKeyAt = null; }
  key(now: number) { if (this.openedAt !== null && this.firstKeyAt === null) this.firstKeyAt = now; }
  get running() { return this.openedAt !== null; }

  /** Commit typed text; returns timing (null if nothing was typed in this session, e.g. a recast). */
  commit(now: number, text: string): CastTiming | null {
    if (this.openedAt === null || this.firstKeyAt === null) return null;
    const sec = Math.max(0, (now - this.openedAt) / 1000);
    const typing = Math.max(0.2, (now - this.firstKeyAt) / 1000);
    const chars = text.length;
    return { sec, cps: chars / typing, chars };
  }

  close() { this.openedAt = null; this.firstKeyAt = null; }
}

export interface CastBests { cast: number | null; cps: number | null; casts: number }

/** Personal bests (successful casts only). */
export class CastRecords {
  best: CastBests = { cast: null, cps: null, casts: 0 };

  record(t: CastTiming): { bestCast: boolean; bestCps: boolean } {
    this.best.casts++;
    const bestCast = t.chars >= 3 && (this.best.cast === null || t.sec < this.best.cast);
    const bestCps = t.chars >= 8 && (this.best.cps === null || t.cps > this.best.cps);
    if (bestCast) this.best.cast = t.sec;
    if (bestCps) this.best.cps = t.cps;
    return { bestCast, bestCps };
  }
}

// ------------------------------------------------------------------------------------------ combo

export class Combo {
  n = 0;
  private lastAt = -Infinity;

  /** The combo a cast committed at `now` would have if it succeeds. */
  peek(now: number) { return now - this.lastAt <= TYPING.comboWindow * 1000 ? Math.min(TYPING.comboMax, this.n + 1) : 1; }

  /** Record the outcome of a cast committed at `now`. */
  hit(now: number, ok: boolean) {
    if (!ok) { this.n = 0; this.lastAt = -Infinity; return 0; }
    this.n = this.peek(now);
    this.lastAt = now;
    return this.n;
  }

  /** Seconds left before the combo lapses (0 when none). */
  left(now: number) { return this.n > 1 ? Math.max(0, TYPING.comboWindow - (now - this.lastAt) / 1000) : 0; }
}

// ------------------------------------------------------------------------------------------ completion

const TIER = { history: 0, saved: 1, concept: 2, addon: 3, verb: 4, name: 5, word: 6 } as const;

let baseVocab: Map<string, number> | null = null;
function lexiconVocab(): Map<string, number> {
  if (baseVocab) return baseVocab;
  const m = new Map<string, number>();
  const put = (w: string, tier: number) => { const k = w.toLowerCase(); if (k.length >= 3 && (m.get(k) ?? 99) > tier) m.set(k, tier); };
  for (const c of CONCEPTS) put(c.id, TIER.concept);
  for (const a of ADDONS) put(a.id, TIER.addon);
  for (const v of Object.keys(VERBS)) if (!v.includes('_')) put(v, TIER.verb);
  for (const n of conceptByName.keys()) put(n, TIER.name);
  for (const n of addonByName.keys()) put(n, TIER.name);
  for (const n of Object.keys(ENTITY_NOUNS)) put(n, TIER.name);
  for (const w of [...Object.keys(SIZE_ADJ), ...Object.keys(MATERIAL_ADJ), ...Object.keys(SPEED_ADJ)]) if (!w.includes('_')) put(w, TIER.word);
  baseVocab = m;
  return m;
}

export interface Suggestion {
  /** Characters to append to the input to complete the current word (ghost text). */
  suffix: string;
  /** The completed word/phrase. */
  word: string;
  /** All candidates (best first) for Tab-cycling. */
  candidates: string[];
  /** Length of the fragment being completed. */
  fragment: number;
}

export class Completer {
  private history = new Map<string, number>();
  private saved = new Set<string>();

  /** Words the player typed before are the best predictor of what they type next. */
  learn(text: string) {
    for (const w of text.toLowerCase().split(/[^a-z'-]+/)) if (w.length >= 3) this.history.set(w, (this.history.get(w) ?? 0) + 1);
  }

  setSaved(names: Iterable<string>) { this.saved = new Set([...names].map((n) => n.toLowerCase())); }

  /** Completion for the word at the end of `text` (null when there's nothing sensible to add). */
  suggest(text: string): Suggestion | null {
    if (!text || /\s$/.test(text)) return null;
    const words = text.toLowerCase().split(/\s+/);
    const last = words[words.length - 1];
    const two = words.length >= 2 ? `${words[words.length - 2]} ${last}` : null;
    // a two-word fragment ("wrecking b") first, then the last word alone
    for (const frag of [two, last]) {
      if (!frag || frag.replace(/\s/g, '').length < 2 || !/^[a-z' -]+$/.test(frag)) continue;
      const cands = this.candidates(frag);
      if (!cands.length) continue;
      if (cands[0] === frag) return null; // already a whole word
      return { suffix: cands[0].slice(frag.length), word: cands[0], candidates: cands, fragment: frag.length };
    }
    return null;
  }

  private candidates(frag: string): string[] {
    const tiers = new Map<string, number>(lexiconVocab());
    for (const s of this.saved) tiers.set(s, TIER.saved);
    for (const w of this.history.keys()) tiers.set(w, TIER.history);
    const out: { w: string; tier: number; n: number }[] = [];
    for (const [w, tier] of tiers) if (w.startsWith(frag)) out.push({ w, tier, n: this.history.get(w) ?? 0 });
    out.sort((a, b) => (a.w === frag ? -1 : b.w === frag ? 1 : 0) || a.tier - b.tier || b.n - a.n || a.w.length - b.w.length || (a.w < b.w ? -1 : 1));
    return out.slice(0, 8).map((x) => x.w);
  }
}

// ------------------------------------------------------------------------------------------ quick-cast

export interface Slot { key: number; text: string; saved: boolean }

export class QuickSlots {
  private saved: string[] = [];
  private recent: string[] = [];

  /** A saved design ("save this as thumper") gets a permanent slot, in save order. */
  addSaved(name: string) { if (!this.saved.includes(name)) this.saved.push(name); }

  /** A typed command that formed something becomes slot 1 (quick-casts don't reorder the bar). */
  noteTyped(text: string) {
    const t = text.trim();
    if (!t || this.saved.includes(t)) return;
    this.recent = [t, ...this.recent.filter((x) => x !== t)].slice(0, 9);
  }

  slots(): Slot[] {
    const out: Slot[] = this.saved.slice(0, 9).map((text, i) => ({ key: i + 1, text, saved: true }));
    for (const text of this.recent) { if (out.length >= 9) break; out.push({ key: out.length + 1, text, saved: false }); }
    return out;
  }

  get(key: number): Slot | null { return this.slots().find((s) => s.key === key) ?? null; }
}
