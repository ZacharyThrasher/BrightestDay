/**
 * Building blocks for catalog machines. Everything here returns plain spec data (parts / joints),
 * so an entry stays declarative and can be validated like any other proposal.
 *
 * Frame: +z forward, +y up, +x to one side. Units: metres (unscaled), degrees.
 */
import { Quaternion, Vector3, Euler } from 'three';
import type { Part, Joint, JointMotion, Shape, Vec3 } from '../spec.js';

export interface PartOpts { role?: string; rot?: Vec3; group?: string; solid?: boolean }

/** One part. `solid: false` = visual only (no collider). */
export function P(shape: Shape, size: Vec3, pos: Vec3, o: PartOpts = {}): Part {
  const p: Part = { shape, size, pos };
  if (o.rot) p.rot = o.rot;
  if (o.role) p.role = o.role;
  if (o.solid === false) p.solid = false;
  if (o.group) p.group = o.group;
  return p;
}

/** Put every part in a group (returns the same array). */
export function inGroup(group: string, parts: Part[]): Part[] {
  for (const p of parts) p.group = group;
  return parts;
}

/**
 * A wheel whose axle runs along x: a tyre plus a spoke bar so the rotation is visible
 * (a bare cylinder looks identical at every angle).
 */
export function wheel(group: string, pos: Vec3, diameter: number, width: number, solid = false): Part[] {
  return [
    P('cylinder', [diameter, width, diameter], pos, { role: 'wheel', rot: [0, 0, 90], group, solid }),
    P('box', [width * 1.08, diameter * 0.86, diameter * 0.16], pos, { role: 'hub', group, solid: false }),
  ];
}

/** Capsule/cylinder/cone from point a to point b (local +y of the shape runs a→b). */
export function limb(shape: Shape, a: Vec3, b: Vec3, thickness: number, o: PartOpts = {}): Part {
  const va = new Vector3(...a), vb = new Vector3(...b);
  const d = vb.clone().sub(va);
  const len = d.length();
  const q = new Quaternion().setFromUnitVectors(new Vector3(0, 1, 0), d.clone().normalize());
  const e = new Euler().setFromQuaternion(q, 'XYZ');
  const r2 = (x: number) => Math.round(x * 1000) / 1000;
  const rot: Vec3 = [r2((e.x * 180) / Math.PI), r2((e.y * 180) / Math.PI), r2((e.z * 180) / Math.PI)];
  const mid = va.add(vb).multiplyScalar(0.5);
  return P(shape, [thickness, len, thickness], [r2(mid.x), r2(mid.y), r2(mid.z)], { ...o, rot });
}

/** Mirror a part across x = 0 (left ↔ right), renaming its group with `rename`. */
export function mirror(p: Part, rename?: (g: string) => string): Part {
  const q: Part = { ...p, pos: [-p.pos[0], p.pos[1], p.pos[2]], size: [...p.size] as Vec3 };
  if (p.rot) q.rot = [p.rot[0], -p.rot[1], -p.rot[2]];
  if (p.group && rename) q.group = rename(p.group);
  return q;
}

export const J = (group: string, pivot: Vec3, motion: JointMotion[], parent?: string): Joint => (parent ? { group, pivot, parent, motion } : { group, pivot, motion });

export const roll = (radius: number, axis: Vec3 = [1, 0, 0]): JointMotion => ({ kind: 'roll', axis, radius });
export const steer = (amp: number, axis: Vec3 = [0, 1, 0]): JointMotion => ({ kind: 'steer', axis, amp });
export const spin = (rate: number, axis: Vec3): JointMotion => ({ kind: 'spin', axis, rate });
export const aimYaw = (rate: number): JointMotion => ({ kind: 'aim', yaw: true, rate });
export const aimPitch = (rate: number, minPitch = -15, maxPitch = 45): JointMotion => ({ kind: 'aim', pitch: true, rate, minPitch, maxPitch });
export const aimBoth = (rate: number, minPitch = -30, maxPitch = 45): JointMotion => ({ kind: 'aim', yaw: true, pitch: true, rate, minPitch, maxPitch });
/** Leg / arm swing that only happens while walking (phase 0 and 0.5 alternate). */
export const gait = (amp: number, rate: number, phase = 0, axis: Vec3 = [1, 0, 0]): JointMotion => ({ kind: 'swing', axis, amp, rate, phase, gait: true });
export const swing = (amp: number, rate: number, axis: Vec3, phase = 0): JointMotion => ({ kind: 'swing', axis, amp, rate, phase });
export const wave = (amp: number, rate: number, axis: Vec3 = [0, 1, 0], phase = 0): JointMotion => ({ kind: 'wave', axis, amp, rate, phase });
/** Recoil / bite / punch: one stroke each time a weapon in this group fires. */
export const recoil = (amp: number, rate = 3, axis: Vec3 = [0, 0, -1]): JointMotion => ({ kind: 'piston', axis, amp, rate, on: 'fire' });
export const snap = (amp: number, rate = 4, axis: Vec3 = [1, 0, 0]): JointMotion => ({ kind: 'swing', axis, amp, rate, on: 'fire' });
export const bob = (amp: number, rate: number, phase = 0, gaitOnly = true): JointMotion => ({ kind: 'piston', axis: [0, 1, 0], amp, rate, phase, gait: gaitOnly });

// =====================================================================================================
// Creature & ride helpers (beasts.ts / monsters.ts / rides.ts). Same rules as above: plain spec data.
// =====================================================================================================

/** Parts + joints being assembled. */
export interface Rig { parts: Part[]; joints: Joint[] }
export const newRig = (): Rig => ({ parts: [], joints: [] });

/**
 * A walking leg hanging from `hip` to a foot on the ground (y ≈ 0): thigh + shin (+ foot), one group that moves
 * with `motion` about the hip. `knee` shifts the knee along z (negative = hind-leg hock).
 */
export function walkLeg(r: Rig, group: string, hip: Vec3, thick: number, motion: JointMotion[], o: { knee?: number; foot?: number; parent?: string; role?: string } = {}) {
  const [x, y, z] = hip;
  const knee: Vec3 = [x, y * 0.5, z + (o.knee ?? 0.1 * y)];
  const ankle: Vec3 = [x, Math.max(0.06, thick * 0.3), z];
  r.parts.push(limb('capsule', hip, knee, thick, { role: o.role ?? 'leg', group, solid: false }));
  r.parts.push(limb('capsule', knee, ankle, thick * 0.8, { role: o.role ?? 'leg', group, solid: false }));
  const fl = o.foot ?? thick * 1.7;
  if (fl > 0) r.parts.push(P('box', [thick * 1.25, Math.max(0.06, thick * 0.35), fl], [x, Math.max(0.03, thick * 0.17), z + fl * 0.3], { role: 'foot', group, solid: false }));
  r.joints.push(J(group, hip, motion, o.parent));
}

/** Four walking legs (diagonal pairs in phase): hips at ±x, front z `zf`, back z `zb`. */
export function legs4(r: Rig, hipY: number, x: number, zf: number, zb: number, thick: number, amp: number, rate: number, o: { knee?: number; foot?: number; prefix?: string } = {}) {
  const p = o.prefix ?? 'leg';
  const L: [string, number, number, number][] = [[`${p}_fl`, -x, zf, 0], [`${p}_fr`, x, zf, 0.5], [`${p}_bl`, -x, zb, 0.5], [`${p}_br`, x, zb, 0]];
  for (const [g, lx, lz, ph] of L) walkLeg(r, g, [lx, hipY, lz], thick, [gait(amp, rate, ph)], { knee: lz < 0 ? -(o.knee ?? 0.12 * hipY) : o.knee, foot: o.foot });
}

/**
 * A segmented chain (tail, neck, tentacle, snake body) from `start`, one child group per segment so waves ripple
 * down it. `step` is the offset from one joint to the next (or a function of the segment index). The first
 * `solid` segments get colliders.
 */
export function chain(r: Rig, prefix: string, start: Vec3, step: Vec3 | ((i: number) => Vec3), n: number, t0: number, t1: number, motion: (i: number) => JointMotion[], o: { parent?: string; role?: string; shape?: Shape; tip?: Shape; solid?: number } = {}): { last: string; tip: Vec3; groups: string[] } {
  let a: Vec3 = [...start] as Vec3;
  let parent = o.parent;
  const groups: string[] = [];
  for (let i = 0; i < n; i++) {
    const s = typeof step === 'function' ? step(i) : step;
    const b: Vec3 = [a[0] + s[0], a[1] + s[1], a[2] + s[2]];
    const g = `${prefix}${i}`;
    const t = t0 + (t1 - t0) * (n > 1 ? i / (n - 1) : 0);
    const shape = i === n - 1 && o.tip ? o.tip : o.shape ?? 'capsule';
    r.parts.push(limb(shape, a, b, t, { role: o.role ?? 'tail', group: g, solid: i < (o.solid ?? 0) ? undefined : false }));
    r.joints.push(J(g, a, motion(i), parent));
    groups.push(g);
    parent = g;
    a = b;
  }
  return { last: groups[groups.length - 1], tip: a, groups };
}

/** A pair of flapping wings rooted at ±root.x: membrane + leading-edge spar (+ tip claw), mirrored. */
export function wingPair(r: Rig, prefix: string, root: Vec3, span: number, chord: number, amp: number, rate: number, o: { parent?: string; phase?: number; tip?: boolean; thick?: number } = {}) {
  const [rx, y, z] = root;
  const gl = `${prefix}_l`, gr = `${prefix}_r`;
  const sp = 0.14 * Math.max(1, chord / 2);
  const L: Part[] = [
    P('box', [span, o.thick ?? 0.08, chord], [-(rx + span / 2), y, z - chord * 0.1], { role: 'wing', group: gl, solid: false }),
    P('cylinder', [sp, span, sp], [-(rx + span / 2), y + 0.04, z + chord * 0.42], { role: 'spar', rot: [0, 0, 90], group: gl, solid: false }),
  ];
  if (o.tip !== false) L.push(P('cone', [0.16, span * 0.18, 0.16], [-(rx + span * 1.08), y + 0.04, z + chord * 0.42], { role: 'claw', rot: [0, 0, 90], group: gl, solid: false }));
  r.parts.push(...L, ...L.map((p) => mirror(p, () => gr)));
  r.joints.push(J(gl, [-rx, y, z], [swing(amp, rate, [0, 0, -1], o.phase ?? 0)], o.parent), J(gr, [rx, y, z], [swing(amp, rate, [0, 0, 1], o.phase ?? 0)], o.parent));
}

const _pq = new Quaternion(), _pq2 = new Quaternion(), _pe = new Euler(), _py = new Vector3(0, 1, 0);
/** Copies of `parts` turned by `yawDeg` about +y (about the origin), then moved by `offset` (swarm members, formations). */
export function placed(parts: Part[], yawDeg: number, offset: Vec3, group?: string): Part[] {
  const r2 = (x: number) => Math.round(x * 1000) / 1000;
  _pq.setFromAxisAngle(_py, (yawDeg * Math.PI) / 180);
  return parts.map((p) => {
    const v = new Vector3(...p.pos).applyQuaternion(_pq);
    const rr = p.rot ?? [0, 0, 0];
    _pq2.setFromEuler(_pe.set((rr[0] * Math.PI) / 180, (rr[1] * Math.PI) / 180, (rr[2] * Math.PI) / 180, 'XYZ')).premultiply(_pq);
    const e = new Euler().setFromQuaternion(_pq2, 'XYZ');
    const out: Part = { ...p, pos: [r2(v.x + offset[0]), r2(v.y + offset[1]), r2(v.z + offset[2])], size: [...p.size] as Vec3, rot: [r2((e.x * 180) / Math.PI), r2((e.y * 180) / Math.PI), r2((e.z * 180) / Math.PI)] };
    if (group) out.group = group;
    return out;
  });
}

/** A direction in the ground plane for a heading in degrees (0 = +z, 90 = +x); `side` gives the heading's left/right axis. */
export function heading(yawDeg: number, side = false): Vec3 {
  const a = ((yawDeg + (side ? 90 : 0)) * Math.PI) / 180;
  return [Math.round(Math.sin(a) * 1000) / 1000, 0, Math.round(Math.cos(a) * 1000) / 1000];
}
