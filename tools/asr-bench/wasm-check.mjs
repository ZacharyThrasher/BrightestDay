#!/usr/bin/env node
// Runs streaming models through the WASM build of sherpa-onnx (npm `sherpa-onnx`,
// no native addon) and the native addon (npm `sherpa-onnx-node`) on the same
// clips, same settings (1 thread, greedy, fresh stream per clip, 100 ms chunks,
// 400 ms tail padding), and reports RTF, per-chunk decode time and WER.
//
//   node wasm-check.mjs [modelDirName ...] [--clips N]
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { normalize, align } from './lib/text.mjs';

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const models = process.env.ASR_MODELS_DIR || resolve(here, '..', '..', 'models');
const native = require('sherpa-onnx-node');
const args = process.argv.slice(2);
const ci = args.indexOf('--clips');
const nClips = ci >= 0 ? +args[ci + 1] : 64;
const dirs = args.filter((a, i) => !a.startsWith('--') && args[i - 1] !== '--clips');
if (!dirs.length) dirs.push('sherpa-onnx-streaming-zipformer-en-2023-06-21', 'sherpa-onnx-nemo-streaming-fast-conformer-transducer-en-80ms-int8');

const man = JSON.parse(readFileSync(join(here, 'data', 'audio', 'manifest.json'), 'utf8'));
const clips = man.clips.filter((c) => c.voice === 'kok-adam').slice(0, nClips);
const audio = clips.map((c) => native.readWave(join(here, 'data', 'audio', 'clean', c.file)).samples.subarray(0, c.speechEnd + 1600));

function files(d) {
  const ls = require('node:fs').readdirSync(d);
  const p = (re) => join(d, ls.filter((f) => re.test(f)).sort((a, b) => (/int8/.test(b) ? 1 : 0) - (/int8/.test(a) ? 1 : 0))[0]);
  return { encoder: p(/^encoder.*\.onnx$/), decoder: p(/^decoder.*\.onnx$/), joiner: p(/^joiner.*\.onnx$/), tokens: join(d, 'tokens.txt') };
}

function run(label, rec, accept, finish, getText) {
  let errs = 0, n = 0, compute = 0, dur = 0;
  const chunkMs = [];
  for (let k = 0; k < clips.length; k++) {
    const x = audio[k];
    const s = rec.createStream();
    for (let i = 0; i < x.length; i += 1600) {
      const t = performance.now();
      accept(s, x.subarray(i, i + 1600));
      while (rec.isReady(s)) rec.decode(s);
      const dt = performance.now() - t;
      chunkMs.push(dt); compute += dt;
    }
    const t = performance.now();
    finish(s);
    while (rec.isReady(s)) rec.decode(s);
    compute += performance.now() - t;
    const ref = normalize(clips[k].text), hyp = normalize(getText(s));
    const a = align(ref, hyp); errs += a.sub + a.del + a.ins; n += a.n;
    dur += x.length / 16000;
    if (s.free) s.free();
  }
  chunkMs.sort((a, b) => a - b);
  console.log(`${label}: clips=${clips.length} RTF=${(compute / 1000 / dur).toFixed(3)} chunk p50=${chunkMs[chunkMs.length >> 1].toFixed(1)}ms p99=${chunkMs[Math.floor(chunkMs.length * 0.99)].toFixed(1)}ms WER=${((100 * errs) / n).toFixed(1)}%`);
}

let wasm;
try { wasm = require('sherpa-onnx'); } catch (e) { console.error('npm i sherpa-onnx (WASM build) first:', e.message); }
for (const name of dirs) {
  const f = files(join(models, name));
  const pad = new Float32Array(6400);
  const nrec = new native.OnlineRecognizer({ featConfig: { sampleRate: 16000, featureDim: 80 }, modelConfig: { transducer: { encoder: f.encoder, decoder: f.decoder, joiner: f.joiner }, tokens: f.tokens, numThreads: 1, provider: 'cpu', debug: 0 } });
  run(`native ${name}`, nrec, (s, x) => s.acceptWaveform({ sampleRate: 16000, samples: x }), (s) => { s.acceptWaveform({ sampleRate: 16000, samples: pad }); s.inputFinished(); }, (s) => nrec.getResult(s).text);
  if (!wasm) continue;
  const t = performance.now();
  let wrec;
  try {
    wrec = wasm.createOnlineRecognizer({ featConfig: { sampleRate: 16000, featureDim: 80 }, modelConfig: { transducer: { encoder: f.encoder, decoder: f.decoder, joiner: f.joiner }, tokens: f.tokens, numThreads: 1, provider: 'cpu', debug: 0 }, decodingMethod: 'greedy_search' });
  } catch (e) { console.log(`wasm ${name}: FAILED to load: ${e.message}`); continue; }
  console.log(`wasm load ${(performance.now() - t).toFixed(0)} ms, rss ${(process.memoryUsage().rss / 2 ** 20).toFixed(0)} MiB`);
  run(`wasm   ${name}`, wrec, (s, x) => s.acceptWaveform(16000, x), (s) => { s.acceptWaveform(16000, pad); s.inputFinished(); }, (s) => wrec.getResult(s).text);
  wrec.free?.();
}
