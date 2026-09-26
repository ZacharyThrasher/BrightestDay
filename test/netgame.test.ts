/**
 * Online duel over the REAL WebSocket transport, host and client Worlds in one process (src/net/game.ts):
 * the client moves its own lantern instantly, casts through the host, and both sides agree on health,
 * the score and the discrete-state hash after scripted hits and kills.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Vector3 } from 'three';
import { World } from '../src/sim/world.js';
import { buildScene } from '../src/sim/levels.js';
import { NO_INPUT, type PlayerInput } from '../src/sim/player.js';
import { NetClient, NetHost } from '../src/electron/net-core.js';
import { ClientSession, HostSession, type GameLink, type NetNotice } from '../src/net/game.js';
import type { NetMessage, PeerEvent } from '../src/net/protocol.js';

const now = () => performance.timeOrigin + performance.now();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

interface Rig {
  host: NetHost; client: NetClient;
  hw: World; hs: HostSession;
  cw: World | null; cs: ClientSession | null;
  hostInput: () => PlayerInput;
  clientInput: () => PlayerInput;
  notices: NetNotice[];
  bytesIn: { host: number; client: number };
}

async function setup(): Promise<Rig> {
  const host = new NetHost({ name: 'Hal', maxPeers: 1, limits: { pingIntervalMs: 100 }, resumeGraceMs: 5000 });
  const port = await host.start();
  const hw = await World.create();
  buildScene(hw, 'duel');
  const hostLink: GameLink = { send: (to, m) => { host.send(to, m); }, hostNow: now };
  const rig: Rig = {
    host, client: new NetClient({ name: 'Guy', limits: { pingIntervalMs: 100 } }), hw, hs: null as any, cw: null, cs: null,
    hostInput: () => ({ ...NO_INPUT, yaw: hw.player.yaw, pitch: hw.player.pitch }),
    clientInput: () => ({ ...NO_INPUT, yaw: rig.cw!.player.yaw, pitch: rig.cw!.player.pitch }),
    notices: [], bytesIn: { host: 0, client: 0 },
  };
  rig.hs = new HostSession(hw, hostLink, { onNotice: (n) => rig.notices.push(n) });
  host.onPeer = (e: PeerEvent) => {
    if (e.kind === 'join') { rig.hs.addPeer(e.peerId, e.name, 'duel'); hw.startMatch({ warmup: 0.5 }); }
    else rig.hs.onPeer(e);
  };
  host.onMessage = (from, m) => { rig.bytesIn.host += JSON.stringify(m).length; rig.hs.onMessage(from, m); };
  const clientLink: GameLink = { send: (_to, m) => { rig.client.send(m); }, hostNow: () => rig.client.clock.hostNow(now()) };
  const started = new Promise<void>((resolve) => {
    rig.client.onMessage = (from, m: NetMessage) => {
      rig.bytesIn.client += JSON.stringify(m).length;
      if (m.t === 'start') {
        // the joiner loads the same scene and becomes the lantern the host gave it
        void (async () => {
          const cw = await World.create();
          buildScene(cw, m.scene as any);
          rig.cw = cw;
          rig.cs = new ClientSession(cw, clientLink, String(m.you), m.names as Record<string, string>, { onNotice: (n) => rig.notices.push(n) });
          rig.cs.ready();
          resolve();
        })();
        return;
      }
      rig.cs?.onMessage(from, m);
    };
  });
  rig.client.onPeer = (e) => rig.cs?.onPeer(e);
  await rig.client.connect(`ws://127.0.0.1:${port}`);
  await started;
  return rig;
}

/** Run both sims in real time (fixed 60 Hz steps, like the app's loop) for `ms`, or until `until()`. */
async function run(rig: Rig, ms: number, until?: () => boolean) {
  const DT = 1000 / 60;
  let last = now(), accH = 0, accC = 0;
  const end = now() + ms;
  while (now() < end) {
    await sleep(4);
    const t = now(), d = Math.min(250, t - last);
    last = t;
    accH += d; accC += d;
    while (accH >= DT) { accH -= DT; if (!rig.hs.paused) rig.hs.step(rig.hostInput()); }
    while (accC >= DT) { accC -= DT; rig.cs?.step(rig.clientInput()); }
    if (until?.()) return true;
  }
  return false;
}

async function teardown(rig: Rig) {
  rig.cs?.dispose();
  await rig.client.stop();
  await rig.host.stop();
}

test('online duel: client moves instantly, casts through the host, both agree on hits, kills and the score', { timeout: 60000 }, async () => {
  const rig = await setup();
  const { hw, hs } = rig;
  try {
    const cs = rig.cs!, cw = rig.cw!;
    assert.equal(cw.player.id, 'p2');
    assert.equal(cw.players.map((p) => p.id).join(), 'p1,p2');
    const hp2 = hw.playerById('p2')!;
    // the host waits for the client to load, then the warmup runs and goes live
    assert.ok(await run(rig, 5000, () => hw.match?.phase === 'live' && cw.match?.phase === 'live'), `match live on both (host ${hw.match?.phase}, client ${cw.match?.phase})`);
    assert.ok(!cs.hostControlled, 'the client owns its lantern once alive and free');

    // ---- responsiveness: the client's own lantern reacts on the very tick the key goes down (no round trip)
    const me = cw.player;
    await run(rig, 300);
    const p0 = me.pos.clone();
    rig.clientInput = () => ({ ...NO_INPUT, yaw: me.yaw, pitch: me.pitch, moveZ: 1 });
    cs.step(rig.clientInput());
    const firstTick = me.vel.length();
    for (let i = 0; i < 9; i++) cs.step(rig.clientInput());
    const moved10 = me.pos.distanceTo(p0);
    console.log(`client input → own lantern: speed after 1 tick ${firstTick.toFixed(2)} m/s, moved ${moved10.toFixed(2)} m in 10 ticks (no host round trip)`);
    assert.ok(firstTick > 0.5, 'speed on the first tick');
    assert.ok(moved10 > 0.3, 'moved within 10 ticks');
    // and the host's proxy follows within a few frames
    const ok = await run(rig, 1500, () => hp2.pos.distanceTo(me.pos) < 1.5 && hp2.pos.distanceTo(p0) > 1);
    console.log(`host proxy of p2 ${hp2.pos.distanceTo(me.pos).toFixed(2)} m from the client's own position`);
    assert.ok(ok, 'host proxy tracks the client');
    rig.clientInput = () => ({ ...NO_INPUT, yaw: me.yaw, pitch: me.pitch });

    // ---- a real cast from the client: stand close, "punch him" → host runs p2's Director, p1 gets hurt
    hw.player.teleport(new Vector3(10, 1, 20));
    me.teleport(new Vector3(10, 1, 8));
    me.yaw = 0; me.pitch = -0.05; // facing -z… turn to face p1 (+z)
    me.yaw = Math.PI;
    hw.player.protectedUntil = -Infinity; hp2.protectedUntil = -Infinity;
    await run(rig, 300);
    hw.player.protectedUntil = -Infinity;
    const hpBefore = hw.player.hp;
    const res = await cs.cast('punch him');
    console.log(`client cast "punch him" → ${res?.applied ? 'formed' : 'refused'}: ${res?.summary}`);
    assert.ok(res && res.applied, `the host formed the client's construct (${res?.summary})`);
    const cid = res!.constructIds[0];
    assert.equal(hw.constructs.get(cid)?.owner, 'p2', 'owned by the client lantern');
    assert.ok(await run(rig, 1500, () => !!cw.constructs.get(cid) || hw.player.hp < hpBefore), 'the construct shows up on the client (built from its spec)');
    const hurt = await run(rig, 3000, () => hw.player.hp < hpBefore);
    if (!hurt) console.log('DEBUG', hw.player.pos.toArray(), hp2.pos.toArray(), me.pos.toArray(), hw.player.isProtected, hw.match?.phase, JSON.stringify(hw.log.slice(-25).map((e) => [e.type, e.target ?? e.id ?? '', e.reason ?? e.cause ?? ''])));
    assert.ok(hurt, 'the punch hurts p1 on the host');
    assert.ok(await run(rig, 1500, () => cw.playerById('p1')!.hp === hw.player.hp || cw.playerById('p1')!.hp < hpBefore), 'the client sees the damage');
    console.log(`p1 hp host ${hw.player.hp.toFixed(1)} / client ${cw.playerById('p1')!.hp.toFixed(1)}`);

    // ---- knockback on the client's lantern comes back as a correction it applies to its own movement
    const corr0 = cs.stats.corrections;
    hp2.protectedUntil = -Infinity;
    await run(rig, 100);
    // hits land inside a host tick (constructs, projectiles): do the same with a one-shot tick hook
    let fired = false;
    hw.tickHooks.push(() => { if (!fired) { fired = true; hw.damagePlayer(hp2, 10, 'strike', 'p1', { dir: new Vector3(1, 0.3, 0) }); } });
    assert.ok(await run(rig, 1500, () => cs.stats.corrections > corr0), 'correction received');
    assert.ok(await run(rig, 1000, () => me.hp < 100), `client sees its own damage (hp ${me.hp})`);

    // ---- scripted kills: p1 scores twice, p2 once; both see the same score and the same state hash
    const killer = hw.constructs.spawn(
      { ...(await import('../src/sim/constructs/spec.js')).defaultSpec({ concept: 'hammer', label: 'hammer', parts: (await import('../src/sim/constructs/forms.js')).buildForm('hammer') }), placement: { at: 'front', orient: 'face_away' } },
      { owner: 'p1' },
    ).construct!;
    for (const [victim, by] of [[hp2, killer.id], [hw.player, 'p2'], [hp2, killer.id]] as const) {
      assert.ok(await run(rig, 6000, () => victim.alive && !victim.isProtected && hw.match?.phase === 'live'), `${victim.id} up again`);
      hw.damagePlayer(victim, 500, 'strike', by);
      await run(rig, 50);
    }
    assert.deepEqual(hw.match!.scores, { p1: 2, p2: 1 });
    assert.ok(await run(rig, 2000, () => JSON.stringify(cw.match?.scores) === JSON.stringify(hw.match!.scores)), `client score ${JSON.stringify(cw.match?.scores)}`);
    const kills = cw.log.filter((e) => e.type === 'kill').length;
    console.log(`scores host ${JSON.stringify(hw.match!.scores)} client ${JSON.stringify(cw.match!.scores)}; kill events on the client: ${kills}; hash checks ${cs.stats.hashChecks}, desyncs ${cs.stats.desyncs}`);
    assert.equal(kills, 3, 'the kill feed replays on the client');
    assert.ok(cs.stats.hashChecks > 50);
    assert.equal(cs.stats.desyncs, 0, 'discrete state hashes agree');
    await run(rig, 300);
    assert.equal(cw.stateHash('sync').length, 8);

    // ---- a jointed machine: built on the client from its spec, joint poses follow the host's
    const { Director } = await import('../src/lang/director.js');
    const tank = new Director(hw, 'p1').commitSync('a tank');
    assert.ok(tank.applied, tank.summary);
    const tid = tank.constructIds[0];
    assert.ok(await run(rig, 3000, () => !!cw.constructs.get(tid)?.insts[0]?.jointPose?.size), 'the tank and its joint poses reach the client');
    const ht = hw.constructs.get(tid)!, ct = cw.constructs.get(tid)!;
    console.log(`tank: host ${ht.spec.label} v${ht.version} joints [${[...(ht.insts[0].jointPose?.keys() ?? [])].join(', ')}] → client ${ct.spec.label}, ${ct.insts.length} inst(s), ${ct.insts[0].jointPose!.size} joint poses, ${ct.insts[0].pos.distanceTo(ht.insts[0].pos).toFixed(2)} m behind`);
    assert.equal(ct.spec.label, ht.spec.label);
    assert.equal(ct.owner, 'p1');

    // ---- bandwidth: every message under the cap, a sane rate
    console.log(`host → client: ${hs.stats.snaps} snapshots, avg ${(hs.stats.bytes / Math.max(1, hs.stats.snaps)).toFixed(0)} B, max ${hs.stats.maxSnap} B; corrections ${hs.stats.corrections}`);
    assert.ok(hs.stats.maxSnap < 60 * 1024);
    assert.equal(rig.host.stats().peers[0].dropped, 0, 'no client message was dropped by the rate limits');
    assert.equal(rig.client.stats().peers[0].dropped, 0, 'no host message was dropped by the rate limits');
  } finally {
    await teardown(rig);
  }
});

test('online duel: a dropped client pauses the match, reconnects, resyncs and play continues', { timeout: 60000 }, async () => {
  const rig = await setup();
  const { hw, hs } = rig;
  try {
    assert.ok(await run(rig, 5000, () => hw.match?.phase === 'live' && rig.cw!.match?.phase === 'live'));
    const t0 = hw.time;
    // kill the socket from the server side (no close handshake)
    for (const ws of (rig.host as any).wss.clients) ws.terminate();
    assert.ok(await run(rig, 3000, () => hs.paused), 'host paused');
    const tp = hw.time;
    await run(rig, 300);
    assert.equal(hw.time, tp, 'no ticks while the opponent is away');
    assert.ok(rig.notices.some((n) => n.kind === 'pause'), 'pause notice (opponent-left banner)');
    assert.ok(await run(rig, 6000, () => !hs.paused && !rig.cs!.paused), 'resumed after the reconnect');
    assert.ok(rig.notices.some((n) => n.kind === 'resume'));
    const hp2 = hw.playerById('p2')!;
    hp2.protectedUntil = -Infinity;
    await run(rig, 200);
    hw.damagePlayer(hp2, 30, 'strike', 'p1');
    assert.ok(await run(rig, 2000, () => rig.cw!.player.hp === hp2.hp), 'state flows again after the resync');
    assert.ok(hw.time > t0);
    assert.equal(rig.cs!.stats.desyncs, 0);
  } finally {
    await teardown(rig);
  }
});
