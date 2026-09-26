import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeWorld, run, look, Vector3 } from './helpers.js';
import { defaultSpec, type ConstructSpec } from '../src/sim/constructs/spec.js';
import { buildForm } from '../src/sim/constructs/forms.js';
import type { World } from '../src/sim/world.js';

const spec = (form: string, p: Partial<ConstructSpec>) => defaultSpec({ concept: form, label: form, parts: buildForm(form), ...p });

function stateHash(w: World) {
  let h = 2166136261 >>> 0;
  const mix = (x: number) => { h = Math.imul(h ^ (Math.round(x * 1000) | 0), 16777619) >>> 0; };
  for (const e of w.entities.values()) {
    const t = e.body.translation(), r = e.body.rotation();
    for (const x of [t.x, t.y, t.z, r.x, r.y, r.z, r.w, e.hp]) mix(x);
  }
  for (const d of w.content.debris) { const t = d.body.translation(); mix(t.x); mix(t.y); mix(t.z); }
  for (const x of [w.player.pos.x, w.player.pos.y, w.player.pos.z, w.player.hp, w.entities.size, w.projectiles.size]) mix(x);
  return h.toString(16);
}

test('ground robot walks to a standing player and punches them', async () => {
  const { w } = await makeWorld('flat');
  const robot = w.content.addRobot(new Vector3(3, 1.1, -16));
  w.refreshQueries();
  run(w, 10);
  const punches = w.log.filter((e) => e.type === 'player_hit' && e.kind === 'punch' && e.from === robot.id);
  const d = w.entityPos(robot).distanceTo(w.player.pos);
  console.log('robot dist', d.toFixed(1), 'punches', punches.length, 'player hp', w.player.hp);
  assert.ok(punches.length >= 1, 'robot should have landed a punch');
  assert.ok(w.player.hp < 100);
  assert.ok(w.log.some((e) => e.type === 'robot_punch' && e.hit));
});

test('robot steers around a wall in its way', async () => {
  const { w } = await makeWorld('flat');
  w.addBox(new Vector3(0, 1.5, -8), new Vector3(8, 3, 1), 0xd9d2c0, 'wall');
  const robot = w.content.addRobot(new Vector3(0.5, 1.1, -16));
  w.refreshQueries();
  run(w, 12);
  assert.ok(w.log.some((e) => e.type === 'player_hit' && e.from === robot.id), 'robot should get around the wall and punch');
});

test('a heavy construct thrown into a block stack topples it (credited to the construct)', async () => {
  const { w } = await makeWorld('flat');
  const st = w.content.addStack(new Vector3(0, 0, -16), { shape: 'pyramid', levels: 4 });
  w.refreshQueries();
  run(w, 1.5);
  const settled = w.log.filter((e) => e.type === 'block_toppled').length;
  assert.equal(settled, 0, 'a fresh stack must stand on its own');
  look(w, new Vector3(0, 1.5, -16));
  const r = w.constructs.spawn(spec('cube', { scale: 1.5, density: 2000, placement: { at: 'hand', orient: 'along_look' } }));
  assert.ok(r.construct, r.report.rejected ?? '');
  run(w, 0.3);
  w.constructs.throwAt(r.construct!.id, { kind: 'point', p: [0, 1.5, -16] }, 30);
  run(w, 3);
  const topples = w.log.filter((e) => e.type === 'block_toppled');
  const stackEv = w.log.find((e) => e.type === 'stack_toppled' && e.stack === st.id);
  console.log('toppled', topples.length, '/', st.total, 'by', stackEv?.by, 'construct', r.construct!.id);
  assert.ok(topples.length >= 4, 'several blocks should topple');
  assert.ok(stackEv, 'stack_toppled event');
  assert.equal(stackEv!.by, r.construct!.id, 'topple attributed to the thrown construct');
});

test('a crate breaks into debris after enough damage, and the debris despawns', async () => {
  const { w } = await makeWorld('flat');
  const crate = w.addEntity('crate', new Vector3(0, 0.8, -6));
  w.refreshQueries();
  run(w, 0.5);
  w.damageEntity(crate, 30, 'strike', 'c_test');
  run(w, 0.1);
  assert.ok(w.entities.has(crate.id), 'crate survives a light hit');
  w.damageEntity(crate, 40, 'strike', 'c_test');
  run(w, 0.1);
  assert.ok(!w.entities.has(crate.id), 'crate should be gone');
  const ev = w.log.find((e) => e.type === 'entity_destroyed' && e.id === crate.id);
  assert.ok(ev, 'entity_destroyed event');
  assert.equal(ev!.by, 'c_test');
  assert.ok(w.content.debris.length >= 3, 'debris pieces spawned');
  run(w, 5);
  assert.equal(w.content.debris.length, 0, 'debris fades and despawns');
});

test('a bystander flees from an approaching robot', async () => {
  const { w } = await makeWorld('flat');
  const civ = w.content.addBystander(new Vector3(0, 0.95, -8));
  w.content.addRobot(new Vector3(0, 1.1, -22));
  w.refreshQueries();
  const p0 = w.entityPos(civ);
  run(w, 6);
  assert.ok(w.log.some((e) => e.type === 'civilian_flee' && e.id === civ.id));
  assert.ok(w.entityPos(civ).distanceTo(p0) > 4, 'civilian ran');
});

test('a bystander carried away from danger by a construct is saved (credited to it)', async () => {
  const { w } = await makeWorld('flat');
  const civ = w.content.addBystander(new Vector3(0, 0.95, -10));
  w.content.addRobot(new Vector3(0, 1.1, -22));
  w.refreshQueries();
  run(w, 0.5);
  // stand-in for a construct's grab: the runtime marks heldBy and drives the body kinematically
  civ.heldBy = 'c_carrier';
  civ.body.setBodyType(w.R.RigidBodyType.KinematicPositionBased, true);
  for (let i = 0; i < 120; i++) { civ.body.setNextKinematicTranslation({ x: 0, y: 3, z: -10 + i * 0.1 }); w.step(); }
  civ.heldBy = null;
  civ.body.setBodyType(w.R.RigidBodyType.Dynamic, true);
  run(w, 1);
  const saved = w.log.find((e) => e.type === 'civilian_saved' && e.id === civ.id);
  assert.ok(saved, 'civilian_saved');
  assert.equal(saved!.by, 'c_carrier');
  assert.equal(saved!.how, 'carried');
});

test('sandbox scene builds deterministically', async () => {
  const hashes: string[] = [];
  for (let k = 0; k < 2; k++) {
    const { w } = await makeWorld('sandbox');
    const types = [...w.entities.values()].map((e) => e.type);
    assert.equal(types.filter((t) => t === 'robot').length, 2);
    assert.equal(types.filter((t) => t === 'drone').length, 2);
    assert.equal(types.filter((t) => t === 'turret').length, 1);
    assert.equal(types.filter((t) => t === 'civilian').length, 1);
    assert.ok(types.filter((t) => t === 'crate').length >= 4);
    assert.ok(types.filter((t) => t === 'block').length >= 20);
    run(w, 10);
    hashes.push(stateHash(w));
  }
  console.log('sandbox hashes', hashes);
  assert.equal(hashes[0], hashes[1]);
});

test('race scene: timer starts at the line and stops at the finish', async () => {
  const { w } = await makeWorld('race');
  assert.ok(![...w.entities.values()].some((e) => e.hostile));
  w.player.teleport(new Vector3(0, 1, 2));
  run(w, 0.2);
  w.player.teleport(new Vector3(0, 1, -1));
  run(w, 0.5);
  assert.ok(w.log.some((e) => e.type === 'race_started'));
  w.player.teleport(new Vector3(0, 1, -201));
  run(w, 0.2);
  const fin = w.log.find((e) => e.type === 'race_finished');
  assert.ok(fin && fin.time > 0.4 && fin.time < 1.2, 'race_finished with a time');
  assert.ok(w.goals[0].reached);
});
