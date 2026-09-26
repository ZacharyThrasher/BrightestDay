"""Hyperline (the endless train) - the speed you hear.

Beds (STEREO, seamless loops, non-spatial; bd_audio maps.ron "hyperline"):
  amb_train_rush   - the roaring slipstream: turbulent band-passed air with fast gust grain, a
                     low body rumble and a subordinate high hiss; loud but soft-edged.
  amb_rail_clatter - rhythmic double clacks of the bogies over rail joints (a tight, fast groove),
                     a maglev whine on E (in key with the hard-light family) and a guideway hum.
One-shots (MONO, positional):
  train_ring_whoosh - a ring gate sweeping over the train: a huge doppler air-rip with a falling
                      metallic hum (the client fires it as each ring passes the camera).
  amb_city_flyby    - a tower / scaffold streaking past close: short doppler swish + a passing hum.

Noise only where the source is air (pink/brown, band-shaped, resonant, amplitude-grained); the clacks
are modal metal hits; tonal parts are in E.
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import material_hit, note
from sounds import P_SOFT, P_TONAL, U, sound


def _circ_st(fn, x):
    return st(circ(fn, x[:, 0]), circ(fn, x[:, 1]))


@sound('amb_train_rush', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-27,
       notes='stereo bed: roaring slipstream on a high-speed train roof - turbulent air + body rumble + soft hiss')
def amb_train_rush(r, v):
    Ld = 12.0
    L, X = ns(Ld), ns(1.0)
    n = L + X
    common = smooth_noise(n, r, 0.5)
    chs = []
    for c in range(2):
        gust = np.clip(0.6 * common + 0.4 * smooth_noise(n, r, 1.7), -1, 1)
        g01 = 0.5 + 0.5 * gust
        # Buffeting grain: fast amplitude flutter of the air mass.
        grain_env = 0.75 + 0.25 * smooth_noise(n, r, 9.0)
        body = normpk(tvf(brown(n, r) + 0.8 * pink(n, r), 380 * 2 ** (0.7 * gust), 1.1, 'bp'))
        roar = normpk(reson(pink(n, r), 190 + 25 * c, 1.4) + 0.6 * reson(pink(n, r), 620, 1.8))
        hiss = normpk(bp(pink(n, r), 2600, 7000)) * g01 ** 1.5
        rumble = normpk(lp(brown(n, r), 110, 4))
        x = grain_env * (0.8 * body + 0.6 * roar) + 0.07 * hiss + 0.55 * rumble * (0.7 + 0.3 * g01)
        chs.append(loop_xfade(x, L, X))
    return _circ_st(lambda y: hp(y, 32, 2), st(*chs))


@sound('amb_rail_clatter', 'ambience', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-30,
       notes='stereo bed: fast double clacks over rail joints (every 0.5 s), maglev whine on E, guideway hum')
def amb_rail_clatter(r, v):
    Ld = 8.0
    L = ns(Ld)
    period = 0.5  # one joint pair per half second (16 per loop): a tight, driving groove
    out = np.zeros((L, 2))
    k = 0
    t = 0.0
    while t < Ld - 1e-6:
        for j, dt in enumerate((0.0, 0.085)):  # front and rear bogie: "clack-clack"
            e = 1.0 if j == 0 else 0.8
            for c in range(2):
                hit = material_hit(r, 'metal', U(r, 0.35, 0.55), e * U(r, 0.8, 1.0))
                hit = lp(hit, 5200, 2)
                pan = 1.0 if (k + c) % 2 == 0 else 0.7
                add_circ(out[:, c], hit, ns(t + dt + U(r, -0.002, 0.002)) % L, 0.5 * pan)
        # A soft low thud under each pair (the car body taking the joint).
        th = thump(0.12, 90, 60, 0.03, 0.05, drive=1.2)
        for c in range(2):
            add_circ(out[:, c], th, ns(t) % L, 0.35)
        k += 1
        t += period
    # Maglev whine on E (E4 + E5 shimmer, detuned per channel) and a guideway hum on E2.
    def tone(m):
        tt = tvec(m)
        return (0.12 * sine(qf(note('E4'), Ld), m) + 0.05 * sine(qf(note('E5') * 1.002, Ld), m)
                + 0.18 * sine(qf(note('E2'), Ld), m) * (0.8 + 0.2 * np.sin(TAU * tt / 2.0)))
    tn = periodic(tone, L)
    for c in range(2):
        out[:, c] = normpk(out[:, c]) + tn
    return _circ_st(lambda y: hp(y, 40, 2), out)


@sound('train_ring_whoosh', 'ambience', 3, pitch=P_SOFT, lufs=-21, max_dur=2.2,
       notes='a ring gate sweeping over the train at speed: doppler air-rip + falling metallic hum, ~1.6 s')
def train_ring_whoosh(r, v):
    d = [1.6, 1.4, 1.8][v]
    n = ns(d)
    t = tvec(n)
    tc = d * 0.42  # closest approach
    speed = 150.0
    D = [9.0, 12.0, 7.0][v]
    xpos = speed * (t - tc)
    dist = np.sqrt(xpos ** 2 + D ** 2)
    vr = speed * xpos / dist
    dop = 343.0 / (343.0 + vr)
    amp = D / dist
    air = tvf(pink(n, r) + 0.6 * brown(n, r), 500 + 2600 * amp ** 2, 0.9, 'bp')
    rip = tvf(pink(n, r), 1800 + 3000 * amp, 2.5, 'bp') * amp ** 2
    hum = np.zeros(n)
    f0 = note('E3')
    for kk, g in ((1, 1.0), (2, 0.55), (3, 0.3), (4.02, 0.2)):
        hum += g * sine(f0 * kk * dop, n, r.uniform(0, TAU))
    x = normpk(air) * amp ** 1.3 + 0.35 * normpk(rip) + 0.3 * normpk(hum) * amp ** 1.6
    x = tvf(x, 1500 + 7000 * amp ** 2, 0.7, 'lp')
    add(x, 0.45 * thump(0.35, 70, 45, 0.08, 0.12, drive=1.3), tc - 0.06)  # the ring's mass going over
    x *= env_pts(n, [(0, 0), (0.1, 1), (d - 0.2, 1), (d, 0)])
    return hp(x, 45, 2)


@sound('amb_city_flyby', 'ambience', 4, pitch=P_SOFT, lufs=-29,
       notes='a tower / scaffold streaking past close: short doppler swish + passing hum, 0.6-1 s')
def amb_city_flyby(r, v):
    d = [0.8, 0.6, 1.0, 0.7][v]
    n = ns(d)
    t = tvec(n)
    tc = d * 0.45
    speed = [120, 140, 100, 160][v]
    D = [6, 5, 9, 4][v] * U(r, 0.9, 1.1)
    xpos = speed * (t - tc)
    dist = np.sqrt(xpos ** 2 + D ** 2)
    vr = speed * xpos / dist
    dop = 343.0 / (343.0 + vr)
    amp = D / dist
    air = tvf(pink(n, r), 700 + 3500 * amp ** 2, 1.2, 'bp')
    f0 = [note('B2'), note('E3'), note('A2'), note('G3')][v]
    hum = sine(f0 * dop, n) + 0.4 * sine(2 * f0 * dop, n)
    x = normpk(air) * amp ** 1.4 + 0.18 * normpk(hum) * amp ** 2
    x *= env_pts(n, [(0, 0), (0.05, 1), (d - 0.1, 1), (d, 0)])
    return hp(x, 60, 2)
