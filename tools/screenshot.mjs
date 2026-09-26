#!/usr/bin/env node
// Scripted visual QA: drives the web build deterministically (sim stepped from the script, not
// real time) and saves screenshots to docs/screenshots/. Requires `npm run build` first.
//   node tools/screenshot.mjs [--only=name,name] [--quality=low|high] [--size=1600x900]
import { launch, step, frames } from './render-harness.mjs';
import { mkdirSync } from 'node:fs';

const args = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, '').split('=')));
const [W, H] = (args.size ?? '1600x900').split('x').map(Number);
const only = args.only ? new Set(args.only.split(',')) : null;
const outDir = args.out ?? 'docs/screenshots';
mkdirSync(outDir, { recursive: true });

const { page, errors, close } = await launch({ width: W, height: H, query: args.quality ? `graphics=${args.quality}` : '' });
const hl = (fn, arg) => page.evaluate(fn, arg);
const shots = [];
async function shot(name, settle = 3) {
  if (only && !only.has(name)) return;
  await frames(page, settle);
  const path = `${outDir}/${name}.png`;
  await page.screenshot({ path });
  const st = await hl(() => window.__hl.stats());
  shots.push({ name, calls: st.sceneCalls, tris: st.sceneTriangles });
  console.log(`  ${name.padEnd(22)} scene calls ${st.sceneCalls}  tris ${st.sceneTriangles}`);
}
const want = (...names) => !only || names.some((n) => only.has(n));

console.log(`[shots] ${W}x${H}`);

// 1. spawn in the playground plaza
if (want('01_plaza')) {
  await hl(() => window.__hl.scene('playground'));
  await hl(() => window.__hl.look(0, -0.05));
  await step(page, 40);
  await shot('01_plaza');
}

// 2. sprint + boost down the avenue: speed lines, FOV widening, thruster
if (want('02_boost')) {
  await hl(() => window.__hl.scene('playground'));
  await hl(() => window.__hl.look(0, 0.02));
  await step(page, 50, { moveZ: 1 });
  await step(page, 40, { moveZ: 1, boost: true, jump: true });
  await step(page, 14, { moveZ: 1, boost: true });
  await shot('02_boost', 8);
}

// 3. tether swing between buildings
if (want('03_tether')) {
  await hl(() => window.__hl.scene('playground'));
  await hl(() => window.__hl.look(0, 0));
  await step(page, 50, { moveZ: 1 });
  await step(page, 6, { moveZ: 1, jump: true });
  await hl(() => window.__hl.aimAt([30, 40, -70]));
  await step(page, 10, { moveZ: 1, hookR: true });
  await hl(() => window.__hl.aimAt([-30, 45, -100]));
  await step(page, 24, { moveZ: 1, hookR: true, hookL: true, reel: true });
  await hl(() => window.__hl.look(window.__hl.world.player.yaw, 0.05));
  await step(page, 2, { moveZ: 1, hookR: true, hookL: true, reel: true });
  await shot('03_tether');
}

// 4-6. imagine a ramp: ghost preview → assembly → formed, then launch off it
if (want('04_preview', '05_assembly', '06_ramp')) {
  await hl(() => window.__hl.scene('playground'));
  await hl(() => window.__hl.look(Math.PI * 0.12, -0.12));
  await step(page, 30);
  await hl(() => window.__hl.preview('make a giant ramp'));
  await step(page, 2);
  await shot('04_preview');
  await hl(() => window.__hl.closeConsole());
  await hl(() => window.__hl.commit('make a giant ramp'));
  await step(page, 1);
  await shot('05_assembly', 1);
  await step(page, 30);
  await shot('06_ramp', 8);
}

// 7. barrage: a wall between me and the turret
if (want('07_wall')) {
  await hl(() => window.__hl.scene('barrage'));
  await hl(() => window.__hl.look(0.35, -0.02));
  await step(page, 100);
  await hl(() => window.__hl.commit('a wall between me and the turret'));
  await step(page, 150);
  await hl(() => window.__hl.look(0.28, 0.0));
  await step(page, 2);
  await shot('07_wall', 4);
}

// 8. drones: rocket boxing glove strike (behaviour composed directly; the stub interpreter can't yet)
if (want('08_glove')) {
  await hl(() => window.__hl.scene('drones'));
  await hl(() => window.__hl.aimAt([0, 5, -22]));
  await step(page, 30);
  await hl(() => {
    const w = window.__hl.world;
    const d = [...w.entities.values()].find((e) => e.type === 'drone' && Math.abs(e.body.translation().z + 22) < 3);
    const glove = window.__hl.spawnForm('glove', { scale: 3, label: 'giant boxing glove + rockets', placement: { at: 'hand', orient: 'along_look' }, behaviors: [{ type: 'strike', target: { kind: 'entity', id: d.id }, speed: 40, after: 'return' }, { type: 'thrust', accel: 25, axis: [0, 0, 1] }] });
    return glove;
  });
  await step(page, 14);
  await shot('08_glove', 2);
  await step(page, 16);
  await shot('09_impact', 1);
}

// 10. flight over the city
if (want('10_flight')) {
  await hl(() => window.__hl.scene('playground'));
  await hl(() => window.__hl.teleport([0, 60, 20], [0, 0, -10]));
  await hl(() => window.__hl.look(0, -0.25));
  await step(page, 1, { flyToggle: true });
  await step(page, 90, { moveZ: 1 });
  await shot('10_flight');
}

// 11. sandbox: several constructs at once in the arena
if (want('11_arena')) {
  await hl(() => window.__hl.scene('arena'));
  await hl(() => window.__hl.look(0.2, -0.1));
  await step(page, 20);
  await hl(() => {
    const H = window.__hl;
    H.spawnForm('trampoline', { scale: 1.2, placement: { at: 'front', orient: 'face_away', distance: 9 }, behaviors: [{ type: 'launch', speed: 30, dir: 'normal' }] });
    H.spawnForm('cage', { scale: 1.1, placement: { at: 'front', orient: 'face_away', distance: 16 } });
    H.spawnForm('hammer', { scale: 2.5, placement: { at: 'front', orient: 'face_away', distance: 6, height: 2 }, behaviors: [{ type: 'spin', rate: 2, axis: [0, 1, 0] }] });
    H.spawnForm('dome', { scale: 1, placement: { at: 'self', orient: 'upright' } });
  });
  await step(page, 40);
  await shot('11_arena', 6);
}

// 13. incoming fire (projectiles + muzzle flashes), no protection
if (want('13_incoming')) {
  await hl(() => window.__hl.scene('barrage'));
  await hl(() => window.__hl.teleport([4, 1, -12]));
  await hl(() => window.__hl.aimAt([0, 2, -30]));
  await step(page, 150);
  await shot('13_incoming', 2);
}

// 14. the gap: goal beacon across the canyon, bridge preview via span placement
if (want('14_gap')) {
  await hl(() => window.__hl.scene('gap'));
  await hl(() => window.__hl.aimAt([0, 0, -30]));
  await step(page, 20);
  await hl(() => window.__hl.commit('make a bridge to the other side'));
  await step(page, 40);
  await hl(() => window.__hl.look(window.__hl.world.player.yaw + 0.5, -0.25));
  await step(page, 2);
  await shot('14_gap', 6);
}

// 12. help + perf overlays
if (want('12_help')) {
  await hl(() => window.__hl.toggleHelp(true));
  await shot('12_help');
  await hl(() => window.__hl.toggleHelp(false));
}

if (errors.length) { console.log('\n[page errors/warnings]'); for (const e of [...new Set(errors)]) console.log('  ' + e); }
await close();
