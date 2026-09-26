/**
 * Preload (sandboxed, context-isolated). Exposes `window.hardlight` to the renderer:
 *   isElectron, settings, saveSettings(), toggleFullscreen(), quit(),
 *   s2.info()/complete()/mode()  ← optional System-2 interpreter (key stays in the main process)
 *   semantic()                   ← optional word-vector file bytes (or null)
 *   voice.start()/stop()/onPartial(cb)/onFinal(cb)   ← IPC placeholder for the voice integrator
 *   net.open()                   ← online play: main answers with a MessagePort, posted to the page as
 *                                  window 'message' {hlNetPort:true} (see src/net/client.ts)
 */
import { contextBridge, ipcRenderer } from 'electron';

let settings: Record<string, unknown> = {};
try { settings = ipcRenderer.sendSync('hl:get-settings') ?? {}; } catch { settings = {}; }

contextBridge.exposeInMainWorld('hardlight', {
  isElectron: true,
  settings,
  saveSettings: (s: Record<string, unknown>) => ipcRenderer.send('hl:save-settings', s),
  toggleFullscreen: () => ipcRenderer.send('hl:toggle-fullscreen'),
  quit: () => ipcRenderer.send('hl:quit'),
  ready: () => ipcRenderer.send('hl:ready'),
  fatal: (msg: string) => ipcRenderer.send('hl:fatal', String(msg)),
  s2: {
    info: () => ipcRenderer.invoke('hl:s2-info'),
    complete: (req: unknown, timeoutMs: number) => ipcRenderer.invoke('hl:s2-complete', req, timeoutMs),
    mode: () => ipcRenderer.sendSync('hl:s2-mode'),
  },
  semantic: () => ipcRenderer.invoke('hl:semantic'),
  net: { open: () => ipcRenderer.send('hl:net-open') },
  voice: {
    start: () => ipcRenderer.send('hl:voice-start'),
    stop: () => ipcRenderer.send('hl:voice-stop'),
    onPartial: (cb: (text: string) => void) => { ipcRenderer.on('hl:voice-partial', (_e, text: string) => cb(String(text))); },
    onFinal: (cb: (text: string) => void) => { ipcRenderer.on('hl:voice-final', (_e, text: string) => cb(String(text))); },
  },
});

// MessagePorts can't cross contextBridge: hand the online-play port to the page (same window only)
ipcRenderer.on('hl:net-port', (e) => { if (e.ports[0]) window.postMessage({ hlNetPort: true }, '*', [e.ports[0]]); });
