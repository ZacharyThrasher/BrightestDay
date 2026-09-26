// Text normalization, WER alignment and keyword accuracy.

const ONES = ['zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ten', 'eleven', 'twelve', 'thirteen', 'fourteen', 'fifteen', 'sixteen', 'seventeen', 'eighteen', 'nineteen'];
const TENS = ['', '', 'twenty', 'thirty', 'forty', 'fifty', 'sixty', 'seventy', 'eighty', 'ninety'];
function numToWords(n) {
  if (n < 20) return ONES[n];
  if (n < 100) return TENS[Math.floor(n / 10)] + (n % 10 ? ' ' + ONES[n % 10] : '');
  if (n < 1000) return ONES[Math.floor(n / 100)] + ' hundred' + (n % 100 ? ' ' + numToWords(n % 100) : '');
  return String(n);
}

export const FILLERS = new Set(['uh', 'um', 'umm', 'uhh', 'er', 'erm', 'ah', 'hmm', 'mm', 'eh']);

// Orthographic variants that the game parser would treat identically.
const COMPOUNDS = [
  [/\bzip line\b/g, 'zipline'], [/\bjet pack\b/g, 'jetpack'], [/\bsee saw\b/g, 'seesaw'],
  [/\bforcefield\b/g, 'force field'], [/\bhover craft\b/g, 'hovercraft'], [/\bflame thrower\b/g, 'flamethrower'],
  [/\brag doll\b/g, 'ragdoll'], [/\bwater melons?\b/g, (m) => m.replace(' ', '')], [/\bok\b/g, 'okay'],
  [/\bspringloaded\b/g, 'spring loaded'], [/\bupside-down\b/g, 'upside down'],
];

export function normalize(s) {
  let t = String(s).toLowerCase();
  t = t.replace(/(\d+)(st|nd|rd|th)\b/g, '$1');
  t = t.replace(/(\d+)\s*(°|degrees?)/g, '$1 degrees');
  t = t.replace(/\d+/g, (d) => ' ' + numToWords(parseInt(d, 10)) + ' ');
  t = t.replace(/[-_/]/g, ' ');
  t = t.replace(/[^a-z' ]+/g, ' ');
  t = t.replace(/'(?![a-z])|(?<![a-z])'/g, ' ');
  t = t.replace(/\s+/g, ' ').trim();
  for (const [re, rep] of COMPOUNDS) t = t.replace(re, rep);
  return t.split(' ').filter((w) => w && !FILLERS.has(w));
}

// Levenshtein alignment. Returns {sub, del, ins, n, ops}, where ops is a list
// of [op, refIndex, hypIndex] with op in {'=', 'S', 'D', 'I'}.
export function align(ref, hyp) {
  const R = ref.length, H = hyp.length;
  const d = Array.from({ length: R + 1 }, () => new Int32Array(H + 1));
  for (let i = 0; i <= R; i++) d[i][0] = i;
  for (let j = 0; j <= H; j++) d[0][j] = j;
  for (let i = 1; i <= R; i++) for (let j = 1; j <= H; j++) {
    const c = ref[i - 1] === hyp[j - 1] ? 0 : 1;
    d[i][j] = Math.min(d[i - 1][j - 1] + c, d[i - 1][j] + 1, d[i][j - 1] + 1);
  }
  const ops = [];
  let i = R, j = H, sub = 0, del = 0, ins = 0;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && d[i][j] === d[i - 1][j - 1] + (ref[i - 1] === hyp[j - 1] ? 0 : 1)) {
      const eq = ref[i - 1] === hyp[j - 1];
      ops.push([eq ? '=' : 'S', i - 1, j - 1]); if (!eq) sub++; i--; j--;
    } else if (i > 0 && d[i][j] === d[i - 1][j] + 1) { ops.push(['D', i - 1, -1]); del++; i--; }
    else { ops.push(['I', -1, j - 1]); ins++; j--; }
  }
  ops.reverse();
  return { sub, del, ins, n: R, ops };
}

// Words that carry no game meaning. Everything else in the reference
// (nouns, verbs, adjectives, numbers, directions) is a "keyword".
export const STOP = new Set(('a an the to of on in at with and that this it its him her me my you your i we us from here there ' +
  'into onto over up is are be so can like now one another all each where was just then them they his please ' +
  'okay').split(' '));

export function keywords(refWords) {
  return refWords.filter((w) => !STOP.has(w));
}

// Keyword accuracy = fraction of reference keywords that are matched ('=')
// in the WER alignment (position-aware, so a word must be recognized in the
// right place, not merely somewhere).
export function keywordHits(ref, hyp) {
  const a = align(ref, hyp);
  let total = 0, hit = 0;
  for (const [op, ri] of a.ops) {
    if (ri < 0 || STOP.has(ref[ri])) continue;
    total++;
    if (op === '=') hit++;
  }
  return { total, hit };
}

// Is ref word k correctly present in hyp (matched in alignment)?
export function matchedRefIndices(ref, hyp) {
  const a = align(ref, hyp);
  const m = new Set();
  for (const [op, ri] of a.ops) if (op === '=') m.add(ri);
  return m;
}
