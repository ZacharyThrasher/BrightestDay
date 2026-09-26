/**
 * World-content probe: builds a scene, runs it with the player standing still, and prints what the
 * robots / bystanders did, event counts, body counts and sim step timings.
 *   npx tsx eval/content-probe.ts [scene] [seconds]        (VERBOSE=1 prints outcome events)
 */
import { World } from '../src/sim/world.js';
import { buildScene, type SceneName } from '../src/sim/levels.js';
import { NO_INPUT } from '../src/sim/player.js';

const OUTCOMES = /toppled|breached|destroyed|disabled|knocked|saved|flee|wave_/;

async function main() {
  const scene = (process.argv[2] ?? 'sandbox') as SceneName;
  const secs = Number(process.argv[3] ?? 20);
  const w = await World.create();
  buildScene(w, scene);
  const counts = new Map<string, number>();
  w.listeners.push((e) => {
    counts.set(e.type, (counts.get(e.type) ?? 0) + 1);
    if (process.env.VERBOSE && OUTCOMES.test(e.type)) console.log(JSON.stringify(e));
  });
  const times: number[] = [];
  const n = Math.round(secs * 60);
  for (let i = 0; i < n; i++) {
    const t0 = performance.now();
    w.step({ ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch });
    times.push(performance.now() - t0);
    if (i % 120 === 0) {
      const rs = [...w.entities.values()].filter((e) => e.type === 'robot' || e.type === 'civilian').map((e) => `${e.id}:${(e.ai as any)?.mode ?? '-'}@${w.entityPos(e).toArray().map((x) => x.toFixed(1)).join(',')}`);
      console.log(`t=${w.time.toFixed(1)} player=${w.player.pos.toArray().map((x) => x.toFixed(1)).join(',')} hp=${w.player.hp} hits=${w.stats.playerHits} ${rs.join(' ')}`);
    }
  }
  const warm = times.slice(60).sort((a, b) => a - b); // skip the first second (JIT warm-up, initial settling)
  const q = (f: number) => warm[Math.floor(warm.length * f)].toFixed(3);
  console.log('events', Object.fromEntries(counts));
  console.log(`entities ${w.entities.size} debris ${w.content.debris.length} bodies ${w.pw.bodies.len()}`);
  console.log(`step ms p50 ${q(0.5)} p90 ${q(0.9)} p99 ${q(0.99)} max ${warm[warm.length - 1].toFixed(3)}`);
}
main();
