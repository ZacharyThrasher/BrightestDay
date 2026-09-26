"""Ring charge (power-up aura) and shout (typed word -> construct) sounds, from the E-minor hard-light family.

Dry, carved by role, deterministic. `charge_loop` is played with a rising playback speed (1.0 -> ~1.5x,
i.e. E -> B, a consonant fifth) and rising gain as the charge fills, so it contains no fixed-rate noise:
its texture is partials, a periodic pentatonic sparkle and irregular modal crackle grains, which just get
brighter/denser when sped up.
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, hl_chord, hl_ping, hl_shards, micro, note, pent, pent_index
from sounds import P_T10, P_TONAL, U, hl_drone, sound


# =============================================================================== charge
@sound('charge_loop', 'construct', 1, loop=True, pitch=P_TONAL, lufs=-24,
       notes='loop; power-up aura: E drone + rising pentatonic shimmer + crackle grains; '
             'game raises speed 1.0->1.5x (E->B) and gain with charge')
def charge_loop(r, v):
    Ld = 2.0
    L = ns(Ld)
    t = tvec(L)
    drone = hl_drone(r, [note(x) for x in ('E2', 'B2', 'E3', 'B3', 'E4')], L, Ld, bright=0.9, shimmer=0.7, lfo=0.3,
                     amps=[0.9, 0.6, 0.7, 0.45, 0.35])
    pulse = 0.8 + 0.2 * np.sin(TAU * 4.0 * t)  # aura throb (8 cycles per loop)
    x = normpk(drone) * pulse
    # rising shimmer: four ascending pentatonic runs per loop (circular, so it rises forever)
    k0 = pent_index('E5')
    for run in range(4):
        t0 = run * Ld / 4
        for j in range(8):
            f = pent(k0 + j + (run % 2))
            p = hl_ping(r, f, 0.06, 1.2, glint=0.5, shimmer=0.3)
            add_circ(x, p, int((t0 + 0.055 * j) * SR), 0.05 + 0.012 * j)
    # crackling energy: irregular tiny modal grains (no fixed rate)
    for _ in range(70):
        add_circ(x, micro(r, U(r, 2500, 7500), U(r, 0.0015, 0.004), 3, (1.3, 2.2)), int(r.uniform(0, L)),
                 U(r, 0.03, 0.12))
    return circ(lambda y: hp(y, 60, 2), x)


@sound('charge_full', 'construct', 2, pitch=P_T10, lufs=-15, max_dur=1.2, notes='pool full: bright Em chord flash + E sub whump + sparkle')
def charge_full(r, v):
    out = buf(1.1)
    add(out, sat(click(r, 3000, 0.003), 2.0), 0, 0.7)
    ch = [['E5', 'B5', 'E6', 'G6', 'B6'], ['G5', 'B5', 'E6', 'B6', 'E7']][v]
    add(out, hl_chord(r, ch, dec=0.35, bright=1.35, stagger=0.003), 0, 0.8)
    add(out, thump(0.8, note('E3'), note('E1'), 0.04, 0.16, drive=2.4), 0, 1.0)
    k0 = pent_index('E6')
    for j in range(7):  # upward sparkle burst
        add(out, hl_ping(r, pent(k0 + j), 0.05, 1.3, glint=0.8), 0.02 + 0.03 * j, 0.25)
    add(out, hl_shards(r, 5, 0.6, 'E8', 'E6', amp=0.35), 0.1, 1.0)
    return out


@sound('charge_break', 'construct', 3, pitch=P_T10, lufs=-18, max_dur=0.7,
       notes='charge interrupted: aura sputters out - falling, detuning hard light + crackle')
def charge_break(r, v):
    d = 0.6
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, sat(click(r, 2200, 0.004), 2.0), 0, 0.7)
    fall = 2 ** (-[5, 7, 4][v] / 12 * (t / d) ** 1.4)  # sags a 4th/5th/3rd
    sput = np.clip(0.55 + 0.6 * smooth_noise(n, r, 22), 0, 1) * env_pts(n, [(0, 1), (d * 0.5, 0.6), (d, 0)], 1.5)
    x = np.zeros(n)
    for nm, a in (('E3', 1.0), ('B3', 0.7), ('E4', 0.6), ('G4', 0.4)):
        det = 2 ** (U(r, -40, 40) / 1200 * t / d)  # partials drift out of tune as it dies
        f = note(nm) * fall * det
        x += a * (sine(f, n, r.uniform(0, TAU)) + 0.4 * sine(2 * f, n, r.uniform(0, TAU)) + 0.2 * sine(3 * f, n))
    add(out, normpk(x) * sput, 0, 0.8)
    for _ in range(int(U(r, 18, 30))):  # crackle, thinning out
        tt = d * r.uniform() ** 1.6
        add(out, micro(r, U(r, 2000, 6500) * (1 - 0.4 * tt / d), U(r, 0.002, 0.005)), tt, U(r, 0.1, 0.35) * (1 - tt / d))
    add(out, hl_shards(r, 3, 0.35, 'E7', 'E5', amp=0.3), 0.01, 1.0)
    return hp(out, 80, 2)


@sound('charge_release', 'construct', 3, pitch=P_T10, lufs=-19, max_dur=0.9,
       notes='charge let go calmly: soft exhale, the aura settling down onto E')
def charge_release(r, v):
    d = 0.75
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    settle = 1 + [0.03, 0.05, 0.02][v] * np.exp(-t / 0.12)  # glides down onto the note
    x = np.zeros(n)
    for nm, a in (('E3', 0.8), ('B3', 0.7), ('E4', 1.0), ('G4', 0.5), ('B4', 0.35)):
        f = note(nm) * settle
        x += a * (sine(f, n, r.uniform(0, TAU)) + 0.35 * sine(2 * f, n) + 0.15 * sine(3 * f, n))
    x *= env_pts(n, [(0, 0), (0.03, 1), (0.25, 0.6), (d, 0)], 1.6)
    add(out, normpk(x), 0, 0.8)
    breath = swish_exhale(r, d)
    add(out, breath, 0, 0.18)
    add(out, hl_ping(r, note(['E6', 'B5', 'G6'][v]), 0.25, 1.0, glint=0.4), 0.02, 0.2)
    return hp(out, 100, 2)


def swish_exhale(r, d):
    n = ns(d)
    y = normpk(tvf(pink(n, r), sweep(n, 3000, 700, 0.6), 2.0, 'bp')) * (1 + 0.3 * smooth_noise(n, r, 40))
    return y * env_pts(n, [(0, 0), (0.05, 1), (d, 0)], 1.5)


# =============================================================================== shout (typed word -> construct)
@sound('shout_erupt', 'construct', 3, pitch=P_T10, lufs=-21, notes='letters leaving the ring: quick rising hard-light whoosh')
def shout_erupt(r, v):
    d = 0.32
    n = ns(d)
    out = buf(d)
    k0 = pent_index(['E5', 'G5', 'B4'][v])
    for j in range(9):  # fast rising run
        add(out, hl_ping(r, pent(k0 + j), 0.04, 1.2, glint=0.6), 0.2 * (j / 9) ** 0.9, 0.18 + 0.05 * j)
    f = sweep(n, note('E4'), note('E6'), 0.8)  # the gliss carries the rise, lands on E
    add(out, (sine(f, n) + 0.3 * sine(2 * f, n)) * env_pts(n, [(0, 0), (0.05, 0.6), (0.22, 1), (d, 0)], 1.5), 0, 0.35)
    air = normpk(tvf(pink(n, r), sweep(n, 1200, 7000, 0.8), 2.0, 'bp')) * env_pts(n, [(0, 0), (0.2, 1), (d, 0)], 1.5)
    add(out, air, 0, 0.12)
    return hp(out, 300, 4)


@sound('shout_slam', 'combat', 3, pitch=P_T10, lufs=-16,
       notes='headline word lands: hard-light letter slam - crack + comic bonk + low thump')
def shout_slam(r, v):
    out = buf(0.9)
    add(out, sat(click(r, 2500, 0.004), 2.5), 0, 0.9)
    ch = [['E5', 'B5', 'E6'], ['G5', 'B5', 'E6'], ['E5', 'G5', 'B5']][v]
    add(out, hl_chord(r, ch, dec=0.14, bright=1.25, stagger=0.002), 0, 0.55)
    nb = ns(0.25)
    add(out, fm(glide(nb, 420, 160, 0.03), 1.0, 2 * env_exp(nb, 0.02), nb) * env_exp(nb, 0.04), 0, 0.45)  # comic bonk
    add(out, thump(0.5, 150 * [1.0, 0.9, 1.1][v], 45, 0.03, 0.1, drive=2.8), 0, 1.1)
    add(out, hl_shards(r, 3, 0.2, 'E8', 'E6', amp=0.25), 0.01, 1.0)
    return glue(peq(out, 3200, -3, 0.8), 1.4)


@sound('shout_release', 'construct', 3, pitch=P_T10, lufs=-22,
       notes='letters shatter into cubes and stream into the construct: shimmering in-key granular rush')
def shout_release(r, v):
    d = 0.9
    out = buf(d)
    k_lo, k_hi = pent_index('E6'), pent_index(['E8', 'B7', 'D8'][v])
    for _ in range(int(U(r, 90, 130))):
        u = r.beta(2.0, 2.6)  # density swells then streams away
        tt = d * u
        k = int(round(k_hi - (k_hi - k_lo) * u + U(r, -1.5, 1.5)))  # the stream drifts down into the construct
        add(out, hl_ping(r, pent(k), U(r, 0.02, 0.05), 1.2, glint=0.4, shimmer=0.2), tt, U(r, 0.15, 0.4))
    n = ns(d)
    air = normpk(tvf(pink(n, r), sweep(n, 6000, 2000, 0.8), 2.2, 'bp')) * env_pts(n, [(0, 0), (0.3, 1), (d, 0)], 1.5)
    add(out, air, 0, 0.08)
    return hp(out, 400, 4)


@sound('shout_fizzle', 'construct', 2, pitch=P_T10, lufs=-24,
       notes='failed cast: letters crumble - dull falling ticks, slightly out of tune')
def shout_fizzle(r, v):
    d = 0.6
    n = ns(d)
    out = buf(d)
    k0 = pent_index('B5')
    for j in range(int(U(r, 9, 13))):
        tt = 0.04 * j + U(r, 0, 0.015)
        f = pent(k0 - j // 2) * 2 ** (U(r, 25, 60) * (1 if r.uniform() < 0.5 else -1) / 1200)  # off-key
        tick = lpf(modal([f, f * 2.3], [0.012, 0.006], [1, 0.4], ns(0.08), r), 2500, 2)
        add(out, tick, tt, 0.8 * (1 - 0.05 * j))
    f = sweep(n, note('B4') * 1.01, note('E4') * 0.98, 0.7)
    add(out, sine(f, n) * env_pts(n, [(0, 0), (0.03, 1), (d, 0)], 2.0), 0, 0.25)
    return carve_foley(out, 200, 6000)
