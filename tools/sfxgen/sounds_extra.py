"""Gameplay cues pass 3: construct assembly, hatches, rotors, mass footfalls, air moves, hook tether,
reveal/alerts, domain, ultimate, orbital strike, misc UI/foley.

Same rules as everything else: dry (no room), no white hiss, carved by role, E-minor hard-light family
for anything made of light, micro pitch ranges, deterministic.
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, grit, hl_chord, hl_ping, hl_shards, hl_tone, material_hit, micro, note, pent, pent_index, shatter
from sounds import (P_IMPACT, P_MICRO, P_SOFT, P_T10, P_TONAL, U, _bloom_env, _cube_tinks, _pulse_train, _thwack,
                    hl_drone, sound, string_twang, swish)
from sounds_ambience import _creak
from sounds_transform import clunk, panel_click
from sounds_weapons import _clank, _snap


# =============================================================================== construct assembly
@sound('assemble_click', 'construct', 5, pitch=P_TONAL, lufs=-24, max_dur=0.12,
       notes='tiny hard-light snap per assembly stage (caller pitches it up per stage); in E, < 120 ms')
def assemble_click(r, v):
    out = buf(0.12)
    f = note(['E7', 'B6', 'E7', 'G7', 'B7'][v])
    add(out, hl_ping(r, f, U(r, 0.015, 0.025), 1.2, glint=1.0, shimmer=0.2), 0, 1.0)
    add(out, click(r, 4000, 0.001), 0, 0.4)
    add(out, micro(r, U(r, 2400, 3200), 0.004, 2, (1.4, 2.0)), 0, 0.3)
    return hpf(out, 800, 4)


@sound('assemble_lock', 'construct', 3, pitch=P_T10, lufs=-19, max_dur=1.0,
       notes='final assembly lock: heavier clunk + E-minor shimmer')
def assemble_lock(r, v):
    out = buf(1.0)
    add(out, sat(click(r, 2000, 0.003), 2.0), 0, 0.7)
    add(out, clunk(r, [0.8, 0.9, 0.7][v]), 0, 1.0)
    add(out, thump(0.3, note('E3'), note('E2'), 0.02, 0.06, drive=1.8), 0, 0.6)
    ch = [['E5', 'B5', 'E6'], ['G5', 'B5', 'E6'], ['E5', 'G5', 'B5']][v]
    add(out, hl_chord(r, ch, dec=0.3, bright=1.2, stagger=0.004), 0.005, 0.5)
    add(out, hl_shards(r, 3, 0.3, 'E8', 'E7', amp=0.2), 0.03, 1.0)
    return out


# =============================================================================== hatches (foley)
@sound('hatch_open', 'construct', 3, pitch=P_MICRO, lufs=-24, notes='hatch hinge opening: latch click + short hinge creak + air')
def hatch_open(r, v):
    out = buf(0.7)
    add(out, panel_click(r, U(r, 0.9, 1.1)), 0, 0.8)
    add(out, _creak(r, [0.3, 0.4, 0.25][v], U(r, 320, 480), 30, 55, 30), 0.03, 0.6)
    n = ns(0.2)
    g = pink(n, r)
    puff = normpk(reson(g, 1400, 2.5) + 0.6 * reson(g, 2800, 3)) * env_pts(n, [(0, 0), (0.01, 1), (0.2, 0)], 1.8)
    add(out, puff, 0.02, 0.15)
    return carve_foley(out, 200, 10000)


@sound('hatch_close', 'construct', 3, pitch=P_MICRO, lufs=-22, notes='hatch closing: short hinge swing -> satisfying clunk + latch')
def hatch_close(r, v):
    out = buf(0.8)
    add(out, _creak(r, 0.18, U(r, 280, 400), 40, 25, 30), 0, 0.35)
    t = [0.16, 0.2, 0.13][v]
    add(out, click(r, 1800, 0.003), t, 0.7)
    add(out, clunk(r, [0.7, 0.8, 0.65][v]), t, 1.0)
    m = material_hit(r, 'metal', 0.8, 1.2)
    add(out, m * env_exp(len(m), 0.08, 0), t, 0.4)
    add(out, panel_click(r, 1.1, 0.6), t + U(r, 0.04, 0.06))
    return carve_foley(out, 150, 10000)


# =============================================================================== vehicle
_ROTOR = [(24.0, 1150.0), (20.0, 950.0)]  # (blade-pass Hz, turbine Hz) per variation; integer per 2 s loop


def _rotor_air(r, n, bp_rate, t0_frac=None):
    """Chopping rotor air: pink through two resonances, amplitude chopped at the blade-pass rate."""
    g = pink(n, r)
    air = normpk(reson(g, 450, 1.5) + 0.7 * reson(g, 1100, 2.0) + 0.3 * lp(brown(n, r), 250))
    cyc = np.cumsum(np.broadcast_to(bp_rate, (n,))) / SR
    chop = 0.35 + 0.65 * (0.5 + 0.5 * np.cos(TAU * cyc)) ** 3
    return air * chop


@sound('rotor_spinup', 'vehicle', 2, pitch=P_SOFT, lufs=-22, fade_out=0.08, notes='~1.5 s rising rotor whirr; tail = the rotor_loop steady state')
def rotor_spinup(r, v):
    d = 1.6
    n = ns(d)
    u = np.clip(np.linspace(0, 1, n) / (1.3 / d), 0, 1)
    bpf, turb = _ROTOR[v % 2]
    rate = 3 + (bpf - 3) * u ** 0.8
    ft = 180 + (turb - 180) * u ** 0.9
    air = _rotor_air(r, n, rate)
    whine = sine(ft, n) + 0.4 * sine(2 * ft, n)
    motor = lp(saw(ft / 8, n) + 0.5 * saw(ft / 4, n), 900)
    env = 0.15 + 0.85 * u ** 1.2
    x = (air + 0.12 * whine + 0.25 * normpk(motor)) * env
    x[:ns(0.02)] *= np.linspace(0, 1, ns(0.02))
    return hp(x, 60, 2)


@sound('rotor_loop', 'vehicle', 1, loop=True, pitch=P_TONAL, lufs=-24, notes='loop: steady rotor whirr (blade chop + turbine whine)')
def rotor_loop(r, v):
    Ld = 2.0
    L, X = ns(Ld), ns(0.3)
    bpf, turb = _ROTOR[0]
    n = L + X
    air = loop_xfade(_rotor_air(r, n, bpf), L, X)  # chop is 48 cycles per 2 s: its envelope is periodic too
    ton = periodic(lambda m: 0.12 * (sine(turb, m) + 0.4 * sine(2 * turb, m))
                   + 0.25 * normpk(lp(saw(turb / 8, m) + 0.5 * saw(turb / 4, m), 900)), L)
    return circ(lambda y: hp(y, 60, 2), air + ton)


@sound('exhaust_puff', 'vehicle', 4, pitch=P_MICRO, lufs=-24, notes='short pneumatic puff: valve click + resonant gas burst')
def exhaust_puff(r, v):
    out = buf(0.35)
    add(out, click(r, 2000, 0.002), 0, 0.5)
    d = [0.14, 0.2, 0.12, 0.25][v]
    n = ns(d)
    g = pink(n, r)
    f1 = [1200, 900, 1500, 1050][v]
    puff = normpk(reson(g, f1, 3) + 0.6 * reson(g, f1 * 2.3, 4)) * (1 + 0.3 * smooth_noise(n, r, 60))
    add(out, puff * env_pts(n, [(0, 0), (0.004, 1), (d, 0)], 2.0), 0.002, 1.0)
    add(out, micro(r, U(r, 400, 600), 0.02, 2, (1.5, 2.2)), 0, 0.3)  # body of the valve
    return carve_foley(out, 200, 10000)


# =============================================================================== mass footfalls
@sound('kaiju_step', 'combat', 5, pitch=P_SOFT, lufs=-13, notes='kaiju footfall: sub thump + ground crack + debris (smaller than kaiju_stomp)')
def kaiju_step(r, v):
    d = 2.2
    n = ns(d)
    out = buf(d)
    add(out, click(r, 900, 0.005), 0, 0.4)
    add(out, thump(1.8, [62, 58, 66, 55, 60][v] * U(r, .97, 1.03), 27, 0.12, 0.4, drive=2.6, attack=0.004), 0, 1.2)
    add(out, material_hit(r, 'stone', 2.6, 1.6), 0, 0.5)
    add(out, shatter(r, 'stone', [8, 10, 6, 12, 9][v], 0.7, 1.0, 1.0), 0.03, 0.3)
    rum = normpk(lp(brown(n, r), 110)) * env_exp(n, 0.4, 0.03)
    add(out, rum, 0, 0.3)
    return carve_heavy(glue(out, 2.2))


@sound('footstep_heavy', 'movement', 5, pitch=P_IMPACT, lufs=-20, notes='heavy suit/robot step: thud + metal sole + a little grit')
def footstep_heavy(r, v):
    out = buf(0.6)
    add(out, thump(0.4, [95, 88, 102, 90, 98][v], 42, 0.03, 0.09, drive=2.2), 0, 1.0)
    m = material_hit(r, 'metal', U(r, 0.8, 1.1), 1.2)
    add(out, m * env_exp(len(m), 0.07, 0), 0, 0.45)
    add(out, material_hit(r, 'stone', U(r, 1.2, 1.6), 1.2), 0.002, 0.5)
    add(out, grit(r, 8, 0.015, 1200, 3500, 0.003), 0.004, 0.3)
    return carve_heavy(glue(out, 1.6), -4, -3)


# =============================================================================== air moves
@sound('air_jump', 'movement', 4, pitch=P_T10, lufs=-24, notes='hard-light step "pop" midair: E-minor tick + soft air puff')
def air_jump(r, v):
    out = buf(0.3)
    add(out, hl_ping(r, note(['E6', 'B6', 'G6', 'E7'][v]), 0.06, 1.2, glint=1.0), 0, 0.8)
    add(out, hl_ping(r, note(['B6', 'E7', 'D7', 'B7'][v]), 0.04, 1.2, glint=0.6), 0.004, 0.35)
    add(out, click(r, 4000, 0.001), 0, 0.3)
    add(out, swish(r, 0.12, 900, 2800, 1.8, 100, [(0, 0), (0.015, 1), (0.12, 0)]), 0, 0.3)
    return carve_foley(out, 300, 12000)


@sound('dodge', 'movement', 5, pitch=P_MICRO, lufs=-24, notes='quick dodge whoosh (shaped resonant air, 150-250 ms)')
def dodge(r, v):
    d = [0.18, 0.22, 0.15, 0.25, 0.2][v]
    out = buf(d)
    f0, f1 = [(700, 2600), (2400, 800), (900, 3000), (2800, 900), (1200, 2200)][v]
    add(out, swish(r, d, f0, f1, 2.5, 140, [(0, 0), (d * 0.35, 1), (d, 0)]), 0, 1.0)
    add(out, swish(r, d * 0.8, f0 * 1.6, f1 * 1.6, 3.5, 90, [(0, 0), (d * 0.3, 1), (d * 0.8, 0)]), 0.01, 0.4)
    for _ in range(3):  # cloth / kit flicks
        add(out, micro(r, U(r, 1500, 3000), 0.004), U(r, 0.0, d * 0.5), 0.12)
    return carve_foley(out, 250, 9000)


@sound('lunge', 'movement', 4, pitch=P_MICRO, lufs=-23, notes='committed dash: foot scuff + short forward whoosh')
def lunge(r, v):
    out = buf(0.4)
    add(out, material_hit(r, 'stone', U(r, 0.7, 0.9), 1.1), 0, 0.8)
    add(out, swish(r, 0.06, 1300, 2400, 2.0, 160, [(0, 0), (0.01, 1), (0.06, 0)]), 0.005, 0.25)  # scuff
    d = [0.22, 0.26, 0.2, 0.24][v]
    add(out, swish(r, d, 800, [2800, 3200, 2400, 3000][v], 2.2, 120, [(0, 0), (d * 0.4, 1), (d, 0)]), 0.02, 0.9)
    return carve_foley(out, 220, 9000)


# =============================================================================== hook tether (foley + one heavy)
@sound('hook_fire', 'tether', 4, pitch=P_MICRO, lufs=-23, notes='hook launch "thwip": latch + FM zip + cable twang')
def hook_fire(r, v):
    out = buf(0.4)
    add(out, click(r, 3000, 0.002), 0, 0.6)
    add(out, _clank(r, U(r, 1800, 2400), 0.012), 0, 0.3)
    n = ns(0.16)
    f = sweep(n, [3200, 2800, 3600, 3000][v], 900, 0.6)
    add(out, fm(f, 1.5, 1.2 * env_exp(n, 0.04), n) * env_pts(n, [(0, 0), (0.005, 1), (0.16, 0)], 1.5), 0.002, 0.6)
    add(out, string_twang(r, [420, 470, 390, 510][v], 0.09, 6), 0.01, 0.3)
    return carve_foley(out, 250, 11000)


@sound('hook_attach', 'tether', 3, pitch=P_IMPACT, lufs=-22, notes='hook bites: damped metal + chips + taut cable')
def hook_attach(r, v):
    out = buf(0.8)
    m = material_hit(r, 'metal', [0.4, 0.35, 0.5][v], 1.4)
    add(out, m * env_exp(len(m), 0.1, 0), 0, 0.7)
    add(out, material_hit(r, 'stone', 0.6, 1.3), 0, 0.6)
    add(out, grit(r, 8, 0.01, 1600, 4500, 0.003), 0.002, 0.3)
    add(out, string_twang(r, [440, 400, 480][v], 0.3, 7, vib=0.002), 0.005, 0.4)
    return carve_foley(out, 180, 10000)


@sound('hook_reel_loop', 'tether', 1, loop=True, pitch=P_TONAL, lufs=-28, notes='loop; hook winch: geared motor + ratchet')
def hook_reel_loop(r, v):
    Ld = 2.0
    L = ns(Ld)
    f0, rate = 210.0, 30.0

    def motor(n):
        t = tvec(n)
        m = lp(saw(f0, n) + saw(f0 + 1.0, n) + 0.4 * saw(1.5 * f0, n), 2400)
        return normpk(m) * (1 + 0.2 * np.sin(TAU * rate * t)) + 0.12 * sine(6 * f0, n)
    x = 0.55 * periodic(motor, L)
    k = int(rate * Ld)
    tick = np.zeros(L)
    for i in range(k):
        add_circ(tick, micro(r, U(r, 2800, 3600), 0.003, 3, (1.4, 2.2)), int((i + 0.5) * L / k), U(r, 0.6, 1.0))
    x = x + 0.4 * normpk(tick)
    return circ(lambda y: carve_foley(y, 220, 10000), x)


@sound('hook_break', 'tether', 3, pitch=P_MICRO, lufs=-21, notes='hook line snaps: sharp crack + whipping recoil + ping')
def hook_break(r, v):
    out = buf(0.6)
    add(out, sat(click(r, 3000, 0.002), 2.0), 0, 0.9)
    n = ns(0.3)
    f = glide(n, [900, 1000, 820][v], 180, 0.05)  # the cable whips as tension goes
    tw = (sine(f, n) + 0.5 * sine(2 * f, n) + 0.3 * sine(3 * f, n)) * env_exp(n, 0.07, 0.001)
    add(out, tw, 0.002, 0.5)
    add(out, micro(r, U(r, 2400, 3200), 0.03, 3, (1.5, 2.8)), 0, 0.35)
    add(out, swish(r, 0.15, 3000, 900, 2.0, 120, [(0, 0), (0.01, 1), (0.15, 0)]), 0.004, 0.2)
    return carve_foley(out, 200, 11000)


@sound('hook_slam', 'combat', 3, pitch=P_MICRO, lufs=-14, notes='pulled-in slam: short rush -> heavy thud + stone break')
def hook_slam(r, v):
    out = buf(1.3)
    rush = [0.14, 0.18, 0.12][v]
    add(out, swish(r, rush, 900, 3000, 2.0, 120, [(0, 0), (rush * 0.9, 1), (rush, 0)]), 0, 0.3)
    add(out, sat(click(r, 1800, 0.004), 2.0), rush, 0.8)
    add(out, _thwack(r, U(r, 150, 220), v, 0.07), rush, 0.7)
    add(out, thump(0.9, 125, 38, 0.05, 0.2, drive=2.8), rush, 1.2)
    add(out, shatter(r, 'stone', [8, 10, 6][v], 0.35, 1.4, 1.3), rush + 0.003, 0.5)
    return carve_heavy(glue(out, 1.8))


# =============================================================================== reveal / recall / UI / alerts
@sound('reveal_pulse', 'feedback', 2, pitch=P_TONAL, lufs=-26, notes='soft sonar-like hard-light ping + faint return')
def reveal_pulse(r, v):
    out = buf(0.8)
    f = note(['E7', 'B6'][v])
    add(out, hl_ping(r, f, 0.25, 1.3, glint=0.5, shimmer=0.8), 0, 1.0)
    add(out, hl_ping(r, f * 2, 0.15, 1.1, glint=0.3, shimmer=0.8), 0, 0.3)
    add(out, hl_ping(r, f, 0.15, 1.2, glint=0.2, shimmer=0.8), 0.19, 0.3)  # the sonar return
    return carve_ui(out, 2000, 11000)


@sound('recall_blink', 'construct', 3, pitch=P_T10, lufs=-21, notes='hard-light blink-in: short reverse swell into a bright ping')
def recall_blink(r, v):
    out = buf(0.6)
    d = 0.14
    n = ns(d)
    u = np.linspace(0, 1, n)
    x = np.zeros(n)
    for nm, a in (('E5', 1.0), ('B5', 0.7), ('E6', 0.5)):
        f = note(nm) * 2 ** (u - 1)
        x += a * (sine(f, n) + 0.3 * sine(2 * f, n))
    add(out, normpk(x) * u ** 3, 0, 0.5)
    ch = [['E6', 'B6'], ['G6', 'E7'], ['B5', 'E6']][v]
    add(out, hl_chord(r, ch, dec=0.18, bright=1.3), d, 0.8)
    add(out, click(r, 5000, 0.001), d, 0.4)
    return hp(out, 300, 4)


@sound('summon_ping', 'ui', 2, pitch=P_TONAL, lufs=-30, notes='tiny UI tick (E minor)')
def summon_ping(r, v):
    n = ns(0.05)
    f = note(['E8', 'B7'][v])
    x = fm(f, 2.0, 1.0 * env_exp(n, 0.003), n) * env_exp(n, 0.008, 0.0003) + 0.3 * fit(click(r, 6000, 0.0006), n)
    return carve_ui(x, 2000)


@sound('alert_danger', 'feedback', 2, pitch=P_TONAL, lufs=-20,
       notes='one clean escalating alarm figure ~0.6 s (caller varies pitch/gain by severity)')
def alert_danger(r, v):
    out = buf(0.7)
    seq = [('B6', 'D7', 'E7'), ('E7', 'B6', 'E7')][v]
    for i, nm in enumerate(seq):
        d = 0.14
        n = ns(d)
        f = note(nm)
        s = fm(f, 1.0, 1.3, n) + 0.4 * fm(f * 2, 1.0, 0.8, n)
        s *= env_pts(n, [(0, 0), (0.004, 1), (d - 0.03, 0.85), (d, 0)])
        add(out, s, 0.19 * i, 0.75 + 0.15 * i)
    return carve_ui(out, 1500)


# =============================================================================== domain
@sound('domain_open', 'combat', 2, pitch=P_SOFT, lufs=-11, notes='dome eruption: rising rush -> sub boom + crack + E-minor chord bloom')
def domain_open(r, v):
    d = 2.6
    out = buf(d)
    pre = 0.18
    add(out, swish(r, pre, 400, 2500, 1.8, 60, [(0, 0), (pre * 0.9, 1), (pre, 0)]), 0, 0.3)
    add(out, sat(click(r, 1200, 0.006), 2.5), pre, 0.9)
    add(out, thump(2.4, 70, 27, 0.12, 0.55, drive=3.0, attack=0.003), pre, 1.3)
    n = ns(3.6)
    env = _bloom_env(n, 0.12, 0.7)
    for nm, a in (('E2', 0.8), ('B2', 0.6), ('E3', 0.7), ('G3', 0.5), ('B3', 0.45), ('E4', 0.4)):
        add(out, hl_tone(r, note(nm), n, env, bright=1.0, shimmer=0.6), pre, a * 0.4)
    add(out, hl_chord(r, ['E5', 'B5', 'E6'], dec=0.4, bright=1.3), pre, 0.35)
    add(out, hl_shards(r, 6, 0.9, 'E8', 'E6', amp=0.35), pre + 0.02, 1.0)
    return carve_heavy(glue(out, 2.0))


@sound('domain_hum_loop', 'construct', 1, loop=True, stereo=True, pitch=P_TONAL, lufs=-26,
       notes='stereo loop: inside-the-dome drone in E, slow shimmer')
def domain_hum_loop(r, v):
    Ld = 8.0
    L = ns(Ld)
    chs = []
    for c in range(2):
        d = hl_drone(r, [note(x) for x in ('E2', 'B2', 'E3', 'G3', 'B3')], L, Ld, bright=0.8, shimmer=0.7, lfo=0.35,
                     amps=[1.0, 0.6, 0.6, 0.4, 0.3])
        sh = hl_drone(r, [note('E6'), note('B6')], L, Ld, bright=0.6, shimmer=1.0, lfo=0.8, amps=[1.0, 0.6])
        chs.append(normpk(d) + 0.03 * normpk(sh))
    return st(*chs)


@sound('domain_shatter', 'combat', 3, pitch=P_T10, lufs=-12, notes='the dome breaks: big hard-light shatter, E1 sub, long falling shard rain')
def domain_shatter(r, v):
    d = 2.6
    out = buf(d)
    add(out, sat(click(r, 2500, 0.006), 2.5), 0, 0.9)
    add(out, _snap(r, 1.0, 3500, 0.005), 0.012, 0.6)
    ch = [['E4', 'B4', 'E5', 'G5', 'B5'], ['G4', 'B4', 'E5', 'B5', 'E6'], ['E4', 'G4', 'B4', 'E5', 'G5']][v]
    add(out, hl_chord(r, ch, dec=0.35, bright=1.3, stagger=0.004), 0, 0.7)
    add(out, thump(1.8, note('E3'), note('E1'), 0.07, 0.4, drive=3.0), 0, 1.3)
    add(out, hl_shards(r, 22, 1.8, 'E8', 'E5', amp=0.8), 0.01, 1.0)
    add(out, _cube_tinks(r, 16, 0.1, 2.0, 'E6', 'B7', amp=0.45), 0, 1.0)
    return peq(glue(out, 1.6), 3200, -3, 0.8)


# =============================================================================== ultimate
@sound('ult_charge', 'construct', 2, pitch=P_TONAL, lufs=-18, max_dur=1.6, notes='~1.5 s rising ultimate windup, lands on E')
def ult_charge(r, v):
    d = 1.5
    n = ns(d)
    u = np.linspace(0, 1, n)
    out = buf(d)
    f = note(['E3', 'B2'][v]) * 4 ** (u ** 1.6)  # two octaves up into E5 / B4
    x = (sine(f, n) + 0.5 * sine(2 * f, n) + 0.3 * sine(3 * f, n) + 0.3 * sine(f / 2, n))
    add(out, normpk(x) * u ** 1.5, 0, 0.6)
    t, j, k0 = 0.0, 0, pent_index('E4')
    while t < d - 0.05:  # accelerating pentatonic arpeggio
        add(out, hl_ping(r, pent(k0 + j % 15), 0.05, 1.2, glint=0.5), t, 0.1 + 0.3 * t / d)
        t += 0.13 * (1 - 0.75 * t / d)
        j += 1
    for _ in range(40):  # crackle builds
        tt = d * r.uniform() ** 0.5
        add(out, micro(r, U(r, 2500, 7000), 0.002), tt, 0.3 * (tt / d) ** 2)
    out[ns(d) - ns(0.01):ns(d)] *= np.linspace(1, 0, ns(0.01))
    out[ns(d):] = 0
    return out


@sound('ult_fire', 'combat', 3, pitch=P_SOFT, lufs=-9, notes='beam ultimate fires: snap + huge E sub + FM beam zap + sustained beam chord; heavy carve')
def ult_fire(r, v):
    d = 2.4
    n = ns(d)
    out = buf(d)
    add(out, _snap(r, 1.0, 2500, 0.006), 0, 1.0)
    add(out, sat(click(r, 1200, 0.006), 2.5), 0, 0.8)
    add(out, thump(2.4, note('E3') * [1.0, 0.9, 1.1][v], note('E1') * 0.75, 0.1, 0.55, drive=3.5, attack=0.002), 0, 1.4)
    nz = ns(0.5)
    add(out, fm(glide(nz, note('E7'), note('E4'), 0.05), 1.0, 8 * env_exp(nz, 0.06) + 0.5, nz) * env_exp(nz, 0.12), 0, 0.5)
    env = env_pts(n, [(0, 0), (0.01, 1), (1.0, 0.6), (d, 0)], 1.4)
    for nm, a in (('E2', 1.0), ('B2', 0.7), ('E3', 0.6), ('B3', 0.4)):
        add(out, hl_tone(r, note(nm), n, env, bright=1.1, shimmer=0.7), 0, a * 0.35)
    add(out, hl_shards(r, 8, 1.2, 'E8', 'E6', amp=0.35), 0.02, 1.0)
    return carve_heavy(glue(out, 2.2))


# =============================================================================== orbital strike
@sound('strike_telegraph', 'feedback', 2, pitch=P_TONAL, lufs=-22,
       notes='~2 s ominous ground-marker hum: dissonant high cluster, accelerating pulse')
def strike_telegraph(r, v):
    d = 2.0
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    rate = np.cumsum(2.0 + 6.0 * (t / d) ** 2) / SR
    pulse = 0.55 + 0.45 * (0.5 + 0.5 * np.cos(TAU * rate)) ** 2
    hi = np.zeros(n)
    for nm, a in (('B6', 1.0), ('C7', 0.7), ('F7', 0.5)):  # minor 2nd + tritone: ominous
        f = note(nm) * (1 + 0.003 * np.sin(TAU * 0.7 * t))
        hi += a * (sine(f, n, r.uniform(0, TAU)) + 0.3 * sine(2 * f, n))
    body = sine(note('E4'), n) + 0.5 * sine(note('F4'), n)
    env = env_pts(n, [(0, 0), (0.3, 0.8), (d - 0.15, 1), (d, 0)])
    x = balance_hi(0.5 * body * env, hi * pulse * env, 0.5)
    add(out, x, 0, 1.0)
    return carve_ui(out, 900)


@sound('strike_beam_loop', 'combat', 1, loop=True, pitch=P_TONAL, lufs=-16,
       notes='loop: orbital beam hammering down - roaring E drone, falling hard-light runs, crackle')
def strike_beam_loop(r, v):
    Ld = 2.0
    L = ns(Ld)
    t = tvec(L)

    def roar(m):
        tt = tvec(m)
        s = saw(qf(note('E1'), Ld), m) + saw(qf(note('E2'), Ld) + 0.5, m) + 0.6 * saw(qf(note('B2'), Ld), m)
        return tvf(s, 900 + 500 * np.sin(TAU * 2.0 * tt), 0.7, 'lp')
    x = normpk(periodic(lambda m: roar(m), L))
    x = x + 0.4 * normpk(hl_drone(r, [note('E4'), note('B4'), note('E5')], L, Ld, bright=1.1, shimmer=0.8, lfo=0.4))
    x *= 0.85 + 0.15 * np.sin(TAU * 12.0 * t)  # beam throb
    k0 = pent_index('E8')
    for run in range(4):  # falling runs = the beam pouring downward (circular)
        for j in range(7):
            add_circ(x, hl_ping(r, pent(k0 - j - run % 2), 0.05, 1.2, glint=0.5), int((run * Ld / 4 + 0.05 * j) * SR), 0.08)
    for _ in range(90):
        add_circ(x, micro(r, U(r, 2000, 7000), U(r, 0.0015, 0.004), 3, (1.3, 2.2)), int(r.uniform(0, L)), U(r, 0.05, 0.2))
    return circ(lambda y: hp(y, 30, 2), glue(x, 1.4))


# =============================================================================== misc
@sound('legendary_sting', 'feedback', 2, pitch=P_TONAL, lufs=-20, notes='short golden shimmer (sparkle, not a reward voice)')
def legendary_sting(r, v):
    out = buf(1.0)
    n = ns(0.9)
    env = env_pts(n, [(0, 0), (0.04, 1), (0.3, 0.6), (0.9, 0)], 1.5)
    for nm, a in (('G6', 0.6), ('B6', 0.5), ('D7', 0.45), ('G7', 0.3)):  # G major = E minor's golden relative
        add(out, hl_tone(r, note(nm), n, env, bright=1.1, shimmer=0.9), 0, a)
    k0 = pent_index(['E7', 'G7'][v])
    for j in range(8):
        add(out, hl_ping(r, pent(k0 + (j % 4) - (j // 4) * 2), 0.06, 1.2, glint=0.8), 0.03 * j, 0.35)
    return carve_ui(out, 1500)


@sound('sputter', 'construct', 3, pitch=P_T10, lufs=-24, notes='flickering underpowered cast: E tone gating on/off, sagging, crackle')
def sputter(r, v):
    d = 0.55
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    gate = np.clip(0.5 + 1.2 * smooth_noise(n, r, [18, 25, 14][v]), 0, 1) ** 1.5
    f = note(['E5', 'B4', 'G5'][v]) * 2 ** (-0.6 / 12 * t / d)
    x = (sine(f, n) + 0.4 * sine(2 * f, n) + 0.2 * sine(3 * f, n)) * gate * env_pts(n, [(0, 0), (0.02, 1), (d, 0)], 1.3)
    add(out, x, 0, 0.8)
    for _ in range(int(U(r, 10, 18))):
        add(out, micro(r, U(r, 2000, 6000), 0.003), r.uniform(0, d), U(r, 0.1, 0.3))
    return hp(out, 200, 4)


def _ads(r, v, up):
    out = buf(0.3)
    d = [0.1, 0.12, 0.09][v]
    f0, f1 = (900, 2400) if up else (2400, 900)
    add(out, swish(r, d, f0, f1, 2.0, 150, [(0, 0), (d * 0.4, 1), (d, 0)]), 0, 0.8)
    for _ in range(2):  # kit rattle
        add(out, micro(r, U(r, 1800, 3200), 0.006, 3, (1.4, 2.4)), U(r, 0.0, d), 0.15)
    add(out, _clank(r, U(r, 1600, 2200), 0.008), d if up else 0.0, 0.2)  # settles (in) / unlatches (out)
    return carve_foley(out, 250, 9000)


@sound('ads_in', 'movement', 3, pitch=P_MICRO, lufs=-30, notes='aim down sights: subtle cloth/weapon raise + settle tick')
def ads_in(r, v):
    return _ads(r, v, True)


@sound('ads_out', 'movement', 3, pitch=P_MICRO, lufs=-30, notes='leave sights: subtle cloth/weapon lower')
def ads_out(r, v):
    return _ads(r, v, False)


@sound('view_toggle', 'ui', 2, pitch=P_TONAL, lufs=-32, notes='camera/view toggle: very subtle bright whoosh')
def view_toggle(r, v):
    d = 0.16
    n = ns(d)
    f0, f1 = [(2500, 6500), (6500, 2500)][v]
    x = normpk(tvf(pink(n, r), sweep(n, f0, f1), 2.0, 'bp')) * env_pts(n, [(0, 0), (d * 0.4, 1), (d, 0)], 1.4)
    x += 0.2 * sine(sweep(n, note('E7'), note('B7')) if v == 0 else sweep(n, note('B7'), note('E7')), n) * env_pts(n, [(0, 0), (d * 0.4, 1), (d, 0)])
    return carve_ui(x, 1500)
