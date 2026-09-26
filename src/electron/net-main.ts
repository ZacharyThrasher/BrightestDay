/**
 * Online play bridge (Electron main process). All sockets and the cloudflared child live here; the
 * sandboxed renderer stays offline and talks to this service through ONE MessageChannelMain port:
 *   renderer → preload: ipc 'hl:net-open'
 *   main → preload:     'hl:net-port' carrying port2 (the preload hands it to the page via window.postMessage)
 *   then PortRequest / PortEvent objects (src/net/protocol.ts) flow over the port.
 * Re-opening (page reload) replaces the previous port and stops the old session.
 */
import { app, ipcMain, MessageChannelMain, type MessagePortMain, type WebContents } from 'electron';
import { join } from 'node:path';
import { NetService } from './net-core.js';
import { findCloudflared, startQuickTunnel } from './tunnel.js';

export function installNetBridge(isOurWindow: (wc: WebContents) => boolean) {
  let port: MessagePortMain | null = null;
  const post = (e: unknown) => { try { port?.postMessage(e); } catch { /* port closed */ } };
  const service = new NetService(post, {
    startTunnel: (p) => {
      const bin = findCloudflared({
        resourcesPath: app.isPackaged ? process.resourcesPath : undefined,
        extra: app.isPackaged ? [] : [
          join(process.cwd(), 'release/cache', process.platform === 'win32' ? 'cloudflared.exe' : 'cloudflared'),
          join(process.cwd(), 'release/cache/cloudflared-linux-amd64'),
        ],
      });
      if (!bin) throw new Error('cloudflared was not found (expected resources/cloudflared/cloudflared.exe next to the game, or on PATH). Use LAN hosting or reinstall.');
      console.log(`[net] starting quick tunnel with ${bin}`);
      return startQuickTunnel(bin, p, { onProgress: (stage, info) => { if (stage === 'url' && info) post({ ev: 'status', status: { role: 'host', state: 'tunnel-starting', detail: 'tunnel address assigned, connecting to Cloudflare…', code: info.code, url: info.url } }); } });
    },
  });

  ipcMain.on('hl:net-open', (e) => {
    if (!isOurWindow(e.sender)) return;
    void service.stop();
    try { port?.close(); } catch { /* ignore */ }
    const { port1, port2 } = new MessageChannelMain();
    port = port1;
    port1.on('message', (m) => { void service.handle(m.data).catch((err) => console.warn('[net] request failed', err)); });
    port1.on('close', () => { if (port === port1) { port = null; void service.stop(); } });
    port1.start();
    e.sender.postMessage('hl:net-port', null, [port2]);
    post({ ev: 'status', status: service.currentStatus });
  });

  let quitting = false;
  app.on('before-quit', (ev) => {
    if (quitting) return;
    quitting = true;
    ev.preventDefault();
    // say bye to peers and kill cloudflared, but never hang the quit
    Promise.race([service.stop(), new Promise((r) => setTimeout(r, 1500))]).finally(() => app.quit());
  });
  // app.exit() (smoke tests, fatal paths) skips before-quit: still never leave cloudflared running
  process.on('exit', () => service.killTunnel());
  return service;
}
