import { test } from 'node:test';
import assert from 'node:assert/strict';
import { FocusMeter, CastClock, CastRecords, Combo, Completer, QuickSlots, TYPING } from '../src/app/typing.js';

const frames = (m: FocusMeter, seconds: number, typing: boolean) => { let simSeconds = 0; for (let i = 0; i < seconds * 60; i++) simSeconds += m.update(1 / 60, typing) / 60; return simSeconds; };

test('focus time: slows while typing, drains, runs dry, recharges', () => {
  const m = new FocusMeter(0.35, 5, 10);
  const sim = frames(m, 2, true);
  console.log(`2 s typing → ${sim.toFixed(2)} s of world time, meter ${m.meter.toFixed(2)}`);
  assert.ok(Math.abs(m.scale - 0.35) < 0.01);
  assert.ok(sim > 0.6 && sim < 0.9, 'about 35% of real time passes in the world');
  assert.ok(Math.abs(m.meter - 0.6) < 0.01);
  frames(m, 4, true);
  assert.ok(m.exhausted && m.scale > 0.99, 'empty meter: time back to normal while still typing');
  frames(m, 1, false);
  assert.ok(m.exhausted, 'needs to recharge past the resume threshold first');
  frames(m, 2, false);
  assert.ok(!m.exhausted && m.meter >= TYPING.resumeAt);
  frames(m, 0.5, true);
  assert.ok(m.active, 'slow time available again');
});

test('focus time off = the world never slows', () => {
  const m = new FocusMeter(1);
  assert.equal(frames(m, 3, true).toFixed(3), (3).toFixed(3));
  assert.ok(!m.active && !m.enabled);
});

test('cast clock and personal bests', () => {
  const c = new CastClock();
  const r = new CastRecords();
  c.open(1000);
  assert.equal(c.commit(1500, 'wall'), null, 'no keystrokes (a recast) → no timing');
  c.key(1300);
  const t = c.commit(2300, 'a giant ramp')!; // 12 chars typed over 1.0 s, 1.3 s after opening
  assert.ok(Math.abs(t.sec - 1.3) < 1e-9 && Math.abs(t.cps - 12) < 1e-9);
  assert.deepEqual(r.record(t), { bestCast: true, bestCps: true });
  assert.deepEqual(r.record({ sec: 2, cps: 5, chars: 12 }), { bestCast: false, bestCps: false });
  assert.equal(r.best.cast, 1.3);
});

test('combo: quick successful casts chain; a gap or a failure breaks it', () => {
  const c = new Combo();
  assert.equal(c.hit(0, true), 1);
  assert.equal(c.peek(3000), 2);
  assert.equal(c.hit(3000, true), 2);
  assert.equal(c.hit(7000, true), 3);
  assert.ok(c.left(8000) > 0);
  assert.equal(c.hit(7000 + TYPING.comboWindow * 1000 + 1, true), 1, 'lapsed');
  c.hit(15000, true);
  assert.equal(c.hit(16000, false), 0, 'a fizzled cast breaks the combo');
});

test('Tab completion: lexicon, history first, two-word names, nothing for whole words', () => {
  const c = new Completer();
  assert.equal(c.suggest('a tramp')?.word, 'trampoline');
  assert.equal(c.suggest('a tramp')?.suffix, 'oline');
  assert.equal(c.suggest('wrecking b')?.word, 'wrecking ball');
  assert.equal(c.suggest('a wall'), null, 'already a word');
  assert.equal(c.suggest('a wall '), null, 'nothing after a space');
  assert.equal(c.suggest('x'), null, 'too short');
  const before = c.suggest('sh')!.word;
  c.learn('a shark with helicopter blades');
  assert.equal(c.suggest('sh')?.word, 'shark', `history beats the lexicon (was ${before})`);
  c.setSaved(['thumper']);
  assert.equal(c.suggest('thu')?.word, 'thumper');
});

test('quick-cast slots: saved designs first, then latest distinct typed commands', () => {
  const q = new QuickSlots();
  q.noteTyped('a wall');
  q.noteTyped('a giant ramp');
  q.noteTyped('a wall');
  q.addSaved('thumper');
  assert.deepEqual(q.slots().map((s) => `${s.key}:${s.text}`), ['1:thumper', '2:a wall', '3:a giant ramp']);
  for (let i = 0; i < 12; i++) q.noteTyped(`thing ${i}`);
  assert.equal(q.slots().length, 9);
  assert.equal(q.get(2)?.text, 'thing 11');
  assert.equal(q.get(12), null);
});
