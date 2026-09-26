#!/usr/bin/env node
// Embeds every test phrase + concept alias with the TF.js Universal Sentence Encoder (lite, 8k
// sentencepiece vocab) loaded from models/semantic/raw/use-lite/, measures per-phrase latency, and
// writes models/semantic/cache/use_lite_embeddings.json for bench_embed.py to score.
//   node tools/semantic/bench_use.mjs [--backend cpu|wasm]
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as tf from '@tensorflow/tfjs';
import { Tokenizer } from '@tensorflow-models/universal-sentence-encoder';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const dir = join(repo, 'models', 'semantic', 'raw', 'use-lite');
const cache = join(repo, 'models', 'semantic', 'cache');
mkdirSync(cache, { recursive: true });

await tf.setBackend('cpu');
const mj = JSON.parse(readFileSync(join(dir, 'model.json'), 'utf8'));
const specs = mj.weightsManifest.flatMap((g) => g.weights);
const bufs = mj.weightsManifest.flatMap((g) => g.paths.map((p) => readFileSync(join(dir, p))));
const data = Buffer.concat(bufs);
const model = await tf.loadGraphModel(tf.io.fromMemory({
  modelTopology: mj.modelTopology, weightSpecs: specs,
  weightData: data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength),
}));
const tok = new Tokenizer(JSON.parse(readFileSync(join(dir, 'vocab.json'), 'utf8')));

async function embed(text) {
  const enc = tok.encode(text);
  const indices = tf.tensor2d(enc.map((_, i) => [0, i]), [enc.length, 2], 'int32');
  const values = tf.tensor1d(enc, 'int32');
  const out = await model.executeAsync({ indices, values });
  const v = Array.from(await out.data());
  indices.dispose(); values.dispose(); out.dispose();
  return v;
}

const concepts = JSON.parse(readFileSync(join(here, 'concepts.json'), 'utf8')).concepts;
const items = JSON.parse(readFileSync(join(here, 'testset.json'), 'utf8')).items;
const vectors = {};
for (const c of concepts) for (const a of c.slice(1)) vectors[a] = await embed(a);
await embed('warm up');
const lat = [];
for (const [p] of items) {
  const t = performance.now();
  vectors[p] = await embed(p);
  lat.push(performance.now() - t);
}
lat.sort((a, b) => a - b);
const mean = lat.reduce((a, b) => a + b, 0) / lat.length;
const p95 = lat[Math.floor(lat.length * 0.95)];
writeFileSync(join(cache, 'use_lite_embeddings.json'), JSON.stringify({ backend: tf.getBackend(), lat_ms_mean: mean, lat_ms_p95: p95, vectors }));
console.log(`USE-lite backend=${tf.getBackend()} embed latency mean=${mean.toFixed(2)}ms p95=${p95.toFixed(2)}ms (${items.length} phrases)`);
