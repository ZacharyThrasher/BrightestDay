#!/usr/bin/env node
// Tiny static server for the web build (no dependencies). Usage: node tools/serve.mjs [dir] [port]
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { join, extname, normalize, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const isMain = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
const dir = join(root, (isMain && process.argv[2]) || 'dist/web');
const port = Number((isMain && process.argv[3]) || process.env.PORT || 5173);
const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.map': 'application/json', '.json': 'application/json', '.wasm': 'application/wasm', '.png': 'image/png' };

export function serve(d = dir, p = port) {
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url ?? '/', 'http://x');
      let path = normalize(decodeURIComponent(url.pathname)).replace(/^([/\\])+/, '');
      if (path.includes('..')) { res.writeHead(400); return res.end(); }
      let file = join(d, path || 'index.html');
      if ((await stat(file).catch(() => null))?.isDirectory()) file = join(file, 'index.html');
      const body = await readFile(file);
      res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream', 'cache-control': 'no-store' });
      res.end(body);
    } catch {
      res.writeHead(404); res.end('not found');
    }
  });
  return new Promise((resolve) => server.listen(p, '127.0.0.1', () => resolve(server)));
}

/** Port the server actually bound (useful with port 0). */
export function portOf(server) {
  return server.address().port;
}

if (isMain) {
  await serve();
  console.log(`[serve] http://127.0.0.1:${port}/  (${dir})`);
}
