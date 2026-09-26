/**
 * Quantization + delta helpers for snapshots (pure). JSON-friendly: quantized values are integers,
 * which serialise shorter than floats and compare exactly (so deltas don't flicker on float noise).
 */
import type { Quat, Vec3 } from './interp.js';

/** Default steps: 1 mm positions, 1 cm/s velocities. */
export const POS_STEP = 0.001;
export const VEL_STEP = 0.01;

export const quantize = (v: number, step: number) => Math.round(v / step);
export const dequantize = (q: number, step: number) => q * step;

export const quantizeVec3 = (v: Readonly<Vec3>, step = POS_STEP): Vec3 => [quantize(v[0], step), quantize(v[1], step), quantize(v[2], step)];
export const dequantizeVec3 = (q: Readonly<Vec3>, step = POS_STEP): Vec3 => [q[0] * step, q[1] * step, q[2] * step];

/**
 * "Smallest three" quaternion encoding: drop the largest-magnitude component (recovered from unit length),
 * store its index and the other three scaled to ±`scale` integers. Default 2^14 → ~0.0001 rad resolution.
 * Output: [index, a, b, c] (integers).
 */
export function quantizeQuat(q: Readonly<Quat>, scale = 16383): [number, number, number, number] {
  const l = Math.hypot(q[0], q[1], q[2], q[3]) || 1;
  let n = [q[0] / l, q[1] / l, q[2] / l, q[3] / l];
  let big = 0;
  for (let i = 1; i < 4; i++) if (Math.abs(n[i]) > Math.abs(n[big])) big = i;
  if (n[big] < 0) n = n.map((x) => -x); // q and -q are the same rotation; make the dropped one positive
  const rest = n.filter((_, i) => i !== big);
  // the three smallest are within ±1/√2
  const k = scale * Math.SQRT2;
  return [big, Math.round(rest[0] * k), Math.round(rest[1] * k), Math.round(rest[2] * k)];
}

export function dequantizeQuat(e: Readonly<[number, number, number, number]>, scale = 16383): Quat {
  const k = scale * Math.SQRT2;
  const big = Math.max(0, Math.min(3, e[0] | 0));
  const r = [e[1] / k, e[2] / k, e[3] / k];
  const w = Math.sqrt(Math.max(0, 1 - r[0] * r[0] - r[1] * r[1] - r[2] * r[2]));
  const out: number[] = [];
  let j = 0;
  for (let i = 0; i < 4; i++) out.push(i === big ? w : r[j++]);
  const l = Math.hypot(out[0], out[1], out[2], out[3]) || 1;
  return [out[0] / l, out[1] / l, out[2] / l, out[3] / l];
}

/** Angle between two unit quaternions (radians). */
export function quatAngle(a: Readonly<Quat>, b: Readonly<Quat>): number {
  const d = Math.abs(a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]);
  return 2 * Math.acos(Math.min(1, d));
}

export const vecEqual = (a: readonly number[], b: readonly number[]) => a.length === b.length && a.every((x, i) => x === b[i]);

/**
 * Shallow delta of two flat records of quantized fields: returns only changed keys (arrays compared
 * element-wise), or null if nothing changed. Removed keys are listed in `_del`.
 */
export type Flat = Record<string, number | string | boolean | null | readonly number[]>;
export function diffFlat(prev: Flat | undefined, next: Flat): Partial<Flat> & { _del?: string[] } | null {
  const out: Record<string, unknown> = {};
  let changed = false;
  for (const k in next) {
    const a = prev?.[k], b = next[k];
    const same = Array.isArray(a) && Array.isArray(b) ? vecEqual(a, b) : a === b;
    if (!same) { out[k] = b; changed = true; }
  }
  if (prev) {
    const del = Object.keys(prev).filter((k) => !(k in next));
    if (del.length) { out._del = del; changed = true; }
  }
  return changed ? (out as any) : null;
}

/** Apply a delta produced by diffFlat. */
export function applyFlat(prev: Flat | undefined, delta: (Partial<Flat> & { _del?: string[] }) | null): Flat {
  const out: Flat = { ...(prev ?? {}) };
  if (!delta) return out;
  for (const k in delta) if (k !== '_del') out[k] = (delta as any)[k];
  for (const k of delta._del ?? []) delete out[k];
  return out;
}

/** Keyed collection delta (entities by id): added/changed records as deltas, removed ids. */
export function diffMap(prev: ReadonlyMap<string | number, Flat>, next: ReadonlyMap<string | number, Flat>) {
  const upd: [string | number, ReturnType<typeof diffFlat>][] = [];
  const del: (string | number)[] = [];
  for (const [id, rec] of next) { const d = diffFlat(prev.get(id), rec); if (d) upd.push([id, d]); }
  for (const id of prev.keys()) if (!next.has(id)) del.push(id);
  return { upd, del };
}
