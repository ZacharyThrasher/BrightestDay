"""Material modal families + the hard-light timbre family.

Every break / debris / bullet impact / construct shatter comes from here, so the whole world
sounds like one physical system:

    material_hit(r, material, size, energy) -> one struck object
    shatter(r, material, count, spread, size, energy) -> a break: main hit + fragment cascade

Materials: stone, wood, metal, glass, crystal, dirt, leaves, hardlight.

Physics sketch: a struck object is a bank of exponentially decaying modes (frequency ratios are a
material/shape fingerprint, decays set by internal damping), excited by a very short contact
transient whose brightness grows with impact energy. Bigger objects are lower and ring longer.
Noise only ever appears as a <= 5 ms contact click, or as a subordinate, band-limited texture where
the source really is noise (dirt crumble, leaf rustle).

Hard light (the ring's constructs) is TONAL and tuned to E minor (pitch classes E G A B D), so it is
consonant with the KO-streak E-minor-pentatonic melody: harmonic partials (1, 2, 3, 4, 6 - never the
5th harmonic, which would add a G# major third), a detuned twin per partial for a slow "light"
shimmer, and a fast harmonic-ratio FM glint on the attack.
"""
from __future__ import annotations

import numpy as np

from dsp import (SR, TAU, add, bounces, env_exp, fit, fm, hpf, lpf, modal, ns, normpk, pink, smooth_noise, thump,
                 tvec)

# --------------------------------------------------------------------------- E minor
_NAMES = {'C': 0, 'C#': 1, 'D': 2, 'D#': 3, 'E': 4, 'F': 5, 'F#': 6, 'G': 7, 'G#': 8, 'A': 9, 'A#': 10, 'B': 11}
PENT = [0, 3, 5, 7, 10]  # E G A B D (semitones above E)


def midi_f(m: float) -> float:
    return 440.0 * 2 ** ((m - 69) / 12)


def note(name: str) -> float:
    """'E4' -> 329.63 Hz, 'F#3', ..."""
    pc, octv = name[:-1], int(name[-1])
    return midi_f(12 * (octv + 1) + _NAMES[pc])


def pent(k: int, base: str = 'E2') -> float:
    """k-th degree of E minor pentatonic above `base` (k=0 -> E2, 1 -> G2, 5 -> E3 ...)."""
    o, d = divmod(int(k), 5)
    return midi_f(12 * (int(base[-1]) + 1) + _NAMES[base[:-1]] + 12 * o + PENT[d])


def pent_index(name: str) -> int:
    """Degree index of a pentatonic note name relative to E2 (e.g. 'E4' -> 10)."""
    m = 12 * (int(name[-1]) + 1) + _NAMES[name[:-1]] - 40
    o, s = divmod(m, 12)
    return 5 * o + PENT.index(s)


def U(r, a, b):
    return r.uniform(a, b)


# --------------------------------------------------------------------------- small building blocks
def click(r, fc=3000.0, d=0.003, amp=1.0) -> np.ndarray:
    """Contact transient: an impulse plus <= d seconds of high-passed noise (never a tail)."""
    n = ns(d) + ns(0.002)
    x = np.zeros(n)
    x[1] = 1.0
    x[:ns(d)] += 0.6 * r.uniform(-1, 1, ns(d)) * np.linspace(1, 0, ns(d)) ** 2
    return amp * normpk(hpf(x, fc, 2))


def micro(r, f, dec=0.003, parts=3, spread=(1.3, 2.9)) -> np.ndarray:
    """A tiny modal tick/tok (grit grain, crinkle, shard): a few inharmonic partials, a few ms long."""
    fr = [f] + [f * U(r, *spread) for _ in range(parts - 1)]
    dc = [dec * U(r, 0.6, 1.2) for _ in fr]
    am = [1.0] + [U(r, 0.25, 0.7) for _ in range(parts - 1)]
    return modal(fr, dc, am, ns(max(dc) * 7 + 0.001), r, attack=0.0002)


def grit(r, count, t_spread, f_lo, f_hi, dec=0.003, amp=1.0, n_len=None) -> np.ndarray:
    """Granular cluster of tiny modal toks (stone grit, debris crumble)."""
    out = np.zeros(n_len or ns(t_spread * 3 + 0.05))
    for _ in range(int(count)):
        t = min(r.exponential(t_spread), t_spread * 3)
        add(out, micro(r, U(r, f_lo, f_hi), dec * U(r, 0.6, 1.5)), t, amp * U(r, 0.3, 1.0) * np.exp(-t / (t_spread * 2)))
    return out


# --------------------------------------------------------------------------- hard light (E minor)
HL_PARTIALS = ((1, 1.0, 1.0), (2, 0.42, 0.62), (3, 0.26, 0.42), (4, 0.16, 0.30), (6, 0.08, 0.2))


def hl_ping(r, f, dec=0.4, bright=1.0, glint=1.0, shimmer=0.45, n=None) -> np.ndarray:
    """One struck hard-light note: harmonic partials in key, a detuned twin per partial (slow beating
    shimmer), and a harmonic-ratio FM glint on the attack."""
    n = n or ns(dec * 7 + 0.01)
    t = tvec(n)
    x = np.zeros(n)
    for k, a, dm in HL_PARTIALS:
        fk = f * k
        if fk > 17000:
            continue
        a = a * (bright ** (k - 1) if k > 1 else 1.0)
        e = np.exp(-t / (dec * dm))
        x += a * e * np.sin(TAU * fk * t + r.uniform(0, TAU))
        if shimmer:
            det = 2 ** (U(r, 2.5, 6.0) / 1200 * (1 if r.uniform() < 0.5 else -1))
            x += shimmer * a * e * np.sin(TAU * fk * det * t + r.uniform(0, TAU))
    if glint:
        m = min(n, ns(0.06))
        g = fm(f * 2, 1.0, 2.5 * np.exp(-tvec(m) / 0.006), m) * env_exp(m, 0.01, 0.0002)
        x[:m] += 0.35 * glint * g
    x *= np.clip(t / 0.0006, 0, 1)
    return x


def hl_tone(r, f, n, env, bright=1.0, shimmer=0.5, beat_hz=None) -> np.ndarray:
    """Sustained hard-light tone (pads, drones, blooms). env: amplitude array of length n."""
    t = tvec(n)
    x = np.zeros(n)
    for k, a, _ in HL_PARTIALS:
        fk = f * k
        if fk > 17000:
            continue
        a = a * (bright ** (k - 1) if k > 1 else 1.0)
        x += a * np.sin(TAU * fk * t + r.uniform(0, TAU))
        if shimmer:
            b = beat_hz if beat_hz is not None else U(r, 0.3, 1.6)
            x += shimmer * a * np.sin(TAU * (fk + b * k ** 0.5) * t + r.uniform(0, TAU))
    return x * env


def hl_chord(r, names_or_freqs, dec=0.6, bright=1.0, stagger=0.0, amps=None, glint=1.0) -> np.ndarray:
    """Struck hard-light chord ('glass gong'): notes may be staggered by `stagger` seconds."""
    fs = [note(x) if isinstance(x, str) else x for x in names_or_freqs]
    amps = amps or [1.0] * len(fs)
    n = ns(dec * 7 + stagger * len(fs) + 0.02)
    out = np.zeros(n)
    for i, (f, a) in enumerate(zip(fs, amps)):
        add(out, hl_ping(r, f, dec * U(r, 0.85, 1.15), bright, glint), i * stagger, a)
    return out


def hl_shards(r, n_runs=6, t_spread=0.8, top='E8', low='E6', dec=(0.03, 0.12), amp=1.0, run_len=(3, 5),
              step=(0.022, 0.045), bright=1.2, n_len=None) -> np.ndarray:
    """Falling arpeggiated hard-light shards: short descending pentatonic runs; later runs start lower."""
    k_top, k_low = pent_index(top), pent_index(low)
    out = np.zeros(n_len or ns(t_spread + 0.9))
    for i in range(n_runs):
        u = (i + U(r, 0, 0.8)) / n_runs
        t0 = t_spread * u ** 1.4
        k = int(round(k_top - (k_top - k_low) * u ** 0.8 + U(r, -1.4, 1.4)))
        a_run = amp * (1 - 0.65 * u) * U(r, 0.6, 1.0)
        tt = t0
        for j in range(int(r.integers(run_len[0], run_len[1] + 1))):
            kk = min(k - j, k_top)
            if kk < k_low - 3:
                break
            f = pent(kk)
            if f > 12000:
                continue
            p = hl_ping(r, f, U(r, *dec), bright, glint=0.6, shimmer=0.3)
            add(out, p + 0.25 * fit(click(r, 6000, 0.0008), len(p)), tt, a_run * (0.85 ** j))
            tt += U(r, *step)
    return out


# --------------------------------------------------------------------------- materials
# ratios: modal frequency fingerprint; f: base-mode range (Hz) at size 1; dec: decay range of the
# lowest mode (s) at size 1; tilt: how fast upper-mode decays shorten; brightness of the contact.
MATERIALS = {
    'stone': dict(ratios=[1.0, 1.47, 1.93, 2.51, 3.08, 3.72, 4.46, 5.3], f=(420, 900), dec=(0.02, 0.07),
                  tilt=0.35, click=(1800, 0.003), jitter=0.05),
    'wood': dict(ratios=[1.0, 2.57, 4.12, 5.9], f=(170, 480), dec=(0.05, 0.12), tilt=0.7, click=(1500, 0.002),
                 jitter=0.04),
    'metal': dict(ratios=[1.0, 2.76, 5.40, 8.93, 1.59, 2.14, 3.51, 4.23], f=(260, 820), dec=(0.5, 1.4),
                  tilt=0.3, click=(3000, 0.0015), jitter=0.015),
    'glass': dict(ratios=[1.0, 2.32, 4.25, 6.63, 9.38], f=(1900, 4200), dec=(0.12, 0.45), tilt=0.5,
                  click=(5000, 0.001), jitter=0.02),
}


def _modal_body(r, m, f, dec, energy, n_modes=None):
    ratios = m['ratios'][:n_modes] if n_modes else m['ratios']
    f = min(f, 9000.0)  # tiny fragments: the fundamental saturates (they get brighter via fewer modes)
    fr, dc, am = [], [], []
    for i, k in enumerate(ratios):
        fk = f * k * (1 + U(r, -m['jitter'], m['jitter']) if i else 1)
        if fk > 16000:
            continue
        fr.append(fk)
        dc.append(dec * U(r, 0.8, 1.2) / (1 + m['tilt'] * i))
        # harder hits excite upper modes more
        am.append((1.0 if i == 0 else U(r, 0.35, 0.9) / (1 + 0.35 * i)) * (energy ** (0.15 * i)))
    n = ns(max(dc) * 6 + 0.01)
    return modal(fr, dc, am, n, r, attack=0.0002)


def material_hit(r, material: str, size: float = 1.0, energy: float = 1.0) -> np.ndarray:
    """One struck object. size ~0.2 (chip) .. 3 (big block); energy ~0.3 (tap) .. 2 (slam)."""
    if material == 'hardlight':
        root = r.choice(['E4', 'G4', 'B4', 'E5', 'A4', 'D5'])
        k0 = pent_index(root) - int(round(3 * np.log2(max(size, 0.25))))
        ch = [pent(k0), pent(k0 + 2), pent(k0 + 5)]
        x = hl_chord(r, ch, dec=0.25 * size ** 0.5, bright=0.9 + 0.2 * energy, stagger=0.004, amps=[1, .7, .5])
        add(x, click(r, 4000, 0.0015), 0, 0.4 * energy)
        return x
    if material == 'crystal':
        # natural crystal: glass-like but tuned (E minor pentatonic) so crystal breaks chime in key
        f = pent(pent_index('E6') + int(r.integers(0, 6)) - int(round(3 * np.log2(max(size, 0.25)))))
        dec = U(r, 0.35, 0.8) * size ** 0.4
        x = modal([f, f * 2.0, f * 3.0, f * 4.16, f * 5.43], np.array([1, .6, .4, .22, .12]) * dec,
                  [1, .45, .3, .16, .08], ns(dec * 6), r)
        add(x, click(r, 5000, 0.001), 0, 0.5 * energy)
        return x
    if material == 'dirt':
        n = ns(0.35)
        x = np.zeros(n)
        f0 = U(r, 110, 170) / size ** 0.4
        add(x, thump(0.2, f0, f0 * 0.55, 0.02, 0.03 * size ** 0.5, drive=0), 0, 0.8)
        for _ in range(int(3 + 3 * energy)):  # soft clods: low toks, damped
            add(x, lpf(micro(r, U(r, 240, 600), 0.012, 2, (1.4, 2.0)), 1400, 2), r.exponential(0.025),
                U(r, 0.25, 0.5))
        m = ns(0.2)
        crum = lpf(pink(m, r) * (0.5 + 0.5 * smooth_noise(m, r, 150)), 1100, 2) * env_exp(m, 0.03, 0.002)
        add(x, normpk(crum), 0, 0.04 * energy)
        return x
    if material == 'leaves':
        n = ns(0.5 * size ** 0.3 + 0.1)
        x = np.zeros(n)
        cnt = int(30 * size * energy + 12)
        for _ in range(cnt):  # crinkles: tiny high modal ticks, clustered
            t = min(r.gamma(1.5, 0.05 * size ** 0.3), n / SR - 0.02)
            add(x, micro(r, U(r, 1800, 6500), U(r, 0.0012, 0.003), 3, (1.2, 1.9)), t, U(r, 0.2, 1.0) * np.exp(-t / 0.2))
        puff = hpf(lpf(pink(ns(0.3), r), 1800, 2), 350, 2) * env_exp(ns(0.3), 0.05, 0.01)
        add(x, normpk(puff), 0, 0.15)
        return x
    m = MATERIALS[material]
    f = U(r, *m['f']) / size ** 0.6
    dec = U(r, *m['dec']) * size ** 0.5
    layers = [(_modal_body(r, m, f, dec, energy), 0.0, 1.0)]
    fc, cd = m['click']
    layers.append((click(r, fc, cd), 0.0, 0.35 * energy))
    if material == 'stone':
        layers.append((grit(r, 5 + 10 * energy * size ** 0.5, 0.012, 1500, 4800, 0.0025), 0.001, 0.35 * energy))
        if size > 1.2:
            layers.append((thump(0.12, f * 0.4, f * 0.25, 0.01, 0.025 * size, drive=0), 0.0, 0.5))
    elif material == 'wood':
        # dry crack: a brief splinter of fibres (<= 10 ms of tiny high ticks)
        layers.append((grit(r, 6 * energy + 3, 0.003, 2200, 6000, 0.0015), 0.0005, 0.3 * energy))
    elif material == 'glass':
        layers.append((grit(r, 4 * energy, 0.004, 5000, 9500, 0.004), 0.0, 0.3 * energy))
    x = np.zeros(max(len(l) + ns(t) for l, t, _ in layers) + ns(0.01))
    for l, t, a in layers:
        add(x, l, t, a)
    return x


def shatter(r, material: str, count: int = 12, spread: float = 0.5, size: float = 1.0, energy: float = 1.0,
            bounce: bool = True) -> np.ndarray:
    """A break: the main hit, then `count` fragments over ~`spread` s (front-loaded), smaller pieces
    higher and quieter; solid fragments bounce to rest."""
    if material == 'hardlight':
        out = np.zeros(ns(spread + 1.2))
        add(out, material_hit(r, 'hardlight', size, energy), 0, 1.0)
        add(out, hl_shards(r, max(2, count // 3), spread, 'E8' if size < 1.5 else 'B7', 'E6' if size < 1.5 else 'B5',
                           amp=0.55), 0.01, 1.0)
        return out
    out = np.zeros(ns(spread * 2 + 1.6))
    add(out, material_hit(r, material, size, energy), 0, 1.0)
    for i in range(int(count)):
        t = spread * r.beta(1.1, 3.0)
        fs = size * U(r, 0.15, 0.5)
        a = energy * U(r, 0.25, 0.7) * (1 - 0.6 * t / (spread + 1e-9))
        if material == 'dirt':  # clods: a soft low tok each, no thud of their own
            add(out, lpf(micro(r, U(r, 250, 650), 0.012, 2, (1.4, 2.0)), 1500, 2), t, a * 0.6)
        elif bounce and material in ('stone', 'wood', 'metal', 'glass', 'crystal') and r.uniform() < 0.6:
            for (tb, ab) in bounces(r, t, a, U(r, 0.05, 0.14), U(r, 0.5, 0.68), 0.55, t + spread * 1.5 + 0.2):
                add(out, material_hit(r, material, fs, 0.4 + 0.4 * ab), tb, ab)
        else:
            add(out, material_hit(r, material, fs, 0.5 + 0.5 * a), t, a)
    return out
