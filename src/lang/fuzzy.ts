/** Cheap ASR-error repair: edit distance + a crude phonetic key, only against the game's own vocabulary. */
import { VERBS, SIZE_ADJ, DIM_ADJ, SPEED_ADJ, MATERIAL_ADJ, ENTITY_NOUNS, FILLERS, NUMBERS, IGNORED_ADJ, COLOR_WORDS, PREPOSITIONS, SELF_WORDS, PRONOUNS_CONSTRUCT, PRONOUNS_PERSON, PRONOUNS_PLURAL, PLACE_NOUNS, ARRANGEMENT_WORDS, lemma } from './words.js';
import { conceptByName, addonByName } from './lexicon.js';

/** A few hundred very common English words that must never be "corrected" into game vocabulary. */
const COMMON = 'better best good bad great just like know think see look want give use find tell ask work seem feel try leave call keep let begin show hear play run move live believe hold bring happen write provide sit stand lose pay meet include continue learn change lead understand watch follow stop speak read allow add spend grow open walk win offer remember love consider appear buy wait serve die send expect build stay fall cut reach kill remain suggest raise pass sell require report decide pull time year people way day thing man woman child world life hand part place case week company system program question government number night point home water room mother area money story fact month lot right study book eye job word business issue side kind head house service friend father power hour game line end member law car city community name president team minute idea kid body information back parent face others level office door health person art war history party result morning reason research girl guy moment air teacher force education foot boy age policy process music market sense nation plan college interest death experience effect class control care field development role effort rate heart drug show leader light voice wife police mind price report decision son view relationship town road arm difference value building action model season society tax director position player record paper space ground form event official matter center couple site project activity star table need court oil situation cost industry figure street image phone data picture practice piece land product doctor wall patient worker news test movie north love support technology step baby computer type attention film tree source organization hair window evidence population truth song color store chance bed pressure top other new old big high different small large next early young important few public same able last long great little own least bad sure free real full special easy clear recent certain personal open red difficult available likely short single medical current wrong private past foreign fine common poor natural significant similar hot dead central happy serious ready simple left physical general environmental financial blue democratic dark various entire close legal religious cold final main green nice huge popular traditional cultural really very also still even back much well only then there here now down over too always never often maybe sometimes around again away almost later perhaps probably quite rather soon together yet already enough ever instead maybe somewhere anyway please thanks thank sorry okay yeah cool awesome damn wow oops yes nah nope sure fine totally kinda sorta pretty super whatever though although because since unless until while where when what which who whom whose why how both each either neither every few many most none several some such than that these this those whether upon onto into within without through throughout toward towards under until above across after against along among around before behind below beneath beside besides between beyond during except inside near outside past since till versus via'.split(' ');

const FUNCTION_WORDS = [...COMMON, 'a', 'an', 'the', 'that', 'this', 'those', 'these', 'it', 'and', 'then', 'with', 'of', 'to', 'at', 'on', 'in', 'my', 'your', 'his', 'her', 'them', 'him', 'me', 'is', 'are', 'be', 'so', 'as', 'or', 'not', 'no', 'all', 'one', 'some', 'there', 'here', 'where', 'when', 'into', 'onto', 'can', 'get', 'got', 'its', 'if', 'for', 'from', 'way', 'side', 'up', 'down', 'back', 'out', 'off', 'over', 'around', 'what', 'which', 'who', 'how', 'more', 'less', 'again', 'too', 'also', 'same', 'other', 'another', 'each', 'every', 'both', 'last', 'first', 'next', 'nearest', 'closest', 'biggest', 'guy', 'thing', 'things', 'something', 'anything', 'everything', 'lol', 'haha', 'oops', 'huh', 'about', 'right', 'left', 'front', 'top', 'bottom', 'end', 'tip', 'sides', 'am', 'was', 'were', 'has', 'have', 'had', 'do', 'does', 'did', 'will', 'would', 'should', 'let', 'go', 'going', 'want', 'need', 'think', 'see', 'look', 'looking', 'aiming', 'try', 'times', 'kind', 'sort', 'type', 'bit', 'little', 'lot', 'very', 'really', 'super', 'extra', 'even', 'much', 'many', 'just', 'only', 'still', 'already', 'yet', 'now', 'fall', 'falling', 'fell', 'coming', 'come', 'shooting', 'firing', 'bullets', 'bullet', 'shots', 'shot', 'lasers', 'projectiles', 'fire', 'attack', 'incoming', 'hurry', 'fast', 'help', 'save', 'safe', 'safely', 'gently', 'quick', 'die', 'dying', 'hurt', 'ground', 'air', 'sky', 'building', 'wall', 'door', 'gap', 'tower', 'roof', 'ledge'];

let vocab: string[] | null = null;
let known: Set<string> | null = null;
let phon: Map<string, string[]> | null = null;

function build() {
  const words = new Set<string>();
  const addAll = (xs: Iterable<string>) => { for (const x of xs) for (const w of x.split(' ')) if (w.length >= 3) words.add(w); };
  addAll(Object.keys(VERBS)); addAll(conceptByName.keys()); addAll(addonByName.keys()); addAll(Object.keys(SIZE_ADJ)); addAll(Object.keys(DIM_ADJ));
  addAll(Object.keys(SPEED_ADJ)); addAll(Object.keys(MATERIAL_ADJ)); addAll(Object.keys(ENTITY_NOUNS)); addAll(Object.keys(NUMBERS)); addAll(PLACE_NOUNS); addAll(Object.keys(ARRANGEMENT_WORDS));
  vocab = [...words];
  known = new Set([...words, ...FILLERS, ...IGNORED_ADJ, ...COLOR_WORDS, ...PREPOSITIONS.flatMap((p) => p.split(' ')), ...SELF_WORDS, ...PRONOUNS_CONSTRUCT, ...PRONOUNS_PERSON, ...PRONOUNS_PLURAL, ...FUNCTION_WORDS]);
  phon = new Map();
  for (const w of vocab) { const k = phonetic(w); if (!phon.has(k)) phon.set(k, []); phon.get(k)!.push(w); }
}

export function isKnownWord(w: string): boolean {
  if (!known) build();
  if (known!.has(w) || /^\d+$/.test(w)) return true;
  const l = lemma(w);
  if (known!.has(l)) return true;
  if (w.endsWith('s') && known!.has(w.slice(0, -1))) return true;
  if (w.endsWith('es') && known!.has(w.slice(0, -2))) return true;
  return false;
}

export function damerau(a: string, b: string): number {
  const d: number[][] = [];
  for (let i = 0; i <= a.length; i++) { d[i] = [i]; }
  for (let j = 0; j <= b.length; j++) d[0][j] = j;
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost);
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) d[i][j] = Math.min(d[i][j], d[i - 2][j - 2] + 1);
    }
  }
  return d[a.length][b.length];
}

/** Very small phonetic key (metaphone-flavoured): collapses common sound-alike spellings. */
export function phonetic(w: string): string {
  let s = w.toLowerCase().replace(/[^a-z]/g, '');
  s = s.replace(/^kn|^gn|^pn|^wr/, (m) => m[1]).replace(/^x/, 's').replace(/^wh/, 'w');
  s = s.replace(/ph/g, 'f').replace(/ck/g, 'k').replace(/sch/g, 'sk').replace(/tch/g, 'ch').replace(/sh/g, 'x').replace(/ch/g, 'x').replace(/th/g, '0');
  s = s.replace(/c(?=[iey])/g, 's').replace(/c/g, 'k').replace(/q/g, 'k').replace(/x/g, 'ks').replace(/z/g, 's').replace(/dg(?=[iey])/g, 'j').replace(/g(?=[iey])/g, 'j');
  s = s.replace(/v/g, 'f').replace(/ks/g, 's').replace(/nts$/, 'ns').replace(/ts$/, 's');
  const first = s[0] ?? '';
  s = first + s.slice(1).replace(/[aeiouyhw]/g, '');
  s = s.replace(/(.)\1+/g, '$1');
  return s.replace(/s$/, '');
}

function phoneticDist(a: string, b: string) { return damerau(phonetic(a), phonetic(b)); }

export interface Correction { from: string; to: string; how: 'edit' | 'phonetic' }

/**
 * Repair tokens. mode 'unknown' only touches words that aren't in the game vocabulary;
 * mode 'aggressive' also re-reads known words phonetically (used when a clause made no sense).
 */
export function repair(tokens: string[], mode: 'unknown' | 'aggressive' = 'unknown'): { tokens: string[]; corrections: Correction[] } {
  if (!vocab) build();
  const out: string[] = [];
  const corrections: Correction[] = [];
  for (const w of tokens) {
    if (w.length < 3 || /^\d+$/.test(w) || !/^[a-z']+$/.test(w)) { out.push(w); continue; }
    const isKnown = isKnownWord(w);
    if (isKnown && mode === 'unknown') { out.push(w); continue; }
    if (isKnown && mode === 'aggressive' && (FUNCTION_WORDS.includes(w) || FILLERS.has(w) || conceptByName.has(w) || VERBS[lemma(w)])) { out.push(w); continue; }
    let best: string | null = null;
    let bestD = Infinity;
    const maxD = w.length >= 8 ? 2 : w.length >= 5 ? (mode === 'aggressive' ? 2 : 1) : 1;
    for (const v of vocab!) {
      if (Math.abs(v.length - w.length) > maxD) continue;
      const d = damerau(w, v);
      // ties: prefer the same first letter (ASR keeps initial sounds), then the closer sound, then length
      const better = d < bestD || (d === bestD && best && (
        (v[0] === w[0]) !== (best[0] === w[0]) ? v[0] === w[0]
          : phoneticDist(w, v) !== phoneticDist(w, best) ? phoneticDist(w, v) < phoneticDist(w, best)
          : Math.abs(v.length - w.length) < Math.abs(best.length - w.length)));
      if (better) { bestD = d; best = v; }
    }
    if (w.length === 3 && bestD > 1) best = null;
    if (best && bestD <= maxD) { out.push(best); corrections.push({ from: w, to: best, how: 'edit' }); continue; }
    const cands = phon!.get(phonetic(w));
    if (cands && cands.length) {
      const pick = cands.sort((a, b) => damerau(w, a) - damerau(w, b))[0];
      if (damerau(w, pick) <= Math.max(2, Math.ceil(w.length / 3))) { out.push(pick); corrections.push({ from: w, to: pick, how: 'phonetic' }); continue; }
    }
    out.push(w);
  }
  return { tokens: out, corrections };
}
