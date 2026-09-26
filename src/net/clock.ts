/**
 * Host-clock estimation from ping/pong samples (NTP-style, pure).
 *
 * A sample is (t0 = local send time, hostNow = host clock when it answered, t1 = local receive time).
 * rtt = t1 − t0; offset = hostNow − (t0 + t1) / 2, i.e. hostTime ≈ localTime + offset.
 * Samples with the lowest RTT carry the least asymmetry error, so the estimate is the median offset of
 * the fastest third of the recent window. Offset changes are slewed to avoid visible time jumps,
 * except for the first few samples and large corrections (> snapMs), which snap.
 */
export interface ClockSample { rtt: number; offset: number }

export class ClockSync {
  private samples: ClockSample[] = [];
  private _offset = 0;
  private _rtt = 0;
  private _rttAvg = 0;
  private count = 0;

  constructor(readonly window = 16, readonly snapMs = 250, readonly slewPerSample = 0.25) {}

  /** Add a ping/pong sample. Returns false for nonsensical samples (negative RTT, non-finite). */
  add(t0: number, hostNow: number, t1: number): boolean {
    const rtt = t1 - t0;
    if (!Number.isFinite(rtt) || !Number.isFinite(hostNow) || rtt < 0 || rtt > 60000) return false;
    const offset = hostNow - (t0 + t1) / 2;
    this.samples.push({ rtt, offset });
    if (this.samples.length > this.window) this.samples.shift();
    this._rtt = rtt;
    this._rttAvg = this.count === 0 ? rtt : this._rttAvg * 0.8 + rtt * 0.2;
    this.count++;
    const target = ClockSync.estimate(this.samples);
    if (this.count <= 3 || Math.abs(target - this._offset) > this.snapMs) this._offset = target;
    else this._offset += (target - this._offset) * this.slewPerSample;
    return true;
  }

  /** Median offset of the lowest-RTT third of the samples. */
  static estimate(samples: readonly ClockSample[]): number {
    if (!samples.length) return 0;
    const byRtt = [...samples].sort((a, b) => a.rtt - b.rtt);
    const best = byRtt.slice(0, Math.max(1, Math.ceil(byRtt.length / 3))).map((s) => s.offset).sort((a, b) => a - b);
    const m = best.length >> 1;
    return best.length % 2 ? best[m] : (best[m - 1] + best[m]) / 2;
  }

  get offset() { return this._offset; }
  /** Latest RTT (ms). */
  get rtt() { return this._rtt; }
  /** Smoothed RTT (ms, EMA). */
  get rttAvg() { return this._rttAvg; }
  get sampleCount() { return this.count; }
  /** Host clock now, from a local clock reading. */
  hostNow(localNow: number) { return localNow + this._offset; }
  reset() { this.samples = []; this._offset = 0; this._rtt = 0; this._rttAvg = 0; this.count = 0; }
}
