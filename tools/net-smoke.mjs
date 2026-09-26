// Two-instance online smoke test (run after `npm run build`):
//   xvfb-run -a node tools/net-smoke.mjs
// Instance A opens "Play online" → "Host on LAN" (no tunnel); instance B joins via ws://127.0.0.1:<port>.
// Both lobbies must reach "connected", receive each other's greeting and show a ping. Then the duel starts on
// both (host = p1, joiner = p2), the joiner's lantern punches the host's (a cast the host runs), and both duel
// HUDs show the host's health dropping. Leaving (Cancel) pauses the host with an opponent-left banner.
import { _electron as electron } from 'playwright-core';
import { createRequire } from 'node:module';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const electronPath = createRequire(import.meta.url)('electron');
const dirs = [mkdtempSync(join(tmpdir(), 'hl-net-a-')), mkdtempSync(join(tmpdir(), 'hl-net-b-'))];
// two software-rendered windows share this machine: low graphics + small windows keep both sims near real time
for (const d of dirs) writeFileSync(join(d, 'settings.json'), JSON.stringify({ graphics: 'low' }));
const launch = async (dir) => {
  const app = await electron.launch({ executablePath: electronPath, args: ['.', '--no-sandbox', '--software-gl', `--user-data-dir=${dir}`] });
  await app.evaluate(({ BrowserWindow }) => { for (const w of BrowserWindow.getAllWindows()) w.setSize(800, 450); }).catch(() => {});
  return app;
};
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

  // ---- the duel: both sides load the arena and go live
  const online = (w) => w.evaluate(() => window.__hl?.online?.());
  for (const [w, who, me] of [[a, 'host', 'p1'], [b, 'joiner', 'p2']]) {
    await w.waitForFunction((me) => { const o = window.__hl?.online?.(); return o && o.me === me && o.phase === 'live' && !o.paused; }, me, { timeout: 20000 })
      .catch(async () => fail(`${who} never got a live online match: ${JSON.stringify(await online(w))}`));
  }
  console.log('match live on both:', JSON.stringify(await online(a)), '|', JSON.stringify(await online(b)));
  // spawn shields drop after 2 s; stand the lanterns 12 m apart in the open, the joiner facing the host
  await a.evaluate(() => { window.__hl.teleport([10, 1, 20]); window.__hl.look(0, -0.05); });
  await b.evaluate(() => { window.__hl.teleport([10, 1, 8]); window.__hl.look(Math.PI, -0.05); });
  await a.waitForFunction(() => !window.__hl.online().protected, null, { timeout: 5000 }).catch(() => {});
  await b.waitForFunction(() => { const o = window.__hl.online(); return !o.protected && !window.__hl.world.playerById('p1').isProtected; }, null, { timeout: 5000 }).catch(() => {});
  const hudHp = (w, side) => w.evaluate((side) => { const f = document.querySelector(`.duel .side.${side} .hp .fill`); return f ? parseFloat(f.style.width) : NaN; }, side);
  let hit = false;
  for (let attempt = 0; attempt < 3 && !hit; attempt++) {
    const r = await b.evaluate(() => window.__hl.commit('punch him'));
    console.log(`joiner cast "punch him" →`, JSON.stringify(r));
    if (!r?.applied) await fail(`the host refused the joiner's cast: ${JSON.stringify(r)}`);
    hit = await a.waitForFunction(() => window.__hl.online().hp.p1 < 100, null, { timeout: 4000 }).then(() => true, () => false);
    if (!hit) await b.evaluate(() => { window.__hl.teleport([10, 1, 12]); window.__hl.look(Math.PI, -0.05); });
  }
  if (!hit) await fail(`the punch never hurt the host: ${JSON.stringify(await online(a))}`);
  // both HUDs show it: the host's own bar and the joiner's rival bar
  await a.waitForFunction(() => parseFloat(document.querySelector('.duel .side.me .hp .fill')?.style.width ?? '100') < 100, null, { timeout: 3000 })
    .catch(async () => fail(`host HUD shows no damage (bar ${await hudHp(a, 'me')}%)`));
  await b.waitForFunction(() => parseFloat(document.querySelector('.duel .side.rival .hp .fill')?.style.width ?? '100') < 100, null, { timeout: 3000 })
    .catch(async () => fail(`joiner HUD shows no damage (bar ${await hudHp(b, 'rival')}%)`));
  const [oa, ob] = [await online(a), await online(b)];
  console.log(`damage on both HUDs: host bar ${await hudHp(a, 'me')}% (hp ${oa.hp.p1}), joiner's rival bar ${await hudHp(b, 'rival')}% (hp ${ob.hp.p1}); desyncs ${ob.stats.desyncs} of ${ob.stats.hashChecks} checks`);
  if (ob.stats.desyncs > 0) await fail(`client desynced: ${JSON.stringify(ob.stats)}`);

  // cancel on the joiner → host goes back to waiting, the match pauses with an opponent-left banner
  await b.evaluate(() => { window.__hl.app.input.exitLock?.(); document.exitPointerLock?.(); });
  await b.evaluate(() => document.querySelector('button.online')?.click());
  await b.click('.lobby button.cancel');
  await a.waitForFunction(() => document.querySelector('.lobby')?.dataset.state === 'waiting', null, { timeout: 5000 }).catch(() => fail('host did not notice the joiner leaving'));
  await b.waitForFunction(() => !document.querySelector('button.play')?.classList.contains('hidden') && document.querySelector('.lobby')?.classList.contains('hidden'), null, { timeout: 5000 })
    .catch(() => fail('cancel did not restore the menu'));
  await a.waitForFunction(() => window.__hl.online()?.paused && document.body.dataset.netPaused === '1', null, { timeout: 5000 }).catch(() => fail('host did not pause when the joiner left'));
  console.log('NET smoke OK: LAN host + join over ws://127.0.0.1, greetings, ping, duel live on both, joiner punched host, damage on both HUDs, leave pauses the host');
  clearTimeout(timer);
  await cleanup();
} catch (err) {
  await fail(err?.stack ?? String(err));
}
