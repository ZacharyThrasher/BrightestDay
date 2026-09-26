/**
 * Combat catalog (weapons / traps / siege): every entry validates, spawns, animates and runs; a sample is tested
 * against scripted moving targets per the PvP speed tiers (docs/PVP_DESIGN.md "Speed vs. hitting"):
 *  - committed strikes hit a straight-mover, but a juke at launch dodges the first blow
 *  - chasers / turrets eventually hit a 25 m/s straight-mover
 *  - traps catch a target that walks into them
 * Targets are hostile entities (scripted dummies with gravity off); the owner is never hurt.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Quaternion } from 'three';
import { makeWorld, run, Vector3 } from './helpers.js';
import { defaultSpec, type ConstructSpec } from '../src/sim/constructs/spec.js';
import { CATALOG, COMBAT_CATALOG, CATALOG_BY_ID, buildEntry } from '../src/sim/constructs/catalog/index.js';
import { validateSpec } from '../src/sim/constructs/validate.js';
import { buildForm } from '../src/sim/constructs/forms.js';
import { specExtent } from '../src/sim/constructs/geom.js';
import { conceptByName } from '../src/lang/lexicon.js';
import { Director } from '../src/lang/director.js';
import type { World, Entity } from '../src/sim/world.js';
import type { Construct } from '../src/sim/constructs/runtime.js';

function catSpec(id: string, over: Partial<ConstructSpec> = {}): ConstructSpec {
  const e = CATALOG_BY_ID.get(id)!;
  return defaultSpec({
    concept: id, label: id, parts: buildForm(id), behaviors: JSON.parse(JSON.stringify(e.behaviors)), motion: e.motion,
    placement: { at: e.at, orient: e.orient ?? 'face_away' }, scale: e.scale ?? 1, density: e.density ?? 400, lifetime: e.lifetime ?? 60, ...over,
  });
}

/** Spawn; with `at`, stand it there on the ground facing -z like the player (y = its lowest point on the ground). */
function spawn(w: World, spec: ConstructSpec, at?: Vector3, yaw = Math.PI): Construct {
  w.constructs.energy = 1e6;
  const pos = at?.clone();
  if (pos) pos.y -= specExtent(validateSpec(spec, 1e6).spec).min.y;
  const r = w.constructs.spawn(spec, pos ? { at: { pos, quat: new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), yaw) } } : {});
  assert.ok(r.construct, `${spec.concept}: ${r.report.rejected ?? 'no construct'}`);
  return r.construct!;
}

/** A hostile scripted target: gravity off (flying) or on (walking), velocity re-imposed every tick. */
function target(w: World, p: Vector3, fly = true): Entity {
  const e = w.addEntity('dummy', p, { hostile: true, hp: 5000, label: 'scripted target' });
  if (fly) e.body.setGravityScale(0, true);
  w.refreshQueries();
  return e;
}

/** Step the world while imposing `vel(t)` on the target (unless something holds it). */
function drive(w: World, e: Entity, seconds: number, vel: (t: number) => Vector3, each?: (t: number) => boolean | void) {
  const n = Math.round(seconds / w.dt);
  for (let i = 0; i < n; i++) {
    const t = i * w.dt;
    if (!e.heldBy && !e.restrainedBy && w.entities.has(e.id)) {
      const v = vel(t);
      const cur = e.body.linvel();
      e.body.setLinvel({ x: v.x, y: e.body.gravityScale() === 0 ? v.y : cur.y, z: v.z }, true);
    }
    run(w, w.dt);
    if (each?.(t + w.dt)) return;
  }
}

/** Damage events from construct `c` on entity `e` (machine weapons, strikes, explosions). */
function hitLog(w: World, c: Construct, e: Entity) {
  const hits: { t: number; amount: number; cause: string }[] = [];
  w.listeners.push((ev) => { if (ev.type === 'entity_damage' && ev.id === e.id && ev.by === c.id) hits.push({ t: ev.t, amount: ev.amount, cause: ev.cause }); });
  return hits;
}

function ownerSafe(w: World) {
  assert.equal(w.player.hp, 100, 'the owner is never hurt by their own constructs');
  assert.equal(w.log.filter((e) => e.type === 'player_hit' && !e.from).length, 0);
}

// ---------------------------------------------------------------------------------------------------- catalog-wide

test('combat catalog: ≥ 40 entries, unique ids and names, lexicon knows them, claims and costs are sane', () => {
  assert.ok(COMBAT_CATALOG.length >= 40, `${COMBAT_CATALOG.length} entries`);
  const ids = new Set<string>();
  const names = new Map<string, string>();
  for (const e of COMBAT_CATALOG) {
    assert.ok(/^[a-z_]+$/.test(e.id), `${e.id}: id is a single token`);
    assert.ok(!ids.has(e.id), `duplicate id ${e.id}`);
    ids.add(e.id);
    for (const n of [e.id, ...e.names]) {
      assert.ok(!names.has(n) || names.get(n) === e.id, `name "${n}" claimed by ${names.get(n)} and ${e.id}`);
      names.set(n, e.id);
    }
    assert.equal(conceptByName.get(e.id)?.catalog, e.id, `${e.id} is a lexicon concept`);
    assert.ok(e.functions.length >= 3, `${e.id}: claims its functions`);
    assert.ok(e.behaviors.length >= 1, `${e.id}: has behaviour`);
    // every joint drives some parts, every grouped part has a joint, parents exist
    const b = buildEntry(e.id)!;
    const groups = new Set(b.parts.map((p) => p.group).filter(Boolean));
    for (const j of b.joints) {
      assert.ok(groups.has(j.group), `${e.id}: joint ${j.group} has parts`);
      assert.ok(!j.parent || b.joints.some((k) => k.group === j.parent), `${e.id}: joint ${j.group}'s parent exists`);
    }
    for (const g of groups) assert.ok(b.joints.some((j) => j.group === g), `${e.id}: group ${g} has a joint`);
    // weapons name a group that has a muzzle (or none: the whole body bites)
    for (const w of e.behaviors) if (w.type === 'weapon' && w.group) assert.ok(groups.has(w.group), `${e.id}: weapon group ${w.group} exists`);
  }
  // earlier catalog entries (vehicles, creatures) keep their names
  for (const e of CATALOG.slice(0, CATALOG.length - COMBAT_CATALOG.length)) for (const n of e.names) assert.ok(!names.has(n), `${n} stolen from ${e.id}`);
  const cost = (id: string) => validateSpec(catSpec(id), 1e6).cost;
  const costs = COMBAT_CATALOG.map((e) => [e.id, cost(e.id)] as const);
  console.log('costs: ' + costs.map(([id, c]) => `${id} ${c.toFixed(1)}`).join(', '));
  for (const [id, c] of costs) assert.ok(c > 3 && c < 60, `${id}: cost ${c.toFixed(1)} within the willpower budget`);
  // a siege tower costs more than a thrown boomerang; a gatling more than a spring trap
  assert.ok(cost('siege_tower') > cost('boomerang') * 2);
  assert.ok(cost('gatling_turret') > cost('spring_trap'));
});

test('every combat entry validates, spawns, animates and runs 5 s without errors or NaN', async () => {
  const lines: string[] = [];
  for (const e of COMBAT_CATALOG) {
    const { w } = await makeWorld('flat');
    const foe = target(w, new Vector3(2, 1.2, -9));
    const spec = catSpec(e.id);
    if (e.at.endsWith('_target')) spec.placement.target = { kind: 'entity', id: foe.id };
    const r = validateSpec(spec, 1e6);
    assert.equal(r.rejected, null, `${e.id} validates`);
    const built = buildEntry(e.id)!;
    assert.equal(r.spec.joints?.length ?? 0, built.joints.length, `${e.id}: every joint survives the validator`);
    assert.ok(r.spec.parts.length <= 48, `${e.id}: part budget`);
    const c = spawn(w, spec);
    const inst = c.insts[0];
    const rest = new Map([...(inst.jointPose ?? [])].map(([g, p]) => [g, { q: p.quat.clone(), p: p.pos.clone() }]));
    let moved = 0;
    for (let s = 0; s < 300; s++) {
      drive(w, foe, w.dt, (t) => new Vector3(Math.sin(t * 2) * 3, 0, 0));
      for (const i of c.insts) {
        assert.ok(Number.isFinite(i.pos.x + i.pos.y + i.pos.z) && Number.isFinite(i.quat.w), `${e.id}: finite pose`);
        for (const [g, p] of i.jointPose ?? []) {
          assert.ok(Number.isFinite(p.pos.x + p.pos.y + p.pos.z + p.quat.x + p.quat.y + p.quat.z + p.quat.w), `${e.id}.${g}: finite joint pose`);
          const r0 = rest.get(g);
          if (r0 && i === inst) moved = Math.max(moved, 1 - Math.abs(r0.q.dot(p.quat)), p.pos.distanceTo(r0.p));
        }
      }
      if (!w.constructs.list.has(c.id)) break; // one-shots (a piano, a shockwave) are gone after acting
    }
    if (built.joints.length) assert.ok(moved > 1e-3, `${e.id}: its joints move (max ${moved.toFixed(4)})`);
    for (const x of w.entities.values()) assert.ok(Number.isFinite(x.hp), `${e.id}: entity hp finite`);
    ownerSafe(w);
    lines.push(`${e.id}:${moved.toFixed(2)}`);
  }
  console.log(`animated: ${lines.join(' ')}`);
});

// ---------------------------------------------------------------------------------------------------- strikes

const STRIKES = ['thunder_hammer', 'greatsword', 'lance', 'war_axe', 'chakram'];

/** A 20 m/s crosser 14 m out; `juke`: it reverses the moment the strike launches. */
async function strikeRun(id: string, juke: boolean) {
  const { w } = await makeWorld('flat');
  const foe = target(w, new Vector3(-7, 1.2, -14));
  const vx = 20;
  foe.body.setLinvel({ x: vx, y: 0, z: 0 }, true);
  run(w, w.dt); // the target is moving when the strike is cast
  const c = spawn(w, catSpec(id));
  const hits = hitLog(w, c, foe);
  const inst = c.insts[0];
  assert.ok(inst.strike && inst.strike.phase === 'go', `${id}: launched`);
  const t0 = w.time;
  let legEnd = Infinity; // world time the first blow finished (hit, or reached its locked point)
  drive(w, foe, 3, () => new Vector3(juke ? -vx : vx, 0, 0), () => {
    if (legEnd === Infinity && inst.strike?.phase !== 'go') legEnd = w.time;
  });
  assert.ok(legEnd < Infinity, `${id}: the first blow finished`);
  const firstLeg = hits.filter((h) => h.t <= legEnd + 1e-6);
  ownerSafe(w);
  return { firstLeg: firstLeg.length, total: hits.length, legEnd: legEnd - t0 };
}

test('strikes: a committed blow hits a 20 m/s straight-mover, and a juke at launch dodges it', async () => {
  for (const id of STRIKES) {
    const straight = await strikeRun(id, false);
    const juked = await strikeRun(id, true);
    console.log(`${id}: straight first-leg hits ${straight.firstLeg} (leg ${straight.legEnd.toFixed(2)} s, total ${straight.total}); juked first-leg hits ${juked.firstLeg} (total ${juked.total})`);
    assert.ok(straight.firstLeg >= 1, `${id} leads a straight-mover`);
    assert.equal(juked.firstLeg, 0, `${id}: the juke dodges the committed first blow`);
  }
});

test('piano drop and giant boot lead a runner from above; harpoon hooks a runner and drags it back', async () => {
  for (const id of ['piano', 'stomp_boot']) {
    const { w } = await makeWorld('flat');
    const foe = target(w, new Vector3(-8, 1.2, -16));
    foe.body.setLinvel({ x: 18, y: 0, z: 0 }, true);
    run(w, w.dt);
    const c = spawn(w, catSpec(id, { placement: { at: 'above_target', target: { kind: 'entity', id: foe.id }, orient: 'face_away' } }));
    const hits = hitLog(w, c, foe);
    drive(w, foe, 2, () => new Vector3(18, 0, 0));
    console.log(`${id}: hits ${hits.length} (${hits.map((h) => h.amount).join(',')})`);
    assert.ok(hits.length >= 1, `${id} lands on a runner`);
    ownerSafe(w);
  }
  const { w } = await makeWorld('flat');
  const foe = target(w, new Vector3(-10, 1.2, -18));
  const c = spawn(w, catSpec('harpoon'));
  let caught = false;
  drive(w, foe, 5, () => new Vector3(15, 0, 0), () => { if (foe.heldBy === c.id) caught = true; return false; });
  const d = w.entityPos(foe).distanceTo(w.player.pos);
  console.log(`harpoon: caught ${caught}, target ends ${d.toFixed(1)} m from the player`);
  assert.ok(caught, 'harpoon catches a 15 m/s runner');
  assert.ok(d < 9, 'and drags it back to you');
});

// ---------------------------------------------------------------------------------------------------- chasers

test('chasers: turrets, siege and swarms eventually hit a 25 m/s straight-mover', async () => {
  const cases: [string, number][] = [['gatling_turret', 6], ['rocket_pod', 6], ['laser_turret', 6], ['crossbow_turret', 6], ['railgun', 6], ['trebuchet', 6], ['drone_swarm', 8], ['bee_swarm', 8]];
  for (const [id, secs] of cases) {
    const { w } = await makeWorld('flat');
    const foe = target(w, new Vector3(-60, 3, -32));
    foe.body.setLinvel({ x: 25, y: 0, z: 0 }, true);
    const c = spawn(w, catSpec(id), new Vector3(0, 0, -4));
    const hits = hitLog(w, c, foe);
    let first = -1;
    drive(w, foe, secs, () => new Vector3(25, 0, 0), (t) => { if (first < 0 && hits.length) first = t; return false; });
    const shots = c.insts[0].m?.shots ?? 0;
    console.log(`${id}: ${hits.length} hits of ${shots} shots on a 25 m/s crosser, first after ${first.toFixed(2)} s`);
    assert.ok(hits.length >= 1, `${id} hits a 25 m/s straight-mover`);
    ownerSafe(w);
  }
});

// ---------------------------------------------------------------------------------------------------- traps

test('traps catch a target that walks into them', async () => {
  const cases: [string, number][] = [['bear_trap', 0], ['spike_floor', 0], ['minefield', 0], ['tar_pit', 0], ['guillotine', 0], ['blade_pendulum', 0], ['scythe_trap', 3.8], ['lava_floor', 0], ['pile_driver', 0], ['pinball_flipper', 0.5]];
  for (const [id, x] of cases) {
    const { w } = await makeWorld('flat');
    const foe = target(w, new Vector3(x, 1.2, -6), false);
    const c = spawn(w, catSpec(id), new Vector3(0, 0, -15));
    const hits = hitLog(w, c, foe);
    let stunned = false;
    drive(w, foe, 4, () => new Vector3(0, 0, -4.5), () => { if (foe.stunnedUntil > w.time) stunned = true; return false; });
    console.log(`${id}: hits ${hits.length} (${[...new Set(hits.map((h) => h.cause))].join('/')}), stunned ${stunned}`);
    assert.ok(hits.length >= 1 || stunned, `${id} catches a walker`);
    ownerSafe(w);
  }
  // a meteor shower parks over a jogger and rains on them
  {
    const { w } = await makeWorld('flat');
    const foe = target(w, new Vector3(-4, 1.2, -20), false);
    const c = spawn(w, catSpec('meteor_shower', { placement: { at: 'above_target', target: { kind: 'entity', id: foe.id }, orient: 'face_away' } }));
    const hits = hitLog(w, c, foe);
    drive(w, foe, 4, () => new Vector3(2, 0, 0));
    console.log(`meteor shower: ${hits.length} hits on a jogger (${c.insts[0].m?.shots} rocks)`);
    assert.ok(hits.length >= 2, 'meteor shower hits a target moving under it');
    ownerSafe(w);
  }
  // a spring trap flings what steps on it; a black hole drags a passer-by in
  {
    const { w } = await makeWorld('flat');
    const foe = target(w, new Vector3(0, 1.2, -9), false);
    spawn(w, catSpec('spring_trap'), new Vector3(0, 0, -15));
    let maxVy = 0;
    drive(w, foe, 2.5, (t) => new Vector3(0, 0, t < 1.3 ? -4.5 : 0), () => { maxVy = Math.max(maxVy, foe.body.linvel().y); return false; });
    console.log(`spring trap: target launched at ${maxVy.toFixed(1)} m/s`);
    assert.ok(maxVy > 15, 'spring trap launches a walker');
  }
  {
    const { w } = await makeWorld('flat');
    const foe = target(w, new Vector3(-14, 4, -20));
    const c = spawn(w, catSpec('black_hole'), new Vector3(0, 4, -20));
    const hits = hitLog(w, c, foe);
    drive(w, foe, 4, () => { const v = foe.body.linvel(); return new Vector3(v.x, v.y, v.z); });
    const d = w.entityPos(foe).distanceTo(c.insts[0].pos);
    console.log(`black hole: target pulled to ${d.toFixed(1)} m, hits ${hits.length}`);
    assert.ok(d < 8 && hits.length >= 1, 'black hole pulls a target in and grinds it');
  }
});

test('area denial: a shockwave blasts everything around the target, minefields and bear traps don\'t fire on the owner', async () => {
  const { w } = await makeWorld('flat');
  const a = target(w, new Vector3(-3, 1.2, -20), false);
  const b = target(w, new Vector3(4, 1.2, -18), false);
  spawn(w, catSpec('shockwave', { placement: { at: 'under_target', target: { kind: 'entity', id: a.id }, orient: 'face_away' } }));
  run(w, 1.5);
  console.log(`shockwave: hp ${a.hp}, ${b.hp}`);
  assert.ok(a.hp < a.maxHp && b.hp < b.maxHp, 'both targets in the blast take damage');
  // the owner walks over their own traps unharmed
  const o = await makeWorld('flat');
  for (const id of ['bear_trap', 'minefield', 'spike_floor', 'lava_floor']) spawn(o.w, catSpec(id), new Vector3(0, 0, -6));
  run(o.w, 3, () => ({ moveZ: 1 }));
  ownerSafe(o.w);
});

// ---------------------------------------------------------------------------------------------------- language

test('the language layer builds combat constructs by name (and typos / slang)', async () => {
  const cases: [string, string][] = [
    ['a gatling turret', 'gatling_turret'], ['build a trebuchet', 'trebuchet'], ['put a bear trap there', 'bear_trap'],
    ['a beartrap', 'bear_trap'], ['make a black hole', 'black_hole'], ['spawn a rubber ducky', 'rubber_duck'], ['a vending machine', 'vending_machine'],
    ['a railgun', 'railgun'], ['the floor is lava', 'lava_floor'], ['a guilotine', 'guillotine'],
  ];
  for (const [text, id] of cases) {
    const { w } = await makeWorld('drones');
    const d = new Director(w);
    await d.commit(text);
    const c = [...w.constructs.list.values()].find((x) => x.spec.concept === id);
    assert.ok(c, `"${text}" → ${id} (got ${[...w.constructs.list.values()].map((x) => x.spec.concept).join(',') || 'nothing'})`);
    assert.ok((c.spec.joints?.length ?? 0) > 0, `"${text}": articulated`);
  }
});

// ---------------------------------------------------------------------------------------------------- determinism + perf

test('combat constructs are deterministic', async () => {
  const sim = async () => {
    const { w } = await makeWorld('flat');
    const foe = target(w, new Vector3(-20, 3, -25));
    spawn(w, catSpec('gatling_turret'), new Vector3(0, 0, -4));
    spawn(w, catSpec('rocket_pod'), new Vector3(6, 0, -4));
    spawn(w, catSpec('bee_swarm'), new Vector3(-6, 0, -4));
    spawn(w, catSpec('trebuchet'), new Vector3(0, 0, 6));
    drive(w, foe, 4, () => new Vector3(10, 0, 0));
    return [...w.constructs.list.values()].map((c) => c.insts[0].pos.toArray().map((x) => x.toFixed(4)).join(',')).join('|') + ` hp ${foe.hp.toFixed(3)}`;
  };
  assert.equal(await sim(), await sim());
});

test('perf: 20 combat constructs active ≤ 3 ms/step p50', async () => {
  const { w } = await makeWorld('flat');
  const foes = [target(w, new Vector3(-30, 3, -30)), target(w, new Vector3(30, 5, -40)), target(w, new Vector3(0, 1.2, -20), false)];
  const ids = [
    'gatling_turret', 'rocket_pod', 'laser_turret', 'tesla_coil', 'flame_turret', 'crossbow_turret', 'blunderbuss', 'net_turret',
    'bear_trap', 'spike_floor', 'blade_wall', 'lava_floor', 'blade_pendulum', 'scythe_trap', 'minefield',
    'trebuchet', 'field_cannon', 'bee_swarm', 'rolling_saw', 'wrecking_crane',
  ];
  ids.forEach((id, i) => spawn(w, catSpec(id), new Vector3(((i % 5) - 2) * 12, 0, -8 - Math.floor(i / 5) * 12)));
  assert.equal(w.constructs.list.size, 20);
  const vel = (i: number) => (t: number) => new Vector3(Math.sin(t * 0.7 + i) * 12, 0, Math.cos(t * 0.5 + i) * 12);
  const step = (t: number) => { foes.forEach((f, i) => { if (!f.heldBy && !f.restrainedBy) { const v = vel(i)(t); const c = f.body.linvel(); f.body.setLinvel({ x: v.x, y: f.body.gravityScale() === 0 ? 0 : c.y, z: v.z }, true); } }); run(w, w.dt); };
  for (let i = 0; i < 60; i++) step(i * w.dt);
  const times: number[] = [];
  for (let i = 0; i < 300; i++) { const t0 = performance.now(); step((60 + i) * w.dt); times.push(performance.now() - t0); }
  times.sort((a, b) => a - b);
  const p50 = times[Math.floor(times.length / 2)], p90 = times[Math.floor(times.length * 0.9)];
  console.log(`20 combat constructs: p50 ${p50.toFixed(2)} ms, p90 ${p90.toFixed(2)} ms/step, ${w.projectiles.size} projectiles in flight, ${w.constructs.list.size} alive`);
  assert.ok(p50 <= 3, `p50 ${p50.toFixed(2)} ms`);
  ownerSafe(w);
});
