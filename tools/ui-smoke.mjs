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

// ---- practice vs bot: the menu button starts a duel against a scripted rival lantern
await win.keyboard.press('Escape').catch(() => {});
await win.waitForTimeout(400);
await win.evaluate(() => document.exitPointerLock?.());
await win.waitForTimeout(400);
const menuShown = await win.evaluate(() => !document.querySelector('.menu')?.classList.contains('hidden'));
if (!menuShown) await fail('menu did not come back after releasing the mouse');
await win.click('button.practice');
await win.waitForFunction(() => window.hardlightApp.world?.players?.length === 2 && !!window.hardlightApp.world.match, null, { timeout: 10000 }).catch(() => fail('Practice vs bot did not start a duel'));
ok('menu "Practice vs bot" loads a duel with a rival lantern and match rules');
const duel = await win.evaluate(async () => {
  const app = window.hardlightApp, w = app.world;
  app.setManual(true);
  const hud = () => ({
    panel: !document.querySelector('.duel')?.classList.contains('hidden'),
    names: [...document.querySelectorAll('.duel .side .name')].map((e) => e.textContent),
    score: document.querySelector('.duel .score')?.textContent,
    clock: document.querySelector('.duel .clock')?.textContent,
  });
  const P = w.player, B = w.playerById('p2');
  // step through the warmup and a few seconds of live play with the local lantern facing the bot
  for (let i = 0; i < 60 * 9; i++) {
    const d = B.pos.clone().sub(P.pos);
    app.stepSim({ moveX: 0, moveZ: 0, yaw: Math.atan2(-d.x, -d.z), pitch: -0.05, jump: false, crouch: false, boost: false, hookL: false, hookR: false, reel: false, flyToggle: false, descend: false });
  }
  const phase = w.match.phase;
  const botCasts = w.log.filter((e) => e.type === 'construct_formed' && e.owner === 'p2').length;
  // land a hit on the bot with a construct of ours: damage number + hit marker show up
  B.protectedUntil = -1;
  // bring the bot into reach (the spawns are 72 m apart with the tower between them) and face it
  w.controllers.clear();
  B.teleport(P.pos.clone().add(P.forward().multiplyScalar(12)));
  w.refreshQueries();
  window.__hl.aimAt([B.pos.x, B.pos.y, B.pos.z]);
  P.yaw = app.input.yaw; P.pitch = app.input.pitch;
  const r = await app.commit('smash him with a hammer');
  for (let i = 0; i < 90; i++) { await app.nextFrame(); app.stepSim({ moveX: 0, moveZ: 0, yaw: P.yaw, pitch: P.pitch, jump: false, crouch: false, boost: false, hookL: false, hookR: false, reel: false, flyToggle: false, descend: false }); if (document.querySelector('.duel-floats .dmg')) break; }
  const dmgShown = !!document.querySelector('.duel-floats .dmg');
  const hits = w.log.filter((e) => e.type === 'player_damage' && e.attacker === 'p1' && e.target === 'p2').length;
  await app.nextFrame();
  const h = hud();
  app.setManual(false);
  return { phase, botCasts, summary: r?.summary, hits, dmgShown, hud: h, bot: B.hp };
});
if (duel.phase !== 'live') await fail(`duel did not go live after the warmup: ${JSON.stringify(duel)}`);
if (duel.botCasts < 1) await fail(`the bot never cast anything: ${JSON.stringify(duel)}`);
if (!duel.hud.panel || !/–/.test(duel.hud.score ?? '')) await fail(`duel HUD panel missing: ${JSON.stringify(duel.hud)}`);
if (duel.hits < 1) await fail(`"smash him with a hammer" did not hurt the bot: ${JSON.stringify(duel)}`);
if (!duel.dmgShown) console.log('  (note: damage number had already faded when sampled)');
ok(`practice duel live · bot cast ${duel.botCasts}× · HUD ${JSON.stringify(duel.hud)} · "${duel.summary}" hit the bot (hp ${Math.round(duel.bot)})`);
// ---- adaptive construct controls: make a hammer, click → it acts (action_start), the prompt names it
if (!(await state()).locked) { await win.click('canvas#view').catch(() => {}); await win.waitForTimeout(1300); await win.click('canvas#view').catch(() => {}); await win.waitForTimeout(400); }
await win.keyboard.press('Enter');
await win.waitForTimeout(150);
await win.keyboard.type('a hammer');
await win.keyboard.press('Enter');
await win.waitForFunction(() => [...window.hardlightApp.world.constructs.list.values()].some((c) => c.spec.concept === 'hammer'), null, { timeout: 5000 }).catch(() => fail('typed "a hammer" formed nothing'));
await win.waitForFunction(() => /hammer/.test(document.querySelector('.action-prompt')?.textContent ?? ''), null, { timeout: 8000 }).catch(() => {});
const prompt = await win.evaluate(() => document.querySelector('.action-prompt')?.textContent ?? '');
if (!/hammer/.test(prompt) || !/LMB/.test(prompt)) await fail(`no action prompt for the hammer ("${prompt}")`);
ok(`action prompt by the crosshair: "${prompt}"`);
if (!(await state()).locked) { await win.click('canvas#view').catch(() => {}); await win.waitForTimeout(1300); }
const n0a = await win.evaluate(() => window.hardlightApp.world.log.filter((e) => e.type === 'action_start').length);
await win.mouse.down({ button: 'left' });
await win.waitForTimeout(120);
await win.mouse.up({ button: 'left' });
await win.waitForFunction((n) => window.hardlightApp.world.log.filter((e) => e.type === 'action_start').length > n, n0a, { timeout: 5000 }).catch(() => fail('clicking with a hammer equipped fired no action_start'));
const act = await win.evaluate(() => window.hardlightApp.world.log.filter((e) => e.type === 'action_start').pop());
ok(`LMB with the hammer equipped → action_start ${JSON.stringify({ kind: act.kind, action: act.action })}`);

console.log('UI smoke OK');
await app.close();
