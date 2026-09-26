/**
 * Visual constants for HARDLIGHT. Everything that defines "the look" lives here so it can be
 * tuned in one place (and overridden from settings.json where it makes sense).
 */

/** The single hard-light construct colour (luminous emerald-mint). Overridable via settings.constructColor. */
export const HARDLIGHT_COLOR = '#3dffa8';

export type Quality = 'low' | 'high';

export interface RenderOptions {
  quality: Quality;
  fov: number; // base vertical FOV in degrees
  constructColor: string;
  pixelRatio?: number; // override devicePixelRatio (tests)
}

export interface QualityPreset {
  msaa: number;
  shadows: boolean;
  shadowMapSize: number;
  bloomLevels: number;
  maxPixelRatio: number;
  clouds: boolean;
}

export const QUALITY: Record<Quality, QualityPreset> = {
  high: { msaa: 4, shadows: true, shadowMapSize: 2048, bloomLevels: 5, maxPixelRatio: 2, clouds: true },
  low: { msaa: 0, shadows: false, shadowMapSize: 1024, bloomLevels: 4, maxPixelRatio: 1, clouds: false },
};

/** Soft pastel daylight. Colours are sRGB hex; converted to linear where used. */
export const SKY = {
  zenith: '#6fa8ff',
  horizon: '#ffd6c4',
  ground: '#c9b7c4',
  sun: '#fff1d6',
  sunDir: [0.78, 0.52, -0.2] as [number, number, number], // normalised in code
  fogStart: 70,
  fogDensity: 0.0017,
  fogHeightFalloff: 0.006,
};

export const LIGHT = {
  sunIntensity: 2.35,
  hemiSky: '#c7dcff',
  hemiGround: '#b59a8a',
  hemiIntensity: 0.95,
};

/** Toon ink used for outlines / baked edge lines. */
export const INK = '#1c1a2e';

/** Player suit colours (original design: deep navy suit, pale panels, mint emitters). */
export const SUIT = {
  base: '#1f2740',
  panel: '#e9edf5',
  accent: '#2d3a5e',
  skin: '#f0c7a6',
  hair: '#2a1d1a',
  visor: '#9ffff0',
};

/**
 * The rival lantern: same hero, a warm plum suit and a magenta ring, and their constructs glow in the ring's
 * colour so "his tank" reads at a glance next to yours.
 */
export const RIVAL = {
  ring: '#ff5fb0',
  suit: { base: '#3b1f36', accent: '#62294f', visor: '#ffc9a8' } as Partial<typeof SUIT>,
};

export const ENTITY_COLORS = {
  droneBody: '#394056',
  droneTrim: '#f2f4f8',
  droneEye: '#ff4d5e',
  turretBase: '#50576e',
  turretHead: '#e7e3dc',
  civilianShirt: '#ff8a5c',
  civilianPants: '#3b5b92',
  boulder: '#8d8173',
  crate: '#d99a55',
  dummy: '#e7d7b0',
  dummyTarget: '#e2524f',
  hostileGlow: '#ff3b52',
  projectile: '#ff6a3d',
  reflected: '#3dffa8',
};

export const RING_COLOR = '#ffc55c';
export const RING_NEXT_COLOR = '#ff7ad9';
export const GOAL_COLOR = '#7fd3ff';

/** Named tints the language layer may put in spec.color. */
export const TINTS: Record<string, string> = {
  green: '#3dffa8', mint: '#3dffa8', emerald: '#2dff8a', red: '#ff4d5e', crimson: '#ff3355', blue: '#4da6ff', cyan: '#3df5ff',
  teal: '#2de0c8', purple: '#b36bff', violet: '#9d6bff', pink: '#ff6bd5', magenta: '#ff4dd8', orange: '#ff9b3d', yellow: '#ffe84d',
  gold: '#ffc94d', white: '#f2fff8', silver: '#d8e6ff',
};

export function tintFor(name: string | undefined, fallback: string): string {
  if (!name) return fallback;
  if (name.startsWith('#')) return name;
  return TINTS[name.toLowerCase()] ?? fallback;
}
