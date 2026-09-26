// Scripted typist vs the falling civilian: does focus time let a typist of a given speed save them?
//   npx tsx tools/focus-sweep.ts        (results: docs/EXPERIMENTS.md §8)
// Real time advances in 60 Hz frames; the world advances by FocusMeter's scale (the app's exact path).
import { World } from '../src/sim/world.js';
import { buildScene, aimAt } from '../src/sim/levels.js';
import { Director } from '../src/lang/director.js';
import { NO_INPUT } from '../src/sim/player.js';
import { FocusMeter } from '../src/app/typing.js';

async function trial(scene: string, aim: string, text: string, cps: number, scale: number, react = 0.5) {
  const w = await World.create();
  const info: any = buildScene(w, scene as any);
  const d = new Director(w);
  const focus = new FocusMeter(scale, 5, 10);
  const typeSec = react + text.length / cps;
  let acc = 0;
  const step = () => { w.step({ ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch }); };
  const target = () => { const e = [...w.entities.values()].find((x) => x.type === aim); return e ? w.entityPos(e) : info.aims[aim]; };
  for (let t = 0; t < typeSec; t += 1 / 60) {
    acc += focus.update(1 / 60, true) / 60;
    while (acc >= 1 / 60) { aimAt(w, target()); step(); acc -= 1 / 60; }
  }
  const worldAtCast = w.time;
  aimAt(w, target());
  await d.commit(text);
  for (let i = 0; i < 60 * 5; i++) step();
  const civ = [...w.entities.values()].find((e) => e.type === 'civilian');
  return { worldAtCast, ok: civ ? !civ.hurt : false };
}

const scales = [1, 0.5, 0.35, 0.2];
for (const text of ['catch him', 'a trampoline under him']) {
  console.log(`\n"${text}" (falling civilian)  — cells: saved? (world seconds elapsed when the cast landed)`);
  console.log('cps   ' + scales.map((s) => `scale ${s}`.padEnd(16)).join(''));
  for (const cps of [3, 5, 8, 12]) {
    const row: string[] = [];
    for (const s of scales) { const r = await trial('falling', 'civilian', text, cps, s); row.push(`${r.ok ? 'SAVED' : 'hurt '} (${r.worldAtCast.toFixed(2)}s)`.padEnd(16)); }
    console.log(`${String(cps).padEnd(6)}${row.join('')}`);
  }
}
