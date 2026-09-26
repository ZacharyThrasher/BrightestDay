#!/usr/bin/env node
// Downloads the exact models used by the ASR bench into <repo>/models/.
//
//   node download-models.mjs                 # everything in models.registry.json
//   node download-models.mjs zipformer-20M silero-vad   # only these ids
//   node download-models.mjs --group asr     # only one group (asr|vad|tts)
//
// Archives are verified against models.registry.lock.json (sha256) when present;
// on first download the hash is recorded there. Extraction uses the system `tar`
// (GNU tar on Linux/macOS, bsdtar on Windows 10+ both handle .tar.bz2).
import { createWriteStream, existsSync, mkdirSync, readFileSync, writeFileSync, createReadStream, renameSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { dirname, join, basename, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const modelsDir = process.env.ASR_MODELS_DIR || join(repo, 'models');
const registry = JSON.parse(readFileSync(join(here, 'models.registry.json'), 'utf8'));
const lockPath = join(here, 'models.registry.lock.json');
const lock = existsSync(lockPath) ? JSON.parse(readFileSync(lockPath, 'utf8')) : {};

const args = process.argv.slice(2);
let wanted = registry.models;
const gi = args.indexOf('--group');
if (gi >= 0) wanted = wanted.filter((m) => m.group === args[gi + 1]);
const ids = args.filter((a, i) => !a.startsWith('--') && args[i - 1] !== '--group');
if (ids.length) wanted = wanted.filter((m) => ids.includes(m.id));

mkdirSync(modelsDir, { recursive: true });

function sha256File(p) {
  return new Promise((res, rej) => {
    const h = createHash('sha256');
    createReadStream(p).on('data', (d) => h.update(d)).on('end', () => res(h.digest('hex'))).on('error', rej);
  });
}

for (const m of wanted) {
  const url = `${registry.baseUrl}/${m.asset}`;
  const file = basename(m.asset);
  const isArchive = file.endsWith('.tar.bz2');
  const outName = isArchive ? file.replace(/\.tar\.bz2$/, '') : file;
  const outPath = join(modelsDir, outName);
  if (existsSync(outPath)) { console.log(`[skip] ${m.id} -> ${outPath} exists`); continue; }
  const tmp = join(modelsDir, file + '.part');
  console.log(`[get ] ${m.id} <- ${url}`);
  const r = await fetch(url, { redirect: 'follow' });
  if (!r.ok) { console.error(`[FAIL] ${m.id}: HTTP ${r.status}`); process.exitCode = 1; continue; }
  await pipeline(Readable.fromWeb(r.body), createWriteStream(tmp));
  const hash = await sha256File(tmp);
  if (lock[m.asset] && lock[m.asset] !== hash) {
    console.error(`[FAIL] ${m.id}: sha256 mismatch ${hash} != ${lock[m.asset]}`);
    rmSync(tmp); process.exitCode = 1; continue;
  }
  lock[m.asset] = hash;
  if (isArchive) {
    execFileSync('tar', ['xjf', tmp, '-C', modelsDir], { stdio: 'inherit' });
    rmSync(tmp);
  } else {
    renameSync(tmp, outPath);
  }
  console.log(`[ok  ] ${m.id} sha256=${hash}`);
  // re-read so parallel invocations don't clobber each other's entries
  const cur = existsSync(lockPath) ? JSON.parse(readFileSync(lockPath, 'utf8')) : {};
  writeFileSync(lockPath, JSON.stringify({ ...cur, [m.asset]: hash }, null, 2) + '\n');
}
