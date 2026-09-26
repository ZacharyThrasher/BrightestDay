/**
 * Procedural WebAudio sound effects — no asset files. Every sound is a few short oscillator / filtered
 * noise layers with pitch & volume variation, routed through a voice pool (max concurrent voices,
 * priority stealing, per-kind throttling), optional 3D panning relative to the camera, an sfx bus, a
 * master gain and a compressor/limiter. Two persistent loops (boost roar, wind rush) follow the local
 * player's speed. Cheap: one shared noise buffer, ≤ ~12 nodes per voice, nodes freed on end.
 */
import { JUICE } from '../app/juice/tuning.js';
import { VoicePool, type Voice } from './voices.js';

type V3 = [number, number, number];

export interface PlayParams { energy?: number; pos?: V3 | null; kind?: string; novelty?: number; pitch?: number; gain?: number }

/** sound name → priority category (see JUICE.audio.priority) */
const CATEGORY: Record<string, string> = {
  impact: 'impact', hitConfirm: 'ui', explosion: 'explosion', form: 'form', reform: 'form', dissolve: 'movement', castFail: 'ui',
  confirm: 'ui', keyclick: 'keyclick', tetherFire: 'movement', tetherAttach: 'movement', tetherRelease: 'movement', tetherMiss: 'movement',
  jump: 'movement', land: 'movement', landHard: 'impact', flyOn: 'movement', flyOff: 'movement', whoosh: 'movement', launch: 'impact',
  actionStart: 'ui', weapon: 'weapon', block: 'impact', reflect: 'impact', hurt: 'hurt', death: 'sting', kill: 'kill', ko: 'kill', break: 'impact',
  shatter: 'impact', chime: 'ui', creative: 'sting', stale: 'ui', win: 'sting', lose: 'sting',
};

/** Seconds of the fresh-construct assembly (constructs.ts FORM_TIME); the form sound's snap lands on it. */
export const FORM_SNAP_AT = 0.35;

class Layered {
  end = 0;
  last: AudioScheduledSourceNode | null = null;
  constructor(private ctx: AudioContext, private out: AudioNode, private t0: number, private pitch: number, private vol: number, private noiseBuf: AudioBuffer | null) {}

  private env(g: GainNode, t: number, peak: number, attack: number, dur: number) {
    const p = Math.max(0.0002, peak * this.vol);
    g.gain.setValueAtTime(0.0001, t);
    g.gain.linearRampToValueAtTime(p, t + attack);
    g.gain.exponentialRampToValueAtTime(0.0001, t + Math.max(attack + 0.005, dur));
  }

  private track(src: AudioScheduledSourceNode, t: number, dur: number) {
    src.start(t);
    src.stop(t + dur + 0.03);
    if (t + dur + 0.03 > this.end) { this.end = t + dur + 0.03; this.last = src; }
  }

  /** Oscillator layer with an exponential pitch glide f0 → f1. */
  tone(type: OscillatorType, f0: number, f1: number, dur: number, peak: number, o: { at?: number; attack?: number; lp?: number } = {}) {
    const ctx = this.ctx, t = this.t0 + (o.at ?? 0);
    const osc = ctx.createOscillator();
    osc.type = type;
    osc.frequency.setValueAtTime(Math.max(20, f0 * this.pitch), t);
    if (f1 !== f0) osc.frequency.exponentialRampToValueAtTime(Math.max(20, f1 * this.pitch), t + dur);
    const g = ctx.createGain();
    this.env(g, t, peak, o.attack ?? 0.003, dur);
    if (o.lp) { const f = ctx.createBiquadFilter(); f.type = 'lowpass'; f.frequency.value = o.lp; osc.connect(f); f.connect(g); } else osc.connect(g);
    g.connect(this.out);
    this.track(osc, t, dur);
  }

  /** Filtered noise layer; the filter frequency glides f0 → f1. */
  noise(type: BiquadFilterType, f0: number, f1: number, dur: number, peak: number, o: { at?: number; attack?: number; q?: number } = {}) {
    if (!this.noiseBuf) return;
    const ctx = this.ctx, t = this.t0 + (o.at ?? 0);
    const src = ctx.createBufferSource();
    src.buffer = this.noiseBuf;
    const f = ctx.createBiquadFilter();
    f.type = type;
    f.Q.value = o.q ?? 0.9;
    f.frequency.setValueAtTime(Math.max(20, f0 * this.pitch), t);
    if (f1 !== f0) f.frequency.exponentialRampToValueAtTime(Math.max(20, f1 * this.pitch), t + dur);
    const g = ctx.createGain();
    this.env(g, t, peak, o.attack ?? 0.002, dur);
    src.connect(f); f.connect(g); g.connect(this.out);
    src.start(t, Math.random() * 0.8);
    src.stop(t + dur + 0.03);
    if (t + dur + 0.03 > this.end) { this.end = t + dur + 0.03; this.last = src; }
  }
}

type Recipe = (L: Layered, e: number, p: PlayParams) => void;

const lim = (e: number) => Math.max(0, Math.min(1, e));

/** The sound designs. `e` = energy 0..1.5 (bigger = heavier / louder / lower). */
const RECIPES: Record<string, Recipe> = {
  impact(L, e) {
    const k = lim(e);
    L.tone('sine', 150 - 50 * k, 42, 0.12 + 0.22 * k, 0.45 + 0.4 * k); // thud
    L.noise('bandpass', 2600, 900, 0.045 + 0.07 * k, 0.22 + 0.4 * k, { q: 1.3 }); // crack
    L.tone('square', 1900, 1400, 0.014, 0.05); // transient click
    if (e > 0.55) L.tone('sine', 72, 28, 0.35 + 0.2 * k, 0.45 * k, { attack: 0.006 }); // sub boom for big hits
  },
  hitConfirm(L, e) {
    L.tone('triangle', 1500 + 500 * lim(e), 950, 0.06, 0.16);
    L.noise('highpass', 5200, 5200, 0.02, 0.09);
    if (e > 0.6) L.tone('sine', 220, 110, 0.08, 0.18);
  },
  explosion(L, e) {
    const k = lim(e);
    L.noise('highpass', 3200, 2000, 0.05, 0.32); // crack
    L.noise('lowpass', 3000, 260, 0.28, 0.6); // body
    L.tone('sine', 92, 26, 0.6 + 0.45 * k, 0.75, { attack: 0.008 }); // boom
    L.noise('lowpass', 700, 110, 1.1 + k, 0.24, { attack: 0.03 }); // tail
  },
  form(L, e) {
    const s = FORM_SNAP_AT;
    L.tone('triangle', 440, 1320, s, 0.1, { attack: s * 0.6 }); // rising shimmer
    L.tone('triangle', 444, 1334, s, 0.08, { attack: s * 0.6 });
    L.noise('highpass', 5500, 7500, s, 0.05, { attack: s * 0.8 });
    L.tone('sine', 1760, 880, 0.09, 0.2, { at: s }); // snap
    L.noise('bandpass', 3600, 2400, 0.03, 0.22, { at: s, q: 2 });
    L.tone('sine', 240 - 60 * lim(e), 105, 0.12, 0.28, { at: s }); // weight
  },
  reform(L) {
    L.tone('triangle', 660, 1320, 0.14, 0.07, { attack: 0.1 });
    L.tone('sine', 1760, 1100, 0.07, 0.14, { at: 0.14 });
    L.noise('bandpass', 3400, 2600, 0.025, 0.14, { at: 0.14, q: 2 });
  },
  dissolve(L) {
    L.noise('bandpass', 2200, 380, 0.5, 0.11, { attack: 0.02, q: 1.4 });
    L.tone('sine', 700, 220, 0.45, 0.06, { attack: 0.01 });
  },
  castFail(L) {
    L.tone('sawtooth', 190, 88, 0.24, 0.1, { lp: 900 });
    L.tone('square', 118, 112, 0.16, 0.05, { at: 0.08, lp: 600 });
  },
  confirm(L) {
    L.tone('sine', 988, 988, 0.1, 0.08);
    L.tone('sine', 1480, 1480, 0.14, 0.07, { at: 0.05 });
  },
  keyclick(L, _e, p) {
    const hi = p.pitch ?? 1;
    L.noise('highpass', 4200, 4200, 0.012, 0.045);
    L.tone('sine', 1150 * hi, 1000 * hi, 0.022, 0.028);
  },
  tetherFire(L) {
    L.tone('sawtooth', 320, 2300, 0.09, 0.06, { lp: 4000 });
    L.noise('highpass', 3000, 6000, 0.08, 0.05);
  },
  tetherAttach(L) {
    L.tone('sine', 540, 250, 0.08, 0.2);
    L.noise('bandpass', 1900, 1500, 0.03, 0.18, { q: 1.6 });
    L.tone('triangle', 1580, 1560, 0.14, 0.045, { at: 0.01 });
  },
  tetherRelease(L) { L.tone('sawtooth', 1500, 320, 0.08, 0.04, { lp: 3000 }); },
  tetherMiss(L) { L.noise('bandpass', 1300, 600, 0.12, 0.05, { q: 1.2 }); },
  jump(L) {
    L.tone('sine', 260, 520, 0.08, 0.07);
    L.noise('highpass', 2200, 3500, 0.06, 0.035);
  },
  land(L, e) {
    const k = lim(e);
    L.tone('sine', 125, 52, 0.1, 0.12 + 0.25 * k);
    L.noise('lowpass', 900, 400, 0.08, 0.08 + 0.12 * k);
  },
  landHard(L, e) {
    const k = lim(e);
    L.tone('sine', 95, 30, 0.32, 0.45 + 0.2 * k);
    L.noise('lowpass', 1300, 180, 0.26, 0.32);
    L.noise('bandpass', 2600, 1600, 0.03, 0.18, { q: 1.4 });
  },
  flyOn(L) { L.noise('bandpass', 400, 2200, 0.35, 0.11, { attack: 0.08, q: 1.5 }); L.tone('sine', 220, 440, 0.3, 0.05, { attack: 0.1 }); },
  flyOff(L) { L.noise('bandpass', 2000, 400, 0.3, 0.09, { attack: 0.03, q: 1.5 }); },
  whoosh(L, e) { L.noise('bandpass', 500, 2600, 0.24, 0.08 + 0.14 * lim(e), { attack: 0.07, q: 1.2 }); },
  launch(L) {
    L.noise('bandpass', 400, 3000, 0.45, 0.22, { attack: 0.05, q: 1 });
    L.tone('sine', 80, 170, 0.4, 0.28, { attack: 0.02 });
    L.tone('sine', 110, 45, 0.18, 0.35);
  },
  actionStart(L) {
    L.tone('sine', 300, 600, 0.06, 0.07);
    L.noise('bandpass', 1800, 2600, 0.04, 0.04, { q: 2 });
  },
  weapon(L, e, p) {
    switch (p.kind) {
      case 'gun': L.noise('bandpass', 2600, 1800, 0.05, 0.28, { q: 1.2 }); L.tone('square', 230, 110, 0.035, 0.09); break;
      case 'missile': L.noise('bandpass', 1300, 600, 0.5, 0.22, { attack: 0.03 }); L.tone('sine', 210, 120, 0.2, 0.18); break;
      case 'bomb': L.tone('sine', 320, 150, 0.15, 0.12); L.noise('lowpass', 900, 500, 0.08, 0.08); break;
      case 'laser': L.tone('sine', 2500, 300, 0.14, 0.15); L.tone('square', 1250, 200, 0.1, 0.045, { lp: 5000 }); break;
      case 'flame': L.noise('lowpass', 1500, 900, 0.24, 0.22, { attack: 0.03 }); L.noise('bandpass', 420, 300, 0.26, 0.14, { attack: 0.04 }); break;
      case 'net': L.noise('bandpass', 900, 700, 0.1, 0.1); L.tone('triangle', 400, 700, 0.1, 0.07); break;
      case 'melee': L.noise('bandpass', 700, 2000, 0.12, 0.12, { attack: 0.04 }); L.tone('sine', 160, 70, 0.1, 0.2, { at: 0.06 }); break;
      default: // cannon
        L.tone('sine', 115, 38, 0.36, 0.55 * Math.max(0.5, lim(e)));
        L.noise('lowpass', 2600, 280, 0.22, 0.42);
        L.noise('highpass', 3200, 3000, 0.03, 0.22);
    }
  },
  block(L) {
    L.tone('triangle', 900, 880, 0.26, 0.16); // hard-light clang
    L.tone('sine', 1353, 1340, 0.3, 0.09);
    L.noise('bandpass', 3100, 2400, 0.03, 0.2, { q: 1.5 });
  },
  reflect(L) {
    RECIPES.block(L, 0.6, {});
    L.tone('sine', 1800, 3600, 0.12, 0.1, { at: 0.02 });
  },
  hurt(L, e) {
    const k = lim(e);
    L.tone('sine', 155, 58, 0.22, 0.5 + 0.3 * k);
    L.noise('lowpass', 1000, 300, 0.16, 0.34);
    L.tone('square', 92, 80, 0.08, 0.12 + 0.1 * k, { lp: 700 });
  },
  death(L) {
    L.tone('sine', 210, 38, 1.2, 0.42, { attack: 0.01 });
    L.noise('lowpass', 900, 90, 1.0, 0.26);
    L.tone('triangle', 330, 165, 0.9, 0.06, { at: 0.1 });
  },
  kill(L) {
    L.noise('highpass', 3200, 2400, 0.05, 0.3);
    L.tone('sine', 82, 32, 0.55, 0.55, { attack: 0.005 });
    [523.25, 659.25, 783.99, 1046.5].forEach((f, i) => L.tone('triangle', f, f, 0.55, 0.09, { at: 0.035 * i, attack: 0.005 }));
  },
  ko(L) {
    L.tone('sine', 120, 50, 0.25, 0.35);
    [659.25, 987.77].forEach((f, i) => L.tone('triangle', f, f, 0.32, 0.08, { at: 0.04 * i }));
  },
  break(L, e) {
    L.noise('bandpass', 1500, 700, 0.18, 0.22 + 0.12 * lim(e), { q: 0.8 });
    L.tone('sine', 180, 60, 0.15, 0.22);
    for (let i = 0; i < 3; i++) L.noise('bandpass', 2400 + i * 700, 2000, 0.02, 0.08, { at: 0.03 + i * 0.035 + Math.random() * 0.02, q: 3 });
  },
  shatter(L, e) {
    L.noise('highpass', 3600, 1500, 0.4, 0.22 * Math.max(0.6, lim(e)));
    L.tone('sine', 140, 48, 0.3, 0.3 * lim(e));
    for (const f of [2093, 2637, 3136, 3951]) L.tone('triangle', f, f * 0.97, 0.2, 0.04, { at: Math.random() * 0.12 });
  },
  chime(L, _e, p) {
    const n = Math.max(0, Math.min(1, p.novelty ?? 0.5));
    const f = 660 * Math.pow(2, n * 0.9); // brighter when more novel
    L.tone('sine', f, f, 0.9, 0.11, { attack: 0.004 });
    L.tone('sine', f * 2.76, f * 2.76, 0.45, 0.045);
    L.tone('sine', f * 5.4, f * 5.4, 0.22, 0.02 + 0.03 * n);
    L.tone('sine', f * 1.5, f * 1.5, 0.7, 0.08, { at: 0.07 });
  },
  creative(L) {
    [783.99, 987.77, 1174.66, 1567.98].forEach((f, i) => { L.tone('sine', f, f, 0.8, 0.09, { at: 0.06 * i }); L.tone('sine', f * 2.76, f * 2.76, 0.3, 0.03, { at: 0.06 * i }); });
    L.noise('highpass', 6000, 9000, 0.6, 0.05, { attack: 0.15 });
  },
  stale(L) {
    L.tone('triangle', 180, 150, 0.25, 0.12, { lp: 500 });
    L.tone('sine', 92, 88, 0.2, 0.1);
  },
  win(L) { [523.25, 659.25, 783.99, 1046.5, 1318.5].forEach((f, i) => L.tone('triangle', f, f, 0.6, 0.1, { at: 0.08 * i })); },
  lose(L) { [440, 392, 349.23, 261.63].forEach((f, i) => L.tone('triangle', f, f * 0.98, 0.7, 0.1, { at: 0.12 * i, lp: 1800 })); },
};

export const SOUND_NAMES = Object.keys(RECIPES);

export interface SfxStats { active: number; max: number; played: number; stolen: number; dropped: number; throttled: number; peak: number; state: string; loops: number }

/**
 * The sound engine. Construct with a factory so tests can inject a mock AudioContext; a null/throwing
 * factory (no audio device) leaves it as a silent no-op that still counts what it would have played.
 */
export class Sfx {
  ctx: AudioContext | null = null;
  readonly pool: VoicePool;
  private master: GainNode | null = null;
  private bus: GainNode | null = null;
  private noiseBuf: AudioBuffer | null = null;
  private loops: {
    boost: { g: GainNode; f: BiquadFilterNode; rg: GainNode } | null;
    wind: { g: GainNode; f: BiquadFilterNode } | null;
    charge: { g: GainNode; o1: OscillatorNode; o2: OscillatorNode; f: BiquadFilterNode } | null;
  } = { boost: null, wind: null, charge: null };
  private volumes = { master: JUICE.defaults.master, sfx: JUICE.defaults.sfx };
  private muted = false;
  /** plays attempted while no context existed (still counted for tests / debug) */
  silentPlays = 0;
  readonly counts: Record<string, number> = {};

  constructor(factory: () => AudioContext | null = defaultFactory, readonly maxVoices = JUICE.audio.maxVoices) {
    this.pool = new VoicePool(maxVoices);
    try { this.ctx = factory(); } catch { this.ctx = null; }
    if (!this.ctx) return;
    const ctx = this.ctx;
    const C = JUICE.audio.compressor;
    const comp = ctx.createDynamicsCompressor();
    comp.threshold.value = C.threshold; comp.knee.value = C.knee; comp.ratio.value = C.ratio; comp.attack.value = C.attack; comp.release.value = C.release;
    this.master = ctx.createGain();
    this.bus = ctx.createGain();
    this.bus.connect(this.master);
    this.master.connect(comp);
    comp.connect(ctx.destination);
    this.applyVolumes();
    // one shared second of white noise
    const n = Math.floor(ctx.sampleRate || 44100);
    this.noiseBuf = ctx.createBuffer(1, n, ctx.sampleRate || 44100);
    const d = this.noiseBuf.getChannelData(0);
    for (let i = 0; i < n; i++) d[i] = Math.random() * 2 - 1;
    const l = ctx.listener as any;
    if (l?.positionX) { l.positionX.value = 0; }
  }

  get now() { return this.ctx?.currentTime ?? performance.now() / 1000; }

  setVolumes(master: number, sfx: number) { this.volumes = { master: clamp01(master), sfx: clamp01(sfx) }; this.applyVolumes(); }
  setMuted(m: boolean) {
    this.muted = m;
    this.applyVolumes();
    if (this.ctx) { try { if (m) void this.ctx.suspend?.(); else void this.ctx.resume?.(); } catch { /* ignore */ } }
  }
  get isMuted() { return this.muted || this.volumes.master <= 0 || this.volumes.sfx <= 0; }
  /** Resume after a user gesture (autoplay policies start contexts suspended). */
  resume() { if (this.ctx && this.ctx.state === 'suspended' && !this.muted) { try { void this.ctx.resume(); } catch { /* ignore */ } } }

  private applyVolumes() {
    if (!this.ctx || !this.master || !this.bus) return;
    const t = this.ctx.currentTime;
    this.master.gain.setTargetAtTime(this.muted ? 0 : this.volumes.master, t, 0.02);
    this.bus.gain.setTargetAtTime(this.volumes.sfx, t, 0.02);
  }

  /** Camera pose for 3D panning. */
  private lastListener = new Float32Array(9);
  setListener(pos: V3, fwd: V3, up: V3) {
    const l = this.ctx?.listener as any;
    if (!l) return;
    const L = this.lastListener;
    const d = Math.abs(L[0] - pos[0]) + Math.abs(L[1] - pos[1]) + Math.abs(L[2] - pos[2]) + 4 * (Math.abs(L[3] - fwd[0]) + Math.abs(L[4] - fwd[1]) + Math.abs(L[5] - fwd[2]) + Math.abs(L[6] - up[0]) + Math.abs(L[7] - up[1]) + Math.abs(L[8] - up[2]));
    if (d < 0.02) return;
    L.set([...pos, ...fwd, ...up]);
    if (l.positionX) {
      l.positionX.value = pos[0]; l.positionY.value = pos[1]; l.positionZ.value = pos[2];
      l.forwardX.value = fwd[0]; l.forwardY.value = fwd[1]; l.forwardZ.value = fwd[2];
      l.upX.value = up[0]; l.upY.value = up[1]; l.upZ.value = up[2];
    } else if (l.setPosition) { l.setPosition(pos[0], pos[1], pos[2]); l.setOrientation(fwd[0], fwd[1], fwd[2], up[0], up[1], up[2]); }
  }

  /** Play a named sound. Returns false if muted, throttled, dropped by the pool or unknown. */
  play(name: string, p: PlayParams = {}): boolean {
    const recipe = RECIPES[name];
    if (!recipe) return false;
    const A = JUICE.audio;
    const key = name === 'weapon' ? (p.kind ?? 'cannon') : name;
    const now = this.now;
    if (!this.pool.throttle(key, now, A.throttleMs[key] ?? A.throttleMs[name] ?? A.throttleMs.default)) return false;
    if (this.muted) return false;
    const prio = A.priority[CATEGORY[name] ?? 'default'] ?? A.priority.default;
    const e = Math.max(0, Math.min(1.5, p.energy ?? 0.5));
    this.counts[name] = (this.counts[name] ?? 0) + 1;
    if (!this.ctx || !this.bus) {
      // no audio device: account for the voice anyway (tests / debug overlay)
      const v = this.pool.acquire(name, prio, now, 0.3);
      this.silentPlays++;
      return !!v;
    }
    const ctx = this.ctx;
    const t0 = ctx.currentTime + 0.005;
    const out = ctx.createGain();
    out.gain.value = p.gain ?? 1;
    let tail: AudioNode = out;
    let panner: PannerNode | null = null;
    if (p.pos) {
      panner = ctx.createPanner();
      panner.panningModel = 'equalpower';
      panner.distanceModel = 'inverse';
      panner.refDistance = A.refDistance;
      panner.rolloffFactor = A.rolloff;
      panner.maxDistance = A.maxDistance;
      const pp = panner as any;
      if (pp.positionX) { pp.positionX.value = p.pos[0]; pp.positionY.value = p.pos[1]; pp.positionZ.value = p.pos[2]; } else pp.setPosition?.(p.pos[0], p.pos[1], p.pos[2]);
      out.connect(panner);
      tail = panner;
    }
    const pitch = (p.pitch && name !== 'keyclick' ? p.pitch : 1) * (1 + (Math.random() * 2 - 1) * A.pitchVar);
    const vol = 1 + (Math.random() * 2 - 1) * A.volVar;
    const L = new Layered(ctx, out, t0, pitch, vol, this.noiseBuf);
    recipe(L, e, p);
    const dur = Math.max(0.02, L.end - t0);
    const v = this.pool.acquire(name, prio, now, dur);
    if (!v) { disconnectAll(out, panner); return false; }
    tail.connect(this.bus);
    v.handle = { stop: () => { try { out.gain.cancelScheduledValues(ctx.currentTime); out.gain.setTargetAtTime(0, ctx.currentTime, 0.01); } catch { /* ignore */ } setTimeout(() => disconnectAll(out, panner), 80); } };
    const release = (vv: Voice) => { this.pool.release(vv); disconnectAll(out, panner); };
    if (L.last) L.last.onended = () => release(v);
    return true;
  }

  /** Per frame: boost roar and wind rush follow the local player's speed. */
  private lastMove = { speed: -1, boosting: false };
  updateMovement(speed: number, boosting: boolean) {
    const ctx = this.ctx;
    if (!ctx || !this.bus || !this.noiseBuf) return;
    // AudioParam automation is not free: only touch it when something audible changed
    if (Math.abs(speed - this.lastMove.speed) < 0.4 && boosting === this.lastMove.boosting) return;
    this.lastMove = { speed, boosting };
    const A = JUICE.audio;
    const t = ctx.currentTime;
    const windK = Math.max(0, Math.min(1, (speed - A.wind.startSpeed) / (A.wind.fullSpeed - A.wind.startSpeed)));
    if (!this.loops.wind && windK > 0) {
      const src = ctx.createBufferSource(); src.buffer = this.noiseBuf; src.loop = true;
      const f = ctx.createBiquadFilter(); f.type = 'bandpass'; f.Q.value = 0.6;
      const g = ctx.createGain(); g.gain.value = 0;
      src.connect(f); f.connect(g); g.connect(this.bus); src.start();
      this.loops.wind = { g, f };
    }
    if (this.loops.wind) {
      this.loops.wind.g.gain.setTargetAtTime(windK * windK * A.wind.gain, t, 0.12);
      this.loops.wind.f.frequency.setTargetAtTime(350 + speed * 28, t, 0.15);
    }
    if (!this.loops.boost && boosting) {
      const src = ctx.createBufferSource(); src.buffer = this.noiseBuf; src.loop = true;
      const f = ctx.createBiquadFilter(); f.type = 'lowpass'; f.Q.value = 1.2;
      const g = ctx.createGain(); g.gain.value = 0;
      const osc = ctx.createOscillator(); osc.type = 'sawtooth'; osc.frequency.value = 52;
      const rf = ctx.createBiquadFilter(); rf.type = 'lowpass'; rf.frequency.value = 180;
      const rg = ctx.createGain(); rg.gain.value = 0;
      src.connect(f); f.connect(g); g.connect(this.bus); src.start();
      osc.connect(rf); rf.connect(rg); rg.connect(this.bus); osc.start();
      this.loops.boost = { g, f, rg };
    }
    if (this.loops.boost) {
      const on = boosting ? 1 : 0;
      this.loops.boost.g.gain.setTargetAtTime(on * A.boost.gain, t, boosting ? 0.04 : 0.12);
      this.loops.boost.rg.gain.setTargetAtTime(on * A.boost.gain * 0.5, t, boosting ? 0.05 : 0.15);
      this.loops.boost.f.frequency.setTargetAtTime(A.boost.cutoffBase + speed * A.boost.cutoffPerSpeed, t, 0.1);
    }
  }

  /** Wind-up hum for a charging construct action: pitch and brightness rise with the charge level (null = off). */
  updateCharge(level: number | null) {
    const ctx = this.ctx;
    if (!ctx || !this.bus) return;
    const t = ctx.currentTime;
    if (!this.loops.charge && level !== null) {
      const o1 = ctx.createOscillator(); o1.type = 'sawtooth';
      const o2 = ctx.createOscillator(); o2.type = 'sine';
      const f = ctx.createBiquadFilter(); f.type = 'lowpass'; f.Q.value = 4;
      const g = ctx.createGain(); g.gain.value = 0;
      o1.connect(f); o2.connect(f); f.connect(g); g.connect(this.bus); o1.start(); o2.start();
      this.loops.charge = { g, o1, o2, f };
    }
    const c = this.loops.charge;
    if (!c) return;
    const lv = level === null ? 0 : Math.max(0, Math.min(1, level));
    c.g.gain.setTargetAtTime(level === null ? 0 : 0.05 + 0.13 * lv, t, level === null ? 0.05 : 0.03);
    c.o1.frequency.setTargetAtTime(80 + 240 * lv * lv, t, 0.04);
    c.o2.frequency.setTargetAtTime(160 + 480 * lv * lv + (lv > 0.95 ? 8 * Math.sin(t * 40) : 0), t, 0.04);
    c.f.frequency.setTargetAtTime(300 + 2600 * lv, t, 0.04);
  }

  stats(): SfxStats {
    this.pool.prune(this.now);
    const p = this.pool;
    return { active: p.active, max: p.max, played: p.played, stolen: p.stolen, dropped: p.dropped, throttled: p.throttled, peak: p.peak, state: this.ctx?.state ?? 'none', loops: (this.loops.wind ? 1 : 0) + (this.loops.boost ? 1 : 0) + (this.loops.charge ? 1 : 0) };
  }
}

function clamp01(x: number) { return Math.max(0, Math.min(1, Number.isFinite(x) ? x : 0)); }
function disconnectAll(...nodes: (AudioNode | null)[]) { for (const n of nodes) { try { n?.disconnect(); } catch { /* ignore */ } } }
function defaultFactory(): AudioContext | null {
  const AC = (globalThis as any).AudioContext ?? (globalThis as any).webkitAudioContext;
  return AC ? new AC({ latencyHint: 'interactive' }) : null;
}
