import { Vector3, Quaternion, Euler } from 'three';
import type { Part, ConstructSpec, Vec3 } from './spec.js';

const D2R = Math.PI / 180;

/** Scaled local transform + size of one part (shared by physics and rendering so they always agree). */
export function partTransform(p: Part, scale: number, stretch: Vec3 = [1, 1, 1]) {
  const pos = new Vector3(p.pos[0] * stretch[0], p.pos[1] * stretch[1], p.pos[2] * stretch[2]).multiplyScalar(scale);
  const quat = p.rot ? new Quaternion().setFromEuler(new Euler(p.rot[0] * D2R, p.rot[1] * D2R, p.rot[2] * D2R, 'XYZ')) : new Quaternion();
  // stretch acts on the construct frame; approximate for rotated parts by stretching the part's own axes
  // that map closest to each construct axis.
  let size = new Vector3(p.size[0], p.size[1], p.size[2]);
  if (stretch[0] !== 1 || stretch[1] !== 1 || stretch[2] !== 1) {
    const axes = [new Vector3(1, 0, 0), new Vector3(0, 1, 0), new Vector3(0, 0, 1)].map((a) => a.applyQuaternion(quat));
    const s = [0, 1, 2].map((i) => {
      const a = axes[i];
      const w = [Math.abs(a.x), Math.abs(a.y), Math.abs(a.z)];
      const k = w.indexOf(Math.max(...w));
      return stretch[k];
    });
    size = new Vector3(size.x * s[0], size.y * s[1], size.z * s[2]);
  }
  size.multiplyScalar(scale);
  return { pos, quat, size };
}

/** Bounding info of the whole (scaled) construct instance in its local frame. */
export function specExtent(spec: ConstructSpec, solidOnly = false) {
  const min = new Vector3(Infinity, Infinity, Infinity);
  const max = new Vector3(-Infinity, -Infinity, -Infinity);
  const parts = solidOnly && spec.parts.some((p) => p.solid !== false) ? spec.parts.filter((p) => p.solid !== false) : spec.parts;
  for (const p of parts) {
    const t = partTransform(p, spec.scale, spec.stretch);
    const r = p.rot && (p.rot[0] || p.rot[1] || p.rot[2]) ? Math.max(t.size.x, t.size.y, t.size.z) / 2 : 0;
    const e = r ? new Vector3(r, r, r) : t.size.clone().multiplyScalar(0.5);
    min.min(t.pos.clone().sub(e));
    max.max(t.pos.clone().add(e));
  }
  if (!isFinite(min.x)) { min.set(-0.5, -0.5, -0.5); max.set(0.5, 0.5, 0.5); }
  const center = min.clone().add(max).multiplyScalar(0.5);
  const radius = max.clone().sub(min).length() / 2;
  return { min, max, center, radius, size: max.clone().sub(min) };
}
