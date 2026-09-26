/**
 * Language layer → machine specs/ops (orders, articulated add-ons, traits, riding, typed shorthand, PvP refs).
 * These check the PROPOSED ops/specs against the contract in src/sim/constructs/spec.ts; world effects are
 * the runtime's job and are tested there.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeWorld, look, Vector3 } from './helpers.js';
import { Director } from '../src/lang/director.js';
import type { ConstructSpec, Behavior } from '../src/sim/constructs/spec.js';
import type { Op } from '../src/lang/types.js';
import type { SceneName } from '../src/sim/levels.js';
import { functionsOf } from '../src/lang/interpreter.js';
import { matchOrder } from '../src/lang/orders.js';
import { expandShorthand } from '../src/lang/parser.js';

async function setup(scene: SceneName = 'drones') {
  const { w, info } = await makeWorld(scene);
  look(w, info.aims.drone ?? info.aims.robot ?? info.aims.ground_ahead);
  const d = new Director(w);
  return { w, info, d };
}
const specOf = (ops: Op[], i = 0): ConstructSpec => {
  const o = ops.filter((x) => x.op === 'create' || x.op === 'modify')[i] as any;
  assert.ok(o, `expected a create/modify op, got ${JSON.stringify(ops.map((x) => x.op))}`);
  return o.spec;
};
const beh = <T extends Behavior['type']>(s: ConstructSpec, t: T) => s.behaviors.filter((b) => b.type === t) as Extract<Behavior, { type: T }>[];
const motions = (s: ConstructSpec) => (s.joints ?? []).flatMap((j) => j.motion.map((m) => m.kind));
const groupsHaveParts = (s: ConstructSpec) => (s.joints ?? []).every((j) => s.parts.some((p) => p.group === j.group));

test('"a car w/ a cannon": rolling wheels, ground drive, cannon on an aiming joint with a muzzle', async () => {
  const { d } = await setup('arena');
  const r = await d.commit('a car w/ a cannon');
  const s = specOf(r.ops);
  assert.equal(s.concept, 'car');
  assert.ok(motions(s).filter((m) => m === 'roll').length >= 3, 'wheels roll'); // catalog car: 2 steering front wheels + a shared rear axle
  assert.equal(beh(s, 'drive')[0]?.mode, 'ground');
  const w = beh(s, 'weapon');
  assert.equal(w[0]?.kind, 'cannon');
  const aim = (s.joints ?? []).find((j) => j.group === w[0].group);
  assert.ok(aim?.motion.some((m) => m.kind === 'aim'), 'cannon group aims');
  assert.ok(s.parts.some((p) => p.role === 'muzzle' && p.group === w[0].group), 'muzzle part in the weapon group');
  assert.ok(groupsHaveParts(s));
  const fns = functionsOf(s, d.world);
  for (const f of ['wheels_spin', 'drives_on_ground', 'shoots', 'aims_at_target', 'has_moving_parts', 'rideable']) assert.ok(fns.includes(f), f);
});

test('"tank, attack that drone" after a tank → brain hunt on that drone', async () => {
  const { w, d } = await setup('drones');
  await d.commit('make a tank');
  const tank = [...w.constructs.list.values()].pop()!;
  const r = await d.commit('tank, attack that drone');
  const m = r.ops.find((o) => o.op === 'modify') as Extract<Op, { op: 'modify' }>;
  assert.equal(m?.target, tank.id);
  const brain = beh(m.spec, 'brain')[0];
  assert.equal(brain.goal, 'hunt');
  assert.equal(brain.target?.kind, 'entity');
  const aimed = [...w.entities.values()].find((e) => brain.target?.kind === 'entity' && e.id === (brain.target as any).id);
  assert.equal(aimed?.type, 'drone');
  assert.ok(beh(m.spec, 'weapon').length, 'tank keeps its cannon');
  assert.ok(!r.ops.some((o) => o.op === 'create'), 'no new construct');
});

test('"tank, attack that drone" with no tank yet → makes a tank that hunts it', async () => {
  const { d } = await setup('drones');
  const r = await d.commit('tank, attack that drone');
  const s = specOf(r.ops);
  assert.equal(s.concept, 'tank');
  assert.equal(beh(s, 'brain')[0]?.goal, 'hunt');
});

test('"get in" / "let me drive" → ride op on the vehicle; "get out" → dismount', async () => {
  const { w, d } = await setup('arena');
  await d.commit('a couch on wheels');
  const couch = [...w.constructs.list.values()].pop()!;
  for (const u of ['get in', 'let me drive', 'hop on', "i'll drive", 'ride it']) {
    const r = await d.commit(u);
    const ride = r.ops.find((o) => o.op === 'ride') as Extract<Op, { op: 'ride' }>;
    assert.ok(ride, `${u} → ride`);
    assert.deepEqual(ride.target, { kind: 'construct', id: couch.id });
  }
  const out = await d.commit('get out');
  assert.deepEqual(out.ops, [{ op: 'dismount' }]);
});

test('"get in" with nothing to ride abstains visibly', async () => {
  const { d } = await setup('arena');
  const r = await d.commit('get in');
  assert.ok(r.abstain && /ride/.test(r.abstain));
});

test('pets: "a dog that follows me" walks and follows; "the dog, sit" idles; "heel" follows', async () => {
  const { w, d } = await setup('arena');
  const r = await d.commit('a dog that follows me');
  const s = specOf(r.ops);
  assert.equal(beh(s, 'brain')[0]?.goal, 'follow');
  assert.equal(beh(s, 'brain')[0]?.target?.kind, 'self');
  assert.equal(beh(s, 'drive')[0]?.mode, 'ground');
  assert.ok((s.joints ?? []).some((j) => j.motion.some((m) => m.kind === 'swing' && m.gait)), 'legs walk');
  assert.ok(!s.behaviors.some((b) => b.type === 'hold'), 'not held in the hand');
  assert.ok(functionsOf(s, w).includes('walks') && functionsOf(s, w).includes('follows_player'));
  const sit = await d.commit('the dog, sit');
  assert.equal(beh(specOf(sit.ops), 'brain')[0]?.goal, 'idle');
  const heel = await d.commit('heel');
  assert.equal(beh(specOf(heel.ops), 'brain')[0]?.goal, 'follow');
  const back = await d.commit('come back');
  assert.ok(d.interp.lastEffect!.functions.includes('returns'));
  void back;
});

test('mash-ups on any base: flying bathtub with cannons, shark with helicopter blades, walking house', async () => {
  const { w, d } = await setup('sandbox');
  const tub = specOf((await d.commit('a flying bathtub with cannons')).ops);
  assert.equal(beh(tub, 'drive')[0]?.mode, 'air');
  assert.equal(beh(tub, 'weapon').filter((x) => x.kind === 'cannon').length, 2, 'plural cannons → two guns');
  assert.ok(groupsHaveParts(tub));
  const shark = specOf((await d.commit('a shark with helicopter blades that hunts drones')).ops);
  assert.ok(motions(shark).includes('spin'), 'rotor spins');
  assert.equal(beh(shark, 'drive')[0]?.mode, 'air');
  assert.equal(beh(shark, 'brain')[0]?.goal, 'hunt');
  assert.ok(beh(shark, 'weapon').some((x) => x.kind === 'melee'), 'it bites');
  const house = specOf((await d.commit('a walking house')).ops);
  assert.ok((house.joints ?? []).filter((j) => j.motion.some((m) => m.kind === 'swing' && m.gait)).length >= 2, 'walking legs');
  assert.equal(beh(house, 'drive')[0]?.mode, 'ground');
  assert.ok(functionsOf(house, w).includes('walks'));
});

test('unknown nouns: machine features get a generic hull and say so; embedding fallback says "closest I know"', async () => {
  const { d } = await setup('arena');
  const r = await d.commit('a zamboni with legs');
  const s = specOf(r.ops);
  assert.ok(s.parts.some((p) => p.role === 'leg'));
  assert.ok(r.messages.some((m) => /zamboni/.test(m)), r.messages.join(' | '));
  d.interp.embedder = { nearest: (t: string) => (t === 'narwhal' ? [{ id: 'shark', score: 0.7 }] : []) };
  const n = await d.commit('a narwhal');
  assert.equal(specOf(n.ops).concept, 'shark');
  assert.ok(n.messages.some((m) => /closest I know/.test(m)));
});

test('verbs and adjectives imply machines', async () => {
  const { w, d } = await setup('sandbox');
  const heli = specOf((await d.commit('an attack helicopter')).ops);
  assert.ok(beh(heli, 'weapon').length && beh(heli, 'brain')[0]?.goal === 'hunt');
  assert.ok(motions(heli).includes('spin'));
  const guard = specOf((await d.commit('a guard robot')).ops);
  assert.equal(beh(guard, 'brain')[0]?.goal, 'guard');
  const race = specOf((await d.commit('a racing car')).ops);
  assert.ok(beh(race, 'drive')[0].speed > 20);
  const dragon = specOf((await d.commit('a fire-breathing dragon')).ops);
  assert.ok(beh(dragon, 'weapon').some((x) => x.kind === 'flame'));
  assert.ok(motions(dragon).includes('swing'), 'wings flap');
  const car = specOf((await d.commit('a flying car')).ops);
  assert.equal(beh(car, 'drive')[0]?.mode, 'air');
  const army = specOf((await d.commit('an army of 5 robots')).ops);
  assert.equal(army.count, 5);
  assert.equal(beh(army, 'brain')[0]?.goal, 'hunt');
  assert.ok(functionsOf(army, w).includes('multiple_agents'));
  const bees = specOf((await d.commit('a swarm of bees that chase the drones')).ops);
  assert.ok(bees.count >= 5);
  assert.equal(beh(bees, 'brain')[0]?.goal, 'hunt');
  assert.equal(beh(bees, 'drive')[0]?.mode, 'air');
  const tank = specOf((await d.commit('a tank that hunts the robots')).ops);
  const t = beh(tank, 'brain')[0];
  assert.equal(t.goal, 'hunt');
  assert.ok(t.target && ['entity', 'entities'].includes(t.target.kind));
  const ids = t.target!.kind === 'entity' ? [t.target!.id] : (t.target as any).ids;
  assert.ok(ids.every((id: string) => w.entities.get(id)?.type === 'robot'), 'targets the robot entities');
});

test('plurals, ordinals and "all of you" route orders to the right constructs', async () => {
  const { w, d } = await setup('sandbox');
  await d.commit('2 dogs');
  await d.commit('a tank');
  const all = await d.commit('all of you, follow me');
  assert.equal(all.ops.filter((o) => o.op === 'modify').length, 2);
  const second = await d.commit('the second dog, fetch that crate');
  const m = second.ops.find((o) => o.op === 'modify') as any;
  assert.equal(beh(m.spec, 'brain')[0].goal, 'fetch');
  const dogs = [...w.constructs.list.values()].filter((c) => c.spec.concept === 'dog');
  assert.equal(dogs.length, 2, 'the second dog was split out of the pair');
  const tanks = await d.commit('tanks attack');
  const tm = tanks.ops.find((o) => o.op === 'modify') as any;
  assert.equal(w.constructs.get(tm.target)?.spec.concept, 'tank');
  assert.equal(beh(tm.spec, 'brain')[0].goal, 'hunt');
});

test('orders: guard / go there / patrol / run away / stop / aim / fire', async () => {
  const { d } = await setup('sandbox');
  await d.commit('a robot with a laser');
  const goal = async (u: string) => beh(specOf((await d.commit(u)).ops), 'brain')[0];
  assert.equal((await goal('guard this spot')).goal, 'guard');
  assert.equal((await goal('protect me')).target?.kind, 'self');
  assert.equal((await goal('go there')).goal, 'goto');
  assert.equal((await goal('patrol')).goal, 'patrol');
  assert.equal((await goal('run away')).goal, 'flee');
  assert.equal((await goal('stay')).goal, 'idle');
  const aim = await goal('aim at that drone');
  assert.equal(aim.target?.kind, 'entity');
  assert.equal((await goal('open fire')).goal, 'hunt');
  assert.equal((await goal('shoooot')).goal, 'hunt');
});

test('orders do not steal ring actions when nothing autonomous is in focus', async () => {
  const { d } = await setup('drones');
  await d.commit('make a wall');
  const r = await d.commit('attack that drone');
  assert.ok(r.ops.some((o) => o.op === 'create' && o.spec.behaviors.some((b) => b.type === 'strike')), 'the ring punches as before');
});

test('typed shorthand and typos', async () => {
  assert.equal(expandShorthand('a car w/ 2 cannons plz'), 'a car with 2 cannons please');
  assert.equal(expandShorthand('tank+missiles'), 'tank with missiles');
  assert.equal(expandShorthand('dog & cat'), 'dog and cat');
  assert.equal(expandShorthand('tanks x3'), '3 tanks');
  assert.equal(expandShorthand('shoooot it'), 'shoot it');
  assert.equal(expandShorthand('u follow ur dog thx'), 'u follow your dog');
  const { d } = await setup('arena');
  const h = specOf((await d.commit('a helicoptor w/ missles')).ops);
  assert.equal(h.concept, 'helicopter');
  assert.ok(beh(h, 'weapon').some((x) => x.kind === 'missile'));
  const t = specOf((await d.commit('a tnak')).ops);
  assert.equal(t.concept, 'tank');
  const c = specOf((await d.commit('2x dogs w legs')).ops);
  assert.equal(c.count, 2);
});

test('order matcher basics', () => {
  assert.equal(matchOrder(['follow', 'me'])?.kind, 'brain');
  assert.equal((matchOrder(['get', 'in', 'the', 'tank']) as any)?.target?.join(' '), 'the tank');
  assert.equal(matchOrder(['get', 'out'])?.kind, 'dismount');
  assert.equal(matchOrder(['get', 'me', 'a', 'car']), null, '"get me a car" is a creation');
  assert.equal(matchOrder(['make', 'a', 'wall']), null);
  assert.equal((matchOrder(['bring', 'me', 'that', 'crate']) as any)?.goal, 'fetch');
  assert.equal((matchOrder(['attack', 'that', 'drone']) as any)?.target?.join(' '), 'that drone');
});

test('features that cannot be built are reported, not dropped', async () => {
  const { d } = await setup('arena');
  const r = await d.commit('a dog that sings');
  assert.ok(r.messages.some((m) => /sings|sing/.test(m)), r.messages.join(' | '));
  const r2 = await d.commit('a car with a quantum flangulator');
  assert.ok(r2.messages.some((m) => /flangulator/.test(m)), r2.messages.join(' | '));
});

test('PvP: "him"/"the enemy" target the opponent lantern; "his wall" targets their construct; steal his car', async () => {
  const { w, d } = await setup('arena');
  (w as any).opponent = { id: 'p2', pos: new Vector3(0, 1, -25) };
  const smash = await d.commit('smash him');
  const sm = specOf(smash.ops);
  assert.deepEqual(beh(sm, 'strike')[0]?.target, { kind: 'player', id: 'p2' });
  const hunt = specOf((await d.commit('a tank that hunts the enemy')).ops);
  assert.deepEqual(beh(hunt, 'brain')[0]?.target, { kind: 'player', id: 'p2' });
  const trap = specOf((await d.commit('cage my rival')).ops);
  assert.deepEqual(beh(trap, 'restrain')[0]?.target, { kind: 'player', id: 'p2' });
  // an opponent-owned wall and car
  const wall = w.constructs.spawn({ ...specOf((await d.commit('a wall')).ops) }).construct!;
  (wall as any).owner = 'p2';
  const car = w.constructs.spawn({ ...specOf((await d.commit('a car')).ops) }).construct!;
  (car as any).owner = 'p2';
  const hit = specOf((await d.commit('smash his wall')).ops);
  assert.deepEqual(beh(hit, 'strike')[0]?.target, { kind: 'construct', id: wall.id });
  const steal = await d.commit('steal his car');
  assert.deepEqual((steal.ops.find((o) => o.op === 'ride') as any)?.target, { kind: 'construct', id: car.id });
  const reshape = await d.commit('make his wall smaller');
  assert.ok(reshape.abstain, 'their constructs are not ours to reshape');
});

test('free composition: part lists, counts, held weapons, materials, giving parts, "make it fly"', async () => {
  const { w, d } = await setup('sandbox');
  const bus = specOf((await d.commit('a bus with 4 legs, a tail and a machine gun on top')).ops);
  assert.ok(bus.parts.filter((p) => p.role === 'leg').length >= 4 && bus.parts.some((p) => p.role === 'tail') && beh(bus, 'weapon').some((x) => x.kind === 'gun'));
  const ship = specOf((await d.commit('a pirate ship with wheels and 6 cannons')).ops);
  assert.equal(beh(ship, 'weapon').length, 6);
  assert.equal(beh(ship, 'drive')[0].mode, 'ground');
  const robot = specOf((await d.commit('a giant robot with 4 arms')).ops);
  assert.equal(robot.parts.filter((p) => p.role === 'arm').length, 4);
  const hammer = specOf((await d.commit('a hammer with legs that chases the robots')).ops);
  assert.ok(beh(hammer, 'weapon').some((x) => x.kind === 'melee'), 'a walking hammer hits with itself');
  const bear = specOf((await d.commit('a bear wielding a chainsaw')).ops);
  assert.ok(motions(bear).includes('spin') && beh(bear, 'weapon').some((x) => x.kind === 'melee'));
  const jelly = specOf((await d.commit('a huge robot made of jelly')).ops);
  assert.ok(jelly.restitution > 0.8, 'jelly is bouncy');
  const heads = specOf((await d.commit('a dragon with 3 heads')).ops);
  assert.equal(heads.parts.filter((p) => p.role === 'head').length, 3);
  // machines cost real willpower now: clear the board so "a dog" isn't refused for lack of energy
  for (const id of [...w.constructs.list.keys()]) w.constructs.dismiss(id);
  w.constructs.energy = w.constructs.energyMax;
  await d.commit('a dog');
  const dog = [...w.constructs.list.values()].pop()!;
  const give = await d.commit('give the dog wings');
  const g = give.ops.find((o) => o.op === 'modify') as any;
  assert.equal(g?.target, dog.id);
  assert.equal(beh(g.spec, 'drive')[0].mode, 'air');
  await d.commit('a couch');
  const fly = specOf((await d.commit('make it fly')).ops);
  assert.equal(fly.concept, 'pillow');
  assert.equal(beh(fly, 'drive')[0].mode, 'air');
});
