import { Vector3 } from 'three';
import { World } from '../src/sim/world.js';
import { buildScene, aimAt, type SceneName } from '../src/sim/levels.js';
import { NO_INPUT, type PlayerInput } from '../src/sim/player.js';

export async function makeWorld(scene: SceneName) {
  const w = await World.create();
  const info = buildScene(w, scene);
  return { w, info };
}

/** Run the sim; `fn` returns the input for each tick (yaw/pitch default to the player's current view). */
export function run(w: World, seconds: number, fn: (t: number, w: World) => Partial<PlayerInput> = () => ({})) {
  const n = Math.round(seconds / w.dt);
  for (let i = 0; i < n; i++) {
    const inp = { ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch, ...fn(i * w.dt, w) };
    w.step(inp);
  }
}

export function look(w: World, p: Vector3) { return aimAt(w, p); }
export { Vector3 };
