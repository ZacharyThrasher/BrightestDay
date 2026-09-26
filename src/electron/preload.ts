/**
 * Preload (sandboxed, context-isolated). Exposes `window.hardlight` to the renderer:
 *   isElectron, settings, saveSettings(), toggleFullscreen(), quit(),
 *   voice.start()/stop()/onPartial(cb)/onFinal(cb)   ← IPC placeholder for the voice integrator
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
  voice: {
    start: () => ipcRenderer.send('hl:voice-start'),
    stop: () => ipcRenderer.send('hl:voice-stop'),
    onPartial: (cb: (text: string) => void) => { ipcRenderer.on('hl:voice-partial', (_e, text: string) => cb(String(text))); },
    onFinal: (cb: (text: string) => void) => { ipcRenderer.on('hl:voice-final', (_e, text: string) => cb(String(text))); },
  },
});
