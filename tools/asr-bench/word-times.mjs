#!/usr/bin/env node
// Estimates the end time of every reference word in every clean clip, used as
// the anchor for per-word partial latency. TTS here gives no word timings, so
// we run Parakeet-TDT-0.6B-v2 (the most accurate model in the set; TDT gives
// token start + duration), align its words to the reference, and take
// end = start + duration of the word's last token (80 ms resolution).
// Reference words the aligner could not match are linearly interpolated
// between their neighbours. Output: data/audio/word-times.json
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { normalize, align } from './lib/text.mjs';

const require = createRequire(import.meta.url);
const sherpa = require('sherpa-onnx-node');
const here = dirname(fileURLToPath(import.meta.url));
const models = process.env.ASR_MODELS_DIR || resolve(here, '..', '..', 'models');
const d = join(models, 'sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8');
const rec = new sherpa.OfflineRecognizer({
  featConfig: { sampleRate: 16000, featureDim: 80 },
  modelConfig: { transducer: { encoder: join(d, 'encoder.int8.onnx'), decoder: join(d, 'decoder.int8.onnx'), joiner: join(d, 'joiner.int8.onnx') }, tokens: join(d, 'tokens.txt'), numThreads: 4, modelType: 'nemo_transducer', debug: 0 },
});
const man = JSON.parse(readFileSync(join(here, 'data', 'audio', 'manifest.json'), 'utf8'));
const out = {};
let matched = 0, total = 0;
for (const c of man.clips) {
  const w = sherpa.readWave(join(here, 'data', 'audio', 'clean', c.file));
  const s = rec.createStream();
  s.acceptWaveform(w);
  rec.decode(s);
  const r = rec.getResult(s);
  // group tokens into words (a token starting with a space starts a new word)
  const hw = [];
  r.tokens.forEach((tok, i) => {
    const end = r.timestamps[i] + (r.durations?.[i] ?? 0.08);
    if (/^\s/.test(tok) || !hw.length) hw.push({ text: tok.trim(), end });
    else { hw[hw.length - 1].text += tok; hw[hw.length - 1].end = end; }
  });
  const hyp = [], hypEnd = [];
  for (const x of hw) for (const nw of normalize(x.text)) { hyp.push(nw); hypEnd.push(x.end); }
  const ref = normalize(c.text);
  const a = align(ref, hyp);
  const ends = new Array(ref.length).fill(null);
  for (const [op, ri, hi] of a.ops) if ((op === '=' || op === 'S') && ri >= 0) ends[ri] = hypEnd[hi];
  for (const [op, ri] of a.ops) if (op === '=') matched++;
  total += ref.length;
  const speechStart = c.speechStart / 16000, speechEnd = c.speechEnd / 16000;
  for (let k = 0; k < ref.length; k++) {
    if (ends[k] != null) continue;
    let p = k - 1; while (p >= 0 && ends[p] == null) p--;
    let n = k + 1; while (n < ref.length && ends[n] == null) n++;
    const t0 = p >= 0 ? ends[p] : speechStart, t1 = n < ref.length ? ends[n] : speechEnd;
    const i0 = p >= 0 ? p : -1, i1 = n < ref.length ? n : ref.length;
    ends[k] = t0 + ((t1 - t0) * (k - i0)) / (i1 - i0);
  }
  // never later than the energy-based end of speech
  out[c.file] = ref.map((word, k) => ({ word, end: +Math.min(ends[k], speechEnd).toFixed(3) }));
}
writeFileSync(join(here, 'data', 'audio', 'word-times.json'), JSON.stringify(out));
console.log(`word times for ${man.clips.length} clips; ${matched}/${total} ref words directly matched by parakeet (${((100 * matched) / total).toFixed(1)}%), rest interpolated/substitution-aligned`);
