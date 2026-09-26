import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeWorld, run, look, Vector3 } from './helpers.js';

test('ground run reaches run speed', async () => {
  const { w } = await makeWorld('flat');
  run(w, 2.5, () => ({ moveZ: 1 }));
  const s = w.player.horizSpeed();
  console.log('run speed', s.toFixed(2));
  assert.ok(s > 10.5 && s < 11.5, `speed ${s}`);
  assert.ok(w.player.onGround);
});

test('landing then immediate jump preserves speed (bhop)', async () => {
  const { w } = await makeWorld('flat');
  run(w, 2, () => ({ moveZ: 1 }));
  const before = w.player.horizSpeed();
  let presses = 0;
  // press jump every time we get close to the ground
  run(w, 4, (t, w) => {
    const nearGround = w.player.pos.y < 1.2 && w.player.vel.y < 0;
    const j = w.player.onGround || nearGround;
    if (j) presses++;
    return { moveZ: 1, jump: j && Math.floor(t * 60) % 2 === 0 };
  });
  const after = w.player.horizSpeed();
  console.log('bhop speed before/after', before.toFixed(2), after.toFixed(2), 'jumps', w.player.stats.jumps);
  assert.ok(after > before * 0.95, 'bhop should not bleed speed');
  assert.ok(w.player.stats.jumps >= 4);
});

test('air strafing gains speed', async () => {
  const { w } = await makeWorld('flat');
  run(w, 2, () => ({ moveZ: 1 }));
  const v0 = w.player.horizSpeed();
  let dir = 1;
  let yaw = w.player.yaw;
  run(w, 8, (t, w) => {
    if (Math.floor(t / 0.55) % 2 === 0) dir = 1; else dir = -1;
    // turn the view in the strafe direction at a rate matched to speed
    const sp = w.player.horizSpeed();
    const rate = Math.min(4, (14 * 1.4) / Math.max(1, sp) * 1.5);
    yaw -= dir * rate * w.dt;
    const near = w.player.pos.y < 1.25 && w.player.vel.y <= 0;
    return { yaw, moveX: dir, jump: (w.player.onGround || near) && Math.floor(t * 60) % 2 === 0 };
  });
  const v1 = w.player.horizSpeed();
  console.log('air strafe', v0.toFixed(2), '->', v1.toFixed(2));
  assert.ok(v1 > v0 + 3, `expected gain, got ${v0} -> ${v1}`);
});

test('ramp keeps momentum and launches upward', async () => {
  const { w } = await makeWorld('flat');
  const { Quaternion } = await import('three');
  // 8 m tall, 12 m long ramp rising toward -z
  w.addWedge(new Vector3(0, 4, -40), new Vector3(8, 8, 12), new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), Math.PI));
  w.player.teleport(new Vector3(0, 1, -28), new Vector3(0, 0, -30));
  run(w, 0.2, () => ({ moveZ: 1, jump: true }));
  const sp = w.player.horizSpeed();
  let maxY = 0;
  run(w, 3, () => { maxY = Math.max(maxY, w.player.pos.y - 0.9); return { moveZ: 1 }; });
  console.log('ramp: approach', sp.toFixed(1), 'max feet height', maxY.toFixed(1), '(ramp top 8)');
  assert.ok(maxY > 11, 'ramp should launch well above its top');
});

test('tether swing builds speed and release keeps it', async () => {
  const { w } = await makeWorld('flat');
  w.addBox(new Vector3(20, 50, -40), new Vector3(4, 100, 4));
  w.refreshQueries();
  w.player.teleport(new Vector3(0, 40, 0), new Vector3(0, 0, -20));
  const hookPoint = new Vector3(19, 70, -38);
  look(w, hookPoint);
  let maxSpeed = 0;
  run(w, 1.6, () => { maxSpeed = Math.max(maxSpeed, w.player.vel.length()); return { hookL: true }; });
  const attached = w.player.tethers.L?.state;
  const atRelease = w.player.vel.length();
  run(w, 0.4, () => ({}));
  const after = w.player.vel.length();
  console.log('tether', attached, 'max', maxSpeed.toFixed(1), 'release', atRelease.toFixed(1), 'after 0.4s', after.toFixed(1));
  assert.equal(attached, 'attached');
  assert.ok(maxSpeed > 25, 'tether should build speed');
  assert.ok(after > atRelease * 0.8, 'release should keep momentum');
});

test('flight cruise is slower than boosted momentum', async () => {
  const { w } = await makeWorld('flat');
  run(w, 0.05, () => ({ flyToggle: true }));
  run(w, 3, () => ({ moveZ: 1 }));
  const cruise = w.player.vel.length();
  run(w, 1.5, () => ({ moveZ: 1, boost: true }));
  const boosted = w.player.vel.length();
  run(w, 2, () => ({ moveZ: 1 }));
  const decayed = w.player.vel.length();
  console.log('flight cruise', cruise.toFixed(1), 'boosted', boosted.toFixed(1), 'after 2s', decayed.toFixed(1));
  assert.ok(cruise < 18.5);
  assert.ok(boosted > cruise + 10);
  assert.ok(decayed > cruise + 5, 'overspeed should decay slowly, not snap');
});
