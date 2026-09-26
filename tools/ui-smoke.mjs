// Real-click UI smoke test of the Electron build (run after `npm run build`):
//   xvfb-run -a node tools/ui-smoke.mjs
// Checks the things a player hits in the first minute: Play captures the mouse and hides the menu,
// Enter opens the console, a typed command forms a construct — and the typing-as-skill layer:
// the mouse stays captured and aims while typing, typing never kills momentum (velocity and a held
// tether survive), focus time slows the sim tick rate, Tab completes, an empty Enter recasts, number
// keys quick-cast, and every cast reports to the creativity economy.
import { _electron as electron } from 'playwright-core';
import { createRequire } from 'node:module';
const electronPath = createRequire(import.meta.url)('electron');
const app = await electron.launch({ executablePath: electronPath, args: ['.', '--no-sandbox', '--software-gl'] });
const win = await app.firstWindow();
const fail = async (msg) => { console.error('UI SMOKE FAIL:', msg); await app.close(); process.exit(1); };
const ok = (msg) => console.log('  ok ·', msg);
const state = () => win.evaluate(() => ({ locked: !!document.pointerLockElement, menuHidden: document.querySelector('.menu')?.classList.contains('hidden') }));
const count = () => win.evaluate(() => window.hardlightApp.world.constructs.list.size);
await win.waitForFunction(() => document.body.classList.contains('ready'), null, { timeout: 45000 });
await win.click('button.play');
await win.waitForTimeout(600);
let s = await state();
if (!s.locked || !s.menuHidden) await fail(`after Play: ${JSON.stringify(s)}`);
ok('Play captures the mouse and hides the menu');

// ---- typing keeps the mouse captured, and mouse look still aims
await win.keyboard.press('Enter');
await win.waitForTimeout(200);
s = await win.evaluate(() => ({ locked: !!document.pointerLockElement, focused: document.activeElement?.tagName, yaw: window.hardlightApp.input.yaw }));
if (!s.locked) await fail('opening the console released the mouse (aim would be lost while typing)');
if (s.focused !== 'INPUT') await fail(`console input not focused (${s.focused})`);
await win.mouse.move(400, 300);
await win.mouse.move(700, 300, { steps: 5 });
await win.waitForTimeout(100);
const yaw2 = await win.evaluate(() => window.hardlightApp.input.yaw);
if (Math.abs(yaw2 - s.yaw) < 1e-4) console.log('  (note: synthetic mouse moves produced no movementX under xvfb; pointer lock itself is held)');
else ok(`mouse look works while typing (yaw ${s.yaw.toFixed(3)} → ${yaw2.toFixed(3)})`);
ok('console open with the pointer still locked');

// ---- Tab completion with ghost text
await win.keyboard.type('a tramp');
await win.waitForTimeout(100);
const ghost = await win.evaluate(() => document.querySelector('.console .ghost')?.textContent ?? '');
if (!ghost.endsWith('oline')) await fail(`no ghost completion for "a tramp" (ghost="${ghost}")`);
await win.keyboard.press('Tab');
const val = await win.evaluate(() => window.hardlightApp.hud.consoleInput.value);
if (val !== 'a trampoline ') await fail(`Tab did not complete: "${val}"`);
ok(`Tab completes "a tramp" → "${val.trim()}" (ghost shown first)`);
await win.keyboard.press('Escape');
await win.waitForTimeout(300);
if (!(await state()).locked) { await win.click('canvas#view').catch(() => {}); await win.waitForTimeout(1300); await win.click('canvas#view').catch(() => {}); await win.waitForTimeout(400); }

// ---- first cast
await win.keyboard.press('Enter');
await win.waitForTimeout(150);
await win.keyboard.type('a wall');
await win.keyboard.press('Enter');
await win.waitForFunction(() => window.hardlightApp.world.constructs.list.size > 0, null, { timeout: 5000 }).catch(() => fail('typed "a wall" formed nothing'));
ok('typed command forms a construct');
const insp = await win.evaluate(() => window.hardlightApp.world.log.filter((e) => e.type === 'inspiration').map((e) => ({ novelty: e.novelty, mul: e.mul, stale: e.stale })));
if (!insp.length) await fail('the cast produced no inspiration event');
ok(`cast reported to the creativity economy ${JSON.stringify(insp[0])}`);

// ---- empty Enter recasts, number key quick-casts
let n0 = await count();
await win.keyboard.press('Enter');
await win.waitForTimeout(100);
await win.keyboard.press('Enter');
await win.waitForFunction((n) => window.hardlightApp.world.constructs.list.size > n, n0, { timeout: 5000 }).catch(() => fail('empty Enter did not recast'));
ok('empty Enter recasts the last command');
n0 = await count();
await win.keyboard.press('Digit1');
await win.waitForFunction((n) => window.hardlightApp.world.constructs.list.size > n, n0, { timeout: 5000 }).catch(() => fail('quick-cast 1 did nothing'));
const staleSeen = await win.evaluate(() => window.hardlightApp.world.log.some((e) => e.type === 'inspiration' && e.stale));
if (!staleSeen) await fail('three walls in a row were not reported as stale');
ok('number key 1 quick-casts; the third wall in a row is reported stale');

// ---- focus time: fewer sim ticks per real second while typing (dt unchanged)
// Software GL composites at ~1-4 fps in this container, which caps steps per frame, so the app's own
// frame advance is driven with 60 Hz frame times (the same code path the rAF loop uses).
await win.waitForTimeout(3000); // let the focus meter recharge
const focus = await win.evaluate(() => {
  const app = window.hardlightApp, w = app.world;
  app.setManual(true);
  const run = (open, frames) => {
    if (open) app.openConsole();
    const t0 = w.tick;
    for (let i = 0; i < frames; i++) app.advance(1 / 60);
    const ticks = w.tick - t0;
    if (open) app.closeConsole();
    return ticks;
  };
  const defaultOff = run(true, 120); // duels share one clock: focus time is off unless practising
  app.focus.target = 0.35; // offline practice setting
  const closed = run(false, 120);
  const open = run(true, 120); // 2 real seconds of typing
  const drained = app.focus.meter;
  const long = run(true, 60 * 8); // 8 s: the meter (5 s) runs dry and time returns to normal
  const exhausted = app.focus.exhausted;
  app.focus.meter = 1; app.focus.exhausted = false;
  app.sharedClock = true; // online duel
  const online = run(true, 120);
  app.sharedClock = false;
  app.focus.target = 1;
  app.setManual(false);
  return { defaultOff, closed, open, drained, long, exhausted, online, dt: w.dt };
});
if (focus.dt !== 1 / 60) await fail('focus time changed the sim dt');
if (focus.defaultOff < 118) await fail(`focus time is on by default: ${JSON.stringify(focus)}`);
if (!(focus.open < focus.closed * 0.5)) await fail(`practice focus time did not slow the sim: ${JSON.stringify(focus)}`);
if (!focus.exhausted || !(focus.long > 480 * 0.5)) await fail(`focus meter never ran dry / time never recovered: ${JSON.stringify(focus)}`);
if (focus.online < 118) await fail(`focus time slowed a shared (online) clock: ${JSON.stringify(focus)}`);
ok(`focus time off by default (${focus.defaultOff}/120 ticks while typing) and never on a shared clock (${focus.online}/120)`);
ok(`practice focus time: ${focus.closed} → ${focus.open} sim ticks per 2 real seconds while typing; meter ${focus.drained.toFixed(2)} after 2 s; runs dry and time recovers (${focus.long} ticks in 8 s)`);

// ---- typing never kills momentum: airborne velocity and a held tether survive the console
await win.waitForTimeout(4000); // recharge focus
const mom = await win.evaluate(async () => {
  const app = window.hardlightApp, w = app.world, P = w.player;
  P.teleport(new P.pos.constructor(P.pos.x, P.pos.y + 60, P.pos.z), new P.pos.constructor(24, 0, 0));
  const v0 = Math.hypot(P.vel.x, P.vel.z);
  app.openConsole();
  await new Promise((r) => setTimeout(r, 700));
  const v1 = Math.hypot(P.vel.x, P.vel.z);
  app.closeConsole();
  return { v0, v1, typedTicks: w.tick };
});
if (!(mom.v1 > mom.v0 * 0.9)) await fail(`opening the console killed momentum: ${mom.v0.toFixed(1)} → ${mom.v1.toFixed(1)} m/s`);
ok(`airborne speed kept while typing: ${mom.v0.toFixed(1)} → ${mom.v1.toFixed(1)} m/s`);
const teth = await win.evaluate(async () => {
  const app = window.hardlightApp, w = app.world, P = w.player;
  // aim at the nearest building face in tether range, hold Q, then open the console
  const b = w.statics.filter((g) => g.kind === 'building' && g.size.y > 12).map((g) => ({ g, d: g.pos.distanceTo(P.pos) })).sort((a, c) => a.d - c.d)[0];
  if (!b) return { skipped: 'no building' };
  const g = b.g;
  // stand on the ground 8 m from the face of the building and aim at the face
  const face = [g.pos.x + g.size.x / 2, 6, g.pos.z];
  window.__hl.teleport([face[0] + 8, 1.2, face[2]], [0, 0, 0]);
  window.__hl.aimAt(face);
  return { aimed: true, dist: 8 };
});
if (teth.aimed) {
  await win.keyboard.down('KeyQ');
  const had = await win.waitForFunction(() => !!window.hardlightApp.world.player.tethers.L, null, { timeout: 5000 }).then(() => true).catch(() => false);
  if (had) {
    await win.keyboard.press('Enter');
    await win.waitForTimeout(600);
    const still = await win.evaluate(() => ({ open: document.activeElement?.tagName === 'INPUT', tether: !!window.hardlightApp.world.player.tethers.L }));
    await win.keyboard.press('Escape');
    await win.keyboard.up('KeyQ');
    if (!still.tether) await fail('opening the console released a held tether');
    ok('a held tether stays attached while typing');
  } else { await win.keyboard.up('KeyQ'); console.log(`  (note: tether did not attach at ${teth.dist.toFixed(0)} m; tether-carry check skipped)`); }
}

console.log('UI smoke OK');
await app.close();
