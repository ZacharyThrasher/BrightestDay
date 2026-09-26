#!/usr/bin/env node
// Synthesizes the test set with sherpa-onnx offline TTS (Kokoro + Piper VITS),
// resamples to 16 kHz mono, pads with silence and writes a clean and a noisy
// (light reverb + pink/white noise at 10-15 dB SNR) copy of every utterance.
//
//   node gen-audio.mjs            -> data/audio/{clean,noisy}/<voice>/<id>.wav + data/audio/manifest.json
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mulberry32, resample, pinkNoise, whiteNoise, reverb, mixAtSnr, speechBounds, normalizePeak, concat } from './lib/audio.mjs';

const require = createRequire(import.meta.url);
const sherpa = require('sherpa-onnx-node');
const here = dirname(fileURLToPath(import.meta.url));
const models = process.env.ASR_MODELS_DIR || resolve(here, '..', '..', 'models');
const out = join(here, 'data', 'audio');
const SR = 16000;
const LEAD = 0.3; // s of silence before speech (push-to-talk press -> first word)
const TRAIL = 1.5; // s of silence after speech (lets us measure endpointing)

const { utterances } = JSON.parse(readFileSync(join(here, 'utterances.json'), 'utf8'));

function piper(name) {
  const d = join(models, name);
  const onnx = name.replace(/^vits-piper-/, '') + '.onnx';
  return new sherpa.OfflineTts({
    model: { vits: { model: join(d, onnx), tokens: join(d, 'tokens.txt'), dataDir: join(d, 'espeak-ng-data') }, numThreads: 2, provider: 'cpu', debug: 0 },
    maxNumSentences: 1,
  });
}
function kokoro() {
  const d = join(models, 'kokoro-en-v0_19');
  return new sherpa.OfflineTts({
    model: { kokoro: { model: join(d, 'model.onnx'), voices: join(d, 'voices.bin'), tokens: join(d, 'tokens.txt'), dataDir: join(d, 'espeak-ng-data') }, numThreads: 4, provider: 'cpu', debug: 0 },
    maxNumSentences: 1,
  });
}

// Voice conditions. libritts_r rotates through many of its 904 speakers.
const voices = [
  { id: 'kok-adam', desc: 'Kokoro am_adam (US male), speed 1.0', engine: 'kokoro', sid: () => 5, speed: 1.0 },
  { id: 'kok-sarah', desc: 'Kokoro af_sarah (US female), speed 1.15', engine: 'kokoro', sid: () => 3, speed: 1.15 },
  { id: 'kok-george', desc: 'Kokoro bm_george (GB male), speed 0.9', engine: 'kokoro', sid: () => 9, speed: 0.9 },
  { id: 'libritts', desc: 'Piper libritts_r medium, speaker = (37*i+11) mod 904 (many US speakers), speed 1.0', engine: 'vits-piper-en_US-libritts_r-medium', sid: (i) => (37 * i + 11) % 904, speed: 1.0 },
  { id: 'alan-fast', desc: 'Piper en_GB alan medium (GB male), speed 1.25 (fast)', engine: 'vits-piper-en_GB-alan-medium', sid: () => 0, speed: 1.25 },
  { id: 'amy-low', desc: 'Piper en_US amy low (US female, 16 kHz low quality), speed 1.0', engine: 'vits-piper-en_US-amy-low', sid: () => 0, speed: 1.0 },
];

const engines = {};
const manifest = { sampleRate: SR, lead: LEAD, trail: TRAIL, voices: voices.map(({ id, desc }) => ({ id, desc })), noisy: 'light synthetic reverb (RT60 0.3 s, DRR 8 dB) + pink noise (80%) / white noise (20%) mixture at uniform 10-15 dB SNR (speech-region power), seed per clip', clips: [] };

for (const v of voices) {
  engines[v.engine] ??= v.engine === 'kokoro' ? kokoro() : piper(v.engine);
  const tts = engines[v.engine];
  for (const cond of ['clean', 'noisy']) mkdirSync(join(out, cond, v.id), { recursive: true });
  let i = 0;
  for (const u of utterances) {
    const t0 = Date.now();
    const gen = tts.generate({ text: u.text, generationConfig: new sherpa.GenerationConfig({ sid: v.sid(i), speed: v.speed, silenceScale: 0.2 }) });
    let x = resample(gen.samples, gen.sampleRate, SR);
    // trim TTS's own leading/trailing silence, then add our own padding
    const [s0, e0] = speechBounds(x, SR);
    x = normalizePeak(x.subarray(Math.max(0, s0 - 160), Math.min(x.length, e0 + 160)), 0.5);
    const clean = concat(new Float32Array(Math.round(LEAD * SR)), x, new Float32Array(Math.round(TRAIL * SR)));
    const [s, e] = speechBounds(clean, SR);
    const rng = mulberry32(1000 * (voices.indexOf(v) + 1) + i);
    const wet = reverb(clean, SR, rng, 0.3, 8);
    const pink = pinkNoise(clean.length, rng), white = whiteNoise(clean.length, rng);
    const pp = Math.sqrt(pink.reduce((a, b) => a + b * b, 0) / pink.length);
    const noise = new Float32Array(clean.length);
    for (let k = 0; k < noise.length; k++) noise[k] = 0.8 * pink[k] / pp + 0.2 * white[k];
    const snr = 10 + 5 * rng();
    const noisy = mixAtSnr(wet, noise, snr, s, e);
    const rel = `${v.id}/${u.id}.wav`;
    sherpa.writeWave(join(out, 'clean', rel), { samples: clean, sampleRate: SR });
    sherpa.writeWave(join(out, 'noisy', rel), { samples: noisy, sampleRate: SR });
    manifest.clips.push({ voice: v.id, id: u.id, cat: u.cat, text: u.text, file: rel, samples: clean.length, speechStart: s, speechEnd: e, snrDb: +snr.toFixed(2), ttsMs: Date.now() - t0 });
    i++;
  }
  console.log(`voice ${v.id}: ${utterances.length} clips`);
}
writeFileSync(join(out, 'manifest.json'), JSON.stringify(manifest, null, 1));
const speech = manifest.clips.map((c) => (c.speechEnd - c.speechStart) / SR);
console.log(`clips=${manifest.clips.length} speech dur: min ${Math.min(...speech).toFixed(2)} s, mean ${(speech.reduce((a, b) => a + b, 0) / speech.length).toFixed(2)} s, max ${Math.max(...speech).toFixed(2)} s`);
