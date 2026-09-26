import { test } from 'node:test';
import assert from 'node:assert/strict';
import { World } from '../src/sim/world.js';
import { buildScene, SCENES } from '../src/sim/levels.js';
import { NO_INPUT } from '../src/sim/player.js';
import { Director } from '../src/lang/director.js';
import { CONCEPTS, ADDONS } from '../src/lang/lexicon.js';
import { VERBS, SIZE_ADJ } from '../src/lang/words.js';

function rng(seed: number) { return () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296; }; }

test('fuzz: random, adversarial and garbage utterances never crash or corrupt the world', async () => {
  const r = rng(42);
  const pick = <T,>(a: T[]) => a[Math.floor(r() * a.length)];
  const words = [...CONCEPTS.flatMap((c) => c.names), ...ADDONS.flatMap((a) => a.names), ...Object.keys(VERBS), ...Object.keys(SIZE_ADJ), 'it', 'him', 'them', 'those', 'that', 'me', 'the', 'a', 'with', 'on', 'under', 'around', 'and', 'then', 'no', 'wait', 'bigger', 'faster', '1000000', '-5', 'NaN', 'Infinity', '💥', 'ñandú', '"; DROP TABLE', '\u0000', 'undefined', '__proto__', 'constructor', 'toString'];
  let commits = 0;
  for (let world = 0; world < 6; world++) {
    const w = await World.create();
    buildScene(w, pick([...SCENES]));
    const d = new Director(w);
    for (let i = 0; i < 60; i++) {
      const n = 1 + Math.floor(r() * 18);
      let text = Array.from({ length: n }, () => pick(words)).join(r() < 0.2 ? ', ' : ' ');
      if (r() < 0.05) text = 'x'.repeat(5000);
      if (r() < 0.05) text = '';
      if (r() < 0.03) text = Array.from({ length: 400 }, () => pick(words)).join(' ');
      await d.commit(text);
      commits++;
      for (let k = 0; k < 10; k++) w.step({ ...NO_INPUT, yaw: r() * 6, pitch: (r() - 0.5) * 2, moveZ: 1, jump: r() < 0.3, boost: r() < 0.2 });
      assert.ok(Number.isFinite(w.player.pos.x + w.player.pos.y + w.player.pos.z), 'player position finite');
      for (const c of w.constructs.list.values()) for (const inst of c.insts) assert.ok(Number.isFinite(inst.pos.x + inst.pos.y + inst.pos.z), `construct ${c.spec.label} position finite`);
      assert.ok(w.pw.bodies.len() < 600, 'body count bounded');
    }
  }
  console.log('fuzz commits', commits);
});

test('fuzz: malformed / hostile System-2 output is dropped, never executed or trusted', async () => {
  const r = rng(7);
  const pick = <T,>(a: T[]) => a[Math.floor(r() * a.length)];
  const w = await World.create();
  buildScene(w, 'drones');
  const d = new Director(w);
  const junkVals: any[] = [null, undefined, 1e308, -1e308, NaN, 'NaN', '', 'x'.repeat(10000), {}, [], [[[]]], { kind: 'self' }, 'entity:nope', 'construct:../../etc', '__proto__', { __proto__: { polluted: true } }, true, -1, 0, 3.5];
  const ops = ['create', 'modify', 'act', 'control', 'abstain', 'eval', 'require', 'script'];
  const beh = ['strike', 'grab', 'orbit', 'explode', 'launch', 'field', 'mount', 'shoot', 'hack', 'exec', 'teleport'];
  let accepted = 0;
  for (let i = 0; i < 400; i++) {
    const raw = Array.from({ length: 1 + Math.floor(r() * 4) }, () => ({
      op: pick(ops), concept: pick([...CONCEPTS.map((c) => c.id), 'nuke', '../x', pick(junkVals)]), scale: pick(junkVals), count: pick([...junkVals, 1e9, 3]),
      placement: { at: pick(['aim', 'self', 'between', 'moon', pick(junkVals)]), target: pick(junkVals) }, target: pick(junkVals), action: pick(['throw', 'dismiss', 'rm -rf', pick(junkVals)]),
      behaviors: Array.from({ length: Math.floor(r() * 4) }, () => ({ type: pick(beh), target: pick(junkVals), speed: pick(junkVals), radius: pick(junkVals), strength: pick(junkVals) })),
      changes: { scale_mul: pick(junkVals), stretch_mul: pick([[1, 2, 3], [NaN, 1, 1], 'x', null]), count: pick(junkVals), add_behaviors: [{ type: pick(beh), target: pick(junkVals) }], concept: pick(junkVals) },
      mods: pick([['scale_up'], 'x', null, [1, 2]]),
    }));
    const ctx: any = (d.interp as any).makeCtx();
    const compiled = d.interp.compileS2(raw, ctx);
    for (const o of compiled) {
      if (o.op === 'create') { const res = w.constructs.spawn(o.spec); if (res.construct) accepted++; }
    }
    for (let k = 0; k < 3; k++) w.step({ ...NO_INPUT, yaw: w.player.yaw, pitch: w.player.pitch });
    for (const c of w.constructs.list.values()) {
      assert.ok(c.spec.scale <= 14 && c.spec.count <= 12, 'validator bounds hold');
      for (const inst of c.insts) assert.ok(Number.isFinite(inst.pos.x + inst.pos.y + inst.pos.z));
    }
  }
  assert.equal(({} as any).polluted, undefined, 'no prototype pollution');
  console.log('hostile S2 batches: 400, constructs accepted after validation:', accepted);
});
