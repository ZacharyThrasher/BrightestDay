import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Quaternion } from 'three';
import { makeWorld, run, look, Vector3 } from './helpers.js';
import { defaultSpec, type ConstructSpec } from '../src/sim/constructs/spec.js';
import { buildForm } from '../src/sim/constructs/forms.js';
import { CATALOG_BY_ID } from '../src/sim/constructs/catalog/index.js';
import { Director } from '../src/lang/director.js';
import { attachPracticeBot } from '../src/lang/practice.js';
import { NO_INPUT } from '../src/sim/player.js';
import type { World } from '../src/sim/world.js';

/** Duel arena with a second lantern 'p2' standing `dist` m in front of p1 (p1 looks at them). Match live. */
async function duel(dist = 14, opts: { match?: boolean } = {}) {
  const { w, info } = await makeWorld('duel');
  const p1 = w.player;
  const p2 = w.addPlayer('p2');
  p1.teleport(new Vector3(10, 1, 20));
  p2.teleport(new Vector3(10, 1, 20 - dist));
  w.refreshQueries();
  if (opts.match !== false) {
    w.startMatch({ warmup: 0 });
    p1.teleport(new Vector3(10, 1, 20));
    p2.teleport(new Vector3(10, 1, 20 - dist));
    p1.protectedUntil = p2.protectedUntil = -Infinity;
    w.refreshQueries();
    run(w, 2 / 60);
  }
  look(w, p2.pos);
  return { w, info, p1, p2 };
}

function spec(form: string, over: Partial<ConstructSpec> = {}): ConstructSpec {
  return defaultSpec({ concept: form, label: form, parts: buildForm(form), ...over });
}

function catSpec(id: string, over: Partial<ConstructSpec> = {}): ConstructSpec {
  const e = CATALOG_BY_ID.get(id)!;
  return defaultSpec({ concept: id, label: id, parts: buildForm(id), behaviors: JSON.parse(JSON.stringify(e.behaviors)), motion: e.motion, placement: { at: e.at, orient: 'face_away' }, lifetime: 150, ...over });
}

const events = (w: World, type: string) => w.log.filter((e) => e.type === type);

test('pvp: p1’s hammer strike damages p2 (knockback, attributed); p1’s constructs never hurt p1', async () => {
  const { w, p1, p2 } = await duel(12);
  p1.energy = 1e6;
  const r = w.constructs.spawn(spec('hammer', { motion: 'kinematic', placement: { at: 'hand', orient: 'along_look' }, behaviors: [{ type: 'strike', target: { kind: 'player', id: 'p2' }, speed: 50, after: 'return' }] }), { owner: 'p1' });
  assert.ok(r.construct);
  const v0 = p2.vel.clone();
  run(w, 1.2);
  const dmg = events(w, 'player_damage').filter((e) => e.target === 'p2');
  console.log(`hammer → p2 hp ${p2.hp.toFixed(0)}, hits ${dmg.map((e) => `${e.amount} (${e.cause})`).join(', ')}, knock ${p2.vel.clone().sub(v0).length().toFixed(1)} m/s`);
  assert.ok(p2.hp < 100, 'the strike should hurt p2');
  assert.equal(dmg[0].attacker, 'p1');
  assert.equal(dmg[0].by, r.construct!.id);
  // now p1's own things: a strike aimed at p1 and a bomb going off in p1's face
  const hp1 = p1.hp;
  w.constructs.spawn(spec('hammer', { motion: 'kinematic', placement: { at: 'front', orient: 'face_away' }, behaviors: [{ type: 'strike', target: { kind: 'player', id: 'p1' }, speed: 50, after: 'return' }] }), { owner: 'p1' });
  w.constructs.spawn(spec('ball', { placement: { at: 'self', orient: 'face_away' }, behaviors: [{ type: 'explode', radius: 6, force: 40, delay: 0.1, onContact: false }] }), { owner: 'p1' });
  run(w, 1.5);
  assert.equal(p1.hp, hp1, 'a construct never hurts its owner');
  assert.equal(events(w, 'player_damage').filter((e) => e.target === 'p1').length, 0);
});

test('pvp: a p2 tank shell kills p1 → respawn after 3 s at the far spawn, with a spawn shield', async () => {
  const { w, p1, p2 } = await duel(26);
  p2.energy = 1e6;
  const tank = w.constructs.spawn(catSpec('tank', { behaviors: [...catSpec('tank').behaviors.filter((b) => b.type !== 'brain'), { type: 'brain', goal: 'hunt', target: { kind: 'opponent' } }] }), { owner: 'p2', at: { pos: p2.pos.clone().add(new Vector3(3, -1, 0)), quat: new Quaternion() } });
  assert.ok(tank.construct, tank.report.rejected ?? '');
  assert.equal(tank.construct!.owner, 'p2');
  p1.hp = 12;
  let killedAt = -1;
  for (let i = 0; i < 12 * 60 && killedAt < 0; i++) {
    w.step({ ...NO_INPUT, yaw: p1.yaw, pitch: p1.pitch });
    if (!p1.alive) killedAt = w.time;
  }
  const kill = events(w, 'player_killed').find((e) => e.target === 'p1');
  console.log(`tank kill after ${killedAt.toFixed(2)} s: ${JSON.stringify(kill && { by: kill.by, attacker: kill.attacker, weapon: kill.weapon, cause: kill.cause })}`);
  assert.ok(kill, 'p2’s tank should kill p1');
  assert.equal(kill!.attacker, 'p2');
  assert.equal(events(w, 'kill').at(-1)?.killer, 'p2');
  assert.equal(w.match!.scores.p2, 1);
  assert.ok(!w.constructs.list.size || [...w.constructs.list.values()].every((c) => c.owner !== 'p1'), 'the dead lantern’s constructs dissolve');
  run(w, 2.9);
  assert.ok(!p1.alive, 'still down before 3 s');
  run(w, 0.2);
  assert.ok(p1.alive, 'respawned after 3 s');
  assert.equal(p1.hp, 100);
  // the spawn farthest from the opponent
  const far = [...w.spawns].sort((a, b) => b.pos.distanceTo(p2.pos) - a.pos.distanceTo(p2.pos))[0];
  const ev = events(w, 'player_respawn').at(-1)!;
  assert.deepEqual(ev.pos.map((x: number) => Math.round(x)), far.pos.toArray().map((x) => Math.round(x)));
  assert.ok(p1.isProtected, 'spawn shield up');
  assert.equal(w.damagePlayer(p1, 50, 'test', tank.construct!.id), 0, 'shielded: no damage');
  run(w, 2.05);
  assert.ok(!p1.isProtected, 'shield drops after 2 s');
});

test('pvp: first to 3 kills ends the match; rematch resets', async () => {
  const { w, p1, p2 } = await duel(20);
  const m = w.match!;
  p1.energy = 1e6;
  const hammer = w.constructs.spawn(spec('hammer', { placement: { at: 'front', orient: 'face_away' } }), { owner: 'p1' }).construct!;
  for (let k = 0; k < 3; k++) {
    // respawn shield: wait it out, then land the kill
    while (!p2.alive || p2.isProtected) run(w, 0.1);
    assert.equal(m.phase, 'live');
    w.damagePlayer(p2, 200, 'strike', hammer.id);
    run(w, 0.05);
  }
  console.log(`scores ${JSON.stringify(m.scores)}, phase ${m.phase}, winner ${m.winner}, kills ${m.kills.map((k) => `${k.killer}>${k.victim} (${k.weapon})`).join(', ')}`);
  assert.equal(m.scores.p1, 3);
  assert.equal(m.phase, 'round_end');
  assert.equal(m.winner, 'p1');
  assert.equal(events(w, 'kill').length, 3);
  assert.equal(events(w, 'kill')[0].weapon, 'hammer');
  assert.equal(w.damagePlayer(p1, 10, 'strike', 'p2'), 0, 'no damage once the round is over');
  run(w, 3.1);
  assert.equal(m.phase, 'match_end');
  assert.ok(events(w, 'match_state').some((e) => e.phase === 'match_end' && e.winner === 'p1'));
  m.rematch();
  assert.equal(m.phase, 'warmup');
  assert.deepEqual(m.scores, { p1: 0, p2: 0 });
  assert.equal(w.constructs.list.size, 0);
  run(w, 3.05);
  assert.equal(m.phase, 'live');
});

test('pvp: time cap — the leader wins; a tie goes to sudden death (next kill wins)', async () => {
  const { w, p1, p2 } = await duel(20, { match: false });
  const m = w.startMatch({ warmup: 0, roundTime: 2 });
  run(w, 0.1);
  p1.protectedUntil = p2.protectedUntil = -Infinity;
  run(w, 2.1);
  assert.ok(m.suddenDeath && m.phase === 'live', 'tied at the cap → sudden death');
  w.damagePlayer(p1, 500, 'strike', 'p2');
  run(w, 0.05);
  assert.equal(m.phase, 'round_end');
  assert.equal(m.winner, 'p2');
});

test('pvp: "grab him and throw him" grabs the opponent lantern and throws them', async () => {
  const { w, p1, p2 } = await duel(12);
  const d = new Director(w, 'p1');
  const r = await d.commit('grab him and throw him');
  console.log(`"${r.text}" → ${r.summary}`);
  assert.ok(r.applied, r.summary);
  let maxSpeed = 0;
  const p0 = p2.pos.clone();
  for (let i = 0; i < 4 * 60; i++) {
    run(w, 1 / 60);
    if (events(w, 'player_thrown').length) maxSpeed = Math.max(maxSpeed, p2.vel.length());
  }
  const grabbed = events(w, 'grabbed').find((e) => e.player === 'p2');
  const thrown = events(w, 'player_thrown').find((e) => e.player === 'p2');
  console.log(`grabbed by ${grabbed?.construct}, thrown ${JSON.stringify(thrown)}, max speed ${maxSpeed.toFixed(1)}, flung ${p2.pos.distanceTo(p0).toFixed(1)} m, p2 hp ${p2.hp.toFixed(0)}`);
  assert.ok(grabbed, 'the hand grabs p2 (the player, not a creature)');
  assert.equal(w.constructs.get(grabbed!.construct)?.owner ?? 'p1', 'p1');
  assert.ok(thrown && maxSpeed > 15 && p2.pos.distanceTo(p0) > 8, 'and throws them');
  assert.ok(p2.hp < 100, 'the grab / the landing hurts');
  assert.equal(p1.hp, 100);
});

test('pvp: lock-on picks the opponent within ~15° of the crosshair, not outside it', async () => {
  const { w, p1, p2 } = await duel(30);
  const cam = () => p1.camera();
  // 8° off to the side: locked
  look(w, p2.pos);
  p1.yaw += (8 * Math.PI) / 180;
  assert.equal(w.lockTarget('p1')?.id, 'p2', 'within the cone');
  // 30° off, 15 m from the ray: no lock
  p1.yaw += (22 * Math.PI) / 180;
  const off = (() => { const c = cam(); const to = p2.pos.clone().sub(c.pos); return Math.sqrt(to.lengthSq() - to.dot(c.dir) ** 2); })();
  assert.ok(off > 6);
  assert.equal(w.lockTarget('p1'), null, 'outside the cone');
  // a plain crosshair cast ("punch") near the opponent locks onto them
  look(w, p2.pos);
  p1.yaw += (9 * Math.PI) / 180;
  const d = new Director(w, 'p1');
  // (a practice dummy stands at the far end: a hostile-less crowd; the lock still wins over "nearest")
  const i = d.interpret('a fist that punches');
  const strike = i.ops.flatMap((o) => (o.op === 'create' ? o.spec.behaviors : [])).find((b) => b.type === 'strike') as any;
  console.log(`"a fist that punches" 9° off the opponent → strike target ${JSON.stringify(strike?.target)}`);
  assert.deepEqual(strike?.target, { kind: 'player', id: 'p2' });
  // "him" always resolves to the opponent, even aiming the other way
  p1.yaw += Math.PI;
  const j = d.interpret('drop a piano on him');
  const tgt = JSON.stringify(j.ops.flatMap((o) => (o.op === 'create' ? [o.spec.placement.target, ...o.spec.behaviors.map((b: any) => b.target)] : [])));
  assert.ok(tgt.includes('"p2"'), tgt);
});

test('pvp: willpower pools and inspiration are per player; players body-block', async () => {
  const { w, p1, p2 } = await duel(10);
  const a = new Director(w, 'p1');
  const b = new Director(w, 'p2');
  const e1 = p1.energy, e2 = p2.energy;
  a.commitSync('a wall');
  assert.ok(p1.energy < e1 - 1, 'p1 paid');
  assert.equal(p2.energy, e2, 'p2 untouched');
  const bw = b.commitSync('a big wall');
  assert.ok(bw.applied);
  assert.equal(w.constructs.get(bw.constructIds[0])!.owner, 'p2');
  assert.ok(p2.energy < e2 - 1);
  for (let i = 0; i < 3; i++) a.commitSync('a wall');
  const ins = events(w, 'inspiration');
  assert.ok(ins.filter((e) => e.player === 'p1').at(-1)!.stale, 'p1 went stale');
  assert.ok(!ins.filter((e) => e.player === 'p2').at(-1)!.stale, 'p2 did not');
  assert.ok(w.inspiration.of('p1').level < w.inspiration.of('p2').level);
  // body-block: p1 runs into p2 standing in the way and is stopped / shoves them
  w.constructs.dismissAll();
  p1.teleport(new Vector3(20, 1, 0)); p2.teleport(new Vector3(20, 1, -4));
  w.refreshQueries();
  run(w, 1, () => ({ moveZ: 1, yaw: 0, pitch: 0 }));
  console.log(`body-block: p1 z ${p1.pos.z.toFixed(2)}, p2 z ${p2.pos.z.toFixed(2)}`);
  assert.ok(p1.pos.z > p2.pos.z + 0.8, 'p1 never passes through p2');
});

test('pvp: two runs of a scripted duel (bot + scripted casts) give identical state hashes', async () => {
  const once = async () => {
    const { w } = await makeWorld('duel');
    const { bot } = attachPracticeBot(w);
    const d = new Director(w, 'p1');
    w.startMatch();
    const hashes: string[] = [];
    const script: [number, string][] = [[240, 'a wall'], [420, 'punch him'], [600, 'a cannon that shoots him'], [800, 'grab him and throw him']];
    for (let i = 0; i < 1200; i++) {
      for (const [at, text] of script) if (i === at) d.commitSync(text);
      w.step({ ...NO_INPUT, yaw: w.player.yaw + Math.sin(i * 0.01) * 0.002, pitch: -0.05, moveX: Math.sin(i / 70) > 0 ? 1 : -1, jump: i % 90 === 0 });
      if (i % 120 === 0) hashes.push(w.stateHash());
    }
    return { hashes, typed: (bot as any).typed.map((x: any) => x.text), events: w.log.filter((e) => ['player_damage', 'kill', 'construct_formed'].includes(e.type)).length, snap: w.snapshotMatch() };
  };
  const a = await once(), b = await once();
  console.log(`bot typed: ${a.typed.join(' | ')}; ${a.events} combat/cast events; final hash ${a.hashes.at(-1)}; p1 hp ${a.snap.players[0].hp}, p2 hp ${a.snap.players[1].hp}, score ${JSON.stringify(a.snap.match?.scores)}`);
  assert.ok(a.typed.length >= 2, 'the bot casts');
  assert.deepEqual(a.hashes, b.hashes);
  assert.equal(JSON.stringify(a.snap), JSON.stringify(b.snap));
});
