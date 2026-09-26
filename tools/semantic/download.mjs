#!/usr/bin/env node
// Re-runnable downloader for the semantic experiment.
//   node tools/semantic/download.mjs                # everything
//   node tools/semantic/download.mjs --group embed  # embedding sources only (llm = LLM runtime + GGUFs)
//   node tools/semantic/download.mjs spacy-md glove-300
// Files land in <repo>/models/semantic/raw/<file> (git-ignored). sha256 is recorded in
// sources.lock.json on first download and verified on every later run / against expectSha256.
import { createWriteStream, existsSync, mkdirSync, readFileSync, writeFileSync, createReadStream, renameSync, rmSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const rawDir = process.env.SEMANTIC_RAW_DIR || join(repo, 'models', 'semantic', 'raw');
const { sources } = JSON.parse(readFileSync(join(here, 'sources.json'), 'utf8'));
const lockPath = join(here, 'sources.lock.json');
const readLock = () => (existsSync(lockPath) ? JSON.parse(readFileSync(lockPath, 'utf8')) : {});

const args = process.argv.slice(2);
let wanted = sources;
const gi = args.indexOf('--group');
if (gi >= 0) wanted = wanted.filter((s) => s.group === args[gi + 1]);
const ids = args.filter((a, i) => !a.startsWith('--') && args[i - 1] !== '--group');
if (ids.length) wanted = wanted.filter((s) => ids.includes(s.id));

const sha256File = (p) => new Promise((res, rej) => {
  const h = createHash('sha256');
  createReadStream(p).on('data', (d) => h.update(d)).on('end', () => res(h.digest('hex'))).on('error', rej);
});

for (const s of wanted) {
  const out = join(rawDir, s.file);
  mkdirSync(dirname(out), { recursive: true });
  const expect = s.expectSha256 || readLock()[s.file]?.sha256;
  if (existsSync(out)) {
    if (process.argv.includes('--verify')) {
      const h = await sha256File(out);
      console.log(`${expect && h !== expect ? '[BAD ]' : '[ok  ]'} ${s.id} ${h}`);
    } else console.log(`[skip] ${s.id} exists`);
    continue;
  }
  let ok = false;
  for (const url of [s.url, ...(s.mirrors || [])]) {
    console.log(`[get ] ${s.id} <- ${url}`);
    const tmp = out + '.part';
    try {
      const r = await fetch(url, { redirect: 'follow' });
      if (!r.ok) { console.error(`[FAIL] HTTP ${r.status}`); continue; }
      await pipeline(Readable.fromWeb(r.body), createWriteStream(tmp));
    } catch (e) { console.error(`[FAIL] ${e.message}`); continue; }
    const h = await sha256File(tmp);
    if (expect && h !== expect) { console.error(`[FAIL] sha256 ${h} != expected ${expect}`); rmSync(tmp); continue; }
    renameSync(tmp, out);
    const lock = readLock();
    lock[s.file] = { id: s.id, url, sha256: h, size: statSync(out).size, license: s.license };
    writeFileSync(lockPath, JSON.stringify(lock, null, 2) + '\n');
    console.log(`[ok  ] ${s.id} sha256=${h}`);
    ok = true; break;
  }
  if (!ok) process.exitCode = 1;
}
