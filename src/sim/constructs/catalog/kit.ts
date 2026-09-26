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
