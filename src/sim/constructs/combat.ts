/**
 * Combat tuning + targeting maths shared by every construct that attacks (classic strike/home/grab/shoot
 * behaviours in runtime.ts and machine weapons/brains in machines.ts).
 *
 * Design (docs/PVP_DESIGN.md "Speed vs. hitting"): movement is defence, constructs are offence.
 *  - everything leads its target (predictive intercept); homing things re-plan every tick
 *  - fast strikes COMMIT: they lock their intercept point at launch, so a juke dodges them
 *  - chasers (hands, creatures, cars, missiles) track continuously, a bit faster than a player can sustain,
 *    but with a limited turn rate, so hard cuts, walls and broken line of sight shake them
 *  - area constructs (cages, domes, fields, explosions) are unaffected — they deny space instead
 */
import { Vector3 } from 'three';

export const COMBAT = {
  /** committed strikes (fist/hammer lunges): speed band m/s; direction locked at launch */
  strike: { minSpeed: 45, maxSpeed: 60, lockAtLaunch: true },
  /** continuous chasers attacking an enemy: top speed m/s, turn rate deg/s */
  chaser: { maxSpeed: 32, turnRate: 170 },
  /** homing missiles / rockets */
  missile: { maxSpeed: 32, turnRate: 150 },
  /** grabbing hands going for a moving enemy (rescues, and targets slower than evadeSpeed m/s, are not limited) */
  grab: { maxSpeed: 32, turnRate: 200, evadeSpeed: 8 },
  /** how far ahead leads are allowed to look (s) */
  lead: { maxTime: 2.5 },
};

/**
 * Time until a shot from `from` travelling at `speed` meets a target at `p` moving with constant velocity `v`.
 * Falls back to plain travel time when there is no solution (target faster than the shot, moving away).
 */
export function interceptTime(from: Vector3, speed: number, p: Vector3, v: Vector3, maxT = COMBAT.lead.maxTime): number {
  const dx = p.x - from.x, dy = p.y - from.y, dz = p.z - from.z;
  const s = Math.max(1e-3, speed);
  const a = v.x * v.x + v.y * v.y + v.z * v.z - s * s;
  const b = 2 * (dx * v.x + dy * v.y + dz * v.z);
  const c = dx * dx + dy * dy + dz * dz;
  let t: number;
  if (Math.abs(a) < 1e-6) t = b < 0 ? -c / b : Math.sqrt(c) / s;
  else {
    const disc = b * b - 4 * a * c;
    if (disc < 0) t = Math.sqrt(c) / s;
    else {
      const r = Math.sqrt(disc);
      const t1 = (-b - r) / (2 * a), t2 = (-b + r) / (2 * a);
      t = Math.min(t1 > 0 ? t1 : Infinity, t2 > 0 ? t2 : Infinity);
      if (!isFinite(t)) t = Math.sqrt(c) / s;
    }
  }
  return Math.min(Math.max(0, t), maxT);
}

/** Where to aim: the target's position after the intercept time. */
export function interceptPoint(from: Vector3, speed: number, p: Vector3, v: Vector3, out = new Vector3(), maxT = COMBAT.lead.maxTime): Vector3 {
  const t = interceptTime(from, speed, p, v, maxT);
  return out.copy(p).addScaledVector(v, t);
}

const _a = new Vector3(), _b = new Vector3(), _ax = new Vector3();

/**
 * Turn-limited steering: rotate `vel` toward `desiredDir` by at most `maxAngle` radians and give it length
 * `speed`. `vel` is modified in place (a zero vel snaps straight to the desired direction).
 */
export function steerToward(vel: Vector3, desiredDir: Vector3, maxAngle: number, speed: number): Vector3 {
  const len = vel.length();
  const want = _b.copy(desiredDir);
  if (want.lengthSq() < 1e-12) return vel.setLength(speed);
  want.normalize();
  if (len < 1e-6) return vel.copy(want).multiplyScalar(speed);
  const cur = _a.copy(vel).divideScalar(len);
  const cos = Math.max(-1, Math.min(1, cur.dot(want)));
  const ang = Math.acos(cos);
  if (ang <= maxAngle) return vel.copy(want).multiplyScalar(speed);
  _ax.crossVectors(cur, want);
  if (_ax.lengthSq() < 1e-12) { _ax.set(0, 1, 0).cross(cur); if (_ax.lengthSq() < 1e-9) _ax.set(1, 0, 0); }
  _ax.normalize();
  return vel.copy(cur).applyAxisAngle(_ax, maxAngle).multiplyScalar(speed);
}
