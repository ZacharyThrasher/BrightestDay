// Static phrase embedder for System-1 "unknown word -> nearest known concept" fallback.
//
// Plain ESM, zero dependencies, runs in Node >= 18 and in a browser / Electron renderer.
// Loads a "BDSV" artifact built by tools/semantic/build_artifact.py (int8 vectors with a per-row
// float32 scale, rows L2-normalised; optional multi-word terms joined with '_').
//
//   import { loadEmbedder } from './embed.mjs';
//   const emb = await loadEmbedder('models/semantic/bd-semantic-nb19-int8.bin'); // path | URL | ArrayBuffer | Uint8Array
//   emb.nearest('pogo stick', [{ id: 'spring', text: 'spring' }, { id: 'ramp', text: ['ramp', 'incline'] }], 3)
//     -> [{ id: 'spring', score: 0.41 }, ...]
//
// Phrase embedding: lowercase, drop function words, then greedy longest-match over multi-word
// vocabulary terms (up to 4 words, e.g. "giant boxing glove" -> ["giant", "boxing_glove"]), falling
// back to naive singularisation for unknown words; the phrase vector is the normalised mean of the
// segment vectors (the last segment -- usually the English head noun -- can be up-weighted).

const MAGIC = 0x56534442; // 'BDSV' little-endian
const STOP = new Set(['a', 'an', 'the', 'of', 'with', 'on', 'in', 'to', 'for', 'and', 'some', 'my', 'your',
  'that', 'this', 'these', 'those', 'its', 'it', 'at', 'from', 'by', 'into', 'onto', 'kind', 'sort', 'like']);

async function readSource(src) {
  if (src instanceof ArrayBuffer) return new Uint8Array(src);
  if (ArrayBuffer.isView(src)) return new Uint8Array(src.buffer, src.byteOffset, src.byteLength);
  const isNode = typeof process !== 'undefined' && !!process.versions?.node && typeof window === 'undefined';
  const str = src instanceof URL ? src.href : String(src);
  if (isNode && (str.startsWith('file:') || !/^[a-z][a-z0-9+.-]*:/i.test(str) || /^[a-z]:[\\/]/i.test(str))) {
    const fsName = 'node:fs/promises'; // non-literal so browser bundlers leave it alone
    const urlName = 'node:url';
    const { readFile } = await import(/* @vite-ignore */ fsName);
    let p = str;
    if (str.startsWith('file:')) p = (await import(/* @vite-ignore */ urlName)).fileURLToPath(str);
    return new Uint8Array(await readFile(p));
  }
  const r = await fetch(str);
  if (!r.ok) throw new Error(`loadEmbedder: HTTP ${r.status} for ${str}`);
  return new Uint8Array(await r.arrayBuffer());
}

function parse(bytes) {
  // copy to a fresh, aligned ArrayBuffer if needed (Float32Array needs 4-byte alignment)
  if (bytes.byteOffset % 4 !== 0) bytes = bytes.slice();
  const buf = bytes.buffer, base = bytes.byteOffset;
  const dv = new DataView(buf, base, bytes.byteLength);
  if (dv.getUint32(0, true) !== MAGIC) throw new Error('loadEmbedder: not a BDSV artifact');
  const hlen = dv.getUint32(4, true);
  const td = new TextDecoder();
  const header = JSON.parse(td.decode(bytes.subarray(8, 8 + hlen)));
  let off = 8 + hlen; off += (4 - (off % 4)) % 4;
  const vlen = dv.getUint32(off, true); off += 4;
  const terms = td.decode(bytes.subarray(off, off + vlen)).split('\n');
  off += vlen; off += (4 - (off % 4)) % 4;
  const { dims, count } = header;
  if (terms.length !== count) throw new Error('loadEmbedder: vocab/count mismatch');
  const scales = new Float32Array(buf, base + off, count); off += count * 4;
  const data = new Int8Array(buf, base + off, count * dims);
  return { header, terms, scales, data };
}

export function normalizeText(text) {
  return String(text).toLowerCase().replace(/[‘’]/g, "'").replace(/[^a-z0-9'\s-]+/g, ' ')
    .replace(/\s+/g, ' ').trim();
}

export async function loadEmbedder(src, opts = {}) {
  const { header, terms, scales, data } = parse(await readSource(src));
  const { dims, count } = header;
  const J = header.phraseJoiner || '_';
  const maxN = header.phraseJoiner ? (opts.maxNgram ?? 4) : 1;
  const headWeight = opts.headWeight ?? 1;
  const row = new Map();
  for (let i = 0; i < count; i++) row.set(terms[i], i);

  function addRow(acc, i, w) {
    const s = scales[i] * w, o = i * dims;
    for (let d = 0; d < dims; d++) acc[d] += data[o + d] * s;
  }
  function lookup(tok) {
    let i = row.get(tok);
    if (i !== undefined) return i;
    if (tok.length > 3) { // naive singularisation for unknown plurals
      if (tok.endsWith('ies')) i = row.get(tok.slice(0, -3) + 'y');
      if (i === undefined && tok.endsWith('es')) i = row.get(tok.slice(0, -2));
      if (i === undefined && tok.endsWith('s')) i = row.get(tok.slice(0, -1));
    }
    if (i === undefined) i = row.get(tok + 's'); // vocab may only hold the plural ("fireworks")
    return i;
  }
  /** Rows (with the matched vocabulary term) that make up a phrase. */
  function segments(text) {
    const norm = normalizeText(text);
    if (!norm) return [];
    const whole = row.get(norm.replace(/[\s-]+/g, J)) ?? row.get(norm);
    if (whole !== undefined) return [{ term: terms[whole], row: whole }];
    const toks = norm.split(/[\s-]+/).filter((t) => t && !STOP.has(t));
    const out = [];
    for (let k = 0; k < toks.length;) {
      let hit = -1, n = Math.min(maxN, toks.length - k);
      for (; n > 1; n--) {
        const i = row.get(toks.slice(k, k + n).join(J));
        if (i !== undefined) { hit = i; break; }
      }
      if (hit < 0) { n = 1; const i = lookup(toks[k]); if (i !== undefined) hit = i; }
      if (hit >= 0) out.push({ term: terms[hit], row: hit });
      k += n;
    }
    return out;
  }
  function embedPhrase(text) {
    const segs = segments(text);
    if (!segs.length) return null;
    const v = new Float32Array(dims);
    segs.forEach((s, j) => addRow(v, s.row, j === segs.length - 1 ? headWeight : 1));
    let n = 0;
    for (let d = 0; d < dims; d++) n += v[d] * v[d];
    n = Math.sqrt(n) || 1;
    for (let d = 0; d < dims; d++) v[d] /= n;
    return v;
  }
  const dot = (a, b) => { let s = 0; for (let d = 0; d < dims; d++) s += a[d] * b[d]; return s; };

  const indexCache = new WeakMap();
  /** Pre-embed candidates once; reused automatically when the same array is passed again. */
  function index(candidates) {
    let ix = indexCache.get(candidates);
    if (ix) return ix;
    const ids = [], vecs = [], missing = [];
    for (const c of candidates) {
      const texts = Array.isArray(c.text) ? c.text : [c.text ?? c.id];
      let any = false;
      for (const t of texts) {
        const v = embedPhrase(t);
        if (v) { ids.push(c.id); vecs.push(v); any = true; }
      }
      if (!any) missing.push(c.id);
    }
    ix = { ids, vecs, missing };
    indexCache.set(candidates, ix);
    return ix;
  }
  /** Top-k candidate ids by cosine (max over each id's texts). Returns [] if nothing in `text` is known. */
  function nearest(text, candidates, k = 5) {
    const q = typeof text === 'string' ? embedPhrase(text) : text;
    if (!q) return [];
    const { ids, vecs } = index(candidates);
    const best = new Map();
    for (let j = 0; j < ids.length; j++) {
      const s = dot(q, vecs[j]);
      if (!(best.get(ids[j]) >= s)) best.set(ids[j], s);
    }
    return [...best].map(([id, score]) => ({ id, score })).sort((a, b) => b.score - a.score).slice(0, k);
  }
  function similarity(a, b) {
    const va = embedPhrase(a), vb = embedPhrase(b);
    return va && vb ? dot(va, vb) : null;
  }
  function has(word) {
    const n = normalizeText(word);
    return row.has(n) || row.has(n.replace(/[\s-]+/g, J));
  }
  return { embedPhrase, nearest, index, similarity, has, segments, dims, size: count, meta: header };
}
