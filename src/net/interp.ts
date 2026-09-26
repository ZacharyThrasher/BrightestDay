/**
 * Snapshot interpolation buffer (pure).
 *
 * Push time-stamped states as they arrive (time = host time of the snapshot); sample at
 * renderTime = hostNow − delay (≈100 ms). Between two samples the state is blended with `lerp`;
 * past the newest sample it is extrapolated (lerp with t > 1) for at most `maxExtrapolateMs`, then held.
 * Before the oldest sample the oldest state is returned.
 */
export type Lerp<T> = (a: T, b: T, t: number) => T;

export interface Sampled<T> { state: T; /** 'interp' | 'extrap' | 'hold' (past extrapolation cap) | 'before' (older than buffer) */ mode: 'interp' | 'extrap' | 'hold' | 'before'; }

export class SnapshotBuffer<T> {
  private times: number[] = [];
  private states: T[] = [];

  constructor(readonly lerp: Lerp<T>, readonly capacity = 64, readonly maxExtrapolateMs = 100) {}

  get size() { return this.times.length; }
  get newestTime() { return this.times.length ? this.times[this.times.length - 1] : -Infinity; }
  get oldestTime() { return this.times.length ? this.times[0] : Infinity; }

  /** Insert a sample (out-of-order arrivals are placed correctly; duplicates replace). */
  push(time: number, state: T) {
    if (!Number.isFinite(time)) return;
    let i = this.times.length;
    while (i > 0 && this.times[i - 1] > time) i--;
    if (i > 0 && this.times[i - 1] === time) { this.states[i - 1] = state; return; }
    this.times.splice(i, 0, time);
    this.states.splice(i, 0, state);
    if (this.times.length > this.capacity) { this.times.shift(); this.states.shift(); }
  }

  /** Drop samples that can no longer be needed (keeps one before `time`). */
  prune(time: number) {
    let n = 0;
    while (n + 1 < this.times.length && this.times[n + 1] <= time) n++;
    if (n > 0) { this.times.splice(0, n); this.states.splice(0, n); }
  }

  clear() { this.times = []; this.states = []; }

  sample(renderTime: number): Sampled<T> | null {
    const n = this.times.length;
    if (!n) return null;
    if (renderTime <= this.times[0]) return { state: this.states[0], mode: n === 1 || renderTime < this.times[0] ? 'before' : 'interp' };
    if (renderTime >= this.times[n - 1]) {
      if (n === 1) return { state: this.states[0], mode: 'hold' };
      const ta = this.times[n - 2], tb = this.times[n - 1];
      const over = renderTime - tb;
      const capped = Math.min(over, this.maxExtrapolateMs);
      if (capped <= 0) return { state: this.states[n - 1], mode: 'interp' };
      const t = 1 + capped / (tb - ta);
      return { state: this.lerp(this.states[n - 2], this.states[n - 1], t), mode: over > this.maxExtrapolateMs ? 'hold' : 'extrap' };
    }
    // binary search: last index with time <= renderTime
    let lo = 0, hi = n - 1;
    while (hi - lo > 1) { const mid = (lo + hi) >> 1; if (this.times[mid] <= renderTime) lo = mid; else hi = mid; }
    const ta = this.times[lo], tb = this.times[hi];
    return { state: this.lerp(this.states[lo], this.states[hi], (renderTime - ta) / (tb - ta)), mode: 'interp' };
  }
}

// ------------------------------------------------------------------------------------ lerp helpers

export type Vec3 = [number, number, number];
export type Quat = [number, number, number, number]; // x, y, z, w

export const lerpNum = (a: number, b: number, t: number) => a + (b - a) * t;
export const lerpVec3: Lerp<Vec3> = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];

/** Angle lerp along the shortest arc (radians). */
export const lerpAngle = (a: number, b: number, t: number) => {
  let d = (b - a) % (Math.PI * 2);
  if (d > Math.PI) d -= Math.PI * 2;
  if (d < -Math.PI) d += Math.PI * 2;
  return a + d * t;
};

/** Normalised lerp for small steps, slerp otherwise; shortest path; t may exceed 1 (extrapolation). */
export const slerpQuat: Lerp<Quat> = (a, b, t) => {
  let [bx, by, bz, bw] = b;
  let cos = a[0] * bx + a[1] * by + a[2] * bz + a[3] * bw;
  if (cos < 0) { cos = -cos; bx = -bx; by = -by; bz = -bz; bw = -bw; }
  let ka: number, kb: number;
  if (cos > 0.9995) { ka = 1 - t; kb = t; }
  else {
    const th = Math.acos(Math.min(1, cos)), s = Math.sin(th);
    ka = Math.sin((1 - t) * th) / s; kb = Math.sin(t * th) / s;
  }
  const q: Quat = [a[0] * ka + bx * kb, a[1] * ka + by * kb, a[2] * ka + bz * kb, a[3] * ka + bw * kb];
  const l = Math.hypot(q[0], q[1], q[2], q[3]) || 1;
  return [q[0] / l, q[1] / l, q[2] / l, q[3] / l];
};

/** Common case: a rigid pose. */
export interface Pose { p: Vec3; q: Quat }
export const lerpPose: Lerp<Pose> = (a, b, t) => ({ p: lerpVec3(a.p, b.p, t), q: slerpQuat(a.q, b.q, t) });
