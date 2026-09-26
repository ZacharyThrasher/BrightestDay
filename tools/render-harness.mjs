// Shared Playwright harness for the web build (screenshots + perf). Headless Chromium + SwiftShader.
import { chromium } from 'playwright-core';
import { serve, portOf } from './serve.mjs';

export async function launch({ width = 1280, height = 720, query = '' } = {}) {
  const server = await serve(undefined, 0);
  const port = portOf(server);
  const browser = await chromium.launch({
    headless: true,
    args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--no-sandbox'],
  });
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') errors.push(`[${m.type()}] ${m.text()}`); if (process.env.HL_VERBOSE) console.log('[page]', m.text()); });
  page.on('pageerror', (e) => errors.push(`[pageerror] ${e.message}`));
  await page.goto(`http://127.0.0.1:${port}/?test=1${query ? '&' + query : ''}`);
  for (let i = 0; i < 600; i++) { if (await page.evaluate(() => !!(window.__hl && window.__hl.world))) break; await new Promise((r) => setTimeout(r, 250)); }
  if (!(await page.evaluate(() => !!window.__hl))) throw new Error('app did not boot: ' + errors.join(' | '));
  await page.evaluate(() => window.__hl.frames(3));
  const close = async () => { await browser.close(); server.close(); };
  return { page, browser, errors, close };
}

/** Step the sim in chunks (keeps each evaluate short) and render a few frames. */
export async function step(page, ticks, input = {}) {
  await page.evaluate(([n, inp]) => window.__hl.step(n, inp), [ticks, input]);
}

export async function frames(page, n = 2) { await page.evaluate((k) => window.__hl.frames(k), n); }
