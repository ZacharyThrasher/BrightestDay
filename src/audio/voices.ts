/**
 * Voice pool for procedural sound effects (pure, unit-tested). Caps concurrent voices, steals the
 * lowest-priority (then oldest) voice when full, and throttles repeats of one sound kind so a machine
 * gun or a laser does not stack 60 identical voices.
 */
export interface VoiceHandle { stop(): void }

export interface Voice {
  id: number;
  name: string;
  priority: number;
  start: number; // seconds (audio clock)
  end: number;
  handle: VoiceHandle | null;
}

export class VoicePool {
  voices: Voice[] = [];
  private nextId = 1;
  private lastByKey = new Map<string, number>();
  /** stats */
  played = 0;
  stolen = 0;
  dropped = 0;
  throttled = 0;
  peak = 0;

  constructor(public max: number) {}

  /** Free voices whose sound has finished by `now`. */
  prune(now: number) {
    if (this.voices.some((v) => v.end <= now)) this.voices = this.voices.filter((v) => v.end > now);
  }

  /** False (and counted) if `key` played less than `gapMs` ago. */
  throttle(key: string, now: number, gapMs: number): boolean {
    const last = this.lastByKey.get(key);
    if (last !== undefined && (now - last) * 1000 < gapMs) { this.throttled++; return false; }
    this.lastByKey.set(key, now);
    return true;
  }

  /**
   * Reserve a voice for `dur` seconds. When the pool is full the weakest voice (lowest priority, then
   * oldest) is stolen if it is not more important than the newcomer; otherwise the newcomer is dropped.
   */
  acquire(name: string, priority: number, now: number, dur: number): Voice | null {
    this.prune(now);
    if (this.voices.length >= this.max) {
      let victim: Voice | null = null;
      for (const v of this.voices) if (!victim || v.priority < victim.priority || (v.priority === victim.priority && v.start < victim.start)) victim = v;
      if (!victim || victim.priority > priority) { this.dropped++; return null; }
      this.release(victim);
      try { victim.handle?.stop(); } catch { /* already stopped */ }
      this.stolen++;
    }
    const v: Voice = { id: this.nextId++, name, priority, start: now, end: now + Math.max(0.01, dur), handle: null };
    this.voices.push(v);
    this.played++;
    this.peak = Math.max(this.peak, this.voices.length);
    return v;
  }

  release(v: Voice) { const i = this.voices.indexOf(v); if (i >= 0) this.voices.splice(i, 1); }

  get active() { return this.voices.length; }
}
