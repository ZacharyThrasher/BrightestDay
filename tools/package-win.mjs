#!/usr/bin/env node
// Build a self-contained Windows x64 folder + zip:  node tools/package-win.mjs
//
// 1. release build (esbuild, minified)
// 2. stage: dist/ + package.json + models (semantic vectors) + win-x64 native deps
// 3. cloudflared.exe (pinned version + sha256 in tools/ship-models.json, cached in release/cache/)
//    → resources/cloudflared/cloudflared.exe (+ LICENSE) via packager --extra-resource
// 4. @electron/packager --platform=win32 --arch=x64 (downloads the Electron win32 zip from GitHub)
// 5. zip -> release/BrightestDay-win32-x64.zip
//
// No wine is needed because we don't rewrite the .exe icon/version resources.
import { execSync } from 'node:child_process';
import { cpSync, mkdirSync, rmSync, writeFileSync, readFileSync, existsSync, statSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const rel = join(root, 'release');
const stage = join(rel, 'stage');
const sh = (cmd, opts = {}) => { console.log('$', cmd); execSync(cmd, { stdio: 'inherit', cwd: root, ...opts }); };
const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
const manifest = JSON.parse(readFileSync(join(root, 'tools/ship-models.json'), 'utf8'));

rmSync(stage, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });

sh('node tools/build.mjs --minify');
cpSync(join(root, 'dist'), join(stage, 'dist'), { recursive: true, filter: (p) => !p.endsWith('.map') && !p.includes(`${join('dist', 'web')}`) });

// models: only what the shipped presets need
for (const m of manifest.models) {
  const src = join(root, m.from);
  if (!existsSync(src)) { console.warn(`!! missing model ${m.from} — build will run without it`); continue; }
  const dst = join(stage, m.to);
  mkdirSync(dirname(dst), { recursive: true });
  cpSync(src, dst, { recursive: true });
}

// runtime package.json (no dev deps; native addons for win-x64 only, if any are listed)
const runtimeDeps = {};
for (const d of manifest.nativeDependencies) runtimeDeps[d] = pkg.dependencies?.[d] ?? pkg.devDependencies?.[d] ?? manifest.versions?.[d] ?? 'latest';
writeFileSync(join(stage, 'package.json'), JSON.stringify({ name: 'brightest-day', productName: 'Brightest Day', version: pkg.version, main: 'dist/main.cjs', type: 'commonjs', dependencies: runtimeDeps }, null, 2));
if (Object.keys(runtimeDeps).length) sh('npm install --omit=dev --os=win32 --cpu=x64 --no-audit --no-fund', { cwd: stage });

// settings template players can edit (System-2 provider, mouse, graphics)
cpSync(join(root, 'tools/settings.example.json'), join(stage, 'settings.example.json'));
cpSync(join(root, 'README.md'), join(stage, 'README.md'));
if (existsSync(join(root, 'THIRD_PARTY_NOTICES.md'))) cpSync(join(root, 'THIRD_PARTY_NOTICES.md'), join(stage, 'THIRD_PARTY_NOTICES.md'));

// cloudflared (online play): downloaded once into release/cache, pinned sha256 verified on every build
async function fetchVerified(url, sha256, dest) {
  const hash = (p) => createHash('sha256').update(readFileSync(p)).digest('hex');
  if (existsSync(dest) && hash(dest) === sha256) return dest;
  console.log(`downloading ${url}`);
  let buf;
  try {
    const res = await fetch(url, { redirect: 'follow' });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    buf = Buffer.from(await res.arrayBuffer());
  } catch (e) {
    throw new Error(`download failed (${url}): ${e.message}${process.env.HTTPS_PROXY ? ' (behind a proxy: run with NODE_USE_ENV_PROXY=1)' : ''}`);
  }
  const got = createHash('sha256').update(buf).digest('hex');
  if (got !== sha256) throw new Error(`sha256 mismatch for ${url}: expected ${sha256}, got ${got}`);
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(dest, buf);
  return dest;
}
const cf = manifest.cloudflared;
const extraDir = join(rel, 'extra-resources');
rmSync(extraDir, { recursive: true, force: true });
let extraResourceArg = '';
if (cf) {
  const cache = join(rel, 'cache');
  const exe = await fetchVerified(cf.url, cf.sha256, join(cache, `cloudflared-${cf.version}-windows-amd64.exe`));
  const lic = await fetchVerified(cf.licenseUrl, cf.licenseSha256, join(cache, `cloudflared-${cf.version}-LICENSE`));
  const dst = join(extraDir, 'cloudflared');
  mkdirSync(dst, { recursive: true });
  cpSync(exe, join(dst, 'cloudflared.exe'));
  cpSync(lic, join(dst, 'LICENSE'));
  writeFileSync(join(dst, 'VERSION'), `cloudflared ${cf.version}\n${cf.url}\nsha256 ${cf.sha256}\n`);
  extraResourceArg = ` --extra-resource="${dst}"`;
  console.log(`cloudflared ${cf.version} verified (sha256 ${cf.sha256.slice(0, 12)}…)`);
}

const electronVersion =JSON.parse(readFileSync(join(root, 'node_modules/electron/package.json'), 'utf8')).version;
sh(`npx --yes @electron/packager@20 "${stage}" BrightestDay --platform=win32 --arch=x64 --electron-version=${electronVersion} --out="${rel}" --overwrite --asar.unpack="**/*.{node,dll,onnx,bin,txt}" --ignore="^/node_modules/\\.bin"${extraResourceArg}`);

const outDir = join(rel, 'BrightestDay-win32-x64');
// user-facing launcher notes next to the exe
cpSync(join(root, 'tools/settings.example.json'), join(outDir, 'settings.example.json'));
cpSync(join(root, 'README.md'), join(outDir, 'README.md'));
rmSync(join(rel, 'BrightestDay-win32-x64.zip'), { force: true });
sh(`zip -qr -9 BrightestDay-win32-x64.zip BrightestDay-win32-x64`, { cwd: rel });

const size = (p) => { let t = 0; const walk = (q) => { const s = statSync(q); if (s.isDirectory()) for (const f of readdirSync(q)) walk(join(q, f)); else t += s.size; }; walk(p); return t; };
console.log(`\nfolder ${(size(outDir) / 1e6).toFixed(0)} MB, zip ${(statSync(join(rel, 'BrightestDay-win32-x64.zip')).size / 1e6).toFixed(0)} MB -> release/BrightestDay-win32-x64.zip`);
