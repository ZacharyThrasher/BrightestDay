/**
 * Combat catalog (beasts.ts / monsters.ts / rides.ts): every entry validates, spawns, animates and runs without
 * NaNs; a representative sample fights hostile targets that move like a lantern does (chasers catch a straight
 * runner, chargers can be side-stepped, flyers stay up, rides carry you), and a crowd of them stays cheap.
 * World state only, deterministic.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeWorld, Vector3 } from './helpers.js';
import { defaultSpec, type ConstructSpec } from '../src/sim/constructs/spec.js';
import { COMBAT_CATALOG, CATALOG_BY_ID, buildEntry } from '../src/sim/constructs/catalog/index.js';
import { validateSpec, LIMITS } from '../src/sim/constructs/validate.js';
import { MACHINE_LIMITS, machineFunctions } from '../src/sim/constructs/machine-spec.js';
import { buildForm } from '../src/sim/constructs/forms.js';
import { conceptByName } from '../src/lang/lexicon.js';
import { NO_INPUT, type PlayerInput } from '../src/sim/player.js';
import type { World, Entity } from '../src/sim/world.js';
import type { Construct } from '../src/sim/constructs/runtime.js';

function catSpec(id: string, over: Partial<ConstructSpec> = {}): ConstructSpec {
  const e = CATALOG_BY_ID.get(id)!;
  assert.ok(e, `no catalog entry ${id}`);
  return defaultSpec({
    concept: id, label: id, parts: buildForm(id), behaviors: JSON.parse(JSON.stringify(e.behaviors)), motion: e.motion,
    placement: { at: e.at === 'self' ? 'ground_front' : e.at, orient: 'face_away' }, lifetime: 150, ...over,
  });
}

function spawn(w: World, spec: ConstructSpec): Construct {
  w.constructs.energy = 1e6;
  const r = w.constructs.spawn(spec);
  assert.ok(r.construct, r.report.rejected ?? 'no construct');
  return r.construct!;
}

/** Step with a per-tick hook (runs before the step: steer targets, give input). */
function steps(w: World, n: number, fn: (i: number) => Partial<PlayerInput> | void = () => {}) {
  for (let i = 0; i < n; i++) {
    const inp = fn(i) || {};
    w.step({ ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch, ...inp });
  }
}

/** A hostile dummy that runs like a lantern: we set its horizontal velocity every tick. */
function runner(w: World, at: Vector3, hp = 5000): Entity {
  const e = w.addEntity('dummy', at, { hostile: true, hp, label: 'runner' });
  w.refreshQueries();
  return e;
}
function drive(e: Entity, vx: number, vz: number) {
  const v = e.body.linvel();
  e.body.setLinvel({ x: vx, y: v.y, z: vz }, true);
}

const hurt = (e: Entity) => e.hp < e.maxHp;

// ------------------------------------------------------------------ every entry

test('combat catalog: ≥ 40 entries; each validates, owns its names, spawns, animates and runs 5 s cleanly', async () => {
  assert.ok(COMBAT_CATALOG.length >= 40, `only ${COMBAT_CATALOG.length} entries`);
  const ids = new Set<string>();
  for (const e of COMBAT_CATALOG) {
    assert.ok(!ids.has(e.id), `duplicate id ${e.id}`);
    ids.add(e.id);
    assert.ok(!/[\s+,]/.test(e.id), `${e.id}: ids can't hold spaces (concept ids are split on them)`);
    for (const n of [e.id, ...e.names]) assert.equal(conceptByName.get(n)?.catalog, e.id, `"${n}" should name ${e.id}, got ${conceptByName.get(n)?.id}`);
    const built = buildEntry(e.id)!;
    assert.ok(built.parts.length <= 42, `${e.id}: ${built.parts.length} parts leaves no room for add-ons`);
    assert.ok(built.joints.length <= MACHINE_LIMITS.joints, `${e.id}: too many joints`);
    const groups = new Set(built.parts.map((p) => p.group).filter(Boolean));
    for (const j of built.joints) assert.ok(groups.has(j.group), `${e.id}: joint ${j.group} has no parts`);
    for (const b of e.behaviors) if (b.type === 'weapon' && b.group) assert.ok(groups.has(b.group), `${e.id}: weapon group ${b.group} missing`);
    const v = validateSpec(catSpec(e.id), LIMITS.energyMax);
    assert.equal(v.rejected, null, `${e.id}: ${v.rejected}`);
    assert.deepEqual(v.clamps, [], `${e.id} clamped: ${v.clamps.join('; ')}`);
    assert.ok(v.cost > 12 && v.cost < LIMITS.energyMax * 0.45, `${e.id}: cost ${v.cost.toFixed(1)} out of the sensible band`);
    // claims hold up against what the spec actually has
    const f = machineFunctions(v.spec);
    for (const x of e.functions) if (['rideable', 'flies', 'walks', 'shoots', 'follows_player'].includes(x)) assert.ok(f.includes(x), `${e.id} claims ${x} but the spec doesn't support it`);

    const { w } = await makeWorld('flat');
    const target = runner(w, new Vector3(5, 1.2, -22), 400);
    const c = spawn(w, catSpec(e.id));
    assert.ok(c.mach?.drive, `${e.id} drives`);
    assert.equal(c.spec.joints?.length ?? 0, built.joints.length, `${e.id}: joints kept`);
    const pose0 = new Map([...(c.insts[0].jointPose ?? [])].map(([g, p]) => [g, { q: p.quat.clone(), p: p.pos.clone() }]));
    const pos0 = c.insts[0].pos.clone();
    let moved = 0;
    for (let s = 0; s < 300; s++) {
      steps(w, 1, () => drive(target, s < 150 ? 6 : -6, 0));
      const inst = c.insts[0];
      assert.ok(inst && isFinite(inst.pos.x) && isFinite(inst.pos.y) && isFinite(inst.quat.w), `${e.id}: finite transform`);
      for (const [g, p] of inst.jointPose ?? []) {
        assert.ok(isFinite(p.pos.x) && isFinite(p.pos.y) && isFinite(p.quat.w), `${e.id}.${g} pose finite`);
        const p0 = pose0.get(g);
        if (p0) moved = Math.max(moved, 1 - Math.abs(p0.q.dot(p.quat)), p0.p.distanceTo(p.pos));
      }
    }
    assert.ok(moved > 1e-4, `${e.id}: something articulates`);
    assert.ok(c.insts[0].pos.distanceTo(pos0) > 2, `${e.id}: gets moving`);
    assert.equal(w.player.hp, 100, `${e.id} never hurts its owner`);
  }
});

// ------------------------------------------------------------------ chasers

test('chasers run down a target sprinting away at 20 m/s (capped at the chaser speed)', async () => {
  const out: string[] = [];
  for (const id of ['wolf', 'raptor', 'tiger', 'werewolf', 'shark', 'piranhas']) {
    const { w } = await makeWorld('flat');
    const c = spawn(w, catSpec(id));
    const t = runner(w, new Vector3(0, 1.2, -16));
    let caught = -1, maxSp = 0;
    for (let s = 0; s < 10 * 60 && caught < 0; s++) {
      steps(w, 1, () => drive(t, 20 * Math.SQRT1_2, -20 * Math.SQRT1_2)); // straight line, diagonally away
      maxSp = Math.max(maxSp, Math.hypot(c.insts[0].m!.vel.x, c.insts[0].m!.vel.z));
      if (hurt(t)) caught = s / 60;
    }
    out.push(`${id} ${caught < 0 ? 'never' : caught.toFixed(2) + ' s'} (top ${maxSp.toFixed(1)} m/s)`);
    assert.ok(caught >= 0, `${id} should catch a 20 m/s straight-line runner`);
    assert.ok(maxSp <= 34, `${id} respects the chaser cap (${maxSp.toFixed(1)})`);
  }
  console.log('chasers:', out.join(', '));
});

test('a hard cut shakes a chaser for a moment: the turn rate is limited', async () => {
  const { w } = await makeWorld('flat');
  const c = spawn(w, catSpec('wolf'));
  const t = runner(w, new Vector3(0, 1.2, -14));
  // flee straight at 22 m/s; when the wolf is 5 m behind, cut 90° at 24 m/s
  let cutAt = -1, gapAtCut = 0, maxAfter = 0;
  for (let s = 0; s < 8 * 60; s++) {
    const gap = w.entityPos(t).distanceTo(c.insts[0].pos);
    if (cutAt < 0 && s > 30 && gap < 5) { cutAt = s; gapAtCut = gap; }
    if (cutAt >= 0 && s - cutAt < 40) maxAfter = Math.max(maxAfter, gap);
    if (cutAt >= 0 && s - cutAt >= 40) break;
    steps(w, 1, () => (cutAt < 0 ? drive(t, 0, -22) : drive(t, 24, 0)));
  }
  console.log(`wolf vs a hard cut: gap ${gapAtCut.toFixed(1)} m → opens to ${maxAfter.toFixed(1)} m`);
  assert.ok(cutAt >= 0, 'the wolf closes in');
  assert.ok(maxAfter > gapAtCut + 3, 'the cut buys distance');
});

// ------------------------------------------------------------------ chargers

test('chargers commit: a late side-step makes rhino, bull, train and minotaur overshoot', async () => {
  const out: string[] = [];
  for (const id of ['rhino', 'bull', 'train', 'minotaur']) {
    const { w } = await makeWorld('flat');
    const c = spawn(w, catSpec(id));
    const t = runner(w, new Vector3(0, 1.2, -45));
    let dodgeAt = -1, hitDuringPass = false, closing = 0;
    for (let s = 0; s < 6 * 60; s++) {
      const gap = Math.hypot(w.entityPos(t).x - c.insts[0].pos.x, w.entityPos(t).z - c.insts[0].pos.z);
      if (dodgeAt < 0 && gap < 11) dodgeAt = s;
      steps(w, 1, () => drive(t, dodgeAt >= 0 && s - dodgeAt < 45 ? 20 : 0, 0));
      if (dodgeAt < 0) closing = Math.max(closing, Math.hypot(c.insts[0].m!.vel.x, c.insts[0].m!.vel.z));
      if (dodgeAt >= 0 && s - dodgeAt < 60 && hurt(t)) hitDuringPass = true;
      if (dodgeAt >= 0 && s - dodgeAt >= 60) break;
    }
    out.push(`${id}: charged at ${closing.toFixed(1)} m/s, dodge ${dodgeAt >= 0 ? 'made' : 'never needed'}, ${hitDuringPass ? 'HIT' : 'missed'}`);
    assert.ok(dodgeAt >= 0, `${id} should charge the target`);
    assert.ok(closing > 14, `${id} charges fast`);
    assert.ok(!hitDuringPass, `${id}: a side-step at 11 m should dodge the charge`);
  }
  console.log('chargers:', out.join('; '));
  // ... but standing still gets you run over
  const { w } = await makeWorld('flat');
  spawn(w, catSpec('rhino'));
  const t = runner(w, new Vector3(0, 1.2, -40));
  steps(w, 5 * 60, () => drive(t, 0, 0));
  assert.ok(hurt(t), 'a target that stands still gets gored');
});

// ------------------------------------------------------------------ flyers

test('flyers stay airborne while hunting a target that runs in circles, and hit it', async () => {
  const out: string[] = [];
  const res: { id: string; ok: boolean }[] = [];
  for (const id of ['eagle', 'bats', 'bees', 'phoenix', 'ice_dragon', 'storm_dragon', 'fighter_jet', 'bomber', 'ufo']) {
    const { w } = await makeWorld('flat');
    const c = spawn(w, catSpec(id));
    const t = runner(w, new Vector3(0, 1.2, -25));
    steps(w, 90);
    let low = Infinity, n = 0, up = 0;
    for (let s = 0; s < 10 * 60; s++) {
      const a = s / 60 * 0.9;
      steps(w, 1, () => drive(t, Math.cos(a) * 16, -Math.sin(a) * 16));
      const inst = c.insts[0];
      const g = w.constructs.machines.groundY(inst.pos.x, inst.pos.y + 1, inst.pos.z, 200, inst.body) ?? 0;
      const clear = inst.pos.y + c.extent.min.y - g;
      low = Math.min(low, clear);
      if (clear > 1.5) up++;
      n++;
    }
    out.push(`${id} min clearance ${low.toFixed(1)} m, up ${Math.round((100 * up) / n)}%, target hp ${t.hp.toFixed(0)}`);
    res.push({ id, ok: low > 0.8 && up / n > 0.9 && hurt(t) && w.player.hp === 100 });
  }
  console.log('flyers:', out.join('; '));
  for (const r of res) assert.ok(r.ok, `${r.id}: stays airborne (> 0.8 m, > 1.5 m 90% of the time), hurts the target, spares the owner`);
});

// ------------------------------------------------------------------ rides

test('rides carry the player ≥ 8 m with forward input (seated on creation, or G to get in)', async () => {
  const out: string[] = [];
  for (const id of ['horse', 'hoverbike', 'rocket_ride', 'roller_coaster', 'chariot', 'griffin', 'rhino', 'monster_truck', 'elephant', 't-rex', 'fighter_jet']) {
    const e = CATALOG_BY_ID.get(id)!;
    const { w } = await makeWorld('flat');
    const c = spawn(w, catSpec(id, { placement: { at: e.at === 'self' ? 'self' : 'ground_front', orient: 'face_away' } }));
    if (!w.player.riding) { steps(w, 1, () => ({ use: true })); steps(w, 1); }
    assert.equal(w.player.riding?.id, c.id, `${id}: player is aboard`);
    const p0 = w.player.pos.clone();
    steps(w, 3 * 60, () => ({ moveZ: 1 }));
    const d = w.player.pos.distanceTo(p0);
    out.push(`${id} ${d.toFixed(1)} m`);
    assert.ok(d >= 8, `${id} carried the player only ${d.toFixed(1)} m`);
    assert.equal(w.player.hp, 100);
  }
  console.log('rides:', out.join(', '));
});

// ------------------------------------------------------------------ specials

test('specials: mummy wraps, kraken ink and the UFO tractor beam pin a target; gorilla/yeti/spider-queen lob', async () => {
  for (const id of ['mummy', 'kraken']) {
    const { w } = await makeWorld('flat');
    w.addEntity('drone', new Vector3(0, 3, -16), { hostile: true, hp: 5000 });
    w.refreshQueries();
    spawn(w, catSpec(id));
    let netted = false;
    w.listeners.push((ev) => { if (ev.type === 'netted') netted = true; });
    steps(w, 8 * 60);
    assert.ok(netted, `${id} snares the drone`);
  }
  for (const id of ['gorilla', 'yeti', 'spider_queen']) {
    const { w } = await makeWorld('flat');
    runner(w, new Vector3(0, 1.2, -26));
    let lobs = 0;
    w.listeners.push((ev) => { if (ev.type === 'construct_fire' && ev.kind === 'bomb') lobs++; });
    spawn(w, catSpec(id));
    steps(w, 5 * 60);
    assert.ok(lobs > 0, `${id} throws`);
  }
  {
    // the UFO's tractor beam pins a drone; with nothing to hunt it cruises around its owner without bothering them
    const { w } = await makeWorld('flat');
    w.addEntity('drone', new Vector3(0, 4, -22), { hostile: true, hp: 5000 });
    w.refreshQueries();
    spawn(w, catSpec('ufo'));
    let netted = false;
    w.listeners.push((ev) => { if (ev.type === 'netted') netted = true; });
    steps(w, 8 * 60);
    assert.ok(netted, 'ufo tractor-beams the drone');
    const b = await makeWorld('flat');
    spawn(b.w, catSpec('ufo'));
    const y0 = b.w.player.pos.y;
    let maxY = y0;
    steps(b.w, 10 * 60, () => { maxY = Math.max(maxY, b.w.player.pos.y); });
    assert.ok(maxY - y0 < 0.5, `the owner stays put (rose ${(maxY - y0).toFixed(2)} m)`);
  }
});

test('a wolf pack: "five wolves" are independent hunters that gang up on a weaving runner', async () => {
  const res: string[] = [];
  for (const arrangement of ['line', 'scatter'] as const) {
    const { w } = await makeWorld('flat');
    const t = runner(w, new Vector3(0, 1.2, -20));
    const c = spawn(w, catSpec('wolf', { count: 5, arrangement }));
    assert.equal(c.insts.length, 5);
    steps(w, 6 * 60, (i) => drive(t, Math.sin(i / 60) * 12, -8));
    const biters = c.insts.filter((i) => i.m!.shots > 0).length;
    res.push(`${arrangement}: ${biters}/5 bit, runner lost ${(t.maxHp - t.hp).toFixed(0)} hp`);
    // (pack members still body-block each other when they converge on one point: see the report's engine gaps)
    assert.ok(biters >= 2, `${arrangement}: several wolves get their teeth in`);
  }
  console.log('wolf pack:', res.join('; '));
});

// ------------------------------------------------------------------ perf

test('perf: 20 combat constructs fighting ≤ 3 ms/step p50', async () => {
  const { w } = await makeWorld('flat');
  const targets = [runner(w, new Vector3(-20, 1.2, -30)), runner(w, new Vector3(20, 1.2, -30)), runner(w, new Vector3(0, 1.2, -50))];
  const ids = ['wolf', 'raptor', 'tiger', 'bees', 'bats', 'eagle', 'rhino', 'bear', 't-rex', 'scorpion', 'kraken', 'golem', 'zombies', 'skeletons', 'ice_dragon', 'hydra', 'monster_truck', 'battleship', 'fighter_jet', 'ufo'];
  ids.forEach((id, i) => {
    const inst = spawn(w, catSpec(id)).insts[0];
    inst.pos.set((i % 5) * 9 - 18, inst.pos.y, -Math.floor(i / 5) * 9);
    inst.body.setTranslation(inst.pos, true);
  });
  assert.equal(w.constructs.list.size, 20);
  steps(w, 60, (i) => targets.forEach((t, k) => drive(t, Math.cos(i / 40 + k) * 15, Math.sin(i / 40 + k) * 15)));
  const times: number[] = [];
  for (let s = 0; s < 300; s++) {
    const t0 = performance.now();
    steps(w, 1, () => targets.forEach((t, k) => drive(t, Math.cos((s + 60) / 40 + k) * 15, Math.sin((s + 60) / 40 + k) * 15)));
    times.push(performance.now() - t0);
  }
  times.sort((a, b) => a - b);
  const p50 = times[times.length >> 1], p95 = times[Math.floor(times.length * 0.95)];
  console.log(`20 combat constructs: step p50 ${p50.toFixed(2)} ms, p95 ${p95.toFixed(2)} ms, alive ${w.constructs.list.size}`);
  assert.ok(p50 <= 3, `p50 ${p50.toFixed(2)} ms`);
});

test('combat catalog is deterministic', async () => {
  const sim = async () => {
    const { w } = await makeWorld('flat');
    const t = runner(w, new Vector3(0, 1.2, -25));
    for (const id of ['wolf', 'rhino', 'eagle', 'skeletons', 'ufo']) spawn(w, catSpec(id));
    steps(w, 4 * 60, (i) => drive(t, Math.sin(i / 30) * 14, -6));
    return [...w.constructs.list.values()].map((c) => c.insts[0].pos.toArray().map((x) => x.toFixed(4)).join(',')).join('|') + ` hp ${t.hp.toFixed(3)}`;
  };
  assert.equal(await sim(), await sim());
});
