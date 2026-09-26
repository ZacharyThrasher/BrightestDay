import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeWorld, run, look } from './helpers.js';
import { Director } from '../src/lang/director.js';
import { defaultSpec, type ConstructSpec } from '../src/sim/constructs/spec.js';
import { buildForm } from '../src/sim/constructs/forms.js';
import { LIMITS } from '../src/sim/constructs/validate.js';
import { signature, scoreNovelty, castMul, scaleWithinLimits, INSPIRATION, INSPIRED_AT, type Feature } from '../src/sim/inspiration.js';
import type { World, GameEvent } from '../src/sim/world.js';

const spec = (form: string, p: Partial<ConstructSpec> = {}) => defaultSpec({ concept: form, label: form, parts: buildForm(form), ...p });

/** Pure scorer driven like the runtime does: score, then record usage (with per-cast decay). */
function scorer() {
  const usage = new Map<string, number>();
  const ever = new Set<string>();
  return (feats: Feature[]) => {
    const s = scoreNovelty(feats, usage, ever);
    for (const [k, v] of usage) usage.set(k, v * INSPIRATION.castDecay);
    for (const f of feats) { usage.set(f.key, (usage.get(f.key) ?? 0) + 1); ever.add(f.key); }
    return s.novelty;
  };
}

function events(w: World, type: string) { return w.log.filter((e) => e.type === type) as GameEvent[]; }

test('signature: nouns, add-on parts, materials, behaviours, targets, verbs and their pairs', () => {
  const tank = spec('turret', { label: 'tank', behaviors: [{ type: 'shoot', target: { kind: 'nearest_enemy' }, rate: 2, speed: 50, size: 0.3 }] });
  const winged = spec('turret', { label: 'tank', parts: [...buildForm('turret'), { shape: 'box', size: [3, 0.2, 1], pos: [0, 1, 0], role: 'wing' }], behaviors: tank.behaviors });
  const a = new Set(signature(tank).map((f) => f.key));
  const b = new Set(signature(winged, { verbs: ['strike', 'create'] }).map((f) => f.key));
  assert.ok(a.has('n:tank') && a.has('b:shoot') && a.has('t:enemy'));
  assert.ok(!a.has('x:wing') && b.has('x:wing'), 'grafted wings are an add-on feature');
  assert.ok(b.has('pair:n:tank|x:wing'), 'tank + wing is a combination feature');
  assert.ok(b.has('v:strike') && !b.has('v:create'), 'plain "make/create" is not an idea');
  const heavy = new Set(signature(spec('anvil', { label: 'heavy bouncy piano', density: 5000, restitution: 0.9 })).map((f) => f.key));
  assert.ok(heavy.has('n:piano') && heavy.has('m:heavy') && heavy.has('m:bouncy'));
});

test('repetition goes stale; a new combination on a stale idea is inspiring again', () => {
  const score = scorer();
  const wall = signature(spec('wall', { label: 'wall', placement: { at: 'between', orient: 'face_away' } }));
  const n = [score(wall), score(wall), score(wall), score(wall)];
  console.log('wall ×4 novelty', n.map((x) => x.toFixed(2)).join(' '));
  assert.ok(n[0] > 0.5, 'first wall is fresh');
  assert.ok(n[1] < n[0] && n[2] < n[1] && n[3] < n[2], 'monotonically staler');
  assert.ok(n[2] < INSPIRATION.staleBelow && n[3] < INSPIRATION.staleBelow, 'third and fourth walls are stale');
  const tankSpec = spec('turret', { label: 'tank', behaviors: [{ type: 'shoot', target: { kind: 'nearest_enemy' }, rate: 2, speed: 50, size: 0.3 }] });
  const tank = signature(tankSpec);
  score(tank); score(tank);
  const stale = score(tank);
  const winged = signature({ ...tankSpec, parts: [...tankSpec.parts, { shape: 'box', size: [3, 0.2, 1], pos: [0, 1, 0], role: 'wing' }] });
  const fresh = score(winged);
  console.log(`tank ×3 → ${stale.toFixed(2)}; tank with wings → ${fresh.toFixed(2)}`);
  assert.ok(stale < INSPIRATION.staleBelow);
  assert.ok(fresh >= INSPIRED_AT, 'tank + wings counts as inspired even though tanks are stale');
});

test('power multiplier: inspired > ordinary > stale, always within bounds', () => {
  for (const lvl of [0, 0.3, 0.6, 1]) {
    const hi = castMul(lvl, 0.95), mid = castMul(lvl, 0.3), lo = castMul(lvl, 0.02);
    assert.ok(hi.mul > mid.mul && mid.mul > lo.mul);
    assert.ok(lo.stale && !hi.stale);
    for (const m of [hi, mid, lo]) assert.ok(m.mul >= INSPIRATION.mulMin && m.mul <= INSPIRATION.mulMax);
  }
});

test('strength scaling never exceeds the validator limits and does not compound across edits', () => {
  const s = spec('fist', { behaviors: [{ type: 'strike', target: { kind: 'aim' }, speed: 80, after: 'return' }, { type: 'launch', speed: 20, dir: 'up' }, { type: 'drive', mode: 'ground', speed: 10, turn: 2 } as any] });
  scaleWithinLimits(s, 1.8);
  const strike = s.behaviors[0] as any, launch = s.behaviors[1] as any, drive = s.behaviors[2] as any;
  assert.equal(strike.speed, LIMITS.strikeSpeed, 'capped at the validator limit');
  assert.ok(Math.abs(launch.speed - 36) < 1e-9);
  assert.equal(drive.speed, 10, 'a field without a known cap is never boosted');
  // an edit re-scales from the base value, not from the boosted one
  scaleWithinLimits(s, 1.0);
  assert.ok(Math.abs((s.behaviors[1] as any).speed - 20) < 1e-9);
  scaleWithinLimits(s, 0.7);
  assert.ok(Math.abs((s.behaviors[1] as any).speed - 14) < 1e-9);
  assert.ok(Math.abs((s.behaviors[2] as any).speed - 7) < 1e-9, 'stale casts are weaker even without a known cap');
});

test('casts through the Director: inspiration events, stale casts cost more and are weaker', async () => {
  const { w, info } = await makeWorld('drones');
  look(w, info.aims.drone);
  const d = new Director(w);
  const costs: number[] = [];
  const speeds: number[] = [];
  for (let i = 0; i < 4; i++) {
    const e0 = w.constructs.energy;
    const r = await d.commit('punch that drone');
    assert.equal(r.constructIds.length, 1);
    const c = w.constructs.get(r.constructIds[0])!;
    costs.push(c.cost);
    speeds.push((c.spec.behaviors.find((b) => b.type === 'strike') as any)?.speed ?? 0);
    assert.ok(Math.abs(e0 - w.constructs.energy - c.cost) < 1e-6, 'energy charged = the (economy-adjusted) construct cost');
    run(w, 1.5);
    d.interp.lastEntityId = null;
    w.constructs.dismissAll();
    look(w, info.aims.drone);
  }
  const ev = events(w, 'inspiration');
  console.log('glove ×4', ev.map((e) => `nov ${e.novelty} mul ${e.mul}${e.stale ? ' STALE' : ''}`).join(' | '), 'cost', costs.map((c) => c.toFixed(1)).join(' '), 'speed', speeds.map((s) => s.toFixed(0)).join(' '));
  assert.equal(ev.length, 4);
  for (const e of ev) { assert.equal(typeof e.novelty, 'number'); assert.equal(typeof e.mul, 'number'); assert.ok(Array.isArray(e.reasons)); }
  assert.ok(!ev[0].stale && ev[3].stale, 'first is fresh, fourth is stale');
  assert.ok(ev[3].reasons[0].includes('in a row'), 'the player is told why');
  assert.ok(costs[3] > costs[0] * 1.3, 'stale costs more');
  assert.ok(speeds[3] < speeds[0], 'stale is weaker');
  for (const s of speeds) assert.ok(s <= LIMITS.strikeSpeed);
});

test('runtime calls outside a cast are neutral (no scaling, validator cost)', async () => {
  const { w } = await makeWorld('flat');
  const r = w.constructs.spawn(spec('fist', { behaviors: [{ type: 'strike', target: { kind: 'aim' }, speed: 40, after: 'return' }] }));
  assert.ok(r.construct);
  assert.equal((r.construct!.spec.behaviors[0] as any).speed, 40);
  assert.equal(events(w, 'inspiration').length, 0);
});

test('creative solutions: a drone downed by a piano pays more than by a gun, and less each repeat', async () => {
  const award = async (form: string, label: string, behaviors: ConstructSpec['behaviors']) => {
    const { w } = await makeWorld('drones');
    const drone = [...w.entities.values()].find((e) => e.type === 'drone')!;
    const r = w.constructs.spawn(spec(form, { label, behaviors }));
    const lvl0 = w.inspiration.level;
    w.emit('impact', { construct: r.construct!.id, entity: drone.id, dv: 30 });
    w.damageEntity(drone, 500, 'strike');
    w.step();
    const gain = w.inspiration.level - lvl0 + (w.dt / INSPIRATION.restTau) * (lvl0 - INSPIRATION.rest);
    // the same trick on the next drone is worth less
    const drone2 = [...w.entities.values()].find((e) => e.type === 'drone' && !e.disabled)!;
    w.emit('impact', { construct: r.construct!.id, entity: drone2.id, dv: 30 });
    w.damageEntity(drone2, 500, 'strike');
    w.step();
    const [ev, again] = events(w, 'creative_solution');
    return { ev, again, gain };
  };
  const piano = await award('anvil', 'piano', []);
  const gun = await award('cannon', 'cannon', [{ type: 'shoot', target: { kind: 'nearest_enemy' }, rate: 2, speed: 50, size: 0.3 }]);
  console.log('piano', piano.ev?.reason, piano.ev?.bonus, 'then', piano.again?.bonus, '| gun', gun.ev?.reason, gun.ev?.bonus);
  assert.ok(piano.ev && gun.ev && piano.again);
  assert.equal(piano.ev.job, 'defeat');
  assert.ok(piano.ev.bonus > gun.ev.bonus * 2, 'unusual tool for the job pays more');
  assert.ok(piano.gain > gun.gain);
  assert.ok(piano.again.bonus < piano.ev.bonus, 'repeating the same solution pays less');
});

test('real play: dropping a piano on a drone is a creative solution', async () => {
  const { w, info } = await makeWorld('drones');
  look(w, info.aims.drone);
  const d = new Director(w);
  await d.commit('drop a piano on that drone');
  run(w, 4);
  const ev = events(w, 'creative_solution');
  console.log(ev.map((e) => e.reason));
  assert.ok(ev.some((e) => e.job === 'defeat' && e.unusual === 1));
});

test('duel: each player has their own inspiration (one player spamming does not make the other stale)', async () => {
  const { w, info } = await makeWorld('arena');
  look(w, info.aims.ground_ahead ?? info.aims[Object.keys(info.aims)[0]]);
  const a = new Director(w); a.playerId = 'p1';
  const b = new Director(w); b.playerId = 'p2';
  for (let i = 0; i < 4; i++) { await a.commit('a wall'); run(w, 0.3); }
  await b.commit('a wall');
  const ev = events(w, 'inspiration');
  const p1 = ev.filter((e) => e.player === 'p1'), p2 = ev.filter((e) => e.player === 'p2');
  console.log('p1', p1.map((e) => e.novelty).join(' '), '| p2', p2.map((e) => e.novelty).join(' '), '| levels', w.inspiration.of('p1').level.toFixed(2), w.inspiration.of('p2').level.toFixed(2));
  assert.equal(p1.length, 4);
  assert.equal(p2.length, 1);
  assert.ok(p1[3].stale && !p2[0].stale && p2[0].novelty > 0.5);
  assert.ok(w.inspiration.of('p2').level > w.inspiration.of('p1').level);
});

test('duel: creative solutions reward hurting / taking down the rival, breaking and blocking their constructs', async () => {
  const { w } = await makeWorld('arena');
  const E = w.inspiration;
  const castAs = (player: string, form: string, label: string, behaviors: ConstructSpec['behaviors'] = []) => {
    E.beginCast([], player);
    const r = w.constructs.spawn(spec(form, { label, behaviors }));
    E.endCast();
    return r.construct!.id;
  };
  const piano = castAs('p1', 'anvil', 'piano');
  const cannon = castAs('p2', 'cannon', 'cannon', [{ type: 'shoot', target: { kind: 'nearest_enemy' }, rate: 2, speed: 50, size: 0.3 }]);
  const wall = castAs('p2', 'wall', 'wall');
  const whale = castAs('p1', 'block', 'whale');
  // p1's piano takes p2 down (explicit attribution); p2's cannon shot hurts p1 (attributed by cause)
  w.emit('player_damage', { target: 'p2', amount: 40, cause: 'strike', by: piano });
  w.emit('player_killed', { target: 'p2', cause: 'strike', by: piano });
  w.emit('construct_fire', { construct: cannon });
  w.emit('player_damage', { target: 'p1', amount: 10, cause: 'projectile' });
  // p1's flying whale smashes p2's wall and blocks a cannon shot (once)
  w.emit('construct_destroyed', { id: wall, by: whale });
  w.emit('blocked', { construct: whale, reflected: false, from: cannon });
  w.emit('blocked', { construct: whale, reflected: false, from: cannon });
  // hurting yourself is not a solution
  w.emit('player_damage', { target: 'p1', amount: 5, cause: 'strike', by: whale });
  w.step();
  const ev = events(w, 'creative_solution');
  console.log(ev.map((e) => `${e.player}: ${e.job} "${e.reason}" +${e.bonus}`).join(' | '));
  const by = (p: string, job: string) => ev.filter((e) => e.player === p && e.job === job);
  assert.equal(by('p1', 'kill').length, 1);
  assert.equal(by('p1', 'hit').length, 1);
  assert.equal(by('p2', 'hit').length, 1, 'shot attributed to the shooter, not the victim');
  assert.ok(by('p1', 'kill')[0].bonus > by('p2', 'hit')[0].bonus);
  assert.ok(by('p2', 'hit')[0].typical && !by('p1', 'hit')[0].typical, 'a cannon is the usual tool, a piano is not');
  assert.equal(by('p1', 'counter').length, 1);
  assert.equal(by('p1', 'block').length, 1, 'block rewarded once per construct');
  assert.ok(ev.every((e) => e.player !== e.against), 'never rewarded for hurting yourself');
  assert.ok(E.of('p1').level > E.of('p2').level);
});

test('deterministic: the same casts in the same scene give identical economy events', async () => {
  const once = async () => {
    const { w, info } = await makeWorld('drones');
    look(w, info.aims.drone);
    const d = new Director(w);
    for (const t of ['a wall', 'a wall', 'three hammers orbiting me', 'a tank with wings', 'a wall']) { await d.commit(t); run(w, 0.5); }
    return JSON.stringify(events(w, 'inspiration').map(({ t, ...e }) => e));
  };
  assert.equal(await once(), await once());
});
