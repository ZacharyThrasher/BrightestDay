"""Small DSP toolkit for Brightest Day's procedural SFX.

Everything is pure synthesis (oscillators, noise, filters, modal resonators,
synthetic reverb).  All functions work on mono float64 numpy arrays at SR.
"""
from __future__ import annotations

import numpy as np
from scipy import signal
from scipy.ndimage import maximum_filter1d, minimum_filter1d, uniform_filter1d

SR = 48000
TAU = 2.0 * np.pi


# --------------------------------------------------------------------------- basics
def ns(d: float) -> int:
    """Seconds -> samples (at least 1)."""
    return max(1, int(round(d * SR)))


def tvec(n: int) -> np.ndarray:
    return np.arange(n) / SR


def buf(d: float) -> np.ndarray:
    """Scratch buffer for a one-shot of nominal length d. Generously padded so ringing layers are
    never truncated; the mastering stage trims trailing silence anyway."""
    return np.zeros(ns(d * 1.5 + 0.5))


def add(out: np.ndarray, x: np.ndarray, t: float = 0.0, gain: float = 1.0) -> np.ndarray:
    """Mix x into out starting at time t (seconds); silently truncates."""
    i = int(round(t * SR))
    if i >= len(out) or i + len(x) <= 0:
        return out
    j0 = max(0, -i)
    i = max(0, i)
    m = min(len(out) - i, len(x) - j0)
    seg = gain * x[j0:j0 + m]
    if m > 8:
        # layers never end on a step: cosine tail fade, long if the layer is still ringing
        pk = np.max(np.abs(seg)) + 1e-12
        tail = np.max(np.abs(seg[-max(2, min(ns(0.004), m // 10)):])) / pk
        f = min(ns(0.06 if tail > 0.01 else 0.004), m // 3)
        if tail > 0.03 and m > ns(0.05):
            CUT_WARNINGS.append(f'layer ends at {20 * np.log10(tail):.0f} dB (len {m / SR:.2f}s)')
        seg = seg.copy()
        seg[-f:] *= 0.5 + 0.5 * np.cos(np.linspace(0, np.pi, f))
    out[i:i + m] += seg
    return out


CUT_WARNINGS: list[str] = []


def add_circ(out: np.ndarray, x: np.ndarray, i: int, gain: float = 1.0) -> np.ndarray:
    """Mix x into a loop buffer starting at sample i, wrapping around the end."""
    n = len(out)
    idx = (i + np.arange(len(x))) % n
    np.add.at(out, idx, gain * x)
    return out


def fit(x: np.ndarray, n: int) -> np.ndarray:
    if len(x) >= n:
        return x[:n]
    return np.concatenate([x, np.zeros(n - len(x))])


def mix(*xs: np.ndarray) -> np.ndarray:
    n = max(len(x) for x in xs)
    out = np.zeros(n)
    for x in xs:
        out[:len(x)] += x
    return out


# --------------------------------------------------------------------------- envelopes
def env_exp(n: int, tau: float, attack: float = 0.0005) -> np.ndarray:
    """Exponential decay with a short raised-cosine attack (no clicks)."""
    t = tvec(n)
    e = np.exp(-t / max(tau, 1e-5))
    if attack > 0:
        a = np.clip(t / attack, 0, 1)
        e *= 0.5 - 0.5 * np.cos(np.pi * a)
    return e


def env_pts(n: int, pts, curve: float = 1.0) -> np.ndarray:
    """Piecewise-linear envelope through (time, value) points; curve>1 = exponential-ish."""
    ts = np.array([p[0] for p in pts], float)
    vs = np.array([p[1] for p in pts], float)
    e = np.interp(tvec(n), ts, vs)
    if curve != 1.0:
        e = np.sign(e) * np.abs(e) ** curve
    return e


def glide(n: int, f0: float, f1: float, tau: float) -> np.ndarray:
    """Exponential pitch glide from f0 towards f1 with time constant tau."""
    t = tvec(n)
    return f1 + (f0 - f1) * np.exp(-t / max(tau, 1e-5))


def sweep(n: int, f0: float, f1: float, shape: float = 1.0) -> np.ndarray:
    """Geometric sweep f0->f1 over the whole length (shape<1 = fast start)."""
    u = np.linspace(0, 1, n) ** shape
    return f0 * (f1 / f0) ** u


# --------------------------------------------------------------------------- oscillators
def _phase(f, n, ph0=0.0):
    f = np.broadcast_to(np.asarray(f, float), (n,))
    cyc = np.cumsum(f) / SR
    cyc = cyc - f / SR  # start exactly at ph0
    return cyc + ph0 / TAU, f


def sine(f, n: int, ph0: float = 0.0) -> np.ndarray:
    cyc, _ = _phase(f, n, ph0)
    return np.sin(TAU * cyc)


def saw(f, n: int, ph0: float = 0.0) -> np.ndarray:
    """PolyBLEP band-limited sawtooth."""
    cyc, f = _phase(f, n, ph0)
    p = cyc % 1.0
    dt = np.clip(np.abs(f) / SR, 1e-9, 0.5)
    y = 2.0 * p - 1.0
    m = p < dt
    tt = p[m] / dt[m]
    y[m] -= tt + tt - tt * tt - 1.0
    m = p > 1.0 - dt
    tt = (p[m] - 1.0) / dt[m]
    y[m] -= tt * tt + tt + tt + 1.0
    return y


def square(f, n: int, ph0: float = 0.0, duty: float = 0.5) -> np.ndarray:
    return 0.5 * (saw(f, n, ph0) - saw(f, n, ph0 + TAU * duty))


def supersaw(f, n: int, rng, voices: int = 5, detune_cents: float = 15.0) -> np.ndarray:
    f = np.broadcast_to(np.asarray(f, float), (n,))
    out = np.zeros(n)
    offs = np.linspace(-1, 1, voices) if voices > 1 else np.zeros(1)
    for o in offs:
        ratio = 2 ** (o * detune_cents / 1200.0)
        out += saw(f * ratio, n, rng.uniform(0, TAU))
    return out / np.sqrt(voices)


def fm(fc, ratio: float, index, n: int, ph0: float = 0.0) -> np.ndarray:
    """2-op FM (phase modulation). fc/index may be arrays."""
    fc = np.broadcast_to(np.asarray(fc, float), (n,))
    cyc_c, _ = _phase(fc, n, ph0)
    cyc_m, _ = _phase(fc * ratio, n)
    return np.sin(TAU * cyc_c + np.asarray(index) * np.sin(TAU * cyc_m))


def fm_bell(n: int, fc: float, ratio: float, idx0: float, tau_idx: float, tau_amp: float,
            attack: float = 0.0005) -> np.ndarray:
    t = tvec(n)
    index = idx0 * np.exp(-t / tau_idx)
    return fm(fc, ratio, index, n) * env_exp(n, tau_amp, attack)


# --------------------------------------------------------------------------- noise
def white(n: int, rng) -> np.ndarray:
    return rng.uniform(-1.0, 1.0, n)


def _colored(n: int, rng, power: float) -> np.ndarray:
    m = max(n, 16)
    X = np.fft.rfft(rng.standard_normal(m))
    f = np.fft.rfftfreq(m, 1 / SR)
    f[0] = f[1]
    X /= f ** (power / 2)
    x = np.fft.irfft(X, m)[:n]
    return x / (np.sqrt(np.mean(x ** 2)) + 1e-12) * 0.35


def pink(n: int, rng) -> np.ndarray:
    return _colored(n, rng, 1.0)


def brown(n: int, rng) -> np.ndarray:
    return _colored(n, rng, 2.0)


def smooth_noise(n: int, rng, rate_hz: float) -> np.ndarray:
    """Slow random control signal, roughly in [-1, 1]."""
    x = rng.standard_normal(n + ns(1.0 / rate_hz) * 4)
    sos = signal.butter(2, min(rate_hz, SR * 0.45), 'low', fs=SR, output='sos')
    x = signal.sosfiltfilt(sos, x)[:n]
    return x / (np.max(np.abs(x)) + 1e-12)


# --------------------------------------------------------------------------- filters
def _sos(kind, fc, order):
    if isinstance(fc, (tuple, list)):
        fc = [min(max(c, 5.0), SR * 0.49) for c in fc]
    else:
        fc = min(max(fc, 5.0), SR * 0.49)
    return signal.butter(order, fc, kind, fs=SR, output='sos')


def lp(x, fc, order=2):
    return signal.sosfilt(_sos('low', fc, order), x)


def hp(x, fc, order=2):
    return signal.sosfilt(_sos('high', fc, order), x)


def bp(x, lo, hi, order=2):
    return signal.sosfilt(_sos('band', (lo, hi), order), x)


def reson(x, fc, q):
    """Constant 0 dB-peak resonant bandpass."""
    fc = min(max(fc, 10.0), SR * 0.48)
    b, a = signal.iirpeak(fc, q, fs=SR)
    return signal.lfilter(b, a, x)


def _rbj(kind, fc, q):
    w0 = TAU * np.clip(fc, 10.0, SR * 0.47) / SR
    cw, sw = np.cos(w0), np.sin(w0)
    alpha = sw / (2 * q)
    if kind == 'lp':
        b = [(1 - cw) / 2, 1 - cw, (1 - cw) / 2]
    elif kind == 'hp':
        b = [(1 + cw) / 2, -(1 + cw), (1 + cw) / 2]
    else:  # 'bp' constant 0 dB peak
        b = [alpha, 0.0, -alpha]
    a = [1 + alpha, -2 * cw, 1 - alpha]
    return np.array(b) / a[0], np.array(a) / a[0]


def tvf(x, fc, q: float = 0.707, kind: str = 'lp', block: int = 32) -> np.ndarray:
    """Time-varying RBJ biquad (coefficients updated every `block` samples)."""
    n = len(x)
    fc = np.broadcast_to(np.asarray(fc, float), (n,))
    y = np.empty(n)
    zi = np.zeros(2)
    for s in range(0, n, block):
        e = min(n, s + block)
        b, a = _rbj(kind, fc[(s + e) // 2], q)
        y[s:e], zi = signal.lfilter(b, a, x[s:e], zi=zi)
    return y


# --------------------------------------------------------------------------- resonators / layers
def modal(freqs, decays, amps, n: int, rng=None, attack: float = 0.0003) -> np.ndarray:
    """Sum of exponentially decaying sinusoids (struck-object model)."""
    t = tvec(n)
    out = np.zeros(n)
    for f, d, a in zip(freqs, decays, amps):
        if f >= SR * 0.47:
            continue
        ph = rng.uniform(0, TAU) if rng is not None else 0.0
        out += a * np.exp(-t / d) * np.sin(TAU * f * t + ph)
    if attack > 0:
        out *= np.clip(t / attack, 0, 1)
    return out


def sat(x, drive: float = 2.0) -> np.ndarray:
    return np.tanh(drive * x) / np.tanh(drive)


def normpk(x, peak: float = 1.0) -> np.ndarray:
    m = np.max(np.abs(x))
    return x * (peak / m) if m > 0 else x


def thump(d, f0, f1, tau_p, tau_a, drive=1.5, attack=0.0008) -> np.ndarray:
    """Pitch-dropping sine kick: the low-end 'weight' of hits and landings."""
    n = ns(max(d, 7 * tau_a))  # never truncate the body
    x = sine(glide(n, f0, f1, tau_p), n) * env_exp(n, tau_a, attack)
    return sat(x, drive) if drive > 0 else x


def burst(d, rng, lo, hi, tau, attack=0.0004, color='white') -> np.ndarray:
    """Band-limited noise burst with exponential decay."""
    n = ns(max(d, 7 * tau))
    src = {'white': white, 'pink': pink, 'brown': brown}[color](n, rng)
    if lo is None:
        y = lp(src, hi)
    elif hi is None:
        y = hp(src, lo)
    else:
        y = bp(src, lo, hi)
    y = normpk(y)
    return y * env_exp(n, tau, attack)


def crack(rng, fc=2500.0, tau=0.004, d=None) -> np.ndarray:
    """Very short bright transient."""
    d = d or max(0.02, tau * 8)
    return burst(d, rng, fc, None, tau, attack=0.0002)


def grain(rng, d, lo, hi) -> np.ndarray:
    n = ns(d)
    g = bp(white(n + 64, rng), lo, hi)[64:]
    return normpk(g) * np.hanning(n)


def tink(rng, f, d_scale=1.0, bright=1.0) -> np.ndarray:
    """A tiny glassy/hard-light cube impact: bar-mode partials + a click."""
    ratios = [1.0, 2.76 * rng.uniform(0.98, 1.02), 5.40 * rng.uniform(0.97, 1.03)]
    decs = np.array([0.055, 0.032, 0.016]) * d_scale * rng.uniform(0.7, 1.3)
    amps = [1.0, 0.5 * bright, 0.25 * bright]
    n = ns(decs.max() * 6)
    x = modal([f * r for r in ratios], decs, amps, n, rng)
    x += 0.4 * crack(rng, 5000, 0.0008, d=n / SR)[:n] if n > 10 else 0
    return x


def tok(rng, f, d_scale=1.0) -> np.ndarray:
    """A small stone/wood cube hitting ground: dull short knock."""
    ratios = [1.0, 1.58 * rng.uniform(0.95, 1.05), 2.37 * rng.uniform(0.95, 1.05)]
    decs = np.array([0.022, 0.014, 0.008]) * d_scale * rng.uniform(0.7, 1.3)
    n = ns(0.12 * d_scale)
    x = modal([f * r for r in ratios], decs, [1.0, 0.6, 0.35], n, rng)
    x += 0.7 * burst(n / SR, rng, f * 0.8, f * 3.5, 0.003)
    x += 0.4 * thump(n / SR, f * 0.35, f * 0.25, 0.01, 0.012, drive=0)
    return x


def bounces(rng, t0, amp0, interval0, ratio, amp_ratio, t_max, min_interval=0.012):
    """Times/amps for a single cube bouncing to rest."""
    out = []
    t, a, iv = t0, amp0, interval0
    while t < t_max and iv > min_interval and a > 0.01:
        out.append((t, a))
        t += iv
        iv *= ratio * rng.uniform(0.9, 1.1)
        a *= amp_ratio * rng.uniform(0.85, 1.1)
    return out


def whoosh(d, rng, f0, f1, q=1.0, pts=None, color='pink') -> np.ndarray:
    n = ns(d)
    src = pink(n, rng) if color == 'pink' else white(n, rng)
    y = tvf(src, sweep(n, f0, f1), q, 'bp')
    pts = pts or [(0, 0), (d * 0.3, 1), (d, 0)]
    return normpk(y) * env_pts(n, pts)


# --------------------------------------------------------------------------- reverb
def make_ir(rt60: float, rng, predelay: float = 0.01, bright: float = 1.0) -> np.ndarray:
    n = ns(rt60 * 1.15)
    t = tvec(n)
    w = rng.standard_normal(n)
    ir = np.zeros(n)
    bands = [(30, 250, 1.25), (250, 1500, 1.0), (1500, 5000, 0.7), (5000, 20000, 0.45)]
    for lo, hi, m in bands:
        rt = rt60 * (m ** (1.0 / max(bright, 0.1)))
        ir += bp(w, lo, hi) * np.exp(-6.91 * t / rt)
    ir *= np.clip(t / 0.006, 0, 1)
    er = np.zeros(n)
    for _ in range(8):
        pos = rng.uniform(0.002, 0.035)
        er[ns(pos)] += rng.choice([-1, 1]) * rng.uniform(0.5, 1.5) * np.exp(-pos / 0.03)
    ir += lp(er, 6000) * np.std(ir[:ns(0.05)]) * 6
    ir = np.concatenate([np.zeros(ns(predelay)), ir])
    return ir / np.sqrt(np.sum(ir ** 2))


def reverb(x, rng, rt60=0.8, wet=0.2, predelay=0.01, bright=1.0) -> np.ndarray:
    ir = make_ir(rt60, rng, predelay, bright)
    w = signal.fftconvolve(x, ir)
    return fit(x, len(w)) + wet * w


# --------------------------------------------------------------------------- loops
def loop_xfade(x: np.ndarray, L: int, X: int) -> np.ndarray:
    """Make x (len >= L+X) loop seamlessly with period L via equal-power crossfade.

    Suitable for uncorrelated (noise) material.  y[0] continues x[L-1] exactly.
    """
    assert len(x) >= L + X
    y = x[:L].copy()
    th = np.linspace(0, np.pi / 2, X)
    y[:X] = x[:X] * np.sin(th) + x[L:L + X] * np.cos(th)
    return y


def periodic(render, L: int, warm: int | None = None) -> np.ndarray:
    """Render a strictly L-periodic source through time-invariant/periodic processing:
    render(n) must return a signal whose *input* is L-periodic; we discard the warm-up."""
    warm = L if warm is None else warm
    y = render(L + warm)
    return y[warm:warm + L]


# --------------------------------------------------------------------------- loudness (BS.1770-ish)
_KB1 = [1.53512485958697, -2.69169618940638, 1.19839281085285]
_KA1 = [1.0, -1.69065929318241, 0.73248077421585]
_KB2 = [1.0, -2.0, 1.0]
_KA2 = [1.0, -1.99004745483398, 0.99007225036621]


def kweight(x):
    return signal.lfilter(_KB2, _KA2, signal.lfilter(_KB1, _KA1, x))


def lufs_momentary_max(x) -> float:
    """Max 400 ms K-weighted loudness (LUFS). Short sounds are measured in a padded 400 ms window."""
    w = ns(0.4)
    y = kweight(np.concatenate([x, np.zeros(w)]))
    ms = uniform_filter1d(y ** 2, w, mode='constant')
    return -0.691 + 10 * np.log10(np.max(ms) + 1e-20)


def lufs_integrated(x, loop=False) -> float:
    """Ungated K-weighted loudness over the whole file (for loops, measured circularly)."""
    y = kweight(np.tile(x, 3))[len(x):2 * len(x)] if loop else kweight(x)
    return -0.691 + 10 * np.log10(np.mean(y ** 2) + 1e-20)


def limiter(x, ceiling: float, hold: float = 0.004) -> np.ndarray:
    """Look-ahead brickwall peak limiter (zero-phase gain computation, offline)."""
    W = ns(hold)
    g = np.minimum(1.0, ceiling / np.maximum(np.abs(x), 1e-12))
    g = minimum_filter1d(g, 2 * W + 1, mode='nearest')
    g = uniform_filter1d(g, W + 1, mode='nearest')
    return x * g


def dc_block(x, loop=False, fc=18.0):
    sos = signal.butter(2, fc, 'high', fs=SR, output='sos')
    if loop:
        y = signal.sosfiltfilt(sos, np.tile(x, 3))
        return y[len(x):2 * len(x)]
    return signal.sosfiltfilt(sos, x, padtype='constant')


def remove_dc(x):
    """Remove net DC area with a smooth (Hann-shaped, sub-audio) correction; keeps edges at 0."""
    w = np.hanning(len(x) + 2)[1:-1]
    return x - np.sum(x) * w / np.sum(w)


def trim(x, thresh_db=-54.0, fade_in=0.001, fade_out=0.02, preroll=0.003):
    a = maximum_filter1d(np.abs(x), ns(0.01))
    pk = a.max()
    idx = np.where(a > pk * 10 ** (thresh_db / 20))[0]
    s = max(0, idx[0] - ns(0.001))
    e = min(len(x), idx[-1] + ns(0.005))
    y = x[s:e].copy()
    fi = min(ns(fade_in), len(y) // 4)
    fo = min(ns(fade_out), len(y) // 3)
    if fi > 1:
        y[:fi] *= 0.5 - 0.5 * np.cos(np.linspace(0, np.pi, fi))
    if fo > 1:
        y[-fo:] *= 0.5 + 0.5 * np.cos(np.linspace(0, np.pi, fo))
    y[-1] = 0.0
    # a few ms of leading silence so lossy codecs' pre-echo doesn't start the file on a step
    return np.concatenate([np.zeros(ns(preroll)), y])
