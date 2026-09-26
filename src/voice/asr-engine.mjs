// Local streaming speech recognition engine for voice commands.
//
// Runs entirely in-process on top of sherpa-onnx-node (N-API addon with
// prebuilt win-x64 / linux-x64 / darwin binaries). Intended host: an Electron
// utility process that receives 16 kHz mono Float32 PCM from the renderer.
//
//   import { createAsrEngine } from './asr-engine.mjs';
//   const asr = createAsrEngine({ kind: 'online-transducer', modelDir, threads: 2, hotwords: ['trampoline'] });
//   asr.onPartial(({ text }) => ...);           // ghost preview while speaking
//   asr.onFinal(({ text, reason }) => ...);     // 'ptt' (endUtterance) or 'endpoint' (hands-free)
//   asr.acceptPcm(float32Array16k);             // any chunk size, e.g. 100 ms
//   asr.endUtterance();                         // push-to-talk key released
//
// Kinds:
//   'online-transducer'  true streaming (zipformer / NeMo fast-conformer). Partials
//                        come from the decoder state; supports hotwords via
//                        modified_beam_search. RECOMMENDED primary path.
//   'offline-moonshine' | 'offline-moonshine2' | 'offline-whisper' | 'offline-transducer'
//                        pseudo-streaming: Silero VAD gates speech, the growing
//                        buffer is re-decoded every ~partialIntervalMs for partials
//                        and once more on endUtterance / VAD endpoint for the final.
//   'two-pass'           online transducer for partials + an offline model for the
//                        final ({ online: {...opts}, offline: {...opts} }).
//
// All work is synchronous inside acceptPcm()/endUtterance(): run this in a
// worker/utility process, never on the render thread.

import { createRequire } from 'node:module';
import { existsSync, readdirSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';

const SR = 16000;
const require = createRequire(import.meta.url);

function loadSherpa(explicit) {
  if (explicit) return explicit;
  return require('sherpa-onnx-node');
}

function pick(dir, re, preferInt8 = true) {
  const files = readdirSync(dir).filter((f) => re.test(f));
  if (!files.length) throw new Error(`asr-engine: no file matching ${re} in ${dir}`);
  const int8 = files.filter((f) => /int8/.test(f));
  const fp = files.filter((f) => !/int8/.test(f));
  const chosen = (preferInt8 ? int8[0] ?? fp[0] : fp[0] ?? int8[0]);
  return join(dir, chosen);
}

function tokensUppercase(tokensPath) {
  const lines = readFileSync(tokensPath, 'utf8').split('\n');
  let up = 0, low = 0;
  for (const l of lines) {
    const t = l.split(' ')[0];
    if (/[A-Z]/.test(t)) up++;
    if (/[a-z]/.test(t)) low++;
  }
  return up > low;
}

// Hotword encoding for BPE models needs a bpe.vocab. The shipped models don't
// always include one; an equivalent is derived from tokens.txt (equal scores ->
// longest-match), written to a cache dir because model dirs may be read-only.
function ensureBpeVocab(tokensPath, cacheDir) {
  const h = createHash('sha1').update(tokensPath).digest('hex').slice(0, 12);
  mkdirSync(cacheDir, { recursive: true });
  const out = join(cacheDir, `bpe-${h}.vocab`);
  if (!existsSync(out)) {
    const vocab = readFileSync(tokensPath, 'utf8').split('\n').filter((l) => l.trim())
      .map((l) => `${l.split(' ')[0]}\t-1.0`).join('\n');
    writeFileSync(out, vocab + '\n');
  }
  return out;
}

function formatHotwords(hotwords, upper, score) {
  // one phrase per line; optional per-phrase boost written as "PHRASE :2.0"
  return hotwords.map((h) => {
    const [phrase, s] = Array.isArray(h) ? h : [h, undefined];
    const p = upper ? phrase.toUpperCase() : phrase.toLowerCase();
    return s != null ? `${p} :${s}` : p;
  }).join('\n');
}

/**
 * Build a sherpa-onnx recognizer config for a model directory.
 * Exported so the bench can reuse exactly the same configs.
 */
export function buildRecognizerConfig(o) {
  const { kind, modelDir: d, threads = 2, provider = 'cpu', int8 = true, hotwords = [], hotwordsScore = 1.5,
    cacheDir = join(tmpdir(), 'brightest-asr-cache'), decodingMethod, maxActivePaths = 4 } = o;
  const feat = { sampleRate: SR, featureDim: 80 };
  const base = { numThreads: threads, provider, debug: 0 };
  if (kind === 'online-transducer') {
    const tokens = join(d, 'tokens.txt');
    const upper = tokensUppercase(tokens);
    const modelConfig = {
      transducer: { encoder: pick(d, /^encoder.*\.onnx$/, int8), decoder: pick(d, /^decoder.*\.onnx$/, int8), joiner: pick(d, /^joiner.*\.onnx$/, int8) },
      tokens, ...base,
    };
    const cfg = { featConfig: feat, modelConfig, decodingMethod: decodingMethod ?? 'greedy_search', maxActivePaths };
    if (hotwords.length) {
      modelConfig.modelingUnit = 'bpe';
      modelConfig.bpeVocab = ensureBpeVocab(tokens, cacheDir);
      cfg.decodingMethod = 'modified_beam_search';
      const buf = formatHotwords(hotwords, upper, hotwordsScore);
      const hf = join(cacheDir, `hotwords-${createHash('sha1').update(buf).digest('hex').slice(0, 12)}.txt`);
      mkdirSync(cacheDir, { recursive: true });
      writeFileSync(hf, buf + '\n');
      cfg.hotwordsFile = hf;
      cfg.hotwordsScore = hotwordsScore;
    }
    return { type: 'online', config: cfg, upper };
  }
  let modelConfig, upper = false, streamHotwords;
  let decoding = decodingMethod ?? 'greedy_search';
  if (kind === 'offline-moonshine') {
    modelConfig = { moonshine: { preprocessor: join(d, 'preprocess.onnx'), encoder: pick(d, /^encode.*\.onnx$/, int8), uncachedDecoder: pick(d, /^uncached_decode.*\.onnx$/, int8), cachedDecoder: pick(d, /^cached_decode.*\.onnx$/, int8) }, tokens: join(d, 'tokens.txt'), ...base };
  } else if (kind === 'offline-moonshine2') {
    modelConfig = { moonshine: { encoder: join(d, 'encoder_model.ort'), mergedDecoder: join(d, 'decoder_model_merged.ort') }, tokens: join(d, 'tokens.txt'), ...base };
  } else if (kind === 'offline-whisper') {
    modelConfig = { whisper: { encoder: pick(d, /-encoder.*\.onnx$/, int8), decoder: pick(d, /-decoder.*\.onnx$/, int8), language: 'en', task: 'transcribe' }, tokens: pick(d, /-tokens\.txt$/), ...base };
  } else if (kind === 'offline-transducer') {
    const tokens = join(d, 'tokens.txt');
    upper = tokensUppercase(tokens);
    modelConfig = { transducer: { encoder: pick(d, /^encoder.*\.onnx$/, int8), decoder: pick(d, /^decoder.*\.onnx$/, int8), joiner: pick(d, /^joiner.*\.onnx$/, int8) }, tokens, modelType: 'nemo_transducer', ...base };
    if (hotwords.length) {
      modelConfig.modelingUnit = 'bpe';
      modelConfig.bpeVocab = ensureBpeVocab(tokens, cacheDir);
      decoding = 'modified_beam_search';
      streamHotwords = formatHotwords(hotwords, upper, hotwordsScore).split('\n').join('/');
    }
  } else {
    throw new Error(`asr-engine: unknown kind ${kind}`);
  }
  const cfg = { featConfig: feat, modelConfig, decodingMethod: decoding, maxActivePaths };
  if (streamHotwords) cfg.hotwordsScore = hotwordsScore;
  return { type: 'offline', config: cfg, upper, streamHotwords };
}

function cleanText(t, upper) {
  let s = (t ?? '').trim();
  if (upper || (s && s === s.toUpperCase())) s = s.toLowerCase();
  return s;
}

class Emitter {
  constructor() { this.p = []; this.f = []; }
  partial(e) { for (const cb of this.p) cb(e); }
  final(e) { for (const cb of this.f) cb(e); }
}

// ---------------------------------------------------------------- online ----
function createOnline(o, sherpa, em) {
  const { config, upper } = buildRecognizerConfig(o);
  const ep = o.endpoint ?? {};
  if (ep.enabled) {
    config.enableEndpoint = 1;
    config.rule1MinTrailingSilence = ep.rule1 ?? 2.4; // no speech decoded yet
    config.rule2MinTrailingSilence = (ep.trailingSilenceMs ?? 600) / 1000; // after speech
    config.rule3MinUtteranceLength = ep.maxUtteranceSec ?? 20;
  }
  const rec = new sherpa.OnlineRecognizer(config);
  const tailPad = new Float32Array(Math.round(((o.tailPaddingMs ?? 400) / 1000) * SR));
  const now = o.now ?? (() => performance.now());
  // Each push-to-talk utterance gets its own stream (inputFinished() is the
  // only way to flush the encoder's right context; with rec.reset() on a
  // long-lived stream the last ~300 ms leaked into the next utterance).
  // With opts.prime, fresh streams are primed with 3 s of real speech + 1 s
  // silence, then reset: measured, a cold stream of the 20M zipformer drops
  // the first ~1 s of speech on our clips, a primed one does not. Other models
  // are fine cold and priming *hurts* them (leaked context, e.g. "bill the
  // trampoline"), so it is off by default (see docs/experiments/ASR.md).
  // Priming runs right after the final is emitted, off the critical path.
  let primeAudio = o.prime ? o.warmup : null;
  if (o.prime && primeAudio === undefined) {
    const f = join(o.modelDir, 'test_wavs', '0.wav');
    if (existsSync(f)) primeAudio = sherpa.readWave(f).samples.subarray(0, Math.round(3 * SR));
  }
  const primeSilence = new Float32Array(Math.round(1.0 * SR));
  let stream, last = '', samples = 0, lastPrimeMs = 0;

  function drain() { while (rec.isReady(stream)) rec.decode(stream); }
  function newStream() {
    const t = now();
    stream = rec.createStream();
    if (primeAudio) {
      stream.acceptWaveform({ sampleRate: SR, samples: primeAudio });
      stream.acceptWaveform({ sampleRate: SR, samples: primeSilence });
      drain();
      rec.reset(stream);
    }
    lastPrimeMs = now() - t;
  }
  newStream();

  return {
    acceptPcm(x) {
      stream.acceptWaveform({ sampleRate: SR, samples: x });
      samples += x.length;
      drain();
      const text = cleanText(rec.getResult(stream).text, upper);
      if (text !== last) { last = text; em.partial({ text, audioMs: (samples / SR) * 1000, at: now() }); }
      if (ep.enabled && rec.isEndpoint(stream)) {
        // hands-free: the endpoint fires after trailing silence, so the
        // encoder has already seen the whole utterance; reset is enough.
        if (text) em.final({ text, reason: 'endpoint', audioMs: (samples / SR) * 1000, decodeMs: 0, at: now() });
        rec.reset(stream); last = ''; samples = 0;
      }
    },
    endUtterance() {
      const t0 = now();
      stream.acceptWaveform({ sampleRate: SR, samples: tailPad });
      stream.inputFinished();
      drain();
      const text = cleanText(rec.getResult(stream).text, upper);
      const at = now();
      em.final({ text, reason: 'ptt', audioMs: (samples / SR) * 1000, decodeMs: at - t0, at });
      last = ''; samples = 0;
      newStream();
    },
    reset() { last = ''; samples = 0; newStream(); },
    dispose() { stream = null; },
    get lastPrimeMs() { return lastPrimeMs; },
    info: { kind: o.kind, modelDir: o.modelDir, decodingMethod: config.decodingMethod, hotwords: !!config.hotwordsFile },
    _rec: rec,
  };
}

// ------------------------------------------------- offline pseudo-stream ----
function createOffline(o, sherpa, em) {
  const { config, upper, streamHotwords } = buildRecognizerConfig(o);
  const rec = new sherpa.OfflineRecognizer(config);
  const now = o.now ?? (() => performance.now());
  const ep = o.endpoint ?? {};
  const baseInterval = Math.round(((o.partialIntervalMs ?? 250) / 1000) * SR);
  const preRoll = Math.round(((o.preRollMs ?? 300) / 1000) * SR);
  const backlogMs = o.backlogMs ?? 150;
  const partials = o.partials !== false;
  if (!o.vadModel) throw new Error('asr-engine: offline kinds need vadModel (silero_vad.onnx)');
  const vadCfg = {
    sileroVad: { model: o.vadModel, threshold: o.vadThreshold ?? 0.5, minSilenceDuration: (ep.trailingSilenceMs ?? 500) / 1000, minSpeechDuration: 0.1, maxSpeechDuration: 20, windowSize: 512 },
    sampleRate: SR, numThreads: 1, provider: 'cpu', debug: 0,
  };
  const vad = new sherpa.Vad(vadCfg, 30);
  const WIN = 512;
  // Pay ONNX Runtime's first-run cost at load time, not on the first command.
  if (o.warmup !== false) {
    const f = join(o.modelDir, 'test_wavs', '0.wav');
    const w = existsSync(f) ? sherpa.readWave(f).samples.subarray(0, SR * 2) : new Float32Array(SR);
    const s = rec.createStream(); s.acceptWaveform({ sampleRate: SR, samples: w }); rec.decode(s);
  }
  // Autoregressive decoders (Moonshine/Whisper) can loop on very short input
  // ("make make make make"). Drop partials with an implausible speaking rate.
  const maxWordsPerSec = o.maxWordsPerSec ?? 6;

  let chunks = [], total = 0, vadPending = new Float32Array(0), speechStart = -1, lastDecodeAt = 0;
  let interval = baseInterval, last = '', t0 = null, peak = 0, lastDecodeSilent = false;

  function buffer(from = 0) {
    const out = new Float32Array(total - from);
    let off = 0, pos = 0;
    for (const c of chunks) {
      const s = Math.max(0, from - pos), e = c.length;
      if (s < e) { out.set(c.subarray(s), off); off += e - s; }
      pos += c.length;
    }
    return out;
  }
  function decode(samples) {
    const s = streamHotwords ? rec.createStream(streamHotwords) : rec.createStream();
    s.acceptWaveform({ sampleRate: SR, samples });
    rec.decode(s);
    return cleanText(rec.getResult(s).text, upper);
  }
  function finalize(reason) {
    const ts = now();
    let text = '';
    const from = speechStart >= 0 ? speechStart : 0;
    // no VAD detection at all: still decode if there was clearly audible signal
    if ((speechStart >= 0 || peak > 0.02) && total - from > SR * 0.1) text = decode(buffer(from));
    em.final({ text, reason, audioMs: (total / SR) * 1000, decodeMs: now() - ts, at: now() });
    reset();
  }
  function reset() {
    chunks = []; total = 0; vadPending = new Float32Array(0); speechStart = -1; lastDecodeAt = 0;
    interval = baseInterval; last = ''; t0 = null; peak = 0; lastDecodeSilent = false; vad.reset();
  }

  return {
    acceptPcm(x) {
      if (t0 === null) t0 = now() - (x.length / SR) * 1000;
      chunks.push(Float32Array.from(x)); total += x.length;
      for (let i = 0; i < x.length; i++) { const a = Math.abs(x[i]); if (a > peak) peak = a; }
      // feed VAD in 512-sample windows
      const v = new Float32Array(vadPending.length + x.length); v.set(vadPending); v.set(x, vadPending.length);
      let i = 0;
      for (; i + WIN <= v.length; i += WIN) {
        vad.acceptWaveform(v.subarray(i, i + WIN));
        if (speechStart < 0 && vad.isDetected()) speechStart = Math.max(0, total - (v.length - i - WIN) - preRoll - Math.round(0.1 * SR) - WIN);
      }
      vadPending = v.slice(i);
      let endpoint = false;
      while (!vad.isEmpty()) { vad.pop(); endpoint = true; }
      if (endpoint && ep.enabled && speechStart >= 0) { finalize('endpoint'); return; }
      if (!partials || speechStart < 0 || total - lastDecodeAt < interval) return;
      // nothing new to hear: VAD says silence now and at the previous decode
      const inSpeech = vad.isDetected();
      if (!inSpeech && lastDecodeSilent) return;
      const behind = now() - (t0 + (total / SR) * 1000);
      if (behind > backlogMs) return; // skip partial, catch up
      const ts = now();
      const text = decode(buffer(speechStart));
      const took = now() - ts;
      lastDecodeAt = total;
      lastDecodeSilent = !inSpeech;
      interval = Math.max(baseInterval, Math.round(((1.5 * took) / 1000) * SR));
      const speechSec = Math.max(0.25, (total - speechStart - preRoll) / SR);
      const plausible = text.split(/\s+/).filter(Boolean).length <= maxWordsPerSec * speechSec + 1;
      if (plausible && text !== last) { last = text; em.partial({ text, audioMs: (total / SR) * 1000, decodeMs: took, at: now() }); }
    },
    endUtterance() { finalize('ptt'); },
    reset,
    dispose() { chunks = []; },
    info: { kind: o.kind, modelDir: o.modelDir, decodingMethod: config.decodingMethod, hotwords: !!streamHotwords },
    _rec: rec,
  };
}

// ---------------------------------------------------------------- two-pass --
function createTwoPass(o, sherpa, em) {
  // Partials from the streaming model; the final re-decodes the utterance audio
  // (everything since reset) with the offline model.
  const onlineEm = new Emitter();
  const online = createOnline({ ...o, ...o.online, kind: 'online-transducer', endpoint: o.endpoint }, sherpa, onlineEm);
  const offCfg = buildRecognizerConfig({ ...o, ...o.offline });
  const off = new sherpa.OfflineRecognizer(offCfg.config);
  const now = o.now ?? (() => performance.now());
  let chunks = [], total = 0;
  const audio = () => { const b = new Float32Array(total); let p = 0; for (const c of chunks) { b.set(c, p); p += c.length; } return b; };
  function second(reason, firstText) {
    const ts = now();
    let text = firstText;
    if (firstText) {
      const s = offCfg.streamHotwords ? off.createStream(offCfg.streamHotwords) : off.createStream();
      s.acceptWaveform({ sampleRate: SR, samples: audio() });
      off.decode(s);
      text = cleanText(off.getResult(s).text, offCfg.upper);
    }
    chunks = []; total = 0;
    em.final({ text, firstPass: firstText, reason, audioMs: 0, decodeMs: now() - ts, at: now() });
  }
  onlineEm.p.push((e) => em.partial(e));
  onlineEm.f.push((e) => second(e.reason, e.text));
  return {
    acceptPcm(x) { chunks.push(Float32Array.from(x)); total += x.length; online.acceptPcm(x); },
    endUtterance() { online.endUtterance(); },
    reset() { online.reset(); chunks = []; total = 0; },
    dispose() { online.dispose(); },
    info: { kind: 'two-pass', online: online.info, offline: o.offline?.modelDir },
  };
}

/**
 * @param {object} opts
 * @param {'online-transducer'|'offline-moonshine'|'offline-moonshine2'|'offline-whisper'|'offline-transducer'|'two-pass'} opts.kind
 * @param {string} opts.modelDir
 * @param {number} [opts.threads=2]
 * @param {(string|[string,number])[]} [opts.hotwords]  transducer models only
 * @param {number} [opts.hotwordsScore=1.5]
 * @param {string} [opts.vadModel]    path to silero_vad.onnx (offline kinds)
 * @param {{enabled?:boolean, trailingSilenceMs?:number}} [opts.endpoint]  hands-free endpointing
 * @param {number} [opts.partialIntervalMs=250] offline kinds: re-decode period
 * @param {number} [opts.tailPaddingMs=400]  online kinds: zero padding on endUtterance
 * @param {() => number} [opts.now]   clock in ms (tests/benchmarks)
 * @param {object} [opts.sherpa]      pre-loaded sherpa-onnx-node module
 */
export function createAsrEngine(opts) {
  const sherpa = loadSherpa(opts.sherpa);
  const em = new Emitter();
  const impl = opts.kind === 'online-transducer' ? createOnline(opts, sherpa, em)
    : opts.kind === 'two-pass' ? createTwoPass(opts, sherpa, em)
    : createOffline(opts, sherpa, em);
  return {
    acceptPcm: (x) => impl.acceptPcm(x),
    onPartial(cb) { em.p.push(cb); return () => { em.p = em.p.filter((f) => f !== cb); }; },
    onFinal(cb) { em.f.push(cb); return () => { em.f = em.f.filter((f) => f !== cb); }; },
    endUtterance: () => impl.endUtterance(),
    reset: () => impl.reset(),
    dispose() { impl.dispose(); em.p = []; em.f = []; },
    info: impl.info,
  };
}
