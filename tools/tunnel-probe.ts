// Real Cloudflare quick-tunnel probe (no Electron):  npx tsx tools/tunnel-probe.ts [seconds]
// Hosts a NetHost on 127.0.0.1, exposes it with cloudflared (PATH, $HARDLIGHT_CLOUDFLARED or
// release/cache/cloudflared-linux-amd64), joins it through wss://<words>.trycloudflare.com and prints RTT.
import { join } from 'node:path';
import { NetClient, NetHost } from '../src/electron/net-core.js';
import { findCloudflared, startQuickTunnel } from '../src/electron/tunnel.js';

const secs = Number(process.argv[2] ?? 10);
const bin = findCloudflared({ extra: [join(process.cwd(), 'release/cache/cloudflared-linux-amd64'), join(process.cwd(), 'release/cache/cloudflared.exe')] });
if (!bin) { console.error('cloudflared not found'); process.exit(2); }
const host = new NetHost({ name: 'probe-host' });
const port = await host.start();
console.log(`host on 127.0.0.1:${port}, starting ${bin}`);
const t = startQuickTunnel(bin, port, { onProgress: (s, i) => console.log(`tunnel: ${s}${i ? ` ${i.url}` : ''}`) });
let code = 0;
try {
  const info = await t.ready;
  console.log(`tunnel ready: ${info.url} (room code ${info.code})`);
  const c = new NetClient({ name: 'probe-client' });
  let j;
  for (let i = 0; i < 10 && !j; i++) {
    try { j = await c.connect(info.code); } catch (e) { console.log(`join attempt ${i + 1}: ${(e as Error).message}`); await new Promise((r) => setTimeout(r, 2000)); }
  }
  if (!j) throw new Error('could not join through the tunnel');
  console.log(`joined as peer ${j.peerId} via ${j.url}`);
  const rtts: number[] = [];
  for (let i = 0; i < secs; i++) { await new Promise((r) => setTimeout(r, 1000)); rtts.push(c.stats().peers[0].rtt); }
  rtts.sort((a, b) => a - b);
  console.log(`RTT over tunnel: min ${rtts[0].toFixed(1)} ms, median ${rtts[rtts.length >> 1].toFixed(1)} ms, max ${rtts[rtts.length - 1].toFixed(1)} ms (${rtts.length} samples)`);
  await c.stop();
} catch (e) {
  console.error(`TUNNEL PROBE FAILED: ${(e as Error).message}`);
  console.error(t.log.slice(-8).join('\n'));
  code = 1;
}
t.stop();
await host.stop();
process.exit(code);
