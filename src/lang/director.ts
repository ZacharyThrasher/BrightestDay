/**
 * Director: the bridge between language and the world.
 *   interpret(text)  -> pure proposal (no side effects), used for ghost previews while typing
 *   commit(text)     -> validates + applies via the construct runtime, may escalate to System 2
 *
 * Every commit is one CAST for the creativity economy (src/sim/inspiration.ts): constructs created or
 * changed while it runs are scored for novelty. Runtime calls outside a commit are neutral.
 *
 * NOTE: this is the stable interface the app/renderer code depends on.
 * The implementation lives in interpreter.ts (rules + lexicon) and escalate.ts (System 2).
 */
import type { World } from '../sim/world.js';
import type { ConstructSpec } from '../sim/constructs/spec.js';
import type { Interpretation, CommitResult } from './types.js';
import { Interpreter } from './interpreter.js';
import { VERBS, MULTI_VERBS, lemma } from './words.js';
import { conceptByName, addonByName } from './lexicon.js';
import { LOCAL_PLAYER } from '../sim/inspiration.js';

/** Verb classes used in a command ("grab that drone and throw it" → grab, throw), for novelty. */
export function verbClasses(text: string): string[] {
  let t = ` ${text.toLowerCase().replace(/[^a-z' ]+/g, ' ')} `;
  const out: string[] = [];
  for (const mv of MULTI_VERBS) if (t.includes(` ${mv} `)) { out.push(VERBS[mv]); t = t.replace(` ${mv} `, ' '); }
  for (const w of t.split(/\s+/)) {
    if (!w || conceptByName.has(w) || addonByName.has(w)) continue; // "a wall", "a hammer" are things, not verbs
    const l = lemma(w);
    if (l !== w && (conceptByName.has(l) || addonByName.has(l))) continue; // "boxing" glove
    const v = VERBS[l];
    if (v) out.push(v);
  }
  return [...new Set(out)];
}

export type DirectorListener = (r: CommitResult) => void;

export class Director {
  readonly interp: Interpreter;
  /** Whose ring this is (the creativity economy is per player; one Director per player on the host). */
  playerId = LOCAL_PLAYER;
  private listeners: DirectorListener[] = [];
  history: CommitResult[] = [];

  constructor(readonly world: World) {
    this.interp = new Interpreter(world);
  }

  /** Pure interpretation for previews; never changes the world. */
  interpret(text: string, opts: { partial?: boolean } = {}): Interpretation {
    return this.interp.interpret(text, opts);
  }

  /** Specs to draw as translucent ghosts while the player is still typing. */
  preview(text: string): { specs: ConstructSpec[]; interpretation: Interpretation } {
    const i = this.interpret(text, { partial: true });
    const specs: ConstructSpec[] = [];
    for (const op of i.ops) if (op.op === 'create' || op.op === 'modify') specs.push(op.spec);
    return { specs, interpretation: i };
  }

  async commit(text: string): Promise<CommitResult> {
    const insp = this.world.inspiration;
    insp?.beginCast(verbClasses(text), this.playerId);
    try {
      // with a System-2 provider configured: instant System-1 result, corrected later if System 2 disagrees
      if (this.interp.s2 && this.interp.s2Mode !== 'off') {
        const { now, correction } = await this.interp.commitSpeculative(text);
        this.record(now);
        correction.then((c) => { if (c) { this.record(c); for (const l of this.correctionListeners) l(c); } });
        return now;
      }
      const r = await this.interp.commit(text);
      this.record(r);
      return r;
    } finally {
      insp?.endCast();
    }
  }

  private record(r: CommitResult) {
    this.history.push(r);
    if (this.history.length > 200) this.history.shift();
    for (const l of this.listeners) l(r);
  }

  private correctionListeners: DirectorListener[] = [];
  /** Fires when System 2 disagreed and replaced System 1's interpretation. */
  onCorrection(l: DirectorListener) { this.correctionListeners.push(l); }

  onCommit(l: DirectorListener) { this.listeners.push(l); }
}
