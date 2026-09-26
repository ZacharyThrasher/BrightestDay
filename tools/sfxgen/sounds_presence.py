"""PRESENCE: the construct presence grammar's sounds (bd_game present::presence).

Contrast, not volume: Tiny tiers get nothing extra; Normal a small bright "shing"; Big a gathering
swell + a hard-light hit; Titan a sub-bass rise that implodes into a deep drop + chord bloom. Every
construct can carry a quiet element hum loop (spatial, pitched by size in the game) so a big thing in
the world is HEARD before it is seen — each element with its own timbre, all in E minor.
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, grit, hl_chord, hl_ping, hl_shards, hl_tone, micro, note, pent, pent_index
from sounds import P_SOFT, P_T10, P_TONAL, U, sound


# =============================================================================== gather / manifest
@sound('presence_shing', 'construct', 4, pitch=P_T10, lufs=-24, max_dur=0.5,
       notes='Normal-tier manifest: crisp hard-light "shing" (upward FM glint into a ringing E6/B6)')
def presence_shing(r, v):
    d = 0.5
    out = buf(d)
    n = ns(0.09)
    f = sweep(n, note(['E6', 'B5', 'G6', 'E6'][v]), note(['B7', 'E7', 'D8', 'E8'][v]), 0.6)
    add(out, fm(f, 1.0, 1.6 * env_exp(n, 0.03), n) * env_pts(n, [(0, 0), (0.01, 1), (0.09, 0)]), 0, 0.35)
    top = [['E6', 'B6'], ['B5', 'E6'], ['G6', 'D7'], ['E6', 'E7']][v]
    for i, nm in enumerate(top):
        add(out, hl_ping(r, note(nm), 0.13, 1.25, glint=0.9, shimmer=0.7), 0.035 + 0.012 * i, 0.55 / (1 + i * 0.4))
    add(out, click(r, 6000, 0.0015), 0.035, 0.25)
    return carve_ui(out, 1400)


@sound('presence_gather_big', 'construct', 2, pitch=P_TONAL, lufs=-22, max_dur=1.0,
       notes='Big-tier gather: ~0.9 s swell pulled INTO the hands (reverse chord + inward pings), ends on the peak')
def presence_gather_big(r, v):
    d = 0.9
    n = ns(d)
    u = np.linspace(0, 1, n)
    out = buf(d)
    env = u ** 2.4
    for nm, a in ((['E3', 'B2'][v], 0.8), ('B3', 0.5), ('E4', 0.45), (['G4', 'D4'][v], 0.35)):
        add(out, hl_tone(r, note(nm), n, env, bright=0.9, shimmer=0.8), 0, a)
    # low weight rising a fifth
    f = note('E2') * 1.5 ** (u ** 2)
    add(out, sine(f, n) * u ** 2.0, 0, 0.35)
    t, j, k0 = 0.05, 0, pent_index('E7')
    while t < d - 0.03:  # inward pings: high and sparse → low and dense
        add(out, hl_ping(r, pent(k0 - j % 10), 0.03, 1.2, glint=0.4), t, 0.05 + 0.2 * t / d)
        t += 0.11 * (1 - 0.8 * t / d)
        j += 1
    out[n - ns(0.008):n] *= np.linspace(1, 0, ns(0.008))
    out[n:] = 0
    return out


@sound('presence_gather_titan', 'construct', 2, pitch=P_TONAL, lufs=-19, max_dur=1.7,
       notes='Titan-tier gather: 1.6 s sub rise (E1 -> E2) + air pulled inward + chord crescendo, cut on the peak')
def presence_gather_titan(r, v):
    d = 1.6
    n = ns(d)
    u = np.linspace(0, 1, n)
    out = buf(d)
    f = note('E1') * 2 ** (u ** 1.8)
    add(out, sat(sine(f, n) + 0.4 * sine(2 * f, n), 1.6) * u ** 1.6, 0, 0.9)
    # air rushing in: a narrowing, rising band of pink noise (subordinate)
    air = normpk(tvf(pink(n, r), sweep(n, 300, 2400, 2.0), 2.2, 'bp')) * u ** 3
    add(out, air, 0, 0.18)
    env = u ** 3
    for nm, a in (('E3', 0.7), ('B3', 0.55), ('E4', 0.5), (['G4', 'B4'][v], 0.4), ('E5', 0.3)):
        add(out, hl_tone(r, note(nm), n, env, bright=1.0, shimmer=0.9), 0, a)
    for _ in range(60):  # crackle converging
        tt = d * r.uniform() ** 0.4
        add(out, micro(r, U(r, 2500, 7000), 0.002), tt, 0.25 * (tt / d) ** 2)
    out[n - ns(0.01):n] *= np.linspace(1, 0, ns(0.01))
    out[n:] = 0
    return hp(out, 25, 2)


@sound('presence_manifest_big', 'combat', 3, pitch=P_SOFT, lufs=-17,
       notes='Big-tier manifest: hard-light chord hit + E2 thump + short shard shimmer')
def presence_manifest_big(r, v):
    d = 1.3
    out = buf(d)
    add(out, click(r, 3000, 0.003), 0, 0.5)
    ch = [['E4', 'B4', 'E5', 'G5'], ['E4', 'G4', 'B4', 'E5'], ['B3', 'E4', 'B4', 'D5']][v]
    add(out, hl_chord(r, ch, dec=0.35, bright=1.1, stagger=0.003), 0, 0.75)
    add(out, thump(0.9, note('E3'), note('E2'), 0.04, 0.16, drive=2.0), 0, 0.9)
    add(out, hl_shards(r, 5, 0.6, 'E8', 'E6', amp=0.35), 0.02, 0.6)
    return carve_heavy(glue(out, 1.5), -4, -3)


@sound('presence_manifest_titan', 'combat', 2, pitch=P_SOFT, lufs=-12,
       notes='Titan-tier manifest: implosion snap, deep E1 drop, huge chord bloom, long falling shard rain')
def presence_manifest_titan(r, v):
    d = 3.0
    n = ns(d)
    out = buf(d)
    # the implosion's last instant: a reversed swell, then the snap
    m = ns(0.12)
    add(out, normpk(tvf(pink(m, r), sweep(m, 800, 5000), 2.0, 'bp')) * np.linspace(0, 1, m) ** 3, 0, 0.3)
    add(out, sat(click(r, 2500, 0.006), 2.5), 0.12, 0.9)
    add(out, thump(2.4, note('E2'), note('E1'), 0.09, 0.55, drive=3.0), 0.12, 1.4)
    nb = ns(2.6)
    env = env_pts(nb, [(0, 0), (0.02, 1), (0.6, 0.55), (2.6, 0)], 1.6)
    ch = [('E3', 0.6), ('B3', 0.5), ('E4', 0.5), ('G4', 0.4), ('B4', 0.35), ('E5', 0.3)]
    if v:
        ch[3] = ('D4', 0.4)
    for nm, a in ch:
        add(out, hl_tone(r, note(nm), nb, env, bright=1.1, shimmer=0.8), 0.12, a * 0.5)
    add(out, hl_shards(r, 14, 2.2, 'E8', 'E5', amp=0.6), 0.14, 0.8)
    rum = normpk(lp(brown(n, r), 90)) * env_exp(n, 0.7, 0.05)
    add(out, rum, 0.12, 0.35)
    return carve_heavy(glue(out, 1.8))


# =============================================================================== element hums (loops)
LD = 3.0  # loop length (s)


def _circ_noise(r, L, render):
    """A seamless loop of a noise texture: render L + X, crossfade the tail over the head."""
    X = ns(0.25)
    return loop_xfade(render(L + X), L, X)


def _ticks(r, x, count, lo, hi, dec=0.002, amp=0.3):
    for _ in range(count):
        add_circ(x, micro(r, U(r, lo, hi), dec), int(r.uniform(0, len(x))), U(r, 0.3, 1.0) * amp)


def _drone(r, names, L, **kw):
    return normpk(hl_drone_p(r, [note(nm) for nm in names], L, LD, **kw))


def hl_drone_p(r, freqs, n, period, bright=1.0, shimmer=0.5, lfo=0.3, amps=None):
    from sounds import hl_drone
    return hl_drone(r, freqs, n, period, bright=bright, shimmer=shimmer, lfo=lfo, amps=amps)


def _hum(name, notes):
    return sound(name, 'construct', 1, loop=True, pitch=P_TONAL, lufs=-28, notes=notes)


@_hum('presence_hum_light', 'loop; hard light at rest: soft E drone with a slow shimmer')
def presence_hum_light(r, v):
    L = ns(LD)
    return _drone(r, ['E2', 'B2', 'E3', 'B3'], L, bright=0.7, shimmer=0.7, lfo=0.25, amps=[1.0, 0.6, 0.5, 0.3])


@_hum('presence_hum_fire', 'loop; fire: a low roar (resonant brown noise, grained) + crackle + an E drone under it')
def presence_hum_fire(r, v):
    L = ns(LD)

    def roar(m):
        y = normpk(tvf(brown(m, r), 220 + 140 * smooth_noise(m, r, 3.0), 1.4, 'lp'))
        return y * (0.75 + 0.35 * smooth_noise(m, r, 9.0))
    x = normpk(_circ_noise(r, L, roar))
    x = x + 0.35 * _drone(r, ['E2', 'B2'], L, bright=0.6, shimmer=0.6, lfo=0.4)
    _ticks(r, x, 70, 1800, 6000, 0.0025, 0.35)  # crackle
    return hp(x, 35, 2)


@_hum('presence_hum_ice', 'loop; ice: glassy high shimmer (fast-beating E5/B5/E6) + faint tinks over a cold E2')
def presence_hum_ice(r, v):
    L = ns(LD)
    x = _drone(r, ['E5', 'B5', 'E6'], L, bright=1.2, shimmer=1.0, lfo=0.6, amps=[1.0, 0.7, 0.5])
    x = 0.6 * x + 0.3 * _drone(r, ['E2'], L, bright=0.4, shimmer=0.3, lfo=0.2)
    for i in range(9):
        add_circ(x, hl_ping(r, pent(pent_index('E7') + int(r.integers(0, 6))), 0.08, 1.3, glint=0.3), int(r.uniform(0, L)), 0.08)
    return x


@_hum('presence_hum_electric', 'loop; electric: an in-key mains buzz (E2 saw, fluttering) + crackle bursts')
def presence_hum_electric(r, v):
    L = ns(LD)
    t = tvec(L)
    f = qf(note('E2'), LD)
    buzz = saw(f, L) + 0.5 * saw(2 * f, L)
    buzz = lp(buzz, 1800) * (0.8 + 0.2 * np.sin(TAU * qf(11.0, LD) * t))
    x = normpk(periodic(lambda m: lp(np.tile(buzz, 2)[:m], 2400), L))
    for _ in range(14):  # crackle bursts (clustered)
        c = int(r.uniform(0, L))
        for _k in range(int(r.integers(4, 10))):
            add_circ(x, micro(r, U(r, 2500, 8000), 0.0015, 3, (1.4, 3.1)), c + int(r.uniform(0, ns(0.05))), U(r, 0.2, 0.5))
    return x


@_hum('presence_hum_water', 'loop; water: flowing resonant noise + rising bubble blips')
def presence_hum_water(r, v):
    L = ns(LD)

    def flow(m):
        return normpk(tvf(pink(m, r), 500 + 350 * smooth_noise(m, r, 1.5), 3.0, 'bp')) * (0.7 + 0.3 * smooth_noise(m, r, 4.0))
    x = 0.8 * normpk(_circ_noise(r, L, flow))
    for _ in range(26):  # bubbles: short upward sine blips in the E pentatonic
        m = ns(U(r, 0.03, 0.07))
        f0 = pent(pent_index('E4') + int(r.integers(0, 8)))
        b = sine(sweep(m, f0, f0 * 1.6), m) * env_pts(m, [(0, 0), (0.005, 1), (m / SR, 0)])
        add_circ(x, b, int(r.uniform(0, L)), U(r, 0.15, 0.35))
    return x + 0.2 * _drone(r, ['E2', 'B2'], L, bright=0.5, shimmer=0.5)


@_hum('presence_hum_wind', 'loop; wind: a streaming resonant whoosh + a soft whistle on B5')
def presence_hum_wind(r, v):
    L = ns(LD)

    def air(m):
        return normpk(tvf(pink(m, r), 700 + 500 * smooth_noise(m, r, 0.8), 2.2, 'bp')) * (0.6 + 0.4 * smooth_noise(m, r, 1.2))
    x = normpk(_circ_noise(r, L, air))
    return x + 0.12 * _drone(r, ['B5', 'E6'], L, bright=0.3, shimmer=1.0, lfo=0.8)


@_hum('presence_hum_earth', 'loop; earth: grinding low rumble + stone grit ticks over an E1 drone')
def presence_hum_earth(r, v):
    L = ns(LD)

    def rumble(m):
        return normpk(lp(brown(m, r), 130)) * (0.7 + 0.3 * smooth_noise(m, r, 2.0))
    x = normpk(_circ_noise(r, L, rumble)) + 0.35 * _drone(r, ['E1', 'B1', 'E2'], L, bright=0.4, shimmer=0.4)
    _ticks(r, x, 60, 600, 2600, 0.004, 0.3)
    return hp(x, 25, 2)


@_hum('presence_hum_psychic', 'loop; psychic: a phasing, slowly beating drone with a wide vibrato')
def presence_hum_psychic(r, v):
    L = ns(LD)
    x = _drone(r, ['E3', 'B3', 'E4', 'G4'], L, bright=0.9, shimmer=1.0, lfo=0.9, amps=[1.0, 0.7, 0.5, 0.35])
    t = tvec(L)
    x = x * (0.75 + 0.25 * np.sin(TAU * qf(0.67, LD) * t))
    return x + 0.25 * _drone(r, ['E2'], L, bright=0.5, shimmer=0.8, lfo=0.5)


@_hum('presence_hum_shadow', 'loop; shadow: a void — low E1/B1 swallowing drone, slow sub pulse, a faint high whisper')
def presence_hum_shadow(r, v):
    L = ns(LD)
    t = tvec(L)
    x = _drone(r, ['E1', 'B1', 'E2'], L, bright=0.35, shimmer=0.6, lfo=0.3)
    x = lp(np.tile(x, 2), 600)[L:]
    x = normpk(x) * (0.7 + 0.3 * np.sin(TAU * qf(0.67, LD) * t))

    def whisper(m):
        return normpk(tvf(pink(m, r), 3200 + 800 * smooth_noise(m, r, 0.6), 4.0, 'bp')) * smooth_noise(m, r, 0.8).clip(0, 1)
    return x + 0.06 * normpk(_circ_noise(r, L, whisper))


@_hum('presence_hum_poison', 'loop; poison: slow thick bubbling + a sour, beating drone')
def presence_hum_poison(r, v):
    L = ns(LD)
    x = _drone(r, ['E2', 'B2', 'D3'], L, bright=0.6, shimmer=1.2, lfo=0.5)
    for _ in range(18):
        m = ns(U(r, 0.06, 0.12))
        f0 = pent(pent_index('E3') + int(r.integers(0, 5)))
        b = sine(sweep(m, f0, f0 * 1.35), m) * env_pts(m, [(0, 0), (0.01, 1), (m / SR, 0)])
        add_circ(x, b, int(r.uniform(0, L)), U(r, 0.25, 0.5))
    return x


@_hum('presence_hum_atomic', 'loop; atomic: a throbbing radiant E drone (6 Hz) + geiger ticks')
def presence_hum_atomic(r, v):
    L = ns(LD)
    t = tvec(L)
    x = _drone(r, ['E2', 'B2', 'E3', 'B3'], L, bright=1.0, shimmer=0.7, lfo=0.3)
    x = x * (0.7 + 0.3 * np.sin(TAU * qf(6.0, LD) * t))
    _ticks(r, x, 45, 3000, 7000, 0.0008, 0.5)
    return x
