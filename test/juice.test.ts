import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JUICE } from '../src/app/juice/tuning.js';
import {
  Trauma, Spring, Hitstop, SpeedThresholds, mapEvent, mapPlayerEvent, killCamWeight, falloff, impulseForPeak, energy,
  type JuiceContext, type V3,
} from '../src/app/juice/core.js';
import { VoicePool } from '../src/audio/voices.js';
import { Sfx, SOUND_NAMES } from '../src/audio/sfx.js';

// ------------------------------------------------------------------------------------------ trauma

test('trauma: shake is trauma², saturates at 1, decays linearly', () => {
  const t = new Trauma();
  t.add(0.3);
  assert.ok(Math.abs(t.shake - 0.09) < 1e-9, 'small hit → tiny shake');
  t.add(0.5);
  assert.ok(Math.abs(t.shake - 0.64) < 1e-9);
  t.add(5);
  assert.equal(t.value, 1, 'stacking saturates');
  t.add(-1); t.add(NaN);
  assert.equal(t.value, 1, 'negative / NaN ignored');
  let s = 0;
  while (t.value > 0) { t.update(1 / 60); s += 1 / 60; }
  console.log(`max trauma settles in ${s.toFixed(2)} s`);
  assert.ok(Math.abs(s - 1 / JUICE.trauma.decay) < 0.05, 'linear decay: 1/decay seconds from max');
  assert.ok(s < 1, 'a kill never shakes for more than a second');
});

test('springs: impulseForPeak hits the requested peak and settles', () => {
  for (const [k, c] of [[JUICE.kick.stiffness, JUICE.kick.damping], [JUICE.fov.springK, JUICE.fov.springC]]) {
    const s = new Spring(k, c);
    s.impulse(impulseForPeak(0.3, k, c));
    let peak = 0, t = 0;
    while (!s.settled && t < 3) { s.update(1 / 240); t += 1 / 240; peak = Math.max(peak, s.x); }
    assert.ok(Math.abs(peak - 0.3) < 0.01, `peak ${peak.toFixed(3)} ≈ 0.3 (k=${k}, c=${c})`);
    assert.ok(t < 1.2, `settles in ${t.toFixed(2)} s`);
  }
  // large frame time stays stable
  const s = new Spring(JUICE.kick.stiffness, JUICE.kick.damping);
  s.impulse(5);
  for (let i = 0; i < 20; i++) s.update(0.25);
  assert.ok(Number.isFinite(s.x) && Math.abs(s.x) < 0.01);
});

// ------------------------------------------------------------------------------------------ hitstop

const run = (h: Hitstop, ms: number, step = 1000 / 60) => { const w: number[] = []; for (let t = 0; t < ms; t += step) { h.update(step); w.push(h.weight('x')); } return w; };

test('hitstop: clamps to [min, max], then catches up smoothly', () => {
  const h = new Hitstop();
  assert.equal(h.trigger('x', 5), JUICE.hitstop.minMs, 'too short → min');
  h.clear(); h.now = 10000;
  assert.equal(h.trigger('x', 999), JUICE.hitstop.maxMs, 'too long → max');
  assert.equal(h.weight('x'), 1);
  const w = run(h, JUICE.hitstop.maxMs + JUICE.hitstop.catchUpMs + 50, 1);
  const holdEnd = w.findIndex((x) => x < 1);
  assert.ok(Math.abs(holdEnd - JUICE.hitstop.maxMs) <= 1, `held ${holdEnd} ms`);
  for (let i = holdEnd + 1; i < w.length; i++) assert.ok(w[i] <= w[i - 1] + 1e-9, 'catch-up is monotone');
  assert.equal(w[w.length - 1], 0, 'fully caught up');
  assert.ok(w.findIndex((x) => x === 0) <= JUICE.hitstop.maxMs + JUICE.hitstop.catchUpMs + 1);
  assert.equal(h.weight('other'), 0);
});

test('hitstop: energy scales the hold; continuous fire never exceeds the chain cap', () => {
  const h = new Hitstop();
  assert.ok(h.msFor(0.1) < h.msFor(0.9));
  assert.equal(h.msFor(0), JUICE.hitstop.minMs);
  assert.equal(h.msFor(5), JUICE.hitstop.maxMs);
  // a machine gun: a hit every 50 ms for 2 s
  let frozenMs = 0, longest = 0, cur = 0;
  for (let t = 0; t < 2000; t++) {
    if (t % 50 === 0) h.trigger('x', 80);
    h.update(1);
    if (h.holding('x')) { frozenMs++; cur++; longest = Math.max(longest, cur); } else cur = 0;
  }
  console.log(`2 s of continuous hits: held ${frozenMs} ms total, longest freeze ${longest} ms, refused ${h.refused}`);
  assert.ok(longest <= JUICE.hitstop.chainCapMs, 'a chain never freezes longer than the cap');
  assert.ok(frozenMs < 1000, 'the target is visibly moving most of the time');
  assert.ok(h.refused > 0, 'immunity kicks in after a capped chain');
});

test('hitstop: whole-view freezes respect the per-second budget', () => {
  const h = new Hitstop();
  let total = 0;
  for (let i = 0; i < 20; i++) { total += h.trigger('*', 120); h.update(30); }
  assert.ok(total <= JUICE.hitstop.globalBudgetMs, `whole-view held ${total} ms in 600 ms`);
});

test('kill cam envelope eases in, holds and releases', () => {
  const D = JUICE.camera.killCam.dur;
  assert.equal(killCamWeight(-0.1), 0);
  assert.ok(killCamWeight(0.01) < 0.2);
  assert.equal(killCamWeight(D * 0.4), 1);
  assert.ok(killCamWeight(D * 0.95) < 0.2);
  assert.equal(killCamWeight(D), 0);
});

test('speed thresholds punch once per crossing (with hysteresis)', () => {
  const s = new SpeedThresholds([30, 45], 4);
  assert.equal(s.update(20), -1);
  assert.equal(s.update(31), 0);
  assert.equal(s.update(33), -1, 'no repeat while above');
  assert.equal(s.update(28), -1, 'inside hysteresis: still disarmed');
  assert.equal(s.update(31), -1);
  assert.equal(s.update(20), -1);
  assert.equal(s.update(50), 1, 'jumping past both reports the highest');
});

// ------------------------------------------------------------------------------------------ event → effect

function ctx(over: Partial<JuiceContext> = {}): JuiceContext {
  const owners: Record<string, string> = { mine: 'p1', rival: 'p2' };
  return {
    local: 'p1',
    ownerOf: (id) => owners[id] ?? null,
    distance: (p) => (p ? Math.hypot(p[0], p[1], p[2]) : Infinity),
    posOf: (k) => (k.startsWith('e:') ? [0, 1, 10] : k.startsWith('c:') ? [0, 1, 5] : k === 'p:p2' ? [0, 1, 20] : null),
    playerPos: (): V3 => [0, 1, 0],
    keyOf: (id) => (id.startsWith('e') ? `e:${id}` : id.startsWith('p') ? `p:${id}` : `c:${id}`),
    ...over,
  };
}

test('event mapping: a 40-damage hit shakes more than a 5-damage hit', () => {
  const c = ctx();
  const big = mapEvent({ type: 'player_damage', target: 'p1', amount: 40, cause: 'strike', by: 'rival' }, c)!;
  const small = mapEvent({ type: 'player_damage', target: 'p1', amount: 5, cause: 'strike', by: 'rival' }, c)!;
  assert.ok(big.trauma! > small.trauma!, `trauma ${big.trauma} > ${small.trauma}`);
  assert.ok(big.trauma! ** 2 > 2 * small.trauma! ** 2, 'and the (trauma²) shake difference is pronounced');
  assert.ok((big.kick?.mag ?? 0) > (small.kick?.mag ?? 0));
  assert.equal(big.sound?.name, 'hurt');
  // impacts from my construct: bigger dv → more shake and a longer hitstop on both objects
  const hi = mapEvent({ type: 'impact', construct: 'mine', entity: 'e1', dv: 40, pos: [0, 0, 6] }, c)!;
  const lo = mapEvent({ type: 'impact', construct: 'mine', entity: 'e1', dv: 5, pos: [0, 0, 6] }, c)!;
  assert.ok(hi.trauma! > lo.trauma!);
  assert.ok(hi.hitstop!.ms > lo.hitstop!.ms);
  assert.deepEqual(hi.hitstop!.keys, ['c:mine', 'e:e1']);
  assert.ok(hi.hitstop!.ms <= JUICE.hitstop.maxMs && lo.hitstop!.ms >= JUICE.hitstop.minMs);
});

test('event mapping: distance falloff and ownership', () => {
  const c = ctx();
  const near = mapEvent({ type: 'explosion', pos: [0, 0, 5], radius: 6 }, c)!;
  const far = mapEvent({ type: 'explosion', pos: [0, 0, 60], radius: 6 }, c)!;
  const gone = mapEvent({ type: 'explosion', pos: [0, 0, 500], radius: 6 }, c)!;
  assert.ok(near.trauma! > far.trauma! && far.trauma! > 0 && gone.trauma === 0);
  assert.equal(falloff(0), 1); assert.equal(falloff(1e9), 0);
  // the rival's construct hitting something: no hitstop for me, weaker shake
  const theirs = mapEvent({ type: 'impact', construct: 'rival', entity: 'e1', dv: 30, pos: [0, 0, 6] }, c)!;
  const mine = mapEvent({ type: 'impact', construct: 'mine', entity: 'e1', dv: 30, pos: [0, 0, 6] }, c)!;
  assert.equal(theirs.hitstop, undefined);
  assert.ok(mine.trauma! > theirs.trauma!);
  // damage numbers / hit markers only for damage I dealt
  assert.equal(mapEvent({ type: 'entity_damage', id: 'e1', amount: 12, by: 'rival' }, c), null);
  const hit = mapEvent({ type: 'entity_damage', id: 'e1', amount: 12, by: 'mine' }, c)!;
  assert.equal(hit.marker, 'hit');
  assert.deepEqual(hit.number, { key: 'e:e1', amount: 12 });
});

test('event mapping: kills, creativity, cast feedback, construct actions', () => {
  const c = ctx();
  const kill = mapEvent({ type: 'player_killed', target: 'p2', by: 'mine' }, c)!;
  assert.equal(kill.marker, 'kill');
  assert.ok(kill.hitstop!.keys.includes('*') && kill.hitstop!.ms === JUICE.hitstop.killMs);
  assert.equal(kill.killCam?.key, 'p:p2');
  const died = mapEvent({ type: 'player_killed', target: 'p1', by: 'rival' }, c)!;
  assert.equal(died.kind, 'killed');
  assert.equal(mapEvent({ type: 'player_killed', target: 'p2', by: 'p3' }, c), null);
  const insp = mapEvent({ type: 'inspiration', player: 'p1', novelty: 0.9, mul: 1.6, stale: false }, c)!;
  assert.equal(insp.callout?.text, 'INSPIRED ×1.6');
  const meh = mapEvent({ type: 'inspiration', player: 'p1', novelty: 0.5, mul: 1.1, stale: false }, c)!;
  assert.ok(insp.callout!.size! > meh.callout!.size!, 'more novel → bigger callout');
  assert.ok(insp.sound!.novelty! > meh.sound!.novelty!, '…and a brighter chime');
  assert.equal(mapEvent({ type: 'inspiration', player: 'p2', novelty: 0.9, mul: 1.6 }, c), null, "the rival's inspiration is theirs");
  assert.equal(mapEvent({ type: 'inspiration', player: 'p1', novelty: 0.1, mul: 0.8, stale: true }, c)!.sound!.name, 'stale');
  assert.equal(mapEvent({ type: 'creative_solution', player: 'p1', job: 'kill', bonus: 0.2 }, c)!.callout!.text, 'CREATIVE KILL');
  assert.equal(mapEvent({ type: 'construct_rejected' }, c)!.sound!.name, 'castFail');
  const formed = mapEvent({ type: 'construct_formed', id: 'mine', cost: 30 }, c)!;
  assert.ok(formed.fov! < 0, 'cast pulse pushes in');
  // adaptive mouse actions
  assert.equal(mapEvent({ type: 'action_start', construct: 'mine', kind: 'swing' }, c)!.marker, 'pulse');
  assert.equal(mapEvent({ type: 'action_charge', construct: 'mine', level: 0.7 }, c)!.charge!.level, 0.7);
  assert.equal(mapEvent({ type: 'action_release', construct: 'mine', kind: 'smash', level: 1 }, c)!.charge!.level, null);
  const charged = mapEvent({ type: 'action_hit', construct: 'mine', target: 'e1', energy: 1 }, c)!;
  const tap = mapEvent({ type: 'action_hit', construct: 'mine', target: 'e1', energy: 0.2 }, c)!;
  const plain = mapEvent({ type: 'impact', construct: 'mine', entity: 'e1', dv: 30, pos: [0, 1, 10] }, c)!;
  assert.ok(charged.trauma! > tap.trauma! && charged.hitstop!.ms > tap.hitstop!.ms);
  assert.ok(charged.trauma! > plain.trauma!, 'a fully charged hit lands heavier than a plain impact of the same size');
  assert.deepEqual(charged.hitstop!.keys, ['c:mine', 'e:e1']);
  // round results (loosely shaped until the duel core lands)
  assert.equal(mapEvent({ type: 'match_state', state: 'round_over', winner: 'p1' }, c)!.sound!.name, 'win');
  assert.equal(mapEvent({ type: 'match_state', state: 'round_over', winner: 'p2' }, c)!.sound!.name, 'lose');
  assert.equal(mapEvent({ type: 'match_state', state: 'playing' }, c), null);
});

test('player movement events', () => {
  const soft = mapPlayerEvent('land', { fallSpeed: 6, playerPos: [0, 0, 0] })!;
  const hard = mapPlayerEvent('land_hard', { fallSpeed: 30, playerPos: [0, 0, 0] })!;
  assert.ok(hard.kick!.lift! < soft.kick!.lift!, 'hard landings dip further');
  assert.ok(hard.trauma! > 0 && !soft.trauma);
  const yank = mapPlayerEvent('tether_attach_L', { fallSpeed: 0, anchor: [0, 10, 0], playerPos: [0, 0, 0] })!;
  assert.deepEqual(yank.kick!.dir, [0, 1, 0], 'tether snap pulls toward the anchor');
  assert.equal(mapPlayerEvent('tether_fire_R', { fallSpeed: 0, playerPos: [0, 0, 0] })!.sound!.name, 'tetherFire');
  assert.equal(mapPlayerEvent('nonsense', { fallSpeed: 0, playerPos: [0, 0, 0] }), null);
  assert.equal(energy(80, 40), 1.5);
});

// ------------------------------------------------------------------------------------------ audio

test('voice pool: caps voices, steals the weakest, drops when all are more important', () => {
  const p = new VoicePool(4);
  const stopped: number[] = [];
  const add = (prio: number, t: number, dur = 1) => { const v = p.acquire('s', prio, t, dur); if (v) v.handle = { stop: () => stopped.push(v.id) }; return v; };
  for (let i = 0; i < 4; i++) add(3, i * 0.01);
  assert.equal(p.active, 4);
  const v = add(5, 0.05);
  assert.ok(v, 'a more important sound steals');
  assert.equal(p.active, 4);
  assert.deepEqual(stopped, [1], 'the oldest lowest-priority voice was stopped');
  for (let i = 0; i < 3; i++) add(5, 0.06);
  assert.equal(add(1, 0.07), null, 'less important than everything playing → dropped');
  assert.equal(p.dropped, 1);
  p.prune(2);
  assert.equal(p.active, 0, 'finished voices are freed');
  assert.ok(p.throttle('gun', 0, 45));
  assert.ok(!p.throttle('gun', 0.02, 45), 'repeats inside the gap are throttled');
  assert.ok(p.throttle('gun', 0.05, 45));
});

/** Minimal WebAudio mock: records node creation, never makes sound. */
class MockParam { value = 0; setValueAtTime(v: number) { this.value = v; return this; } linearRampToValueAtTime(v: number) { this.value = v; return this; } exponentialRampToValueAtTime(v: number) { this.value = v; return this; } setTargetAtTime(v: number) { this.value = v; return this; } cancelScheduledValues() { return this; } }
class MockNode {
  connected = 0;
  onended: (() => void) | null = null;
  gain = new MockParam(); frequency = new MockParam(); Q = new MockParam(); threshold = new MockParam(); knee = new MockParam(); ratio = new MockParam(); attack = new MockParam(); release = new MockParam();
  positionX = new MockParam(); positionY = new MockParam(); positionZ = new MockParam();
  type = ''; buffer: unknown = null; loop = false; panningModel = ''; distanceModel = ''; refDistance = 1; rolloffFactor = 1; maxDistance = 1;
  constructor(private ctx: MockCtx, readonly kind: string) { ctx.nodes++; ctx.byKind[kind] = (ctx.byKind[kind] ?? 0) + 1; }
  connect(n: MockNode) { this.connected++; return n; }
  disconnect() { this.connected = 0; }
  start() { this.ctx.started++; }
  stop() { /* scheduled */ }
}
class MockCtx {
  nodes = 0; started = 0; byKind: Record<string, number> = {};
  currentTime = 0; sampleRate = 8000; state = 'running';
  destination = {};
  listener = { positionX: new MockParam(), positionY: new MockParam(), positionZ: new MockParam(), forwardX: new MockParam(), forwardY: new MockParam(), forwardZ: new MockParam(), upX: new MockParam(), upY: new MockParam(), upZ: new MockParam() };
  createGain() { return new MockNode(this, 'gain'); }
  createOscillator() { return new MockNode(this, 'osc'); }
  createBufferSource() { return new MockNode(this, 'src'); }
  createBiquadFilter() { return new MockNode(this, 'filter'); }
  createPanner() { return new MockNode(this, 'panner'); }
  createDynamicsCompressor() { return new MockNode(this, 'comp'); }
  createBuffer(_c: number, n: number) { return { getChannelData: () => new Float32Array(n) }; }
  suspend() { this.state = 'suspended'; return Promise.resolve(); }
  resume() { this.state = 'running'; return Promise.resolve(); }
}

test('sfx: every sound synthesises; voice limit holds under a barrage; mute and throttling work', () => {
  const ctx = new MockCtx();
  const sfx = new Sfx(() => ctx as unknown as AudioContext, 8);
  assert.equal(ctx.byKind.comp, 1, 'master limiter in the chain');
  for (const name of SOUND_NAMES) { ctx.currentTime += 0.2; sfx.pool.prune(ctx.currentTime); assert.ok(sfx.play(name, { energy: 0.8, kind: 'laser', novelty: 0.7, pos: [1, 2, 3] }), `plays ${name}`); }
  const perVoice = (ctx.nodes - 4) / SOUND_NAMES.length;
  console.log(`${SOUND_NAMES.length} sounds, ~${perVoice.toFixed(1)} nodes per voice`);
  assert.ok(perVoice < 20, 'voices stay cheap');
  // barrage: 200 impacts in one audio instant → capped at 8 concurrent, most throttled / stolen / dropped
  ctx.currentTime += 5;
  let played = 0;
  for (let i = 0; i < 200; i++) { ctx.currentTime += 0.03; if (sfx.play(i % 3 ? 'impact' : 'explosion', { energy: 1, pos: [0, 0, i] })) played++; assert.ok(sfx.pool.active <= 8); }
  const st = sfx.stats();
  console.log(`barrage: played ${played}, stolen ${st.stolen}, dropped ${st.dropped}, throttled ${st.throttled}, peak ${st.peak}`);
  assert.ok(st.peak <= 8);
  assert.ok(st.stolen > 0);
  // throttling merges spam of one kind
  const before = sfx.pool.throttled;
  sfx.play('weapon', { kind: 'gun' }); sfx.play('weapon', { kind: 'gun' });
  assert.equal(sfx.pool.throttled, before + 1);
  // mute (background tab)
  sfx.setMuted(true);
  ctx.currentTime += 1;
  assert.equal(sfx.play('impact', { energy: 1 }), false);
  assert.equal(ctx.state, 'suspended');
  sfx.setMuted(false);
  assert.equal(ctx.state, 'running');
  // loops are persistent (created once), not per frame
  const n0 = ctx.nodes;
  sfx.updateMovement(40, true);
  const n1 = ctx.nodes;
  for (let i = 0; i < 100; i++) sfx.updateMovement(40 + i * 0.1, i % 2 === 0);
  sfx.updateCharge(0.5); const n2 = ctx.nodes; sfx.updateCharge(1); sfx.updateCharge(null);
  assert.ok(n1 > n0 && ctx.nodes === n2, 'loops allocate once');
  assert.equal(sfx.stats().loops, 3);
  // unknown sound, no context
  assert.equal(sfx.play('nope'), false);
  const silent = new Sfx(() => null, 4);
  assert.ok(silent.play('impact'));
  assert.equal(silent.silentPlays, 1);
});
