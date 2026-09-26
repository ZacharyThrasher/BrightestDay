import type { ConstructSpec, Ref } from '../sim/constructs/spec.js';

/** One proposed operation. Proposals are data; only the Director applies them via the construct runtime. */
export type Op =
  | { op: 'create'; spec: ConstructSpec; name?: string }
  | { op: 'modify'; target: string /* construct id */; spec: ConstructSpec; mods: string[] }
  | { op: 'act'; target: string; action: 'throw' | 'release' | 'dismiss'; ref?: Ref; speed?: number }
  | { op: 'control'; action: 'dismiss_all' | 'undo' | 'save' | 'recall' | 'cancel' | 'release_all' | 'tether_release'; name?: string; target?: string }
  /** The player gets in/on a rideable construct (target is a { kind: 'construct' } ref). Executed by the app via the player's ride API. */
  | { op: 'ride'; target: Ref }
  /** The player gets out of / off whatever they are riding. */
  | { op: 'dismount' }
  | { op: 'abstain'; reason: string };

export type SourceKind = 'rule' | 'lexicon' | 'embedding' | 'llm' | 'cache' | 'none';

export interface Interpretation {
  text: string; // the text actually interpreted (after normalisation)
  ops: Op[];
  summary: string; // compact human-readable description of what the ring believes
  confidence: number; // proposer's claim, 0..1 (not trusted by the runtime)
  source: SourceKind;
  approximations: string[]; // e.g. `"hoverboard" ≈ board`
  unknownWords: string[];
  abstain: string | null;
  needsEscalation: boolean; // a stronger interpreter might do better
  timings: { parseMs: number };
}

export interface CommitResult extends Interpretation {
  applied: boolean;
  messages: string[]; // visible feedback lines (clamps, rejections, approximations)
  clamps: string[];
  constructIds: string[];
  latencyMs: number;
  escalated: boolean;
}

/** Normalised functional effect report used by the evaluator (see eval/EVAL_SPEC.md). */
export interface EffectReport {
  intent: 'create' | 'modify' | 'act' | 'control' | 'abstain' | 'none';
  functions: string[];
  target: string | null;
  count: number | null;
  modifications: string[];
  approximated: boolean;
  clamped: boolean;
  abstained: boolean;
  confidence: number;
}
