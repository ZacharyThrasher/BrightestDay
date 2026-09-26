/**
 * Rule-based System-1 parser: messy spoken English -> small clause structures.
 * Deliberately shallow: it extracts slots (verb class, objects, prepositions, modifiers,
 * relative clauses) and leaves meaning/resolution to interpreter.ts.
 */
import {
  VERBS, MULTI_VERBS, lemma, SIZE_ADJ, DIM_ADJ, SPEED_ADJ, MATERIAL_ADJ, IGNORED_ADJ, COLOR_WORDS, NUMBERS, ARRANGEMENT_WORDS, FILLERS,
  PREPOSITIONS, ENTITY_NOUNS, PLACE_NOUNS, PRONOUNS_CONSTRUCT, PRONOUNS_PLURAL, PRONOUNS_PERSON, SELF_WORDS, type VerbClass,
} from './words.js';
import { conceptByName, addonByName, MAX_NAME_WORDS, type Concept, type Addon } from './lexicon.js';

export interface RefWord {
  kind: 'self' | 'construct' | 'constructs' | 'person' | 'entity' | 'place' | 'aim' | 'saved' | 'all';
  entityType?: string;
  place?: string;
  plural?: boolean;
  noun?: string;
  which?: 'nearest' | 'all' | 'that' | 'other' | 'last' | 'first';
}

export interface AddonUse { addon: Addon; where: string | null; count: number }

export interface NP {
  tokens: string[];
  det: string | null;
  count: number | null;
  plural: boolean;
  sizes: number[];
  dims: [number, number, number][];
  speeds: number[];
  materials: string[];
  colors: string[];
  concept: Concept | null;
  conceptWords: string | null;
  unknownHead: string | null;
  unknownWords: string[];
  ignoredWords: string[];
  addons: AddonUse[];
  rel: Clause[];
  ref: RefWord | null;
  analogy: boolean;
  arrangement: string | null;
}

export interface Prep { prep: string; np: NP }

export interface Clause {
  raw: string[];
  verb: string | null;
  vclass: VerbClass | null;
  particle: string | null;
  subject: NP | null; // for causatives ("make THEM orbit me") and relative clauses
  objects: NP[];
  preps: Prep[];
  bareAdjs: string[]; // "bigger!", "make it faster"
  negated: boolean;
  purpose: Clause | null;
  causative: boolean;
  beneficiarySelf: boolean; // "give me", "get me", "make me"
  savedName: string | null;
  notes: string[];
}

export interface Parse { clauses: Clause[]; normalized: string; corrections: number; analogies: string[] }

const PARTICLES = new Set(['up', 'down', 'away', 'back', 'around', 'over', 'off', 'out', 'in', 'forward', 'backward', 'backwards', 'aside', 'along', 'across', 'through']);
const DETS = new Set(['a', 'an', 'the', 'that', 'this', 'those', 'these', 'my', 'your', 'his', 'her', 'their', 'our', 'another', 'some', 'any', 'each', 'every', 'all', 'one', 'its', 'which', 'what']);
const REL_PRONOUNS = new Set(['that', 'which', 'who', 'what']);
const SPLITTERS = ['and then', 'then', 'after that', 'afterwards', 'and also', 'also', 'plus', 'and now', 'now', 'next'];
const CORRECTION = /(?:\b(?:no wait|no no|wait no|wait|actually no|actually|i mean|i meant|scratch that|sorry|rather|make that|on second thought|nope|correction|er no|uh no)\b|(?:^|,|\s)no\s*,|,\s*no\b)/;
const RELATION_WORDS: Record<string, string> = { back: 'back', rear: 'back', behind: 'back', front: 'front', nose: 'front', face: 'front', top: 'top', roof: 'top', bottom: 'bottom', underside: 'bottom', base: 'bottom', underneath: 'bottom', side: 'sides', sides: 'sides', edges: 'sides', left: 'sides', right: 'sides', end: 'tip', ends: 'tip', tip: 'tip', tips: 'tip', around: 'around', all: 'around', everywhere: 'around' };

// ------------------------------------------------------------------------------- normalisation

const CONTRACTIONS: [RegExp, string][] = [
  [/\bwon't\b/g, 'will not'], [/\bcan't\b/g, 'can not'], [/n't\b/g, ' not'], [/'re\b/g, ' are'], [/'ll\b/g, ' will'], [/'ve\b/g, ' have'], [/'d\b/g, ' would'],
  [/\blet's\b/g, 'let us'], [/\bit's\b/g, 'it is'], [/\bthat's\b/g, 'that is'], [/\bwhat's\b/g, 'what is'], [/\bthere's\b/g, 'there is'], [/\bhe's\b/g, 'he is'], [/\bshe's\b/g, 'she is'],
  [/\bi'm\b/g, 'i am'], [/\bgimme\b/g, 'give me'], [/\bgonna\b/g, 'going to'], [/\bwanna\b/g, 'want to'], [/\bgotta\b/g, 'got to'], [/\blemme\b/g, 'let me'], [/\b'em\b/g, 'them'], [/\bem\b/g, 'them'],
  [/\bya\b/g, 'you'], [/\bu\b/g, 'you'], [/\bcuz\b/g, 'because'], [/\bplz\b/g, 'please'], [/\btho\b/g, 'though'],
];

export function normalize(text: string): { text: string; corrections: number } {
  let t = ` ${text.toLowerCase()} `;
  t = t.replace(/[’‘`]/g, "'").replace(/[“”]/g, '"');
  for (const [re, rep] of CONTRACTIONS) t = t.replace(re, rep);
  t = t.replace(/\b(out of|made of|made out of|of) (hard )?light\b/g, ' ');
  t = t.replace(/(\d+)\s*(x|times)\b/g, '$1 times');
  t = t.replace(/[^a-z0-9'.,!?;:\- ]/g, ' ');
  t = t.replace(/-/g, ' ');
  // repeated stutters "a a ramp", "the the"
  t = t.replace(/\b(\w+)( \1\b)+/g, '$1');
  // corrections: keep what follows the last correction marker, splicing it onto the left context
  let corrections = 0;
  // unpunctuated "no" followed by a determiner or a verb is a correction marker
  t = t.replace(/ no (?=(a|an|the|make|grab|punch|build|put|throw|give|use|actually)\b)/g, ' , no , ');
  for (let guard = 0; guard < 4; guard++) {
    const m = CORRECTION.exec(t);
    if (!m) break;
    const left = t.slice(0, m.index);
    let right = t.slice(m.index + m[0].length);
    right = right.replace(/^[\s,.!]+/, ' ');
    corrections++;
    t = spliceCorrection(left, right);
  }
  t = t.replace(/\s+/g, ' ').trim();
  return { text: t, corrections };
}

function spliceCorrection(left: string, right: string): string {
  const rt = tokenize(right);
  const lt = tokenize(left);
  if (!rt.length) return left;
  const rightHasVerb = rt.some((w) => VERBS[lemma(w)] && !DETS.has(w) && !conceptByName.has(w));
  // "make a ramp no wait a bridge" -> "make a bridge"; "put it on the left, I mean right" -> "put it on the right"
  if (!rightHasVerb || DETS.has(rt[0]) || NUMBERS[rt[0]] !== undefined) {
    // find where the replaced phrase started in the left: last determiner/number/preposition-object
    let cut = -1;
    for (let i = lt.length - 1; i >= 0; i--) {
      if (DETS.has(lt[i]) || NUMBERS[lt[i]] !== undefined || /^\d+$/.test(lt[i])) { cut = i; break; }
      if (VERBS[lemma(lt[i])] && i < lt.length - 1) { cut = i + 1; break; }
    }
    if (cut < 0) cut = Math.max(0, lt.length - rt.length);
    // if the replacement starts with a determiner keep it, else keep the left determiner
    const keepDet = !DETS.has(rt[0]) && cut < lt.length && DETS.has(lt[cut]) ? [lt[cut]] : [];
    return [...lt.slice(0, cut), ...keepDet, ...rt].join(' ');
  }
  // right is a whole new command: drop the left clause (but keep earlier clauses separated by "and then")
  const prevBoundary = Math.max(left.lastIndexOf(' then '), left.lastIndexOf('.'), left.lastIndexOf(' and '));
  return (prevBoundary >= 0 ? left.slice(0, prevBoundary + 1) + ' then ' : '') + right;
}

export function tokenize(t: string): string[] {
  return t.replace(/[.,!?;:]/g, ' ').split(/\s+/).filter(Boolean);
}

// ------------------------------------------------------------------------------- clause splitting

function splitClauses(t: string): string[][] {
  // sentence punctuation
  const raw = t.split(/[.!?;]+|,\s*(?=(?:and )?(?:then|now|next|also)\b)/).map((s) => s.trim()).filter(Boolean);
  // commas followed by a new imperative verb start a new clause ("make a carousel, turn it vertical, ...")
  const parts: string[] = [];
  for (const r of raw) {
    const segs = r.split(/\s*,\s*/);
    let cur = segs[0];
    for (const sg of segs.slice(1)) {
      const tk = tokenize(sg.replace(/^and\s+/, ''));
      if (tk.length && startsWithVerb(tk, 0)) { parts.push(cur); cur = sg.replace(/^and\s+/, ''); }
      else cur += ' ' + sg;
    }
    parts.push(cur);
  }
  const out: string[][] = [];
  for (const p of parts) {
    let toks = tokenize(p);
    // split on connective phrases
    let cur: string[] = [];
    for (let i = 0; i < toks.length; i++) {
      const two = toks[i] + ' ' + (toks[i + 1] ?? '');
      const three = two + ' ' + (toks[i + 2] ?? '');
      const sp = SPLITTERS.find((s) => s === three || s === two || s === toks[i]);
      if (sp && cur.length && hasVerbAhead(toks, i + sp.split(' ').length)) {
        out.push(cur); cur = []; i += sp.split(' ').length - 1; continue;
      }
      if (toks[i] === 'and' && cur.length && startsWithVerb(toks, i + 1) && !isInsideWith(cur)) { out.push(cur); cur = []; continue; }
      cur.push(toks[i]);
    }
    if (cur.length) out.push(cur);
    void toks;
    toks = [];
  }
  return out;
}

function isInsideWith(cur: string[]) {
  // "with rockets and spikes" should not split; "with rockets and punch him" should.
  return false && cur.includes('with');
}

function verbAt(toks: string[], i: number): { verb: string; len: number } | null {
  for (const mv of MULTI_VERBS) {
    const n = mv.split(' ').length;
    if (toks.slice(i, i + n).join(' ') === mv) return { verb: mv, len: n };
  }
  const w = toks[i];
  if (!w) return null;
  const l = lemma(w);
  if (VERBS[l]) return { verb: l, len: 1 };
  return null;
}

function startsWithVerb(toks: string[], i: number): boolean {
  let j = i;
  while (j < toks.length && (FILLERS.has(toks[j]) || toks[j] === 'then' || toks[j] === 'also')) j++;
  const v = verbAt(toks, j);
  if (!v) return false;
  // nouns that are also verbs ("a ramp and a hammer") — only treat as verb if not preceded by a determiner
  const next = toks[j + v.len];
  if (conceptByName.has(toks[j]) && !(next && (DETS.has(next) || SELF_WORDS.has(next) || PRONOUNS_PERSON.has(next) || PRONOUNS_CONSTRUCT.has(next) || PRONOUNS_PLURAL.has(next) || NUMBERS[next] !== undefined))) return false;
  return true;
}

function hasVerbAhead(toks: string[], i: number) {
  for (let j = i; j < Math.min(toks.length, i + 4); j++) if (verbAt(toks, j)) return true;
  // bare modifier follow-ups: "then bigger"
  for (let j = i; j < toks.length; j++) if (SIZE_ADJ[toks[j]] || DIM_ADJ[toks[j]] || SPEED_ADJ[toks[j]]) return true;
  return i < toks.length;
}

// ------------------------------------------------------------------------------- clause parsing

export function parse(text: string): Parse {
  const { text: norm, corrections } = normalize(text);
  const analogies: string[] = [];
  const clauses: Clause[] = [];
  for (const toks of splitClauses(norm)) {
    const c = parseClause(toks, analogies);
    if (c) clauses.push(c);
  }
  return { clauses, normalized: norm, corrections, analogies };
}

function emptyClause(raw: string[]): Clause {
  return { raw, verb: null, vclass: null, particle: null, subject: null, objects: [], preps: [], bareAdjs: [], negated: false, purpose: null, causative: false, beneficiarySelf: false, savedName: null, notes: [] };
}

function parseClause(toksIn: string[], analogies: string[]): Clause | null {
  // strip fillers but keep 'like' as an analogy marker
  let toks: string[] = [];
  for (let i = 0; i < toksIn.length; i++) {
    const w = toksIn[i];
    if (w === 'like' && (DETS.has(toksIn[i + 1]) || conceptByName.has(toksIn[i + 1] ?? ''))) { analogies.push(toksIn.slice(i + 1, i + 4).join(' ')); continue; }
    if (w === 'like') continue;
    if (FILLERS.has(w) && !(w === 'go' && verbAt(toksIn, i)?.len === 2)) continue;
    toks.push(w);
  }
  const c = emptyClause(toksIn);
  if (!toks.length) return null;
  if (toks[0] === 'not' || toks[0] === 'dont' || toks[0] === 'do' && toks[1] === 'not') c.negated = true;

  // "save this as X" / "call it X" / "name it X"
  const saveM = toks.join(' ').match(/\b(?:save|remember|store|keep)\b.*?\b(?:as|called)\s+(.+)$/) || toks.join(' ').match(/\b(?:call|name)\s+(?:it|this|that|them)\s+(.+)$/);
  if (saveM) {
    c.verb = 'save'; c.vclass = 'save'; c.savedName = saveM[1].replace(/^(the|a|an)\s+/, '').trim();
    return c;
  }

  // find the main verb
  let vi = -1;
  let vlen = 0;
  for (let i = 0; i < toks.length; i++) {
    const v = verbAt(toks, i);
    if (!v) continue;
    // skip "want/need to <verb>" -> use the inner verb
    if ((v.verb === 'want' || v.verb === 'need') && toks[i + 1] === 'to') continue;
    // a noun used after a determiner is not a verb ("a hammer", "the ring")
    if (i > 0 && (DETS.has(toks[i - 1]) || NUMBERS[toks[i - 1]] !== undefined || SIZE_ADJ[toks[i - 1]] || MATERIAL_ADJ[toks[i - 1]] || IGNORED_ADJ.has(toks[i - 1]) || COLOR_WORDS.has(toks[i - 1]))) continue;
    // noun-verbs at the very start followed by a determiner are verbs ("hammer that drone"), otherwise nouns ("hammer!")
    if (conceptByName.has(toks[i]) && v.len === 1) {
      const nx = toks[i + 1];
      const verbish = nx && (DETS.has(nx) || SELF_WORDS.has(nx) || PRONOUNS_PERSON.has(nx) || PRONOUNS_CONSTRUCT.has(nx) || PRONOUNS_PLURAL.has(nx) || PARTICLES.has(nx) || ENTITY_NOUNS[nx] || /^\d+$/.test(nx) || NUMBERS[nx] !== undefined);
      if (!verbish) continue;
    }
    vi = i; vlen = v.len; c.verb = v.verb; c.vclass = VERBS[v.verb];
    break;
  }

  let rest: string[];
  if (vi > 0) {
    const pre = toks.slice(0, vi).filter((w) => !['i', 'we', 'you', 'please', 'to', 'and', 'then', 'now'].includes(w));
    const preNP = pre.length ? parseNP(pre, analogies) : null;
    if (preNP && (preNP.concept || preNP.unknownHead && pre.length > 1) && !preNP.ref && !['want', 'need'].includes(c.verb ?? '')) {
      const sub = parseClause(toks.slice(vi), analogies);
      const obj = preNP;
      if (sub) obj.rel.push(sub);
      const cc = emptyClause(toksIn);
      cc.objects.push(obj);
      return cc;
    }
  }
  if (vi < 0) {
    rest = toks; // bare noun phrase or bare adjectives
  } else {
    const pre = toks.slice(0, vi);
    if (pre.includes('not') || pre.includes('never')) c.negated = true;
    rest = toks.slice(vi + vlen);
    // particles right after the verb ("pick it up", "knock him back") are collected wherever they are
    // beneficiary: "give me a", "get me a", "make me a", "build me a"
    if ((c.vclass === 'create' || c.verb === 'get' || c.verb === 'bring') && SELF_WORDS.has(rest[0]) && rest[1] && (DETS.has(rest[1]) || NUMBERS[rest[1]] !== undefined || SIZE_ADJ[rest[1]] || conceptByName.has(rest[1]) || MATERIAL_ADJ[rest[1]] || addonByName.has(rest[1]) || IGNORED_ADJ.has(rest[1]))) {
      c.beneficiarySelf = true;
      rest = rest.slice(1);
      if (c.verb === 'get' || c.verb === 'bring') { c.verb = 'make'; c.vclass = 'create'; }
    }
    // "get me across/up there" -> goal verbs
    if (c.verb === 'get' && (SELF_WORDS.has(rest[0]) || rest[0] === 'to')) {
      const r2 = rest.slice(SELF_WORDS.has(rest[0]) ? 1 : 0);
      if (['across', 'over', 'past', 'through'].includes(r2[0])) { c.vclass = 'cross'; rest = r2.slice(1); }
      else if (['up', 'onto', 'on', 'to', 'into', 'atop'].includes(r2[0])) { c.vclass = 'climb'; rest = r2.slice(1); }
      else if (['down', 'off'].includes(r2[0])) { c.vclass = 'descend'; rest = r2.slice(1); }
      else if (['out'].includes(r2[0])) { c.vclass = 'cross'; rest = r2.slice(1); }
    }
    // causatives: make/have/let/get + REF + verb ("make them orbit me", "make the fingers grab him", "have it spin")
    if (['make', 'have', 'let', 'get', 'turn'].includes(c.verb!) || c.vclass === 'create') {
      const inner = findInnerVerb(rest);
      if (inner && inner.at > 0 && inner.at <= 5) {
        const subjToks = rest.slice(0, inner.at);
        const subj = parseNP(subjToks, analogies);
        const isRefSubject = subj.ref || subj.det === 'the' || subj.det === 'those' || subj.det === 'these' || subj.det === 'that' || subj.det === 'this' || subj.det === 'its' || subj.det === 'my';
        if (isRefSubject && !(inner.verb === 'hold' && subjToks.length === 0)) {
          c.causative = true;
          c.subject = subj;
          c.verb = inner.verb; c.vclass = VERBS[inner.verb];
          rest = rest.slice(inner.at + inner.len);
        }
      }
    }
  }

  // purpose clause: "... to block the bullets", "so i can get across"
  const toIdx = rest.findIndex((w, i) => (w === 'to' || w === 'so') && verbAt(rest, i + 1 + (rest[i + 1] === 'that' ? 1 : 0) + (rest[i + 1] === 'i' ? 1 : 0)) && i > 0);
  if (toIdx > 0) {
    const sub = parseClause(rest.slice(toIdx + 1), analogies);
    c.purpose = sub;
    rest = rest.slice(0, toIdx);
  }

  // collect particles
  const kept: string[] = [];
  for (let i = 0; i < rest.length; i++) {
    const w = rest[i];
    if (PARTICLES.has(w) && (i === 0 || i === rest.length - 1 || SELF_WORDS.has(rest[i - 1]) || PRONOUNS_CONSTRUCT.has(rest[i - 1]) || PRONOUNS_PERSON.has(rest[i - 1]) || PRONOUNS_PLURAL.has(rest[i - 1])) && !(i < rest.length - 1 && isPrepStart(rest, i) && w !== 'up' && w !== 'down')) {
      if (!c.particle) c.particle = w;
      continue;
    }
    kept.push(w);
  }
  rest = kept;

  // split objects / prepositional phrases / relative clauses
  const segs: { prep: string | null; toks: string[] }[] = [{ prep: null, toks: [] }];
  for (let i = 0; i < rest.length; i++) {
    const p = isPrepStart(rest, i);
    // relative clause: "that orbits me", "which spins", or participle "orbiting me"
    const isRel = (REL_PRONOUNS.has(rest[i]) && verbAt(rest, i + 1) && i > 0) || (/ing$/.test(rest[i]) && VERBS[lemma(rest[i])] && i > 0 && !conceptByName.has(rest[i]) && !MATERIAL_ADJ[rest[i]]);
    if (isRel) {
      const relToks = rest.slice(REL_PRONOUNS.has(rest[i]) ? i + 1 : i);
      const sub = parseClause(relToks, analogies);
      if (sub) {
        const target = segs[segs.length - 1];
        (target as any).rel = [...((target as any).rel ?? []), sub];
      }
      break;
    }
    if (p) {
      segs.push({ prep: p, toks: [] });
      i += p.split(' ').length - 1;
      continue;
    }
    segs[segs.length - 1].toks.push(rest[i]);
  }

  // objects: first segment, split on "and" into several NPs
  const objSeg = segs[0];
  const objNPs = splitAnd(objSeg.toks).map((t) => parseNP(t, analogies)).filter((np) => np.tokens.length || np.ref);
  if ((objSeg as any).rel) {
    if (objNPs.length) objNPs[objNPs.length - 1].rel.push(...(objSeg as any).rel);
    else c.objects.push({ ...parseNP([], analogies), rel: (objSeg as any).rel });
  }
  c.objects.push(...objNPs);
  for (const s of segs.slice(1)) {
    const np = parseNP(s.toks, analogies);
    if ((s as any).rel) np.rel.push(...(s as any).rel);
    // "with rockets on the back": the "on the back" is a relation for the with-addon, not a placement
    const prev = c.preps[c.preps.length - 1];
    if (prev && prev.prep === 'with' && (s.prep === 'on' || s.prep === 'at' || s.prep === 'in' || s.prep === 'along' || s.prep === 'around' || s.prep === 'under' || s.prep === 'underneath') && np.tokens.some((w) => RELATION_WORDS[w])) {
      const rel = np.tokens.map((w) => RELATION_WORDS[w]).find(Boolean) ?? null;
      for (const a of prev.np.addons) a.where = rel;
      continue;
    }
    if (s.prep === 'with' && !np.addons.length && !np.concept && !np.ref && np.tokens.some((w) => RELATION_WORDS[w])) continue;
    // "put rockets on the back of it"
    if ((s.prep === 'on' || s.prep === 'to') && np.tokens.some((w) => RELATION_WORDS[w]) && !np.concept && !np.ref) {
      const rel = np.tokens.map((w) => RELATION_WORDS[w]).find(Boolean) ?? null;
      for (const o of c.objects) for (const a of o.addons) a.where = rel;
      (c as any).relationWhere = rel;
      continue;
    }
    c.preps.push({ prep: s.prep!, np });
  }

  // bare adjectives ("bigger", "make it faster")
  const allToks = [...(c.objects.flatMap((o) => o.tokens)), ...rest];
  for (const w of allToks) if (SIZE_ADJ[w] || DIM_ADJ[w] || SPEED_ADJ[w] || MATERIAL_ADJ[w]) c.bareAdjs.push(w);
  c.bareAdjs = [...new Set(c.bareAdjs)];
  return c;
}

function findInnerVerb(toks: string[]): { verb: string; at: number; len: number } | null {
  for (let i = 0; i < toks.length; i++) {
    const standalonePronoun = i === 1 && ['those', 'these', 'that', 'this', 'all', 'both', 'each', 'one'].includes(toks[0]);
    if (i > 0 && DETS.has(toks[i - 1]) && !standalonePronoun) continue;
    const v = verbAt(toks, i);
    if (v && !conceptByName.has(toks[i]) && !MATERIAL_ADJ[toks[i]] && !SIZE_ADJ[toks[i]]) return { verb: v.verb, at: i, len: v.len };
    if (v && conceptByName.has(toks[i]) && i > 0 && (SELF_WORDS.has(toks[i + 1] ?? '') || PRONOUNS_PERSON.has(toks[i + 1] ?? '') || DETS.has(toks[i + 1] ?? '') || PARTICLES.has(toks[i + 1] ?? ''))) return { verb: v.verb, at: i, len: v.len };
  }
  return null;
}

function isPrepStart(toks: string[], i: number): string | null {
  for (const p of PREPOSITIONS) {
    const n = p.split(' ').length;
    if (toks.slice(i, i + n).join(' ') === p) {
      // 'up'/'down' are only prepositions when followed by a noun phrase
      if ((p === 'up' || p === 'down' || p === 'in' || p === 'off' || p === 'past') && !toks[i + n]) return null;
      if (p === 'of' && i > 0 && (NUMBERS[toks[i - 1]] !== undefined || ARRANGEMENT_WORDS[toks[i - 1]] || toks[i - 1] === 'one' || toks[i - 1] === 'all' || toks[i - 1] === 'both' || toks[i - 1] === 'each' || toks[i - 1] === 'kind' || toks[i - 1] === 'sort' || toks[i - 1] === 'top' || toks[i - 1] === 'front' || toks[i - 1] === 'back' || toks[i - 1] === 'side' || toks[i - 1] === 'end' || toks[i - 1] === 'bunch' || toks[i - 1] === 'lot' || toks[i - 1] === 'couple' || toks[i - 1] === 'pair' || toks[i - 1] === 'dozen' || toks[i - 1] === 'ton' || toks[i - 1] === 'tons' || toks[i - 1] === 'lots')) return null;
      if (p === 'like') return null;
      return p;
    }
  }
  return null;
}

function splitAnd(toks: string[]): string[][] {
  const out: string[][] = [[]];
  for (let i = 0; i < toks.length; i++) {
    if ((toks[i] === 'and' || toks[i] === 'or' || toks[i] === 'plus') && out[out.length - 1].length && (DETS.has(toks[i + 1]) || NUMBERS[toks[i + 1]] !== undefined || conceptByName.has(toks[i + 1]) || SIZE_ADJ[toks[i + 1]])) {
      out.push([]);
      continue;
    }
    out[out.length - 1].push(toks[i]);
  }
  return out.filter((t) => t.length);
}

// ------------------------------------------------------------------------------- noun phrases

function singular(w: string): string {
  if (conceptByName.has(w) || ENTITY_NOUNS[w]) return w;
  if (w.endsWith('ies')) return w.slice(0, -3) + 'y';
  if (w.endsWith('ves')) return w.slice(0, -3) + 'f';
  if (w.endsWith('ses') || w.endsWith('xes') || w.endsWith('ches') || w.endsWith('shes')) return w.slice(0, -2);
  if (w.endsWith('s') && !w.endsWith('ss')) return w.slice(0, -1);
  return w;
}

export function parseNP(toksIn: string[], analogies: string[]): NP {
  const np: NP = {
    tokens: toksIn, det: null, count: null, plural: false, sizes: [], dims: [], speeds: [], materials: [], colors: [], concept: null, conceptWords: null,
    unknownHead: null, unknownWords: [], ignoredWords: [], addons: [], rel: [], ref: null, analogy: false, arrangement: null,
  };
  let toks = [...toksIn];
  if (!toks.length) return np;
  // multiword counts ("a bunch of", "a couple of", "a row of")
  const joined = toks.join(' ');
  for (const [k, v] of Object.entries(NUMBERS)) {
    if (k.includes(' ') && (joined === k || joined.startsWith(k + ' '))) {
      np.count = v;
      const arr = k.split(' ')[1];
      if (ARRANGEMENT_WORDS[arr]) np.arrangement = ARRANGEMENT_WORDS[arr];
      toks = toks.slice(k.split(' ').length);
      break;
    }
  }
  // "it longer", "them bigger and faster": pronoun + modifiers
  if (toks.length > 1 && (PRONOUNS_CONSTRUCT.has(toks[0]) || PRONOUNS_PLURAL.has(toks[0])) && toks.slice(1).every((w) => SIZE_ADJ[w] || DIM_ADJ[w] || SPEED_ADJ[w] || MATERIAL_ADJ[w] || IGNORED_ADJ.has(w) || ['more', 'even', 'much', 'way', 'lot', 'a', 'bit', 'little', 'and', 'too', 'again'].includes(w))) {
    np.ref = PRONOUNS_PLURAL.has(toks[0]) ? { kind: 'constructs', plural: true } : { kind: 'construct', which: 'last' };
    for (const w of toks.slice(1)) { if (SIZE_ADJ[w]) np.sizes.push(SIZE_ADJ[w]); if (DIM_ADJ[w]) np.dims.push(DIM_ADJ[w]); if (SPEED_ADJ[w]) np.speeds.push(SPEED_ADJ[w]); if (MATERIAL_ADJ[w]) np.materials.push(MATERIAL_ADJ[w]); }
    return np;
  }
  // references first
  const j = toks.join(' ');
  if (!toks.length) return np;
  if (SELF_WORDS.has(j) || j === 'my self') { np.ref = { kind: 'self' }; return np; }
  if (PRONOUNS_PERSON.has(j)) { np.ref = { kind: 'person', plural: false }; return np; }
  if (PRONOUNS_PLURAL.has(j) || j === 'all of them' || j === 'all of those') { np.ref = { kind: 'constructs', plural: true }; return np; }
  if (PRONOUNS_CONSTRUCT.has(j) || j === 'the last one' || j === 'the first one' || j === 'the other one' || j === 'the previous one') {
    np.ref = { kind: 'construct', which: j.includes('first') ? 'first' : j.includes('other') ? 'other' : 'last' };
    return np;
  }
  if (j === 'there' || j === 'here' || j === 'over there' || j === 'right there' || j === 'that spot' || j === 'this spot' || j === 'where i am looking' || j === 'where i am aiming') { np.ref = { kind: 'aim' }; return np; }

  // determiner / counts
  while (toks.length) {
    const w = toks[0];
    if (DETS.has(w) && !np.det) { np.det = w; if (w === 'those' || w === 'these' || w === 'all' || w === 'every' || w === 'each') np.plural = true; if (w === 'another') np.count = 1; toks.shift(); continue; }
    if (/^\d+$/.test(w)) { np.count = parseInt(w, 10); toks.shift(); if (toks[0] === 'times') toks.shift(); continue; }
    if (NUMBERS[w] !== undefined && w !== 'a' && w !== 'an' && !(w === 'double' && toks.length === 1)) {
      // "ten thousand"
      let n = NUMBERS[w];
      toks.shift();
      while (toks.length && NUMBERS[toks[0]] !== undefined && NUMBERS[toks[0]] >= 100) { n *= NUMBERS[toks[0]]; toks.shift(); }
      np.count = (np.count ?? 1) * n;
      if (toks[0] === 'of') toks.shift();
      continue;
    }
    if (w === 'of' && np.count) { toks.shift(); continue; }
    break;
  }
  // "all the drones", "both drones"
  if (toks[0] === 'the' && (np.det === 'all' || np.det === 'both')) toks.shift();
  // "five of them", "3 more of those"
  if (toks[0] === 'more') toks.shift();
  if (np.count !== null && toks.length && (PRONOUNS_PLURAL.has(toks.join(' ')) || PRONOUNS_CONSTRUCT.has(toks.join(' ')))) { np.ref = { kind: 'constructs', plural: true }; return np; }

  // modifiers + head search (longest compound match, rightmost wins as head)
  const remaining: string[] = [];
  let i = 0;
  const matches: { start: number; len: number; words: string; concept: Concept | null; addon: Addon | null }[] = [];
  while (i < toks.length) {
    let found = false;
    for (let n = Math.min(MAX_NAME_WORDS, toks.length - i); n >= 1; n--) {
      const phrase = toks.slice(i, i + n).join(' ');
      const sing = n === 1 ? singular(phrase) : toks.slice(i, i + n - 1).concat(singular(toks[i + n - 1])).join(' ');
      const c = conceptByName.get(phrase) ?? conceptByName.get(sing) ?? null;
      const a = addonByName.get(phrase) ?? addonByName.get(sing) ?? null;
      // multiword adjective phrases ("planet sized", "super big")
      if (n > 1 && (SIZE_ADJ[phrase] || DIM_ADJ[phrase] || SPEED_ADJ[phrase] || MATERIAL_ADJ[phrase])) {
        if (SIZE_ADJ[phrase]) np.sizes.push(SIZE_ADJ[phrase]);
        if (DIM_ADJ[phrase]) np.dims.push(DIM_ADJ[phrase]);
        if (SPEED_ADJ[phrase]) np.speeds.push(SPEED_ADJ[phrase]);
        if (MATERIAL_ADJ[phrase]) np.materials.push(MATERIAL_ADJ[phrase]);
        i += n; found = true; break;
      }
      if (c || a) {
        if (n === 1 && (SIZE_ADJ[toks[i]] || MATERIAL_ADJ[toks[i]]) && i < toks.length - 1) break; // "light" etc as adjective before a noun
        matches.push({ start: i, len: n, words: phrase, concept: c, addon: a });
        if (phrase !== sing || /s$/.test(toks[i + n - 1]) && !c?.names.includes(phrase)) np.plural = np.plural || singular(toks[i + n - 1]) !== toks[i + n - 1];
        i += n; found = true; break;
      }
    }
    if (found) continue;
    const w = toks[i];
    if (SIZE_ADJ[w] !== undefined) np.sizes.push(SIZE_ADJ[w]);
    else if (DIM_ADJ[w]) np.dims.push(DIM_ADJ[w]);
    else if (SPEED_ADJ[w]) np.speeds.push(SPEED_ADJ[w]);
    else if (MATERIAL_ADJ[w]) np.materials.push(MATERIAL_ADJ[w]);
    else if (COLOR_WORDS.has(w)) np.colors.push(w);
    else if (ARRANGEMENT_WORDS[w] && toks[i + 1] === 'of') { np.arrangement = ARRANGEMENT_WORDS[w]; i++; }
    else if (IGNORED_ADJ.has(w) || w === 'of' || w === 'kind' || w === 'sort') np.ignoredWords.push(w);
    else remaining.push(w);
    i++;
  }

  // entity references ("that drone", "the turret", "those robots", "the guy")
  const lastTok = toks[toks.length - 1];
  const entKey = ENTITY_NOUNS[lastTok] ? lastTok : ENTITY_NOUNS[singular(lastTok ?? '')] ? singular(lastTok) : null;
  const refDet = np.det && ['the', 'that', 'this', 'those', 'these', 'all', 'every', 'each', 'his', 'her', 'their'].includes(np.det);
  if (entKey && (refDet || !matches.length || toks.length === 1 && !np.det)) {
    const plural = np.plural || /s$/.test(lastTok) && lastTok !== entKey || ['those', 'these', 'all', 'every', 'each'].includes(np.det ?? '') || lastTok !== singular(lastTok);
    const which = toks.includes('nearest') || toks.includes('closest') ? 'nearest' : toks.includes('other') ? 'other' : toks.includes('last') ? 'last' : toks.includes('first') ? 'first' : np.det === 'all' || np.det === 'every' ? 'all' : 'that';
    np.ref = { kind: 'entity', entityType: ENTITY_NOUNS[entKey], plural, noun: entKey, which };
    np.plural = plural;
    if (!(matches.length && !refDet)) return np;
  }

  if (matches.length) {
    // head = rightmost concept match; earlier addon-capable matches become add-ons ("rocket glove")
    const conceptMatches = matches.filter((m) => m.concept);
    const head = conceptMatches[conceptMatches.length - 1] ?? null;
    if (head) {
      np.concept = head.concept;
      np.conceptWords = head.words;
      for (const m of matches) {
        if (m === head) continue;
        if (m.addon) np.addons.push({ addon: m.addon, where: null, count: 1 });
        else np.ignoredWords.push(m.words);
      }
    } else {
      // only add-ons ("rockets", "wheels"): the phrase is add-on material
      for (const m of matches) if (m.addon) np.addons.push({ addon: m.addon, where: null, count: /s$/.test(m.words) ? 2 : 1 });
    }
    // unknown words after the head are probably the real head ("glove thing", "hammer guy")
    np.unknownWords = remaining;
  } else if (remaining.length) {
    const content = remaining.filter((w) => !PLACE_NOUNS.has(w) && !['one', 'ones', 'thing', 'things', 'it', 'up', 'out', 'at', 'to'].includes(w));
    if (PLACE_NOUNS.has(remaining[remaining.length - 1]) && refDet !== undefined) {
      np.ref = { kind: 'place', place: remaining[remaining.length - 1] };
    }
    if (content.length) {
      np.unknownHead = content[content.length - 1];
      np.unknownWords = content;
      if (/s$/.test(np.unknownHead) && !/ss$/.test(np.unknownHead)) np.plural = true;
    }
  }
  if (np.plural && np.count === null && !np.ref) np.count = 2; // "rockets" -> a pair
  if (np.analogy === false && analogies.length) np.analogy = analogies.some((a) => toksIn.join(' ').includes(a.split(' ').slice(-1)[0]));
  return np;
}

export function isVerbWord(w: string) { return !!VERBS[lemma(w)]; }
