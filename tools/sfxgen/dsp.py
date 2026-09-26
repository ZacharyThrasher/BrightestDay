"""Small DSP toolkit for Brightest Day's procedural SFX.

Everything is pure synthesis (oscillators, noise, filters, modal resonators,
EQ carving by frequency role). Sounds are DRY: the game mixer supplies the one shared space.
Functions work on mono float64 numpy arrays at SR unless noted (mastering helpers accept (n, 2) stereo).
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


def glue(x, drive: float = 1.6) -> np.ndarray:
    """Scale-independent soft clip (normalise, then tanh): lowers crest factor a few dB so short
    impacts reach their loudness without brickwall limiting. drive 1.5 ~ +3 dB on the body."""
    m = np.max(np.abs(x))
    return np.tanh(drive * x / m) / np.tanh(drive) if m > 0 else x


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


# --------------------------------------------------------------------------- carving EQ (frequency roles)
def hpf(x, fc, order=4):
    """Steep Butterworth high-pass (default 24 dB/oct): foley/UI carving."""
    return signal.sosfilt(_sos('high', fc, order), x)


def lpf(x, fc, order=4):
    return signal.sosfilt(_sos('low', fc, order), x)


def peq(x, f0, gain_db, q=1.0):
    """RBJ peaking EQ (cut or boost around f0)."""
    A = 10 ** (gain_db / 40)
    w0 = TAU * min(f0, SR * 0.45) / SR
    alpha = np.sin(w0) / (2 * q)
    b = [1 + alpha * A, -2 * np.cos(w0), 1 - alpha * A]
    a = [1 + alpha / A, -2 * np.cos(w0), 1 - alpha / A]
    return signal.lfilter(np.array(b) / a[0], np.array(a) / a[0], x)


def hshelf(x, f0, gain_db, s=1.0):
    """RBJ high shelf."""
    A = 10 ** (gain_db / 40)
    w0 = TAU * min(f0, SR * 0.45) / SR
    cw, sw = np.cos(w0), np.sin(w0)
    alpha = sw / 2 * np.sqrt((A + 1 / A) * (1 / s - 1) + 2)
    sq = 2 * np.sqrt(A) * alpha
    b = [A * ((A + 1) + (A - 1) * cw + sq), -2 * A * ((A - 1) + (A + 1) * cw), A * ((A + 1) + (A - 1) * cw - sq)]
    a = [(A + 1) - (A - 1) * cw + sq, 2 * ((A - 1) - (A + 1) * cw), (A + 1) - (A - 1) * cw - sq]
    return signal.lfilter(np.array(b) / a[0], np.array(a) / a[0], x)


def carve_heavy(x, notch_db=-6.0, air_db=-5.0):
    """Explosions / heavy impacts / kaiju / collapse: leave room on top for hit confirms and footsteps:
    a broad dip over 2-5 kHz and a modest top above 6 kHz."""
    y = peq(x, 3200.0, notch_db, 0.8)
    return hshelf(y, 6500.0, air_db)


def carve_foley(x, hp_fc=200.0, lp_fc=None):
    """Footsteps / foley / movement / tethers: live in the mids, never add mud."""
    y = hpf(x, hp_fc, 4)
    return lpf(y, lp_fc, 2) if lp_fc else y


def carve_ui(x, hp_fc=1800.0, lp_fc=13000.0):
    """UI / feedback: small but bright (energy centred 5-10 kHz). ~36 dB/oct high-pass."""
    y = hpf(hpf(x, hp_fc, 4), hp_fc * 0.8, 2)
    return lpf(y, lp_fc, 2) if lp_fc else y


def qf(f: float, period: float) -> float:
    """Quantise a frequency so that f * period is an integer (strictly periodic loops)."""
    return max(1, round(f * period)) / period


# --------------------------------------------------------------------------- stereo
def st(l: np.ndarray, r: np.ndarray) -> np.ndarray:
    n = max(len(l), len(r))
    return np.stack([fit(l, n), fit(r, n)], axis=1)


def is_st(x) -> bool:
    return np.ndim(x) == 2


def chans(x):
    return [x[:, 0], x[:, 1]] if is_st(x) else [x]


def monosum(x):
    return x.mean(axis=1) if is_st(x) else x


# --------------------------------------------------------------------------- loudness (BS.1770-ish)
_KB1 = [1.53512485958697, -2.69169618940638, 1.19839281085285]
_KA1 = [1.0, -1.69065929318241, 0.73248077421585]
_KB2 = [1.0, -2.0, 1.0]
_KA2 = [1.0, -1.99004745483398, 0.99007225036621]


def kweight(x):
    return signal.lfilter(_KB2, _KA2, signal.lfilter(_KB1, _KA1, x))


def lufs_momentary_max(x) -> float:
    """Max 400 ms K-weighted loudness (LUFS). Short sounds are measured in a padded 400 ms window.
    Stereo: channel powers are summed (BS.1770, L/R weight 1)."""
    w = ns(0.4)
    ms = 0.0
    for c in chans(x):
        y = kweight(np.concatenate([c, np.zeros(w)]))
        ms = ms + uniform_filter1d(y ** 2, w, mode='constant')
    return -0.691 + 10 * np.log10(np.max(ms) + 1e-20)


def lufs_integrated(x, loop=False) -> float:
    """Ungated K-weighted loudness over the whole file (for loops, measured circularly)."""
    p = 0.0
    for c in chans(x):
        y = kweight(np.tile(c, 3))[len(c):2 * len(c)] if loop else kweight(c)
        p += np.mean(y ** 2)
    return -0.691 + 10 * np.log10(p + 1e-20)


def peak(x) -> float:
    return float(np.max(np.abs(x)))


def limiter(x, ceiling: float, hold: float = 0.004) -> np.ndarray:
    """Look-ahead brickwall peak limiter (zero-phase gain computation, offline). Linked stereo."""
    W = ns(hold)
    a = np.max(np.abs(x), axis=1) if is_st(x) else np.abs(x)
    g = np.minimum(1.0, ceiling / np.maximum(a, 1e-12))
    g = minimum_filter1d(g, 2 * W + 1, mode='nearest')
    g = uniform_filter1d(g, W + 1, mode='nearest')
    return x * (g[:, None] if is_st(x) else g)


def dc_block(x, loop=False, fc=18.0):
    if is_st(x):
        return st(dc_block(x[:, 0], loop, fc), dc_block(x[:, 1], loop, fc))
    sos = signal.butter(2, fc, 'high', fs=SR, output='sos')
    if loop:
        y = signal.sosfiltfilt(sos, np.tile(x, 3))
        return y[len(x):2 * len(x)]
    return signal.sosfiltfilt(sos, x, padtype='constant')


def remove_dc(x):
    """Remove net DC area with a smooth (Hann-shaped, sub-audio) correction; keeps edges at 0."""
    if is_st(x):
        return st(remove_dc(x[:, 0]), remove_dc(x[:, 1]))
    w = np.hanning(len(x) + 2)[1:-1]
    return x - np.sum(x) * w / np.sum(w)


def trim(x, thresh_db=-54.0, fade_in=0.001, fade_out=0.02, preroll=0.003):
    env = np.max(np.abs(x), axis=1) if is_st(x) else np.abs(x)
    a = maximum_filter1d(env, ns(0.01))
    pk = a.max()
    idx = np.where(a > pk * 10 ** (thresh_db / 20))[0]
    s = max(0, idx[0] - ns(0.001))
    e = min(len(x), idx[-1] + ns(0.005))
    y = x[s:e].copy()
    fi = min(ns(fade_in), len(y) // 4)
    fo = min(ns(fade_out), len(y) // 3)
    shp = (lambda v: v[:, None]) if is_st(y) else (lambda v: v)
    if fi > 1:
        y[:fi] *= shp(0.5 - 0.5 * np.cos(np.linspace(0, np.pi, fi)))
    if fo > 1:
        y[-fo:] *= shp(0.5 + 0.5 * np.cos(np.linspace(0, np.pi, fo)))
    y[-1] = 0.0
    # a few ms of leading silence so lossy codecs' pre-echo doesn't start the file on a step
    pad = np.zeros((ns(preroll),) + y.shape[1:])
    return np.concatenate([pad, y])


def seam_candidates(x, k: int = 8) -> list[int]:
    """For a seamless loop: good places to put the wrap point - quiet in the >6 kHz band, low level,
    small sample step - one per 1/k of the loop, best first. Rolling a loop is free (the content is
    unchanged); the writer encodes a few candidates and keeps the one whose *decoded* seam is cleanest,
    because the Vorbis encoder's edge error depends on where the file starts."""
    m = monosum(x)
    L = len(m)
    h = hp(np.tile(m, 3), 6000, 4)[L:2 * L] ** 2
    h = uniform_filter1d(h, ns(0.004), mode='wrap')
    e = uniform_filter1d(m ** 2, ns(0.004), mode='wrap')
    step = np.abs(m - np.roll(m, 1))
    score = h / (np.median(h) + 1e-20) + e / (np.median(e) + 1e-20) + 4 * step / (np.percentile(step, 99) + 1e-12)
    seg = L // k
    c = [s + int(np.argmin(score[s:s + seg])) for s in range(0, seg * k, seg)]
    return sorted(c, key=lambda i: score[i])


def seam_metrics(x):
    """Return (jump_ratio, hf_ratio) for one loop channel. jump: |x[0]-x[-1]| / p99(|diff|).
    hf: HF (>6 kHz) energy in 4 ms around the wrap vs. median 4 ms window energy of the loop, floored
    at -45 dB re the loop's mean power (a >6 kHz burst that far below the loop is inaudible under it;
    tonal loops have almost no HF, so without a floor the codec's edge error dominates the ratio)."""
    d = np.abs(np.diff(x))
    jump = abs(x[0] - x[-1]) / (np.percentile(d, 99) + 1e-12)
    h = hp(np.tile(x, 3), 6000, 4)[len(x):2 * len(x)]
    h = np.roll(h, len(x) // 2)
    w = ns(0.004)
    c = len(x) // 2
    e_seam = np.mean(h[c - w // 2:c + w // 2] ** 2)
    frames = h[:len(h) // w * w].reshape(-1, w)
    e_med = np.median(np.mean(frames ** 2, axis=1))
    floor = np.mean(x ** 2) * 10 ** (-45 / 10)
    return jump, e_seam / (max(e_med, floor) + 1e-20)


def hi_share(x, split: float = 2000.0) -> float:
    X = np.abs(np.fft.rfft(monosum(x))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / SR)
    return float(X[f >= split].sum() / (X.sum() + 1e-20))


def balance_hi(base, bright, target: float = 0.42, split: float = 2000.0, g_max: float = 8.0):
    """Mix `bright` into `base` at the gain that puts `target` of the energy above `split` Hz
    (bisection). Used by feedback sounds that must own the top of the spectrum without losing
    their body (e.g. the KO-streak notes keep their fundamental for the melody)."""
    n = max(len(base), len(bright))
    base, bright = fit(base, n), fit(bright, n)
    if hi_share(base) >= target:
        return base + 0.5 * bright
    lo, hi = 0.0, g_max
    if hi_share(base + hi * bright) < target:
        return base + hi * bright
    for _ in range(24):
        g = 0.5 * (lo + hi)
        lo, hi = (g, hi) if hi_share(base + g * bright) < target else (lo, g)
    return base + hi * bright


# --------------------------------------------------------------------------- analysis (verify.py)
def _frames(x, nfft=4096, hop=1024):
    x = monosum(np.asarray(x, float))
    if len(x) < nfft:
        x = np.concatenate([x, np.zeros(nfft - len(x))])
    idx = np.arange(0, len(x) - nfft + 1, hop)
    win = np.hanning(nfft)
    F = np.stack([x[i:i + nfft] * win for i in idx])
    P = np.abs(np.fft.rfft(F, axis=1)) ** 2
    return P, np.fft.rfftfreq(nfft, 1 / SR)


def _loud(P, floor_db=30.0):
    e = P.sum(axis=1)
    return e >= e.max() * 10 ** (-floor_db / 10)


def spectral_flatness(x, lo=60.0, hi=16000.0) -> float:
    """Median full-band spectral flatness (Wiener entropy: 0 = pure tone, ~0.56 = white noise)
    over the loud frames (within 30 dB of the loudest)."""
    P, f = _frames(x)
    m = (f >= lo) & (f < hi)
    Q = P[:, m] + 1e-20
    sfm = np.exp(np.mean(np.log(Q), axis=1)) / np.mean(Q, axis=1)
    return float(np.median(sfm[_loud(P)]))


def noisiness(x) -> float:
    """Energy-weighted octave-band spectral flatness, median over loud frames. Unlike full-band
    flatness this also flags *band-limited* noise (filtered hiss) as noisy: ~0.5-0.6 for any noise,
    near 0 for partials / tones / modal rings."""
    P, f = _frames(x)
    edges = [125 * 2 ** k for k in range(8)]  # 125 Hz .. 16 kHz
    bands = [(f >= lo) & (f < hi) for lo, hi in zip(edges[:-1], edges[1:])]
    vals = []
    for row in P[_loud(P)]:
        num = den = 0.0
        for m in bands:
            q = row[m] + 1e-20
            e = q.sum()
            num += e * np.exp(np.mean(np.log(q))) / np.mean(q)
            den += e
        vals.append(num / (den + 1e-20))
    return float(np.median(vals))


# --------------------------------------------------------------------------- circular (loop) processing
def circ(fn, x):
    """Apply a causal filter fn to an L-periodic signal circularly (render 3 periods, keep the middle)."""
    L = len(x)
    return fn(np.tile(x, 3))[L:2 * L]


def pitch_trend(x, t0=0.03, t1=0.66):
    """Slope (octaves/s) of the 150-12000 Hz spectral centroid over [t0, t1] after onset,
    using 20 ms frames within 20 dB of the loudest (equally weighted)."""
    on = int(np.argmax(np.abs(x) > 0.02 * np.max(np.abs(x))))
    w = ns(0.02)
    f = np.fft.rfftfreq(4096, 1 / SR)
    m = (f > 150) & (f < 12000)
    ts, cs, es = [], [], []
    for s in range(on + ns(t0), min(len(x), on + ns(t1)) - w, w // 2):
        X = np.abs(np.fft.rfft(x[s:s + w] * np.hanning(w), 4096)) ** 2
        e = X[m].sum()
        ts.append(s / SR); cs.append(np.log2((f[m] * X[m]).sum() / (e + 1e-20))); es.append(e)
    es = np.array(es)
    keep = es > es.max() * 0.01
    return float(np.polyfit(np.array(ts)[keep], np.array(cs)[keep], 1)[0])
