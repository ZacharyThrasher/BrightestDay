#!/usr/bin/env node
// Runs one model preset through src/voice/asr-engine.mjs on the synthetic test
// set and writes a JSON result file. One model per process so RSS is clean.
//
//   node bench.mjs --preset zipformer-20M --mode accuracy --threads 2 [--hotwords] [--decoding modified_beam_search]
//   node bench.mjs --preset moonshine2-base --mode latency --threads 1 [--voices kok-adam,libritts] [--cond clean]
//
// accuracy: every clip (all voices, clean+noisy unless --cond), push-to-talk
//           style: audio up to speechEnd+100 ms, then endUtterance(). WER,
//           keyword accuracy, RTF, RSS, model size.
// latency : virtual-clock real-time simulation (see simulate()) on a subset;
//           partial timing, per-word partial latency, PTT finalization and
//           hands-free endpoint delay.
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync, mkdirSync, statSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createAsrEngine, buildRecognizerConfig } from '../../src/voice/asr-engine.mjs';
import { PRESETS, GAME_HOTWORDS } from '../../src/voice/asr-presets.mjs';
import { normalize, align, keywordHits, matchedRefIndices } from './lib/text.mjs';

const require = createRequire(import.meta.url);
const sherpa = require('sherpa-onnx-node');
const here = dirname(fileURLToPath(import.meta.url));
const models = process.env.ASR_MODELS_DIR || resolve(here, '..', '..', 'models');
const SR = 16000, CHUNK = 1600; // 100 ms chunks
const RELEASE_AFTER = 0.1; // s after energy-based end of speech the PTT key is released

const argv = process.argv.slice(2);
const arg = (k, d) => { const i = argv.indexOf('--' + k); return i >= 0 ? argv[i + 1] : d; };
const flag = (k) => argv.includes('--' + k);
const preset = arg('preset');
const mode = arg('mode', 'accuracy');
const threads = +arg('threads', 2);
const useHot = flag('hotwords');
const decoding = arg('decoding');
const conds = (arg('cond', mode === 'accuracy' ? 'clean,noisy' : 'clean')).split(',');
const voicesArg = arg('voices', mode === 'accuracy' ? '' : 'kok-adam,libritts');
const limit = +arg('limit', 0);
const tag = arg('tag', '');
const partialMs = +arg('partial-ms', 250);
if (!PRESETS[preset]) { console.error('unknown --preset; one of', Object.keys(PRESETS).join(', ')); process.exit(2); }

const man = JSON.parse(readFileSync(join(here, 'data', 'audio', 'manifest.json'), 'utf8'));
const wordTimes = JSON.parse(readFileSync(join(here, 'data', 'audio', 'word-times.json'), 'utf8'));
let clips = man.clips;
if (voicesArg) clips = clips.filter((c) => voicesArg.split(',').includes(c.voice));
if (limit) clips = clips.slice(0, limit);

// ---- virtual clock -------------------------------------------------------
// The engine is synchronous. We model the worker as a single queue: chunk i
// "arrives" at its real-time end; processing starts at max(arrival, previous
// done) and takes the measured wall time. Event timestamps = virtual time.
let vBase = 0, rBase = 0;
const vnow = () => vBase + (performance.now() - rBase);
function runAt(t, fn) { vBase = t; rBase = performance.now(); fn(); return vnow(); }

const baseOpts = PRESETS[preset](models, { threads, hotwords: useHot ? GAME_HOTWORDS : [] });
if (decoding) baseOpts.decodingMethod = decoding;
baseOpts.partialIntervalMs = partialMs;
if (arg('tail-ms')) baseOpts.tailPaddingMs = +arg('tail-ms');

function modelFiles(o) {
  const files = new Set();
  const walk = (x) => { if (!x) return; if (typeof x === 'string') { if (/\.(onnx|ort|txt)$/.test(x) && !/hotwords|bpe-/.test(x)) files.add(x); } else if (typeof x === 'object') Object.values(x).forEach(walk); };
  if (o.kind === 'two-pass') { walk(buildRecognizerConfig({ ...o, ...o.online, kind: 'online-transducer' }).config); walk(buildRecognizerConfig({ ...o, ...o.offline }).config); }
  else walk(buildRecognizerConfig(o).config);
  if (o.vadModel) files.add(o.vadModel);
  const list = [...files].map((f) => ({ file: f.replace(models + '/', ''), bytes: statSync(f).size }));
  return { list, bytes: list.reduce((a, b) => a + b.bytes, 0) };
}

const rss0 = process.memoryUsage().rss;
const tl = performance.now();
const make = (extra = {}) => createAsrEngine({ ...baseOpts, sherpa, now: vnow, ...extra });
let engine = make(mode === 'accuracy' ? { partials: false } : {});
const loadMs = performance.now() - tl;
const rssLoaded = process.memoryUsage().rss;
let rssPeak = rssLoaded;
const files = modelFiles(baseOpts);

function readClip(cond, c) { return sherpa.readWave(join(here, 'data', 'audio', cond, c.file)).samples; }

function score(ref, hypText) {
  const r = normalize(ref), h = normalize(hypText);
  const a = align(r, h), k = keywordHits(r, h);
  return { n: a.n, err: a.sub + a.del + a.ins, sub: a.sub, del: a.del, ins: a.ins, kwTotal: k.total, kwHit: k.hit, exact: a.sub + a.del + a.ins === 0 };
}

// ---- accuracy --------------------------------------------------------------
// sherpa-onnx-node frees streams in N-API finalizers, which only run when the
// event loop turns; yield between clips or native memory grows without bound.
const tick = () => new Promise((r) => setImmediate(r));

async function accuracy() {
  const rows = [];
  for (const cond of conds) for (const c of clips) {
    await tick();
    const x = readClip(cond, c);
    const rel = Math.min(x.length, c.speechEnd + Math.round(RELEASE_AFTER * SR));
    let final = null;
    const off = engine.onFinal((e) => { final = e; });
    let compute = 0;
    for (let i = 0; i < rel; i += CHUNK) {
      const t = performance.now();
      engine.acceptPcm(x.subarray(i, Math.min(rel, i + CHUNK)));
      compute += performance.now() - t;
    }
    const t = performance.now();
    engine.endUtterance();
    const callMs = performance.now() - t;
    const finalMs = final?.decodeMs ?? callMs; // excludes post-final stream priming (zipformer-20M)
    const primeMs = callMs - finalMs;
    compute += finalMs;
    off();
    rssPeak = Math.max(rssPeak, process.memoryUsage().rss);
    rows.push({ cond, voice: c.voice, id: c.id, cat: c.cat, ref: c.text, hyp: final?.text ?? '', audioSec: rel / SR, computeMs: +compute.toFixed(1), finalMs: +finalMs.toFixed(1), primeMs: +primeMs.toFixed(1), ...score(c.text, final?.text ?? '') });
  }
  return rows;
}

// ---- latency ---------------------------------------------------------------
function simulate(c, x, handsFree) {
  const e = handsFree ? hfEngine : engine;
  const partials = [];
  let final = null;
  const offP = e.onPartial((p) => partials.push({ t: vnow(), text: p.text }));
  const offF = e.onFinal((f) => { if (!final) final = { t: vnow(), text: f.text, reason: f.reason, decodeMs: f.decodeMs }; });
  const end = handsFree ? x.length : Math.min(x.length, c.speechEnd + Math.round(RELEASE_AFTER * SR));
  let done = 0, busy = 0;
  for (let i = 0; i < end; i += CHUNK) {
    const j = Math.min(end, i + CHUNK);
    const arrival = j / SR * 1000;
    const start = Math.max(arrival, done);
    const t = runAt(start, () => e.acceptPcm(x.subarray(i, j)));
    busy += t - start;
    done = t;
    if (handsFree && final) break;
  }
  const release = end / SR * 1000;
  if (!final) { const start = Math.max(release, done); const t = runAt(start, () => e.endUtterance()); busy += t - start; if (final) final.missedEndpoint = handsFree; }
  offP(); offF();
  return { partials, final, release, busy };
}
let hfEngine = null;
async function latency() {
  hfEngine = make({ endpoint: { enabled: true, trailingSilenceMs: +arg('endpoint-ms', 500) } });
  const rows = [];
  for (const cond of conds) for (const c of clips) {
    await tick();
    const x = readClip(cond, c);
    const ref = normalize(c.text);
    const wt = wordTimes[c.file];
    const sStart = c.speechStart / SR * 1000, sEnd = c.speechEnd / SR * 1000;
    const ptt = simulate(c, x, false);
    const nonEmpty = ptt.partials.filter((p) => p.text.trim());
    const firstPartial = nonEmpty.length ? nonEmpty[0].t - sStart : null;
    // per-word: first partial time at which ref word k is correctly present
    const wordLat = [];
    const seen = new Array(ref.length).fill(null);
    for (const p of nonEmpty) {
      const m = matchedRefIndices(ref, normalize(p.text));
      for (const k of m) if (seen[k] == null) seen[k] = p.t;
    }
    let inPartial = 0;
    ref.forEach((w, k) => {
      if (seen[k] != null) { inPartial++; wordLat.push(seen[k] - wt[k].end * 1000); }
    });
    const hf = simulate(c, x, true);
    rssPeak = Math.max(rssPeak, process.memoryUsage().rss);
    rows.push({
      cond, voice: c.voice, id: c.id, cat: c.cat, speechMs: sEnd - sStart,
      nPartials: nonEmpty.length, firstPartialMs: firstPartial,
      partialRateHz: nonEmpty.length / Math.max(0.25, (Math.min(ptt.release, sEnd) - sStart) / 1000),
      wordsInPartial: inPartial, words: ref.length, wordLatMs: wordLat.map((v) => +v.toFixed(0)),
      pttFinalMs: ptt.final ? +(ptt.final.t - ptt.release).toFixed(1) : null,
      pttFinalFromSpeechEndMs: ptt.final ? +(ptt.final.t - sEnd).toFixed(1) : null,
      pttDecodeMs: ptt.final ? +ptt.final.decodeMs.toFixed(1) : null,
      pttBusyMs: +ptt.busy.toFixed(1),
      pttHyp: ptt.final?.text ?? '', ...score(c.text, ptt.final?.text ?? ''),
      hfEndpointMs: hf.final && hf.final.reason === 'endpoint' ? +(hf.final.t - sEnd).toFixed(1) : null,
      hfDecodeMs: hf.final ? +(hf.final.decodeMs ?? 0).toFixed(1) : null,
      hfMissed: !hf.final || hf.final.reason !== 'endpoint', hfHyp: hf.final?.text ?? '',
    });
  }
  return rows;
}

// warm-up (JIT, allocator): one clip not scored
{ const c = man.clips[0]; const x = readClip('clean', c); engine.acceptPcm(x.subarray(0, c.speechEnd)); engine.endUtterance(); }

const t0 = performance.now();
const rows = mode === 'accuracy' ? await accuracy() : await latency();
const wallMs = performance.now() - t0;
const res = {
  preset, mode, threads, hotwords: useHot, decodingMethod: engine.info.decodingMethod ?? engine.info.online?.decodingMethod, tag, partialIntervalMs: partialMs,
  sherpaVersion: require('sherpa-onnx-node/package.json').version, node: process.version, cpu: (await import('node:os')).cpus()[0].model, nCpu: (await import('node:os')).cpus().length,
  loadMs: +loadMs.toFixed(0), rssBaseMiB: +(rss0 / 2 ** 20).toFixed(1), rssLoadedMiB: +(rssLoaded / 2 ** 20).toFixed(1), rssPeakMiB: +(rssPeak / 2 ** 20).toFixed(1),
  modelBytes: files.bytes, modelFiles: files.list, wallMs: +wallMs.toFixed(0), date: new Date().toISOString(), rows,
};
const outDir = join(here, 'data', 'results');
mkdirSync(outDir, { recursive: true });
const name = `${preset}__${mode}__t${threads}${useHot ? '__hot' : ''}${decoding ? '__' + decoding : ''}${tag ? '__' + tag : ''}.json`;
writeFileSync(join(outDir, name), JSON.stringify(res));
const sum = (k) => rows.reduce((a, r) => a + (r[k] ?? 0), 0);
console.log(`${name}: clips=${rows.length} WER=${(100 * sum('err') / sum('n')).toFixed(1)}% kw=${(100 * sum('kwHit') / sum('kwTotal')).toFixed(1)}% load=${res.loadMs}ms rss=${res.rssPeakMiB}MiB wall=${(wallMs / 1000).toFixed(0)}s`);
