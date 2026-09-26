import { HARDLIGHT_COLOR, type Quality } from '../render/config.js';

export interface Settings {
  mouseSensitivity: number; // 1 = default
  invertY: boolean;
  fov: number;
  constructColor: string;
  graphics: Quality;
}

export const DEFAULT_SETTINGS: Settings = {
  mouseSensitivity: 1,
  invertY: false,
  fov: 78,
  constructColor: HARDLIGHT_COLOR,
  graphics: 'high',
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
  voice?: {
    start?: () => void;
    stop?: () => void;
    onPartial?: (cb: (text: string) => void) => void;
    onFinal?: (cb: (text: string) => void) => void;
  };
}

declare global {
  interface Window { hardlight?: HardlightBridge }
}

function sanitize(s: Partial<Settings>): Partial<Settings> {
  const out: Partial<Settings> = {};
  if (typeof s.mouseSensitivity === 'number' && isFinite(s.mouseSensitivity)) out.mouseSensitivity = Math.min(10, Math.max(0.05, s.mouseSensitivity));
  if (typeof s.invertY === 'boolean') out.invertY = s.invertY;
  if (typeof s.fov === 'number' && isFinite(s.fov)) out.fov = Math.min(110, Math.max(50, s.fov));
  if (typeof s.constructColor === 'string' && /^#[0-9a-fA-F]{6}$/.test(s.constructColor)) out.constructColor = s.constructColor;
  if (s.graphics === 'low' || s.graphics === 'high') out.graphics = s.graphics;
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
  if (q.has('sens')) qs.mouseSensitivity = Number(q.get('sens'));
  if (q.has('invertY')) qs.invertY = q.get('invertY') === '1' || q.get('invertY') === 'true';
  if (q.has('color')) qs.constructColor = '#' + q.get('color')!.replace('#', '');
  return { ...s, ...sanitize(qs) };
}

export function saveSettings(s: Settings) {
  if (window.hardlight?.saveSettings) { window.hardlight.saveSettings(s); return; }
  try { localStorage.setItem('hardlight.settings', JSON.stringify(s)); } catch { /* ignore */ }
}
