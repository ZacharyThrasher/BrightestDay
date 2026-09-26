import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeWorld, run, Vector3 } from './helpers.js';
import { defaultSpec } from '../src/sim/constructs/spec.js';
import { buildForm } from '../src/sim/constructs/forms.js';
import { interceptTime, steerToward, COMBAT } from '../src/sim/constructs/combat.js';
import type { World, Entity } from '../src/sim/world.js';

/** A hostile target we fly around by hand (stands in for a fast-moving player). */
function mover(w: World, at: Vector3): Entity {
  const e = w.addEntity('dummy', at, { hostile: true, label: 'mover' });
  e.body.setGravityScale(0, true);
  w.refreshQueries();
  return e;
}

test('intercept maths: leads a moving target; steering is turn-limited', () => {
  const t = interceptTime(new Vector3(0, 0, 0), 40, new Vector3(0, 0, -20), new Vector3(25, 0, 0));
  // shot and target meet: |p + v t| = 40 t
  const p = new Vector3(25 * t, 0, -20);
  assert.ok(Math.abs(p.length() - 40 * t) < 1e-6);
  const v = new Vector3(0, 0, 10);
  steerToward(v, new Vector3(1, 0, 0), 0.1, 10);
  assert.ok(Math.abs(Math.atan2(v.x, v.z) - 0.1) < 1e-6 && Math.abs(v.length() - 10) < 1e-6);
});

test('a homing chaser hits a target crossing at 25 m/s in a straight line', async () => {
  const { w } = await makeWorld('flat');
  const e = mover(w, new Vector3(-12, 1.6, -30));
  const vel = new Vector3(25, 0, 0);
  w.constructs.energy = 1e6;
  const r = w.constructs.spawn(defaultSpec({
    concept: 'rocket', label: 'rocket', parts: buildForm('rocket'), motion: 'kinematic', placement: { at: 'hand', orient: 'along_look' },
    behaviors: [{ type: 'home', target: { kind: 'entity', id: e.id }, speed: 38 }, { type: 'explode', radius: 3, force: 20, delay: 8, onContact: true }],
  }));
  assert.ok(r.construct);
  let hitAt = -1;
  for (let i = 0; i < 240 && hitAt < 0; i++) {
    e.body.setLinvel(vel, true);
    run(w, 1 / 60);
    if (e.hp < e.maxHp) hitAt = i / 60;
  }
  console.log(`homing chaser (cap ${COMBAT.missile.maxSpeed} m/s, ${COMBAT.missile.turnRate}°/s) hit after ${hitAt.toFixed(2)} s, target hp ${e.hp.toFixed(0)}`);
  assert.ok(hitAt > 0, 'chaser should catch a 25 m/s straight mover');
});

async function strikeAt(juke: boolean) {
  const { w } = await makeWorld('flat');
  const e = mover(w, new Vector3(-6, 1.6, -20));
  let t = 0;
  // 0.45 s into a leg when the strike launches; with juke the target cuts 90° 0.15 s later, and every 0.6 s after
  const velAt = (time: number) => {
    if (!juke) return new Vector3(25, 0, 0);
    const leg = Math.floor((time + 0.15) / 0.6);
    return leg % 2 === 0 ? new Vector3(25, 0, 0) : new Vector3(0, 0, -25);
  };
  for (let i = 0; i < 27; i++) { e.body.setLinvel(velAt(t), true); run(w, 1 / 60); t += 1 / 60; }
  w.constructs.energy = 1e6;
  const r = w.constructs.spawn(defaultSpec({
    concept: 'fist', label: 'fist', parts: buildForm('fist'), scale: 1.8, motion: 'kinematic', placement: { at: 'hand', orient: 'along_look' },
    behaviors: [{ type: 'strike', target: { kind: 'entity', id: e.id }, speed: 50, after: 'return' }],
  }));
  assert.ok(r.construct);
  for (let i = 0; i < 90; i++) { e.body.setLinvel(velAt(t), true); run(w, 1 / 60); t += 1 / 60; }
  return e.hp < e.maxHp;
}

test('a committed strike leads a straight mover but a hard 90° cut dodges it', async () => {
  COMBAT.strike.lockAtLaunch = false;
  const tracked = await strikeAt(true);
  COMBAT.strike.lockAtLaunch = true;
  console.log(`(without the launch lock the same juke is hit=${tracked})`);
  assert.ok(tracked, "control: a re-aiming strike would have caught the juke");
  const straight = await strikeAt(false);
  const juked = await strikeAt(true);
  console.log(`strike (${COMBAT.strike.minSpeed}–${COMBAT.strike.maxSpeed} m/s, locked at launch): straight mover hit=${straight}, juking mover hit=${juked}`);
  assert.ok(straight, 'the lead should connect with a predictable target');
  assert.ok(!juked, 'a juke right after launch should dodge a committed strike');
});
