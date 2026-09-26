#!/usr/bin/env node
// Writes docs/experiments/asr-models.manifest.json: for every model we would
// ship, the release-archive URL + sha256 and the exact files the engine loads
// (path inside the archive, bytes, sha256).
//
//   node make-manifest.mjs
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, statSync, existsSync } from 'node:fs';
import { join, dirname, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildRecognizerConfig } from '../../src/voice/asr-engine.mjs';
import { PRESETS } from '../../src/voice/asr-presets.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const models = process.env.ASR_MODELS_DIR || join(repo, 'models');
const registry = JSON.parse(readFileSync(join(here, 'models.registry.json'), 'utf8'));
const lock = JSON.parse(readFileSync(join(here, 'models.registry.lock.json'), 'utf8'));
const sha = (p) => createHash('sha256').update(readFileSync(p)).digest('hex');

// role: what the game would ship it for
const SHIP = [
  { preset: 'nemo-fc-80ms', registryId: 'nemo-fc-80ms', role: 'primary: streaming partials + final (recommended)' },
  { preset: 'moonshine2-base', registryId: 'moonshine2-base', role: 'optional second pass / runner-up (offline, pseudo-streaming)' },
  { preset: 'zipformer-20M', registryId: 'zipformer-20M', role: 'low-end fallback (small, weaker accuracy; needs priming wav)' },
  { preset: null, registryId: 'silero-vad', role: 'VAD for offline kinds / hands-free' },
];

const out = { generated: new Date().toISOString(), runtime: { npm: 'sherpa-onnx-node@1.13.8', platformPackages: ['sherpa-onnx-win-x64@1.13.8 (8.9 MB tgz, 23.5 MB unpacked: sherpa-onnx.node, sherpa-onnx-c-api.dll, sherpa-onnx-cxx-api.dll, onnxruntime.dll, onnxruntime_providers_shared.dll)'] }, models: [] };
for (const s of SHIP) {
  const reg = registry.models.find((m) => m.id === s.registryId);
  const url = `${registry.baseUrl}/${reg.asset}`;
  const paths = new Set();
  if (s.preset) {
    const o = PRESETS[s.preset](models, { threads: 2, hotwords: [] });
    const walk = (x) => { if (typeof x === 'string') { if (/\.(onnx|ort|txt)$/.test(x) && x.startsWith(models)) paths.add(x); } else if (x && typeof x === 'object') Object.values(x).forEach(walk); };
    walk(buildRecognizerConfig(o).config);
    if (o.prime) paths.add(join(o.modelDir, 'test_wavs', '0.wav'));
  } else paths.add(join(models, reg.asset.split('/').pop()));
  const files = [...paths].filter(existsSync).sort().map((p) => ({ path: relative(models, p), bytes: statSync(p).size, sha256: sha(p) }));
  out.models.push({ id: s.registryId, preset: s.preset, role: s.role, url, archiveSha256: lock[reg.asset] ?? null, files, totalBytes: files.reduce((a, f) => a + f.bytes, 0) });
}
const dest = join(repo, 'docs', 'experiments', 'asr-models.manifest.json');
writeFileSync(dest, JSON.stringify(out, null, 2) + '\n');
for (const m of out.models) console.log(`${m.id}: ${(m.totalBytes / 1e6).toFixed(1)} MB in ${m.files.length} files`);
console.log('->', dest);
