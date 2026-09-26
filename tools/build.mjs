#!/usr/bin/env node
// Build HARDLIGHT: renderer bundle (Electron + plain web), Electron main + preload.
//   node tools/build.mjs            dev build (sourcemaps, unminified)
//   node tools/build.mjs --minify   release build
//   node tools/build.mjs --watch    rebuild renderer on change
import * as esbuild from 'esbuild';
import { mkdirSync, copyFileSync, rmSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(root, 'dist');
const web = join(dist, 'web');
const args = new Set(process.argv.slice(2));
const minify = args.has('--minify');
const watch = args.has('--watch');

const t0 = Date.now();
for (const d of [dist, web]) mkdirSync(d, { recursive: true });
for (const f of ['renderer.js', 'renderer.js.map']) if (existsSync(join(dist, f))) rmSync(join(dist, f));

const rendererOpts = {
  entryPoints: [join(root, 'src/app/main.ts')],
  bundle: true,
  format: 'iife', // classic script: works from file:// in Electron without module CORS rules
  platform: 'browser',
  target: ['chrome120'],
  sourcemap: true,
  minify,
  legalComments: 'none',
  define: { 'process.env.NODE_ENV': '"production"' },
  logLevel: 'warning',
  outfile: join(dist, 'renderer.js'),
};

function copyStatic() {
  for (const f of ['index.html', 'styles.css']) {
    copyFileSync(join(root, 'src/app', f), join(dist, f));
    copyFileSync(join(root, 'src/app', f), join(web, f));
  }
  for (const f of ['renderer.js', 'renderer.js.map']) if (existsSync(join(dist, f))) copyFileSync(join(dist, f), join(web, f));
}

const nodeOpts = {
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: ['node20'],
  sourcemap: true,
  minify,
  external: ['electron', 'bufferutil', 'utf-8-validate'], // optional native speedups of `ws` (it falls back to JS)
  logLevel: 'warning',
};

if (watch) {
  const ctx = await esbuild.context({ ...rendererOpts, plugins: [{ name: 'copy', setup(b) { b.onEnd(() => { copyStatic(); console.log('[build] renderer rebuilt'); }); } }] });
  await ctx.watch();
  console.log('[build] watching src/ …');
} else {
  await Promise.all([
    esbuild.build(rendererOpts),
    esbuild.build({ ...nodeOpts, entryPoints: [join(root, 'src/electron/main.ts')], outfile: join(dist, 'main.cjs') }),
    esbuild.build({ ...nodeOpts, entryPoints: [join(root, 'src/electron/preload.ts')], outfile: join(dist, 'preload.cjs') }),
  ]);
  copyStatic();
  console.log(`[build] dist/ (electron) + dist/web/ (browser) in ${Date.now() - t0} ms${minify ? ' (minified)' : ''}`);
}
