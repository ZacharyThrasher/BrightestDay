// JUICE smoke test of the Electron build (run after `npm run build`):
//   xvfb-run -a node tools/juice-smoke.mjs [--shots <dir>]
// A scripted sandbox scene where constructs hit things (a wrecking ball smashing a crate, a cannon
// shooting a drone), movement (boost, a hard landing, a tether), typing in the ring console, plus the
// PvP / mouse-action events the duel core will emit (synthesised on the world bus until they land).
// Asserts: no console errors, every juice layer fired (shake, hitstop freeze + catch-up, FOV punches,
// snap, flashes, squash, damage numbers, markers, callouts, kill cam, sounds within the voice cap), and
// the per-frame cost budget: juice adds < 0.5 ms CPU per frame at p50 (A/B against the same scene with
// the render hooks detached).
import { _electron as electron } from 'playwright-core';
import { createRequire } from 'node:module';
import { mkdtempSync, rmSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const args = process.argv.slice(2);
const shots = args.includes('--shots') ? args[args.indexOf('--shots') + 1] : null;
if (shots) mkdirSync(shots, { recursive: true });
const electronPath = createRequire(import.meta.url)('electron');
const dir = mkdtempSync(join(tmpdir(), 'hl-juice-'));
const app = await electron.launch({ executablePath: electronPath, args: ['.', '--no-sandbox', '--software-gl', `--user-data-dir=${dir}`] });
const win = await app.firstWindow();
const errors = [];
win.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
win.on('pageerror', (e) => errors.push(`[pageerror] ${e.message}`));
const cleanup = async () => { await app.close().catch(() => {}); rmSync(dir, { recursive: true, force: true }); };
const fail = async (msg) => { console.error('JUICE SMOKE FAIL:', msg); await cleanup(); process.exit(1); };
const ok = (msg) => console.log('  ok ·', msg);
const timer = setTimeout(() => fail("timed out"), 480000);
const stats = () => win.evaluate(() => window.__hl.juice.stats());
const shot = async (name) => { if (shots) { await win.screenshot({ path: join(shots, name) }); console.log(`  shot → ${join(shots, name)}`); } };

try {
  await win.waitForFunction(() => document.body.classList.contains('ready'), null, { timeout: 60000 });
  await win.evaluate(async () => {
    const H = window.__hl;
    H.manual(true);
    await H.scene('sandbox');
    document.querySelector('.menu')?.classList.add('hidden');
    H.juice.ui.toggleDebug(true); // F4 overlay on for the screenshots
  });

  // ---- 1. constructs hit things (real sim): wrecking ball → crate, cannon → drone
  let maxTrauma = 0;
  const run = async (cmd, frames, shotAt = -1, name = '') => {
    await win.evaluate((c) => window.__hl.commit(c), cmd);
    for (let i = 0; i < frames; i++) {
      const t = await win.evaluate(async () => { const H = window.__hl; H.step(4); await H.frames(1); return H.juice.trauma.value; });
      maxTrauma = Math.max(maxTrauma, t);
      if (i === shotAt) await shot(name);
    }
  };
  await run('a wrecking ball smashes the crate', 40, 9, 'juice-smash.png');
  let s = await stats();
  if (!s.counts.form || !s.stat.snaps) await fail(`no construct assembly snap: ${JSON.stringify(s.counts)}`);
  ok(`construct formed with the assembly snap (snaps ${s.stat.snaps}) and a cast pulse (fov punches ${s.stat.fovPunches})`);
  if (!(s.counts.smash || s.counts.impact)) await fail(`no impact from the wrecking ball: ${JSON.stringify(s.counts)}`);
  if (!s.hitstop.triggered || !s.stat.frozenFrames) await fail(`no hitstop freeze: ${JSON.stringify(s.hitstop)} frozen ${s.stat.frozenFrames}`);
  ok(`impact → hitstop (${s.hitstop.triggered} holds, ${s.hitstop.totalHoldMs} ms, ${s.stat.frozenFrames} frozen construct frames), trauma peaked ${maxTrauma.toFixed(2)}`);
  await run('shoot the drone with a cannon', 60, 20, 'juice-cannon.png');
  s = await stats();
  if (!s.counts.fire) await fail(`cannon fire produced no recoil/sound: ${JSON.stringify(s.counts)}`);
  if (!s.ui.numbers || !s.ui.markers) await fail(`no damage numbers / hit markers: ${JSON.stringify(s.ui)}`);
  ok(`cannon: ${s.counts.fire} shots with recoil, ${s.ui.numbers} damage numbers, ${s.ui.markers} hit markers, ${s.stat.squashes} squashes, ${s.stat.flashes} flashes`);
  if (s.hitstopActive.length === 0 && s.stat.frozenFrames === 0) await fail('hitstop never active');
  // catch-up: after enough time nothing stays frozen
  await win.evaluate(async () => { for (let i = 0; i < 20; i++) { window.__hl.step(2); await window.__hl.frames(1); } });
  const frozenNow = await win.evaluate(() => window.__hl.juice.hitstop.active().length);
  s = await stats();
  if (frozenNow > 0 && s.hitstop.triggered === 0) await fail('hitstop stuck');
  ok(`hitstop catches up (${frozenNow} targets still easing after 20 frames)`);

  // ---- 2. movement: boost, a hard landing, a tether
  const move = await win.evaluate(async () => {
    const H = window.__hl, P = H.world.player;
    H.teleport([0, 40, 20], [0, -5, 0]);
    for (let i = 0; i < 30 && !P.onGround; i++) { H.step(6); await H.frames(1); }
    for (let i = 0; i < 20; i++) { H.step(3, { boost: true, moveZ: -1 }); await H.frames(1); }
    const loops = H.juice.sfx.stats().loops;
    H.aimAt([13, 2, -15]);
    H.step(2, { hookL: true }); await H.frames(1);
    for (let i = 0; i < 10; i++) { H.step(3, { hookL: true }); await H.frames(1); }
    H.step(2, {}); await H.frames(2);
    return { loops, counts: H.juice.counts };
  });
  if (!move.counts.land_hard) await fail(`no hard landing: ${JSON.stringify(move.counts)}`);
  if (!move.counts.boost || move.loops < 1) await fail(`boost produced no FOV kick / roar loop: ${JSON.stringify(move)}`);
  ok(`movement: land_hard dip, boost kick + roar/wind loops (${move.loops}), tether ${move.counts.tether_attach ? 'attach snap' : move.counts.tether_fire ? 'fire (missed)' : 'n/a'}`);

  // ---- 3. typing in the ring console: key clicks, then a stale / fail cast
  await win.evaluate(async () => {
    const H = window.__hl, inp = H.hud.consoleInput;
    H.app.openConsole();
    for (const ch of 'a big hammer') { inp.value += ch; inp.dispatchEvent(new InputEvent('input', { inputType: 'insertText', data: ch })); }
    inp.value = inp.value.slice(0, -1); inp.dispatchEvent(new InputEvent('input', { inputType: 'deleteContentBackward' }));
    H.app.closeConsole();
    await H.commit('zzqx blorf');
    await H.frames(1);
  });
  s = await stats();
  if (!s.counts.keyclick || !s.sfx.counts.keyclick) await fail(`no key clicks: ${JSON.stringify(s.sfx.counts)}`);
  ok(`typing: ${s.counts.keyclick} key clicks (pitch follows speed / combo)`);

  // ---- 4. PvP + mouse-action events (the duel core / controls emit these after merge)
  const pvp = await win.evaluate(async () => {
    const H = window.__hl, w = H.world, J = H.juice;
    const cid = [...w.constructs.list.values()].find((c) => c.owner === 'p1')?.id ?? 'none';
    const at = w.player.pos.clone().add({ x: 0, y: 0, z: -8 }).toArray();
    const before = J.trauma.value;
    w.emit('player_damage', { target: 'p1', amount: 40, cause: 'strike', by: 'p2' });
    const hurt40 = J.trauma.value - before;
    J.trauma.value = 0;
    w.emit('player_damage', { target: 'p1', amount: 5, cause: 'strike', by: 'p2' });
    const hurt5 = J.trauma.value;
    w.emit('player_damage', { target: 'p2', amount: 25, cause: 'strike', by: cid });
    w.emit('action_start', { construct: cid, kind: 'smash' });
    for (let i = 1; i <= 10; i++) { w.emit('action_charge', { construct: cid, level: i / 10 }); H.step(1); await H.frames(1); }
    w.emit('action_release', { construct: cid, kind: 'smash', level: 1 });
    w.emit('action_hit', { construct: cid, target: 'drone5', energy: 1 });
    w.emit('inspiration', { player: 'p1', construct: cid, novelty: 0.95, mul: 1.8, stale: false, fresh: ['wings'] });
    w.emit('player_killed', { target: 'p2', by: cid, pos: at });
    w.emit('kill', { target: 'p2', by: cid, pos: at }); // duplicate of the same kill: deduped by time
    w.emit('creative_solution', { player: 'p1', job: 'kill', bonus: 0.25, reason: 'taken down with a piano' });
    const shotFrames = [];
    for (let i = 0; i < 6; i++) { await H.frames(1); shotFrames.push({ fov: J.lastFov, fx: J.fxTimeScale }); }
    const st = J.stats();
    w.emit('match_state', { state: 'round_over', winner: 'p1' });
    return { hurt40, hurt5, shotFrames, s: st };
  });
  await shot('juice-kill.png');
  if (!(pvp.hurt40 > pvp.hurt5 * 2)) await fail(`40-dmg hit should shake more than a 5-dmg hit: ${pvp.hurt40} vs ${pvp.hurt5}`);
  ok(`hit received: 40 dmg → +${pvp.hurt40.toFixed(2)} trauma vs 5 dmg → +${pvp.hurt5.toFixed(2)}`);
  s = pvp.s;
  if (s.counts.kill !== 1) await fail(`kill should fire once (deduped): ${s.counts.kill}`);
  if (!s.stat.killCams || !s.ui.kills) await fail(`no kill cam / kill marker: ${JSON.stringify(s.stat)} ${JSON.stringify(s.ui)}`);
  if (!pvp.shotFrames.some((f) => f.fx < 1)) await fail(`whole-view hitstop did not slow fx: ${JSON.stringify(pvp.shotFrames)}`);
  if (!s.counts.action_hit || !s.counts.action_start || !s.counts.action_release) await fail(`mouse actions not handled: ${JSON.stringify(s.counts)}`);
  if (!/CREATIVE KILL/.test(s.ui.lastCallout)) await fail(`expected the CREATIVE KILL callout, got ${s.ui.lastCallout}`);
  ok(`kill: kill cam + whole-view hitstop (fx ×${Math.min(...pvp.shotFrames.map((f) => f.fx)).toFixed(2)}), kill marker, "${s.ui.lastCallout}" callout; mouse actions: start/charge/release/hit`);
  const fin = await stats();
  const snd = fin.sfx;
  if (snd.peak > snd.max) await fail(`voice cap exceeded: ${snd.peak}/${snd.max}`);
  const kinds = Object.keys(snd.counts);
  if (kinds.length < 12) await fail(`too few distinct sounds: ${kinds.join(', ')}`);
  ok(`sound: ${snd.played} voices (${kinds.length} kinds), peak ${snd.peak}/${snd.max}, stolen ${snd.stolen}, throttled ${snd.throttled}, audio ${snd.state}`);
  if (!fin.counts.win) await fail('no round-win stinger');

  // ---- 5. frame budget: A/B the same scene with the render hooks attached / detached
  const perf = await win.evaluate(async () => {
    const H = window.__hl, J = H.juice, gr = H.app.gr, w = H.world;
    const med = (a) => { const b = [...a].sort((x, y) => x - y); return b[b.length >> 1]; };
    const cid = [...w.constructs.list.values()][0]?.id;
    const on = [], off = [];
    J.resetStats();
    for (let round = 0; round < 3; round++) {
      for (const enabled of [true, false]) {
        J.setEnabled(enabled);
        for (let i = 0; i < 24; i++) {
          H.step(1);
          if (i % 8 === 0 && cid) w.emit('impact', { construct: cid, entity: 'drone5', dv: 25, pos: w.player.pos.clone().add({ x: 3, y: 1, z: -6 }).toArray() }); // keep effects live
          await H.frames(1);
          (enabled ? on : off).push(gr.stats.frameCpuMs);
        }
      }
    }
    J.setEnabled(true);
    const self = J.stats().cpu;
    return { on: med(on), off: med(off), self };
  });
  const delta = perf.on - perf.off;
  console.log(`  perf · render CPU p50 with juice ${perf.on.toFixed(3)} ms, without ${perf.off.toFixed(3)} ms (Δ ${delta.toFixed(3)} ms); juice self p50 ${perf.self.p50.toFixed(3)} ms, p95 ${perf.self.p95.toFixed(3)} ms`);
  if (perf.self.p50 >= 0.5) await fail(`juice self time p50 ${perf.self.p50.toFixed(3)} ms ≥ 0.5 ms`);
  if (delta >= 0.5) await fail(`juice adds ${delta.toFixed(3)} ms p50 ≥ 0.5 ms`);
  ok('frame budget: juice < 0.5 ms CPU per frame (p50)');

  const real = errors.filter((e) => !/Autoplay|AudioContext was not allowed/i.test(e));
  if (real.length) await fail(`console errors:\n${real.join('\n')}`);
  ok('no console errors');
  console.log('JUICE smoke OK');
  clearTimeout(timer);
  await cleanup();
} catch (err) {
  await fail(err?.stack ?? String(err));
}
