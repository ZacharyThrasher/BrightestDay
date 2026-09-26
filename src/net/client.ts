/**
 * Renderer-side online API. The renderer never opens sockets: it asks the preload for a MessagePort to
 * the main-process net service (src/electron/net-main.ts) and exchanges PortRequest/PortEvent objects.
 *
 *   import { net } from '../net/client.js';
 *   const info = await net.host({ tunnel: true });        // {port, code, url, addresses}
 *   const j = await net.join('apple-banana-cherry-delta'); // {peerId, hostTick, hostName, url}
 *   net.send('all', { t: 'snap', ... });                  // host → peers; clients always send to the host
 *   net.onMessage((from, msg) => …); net.onPeer((e) => …); net.onStatus((s) => …);
 *   net.stats();  net.setHostTick(tick);  net.hostNow();  await net.stop();
 */
import {
  EMPTY_STATS, HOST_PEER, isPlainObject,
  type HostInfo, type HostOptions, type JoinInfo, type NetMessage, type NetStats, type NetStatus, type PeerEvent, type PeerId, type PortEvent, type PortRequest,
} from './protocol.js';

export type { HostInfo, HostOptions, JoinInfo, NetMessage, NetStats, NetStatus, PeerEvent, PeerId };
export { HOST_PEER };

interface NetBridge { open(): void }
type WithoutReq<T> = T extends unknown ? Omit<T, 'req'> : never;
const bridge = (): NetBridge | undefined => (window as any).hardlight?.net;

type Listener<A extends unknown[]> = (...a: A) => void;
function listeners<A extends unknown[]>() {
  const set = new Set<Listener<A>>();
  return {
    add(cb: Listener<A>) { set.add(cb); return () => { set.delete(cb); }; },
    emit(...a: A) { for (const cb of set) { try { cb(...a); } catch (err) { console.error('[net] listener failed', err); } } },
  };
}

export class NetApi {
  private port: MessagePort | null = null;
  private portWaiters: ((p: MessagePort) => void)[] = [];
  private reqId = 0;
  private pending = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void }>();
  private _stats: NetStats = EMPTY_STATS;
  private _status: NetStatus = { role: 'idle', state: 'idle' };
  private msgL = listeners<[PeerId, NetMessage]>();
  private peerL = listeners<[PeerEvent]>();
  private statusL = listeners<[NetStatus]>();
  private statsL = listeners<[NetStats]>();
  private windowHooked = false;

  /** False in the plain web build (no Electron main process → no online play). */
  get available() { return !!bridge()?.open; }
  get status() { return this._status; }
  get role() { return this._status.role; }

  private ensurePort(): Promise<MessagePort> {
    if (this.port) return Promise.resolve(this.port);
    const b = bridge();
    if (!b?.open) return Promise.reject(new Error('Online play needs the desktop app.'));
    if (!this.windowHooked) {
      this.windowHooked = true;
      window.addEventListener('message', (e) => {
        if (e.source !== window || !isPlainObject(e.data) || e.data.hlNetPort !== true || !e.ports[0]) return;
        this.attach(e.ports[0]);
      });
    }
    const p = new Promise<MessagePort>((resolve, reject) => {
      this.portWaiters.push(resolve);
      setTimeout(() => reject(new Error('net service did not answer')), 5000);
    });
    b.open();
    return p;
  }

  private attach(port: MessagePort) {
    this.port?.close();
    this.port = port;
    port.onmessage = (e) => this.onEvent(e.data as PortEvent);
    port.start();
    const w = this.portWaiters; this.portWaiters = [];
    w.forEach((f) => f(port));
  }

  private onEvent(e: PortEvent) {
    if (!isPlainObject(e)) return;
    switch (e.ev) {
      case 'msg': this.msgL.emit(e.from, e.msg); break;
      case 'stats': this._stats = e.stats; this.statsL.emit(e.stats); break;
      case 'peer': this.peerL.emit(e.peer); break;
      case 'status': this._status = e.status; this.statusL.emit(e.status); break;
      case 'reply': {
        const p = this.pending.get(e.req);
        if (!p) break;
        this.pending.delete(e.req);
        if (e.ok) p.resolve(e.data); else p.reject(new Error(e.error));
        break;
      }
    }
  }

  private async request<T>(r: WithoutReq<Extract<PortRequest, { req: number }>>): Promise<T> {
    const port = await this.ensurePort();
    const req = ++this.reqId;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(req, { resolve, reject });
      port.postMessage({ ...r, req });
    });
  }

  /** Host a match. Resolves when peers can connect (tunnel: once Cloudflare registered the address). */
  host(opts: HostOptions): Promise<HostInfo> { return this.request<HostInfo>({ op: 'host', opts }); }
  /** Join by room code, trycloudflare URL or ws://ip:port. Resolves on welcome; rejects with a readable reason. */
  join(codeOrUrl: string, name = 'Challenger'): Promise<JoinInfo> { return this.request<JoinInfo>({ op: 'join', target: codeOrUrl, name }); }
  /** Leave / stop hosting (also kills the tunnel). */
  stop(): Promise<void> { return this.port ? this.request<void>({ op: 'stop' }) : Promise.resolve(); }

  /** Send a game message (`t` must not be a reserved transport type). Clients always reach the host. Fire-and-forget. */
  send(to: PeerId | 'all', msg: NetMessage) { this.port?.postMessage({ op: 'send', to, msg } satisfies PortRequest); }
  /** Host: current sim tick (reported to joiners in welcome). */
  setHostTick(tick: number) { this.port?.postMessage({ op: 'tick', tick } satisfies PortRequest); }

  onMessage(cb: (from: PeerId, msg: NetMessage) => void) { return this.msgL.add(cb); }
  onPeer(cb: (e: PeerEvent) => void) { return this.peerL.add(cb); }
  onStatus(cb: (s: NetStatus) => void) { return this.statusL.add(cb); }
  onStats(cb: (s: NetStats) => void) { return this.statsL.add(cb); }

  /** Latest stats pushed by main (every 500 ms): rtt, bytes/s, per-peer info. */
  stats(): NetStats { return this._stats; }
  /** Client: estimated host clock (ms, host's Date.now()). Host: Date.now(). */
  hostNow(): number { return Date.now() + (this._stats.role === 'client' ? this._stats.clockOffset : 0); }
}

export const net = new NetApi();
