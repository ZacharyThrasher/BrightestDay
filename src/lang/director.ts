/**
 * Director: the bridge between language and the world.
 *   interpret(text)  -> pure proposal (no side effects), used for ghost previews while speaking
 *   commit(text)     -> validates + applies via the construct runtime, may escalate to System 2
 *
 * NOTE: this is the stable interface the renderer/voice code depends on.
 * The implementation lives in interpreter.ts (rules + lexicon) and escalate.ts (System 2).
 */
import type { World } from '../sim/world.js';
import type { ConstructSpec } from '../sim/constructs/spec.js';
import type { Interpretation, CommitResult } from './types.js';
import { Interpreter } from './interpreter.js';

export type DirectorListener = (r: CommitResult) => void;

export class Director {
  readonly interp: Interpreter;
  private listeners: DirectorListener[] = [];
  history: CommitResult[] = [];

  constructor(readonly world: World) {
    this.interp = new Interpreter(world);
  }

  /** Pure interpretation for previews; never changes the world. */
  interpret(text: string, opts: { partial?: boolean } = {}): Interpretation {
    return this.interp.interpret(text, opts);
  }

  /** Specs to draw as translucent ghosts while the player is still speaking/typing. */
  preview(text: string): { specs: ConstructSpec[]; interpretation: Interpretation } {
    const i = this.interpret(text, { partial: true });
    const specs: ConstructSpec[] = [];
    for (const op of i.ops) if (op.op === 'create' || op.op === 'modify') specs.push(op.spec);
    return { specs, interpretation: i };
  }

  async commit(text: string): Promise<CommitResult> {
    const r = await this.interp.commit(text);
    this.history.push(r);
    if (this.history.length > 200) this.history.shift();
    for (const l of this.listeners) l(r);
    return r;
  }

  onCommit(l: DirectorListener) { this.listeners.push(l); }
}
