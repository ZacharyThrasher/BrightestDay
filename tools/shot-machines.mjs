// Screenshots of the catalog machines in action: node tools/shot-machines.mjs [outdir]
// (needs a build: node tools/build.mjs; Playwright browsers: PLAYWRIGHT_BROWSERS_PATH)
import { launch, step, frames } from './render-harness.mjs';
const out = process.argv[2] ?? 'docs/screenshots';
const { page, errors, close } = await launch({ query: 'scene=drones' });
await page.evaluate(() => window.__hl.manual(true));
const shots = [
  { name: 'machines_tank', say: ['a tank'], ticks: 150, look: [0.35, -0.12] },
  { name: 'machines_dog_heli', say: ['a dog', 'a helicopter'], ticks: 150, look: [0.2, 0.05] },
  { name: 'machines_dragon', say: ['a dragon'], ticks: 180, look: [0.1, 0.12] },
  { name: 'machines_spider_mech', say: ['a spider', 'a mech'], ticks: 140, look: [0.3, -0.05] },
  { name: 'machines_car_ride', say: ['a car'], ticks: 90, input: { moveZ: 1, moveX: 0.4 }, look: [2.6, -0.25] },
];
for (const s of shots) {
  await page.evaluate(() => window.__hl.scene('drones'));
  await frames(page, 2);
  await page.evaluate(([y, p]) => window.__hl.look(y, p), [0, -0.05]);
  for (const u of s.say) { await page.evaluate((t) => window.__hl.commit(t), u); await step(page, 20); }
  await step(page, s.ticks, s.input ?? {});
  if (s.input) await page.evaluate(([y, p]) => window.__hl.look(y, p), s.look);
  else await page.evaluate(() => {
    // frame the machines: look at their centroid from the player's spot
    const w = window.__hl.world;
    const cs = [...w.constructs.list.values()].map((c) => c.insts[0].pos);
    const m = cs.reduce((a, p) => [a[0] + p.x / cs.length, a[1] + p.y / cs.length, a[2] + p.z / cs.length], [0, 0, 0]);
    window.__hl.aimAt([m[0], m[1] + 0.5, m[2]]);
  });
  await step(page, 2, s.input ?? {});
  await frames(page, 30);
  await page.screenshot({ path: `${out}/${s.name}.png` });
  const info = await page.evaluate(() => {
    const w = window.__hl.world;
    return [...w.constructs.list.values()].map((c) => `${c.spec.label}: joints ${c.spec.joints?.length ?? 0}, moved ${c.insts[0].m ? c.insts[0].m.travel.toFixed(1) : '-'} m, shots ${c.insts[0].m?.shots ?? 0}`).join(' | ') + ` | riding ${w.player.riding?.id ?? '-'} | calls ${window.__hl.stats().sceneCalls}`;
  });
  console.log(s.name, info);
}
console.log(errors.join('\n') || 'no errors');
await close();
