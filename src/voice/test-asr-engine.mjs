#!/usr/bin/env node
// Demo / smoke test for asr-engine.mjs: streams a 16 kHz mono WAV through the
// engine in 100 ms chunks at (simulated) real-time pacing and prints partials
// and the final.
//
//   node src/voice/test-asr-engine.mjs <file.wav> [preset] [--realtime] [--hands-free] [--threads N]
//
// presets (model dirs are resolved under <repo>/models or $ASR_MODELS_DIR):
//   zipformer-20M (default), zipformer-0621, nemo-fc-80ms, nemo-fc-480ms, kroko-en,
//   moonshine-tiny, moonshine-base, moonshine2-tiny, moonshine2-base,
//   whisper-tiny.en, whisper-base.en, parakeet-tdt-v2, two-pass
import { createRequire } from 'node:module';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createAsrEngine } from './asr-engine.mjs';
import { PRESETS, GAME_HOTWORDS } from './asr-presets.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const models = process.env.ASR_MODELS_DIR || join(repo, 'models');

let sherpa;
try { sherpa = createRequire(import.meta.url)('sherpa-onnx-node'); }
catch { sherpa = createRequire(join(repo, 'tools', 'asr-bench', 'package.json'))('sherpa-onnx-node'); }

const args = process.argv.slice(2);
const wav = args.find((a) => a.endsWith('.wav'));
if (!wav) { console.error('usage: test-asr-engine.mjs <file.wav> [preset] [--realtime] [--hands-free] [--threads N] [--no-hotwords]'); process.exit(2); }
const presetName = args.find((a) => PRESETS[a]) ?? 'zipformer-20M';
const realtime = args.includes('--realtime');
const handsFree = args.includes('--hands-free');
const ti = args.indexOf('--threads');
const threads = ti >= 0 ? +args[ti + 1] : 2;
const hotwords = args.includes('--no-hotwords') ? [] : GAME_HOTWORDS;

const opts = PRESETS[presetName](models, { threads, hotwords });
const t0 = performance.now();
const asr = createAsrEngine({ ...opts, sherpa, endpoint: { enabled: handsFree } });
console.log(`[${presetName}] loaded in ${(performance.now() - t0).toFixed(0)} ms`, asr.info);

const w = sherpa.readWave(wav);
if (w.sampleRate !== 16000) throw new Error('expects 16 kHz mono wav');
const chunk = 1600;
const start = performance.now();
const ms = () => (performance.now() - start).toFixed(0).padStart(5);
asr.onPartial((e) => console.log(`${ms()} ms  partial: ${e.text}`));
asr.onFinal((e) => console.log(`${ms()} ms  FINAL(${e.reason}, decode ${e.decodeMs.toFixed(0)} ms): ${e.text}`));

for (let i = 0; i < w.samples.length; i += chunk) {
  if (realtime) { const due = ((i + chunk) / 16000) * 1000; const wait = due - (performance.now() - start); if (wait > 0) await new Promise((r) => setTimeout(r, wait)); }
  asr.acceptPcm(w.samples.subarray(i, i + chunk));
}
if (!handsFree) asr.endUtterance();
console.log(`audio ${(w.samples.length / 16).toFixed(0)} ms, rss ${(process.memoryUsage().rss / 2 ** 20).toFixed(0)} MiB`);
asr.dispose();
