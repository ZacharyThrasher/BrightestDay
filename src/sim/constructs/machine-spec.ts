/**
 * Spec-level machine logic (pure data → data, no physics): turning legacy behaviours on a vehicle/creature into
 * machine behaviours, rigging joints from the catalog or from part roles, the validator's clamps and energy cost,
 * and the functional claims a machine makes (for the effect report).
 */
import type { ConstructSpec, Behavior, Joint, Part, Vec3 } from './spec.js';
import { cloneSpec } from './spec.js';
import { CATALOG_BY_ID, catalogJoints } from './catalog/index.js';

type Drive = Extract<Behavior, { type: 'drive' }>;
type Brain = Extract<Behavior, { type: 'brain' }>;
type Weapon = Extract<Behavior, { type: 'weapon' }>;

export const MACHINE_LIMITS = {
  groundSpeed: 40,
  airSpeed: 60,
  hoverSpeed: 40,
  turn: 540, // deg/s
  altitude: 60,
  jump: 20,
  weaponRate: 12, // shots/s
  weaponDamage: 60,
  weaponRange: 80,
  weaponSpeed: 95,
  weaponSize: 1.5,
  weapons: 4,
  brainRange: 60,
  joints: 24,
  motionsPerJoint: 4,
  spinRate: 8, // rev/s
  swingAmp: 150, // deg
  swingRate: 6, // cycles/s
  pistonAmp: 3, // m
  aimRate: 720, // deg/s
};

const MACHINE_TYPES = new Set(['drive', 'brain', 'weapon', 'rideable']);

export function isMachine(s: ConstructSpec): boolean {
  return s.behaviors.some((b) => b.type === 'drive');
}

const find = <T extends Behavior['type']>(s: ConstructSpec, t: T) => s.behaviors.find((b) => b.type === t) as Extract<Behavior, { type: T }> | undefined;

/**
 * Machines move themselves, so the classic "driving" behaviours become orders: follow → brain follow,
 * strike/home → hunt (+ a ramming melee weapon), grab → fetch, shoot → a gun + hunt, mount → rideable,
 * thrust → a faster drive. Wings / rotors / propellers on a ground machine make it fly.
 * A brain with no drive gets a ground drive (a creature that was asked to do something has legs).
 */
export function adaptMachine(s: ConstructSpec): void {
  const hasMachine = s.behaviors.some((b) => MACHINE_TYPES.has(b.type));
  if (!hasMachine) return;
  let drive = find(s, 'drive');
  if (!drive && find(s, 'brain') && s.motion !== 'static') {
    drive = { type: 'drive', mode: 'ground', speed: 8, turn: 180 };
    s.behaviors.push(drive);
  }
  if (!drive) return;
  const out: Behavior[] = [];
  // an explicit order from the language layer (strike / follow / grab / shoot ...) replaces a default brain
  let ordered: Brain | null = null;
  const setBrain = (b: Brain) => { if (!ordered) ordered = b; };
  const hasWeapon = () => s.behaviors.some((b) => b.type === 'weapon') || out.some((b) => b.type === 'weapon');
  for (const b of s.behaviors) {
    switch (b.type) {
      case 'follow': setBrain({ type: 'brain', goal: 'follow', target: b.target }); break;
      case 'orbit': setBrain({ type: 'brain', goal: 'follow', target: b.around }); break;
      case 'strike': case 'home': case 'crush':
        setBrain({ type: 'brain', goal: 'hunt', target: b.target });
        if (!hasWeapon()) out.push({ type: 'weapon', kind: 'melee', rate: 1.2, speed: 0, damage: Math.min(40, 10 + ((b as any).speed ?? 20) * 0.4), range: 1.8 });
        break;
      case 'grab': setBrain({ type: 'brain', goal: 'fetch', target: b.target }); break;
      case 'shoot':
        if (!hasWeapon()) out.push({ type: 'weapon', kind: b.size >= 0.5 ? 'cannon' : 'gun', rate: b.rate, speed: b.speed, damage: b.size >= 0.5 ? 30 : 10, range: 40, size: b.size });
        setBrain({ type: 'brain', goal: 'hunt', target: b.target });
        break;
      case 'mount':
        if (!find(s, 'rideable')) out.push({ type: 'rideable', seat: seatFor(s.parts) });
        break;
      case 'thrust': drive.speed *= 1.35; break;
      case 'hold': case 'spin': case 'sweep': case 'lift': break; // the machine moves (and animates) itself
      default: out.push(b);
    }
  }
  // flying add-ons on a ground machine: it flies now ("a car with wings")
  if (drive.mode === 'ground' && s.parts.some((p) => !p.group && ['wing', 'propeller', 'rotor', 'blade'].includes(p.role ?? ''))) {
    drive.mode = 'air';
    drive.altitude = drive.altitude ?? 8;
  }
  if (ordered) {
    const o: Brain = ordered;
    const i = out.findIndex((b) => b.type === 'brain');
    if (i >= 0) out[i] = { ...o, range: (out[i] as Brain).range ?? o.range }; else out.push(o);
  }
  s.behaviors = out;
  if (s.motion !== 'kinematic') s.motion = 'kinematic';
}

/** Rider seat on top of the solid parts, a little behind the middle. */
export function seatFor(parts: Part[]): Vec3 {
  let top = -Infinity, zmin = Infinity, zmax = -Infinity;
  for (const p of parts) {
    if (p.solid === false) continue;
    top = Math.max(top, p.pos[1] + p.size[1] / 2);
    zmin = Math.min(zmin, p.pos[2] - p.size[2] / 2);
    zmax = Math.max(zmax, p.pos[2] + p.size[2] / 2);
  }
  if (!isFinite(top)) return [0, 1.5, 0];
  return [0, top + 0.9, (zmin + zmax) / 2 - (zmax - zmin) * 0.1];
}

/**
 * Give a machine joints: first from the catalog (its grouped parts lost their joints, e.g. after an edit), then
 * from part roles on ungrouped parts (wheels roll, rotors spin, legs walk, wings flap, tails wag).
 */
export function rigMachine(s: ConstructSpec): void {
  const joints: Joint[] = [...(s.joints ?? [])];
  const has = new Set(joints.map((j) => j.group));
  const grouped = new Set(s.parts.map((p) => p.group).filter((g): g is string => !!g));
  for (const j of catalogJoints(s.concept)) {
    if (!grouped.has(j.group)) continue;
    const i = joints.findIndex((x) => x.group === j.group);
    if (i < 0) { joints.push(j); has.add(j.group); continue; }
    // a generic default with a foreign pivot (same kinds of motion, e.g. another form's stock turret joint) is
    // replaced by the catalog's exact joint; edits (same pivot, new rates) and different motions are kept
    const kinds = new Set(j.motion.map((m) => m.kind));
    const moved = Math.hypot(joints[i].pivot[0] - j.pivot[0], joints[i].pivot[1] - j.pivot[1], joints[i].pivot[2] - j.pivot[2]) > 1e-3;
    if (moved && joints[i].motion.every((m) => kinds.has(m.kind))) joints[i] = j;
  }
  if (isMachine(s)) autoRig(s, joints, has);
  if (joints.length) s.joints = joints;
}

function autoRig(s: ConstructSpec, joints: Joint[], has: Set<string>) {
  const drive = find(s, 'drive')!;
  const flying = drive.mode !== 'ground';
  let n = 0;
  const free = (p: Part) => !p.group;
  // wheels (and anything sharing their centre, e.g. a tyre torus)
  const wheels = s.parts.filter((p) => free(p) && p.role === 'wheel');
  for (const w of wheels) {
    if (w.group) continue;
    const g = `auto_wheel${n++}`;
    for (const p of s.parts) if (free(p) && Math.hypot(p.pos[0] - w.pos[0], p.pos[1] - w.pos[1], p.pos[2] - w.pos[2]) < 1e-3) p.group = g;
    const r = Math.max(w.size[0], w.size[2]) / 2;
    const axis = axisOf(w.rot);
    joints.push({ group: g, pivot: [...w.pos] as Vec3, motion: [{ kind: 'roll', axis, radius: Math.max(0.05, r) }] });
    has.add(g);
  }
  // rotors / propellers: one spinning group per hub position
  const blades = s.parts.filter((p) => free(p) && ['propeller', 'rotor', 'blade', 'hub'].includes(p.role ?? ''));
  if (blades.length && blades.some((p) => p.role !== 'hub')) {
    const clusters: Part[][] = [];
    for (const b of blades) {
      const c = clusters.find((cl) => Math.hypot(cl[0].pos[0] - b.pos[0], cl[0].pos[1] - b.pos[1], cl[0].pos[2] - b.pos[2]) < Math.max(0.6, Math.max(...cl[0].size) * 0.3));
      if (c) c.push(b); else clusters.push([b]);
    }
    for (const cl of clusters) {
      const g = `auto_rotor${n++}`;
      const hub = cl.find((p) => p.role === 'hub');
      const blade = cl.find((p) => p.role !== 'hub')!;
      const axis: Vec3 = hub ? axisOf(hub.rot) : thinAxis(blade);
      for (const p of cl) p.group = g;
      joints.push({ group: g, pivot: [...(hub ?? blade).pos] as Vec3, motion: [{ kind: 'spin', axis, rate: flying ? 5 : 3 }] });
      has.add(g);
    }
  }
  // legs walk (alternating), wings flap when flying, tails wag — unless the body is already rigged (catalog creatures)
  if (joints.length > n) return;
  const legs = s.parts.filter((p) => free(p) && ['leg', 'foot'].includes(p.role ?? '') && p.role === 'leg');
  legs.sort((a, b) => a.pos[2] - b.pos[2] || a.pos[0] - b.pos[0]);
  legs.forEach((l, i) => {
    const g = `auto_leg${n++}`;
    l.group = g;
    const top: Vec3 = [l.pos[0], l.pos[1] + l.size[1] / 2, l.pos[2]];
    const side = l.pos[0] < 0 ? 0 : 1;
    joints.push({ group: g, pivot: top, motion: [{ kind: 'swing', axis: [1, 0, 0], amp: 28, rate: 2, phase: ((i >> 1) + side) % 2 ? 0.5 : 0, gait: true }] });
    has.add(g);
  });
  if (flying) {
    for (const wng of s.parts.filter((p) => free(p) && p.role === 'wing')) {
      if (Math.abs(wng.pos[0]) < 0.3) continue; // a single central wing (plane) stays rigid
      const g = `auto_wing${n++}`;
      wng.group = g;
      const sgn = Math.sign(wng.pos[0]);
      joints.push({ group: g, pivot: [sgn * Math.max(0.1, Math.abs(wng.pos[0]) - wng.size[0] / 2), wng.pos[1], wng.pos[2]], motion: [{ kind: 'swing', axis: [0, 0, sgn], amp: 30, rate: 1.4 }] });
      has.add(g);
    }
  }
  for (const t of s.parts.filter((p) => free(p) && p.role === 'tail')) {
    if (flying && t.size[1] > 2) continue; // helicopter/plane tail booms stay put
    const g = `auto_tail${n++}`;
    t.group = g;
    joints.push({ group: g, pivot: [t.pos[0], t.pos[1], t.pos[2] + t.size[1] / 2], motion: [{ kind: 'swing', axis: [0, 1, 0], amp: 25, rate: 1.6 }] });
    has.add(g);
  }
}

function axisOf(rot?: Vec3): Vec3 {
  // a cylinder's axis is its local y; rotate it by the part rotation (XYZ euler, degrees)
  const [a, b, c] = (rot ?? [0, 0, 0]).map((d) => (d * Math.PI) / 180);
  // R = Rx(a) Ry(b) Rz(c) applied to (0,1,0)
  let x = -Math.sin(c), y = Math.cos(c), z = 0;
  // Ry(b)
  const x2 = x * Math.cos(b) + z * Math.sin(b); const z2 = -x * Math.sin(b) + z * Math.cos(b); x = x2; z = z2;
  // Rx(a)
  const y3 = y * Math.cos(a) - z * Math.sin(a); const z3 = y * Math.sin(a) + z * Math.cos(a); y = y3; z = z3;
  const r = (v: number) => Math.round(v * 1000) / 1000;
  return [r(x), r(y), r(z)];
}

function thinAxis(p: Part): Vec3 {
  const i = p.size.indexOf(Math.min(...p.size));
  return [i === 0 ? 1 : 0, i === 1 ? 1 : 0, i === 2 ? 1 : 0];
}

// ------------------------------------------------------------------ validation

const clampN = (v: number | undefined, lo: number, hi: number, dflt: number) => {
  const x = v === undefined || !isFinite(v) ? dflt : v;
  return Math.min(hi, Math.max(lo, x));
};
const finiteVec = (v: any, lim = 40): v is Vec3 => Array.isArray(v) && v.length === 3 && v.every((x) => typeof x === 'number' && isFinite(x) && Math.abs(x) <= lim * 4);

/** Clamp machine behaviours. Returns null to drop one. */
export function clampMachineBehavior(b: Behavior, note: (m: string) => void): Behavior | null {
  const c = (v: number | undefined, lo: number, hi: number, what: string, dflt: number) => {
    const x = v === undefined || !isFinite(v) ? dflt : v;
    if (x > hi) { note(`${what} capped`); return hi; }
    return Math.max(lo, x);
  };
  switch (b.type) {
    case 'drive': {
      const mode = b.mode === 'air' || b.mode === 'hover' ? b.mode : 'ground';
      const max = mode === 'air' ? MACHINE_LIMITS.airSpeed : mode === 'hover' ? MACHINE_LIMITS.hoverSpeed : MACHINE_LIMITS.groundSpeed;
      const d: Drive = { type: 'drive', mode, speed: c(b.speed, 0.5, max, 'top speed', 8), turn: c(b.turn, 10, MACHINE_LIMITS.turn, 'turn rate', 120) };
      if (b.altitude !== undefined || mode !== 'ground') d.altitude = c(b.altitude, mode === 'hover' ? 0.3 : 2, MACHINE_LIMITS.altitude, 'flight altitude', mode === 'hover' ? 1.2 : 8);
      if (b.jump !== undefined) d.jump = c(b.jump, 0, MACHINE_LIMITS.jump, 'jump', 8);
      if (b.minSpeed !== undefined) d.minSpeed = Math.min(d.speed * 0.8, c(b.minSpeed, 0, max, 'stall speed', 0));
      return d;
    }
    case 'brain': {
      const goals = ['wander', 'follow', 'guard', 'hunt', 'fetch', 'patrol', 'goto', 'flee', 'idle'];
      if (!goals.includes(b.goal)) return null;
      return { ...b, range: b.range !== undefined ? c(b.range, 2, MACHINE_LIMITS.brainRange, 'engage range', 30) : undefined };
    }
    case 'weapon': {
      const kinds = ['gun', 'cannon', 'laser', 'missile', 'flame', 'net', 'melee', 'bomb'];
      if (!kinds.includes(b.kind)) return null;
      const w: Weapon = {
        ...b,
        rate: c(b.rate, 0.1, MACHINE_LIMITS.weaponRate, 'fire rate', 2),
        speed: c(b.speed, 0, MACHINE_LIMITS.weaponSpeed, 'shot speed', 50),
        damage: c(b.damage, 0, MACHINE_LIMITS.weaponDamage, 'weapon damage', 10),
        range: c(b.range, 0.5, MACHINE_LIMITS.weaponRange, 'weapon range', 30),
      };
      if (b.size !== undefined) w.size = c(b.size, 0.05, MACHINE_LIMITS.weaponSize, 'shot size', 0.3);
      if (b.spread !== undefined) w.spread = c(b.spread, 0, 30, 'spread', 0);
      return w;
    }
    case 'rideable':
      if (!finiteVec(b.seat)) return { type: 'rideable', seat: [0, 1.5, 0], view: b.view };
      return { ...b, seat: b.seat.map((x) => clampN(x, -40, 40, 0)) as Vec3 };
    default: return b;
  }
}

/** One drive / brain / rideable, at most N weapons. */
export function dedupeMachine(s: ConstructSpec, note: (m: string) => void) {
  const seen = new Set<string>();
  let weapons = 0;
  s.behaviors = s.behaviors.filter((b) => {
    if (b.type === 'weapon') { if (++weapons > MACHINE_LIMITS.weapons) { note(`at most ${MACHINE_LIMITS.weapons} weapons`); return false; } return true; }
    if (b.type === 'drive' || b.type === 'brain' || b.type === 'rideable') { if (seen.has(b.type)) return false; seen.add(b.type); }
    return true;
  });
}

/** Validate joints: drop malformed / orphaned ones, clamp rates and amplitudes, cap the count. */
export function clampJoints(s: ConstructSpec, note: (m: string) => void) {
  if (!s.joints) return;
  if (!Array.isArray(s.joints)) { delete s.joints; return; }
  const groups = new Set(s.parts.map((p) => p.group).filter(Boolean));
  const out: Joint[] = [];
  const seen = new Set<string>();
  for (const j of s.joints) {
    if (!j || typeof j.group !== 'string' || !groups.has(j.group) || seen.has(j.group)) continue;
    if (!finiteVec(j.pivot)) continue;
    if (!Array.isArray(j.motion)) continue;
    seen.add(j.group);
    const motion = j.motion.slice(0, MACHINE_LIMITS.motionsPerJoint).map((m: any) => {
      const axis: Vec3 = finiteVec(m.axis) && Math.hypot(...m.axis) > 1e-6 ? m.axis : [0, 1, 0];
      switch (m.kind) {
        case 'spin': { const r = clampN(m.rate, -MACHINE_LIMITS.spinRate, MACHINE_LIMITS.spinRate, 2); if (Math.abs(m.rate) > MACHINE_LIMITS.spinRate) note('spin rate capped'); return { kind: 'spin', axis, rate: r }; }
        case 'roll': return { kind: 'roll', axis, radius: clampN(m.radius, 0.05, 20, 0.5) };
        case 'steer': return { kind: 'steer', axis, amp: clampN(m.amp, -80, 80, 25) };
        case 'aim': return { kind: 'aim', yaw: !!m.yaw || (!m.yaw && !m.pitch), pitch: !!m.pitch, rate: clampN(m.rate, 5, MACHINE_LIMITS.aimRate, 90), minPitch: clampN(m.minPitch, -89, 89, -30), maxPitch: clampN(m.maxPitch, -89, 89, 60) };
        case 'swing': case 'piston': case 'wave': {
          const ampMax = m.kind === 'piston' ? MACHINE_LIMITS.pistonAmp : MACHINE_LIMITS.swingAmp;
          const o: any = { kind: m.kind, axis, amp: clampN(m.amp, -ampMax, ampMax, m.kind === 'piston' ? 0.2 : 20), rate: clampN(m.rate, 0, MACHINE_LIMITS.swingRate, 1) };
          if (m.phase !== undefined) o.phase = clampN(m.phase, -10, 10, 0);
          if (m.gait && m.kind !== 'wave') o.gait = true;
          if (m.on === 'fire' && m.kind !== 'wave') o.on = 'fire';
          return o;
        }
        default: return null;
      }
    }).filter(Boolean);
    const jj: Joint = { group: j.group, pivot: j.pivot.map((x) => clampN(x, -40, 40, 0)) as Vec3, motion };
    if (typeof j.parent === 'string' && j.parent !== j.group) jj.parent = j.parent;
    out.push(jj);
  }
  // parents must exist; break cycles
  const byG = new Map(out.map((j) => [j.group, j]));
  for (const j of out) {
    if (j.parent && !byG.has(j.parent)) delete j.parent;
    let p = j.parent, guard = 0;
    while (p && guard++ < 32) { if (p === j.group) { delete j.parent; break; } p = byG.get(p)?.parent; }
  }
  if (out.length > MACHINE_LIMITS.joints) { note(`simplified to ${MACHINE_LIMITS.joints} moving parts`); out.length = MACHINE_LIMITS.joints; }
  if (out.length) s.joints = out; else delete s.joints;
}

/** Willpower cost of the machine features (on top of geometry + behaviour count). */
export function machineCost(s: ConstructSpec): number {
  let k = 0;
  for (const b of s.behaviors) {
    if (b.type === 'drive') k += 3 + b.speed * (b.mode === 'ground' ? 0.12 : 0.1) + (b.mode !== 'ground' ? 2 : 0);
    else if (b.type === 'brain') k += 3;
    else if (b.type === 'weapon') k += 2 + Math.min(14, b.damage * b.rate * 0.06);
    else if (b.type === 'rideable') k += 1;
  }
  k += (s.joints?.length ?? 0) * 0.25;
  return k * s.count;
}

// ------------------------------------------------------------------ claims

/** Functional effects a (proposed) machine claims, for the effect report / eval. Empty for non-machines. */
export function machineFunctions(spec: ConstructSpec): string[] {
  if (!spec.behaviors.some((b) => MACHINE_TYPES.has(b.type))) return [];
  const s = cloneSpec(spec);
  adaptMachine(s);
  rigMachine(s);
  const f = new Set<string>();
  for (const id of s.concept.split(/[+,\s]/)) for (const x of CATALOG_BY_ID.get(id)?.functions ?? []) f.add(x);
  const drive = find(s, 'drive');
  const brain = find(s, 'brain');
  const weapons = s.behaviors.filter((b): b is Weapon => b.type === 'weapon');
  const ride = find(s, 'rideable');
  const motions = (s.joints ?? []).flatMap((j) => j.motion);
  if (s.joints?.length) f.add('has_moving_parts');
  if (motions.some((m) => m.kind === 'roll')) f.add('wheels_spin');
  if (motions.some((m) => m.kind === 'aim') && (weapons.length || brain)) f.add('aims_at_target');
  if (drive) {
    if (brain && !['idle', 'follow', 'guard'].includes(brain.goal)) f.add('moves_autonomously');
    if (!ride && !brain) f.add('moves_autonomously');
    if (drive.mode === 'ground') {
      f.add('drives_on_ground');
      f.add('pushes_objects');
      if (motions.some((m) => (m.kind === 'swing' || m.kind === 'piston') && m.gait)) f.add('walks');
    } else if (drive.mode === 'air') f.add('flies');
    if (s.count > 1 && (brain || !ride)) f.add('multiple_agents');
  }
  if (weapons.length) {
    f.add('attacks_enemies');
    f.add('damages_target');
    if (weapons.some((w) => w.kind !== 'melee')) { f.add('shoots'); f.add('projectile_attack'); }
    else f.add('strike_target');
    if (weapons.some((w) => w.damage * w.rate >= 20)) f.add('destroys_target');
  }
  if (brain) {
    switch (brain.goal) {
      case 'follow': f.add('follows_player'); f.add('returns'); break;
      case 'guard': f.add('guards_area'); break;
      case 'fetch': f.add('fetches'); f.add('grab_target'); f.add('returns'); f.add('pull_target'); break;
      case 'hunt': if (weapons.length) { f.add('strike_target'); } break;
    }
  }
  if (ride) { f.add('rideable'); f.add('carries_player'); f.add('propel_player'); if (drive?.mode === 'air') f.add('raise_player'); }
  // a catalog creature that was given wings flies now (and vice versa): drop claims its locomotion no longer supports
  if (drive?.mode === 'air') { f.delete('drives_on_ground'); f.delete('walks'); f.delete('pushes_objects'); }
  else if (drive) f.delete('flies');
  return [...f];
}
