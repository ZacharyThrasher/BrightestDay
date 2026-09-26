/**
 * Preload (sandboxed, context-isolated). Exposes `window.hardlight` to the renderer:
 *   isElectron, settings, saveSettings(), toggleFullscreen(), quit(),
 *   s2.info()/complete()/mode()  ← optional System-2 interpreter (key stays in the main process)
 *   semantic()                   ← optional word-vector file bytes (or null)
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
});
