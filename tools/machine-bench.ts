// CPU cost of the simulation step with many active machines (no rendering). npx tsx tools/machine-bench.ts
import { Vector3, Quaternion } from 'three';
import { World } from '../src/sim/world.js';
import { buildScene } from '../src/sim/levels.js';
import { NO_INPUT } from '../src/sim/player.js';
import { defaultSpec } from '../src/sim/constructs/spec.js';
import { CATALOG } from '../src/sim/constructs/catalog/index.js';
import { buildForm } from '../src/sim/constructs/forms.js';
import { MACHINE_OPTS } from '../src/sim/constructs/machines.js';

const w = await World.create();
buildScene(w, 'playground');
const P = w.player;
const stepMs: number[] = [];
const run = (n: number, input: any = {}) => { for (let i = 0; i < n; i++) { const t = performance.now(); w.step({ ...NO_INPUT, yaw: P.yaw, pitch: P.pitch, ...input }); stepMs.push(performance.now() - t); } };
const stats = (label: string) => {
  const a = stepMs.splice(0).sort((x, y) => x - y);
  console.log(label.padEnd(46), `p50 ${a[Math.floor(a.length * 0.5)].toFixed(2)} ms  p95 ${a[Math.floor(a.length * 0.95)].toFixed(2)} ms  max ${a[a.length - 1].toFixed(2)} ms  colliders ${w.pw.colliders.len()}`);
};
run(600); stepMs.splice(0); run(600); stats("idle playground");
// 20 machines: two of every catalog entry, spread around the plaza and the drone field (they hunt / follow / wander)
w.constructs.energy = 1e9;
let k = 0;
for (let rep = 0; rep < 2; rep++) {
  for (const e of CATALOG) {
    const a = (k / 20) * Math.PI * 2;
    const at = new Vector3(Math.cos(a) * 25, 0, 60 + Math.sin(a) * 25);
    const spec = defaultSpec({ concept: e.id, label: e.id, parts: buildForm(e.id), behaviors: JSON.parse(JSON.stringify(e.behaviors)), motion: 'kinematic', lifetime: 150 });
    // parked vehicles get orders so everything is moving
    if (!spec.behaviors.some((b) => b.type === 'brain')) spec.behaviors.push({ type: 'brain', goal: 'wander' });
    const r = w.constructs.spawn(spec, { at: { pos: at.setY(1), quat: new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), a) } });
    if (!r.construct) console.log('rejected', e.id, r.report.rejected);
    k++;
  }
}
run(60);
stepMs.splice(0);
run(600, { moveZ: 1 }); stepMs.splice(0); run(1200, { moveZ: 1 });
stats(`${w.constructs.list.size} machines active`);
MACHINE_OPTS.syncColliders = false;
run(1200, { moveZ: 1 });
stats(`  … without collider sync`);
MACHINE_OPTS.syncColliders = true;
let moving = 0;
for (const c of w.constructs.list.values()) if (c.insts[0].m && c.insts[0].m.vel.length() > 0.5) moving++;
console.log(`moving now: ${moving}, shots fired: ${[...w.constructs.list.values()].reduce((a, c) => a + (c.insts[0].m?.shots ?? 0), 0)}, hostile hp left: ${[...w.entities.values()].filter((e) => e.hostile).map((e) => Math.round(e.hp)).join(',')}`);
