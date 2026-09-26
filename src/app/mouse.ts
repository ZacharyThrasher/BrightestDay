/**
 * Mouse-look input hygiene. Two known Chromium/Windows pointer-lock problems make aim "spaz out":
 *  1. Occasional single mousemove events with a huge bogus movementX/Y (hundreds of px) — seen with
 *     high polling-rate mice, when the hidden cursor touches the window edge, and right after the lock
 *     is (re)acquired.
 *  2. OS pointer acceleration ("Enhance pointer precision") makes the same hand motion turn by
 *     different amounts depending on speed — inconsistent aim.
 * Fix: request raw input (`unadjustedMovement`, which also bypasses acceleration), ignore movement for a
 * few ms after the lock is acquired, and drop isolated spikes that are physically implausible compared
 * with the recent motion. Genuine fast flicks ramp up over several events, so they pass.
 */

export const MOUSE = {
  /** Source/Quake convention: degrees of yaw per mouse count at sensitivity 1 (m_yaw 0.022). */
  degPerCount: 0.022,
  /** ignore movement this long after pointer lock is (re)acquired (the first events carry the jump to centre) */
  settleMs: 60,
  /** a single event larger than this (in counts) is suspicious … */
  spikeAbs: 180,
  /** … and is dropped only if it is also this many times larger than the recent typical event size */
  spikeRatio: 8,
  /** window of recent events used to judge "typical" */
  history: 12,
};

/**
 * Sensitivity is in Source/Quake units (yaw = sens × 0.022° per mouse count), so a player's CS/Quake/
 * Apex-style number carries over 1:1. The default (5.7) keeps the feel the game originally shipped with.
 */
export const DEFAULT_SENS = 5.7;
export function radPerCount(sensitivity: number): number {
  return (sensitivity * MOUSE.degPerCount * Math.PI) / 180;
}

/** Old saved settings used a multiplier (1 = 0.0022 rad/count); convert them to Source units. */
export function legacyMultiplierToSource(mult: number): number {
  return (0.0022 * mult * 180) / Math.PI / MOUSE.degPerCount;
}

export class MouseFilter {
  private recent: number[] = [];
  private settleUntil = 0;
  dropped = 0;
  accepted = 0;

  /** Call when the pointer lock is acquired. */
  onLock(now: number) {
    this.settleUntil = now + MOUSE.settleMs;
    this.recent = [];
  }

  /** Returns the movement to apply (possibly [0,0] when the event is judged bogus). */
  filter(dx: number, dy: number, now: number): [number, number] {
    if (now < this.settleUntil) { this.dropped++; return [0, 0]; }
    const mag = Math.hypot(dx, dy);
    const typical = this.typical();
    if (mag > MOUSE.spikeAbs && mag > MOUSE.spikeRatio * Math.max(typical, 4)) {
      // don't record it in the history (so one spike can't teach the filter to accept the next)
      this.dropped++;
      return [0, 0];
    }
    this.recent.push(mag);
    if (this.recent.length > MOUSE.history) this.recent.shift();
    this.accepted++;
    return [dx, dy];
  }

  /** Upper-middle of recent magnitudes: robust to the odd outlier, tracks a flick as it ramps up. */
  private typical(): number {
    if (!this.recent.length) return 0;
    const s = [...this.recent].sort((a, b) => a - b);
    return s[Math.floor(s.length * 0.75)];
  }
}

/**
 * Request pointer lock with raw (unaccelerated) input when supported; fall back to a plain lock.
 * Returns a promise that settles when the request does (errors are swallowed; callers watch
 * pointerlockchange/pointerlockerror).
 */
export async function requestRawPointerLock(el: HTMLElement): Promise<'raw' | 'plain' | 'failed'> {
  try {
    const p = (el.requestPointerLock as any).call(el, { unadjustedMovement: true });
    if (p && typeof p.then === 'function') await p;
    return 'raw';
  } catch {
    try {
      const p = el.requestPointerLock() as unknown as Promise<void> | undefined;
      if (p && typeof (p as any).then === 'function') await p;
      return 'plain';
    } catch {
      return 'failed';
    }
  }
}
