// Two-instance online smoke test (run after `npm run build`):
//   xvfb-run -a node tools/net-smoke.mjs
// Instance A opens "Play online" → "Host on LAN" (no tunnel); instance B joins via ws://127.0.0.1:<port>.
// Both lobbies must reach "connected", receive each other's greeting and show a ping.
import { _electron as electron } from 'playwright-core';
import { createRequire } from 'node:module';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const electronPath = createRequire(import.meta.url)('electron');
const dirs = [mkdtempSync(join(tmpdir(), 'hl-net-a-')), mkdtempSync(join(tmpdir(), 'hl-net-b-'))];
const launch = (dir) => electron.launch({ executablePath: electronPath, args: ['.', '--no-sandbox', '--software-gl', `--user-data-dir=${dir}`] });
const apps = [];
const cleanup = async () => { for (const a of apps) await a.close().catch(() => {}); for (const d of dirs) rmSync(d, { recursive: true, force: true }); };
const fail = async (msg) => { console.error('NET SMOKE FAIL:', msg); await cleanup(); process.exit(1); };
const timer = setTimeout(() => fail('timed out'), 120000);

try {
  apps.push(await launch(dirs[0]), await launch(dirs[1]));
  const [a, b] = await Promise.all(apps.map((x) => x.firstWindow()));
  for (const w of [a, b]) await w.waitForFunction(() => document.body.classList.contains('ready'), null, { timeout: 60000 });
  const lobby = (w) => w.evaluate(() => { const l = document.querySelector('.lobby'); return l && { state: l.dataset.state, status: l.querySelector('.net-status')?.textContent, address: l.dataset.address, greeted: l.dataset.greeted, code: l.querySelector('.code')?.textContent }; });

  await a.click('button.online');
  await a.click('.lobby button.host-lan');
  await a.waitForFunction(() => document.querySelector('.lobby')?.dataset.state === 'waiting', null, { timeout: 10000 }).catch(async () => fail(`host never ready: ${JSON.stringify(await lobby(a))}`));
  const host = await lobby(a);
  console.log('host lobby:', JSON.stringify(host));
  if (!/^ws:\/\/127\.0\.0\.1:\d+$/.test(host.address ?? '')) await fail(`no host address: ${JSON.stringify(host)}`);

  await b.click('button.online');
  await b.click('.lobby button.join-open');
  await b.fill('.lobby input[name=code]', host.address);
  await b.click('.lobby button.join');
  for (const [w, who] of [[a, 'host'], [b, 'joiner']]) {
    await w.waitForFunction(() => { const l = document.querySelector('.lobby'); return l?.dataset.state === 'connected' && !!l.dataset.greeted && /ping \d+ ms/.test(l.querySelector('.net-status')?.textContent ?? ''); }, null, { timeout: 10000 })
      .catch(async () => fail(`${who} not connected: ${JSON.stringify(await lobby(w))}`));
  }
  const [la, lb] = [await lobby(a), await lobby(b)];
  console.log('host  :', la.status, '| greeted by', la.greeted);
  console.log('joiner:', lb.status, '| greeted by', lb.greeted);

  // cancel on the joiner → host goes back to waiting
  await b.click('.lobby button.cancel');
  await a.waitForFunction(() => document.querySelector('.lobby')?.dataset.state === 'waiting', null, { timeout: 5000 }).catch(() => fail('host did not notice the joiner leaving'));
  await b.waitForFunction(() => !document.querySelector('button.play')?.classList.contains('hidden') && document.querySelector('.lobby')?.classList.contains('hidden'), null, { timeout: 5000 })
    .catch(() => fail('cancel did not restore the menu'));
  console.log('NET smoke OK: LAN host + join over ws://127.0.0.1, greetings exchanged, ping shown, leave detected');
  clearTimeout(timer);
  await cleanup();
} catch (err) {
  await fail(err?.stack ?? String(err));
}
