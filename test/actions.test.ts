/**
 * Adaptive construct controls (src/sim/constructs/actions.ts): LMB / RMB / wheel as sim inputs, per construct kind.
 * World-state tests with scripted inputs; everything is deterministic.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeWorld, run, look, Vector3 } from './helpers.js';
import { Director } from '../src/lang/director.js';
import { classify, AFFORDANCES } from '../src/sim/constructs/actions.js';
import type { World, GameEvent } from '../src/sim/world.js';
import type { PlayerInput } from '../src/sim/player.js';

async function setup(scene: 'flat' | 'duel' = 'flat') {
  const { w, info } = await makeWorld(scene);
  const d = new Director(w);
  const events: GameEvent[] = [];
  w.listeners.push((e) => events.push(e));
  return { w, d, info, events };
}

async function make(w: World, d: Director, text: string) {
  const r = await d.commit(text);
  assert.ok(r.applied, `"${text}" should form`);
  const c = w.constructs.get(w.constructs.focusId)!;
  assert.ok(c, `"${text}" made nothing`);
  run(w, 0.3); // settle into the hand; auto-equip happens on the next tick
  return c;
}

/** Hold a button for `sec` seconds, then release it for `after` seconds, aiming at `at` (re-aimed every tick). */
function press(w: World, btn: Partial<PlayerInput>, sec: number, after = 0.6, at?: () => Vector3) {
  const aim = () => { if (at) look(w, at()); };
  aim();
  run(w, sec, () => { aim(); return btn; });
  run(w, after, () => { aim(); return {}; });
}
const click = (w: World, at?: () => Vector3, after = 0.8) => press(w, { primary: true }, 1 / 60, after, at);

function target(w: World, p: Vector3, hp = 5000, hostile = true) {
  const e = w.addEntity('dummy', p, { hostile, hp, label: 'target' });
  w.refreshQueries();
  return e;
}

const dmg = (e: { hp: number; maxHp: number }) => e.maxHp - e.hp;

test('affordance table: every construct kind gets LMB/RMB labels; classification follows what it is', async () => {
  const { w, d } = await setup();
  const kinds: Record<string, string> = {};
  for (const t of ['a hammer', 'a hand', 'a shield', 'a tank', 'a dog', 'a boulder', 'a trampoline', 'a bomb', 'a fan', 'a sword']) {
    const c = await make(w, d, t);
    kinds[t] = classify(c, w.player);
    assert.ok(AFFORDANCES[classify(c, w.player)].lmb, t);
  }
  console.log('kinds', kinds);
  assert.equal(kinds['a hammer'], 'strike');
  assert.equal(kinds['a sword'], 'strike');
  assert.equal(kinds['a hand'], 'grab');
  assert.equal(kinds['a shield'], 'shield');
  assert.equal(kinds['a tank'], 'fire');
  assert.equal(kinds['a dog'], 'command');
  assert.equal(kinds['a boulder'], 'hurl');
  assert.equal(kinds['a trampoline'], 'launch');
  assert.equal(kinds['a bomb'], 'detonate');
  assert.equal(kinds['a fan'], 'field');
});

test('hammer: LMB strikes a target in the crosshair cone (lock-on), acting on the click tick', async () => {
  const { w, d, events } = await setup();
  const hammer = await make(w, d, 'a hammer');
  const e = target(w, new Vector3(3.5, 1.2, -18)); // ~11° off the aim line
  look(w, new Vector3(0, 1.2, -18));
  const t0 = w.tick;
  run(w, 1 / 60, () => ({ primary: true }));
  const start = events.find((x) => x.type === 'action_start' && x.construct === hammer.id);
  assert.ok(start, 'action_start on the click tick');
  assert.equal(Math.round(start.t * 60), t0, 'the swing starts on the very tick of the click');
  run(w, 1.2);
  const hit = events.find((x) => x.type === 'action_hit' && x.construct === hammer.id);
  console.log(`hammer tap: ${dmg(e).toFixed(0)} damage, hit after ${hit ? (hit.t - start.t).toFixed(2) : '-'} s`);
  assert.ok(hit && hit.target === e.id, 'the locked target is hit');
  assert.ok(dmg(e) > 10);
});

test('hammer: RMB charge (hold ~1 s, release) hits harder than a tap', async () => {
  const measure = async (charge: boolean) => {
    const { w, d, events } = await setup();
    const hammer = await make(w, d, 'a hammer');
    const e = target(w, new Vector3(0, 1.2, -16));
    const at = () => new Vector3(0, 1.2, -16);
    if (charge) press(w, { secondary: true }, 1.05, 1.2, at);
    else click(w, at, 1.2);
    const hit = events.filter((x) => x.type === 'action_hit' && x.construct === hammer.id).reduce((a, x) => a + x.energy, 0);
    const charges = events.filter((x) => x.type === 'action_charge').map((x) => x.level);
    return { total: dmg(e), hit, charges };
  };
  const tap = await measure(false);
  const charged = await measure(true);
  console.log(`tap ${tap.hit} (total ${tap.total.toFixed(0)}) vs charged ${charged.hit} (total ${charged.total.toFixed(0)}); charge events ${charged.charges}`);
  assert.deepEqual(charged.charges, [0.25, 0.5, 0.75, 1]);
  assert.ok(charged.hit > tap.hit * 1.6, 'a full charge is close to double');
  assert.ok(charged.total > tap.total);
});

test('hammer: charging while aiming down slams the ground with a shockwave', async () => {
  const { w, d, events } = await setup();
  const hammer = await make(w, d, 'a hammer');
  const a = target(w, new Vector3(2.5, 1.2, -5));
  const b = target(w, new Vector3(-2.5, 1.2, -5));
  press(w, { secondary: true }, 0.8, 1, () => new Vector3(0, 0, -5));
  assert.ok(events.some((x) => x.type === 'smash' && x.construct === hammer.id), 'shockwave');
  console.log(`slam: ${dmg(a).toFixed(0)} / ${dmg(b).toFixed(0)}`);
  assert.ok(dmg(a) > 0 && dmg(b) > 0, 'both sides of the slam are hit');
});

test('hand: LMB grabs a crate under the crosshair, LMB again throws it to the aim point', async () => {
  const { w, d, events } = await setup();
  const hand = await make(w, d, 'a hand');
  const crate = w.addEntity('crate', new Vector3(0, 0.8, -9));
  w.refreshQueries();
  click(w, () => new Vector3(0, 0.8, -9), 1.5);
  assert.equal(crate.heldBy, hand.id, 'the hand holds the crate');
  const aimPt = new Vector3(8, 0, -26);
  look(w, aimPt);
  run(w, 0.2, () => { look(w, aimPt); return {}; });
  const exact = w.player.aim(250).point.clone();
  run(w, 1 / 60, () => ({ primary: true }));
  assert.equal(crate.heldBy, null, 'thrown');
  let best = Infinity, landed: Vector3 | null = null;
  for (let i = 0; i < 180 && !landed; i++) {
    run(w, 1 / 60);
    const p = w.entityPos(crate);
    best = Math.min(best, Math.hypot(p.x - exact.x, p.z - exact.z));
    if (i > 5 && p.y < 1.0) landed = p.clone();
  }
  console.log(`thrown crate: closest ${best.toFixed(2)} m to the aim point; first touchdown ${landed ? Math.hypot(landed.x - exact.x, landed.z - exact.z).toFixed(2) : '-'} m away`);
  assert.ok(landed && Math.hypot(landed.x - exact.x, landed.z - exact.z) < 3, 'lands within a few metres of the aim point');
  assert.ok(events.some((x) => x.type === 'action_start' && x.action === 'throw'));
});

test('shield: RMB raised toward a turret blocks its shots', async () => {
  const { w, d, events } = await setup();
  const shield = await make(w, d, 'a shield');
  w.addEntity('turret', new Vector3(0, 1.2, -30));
  w.refreshQueries();
  const hp0 = w.player.hp;
  // aim up and to the side: only the raised shield (it follows the mouse) is between you and the turret
  press(w, { secondary: true }, 4, 0, () => new Vector3(0, 2.5, -30));
  const blocked = events.filter((x) => x.type === 'blocked' && x.construct === shield.id).length;
  console.log(`shield: ${blocked} shots blocked, player hp ${hp0} → ${w.player.hp}`);
  assert.ok(blocked >= 3, 'the raised shield blocks the turret');
  assert.equal(w.player.hp, hp0, 'nothing got through');
});

test('mirror: raised with RMB, reflected shots go where you aim (not just back at the shooter)', async () => {
  const { w, d } = await setup();
  const mirror = await make(w, d, 'a mirror');
  const turret = w.addEntity('turret', new Vector3(0, 1.2, -30));
  const other = target(w, new Vector3(6, 1.2, -30), 5000); // ~11° to the side of the turret
  w.refreshQueries();
  press(w, { secondary: true }, 5, 0, () => new Vector3(6, 1.2, -30));
  console.log(`mirror: reflected ${w.stats.projectilesReflected}, side target took ${dmg(other).toFixed(0)}, turret took ${dmg(turret).toFixed(0)}, player hp ${w.player.hp}`);
  assert.equal(classify(mirror, w.player), 'shield');
  assert.ok(w.stats.projectilesReflected > 0, 'shots are reflected');
  assert.ok(dmg(other) > 0, 'reflected shots go to the crosshair target');
});

test('tank (not ridden): LMB fires at the aim point', async () => {
  const { w, d, events } = await setup();
  const tank = await make(w, d, 'a tank');
  run(w, 1);
  const aimPt = new Vector3(-10, 0, -40);
  press(w, { primary: true }, 1.5, 0.5, () => aimPt);
  const fires = events.filter((x) => x.type === 'construct_fire' && x.construct === tank.id);
  console.log(`tank: ${fires.length} shots`);
  assert.ok(fires.length >= 1, 'the tank fires');
  const exact = w.player.aim(250).point;
  const f = fires[0];
  const dir = new Vector3(...(f.dir as [number, number, number]));
  const to = exact.clone().sub(new Vector3(...(f.pos as [number, number, number]))).normalize();
  console.log(`first shot direction · to aim point = ${dir.dot(to).toFixed(3)}`);
  assert.ok(dir.dot(to) > 0.97, 'fired at the crosshair');
});

test('dog: LMB sics it on the locked target', async () => {
  const { w, d } = await setup();
  const dog = await make(w, d, 'a dog');
  const e = target(w, new Vector3(4, 1.2, -25), 5000);
  const p0 = dog.insts[0].pos.distanceTo(w.entityPos(e));
  click(w, () => new Vector3(4, 1.2, -25), 0.1);
  const m = dog.insts[0].m!;
  assert.equal(m.goal, 'hunt');
  run(w, 3);
  assert.equal(m.target?.id, e.id, 'hunting the locked target');
  const p1 = dog.insts[0].pos.distanceTo(w.entityPos(e));
  console.log(`dog: ${p0.toFixed(1)} m → ${p1.toFixed(1)} m, target damage ${dmg(e).toFixed(0)}`);
  assert.ok(p1 < p0 - 5 || dmg(e) > 0);
});

test('universal fallback: a boulder (no better affordance) is hurled and hits a target 20 m away', async () => {
  const { w, d, events } = await setup();
  look(w, new Vector3(-6, 0, -6));
  const boulder = await make(w, d, 'a boulder');
  run(w, 2);
  assert.equal(classify(boulder, w.player), 'hurl');
  const e = target(w, new Vector3(3, 1.2, -20));
  click(w, () => new Vector3(3, 1.2, -20), 2.5);
  const hit = events.find((x) => x.type === 'action_hit' && x.construct === boulder.id);
  console.log(events.filter((x) => x.type.startsWith('action') || x.type === 'equip').map((x) => JSON.stringify(x)).join('\n'));
  console.log(`boulder hurl: hit ${!!hit}, damage ${dmg(e).toFixed(0)}`);
  assert.ok(hit && hit.target === e.id, 'the hurled boulder hits');
  // RMB pulls it back and holds it in front of you
  press(w, { secondary: true }, 1 / 60, 1.2);
  assert.ok(boulder.insts[0].pos.distanceTo(w.player.pos) < 8, 'pulled back to the player');
});

test('an unusual tool landing the blow is a creative solution (hurled boulder takes down a hostile)', async () => {
  const { w, d, events } = await setup();
  look(w, new Vector3(-6, 0, -6));
  const boulder = await make(w, d, 'a boulder');
  run(w, 2);
  const e = target(w, new Vector3(3, 1.2, -20), 40);
  click(w, () => new Vector3(3, 1.2, -20), 2);
  const cs = events.find((x) => x.type === 'creative_solution' && x.construct === boulder.id);
  console.log(`creative: ${cs ? cs.reason + ' +' + cs.bonus : 'none'}`);
  assert.ok(e.disabled, 'taken down');
  assert.ok(cs, 'counted as a creative solution for the boulder');
});

test('riding: LMB fires the machine you are in, RMB boosts; the equipped prompt follows the ride', async () => {
  const { w, d, events } = await setup();
  const tank = await make(w, d, 'a tank');
  assert.ok(w.player.ride(tank.id));
  assert.equal(classify(tank, w.player), 'ride');
  assert.equal(w.actions.view(w.player).equipped?.lmb, 'Fire');
  press(w, { primary: true }, 1.5, 0.2, () => new Vector3(0, 0, -60));
  const fires = events.filter((x) => x.type === 'construct_fire' && x.construct === tank.id).length;
  assert.ok(fires >= 1, 'fires while ridden');
  const p0 = tank.insts[0].pos.clone();
  run(w, 1, () => ({ moveZ: 1 }));
  const plain = tank.insts[0].pos.distanceTo(p0);
  const p1 = tank.insts[0].pos.clone();
  run(w, 1, () => ({ moveZ: 1, secondary: true }));
  const boosted = tank.insts[0].pos.distanceTo(p1);
  console.log(`riding: ${fires} shots; 1 s driving ${plain.toFixed(1)} m, with RMB boost ${boosted.toFixed(1)} m`);
  assert.ok(boosted > plain * 1.05, 'RMB boosts');
});

test('wheel cycles the equipped construct among the ones you own; naming one equips it', async () => {
  const { w, d } = await setup();
  const hammer = await make(w, d, 'a hammer');
  const hand = await make(w, d, 'a hand');
  const shield = await make(w, d, 'a shield');
  const eq = () => w.actions.equipped(w.player)?.id;
  assert.equal(eq(), shield.id, 'newest is equipped');
  run(w, 1 / 60, () => ({ cycle: -1 }));
  assert.equal(eq(), hand.id);
  run(w, 1 / 60, () => ({ cycle: -1 }));
  assert.equal(eq(), hammer.id);
  run(w, 1 / 60, () => ({ cycle: -1 }));
  assert.equal(eq(), shield.id, 'wraps around');
  run(w, 1 / 60, () => ({ cycle: 1 }));
  assert.equal(eq(), hammer.id);
  await d.commit('the hand');
  run(w, 1 / 60);
  assert.equal(eq(), hand.id, 'typing "the hand" equips it');
  w.constructs.dismiss(hand.id);
  run(w, 1 / 60);
  assert.ok(eq() === shield.id || eq() === hammer.id, 'falls back when the equipped one is gone');
  const v = w.actions.view(w.player);
  assert.equal(v.list.length, 2);
  assert.ok(v.equipped && v.equipped.lmb);
});

test('actions never hurt the owner or the owner\'s constructs', async () => {
  const { w, d } = await setup();
  const wall = await make(w, d, 'a wall');
  const hammer = await make(w, d, 'a hammer');
  const wallHp = wall.hp;
  const hp0 = w.player.hp;
  click(w, () => wall.insts[0].pos, 1);
  press(w, { secondary: true }, 1, 1, () => new Vector3(0, 0, -3)); // slam at your own feet
  press(w, { secondary: true }, 1, 1, () => wall.insts[0].pos); // charged into your own wall
  await make(w, d, 'a bomb');
  click(w, undefined, 1); // detonate in hand
  await make(w, d, 'a boulder');
  click(w, () => w.player.pos.clone().add(new Vector3(0, 30, 0)), 3); // hurl it straight up; it comes down
  console.log(`owner hp ${hp0} → ${w.player.hp}; wall ${wallHp} → ${wall.hp}`);
  assert.equal(w.player.hp, hp0);
  assert.equal(wall.hp, wallHp);
  assert.ok(w.constructs.get(hammer.id));
});

test('replay determinism: the same inputs give the same world', async () => {
  const script = async () => {
    const { w, d } = await setup();
    const e = target(w, new Vector3(2, 1.2, -15), 800);
    const crate = w.addEntity('crate', new Vector3(-3, 0.8, -8));
    w.refreshQueries();
    await make(w, d, 'a hammer');
    click(w, () => new Vector3(2, 1.2, -15));
    press(w, { secondary: true }, 0.7, 0.8, () => new Vector3(2, 1.2, -15));
    await make(w, d, 'a hand');
    click(w, () => new Vector3(-3, 0.8, -8), 1.2);
    click(w, () => new Vector3(2, 1.2, -15), 1.5);
    run(w, 0.2, () => ({ cycle: 1 }));
    click(w, () => new Vector3(2, 1.2, -15), 1);
    const r = (v: number) => v.toFixed(4);
    const parts = [r(e.hp), ...w.entityPos(e).toArray().map(r), ...w.entityPos(crate).toArray().map(r), ...w.player.pos.toArray().map(r)];
    for (const c of w.constructs.list.values()) for (const i of c.insts) parts.push(c.id, ...i.pos.toArray().map(r));
    return parts.join(',');
  };
  const a = await script();
  const b = await script();
  assert.equal(a, b);
});

test('PvP: LMB with a hammer hits the rival lantern (their hp drops, never yours)', async () => {
  const { w, d } = await setup('duel');
  const p2 = w.addPlayer('p2');
  p2.teleport(w.player.pos.clone().add(new Vector3(0, 0, -12)), new Vector3());
  w.refreshQueries();
  const c = await make(w, d, 'a hammer');
  const e0 = w.player.energy, hp0 = w.player.hp;
  click(w, () => p2.pos.clone());
  console.log('rival hp', p2.hp, '/', p2.maxHp, '| my energy', e0.toFixed(1), '->', w.player.energy.toFixed(1));
  assert.ok(p2.hp < p2.maxHp, 'the swing damaged the rival');
  assert.equal(w.player.hp, hp0, 'never hurts its owner');
  void c;
});
