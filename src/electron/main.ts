/**
 * HARDLIGHT — Electron main process.
 * One window, local files only, context isolation on. The preload exposes `window.hardlight`
 * (settings + a voice IPC placeholder the voice integrator fills in).
 *
 * Flags:
 *   --smoke-test[=ms]  quit automatically after the renderer reports ready (+ms, default 3000); exit 1 on failure
 *   --scene=<name>     start scene
 *   --graphics=low     override quality
 *   --software-gl      force SwiftShader WebGL (VMs / machines with a blocklisted GPU)
 *   --smoke-say=<text> with --smoke-test: send one utterance to the ring and log the result
 */
import { app, BrowserWindow, ipcMain, Menu, session, shell } from 'electron';
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { installS2Bridge, type S2Settings } from './s2-main.js';

interface Settings { mouseSensitivity?: number; invertY?: boolean; fov?: number; constructColor?: string; graphics?: 'low' | 'high'; system2?: S2Settings & { mode?: 'off' | 'hybrid' | 'always' }; [k: string]: unknown }

const argv = process.argv.slice(1);
const flag = (name: string) => argv.find((a) => a === `--${name}` || a.startsWith(`--${name}=`));
const flagVal = (name: string) => flag(name)?.split('=')[1];
const smoke = flag('smoke-test');
const smokeMs = Number(flagVal('smoke-test') ?? 3000) || 3000;

// Headless CI / VMs without a GPU: --software-gl (implied by --smoke-test) forces SwiftShader WebGL.
if (smoke || flag('software-gl')) {
  app.commandLine.appendSwitch('ignore-gpu-blocklist');
  app.commandLine.appendSwitch('use-angle', 'swiftshader');
  app.commandLine.appendSwitch('enable-unsafe-swiftshader');
}

/** settings.json locations, lowest precedence first. */
function settingsPaths(): string[] {
  const paths = [join(app.getPath('userData'), 'settings.json'), join(dirname(app.getPath('exe')), 'settings.json')];
  if (!app.isPackaged) paths.push(join(process.cwd(), 'settings.json'));
  return paths;
}

function loadSettings(): Settings {
  let s: Settings = {};
  for (const p of settingsPaths()) {
    try {
      if (existsSync(p)) s = { ...s, ...JSON.parse(readFileSync(p, 'utf8')) };
    } catch (err) {
      console.warn(`[hardlight] ignoring malformed ${p}:`, (err as Error).message);
    }
  }
  const g = flagVal('graphics');
  if (g === 'low' || g === 'high') s.graphics = g;
  return s;
}

function saveSettings(s: Settings) {
  const p = join(app.getPath('userData'), 'settings.json');
  try { mkdirSync(dirname(p), { recursive: true }); writeFileSync(p, JSON.stringify(s, null, 2)); } catch (err) { console.warn('[hardlight] could not save settings', err); }
}

let win: BrowserWindow | null = null;
let settings: Settings = {};

function createWindow() {
  win = new BrowserWindow({
    width: 1600,
    height: 900,
    minWidth: 960,
    minHeight: 540,
    backgroundColor: '#9fc3f0',
    title: 'HARDLIGHT',
    show: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
      spellcheck: false,
      backgroundThrottling: false,
    },
  });
  win.once('ready-to-show', () => win?.show());
  const scene = flagVal('scene');
  win.loadFile(join(__dirname, 'index.html'), { query: scene ? { scene } : {} });

  // no remote content, no new windows, no navigation away from the bundle
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:/.test(url)) void shell.openExternal(url);
    return { action: 'deny' };
  });
  win.webContents.on('will-navigate', (e, url) => { if (!url.startsWith('file://')) e.preventDefault(); });
  win.webContents.on('before-input-event', (e, input) => {
    if (input.type === 'keyDown' && input.key === 'F11') { win?.setFullScreen(!win.isFullScreen()); e.preventDefault(); }
    if (input.type === 'keyDown' && input.key === 'F12' && !app.isPackaged) win?.webContents.toggleDevTools();
  });
  win.webContents.on('console-message', (...args: any[]) => {
    // Electron ≥ 35 passes a details object; older versions pass (event, level, message)
    const d = args[0]?.message !== undefined ? args[0] : { level: args[1], message: args[2] };
    if (smoke || !app.isPackaged) console.log(`[renderer:${d.level}] ${d.message}`);
  });
  win.webContents.on('render-process-gone', (_e, details) => {
    console.error('[hardlight] renderer gone', details);
    if (smoke) app.exit(1);
  });
  win.on('closed', () => { win = null; });
}

// ---- IPC -------------------------------------------------------------------------------------------
ipcMain.on('hl:get-settings', (e) => { e.returnValue = settings; });
ipcMain.on('hl:save-settings', (_e, s: Settings) => { settings = { ...settings, ...s }; saveSettings(settings); });
ipcMain.on('hl:toggle-fullscreen', () => win?.setFullScreen(!win.isFullScreen()));
ipcMain.on('hl:quit', () => app.quit());
ipcMain.on('hl:fatal', (_e, msg: string) => {
  console.error('[hardlight] renderer failed to start:', msg);
  if (smoke) app.exit(1);
});
ipcMain.on('hl:ready', () => {
  console.log('[hardlight] renderer ready');
  // --smoke-say="text": commit one utterance through the full pipeline and log the outcome
  const say = argv.find((a) => a.startsWith('--smoke-say='))?.slice('--smoke-say='.length);
  if (say && win) {
    setTimeout(() => {
      win?.webContents.executeJavaScript(`hardlightApp.commit(${JSON.stringify(say)}).then((r) => r && JSON.stringify({ source: r.source, summary: r.summary, applied: r.applied, ms: Math.round(r.latencyMs) }))`)
        .then((r) => console.log('[hardlight] smoke-say →', r)).catch((e) => console.error('[hardlight] smoke-say failed', e));
    }, 1500);
  }
  if (smoke) setTimeout(() => { console.log('[hardlight] smoke test OK'); app.exit(0); }, smokeMs);
});
// Voice placeholder: the voice integrator replaces these with the real ASR pipeline and sends
// 'hl:voice-partial' / 'hl:voice-final' (string) to the renderer.
ipcMain.on('hl:voice-start', () => console.log('[hardlight] voice start requested (voice pipeline not wired yet)'));
ipcMain.on('hl:voice-stop', () => console.log('[hardlight] voice stop requested'));

/** Bundled model files (read through asar; native code needs the .unpacked path, see unpackedPath). */
const modelsRoot = () => join(app.getAppPath(), 'models');

// System 1.5 word vectors are optional: the interpreter works without them (see docs/EXPERIMENTS.md §5)
ipcMain.handle('hl:semantic', () => {
  const p = join(modelsRoot(), 'semantic', 'bd-semantic-nb19-int8.bin');
  try { return existsSync(p) ? readFileSync(p) : null; } catch { return null; }
});

app.whenReady().then(() => {
  settings = loadSettings();
  installS2Bridge(() => settings.system2);
  ipcMain.on('hl:s2-mode', (e) => { e.returnValue = settings.system2?.mode ?? 'hybrid'; });
  Menu.setApplicationMenu(null);
  // deny every permission request except the microphone (for push-to-talk)
  session.defaultSession.setPermissionRequestHandler((_wc, permission, cb) => cb(permission === 'media'));
  // block any non-file network request (defence in depth on top of the CSP)
  session.defaultSession.webRequest.onBeforeRequest((details, cb) => {
    const ok = details.url.startsWith('file://') || details.url.startsWith('devtools://') || details.url.startsWith('data:') || details.url.startsWith('blob:');
    cb({ cancel: !ok });
  });
  createWindow();
  if (smoke) setTimeout(() => { console.error('[hardlight] smoke test timed out (renderer never became ready)'); app.exit(1); }, 45000);
});

app.on('window-all-closed', () => app.quit());
