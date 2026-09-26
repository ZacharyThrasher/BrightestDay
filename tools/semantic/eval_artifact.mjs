#!/usr/bin/env node
// Scores a built BDSV artifact through the shipped JS module (src/semantic/embed.mjs) on the
// labelled test set, and times load / first query / steady-state queries.
//   node tools/semantic/eval_artifact.mjs [artifact.bin ...] [--head 2] [--json out.json]
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join, dirname, resolve, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadEmbedder } from '../../src/semantic/embed.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const args = process.argv.slice(2);
const opt = (k, d) => (args.includes(k) ? args[args.indexOf(k) + 1] : d);
const skip = new Set([opt('--head'), opt('--json')].filter(Boolean));
let files = args.filter((a) => !a.startsWith('--') && !skip.has(a));
if (!files.length) files = readdirSync(join(repo, 'models', 'semantic')).filter((f) => f.endsWith('.bin')).map((f) => join(repo, 'models', 'semantic', f));
const headWeight = Number(opt('--head', 1));

const concepts = JSON.parse(readFileSync(join(here, 'concepts.json'), 'utf8')).concepts;
const items = JSON.parse(readFileSync(join(here, 'testset.json'), 'utf8')).items;
const candidates = concepts.map(([id, ...aliases]) => ({ id, text: aliases }));
const results = [];
for (const f of files) {
  let t = performance.now();
  const emb = await loadEmbedder(f, { headWeight });
  const loadMs = performance.now() - t;
  t = performance.now();
  emb.nearest('warm up', candidates, 3);
  const firstMs = performance.now() - t;
  let top1 = 0, top3 = 0, none = 0;
  const lat = [], misses = [];
  for (const [phrase, accept] of items) {
    t = performance.now();
    const r = emb.nearest(phrase, candidates, 3);
    lat.push(performance.now() - t);
    if (!r.length) { none++; misses.push([phrase, null]); continue; }
    if (accept.includes(r[0].id)) top1++; else misses.push([phrase, r.map((x) => `${x.id}:${x.score.toFixed(2)}`).join(' ')]);
    if (r.some((x) => accept.includes(x.id))) top3++;
  }
  lat.sort((a, b) => a - b);
  const mean = lat.reduce((a, b) => a + b, 0) / lat.length;
  const res = { file: basename(f), terms: emb.size, dims: emb.dims, license: emb.meta.license, headWeight,
    top1: top1 / items.length, top3: top3 / items.length, none, loadMs, firstQueryMs: firstMs,
    queryMsMean: mean, queryMsP95: lat[Math.floor(lat.length * 0.95)], misses };
  results.push(res);
  console.log(`${res.file.padEnd(30)} terms=${res.terms} top1=${res.top1.toFixed(3)} top3=${res.top3.toFixed(3)} none=${none} ` +
    `load=${loadMs.toFixed(0)}ms firstQuery(index build)=${firstMs.toFixed(1)}ms query mean=${mean.toFixed(3)}ms p95=${res.queryMsP95.toFixed(3)}ms`);
}
if (opt('--json')) writeFileSync(opt('--json'), JSON.stringify(results, null, 1));
