// CPU cost of the simulation step under load (no rendering). npx tsx tools/sim-bench.ts
import { World } from '../src/sim/world.js';
import { buildScene, aimAt } from '../src/sim/levels.js';
import { NO_INPUT } from '../src/sim/player.js';
import { Director } from '../src/lang/director.js';
import { Vector3 } from 'three';
const w = await World.create();
buildScene(w, 'playground');
const d = new Director(w);
const P = w.player;
const stepMs: number[] = [];
const run = (n: number, input: any = {}) => { for (let i = 0; i < n; i++) { const t = performance.now(); w.step({ ...NO_INPUT, yaw: P.yaw, pitch: P.pitch, ...input }); stepMs.push(performance.now() - t); } };
const stats = (label: string) => { const a = stepMs.splice(0).sort((x, y) => x - y); console.log(label.padEnd(44), `p50 ${a[Math.floor(a.length * 0.5)].toFixed(2)} ms  p95 ${a[Math.floor(a.length * 0.95)].toFixed(2)} ms  max ${a[a.length - 1].toFixed(2)} ms  bodies ${w.pw.bodies.len()}  colliders ${w.pw.colliders.len()}`); };
run(120); stats('idle playground');
run(240, { moveZ: 1, boost: true, jump: true }); stats('running + boosting');
const utter = ['a wall', 'a giant hammer that orbits me', 'three shields orbiting me', 'a trampoline', 'a cage', 'a fan', 'a magnet', 'a bridge', 'a dome over me', 'five fists', 'a tornado', 'a conveyor belt', 'a ramp', 'a spring', 'a car with wings', 'a catapult', 'a pillar', 'a cube', 'a net', 'a ball with rockets'];
aimAt(w, new Vector3(-90, 7, 150));
for (const u of utter) { await d.commit(u); run(6); }
stepMs.splice(0);
run(300); stats(`${w.constructs.list.size} constructs active`);
for (let i = 0; i < 6; i++) await d.commit('drop ten anvils on the drones');
run(300); stats(`${w.constructs.list.size} constructs + anvil rain`);
const tInterp: number[] = [];
for (let i = 0; i < 200; i++) { const t = performance.now(); d.interpret(utter[i % utter.length] + ' with rockets on the back that orbits me'); tInterp.push(performance.now() - t); }
tInterp.sort((a, b) => a - b);
console.log('interpret (System 1) p50', tInterp[100].toFixed(2), 'ms p95', tInterp[190].toFixed(2), 'ms');
