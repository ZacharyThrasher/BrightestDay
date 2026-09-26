// TEMPORARY STUB — replaced by the real interpreter. Keeps the interface compiling.
import type { World } from '../sim/world.js';
import type { Interpretation, CommitResult } from './types.js';
import { defaultSpec } from '../sim/constructs/spec.js';
import { buildForm } from '../sim/constructs/forms.js';

export class Interpreter {
  constructor(private w: World) {}
  interpret(text: string, _o: { partial?: boolean } = {}): Interpretation {
    const t = text.toLowerCase();
    const form = ['wall', 'ramp', 'glove', 'bridge', 'trampoline'].find((f) => t.includes(f)) ?? null;
    const ops = form ? [{ op: 'create' as const, spec: defaultSpec({ concept: form, label: form, parts: buildForm(form), placement: { at: form === 'ramp' ? 'ground_front' : 'aim', orient: 'face_away' } }) }] : [];
    return { text, ops, summary: form ?? '?', confidence: form ? 0.9 : 0, source: form ? 'rule' : 'none', approximations: [], unknownWords: [], abstain: form ? null : 'not understood', needsEscalation: false, timings: { parseMs: 0 } };
  }
  async commit(text: string): Promise<CommitResult> {
    const t0 = performance.now();
    const i = this.interpret(text);
    const ids: string[] = [];
    const clamps: string[] = [];
    for (const op of i.ops) if (op.op === 'create') { const r = this.w.constructs.spawn(op.spec); if (r.construct) ids.push(r.construct.id); clamps.push(...r.report.clamps); }
    return { ...i, applied: ids.length > 0, messages: clamps, clamps, constructIds: ids, latencyMs: performance.now() - t0, escalated: false };
  }
}
