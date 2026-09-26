import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeWorld, run, look, Vector3 } from './helpers.js';
import { defaultSpec, type ConstructSpec } from '../src/sim/constructs/spec.js';
import { buildForm } from '../src/sim/constructs/forms.js';

const spec = (form: string, p: Partial<ConstructSpec>) => defaultSpec({ concept: form, label: form, parts: buildForm(form), ...p });

test('no construct: barrage hits player; wall between blocks it', async () => {
  const a = await makeWorld('barrage');
  run(a.w, 4);
  const hitsBare = a.w.stats.playerHits;
  const b = await makeWorld('barrage');
  const r = b.w.constructs.spawn(spec('wall', { placement: { at: 'between', orient: 'face_away' } }));
  assert.ok(r.construct);
  run(b.w, 4);
  console.log('hits bare', hitsBare, 'with wall', b.w.stats.playerHits, 'blocked', b.w.stats.projectilesBlocked);
  assert.ok(hitsBare >= 5);
  assert.ok(b.w.stats.playerHits <= 1);
});

test('reflecting shield sends shots back and damages the turret', async () => {
  const { w } = await makeWorld('barrage');
  w.constructs.spawn(spec('shield', { placement: { at: 'between', orient: 'face_away' }, behaviors: [{ type: 'reflect' }] }));
  run(w, 6);
  const turret = [...w.entities.values()].find((e) => e.type === 'turret')!;
  console.log('reflected', w.stats.projectilesReflected, 'turret hp', turret.hp);
  assert.ok(w.stats.projectilesReflected > 3);
  assert.ok(turret.hp < turret.maxHp);
});

test('rocket glove punches a drone', async () => {
  const { w, info } = await makeWorld('drones');
  look(w, info.aims.drone);
  const drone = [...w.entities.values()].find((e) => e.type === 'drone' && Math.abs(w.entityPos(e).z + 22) < 3)!;
  const r = w.constructs.spawn(spec('glove', { scale: 3, placement: { at: 'hand', orient: 'along_look' }, behaviors: [{ type: 'strike', target: { kind: 'entity', id: drone.id }, speed: 40, after: 'return' }, { type: 'thrust', accel: 25, axis: [0, 0, 1] }] }));
  assert.ok(r.construct, r.report.rejected ?? '');
  run(w, 2);
  console.log('drone hp', drone.hp, 'disabled', drone.disabled, 'events', w.log.filter((e) => e.type === 'impact').length);
  assert.ok(drone.hp < 100);
});

test('grab, spin around, throw into wall', async () => {
  const { w, info } = await makeWorld('drones');
  look(w, info.aims.drone);
  const drone = [...w.entities.values()].find((e) => e.type === 'drone' && Math.abs(w.entityPos(e).z + 22) < 3)!;
  const r = w.constructs.spawn(spec('hand', { scale: 2.5, placement: { at: 'hand', orient: 'along_look' }, behaviors: [{ type: 'grab', target: { kind: 'entity', id: drone.id }, then: 'hold' }] }));
  run(w, 1.5);
  assert.equal(drone.heldBy, r.construct!.id, 'drone should be held');
  const c = r.construct!;
  const edited = JSON.parse(JSON.stringify(c.spec));
  edited.behaviors.push({ type: 'orbit', around: { kind: 'self' }, radius: 5, speed: 6 });
  w.constructs.modify(c.id, edited);
  run(w, 1.5);
  look(w, info.aims.wall);
  w.constructs.throwAt(c.id, { kind: 'point', p: info.aims.wall.toArray() as any }, 45);
  run(w, 2);
  const p = w.entityPos(drone);
  console.log('drone after throw', p.toArray().map((x) => x.toFixed(1)), 'hp', drone.hp.toFixed(0), 'disabled', drone.disabled);
  assert.equal(drone.heldBy, null);
  assert.ok(p.z < -35, 'drone should have flown toward the wall');
  assert.ok(drone.hp < 100);
});

test('cushioned net under falling civilian keeps them safe', async () => {
  const bare = await makeWorld('falling');
  run(bare.w, 5);
  const civ0 = [...bare.w.entities.values()][0];
  const { w, info } = await makeWorld('falling');
  const civ = [...w.entities.values()][0];
  w.constructs.spawn(spec('trampoline', { scale: 1.5, placement: { at: 'under_target', target: { kind: 'entity', id: civ.id }, orient: 'face_away' }, behaviors: [{ type: 'cushion' }, { type: 'launch', speed: 12, dir: 'normal' }] }));
  run(w, 6);
  console.log('bare hurt', civ0.hurt, 'with trampoline hurt', civ.hurt, 'safe', civ.safe);
  assert.equal(civ0.hurt, true);
  assert.equal(civ.hurt, false);
  void info;
});

test('bridge spans the gap and the player walks across', async () => {
  const { w, info } = await makeWorld('gap');
  look(w, info.aims.far_ledge);
  w.constructs.spawn(spec('bridge', { placement: { at: 'span', orient: 'face_away' } }));
  run(w, 6, () => ({ moveZ: 1 }));
  console.log('player', w.player.pos.toArray().map((x) => x.toFixed(1)), 'goal', w.goals[0].reached);
  assert.ok(w.goals[0].reached);
});

test('launch pad throws the player onto the tower roof height', async () => {
  const { w } = await makeWorld('tower');
  w.constructs.spawn(spec('launchpad', { placement: { at: 'front', orient: 'face_away', distance: 3 }, behaviors: [{ type: 'launch', speed: 45, dir: 'up' }] }));
  let maxY = 0;
  run(w, 4, (t) => { maxY = Math.max(maxY, w.player.pos.y); return { moveZ: t < 0.6 ? 1 : 0 }; });
  console.log('max height', maxY.toFixed(1));
  assert.ok(maxY > 40);
});

test('validator clamps ten thousand planet-sized anvils', async () => {
  const { w } = await makeWorld('drones');
  const r = w.constructs.spawn(spec('anvil', { scale: 1e7, count: 10000, placement: { at: 'above_target', target: { kind: 'nearest_enemy' }, orient: 'face_away' }, motion: 'dynamic', density: 7000, behaviors: [{ type: 'crush', target: { kind: 'nearest_enemy' } }] }));
  console.log('clamps', r.report.clamps, 'count', r.construct?.insts.length, 'scale', r.construct?.spec.scale.toFixed(2));
  assert.ok(r.report.clamps.length >= 2);
  assert.ok((r.construct?.insts.length ?? 0) <= 12);
  run(w, 3);
});

test('wings let the player glide far', async () => {
  const { w } = await makeWorld('flat');
  w.player.teleport(new Vector3(0, 80, 0), new Vector3(0, 0, -15));
  w.constructs.spawn(spec('wings', { placement: { at: 'self', orient: 'face_away' }, behaviors: [{ type: 'mount', mode: 'wear', glide: 1.2 }] }));
  w.player.pitch = -0.15;
  run(w, 8, () => ({ pitch: -0.15 }));
  const winged = -w.player.pos.z;
  const b = await makeWorld('flat');
  b.w.player.teleport(new Vector3(0, 80, 0), new Vector3(0, 0, -15));
  run(b.w, 8, () => ({ pitch: -0.15 }));
  const bare = -b.w.player.pos.z;
  console.log('glide distance', winged.toFixed(0), 'vs falling', bare.toFixed(0));
  assert.ok(winged > bare * 1.5);
});
