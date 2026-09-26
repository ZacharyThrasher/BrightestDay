// node --test src/semantic/embed.test.mjs
// Always runs against a tiny synthetic BDSV buffer; additionally checks the real artifact when
// models/semantic/bd-semantic-nb19-int8.bin has been built (tools/semantic/build_artifact.py).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { loadEmbedder, normalizeText } from './embed.mjs';

function buildBDSV(terms, vecs, joiner = '_') {
  const dims = vecs[0].length, count = terms.length, enc = new TextEncoder();
  const hb = enc.encode(JSON.stringify({ version: 1, dims, count, quant: 'int8-rowscale', phraseJoiner: joiner }));
  const vb = enc.encode(terms.join('\n'));
  const pad = (n) => (4 - (n % 4)) % 4;
  let off = 8 + hb.length; off += pad(off);
  const vOff = off; off += 4 + vb.length; off += pad(off);
  const sOff = off; off += count * 4;
  const dOff = off; off += count * dims;
  const buf = new ArrayBuffer(off), dv = new DataView(buf), u8 = new Uint8Array(buf);
  dv.setUint32(0, 0x56534442, true); dv.setUint32(4, hb.length, true); u8.set(hb, 8);
  dv.setUint32(vOff, vb.length, true); u8.set(vb, vOff + 4);
  const sc = new Float32Array(buf, sOff, count), q = new Int8Array(buf, dOff, count * dims);
  vecs.forEach((v, i) => {
    const n = Math.hypot(...v), m = Math.max(...v.map((x) => Math.abs(x / n)));
    sc[i] = m / 127; v.forEach((x, d) => { q[i * dims + d] = Math.round(x / n / sc[i]); });
  });
  return buf;
}

const TERMS = ['spring', 'stick', 'pogo_stick', 'ramp', 'board', 'glove', 'boxing_glove', 'giant', 'rockets'];
const VECS = [[1, 0, 0, 0], [0, 1, 0, 0], [0.9, 0.3, 0, 0], [0, 0, 1, 0], [0, 0.1, 0.9, 0.1], [0, 0, 0, 1], [0.1, 0, 0, 1], [0.5, 0.5, 0.5, 0.5], [0.2, 0, 0, 0.1]];

test('normalizeText', () => {
  assert.equal(normalizeText('  A Pogo-Stick!! '), 'a pogo-stick');
});

test('synthetic artifact: lookup, phrases, nearest, has', async () => {
  const buf = buildBDSV(TERMS, VECS);
  for (const src of [buf, new Uint8Array(buf)]) {
    const e = await loadEmbedder(src);
    assert.equal(e.dims, 4);
    assert.equal(e.size, TERMS.length);
    assert.ok(e.has('pogo stick') && e.has('Pogo-Stick') && e.has('ramp') && !e.has('hoverboard'));
    assert.deepEqual(e.segments('pogo stick').map((s) => s.term), ['pogo_stick']);
    assert.deepEqual(e.segments('a giant boxing glove with rockets').map((s) => s.term), ['giant', 'boxing_glove', 'rockets']);
    assert.deepEqual(e.segments('springs').map((s) => s.term), ['spring']); // singularised
    assert.deepEqual(e.segments('rocket').map((s) => s.term), ['rockets']); // pluralised
    assert.equal(e.embedPhrase('zzz qqq'), null);
    const v = e.embedPhrase('pogo stick');
    assert.ok(v instanceof Float32Array && Math.abs(Math.hypot(...v) - 1) < 1e-5);
    const cands = [{ id: 'spring', text: 'spring' }, { id: 'ramp', text: ['ramp', 'board'] }, { id: 'glove', text: 'glove' }];
    const r = e.nearest('pogo stick', cands, 2);
    assert.equal(r.length, 2);
    assert.equal(r[0].id, 'spring');
    assert.ok(r[0].score > r[1].score);
    assert.equal(e.nearest('boxing glove', cands, 1)[0].id, 'glove');
    assert.deepEqual(e.nearest('zzz', cands), []);
    assert.ok(e.similarity('spring', 'spring') > 0.99);
  }
});

test('rejects non-BDSV input', async () => {
  await assert.rejects(() => loadEmbedder(new Uint8Array(64)), /not a BDSV/);
});

const real = fileURLToPath(new URL('../../models/semantic/bd-semantic-nb19-int8.bin', import.meta.url));
test('real artifact (if built): sensible neighbours, fast queries', { skip: !existsSync(real) && 'artifact not built' }, async () => {
  const e = await loadEmbedder(real); // also accepts a file:// URL, http(s) URL or ArrayBuffer
  const cands = ['ramp', 'hammer', 'glove', 'spring', 'cart', 'boat', 'saw', 'cage', 'rocket', 'magnet', 'fan', 'shield']
    .map((id) => ({ id, text: id }));
  const top = (p) => e.nearest(p, cands, 1)[0]?.id;
  assert.equal(top('sledgehammer'), 'hammer');
  assert.equal(top('shopping cart'), 'cart');
  assert.equal(top('canoe'), 'boat');
  assert.equal(top('chainsaw'), 'saw');
  assert.equal(top('electromagnet'), 'magnet');
  const t = performance.now();
  for (let i = 0; i < 200; i++) e.nearest('baggage carousel', cands, 3);
  assert.ok((performance.now() - t) / 200 < 10, 'query should be well under 10 ms');
});
