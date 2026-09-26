import { type ConstructSpec, type Behavior, cloneSpec, partsBounds, partVolume } from './spec.js';
import { FORMS } from './forms.js';
import { adaptMachine, rigMachine, clampJoints, clampMachineBehavior, dedupeMachine, machineCost } from './machine-spec.js';

/**
 * The authority boundary. Whatever proposed the spec (rules, lexicon, embeddings, an LLM),
 * this function decides what is actually allowed. It never trusts the proposer's confidence.
 * Every reduction is reported so the player can SEE the clamp.
 */
export const LIMITS = {
  scale: [0.15, 14] as [number, number],
  stretch: [0.15, 14] as [number, number],
  maxDimension: 64,
  maxCount: 12,
  maxParts: 48,
  density: [20, 8000] as [number, number],
  maxVirtualMass: 20000,
  strikeSpeed: 90,
  throwSpeed: 80,
  homeSpeed: 70,
  shootSpeed: 70,
  shootRate: 6,
  spinRate: 30,
  orbitSpeed: 9,
  orbitRadius: 30,
  fieldRadius: 30,
  fieldStrength: 90,
  launchSpeed: 48,
  thrust: 60,
  explodeRadius: 16,
  explodeForce: 70,
  liftHeight: 45,
  lifetime: 150,
  energyMax: 100,
  energyRegen: 9, // per second
};

export interface ValidationReport {
  spec: ConstructSpec;
  clamps: string[];
  rejected: string | null;
  cost: number;
  mass: number;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function specVolume(s: ConstructSpec): number {
  const st = s.stretch ?? [1, 1, 1];
  const k = s.scale ** 3 * st[0] * st[1] * st[2];
  return s.parts.filter((p) => p.solid !== false).reduce((a, p) => a + partVolume(p), 0) * k;
}

export function specCost(s: ConstructSpec): number {
  const v = specVolume(s);
  return s.count * (2 + 1.1 * Math.sqrt(v)) + 1.5 * s.behaviors.length + machineCost(s);
}

export function validateSpec(input: ConstructSpec, energyAvailable: number): ValidationReport {
  const s = cloneSpec(input);
  const clamps: string[] = [];
  const note = (m: string) => { if (!clamps.includes(m)) clamps.push(m); };

  // structural sanity (reject malformed proposals rather than guessing)
  if (!Array.isArray(s.parts) || s.parts.length === 0) return { spec: s, clamps, rejected: 'no geometry', cost: 0, mass: 0 };
  for (const p of s.parts) {
    if (!['box', 'sphere', 'cylinder', 'capsule', 'cone', 'wedge', 'torus', 'dome'].includes(p.shape)) return { spec: s, clamps, rejected: `unknown shape "${p.shape}"`, cost: 0, mass: 0 };
    if (!p.size || p.size.length !== 3 || p.size.some((x) => !isFinite(x) || x <= 0)) return { spec: s, clamps, rejected: 'invalid part size', cost: 0, mass: 0 };
    if (!p.pos || p.pos.length !== 3 || p.pos.some((x) => !isFinite(x))) return { spec: s, clamps, rejected: 'invalid part position', cost: 0, mass: 0 };
    p.size = p.size.map((x) => clamp(x, 0.02, 40)) as any;
    p.pos = p.pos.map((x) => clamp(x, -40, 40)) as any;
  }
  if (s.parts.length > LIMITS.maxParts) { s.parts = s.parts.slice(0, LIMITS.maxParts); note(`simplified to ${LIMITS.maxParts} parts`); }
  if (!isFinite(s.scale) || s.scale <= 0) s.scale = 1;
  if (!isFinite(s.count) || s.count < 1) s.count = 1;
  s.count = Math.floor(s.count);

  if (s.scale > LIMITS.scale[1]) { note(`size capped (asked ×${fmt(s.scale)}, max ×${LIMITS.scale[1]})`); s.scale = LIMITS.scale[1]; }
  if (s.scale < LIMITS.scale[0]) { note(`size raised to minimum`); s.scale = LIMITS.scale[0]; }
  if (s.stretch) s.stretch = s.stretch.map((x) => clamp(isFinite(x) ? x : 1, ...LIMITS.stretch)) as any;

  // overall dimension
  const b = partsBounds(s.parts);
  const st = s.stretch ?? [1, 1, 1];
  const dim = Math.max((b.max[0] - b.min[0]) * st[0], (b.max[1] - b.min[1]) * st[1], (b.max[2] - b.min[2]) * st[2]) * s.scale;
  if (dim > LIMITS.maxDimension) {
    const k = LIMITS.maxDimension / dim;
    s.scale *= k;
    note(`size capped at ${LIMITS.maxDimension} m`);
  }
  if (s.count > LIMITS.maxCount) { note(`count capped (asked ${s.count}, max ${LIMITS.maxCount})`); s.count = LIMITS.maxCount; }

  s.density = clamp(s.density || 300, ...LIMITS.density);
  s.restitution = clamp(s.restitution ?? 0.1, 0, 1.2);
  s.friction = clamp(s.friction ?? 0.6, 0, 2);
  s.lifetime = clamp(isFinite(s.lifetime) ? s.lifetime : LIMITS.lifetime, 1, LIMITS.lifetime);
  if (!isFinite(input.lifetime) || input.lifetime > LIMITS.lifetime) if (input.lifetime > LIMITS.lifetime && input.lifetime !== 60) note(`lasts at most ${LIMITS.lifetime}s`);

  // machines: legacy orders → machine behaviours, joints from the catalog / part roles, then clamp everything
  if (!Array.isArray(s.behaviors)) s.behaviors = [];
  adaptMachine(s);
  s.behaviors = s.behaviors.map((bh) => clampBehavior(bh, note)).filter((x): x is Behavior => !!x);
  dedupeMachine(s, note);
  rigMachine(s);
  clampJoints(s, note);

  // virtual mass
  let mass = specVolume(s) * s.density / s.count;
  if (mass > LIMITS.maxVirtualMass) {
    s.density = Math.max(LIMITS.density[0], s.density * (LIMITS.maxVirtualMass / mass));
    mass = specVolume(s) * s.density / s.count;
    note('mass capped');
  }

  // energy (willpower) budget: reduce count first, then scale
  let cost = specCost(s);
  const budget = Math.max(0, energyAvailable);
  if (cost > budget) {
    const asked = s.count;
    while (s.count > 1 && specCost(s) > budget) s.count--;
    if (s.count < asked) note(`willpower: made ${s.count} of ${asked}`);
    const askedScale = s.scale;
    while (specCost(s) > budget && s.scale > LIMITS.scale[0] * 1.5) s.scale *= 0.85;
    if (s.scale < askedScale * 0.99) note(`willpower: shrank to ×${fmt(s.scale)}`);
    cost = specCost(s);
    if (cost > budget) return { spec: s, clamps, rejected: 'not enough willpower — let it recharge or dismiss something', cost, mass };
  }
  return { spec: s, clamps, rejected: null, cost, mass };
}

function clampBehavior(b: Behavior, note: (m: string) => void): Behavior | null {
  const c = (v: number | undefined, lo: number, hi: number, what: string, dflt: number) => {
    const x = v === undefined || !isFinite(v) ? dflt : v;
    if (x > hi) { note(`${what} capped`); return hi; }
    return Math.max(lo, x);
  };
  switch (b.type) {
    case 'strike': return { ...b, speed: c(b.speed, 5, LIMITS.strikeSpeed, 'strike speed', 40), repeat: b.repeat ? Math.min(8, Math.floor(b.repeat)) : undefined };
    case 'throw': return { ...b, speed: c(b.speed, 5, LIMITS.throwSpeed, 'throw speed', 38) };
    case 'home': return { ...b, speed: c(b.speed, 3, LIMITS.homeSpeed, 'homing speed', 30) };
    case 'shoot': return { ...b, speed: c(b.speed, 10, LIMITS.shootSpeed, 'shot speed', 45), rate: c(b.rate, 0.2, LIMITS.shootRate, 'fire rate', 2), size: c(b.size, 0.1, 1.5, 'shot size', 0.3) };
    case 'spin': return { ...b, rate: c(b.rate, -LIMITS.spinRate, LIMITS.spinRate, 'spin rate', 6) };
    case 'orbit': return { ...b, speed: c(b.speed, -LIMITS.orbitSpeed, LIMITS.orbitSpeed, 'orbit speed', 3), radius: c(b.radius, 1, LIMITS.orbitRadius, 'orbit radius', 4) };
    case 'field': return { ...b, radius: c(b.radius, 1, LIMITS.fieldRadius, 'field radius', 8), strength: c(b.strength, 1, LIMITS.fieldStrength, 'field strength', 30) };
    case 'launch': return { ...b, speed: c(b.speed, 3, LIMITS.launchSpeed, 'launch speed', 26) };
    case 'thrust': return { ...b, accel: c(b.accel, 0, LIMITS.thrust, 'thrust', 25) };
    case 'explode': return { ...b, radius: c(b.radius, 1, LIMITS.explodeRadius, 'blast radius', 6), force: c(b.force, 1, LIMITS.explodeForce, 'blast force', 30), delay: c(b.delay, 0, 30, 'fuse', 1.5) };
    case 'lift': return { ...b, height: c(b.height, 1, LIMITS.liftHeight, 'lift height', 8) };
    case 'conveyor': return { ...b, speed: c(b.speed, -40, 40, 'belt speed', 10) };
    case 'sweep': return { ...b, speed: c(b.speed, 1, 40, 'sweep speed', 14), width: c(b.width, 1, 40, 'sweep distance', 12) };
    case 'pulse': return { ...b, period: c(b.period, 0.3, 30, 'pulse period', 1.5) };
    case 'mount': return {
      ...b,
      glide: b.glide !== undefined ? c(b.glide, 0, 4, 'glide', 1) : undefined,
      groundSpeedMul: b.groundSpeedMul !== undefined ? c(b.groundSpeedMul, 0.2, 4, 'ground speed', 1) : undefined,
      thrustLook: b.thrustLook !== undefined ? c(b.thrustLook, 0, LIMITS.thrust, 'thrust', 20) : undefined,
      thrustUp: b.thrustUp !== undefined ? c(b.thrustUp, 0, LIMITS.thrust, 'lift thrust', 28) : undefined,
      maxFall: b.maxFall !== undefined ? c(b.maxFall, 1, 200, 'fall speed', 5) : undefined,
      bounce: b.bounce !== undefined ? c(b.bounce, 0, 40, 'bounce', 14) : undefined,
    };
    case 'drive': case 'brain': case 'weapon': case 'rideable': return clampMachineBehavior(b, note);
    default: return b;
  }
}

function fmt(n: number) { return Math.round(n * 10) / 10; }

export const KNOWN_FORMS = new Set(Object.keys(FORMS));
