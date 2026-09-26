// Audio helpers for the bench: seeded RNG, windowed-sinc resampler, pink/white
// noise, synthetic reverb, SNR mixing and energy-based speech bounds.

export function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function gauss(rng) {
  let u = 0, v = 0;
  while (u === 0) u = rng();
  while (v === 0) v = rng();
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
}

// Band-limited resampling with a Hann-windowed sinc (half-width 16 input
// samples at the lower of the two rates). Quality is plenty for ASR input.
export function resample(x, fromRate, toRate) {
  if (fromRate === toRate) return Float32Array.from(x);
  const ratio = toRate / fromRate;
  const n = Math.floor(x.length * ratio);
  const y = new Float32Array(n);
  const cutoff = Math.min(1, ratio) * 0.95; // normalized to input Nyquist
  const half = 16 / Math.min(1, ratio);
  for (let i = 0; i < n; i++) {
    const t = i / ratio;
    const c = Math.floor(t);
    let acc = 0, wsum = 0;
    const lo = Math.max(0, Math.ceil(t - half));
    const hi = Math.min(x.length - 1, Math.floor(t + half));
    for (let k = lo; k <= hi; k++) {
      const d = t - k;
      const s = d === 0 ? 1 : Math.sin(Math.PI * cutoff * d) / (Math.PI * cutoff * d);
      const w = 0.5 + 0.5 * Math.cos((Math.PI * d) / half);
      const h = s * w;
      acc += x[k] * h;
      wsum += h;
    }
    void c;
    y[i] = wsum !== 0 ? acc / wsum : 0;
  }
  return y;
}

export function pinkNoise(n, rng) {
  // Paul Kellet's refined pink filter on white gaussian noise
  let b0 = 0, b1 = 0, b2 = 0, b3 = 0, b4 = 0, b5 = 0, b6 = 0;
  const y = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    const w = gauss(rng);
    b0 = 0.99886 * b0 + w * 0.0555179;
    b1 = 0.99332 * b1 + w * 0.0750759;
    b2 = 0.969 * b2 + w * 0.153852;
    b3 = 0.8665 * b3 + w * 0.3104856;
    b4 = 0.55 * b4 + w * 0.5329522;
    b5 = -0.7616 * b5 - w * 0.016898;
    y[i] = b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362;
    b6 = w * 0.115926;
  }
  return y;
}

export function whiteNoise(n, rng) {
  const y = new Float32Array(n);
  for (let i = 0; i < n; i++) y[i] = gauss(rng);
  return y;
}

// Very light synthetic room: exponentially decaying noise tail (RT60 seconds),
// mixed so that the direct path dominates (direct-to-reverberant ratio ~ drrDb).
export function reverb(x, sr, rng, rt60 = 0.3, drrDb = 8) {
  const len = Math.floor(rt60 * sr);
  const ir = new Float32Array(len);
  const decay = Math.log(1000) / (rt60 * sr); // -60 dB at rt60
  let e = 0;
  for (let i = 0; i < len; i++) { ir[i] = gauss(rng) * Math.exp(-decay * i); e += ir[i] * ir[i]; }
  const g = Math.pow(10, -drrDb / 20) / Math.sqrt(e);
  for (let i = 0; i < len; i++) ir[i] *= g;
  // direct-form convolution on a decimated IR would be faster; clips are short
  const y = new Float32Array(x.length);
  const predelay = Math.floor(0.005 * sr);
  for (let i = 0; i < x.length; i++) {
    const xi = x[i];
    if (xi === 0) continue;
    y[i] += xi;
    const lim = Math.min(len, x.length - i - predelay);
    for (let k = 0; k < lim; k += 1) y[i + predelay + k] += xi * ir[k];
  }
  return y;
}

export function power(x, s = 0, e = x.length) {
  let p = 0;
  for (let i = s; i < e; i++) p += x[i] * x[i];
  return p / Math.max(1, e - s);
}

// Returns a new array: speech + noise scaled so that speech power (measured
// over [s,e)) / noise power = snrDb.
export function mixAtSnr(speech, noise, snrDb, s, e) {
  const ps = power(speech, s, e);
  const pn = power(noise);
  const g = Math.sqrt(ps / (pn * Math.pow(10, snrDb / 10)));
  const y = new Float32Array(speech.length);
  for (let i = 0; i < y.length; i++) y[i] = speech[i] + g * noise[i];
  return y;
}

// Energy-based speech bounds on clean audio (10 ms frames, threshold 40 dB
// below the loudest frame). Returns sample indices [start, end).
export function speechBounds(x, sr, relDb = -40) {
  const f = Math.floor(sr / 100);
  const n = Math.floor(x.length / f);
  const rms = new Float64Array(n);
  let peak = 0;
  for (let i = 0; i < n; i++) {
    rms[i] = Math.sqrt(power(x, i * f, (i + 1) * f));
    if (rms[i] > peak) peak = rms[i];
  }
  const thr = peak * Math.pow(10, relDb / 20);
  let a = 0, b = n - 1;
  while (a < n && rms[a] < thr) a++;
  while (b > a && rms[b] < thr) b--;
  return [a * f, (b + 1) * f];
}

export function normalizePeak(x, peak = 0.7) {
  let m = 0;
  for (const v of x) m = Math.max(m, Math.abs(v));
  if (m === 0) return x;
  const g = peak / m;
  const y = new Float32Array(x.length);
  for (let i = 0; i < x.length; i++) y[i] = x[i] * g;
  return y;
}

export function concat(...parts) {
  const n = parts.reduce((a, p) => a + p.length, 0);
  const y = new Float32Array(n);
  let o = 0;
  for (const p of parts) { y.set(p, o); o += p.length; }
  return y;
}
