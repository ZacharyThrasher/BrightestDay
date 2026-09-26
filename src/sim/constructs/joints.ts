/**
 * Articulation: evaluates each joint group's pose (in the construct's scaled frame) from the construct's motion
 * state. Pure maths, no physics; deterministic given its inputs. Used by the machine runtime, which syncs colliders
 * and exposes the poses to the renderer.
 */
import { Quaternion, Vector3 } from 'three';
import type { ConstructSpec, Joint, JointMotion } from './spec.js';

const D2R = Math.PI / 180;
const TAU = Math.PI * 2;

export interface Pose { pos: Vector3; quat: Quaternion }

/** A joint prepared for fast evaluation. */
export interface JointRT {
  j: Joint;
  index: number;
  parent: number; // index into the joint list, -1 = body
  depth: number; // chain depth (for wave phase lag)
  pivot: Vector3; // scaled construct-frame pivot
  axes: Vector3[]; // normalised axis per motion (unused for aim)
  aim: { yaw: boolean; pitch: boolean; rate: number; min: number; max: number } | null;
  hasFire: boolean;
  aimLeaf: boolean; // an aim joint with no aiming descendant
}

export interface JointInputs {
  t: number; // seconds since the construct formed
  travel: number; // metres travelled along the construct's forward axis (signed)
  gait: number; // walking time in seconds (advances in proportion to speed / top speed)
  gaitAmp: number; // 0..1 how much walking is happening
  steer: number; // -1..1 current turning (positive = turning toward +x)
  fireAge: (jointIndex: number) => number; // seconds since a weapon fired through this joint (Infinity = never)
  /** Aim hook: called for aim joints with the pivot in world space and the parent's world rotation; returns the new yaw/pitch (radians). */
  aim?: (rt: JointRT, pivotWorld: Vector3, parentWorldQ: Quaternion, cur: { yaw: number; pitch: number }) => void;
}

export interface JointState {
  aimYaw: Float64Array;
  aimPitch: Float64Array;
  poses: Map<string, Pose>;
  /** world-frame rotation of each joint group (valid after evaluate with a world transform) */
  worldQ: Quaternion[];
  worldP: Vector3[];
}

export function prepareJoints(spec: ConstructSpec): JointRT[] {
  const joints = spec.joints ?? [];
  if (!joints.length) return [];
  const st = spec.stretch ?? [1, 1, 1];
  const k = spec.scale;
  const byGroup = new Map(joints.map((j, i) => [j.group, i]));
  // topological order: parents first
  const order: number[] = [];
  const seen = new Set<number>();
  const visit = (i: number, guard: number) => {
    if (seen.has(i) || guard > joints.length) return;
    const p = joints[i].parent ? byGroup.get(joints[i].parent!) : undefined;
    if (p !== undefined && p !== i) visit(p, guard + 1);
    if (!seen.has(i)) { seen.add(i); order.push(i); }
  };
  for (let i = 0; i < joints.length; i++) visit(i, 0);
  const rts: JointRT[] = [];
  const newIndex = new Map<number, number>();
  for (const i of order) {
    const j = joints[i];
    const pOld = j.parent ? byGroup.get(j.parent) : undefined;
    const parent = pOld !== undefined && newIndex.has(pOld) ? newIndex.get(pOld)! : -1;
    const aimM = j.motion.find((m) => m.kind === 'aim') as Extract<JointMotion, { kind: 'aim' }> | undefined;
    const rt: JointRT = {
      j, index: rts.length, parent, depth: parent >= 0 ? rts[parent].depth + 1 : 0,
      pivot: new Vector3(j.pivot[0] * st[0] * k, j.pivot[1] * st[1] * k, j.pivot[2] * st[2] * k),
      axes: j.motion.map((m) => ('axis' in m && m.axis ? new Vector3(m.axis[0], m.axis[1], m.axis[2]) : new Vector3(0, 1, 0)).normalize()),
      aim: aimM ? { yaw: !!aimM.yaw, pitch: !!aimM.pitch, rate: aimM.rate * D2R, min: (aimM.minPitch ?? -30) * D2R, max: (aimM.maxPitch ?? 60) * D2R } : null,
      hasFire: j.motion.some((m) => (m.kind === 'swing' || m.kind === 'piston') && m.on === 'fire'),
      aimLeaf: false,
    };
    newIndex.set(i, rts.length);
    rts.push(rt);
  }
  for (const rt of rts) if (rt.aim) rt.aimLeaf = !rts.some((o) => o.aim && isAncestor(rts, rt.index, o.index));
  return rts;
}

export function isAncestor(rts: JointRT[], anc: number, j: number): boolean {
  let p = rts[j].parent;
  let guard = 0;
  while (p >= 0 && guard++ < 64) { if (p === anc) return true; p = rts[p].parent; }
  return false;
}

export function newJointState(rts: JointRT[]): JointState {
  const poses = new Map<string, Pose>();
  for (const rt of rts) poses.set(rt.j.group, { pos: new Vector3(), quat: new Quaternion() });
  return {
    aimYaw: new Float64Array(rts.length), aimPitch: new Float64Array(rts.length), poses,
    worldQ: rts.map(() => new Quaternion()), worldP: rts.map(() => new Vector3()),
  };
}

const _q = new Quaternion(), _qm = new Quaternion(), _t = new Vector3(), _v = new Vector3(), _pw = new Vector3();
const _X = new Vector3(1, 0, 0), _Y = new Vector3(0, 1, 0);

/** 0..1..0 stroke: fast out, slower return (recoil, bites). */
function fireStroke(age: number, rate: number): number {
  const k = age * Math.max(0.1, rate);
  if (!(k >= 0) || k >= 1) return 0;
  return Math.sin(Math.PI * Math.sqrt(k));
}

/**
 * Evaluate every joint. Poses are construct-frame (scaled) transforms: x' = quat·x + pos.
 * If instPos/instQuat are given, also fills world-frame rotations/pivots (needed for aim joints).
 */
export function evaluateJoints(rts: JointRT[], st: JointState, inp: JointInputs, scale: number, instPos?: Vector3, instQuat?: Quaternion) {
  for (const rt of rts) {
    const j = rt.j;
    // local motion (about the pivot)
    const q = _q.identity();
    const tr = _t.set(0, 0, 0);
    for (let mi = 0; mi < j.motion.length; mi++) {
      const m = j.motion[mi];
      const axis = rt.axes[mi];
      switch (m.kind) {
        case 'spin': q.multiply(_qm.setFromAxisAngle(axis, TAU * m.rate * inp.t)); break;
        case 'roll': q.multiply(_qm.setFromAxisAngle(axis, inp.travel / Math.max(0.02, m.radius * scale))); break;
        case 'steer': q.multiply(_qm.setFromAxisAngle(axis, Math.max(-1, Math.min(1, inp.steer)) * m.amp * D2R)); break;
        case 'aim': {
          if (inp.aim && instPos && instQuat) {
            // parent world frame
            const pq = rt.parent >= 0 ? st.worldQ[rt.parent] : instQuat;
            const pp = rt.parent >= 0 ? poseOf(st, rts[rt.parent]) : null;
            _pw.copy(rt.pivot);
            if (pp) _pw.applyQuaternion(pp.quat).add(pp.pos);
            _pw.applyQuaternion(instQuat).add(instPos);
            const cur = { yaw: st.aimYaw[rt.index], pitch: st.aimPitch[rt.index] };
            inp.aim(rt, _pw, pq, cur);
            st.aimYaw[rt.index] = cur.yaw;
            st.aimPitch[rt.index] = cur.pitch;
          }
          if (rt.aim!.yaw) q.multiply(_qm.setFromAxisAngle(_Y, st.aimYaw[rt.index]));
          if (rt.aim!.pitch) q.multiply(_qm.setFromAxisAngle(_X, -st.aimPitch[rt.index]));
          break;
        }
        case 'swing': case 'piston': case 'wave': {
          let s: number;
          if ((m.kind === 'swing' || m.kind === 'piston') && m.on === 'fire') s = fireStroke(inp.fireAge(rt.index), m.rate);
          else if ((m.kind === 'swing' || m.kind === 'piston') && m.gait) s = Math.sin(TAU * (m.rate * inp.gait + (m.phase ?? 0))) * inp.gaitAmp;
          else s = Math.sin(TAU * (m.rate * inp.t + (m.phase ?? 0) - (m.kind === 'wave' ? rt.depth * 0.18 : 0)));
          if (m.kind === 'piston') tr.addScaledVector(_v.copy(axis).applyQuaternion(q), s * m.amp * scale);
          else q.multiply(_qm.setFromAxisAngle(axis, s * m.amp * D2R));
          break;
        }
      }
    }
    // local pose about pivot: x' = q(x - p) + p + tr
    const pose = st.poses.get(j.group)!;
    pose.quat.copy(q);
    pose.pos.copy(rt.pivot).applyQuaternion(q).negate().add(rt.pivot).add(tr);
    if (rt.parent >= 0) {
      const pp = poseOf(st, rts[rt.parent]);
      // compose parent ∘ local
      pose.pos.applyQuaternion(pp.quat).add(pp.pos);
      pose.quat.premultiply(pp.quat);
    }
    if (instQuat) st.worldQ[rt.index].copy(instQuat).multiply(pose.quat);
    if (instPos && instQuat) st.worldP[rt.index].copy(rt.pivot).applyQuaternion(pose.quat).add(pose.pos).applyQuaternion(instQuat).add(instPos);
  }
}

function poseOf(st: JointState, rt: JointRT): Pose { return st.poses.get(rt.j.group)!; }

