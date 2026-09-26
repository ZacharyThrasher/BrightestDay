#!/usr/bin/env node
// Build a self-contained Windows x64 folder + zip:  node tools/package-win.mjs
//
// 1. release build (esbuild, minified)
// 2. stage: dist/ + package.json + models (semantic vectors) + win-x64 native deps (none today)
// 3. @electron/packager --platform=win32 --arch=x64 (downloads the Electron win32 zip from GitHub)
// 4. zip -> release/HARDLIGHT-win32-x64.zip
//
// No wine is needed because we don't rewrite the .exe icon/version resources.
import { execSync } from 'node:child_process';
import { cpSync, mkdirSync, rmSync, writeFileSync, readFileSync, existsSync, statSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
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
writeFileSync(join(stage, 'package.json'), JSON.stringify({ name: 'hardlight', productName: 'HARDLIGHT', version: pkg.version, main: 'dist/main.cjs', type: 'commonjs', dependencies: runtimeDeps }, null, 2));
if (Object.keys(runtimeDeps).length) sh('npm install --omit=dev --os=win32 --cpu=x64 --no-audit --no-fund', { cwd: stage });

// settings template players can edit (System-2 provider, mouse, graphics)
cpSync(join(root, 'tools/settings.example.json'), join(stage, 'settings.example.json'));
cpSync(join(root, 'README.md'), join(stage, 'README.md'));
if (existsSync(join(root, 'THIRD_PARTY_NOTICES.md'))) cpSync(join(root, 'THIRD_PARTY_NOTICES.md'), join(stage, 'THIRD_PARTY_NOTICES.md'));

const electronVersion = JSON.parse(readFileSync(join(root, 'node_modules/electron/package.json'), 'utf8')).version;
sh(`npx --yes @electron/packager@20 "${stage}" HARDLIGHT --platform=win32 --arch=x64 --electron-version=${electronVersion} --out="${rel}" --overwrite --asar.unpack="**/*.{node,dll,onnx,bin,txt}" --ignore="^/node_modules/\\.bin"`);

const outDir = join(rel, 'HARDLIGHT-win32-x64');
// user-facing launcher notes next to the exe
cpSync(join(root, 'tools/settings.example.json'), join(outDir, 'settings.example.json'));
cpSync(join(root, 'README.md'), join(outDir, 'README.md'));
rmSync(join(rel, 'HARDLIGHT-win32-x64.zip'), { force: true });
sh(`zip -qr -9 HARDLIGHT-win32-x64.zip HARDLIGHT-win32-x64`, { cwd: rel });

const size = (p) => { let t = 0; const walk = (q) => { const s = statSync(q); if (s.isDirectory()) for (const f of readdirSync(q)) walk(join(q, f)); else t += s.size; }; walk(p); return t; };
console.log(`\nfolder ${(size(outDir) / 1e6).toFixed(0)} MB, zip ${(statSync(join(rel, 'HARDLIGHT-win32-x64.zip')).size / 1e6).toFixed(0)} MB -> release/HARDLIGHT-win32-x64.zip`);
