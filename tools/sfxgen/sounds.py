"""Sound definitions for Brightest Day.

Each @sound function takes (rng, v) - a seeded numpy Generator and the variation index - and
returns a mono float signal, or an (n, 2) array for `stereo=True` sounds (any scale; the pipeline
normalises). Loop sounds must return an exactly-seamless loop (see dsp.loop_xfade / dsp.periodic).

Design rules (see README):
  * DRY - no baked room reverb; the game's mixer supplies one shared space. Natural physical decay
    (a plate ringing, a rumble, a debris cascade) is part of the source and stays.
  * No broadband white noise: modal synthesis, pitch-dropping thumps, FM, tuned resonators and
    granular clusters of tiny modal ticks. Noise only as a <= 15 ms transient, as a shaped,
    subordinate texture, or where the source really is noise (gas, wind, surf).
  * Frequency carving by role: heavy = sub/low + transient with a 2-5 kHz dip; foley = mids with a
    steep high-pass; UI/feedback = small and bright (5-12 kHz).
  * The loudness hierarchy is baked into the files (`lufs` per sound); manifest `volume` is 1.0.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Callable

import numpy as np

from dsp import *  # noqa: F401,F403  (tiny DSP vocabulary)
from dsp import SR, TAU
from materials import (click, grit, hl_chord, hl_ping, hl_shards, hl_tone, material_hit, micro, note, pent,
                       pent_index, shatter)

# Pitch-variation presets (playback-speed ranges). Micro variation only: the files carry the variety.
P_IMPACT = (0.97, 1.03)     # footsteps / impacts (+-51 cents)
P_MICRO = (0.975, 1.025)    # most one-shots (+-43 cents)
P_SOFT = (0.985, 1.015)     # big / slow sounds (+-26 cents)
P_T10 = (0.994, 1.006)      # tonal sounds that must stay in key (+-10 cents)
P_TONAL = (1.0, 1.0)

# Group labels for README / manifest header only; every sound sets its own `lufs`.
CATEGORY_LUFS = {
    'combat': 'explosions -9..-12, hits -12/-15/-18',
    'weapon': 'guns -11..-13, mechanics -20..-24, casings -32',
    'construct': 'hard light -14..-23',
    'destruction': 'breaks -18/-22, impacts -20, debris -28',
    'movement': 'foley -20..-30',
    'tether': '-22..-28',
    'feedback': 'bright, -18..-30',
    'ui': 'bright, -15..-32',
    'ko_streak': '-17 (+0.5 dB/step), accents -16, shutdown -15',
    'ambience': 'beds -30..-40, positional -26..-32',
}


@dataclass
class Sound:
    name: str
    fn: Callable
    category: str
    variations: int = 3
    loop: bool = False
    pitch: tuple = P_MICRO
    lufs: float = -20.0
    notes: str = ''
    sequence: bool = False      # files are ordered steps (play file[i]), not random variations
    lufs_ramp: float = 0.0      # per-variation/step loudness increment (dB)
    max_dur: float | None = None  # hard cap on the trimmed length (smooth fade)
    stereo: bool = False
    fade_out: float = 0.02      # end fade applied by the mastering trim (s)
    volume: float = 1.0         # always 1.0: the hierarchy is in the files, the mixer decides the rest
    files: list = field(default_factory=list)

    @property
    def target(self) -> float:
        return self.lufs


REGISTRY: dict[str, Sound] = {}


def sound(name, category, variations=3, loop=False, pitch=P_MICRO, lufs=-20.0, notes='', sequence=False,
          lufs_ramp=0.0, max_dur=None, stereo=False, fade_out=0.02):
    def deco(fn):
        REGISTRY[name] = Sound(name, fn, category, variations, loop, pitch, float(lufs), notes, sequence,
                               lufs_ramp, max_dur, stereo, fade_out)
        return fn
    return deco


def U(rng, a, b):
    return rng.uniform(a, b)


def _pulse_train(n, rate):
    cyc = np.cumsum(np.broadcast_to(rate, (n,))) / SR
    tr = np.zeros(n)
    tr[1:][np.diff(np.floor(cyc)) > 0] = 1.0
    return tr


def swish(r, d, f0, f1, q=1.6, grain_hz=90.0, pts=None):
    """Shaped air/cloth movement: pink noise through a moving resonant band with amplitude grain.
    Always used as a subordinate layer."""
    n = ns(d)
    y = normpk(tvf(pink(n, r), sweep(n, f0, f1), q, 'bp'))
    y *= 1 + 0.45 * smooth_noise(n, r, grain_hz)
    return y * env_pts(n, pts or [(0, 0), (d * 0.3, 1), (d, 0)])


def string_twang(r, f, dec=0.25, n_part=6, stretch=0.0015, vib=0.0):
    """Taut cable / spring: near-harmonic partials (slightly stretched), upper ones decay faster."""
    n = ns(dec * 6)
    t = tvec(n)
    x = np.zeros(n)
    for k in range(1, n_part + 1):
        fk = f * k * (1 + stretch * k * k)
        if fk > 16000:
            break
        ph = TAU * fk * t + (vib * np.sin(TAU * 6.0 * t) * k if vib else 0)
        x += (1.0 / k ** 0.8) * np.exp(-t * (1 + 0.6 * k) / dec) * np.sin(ph + r.uniform(0, TAU))
    return x * np.clip(t / 0.0004, 0, 1)


# =============================================================================== movement (foley: mids, steep HPF)
def _sole(r, size, energy, material='stone'):
    return material_hit(r, material, size, energy)


@sound('jump', 'movement', 4, pitch=P_MICRO, lufs=-26, notes='push-off: boot sole + kit rattle + short cloth swish')
def jump(r, v):
    out = buf(0.35)
    add(out, _sole(r, U(r, 0.55, 0.8), U(r, 0.7, 1.0)), 0, 0.9)
    if v == 1:  # double push (both feet)
        add(out, _sole(r, U(r, 0.5, 0.7), 0.6), U(r, 0.012, 0.022), 0.6)
    for _ in range(int(r.integers(2, 5)) if v != 2 else 1):  # buckles / kit
        add(out, micro(r, U(r, 2300, 3600), 0.01, 3, (1.4, 2.7)), U(r, 0.01, 0.07), U(r, 0.12, 0.3))
    top = [2600, 2400, 3000, 3300][v]
    add(out, swish(r, 0.14, 900, top, 1.8, 110, [(0, 0), (0.03, 1), (0.14, 0)]), 0.006, 0.16)
    if v == 3:  # lighter hop: tiny rising 'lift' chirp
        n = ns(0.07)
        add(out, sine(sweep(n, 900, 1600, 0.7), n) * env_pts(n, [(0, 0), (0.01, 1), (0.07, 0)]), 0.004, 0.06)
    return carve_foley(out, 220, 9000)


@sound('land_light', 'movement', 5, pitch=P_IMPACT, lufs=-27, notes='heel+toe landing, grit patter, small body knock')
def land_light(r, v):
    out = buf(0.35)
    gap = [0.012, 0.02, 0.008, 0.026, 0.016][v]
    add(out, _sole(r, U(r, 0.8, 1.1), U(r, 0.9, 1.2)), 0, 1.0)
    add(out, _sole(r, U(r, 0.6, 0.85), U(r, 0.6, 0.9)), gap, 0.7)
    f = U(r, 230, 330)
    add(out, modal([f, f * 1.7], [0.03, 0.018], [1, .5], ns(0.2), r), 0, 0.5)  # body weight, low-mid
    add(out, grit(r, [8, 14, 6, 10, 18][v], 0.02, 1400, 4200, 0.003), 0.004, 0.35)
    if v in (1, 4):
        add(out, micro(r, U(r, 2400, 3400), 0.012, 3, (1.4, 2.6)), U(r, 0.015, 0.04), 0.18)
    return carve_foley(out, 180, 9000)


@sound('land_heavy', 'movement', 4, pitch=P_MICRO, lufs=-20,
       notes='high-speed / long-fall landing: sub thud + stone crunch + a little gravel')
def land_heavy(r, v):
    out = buf(0.9)
    add(out, click(r, 1500, 0.003), 0, 0.5)
    add(out, thump(0.6, 100 * U(r, .9, 1.1), 36, 0.05, 0.14, drive=2.6), 0, 1.0)
    add(out, material_hit(r, 'stone', [1.8, 2.2, 1.6, 2.5][v], 1.5), 0, 0.7)
    add(out, shatter(r, 'stone', [5, 8, 4, 10][v], 0.25, 0.5, 0.8), 0.015, 0.3)
    return carve_heavy(out)


@sound('footstep', 'movement', 8, pitch=P_IMPACT, lufs=-30, notes='short heel-toe on voxel ground (mids only)')
def footstep(r, v):
    out = buf(0.2)
    heel_size = [0.7, 0.8, 0.65, 0.9, 0.75, 0.6, 0.85, 0.7][v]
    add(out, _sole(r, heel_size, U(r, 0.6, 0.85)), 0, 1.0)
    gap = [0.045, 0.06, 0.035, 0.07, 0.05, 0.04, 0.065, 0.055][v]
    add(out, _sole(r, heel_size * U(r, 0.6, 0.8), U(r, 0.4, 0.6)), gap, U(r, 0.4, 0.6))
    if v % 3 == 1:  # boot clink
        add(out, micro(r, U(r, 2600, 3400), 0.008, 3, (1.4, 2.4)), U(r, 0.002, 0.01), 0.15)
    if v % 4 == 3:  # scuff: short grained swish in the mids
        add(out, swish(r, 0.05, 1400, 2400, 2.0, 160, [(0, 0), (0.01, 1), (0.05, 0)]), gap * 0.6, 0.12)
    return carve_foley(out, 220, 7000)


@sound('slide_loop', 'movement', 2, loop=True, pitch=P_SOFT, lufs=-29, notes='loop; granular scrape; scale with slide speed')
def slide_loop(r, v):
    L, X = ns(3.0), ns(0.3)
    n = L + X
    scrape = np.zeros(n)
    rate = [420, 360][v]
    for t in np.sort(r.uniform(0, n / SR, int(rate * n / SR))):
        add(scrape, micro(r, U(r, 800, 3500), U(r, 0.0015, 0.004), 2, (1.3, 2.2)), t, U(r, 0.15, 1.0))
    bands = [(1150, 6), (1900, 7), (2900, 6)] if v == 0 else [(1000, 6), (1700, 7), (3300, 5)]
    tex = sum(reson(pink(n, r), f, q) for f, q in bands)
    tex = normpk(tex) * (1 + 0.5 * smooth_noise(n, r, 35))
    am = 1 + 0.25 * smooth_noise(n, r, 3)
    x = am * (normpk(scrape) + 0.2 * tex)
    return circ(lambda y: carve_foley(y, 220, 8000), loop_xfade(x, L, X))  # circular: stays seamless


@sound('wind_loop', 'movement', 2, loop=True, pitch=P_TONAL, lufs=-30,
       notes='loop; game scales volume/pitch with speed; resonant air + gusts + soft whistle')
def wind_loop(r, v):
    L, X = ns(6.0), ns(1.0)
    n = L + X
    gust = 0.6 + 0.4 * smooth_noise(n, r, 0.4)
    fc = 520 * 2 ** (0.7 * smooth_noise(n, r, 0.3))
    body = normpk(tvf(brown(n, r) + 0.6 * pink(n, r), fc, 2.2, 'bp'))
    whistle = normpk(tvf(pink(n, r), (1100 + 150 * v) * 2 ** (0.35 * smooth_noise(n, r, 0.25)), 22, 'bp'))
    air = normpk(lp(bp(pink(n, r), 1800, 5000), 5000)) * (0.5 + 0.5 * smooth_noise(n, r, 0.7))
    low = normpk(lp(brown(n, r), 140))
    x = gust * (body + 0.12 * whistle + 0.06 * air) + 0.35 * low * gust
    return circ(lambda y: hp(y, 60, 2), loop_xfade(x, L, X))


@sound('boost_burst', 'movement', 3, pitch=P_MICRO, lufs=-21, notes='ignition pop + shaped nozzle gas jet')
def boost_burst(r, v):
    d = 0.55
    n = ns(d)
    out = buf(d)
    add(out, click(r, 1500, 0.003), 0, 0.6)
    add(out, _pop(r), 0, 0.5)
    g = pink(n, r)
    f1, f2 = [(4200, 2100), (3600, 1800), (5000, 2500)][v]
    jet = tvf(g, sweep(n, f1, f1 * 0.45, 0.5), 3.0, 'bp') + 0.7 * tvf(g, sweep(n, f2, f2 * 0.45, 0.5), 3.5, 'bp')
    flutter = 1 + 0.25 * np.sin(TAU * U(r, 30, 45) * tvec(n)) + 0.2 * smooth_noise(n, r, 60)
    add(out, normpk(jet) * flutter * env_exp(n, 0.14, 0.004), 0, 0.8)
    return carve_foley(out, 150, 11000)


def _pop(r):
    """Small ignition pop: a damped metal-chamber knock."""
    f = U(r, 320, 480)
    n = ns(0.12)
    return modal([f, f * 1.9, f * 3.1], [0.03, 0.02, 0.01], [1, .5, .3], n, r) + 0.4 * fit(click(r, 2000, 0.002), n)


@sound('boost_loop', 'movement', 2, loop=True, pitch=P_SOFT, lufs=-26, notes='loop; nozzle jet: resonant bands + turbine whine')
def boost_loop(r, v):
    Ld = 2.0
    L, X = ns(Ld), ns(0.3)
    n = L + X
    g = pink(n, r)
    bands = [(1250, 3.0), (2700, 3.5), (4900, 3.0)] if v == 0 else [(1100, 3.0), (2400, 3.5), (4400, 3.0)]
    jet = normpk(sum(reson(g, f, q) * a for (f, q), a in zip(bands, (1.0, 0.8, 0.5))))
    jet *= 1 + 0.2 * smooth_noise(n, r, 30)
    noise = loop_xfade(jet, L, X)
    w1, w2 = qf(2350 + 200 * v, Ld), qf(3525 + 300 * v, Ld)
    tonal = periodic(lambda m: sine(w1, m) + 0.5 * sine(w2, m) * (1 + 0.3 * np.sin(TAU * 3.0 * tvec(m))), L)
    return circ(lambda y: carve_foley(y, 150, 11000), noise + 0.1 * tonal)


# =============================================================================== tethers (foley)
@sound('tether_fire', 'tether', 4, pitch=P_MICRO, lufs=-23, notes='latch click + cable zip + spring twang')
def tether_fire(r, v):
    d = 0.6
    n = ns(d)
    out = buf(d)
    add(out, click(r, 3000, 0.002), 0, 0.6)
    f = U(r, 2000, 2600)
    add(out, modal([f, f * 1.61, f * 2.26], [0.03, 0.02, 0.015], [1, .6, .4], ns(0.15), r), 0, 0.35)
    r0, r1 = [(650, 110), (800, 140), (560, 90), (720, 160)][v]
    tr = _pulse_train(n, sweep(n, r0, r1, 0.7)) * env_exp(n, 0.13, 0.002)
    z = reson(tr, 3200 * U(r, .9, 1.1), 5) + 0.7 * reson(tr, 1400 * U(r, .9, 1.1), 4)
    add(out, normpk(z), 0.004, 0.7)
    nn = ns(0.14)
    add(out, sine(sweep(nn, 2400, 850, 0.5), nn) * env_exp(nn, 0.03), 0, 0.15)
    add(out, string_twang(r, [360, 410, 330, 450][v], 0.12, 6), 0.01, 0.35)
    return carve_foley(out, 250, 10000)


@sound('tether_attach', 'tether', 4, pitch=P_IMPACT, lufs=-22, notes='hook bites stone: short metal clank + chips + taut cable tung')
def tether_attach(r, v):
    out = buf(0.9)
    hook = material_hit(r, 'metal', [0.35, 0.3, 0.45, 0.4][v], 1.3)
    add(out, hook * env_exp(len(hook), 0.12, 0), 0, 0.7)  # a hook, not a bell: damped by the rock
    add(out, material_hit(r, 'stone', 0.7, 1.2), 0, 0.7)
    add(out, grit(r, 10, 0.012, 1500, 4500, 0.003), 0.002, 0.35)
    add(out, string_twang(r, [380, 430, 350, 470][v], 0.28, 7, vib=0.002), 0.004, 0.35)
    return carve_foley(out, 180, 10000)


@sound('tether_reel_loop', 'tether', 2, loop=True, pitch=P_TONAL, lufs=-28, notes='loop; winch motor + gear teeth')
def tether_reel_loop(r, v):
    Ld = 2.0
    L = ns(Ld)
    f0 = [160.0, 180.0][v % 2]  # f*Ld integer => exactly periodic
    click_rate = [22.0, 26.0][v % 2]

    def motor(n):
        t = tvec(n)
        m = saw(f0, n) + saw(f0 + 0.5, n) + 0.5 * saw(2 * f0, n)
        m = lp(m, 2200)
        gear = 1 + 0.25 * np.sin(TAU * click_rate * t)
        whine = sine(8 * f0 + 4.0 * np.sin(TAU * 6.0 * t), n)
        return normpk(m) * gear + 0.18 * whine
    tonal = periodic(motor, L)
    clicks = np.zeros(L)
    k = int(round(click_rate * Ld))
    for i in range(k):
        c = modal([U(r, 2500, 2700), U(r, 4000, 4200)], [0.006, 0.004], [1, .6], ns(0.04), r)
        add_circ(clicks, c, int((i + 0.5) * L / k), U(r, 0.7, 1.0))
    teeth = np.zeros(L)
    kt = int(round(click_rate * 4 * Ld))  # fine gear teeth, 4 per click
    for i in range(kt):
        add_circ(teeth, micro(r, U(r, 1600, 2200), 0.0015, 2, (1.4, 1.9)), int((i + 0.25) * L / kt), U(r, 0.3, 0.6))
    x = 0.6 * tonal + 0.5 * normpk(clicks) + 0.15 * normpk(teeth)
    return circ(lambda y: carve_foley(y, 200, 10000), x)


@sound('tether_release', 'tether', 3, pitch=P_MICRO, lufs=-25, notes='latch snap + cable recoil')
def tether_release(r, v):
    out = buf(0.6)
    add(out, click(r, 2500, 0.002), 0, 0.8)
    add(out, micro(r, U(r, 2200, 2900), 0.02, 3, (1.5, 2.8)), 0, 0.4)
    n = ns(0.3)
    f = sweep(n, [2200, 1900, 2500][v], 520, 0.6)
    add(out, fm(f, 1.5, 0.8 * env_exp(n, 0.08), n) * env_pts(n, [(0, 0), (0.01, 1), (0.3, 0)], 1.5), 0.004, 0.35)
    add(out, string_twang(r, U(r, 300, 420), 0.1, 5), 0.002, 0.3)
    for _ in range(3):
        add(out, micro(r, U(r, 1200, 2600), 0.004), U(r, 0.03, 0.12), U(r, 0.1, 0.25))
    return carve_foley(out, 200, 10000)


# =============================================================================== hard light (constructs, E minor)
def _bloom_env(n, t_peak, tau, t0=0.0):
    t = tvec(n)
    a = np.clip((t - t0) / max(t_peak - t0, 1e-3), 0, 1) ** 1.5
    return np.where(t < t_peak, a, np.exp(-(t - t_peak) / tau))


@sound('construct_form', 'construct', 3, pitch=P_TONAL, lufs=-19,
       notes='hard light forming: rising E-minor arpeggio bloom + crystalline build + lock ping')
def construct_form(r, v):
    d = 1.1
    out = buf(d)
    voicing = [['E3', 'B3', 'E4', 'G4', 'B4'], ['E3', 'G3', 'B3', 'E4', 'G4'], ['E3', 'B3', 'D4', 'G4', 'B4']][v]
    n = ns(1.8)
    for i, nm in enumerate(voicing):
        t0 = 0.055 * i
        env = _bloom_env(n, 0.34, 0.22, 0.0)
        add(out, hl_tone(r, note(nm), n, env, bright=0.9, shimmer=0.5), t0, 0.5 / (1 + 0.15 * i))
    add(out, sine(note('E2'), n) * _bloom_env(n, 0.32, 0.12), 0, 0.3)  # weight of materialisation
    k0 = pent_index('E5')
    for j in range(7):  # crystalline build: ascending pentatonic sparkle, crescendo
        tt = 0.04 + 0.045 * j
        add(out, hl_ping(r, pent(k0 + j + v), 0.05, 1.2, glint=0.8), tt, 0.06 + 0.03 * j)
    add(out, hl_ping(r, note('E6'), 0.22, 1.2), 0.34, 0.35)  # lock
    add(out, hl_ping(r, note('B6'), 0.16, 1.2), 0.345, 0.2)
    add(out, click(r, 5000, 0.001), 0.34, 0.15)
    return out


@sound('construct_dismiss', 'construct', 3, pitch=P_TONAL, lufs=-23,
       notes='hard light dissolving: the form arpeggio falling, granulating away')
def construct_dismiss(r, v):
    d = 0.8
    out = buf(d)
    voicing = [['B4', 'G4', 'E4', 'B3', 'E3'], ['G4', 'E4', 'B3', 'G3', 'E3'], ['B4', 'G4', 'D4', 'B3', 'E3']][v]
    for i, nm in enumerate(voicing):
        p = hl_ping(r, note(nm), 0.22, 0.9, glint=0.4)
        m = len(p)
        gr = 1 - 0.6 * np.clip(tvec(m) / 0.3, 0, 1) * (0.5 + 0.5 * np.sin(TAU * U(r, 7, 11) * tvec(m)))  # flicker out
        add(out, p * gr, 0.05 * i, 0.45)
    n = ns(0.5)
    f = sweep(n, note('E4'), note('E3'), 0.8)  # the whole form sinking an octave (ends in key)
    add(out, (sine(f, n) + 0.3 * sine(2 * f, n)) * env_pts(n, [(0, 0), (0.03, 1), (0.5, 0)], 1.5), 0, 0.2)
    add(out, hl_shards(r, 4, 0.3, 'E8', 'E6', amp=0.35, dec=(0.02, 0.06)), 0.02, 1.0)
    return out


def hl_drone(r, freqs, n, period, bright=1.0, shimmer=0.5, lfo=0.3, amps=None):
    """Strictly periodic hard-light drone (every partial, beat and LFO has an integer number of cycles
    per `period`)."""
    t = tvec(n)
    x = np.zeros(n)
    amps = amps or [1.0] * len(freqs)
    for f, A in zip(freqs, amps):
        for k, a, _ in ((1, 1.0, 0), (2, 0.42, 0), (3, 0.26, 0), (4, 0.16, 0), (6, 0.08, 0)):
            fk = qf(f * k, period)
            if fk > 16000:
                continue
            a = A * a * (bright ** (k - 1) if k > 1 else 1.0)
            j = int(r.integers(1, 4))
            m = int(r.integers(1, 4))
            am = 1 + lfo * np.sin(TAU * m / period * t + r.uniform(0, TAU))
            x += a * am * (np.sin(TAU * fk * t + r.uniform(0, TAU))
                           + shimmer * np.sin(TAU * (fk + j / period) * t + r.uniform(0, TAU)))
    return x


@sound('beam_loop', 'construct', 2, loop=True, pitch=P_TONAL, lufs=-22,
       notes='loop; held beam: sustained E-minor drone, slow shimmer, sparse glints')
def beam_loop(r, v):
    Ld = 4.0
    L = ns(Ld)
    voicing = [['E2', 'B2', 'E3', 'G3', 'B3', 'E4'], ['E2', 'B2', 'D3', 'G3', 'B3', 'E4']][v]
    x = hl_drone(r, [note(nm) for nm in voicing], L, Ld, bright=1.0, shimmer=0.6, lfo=0.25,
                 amps=[0.9, 0.7, 0.6, 0.5, 0.4, 0.3])
    t = tvec(L)
    buzz = sine(qf(note('E6'), Ld), L) * (0.5 + 0.5 * np.sin(TAU * 8.0 * t))  # energy flutter
    x = normpk(x) + 0.05 * buzz
    for i in range(10):  # sparse hard-light glints (kept clear of the loop point)
        f = pent(pent_index('E6') + int(r.integers(0, 8)))
        add(x, hl_ping(r, f, 0.05, 1.2, glint=0.6), 0.1 + (Ld - 0.7) * (i + U(r, 0.1, 0.9)) / 10, 0.05)
    return x


@sound('shield_block', 'construct', 3, pitch=P_T10, lufs=-16,
       notes='struck E chord "glass gong" + E-tuned low thud')
def shield_block(r, v):
    out = buf(1.6)
    chord = [['E4', 'B4', 'E5', 'G5'], ['G4', 'B4', 'E5', 'B5'], ['E4', 'G4', 'B4', 'E5']][v]
    add(out, hl_chord(r, chord, dec=0.45, bright=1.05, stagger=0.002, amps=[1, .8, .7, .5]), 0, 0.8)
    add(out, click(r, 2500, 0.003), 0, 0.5)
    nn = ns(0.08)
    add(out, fm(glide(nn, 2637, 1319, 0.012), 1.0, 3 * env_exp(nn, 0.01), nn) * env_exp(nn, 0.015), 0, 0.25)
    add(out, thump(0.4, note('E3'), note('E2'), 0.02, 0.07, drive=1.8), 0, 0.7)
    return out


def _cube_tinks(r, n_cubes, t_first, t_max, lo='E6', hi='E7', amp=0.3):
    """Hard-light cubes bouncing to rest; each cube rings its own pentatonic note."""
    out = np.zeros(ns(t_max + 0.6))
    k_lo, k_hi = pent_index(lo), pent_index(hi)
    for _ in range(n_cubes):
        t0 = t_first + min(r.exponential(0.16), t_max * 0.6)
        f = pent(int(r.integers(k_lo, k_hi + 1)))
        for (t, a) in bounces(r, t0, U(r, 0.4, 1.0), U(r, 0.07, 0.18), U(r, 0.5, 0.7), 0.58, t_max):
            add(out, hl_ping(r, f, U(r, 0.03, 0.07), 1.1, glint=0.5, shimmer=0.2), t, amp * a)
    return out


@sound('construct_shatter', 'combat', 5, pitch=P_T10, lufs=-15,
       notes='SIGNATURE: hard-light break: crack + E-minor chord burst + E1 sub + falling arpeggiated shards')
def construct_shatter(r, v):
    d = 1.7
    out = buf(d)
    add(out, sat(click(r, 3000, 0.004), 2.0), 0, 0.8)
    chord = [['E5', 'B5', 'E6', 'G6'], ['G5', 'B5', 'E6', 'B6'], ['E5', 'G5', 'B5', 'E6'],
             ['B4', 'E5', 'G5', 'D6'], ['E5', 'A5', 'B5', 'E6']][v]
    add(out, hl_chord(r, chord, dec=0.16, bright=1.3, stagger=0.003), 0, 0.7)
    add(out, thump(0.7, note('E3'), note('E1'), 0.05, 0.16, drive=2.4), 0, 1.2)
    n = ns(0.4)
    f = sweep(n, note('E5'), note('E4'), 0.3)  # energy collapsing an octave
    add(out, (sine(f, n) + 0.4 * sine(2 * f, n)) * env_exp(n, 0.06, 0.002), 0, 0.3)
    add(out, hl_shards(r, int(U(r, 10, 14)), 1.0, 'E8', 'E6', amp=0.6), 0.01, 1.0)
    add(out, _cube_tinks(r, int(U(r, 6, 10)), 0.08, 1.3, amp=0.4), 0, 1.0)
    return out


@sound('clash', 'combat', 5, pitch=P_T10, lufs=-14,
       notes='two hard-light chords colliding: E and B chords, dissonance spike that resolves + E1 boom')
def clash(r, v):
    d = 1.5
    out = buf(d)
    add(out, sat(click(r, 1500, 0.005), 3.0), 0, 0.9)
    nz = ns(0.12)
    add(out, fm(glide(nz, 2637, 330, 0.03), 1.0, 7 * np.exp(-tvec(nz) / 0.02), nz) * env_exp(nz, 0.03), 0, 0.35)
    a = [['E4', 'G4', 'B4'], ['E4', 'B4', 'E5'], ['G4', 'B4', 'E5'], ['E4', 'G4', 'E5'], ['B3', 'E4', 'G4']][v]
    b = [['B4', 'D5', 'F#5'], ['B4', 'F#5', 'B5'], ['D5', 'F#5', 'B5'], ['B4', 'D5', 'B5'], ['F#4', 'B4', 'D5']][v]
    add(out, hl_chord(r, a, dec=0.4, bright=1.2, stagger=0.002), 0, 0.55)
    # the second construct arrives out of tune and is forced into consonance
    n = ns(1.9)
    t = tvec(n)
    det = 2 ** (U(r, 35, 55) / 1200 * np.exp(-t / 0.05))
    y = np.zeros(n)
    for nm in b:
        f = note(nm) * det
        for k, amp in ((1, 1.0), (2, 0.4), (3, 0.25)):
            y += amp * sine(f * k, n, r.uniform(0, TAU)) * np.exp(-t / (0.35 / k ** 0.5))
    add(out, y * np.clip(t / 0.0006, 0, 1), 0.004, 0.35)
    add(out, thump(1.0, note('E3'), note('E1'), 0.06, 0.28, drive=3.0), 0, 1.1)
    add(out, hl_shards(r, 4, 0.5, 'E8', 'B6', amp=0.3), 0.01, 1.0)
    return peq(out, 3200, -4, 0.8)


# =============================================================================== hits
_THWACK = [[1.0, 1.62, 2.4, 3.3], [1.0, 2.1, 3.7], [1.0, 1.45, 2.9, 4.1], [1.0, 1.8, 2.7], [1.0, 1.55, 2.25, 3.9],
           [1.0, 2.35, 3.1]]


def _thwack(r, f, v, dec=0.03):
    ratios = _THWACK[v % len(_THWACK)]
    decs = [dec / (1 + 0.5 * i) for i in range(len(ratios))]
    return modal([f * k for k in ratios], decs, [1 / (1 + 0.4 * i) for i in range(len(ratios))], ns(dec * 8), r)


@sound('hit_light', 'combat', 6, pitch=P_IMPACT, lufs=-18, notes='light punch: contact click + modal thwack + small thump')
def hit_light(r, v):
    out = buf(0.3)
    add(out, click(r, 2500, 0.002), 0, 0.5)
    add(out, _thwack(r, U(r, 300, 460), v, 0.03), 0, 0.9)
    add(out, thump(0.15, 180 * U(r, .92, 1.08), 95, 0.02, 0.04, drive=1.5), 0, 0.7)
    add(out, grit(r, 4, 0.006, 1800, 4000, 0.002), 0, 0.2)
    return glue(out, 1.8)


@sound('hit_medium', 'combat', 6, pitch=P_IMPACT, lufs=-15, notes='medium punch: click + lower thwack + saturated thump')
def hit_medium(r, v):
    out = buf(0.5)
    add(out, sat(click(r, 2200, 0.003), 1.5), 0, 0.6)
    add(out, _thwack(r, U(r, 200, 320), v, 0.045), 0, 0.9)
    add(out, thump(0.4, 150 * U(r, .92, 1.08), 58, 0.03, 0.09, drive=2.2), 0, 1.0)
    add(out, grit(r, 6, 0.01, 1200, 3500, 0.003), 0, 0.25)
    return peq(glue(out, 1.6), 3200, -3, 0.8)


@sound('hit_heavy', 'combat', 5, pitch=P_MICRO, lufs=-12, notes='big bass-drop punch; pair with hit-stop')
def hit_heavy(r, v):
    out = buf(1.6)
    add(out, sat(click(r, 2000, 0.005), 2.0), 0, 0.9)
    add(out, _thwack(r, U(r, 130, 200), v, 0.08), 0, 0.8)
    add(out, thump(1.0, 170 * U(r, .92, 1.08), 32, 0.07, 0.3, drive=3.0, attack=0.002), 0, 1.2)
    add(out, thump(1.2, 48, 30, 0.3, 0.4, drive=1.2, attack=0.02), 0.02, 0.5)
    add(out, grit(r, 10, 0.012, 900, 2600, 0.004), 0, 0.3)
    out = glue(out, 1.5)
    return carve_heavy(out)


@sound('wall_splat', 'combat', 3, pitch=P_MICRO, lufs=-12, notes='body slammed into stone: thud + stone break cascade')
def wall_splat(r, v):
    out = buf(1.5)
    add(out, thump(0.8, 110, 42, 0.05, 0.18, drive=2.5), 0, 1.0)
    add(out, _thwack(r, U(r, 160, 240), v, 0.06), 0, 0.6)
    add(out, shatter(r, 'stone', [10, 14, 8][v], 0.35, 1.6, 1.5), 0.002, 0.8)
    return carve_heavy(glue(out, 1.8))


@sound('kaiju_stomp', 'combat', 3, pitch=P_SOFT, lufs=-10, notes='sub-bass boom + ground crack + debris + rumble')
def kaiju_stomp(r, v):
    d = 3.5
    n = ns(d)
    out = buf(d)
    add(out, click(r, 900, 0.006), 0, 0.5)
    add(out, thump(2.5, 55 * U(r, .92, 1.08), 24, 0.15, 0.6, drive=2.5, attack=0.004), 0, 1.2)
    rum = normpk(lp(brown(n, r), 120)) * (1 + 0.4 * smooth_noise(n, r, 4)) * env_exp(n, 0.8, 0.05)
    add(out, rum, 0, 0.45)
    add(out, material_hit(r, 'stone', 3.0, 2.0), 0, 0.6)
    add(out, shatter(r, 'stone', [14, 18, 12][v], 1.0, 1.4, 1.2), 0.03, 0.35)
    return carve_heavy(glue(out, 2.6))


@sound('kaiju_roar', 'combat', 3, pitch=P_SOFT, lufs=-12,
       notes='synth monster roar ~1.6 s (detuned saws + breath through moving formants)')
def kaiju_roar(r, v):
    d = 2.0
    n = ns(d)
    t = tvec(n)
    k = U(r, 0.9, 1.1)
    f0 = env_pts(n, [(0, 55), (0.25, 85), (0.9, 78), (1.4, 60), (1.9, 42)]) * k
    f0 = f0 * (1 + 0.03 * smooth_noise(n, r, 12) + 0.02 * np.sin(TAU * 5 * t))
    growl = 1 + 0.55 * np.sin(TAU * np.cumsum(28 + 8 * smooth_noise(n, r, 2)) / SR)
    src = supersaw(f0, n, r, 5, 25) + 0.5 * square(f0 / 2, n)
    breath = pink(n, r) * 1.0
    exc = (src + breath) * growl
    F1 = env_pts(n, [(0, 550), (0.3, 750), (1.2, 700), (1.9, 450)])
    F2 = env_pts(n, [(0, 950), (0.3, 1250), (1.2, 1150), (1.9, 850)])
    body = (tvf(exc, F1, 5, 'bp') + 0.7 * tvf(exc, F2, 6, 'bp') + 0.4 * reson(exc, 2500, 8)
            + 0.6 * lp(src, 300))
    scr_f = f0 * 6 * (1 + 0.04 * smooth_noise(n, r, 20))
    screech = reson(saw(scr_f, n), 2400 * U(r, .9, 1.1), 3)
    screech *= env_pts(n, [(0, 0), (0.2, 0), (0.5, 1), (1.1, 0.8), (1.6, 0)])
    x = normpk(body) + 0.4 * normpk(screech)
    x = glue(x, 3.0)
    x *= env_pts(n, [(0, 0), (0.15, 0.8), (0.35, 1), (1.2, 0.85), (1.6, 0.3), (1.95, 0)])
    return carve_heavy(x, -4, -4)


def _fireball(r, d, fc0, fc1, tau_f, tau_a):
    """Combustion body: brown+pink (never white), falling low-pass, rough amplitude grain."""
    n = ns(d)
    src = brown(n, r) + 0.5 * pink(n, r)
    y = tvf(src, glide(n, fc0, fc1, tau_f), 0.8, 'lp') * (1 + 0.5 * smooth_noise(n, r, 45))
    return normpk(y) * env_exp(n, tau_a, 0.003)


@sound('explosion_small', 'combat', 3, pitch=P_MICRO, lufs=-12, notes='crack + sub punch + fireball + stone debris')
def explosion_small(r, v):
    d = 1.6
    out = buf(d)
    add(out, sat(click(r, 1500, 0.005), 2.0), 0, 0.8)
    add(out, _fireball(r, 1.0, 5000, 300, 0.1, 0.14), 0, 0.55)
    add(out, thump(1.0, 80 * U(r, .92, 1.08), 35, 0.05, 0.22, drive=2.8), 0, 1.15)
    add(out, shatter(r, 'stone', [8, 10, 7][v], 0.6, 0.8, 0.9), 0.04, 0.3)
    return carve_heavy(glue(out, 1.4))


@sound('explosion_big', 'combat', 3, pitch=P_SOFT, lufs=-9, notes='double crack + huge sub + fireball + debris rain + rumble')
def explosion_big(r, v):
    d = 4.0
    n = ns(d)
    out = buf(d)
    add(out, sat(click(r, 1200, 0.006), 2.5), 0, 0.9)
    add(out, sat(click(r, 2000, 0.004), 2.0), U(r, 0.012, 0.025), 0.6)
    add(out, _fireball(r, 2.5, 6000, 200, 0.25, 0.4), 0, 0.6)
    add(out, thump(3.0, 60, 25, 0.15, 0.7, drive=3.2, attack=0.003), 0, 1.3)
    rum = normpk(lp(brown(n, r), 120)) * (1 + 0.5 * smooth_noise(n, r, 3)) * env_pts(n, [(0, 0), (0.1, 1), (4, 0)], 2)
    add(out, rum, 0, 0.55)
    add(out, shatter(r, 'stone', [22, 26, 20][v], 2.0, 1.0, 1.0), 0.15, 0.28)
    for _ in range(4):
        add(out, thump(0.3, 130, 60, 0.02, 0.06, drive=1.5), U(r, 0.4, 1.6), 0.2)
    return carve_heavy(glue(out, 1.4))


# =============================================================================== destruction (material families)
@sound('break_stone', 'destruction', 6, pitch=P_IMPACT, lufs=-18, notes='stone block breaks: dense low partials + gritty chips')
def break_stone(r, v):
    out = buf(1.0)
    add(out, shatter(r, 'stone', int(U(r, 8, 14)), [0.3, 0.4, 0.25, 0.35, 0.45, 0.3][v], U(r, 1.2, 1.8), U(r, 1.0, 1.5)), 0, 1.0)
    if v in (2, 5):  # two-stage break
        add(out, material_hit(r, 'stone', 1.3, 1.2), U(r, 0.03, 0.06), 0.6)
    add(out, thump(0.2, 150, 75, 0.02, 0.05, drive=1.5), 0, 0.35)
    return out


@sound('break_dirt', 'destruction', 6, pitch=P_IMPACT, lufs=-22, notes='dirt/grass block: soft low-passed thud + clods')
def break_dirt(r, v):
    out = buf(0.7)
    add(out, shatter(r, 'dirt', int(U(r, 2, 5)), 0.25, [1.0, 1.2, 0.9, 1.3, 1.1, 1.0][v], 1.0, bounce=False), 0, 1.0)
    return lpf(out, 3500, 2)


@sound('break_wood', 'destruction', 6, pitch=P_IMPACT, lufs=-18, notes='hollow knock + dry crack + splinter crackle')
def break_wood(r, v):
    out = buf(1.0)
    add(out, shatter(r, 'wood', int(U(r, 4, 8)), 0.3, U(r, 1.1, 1.5), U(r, 1.1, 1.5)), 0, 1.0)
    add(out, grit(r, int(U(r, 14, 30)), [0.03, 0.05, 0.02, 0.04, 0.06, 0.035][v], 1800, 5500, 0.0015), 0.005, 0.35)
    if v % 2:
        add(out, material_hit(r, 'wood', 0.9, 1.3), U(r, 0.02, 0.05), 0.6)  # second crack
    return out


@sound('break_leaves', 'destruction', 6, pitch=P_IMPACT, lufs=-22, notes='leaf burst: crinkle clusters + twig snaps')
def break_leaves(r, v):
    out = buf(0.8)
    add(out, material_hit(r, 'leaves', [1.4, 1.8, 1.2, 1.6, 2.0, 1.3][v], 1.2), 0, 1.0)
    for _ in range(int(r.integers(1, 4))):
        add(out, material_hit(r, 'wood', 0.12, 0.8), U(r, 0.0, 0.08), 0.25)
    return carve_foley(out, 300, 9000)


@sound('break_crystal', 'destruction', 6, pitch=P_T10, lufs=-18, notes='crystal chime shatter, tuned to E minor pentatonic')
def break_crystal(r, v):
    out = buf(2.0)
    add(out, click(r, 4000, 0.002), 0, 0.6)
    k = pent_index('E6')
    notes = r.choice(np.arange(k, k + 7), 4 + v % 2, replace=False)
    for i, kk in enumerate(sorted(notes, reverse=True)):
        f = pent(int(kk))
        dec = U(r, 0.35, 0.7)
        add(out, modal([f, 2 * f, 3 * f, 4.16 * f, 5.43 * f], np.array([1, .6, .4, .22, .12]) * dec, [1, .45, .3, .15, .07],
                       ns(dec * 6), r), i * U(r, 0.012, 0.035), 0.3)
    add(out, shatter(r, 'glass', int(U(r, 6, 10)), 0.25, 0.5, 0.8), 0.005, 0.35)
    return out


@sound('break_metal', 'destruction', 6, pitch=P_IMPACT, lufs=-18, notes='metal block: inharmonic clang + ringing fragments')
def break_metal(r, v):
    out = buf(2.2)
    add(out, shatter(r, 'metal', int(U(r, 3, 6)), 0.4, U(r, 1.1, 1.6), U(r, 1.1, 1.5)), 0, 1.0)
    add(out, thump(0.2, 180, 90, 0.015, 0.04, drive=1.5), 0, 0.3)
    return out


@sound('break_glass', 'destruction', 6, pitch=P_IMPACT, lufs=-18, notes='glass pane: bright crack + tinkling shard cascade')
def break_glass(r, v):
    out = buf(1.6)
    add(out, shatter(r, 'glass', int(U(r, 14, 24)), [0.35, 0.45, 0.3, 0.5, 0.4, 0.3][v], U(r, 1.0, 1.4), U(r, 1.1, 1.5)), 0, 1.0)
    return out


@sound('break_hardlight', 'destruction', 6, pitch=P_T10, lufs=-18, notes='hard-light block: E-minor chord ping + falling shards')
def break_hardlight(r, v):
    out = buf(1.4)
    add(out, shatter(r, 'hardlight', int(U(r, 9, 15)), [0.5, 0.6, 0.4, 0.7, 0.55, 0.45][v], U(r, 0.8, 1.3), 1.0), 0, 1.0)
    add(out, thump(0.2, note('E3'), note('E2'), 0.02, 0.04, drive=1.2), 0, 0.3)
    return out


@sound('debris_patter', 'destruction', 6, pitch=P_IMPACT, lufs=-28, notes='small stone cubes bouncing to rest')
def debris_patter(r, v):
    out = buf(1.2)
    for _ in range(int(U(r, 4, 8))):
        t0 = U(r, 0, 0.25)
        s = U(r, 0.25, 0.5)
        for (t, a) in bounces(r, t0, U(r, 0.4, 1.0), U(r, 0.08, 0.18), U(r, 0.55, 0.7), 0.6, 1.1):
            add(out, material_hit(r, 'stone', s * U(r, 0.95, 1.05), 0.4 + 0.5 * a), t, a)
    return carve_foley(out, 200, 9000)


@sound('collapse_rumble', 'destruction', 2, pitch=P_SOFT, lufs=-14, notes='a tower falling: groans, thumps, stone cascades, rumble')
def collapse_rumble(r, v):
    d = 5.0
    n = ns(d)
    out = buf(d)
    add(out, material_hit(r, 'stone', 3.0, 2.0), 0, 0.7)
    groan = tvf(brown(n, r) * 3, sweep(n, 180, 80), 6, 'bp')
    add(out, normpk(groan) * env_pts(n, [(0, 0), (0.3, 0.8), (1.5, 0.5), (2.5, 0)]), 0, 0.45)
    rum = normpk(lp(brown(n, r), 150)) * (1 + 0.5 * smooth_noise(n, r, 2))
    rum *= env_pts(n, [(0, 0), (0.4, 1), (2.8, 1), (4.8, 0)])
    add(out, rum, 0, 0.8)
    for _ in range(5):
        tt = U(r, 0.7, 2.8)
        add(out, thump(0.8, 90, 35, 0.05, 0.25, drive=2.0), tt, U(r, 0.6, 1.0))
        add(out, shatter(r, 'stone', 8, 0.6, 2.0, 1.4), tt, 0.3)
    return carve_heavy(out)


# =============================================================================== UI (bright, small, in key)
@sound('ui_move', 'ui', 3, pitch=P_TONAL, lufs=-29, notes='menu hover tick (FM, E minor)')
def ui_move(r, v):
    n = ns(0.05)
    f = note(['E8', 'D8', 'B7'][v])
    x = fm(f, 2.0, 1.2 * env_exp(n, 0.004), n) * env_exp(n, 0.009, 0.0003)
    x += 0.3 * fit(click(r, 6000, 0.0008), n)
    return carve_ui(x, 2000)


@sound('ui_select', 'ui', 3, pitch=P_TONAL, lufs=-24, notes='confirm: rising glassy two-note (E minor)')
def ui_select(r, v):
    out = buf(0.3)
    a, b = [('E7', 'B7'), ('G7', 'D8'), ('B6', 'E7')][v]
    add(out, hl_ping(r, note(a), 0.06, 1.3, glint=1.0), 0, 0.8)
    add(out, hl_ping(r, note(b), 0.08, 1.3, glint=1.0), 0.05, 1.0)
    add(out, click(r, 5000, 0.0008), 0, 0.3)
    return carve_ui(out, 1800)


@sound('ui_back', 'ui', 3, pitch=P_TONAL, lufs=-25, notes='back: falling two-note, softer')
def ui_back(r, v):
    out = buf(0.3)
    a, b = [('B7', 'E7'), ('D8', 'G7'), ('E7', 'B6')][v]
    add(out, hl_ping(r, note(a), 0.05, 1.0, glint=0.6), 0, 0.8)
    add(out, hl_ping(r, note(b), 0.07, 1.0, glint=0.6), 0.05, 0.9)
    return carve_ui(out, 1800, 9000)


@sound('type_key', 'ui', 8, pitch=P_T10, lufs=-32, notes='ring-console keypress; 8 distinct ticks')
def type_key(r, v):
    n = ns(0.045)
    f = note(['E8', 'G8', 'A8', 'B7', 'D8', 'E8', 'G8', 'B7'][v])
    ratio = [2.0, 1.0, 3.0, 2.0, 1.5, 3.0, 2.0, 1.0][v]
    x = fm(f, ratio, [1.0, 1.6, 0.8, 1.3, 1.1, 0.6, 2.0, 1.4][v] * env_exp(n, 0.003), n) * env_exp(n, U(r, 0.006, 0.011), 0.0002)
    x += 0.4 * fit(click(r, 5000, 0.0007), n)
    g = U(r, 2600, 3400)
    x += 0.35 * fit(modal([g, g * 1.47], [0.004, 0.003], [1, .5], n, r), n)  # the key's body
    return carve_ui(x, 2000)


@sound('type_error', 'ui', 2, pitch=P_TONAL, lufs=-26, notes='denied: two buzzy minor-second blips')
def type_error(r, v):
    out = buf(0.3)
    for i in range(2):
        n = ns(0.07)
        f = 2200 * (1 + 0.03 * v)
        t = tvec(n)
        s = (fm(f, 1.0, 1.5, n) + fm(f * 2 ** (1 / 12), 1.0, 1.5, n)) * (0.6 + 0.4 * np.sign(np.sin(TAU * 55 * t)))
        s *= env_pts(n, [(0, 0), (0.004, 1), (0.055, 0.8), (0.07, 0)])
        add(out, s, i * 0.1, 1.0)
    return carve_ui(out, 1500)


@sound('cast_confirm', 'ui', 3, pitch=P_TONAL, lufs=-22, notes='ring console Enter: quick rising hard-light pings + glint')
def cast_confirm(r, v):
    out = buf(0.5)
    seq = [['E6', 'B6', 'E7'], ['G6', 'D7', 'G7'], ['B6', 'E7', 'B7']][v]
    for i, nm in enumerate(seq):
        add(out, hl_ping(r, note(nm), 0.07 + 0.03 * i, 1.3, glint=1.0), 0.035 * i, 0.6 + 0.2 * i)
    add(out, click(r, 6000, 0.0008), 0, 0.3)
    for _ in range(8):
        nn = ns(0.008)
        add(out, sine(pent(pent_index('E8') + int(r.integers(0, 4))), nn) * np.hanning(nn), U(r, 0.07, 0.2), 0.12)
    return carve_ui(out, 1800)


@sound('oath_complete', 'ui', 2, pitch=P_TONAL, lufs=-19, notes='triumphant rising E-minor arpeggio + bright Em7 bloom')
def oath_complete(r, v):
    d = 2.0
    out = buf(d)
    arp = [['E6', 'G6', 'B6', 'D7', 'E7'], ['G6', 'B6', 'D7', 'E7', 'G7']][v]
    for i, nm in enumerate(arp):
        add(out, hl_ping(r, note(nm), 0.25, 1.2, glint=1.0), 0.07 * i, 0.5)
    t0 = 0.35
    n = ns(1.6)
    env = env_pts(n, [(0, 0), (0.08, 1), (0.5, 0.7), (1.6, 0)], 1.4)
    for nm, a in (('E6', 0.6), ('B6', 0.5), ('D7', 0.4), ('G7', 0.35), ('E7', 0.4)):
        add(out, hl_tone(r, note(nm), n, env, bright=1.1, shimmer=0.6), t0, a)
    add(out, hl_shards(r, 4, 0.6, 'E8', 'E7', amp=0.25), t0, 1.0)
    return carve_ui(out, 1500)


@sound('willpower_low', 'ui', 2, pitch=P_TONAL, lufs=-24, notes='ring running dry: two flickering glassy pulses, falling G7 -> E7')
def willpower_low(r, v):
    out = buf(0.8)
    for i, (nm, a) in enumerate([('G7', 1.0), ('E7', 0.8)]):
        n = ns(0.22)
        t = tvec(n)
        flick = 0.55 + 0.45 * np.sin(TAU * np.cumsum(16 + 6 * smooth_noise(n, r, 8)) / SR)
        bend = 2 ** (-20 / 1200 * np.clip((t - 0.1) / 0.12, 0, 1))
        f = note(nm) * (1 + 0.01 * v) * bend
        s = (sine(f, n) + 0.3 * sine(2 * f, n)) * flick * env_pts(n, [(0, 0), (0.02, 1), (0.22, 0)], 1.5)
        add(out, s, 0.2 * i, a)
    return carve_ui(out, 1800)


@sound('ko', 'ui', 2, pitch=P_TONAL, lufs=-15, notes='KO sting: HF crack + bright Em chord stab + falling shard run')
def ko(r, v):
    d = 1.8
    out = buf(d)
    add(out, sat(click(r, 3000, 0.004), 2.0), 0, 0.8)
    chord = [['E6', 'G6', 'B6', 'E7'], ['B5', 'E6', 'G6', 'B6']][v]
    add(out, hl_chord(r, chord, dec=0.5, bright=1.35, stagger=0.004), 0, 0.8)
    n = ns(0.5)
    add(out, fm(note('E6'), 1.0, 3 * env_exp(n, 0.05) + 0.5, n) * env_exp(n, 0.12, 0.002), 0, 0.35)  # brassy sheen
    add(out, hl_shards(r, 8, 0.9, 'E8', 'E6', amp=0.4), 0.05, 1.0)
    return carve_ui(out, 1100)


@sound('respawn', 'ui', 2, pitch=P_TONAL, lufs=-20, notes='rising hard-light arpeggio swell + arrival chord')
def respawn(r, v):
    d = 1.6
    out = buf(d)
    k0 = pent_index('E5')
    for j in range(11):  # fast rising pentatonic run, crescendo
        tt = 0.55 * (j / 11) ** 0.8
        add(out, hl_ping(r, pent(k0 + j), 0.05, 1.2, glint=0.6), tt, 0.1 + 0.035 * j)
    n = ns(0.6)
    air = normpk(tvf(pink(n, r), sweep(n, 2000, 8000), 2.0, 'bp')) * env_pts(n, [(0, 0), (0.55, 1), (0.6, 0)], 2.0)
    add(out, air, 0, 0.12)
    ch = [['E6', 'B6', 'E7'], ['G6', 'B6', 'E7']][v]
    add(out, hl_chord(r, ch, dec=0.35, bright=1.3, stagger=0.003), 0.58, 0.8)
    add(out, click(r, 5000, 0.001), 0.58, 0.3)
    return carve_ui(out, 1200)


@sound('match_start', 'ui', 2, pitch=P_TONAL, lufs=-16, notes='fight-start sting: two count-in ticks + bright Em stab')
def match_start(r, v):
    d = 2.2
    out = buf(d)
    for tt in (0.0, 0.22):
        nn = ns(0.06)
        add(out, fm(note('E7'), 2.0, 1.5 * env_exp(nn, 0.006), nn) * env_exp(nn, 0.02), tt, 0.5)
        add(out, click(r, 4000, 0.002), tt, 0.4)
    t0 = 0.45
    top = buf(d)
    add(top, sat(click(r, 2500, 0.004), 2.0), t0, 0.8)
    ch = [['E5', 'B5', 'E6', 'G6', 'B6'], ['G5', 'B5', 'E6', 'G6', 'E7']][v]
    add(out, hl_chord(r, ch, dec=0.55, bright=1.35, stagger=0.003), t0, 0.9)
    n = ns(0.7)
    add(out, fm(note('E6'), 1.0, 3 * env_exp(n, 0.06) + 0.4, n) * env_exp(n, 0.2, 0.003), t0, 0.35)
    add(top, hl_chord(r, ['E7', 'B7'], dec=0.4, bright=1.2), t0, 0.5)
    add(top, hl_shards(r, 5, 0.6, 'E8', 'E7', amp=0.4), t0 + 0.03, 1.0)
    return carve_ui(balance_hi(out, top, 0.42), 900)


@sound('match_end', 'ui', 2, pitch=P_TONAL, lufs=-16, notes='match-over sting: Am(add9) -> Em resolve, bright')
def match_end(r, v):
    d = 3.0
    out = buf(d)
    add(out, sat(click(r, 2000, 0.004), 2.0), 0, 0.7)
    n1 = ns(0.8)
    e1 = env_pts(n1, [(0, 0), (0.01, 1), (0.6, 0.6), (0.8, 0)])
    for nm, a in (('A5', 0.6), ('C6', 0.5), ('E6', 0.5), ('B6', 0.4)):
        add(out, hl_tone(r, note(nm), n1, e1, bright=1.2, shimmer=0.5), 0, a)
    n2 = ns(2.2)
    e2 = env_pts(n2, [(0, 0), (0.03, 1), (0.8, 0.7), (2.2, 0)], 1.4)
    root = ['E5', 'G5'][v]
    for nm, a in ((root, 0.6), ('B5', 0.5), ('E6', 0.55), ('G6', 0.45), ('B6', 0.4), ('E7', 0.3)):
        add(out, hl_tone(r, note(nm), n2, e2, bright=1.2, shimmer=0.6), 0.65, a)
    top = buf(d)
    add(top, hl_chord(r, ['E6', 'B6', 'E7'], dec=0.4, bright=1.3), 0.65, 0.5)
    add(top, hl_chord(r, ['E7', 'B7'], dec=0.6, bright=1.2), 0.66, 0.4)
    add(top, hl_shards(r, 4, 0.8, 'E8', 'E7', amp=0.3), 0.68, 1.0)
    return carve_ui(balance_hi(out, top, 0.42), 900)
