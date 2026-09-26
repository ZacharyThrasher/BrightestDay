import { DEFAULT_SENS, legacyMultiplierToSource } from './mouse.js';
import { HARDLIGHT_COLOR, type Quality } from '../render/config.js';

export interface Settings {
  mouseSensitivity: number; // Source/Quake units (sens × 0.022° per count)
  /** marks mouseSensitivity as Source units (settings saved before this were a multiplier) */
  sensUnits?: 'source';
  invertY: boolean;
  fov: number;
  constructColor: string;
  graphics: Quality;
  /** low-latency canvas (lower mouse-to-screen delay, may tear); default on */
  lowLatency: boolean;
  /** World speed while the ring console is open (1 = focus time off). */
  focusTimeScale: number;
  /** Real seconds of slowed time in a full focus meter. */
  focusSeconds: number;
}

export const DEFAULT_SETTINGS: Settings = {
  mouseSensitivity: DEFAULT_SENS,
  sensUnits: 'source',
  invertY: false,
  fov: 78,
  constructColor: HARDLIGHT_COLOR,
  graphics: 'high',
  lowLatency: true,
  // focus time is an offline-practice aid (duels share one clock): off by default
  focusTimeScale: 1,
  focusSeconds: 5,
};

/** Bridge exposed by the Electron preload (absent in the plain web build). */
export interface HardlightBridge {
  isElectron?: boolean;
  settings?: Partial<Settings>;
  saveSettings?: (s: Partial<Settings>) => void;
  quit?: () => void;
  ready?: () => void;
  fatal?: (msg: string) => void;
  toggleFullscreen?: () => void;
  s2?: {
    info(): Promise<{ enabled: boolean; name: string }>;
    complete(req: unknown, timeoutMs: number): Promise<{ ok: boolean; text?: string | null; error?: string; ms?: number }>;
    mode(): 'off' | 'hybrid' | 'always';
  };
  semantic?: () => Promise<Uint8Array | null>;
}

declare global {
  interface Window { hardlight?: HardlightBridge }
}

function sanitize(s: Partial<Settings>): Partial<Settings> {
  const out: Partial<Settings> = {};
  if (typeof s.mouseSensitivity === 'number' && isFinite(s.mouseSensitivity)) {
    const v = s.sensUnits === 'source' ? s.mouseSensitivity : legacyMultiplierToSource(s.mouseSensitivity); // migrate old multiplier saves
    out.mouseSensitivity = Math.min(40, Math.max(0.1, v));
    out.sensUnits = 'source';
  }
  if (typeof s.invertY === 'boolean') out.invertY = s.invertY;
  if (typeof s.fov === 'number' && isFinite(s.fov)) out.fov = Math.min(110, Math.max(50, s.fov));
  if (typeof s.constructColor === 'string' && /^#[0-9a-fA-F]{6}$/.test(s.constructColor)) out.constructColor = s.constructColor;
  if (s.graphics === 'low' || s.graphics === 'high') out.graphics = s.graphics;
  if (typeof s.lowLatency === 'boolean') out.lowLatency = s.lowLatency;
  if ((s.focusTimeScale as unknown) === false || (s.focusTimeScale as unknown) === 'off') out.focusTimeScale = 1;
  else if (typeof s.focusTimeScale === 'number' && isFinite(s.focusTimeScale)) out.focusTimeScale = Math.min(1, Math.max(0.1, s.focusTimeScale));
  if (typeof s.focusSeconds === 'number' && isFinite(s.focusSeconds)) out.focusSeconds = Math.min(30, Math.max(1, s.focusSeconds));
  return out;
}

/** Precedence: defaults < settings.json (Electron) < localStorage (web) < URL query (?graphics=low&fov=90). */
export function loadSettings(): Settings {
  let s: Settings = { ...DEFAULT_SETTINGS };
  if (window.hardlight?.settings) s = { ...s, ...sanitize(window.hardlight.settings) };
  try {
    const raw = localStorage.getItem('hardlight.settings');
    if (raw && !window.hardlight?.isElectron) s = { ...s, ...sanitize(JSON.parse(raw)) };
  } catch { /* storage may be unavailable */ }
  const q = new URLSearchParams(location.search);
  const qs: any = {};
  if (q.has('graphics')) qs.graphics = q.get('graphics');
  if (q.has('quality')) qs.graphics = q.get('quality');
  if (q.has('fov')) qs.fov = Number(q.get('fov'));
  if (q.has('sens')) { qs.mouseSensitivity = Number(q.get('sens')); qs.sensUnits = 'source'; }
  if (q.has('invertY')) qs.invertY = q.get('invertY') === '1' || q.get('invertY') === 'true';
  if (q.has('color')) qs.constructColor = '#' + q.get('color')!.replace('#', '');
  if (q.has('focus')) qs.focusTimeScale = q.get('focus') === 'off' ? 1 : Number(q.get('focus'));
  return { ...s, ...sanitize(qs) };
}

export function saveSettings(s: Settings) {
  if (window.hardlight?.saveSettings) { window.hardlight.saveSettings(s); return; }
  try { localStorage.setItem('hardlight.settings', JSON.stringify(s)); } catch { /* ignore */ }
}
