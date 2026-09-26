"""Sound definitions for Brightest Day.

Each @sound function takes (rng, v) — a seeded numpy Generator and the variation
index — and returns a mono float signal (any scale; the pipeline normalises).
Loop sounds must return an exactly-seamless loop (see dsp.loop_xfade / dsp.periodic).
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Callable

import numpy as np

from dsp import *  # noqa: F401,F403  (tiny DSP vocabulary)
from dsp import SR, TAU

# Loudness targets (BS.1770 K-weighted). One-shots: max momentary (400 ms) LUFS.
# Loops: integrated LUFS. Peak ceiling -1 dBFS everywhere.
CATEGORY_LUFS = {
    'movement': -18.0,
    'tether': -17.0,
    'construct': -15.0,
    'combat': -12.0,
    'destruction': -15.0,
    'ui': -21.0,
    'loop': -21.0,
}


@dataclass
class Sound:
    name: str
    fn: Callable
    category: str
    variations: int = 3
    loop: bool = False
    volume: float = 1.0
    pitch: tuple = (0.95, 1.05)
    lufs: float | None = None
    notes: str = ''
    files: list = field(default_factory=list)

    @property
    def target(self) -> float:
        if self.lufs is not None:
            return self.lufs
        return CATEGORY_LUFS['loop' if self.loop else self.category]


REGISTRY: dict[str, Sound] = {}


def sound(name, category, variations=3, loop=False, volume=1.0, pitch=(0.95, 1.05), lufs=None, notes=''):
    def deco(fn):
        REGISTRY[name] = Sound(name, fn, category, variations, loop, volume, pitch, lufs, notes)
        return fn
    return deco


def U(rng, a, b):
    return rng.uniform(a, b)


# =============================================================================== movement
@sound('jump', 'movement', 3, volume=0.6, pitch=(0.94, 1.08))
def jump(r, v):
    out = buf(0.4)
    add(out, thump(0.1, 150 * U(r, .9, 1.1), 75, 0.025, 0.03, drive=1.2), 0, 0.7)
    add(out, burst(0.07, r, 250, 2500, 0.014), 0, 0.45)
    add(out, whoosh(0.3, r, 450, 2600 * U(r, .9, 1.2), 1.3, [(0, 0), (0.06, 1), (0.3, 0)]), 0.008, 0.55)
    return out


@sound('land_light', 'movement', 3, volume=0.6, pitch=(0.92, 1.08), lufs=-19)
def land_light(r, v):
    out = buf(0.35)
    add(out, thump(0.12, 110 * U(r, .9, 1.1), 55, 0.03, 0.045, drive=1.4), 0, 1.0)
    add(out, burst(0.1, r, None, 1600, 0.03, color='pink'), 0, 0.6)
    add(out, burst(0.02, r, 1500, 5000, 0.004), 0, 0.25)
    for _ in range(6):
        add(out, grain(r, U(r, .002, .006), 1800, 5000), U(r, 0.004, 0.09), U(r, .05, .15))
    return out


@sound('land_heavy', 'movement', 3, volume=0.85, pitch=(0.92, 1.05), lufs=-15,
       notes='bassy thud for high-speed / long-fall landings')
def land_heavy(r, v):
    out = buf(1.0)
    add(out, burst(0.05, r, None, 3000, 0.006), 0, 0.8)
    add(out, thump(0.7, 100 * U(r, .9, 1.1), 36, 0.06, 0.22, drive=2.6), 0, 1.0)
    add(out, burst(0.4, r, None, 260, 0.12, color='brown'), 0, 0.7)
    for i in range(18):
        t = r.exponential(0.1)
        add(out, grain(r, U(r, .003, .01), 1200, 5000), t, 0.2 * np.exp(-t / 0.2))
    return reverb(out, r, 0.6, 0.15)


@sound('footstep', 'movement', 4, volume=0.35, pitch=(0.9, 1.1), lufs=-22, notes='short, soft')
def footstep(r, v):
    out = buf(0.18)
    tilt = U(r, 1200, 2400)
    add(out, burst(0.06, r, 150, tilt, 0.012, color='pink'), 0, 1.0)
    add(out, thump(0.06, 95, 70, 0.02, 0.018, drive=0), 0, 0.5)
    add(out, burst(0.05, r, 200, tilt * 1.3, 0.009, color='pink'), U(r, 0.03, 0.055), U(r, .35, .55))
    return out


@sound('slide_loop', 'movement', 2, loop=True, volume=0.5, pitch=(0.97, 1.03), lufs=-21,
       notes='loop; scale volume with slide speed')
def slide_loop(r, v):
    L, X = ns(2.0), ns(0.3)
    n = L + X
    fc = 1200 * 2 ** (0.5 * smooth_noise(n, r, 1.5))
    base = normpk(tvf(pink(n, r), fc, 0.9, 'bp'))
    grit = np.zeros(n)
    for t in np.sort(r.uniform(0, n / SR, int(180 * n / SR))):
        add(grit, grain(r, U(r, .001, .004), 2000, 7000), t, U(r, 0.1, 1.0))
    low = normpk(lp(brown(n, r), 160))
    am = 1 + 0.25 * smooth_noise(n, r, 3)
    x = am * (base + 0.35 * grit + 0.45 * low)
    return loop_xfade(x, L, X)


@sound('wind_loop', 'movement', 2, loop=True, volume=0.5, pitch=(1.0, 1.0), lufs=-22,
       notes='loop; game scales volume/pitch with speed')
def wind_loop(r, v):
    L, X = ns(4.0), ns(0.8)
    n = L + X
    main = normpk(tvf(brown(n, r) + pink(n, r), 600 * 2 ** (0.8 * smooth_noise(n, r, 0.35)), 1.1, 'bp'))
    hiss = normpk(hp(pink(n, r), 3000)) * (0.6 + 0.4 * smooth_noise(n, r, 1.0))
    whistle = normpk(tvf(white(n, r), 1400 * U(r, .8, 1.2) * 2 ** (0.5 * smooth_noise(n, r, 0.25)), 14, 'bp'))
    low = normpk(lp(brown(n, r), 120)) * (0.7 + 0.3 * smooth_noise(n, r, 0.5))
    x = main + 0.1 * hiss + 0.15 * whistle + 0.6 * low
    return loop_xfade(x, L, X)


@sound('boost_burst', 'movement', 3, volume=0.8, pitch=(0.94, 1.06), lufs=-16, notes='gas hiss burst')
def boost_burst(r, v):
    d = 0.7
    n = ns(d)
    out = buf(d)
    add(out, crack(r, 1000, 0.003), 0, 0.6)
    add(out, thump(0.12, 95, 50, 0.02, 0.04, drive=1.5), 0, 0.55)
    hiss = normpk(tvf(white(n, r), sweep(n, 8000 * U(r, .9, 1.1), 3000, 0.5), 0.8, 'bp'))
    flutter = 1 + 0.2 * np.sin(TAU * U(r, 30, 45) * tvec(n)) + 0.15 * smooth_noise(n, r, 40)
    add(out, hiss * flutter * env_exp(n, 0.17, 0.005), 0, 1.0)
    add(out, burst(d, r, None, 400, 0.14, attack=0.01, color='brown'), 0, 0.4)
    add(out, whoosh(0.5, r, 2200, 600, 1.0, [(0, 0), (0.04, 1), (0.5, 0)]), 0, 0.3)
    return out


@sound('boost_loop', 'movement', 2, loop=True, volume=0.6, pitch=(0.98, 1.02), lufs=-20)
def boost_loop(r, v):
    L, X = ns(2.0), ns(0.3)
    n = L + X
    hiss = normpk(bp(white(n, r), 3000, 9000)) * (1 + 0.18 * smooth_noise(n, r, 25))
    roar = normpk(lp(brown(n, r), 320)) * (1 + 0.2 * smooth_noise(n, r, 6))
    mid = normpk(reson(pink(n, r), 1200 * U(r, .9, 1.1), 1.5))
    return loop_xfade(hiss + 0.55 * roar + 0.25 * mid, L, X)


# =============================================================================== tethers
def _pulse_train(n, rate):
    cyc = np.cumsum(np.broadcast_to(rate, (n,))) / SR
    tr = np.zeros(n)
    tr[1:][np.diff(np.floor(cyc)) > 0] = 1.0
    return tr


@sound('tether_fire', 'tether', 3, volume=0.8, pitch=(0.95, 1.06), notes='sharp mechanical zip')
def tether_fire(r, v):
    d = 0.5
    n = ns(d)
    out = buf(d)
    add(out, crack(r, 3000, 0.002), 0, 0.8)
    f = U(r, 2100, 2500)
    add(out, modal([f, f * 1.61, f * 2.26], [0.03, 0.02, 0.015], [1, .6, .4], ns(0.15), r), 0, 0.35)
    rate = sweep(n, 650, 110, 0.7)
    tr = _pulse_train(n, rate) * env_exp(n, 0.14, 0.002)
    z = reson(tr, 3200 * U(r, .9, 1.1), 5) + 0.7 * reson(tr, 1400, 4)
    add(out, normpk(z), 0.004, 0.7)
    add(out, whoosh(0.32, r, 4200, 1400, 1.2, [(0, 0), (0.02, 1), (0.32, 0)], color='white'), 0, 0.45)
    nn = ns(0.12)
    add(out, sine(sweep(nn, 2300, 850, 0.5), nn) * env_exp(nn, 0.025), 0, 0.2)
    return out


@sound('tether_attach', 'tether', 3, volume=0.85, pitch=(0.93, 1.07), notes='metallic thunk into stone')
def tether_attach(r, v):
    out = buf(1.6)
    add(out, crack(r, 1500, 0.004), 0, 0.9)
    add(out, thump(0.2, 170, 90, 0.03, 0.06, drive=2.0), 0, 0.9)
    f = U(r, 470, 640)
    ratios = [1, 2.76, 5.40, 8.93, U(r, 3.3, 3.9)]
    add(out, modal([f * k for k in ratios], [0.35, 0.2, 0.12, 0.07, 0.15], [0.5, 0.35, 0.25, 0.15, 0.2],
                   ns(1.6), r), 0, 0.5)
    for _ in range(10):
        add(out, grain(r, U(r, .003, .008), 1000, 4000), r.exponential(0.015), U(r, .1, .3))
    return reverb(out, r, 0.5, 0.18)


@sound('tether_reel_loop', 'tether', 2, loop=True, volume=0.55, pitch=(1.0, 1.0), lufs=-21, notes='loop; winch')
def tether_reel_loop(r, v):
    Ld = 2.0
    L, X = ns(Ld), ns(0.25)
    f0 = [160.0, 180.0][v % 2]  # f*Ld integer => exactly periodic
    click_rate = [22.0, 26.0][v % 2]

    def motor(n):
        t = tvec(n)
        vib = 4.0 * np.sin(TAU * 6.0 * t)
        m = saw(f0, n) + saw(f0 + 0.5, n) + 0.5 * saw(2 * f0, n)
        m = lp(m, 1800)
        gear = 1 + 0.25 * np.sin(TAU * click_rate * t)
        whine = sine(8 * f0 + vib, n)
        return normpk(m) * gear + 0.18 * whine
    tonal = periodic(motor, L)
    clicks = np.zeros(L)
    k = int(round(click_rate * Ld))
    for i in range(k):
        c = modal([U(r, 2500, 2700), U(r, 4000, 4200)], [0.006, 0.004], [1, .6], ns(0.04), r)
        c += 0.5 * crack(r, 3000, 0.0015, d=0.04)
        add_circ(clicks, c, int((i + 0.5) * L / k), U(r, 0.7, 1.0))  # keep clicks off the seam
    hiss = normpk(bp(white(L + X, r), 2000, 5000))
    hiss = loop_xfade(hiss, L, X)
    return 0.6 * tonal + 0.5 * normpk(clicks) + 0.12 * hiss


@sound('tether_release', 'tether', 3, volume=0.75, pitch=(0.94, 1.06), notes='snap + whoosh')
def tether_release(r, v):
    out = buf(0.9)
    add(out, crack(r, 2500, 0.0025), 0, 1.0)
    add(out, thump(0.06, 1800, 600, 0.01, 0.015, drive=0), 0, 0.4)
    add(out, thump(0.1, 120, 70, 0.02, 0.03, drive=1.5), 0, 0.4)
    f = U(r, 180, 240)
    add(out, modal([f, 2 * f, 3 * f, 4.02 * f], [.15] * 4, [1, .5, .3, .2], ns(0.85), r), 0, 0.12)
    add(out, whoosh(0.45, r, 3000, 500, 1.0, [(0, 0), (0.03, 1), (0.45, 0)]), 0.005, 0.6)
    return out


# =============================================================================== ring / constructs
@sound('construct_form', 'construct', 3, volume=0.8, pitch=(0.97, 1.03),
       notes='rising hard-light vwoom ~0.4 s + lock ping')
def construct_form(r, v):
    d = 0.9
    n = ns(d)
    t = tvec(n)
    f_end = [130.8, 146.8, 164.8][v % 3] * 1.0
    u = np.clip(t / 0.36, 0, 1)
    f = f_end / 2 * 2 ** (1 - (1 - u) ** 2)  # octave rise, ease-out
    fc = 200 * (7000 / 200) ** (u ** 1.6)
    body = tvf(supersaw(f, n, r, 5, 18), fc, 2.5, 'lp')
    env = env_pts(n, [(0, 0), (0.05, 0.6), (0.33, 1.0), (0.55, 0)])
    out = normpk(body) * env
    out += 0.5 * sine(f / 2, n) * env
    sh = sum(sine(f * k, n, r.uniform(0, TAU)) for k in (8, 12, 16, 20))
    trem = 0.5 + 0.5 * np.sin(TAU * np.cumsum(18 + 14 * u) / SR)
    out += 0.06 * sh * trem * env_pts(n, [(0, 0), (0.2, 0.3), (0.4, 1), (0.6, 0)])
    add(out, fm_bell(ns(0.55), f_end * 16, 1.414, 3, 0.05, 0.12), 0.34, 0.22)
    return reverb(out, r, 0.7, 0.25, bright=1.3)


@sound('construct_dismiss', 'construct', 3, volume=0.6, pitch=(0.95, 1.05))
def construct_dismiss(r, v):
    d = 0.45
    n = ns(d)
    f = glide(n, 150 * U(r, .95, 1.05), 50, 0.12)
    body = tvf(supersaw(f, n, r, 5, 20), glide(n, 6000, 150, 0.1), 2.0, 'lp')
    out = normpk(body) * env_exp(n, 0.12, 0.005)
    out += 0.4 * sine(f / 2, n) * env_exp(n, 0.1, 0.005)
    add(out, whoosh(0.18, r, 4000, 800, 1.2, [(0, 0), (0.01, 1), (0.18, 0)]), 0, 0.35)
    for _ in range(25):
        t = r.uniform(0, 0.35) ** 1.3
        nn = ns(0.012)
        add(out, sine(U(r, 3000, 8000), nn) * np.hanning(nn), t, 0.12 * (1 - t / 0.4))
    return reverb(out, r, 0.5, 0.2, bright=1.3)


@sound('ring_blast', 'construct', 4, volume=0.8, pitch=(0.93, 1.07), notes='always-available light bolt')
def ring_blast(r, v):
    d = 0.4
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, crack(r, 3000, 0.003), 0, 0.6)
    fc = glide(n, 1400 * U(r, .9, 1.1), 320, 0.06)
    zap = fm(fc, 0.5, 6 * np.exp(-t / 0.08) + 0.5, n) * env_exp(n, 0.09)
    add(out, zap, 0, 0.6)
    add(out, lp(supersaw(glide(n, 220, 110, 0.05), n, r, 3, 25), 3000) * env_exp(n, 0.07), 0, 0.3)
    add(out, thump(0.2, 110, 50, 0.025, 0.06, drive=1.8), 0, 0.7)
    for _ in range(30):
        tt = r.exponential(0.08)
        add(out, grain(r, 0.002, 3000, 9000), tt, 0.15 * np.exp(-tt / 0.1))
    return reverb(out, r, 0.4, 0.12, bright=1.2)


@sound('ring_blast_hit', 'construct', 4, volume=0.85, pitch=(0.93, 1.07))
def ring_blast_hit(r, v):
    d = 0.9
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, sat(crack(r, 2000, 0.005), 2.0), 0, 1.0)
    add(out, fm(glide(n, 900, 200, 0.03), 0.5, 8 * np.exp(-t / 0.03), n) * env_exp(n, 0.05), 0, 0.5)
    add(out, thump(0.4, 115, 45, 0.03, 0.12, drive=2.5), 0, 0.9)
    crackle = bp(white(n, r), 3000, 9000) * (white(n, r) > 0.93) * 3
    add(out, crackle * env_exp(n, 0.15), 0.01, 0.35)
    return reverb(out, r, 0.6, 0.18, bright=1.2)


@sound('beam_loop', 'construct', 2, loop=True, volume=0.7, pitch=(1.0, 1.0), lufs=-18, notes='loop; held beam')
def beam_loop(r, v):
    Ld = 2.0
    L, X = ns(Ld), ns(0.25)
    root = [110.0, 123.0][v % 2]  # every partial/LFO * Ld is an integer

    def tone(n):
        t = tvec(n)
        chord = (saw(root, n) + saw(root + 0.5, n) + saw(root * 1.5, n) + saw(root * 1.5 + 0.5, n)
                 + 0.7 * saw(root * 2, n))
        fc = 1400 * 2 ** (0.7 * np.sin(TAU * 4.0 * t))
        c = normpk(tvf(chord, fc, 3.0, 'lp'))
        whine = sine(root * 16 + 6 * np.sin(TAU * 5.0 * t), n)
        hum = sine(root / 2, n) * (1 + 0.2 * np.sin(TAU * 8.0 * t))
        return c + 0.08 * whine + 0.45 * hum
    tonal = periodic(tone, L)
    n = L + X
    crackle = bp(white(n, r), 2000, 8000) * (white(n, r) > 0.95) * 3 * (1 + 0.5 * smooth_noise(n, r, 10))
    hiss = normpk(hp(white(n, r), 5000))
    noise = loop_xfade(0.25 * crackle + 0.05 * hiss, L, X)
    return tonal + noise


@sound('shield_block', 'construct', 3, volume=0.9, pitch=(0.94, 1.06), lufs=-13, notes='energy clang')
def shield_block(r, v):
    out = buf(2.6)
    add(out, crack(r, 2000, 0.004), 0, 0.9)
    add(out, thump(0.4, 130, 60, 0.02, 0.1, drive=2.0), 0, 0.8)
    f1 = U(r, 520, 640)
    add(out, fm_bell(ns(2.6), f1, 1.414, 4, 0.15, 0.5), 0, 0.4)
    add(out, fm_bell(ns(2.6), f1 * 1.5 * 1.012, 2.01, 3, 0.1, 0.4), 0, 0.35)
    add(out, modal([f1 * 2.76, f1 * 5.4], [0.3, 0.15], [1, .6], ns(1.8), r), 0, 0.2)
    nn = ns(0.12)
    add(out, fm(glide(nn, 3000, 800, 0.015), 0.5, 5, nn) * env_exp(nn, 0.02), 0, 0.2)
    return reverb(out, r, 0.9, 0.25, bright=1.3)


def _shards(out, r, count, t_spread, fmin, fmax, amp):
    for _ in range(count):
        t = r.exponential(t_spread / 3)
        if t > t_spread:
            t = r.uniform(0, t_spread)
        f = U(r, fmin, fmax)
        dcy = U(r, 0.02, 0.15)
        s = modal([f, f * U(r, 2.3, 2.8)], [dcy, dcy * 0.6], [1, 0.4], ns(dcy * 8), r)
        add(out, s, t, amp * U(r, 0.3, 1.0) * (1 - 0.6 * t / t_spread))


def _cascade(out, r, n_cubes, t_first, t_max, fmin, fmax, amp, kind='tink'):
    for _ in range(n_cubes):
        t0 = t_first + min(r.exponential(0.18), t_max * 0.6)
        f = U(r, fmin, fmax)
        for (t, a) in bounces(r, t0, U(r, 0.4, 1.0), U(r, 0.07, 0.2), U(r, 0.5, 0.7), 0.58, t_max):
            ff = f * U(r, 0.97, 1.03)
            s = tink(r, ff, U(r, 0.6, 1.4)) if kind == 'tink' else tok(r, ff)
            add(out, s, t, amp * a)


@sound('construct_shatter', 'combat', 4, volume=1.0, pitch=(0.95, 1.05),
       notes='SIGNATURE: glassy crystalline burst + tinkling hard-light cube cascade ~1.2 s')
def construct_shatter(r, v):
    d = 1.8
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    # 1. crack
    add(out, sat(crack(r, 3000, 0.006, d=0.06), 2.5), 0, 1.0)
    # 2. glass burst: dense shard cluster + bright grainy noise
    _shards(out, r, 50, 0.08, 2200, 9500, 0.35)
    nb = ns(0.4)
    grit = 1 + 0.8 * smooth_noise(nb, r, 400)
    add(out, normpk(hp(white(nb, r), 4000)) * grit * env_exp(nb, 0.06), 0, 0.45)
    # 3. hard-light 'pwash': energy collapsing + weight
    f = glide(n, 330 * U(r, .9, 1.1), 85, 0.12)
    pw = tvf(supersaw(f, n, r, 5, 22), glide(n, 9000, 300, 0.15), 1.5, 'lp') * env_exp(n, 0.18, 0.002)
    add(out, normpk(pw), 0, 0.35)
    add(out, thump(0.5, 95, 40, 0.04, 0.15, drive=2.2), 0, 0.85)
    # 4. tinkling cube cascade
    _cascade(out, r, int(U(r, 14, 20)), 0.06, 1.3, 2500, 7500, 0.28)
    # 5. sparkle fizz
    fz = normpk(hp(white(n, r), 6000)) * (white(n, r) > 0.7) * env_exp(n, 0.4, 0.01)
    add(out, fz, 0, 0.1)
    return reverb(out, r, 1.0, 0.22, bright=1.6)


@sound('clash', 'combat', 4, volume=1.0, pitch=(0.94, 1.06),
       notes='two hard-light constructs colliding: energy crack + low boom')
def clash(r, v):
    d = 2.4
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, sat(crack(r, 1500, 0.006, d=0.06), 3.0), 0, 1.0)
    nz = ns(0.15)
    add(out, fm(glide(nz, 2500, 400, 0.04), 0.5, 10 * np.exp(-tvec(nz) / 0.04), nz) * env_exp(nz, 0.04), 0, 0.5)
    a = U(r, 400, 520)
    add(out, fm_bell(n, a, 1.414, 5, 0.1, 0.45), 0, 0.35)
    add(out, fm_bell(n, a * 1.059, 1.414, 5, 0.1, 0.45), 0.003, 0.35)
    add(out, thump(1.2, 72, 28, 0.08, 0.35, drive=3.0), 0, 1.1)
    crackle = bp(white(n, r), 2500, 9000) * (white(n, r) > 0.94) * 3
    add(out, crackle * env_exp(n, 0.25, 0.01), 0.01, 0.3)
    _cascade(out, r, 5, 0.1, 0.8, 3000, 7000, 0.12)
    return reverb(out, r, 1.4, 0.28, bright=1.3)


# =============================================================================== hits
@sound('hit_light', 'combat', 4, volume=0.8, pitch=(0.9, 1.1), lufs=-14)
def hit_light(r, v):
    out = buf(0.3)
    add(out, crack(r, 3000, 0.002), 0, 0.6)
    add(out, burst(0.08, r, 800, 3500 * U(r, .8, 1.2), 0.012), 0, 0.7)
    add(out, thump(0.15, 190 * U(r, .9, 1.1), 95, 0.02, 0.05, drive=1.5), 0, 0.8)
    return reverb(out, r, 0.3, 0.08)


@sound('hit_medium', 'combat', 4, volume=0.9, pitch=(0.92, 1.08), lufs=-13)
def hit_medium(r, v):
    out = buf(0.55)
    add(out, crack(r, 2500, 0.003), 0, 0.8)
    add(out, burst(0.12, r, 500, 3000, 0.02), 0, 0.8)
    add(out, thump(0.4, 150 * U(r, .9, 1.1), 58, 0.03, 0.1, drive=2.2), 0, 1.0)
    add(out, burst(0.2, r, None, 300, 0.05, color='brown'), 0, 0.4)
    return reverb(out, r, 0.5, 0.12)


@sound('hit_heavy', 'combat', 4, volume=1.0, pitch=(0.93, 1.05),
       notes='big bass-drop punch; pair with hit-stop')
def hit_heavy(r, v):
    d = 2.4
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, sat(crack(r, 2000, 0.006, d=0.05), 2.0), 0, 1.0)
    nc = ns(0.05)
    add(out, fm(3200, 1.41, 5 * np.exp(-tvec(nc) / 0.006), nc) * env_exp(nc, 0.01), 0, 0.3)
    add(out, burst(0.2, r, 120, 900, 0.045, color='pink'), 0, 0.9)
    add(out, thump(1.2, 170 * U(r, .92, 1.08), 32, 0.07, 0.38, drive=3.0, attack=0.002), 0, 1.2)
    add(out, thump(1.3, 48, 30, 0.3, 0.5, drive=1.2, attack=0.02), 0.02, 0.5)
    for _ in range(8):
        add(out, grain(r, U(r, .003, .008), 1000, 4000), r.uniform(0, 0.03), 0.3)
    out = sat(out * 1.2, 1.5)
    return reverb(out, r, 1.3, 0.22, bright=0.6)


@sound('wall_splat', 'combat', 3, volume=1.0, pitch=(0.93, 1.06), notes='body slammed into stone')
def wall_splat(r, v):
    out = buf(1.2)
    add(out, thump(0.8, 110, 42, 0.05, 0.2, drive=2.5), 0, 1.0)
    for i in range(3):
        add(out, burst(0.05, r, 400, 2500, 0.015), i * U(r, .004, .007), 0.7 / (1 + i))
    for _ in range(20):
        tt = r.exponential(0.04)
        f = U(r, 900, 1800)
        g = mix(2 * reson(grain(r, U(r, .003, .01), 1200, 5000), f, 4),
                modal([f], [U(r, .01, .03)], [0.5], ns(0.25), r))
        add(out, g, tt, 0.35 * np.exp(-tt / 0.12))
    _cascade(out, r, 5, 0.1, 0.9, 700, 2000, 0.12, kind='tok')
    return reverb(out, r, 0.7, 0.2)


@sound('kaiju_stomp', 'combat', 3, volume=1.0, pitch=(0.9, 1.05), lufs=-11, notes='sub-bass boom + rumble')
def kaiju_stomp(r, v):
    d = 4.5
    n = ns(d)
    out = buf(d)
    add(out, burst(0.1, r, None, 500, 0.02), 0, 0.8)
    add(out, thump(2.5, 55 * U(r, .92, 1.08), 24, 0.15, 0.7, drive=2.5, attack=0.004), 0, 1.2)
    rum = normpk(lp(brown(n, r), 140)) * (1 + 0.4 * smooth_noise(n, r, 4)) * env_exp(n, 1.0, 0.05)
    add(out, rum, 0, 0.6)
    for _ in range(10):
        tt = U(r, 0.02, 0.2)
        add(out, grain(r, U(r, .004, .012), 900, 4000), tt, 0.25)
    _cascade(out, r, 8, 0.1, 1.8, 500, 1600, 0.1, kind='tok')
    return reverb(out, r, 2.0, 0.25, bright=0.6)


@sound('kaiju_roar', 'combat', 3, volume=1.0, pitch=(0.92, 1.05), lufs=-12,
       notes='synth monster roar ~1.6 s (detuned saws + noise through moving formants)')
def kaiju_roar(r, v):
    d = 2.0
    n = ns(d)
    t = tvec(n)
    k = U(r, 0.9, 1.1)
    f0 = env_pts(n, [(0, 55), (0.25, 85), (0.9, 78), (1.4, 60), (1.9, 42)]) * k
    f0 = f0 * (1 + 0.03 * smooth_noise(n, r, 12) + 0.02 * np.sin(TAU * 5 * t))
    growl = 1 + 0.55 * np.sin(TAU * np.cumsum(28 + 8 * smooth_noise(n, r, 2)) / SR)
    src = supersaw(f0, n, r, 5, 25) + 0.5 * square(f0 / 2, n)
    breath = pink(n, r) * 2.2
    exc = (src + breath) * growl
    F1 = env_pts(n, [(0, 550), (0.3, 750), (1.2, 700), (1.9, 450)])
    F2 = env_pts(n, [(0, 950), (0.3, 1250), (1.2, 1150), (1.9, 850)])
    body = (tvf(exc, F1, 5, 'bp') + 0.7 * tvf(exc, F2, 6, 'bp') + 0.4 * reson(exc, 2500, 8)
            + 0.5 * lp(src, 300))
    scr_f = f0 * 6 * (1 + 0.04 * smooth_noise(n, r, 20))
    screech = reson(saw(scr_f, n) + 0.5 * white(n, r), 2400 * U(r, .9, 1.1), 3)
    screech *= env_pts(n, [(0, 0), (0.2, 0), (0.5, 1), (1.1, 0.8), (1.6, 0)])
    x = normpk(body) + 0.45 * normpk(screech)
    x = sat(x * 1.5, 3.0)
    x *= env_pts(n, [(0, 0), (0.15, 0.8), (0.35, 1), (1.2, 0.85), (1.6, 0.3), (1.95, 0)])
    return reverb(x, r, 1.8, 0.3, bright=0.8)


@sound('explosion_small', 'combat', 3, volume=0.95, pitch=(0.92, 1.08))
def explosion_small(r, v):
    d = 1.5
    n = ns(d)
    out = buf(d)
    add(out, sat(crack(r, 1500, 0.006, d=0.05), 2.0), 0, 1.0)
    rough = 1 + 0.5 * smooth_noise(n, r, 60)
    body = tvf(white(n, r), glide(n, 9000, 400, 0.12), 0.8, 'lp') * rough * env_exp(n, 0.18, 0.002)
    add(out, normpk(body), 0, 1.0)
    add(out, thump(1.0, 80, 35, 0.05, 0.25, drive=2.5), 0, 1.0)
    for _ in range(30):
        tt = U(r, 0.05, 0.6)
        add(out, grain(r, 0.002, 2000, 8000), tt, 0.25 * np.exp(-tt / 0.25))
    return reverb(out, r, 1.0, 0.22, bright=0.8)


@sound('explosion_big', 'combat', 3, volume=1.0, pitch=(0.92, 1.05), lufs=-11, notes='with debris rumble tail')
def explosion_big(r, v):
    d = 4.0
    n = ns(d)
    out = buf(d)
    add(out, sat(crack(r, 1200, 0.008, d=0.08), 2.5), 0, 1.0)
    add(out, sat(crack(r, 2000, 0.006, d=0.06), 2.0), U(r, 0.012, 0.025), 0.7)
    rough = 1 + 0.6 * smooth_noise(n, r, 50)
    body = tvf(white(n, r), glide(n, 12000, 250, 0.3), 0.8, 'lp') * rough * env_exp(n, 0.5, 0.003)
    add(out, normpk(body), 0, 1.1)
    add(out, thump(3.0, 60, 25, 0.15, 0.8, drive=3.0, attack=0.003), 0, 1.3)
    rum = normpk(lp(brown(n, r), 120)) * (1 + 0.5 * smooth_noise(n, r, 3)) * env_pts(n, [(0, 0), (0.1, 1), (4, 0)], 2)
    add(out, rum, 0, 0.7)
    _cascade(out, r, 25, 0.25, 3.2, 500, 2200, 0.12, kind='tok')
    for _ in range(4):
        add(out, thump(0.3, 130, 60, 0.02, 0.06, drive=1.5), U(r, 0.4, 1.6), 0.25)
    for _ in range(40):
        tt = U(r, 0.05, 1.5)
        add(out, grain(r, 0.002, 2000, 8000), tt, 0.2 * np.exp(-tt / 0.6))
    return reverb(out, r, 2.5, 0.3, bright=0.6)


# =============================================================================== destruction
@sound('break_stone', 'destruction', 4, volume=0.85, pitch=(0.9, 1.1))
def break_stone(r, v):
    out = buf(1.0)
    add(out, crack(r, 2000, 0.004), 0, 0.7)
    add(out, thump(0.25, 140, 70, 0.02, 0.07, drive=1.8), 0, 0.7)
    for i in range(28):
        tt = r.exponential(0.03) if i < 20 else U(r, 0.05, 0.3)
        f = U(r, 900, 4000)
        g = reson(grain(r, U(r, .003, .008), 700, 6000), f, 6) * 2
        g = mix(g, modal([f * 0.7], [U(r, .008, .02)], [0.6], ns(0.08), r))
        add(out, g, tt, 0.5 * np.exp(-tt / 0.15))
    n = ns(0.9)
    cr = lp(pink(n, r), 2000) * (white(n, r) > 0.6) * env_exp(n, 0.15, 0.01)
    add(out, normpk(cr), 0.02, 0.2)
    return reverb(out, r, 0.5, 0.14)


@sound('break_dirt', 'destruction', 4, volume=0.75, pitch=(0.9, 1.1))
def break_dirt(r, v):
    out = buf(0.6)
    add(out, thump(0.3, 110, 55, 0.03, 0.08, drive=1.5), 0, 0.8)
    add(out, burst(0.25, r, None, 700, 0.06, color='brown'), 0, 0.8)
    for _ in range(30):
        tt = r.exponential(0.1)
        add(out, lp(grain(r, U(r, .004, .012), 300, 3000), 1500), tt, 0.3 * np.exp(-tt / 0.15))
    for _ in range(10):
        tt = r.exponential(0.06)
        add(out, grain(r, 0.003, 2000, 4000), tt, 0.08)
    return reverb(out, r, 0.35, 0.08)


@sound('break_wood', 'destruction', 4, volume=0.85, pitch=(0.9, 1.1), notes='crack/splinter')
def break_wood(r, v):
    out = buf(0.8)
    add(out, sat(crack(r, 1500, 0.003), 2.0), 0, 1.0)
    add(out, crack(r, 2000, 0.002), U(r, 0.02, 0.04), 0.6)
    f0 = U(r, 180, 260)
    add(out, modal([f0, f0 * 2.3, f0 * 3.9, f0 * 5.6], [0.12, 0.07, 0.04, 0.03], [1, .6, .35, .2], ns(0.6), r), 0, 0.7)
    add(out, thump(0.1, 200, 120, 0.01, 0.03, drive=1.5), 0, 0.5)
    for _ in range(45):
        tt = 0.02 + r.gamma(2.0, 0.04)
        n = ns(U(r, 0.004, 0.012))
        c = bp(white(n, r), 2000, 7000) * env_exp(n, U(r, 0.001, 0.003))
        add(out, c, tt, U(r, .1, .4) * np.exp(-tt / 0.2))
    return reverb(out, r, 0.45, 0.12)


@sound('break_leaves', 'destruction', 4, volume=0.6, pitch=(0.9, 1.12), lufs=-17, notes='rustle burst')
def break_leaves(r, v):
    out = buf(0.8)
    for _ in range(260):
        tt = r.exponential(0.15)
        if tt > 0.7:
            continue
        lo = U(r, 1800, 5000)
        add(out, grain(r, U(r, .004, .025), lo, lo * 2), tt, U(r, .2, 1.0) * np.exp(-tt / 0.2))
    add(out, whoosh(0.4, r, 1000, 4000, 0.8, [(0, 0), (0.02, 1), (0.4, 0)]), 0, 0.3)
    for _ in range(3):
        add(out, crack(r, 2000, 0.002), U(r, 0, 0.1), 0.4)
    add(out, thump(0.1, 90, 70, 0.02, 0.03, drive=0), 0, 0.2)
    return out


@sound('break_crystal', 'destruction', 3, volume=0.85, pitch=(0.95, 1.05), notes='chime shatter')
def break_crystal(r, v):
    d = 2.4
    out = buf(d)
    add(out, crack(r, 3000, 0.004), 0, 0.8)
    scale = [1760.0, 1975.5, 2217.5, 2637.0, 2960.0, 3520.0]  # A6 pentatonic
    notes = r.choice(scale, 5, replace=False)
    for i, f in enumerate(notes):
        ratios = [1, 2.0, 3.0, 4.16, 5.43]
        decs = np.array([0.9, 0.6, 0.4, 0.25, 0.15]) * U(r, 0.7, 1.1)
        add(out, modal([f * k for k in ratios], decs, [1, .5, .35, .2, .1], ns(2.3), r),
            i * U(r, 0.012, 0.035), 0.3)
    _shards(out, r, 25, 0.06, 3000, 9000, 0.3)
    _cascade(out, r, 6, 0.08, 1.2, 3500, 7500, 0.18)
    return reverb(out, r, 1.4, 0.3, bright=1.6)


@sound('debris_patter', 'destruction', 4, volume=0.6, pitch=(0.88, 1.12), lufs=-19, notes='small cubes bouncing')
def debris_patter(r, v):
    out = buf(1.2)
    for _ in range(int(U(r, 4, 8))):
        t0 = U(r, 0, 0.25)
        f = U(r, 700, 2200)
        for (t, a) in bounces(r, t0, U(r, 0.4, 1.0), U(r, 0.08, 0.18), U(r, 0.55, 0.7), 0.6, 1.1):
            add(out, tok(r, f * U(r, 0.96, 1.04)), t, a)
    return reverb(out, r, 0.3, 0.1)


@sound('collapse_rumble', 'destruction', 2, volume=1.0, pitch=(0.92, 1.05), lufs=-13, notes='a tower falling')
def collapse_rumble(r, v):
    d = 5.0
    n = ns(d)
    out = buf(d)
    add(out, burst(0.2, r, None, 3000, 0.02), 0, 0.7)
    add(out, burst(0.2, r, None, 2500, 0.02), 0.15, 0.6)
    for _ in range(15):
        add(out, grain(r, U(r, .004, .01), 900, 4000), r.exponential(0.08), 0.3)
    groan = tvf(brown(n, r) * 3, sweep(n, 180, 80), 6, 'bp')
    add(out, normpk(groan) * env_pts(n, [(0, 0), (0.3, 0.8), (1.5, 0.5), (2.5, 0)]), 0, 0.45)
    rum = normpk(lp(brown(n, r), 150)) * (1 + 0.5 * smooth_noise(n, r, 2))
    rum *= env_pts(n, [(0, 0), (0.4, 1), (2.8, 1), (4.8, 0)])
    add(out, rum, 0, 1.0)
    for _ in range(5):
        tt = U(r, 0.7, 2.8)
        add(out, thump(0.8, 90, 35, 0.05, 0.3, drive=2.0), tt, U(r, 0.6, 1.0))
        for _ in range(8):
            add(out, grain(r, U(r, .004, .01), 900, 4000), tt + r.exponential(0.03), 0.2)
    _cascade(out, r, 50, 0.3, 4.2, 500, 2000, 0.1, kind='tok')
    return reverb(out, r, 2.5, 0.3, bright=0.6)


# =============================================================================== status / UI
@sound('ui_move', 'ui', 3, volume=0.4, pitch=(1.0, 1.0), lufs=-24, notes='menu hover tick')
def ui_move(r, v):
    n = ns(0.08)
    x = sine(2200 * (1 + 0.03 * v), n) * env_exp(n, 0.012, 0.0005)
    return x + 0.3 * crack(r, 4000, 0.0008, d=0.08)


@sound('ui_select', 'ui', 3, volume=0.55, pitch=(1.0, 1.0))
def ui_select(r, v):
    k = 1 + 0.015 * (v - 1)
    out = buf(0.8)
    add(out, fm_bell(ns(0.6), 880 * k, 2, 1.5, 0.05, 0.08), 0, 0.8)
    add(out, fm_bell(ns(0.7), 1318.5 * k, 2, 1.5, 0.05, 0.11), 0.06, 1.0)
    add(out, thump(0.05, 200, 120, 0.01, 0.012, drive=0), 0, 0.2)
    return reverb(out, r, 0.3, 0.15, bright=1.4)


@sound('ui_back', 'ui', 3, volume=0.5, pitch=(1.0, 1.0), lufs=-22)
def ui_back(r, v):
    k = 1 + 0.015 * (v - 1)
    out = buf(0.7)
    add(out, fm_bell(ns(0.5), 1318.5 * k, 2, 1.0, 0.04, 0.07), 0, 0.8)
    add(out, fm_bell(ns(0.6), 880 * k, 2, 1.0, 0.04, 0.09), 0.06, 0.9)
    out = lp(out, 3500)
    return reverb(out, r, 0.3, 0.12)


@sound('type_key', 'ui', 8, volume=0.35, pitch=(0.94, 1.06), lufs=-26, notes='ring-console keypress; many variations')
def type_key(r, v):
    n = ns(0.05)
    x = crack(r, 3000, 0.0008, d=0.05)
    f = U(r, 2500, 5000)
    x += 0.5 * modal([f, f * 1.5], [0.006, 0.004], [1, .5], n, r)
    x += 0.3 * burst(0.05, r, 800, 1600, 0.004)
    return x


@sound('type_error', 'ui', 2, volume=0.5, pitch=(1.0, 1.0))
def type_error(r, v):
    out = buf(0.28)
    for i in range(2):
        n = ns(0.075)
        s = lp(square(150, n) + square(158, n), 1800) * env_pts(n, [(0, 0), (0.004, 1), (0.06, 0.8), (0.072, 0)])
        add(out, s, i * 0.11 + 0.01 * v, 1.0)
    return out


@sound('cast_confirm', 'ui', 3, volume=0.75, pitch=(0.98, 1.02), lufs=-17, notes='ring console Enter: construct cast')
def cast_confirm(r, v):
    out = buf(1.2)
    n = ns(0.16)
    add(out, sine(sweep(n, 600, 2000, 0.7), n) * env_pts(n, [(0, 0), (0.01, 1), (0.1, 0.5), (0.16, 0)]), 0, 0.5)
    base = [1760.0, 1975.5, 2093.0][v % 3]
    add(out, fm_bell(ns(1.1), base, 1.0, 1.2, 0.1, 0.25), 0.1, 0.5)
    add(out, fm_bell(ns(1.1), base * 1.5, 1.0, 1.2, 0.1, 0.22), 0.1, 0.4)
    add(out, thump(0.15, 120, 60, 0.02, 0.04, drive=1.2), 0.1, 0.4)
    for _ in range(12):
        nn = ns(0.01)
        add(out, sine(U(r, 4000, 9000), nn) * np.hanning(nn), U(r, 0.1, 0.4), 0.08)
    return reverb(out, r, 0.6, 0.25, bright=1.5)


def _chord_pad(n, r, freqs, fc0, fc1, env):
    x = sum(supersaw(f, n, r, 3, 12) for f in freqs)
    return normpk(tvf(x, sweep(n, fc0, fc1, 0.5), 1.2, 'lp')) * env


@sound('oath_complete', 'ui', 2, volume=0.8, pitch=(1.0, 1.0), lufs=-16, notes='triumphant rising chord')
def oath_complete(r, v):
    d = 4.2
    out = buf(d)
    root = [261.63, 293.66][v % 2]  # C4 / D4
    arp = [1, 1.25992, 1.49831, 2.0]
    for i, k in enumerate(arp):
        add(out, fm_bell(ns(2.2), root * 2 * k, 1.0, 1.5, 0.15, 0.35), i * 0.085, 0.5)
    n = ns(d - 0.34)
    env = env_pts(n, [(0, 0), (0.12, 1), (0.8, 0.8), (2.2, 0)])
    add(out, _chord_pad(n, r, [root / 2, root * 0.749, root, root * 1.26, root * 1.5], 600, 5500, env), 0.34, 0.7)
    add(out, sine(root / 4, n) * env, 0.34, 0.4)
    add(out, fm_bell(ns(4.0), root * 4, 1.0, 2.0, 0.3, 0.8), 0.34, 0.35)
    add(out, thump(0.3, 110, 55, 0.03, 0.1, drive=1.5), 0.34, 0.5)
    for _ in range(30):
        nn = ns(0.012)
        add(out, sine(U(r, 3500, 9000), nn) * np.hanning(nn), U(r, 0.34, 1.6), 0.06)
    return reverb(out, r, 1.8, 0.35, bright=1.4)


@sound('willpower_low', 'ui', 2, volume=0.45, pitch=(1.0, 1.0), lufs=-22, notes='subtle low heartbeat pulse')
def willpower_low(r, v):
    out = buf(0.8)
    for i, (tt, a) in enumerate([(0, 1.0), (0.18, 0.7)]):
        n = ns(0.3)
        p = (sine(glide(n, 80, 62, 0.05), n) + 0.3 * sine(140, n)) * env_exp(n, 0.07, 0.01)
        add(out, lp(p, 400), tt, a)
    n = ns(0.6)
    fl = sine(880 * (1 + 0.02 * v), n) * (0.5 + 0.5 * np.sin(TAU * 14 * tvec(n))) * env_pts(n, [(0, 0), (0.05, 1), (0.6, 0)])
    add(out, fl, 0, 0.03)
    return out


@sound('ko', 'ui', 2, volume=1.0, pitch=(1.0, 1.0), lufs=-12, notes='dramatic KO sting')
def ko(r, v):
    d = 4.5
    n = ns(d)
    out = buf(d)
    add(out, sat(crack(r, 1500, 0.006, d=0.06), 2.5), 0, 1.0)
    add(out, thump(2.5, 62, 27, 0.1, 0.8, drive=3.0, attack=0.002), 0, 1.2)
    root = [110.0, 103.83][v % 2]
    env = env_pts(n, [(0, 0), (0.005, 1), (0.4, 0.6), (2.5, 0)])
    stab = sum(supersaw(root * k, n, r, 3, 15) for k in (1, 1.189, 1.414, 2.0, 2.378))
    stab = tvf(stab, glide(n, 5000, 500, 0.4), 1.5, 'lp') * env
    add(out, normpk(stab), 0, 0.6)
    add(out, fm_bell(ns(4.2), root * 8, 1.414, 4, 0.3, 0.9), 0, 0.2)
    add(out, fm_bell(ns(4.2), root * 8 * 1.189, 1.414, 4, 0.3, 0.9), 0.01, 0.2)
    _shards(out, r, 20, 0.08, 3000, 9000, 0.2)
    _cascade(out, r, 8, 0.1, 1.5, 2500, 6000, 0.12)
    return reverb(out, r, 2.5, 0.35, bright=1.0)


@sound('respawn', 'ui', 2, volume=0.8, pitch=(1.0, 1.0), lufs=-16, notes='rising energy swell + arrival')
def respawn(r, v):
    d = 2.2
    out = buf(d)
    n = ns(0.65)
    rise = env_pts(n, [(0, 0), (0.6, 1), (0.645, 0)], 2.0)
    add(out, normpk(tvf(white(n, r), sweep(n, 400, 6000), 2.0, 'bp')) * rise, 0, 0.5)
    f = sweep(n, 110, 440, 1.5)
    add(out, normpk(tvf(supersaw(f, n, r, 5, 18), sweep(n, 300, 5000), 2.0, 'lp')) * rise, 0, 0.5)
    root = [880.0, 987.8][v % 2]
    add(out, fm_bell(ns(1.5), root, 1.0, 1.5, 0.1, 0.3), 0.6, 0.6)
    add(out, fm_bell(ns(1.5), root * 1.5, 1.0, 1.5, 0.1, 0.3), 0.6, 0.45)
    add(out, thump(0.3, 110, 55, 0.03, 0.08, drive=1.5), 0.6, 0.7)
    for _ in range(20):
        nn = ns(0.01)
        add(out, sine(U(r, 3500, 9000), nn) * np.hanning(nn), U(r, 0.6, 1.1), 0.08)
    return reverb(out, r, 1.2, 0.3, bright=1.4)


@sound('match_start', 'ui', 2, volume=0.9, pitch=(1.0, 1.0), lufs=-13, notes='fight-start sting')
def match_start(r, v):
    d = 3.8
    out = buf(d)
    for tt in (0.0, 0.22):
        add(out, thump(0.4, 120, 50, 0.03, 0.1, drive=2.0), tt, 0.9)
        add(out, crack(r, 2000, 0.004), tt, 0.5)
    root = [146.83, 164.81][v % 2]
    n = ns(d - 0.45)
    env = env_pts(n, [(0, 0), (0.01, 1), (0.5, 0.7), (2.0, 0)])
    add(out, _chord_pad(n, r, [root, root * 1.5, root * 2, root * 2.52, root * 3], 5000, 800, env), 0.45, 0.8)
    add(out, thump(1.5, 70, 30, 0.08, 0.4, drive=2.5), 0.45, 1.0)
    add(out, sat(crack(r, 1500, 0.006, d=0.05), 2.0), 0.45, 0.8)
    add(out, fm_bell(ns(3.2), root * 8, 1.0, 2, 0.2, 0.6), 0.45, 0.3)
    return reverb(out, r, 1.8, 0.3, bright=1.2)


@sound('match_end', 'ui', 2, volume=0.9, pitch=(1.0, 1.0), lufs=-13, notes='match-over sting (IV -> I resolve)')
def match_end(r, v):
    d = 5.0
    out = buf(d)
    root = [146.83, 130.81][v % 2]
    add(out, thump(2.0, 70, 28, 0.1, 0.6, drive=2.5), 0, 1.1)
    add(out, sat(crack(r, 1500, 0.006, d=0.05), 2.0), 0, 0.8)
    n1 = ns(0.9)
    e1 = env_pts(n1, [(0, 0), (0.01, 1), (0.7, 0.6), (0.9, 0)])
    iv = root * 4 / 3
    add(out, _chord_pad(n1, r, [iv, iv * 1.26, iv * 1.5, iv * 2], 4000, 1500, e1), 0, 0.7)
    n2 = ns(d - 0.65)
    e2 = env_pts(n2, [(0, 0), (0.05, 1), (1.0, 0.7), (2.7, 0)])
    add(out, _chord_pad(n2, r, [root / 2, root, root * 1.26, root * 1.5, root * 2], 3000, 500, e2), 0.65, 0.8)
    add(out, sine(root / 2, n2) * e2, 0.65, 0.4)
    add(out, fm_bell(ns(4.2), root * 8, 1.0, 2, 0.3, 0.9), 0.65, 0.3)
    add(out, fm_bell(ns(4.2), root * 12, 1.0, 2, 0.3, 0.9), 0.7, 0.2)
    return reverb(out, r, 2.2, 0.35, bright=1.1)
