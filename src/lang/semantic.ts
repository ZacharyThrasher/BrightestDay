/**
 * System 1.5: static word vectors (ConceptNet Numberbatch, int8) map words the lexicon doesn't know
 * onto the nearest known concept / verb class. Only ever consulted for UNKNOWN words, and every use
 * is surfaced to the player as an approximation.
 */
// @ts-ignore - plain ESM module without types
import { loadEmbedder } from '../semantic/embed.mjs';
import { CONCEPTS } from './lexicon.js';
import { VERBS, type VerbClass } from './words.js';
import type { Embedder } from './interpreter.js';

export interface SemanticIndex extends Embedder {
  nearestVerb(word: string): { verb: string; cls: VerbClass; score: number } | null;
  has(word: string): boolean;
}

let cached: Promise<SemanticIndex | null> | null = null;

export function loadSemantic(src: string | ArrayBuffer | URL): Promise<SemanticIndex | null> {
  if (!cached) cached = build(src).catch((e) => { console.warn('semantic index unavailable:', String(e).slice(0, 200)); return null; });
  return cached;
}

async function build(src: any): Promise<SemanticIndex> {
  const emb = await loadEmbedder(src);
  const conceptCands = CONCEPTS.map((c) => ({ id: c.id, text: c.names.slice(0, 12) }));
  const verbCands = Object.keys(VERBS).filter((v) => !v.includes(' ') && v.length > 2).map((v) => ({ id: v, text: v }));
  return {
    nearest(text: string, k: number) {
      return emb.nearest(text, conceptCands, k) as { id: string; score: number }[];
    },
    nearestVerb(word: string) {
      const r = emb.nearest(word, verbCands, 1) as { id: string; score: number }[];
      if (!r.length) return null;
      return { verb: r[0].id, cls: VERBS[r[0].id], score: r[0].score };
    },
    has(word: string) { return !!emb.has(word); },
  };
}
