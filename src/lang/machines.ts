/**
 * Machine composition for the language layer: turns "X with Y", "a flying X", "X that hunts drones"
 * into articulated specs (parts + joints + drive/brain/weapon/rideable behaviours) for ANY host X.
 *
 * Everything here is pure spec editing against the contract in src/sim/constructs/spec.ts; nothing
 * touches the world. Positions/pivots are in the host's unscaled part frame (+z forward, +y up).
 */
import { type ConstructSpec, type Part, type Joint, type JointMotion, type Behavior, type Vec3, type Ref, partsBounds } from '../sim/constructs/spec.js';
import { buildForm, FORM_JOINTS } from '../sim/constructs/forms.js';
import { conceptByName, type Concept, type LocoKind, type MachineKind, type WeaponKind, type MachineDefaults } from './lexicon.js';
import { Euler, Vector3 } from 'three';

export type BrainGoal = 'wander' | 'follow' | 'guard' | 'hunt' | 'fetch' | 'patrol' | 'goto' | 'flee' | 'idle';
export type DriveB = Extract<Behavior, { type: 'drive' }>;
export type BrainB = Extract<Behavior, { type: 'brain' }>;
export type WeaponB = Extract<Behavior, { type: 'weapon' }>;

export const MOVING_GOALS = new Set<BrainGoal>(['wander', 'follow', 'hunt', 'fetch', 'patrol', 'goto', 'flee']);
const MAX_PARTS = 46; // validator keeps 48

export const WEAPON_STATS: Record<WeaponKind, { rate: number; speed: number; damage: number; range: number; size?: number; spread?: number }> = {
  gun: { rate: 6, speed: 70, damage: 5, range: 50, size: 0.15, spread: 3 },
  cannon: { rate: 0.8, speed: 50, damage: 35, range: 70, size: 0.5 },
  laser: { rate: 2, speed: 70, damage: 14, range: 80, size: 0.2 },
  missile: { rate: 0.5, speed: 35, damage: 45, range: 90, size: 0.4 },
  flame: { rate: 10, speed: 18, damage: 3, range: 12, size: 0.5, spread: 12 },
  net: { rate: 0.5, speed: 35, damage: 0, range: 40, size: 1 },
  melee: { rate: 1.2, speed: 0, damage: 20, range: 2.5 },
  bomb: { rate: 0.4, speed: 5, damage: 50, range: 25, size: 0.6 },
};

const LOCO_DRIVE: Record<LocoKind, { mode: DriveB['mode']; speed: number; turn: number; altitude?: number; jump?: number }> = {
  wheels: { mode: 'ground', speed: 16, turn: 100 },
  treads: { mode: 'ground', speed: 9, turn: 60 },
  legs: { mode: 'ground', speed: 6, turn: 120, jump: 4 },
  roll: { mode: 'ground', speed: 12, turn: 120 },
  slither: { mode: 'ground', speed: 5, turn: 90 },
  wings: { mode: 'air', speed: 14, turn: 90, altitude: 8 },
  rotor: { mode: 'air', speed: 12, turn: 90, altitude: 10 },
  propeller: { mode: 'air', speed: 22, turn: 60, altitude: 12 },
  jets: { mode: 'air', speed: 18, turn: 90, altitude: 8 },
  hover: { mode: 'hover', speed: 10, turn: 120, altitude: 1.5 },
  swim: { mode: 'hover', speed: 9, turn: 100, altitude: 1.4 },
};

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

// ------------------------------------------------------------------------------------ inspection

export const driveOf = (s: ConstructSpec) => s.behaviors.find((b) => b.type === 'drive') as DriveB | undefined;
export const brainOf = (s: ConstructSpec) => s.behaviors.find((b) => b.type === 'brain') as BrainB | undefined;
export const weaponsOf = (s: ConstructSpec) => s.behaviors.filter((b) => b.type === 'weapon') as WeaponB[];
export const isRideable = (s: ConstructSpec) => s.behaviors.some((b) => b.type === 'rideable');

/** Does this spec behave as a machine/creature (moving parts, locomotion, autonomy, weapons or a seat)? */
export function isMachine(s: ConstructSpec): boolean {
  return !!(s.joints?.length || s.behaviors.some((b) => b.type === 'drive' || b.type === 'brain' || b.type === 'weapon' || b.type === 'rideable'));
}

export function conceptOf(s: ConstructSpec): Concept | null { return conceptByName.get(s.concept) ?? null; }

/** Hosts that naturally take machine traits (a sword that "flies" is a thrown sword, a dog that flies is a machine). */
export function machineCapable(s: ConstructSpec): boolean {
  const c = conceptOf(s);
  if (isMachine(s) || c?.mach) return true;
  if (!c) return true; // generic / unknown bodies
  return !['weapon', 'projectile', 'manipulator', 'tool', 'rope', 'wearable', 'field'].includes(c.cls);
}

interface Box { min: Vec3; max: Vec3; c: Vec3; size: Vec3 }
function hostBox(s: ConstructSpec): Box {
  const solid = s.parts.filter((p) => p.solid !== false && !['flame', 'tooth', 'rotor', 'blade'].includes(p.role ?? ''));
  const b = partsBounds(solid.length ? solid : s.parts);
  const size: Vec3 = [b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]].map((x) => Math.max(0.2, x)) as Vec3;
  return { min: b.min, max: b.max, size, c: [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2] };
}

function uniqGroup(s: ConstructSpec, base: string): string {
  const used = new Set([...(s.joints ?? []).map((j) => j.group), ...s.parts.map((p) => p.group).filter(Boolean) as string[]]);
  if (!used.has(base)) return base;
  for (let i = 2; ; i++) if (!used.has(`${base}_${i}`)) return `${base}_${i}`;
}

function addJoint(s: ConstructSpec, j: Joint) { (s.joints ??= []).push(j); }
function room(s: ConstructSpec, n: number) { return s.parts.length + n <= MAX_PARTS; }
function push(s: ConstructSpec, parts: Part[]): boolean {
  if (!room(s, parts.length)) return false;
  s.parts.push(...parts);
  return true;
}
const part = (shape: Part['shape'], size: Vec3, pos: Vec3, role: string, group?: string, rot?: Vec3, solid = true): Part =>
  ({ shape, size, pos, role, ...(group ? { group } : {}), ...(rot ? { rot } : {}), ...(solid ? {} : { solid: false }) });
const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];

// ------------------------------------------------------------------------------------ behaviours

export function setDrive(s: ConstructSpec, loco: LocoKind, d?: MachineDefaults | null) {
  const base = LOCO_DRIVE[loco];
  const cur = driveOf(s);
  const speed = d?.speed && d.loco === loco ? d.speed : base.speed;
  const altitude = d?.altitude && d.loco === loco ? d.altitude : base.altitude;
  const next: DriveB = { type: 'drive', mode: base.mode, speed, turn: base.turn, ...(altitude !== undefined ? { altitude } : {}), ...(base.jump ? { jump: base.jump } : {}) };
  if (cur) {
    // a flying request upgrades a ground machine; a ground request never grounds a flyer
    if (cur.mode === 'air' && next.mode !== 'air') return;
    if (cur.mode === next.mode) { cur.speed = Math.max(cur.speed, next.speed); return; }
    const merged: DriveB = { ...next, speed: Math.max(cur.speed, next.speed) };
    s.behaviors = s.behaviors.map((b) => (b === cur ? merged : b));
    return;
  }
  s.behaviors.push(next);
  if (s.motion === 'static') s.motion = 'kinematic';
}

export function setBrain(s: ConstructSpec, goal: BrainGoal, target?: Ref | null, range?: number) {
  s.behaviors = s.behaviors.filter((b) => b.type !== 'brain');
  s.behaviors.push({ type: 'brain', goal, ...(target ? { target } : {}), ...(range ? { range } : {}) });
}

export function addWeapon(s: ConstructSpec, kind: WeaponKind, group?: string, damageMul = 1) {
  const st = WEAPON_STATS[kind];
  s.behaviors.push({ type: 'weapon', kind, ...(group ? { group } : {}), rate: st.rate, speed: st.speed, damage: Math.round(st.damage * damageMul), range: st.range, ...(st.size ? { size: st.size } : {}), ...(st.spread ? { spread: st.spread } : {}) });
}

export function setRideable(s: ConstructSpec, seat?: Vec3) {
  if (isRideable(s)) return;
  const B = hostBox(s);
  s.behaviors.push({ type: 'rideable', seat: seat ?? [B.c[0], B.max[1] + 0.2, B.c[2] - B.size[2] * 0.1], view: 'third' });
}

// ------------------------------------------------------------------------------------ articulation of existing parts

const EUL = new Euler();
function rotated(v: Vec3, rot?: Vec3): Vec3 {
  if (!rot || (!rot[0] && !rot[1] && !rot[2])) return v;
  const r = new Vector3(...v).applyEuler(EUL.set(rot[0] * Math.PI / 180, rot[1] * Math.PI / 180, rot[2] * Math.PI / 180));
  return [r.x, r.y, r.z];
}
/** The face centre of a part nearest to a point (where a limb / tail / wing attaches to the body). */
function attachPoint(p: Part, toward: Vec3): Vec3 {
  let best: Vec3 = p.pos, bd = Infinity;
  for (let ax = 0; ax < 3; ax++) for (const sgn of [-1, 1]) {
    const local: Vec3 = [0, 0, 0];
    local[ax] = (sgn * p.size[ax]) / 2;
    const q = add(p.pos, rotated(local, p.rot));
    const d = Math.hypot(q[0] - toward[0], q[1] - toward[1], q[2] - toward[2]);
    if (d < bd) { bd = d; best = q; }
  }
  return best;
}

/** Give joints to a body's own wheels / legs / wings / rotor / tail / arms / jaw (by part role). Idempotent. */
export function articulate(s: ConstructSpec, opts: { creature?: boolean } = {}): void {
  const concept = conceptOf(s);
  const creature = concept?.cls === 'creature' || !!opts.creature;
  if (concept?.form && FORM_JOINTS[concept.form]) {
    for (const j of FORM_JOINTS[concept.form]()) if (!(s.joints ?? []).some((x) => x.group === j.group) && s.parts.some((p) => p.group === j.group)) addJoint(s, j);
  }
  const B = hostBox(s);
  const body = s.parts.filter((p) => p.solid !== false && !p.group && !['leg', 'arm', 'tail', 'wing', 'wheel', 'jaw'].includes(p.role ?? ''));
  const core: Vec3 = body.length ? (() => { const b = partsBounds(body); return [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2] as Vec3; })() : B.c;
  const free = (role: string) => s.parts.filter((p) => p.role === role && !p.group);
  // wheels: one group per wheel, rolling (front ones also steer)
  const wheels = free('wheel');
  wheels.forEach((w) => {
    const g = uniqGroup(s, 'wheel');
    w.group = g;
    const r = Math.max(w.size[0], w.size[2]) / 2;
    const motion: JointMotion[] = [];
    if (wheels.length >= 3 && w.pos[2] > B.c[2] + B.size[2] * 0.1 && !s.parts.some((p) => p.role === 'tread')) motion.push({ kind: 'steer', axis: [0, 1, 0], amp: 25 });
    motion.push({ kind: 'roll', axis: [1, 0, 0], radius: r });
    addJoint(s, { group: g, pivot: [...w.pos] as Vec3, motion });
  });
  // rotor blades share one spinning hub
  const rotors = free('rotor');
  if (rotors.length) {
    const g = uniqGroup(s, 'rotor');
    const piv: Vec3 = [rotors.reduce((a, p) => a + p.pos[0], 0) / rotors.length, Math.min(...rotors.map((p) => p.pos[1])), rotors.reduce((a, p) => a + p.pos[2], 0) / rotors.length];
    for (const p of rotors) p.group = g;
    addJoint(s, { group: g, pivot: piv, motion: [{ kind: 'spin', axis: [0, 1, 0], rate: 5 }] });
  }
  // flapping wings (creatures only; plane wings stay rigid)
  if (creature) for (const w of free('wing')) {
    const side = w.pos[0] < core[0] ? -1 : 1;
    const g = uniqGroup(s, side < 0 ? 'wing_l' : 'wing_r');
    w.group = g;
    addJoint(s, { group: g, pivot: attachPoint(w, core), motion: [{ kind: 'swing', axis: [0, 0, side < 0 ? 1 : -1], amp: 25, rate: 1.5 }] });
  }
  if (creature) {
    // legs: gait-scaled swing; diagonal pairs (and alternating tripods for many legs) in phase
    const legs = free('leg');
    for (const left of [true, false]) {
      legs.filter((l) => (l.pos[0] < core[0]) === left).sort((a, b) => b.pos[2] - a.pos[2]).forEach((l, idx) => {
        const g = uniqGroup(s, 'leg');
        l.group = g;
        const phase = legs.length <= 2 ? (left ? 0 : 0.5) : ((idx + (left ? 0 : 1)) % 2) * 0.5;
        addJoint(s, { group: g, pivot: attachPoint(l, core), motion: [{ kind: 'swing', axis: [1, 0, 0], amp: 28, rate: 1.6, phase, gait: true }] });
      });
    }
    for (const a of free('arm')) {
      const left = a.pos[0] < core[0];
      const g = uniqGroup(s, left ? 'arm_l' : 'arm_r');
      a.group = g;
      addJoint(s, { group: g, pivot: attachPoint(a, core), motion: [{ kind: 'swing', axis: [1, 0, 0], amp: 30, rate: 1.6, phase: left ? 0.5 : 0, gait: true }] });
    }
    for (const t of free('tail')) {
      const g = uniqGroup(s, 'tail');
      t.group = g;
      addJoint(s, { group: g, pivot: attachPoint(t, core), motion: [{ kind: 'wave', axis: [0, 1, 0], amp: 20, rate: 1 }] });
    }
    for (const j of free('jaw')) {
      const g = uniqGroup(s, 'jaw');
      j.group = g;
      addJoint(s, { group: g, pivot: [j.pos[0], j.pos[1], j.pos[2] - j.size[2] / 2], motion: [{ kind: 'swing', axis: [1, 0, 0], amp: 15, rate: 2 }] });
    }
  }
  // a static turret body (head + barrel) becomes an aiming turret
  const head = free('head'), barrel = free('barrel');
  if (concept?.id === 'turret' && head.length && barrel.length) {
    const g = uniqGroup(s, 'turret');
    for (const p of [...head, ...barrel]) p.group = g;
    addJoint(s, { group: g, pivot: [...head[0].pos] as Vec3, motion: [{ kind: 'aim', yaw: true, pitch: true, rate: 90, minPitch: -10, maxPitch: 60 }] });
  }
}

// ------------------------------------------------------------------------------------ add-on kits

export interface KitOpts { where?: string | null; count?: number | null; weapon?: WeaponKind; notes?: string[] }

function spot(s: ConstructSpec, where: string, B: Box): Vec3 {
  const head = s.parts.find((p) => p.role === 'head' && p.solid !== false);
  switch (where) {
    case 'head': return head ? [head.pos[0], head.pos[1] + head.size[1] / 2, head.pos[2]] : [B.c[0], B.max[1], B.c[2] + B.size[2] * 0.3];
    case 'front': case 'tip': case 'end': return head ? [head.pos[0], head.pos[1], head.pos[2] + head.size[2] / 2] : [B.c[0], B.c[1], B.max[2]];
    case 'back': return [B.c[0], B.c[1], B.min[2]];
    case 'bottom': return [B.c[0], B.min[1], B.c[2]];
    case 'left': return [B.min[0], B.c[1], B.c[2]];
    case 'right': return [B.max[0], B.c[1], B.c[2]];
    default: return [B.c[0], B.max[1], B.c[2]]; // top
  }
}

/** Attach an articulated part kit. Returns true when something was added. */
export function attachKit(s: ConstructSpec, kind: MachineKind, o: KitOpts = {}): boolean {
  const notes = o.notes ?? [];
  const B = hostBox(s);
  const [sx, sy, sz] = B.size;
  const H = Math.max(sx, sy, sz);
  const mach = conceptOf(s)?.mach ?? null;
  const has = (role: string) => s.parts.some((p) => p.role === role);
  switch (kind) {
    case 'wheels': {
      if (has('wheel')) { articulate(s); setDrive(s, 'wheels', mach); notes.push('it already had wheels — made them roll'); return true; }
      const n = clamp(Math.round(o.count ?? 4), 1, 8);
      const d = clamp(Math.max(0.32 * sy, 0.2 * sz, 0.2 * sx), 0.35, 4), t = d * 0.35;
      const pos: Vec3[] = [];
      if (n === 1) pos.push([B.c[0], B.min[1], B.c[2]]);
      else if (n === 2) pos.push([B.c[0], B.min[1], B.c[2] + sz * 0.38], [B.c[0], B.min[1], B.c[2] - sz * 0.38]);
      else if (n === 3) pos.push([B.c[0], B.min[1], B.c[2] + sz * 0.38], [B.min[0] - t * 0.3, B.min[1], B.c[2] - sz * 0.38], [B.max[0] + t * 0.3, B.min[1], B.c[2] - sz * 0.38]);
      else {
        const per = Math.ceil(n / 2);
        for (let i = 0; i < per; i++) for (const side of [-1, 1]) {
          if (pos.length >= n) break;
          const z = per === 1 ? B.c[2] : B.c[2] + sz * 0.38 - (i * sz * 0.76) / (per - 1);
          pos.push([side < 0 ? B.min[0] - t * 0.3 : B.max[0] + t * 0.3, B.min[1], z]);
        }
      }
      if (!room(s, pos.length)) return full(notes, 'wheels');
      for (const p of pos) s.parts.push(part('cylinder', [d, t, d], p, 'wheel', undefined, [0, 0, 90]));
      articulate(s);
      setDrive(s, 'wheels', mach);
      return true;
    }
    case 'treads': {
      const tw = clamp(sx * 0.22, 0.3, 2), th = clamp(sy * 0.4, 0.35, 3), len = sz * 0.95;
      if (!room(s, 8)) return full(notes, 'treads');
      for (const side of [-1, 1]) {
        const x = side < 0 ? B.min[0] - tw / 2 : B.max[0] + tw / 2;
        s.parts.push(part('box', [tw, th, len], [x, B.min[1], B.c[2]], 'tread'));
        for (const k of [-1, 0, 1]) s.parts.push(part('cylinder', [th * 0.8, tw * 0.15, th * 0.8], [x + side * tw * 0.55, B.min[1], B.c[2] + k * len * 0.33], 'wheel', undefined, [0, 0, 90]));
      }
      articulate(s);
      setDrive(s, 'treads', mach);
      return true;
    }
    case 'legs': {
      if (s.parts.some((p) => p.role === 'leg')) { articulate(s, { creature: true }); setDrive(s, 'legs', mach); return true; }
      const n = clamp(Math.round(o.count ?? mach?.legs ?? (sy > 1.3 * Math.max(sx, sz) ? 2 : 4)), 1, 8);
      const L = clamp(Math.max(0.9 * sy, 0.35 * sz), 0.6, 8), w = clamp(L * 0.18, 0.12, 1.2);
      const hipY = B.min[1] + w * 0.3;
      const hips: { p: Vec3; phase: number }[] = [];
      if (n <= 2) for (const side of n === 1 ? [0] : [-1, 1]) hips.push({ p: [B.c[0] + side * sx * 0.25, hipY, B.c[2]], phase: side < 0 ? 0 : 0.5 });
      else {
        const per = Math.ceil(n / 2);
        for (let i = 0; i < per; i++) for (const side of [-1, 1]) {
          if (hips.length >= n) break;
          const z = per === 1 ? B.c[2] : B.c[2] + sz * 0.35 - (i * sz * 0.7) / (per - 1);
          hips.push({ p: [B.c[0] + side * sx * 0.38, hipY, z], phase: ((i + (side < 0 ? 0 : 1)) % 2) * 0.5 });
        }
      }
      if (!room(s, hips.length)) return full(notes, 'legs');
      for (const h of hips) {
        const g = uniqGroup(s, 'leg');
        s.parts.push(part('capsule', [w, L, w], [h.p[0], h.p[1] - L / 2, h.p[2]], 'leg', g));
        addJoint(s, { group: g, pivot: h.p, motion: [{ kind: 'swing', axis: [1, 0, 0], amp: 28, rate: 1.6, phase: h.phase, gait: true }] });
      }
      setDrive(s, 'legs', mach);
      return true;
    }
    case 'wings': {
      if (s.parts.some((p) => p.role === 'wing')) { articulate(s, { creature: conceptOf(s)?.id !== 'plane' }); setDrive(s, 'wings', mach); return true; }
      const span = clamp(Math.max(sz, sx) * 0.8, 0.8, 10), chord = clamp(sz * 0.4, 0.4, 4), th = clamp(span * 0.03, 0.05, 0.3);
      const y = B.c[1] + sy * 0.2;
      if (!room(s, 2)) return full(notes, 'wings');
      for (const side of [-1, 1]) {
        const root = side < 0 ? B.min[0] : B.max[0];
        const g = uniqGroup(s, side < 0 ? 'wing_l' : 'wing_r');
        s.parts.push(part('wedge', [span, th, chord], [root + side * span / 2, y, B.c[2]], 'wing', g));
        addJoint(s, { group: g, pivot: [root, y, B.c[2]], motion: [{ kind: 'swing', axis: [0, 0, side < 0 ? 1 : -1], amp: 25, rate: 1.5 }] });
      }
      setDrive(s, 'wings', mach);
      return true;
    }
    case 'rotor': {
      if (s.parts.some((p) => p.role === 'rotor') && !o.count) { articulate(s); setDrive(s, 'rotor', mach); return true; }
      const n = clamp(Math.round(o.count ?? 1), 1, 4);
      const R = clamp(Math.max(sx, sz) * (n > 1 ? 0.7 : 1.3), 1.2, 12);
      const hubs: Vec3[] = n === 1 ? [[B.c[0], B.max[1], B.c[2]]] : n === 2 ? [[B.c[0], B.max[1], B.max[2]], [B.c[0], B.max[1], B.min[2]]]
        : [[B.min[0], B.max[1], B.max[2]], [B.max[0], B.max[1], B.max[2]], [B.min[0], B.max[1], B.min[2]], [B.max[0], B.max[1], B.min[2]]].slice(0, n) as Vec3[];
      if (!room(s, hubs.length * 3)) return full(notes, 'rotors');
      hubs.forEach((h, i) => {
        const g = uniqGroup(s, 'rotor');
        const mast = R * 0.12;
        s.parts.push(part('cylinder', [R * 0.06, mast, R * 0.06], [h[0], h[1] + mast / 2, h[2]], 'hub'));
        s.parts.push(part('box', [R, R * 0.02, R * 0.08], [h[0], h[1] + mast, h[2]], 'rotor', g), part('box', [R * 0.08, R * 0.02, R], [h[0], h[1] + mast, h[2]], 'rotor', g));
        addJoint(s, { group: g, pivot: [h[0], h[1] + mast, h[2]], motion: [{ kind: 'spin', axis: [0, 1, 0], rate: i % 2 ? -5 : 5 }] });
      });
      setDrive(s, 'rotor', mach);
      return true;
    }
    case 'propeller': {
      const front = o.where === 'front' || o.where === 'tip' || o.where === 'back' || has('wing') && o.where !== 'top';
      if (!front) return attachKit(s, 'rotor', { ...o, count: o.count ?? 1 });
      const R = clamp(Math.max(sx, sy) * 0.8, 0.8, 6);
      const z = o.where === 'back' ? B.min[2] : B.max[2];
      const g = uniqGroup(s, 'propeller');
      if (!room(s, 2)) return full(notes, 'propeller');
      s.parts.push(part('cylinder', [R * 0.15, R * 0.2, R * 0.15], [B.c[0], B.c[1], z], 'hub', g, [90, 0, 0]), part('box', [R, R * 0.1, R * 0.04], [B.c[0], B.c[1], z + R * 0.08], 'blade', g));
      addJoint(s, { group: g, pivot: [B.c[0], B.c[1], z], motion: [{ kind: 'spin', axis: [0, 0, 1], rate: 8 }] });
      setDrive(s, 'propeller', mach);
      return true;
    }
    case 'jets': {
      const r = clamp(Math.min(sx, sz) * 0.2, 0.2, 2);
      if (!room(s, 4)) return full(notes, 'jets');
      for (const side of [-1, 1]) {
        const p: Vec3 = [B.c[0] + side * sx * 0.3, B.min[1], B.c[2] - sz * 0.2];
        s.parts.push(part('cylinder', [r, r * 1.6, r], p, 'thruster'), part('cone', [r * 0.9, r * 1.4, r * 0.9], add(p, [0, -r * 1.4, 0]), 'flame', undefined, [180, 0, 0], false));
      }
      setDrive(s, 'jets', mach);
      return true;
    }
    case 'hover': {
      const r = clamp(Math.min(sx, sz) * 0.35, 0.3, 3);
      if (!room(s, 2)) return full(notes, 'hover pads');
      for (const k of [-1, 1]) s.parts.push(part('cylinder', [r, r * 0.15, r], [B.c[0], B.min[1] - r * 0.1, B.c[2] + k * sz * 0.25], 'pad'));
      setDrive(s, 'hover', mach);
      return true;
    }
    case 'turret': {
      const r = clamp(0.4 * Math.min(sx, sz), 0.35, 3);
      const base = o.where && o.where !== 'top' ? spot(s, o.where === 'back' ? 'top' : o.where, B) : spot(s, 'top', B);
      if (!room(s, 3)) return full(notes, 'turret');
      const gT = uniqGroup(s, 'turret'), gB = uniqGroup(s, 'barrel');
      const by = base[1] + r * 0.45;
      s.parts.push(part('dome', [r * 2, r * 0.9, r * 2], base, 'turret', gT));
      s.parts.push(part('cylinder', [r * 0.28, r * 2.2, r * 0.28], [base[0], by, base[2] + r * 1.1], 'barrel', gB, [90, 0, 0]));
      s.parts.push(part('cylinder', [r * 0.36, r * 0.2, r * 0.36], [base[0], by, base[2] + r * 2.2], 'muzzle', gB, [90, 0, 0]));
      addJoint(s, { group: gT, pivot: base, motion: [{ kind: 'aim', yaw: true, rate: 90 }] });
      addJoint(s, { group: gB, pivot: [base[0], by, base[2]], parent: gT, motion: [{ kind: 'aim', pitch: true, rate: 60, minPitch: -10, maxPitch: 45 }] });
      addWeapon(s, o.weapon ?? 'cannon', gB);
      return true;
    }
    case 'gun': {
      const kind = o.weapon ?? 'gun';
      const n = clamp(Math.round(o.count ?? 1), 1, 8);
      const creature = conceptOf(s)?.cls === 'creature';
      const where = o.where === 'back' ? 'top' : o.where ?? (creature && (kind === 'laser' || kind === 'flame') ? (kind === 'flame' ? 'front' : 'head') : kind === 'bomb' ? 'bottom' : kind === 'flame' ? 'front' : 'top');
      const len = clamp(0.4 * H, 0.5, 5) * (kind === 'cannon' ? 1.2 : kind === 'flame' ? 0.7 : 1);
      const dia = len * (kind === 'cannon' ? 0.24 : kind === 'missile' ? 0.4 : 0.16);
      const bases: Vec3[] = [];
      if (where === 'sides') bases.push(spot(s, 'left', B), spot(s, 'right', B));
      else {
        const sp = spot(s, where, B);
        // a row across (up to 3), further rows front-to-back
        const cols = Math.min(n, 3), rows = Math.ceil(n / cols);
        const spanX = where === 'bottom' || where === 'top' || where === 'head' || where === 'front' ? Math.max(sx * 0.6, dia * 3) : 0;
        const spanZ = where === 'front' ? 0 : Math.min(sz * 0.6, dia * 3 * rows);
        for (let i = 0; i < n; i++) {
          const c = i % cols, r = Math.floor(i / cols);
          bases.push([sp[0] + (cols > 1 ? (c / (cols - 1) - 0.5) * spanX : 0), sp[1], sp[2] + (rows > 1 ? (0.5 - r / (rows - 1)) * spanZ : 0)]);
        }
      }
      if (!room(s, bases.length * 2)) return full(notes, `${kind}s`);
      for (const b of bases) {
        const g = uniqGroup(s, kind === 'flame' ? 'flamer' : kind);
        const lift = where === 'bottom' ? -dia / 2 : dia / 2;
        if (kind === 'missile') s.parts.push(part('box', [dia, dia * 0.8, len * 0.8], [b[0], b[1] + lift, b[2] + len * 0.4], 'launcher', g));
        else s.parts.push(part('cylinder', [dia, len, dia], [b[0], b[1] + lift, b[2] + len / 2], 'barrel', g, [90, 0, 0]));
        s.parts.push(part('cylinder', [dia * 1.3, len * 0.1, dia * 1.3], [b[0], b[1] + lift, b[2] + len], 'muzzle', g, [90, 0, 0]));
        addJoint(s, { group: g, pivot: [b[0], b[1] + lift, b[2]], motion: [{ kind: 'aim', yaw: true, pitch: true, rate: kind === 'missile' ? 90 : 120, minPitch: -15, maxPitch: kind === 'bomb' ? 0 : 60 }] });
        addWeapon(s, kind, g);
      }
      return true;
    }
    case 'jaws': {
      if (s.parts.some((p) => p.role === 'jaw')) forceJaw(s);
      else {
        const head = s.parts.find((p) => p.role === 'head' && p.solid !== false);
        const w = clamp(head ? Math.min(head.size[0], head.size[2]) * 0.9 : Math.min(sx, sy) * 0.5, 0.25, 4);
        const piv: Vec3 = head ? [head.pos[0], head.pos[1] - head.size[1] * 0.25, head.pos[2]] : [B.c[0], B.c[1] - sy * 0.2, B.max[2] - w * 0.3];
        const g = uniqGroup(s, 'jaw');
        if (!room(s, 4)) return full(notes, 'jaws');
        s.parts.push(part('box', [w, w * 0.22, w * 0.9], add(piv, [0, 0, w * 0.45]), 'jaw', g));
        for (const x of [-0.3, 0, 0.3]) s.parts.push(part('cone', [w * 0.12, w * 0.25, w * 0.12], add(piv, [x * w, w * 0.2, w * 0.8]), 'tooth', g, undefined, false));
        addJoint(s, { group: g, pivot: piv, motion: [{ kind: 'swing', axis: [1, 0, 0], amp: 18, rate: 2.5 }] });
      }
      const g = s.parts.find((p) => p.role === 'jaw')?.group;
      if (!weaponsOf(s).some((w) => w.kind === 'melee')) addWeapon(s, 'melee', g, 1.25);
      return true;
    }
    case 'arms': {
      if (s.parts.some((p) => p.role === 'arm')) articulate(s, { creature: true });
      else {
        const L = clamp(0.7 * sy, 0.5, 6), w = clamp(L * 0.22, 0.15, 1.5);
        const sh = B.c[1] + sy * 0.25;
        if (!room(s, 4)) return full(notes, 'arms');
        for (const side of [-1, 1]) {
          const x = side < 0 ? B.min[0] - w / 2 : B.max[0] + w / 2;
          const g = uniqGroup(s, side < 0 ? 'arm_l' : 'arm_r');
          s.parts.push(part('capsule', [w, L, w], [x, sh - L / 2, B.c[2]], 'arm', g), part('sphere', [w * 1.7, w * 1.7, w * 1.7], [x, sh - L, B.c[2]], 'fist', g));
          addJoint(s, { group: g, pivot: [x, sh, B.c[2]], motion: [{ kind: 'swing', axis: [1, 0, 0], amp: 40, rate: 1.2, phase: side < 0 ? 0 : 0.5 }] });
        }
      }
      const g = s.parts.find((p) => p.role === 'arm' && p.group?.startsWith('arm_r'))?.group ?? s.parts.find((p) => p.role === 'arm')?.group;
      if (!weaponsOf(s).some((w) => w.kind === 'melee')) addWeapon(s, 'melee', g, 0.9);
      return true;
    }
    case 'claws': {
      const w = clamp(Math.min(sx, sy) * 0.45, 0.3, 4);
      const f = spot(s, 'front', B);
      if (!room(s, 2)) return full(notes, 'claws');
      let gR = '';
      for (const side of [-1, 1]) {
        const g = uniqGroup(s, side < 0 ? 'claw_l' : 'claw_r');
        if (side > 0) gR = g;
        const piv: Vec3 = [f[0] + side * w * 0.35, f[1], f[2]];
        s.parts.push(part('box', [w * 0.22, w * 0.25, w], add(piv, [0, 0, w / 2]), 'claw', g));
        addJoint(s, { group: g, pivot: piv, motion: [{ kind: 'swing', axis: [0, side < 0 ? 1 : -1, 0], amp: 20, rate: 2 }] });
      }
      if (!weaponsOf(s).some((x) => x.kind === 'melee')) addWeapon(s, 'melee', gR, 1.1);
      return true;
    }
    case 'tentacles': case 'tail': {
      const tail = kind === 'tail';
      if (tail && s.parts.some((p) => p.role === 'tail' && !p.group)) { forceTail(s); return true; }
      const n = tail ? 1 : clamp(Math.round(o.count ?? 4), 1, 8);
      const segs = tail ? 4 : 3;
      if (!room(s, n * segs)) return full(notes, tail ? 'tail' : 'tentacles');
      const seg = clamp((tail ? 0.3 : 0.22) * Math.min(sx, sz), 0.12, 1.5);
      for (let i = 0; i < n; i++) {
        const a = (i / n) * Math.PI * 2;
        const root: Vec3 = tail ? [B.c[0], B.c[1], B.min[2]] : [B.c[0] + Math.cos(a) * sx * 0.3, B.min[1], B.c[2] + Math.sin(a) * sz * 0.3];
        const dir: Vec3 = tail ? [0, 0, -1] : [0, -1, 0];
        let parent: string | undefined;
        for (let k = 0; k < segs; k++) {
          const g = uniqGroup(s, tail ? 'tail' : `tentacle${i}`);
          const r = seg * (1 - k * 0.18);
          const piv = add(root, [dir[0] * seg * 1.3 * k, dir[1] * seg * 1.3 * k, dir[2] * seg * 1.3 * k]);
          s.parts.push(part(tail ? 'sphere' : 'capsule', [r, tail ? r : r * 1.5, r], add(piv, [dir[0] * seg * 0.65, dir[1] * seg * 0.65, dir[2] * seg * 0.65]), tail ? 'tail' : 'tentacle', g));
          addJoint(s, { group: g, pivot: piv, ...(parent ? { parent } : {}), motion: [{ kind: 'wave', axis: tail ? [0, 1, 0] : [1, 0, 0], amp: 20, rate: tail ? 1 : 0.8, phase: i * 0.25 + k * 0.15 }] });
          parent = g;
        }
      }
      if (!tail && !weaponsOf(s).some((w) => w.kind === 'melee')) addWeapon(s, 'melee', undefined, 0.6);
      return true;
    }
    case 'seat': {
      const w = clamp(Math.min(sx, sz) * 0.45, 0.5, 2);
      const top = spot(s, 'top', B);
      if (!room(s, 2)) return full(notes, 'seat');
      s.parts.push(part('box', [w, w * 0.25, w], top, 'seat'), part('box', [w, w * 0.9, w * 0.2], add(top, [0, w * 0.45, -w * 0.45]), 'seatback'));
      s.behaviors = s.behaviors.filter((b) => b.type !== 'rideable');
      setRideable(s, add(top, [0, w * 0.3, 0]));
      return true;
    }
    case 'melee': {
      if (!weaponsOf(s).some((w) => w.kind === 'melee')) addWeapon(s, 'melee', undefined, 0.6);
      return true;
    }
    case 'heads': {
      // "a dragon with 3 heads": extra heads side by side, each looking around on its own neck joint
      const head = s.parts.find((p) => p.role === 'head' && p.solid !== false);
      const have = head ? 1 : 0;
      const n = clamp(Math.round(o.count ?? 2) - have, 1, 6);
      const d = head ? Math.max(...head.size) : clamp(Math.min(sx, sy) * 0.4, 0.3, 3);
      const base: Vec3 = head ? [...head.pos] as Vec3 : [B.c[0], B.max[1] + d * 0.3, B.max[2] - d * 0.3];
      if (!room(s, n)) return full(notes, 'heads');
      for (let i = 0; i < n; i++) {
        const side = i % 2 ? 1 : -1, k = Math.floor(i / 2) + 1;
        const g = uniqGroup(s, 'head');
        const pos: Vec3 = [base[0] + side * k * d * 1.1, base[1], base[2] - k * d * 0.2];
        s.parts.push(part('sphere', [d, d * 0.9, d * 1.1], pos, 'head', g));
        addJoint(s, { group: g, pivot: [pos[0], pos[1] - d * 0.4, pos[2] - d * 0.4], motion: [{ kind: 'aim', yaw: true, pitch: true, rate: 120, minPitch: -30, maxPitch: 30 }] });
      }
      return true;
    }
    case 'saw': {
      const d = clamp(Math.min(sy, sx) * 0.7, 0.4, 5);
      const f = spot(s, o.where ?? 'front', B);
      const g = uniqGroup(s, 'saw');
      if (!room(s, 1)) return full(notes, 'saw');
      s.parts.push(part('cylinder', [d, d * 0.05, d], add(f, [0, 0, d * 0.3]), 'saw', g, [0, 0, 90]));
      addJoint(s, { group: g, pivot: add(f, [0, 0, d * 0.3]), motion: [{ kind: 'spin', axis: [1, 0, 0], rate: 6 }] });
      addWeapon(s, 'melee', g, 1.4);
      return true;
    }
    case 'drill': {
      const k = clamp(Math.min(sx, sy) * 0.5, 0.3, 4) / 1.2;
      const f = spot(s, o.where ?? 'front', B);
      const g = uniqGroup(s, 'drill');
      if (!room(s, 1)) return full(notes, 'drill');
      s.parts.push(part('cone', [1.2 * k, 2.6 * k, 1.2 * k], add(f, [0, 0, 1.3 * k]), 'bit', g, [90, 0, 0]));
      addJoint(s, { group: g, pivot: f, motion: [{ kind: 'spin', axis: [0, 0, 1], rate: 5 }] });
      addWeapon(s, 'melee', g, 1.4);
      return true;
    }
  }
  return false;
}

function full(notes: string[], what: string): false { notes.push(`no room left for ${what} (part limit)`); return false; }
function forceJaw(s: ConstructSpec) {
  for (const p of s.parts) if (p.role === 'jaw' && !p.group) {
    const g = uniqGroup(s, 'jaw'); p.group = g;
    addJoint(s, { group: g, pivot: [p.pos[0], p.pos[1], p.pos[2] - p.size[2] / 2], motion: [{ kind: 'swing', axis: [1, 0, 0], amp: 15, rate: 2 }] });
  }
}
function forceTail(s: ConstructSpec) {
  for (const t of s.parts) if (t.role === 'tail' && !t.group) {
    const g = uniqGroup(s, 'tail'); t.group = g;
    addJoint(s, { group: g, pivot: [t.pos[0], t.pos[1], t.pos[2] + Math.max(...t.size) * 0.4], motion: [{ kind: 'wave', axis: [0, 1, 0], amp: 20, rate: 1 }] });
  }
}

// ------------------------------------------------------------------------------------ traits

/** A requested capability: from adjectives ("flying", "attack"), relative clauses ("that hunts drones") or orders. */
export interface Trait {
  loco?: LocoKind | 'fly';
  goal?: BrainGoal;
  target?: Ref | null;
  weapon?: WeaponKind | 'natural';
  fast?: number;
  ride?: boolean;
  word: string;
}

const ADJ_TRAITS: [RegExp, Omit<Trait, 'word'>][] = [
  [/^(flying|flyin|flyng|winged|soaring|airborne)$/, { loco: 'fly' }],
  [/^(walking|legged|striding|stomping|marching)$/, { loco: 'legs' }],
  [/^(swimming)$/, { loco: 'swim' }],
  [/^(crawling|creeping|scuttling)$/, { loco: 'legs' }],
  [/^(wheeled|motorized|motorised|driving|self-driving|rolling-robot)$/, { loco: 'wheels' }],
  [/^(attack|killer|assault|hunter|hunting|predator|berserk|rampaging|murder|murderous)$/, { goal: 'hunt', weapon: 'natural' }],
  [/^(war|battle|combat|armed|armored|armoured|military|weaponized|weaponised|gunship|fighter|warrior|tactical)$/, { weapon: 'natural' }],
  [/^(guard|sentry|security|guardian|watchdog|bodyguard|defender)$/, { goal: 'guard' }],
  [/^(patrol|patrolling)$/, { goal: 'patrol' }],
  [/^(pet|companion|buddy|sidekick|loyal|trusty|helper)$/, { goal: 'follow' }],
  [/^(racing|race|racer|speedy|turbo|sports|drag)$/, { fast: 1.8 }],
  [/^(rideable|ridable)$/, { ride: true }],
];
const PHRASE_TRAITS: [RegExp, Omit<Trait, 'word'>][] = [
  [/\b(fire ?breathing|flame ?breathing|fire ?spitting|flame ?throwing)\b/, { weapon: 'flame' }],
  [/\b(laser ?(shooting|eyed|firing))\b/, { weapon: 'laser' }],
  [/\b(missile ?(firing|launching|shooting))\b/, { weapon: 'missile' }],
  [/\b(gun ?toting|machine ?gun ?toting)\b/, { weapon: 'gun' }],
  [/\b(army|horde|legion|swarm|mob|battalion|platoon) of\b/, { goal: 'hunt', weapon: 'natural' }],
  [/\b(squad|pack|team|gang|flock|posse) of\b/, { goal: 'follow' }],
  [/\bself ?driving\b/, { loco: 'wheels' }],
];

/** Machine traits named by adjectives in a noun phrase ("a flying bathtub", "an attack helicopter", "an army of robots"). */
export function traitsFromWords(tokens: string[], raw = tokens.join(' ')): Trait[] {
  const out: Trait[] = [];
  for (const w of tokens) for (const [re, t] of ADJ_TRAITS) if (re.test(w)) out.push({ ...t, word: w });
  for (const [re, t] of PHRASE_TRAITS) { const m = re.exec(raw); if (m) out.push({ ...t, word: m[0] }); }
  return out;
}

const LOCO_VERBS: Record<string, LocoKind | 'fly'> = {
  fly: 'fly', glide: 'fly', soar: 'fly', flap: 'fly', drive: 'wheels', roll: 'wheels', race: 'wheels', zoom: 'wheels', walk: 'legs', run: 'legs', march: 'legs', stomp: 'legs',
  trot: 'legs', gallop: 'legs', hop: 'legs', crawl: 'legs', scuttle: 'legs', waddle: 'legs', stroll: 'legs', prowl: 'legs', swim: 'swim', hover: 'hover', float: 'hover', slither: 'slither',
};
export function locoForVerb(verb: string | null): LocoKind | 'fly' | null { return verb ? LOCO_VERBS[verb] ?? null : null; }

/** The locomotion a host would naturally use (concept defaults, else what its parts suggest). */
export function naturalLoco(s: ConstructSpec): LocoKind | null {
  const m = conceptOf(s)?.mach;
  if (m) return m.loco;
  if (s.parts.some((p) => p.role === 'wheel')) return 'wheels';
  if (s.parts.some((p) => p.role === 'leg')) return 'legs';
  if (s.parts.some((p) => p.role === 'rotor')) return 'rotor';
  if (s.parts.some((p) => p.role === 'wing') && conceptOf(s)?.cls === 'creature') return 'wings';
  return null;
}

/** Make sure the machine can move; picks the host's natural locomotion (or hard-light hover) and says so. */
export function ensureLocomotion(s: ConstructSpec, notes: string[], quiet = false): void {
  if (driveOf(s)) return;
  const loco = naturalLoco(s);
  const mach = conceptOf(s)?.mach ?? null;
  if (!loco) {
    attachKit(s, 'hover', { notes });
    if (!quiet) notes.push('no wheels, legs or wings asked for — it hovers on hard-light pads');
    return;
  }
  if (loco === 'legs' && !s.parts.some((p) => p.role === 'leg')) attachKit(s, 'legs', { notes });
  else if (loco === 'treads' && !s.parts.some((p) => p.role === 'tread')) attachKit(s, 'treads', { notes });
  else if (loco === 'wheels' && !s.parts.some((p) => p.role === 'wheel')) attachKit(s, 'wheels', { notes });
  else if (loco === 'rotor' && !s.parts.some((p) => p.role === 'rotor')) attachKit(s, 'rotor', { notes });
  else if (loco === 'wings' && !s.parts.some((p) => p.role === 'wing')) attachKit(s, 'wings', { notes });
  else if (loco === 'propeller' && !s.parts.some((p) => p.role === 'blade' || p.role === 'wing')) attachKit(s, 'jets', { notes });
  else articulate(s);
  if (!driveOf(s)) setDrive(s, loco, mach);
}

/** Give the host a way to fly that suits it: flapping wings for creatures, lift jets for everything else. */
function makeFly(s: ConstructSpec, notes: string[]) {
  const c = conceptOf(s);
  const d = driveOf(s);
  if (d && d.mode === 'air') return;
  if (s.parts.some((p) => p.role === 'rotor')) { articulate(s); setDrive(s, 'rotor', c?.mach); return; }
  if (s.parts.some((p) => p.role === 'wing')) { articulate(s, { creature: c?.cls === 'creature' }); setDrive(s, 'wings', c?.mach); return; }
  if (c?.cls === 'creature') attachKit(s, 'wings', { notes });
  else attachKit(s, 'jets', { notes });
}

/** Natural weapon for a host that is told to attack but has nothing to attack with. */
export function ensureWeapon(s: ConstructSpec, notes: string[], kind: WeaponKind | 'natural' = 'natural') {
  const ws = weaponsOf(s);
  if (kind !== 'natural') { if (!ws.some((w) => w.kind === kind)) attachKit(s, kind === 'melee' ? 'melee' : 'gun', { weapon: kind, notes }); return; }
  if (ws.length) return;
  // convert a legacy 'shoot' behaviour into a weapon (turret concept)
  if (s.behaviors.some((b) => b.type === 'shoot')) {
    s.behaviors = s.behaviors.filter((b) => b.type !== 'shoot');
    articulate(s);
    const aimG = (s.joints ?? []).find((j) => j.motion.some((m) => m.kind === 'aim'))?.group;
    addWeapon(s, 'cannon', aimG);
    return;
  }
  const c = conceptOf(s);
  const melee = c?.mach?.melee ?? (c?.cls === 'creature' ? 'jaws' : null);
  if (s.parts.some((p) => ['spike', 'blade', 'saw', 'bit', 'claw', 'fist'].includes(p.role ?? ''))) { addWeapon(s, 'melee', undefined, 0.8); return; }
  if (melee === 'jaws' && attachKit(s, 'jaws', { notes })) return;
  if (melee === 'arms' && attachKit(s, 'arms', { notes })) return;
  if (melee === 'claws' && attachKit(s, 'claws', { notes })) return;
  if (melee === 'sting') { addWeapon(s, 'melee', undefined, 0.5); return; }
  if (c?.cls === 'vehicle' || c?.cls === 'weapon') { attachKit(s, 'turret', { notes, weapon: 'cannon' }); return; }
  addWeapon(s, 'melee', undefined, 0.8);
  notes.push('it has no weapon — it will ram them');
}

/** Apply requested traits to a host (visible notes go to `notes`). */
export function applyTraits(s: ConstructSpec, traits: Trait[], notes: string[]) {
  const ROLE: Partial<Record<LocoKind, string>> = { legs: 'leg', wheels: 'wheel', treads: 'tread', wings: 'wing', rotor: 'rotor', propeller: 'blade', jets: 'thruster', hover: 'pad' };
  for (const t of traits) {
    const mach = conceptOf(s)?.mach ?? null;
    if (t.loco === 'fly') makeFly(s, notes);
    else if (t.loco === 'slither' || t.loco === 'roll') { articulate(s); setDrive(s, t.loco, mach); }
    else if (t.loco === 'swim') {
      articulate(s);
      const d = driveOf(s);
      if (!d || d.mode === 'ground') { s.behaviors = s.behaviors.filter((b) => b.type !== 'drive'); setDrive(s, 'swim', mach); }
    } else if (t.loco) {
      const d = driveOf(s);
      const role = ROLE[t.loco];
      if (!(d && d.mode === LOCO_DRIVE[t.loco].mode && (!role || s.parts.some((p) => p.role === role)))) attachKit(s, t.loco as MachineKind, { notes });
    }
    if (t.weapon) ensureWeapon(s, notes, t.weapon);
    if (t.goal && !brainOf(s)) setBrain(s, t.goal, t.target ?? (t.goal === 'follow' ? { kind: 'self' } : t.goal === 'hunt' ? { kind: 'all_enemies' } : null));
    if (t.ride) setRideable(s);
    if (t.fast) {
      if (!driveOf(s)) ensureLocomotion(s, notes, true);
      const d = driveOf(s);
      if (d) d.speed *= t.fast;
    }
  }
}

/**
 * Make a spec that asked for machine behaviour consistent: moving goals need locomotion, attacks need
 * a weapon, weapons with no rider get a sentry brain, autonomous things leave the player's hands.
 */
export function finalizeMachine(s: ConstructSpec, notes: string[], opts: { keepMount?: boolean } = {}) {
  if (!isMachine(s)) return;
  const brain0 = brainOf(s);
  const moving = !!brain0 && MOVING_GOALS.has(brain0.goal);
  if ((moving || isRideable(s)) && !driveOf(s)) ensureLocomotion(s, notes);
  if (brain0?.goal === 'hunt' || brain0?.goal === 'guard' && (driveOf(s) || conceptOf(s)?.mach)) ensureWeapon(s, notes);
  if (s.behaviors.some((b) => b.type === 'shoot') && (driveOf(s) || brain0)) ensureWeapon(s, notes);
  if (weaponsOf(s).length === 0 && driveOf(s) && s.parts.some((p) => ['spike', 'blade'].includes(p.role ?? ''))) addWeapon(s, 'melee', undefined, 0.8);
  const mounted = s.behaviors.some((b) => b.type === 'mount') && s.placement.at === 'self';
  const held = mounted || s.behaviors.some((b) => b.type === 'hold' || b.type === 'mount') || ['hand', 'self'].includes(s.placement.at);
  // armed but nobody to aim it: a sentry that engages hostiles that come near
  if (weaponsOf(s).length && !brain0 && !held && !isRideable(s)) setBrain(s, 'guard');
  // something that can move but was given no job is yours to ride ("a couch on wheels", "a flying bathtub")
  if (driveOf(s) && !brainOf(s) && !isRideable(s) && !s.behaviors.some((b) => b.type === 'mount' || b.type === 'hold')) setRideable(s);
  // placements that need a target make no sense for a free-standing machine
  if (driveOf(s) && ['under_target', 'above_target', 'around_target', 'target'].includes(s.placement.at) && !s.placement.target) s.placement = { ...s.placement, at: driveOf(s)!.mode === 'ground' ? 'ground_front' : 'front', orient: 'face_away' };
  if (driveOf(s) && !(s.joints ?? []).length) articulate(s);
  if ((moving || brain0?.goal === 'guard') && !opts.keepMount) {
    // autonomous: it leaves your hand / you get off it, and it stands on its own
    s.behaviors = s.behaviors.filter((b) => !['hold', 'follow', 'home', 'orbit'].includes(b.type) && !(b.type === 'mount' && moving));
    if (['hand', 'self', 'under_self', 'above_self', 'behind_self'].includes(s.placement.at) || mounted) {
      s.placement = { ...s.placement, at: driveOf(s)?.mode === 'ground' ? 'ground_front' : 'front', orient: 'face_away', target: undefined, distance: s.placement.distance ?? 6 };
    }
  }
  if (driveOf(s) && s.motion === 'static') s.motion = 'kinematic';
  const c = conceptOf(s);
  if (c?.mach?.rideable && driveOf(s) && !isRideable(s)) setRideable(s);
}

// ------------------------------------------------------------------------------------ claims

const RANGED = new Set(['gun', 'cannon', 'laser', 'missile', 'flame', 'net', 'bomb']);

/** Functional claims (eval/EVAL_SPEC_MACHINES.md) for a spec's machine behaviours. */
export function machineFunctions(s: ConstructSpec): string[] {
  const f = new Set<string>();
  const drive = driveOf(s), brain = brainOf(s), ws = weaponsOf(s), joints = s.joints ?? [];
  const moving = !!brain && MOVING_GOALS.has(brain.goal);
  const ride = isRideable(s) || s.behaviors.some((b) => b.type === 'mount' && b.mode === 'ride');
  const motions = joints.flatMap((j) => j.motion);
  if (joints.length) f.add('has_moving_parts');
  if (motions.some((m) => m.kind === 'spin')) f.add('spin');
  if (drive) {
    if (moving) { f.add('moves_autonomously'); if (s.count > 1) f.add('multiple_agents'); }
    if (moving || ride) {
      if (drive.mode === 'ground') f.add('drives_on_ground');
      if (drive.mode === 'air' || drive.mode === 'hover' && (drive.altitude ?? 0) >= 3) f.add('flies');
      if (drive.mode === 'ground' && motions.some((m) => m.kind === 'swing' && m.gait)) f.add('walks');
    }
    if (motions.some((m) => m.kind === 'roll')) f.add('wheels_spin');
    if (isRideable(s)) { f.add('rideable'); f.add('carries_player'); f.add('propel_player'); }
  }
  if (motions.some((m) => m.kind === 'aim') && (brain || ws.length)) f.add('aims_at_target');
  if (ws.some((w) => RANGED.has(w.kind))) { f.add('shoots'); f.add('projectile_attack'); }
  const engaged = !!brain && ['hunt', 'guard'].includes(brain.goal);
  if (ws.length && engaged) {
    f.add('attacks_enemies');
    if (ws.some((w) => w.kind === 'melee')) { f.add('strike_target'); }
    const t = brain!.target;
    if (brain!.goal === 'hunt' && t && (t.kind === 'entity' || t.kind === 'entities' || t.kind === 'nearest_enemy' || t.kind === 'all_enemies')) { f.add('damages_target'); f.add('destroys_target'); }
  }
  if (ws.some((w) => w.kind === 'net') && engaged) f.add('restrain_target');
  if (ws.some((w) => w.kind === 'flame' || w.kind === 'bomb' || w.kind === 'missile') && engaged) f.add('area_attack');
  if (brain) {
    const selfT = !brain.target || brain.target.kind === 'self';
    if (brain.goal === 'follow' && selfT) { f.add('follows_player'); f.add('returns'); }
    if (brain.goal === 'guard') { f.add('guards_area'); if (brain.target && brain.target.kind !== 'point' && brain.target.kind !== 'self') f.add('shield_other'); }
    if (brain.goal === 'fetch') { f.add('fetches'); f.add('grab_target'); f.add('pull_target'); }
  }
  return [...f];
}
