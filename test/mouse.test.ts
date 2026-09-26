import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MouseFilter, radPerCount, legacyMultiplierToSource, DEFAULT_SENS } from '../src/app/mouse.js';

test('mouse filter: steady motion and fast flicks pass untouched', () => {
  const f = new MouseFilter();
  f.onLock(0);
  let t = 100, sum = 0;
  for (let i = 0; i < 20; i++) { sum += f.filter(3, 1, t += 1)[0]; } // slow tracking at 1 kHz polling
  assert.equal(sum, 60);
  // a real flick ramps up over a few events (8 → 400 counts/event) — none may be dropped
  let flick = 0;
  for (const dx of [8, 20, 45, 90, 160, 260, 400, 380, 250, 120, 40, 10]) flick += f.filter(dx, 0, t += 1)[0];
  assert.equal(flick, 8 + 20 + 45 + 90 + 160 + 260 + 400 + 380 + 250 + 120 + 40 + 10);
  assert.equal(f.dropped, 0);
});

test('mouse filter: an isolated bogus jump is dropped, motion around it is kept', () => {
  const f = new MouseFilter();
  f.onLock(0);
  let t = 100;
  for (let i = 0; i < 10; i++) f.filter(2, 0, t += 1);
  assert.deepEqual(f.filter(-640, 390, t += 1), [0, 0]); // the classic Chromium spike
  assert.deepEqual(f.filter(2, 0, t += 1), [2, 0]);
  assert.equal(f.dropped, 1);
});

test('mouse filter: movement right after the lock is acquired is ignored (jump-to-centre)', () => {
  const f = new MouseFilter();
  f.onLock(1000);
  assert.deepEqual(f.filter(500, 300, 1010), [0, 0]);
  assert.deepEqual(f.filter(5, 0, 1100), [5, 0]);
});

test('sensitivity is in Source/Quake units; old multiplier saves migrate without changing the feel', () => {
  assert.ok(Math.abs(radPerCount(1) - (0.022 * Math.PI) / 180) < 1e-12); // sens 1 = 0.022° per count
  assert.ok(Math.abs(legacyMultiplierToSource(1) - 5.73) < 0.01);
  assert.ok(Math.abs(radPerCount(DEFAULT_SENS) - 0.0022) / 0.0022 < 0.01); // default ≈ the original feel
});
