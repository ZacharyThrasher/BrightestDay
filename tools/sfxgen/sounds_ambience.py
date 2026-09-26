"""World ambience + mix/danger cues.

Ambience beds are STEREO (decorrelated L/R above ~150 Hz, shared mono lows), seamless 10-16 s loops,
quiet and soft-edged. Positional ambience (emitters placed in the world) is MONO: a few loops and
many one-shot variations the game scatters in time and space.

Where the physical source really is noise (air, surf, tyres, thunder) it is pink/brown, band-shaped,
resonant and amplitude-grained - never raw white hiss. Everything else is tonal/modal.
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, hl_ping, material_hit, micro, note, pent, pent_index
from sounds import P_IMPACT, P_SOFT, P_TONAL, U, hl_drone, sound


def _noise_loop(x, L, X):
    return loop_xfade(x, L, X)


def _bed(r, Ld, Xd, make_ch, low=None):
    """Build a stereo loop: make_ch(n) renders one channel's (noise) material of n samples; each
    channel gets independent draws (decorrelated); `low(n)` (optional) is shared mono."""
    L, X = ns(Ld), ns(Xd)
    n = L + X
    lo = low(n) if low else 0.0
    chs = [loop_xfade(make_ch(n) + lo, L, X) for _ in range(2)]
    return st(*chs)


def _circ_st(fn, x):
    return st(circ(fn, x[:, 0]), circ(fn, x[:, 1]))


# =============================================================================== stereo beds
@sound('amb_archipelago_bed', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-32,
       notes='stereo bed: open sky air (low-passed moving air + slow swells) + distant ocean wash far below')
def amb_archipelago_bed(r, v):
    Ld = 12.0

    def ch(n):
        t = tvec(n)
        air = normpk(tvf(brown(n, r) + 0.5 * pink(n, r), 380 * 2 ** (0.6 * smooth_noise(n, r, 0.15)), 1.3, 'bp'))
        air *= 0.65 + 0.35 * smooth_noise(n, r, 0.1)
        hi = normpk(lp(bp(pink(n, r), 900, 2400), 2400)) * (0.5 + 0.5 * smooth_noise(n, r, 0.2))
        ph = r.uniform(0, TAU)
        swell = (0.5 + 0.5 * np.sin(TAU * t / 6.0 + ph)) ** 2  # two slow swells per loop (period 6 s)
        ocean = normpk(lp(brown(n, r), 280, 4)) * (0.35 + 0.65 * swell)
        return air + 0.06 * hi + 0.55 * ocean

    x = _bed(r, Ld, 1.0, ch, low=lambda n: 0.4 * normpk(lp(brown(n, r), 90, 4)))
    return _circ_st(lambda y: hp(y, 35, 2), x)


@sound('amb_wind_high', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-30,
       notes='stereo bed: altitude wind, slow gust envelope + soft tonal whistle; the game raises it with altitude/speed')
def amb_wind_high(r, v):
    Ld = 10.0
    L, X = ns(Ld), ns(1.0)
    n = L + X
    common = smooth_noise(n, r, 0.22)
    chs = []
    for c in range(2):
        gust = np.clip(0.75 * common + 0.25 * smooth_noise(n, r, 0.3), -1, 1)
        g01 = 0.5 + 0.5 * gust
        body = normpk(tvf(pink(n, r) + 0.7 * brown(n, r), 480 * 2 ** (0.9 * gust), 2.0, 'bp'))
        whistle = normpk(tvf(pink(n, r), (820 + 90 * c) * 2 ** (0.5 * gust), 26, 'bp'))
        top = normpk(bp(pink(n, r), 2500, 6000)) * g01 ** 2
        low = normpk(lp(brown(n, r), 160))
        x = (0.3 + 0.7 * g01 ** 1.5) * (body + 0.14 * whistle * g01 + 0.04 * top) + 0.3 * low * g01
        chs.append(loop_xfade(x, L, X))
    return _circ_st(lambda y: hp(y, 45, 2), st(*chs))


@sound('amb_city_bed', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-31,
       notes='stereo bed: distant city - low traffic rumble swells, faint hum partials, far soft horns; very low-passed')
def amb_city_bed(r, v):
    Ld = 16.0
    L, X = ns(Ld), ns(1.0)
    n = L + X
    shared = normpk(lp(brown(n, r), 120, 4)) * (0.7 + 0.3 * smooth_noise(n, r, 0.1))
    chs = []
    for c in range(2):
        rum = normpk(lp(brown(n, r), 220, 4)) * (0.6 + 0.4 * smooth_noise(n, r, 0.12))
        wash = normpk(reson(pink(n, r), 520, 1.2) + 0.6 * reson(pink(n, r), 850, 1.5))
        wash *= (0.4 + 0.6 * (0.5 + 0.5 * smooth_noise(n, r, 0.15)) ** 2)
        chs.append(loop_xfade(0.8 * shared + rum + 0.35 * wash, L, X))

    def hum(m):
        t = tvec(m)
        return (sine(qf(60, Ld), m) + 0.5 * sine(qf(120, Ld), m) + 0.25 * sine(qf(180, Ld), m)
                + 0.15 * sine(qf(note('E4'), Ld), m) * (0.6 + 0.4 * np.sin(TAU * t / 8.0)))
    tone = periodic(hum, L)
    x = st(*chs)
    for c in range(2):
        x[:, c] += 0.05 * tone
        for _ in range(2):  # far soft horns, a couple per loop, each on one side
            if r.uniform() < 0.5:
                continue
            d = U(r, 0.25, 0.6)
            m = ns(d)
            f = U(r, 300, 420)
            h = lp(square(f, m, duty=0.35) + 0.6 * saw(f * 1.26, m), 700, 4)
            h *= env_pts(m, [(0, 0), (0.05, 1), (d - 0.08, 0.9), (d, 0)])
            add_circ(x[:, c], normpk(h), int(r.uniform(0, L)), 0.05)
    return _circ_st(lambda y: lp(hp(y, 30, 2), 1500, 2), x)


def _wave_env(t, period, off, rise=0.35, crash=0.38):
    """Envelopes of one breaking-wave cycle (periodic): swell, crash, foam, recede."""
    u = ((t - off) % period) / period
    swell = np.where(u < crash, (u / crash) ** 2, np.exp(-(u - crash) / 0.08))
    crash_e = np.where(u >= crash, np.exp(-(u - crash) / 0.05), 0.0) * np.clip((u - crash + 0.004) / 0.004, 0, 1)
    foam = np.where(u >= crash, np.exp(-(u - crash) / 0.18), 0.0) * np.clip((u - crash) / 0.02, 0, 1)
    recede = np.where(u >= 0.5, np.sin(np.pi * np.clip((u - 0.5) / 0.5, 0, 1)) ** 2, 0.0)
    return swell, crash_e, foam, recede


def _surf_channel(r, n, period, off, scale=1.0, bubbles_per_s=40):
    t = tvec(n)
    swell, crash_e, foam, recede = _wave_env(t, period, off)
    b = brown(n, r)
    p = pink(n, r)
    x = 0.8 * normpk(lp(b, 450, 4)) * swell
    x += 0.8 * normpk(bp(p, 450, 3000)) * crash_e * (1 + 0.4 * smooth_noise(n, r, 40))
    x += 0.35 * normpk(lp(hp(pink(n, r), 1500, 2), 6500)) * foam * (1 + 0.6 * smooth_noise(n, r, 60))
    x += 0.3 * normpk(lp(pink(n, r), 1300)) * recede
    bub = np.zeros(n)  # foam bubbles: tiny tonal pops instead of hiss
    for tt in r.uniform(0, n / SR, int(bubbles_per_s * n / SR)):
        _, _, fo, rc = _wave_env(np.array([tt]), period, off)
        a = float(fo[0] + 0.3 * rc[0])
        if a > 0.03:
            add(bub, micro(r, U(r, 1800, 6500), U(r, 0.002, 0.006), 2, (1.4, 2.1)), tt, a * U(r, 0.3, 1.0))
    x += 0.25 * normpk(bub)
    return scale * x


@sound('amb_ocean_loop', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-30,
       notes='stereo bed: beach surf, wave swells every 8 s (rolling across L->R), foam wash with bubbles')
def amb_ocean_loop(r, v):
    Ld = 16.0
    L, X = ns(Ld), ns(1.0)
    n = L + X
    offs = [0.0, 0.7]
    chs = [loop_xfade(_surf_channel(r, n, 8.0, o), L, X) for o in offs]
    return _circ_st(lambda y: hp(y, 35, 2), st(*chs))


@sound('amb_tutorial_bed', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-36,
       notes='stereo bed: sparse, calm E-minor hard-light pad breathing + faint air')
def amb_tutorial_bed(r, v):
    Ld = 16.0
    L, X = ns(Ld), ns(1.0)
    voicing = [note(x) for x in ('E3', 'B3', 'E4', 'G4', 'D5')]
    chs = []
    for c in range(2):
        pad = hl_drone(r, voicing, L, Ld, bright=0.55, shimmer=0.6, lfo=0.6, amps=[1.0, 0.7, 0.6, 0.45, 0.3])
        t = tvec(L)
        breath = 0.55 + 0.45 * np.sin(TAU * t / 8.0 + (0.6 if c else 0.0)) ** 2
        n = L + X
        air = normpk(lp(hp(pink(n, r), 250, 2), 1400)) * (0.6 + 0.4 * smooth_noise(n, r, 0.2))
        chs.append(normpk(pad) * breath + 0.08 * loop_xfade(air, L, X))
    return st(*chs)


@sound('amb_planet_hum', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-30,
       notes='stereo bed (Orbit): deep warm E1/E2 drone with slowly beating partials')
def amb_planet_hum(r, v):
    Ld = 12.0
    L = ns(Ld)
    chs = []
    for c in range(2):
        d = hl_drone(r, [note('E1'), note('E2'), note('B2'), note('E3')], L, Ld, bright=0.8, shimmer=0.8, lfo=0.25,
                     amps=[1.0, 0.8, 0.35, 0.25])
        d = circ(lambda y: lp(y, 900, 2), d)
        sh = hl_drone(r, [note('E5'), note('B5')], L, Ld, bright=0.5, shimmer=1.0, lfo=0.8, amps=[1.0, 0.6])
        chs.append(normpk(d) + 0.02 * normpk(sh))
    return st(*chs)


@sound('amb_space_bed', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-40,
       notes='stereo bed: near-silent vacuum - low body-conducted hum + faint suit-like tonal air')
def amb_space_bed(r, v):
    Ld = 12.0
    L, X = ns(Ld), ns(1.0)
    n = L + X
    chs = []
    for c in range(2):
        t = tvec(L)
        hum = hl_drone(r, [note('E1'), note('E2')], L, Ld, bright=0.4, shimmer=0.5, lfo=0.15, amps=[1.0, 0.5])
        vent = 0.5 + 0.5 * np.sin(TAU * t / 4.0 + (0.4 if c else 0.0)) ** 2  # suit ventilation cycle, 4 s
        suit = hl_drone(r, [note('B4'), note('E5')], L, Ld, bright=0.3, shimmer=1.0, lfo=0.3, amps=[1.0, 0.6])
        air = loop_xfade(normpk(reson(pink(n, r), 520, 2.0) + 0.6 * reson(pink(n, r), 880, 2.5)), L, X)
        chs.append(normpk(hum) + 0.02 * suit * vent + 0.05 * air * vent)
    return st(*chs)


@sound('amb_interior_tone', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-38,
       notes='stereo bed: sealed-interior room tone - soft 60/120 Hz hum + faint duct HVAC')
def amb_interior_tone(r, v):
    Ld = 10.0
    L, X = ns(Ld), ns(1.0)
    n = L + X
    chs = []
    for c in range(2):
        hum = periodic(lambda m: sine(60.0, m) + 0.6 * sine(120.0, m) + 0.2 * sine(180.0, m) + 0.1 * sine(240.0, m), L)
        g = pink(n, r)
        hvac = normpk(lp(hp(g, 70, 2), 600) + 0.5 * reson(g, 180, 3) + 0.25 * reson(g, 410, 4))
        hvac *= 1 + 0.08 * smooth_noise(n, r, 0.5)
        chs.append(0.5 * normpk(hum) + loop_xfade(hvac, L, X))
    return st(*chs)


# =============================================================================== positional loops (mono)
@sound('amb_neon_hum', 'ambience', 1, loop=True, pitch=P_TONAL, lufs=-30,
       notes='positional loop: neon/electrical buzz (60 Hz family) + gentle crackle + rare flicker')
def amb_neon_hum(r, v):
    Ld = 4.0
    L = ns(Ld)
    t = tvec(L)
    x = 0.3 * sine(60.0, L)
    for k in range(1, 14):
        x += (1.0 / k ** 0.75) * sine(120.0 * k, L, r.uniform(0, TAU))
    x = sat(0.6 * normpk(x), 1.8)
    flick = np.ones(L)
    for c in (1.3, 2.9):  # two soft flickers per loop
        flick -= 0.35 * np.exp(-((t - c) / 0.03) ** 2)
    x *= flick
    for _ in range(24):
        add_circ(x, micro(r, U(r, 2500, 7000), 0.0015, 2, (1.3, 2.0)), int(r.uniform(0, L)), U(r, 0.02, 0.1))
    return circ(lambda y: hp(y, 50, 2), x)


@sound('amb_ac_hum', 'ambience', 1, loop=True, pitch=P_TONAL, lufs=-30,
       notes='positional loop: AC unit - blade-pass fan tone + motor hum + soft rattle + airflow')
def amb_ac_hum(r, v):
    Ld = 6.0
    L, X = ns(Ld), ns(0.6)
    n = L + X
    shaft = 19.0
    bpf = 5 * shaft

    def tonal(m):
        t = tvec(m)
        fan = sum((1.0 / k) * sine(bpf * k, m) for k in range(1, 7)) * (1 + 0.2 * np.sin(TAU * shaft * t))
        motor = sine(120.0, m) + 0.4 * sine(240.0, m)
        return lp(fan, 900) + 0.5 * motor
    ton = periodic(tonal, L)
    g = pink(n, r)
    air = loop_xfade(normpk(lp(hp(g, 150, 2), 1600) + 0.5 * reson(g, 700, 2)), L, X)
    rattle = np.zeros(L)
    k = int(shaft * Ld)
    for i in range(k):
        if r.uniform() < 0.45:
            add_circ(rattle, micro(r, U(r, 1400, 2600), 0.004, 3, (1.4, 2.4)), int((i + 0.3) * L / k), U(r, 0.2, 1.0))
    return normpk(ton) + 0.6 * air + 0.12 * normpk(rattle)


@sound('amb_insects_loop', 'ambience', 1, loop=True, pitch=P_SOFT, lufs=-32,
       notes='positional loop: crickets (grouped tonal pulses 4-5 kHz) + a soft cicada swell')
def amb_insects_loop(r, v):
    Ld = 8.0
    L = ns(Ld)
    x = np.zeros(L)
    for f, period, pulses in ((4300.0, 0.62, 3), (4750.0, 0.83, 4), (5200.0, 1.1, 3)):
        t = U(r, 0, 0.3)
        amp = U(r, 0.5, 1.0)
        while t < Ld:
            for p in range(pulses):
                m = ns(U(r, 0.012, 0.018))
                s = sine(sweep(m, f * 1.01, f * 0.99), m) * np.hanning(m)
                add_circ(x, s, int((t + p * U(r, 0.03, 0.038)) * SR), amp * U(r, 0.7, 1.0))
            t += period * U(r, 0.9, 1.1)
    t = tvec(L)
    cic = sine(5600.0, L) * (0.5 + 0.5 * np.sin(TAU * 180.0 * t)) ** 3
    cic *= (0.5 + 0.5 * np.sin(TAU * t / Ld)) ** 3
    return circ(lambda y: hpf(y, 2000, 4), x + 0.12 * cic)


@sound('amb_surf_loop', 'ambience', 1, loop=True, pitch=P_TONAL, lufs=-30,
       notes='positional loop: close waves lapping a shore/rock edge, 6 s swell cycle, bubbles')
def amb_surf_loop(r, v):
    Ld = 12.0
    L, X = ns(Ld), ns(1.0)
    n = L + X
    x = _surf_channel(r, n, 6.0, 0.0, bubbles_per_s=60)
    t = tvec(n)
    for c in (0.38 * 6.0, 0.38 * 6.0 + 6.0, 0.38 * 6.0 + 12.0):  # rock slaps at each break
        if c < n / SR:
            add(x, thump(0.2, 130, 75, 0.015, 0.04, drive=0), c, 0.5)
            add(x, normpk(lp(pink(ns(0.04), r), 1200)) * env_exp(ns(0.04), 0.01), c, 0.3)
    return circ(lambda y: hp(y, 60, 2), loop_xfade(x, L, X))


# =============================================================================== positional one-shots (mono)
def _chirp(f0, f1, d, shape=1.0, vib_hz=0.0, vib=0.0, h2=0.12, pts=None):
    n = ns(d)
    t = tvec(n)
    f = sweep(n, f0, f1, shape) * (1 + vib * np.sin(TAU * vib_hz * t))
    x = sine(f, n) + h2 * sine(2 * f, n)
    return x * env_pts(n, pts or [(0, 0), (d * 0.2, 1), (d * 0.7, 0.8), (d, 0)])


@sound('amb_bird', 'ambience', 8, pitch=P_IMPACT, lufs=-30, notes='songbird phrases (8 original "species")')
def amb_bird(r, v):
    out = buf(1.6)
    k = U(r, 0.95, 1.05)
    if v == 0:  # three rising whistles
        for i in range(3):
            add(out, _chirp(2800 * k * (1 + 0.08 * i), 3600 * k * (1 + 0.08 * i), 0.09), 0.15 * i, 1.0)
    elif v == 1:  # accelerating trill
        t = 0.0
        for i in range(14):
            add(out, _chirp(5200 * k, 4200 * k, 0.025, 0.7), t, 0.8 + 0.2 * np.sin(i))
            t += 0.05 * (0.97 ** i)
    elif v == 2:  # tee-tuu
        add(out, _chirp(4000 * k, 4050 * k, 0.18, vib_hz=12, vib=0.01), 0, 1.0)
        add(out, _chirp(3200 * k, 2950 * k, 0.26, vib_hz=10, vib=0.008), 0.24, 0.9)
    elif v == 3:  # warble
        add(out, _chirp(3300 * k, 3600 * k, 0.5, vib_hz=28, vib=0.08, pts=[(0, 0), (0.1, 1), (0.4, 0.9), (0.5, 0)]), 0, 1.0)
    elif v == 4:  # chip series
        t = 0.0
        for i in range(5):
            add(out, _chirp(6000 * k, 3500 * k, 0.014, 0.5, h2=0.05), t, U(r, 0.7, 1.0))
            t += U(r, 0.07, 0.12)
    elif v == 5:  # descending cascade with small up-flicks
        for i in range(8):
            f = 5000 * k * 0.93 ** i
            add(out, _chirp(f * 0.95, f * 1.05, 0.05), 0.065 * i, 1.0 - 0.06 * i)
    elif v == 6:  # rising whistle then trill
        add(out, _chirp(2200 * k, 4200 * k, 0.3, 0.8), 0, 1.0)
        for i in range(8):
            add(out, _chirp(4400 * k, 3800 * k, 0.03), 0.35 + 0.045 * i, 0.7)
    else:  # chips + buzzy notes
        add(out, _chirp(6000 * k, 5200 * k, 0.03), 0, 0.8)
        add(out, _chirp(5600 * k, 4900 * k, 0.03), 0.07, 0.8)
        for i in range(3):
            n = ns(0.12)
            b = _chirp(3200 * k, 3100 * k, 0.12) * (0.6 + 0.4 * np.sin(TAU * 90 * tvec(n)))
            add(out, b, 0.18 + 0.16 * i, 0.9)
    return hpf(out, 1500, 4)


def _gull_call(r, d, fpts, rough=55.0, bright=1.0):
    n = ns(d)
    f = env_pts(n, fpts)
    src = saw(f, n) + 0.4 * square(f, n, duty=0.3)
    src = src * (1 + 0.3 * np.sin(TAU * rough * tvec(n)))
    breath = pink(n, r) * 0.15
    x = reson(src + breath, 1400, 3) + 0.8 * reson(src + breath, 2800 * bright, 5) + 0.4 * reson(src, 3900, 6)
    return normpk(lp(x, 6000)) * env_pts(n, [(0, 0), (0.02, 1), (d * 0.7, 0.8), (d, 0)])


@sound('amb_gull', 'ambience', 4, pitch=P_IMPACT, lufs=-28, notes='seagull cries (original synthesis)')
def amb_gull(r, v):
    out = buf(2.0)
    k = U(r, 0.93, 1.07)
    if v == 0:
        for i in range(3):
            add(out, _gull_call(r, 0.32, [(0, 900 * k), (0.07, 1500 * k), (0.22, 1150 * k), (0.32, 750 * k)]), 0.42 * i, 1 - 0.15 * i)
    elif v == 1:
        add(out, _gull_call(r, 0.8, [(0, 800 * k), (0.12, 1550 * k), (0.5, 1350 * k), (0.8, 700 * k)], 45), 0, 1.0)
    elif v == 2:
        for i in range(4):
            add(out, _gull_call(r, 0.12, [(0, 700 * k), (0.04, 1000 * k), (0.12, 650 * k)], 70, 0.9), 0.16 * i, 0.9)
    else:
        for i in range(2):
            add(out, lp(_gull_call(r, 0.38, [(0, 950 * k), (0.08, 1450 * k), (0.38, 800 * k)]), 3000), 0.5 * i, 1.0)
    return hpf(out, 400, 4)


@sound('amb_surf', 'ambience', 6, pitch=P_IMPACT, lufs=-28, notes='a single wave washing / crashing on rocks, 2-4 s')
def amb_surf(r, v):
    d = [2.4, 3.2, 2.8, 3.8, 2.2, 3.4][v]
    n = ns(d)
    out = buf(d)
    tc = U(r, 0.4, 0.8)
    t = tvec(n)
    swell = np.clip(t / tc, 0, 1) ** 2 * np.exp(-np.clip(t - tc, 0, None) / 0.2)
    crash = np.where(t >= tc, np.exp(-(t - tc) / U(r, 0.2, 0.4)), 0.0) * np.clip((t - tc) / 0.02, 0, 1)
    foam = np.where(t >= tc, np.exp(-(t - tc) / (d * 0.35)), 0.0) * np.clip((t - tc) / 0.05, 0, 1)
    rec = np.sin(np.pi * np.clip((t - tc - 0.3) / (d - tc - 0.3), 0, 1)) ** 2
    x = 0.8 * normpk(lp(brown(n, r), 500, 4)) * swell
    x += normpk(bp(pink(n, r), 400, 3000)) * crash * (1 + 0.4 * smooth_noise(n, r, 40))
    x += 0.4 * normpk(lp(hp(pink(n, r), 1600, 2), 7000)) * foam * (1 + 0.6 * smooth_noise(n, r, 60))
    x += 0.3 * normpk(lp(pink(n, r), 1200)) * rec
    add(out, x, 0, 1.0)
    if v % 2 == 0:  # rock slap
        add(out, thump(0.25, 120, 70, 0.015, 0.05, drive=0), tc, 0.6)
    for _ in range(int(U(r, 60, 120))):  # bubbles
        tt = tc + r.exponential(d * 0.25)
        if tt < d:
            add(out, micro(r, U(r, 1800, 6500), U(r, 0.002, 0.006), 2, (1.4, 2.1)), tt, U(r, 0.05, 0.25) * np.exp(-(tt - tc) / d))
    return hp(out, 50, 2)


@sound('amb_palm_rustle', 'ambience', 4, pitch=P_IMPACT, lufs=-32, notes='palm/leaf rustle in a gust, crinkle clusters, 1-2 s')
def amb_palm_rustle(r, v):
    d = [1.2, 1.8, 1.5, 2.0][v]
    n = ns(d)
    out = buf(d)
    tp = d * U(r, 0.35, 0.55)
    gust = env_pts(n, [(0, 0), (tp, 1), (d, 0)], 1.5)
    for _ in range(int(U(r, 180, 300) * d)):
        tt = r.uniform(0, d)
        g = float(gust[min(n - 1, ns(tt))])
        if r.uniform() < g:
            add(out, micro(r, U(r, 1600, 6000), U(r, 0.0012, 0.003), 3, (1.2, 1.8)), tt, U(r, 0.2, 1.0) * g)
    air = normpk(bp(pink(n, r), 500, 2500)) * gust * (1 + 0.4 * smooth_noise(n, r, 25))
    add(out, air, 0, 0.12)
    for _ in range(int(r.integers(2, 5))):
        add(out, lpf(material_hit(r, 'wood', 0.3, 0.4), 2000, 2), tp + U(r, -0.2, 0.2), 0.12)
    return hpf(out, 300, 4)


@sound('amb_traffic_swish', 'ambience', 6, pitch=P_IMPACT, lufs=-30,
       notes='a car passing by: doppler engine harmonics + tyre roar, distance-shaped, 1.5-3 s')
def amb_traffic_swish(r, v):
    d = [2.4, 1.8, 3.0, 2.2, 1.6, 2.8][v]
    n = ns(d)
    t = tvec(n)
    speed = [18, 24, 14, 20, 28, 12][v] * U(r, 0.9, 1.1)
    D = [6, 8, 10, 5, 7, 9][v] * U(r, 0.9, 1.1)
    fe = [85, 100, 70, 90, 165, 52][v] * U(r, 0.95, 1.05)
    xpos = speed * (t - d / 2)
    dist = np.sqrt(xpos ** 2 + D ** 2)
    vr = speed * xpos / dist
    dop = 343.0 / (343.0 + vr)
    amp = D / dist
    eng = np.zeros(n)
    for k in range(1, 14):
        eng += (1.0 / k ** 0.9) * (1.3 if k % 2 else 1.0) * sine(fe * k * dop, n, r.uniform(0, TAU))
    g = pink(n, r)
    tyre = normpk(reson(g, 750, 1.4) + 0.7 * reson(g, 1400, 1.8) + 0.4 * lp(brown(n, r), 300))
    x = (0.5 * normpk(eng) * (0.6 if v != 5 else 1.0) + tyre) * amp ** 1.5
    x = tvf(x, 1200 + 5000 * amp ** 2, 0.7, 'lp')  # air absorption: brighter when close
    x *= env_pts(n, [(0, 0), (0.15, 1), (d - 0.15, 1), (d, 0)])
    return hp(x, 45, 2)


def _creak(r, d, f0, rate0, rate1, q=40.0):
    """Stick-slip friction exciting a metal plate's modes: irregular impulse train -> resonator bank."""
    n = ns(d)
    imp = np.zeros(n)
    t = 0.0
    while t < d:
        u = t / d
        rate = rate0 + (rate1 - rate0) * u
        a = np.sin(np.pi * u) ** 0.7 * U(r, 0.3, 1.0)
        imp[min(n - 1, ns(t))] += a * (1 if r.uniform() < 0.5 else -1)
        t += U(r, 0.6, 1.4) / rate
    y = np.zeros(n)
    for k, g in ((1.0, 1.0), (1.59, 0.8), (2.14, 0.6), (2.76, 0.5), (3.51, 0.35), (5.4, 0.2)):
        y += g * reson(imp, f0 * k * U(r, 0.98, 1.02), q)
    return y


@sound('amb_station_creak', 'ambience', 4, pitch=P_IMPACT, lufs=-30, notes='Orbit: metal hull creak / groan (modal stick-slip)')
def amb_station_creak(r, v):
    out = buf(2.5)
    if v == 0:
        add(out, _creak(r, 1.2, U(r, 140, 190), 14, 30), 0, 1.0)
    elif v == 1:
        add(out, _creak(r, 1.8, U(r, 70, 100), 7, 12, 60), 0, 1.0)  # slow groan
    elif v == 2:
        add(out, _creak(r, 0.6, U(r, 200, 260), 25, 40), 0, 1.0)
        add(out, _creak(r, 0.5, U(r, 180, 230), 30, 20), 0.75, 0.8)
    else:
        add(out, _creak(r, 1.4, U(r, 110, 150), 10, 28, 50), 0, 1.0)
        add(out, material_hit(r, 'metal', 2.5, 0.4), 1.35, 0.25)  # settles with a soft hull knock
    return hp(out, 50, 2)


@sound('amb_distant_boom', 'ambience', 3, pitch=P_IMPACT, lufs=-26, notes='far thunder-like boom, rolling, low-passed')
def amb_distant_boom(r, v):
    d = [3.0, 3.8, 2.5][v]
    n = ns(d)
    out = buf(d)
    t = tvec(n)
    rolls = np.zeros(n)
    for i in range([4, 5, 3][v]):
        c = 0.05 + (d * 0.55) * (i / 4) ** 1.3 + U(r, 0, 0.15)
        rolls += U(r, 0.4, 1.0) * (1 - 0.15 * i) * np.exp(-((t - c) / U(r, 0.12, 0.3)) ** 2)
    body = normpk(lp(brown(n, r), 260, 4)) * rolls * env_pts(n, [(0, 0), (0.05, 1), (d, 0)])
    add(out, body, 0, 1.0)
    add(out, thump(2.0, 48, 28, 0.3, 0.5, drive=1.2, attack=0.04), 0, 0.8)
    return lpf(hp(out, 25, 2), 350, 4)


# =============================================================================== danger / mix cues (feedback)
@sound('heartbeat', 'feedback', 2, pitch=P_TONAL, lufs=-22,
       notes='danger cue: lub-dub, mostly 40-90 Hz + a small 200 Hz knock so it reads on laptops')
def heartbeat(r, v):
    out = buf(0.7)
    gap = [0.2, 0.17][v]
    for (tt, a, f0) in ((0.0, 1.0, 50.0), (gap, 0.75, 58.0)):
        add(out, thump(0.3, f0 * 1.5, f0, 0.03, 0.06, drive=1.3, attack=0.006), tt, a)
        add(out, modal([200.0, 310.0], [0.02, 0.012], [1, .5], ns(0.15), r), tt + 0.004, 0.18 * a)
    return lpf(out, 500, 2)


@sound('tinnitus', 'feedback', 1, pitch=P_TONAL, lufs=-30, notes='soft ~6.5 kHz ring fading in/out (rare, after huge blasts)')
def tinnitus(r, v):
    d = 1.5
    n = ns(d)
    x = sine(6500.0, n) + 0.3 * sine(6503.0, n)
    return x * env_pts(n, [(0, 0), (0.35, 1), (0.7, 0.9), (d, 0)], 1.5)


@sound('final_blow_suck', 'feedback', 2, pitch=P_TONAL, lufs=-20, fade_out=0.008,
       notes='reverse-swell inhale before the KO impact: rising tonal swoosh that cuts dead at the end')
def final_blow_suck(r, v):
    d = [0.35, 0.3][v]
    n = ns(d)
    u = np.linspace(0, 1, n)
    env = u ** 3
    x = np.zeros(n)
    for nm, a in (('E6', 1.0), ('B6', 0.7), ('E7', 0.5)):
        f = note(nm) * 2 ** (u - 1)  # rises an octave into the note
        x += a * (sine(f, n) + 0.35 * sine(2 * f, n) + 0.2 * sine(3 * f, n))
    x += 0.4 * sine(note('E5') * 2 ** (u - 1), n)  # a little body underneath
    air = normpk(tvf(pink(n, r), sweep(n, 1500, 8000, 0.8), 2.5, 'bp'))
    y = (normpk(x) + 0.35 * air) * env
    return np.concatenate([carve_ui(y, 1200), np.zeros(ns(0.01))])


@sound('mix_whoosh_in', 'feedback', 1, pitch=P_TONAL, lufs=-26, notes='ring console open: soft rising air + faint E-minor shimmer')
def mix_whoosh_in(r, v):
    d = 0.25
    n = ns(d)
    air = normpk(tvf(pink(n, r), sweep(n, 2000, 6500), 1.8, 'bp')) * env_pts(n, [(0, 0), (0.18, 1), (d, 0)], 1.4)
    sh = (sine(note('E7'), n) + 0.7 * sine(note('B7'), n)) * env_pts(n, [(0, 0), (0.2, 1), (d, 0)], 2.0)
    return carve_ui(0.7 * air + 0.4 * sh, 1500)


@sound('mix_whoosh_out', 'feedback', 1, pitch=P_TONAL, lufs=-28, notes='ring console close: soft falling air + faint shimmer')
def mix_whoosh_out(r, v):
    d = 0.25
    n = ns(d)
    air = normpk(tvf(pink(n, r), sweep(n, 6500, 2000), 1.8, 'bp')) * env_pts(n, [(0, 0), (0.04, 1), (d, 0)], 1.4)
    sh = (sine(note('B7'), n) + 0.7 * sine(note('E7'), n)) * env_pts(n, [(0, 0), (0.03, 1), (d, 0)], 2.0)
    return carve_ui(0.7 * air + 0.4 * sh, 1500)
