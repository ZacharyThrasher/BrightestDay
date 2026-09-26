"""Transformation set (robot <-> vehicle), original design: a staggered cascade of mechanical clicks,
servo whirs and panel clunks with a pitch trend (up = robot, down = vehicle), ending in a solid lock
and a short hydraulic hiss.  Plus combine, truck engine loop, air horn, heavy robot footsteps."""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from sounds import U, sound


def panel_click(r, k=1.0, amp=1.0):
    """Small armour panel snapping into place. k scales pitch."""
    n = ns(0.12)
    f = U(r, 1800, 2500) * k
    x = modal([f, f * 1.47, f * 2.31], [0.012, 0.009, 0.006], [1, .6, .35], n, r)
    x += 0.7 * lp(crack(r, 2500 * k, 0.001, d=n / SR)[:n], 6000 * k)
    x += 0.5 * thump(n / SR, 260 * k, 150 * k, 0.008, 0.012, drive=1.2)[:n]
    return amp * x


def clunk(r, k=1.0, amp=1.0):
    """Heavier plate / joint clunk."""
    n = ns(0.3)
    f = U(r, 380, 800) * k
    x = modal([f, f * 1.83, f * 2.7, f * 4.1], [0.05, 0.035, 0.02, 0.012], [1, .6, .4, .2], n, r)
    x += 0.8 * lp(crack(r, 1500 * k, 0.002, d=n / SR)[:n], 5000 * k)
    x += thump(n / SR, 170 * k, 80 * k, 0.015, 0.035, drive=1.8)[:n]
    return amp * x


def servo(r, d, f0, f1, amp=1.0):
    """Servo / actuator whir with gear grain, gliding f0 -> f1."""
    n = ns(d)
    f = sweep(n, f0, f1, 0.8)
    m = lp(saw(f, n) + 0.5 * square(f * 1.5, n, duty=0.3) + 0.3 * saw(f * 2.01, n), 3500)
    gear = 1 + 0.35 * np.sign(np.sin(TAU * np.cumsum(f / 6) / SR))  # gear-tooth chatter
    w = normpk(tvf(white(n, r), f * 6, 1.2, 'bp')) * 0.25  # gear noise tracks the motor pitch
    return amp * (normpk(m) * gear + w) * env_pts(n, [(0, 0), (0.015, 1), (d * 0.8, 0.8), (d, 0)])


def hiss(r, d=0.25, amp=1.0):
    n = ns(d)
    h = normpk(bp(white(n, r), 2500, 9000)) * (1 + 0.2 * smooth_noise(n, r, 40))
    return amp * h * env_exp(n, d / 3.5, 0.004)


def lock(r, k=1.0, amp=1.0):
    """The solid final lock: crack + low clunk + sub knock + short metal ring."""
    n = ns(0.8)
    x = np.zeros(n)
    add(x, sat(crack(r, 1800, 0.003, d=0.04), 2.5), 0, 1.0)
    add(x, clunk(r, 0.8 * k), 0, 1.0)
    add(x, thump(0.6, 120 * k, 48, 0.02, 0.1, drive=2.5), 0, 1.1)
    f = U(r, 900, 1200) * k
    add(x, modal([f, f * 2.76, f * 5.4], [0.25, 0.12, 0.06], [1, .5, .2], ns(0.8), r), 0, 0.2)
    return amp * x


def _cascade(r, d_casc, k0, k1, n_clicks, n_servos, n_clunks, density_pow=0.8, a1=1.0):
    """Staggered mechanical cascade; pitch factor slides k0 -> k1 over d_casc."""
    out = buf(d_casc + 0.3)
    kf = lambda t: k0 * (k1 / k0) ** min(1.0, t / d_casc)  # noqa: E731
    # clicks: accelerating in the middle, staggered pairs/triplets (original rhythm, randomised)
    times = np.sort(d_casc * r.uniform(0, 1, n_clicks) ** density_pow)
    for t in times:
        g = 1 + (a1 - 1) * t / d_casc  # click level trend (vehicle settles, robot builds)
        add(out, panel_click(r, kf(t), g * U(r, 0.35, 0.8)), t)
        if r.uniform() < 0.3:  # flam
            add(out, panel_click(r, kf(t) * 1.1, U(r, 0.2, 0.4)), t + U(r, 0.008, 0.02))
    for i in range(n_servos):
        t = U(r, 0, d_casc * 0.6)
        dd = U(r, 0.18, 0.35)
        base = U(r, 260, 420)
        add(out, servo(r, dd, base * kf(t), base * kf(t + dd) * (1.25 if k1 > k0 else 0.8)), t, 0.28)
    for i in range(n_clunks):
        t = d_casc * (i + U(r, 0.2, 0.8)) / n_clunks
        add(out, clunk(r, kf(t), U(r, 0.5, 0.8)), t)
    return out


def _take(render, r, want_up, tries=8):
    """Render takes until the cascade clearly carries the intended pitch contour (|slope| >= 0.6 oct/s
    of spectral centroid); keep the best take. Deterministic: consumes the same seeded rng."""
    best, best_s = None, None
    for _ in range(tries):
        x = render(r)
        s = pitch_trend(x) * (1 if want_up else -1)
        if best is None or s > best_s:
            best, best_s = x, s
        if s >= 0.6:
            break
    return best


@sound('transform_to_robot', 'transform', 3, volume=1.0, pitch=(0.97, 1.03),
       notes='~0.9 s rising mechanical cascade -> lock + hydraulic hiss')
def transform_to_robot(r, v):
    return _take(_to_robot, r, True)


def _to_robot(r):
    dc = 0.68
    out = _cascade(r, dc, 0.6, 2.0, int(U(r, 20, 28)), 3, 4, density_pow=1.1)
    out = fit(out, ns(1.4))
    add(out, servo(r, dc, U(r, 220, 280), U(r, 780, 900)), 0.01, 0.55)  # main actuator carries the rise
    add(out, lock(r, 1.1), dc + 0.02, 1.0)
    add(out, hiss(r, 0.25), dc + 0.06, 0.35)
    return reverb(out, r, 0.5, 0.14, bright=1.1)


@sound('transform_to_vehicle', 'transform', 3, volume=1.0, pitch=(0.97, 1.03),
       notes='~0.9 s falling mechanical cascade -> lock + hydraulic hiss')
def transform_to_vehicle(r, v):
    return _take(_to_vehicle, r, False)


def _to_vehicle(r):
    dc = 0.68
    out = _cascade(r, dc, 1.9, 0.6, int(U(r, 20, 28)), 3, 4, density_pow=0.9, a1=0.5)
    out = fit(out, ns(1.4))
    add(out, servo(r, dc, U(r, 700, 820), U(r, 200, 250)), 0.01, 0.55)  # main actuator carries the fall
    add(out, lock(r, 0.8), dc + 0.02, 1.0)
    add(out, hiss(r, 0.3), dc + 0.05, 0.4)
    return reverb(out, r, 0.5, 0.14, bright=0.9)


@sound('transform_panel_click', 'transform', 8, volume=0.6, pitch=(0.9, 1.15), lufs=-20,
       notes='single panel click; trigger per animated panel in sync')
def transform_panel_click(r, v):
    out = buf(0.12)
    add(out, panel_click(r, U(r, 0.85, 1.3)), 0, 1.0)
    if v % 3 == 2:
        add(out, panel_click(r, U(r, 1.0, 1.3), 0.4), U(r, 0.01, 0.02))
    return out


@sound('transform_combine', 'transform', 2, volume=1.0, pitch=(0.98, 1.02), lufs=-11, max_dur=3.2,
       notes='~2.2 s combiner transformation: huge layered cascade, final lock + sub boom')
def transform_combine(r, v):
    d = 2.4
    out = buf(d)
    dc = 1.75
    add(out, _cascade(r, dc, 0.7, 1.6, 60, 7, 8), 0)
    add(out, _cascade(r, dc, 0.5, 1.1, 20, 3, 5, density_pow=1.3), 0.05, 0.8)  # bigger, lower plates
    for i, t in enumerate([0.35, 0.8, 1.2, 1.5]):  # heavy joint slams, rising
        add(out, clunk(r, 0.5 + 0.12 * i), t, 1.0)
        add(out, thump(0.4, 110 + 15 * i, 50, 0.02, 0.08, drive=2.0), t, 0.8)
    n = ns(dc)
    riser = normpk(tvf(white(n, r), sweep(n, 300, 5000, 1.5), 2.0, 'bp')) * env_pts(n, [(0, 0), (dc, 1)], 2.0)
    add(out, riser, 0, 0.25)
    add(out, lock(r, 0.7, 1.2), dc, 1.0)
    add(out, thump(2.0, 75, 28, 0.08, 0.6, drive=3.0, attack=0.002), dc, 1.4)
    add(out, hiss(r, 0.5), dc + 0.05, 0.45)
    add(out, hiss(r, 0.35), dc + 0.22, 0.25)
    return reverb(out, r, 1.8, 0.28, bright=0.9)


@sound('vehicle_engine_loop', 'vehicle', 2, loop=True, volume=0.7, pitch=(1.0, 1.0), lufs=-18,
       notes='loop: diesel truck idle/cruise; the game pitches by speed (~0.8-2.2x)')
def vehicle_engine_loop(r, v):
    Ld = 2.0
    L, X = ns(Ld), ns(0.3)
    fire_hz = [45.0, 40.0][v % 2]           # firing frequency; fire_hz * Ld is an integer
    k = int(round(fire_hz * Ld))
    pulses = np.zeros(L)
    clat = np.zeros(L)
    for i in range(k):
        a = U(r, 0.75, 1.0) * (1.0 if i % 3 else 1.15)  # slightly uneven firing = diesel lope
        pos = int(round((i + 0.5) * L / k))  # keep firings off the loop seam
        pn = ns(0.03)
        p = white(pn, r) * env_exp(pn, 0.006, 0.001)
        add_circ(pulses, p, pos, a)
        add_circ(clat, crack(r, 1800, 0.0012, d=0.02), pos + ns(0.004), a * U(r, 0.5, 1.0))
    body = circ(lambda y: lp(y, 380, 4), pulses)
    exhaust = circ(lambda y: reson(y, 110, 2.5), pulses)

    def tonal(n):
        t = tvec(n)
        return (sine(fire_hz / 2, n) * 0.6 + sine(fire_hz, n) * 0.4
                + 0.04 * sine(2400 + 30 * np.sin(TAU * 0.5 * t), n))  # turbo whine
    ton = periodic(tonal, L)
    rum = loop_xfade(normpk(lp(brown(L + X, r), 150)), L, X)
    x = normpk(body) + 0.6 * normpk(exhaust) + 0.35 * ton + 0.12 * normpk(circ(lambda y: hp(y, 1500), clat)) + 0.3 * rum
    return sat(x, 1.5)


@sound('vehicle_horn', 'vehicle', 3, volume=0.9, pitch=(1.0, 1.0), lufs=-13,
       notes='big two-tone truck air horn (easter egg): v1 long, v2 double honk, v3 long low')
def vehicle_horn(r, v):
    pairs = [(233.1, 293.7), (233.1, 293.7), (185.0, 233.1)]  # Bb3+D4, Bb3+D4, F#3+Bb3 (major thirds)
    fa, fb = pairs[v]
    blasts = [(0.0, 1.1)] if v != 1 else [(0.0, 0.32), (0.42, 0.75)]
    out = buf(2.0)
    for (t0, dur) in blasts:
        n = ns(dur)
        t = tvec(n)
        tone = np.zeros(n)
        for f, a in ((fa, 1.0), (fb, 0.85)):
            fr = f * (1 - 0.05 * np.exp(-t / 0.05)) * (1 + 0.002 * np.sin(TAU * 5.5 * t))  # air-pressure scoop
            tone += a * (square(fr, n, duty=0.3) + 0.6 * saw(fr * 1.002, n))
        horn = reson(tone, 600, 2.0) + 0.7 * reson(tone, 1500, 3.0) + 0.3 * lp(tone, 400)
        horn = sat(normpk(horn) * 1.8, 2.0)
        horn *= env_pts(n, [(0, 0), (0.035, 1), (dur - 0.12, 0.95), (dur, 0)])
        add(out, horn, t0, 1.0)
        add(out, hiss(r, 0.15), t0, 0.1)          # air valve
    return reverb(out, r, 1.2, 0.2)


@sound('robot_footstep_heavy', 'movement', 4, volume=0.9, pitch=(0.92, 1.06), lufs=-14,
       notes='giant robot stomp: servo knee + metal foot + ground thud')
def robot_footstep_heavy(r, v):
    out = buf(1.0)
    add(out, servo(r, 0.12, U(r, 300, 380), U(r, 200, 260)), 0, 0.25)
    add(out, hiss(r, 0.08), 0.02, 0.15)
    t = 0.09
    add(out, sat(crack(r, 1500, 0.003, d=0.04), 2.0), t, 0.7)
    add(out, thump(0.8, 95 * U(r, .92, 1.08), 40, 0.04, 0.16, drive=2.8), t, 1.3)
    f = U(r, 300, 450)
    add(out, modal([f, f * 1.9, f * 2.8, f * 4.3], [0.18, 0.12, 0.07, 0.04], [1, .6, .4, .2], ns(0.8), r), t, 0.35)
    for _ in range(10):
        add(out, grain(r, U(r, .003, .01), 900, 4000), t + r.exponential(0.03), 0.2)
    return reverb(out, r, 0.6, 0.18, bright=0.8)
