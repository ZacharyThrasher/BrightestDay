// Does skill matter? Bots fly the playground ring course with different technique sets.
import { Vector3 } from 'three';
import { World } from '../src/sim/world.js';
import { buildScene } from '../src/sim/levels.js';
import { NO_INPUT, type PlayerInput } from '../src/sim/player.js';

type Bot = (w: World, target: Vector3) => Partial<PlayerInput>;
const steer = (w: World, t: Vector3) => { const d = t.clone().sub(w.player.pos); return { yaw: Math.atan2(-d.x, -d.z), pitch: Math.atan2(d.y, Math.hypot(d.x, d.z)) }; };
const bots: Record<string, Bot> = {
  'flight only (cruise)': (w, t) => ({ ...steer(w, t), moveZ: 1 }),
  'flight + boost when gas ≥ 30%': (w, t) => ({ ...steer(w, t), moveZ: 1, boost: w.player.gas > 30 }),
  'flight + boost + dive from altitude': (w, t) => {
    const s = steer(w, t);
    const P = w.player;
    const d = t.clone().sub(P.pos);
    // climb high early, then dive at the ring (converts altitude into speed)
    const wantHigh = Math.hypot(d.x, d.z) > 60 && P.pos.y < t.y + 25;
    return { ...s, pitch: wantHigh ? Math.max(s.pitch, 0.35) : s.pitch, moveZ: 1, boost: P.gas > 20 };
  },
};
for (const [name, bot] of Object.entries(bots)) {
  const w = await World.create();
  buildScene(w, 'playground');
  let t = 0; let next = 0; const maxT = 180;
  w.step({ ...NO_INPUT, flyToggle: true });
  let maxSpeed = 0; const splits: string[] = [];
  while (next < w.rings.length && t < maxT) {
    const r = w.rings[next];
    w.step({ ...NO_INPUT, ...bot(w, r.pos) });
    t += w.dt;
    maxSpeed = Math.max(maxSpeed, w.player.vel.length());
    if (r.passed || w.player.pos.distanceTo(r.pos) < r.radius * 0.8) { r.passed = true; next++; splits.push(t.toFixed(1)); }
  }
  console.log(`${name.padEnd(40)} rings ${next}/${w.rings.length}  time ${t.toFixed(1)} s  top speed ${maxSpeed.toFixed(0)} m/s  splits ${splits.join(" ")}`);
}
