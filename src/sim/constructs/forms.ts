import type { Part, Vec3 } from './spec.js';

/**
 * Parametric geometry vocabulary. Each form builds parts at a natural size in metres.
 * Local frame: +z forward (away from the player / toward the target), +y up, +x right.
 *
 * Forms are *shapes only*. What a form does comes from behaviours, which the semantic layer
 * composes independently — a "hammer" and a "wall" can both orbit, strike, launch or be thrown.
 */

const P = (shape: Part['shape'], size: Vec3, pos: Vec3 = [0, 0, 0], role?: string, rot?: Vec3, solid = true): Part =>
  ({ shape, size, pos, ...(rot ? { rot } : {}), ...(role ? { role } : {}), ...(solid ? {} : { solid: false }) });

type FormFn = () => Part[];

function ringOf(n: number, radius: number, make: (angleDeg: number, x: number, z: number) => Part): Part[] {
  const out: Part[] = [];
  for (let i = 0; i < n; i++) {
    const a = (i / n) * Math.PI * 2;
    out.push(make((a * 180) / Math.PI, Math.cos(a) * radius, Math.sin(a) * radius));
  }
  return out;
}

export const FORMS: Record<string, FormFn> = {
  block: () => [P('box', [1.2, 1.2, 1.2])],
  cube: () => [P('box', [1.5, 1.5, 1.5])],
  ball: () => [P('sphere', [1.4, 1.4, 1.4])],
  boulder: () => [P('sphere', [3, 2.6, 3])],
  wall: () => [P('box', [7, 4.5, 0.5], [0, 2.25, 0], 'body')],
  panel: () => [P('box', [3, 3, 0.25], [0, 0, 0], 'body')],
  shield: () => [P('cylinder', [3.2, 0.3, 3.2], [0, 0, 0], 'body', [90, 0, 0]), P('torus', [3.4, 0.3, 3.4], [0, 0, 0.05], 'rim', [90, 0, 0])],
  platform: () => [P('box', [5, 0.5, 5], [0, 0, 0], 'body')],
  slab: () => [P('box', [4, 0.5, 8], [0, 0, 0], 'body')],
  ramp: () => [P('wedge', [5, 3, 10], [0, 1.5, 0], 'body')],
  bridge: () => [P('box', [4, 0.4, 10], [0, 0, 0], 'deck'), P('box', [0.2, 1, 10], [-1.9, 0.7, 0], 'rail'), P('box', [0.2, 1, 10], [1.9, 0.7, 0], 'rail')],
  stairs: () => {
    const parts: Part[] = [P('wedge', [3, 4, 6], [0, 2, 0], 'collider')];
    for (let i = 0; i < 8; i++) parts.push(P('box', [3, 0.5, 0.75], [0, 0.25 + i * 0.5, -3 + 0.375 + i * 0.75], 'step', undefined, false));
    return parts;
  },
  ladder: () => {
    const parts: Part[] = [P('box', [0.15, 8, 0.15], [-0.5, 4, 0], 'rail'), P('box', [0.15, 8, 0.15], [0.5, 4, 0], 'rail')];
    for (let i = 0; i < 12; i++) parts.push(P('cylinder', [0.1, 1, 0.1], [0, 0.4 + i * 0.65, 0], 'rung', [0, 0, 90]));
    parts.push(P('box', [1.2, 8, 0.1], [0, 4, -0.1], 'climb'));
    return parts;
  },
  pillar: () => [P('cylinder', [1.6, 8, 1.6], [0, 4, 0], 'body')],
  tower: () => [P('box', [3, 16, 3], [0, 8, 0], 'body'), P('box', [4, 0.5, 4], [0, 16.25, 0], 'top')],
  pole: () => [P('cylinder', [0.3, 10, 0.3], [0, 5, 0], 'body')],
  post: () => [P('cylinder', [0.6, 6, 0.6], [0, 3, 0], 'body'), P('sphere', [0.9, 0.9, 0.9], [0, 6.2, 0], 'anchor')],
  anchor: () => [P('sphere', [1, 1, 1], [0, 0, 0], 'anchor'), P('torus', [1.6, 0.2, 1.6], [0, 0, 0], 'ring', [90, 0, 0])],
  fist: () => [
    P('box', [0.9, 0.8, 0.8], [0, 0, 0.25], 'knuckles'),
    P('capsule', [0.3, 0.45, 0.3], [0.5, -0.1, 0.1], 'thumb', [90, 0, 20]),
    P('cylinder', [0.65, 0.6, 0.65], [0, 0, -0.4], 'cuff', [90, 0, 0]),
  ],
  glove: () => [
    P('sphere', [1.2, 1.05, 1.25], [0, 0, 0.2], 'knuckles'),
    P('capsule', [0.4, 0.5, 0.4], [0.62, -0.1, 0.1], 'thumb', [70, 0, 25]),
    P('cylinder', [0.8, 0.7, 0.8], [0, 0, -0.55], 'cuff', [90, 0, 0]),
  ],
  hand: () => {
    const parts: Part[] = [P('box', [1.1, 0.35, 1.1], [0, 0, 0], 'palm')];
    for (let i = 0; i < 4; i++) parts.push(P('capsule', [0.22, 0.7, 0.22], [-0.39 + i * 0.26, 0.05, 1.0], 'finger', [80, 0, 0]));
    parts.push(P('capsule', [0.25, 0.55, 0.25], [0.7, 0.05, 0.3], 'thumb', [80, 0, -40]));
    return parts;
  },
  claw: () => {
    const parts: Part[] = [P('cylinder', [1, 0.4, 1], [0, 0, 0], 'palm', [90, 0, 0])];
    for (let i = 0; i < 3; i++) {
      const a = (i / 3) * Math.PI * 2;
      parts.push(P('capsule', [0.2, 1.2, 0.2], [Math.cos(a) * 0.45, Math.sin(a) * 0.45, 0.8], 'finger', [90 - Math.sin(a) * 25, Math.cos(a) * 25, 0]));
    }
    return parts;
  },
  foot: () => [P('box', [0.8, 0.6, 1.6], [0, 0, 0.3], 'sole'), P('cylinder', [0.7, 1, 0.7], [0, 0.6, -0.2], 'ankle')],
  hammer: () => [P('cylinder', [0.22, 2.4, 0.22], [0, 0, -1.2], 'handle', [90, 0, 0]), P('box', [1.8, 0.9, 0.9], [0, 0, 0.2], 'head')],
  mallet: () => [P('cylinder', [0.2, 2, 0.2], [0, 0, -1], 'handle', [90, 0, 0]), P('cylinder', [1, 1.6, 1], [0, 0, 0.2], 'head', [0, 0, 90])],
  sword: () => [P('box', [0.12, 0.35, 3], [0, 0, 0.4], 'blade'), P('box', [0.8, 0.15, 0.15], [0, 0, -1.15], 'guard'), P('cylinder', [0.16, 0.6, 0.16], [0, 0, -1.5], 'handle', [90, 0, 0])],
  axe: () => [P('cylinder', [0.18, 2.2, 0.18], [0, 0, -0.9], 'handle', [90, 0, 0]), P('wedge', [0.2, 1.1, 0.9], [0, 0.45, 0.2], 'blade', [0, 0, 0])],
  bat: () => [P('cone', [0.55, 2.4, 0.55], [0, 0, 0], 'body', [-90, 0, 0])],
  club: () => [P('cone', [0.8, 2, 0.8], [0, 0, 0], 'body', [-90, 0, 0]), P('sphere', [0.8, 0.8, 0.8], [0, 0, 1], 'head')],
  spear: () => [P('cylinder', [0.14, 4, 0.14], [0, 0, 0], 'shaft', [90, 0, 0]), P('cone', [0.35, 0.8, 0.35], [0, 0, 2.4], 'tip', [90, 0, 0])],
  arrow: () => [P('cylinder', [0.08, 1.6, 0.08], [0, 0, 0], 'shaft', [90, 0, 0]), P('cone', [0.2, 0.35, 0.2], [0, 0, 0.95], 'tip', [90, 0, 0]), P('box', [0.4, 0.02, 0.3], [0, 0, -0.7], 'fletch', undefined, false)],
  whip: () => { const p: Part[] = []; for (let i = 0; i < 10; i++) p.push(P('sphere', [0.18, 0.18, 0.18], [0, 0, i * 0.5], 'segment')); return p; },
  cage: () => {
    const parts: Part[] = [P('torus', [3.2, 0.2, 3.2], [0, 0, 0], 'ring'), P('torus', [3.2, 0.2, 3.2], [0, 3.2, 0], 'ring')];
    parts.push(...ringOf(10, 1.5, (_a, x, z) => P('cylinder', [0.14, 3.2, 0.14], [x, 1.6, z], 'bar')));
    parts.push(P('box', [3, 0.15, 3], [0, 3.25, 0], 'lid'), P('box', [3, 0.15, 3], [0, -0.05, 0], 'floor'));
    return parts;
  },
  net: () => {
    const parts: Part[] = [];
    for (let i = 0; i < 6; i++) {
      parts.push(P('box', [0.08, 0.08, 6], [-2.5 + i, 0, 0], 'strand'));
      parts.push(P('box', [6, 0.08, 0.08], [0, 0, -2.5 + i], 'strand'));
    }
    parts.push(P('box', [6, 0.1, 6], [0, 0, 0], 'mesh', undefined, true));
    return parts;
  },
  dome: () => [P('dome', [6, 3.2, 6], [0, 0, 0], 'shell')],
  bubble: () => [P('sphere', [3.4, 3.4, 3.4], [0, 0, 0], 'shell')],
  sphere_shell: () => [P('sphere', [4, 4, 4], [0, 0, 0], 'shell')],
  spring: () => {
    const parts: Part[] = [P('cylinder', [2.4, 0.3, 2.4], [0, 1.5, 0], 'pad'), P('cylinder', [2.2, 0.2, 2.2], [0, 0.1, 0], 'base')];
    for (let i = 0; i < 4; i++) parts.push(P('torus', [1.5, 0.15, 1.5], [0, 0.35 + i * 0.3, 0], 'coil', undefined, false));
    return parts;
  },
  trampoline: () => {
    const parts: Part[] = [P('cylinder', [5, 0.2, 5], [0, 1, 0], 'pad'), P('torus', [5.2, 0.3, 5.2], [0, 1, 0], 'rim')];
    parts.push(...ringOf(6, 2.4, (_a, x, z) => P('cylinder', [0.2, 1, 0.2], [x, 0.5, z], 'leg')));
    return parts;
  },
  launchpad: () => [P('cylinder', [3.2, 0.3, 3.2], [0, 0.15, 0], 'pad'), P('cone', [0.9, 0.6, 0.9], [0, 0.5, 0], 'chevron', [0, 0, 0], false)],
  catapult: () => [
    P('box', [3, 0.8, 4], [0, 0.4, 0], 'base'),
    P('box', [0.4, 0.4, 5], [0, 1.6, -0.5], 'arm', [-25, 0, 0]),
    P('box', [1.6, 0.4, 1.6], [0, 2.8, -2.7], 'pad', [-25, 0, 0]),
  ],
  cannon: () => [P('cylinder', [1.1, 3.2, 1.1], [0, 1.2, 0.4], 'barrel', [80, 0, 0]), P('box', [1.6, 0.8, 2], [0, 0.4, -0.3], 'base'), P('cylinder', [1.2, 0.3, 1.2], [-1, 0.6, -0.3], 'wheel', [0, 0, 90]), P('cylinder', [1.2, 0.3, 1.2], [1, 0.6, -0.3], 'wheel', [0, 0, 90])],
  turret: () => [P('cylinder', [1.6, 1.2, 1.6], [0, 0.6, 0], 'base'), P('sphere', [1.3, 1.3, 1.3], [0, 1.5, 0], 'head'), P('cylinder', [0.35, 1.8, 0.35], [0, 1.5, 0.9], 'barrel', [90, 0, 0])],
  blaster: () => [P('box', [0.4, 0.5, 1.2], [0, 0, 0], 'body'), P('cylinder', [0.25, 0.9, 0.25], [0, 0.05, 0.9], 'barrel', [90, 0, 0])],
  bow: () => [P('torus', [2.4, 0.12, 2.4], [0, 0, -0.6], 'limb', [0, 0, 90]), P('box', [0.04, 2.3, 0.04], [0, 0, -0.1], 'string', undefined, false)],
  hook: () => [P('cylinder', [0.2, 1.2, 0.2], [0, 0, 0], 'shank', [90, 0, 0]), P('torus', [0.9, 0.18, 0.9], [0, -0.35, 0.6], 'bend', [0, 90, 0])],
  rope: () => [P('sphere', [0.5, 0.5, 0.5], [0, 0, 0], 'knot')],
  wheel: () => [P('cylinder', [2.4, 0.6, 2.4], [0, 0, 0], 'wheel', [0, 0, 90]), P('torus', [2.5, 0.3, 2.5], [0, 0, 0], 'tire', [0, 0, 90], false)],
  gear: () => [P('cylinder', [2.6, 0.4, 2.6], [0, 0, 0], 'disc', [90, 0, 0]), ...ringOf(10, 1.4, (a, x, y) => P('box', [0.45, 0.45, 0.4], [x, y, 0], 'tooth', [0, 0, a]))],
  saw: () => [P('cylinder', [3, 0.12, 3], [0, 0, 0], 'disc', [90, 0, 0]), ...ringOf(16, 1.55, (a, x, y) => P('cone', [0.25, 0.4, 0.1], [x, y, 0], 'tooth', [0, 0, a - 90], false))],
  drill: () => [P('cone', [1.2, 2.6, 1.2], [0, 0, 1.3], 'bit', [90, 0, 0]), P('cylinder', [1.4, 1.2, 1.4], [0, 0, -0.5], 'body', [90, 0, 0])],
  fan: () => [P('cylinder', [0.6, 0.5, 0.6], [0, 0, 0], 'hub', [90, 0, 0]), ...ringOf(4, 1.1, (a, x, y) => P('box', [0.5, 1.8, 0.08], [x, y, 0], 'blade', [0, 12, a - 90])), P('torus', [4.6, 0.15, 4.6], [0, 0, 0], 'guard', [90, 0, 0], false)],
  propeller: () => [P('cylinder', [0.4, 0.4, 0.4], [0, 0, 0], 'hub', [90, 0, 0]), P('box', [3.6, 0.4, 0.08], [0, 0, 0.1], 'blade', [0, 10, 0])],
  magnet: () => [P('box', [0.6, 2, 0.6], [-0.9, 0, 0], 'arm'), P('box', [0.6, 2, 0.6], [0.9, 0, 0], 'arm'), P('box', [2.4, 0.6, 0.6], [0, -1.2, 0], 'base'), P('box', [0.62, 0.4, 0.62], [-0.9, 1.0, 0], 'pole', undefined, false), P('box', [0.62, 0.4, 0.62], [0.9, 1.0, 0], 'pole', undefined, false)],
  funnel: () => [P('cone', [3.5, 3, 3.5], [0, 0, 0], 'mouth', [-90, 0, 0])],
  anvil: () => [P('box', [2.6, 0.9, 1.3], [0, 0.9, 0], 'top'), P('box', [1.2, 0.9, 0.9], [0, 0.2, 0], 'waist'), P('box', [2, 0.4, 1.4], [0, -0.35, 0], 'foot'), P('cone', [0.7, 1, 0.7], [1.7, 0.9, 0], 'horn', [0, 0, -90])],
  weight: () => [P('box', [2.4, 1.6, 2.4], [0, 0, 0], 'body'), P('torus', [1, 0.25, 1], [0, 1.0, 0], 'handle', [90, 0, 0])],
  wrecking_ball: () => [P('sphere', [3, 3, 3], [0, 0, 0], 'ball'), P('cylinder', [0.2, 8, 0.2], [0, 5.5, 0], 'chain', undefined, false)],
  pendulum: () => [P('sphere', [2.4, 2.4, 2.4], [0, 0, 0], 'bob'), P('cylinder', [0.2, 6, 0.2], [0, 4, 0], 'rod')],
  car: () => [
    P('box', [2.2, 0.9, 4.2], [0, 0.8, 0], 'body'), P('box', [1.9, 0.8, 2], [0, 1.6, -0.3], 'cabin'),
    P('cylinder', [0.9, 0.4, 0.9], [-1.1, 0.45, 1.3], 'wheel', [0, 0, 90]), P('cylinder', [0.9, 0.4, 0.9], [1.1, 0.45, 1.3], 'wheel', [0, 0, 90]),
    P('cylinder', [0.9, 0.4, 0.9], [-1.1, 0.45, -1.3], 'wheel', [0, 0, 90]), P('cylinder', [0.9, 0.4, 0.9], [1.1, 0.45, -1.3], 'wheel', [0, 0, 90]),
  ],
  board: () => [P('capsule', [0.7, 2.2, 0.7], [0, 0, 0], 'deck', [90, 0, 0])].map((p) => ({ ...p, size: [0.8, 1.6, 0.18] as Vec3, shape: 'box' as const, rot: undefined })),
  skateboard: () => [P('box', [0.6, 0.1, 1.8], [0, 0.25, 0], 'deck'), P('cylinder', [0.25, 0.5, 0.25], [0, 0.1, 0.6], 'wheel', [0, 0, 90]), P('cylinder', [0.25, 0.5, 0.25], [0, 0.1, -0.6], 'wheel', [0, 0, 90])],
  surfboard: () => [P('capsule', [0.7, 1.8, 0.12], [0, 0, 0], 'deck', [90, 0, 0])],
  wings: () => [P('wedge', [3.2, 0.12, 1.4], [-1.8, 0.2, 0], 'wing', [0, 0, 8]), P('wedge', [3.2, 0.12, 1.4], [1.8, 0.2, 0], 'wing', [0, 0, -8]), P('box', [0.5, 0.8, 0.3], [0, 0, 0], 'spine')],
  glider: () => [P('wedge', [7, 0.15, 2.4], [0, 1.6, 0], 'wing'), P('cylinder', [0.08, 1.6, 0.08], [0, 0.8, 0], 'strut', undefined, false)],
  jetpack: () => [P('cylinder', [0.45, 1, 0.45], [-0.3, 0, -0.35], 'thruster'), P('cylinder', [0.45, 1, 0.45], [0.3, 0, -0.35], 'thruster'), P('cone', [0.35, 0.5, 0.35], [-0.3, -0.75, -0.35], 'flame', [180, 0, 0], false), P('cone', [0.35, 0.5, 0.35], [0.3, -0.75, -0.35], 'flame', [180, 0, 0], false)],
  rocket: () => [P('cylinder', [0.9, 3.2, 0.9], [0, 0, 0], 'body', [90, 0, 0]), P('cone', [0.9, 1, 0.9], [0, 0, 2.1], 'nose', [90, 0, 0]), P('box', [1.8, 0.08, 0.8], [0, 0, -1.3], 'fin'), P('box', [0.08, 1.8, 0.8], [0, 0, -1.3], 'fin')],
  thruster: () => [P('cylinder', [0.45, 0.8, 0.45], [0, 0, 0], 'thruster', [90, 0, 0]), P('cone', [0.4, 0.6, 0.4], [0, 0, -0.7], 'flame', [-90, 0, 0], false)],
  plane: () => [P('capsule', [1.1, 5, 1.1], [0, 0, 0], 'fuselage', [90, 0, 0]), P('box', [8, 0.15, 1.6], [0, 0, 0.3], 'wing'), P('box', [2.6, 0.12, 0.9], [0, 0, -2.6], 'tail'), P('box', [0.12, 1.2, 0.9], [0, 0.6, -2.6], 'fin')],
  helicopter: () => [P('sphere', [2.4, 2, 3], [0, 0, 0], 'cabin'), P('cylinder', [0.3, 3.5, 0.3], [0, 0.3, -2.6], 'tail', [90, 0, 0]), P('box', [7, 0.08, 0.35], [0, 1.4, 0], 'rotor'), P('box', [0.35, 0.08, 7], [0, 1.4, 0], 'rotor')],
  boat: () => [P('wedge', [2.2, 1, 5], [0, 0.5, 0], 'hull', [0, 180, 0])],
  parachute: () => [P('dome', [5, 2, 5], [0, 3.5, 0], 'canopy', undefined, false), P('cylinder', [0.05, 3, 0.05], [0, 1.8, 0], 'line', undefined, false)],
  umbrella: () => [P('dome', [2.6, 1, 2.6], [0, 2.2, 0], 'canopy', undefined, true), P('cylinder', [0.08, 2.2, 0.08], [0, 1.1, 0], 'handle', undefined, false)],
  tube: () => ringOf(10, 1.8, (a, x, y) => P('box', [1.2, 0.2, 8], [x, y, 0], 'wall', [0, 0, a + 90])),
  pipe: () => ringOf(8, 1, (a, x, y) => P('box', [0.8, 0.15, 6], [x, y, 0], 'wall', [0, 0, a + 90])),
  slide: () => [P('wedge', [2.4, 5, 10], [0, 2.5, 0], 'chute'), P('box', [0.2, 0.8, 10.5], [-1.2, 3, 0], 'rail', [-27, 0, 0], false), P('box', [0.2, 0.8, 10.5], [1.2, 3, 0], 'rail', [-27, 0, 0], false)],
  conveyor: () => { const p: Part[] = [P('box', [3, 0.4, 12], [0, 0, 0], 'belt')]; for (let i = 0; i < 8; i++) p.push(P('cylinder', [0.5, 3.1, 0.5], [0, -0.25, -5.25 + i * 1.5], 'roller', [0, 0, 90], false)); return p; },
  carousel: () => { const p: Part[] = [P('torus', [7, 1.2, 7], [0, 0, 0], 'belt')]; p.push(P('cylinder', [5, 0.6, 5], [0, -0.2, 0], 'hub', undefined, false)); return p; },
  bucket: () => [P('cylinder', [2, 0.2, 2], [0, 0, 0], 'bottom'), ...ringOf(10, 1, (a, x, z) => P('box', [0.7, 1.8, 0.12], [x, 0.9, z], 'wall', [0, -a + 90, 0]))],
  box_open: () => [P('box', [3, 0.2, 3], [0, 0, 0], 'bottom'), P('box', [3, 2, 0.2], [0, 1, 1.5], 'side'), P('box', [3, 2, 0.2], [0, 1, -1.5], 'side'), P('box', [0.2, 2, 3], [1.5, 1, 0], 'side'), P('box', [0.2, 2, 3], [-1.5, 1, 0], 'side')],
  pillow: () => [P('box', [4, 1.2, 4], [0, 0.6, 0], 'cushion')],
  tornado: () => [P('cone', [5, 9, 5], [0, 4.5, 0], 'funnel', [180, 0, 0], false), ...[0, 1, 2, 3].map((i) => P('torus', [4.5 - i * 0.9, 0.15, 4.5 - i * 0.9], [0, 7.5 - i * 2, 0], 'swirl', undefined, false))],
  vortex: () => [...[0, 1, 2].map((i) => P('torus', [5 - i * 1.4, 0.2, 5 - i * 1.4], [0, 0, i * 0.3], 'swirl', [90, 0, 0], false)), P('sphere', [0.8, 0.8, 0.8], [0, 0, 0], 'core', undefined, false)],
  ring: () => [P('torus', [4, 0.4, 4], [0, 0, 0], 'ring', [90, 0, 0])],
  hoop: () => [P('torus', [3, 0.25, 3], [0, 0, 0], 'ring', [90, 0, 0])],
  cone: () => [P('cone', [1.6, 2.4, 1.6], [0, 1.2, 0], 'body')],
  spike: () => [P('cone', [0.6, 1.6, 0.6], [0, 0.8, 0], 'spike')],
  disc: () => [P('cylinder', [2.4, 0.2, 2.4], [0, 0, 0], 'disc', [90, 0, 0])],
  star: () => ringOf(5, 0.7, (a, x, y) => P('cone', [0.5, 1.4, 0.3], [x, y, 0], 'point', [0, 0, a - 90])),
  bolt: () => [P('box', [0.3, 1.4, 0.2], [0, 0.6, 0], 'seg', [0, 0, 20]), P('box', [0.3, 1.4, 0.2], [0.2, -0.6, 0], 'seg', [0, 0, 20])],
  hand_giant: () => FORMS.hand(),
  chain: () => { const p: Part[] = []; for (let i = 0; i < 8; i++) p.push(P('torus', [0.8, 0.18, 0.8], [0, 0, i * 0.6], 'link', i % 2 ? [0, 0, 90] : [90, 0, 0])); return p; },
  creature: () => [P('capsule', [1.4, 2.6, 1.4], [0, 1.2, 0], 'body', [90, 0, 0]), P('sphere', [1, 1, 1.1], [0, 1.8, 2.2], 'head'), ...[-1, 1].map((s) => P('box', [2.4, 0.1, 1.2], [s * 1.6, 1.6, 0], 'wing', [0, 0, s * -10], false))],
  dragon: () => [P('capsule', [1.8, 4, 1.8], [0, 1.6, 0], 'body', [90, 0, 0]), P('sphere', [1.3, 1.1, 1.8], [0, 2.4, 3.4], 'head'), P('cone', [0.8, 3, 0.8], [0, 1.4, -3.8], 'tail', [-90, 0, 0]), ...[-1, 1].map((s) => P('wedge', [4.5, 0.12, 2.4], [s * 3, 2.4, 0], 'wing', [0, 0, s * -12]))],
  bird: () => [P('sphere', [1, 0.9, 1.6], [0, 0, 0], 'body'), P('cone', [0.3, 0.6, 0.3], [0, 0.1, 1], 'beak', [90, 0, 0]), ...[-1, 1].map((s) => P('wedge', [2, 0.08, 0.9], [s * 1.3, 0.2, 0], 'wing'))],
  snake: () => { const p: Part[] = []; for (let i = 0; i < 10; i++) p.push(P('sphere', [0.8, 0.8, 0.8], [Math.sin(i * 0.8) * 0.8, 0.4, i * 0.7], 'segment')); return p; },
  horse: () => [P('capsule', [1.2, 2, 1.2], [0, 1.6, 0], 'body', [90, 0, 0]), P('box', [0.5, 1.2, 0.9], [0, 2.4, 1.6], 'head', [30, 0, 0]), ...[[-0.4, 0.8], [0.4, 0.8], [-0.4, -0.8], [0.4, -0.8]].map(([x, z]) => P('cylinder', [0.25, 1.4, 0.25], [x, 0.7, z], 'leg'))],
};

export const FORM_IDS = Object.keys(FORMS);

export function buildForm(id: string): Part[] {
  const f = FORMS[id] ?? FORMS.block;
  return f().map((p) => ({ ...p, size: [...p.size] as Vec3, pos: [...p.pos] as Vec3, ...(p.rot ? { rot: [...p.rot] as Vec3 } : {}) }));
}

/** Where to mount an added part on a host, by relation word. Returns local position and rotation. */
export function mountPoint(hostParts: Part[], where: 'back' | 'front' | 'top' | 'bottom' | 'sides' | 'left' | 'right' | 'around' | 'end' | 'tip'): { pos: Vec3; rot?: Vec3 }[] {
  let min: Vec3 = [Infinity, Infinity, Infinity];
  let max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (const p of hostParts) {
    if (p.solid === false) continue;
    const r = Math.max(...p.size) / 2;
    for (let i = 0; i < 3; i++) { min[i] = Math.min(min[i], p.pos[i] - r * 0.8); max[i] = Math.max(max[i], p.pos[i] + r * 0.8); }
  }
  if (!isFinite(min[0])) { min = [-0.5, -0.5, -0.5]; max = [0.5, 0.5, 0.5]; }
  const c: Vec3 = [(min[0] + max[0]) / 2, (min[1] + max[1]) / 2, (min[2] + max[2]) / 2];
  switch (where) {
    case 'back': return [{ pos: [c[0] - (max[0] - min[0]) * 0.22, c[1], min[2]] }, { pos: [c[0] + (max[0] - min[0]) * 0.22, c[1], min[2]] }];
    case 'front': case 'tip': case 'end': return [{ pos: [c[0], c[1], max[2]] }];
    case 'top': return [{ pos: [c[0], max[1], c[2]] }];
    case 'bottom': return [{ pos: [c[0], min[1], c[2]] }];
    case 'left': return [{ pos: [min[0], c[1], c[2]] }];
    case 'right': return [{ pos: [max[0], c[1], c[2]] }];
    case 'sides': return [{ pos: [min[0], c[1], c[2]] }, { pos: [max[0], c[1], c[2]] }];
    case 'around': return [0, 1, 2, 3, 4, 5].map((i) => { const a = (i / 6) * Math.PI * 2; return { pos: [c[0] + Math.cos(a) * (max[0] - min[0]) / 2, c[1], c[2] + Math.sin(a) * (max[2] - min[2]) / 2] as Vec3 }; });
    default: return [{ pos: [c[0], max[1], c[2]] }]; // unknown relation words ("head") → on top
  }
}

// =====================================================================================================
// Machine / creature helper forms (language layer). Catalog-independent: generic articulated bodies
// and add-on part kits. Parts carry `role` (+ `group` where the form is pre-articulated); FORM_JOINTS
// gives the joints for pre-articulated forms. A machine catalog may supersede these bodies.
// =====================================================================================================

const G = (p: Part, group: string): Part => ({ ...p, group });

const MACHINE_FORMS: Record<string, FormFn> = {
  /** four-legged animal: body along z, head at +z, legs (role leg), tail (role tail) */
  quadruped: () => [
    P('capsule', [1.1, 2.2, 1.1], [0, 1.4, 0], 'body', [90, 0, 0]),
    P('sphere', [0.8, 0.75, 0.9], [0, 1.9, 1.5], 'head'),
    P('box', [0.45, 0.35, 0.5], [0, 1.75, 2.0], 'snout'),
    ...[[-0.38, 0.75], [0.38, 0.75], [-0.38, -0.75], [0.38, -0.75]].map(([x, z]) => P('capsule', [0.26, 0.9, 0.26], [x, 0.55, z], 'leg')),
    P('cone', [0.22, 1, 0.22], [0, 1.6, -1.55], 'tail', [-120, 0, 0]),
  ],
  /** two-legged robot / golem: torso, head, arms (role arm), legs (role leg) */
  humanoid: () => [
    P('box', [1.2, 1.4, 0.7], [0, 2.1, 0], 'body'),
    P('box', [0.7, 0.6, 0.6], [0, 3.15, 0], 'head'),
    P('capsule', [0.3, 1.2, 0.3], [-0.8, 2.0, 0], 'arm'),
    P('capsule', [0.3, 1.2, 0.3], [0.8, 2.0, 0], 'arm'),
    P('capsule', [0.36, 1.3, 0.36], [-0.32, 0.75, 0], 'leg'),
    P('capsule', [0.36, 1.3, 0.36], [0.32, 0.75, 0], 'leg'),
  ],
  /** eight-legged bug */
  spider: () => {
    const p: Part[] = [P('sphere', [1.1, 0.8, 1.1], [0, 0.9, 0.4], 'body'), P('sphere', [1.3, 1.0, 1.5], [0, 1.0, -0.9], 'abdomen'), P('sphere', [0.5, 0.45, 0.5], [0, 0.95, 1.1], 'head')];
    for (let i = 0; i < 4; i++) for (const s of [-1, 1]) p.push(P('capsule', [0.14, 1.3, 0.14], [s * 0.85, 0.55, 0.9 - i * 0.5], 'leg', [0, 0, s * 35]));
    return p;
  },
  /** fish / shark: body along z, tail fin (role tail), dorsal fin, jaw (role jaw) */
  fish: () => [
    P('capsule', [1.2, 3.0, 1.0], [0, 1.2, 0], 'body', [90, 0, 0]),
    P('wedge', [0.1, 1.0, 0.8], [0, 1.95, 0.1], 'fin'),
    P('box', [0.1, 1.2, 0.6], [0, 1.2, -1.9], 'tail'),
    P('box', [0.8, 0.2, 0.7], [0, 0.85, 1.35], 'jaw'),
    ...[-1, 1].map((s) => P('wedge', [0.8, 0.08, 0.5], [s * 0.7, 1.0, 0.3], 'fin', [0, 0, s * -20])),
  ],
  /** tracked tank: hull, treads, road wheels (role wheel), turret (group turret), barrel + muzzle (group barrel) */
  tank: () => {
    const p: Part[] = [P('box', [2.6, 0.9, 4.6], [0, 1.0, 0], 'body')];
    for (const s of [-1, 1]) {
      p.push(P('box', [0.6, 0.9, 4.8], [s * 1.55, 0.55, 0], 'tread'));
      for (const z of [-1.6, 0, 1.6]) p.push(P('cylinder', [0.7, 0.12, 0.7], [s * 1.92, 0.55, z], 'wheel', [0, 0, 90]));
    }
    p.push(G(P('dome', [1.8, 0.8, 1.8], [0, 1.45, -0.2], 'turret'), 'turret'));
    p.push(G(P('cylinder', [0.26, 2.4, 0.26], [0, 1.75, 1.0], 'barrel', [90, 0, 0]), 'barrel'));
    p.push(G(P('cylinder', [0.34, 0.3, 0.34], [0, 1.75, 2.3], 'muzzle', [90, 0, 0]), 'barrel'));
    return p;
  },
  // ---- add-on kits at unit size (the language layer scales and positions them on a host) ----
  m_wheel: () => [P('cylinder', [1, 0.35, 1], [0, 0, 0], 'wheel', [0, 0, 90])],
  m_leg: () => [P('capsule', [0.25, 1, 0.25], [0, -0.5, 0], 'leg')],
  m_rotor: () => [P('cylinder', [0.08, 0.3, 0.08], [0, 0, 0], 'hub'), P('box', [1, 0.04, 0.1], [0, 0.15, 0], 'rotor'), P('box', [0.1, 0.04, 1], [0, 0.15, 0], 'rotor')],
  m_prop: () => [P('cylinder', [0.2, 0.25, 0.2], [0, 0, 0], 'hub', [90, 0, 0]), P('box', [1, 0.1, 0.04], [0, 0, 0.05], 'blade', [0, 0, 15])],
  m_barrel: () => [P('cylinder', [0.18, 1, 0.18], [0, 0, 0.5], 'barrel', [90, 0, 0]), P('cylinder', [0.24, 0.12, 0.24], [0, 0, 1], 'muzzle', [90, 0, 0])],
  m_jaw: () => [P('box', [1, 0.22, 0.9], [0, 0, 0.45], 'jaw'), ...[-0.3, 0, 0.3].map((x) => P('cone', [0.12, 0.25, 0.12], [x, 0.2, 0.75], 'tooth', undefined, false))],
  m_segment: () => [P('sphere', [1, 1, 1], [0, 0, 0], 'segment')],
  m_seat: () => [P('box', [1, 0.25, 1], [0, 0.12, 0], 'seat'), P('box', [1, 0.9, 0.2], [0, 0.55, -0.45], 'seatback')],
};
Object.assign(FORMS, MACHINE_FORMS);
FORM_IDS.push(...Object.keys(MACHINE_FORMS));

/** Joints for forms that come pre-articulated (pivots in the form's own frame, unscaled). */
export const FORM_JOINTS: Record<string, () => import('./spec.js').Joint[]> = {
  tank: () => [
    { group: 'turret', pivot: [0, 1.45, -0.2], motion: [{ kind: 'aim', yaw: true, rate: 90 }] },
    { group: 'barrel', pivot: [0, 1.75, -0.2], parent: 'turret', motion: [{ kind: 'aim', pitch: true, rate: 60, minPitch: -8, maxPitch: 35 }] },
  ],
};
