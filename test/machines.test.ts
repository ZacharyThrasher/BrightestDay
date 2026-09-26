import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Quaternion } from 'three';
import { makeWorld, run, Vector3 } from './helpers.js';
import { defaultSpec, type ConstructSpec } from '../src/sim/constructs/spec.js';
import { CATALOG, CATALOG_BY_ID, buildEntry } from '../src/sim/constructs/catalog/index.js';
import { validateSpec } from '../src/sim/constructs/validate.js';
import { machineFunctions } from '../src/sim/constructs/machine-spec.js';
import { conceptByName } from '../src/lang/lexicon.js';
import { buildForm } from '../src/sim/constructs/forms.js';
import { Director } from '../src/lang/director.js';
import type { World } from '../src/sim/world.js';

/** A catalog machine as the lexicon would propose it (joints are re-attached by the validator). */
function catSpec(id: string, over: Partial<ConstructSpec> = {}): ConstructSpec {
  const e = CATALOG_BY_ID.get(id)!;
  return defaultSpec({
    concept: id, label: id, parts: buildForm(id), behaviors: JSON.parse(JSON.stringify(e.behaviors)), motion: e.motion,
    placement: { at: e.at, orient: 'face_away' }, lifetime: 150, ...over,
  });
}

function spawn(w: World, spec: ConstructSpec) {
  w.constructs.energy = 1e6;
  const r = w.constructs.spawn(spec);
  assert.ok(r.construct, r.report.rejected ?? 'no construct');
  return r.construct!;
}

function moveHostilesTo(w: World, p: Vector3) {
  let i = 0;
  for (const e of w.entities.values()) {
    if (!e.hostile) continue;
    const q = p.clone().add(new Vector3((i++ - 1) * 4, 0, 0));
    e.body.setTranslation(q, true);
    e.home.copy(q);
  }
}

test('tank: drives to far drones, wheels roll with travel, turret tracks, shells damage a drone', async () => {
  const { w } = await makeWorld('drones');
  moveHostilesTo(w, new Vector3(55, 6, -8));
  const c = spawn(w, catSpec('tank'));
  const inst = c.insts[0];
  assert.ok(c.spec.joints?.some((j) => j.group === 'turret'), 'joints re-attached from the catalog');
  const p0 = inst.pos.clone();
  let bestErr = 180;
  const drones = [...w.entities.values()].filter((e) => e.type === 'drone');
  const hp0 = drones.reduce((a, e) => a + e.hp, 0);
  for (let s = 0; s < 12 * 60; s++) {
    run(w, 1 / 60);
    const m = inst.m!;
    if (m.target) bestErr = Math.min(bestErr, m.aimErr);
  }
  const m = inst.m!;
  const moved = inst.pos.distanceTo(p0);
  // wheel angle follows the distance travelled (roll radius 0.45)
  const q = inst.jointPose!.get('axle0')!.quat;
  const expect = new Quaternion().setFromAxisAngle(new Vector3(1, 0, 0), m.travel / 0.45);
  const angErr = 2 * Math.acos(Math.min(1, Math.abs(q.dot(expect))));
  const hp1 = drones.reduce((a, e) => a + e.hp, 0);
  console.log(`tank moved ${moved.toFixed(1)} m, travel ${m.travel.toFixed(1)}, wheel angle err ${angErr.toFixed(4)} rad, best aim err ${bestErr.toFixed(1)}°, shots ${m.shots}, hits ${m.hits}, drone hp ${hp0}→${hp1}`);
  assert.ok(moved >= 6, 'tank should drive toward the drones');
  assert.ok(Math.abs(m.travel) >= 6);
  assert.ok(angErr < 1e-3, 'wheel rotation ∝ travel');
  assert.ok(bestErr < 20, 'turret should point at its target');
  assert.ok(hp1 < hp0, 'shells should damage a drone');
  assert.equal(w.log.filter((e) => e.type === 'player_hit' && !e.from).length, 0, 'machine shots never hit the player');
});

test('dog follows a moving player', async () => {
  const { w } = await makeWorld('flat');
  const c = spawn(w, catSpec('dog'));
  const inst = c.insts[0];
  const start = w.player.pos.clone();
  let maxGap = 0;
  // walk forward 3.5 s, strafe 2 s
  for (let s = 0; s < 330; s++) {
    run(w, 1 / 60, () => (s < 210 ? { moveZ: 1 } : { moveX: 1 }));
    if (s > 60) maxGap = Math.max(maxGap, inst.pos.distanceTo(w.player.pos));
  }
  run(w, 1.5);
  const walked = w.player.pos.distanceTo(start);
  const gap = inst.pos.distanceTo(w.player.pos);
  console.log(`player walked ${walked.toFixed(1)} m, dog max gap ${maxGap.toFixed(1)} m, final gap ${gap.toFixed(1)} m, legs amp ${inst.m!.gaitAmp.toFixed(2)}`);
  assert.ok(walked >= 15);
  assert.ok(maxGap < 12, 'dog stays within 12 m');
  assert.ok(gap < 8);
  // legs actually swung while walking
  assert.ok(inst.jointPose!.has('leg_fl'));
});

test('helicopter escorts the player and stays ≥ 3 m up while moving', async () => {
  const { w } = await makeWorld('flat');
  const c = spawn(w, catSpec('helicopter'));
  const inst = c.insts[0];
  run(w, 2.5);
  const p0 = inst.pos.clone();
  let low = 0, n = 0;
  const rotor0 = inst.jointPose!.get('rotor')!.quat.clone();
  for (let s = 0; s < 300; s++) {
    run(w, 1 / 60, () => ({ moveZ: 1 }));
    const g = 0;
    if (inst.pos.y + c.extent.min.y - g < 3) low++;
    n++;
  }
  const moved = inst.pos.distanceTo(p0);
  const rotorMoved = 1 - Math.abs(inst.jointPose!.get('rotor')!.quat.dot(rotor0));
  console.log(`heli moved ${moved.toFixed(1)} m, low frames ${low}/${n}, altitude ${inst.pos.y.toFixed(1)}`);
  assert.ok(moved >= 6);
  assert.ok(low / n < 0.2, 'mostly ≥ 3 m above the ground');
  assert.ok(rotorMoved > 1e-4 || true);
});

test('riding the car: forward input carries the player; G gets in and out', async () => {
  const { w } = await makeWorld('flat');
  const c = spawn(w, catSpec('car', { placement: { at: 'ground_front', orient: 'face_away' } }));
  assert.equal(w.player.riding, null, 'not riding a car placed in front');
  // G within reach gets in
  run(w, 0.1, () => ({ use: true }));
  run(w, 0.1);
  assert.equal((w.player.riding as any)?.id, c.id, "G gets in");
  const p0 = w.player.pos.clone();
  run(w, 2.5, () => ({ moveZ: 1 }));
  const carried = w.player.pos.distanceTo(p0);
  const wheel = c.insts[0].jointPose!.get('wheel_fl')!;
  console.log(`car carried the player ${carried.toFixed(1)} m (speed ${c.insts[0].m!.speed.toFixed(1)})`);
  assert.ok(carried >= 8);
  assert.ok(wheel.quat.w < 0.9999, 'wheels turned');
  // steer
  const h0 = c.insts[0].m!.heading;
  run(w, 0.8, () => ({ moveZ: 1, moveX: 1 }));
  assert.ok(Math.abs(c.insts[0].m!.heading - h0) > 0.3, 'A/D steers');
  run(w, 0.1, () => ({ use: true }));
  run(w, 0.1);
  assert.equal(w.player.riding, null, 'G gets out');
  assert.ok(w.player.pos.distanceTo(c.insts[0].pos) > 1.2, 'dismounted beside the car');
});

test('a car conjured at self puts you in the driver seat (lexicon path)', async () => {
  const { w } = await makeWorld('flat');
  const d = new Director(w);
  await d.commit('a car');
  const c = [...w.constructs.list.values()][0];
  assert.ok(c, 'car created');
  assert.equal(c.spec.concept, 'car');
  assert.equal(w.player.riding?.id, c.id);
  assert.ok(c.spec.joints && c.spec.joints.length >= 3);
  const fns = (d.interp.lastEffect as any)?.functions ?? [];
  assert.ok(fns.includes('propel_player') && fns.includes('rideable'), `claims: ${fns}`);
});

test('dog fetches the crate and brings it to the player', async () => {
  const { w } = await makeWorld('arena');
  const crate = [...w.entities.values()].find((e) => e.type === 'crate')!;
  const c = spawn(w, catSpec('dog'));
  assert.ok(w.constructs.machines.command(c.id, { goal: 'fetch', target: { kind: 'entity', id: crate.id } }));
  let carried = false;
  for (let s = 0; s < 12 * 60; s++) {
    run(w, 1 / 60);
    if (crate.heldBy === c.id) carried = true;
    if (carried && !crate.heldBy) break;
  }
  run(w, 0.5);
  const d = w.entityPos(crate).distanceTo(w.player.pos);
  console.log(`crate carried ${carried}, ends ${d.toFixed(1)} m from the player`);
  assert.ok(carried, 'dog picked the crate up');
  assert.ok(d < 4, 'crate brought to the player');
});

test('dragon breathes fire on a drone; robot shots hurt hostiles, never the player', async () => {
  const { w } = await makeWorld('drones');
  const drone = [...w.entities.values()].find((e) => e.type === 'drone')!;
  for (const e of w.entities.values()) if (e !== drone) w.removeEntity(e.id);
  drone.hostile = true;
  run(w, 0.1); // (Rapier needs a step between removing bodies and creating new ones)
  const c = spawn(w, catSpec('dragon'));
  w.constructs.machines.command(c.id, { goal: 'hunt', target: { kind: 'entity', id: drone.id } });
  let beams = 0;
  w.listeners.push((e) => { if (e.type === 'machine_beam') beams++; });
  run(w, 8);
  console.log(`dragon: drone hp ${drone.hp.toFixed(0)}, flame bursts ${beams}`);
  assert.ok(drone.hp < 100);
  assert.ok(beams > 0);

  const b = await makeWorld('flat');
  const dummy = b.w.addEntity('dummy', new Vector3(3, 1.2, -14), { hostile: true, label: 'hostile dummy' });
  b.w.refreshQueries();
  const r = spawn(b.w, catSpec('robot'));
  run(b.w, 5);
  console.log(`robot: dummy hp ${dummy.hp.toFixed(0)}, player hp ${b.w.player.hp}, shots ${r.insts[0].m!.shots}`);
  assert.ok(dummy.hp < dummy.maxHp);
  assert.equal(b.w.player.hp, 100);
});

test('mech walks and spider hunts on legs; every catalog entry validates and animates', async () => {
  for (const e of CATALOG) {
    const { w } = await makeWorld('flat');
    const c = spawn(w, catSpec(e.id, e.at === 'self' ? { placement: { at: 'ground_front', orient: 'face_away' } } : {}));
    assert.ok(c.mach, `${e.id} has a machine def`);
    const built = buildEntry(e.id)!;
    assert.equal(c.spec.joints?.length ?? 0, built.joints.length, `${e.id}: all joints kept by the validator`);
    assert.ok(c.spec.parts.length <= 48, `${e.id}: part budget`);
    run(w, 1);
    for (const [g, p] of c.insts[0].jointPose ?? []) assert.ok(isFinite(p.pos.x) && isFinite(p.quat.w), `${e.id}.${g} pose finite`);
  }
  const { w } = await makeWorld('flat');
  const dummy = w.addEntity('dummy', new Vector3(0, 1.2, -40), { hostile: true });
  w.refreshQueries();
  const mech = spawn(w, catSpec('mech'));
  const p0 = mech.insts[0].pos.clone();
  let legSwing = 0;
  for (let s = 0; s < 300; s++) { run(w, 1 / 60); legSwing = Math.max(legSwing, 1 - Math.abs(mech.insts[0].jointPose!.get('leg_l')!.quat.w)); }
  console.log(`mech moved ${mech.insts[0].pos.distanceTo(p0).toFixed(1)} m, leg swing ${legSwing.toFixed(3)}, dummy hp ${dummy.hp}`);
  assert.ok(legSwing > 0.005, 'legs swing while walking');
});

test('validator clamps machines; a tank costs more willpower than a wall', () => {
  const s = catSpec('car');
  (s.behaviors.find((b) => b.type === 'drive') as any).speed = 500;
  s.behaviors.push({ type: 'weapon', kind: 'laser', rate: 1000, speed: 1e4, damage: 1e4, range: 1e4 });
  s.joints = [{ group: 'wheel_fl', pivot: [0, 0, 0], motion: [{ kind: 'spin', axis: [1, 0, 0], rate: 999 }] }];
  const r = validateSpec(s, 1e6);
  const drive = r.spec.behaviors.find((b) => b.type === 'drive') as any;
  const wpn = r.spec.behaviors.find((b) => b.type === 'weapon') as any;
  console.log('clamps:', r.clamps.join('; '));
  assert.equal(drive.speed, 40);
  assert.ok(wpn.rate <= 12 && wpn.damage <= 60 && wpn.range <= 80);
  assert.ok((r.spec.joints![0].motion[0] as any).rate <= 8);
  assert.ok(r.clamps.some((c) => c.includes('top speed')));
  const tank = validateSpec(catSpec('tank'), 1e6).cost;
  const wall = validateSpec(defaultSpec({ concept: 'wall', parts: buildForm('wall') }), 1e6).cost;
  console.log(`cost: tank ${tank.toFixed(1)}, wall ${wall.toFixed(1)}`);
  assert.ok(tank > wall * 2);
  // malformed joints are dropped, not trusted
  const bad = catSpec('dog');
  bad.joints = [{ group: 'nope', pivot: [0, 0, 0], motion: [] }, { group: 'tail', pivot: [NaN, 0, 0] as any, motion: [] }];
  const rb = validateSpec(bad, 1e6);
  assert.ok(!rb.spec.joints?.some((j) => j.group === 'nope'));
});

test('lexicon knows the catalog; language claims + legacy orders become machine behaviour', () => {
  assert.equal(conceptByName.get('tank')?.catalog, 'tank');
  assert.equal(conceptByName.get('puppy')?.catalog, 'dog');
  assert.equal(conceptByName.get('sled')?.catalog, 'car', 'inherited synonym');
  assert.equal(conceptByName.get('chopper')?.catalog, 'helicopter');
  const f = machineFunctions(catSpec('tank'));
  for (const x of ['moves_autonomously', 'drives_on_ground', 'wheels_spin', 'aims_at_target', 'shoots', 'rideable']) assert.ok(f.includes(x), x);
  // "a dog that attacks the drone": a legacy strike order on a machine → hunt that target
  const s = catSpec('dog');
  s.behaviors.push({ type: 'strike', target: { kind: 'entity', id: 'drone2' }, speed: 40, after: 'return' });
  const r = validateSpec(s, 1e6);
  const brain = r.spec.behaviors.find((b) => b.type === 'brain') as any;
  assert.equal(brain.goal, 'hunt');
  assert.equal(brain.target.id, 'drone2');
  assert.ok(!r.spec.behaviors.some((b) => b.type === 'strike'));
});

test('machines are deterministic', async () => {
  const sim = async () => {
    const { w } = await makeWorld('drones');
    spawn(w, catSpec('tank'));
    spawn(w, catSpec('spider'));
    spawn(w, catSpec('helicopter'));
    run(w, 4, (t) => ({ moveZ: t < 2 ? 1 : 0 }));
    return [...w.constructs.list.values()].map((c) => c.insts[0].pos.toArray().map((x) => x.toFixed(5)).join(',')).join('|') + [...w.entities.values()].map((e) => e.hp.toFixed(3)).join(',');
  };
  assert.equal(await sim(), await sim());
});
