import { test } from 'node:test';
import assert from 'node:assert/strict';
import { WebSocket } from 'ws';
import {
  CLOSE, JsonCodec, PROTOCOL_VERSION, TokenBucket, decodeMessage, depthWithin, findQuickTunnelUrl, parseJoinTarget, roomCodeFromUrl, validateMessage,
  type NetMessage, type PeerEvent, type PortEvent,
} from '../src/net/protocol.js';
import { ClockSync } from '../src/net/clock.js';
import { SnapshotBuffer, lerpNum, lerpPose, slerpQuat, type Pose, type Quat } from '../src/net/interp.js';
import { applyFlat, dequantizeQuat, dequantizeVec3, diffFlat, diffMap, quantizeQuat, quantizeVec3, quatAngle } from '../src/net/quant.js';
import { NetClient, NetHost, NetService, sanitizeName } from '../src/electron/net-core.js';
import { tunnelFailureReason } from '../src/electron/tunnel.js';

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
async function until(cond: () => boolean, ms = 3000, what = 'condition') {
  const t0 = Date.now();
  while (!cond()) { if (Date.now() - t0 > ms) throw new Error(`timeout waiting for ${what}`); await sleep(5); }
}

// ------------------------------------------------------------------------------------ protocol helpers

test('join target parsing: codes, tunnel URLs, LAN addresses', () => {
  assert.equal(parseJoinTarget('apple-banana-cherry-delta'), 'wss://apple-banana-cherry-delta.trycloudflare.com');
  assert.equal(parseJoinTarget('  Apple-Banana-Cherry-Delta '), 'wss://apple-banana-cherry-delta.trycloudflare.com');
  assert.equal(parseJoinTarget('https://apple-banana-cherry-delta.trycloudflare.com/'), 'wss://apple-banana-cherry-delta.trycloudflare.com');
  assert.equal(parseJoinTarget('apple-banana-cherry-delta.trycloudflare.com'), 'wss://apple-banana-cherry-delta.trycloudflare.com');
  assert.equal(parseJoinTarget('ws://127.0.0.1:4567'), 'ws://127.0.0.1:4567');
  assert.equal(parseJoinTarget('192.168.1.20:4567'), 'ws://192.168.1.20:4567');
  assert.equal(parseJoinTarget('localhost:4567'), 'ws://localhost:4567');
  assert.equal(parseJoinTarget('http://10.0.0.2:8080'), 'ws://10.0.0.2:8080');
  assert.equal(parseJoinTarget('https://example.org/'), 'wss://example.org');
  for (const bad of ['', 'hello', 'ftp://x', 'javascript:alert(1)', 'ws://user:pw@host:1', '1-2-3', 'x'.repeat(400)]) assert.equal(parseJoinTarget(bad), null, bad);
});

test('quick tunnel URL + room code extraction from cloudflared output', () => {
  const line = '2026-09-26T03:00:09Z INF |  https://quiet-river-maple-orbit.trycloudflare.com                                   |';
  assert.equal(findQuickTunnelUrl(line), 'https://quiet-river-maple-orbit.trycloudflare.com');
  assert.equal(findQuickTunnelUrl('Requesting new quick Tunnel on trycloudflare.com...'), null);
  assert.equal(findQuickTunnelUrl('POST https://api.trycloudflare.com/tunnel'), null);
  assert.equal(roomCodeFromUrl('https://quiet-river-maple-orbit.trycloudflare.com'), 'quiet-river-maple-orbit');
  assert.equal(tunnelFailureReason(['2026-09-26T03:00:06Z INF Requesting new quick Tunnel on trycloudflare.com...', 'quick tunnel provisioning failed with status 403'], 'x'), 'quick tunnel provisioning failed with status 403');
});

test('message validation: plain objects with a string t only', () => {
  assert.equal(validateMessage({ t: 'snap', x: 1 }), null);
  for (const bad of [null, 1, 'x', [], [{ t: 'a' }], { t: 1 }, { t: '' }, { x: 1 }, { t: 'x'.repeat(40) }, new Date()]) assert.notEqual(validateMessage(bad), null, JSON.stringify(bad));
  let deep: any = { t: 'deep' };
  for (let i = 0; i < 100; i++) deep = { t: 'deep', c: deep };
  assert.equal(depthWithin(deep, 24), false);
  assert.notEqual(validateMessage(deep), null);
  // __proto__ from JSON stays an own property and doesn't pollute
  const r = decodeMessage(JsonCodec, '{"t":"x","__proto__":{"polluted":1}}');
  assert.ok(r.ok);
  assert.equal(({} as any).polluted, undefined);
  assert.equal(decodeMessage(JsonCodec, '{not json').ok, false);
  assert.equal(decodeMessage(JsonCodec, '[1,2]').ok, false);
  assert.equal(sanitizeName('  Hal\u0000‮ Jordan the Greatest Lantern Ever  '), 'Hal Jordan the Greatest');
});

test('token bucket limits sustained rate and burst', () => {
  const b = new TokenBucket(10, 20, 0);
  let n = 0;
  for (let i = 0; i < 100; i++) if (b.take(1, 0)) n++;
  assert.equal(n, 20);
  assert.equal(b.take(1, 50), false); // 0.5 token refilled
  assert.equal(b.take(1, 100), true); // 1 token after 100 ms
});

// ------------------------------------------------------------------------------------ pure helpers

test('clock sync: offset from pings, robust to a slow asymmetric sample', () => {
  const c = new ClockSync();
  const trueOffset = 5000;
  let t = 1000;
  for (let i = 0; i < 12; i++) {
    const rtt = i === 5 ? 400 : 40 + (i % 3) * 4;
    const up = i === 5 ? 350 : rtt / 2; // one very asymmetric sample
    c.add(t, t + up + trueOffset, t + rtt);
    t += 1000;
  }
  assert.ok(Math.abs(c.offset - trueOffset) < 5, `offset ${c.offset}`);
  assert.ok(c.rttAvg > 30 && c.rttAvg < 120);
  assert.equal(c.hostNow(100), 100 + c.offset);
  assert.equal(c.add(10, 0, 5), false); // negative rtt rejected
  // large jump snaps
  for (let i = 0; i < 16; i++) { c.add(t, t + 20 + 9000, t + 40); t += 1000; }
  assert.ok(Math.abs(c.offset - 9000) < 5, `snapped offset ${c.offset}`);
});

test('snapshot buffer: interpolation, out-of-order insert, extrapolation cap', () => {
  const b = new SnapshotBuffer<number>(lerpNum, 8, 100);
  assert.equal(b.sample(0), null);
  b.push(0, 0); b.push(200, 20); b.push(100, 10); // out of order
  assert.equal(b.size, 3);
  assert.equal(b.sample(50)!.state, 5);
  assert.equal(b.sample(150)!.state, 15);
  assert.equal(b.sample(-10)!.mode, 'before');
  const ex = b.sample(250)!;
  assert.equal(ex.mode, 'extrap'); assert.ok(Math.abs(ex.state - 25) < 1e-9);
  const held = b.sample(1000)!;
  assert.equal(held.mode, 'hold'); assert.ok(Math.abs(held.state - 30) < 1e-9); // capped at +100 ms
  for (let i = 3; i < 20; i++) b.push(i * 100, i * 10);
  assert.equal(b.size, 8);
  b.prune(1500);
  assert.equal(b.oldestTime, 1500);
  // poses
  const pb = new SnapshotBuffer<Pose>(lerpPose);
  const s = Math.SQRT1_2;
  pb.push(0, { p: [0, 0, 0], q: [0, 0, 0, 1] });
  pb.push(100, { p: [10, 0, 0], q: [0, s, 0, s] }); // 90° about Y
  const mid = pb.sample(50)!.state;
  assert.deepEqual(mid.p, [5, 0, 0]);
  assert.ok(Math.abs(quatAngle(mid.q, [0, 0, 0, 1]) - Math.PI / 4) < 1e-6);
  // shortest path with sign-flipped quaternion
  const q = slerpQuat([0, 0, 0, 1], [0, 0, 0, -1], 0.5);
  assert.ok(Math.abs(Math.abs(q[3]) - 1) < 1e-9);
});

test('quantization + deltas', () => {
  const v = quantizeVec3([1.23456, -7.0004, 0.0005]);
  assert.deepEqual(v, [1235, -7000, 1]);
  const back = dequantizeVec3(v);
  assert.ok(Math.abs(back[0] - 1.235) < 1e-9);
  for (let i = 0; i < 200; i++) {
    const a = [Math.sin(i), Math.cos(i * 1.3), Math.sin(i * 0.7) * 0.3, Math.cos(i * 2.1)];
    const l = Math.hypot(...a);
    const q = a.map((x) => x / l) as Quat;
    const e = quantizeQuat(q);
    assert.ok(e.every((x) => Number.isInteger(x)));
    assert.ok(quatAngle(q, dequantizeQuat(e)) < 0.001, `quat ${i}`);
  }
  const d = diffFlat({ x: 1, p: [1, 2, 3], hp: 100, gone: 1 }, { x: 1, p: [1, 2, 4], hp: 100 });
  assert.deepEqual(d, { p: [1, 2, 4], _del: ['gone'] });
  assert.equal(diffFlat({ a: 1 }, { a: 1 }), null);
  assert.deepEqual(applyFlat({ x: 1, p: [1, 2, 3], hp: 100, gone: 1 }, d), { x: 1, p: [1, 2, 4], hp: 100 });
  const m = diffMap(new Map([[1, { a: 1 }], [2, { a: 2 }]]), new Map([[1, { a: 1 }], [3, { a: 3 }]]));
  assert.deepEqual(m, { upd: [[3, { a: 3 }]], del: [2] });
});

// ------------------------------------------------------------------------------------ real sockets on localhost

test('host/client over real WebSockets: handshake, relay both ways, ping, disconnect', async () => {
  const host = new NetHost({ name: 'Hal', maxPeers: 1, getTick: () => 1234, limits: { pingIntervalMs: 50 }, resumeGraceMs: 0 });
  const port = await host.start();
  const hostPeers: PeerEvent[] = [];
  const hostMsgs: [number, NetMessage][] = [];
  host.onPeer = (e) => hostPeers.push(e);
  host.onMessage = (from, m) => hostMsgs.push([from, m]);

  const client = new NetClient({ name: 'Guy', limits: { pingIntervalMs: 50 } });
  const clientMsgs: NetMessage[] = [];
  const clientPeers: PeerEvent[] = [];
  client.onMessage = (_f, m) => clientMsgs.push(m);
  client.onPeer = (e) => clientPeers.push(e);
  const j = await client.connect(`ws://127.0.0.1:${port}`);
  assert.equal(j.peerId, 1);
  assert.equal(j.hostTick, 1234);
  assert.equal(j.hostName, 'Hal');
  await until(() => hostPeers.length === 1, 1000, 'host join event');
  assert.deepEqual(hostPeers[0], { kind: 'join', peerId: 1, name: 'Guy' });
  assert.equal(clientPeers[0].kind, 'join');

  // relay both directions
  assert.equal(client.send({ t: 'input', seq: 1, keys: [1, 0, 1] }), true);
  assert.equal(host.send('all', { t: 'snap', tick: 1, players: [{ id: 1, p: [1, 2, 3] }] }), true);
  assert.equal(host.send(1, { t: 'direct' }), true);
  assert.equal(host.send(99, { t: 'nobody' }), false);
  assert.equal(host.send('all', { t: 'welcome' } as any), false); // reserved type refused
  await until(() => hostMsgs.length === 1 && clientMsgs.length === 2, 1000, 'relay');
  assert.deepEqual(hostMsgs[0], [1, { t: 'input', seq: 1, keys: [1, 0, 1] }]);
  assert.deepEqual(clientMsgs.map((m) => m.t), ['snap', 'direct']);

  // ping / RTT / clock both sides
  await until(() => client.clock.sampleCount >= 4 && host.stats().peers[0].rtt > 0, 2000, 'pings');
  const cs = client.stats(), hs = host.stats();
  console.log(`localhost RTT: client→host ${cs.peers[0].rtt.toFixed(2)} ms (avg ${cs.rtt.toFixed(2)}), host→client ${hs.peers[0].rtt.toFixed(2)} ms; clock offset ${cs.clockOffset.toFixed(2)} ms`);
  assert.ok(cs.peers[0].rtt < 50 && Math.abs(cs.clockOffset) < 20);
  assert.ok(hs.inBps >= 0 && hs.peers[0].connected);

  // a second client is refused (duel is full)
  const extra = new NetClient({ name: 'Third' });
  await assert.rejects(extra.connect(`ws://127.0.0.1:${port}`), /full/);

  // clean disconnect
  await client.stop('gg');
  await until(() => hostPeers.some((e) => e.kind === 'leave'), 2000, 'leave');
  assert.equal(hostPeers.at(-1)!.reason, 'gg');
  assert.equal(host.peerCount, 0);
  await host.stop();
});

test('version mismatch gives a clear error', async () => {
  const host = new NetHost({});
  const port = await host.start();
  const ws = new WebSocket(`ws://127.0.0.1:${port}`);
  const got: any[] = [];
  ws.on('message', (d) => got.push(JSON.parse(d.toString())));
  await new Promise((r) => ws.on('open', r));
  ws.send(JSON.stringify({ t: 'hello', v: PROTOCOL_VERSION + 1, name: 'future' }));
  const code = await new Promise<number>((r) => ws.on('close', (c) => r(c)));
  assert.equal(code, CLOSE.version);
  assert.equal(got[0].t, 'error');
  assert.equal(got[0].code, 'version');
  assert.match(got[0].message, /Version mismatch/);
  await host.stop();
});

test('browser-originated connections (Origin header) are refused', async () => {
  const host = new NetHost({});
  const port = await host.start();
  const ws = new WebSocket(`ws://127.0.0.1:${port}`, { origin: 'https://evil.example' });
  const err = await new Promise<Error>((r) => ws.on('error', r));
  assert.match(err.message, /401|403|Unexpected server response/);
  await host.stop();
});

test('garbage, oversize and floods are dropped / kicked', async () => {
  const host = new NetHost({ limits: { maxMessageBytes: 4096, maxViolations: 10, msgPerSec: 50, msgBurst: 50, pingIntervalMs: 100000 } });
  const port = await host.start();
  const msgs: NetMessage[] = [];
  const peers: PeerEvent[] = [];
  host.onMessage = (_f, m) => msgs.push(m);
  host.onPeer = (e) => peers.push(e);
  // garbage before hello → rejected handshake
  {
    const ws = new WebSocket(`ws://127.0.0.1:${port}`);
    await new Promise((r) => ws.on('open', r));
    ws.send('{"t":"snap"}');
    assert.equal(await new Promise<number>((r) => ws.on('close', (c) => r(c))), CLOSE.handshake);
  }
  const c = new NetClient({ limits: { pingIntervalMs: 100000, maxMessageBytes: 4096 }, maxReconnects: 0 });
  await c.connect(`ws://127.0.0.1:${port}`);
  // invalid messages are dropped, valid ones still arrive
  for (const g of ['not json', '[1,2,3]', '{"x":1}', '{"t":5}', 'null', '{"t":"hello","v":1}']) c.sendRawForTest(g);
  c.send({ t: 'ok' });
  await until(() => msgs.length === 1, 1000, 'valid message after garbage');
  assert.equal(msgs[0].t, 'ok');
  assert.equal(host.stats().peers[0].dropped, 6);
  // local oversize send is refused client-side
  assert.equal(c.send({ t: 'big', pad: 'x'.repeat(5000) }), false);
  // raw oversize frame → connection closed by the server (1009)
  const closed = new Promise<void>((r) => { c.onStatus = (s) => { if (s.state === 'failed' || s.state === 'closed') r(); }; });
  c.sendRawForTest(JSON.stringify({ t: 'big', pad: 'x'.repeat(5000) }));
  await closed;
  await until(() => peers.some((e) => e.kind === 'lost' || e.kind === 'leave'), 1000, 'host noticed');

  // flood: over the message rate → dropped, then kicked after maxViolations
  const f = new NetClient({ limits: { pingIntervalMs: 100000 }, maxReconnects: 0 });
  await until(() => host.peerCount === 0, 12000, 'slot free'); // lost peer freed after grace
  await f.connect(`ws://127.0.0.1:${port}`);
  const before = msgs.length;
  const fclosed = new Promise<void>((r) => { f.onStatus = (s) => { if (s.state === 'closed' || s.state === 'failed') r(); }; });
  for (let i = 0; i < 200; i++) f.sendRawForTest(JSON.stringify({ t: 'spam', i }));
  await fclosed;
  const got = msgs.length - before;
  assert.ok(got <= 51 && got >= 40, `accepted ${got} of 200`);
  await host.stop();
});

test('client reconnects and resumes its peer id after a drop', async () => {
  const host = new NetHost({ limits: { pingIntervalMs: 100000 }, resumeGraceMs: 5000 });
  const port = await host.start();
  const peers: PeerEvent[] = [];
  host.onPeer = (e) => peers.push(e);
  const c = new NetClient({ limits: { pingIntervalMs: 100000 } });
  const states: string[] = [];
  c.onStatus = (s) => states.push(s.state);
  const j = await c.connect(`ws://127.0.0.1:${port}`);
  // kill the socket from the server side without a close handshake
  for (const ws of (host as any).wss.clients) ws.terminate();
  await until(() => peers.some((e) => e.kind === 'resume'), 5000, 'resume');
  assert.deepEqual(peers.map((e) => e.kind), ['join', 'lost', 'resume']);
  assert.equal(c.peerId, j.peerId);
  assert.ok(states.includes('reconnecting') && states.at(-1) === 'connected');
  assert.equal(c.send({ t: 'after' }), true);
  await c.stop();
  await host.stop();
});

test('NetService: host (LAN) + join via port-protocol requests, tunnel failure is reported', async () => {
  const hostEvents: PortEvent[] = [];
  const joinEvents: PortEvent[] = [];
  const hs = new NetService((e) => hostEvents.push(e), { statsIntervalMs: 50 });
  const js = new NetService((e) => joinEvents.push(e), { statsIntervalMs: 50 });
  await hs.handle({ op: 'host', req: 1, opts: { tunnel: false, name: 'Hal' } });
  const r1 = hostEvents.find((e) => e.ev === 'reply') as any;
  assert.ok(r1.ok, JSON.stringify(r1));
  const addr = r1.data.addresses[0];
  assert.match(addr, /^ws:\/\/127\.0\.0\.1:\d+$/);
  await js.handle({ op: 'join', req: 7, target: addr, name: 'Guy' });
  const r2 = joinEvents.find((e) => e.ev === 'reply') as any;
  assert.ok(r2.ok && r2.data.hostName === 'Hal', JSON.stringify(r2));
  await js.handle({ op: 'send', to: 0, msg: { t: 'greet', name: 'Guy' } });
  await hs.handle({ op: 'send', to: 'all', msg: { t: 'greet', name: 'Hal' } });
  await hs.handle({ op: 'send', to: 'all', msg: 'nope' }); // ignored
  await hs.handle({ nonsense: true }); // ignored
  await until(() => hostEvents.some((e) => e.ev === 'msg') && joinEvents.some((e) => e.ev === 'msg'), 1000, 'greetings');
  await until(() => joinEvents.some((e) => e.ev === 'stats' && e.stats.peers[0]?.connected), 1000, 'stats');
  await js.handle({ op: 'stop', req: 8 });
  await until(() => hostEvents.some((e) => e.ev === 'peer' && e.peer.kind === 'leave'), 2000, 'leave');
  await hs.stop();

  // tunnel mode with a cloudflared that dies immediately → clear failure, server closed
  const ts = new NetService(() => {}, {
    startTunnel: () => ({ ready: Promise.reject(new Error('quick tunnel provisioning failed with status 403')), log: [], url: null, onExit() {}, stop() {} }),
  });
  await assert.rejects(ts.startHost({ tunnel: true }), /tunnel failed: quick tunnel provisioning failed with status 403/);
  assert.equal(ts.currentStatus.state, 'failed');
});
