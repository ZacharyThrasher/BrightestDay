"""LEGENDARY move signatures (bd_sim `constructs/legend.rs`, voiced by bd_game `present/legend_sound.rs`).

One sound family per rule, never a wall of noise:
  * RECALL - the rising recall whir, THE CATCH (metal smacking into a palm + sub), the lodge (a blade biting
    stone with the embedded-handle quiver), the recall-flight wind rush (loop).
  * CALL THE STORM - the raise (crackling charge-up / a fizzle), the telegraph (an ominous building tone), one
    strike per element (lightning crack + thunder roll, meteor whistle + rock crunch, icicle shatter, tornado,
    geyser, psychic crush, earth spikes, shadow maw, atomic nova, poison miasma), the arc zap.
  * RICOCHET - a bright ding + spark (the game pitches it up the E-minor pentatonic per bounce).
  * DEFLECT - the lightsaber: sizzling deflect ping, the idle hum (loop), the doppler swoosh.
  * BLADE WAVE - the crescent launch + its travelling whirr (loop, carried along the wave).
  * HEAVY BLADE - the leap whoosh, the cleave slam (heavy: owns the sub), the super-armour clang.
  * CHAIN BLADES - the whirling 360 sweep, the fire roar, the snap + yank.
  * GUNS - golden-gun report with a golden ring-out (dry), needle ticks, supercombine, gravity grab / punt,
    proton stream (loop) + release snap, charge hum (loop, pitched up by the game) + full chime, BFG
    spin-up, charge shot (tap pellet / full blast), BFG tendril zaps.

Same rules as everything else (README): dry; noise only where the source is noise (wind, thunder, water,
gas, fire) and then pink/brown, band-limited, grained and subordinate; electricity is FM buzz + irregular
modal crackle grains, never hiss; carved by role; hard-light / tuned parts in E minor.
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, grit, hl_chord, hl_ping, hl_shards, hl_tone, material_hit, micro, note, pent, pent_index, shatter
from sounds import P_IMPACT, P_MICRO, P_SOFT, P_T10, P_TONAL, U, sound, string_twang, swish


# =============================================================================== shared pieces
def _crackle(r, d, count, f_lo=2000.0, f_hi=8000.0, dec=(0.0012, 0.004), shape=1.0, amp=1.0, grow=False):
    """Electric crackle: irregular tiny modal ticks (no fixed rate, no hiss). `shape` > 1 front-loads them;
    `grow` back-loads them (charge-ups)."""
    out = np.zeros(ns(d + 0.03))
    for _ in range(int(count)):
        u = r.uniform() ** shape
        t = d * (1 - u if grow else u)
        k = (t / d) if grow else (1 - 0.7 * t / d)
        add(out, micro(r, U(r, f_lo, f_hi), U(r, *dec), 3, (1.3, 2.4)), t, amp * U(r, 0.25, 1.0) * max(k, 0.05))
    return out


def _buzz(r, d, f0, f1, idx=2.5, flick=80.0, shape=1.0):
    """Electric arc body: harmonic FM buzz on a jittering pitch, amplitude flickering (gated)."""
    n = ns(d)
    f = sweep(n, f0, f1, shape) * (1 + 0.04 * smooth_noise(n, r, 40))
    x = fm(f, 1.0, idx, n) + 0.5 * fm(f * 1.5, 1.0, idx * 0.6, n)
    gate = np.clip(0.55 + 0.9 * smooth_noise(n, r, flick), 0.0, 1.0)
    return normpk(x) * gate


def _zap(r, d=0.14, f=110.0, bright=1.0):
    """A short arc: click + buzz with a falling pitch + crackle."""
    out = buf(d)
    n = ns(d)
    add(out, click(r, 3500, 0.002), 0, 0.7)
    add(out, _buzz(r, d, f * U(r, 1.3, 1.6), f * U(r, 0.8, 1.0), U(r, 2.0, 3.5), 120) * env_exp(n, d * 0.35, 0.0005), 0, 0.8)
    add(out, _crackle(r, d * 0.8, 14 * bright, 2500, 9000, shape=1.8), 0, 0.5 * bright)
    return out


def _air(r, d, f0, f1, q=1.4, pts=None, color='pink'):
    """Moving air (subordinate): pink noise through a sweeping resonant band, grained."""
    n = ns(d)
    src = pink(n, r) if color == 'pink' else brown(n, r) + 0.4 * pink(n, r)
    y = normpk(tvf(src, sweep(n, f0, f1), q, 'bp')) * (1 + 0.35 * smooth_noise(n, r, 70))
    return y * env_pts(n, pts or [(0, 0), (d * 0.4, 1), (d, 0)])


def _rumble(r, d, fc=120.0, tau=0.7, grain=4.0, attack=0.05):
    """Low physical rumble (brown, low-passed, slow amplitude grain)."""
    n = ns(d)
    return normpk(lp(brown(n, r), fc)) * (1 + 0.45 * smooth_noise(n, r, grain)) * env_exp(n, tau, attack)


def _fireball(r, d, fc0, fc1, tau_f, tau_a):
    """Combustion body: brown+pink (never white), falling low-pass, rough amplitude grain."""
    n = ns(d)
    src = brown(n, r) + 0.5 * pink(n, r)
    y = tvf(src, glide(n, fc0, fc1, tau_f), 0.8, 'lp') * (1 + 0.5 * smooth_noise(n, r, 45))
    return normpk(y) * env_exp(n, tau_a, 0.003)


def _metal(r, f, decs=(0.25, 0.15, 0.09, 0.06, 0.05, 0.04), energy=1.0):
    """A struck steel body with explicit (short = gripped / embedded) decays."""
    ratios = [1.0, 2.76 * U(r, .99, 1.01), 5.40 * U(r, .98, 1.02), 1.59 * U(r, .98, 1.02), 2.14, 3.51]
    amps = [1.0, 0.6 * energy, 0.35 * energy ** 1.3, 0.5, 0.35, 0.25 * energy]
    n = ns(max(decs) * 6 + 0.01)
    return modal([f * k for k in ratios], list(decs), amps, n, r, attack=0.0002)


def _clank(r, f, decay=0.035, amp=1.0):
    ratios = [1.0, 1.73 * U(r, .97, 1.03), 2.61 * U(r, .97, 1.03), 3.9 * U(r, .97, 1.03)]
    n = ns(decay * 7)
    x = modal([f * k for k in ratios], [decay, decay * .8, decay * .6, decay * .4], [1, .7, .5, .3], n, r)
    return amp * (x + 0.5 * fit(click(r, 3000, 0.001), n))


def _snap(r, amp=1.0, fc=3000.0, d=0.006):
    n = ns(d) + ns(0.003)
    x = np.zeros(n)
    x[1] = 1.0
    x[:ns(d)] += r.uniform(-1, 1, ns(d)) * np.exp(-np.arange(ns(d)) / ns(d / 4))
    return amp * normpk(lpf(hpf(x, fc, 2), 9000, 2))


def _report(r, lo=250.0, hi=2200.0, tau=0.006, amp=1.0):
    n = ns(0.03)
    b = normpk(bp(pink(n, r), lo, hi)) * env_exp(n, tau, 0.0003)
    return amp * sat(b * 1.5, 2.0)


def _swell(n, curve=2.0):
    """0 -> 1 rising envelope (pre-impact suck / build)."""
    return np.linspace(0, 1, n) ** curve


def _chain_rattle(r, d, count, f_lo=1400.0, f_hi=4200.0, amp=1.0, dens=None):
    """Chain links: many tiny damped steel ticks, density following `dens(u)`."""
    out = np.zeros(ns(d + 0.05))
    for _ in range(int(count)):
        u = r.uniform()
        if dens is not None and r.uniform() > dens(u):
            continue
        f = U(r, f_lo, f_hi)
        tick = modal([f, f * 2.76, f * 1.59], [0.012, 0.008, 0.01], [1, .5, .4], ns(0.08), r, attack=0.0002)
        add(out, tick, d * u, amp * U(r, 0.3, 1.0))
    return out


# =============================================================================== RECALL
@sound('recall_whoosh', 'combat', 3, pitch=P_MICRO, lufs=-18, max_dur=0.7,
       notes='LEGEND recall: the item tears back to the hand - rising spin whir + air, building toward the catch')
def recall_whoosh(r, v):
    d = 0.55
    n = ns(d)
    out = buf(d)
    t = tvec(n)
    # the spinning item: a resonant band chopped at a rising rotation rate (whum-whum-whum faster)
    rot = sweep(n, [9, 12, 7][v], [26, 32, 22][v], 0.8)
    chop = 0.55 + 0.45 * np.sin(TAU * np.cumsum(rot) / SR)
    band = tvf(pink(n, r) + 0.5 * brown(n, r), sweep(n, 380, [1500, 1800, 1300][v], 0.9), 2.4, 'bp')
    add(out, normpk(band) * chop * env_pts(n, [(0, 0.15), (d * 0.75, 1), (d, 0)], 1.3), 0, 0.9)
    # a tonal whistle gliding up (the item's edge singing), E3 -> B3 family
    f = sweep(n, note('E4'), note('B4') * U(r, 0.98, 1.02), 0.8)
    wh = (sine(f, n) + 0.3 * sine(2 * f, n)) * env_pts(n, [(0, 0), (d * 0.5, 0.4), (d * 0.85, 1), (d, 0)], 1.5)
    add(out, wh * (1 + 0.2 * np.sin(TAU * 7 * t)), 0, 0.22)
    add(out, thump(0.2, 90, 150, 0.2, 0.08, drive=0), d * 0.55, 0.3)  # air push arriving
    return carve_foley(out, 120, 12000)


@sound('recall_catch', 'combat', 4, pitch=P_MICRO, lufs=-13,
       notes='LEGEND the CATCH SNAP: steel smacking into a gripping palm (muted ring) + leather slap + sub thump')
def recall_catch(r, v):
    out = buf(0.8)
    add(out, sat(click(r, 2600, 0.004), 2.5), 0, 1.0)
    # leather / gauntlet slap: a <= 12 ms band-limited contact burst + a damped low-mid thwack
    m = ns(0.012)
    add(out, normpk(bp(pink(m, r), 700, 3200)) * env_exp(m, 0.003, 0.0002), 0, 0.8)
    f = U(r, 190, 260)
    slap = modal([f, f * 1.62, f * 2.43, f * 3.3], [0.035, 0.022, 0.015, 0.01], [1, .6, .4, .25], ns(0.25), r)
    add(out, slap, 0, 0.9)
    # the metal head: a hard bright smack that the grip mutes fast (the upper body rings a moment longer)
    fm_ = U(r, 380, 560) * [1.0, 0.85, 1.15, 0.95][v]
    add(out, _metal(r, fm_, (0.08, 0.05, 0.03, 0.06, 0.04, 0.03), 1.8), 0.001, 0.8)
    add(out, _metal(r, fm_ * U(r, 2.9, 3.3), (0.14, 0.09, 0.05, 0.1, 0.06, 0.04), 1.4), 0.001, 0.45)
    # the weight: sub thump (you FEEL it arrive) - short, it's a catch, not a crater
    add(out, thump(0.4, 130 * U(r, .92, 1.08), 45, 0.03, 0.075, drive=3.0, attack=0.001), 0, 1.1)
    add(out, thump(0.15, 280, 150, 0.01, 0.025, drive=1.8), 0, 0.5)  # mid punch for small speakers
    add(out, grit(r, 6, 0.006, 1800, 4200, 0.002), 0, 0.3)
    return peq(glue(out, 2.6), 3200, -2, 0.8)


@sound('lodge_thunk', 'combat', 4, pitch=P_IMPACT, lufs=-15,
       notes='LEGEND lodge: blade bites stone/earth - chunk + chips + the embedded handle quivering')
def lodge_thunk(r, v):
    out = buf(0.9)
    add(out, sat(click(r, 1800, 0.004), 1.8), 0, 0.8)
    add(out, material_hit(r, 'stone' if v % 2 == 0 else 'dirt', U(r, 1.2, 1.7), 1.6), 0, 0.9)
    add(out, _metal(r, U(r, 700, 1000), (0.04, 0.025, 0.015, 0.03, 0.02, 0.012), 1.5), 0, 0.65)  # wedge bite (embedded)
    add(out, thump(0.4, 150, 60, 0.03, 0.08, drive=2.0), 0, 0.9)
    add(out, grit(r, 10 + 4 * v, 0.015, 1200, 4000, 0.003), 0.002, 0.35)
    # the quiver: handle + head vibrating in the wedge (low partial, fast wobble, ~0.35 s)
    n = ns(0.45)
    t = tvec(n)
    fq = U(r, 150, 210)
    wob = 0.5 + 0.5 * np.sin(TAU * U(r, 16, 22) * t)
    q = (sine(fq * (1 + 0.01 * np.sin(TAU * 19 * t)), n) + 0.35 * sine(fq * 2.7, n)) * wob * env_exp(n, 0.12, 0.01)
    add(out, q, 0.02, 0.35)
    return peq(glue(out, 1.6), 3200, -2, 0.8)


@sound('thor_flight_loop', 'combat', 1, loop=True, pitch=P_TONAL, lufs=-22,
       notes='LEGEND loop: recall flight - dragged behind the item: wind rush + low roar + edge whistle')
def thor_flight_loop(r, v):
    Ld = 2.0
    L, X = ns(Ld), ns(0.4)
    n = L + X
    gust = 0.75 + 0.25 * smooth_noise(n, r, 1.5)
    body = normpk(tvf(brown(n, r) + 0.7 * pink(n, r), 700 * 2 ** (0.4 * smooth_noise(n, r, 1.0)), 1.6, 'bp'))
    low = normpk(lp(brown(n, r), 130))
    whistle = normpk(tvf(pink(n, r), 1650 * 2 ** (0.2 * smooth_noise(n, r, 0.8)), 26, 'bp'))
    x = loop_xfade(gust * (body + 0.45 * low + 0.1 * whistle), L, X)
    # a steady tonal core so it reads as "speed", not hiss (strictly periodic)
    core = periodic(lambda m: sine(qf(note('E2'), Ld), m) + 0.4 * sine(qf(note('B2'), Ld), m), L)
    return circ(lambda y: lp(hp(y, 45, 2), 4500, 2), normpk(x) + 0.16 * normpk(core))


# =============================================================================== CALL THE STORM
@sound('storm_raise', 'construct', 3, pitch=P_T10, lufs=-18, max_dur=1.1,
       notes='LEGEND storm raise: the item lifted to the sky - crackling charge-up rising to a hard-light Em flash')
def storm_raise(r, v):
    d = 0.85
    n = ns(d)
    out = buf(d)
    rise = _buzz(r, d, note('E2'), note('E3'), 2.0 + v * 0.5, 60, 0.7) * env_pts(n, [(0, 0), (d * 0.8, 1), (d, 0.2)], 1.6)
    add(out, rise, 0, 0.55)
    add(out, _crackle(r, d, 60, 2000, 8500, grow=True, shape=1.4), 0, 0.6)
    ch = [['E4', 'B4', 'E5', 'G5'], ['E4', 'G4', 'B4', 'E5'], ['B3', 'E4', 'B4', 'E5']][v]
    swell = hl_tone(r, note(ch[0]), n, env_pts(n, [(0, 0), (d * 0.9, 1), (d, 0)], 2.2), bright=1.1)
    for nm in ch[1:]:
        swell += 0.6 * hl_tone(r, note(nm), n, env_pts(n, [(0, 0), (d * 0.9, 1), (d, 0)], 2.4), bright=1.1)
    add(out, normpk(swell), 0, 0.4)
    add(out, hl_chord(r, ch[1:] + ['E6'], dec=0.18, bright=1.3, stagger=0.004), d * 0.86, 0.45)
    add(out, thump(0.3, note('E3'), note('E2'), 0.04, 0.08, drive=1.8), d * 0.86, 0.5)
    return hp(out, 70, 2)


@sound('storm_fizzle', 'construct', 2, pitch=P_T10, lufs=-22, max_dur=0.6,
       notes='LEGEND storm raise while recharging: a sputter - weak crackles, a sagging buzz, dies')
def storm_fizzle(r, v):
    d = 0.5
    n = ns(d)
    out = buf(d)
    sag = _buzz(r, d, note('B2'), note('E2') * 0.9, 1.6, 30) * env_pts(n, [(0, 0), (0.04, 1), (d * 0.5, 0.5), (d, 0)])
    add(out, sag, 0, 0.5)
    add(out, _crackle(r, d, 18, 1800, 6000, shape=1.8), 0, 0.5)
    add(out, hl_ping(r, note('E5'), 0.08, 0.8, glint=0.3), 0.01, 0.25)
    return hp(out, 90, 2)


@sound('storm_mark', 'construct', 2, pitch=P_T10, lufs=-19, max_dur=0.62,
       notes='LEGEND storm telegraph (0.5 s): an ominous low swell with quickening tremolo, peaking as it lands')
def storm_mark(r, v):
    d = 0.52
    n = ns(d)
    t = tvec(n)
    trem = 0.6 + 0.4 * np.sin(TAU * np.cumsum(sweep(n, 6, 26, 0.8)) / SR)
    env = env_pts(n, [(0, 0), (d * 0.9, 1), (d * 0.97, 0.45), (d, 0)], 2.0)
    base = [note('E2'), note('B1')][v]
    x = hl_tone(r, base * 2, n, env, bright=0.9, shimmer=0.6) + 0.8 * hl_tone(r, base * 3, n, env, bright=1.0)
    x += 0.7 * sine(sweep(n, base, base * 1.5, 1.5), n) * env  # rising under-tone (a fifth)
    # a high thin wire of tension (in key) rising in the last half
    x += 0.12 * sine(sweep(n, note('B5'), note('E6'), 1.0), n) * env_pts(n, [(0, 0), (d * 0.5, 0), (d * 0.95, 1), (d, 0)])
    return normpk(x * trem)


@sound('storm_bolt', 'combat', 3, pitch=P_SOFT, lufs=-11,
       notes='LEGEND lightning: tearing crack (dense crackle, 2 strokes) + arc buzz + sub + rolling thunder')
def storm_bolt(r, v):
    d = 2.6
    out = buf(d)
    add(out, sat(click(r, 2500, 0.004), 2.5), 0, 1.0)
    add(out, _crackle(r, 0.06, 110, 2500, 9500, (0.001, 0.003), shape=1.3), 0, 1.3)
    m = ns(0.09)
    add(out, fm(sweep(m, 4200, 1800, 0.5), 1.0, 3.0, m) * env_exp(m, 0.02, 0.0002), 0, 0.35)  # the tearing zzap
    t2 = U(r, 0.05, 0.08)
    add(out, sat(click(r, 3000, 0.003), 2.0), t2, 0.7)
    add(out, _crackle(r, 0.025, 40, 3000, 9500, (0.001, 0.003)), t2, 0.6)
    add(out, _buzz(r, 0.18, 180, 90, 3.0, 150) * env_exp(ns(0.18), 0.05, 0.0005), 0, 0.5)
    add(out, thump(1.2, 95 * U(r, .92, 1.08), 30, 0.05, 0.25, drive=3.2, attack=0.001), 0, 1.3)
    # thunder: rolling brown rumble with a couple of swells (physical noise, low-passed)
    n = ns(2.3)
    swells = env_pts(n, [(0, 0), (0.1, 1), (0.5, 0.55), (0.8 + 0.2 * v, 0.8), (1.3, 0.4), (2.3, 0)], 1.3)
    th = normpk(tvf(brown(n, r), glide(n, 700, 150, 0.35), 0.7, 'lp')) * (1 + 0.6 * smooth_noise(n, r, 7)) * swells
    add(out, th, 0.04, 0.75)
    return carve_heavy(glue(out, 2.4), -2, 0)


@sound('storm_meteor_whistle', 'combat', 2, pitch=P_SOFT, lufs=-18, max_dur=0.62,
       notes='LEGEND meteor rain incoming: 3 falling whistles + a growing low roar, timed to land at 0.5 s')
def storm_meteor_whistle(r, v):
    d = 0.55
    out = buf(d)
    for i, (t0, top) in enumerate([(0.0, 2600), (0.06, 2200), (0.13, 3000)]):
        dd = d - t0
        n = ns(dd)
        f = sweep(n, top * U(r, 0.9, 1.1), top * 0.3, 1.2)
        w = sine(f, n) + 0.25 * sine(2 * f, n)
        air = normpk(tvf(pink(n, r), f * 1.02, 14, 'bp'))
        e = env_pts(n, [(0, 0), (dd * 0.3, 0.6), (dd * 0.95, 1), (dd, 0)], 1.2)
        add(out, (0.6 * w + 0.5 * air) * e, t0, [0.5, 0.4, 0.3][i])
    n = ns(d)
    roar = _fireball(r, d, 400, 900, 0.3, 10.0) * _swell(n, 2.5)
    add(out, roar, 0, 0.5)
    return hp(out, 80, 2)


@sound('storm_meteor_hit', 'combat', 4, pitch=P_IMPACT, lufs=-16,
       notes='LEGEND meteor drop (layered on the explosion): rock crunch + a fire whump, no second boom')
def storm_meteor_hit(r, v):
    out = buf(0.9)
    add(out, sat(click(r, 1600, 0.004), 2.0), 0, 0.7)
    add(out, shatter(r, 'stone', [5, 7, 4, 6][v], 0.25, U(r, 1.0, 1.5), 1.6), 0, 0.8)
    add(out, _fireball(r, 0.6, 3500, 300, 0.07, 0.12), 0.005, 0.5)
    add(out, _crackle(r, 0.4, 16, 1500, 5000, (0.002, 0.005), shape=1.5), 0.02, 0.25)  # embers
    add(out, thump(0.3, 180, 80, 0.02, 0.05, drive=1.8), 0, 0.5)
    return carve_heavy(glue(out, 1.5), -3, -3)


@sound('storm_icicle', 'combat', 4, pitch=P_IMPACT, lufs=-15, max_dur=1.1,
       notes='LEGEND icicle spear: sharp crack + ice shatter (tuned crystal) + glassy shards + small thud')
def storm_icicle(r, v):
    out = buf(1.0)
    add(out, click(r, 5000, 0.0015), 0, 0.8)
    add(out, shatter(r, 'crystal', [7, 9, 6, 8][v], 0.35, U(r, 0.7, 1.1), 1.5), 0, 0.8)
    add(out, shatter(r, 'glass', [5, 6, 4, 6][v], 0.3, 1.0, 1.3), 0.002, 0.45)
    f = U(r, 2600, 3400)  # the spear's own snap: a high, very short ice mode cluster
    add(out, modal([f, f * 1.5, f * 2.2], [0.02, 0.012, 0.008], [1, .6, .4], ns(0.12), r), 0, 0.5)
    add(out, thump(0.3, 200, 90, 0.015, 0.05, drive=1.6), 0, 0.55)
    return glue(out, 1.5)


@sound('storm_tornado', 'combat', 2, pitch=P_SOFT, lufs=-13, max_dur=2.3,
       notes='LEGEND tornado (lasts the ~1.6 s linger): vortex roar + wobbling whistle + rotating whum + debris')
def storm_tornado(r, v):
    d = 2.1
    n = ns(d)
    t = tvec(n)
    env = env_pts(n, [(0, 0), (0.18, 1), (1.6, 0.9), (d, 0)], 1.2)
    spin = 0.7 + 0.3 * np.sin(TAU * np.cumsum(sweep(n, 2.5, 5.0, 0.6)) / SR)  # rotating whum
    body = normpk(lp(tvf(brown(n, r) + 0.6 * pink(n, r), 480 * 2 ** (0.5 * smooth_noise(n, r, 2.0)), 2.2, 'bp'), 2200, 2))
    low = normpk(lp(brown(n, r), 110)) * (1 + 0.4 * smooth_noise(n, r, 3))
    wf = [880, 740][v] * 2 ** (0.25 * np.sin(TAU * 0.9 * t) + 0.1 * smooth_noise(n, r, 2))
    whistle = normpk(tvf(pink(n, r), wf, 30, 'bp'))
    drone = sine(note('E2') * (1 + 0.04 * np.sin(TAU * 0.9 * t)), n) + 0.5 * sine(note('B2') * (1 + 0.04 * np.sin(TAU * 0.9 * t)), n)
    x = (body * spin + 0.55 * low + 0.3 * whistle * spin + 0.25 * drone * spin) * env
    x = normpk(x)
    add(x, grit(r, 40, 0.5, 900, 3200, 0.004, n_len=n), 0.1, 0.12)  # debris in the funnel
    return carve_heavy(glue(x, 1.3), -3, -3)


@sound('storm_geyser', 'combat', 3, pitch=P_MICRO, lufs=-12,
       notes='LEGEND geyser: pressure thump + rising water column rush + gurgle bloops + falling droplet patter')
def storm_geyser(r, v):
    d = 1.5
    out = buf(d)
    add(out, thump(0.8, 110 * U(r, .92, 1.08), 38, 0.05, 0.2, drive=2.8), 0, 1.1)
    n = ns(0.9)
    rush = normpk(lp(tvf(pink(n, r) + 0.5 * brown(n, r), sweep(n, 350, 1400, 0.7), 1.6, 'bp'), 3500, 2))
    rush *= (1 + 0.5 * smooth_noise(n, r, 35)) * env_pts(n, [(0, 0), (0.05, 1), (0.5, 0.7), (0.9, 0)])
    add(out, rush, 0.01, 0.55)
    for _ in range(int(U(r, 10, 16))):  # gurgles: short upward sine bloops (bubbles)
        m = ns(U(r, 0.025, 0.05))
        f0 = U(r, 180, 520)
        b = sine(sweep(m, f0, f0 * U(r, 1.8, 2.6), 0.7), m) * np.hanning(m)
        add(out, b, U(r, 0.0, 0.5), U(r, 0.15, 0.4))
    for _ in range(int(U(r, 30, 45))):  # droplets raining back down
        tt = 0.35 + 0.9 * r.uniform() ** 0.8
        add(out, micro(r, U(r, 1500, 4200), U(r, 0.002, 0.005), 2, (1.5, 2.3)), tt, U(r, 0.08, 0.3) * (1.4 - tt / 1.3))
    return carve_heavy(glue(out, 1.5), -3, -3)


@sound('storm_crush', 'combat', 3, pitch=P_SOFT, lufs=-11, max_dur=2.0,
       notes='LEGEND psychic crush: a pressure suck-in, then a deep slam + a warped low ring pressing down')
def storm_crush(r, v):
    d = 1.6
    out = buf(d)
    pre = 0.14
    m = ns(pre)
    suck = (sine(sweep(m, note('E1'), note('E2'), 2.0), m) + 0.4 * sine(sweep(m, note('B2'), note('E4'), 2.0), m)) * _swell(m, 2.5)
    add(out, suck, 0, 0.5)
    add(out, sat(click(r, 1200, 0.005), 2.0), pre, 0.6)
    add(out, thump(1.5, 75 * U(r, .92, 1.08), 26, 0.09, 0.42, drive=3.5, attack=0.002), pre, 1.35)
    n = ns(1.2)
    ring = fm(sweep(n, [98.0, 110.0, 92.5][v], 70.0, 0.6), 1.41, 3.0 * env_exp(n, 0.2), n) * env_exp(n, 0.35, 0.004)
    beat = sine(sweep(n, note('E2'), note('E2') * 0.94, 0.5), n) * sine(np.full(n, 3.0), n) * env_exp(n, 0.4, 0.01)
    add(out, ring + 0.6 * beat, pre, 0.55)
    add(out, _rumble(r, 1.3, 90, 0.4, 5, 0.01), pre, 0.45)
    return carve_heavy(glue(out, 1.8))


@sound('storm_spikes', 'combat', 3, pitch=P_MICRO, lufs=-11,
       notes='LEGEND earth spikes: a rising grind, then stone spikes bursting up one after another + rumble')
def storm_spikes(r, v):
    d = 1.4
    out = buf(d)
    add(out, grit(r, 40, 0.06, 500, 2400, 0.005), 0, 0.35)  # the ground grinding open
    add(out, _rumble(r, 1.2, 110, 0.35, 6, 0.02), 0, 0.5)
    t = 0.07
    for i in range([4, 5, 3][v]):
        size = U(r, 1.6, 2.6) * (1 - 0.1 * i)
        add(out, material_hit(r, 'stone', size, 1.8), t, 0.9 - 0.1 * i)
        add(out, thump(0.4, U(r, 120, 170), 45, 0.03, 0.09, drive=2.5), t, 1.0 - 0.12 * i)
        add(out, grit(r, 12, 0.02, 1400, 4200, 0.003), t, 0.3)
        t += U(r, 0.05, 0.1)
    add(out, shatter(r, 'stone', 10, 0.5, 0.8, 1.0), 0.15, 0.3)
    return carve_heavy(glue(out, 1.7))


@sound('storm_maw', 'combat', 2, pitch=P_SOFT, lufs=-13, max_dur=2.0,
       notes='LEGEND shadow maw: hollow growl drone, two jaw chomps, grinding chew, a swallowing suck')
def storm_maw(r, v):
    d = 1.8
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    f0 = [note('E1'), note('D1')][v] * (1 + 0.02 * smooth_noise(n, r, 6))
    growl = 1 + 0.6 * np.sin(TAU * np.cumsum(20 + 6 * smooth_noise(n, r, 2)) / SR)
    src = saw(f0 * 2, n) + 0.6 * saw(f0 * 3.02, n)
    body = tvf(src * growl, 420 + 180 * np.sin(TAU * 0.7 * t), 5, 'bp') + 0.5 * lp(src, 180)
    add(out, normpk(body) * env_pts(n, [(0, 0), (0.1, 1), (1.4, 0.8), (d, 0)]), 0, 0.6)
    for tj in (0.02, [0.62, 0.75][v]):  # jaws: thump + a bony crunch
        add(out, thump(0.5, 140, 40, 0.03, 0.12, drive=3.0), tj, 1.0)
        add(out, material_hit(r, 'wood', 1.4, 1.6), tj, 0.5)
        add(out, grit(r, 14, 0.03, 700, 2200, 0.005), tj + 0.01, 0.35)
    m = ns(0.35)
    add(out, normpk(lp(brown(m, r), 400)) * _swell(m, 2.0), 1.3, 0.3)  # the swallow
    return carve_heavy(glue(out, 1.6))


@sound('storm_nova', 'combat', 2, pitch=P_SOFT, lufs=-13,
       notes='LEGEND atomic nova (layered on the explosion): suck-in rise, then a searing crackling ring-out')
def storm_nova(r, v):
    d = 1.6
    out = buf(d)
    pre = 0.22
    m = ns(pre)
    add(out, sine(sweep(m, 200, 1600, 2.0), m) * _swell(m, 3.0), 0, 0.45)
    add(out, _crackle(r, pre, 30, 2500, 8000, grow=True), 0, 0.4)
    n = ns(1.3)
    sear = fm(sweep(n, note('E5'), note('E4'), 0.5), 2.0, 4.0 * env_exp(n, 0.15), n) * env_exp(n, 0.3, 0.001)
    add(out, sear, pre, 0.5)
    add(out, hl_chord(r, ['E5', 'B5', 'E6', 'B6'], dec=0.25, bright=1.4, stagger=0.002), pre, 0.35)
    add(out, _crackle(r, 0.9, 70, 2000, 9000, shape=1.6), pre, 0.55)
    add(out, thump(0.8, 150, 40, 0.03, 0.15, drive=2.5), pre, 0.8)
    return glue(out, 1.6)


@sound('storm_miasma', 'combat', 2, pitch=P_SOFT, lufs=-18, max_dur=2.0,
       notes='LEGEND poison miasma: gas vent hiss (band-limited, grained) + slow bubbling + a sickly detuned drone')
def storm_miasma(r, v):
    d = 1.8
    n = ns(d)
    out = buf(d)
    env = env_pts(n, [(0, 0), (0.12, 1), (1.3, 0.7), (d, 0)])
    gas = normpk(bp(pink(n, r), 900, 3200)) * (0.6 + 0.4 * smooth_noise(n, r, 9)) * env
    add(out, gas, 0, 0.3)
    f = note('A2') if v == 0 else note('G2')
    drone = sine(f, n) + 0.8 * sine(f * 1.06, n) + 0.3 * sine(f * 2.03, n)  # detuned - wrong, sickly
    add(out, drone * env, 0, 0.35)
    for _ in range(int(U(r, 16, 24))):  # thick bubbles popping
        m = ns(U(r, 0.04, 0.08))
        f0 = U(r, 110, 320)
        b = sine(sweep(m, f0, f0 * U(r, 1.5, 2.2), 0.6), m) * np.hanning(m)
        add(out, b, U(r, 0.05, 1.5), U(r, 0.2, 0.5))
    add(out, thump(0.3, 160, 70, 0.02, 0.06, drive=1.5), 0, 0.5)
    return hp(out, 60, 2)


@sound('storm_arc', 'combat', 4, pitch=P_MICRO, lufs=-18, max_dur=0.3,
       notes='LEGEND lightning arcs on to the next enemy: a short zap')
def storm_arc(r, v):
    return _zap(r, [0.14, 0.12, 0.17, 0.1][v], [120, 150, 100, 170][v])


# =============================================================================== RICOCHET
@sound('ricochet_ding', 'combat', 3, pitch=P_T10, lufs=-17, max_dur=0.8,
       notes='LEGEND ricochet bounce: bright steel ding tuned on E6 + a spark (game pitches it up the pentatonic per bounce)')
def ricochet_ding(r, v):
    out = buf(0.7)
    f = note('E6')
    ratios = [1.0, 2.0, 2.76, 3.0, 5.4]
    decs = [0.22, 0.12, 0.05, 0.08, 0.02]
    amps = [1.0, 0.35, 0.3, 0.2, 0.12]
    add(out, modal([f * k for k in ratios], decs, amps, ns(1.2), r, attack=0.0002), 0, 1.0)
    add(out, hl_ping(r, note(['B6', 'E7', 'G6'][v]), 0.06, 1.2, glint=0.8), 0, 0.3)
    add(out, click(r, 4000, 0.0015), 0, 0.5)
    add(out, _crackle(r, 0.08, 10, 3500, 9000, shape=1.5), 0.002, 0.3)
    add(out, thump(0.1, 400, 220, 0.01, 0.02, drive=1.2), 0, 0.3)
    return hp(out, 250, 2)


# =============================================================================== DEFLECT (the lightsaber)
_HUM_F = note('E2') * 1.05  # ~86.5 Hz projector hum


def _saber_tone(r, f, n, bright=1.0):
    """Hum voice: a rich harmonic series (motor buzz) with a slight upper 'interference' partial."""
    x = np.zeros(n)
    for k in range(1, 22):
        a = (1.0 / k ** 1.15) * (bright ** (k / 8))
        if k in (2, 3):
            a *= 1.3
        x += a * sine(f * k, n, r.uniform(0, TAU))
    return x


@sound('saber_hum_loop', 'construct', 1, loop=True, pitch=P_TONAL, lufs=-26,
       notes='LEGEND loop: energy blade idle hum (Blade + Fast, Electric / Light) - motor harmonics + slow beat; '
             'the game pitches / swells it with blade speed')
def saber_hum_loop(r, v):
    Ld = 2.0
    L = ns(Ld)

    def render(m):
        t = tvec(m)
        f = qf(_HUM_F, Ld)
        x = np.zeros(m)
        for k in range(1, 22):
            a = 1.0 / k ** 1.15 * (1.3 if k in (2, 3) else 1.0)
            fk = f * k
            x += a * np.sin(TAU * fk * t + r.uniform(0, TAU))
            x += 0.35 * a * np.sin(TAU * (fk + 1.5 / Ld * 2) * t + r.uniform(0, TAU))  # slow beat (1.5 Hz)
        am = 1 + 0.08 * np.sin(TAU * (6 / Ld) * t) + 0.05 * np.sin(TAU * (23 / Ld) * t)
        whine = 0.03 * np.sin(TAU * qf(note('E6'), Ld) * t) * (1 + 0.5 * np.sin(TAU * (4 / Ld) * t))
        return x * am + whine
    x = periodic(render, L)
    return circ(lambda y: lp(hp(y, 55, 2), 6000, 2), x)


@sound('saber_swoosh', 'construct', 4, pitch=P_MICRO, lufs=-19, max_dur=0.5,
       notes='LEGEND energy blade swing: doppler swoosh - the hum pitched up and back down past you + a little air')
def saber_swoosh(r, v):
    d = [0.34, 0.28, 0.4, 0.3][v]
    n = ns(d)
    t = tvec(n)
    c = d * U(r, 0.4, 0.5)
    w = d * [0.22, 0.18, 0.25, 0.2][v]
    bell = np.exp(-((t - c) / w) ** 2)
    f = _HUM_F * (1 + [0.55, 0.7, 0.45, 0.6][v] * bell) * (1 + 0.15 * np.tanh((t - c) / (w * 0.5)) * -1)
    x = np.zeros(n)
    for k in range(1, 16):
        x += (1.0 / k ** 1.0) * sine(f * k, n, r.uniform(0, TAU))
    out = buf(d)
    add(out, normpk(x) * bell ** 0.8, 0, 0.9)
    add(out, _air(r, d, 600, 2600, 1.6, [(0, 0), (c, 1), (d, 0)]), 0, 0.2)
    return hp(out, 60, 2)


@sound('saber_deflect', 'combat', 3, pitch=P_MICRO, lufs=-15, max_dur=0.8,
       notes='LEGEND energy blade deflect: a sizzling ping - bright FM strike + crackling sizzle + hum flare')
def saber_deflect(r, v):
    out = buf(0.7)
    add(out, sat(click(r, 3500, 0.002), 2.0), 0, 0.8)
    n = ns(0.5)
    ping = fm(note(['E6', 'B5', 'G6'][v]), 1.41, 2.5 * env_exp(n, 0.02), n) * env_exp(n, 0.1, 0.0003)
    add(out, ping, 0, 0.7)
    add(out, hl_ping(r, note('E7'), 0.05, 1.3, glint=1.0), 0, 0.25)
    add(out, _crackle(r, 0.3, 55, 2500, 9000, (0.0008, 0.0025), shape=1.5), 0, 0.7)  # the sizzle
    m = ns(0.35)
    flare = _saber_tone(r, _HUM_F * 1.6, m) * env_pts(m, [(0, 1), (0.35, 0)], 2.0)
    add(out, normpk(flare), 0, 0.4)
    add(out, thump(0.12, 300, 160, 0.01, 0.03, drive=1.5), 0, 0.35)
    return hp(out, 90, 2)


# =============================================================================== BLADE WAVE
@sound('blade_wave', 'combat', 3, pitch=P_MICRO, lufs=-15, max_dur=0.9,
       notes='LEGEND crescent slash launched: a falling energy "shing" + a heavy air slice + a low push '
             '(the game pitches it down for bigger waves)')
def blade_wave(r, v):
    d = 0.6
    n = ns(d)
    out = buf(d)
    add(out, click(r, 3000, 0.002), 0, 0.6)
    top = note(['E7', 'B6', 'D7'][v])
    shing = fm(sweep(n, top, top / 4, 0.5), 2.0, 1.5 * env_exp(n, 0.08), n) * env_exp(n, 0.14, 0.0005)
    add(out, shing, 0, 0.6)
    add(out, hl_ping(r, note('E5'), 0.12, 1.1, glint=0.8), 0.005, 0.4)
    add(out, _air(r, d, 3200, 500, 1.8, [(0, 0), (0.03, 1), (d, 0)]), 0, 0.35)
    add(out, thump(0.3, 140, 70, 0.03, 0.07, drive=1.8), 0, 0.5)
    return hp(out, 60, 2)


@sound('blade_wave_loop', 'combat', 1, loop=True, pitch=P_TONAL, lufs=-20,
       notes='LEGEND loop carried along a flying crescent: fluttering tonal whirr (E) + cutting air')
def blade_wave_loop(r, v):
    Ld = 1.0
    L, X = ns(Ld), ns(0.2)
    n = L + X
    air = normpk(tvf(pink(n, r), 1600 * 2 ** (0.3 * smooth_noise(n, r, 3)), 2.0, 'bp'))
    air = loop_xfade(air, L, X)

    def render(m):
        t = tvec(m)
        flut = 0.6 + 0.4 * np.sin(TAU * qf(18.0, Ld) * t)
        return flut * (np.sin(TAU * qf(note('E4'), Ld) * t) + 0.6 * np.sin(TAU * qf(note('B4'), Ld) * t)
                       + 0.3 * np.sin(TAU * qf(note('E5'), Ld) * t))
    tone = periodic(render, L)
    return circ(lambda y: hp(y, 120, 2), 0.5 * air + 0.5 * normpk(tone))


# =============================================================================== HEAVY BLADE
@sound('leap_whoosh', 'combat', 2, pitch=P_MICRO, lufs=-17, max_dur=0.8,
       notes='LEGEND heavy-blade leap: a big body of air shoved upward + armour rattle + push thump')
def leap_whoosh(r, v):
    d = 0.65
    out = buf(d)
    add(out, thump(0.4, 110, 55, 0.04, 0.1, drive=2.2), 0, 0.8)
    add(out, lp(_air(r, d, 250, [950, 1200][v], 1.4, [(0, 0), (0.08, 1), (d, 0)], color='brown'), 2500, 2), 0, 0.9)
    add(out, _chain_rattle(r, 0.25, 12, 1600, 4000, 0.3), 0.01, 1.0)
    return hp(out, 50, 2)


@sound('leap_slam', 'combat', 3, pitch=P_SOFT, lufs=-9,
       notes='LEGEND heavy-blade cleave landing: massive blade-into-ground impact, crater debris, long rumble')
def leap_slam(r, v):
    d = 2.8
    out = buf(d)
    add(out, sat(click(r, 1400, 0.006), 2.5), 0, 1.0)
    add(out, _metal(r, U(r, 240, 320), (0.12, 0.08, 0.05, 0.1, 0.06, 0.04), 1.8), 0, 0.5)  # the blade (muted by earth)
    add(out, material_hit(r, 'stone', 3.0, 2.0), 0, 0.7)
    add(out, thump(2.5, 62 * U(r, .92, 1.08), 24, 0.14, 0.55, drive=3.4, attack=0.003), 0, 1.35)
    add(out, _rumble(r, 2.5, 120, 0.8, 4, 0.04), 0.02, 0.5)
    add(out, shatter(r, 'stone', [16, 20, 14][v], 1.0, 1.3, 1.3), 0.03, 0.35)
    return carve_heavy(glue(out, 2.2))


@sound('armor_clang', 'combat', 4, pitch=P_IMPACT, lufs=-16, max_dur=0.9,
       notes='LEGEND super armour soaked a hit: a hard metallic clang + a low stagger thump (the wielder does not budge)')
def armor_clang(r, v):
    out = buf(0.8)
    add(out, sat(click(r, 3000, 0.002), 2.0), 0, 0.8)
    f = U(r, 300, 460) * [1.0, 0.9, 1.1, 0.8][v]
    add(out, _metal(r, f, (0.35, 0.2, 0.08, 0.25, 0.15, 0.06), 1.5), 0, 0.8)
    add(out, thump(0.3, 160, 80, 0.02, 0.06, drive=2.0), 0, 0.6)
    return hp(out, 80, 2)


# =============================================================================== CHAIN BLADES
@sound('chain_sweep', 'combat', 2, pitch=P_MICRO, lufs=-14, max_dur=0.9,
       notes='LEGEND chain blades 360 sweep: chains whirling one full circle (rattle + air sweeping round) + blade hum')
def chain_sweep(r, v):
    d = 0.6
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    circle = 0.5 - 0.5 * np.cos(TAU * t / d)  # one pass around you: loudest behind / in front
    add(out, _chain_rattle(r, d, 90, 1300, 4200, 0.8, dens=lambda u: 0.3 + 0.7 * np.sin(np.pi * u)), 0, 1.0)
    band = sweep(n, 600, 2200, 1.0) * (1 + 0.5 * np.sin(TAU * t / d))
    air = normpk(tvf(pink(n, r), band, 1.6, 'bp')) * circle
    add(out, air, 0, 0.6)
    f = 520 * (1 + 0.25 * np.sin(TAU * t / d))  # the blades singing past (doppler around)
    add(out, (sine(f, n) + 0.4 * sine(f * 2.76, n)) * circle, 0, 0.15)
    return carve_foley(out, 140)


@sound('chain_fire_roar', 'combat', 2, pitch=P_MICRO, lufs=-18, max_dur=1.0,
       notes='LEGEND flaming chain sweep layer: a fire roar whooshing round + ember crackle')
def chain_fire_roar(r, v):
    d = 0.8
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    flame = _fireball(r, d, 2500, 700, 0.4, 0.5) * (0.6 + 0.4 * np.sin(np.pi * t / d))
    add(out, flame, 0, 0.8)
    add(out, _crackle(r, d, 30, 1200, 4500, (0.002, 0.006), shape=1.2), 0, 0.35)
    return hp(out, 70, 2)


@sound('chain_pull', 'combat', 3, pitch=P_MICRO, lufs=-15, max_dur=0.8,
       notes='LEGEND chain blades latch + yank: links rattling taut, a steel snap, the chain twang, a heave')
def chain_pull(r, v):
    out = buf(0.7)
    add(out, _chain_rattle(r, 0.07, 30, 1500, 4500, 0.8), 0, 1.0)
    add(out, sat(click(r, 2500, 0.003), 2.0), 0.07, 0.9)
    add(out, _clank(r, U(r, 900, 1300), 0.03), 0.07, 0.6)
    add(out, string_twang(r, U(r, 95, 130), 0.12, 6, 0.004), 0.07, 0.6)
    add(out, thump(0.3, 130, 60, 0.03, 0.08, drive=2.2), 0.07, 0.8)
    add(out, _air(r, 0.3, 400, 1400, 1.4, [(0, 0), (0.05, 1), (0.3, 0)]), 0.08, 0.35)
    return hp(out, 70, 2)


# =============================================================================== LEGENDARY GUNS
@sound('golden_gun', 'weapon', 2, pitch=P_SOFT, lufs=-10,
       notes='LEGEND one-shot gun: one huge dry report + a golden ring-out (tuned steel, E minor) - no room tail')
def golden_gun(r, v):
    out = buf(1.6)
    add(out, _snap(r, 1.0), 0, 1.0)
    add(out, _report(r, 200, 2400, 0.008), 0, 0.85)
    add(out, thump(1.2, 120 * U(r, .95, 1.05), 40, 0.035, 0.18, drive=3.4, attack=0.001), 0, 1.45)
    add(out, thump(0.12, 280, 130, 0.012, 0.025, drive=1.5), 0, 0.45)
    add(out, _clank(r, U(r, 1600, 2000), 0.02), 0.0, 0.3)
    # the golden ring-out: a tuned steel bell (E5 + B5 + E6 family), bright and slow-ish, dry
    base = note(['E5', 'B4'][v])
    n = ns(1.4)
    bell = modal([base, base * 2.0, base * 3.0, base * 4.2, base * 5.4], [0.55, 0.35, 0.25, 0.12, 0.06],
                 [1.0, 0.5, 0.35, 0.18, 0.1], n, r)
    add(out, bell, 0.004, 0.5)
    add(out, hl_ping(r, note('E6'), 0.3, 1.2, glint=0.8), 0.006, 0.25)
    return glue(out, 1.6)


@sound('needle_tick', 'weapon', 4, pitch=P_T10, lufs=-21, max_dur=0.35,
       notes='LEGEND needle stuck in: a glassy crystal tick (game pitches it up per needle)')
def needle_tick(r, v):
    out = buf(0.3)
    f = note(['E6', 'G6', 'B6', 'D7'][v])
    add(out, modal([f, f * 2.0, f * 3.0, f * 4.16], [0.06, 0.035, 0.02, 0.012], [1, .5, .3, .15], ns(0.35), r), 0, 1.0)
    add(out, click(r, 5000, 0.0008), 0, 0.5)
    add(out, micro(r, U(r, 2000, 3200), 0.006, 3), 0, 0.3)
    return hp(out, 400, 2)


@sound('supercombine', 'combat', 3, pitch=P_T10, lufs=-12, max_dur=1.5,
       notes='LEGEND needles supercombine: a crystalline burst (tuned crystal shatter + pink shard run + chord bloom)')
def supercombine(r, v):
    out = buf(1.5)
    add(out, sat(click(r, 3500, 0.003), 2.0), 0, 0.8)
    add(out, shatter(r, 'crystal', [14, 18, 12][v], 0.5, 0.9, 1.8), 0, 0.8)
    add(out, hl_chord(r, [['E5', 'B5', 'E6', 'G6'], ['G5', 'B5', 'E6', 'B6'], ['E5', 'G5', 'B5', 'E6']][v],
                      dec=0.2, bright=1.4, stagger=0.003), 0, 0.5)
    add(out, hl_shards(r, 5, 0.5, 'E8', 'E6', amp=0.4), 0.02, 1.0)
    add(out, thump(0.5, note('E3'), note('E1'), 0.04, 0.12, drive=2.4), 0, 0.9)
    return glue(out, 1.4)


@sound('grav_grab', 'weapon', 2, pitch=P_T10, lufs=-18, max_dur=0.8,
       notes='LEGEND gravity gun catch: latch clunk + a tractor hum swelling in (rising zwoop, wobble)')
def grav_grab(r, v):
    d = 0.6
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, _clank(r, U(r, 700, 900), 0.03), 0, 0.5)
    f = note(['E2', 'B1'][v])
    hum = (sine(f, n) + 0.6 * sine(2 * f, n) + 0.4 * sine(3 * f, n) + 0.2 * sine(4.01 * f, n))
    hum *= (1 + 0.25 * np.sin(TAU * 7 * t)) * env_pts(n, [(0, 0), (0.08, 1), (d, 0.0)], 1.2)
    add(out, hum, 0.01, 0.6)
    add(out, sine(sweep(n, 300, 1100, 0.6), n) * env_pts(n, [(0, 0), (0.05, 1), (0.25, 0)]), 0, 0.2)
    add(out, thump(0.2, 180, 90, 0.02, 0.05, drive=1.5), 0, 0.5)
    return hp(out, 60, 2)


@sound('grav_punt', 'weapon', 3, pitch=P_MICRO, lufs=-12,
       notes='LEGEND gravity gun punt: a huge air-pressure thump + release whoosh + mechanism clank')
def grav_punt(r, v):
    out = buf(0.8)
    add(out, sat(click(r, 1500, 0.005), 2.0), 0, 0.7)
    add(out, thump(0.7, 140 * U(r, .92, 1.08), 42, 0.04, 0.14, drive=3.2, attack=0.001), 0, 1.3)
    add(out, _air(r, 0.35, 1500, 350, 1.3, [(0, 0), (0.01, 1), (0.35, 0)], color='brown'), 0, 0.6)
    add(out, _clank(r, U(r, 1100, 1500), 0.025), 0.03, 0.35)
    add(out, sine(sweep(ns(0.2), 900, 200, 0.5), ns(0.2)) * env_exp(ns(0.2), 0.05), 0, 0.2)
    return carve_heavy(glue(out, 1.6), -3, -3)


@sound('stream_loop', 'weapon', 1, loop=True, pitch=P_TONAL, lufs=-20,
       notes='LEGEND loop: proton stream latched - raw wobbling FM buzz + irregular crackle + thin whine')
def stream_loop(r, v):
    Ld = 2.0
    L = ns(Ld)

    def render(m):
        t = tvec(m)
        f = qf(note('A2'), Ld) * (1 + 0.06 * np.sin(TAU * (7 / Ld) * t) + 0.03 * np.sin(TAU * (19 / Ld) * t))
        idx = 2.5 + 1.2 * np.sin(TAU * (3 / Ld) * t)
        x = fm(f, 1.0, idx, m) + 0.6 * fm(f * 1.5, 1.0, idx * 0.7, m)
        wh = 0.06 * np.sin(TAU * qf(note('E6'), Ld) * t + 2.0 * np.sin(TAU * (11 / Ld) * t))
        return x * (0.8 + 0.2 * np.sin(TAU * (31 / Ld) * t)) + wh
    x = normpk(periodic(render, L))
    for _ in range(90):
        add_circ(x, micro(r, U(r, 2000, 8000), U(r, 0.0012, 0.004), 3, (1.3, 2.3)), int(r.uniform(0, L)), U(r, 0.05, 0.25))
    return circ(lambda y: hp(y, 70, 2), x)


@sound('stream_snap', 'weapon', 3, pitch=P_MICRO, lufs=-16, max_dur=0.5,
       notes='LEGEND proton stream let go / broke: a crackling snap + falling whine')
def stream_snap(r, v):
    out = buf(0.4)
    add(out, sat(click(r, 2500, 0.003), 2.0), 0, 0.8)
    add(out, _crackle(r, 0.12, 40, 2000, 9000, shape=1.6), 0, 0.7)
    n = ns(0.3)
    add(out, fm(sweep(n, note('E5'), note('E3'), 0.6), 1.0, 2.0, n) * env_exp(n, 0.08), 0, 0.4)
    add(out, thump(0.2, 200, 90, 0.02, 0.04, drive=1.6), 0, 0.5)
    return hp(out, 80, 2)


@sound('gun_charge_loop', 'weapon', 1, loop=True, pitch=P_TONAL, lufs=-22,
       notes='LEGEND loop: charge-shot capacitor whine (E) + tremolo + crackle; the game raises its pitch 1.0 -> 1.5x')
def gun_charge_loop(r, v):
    Ld = 1.0
    L = ns(Ld)

    def render(m):
        t = tvec(m)
        trem = 0.75 + 0.25 * np.sin(TAU * qf(14.0, Ld) * t)
        return trem * (np.sin(TAU * qf(note('E4'), Ld) * t) + 0.6 * np.sin(TAU * qf(note('B4'), Ld) * t)
                       + 0.35 * np.sin(TAU * qf(note('E5'), Ld) * t) + 0.3 * np.sin(TAU * qf(note('E3'), Ld) * t))
    x = normpk(periodic(render, L))
    for _ in range(35):
        add_circ(x, micro(r, U(r, 2500, 7500), U(r, 0.0015, 0.004), 3, (1.3, 2.2)), int(r.uniform(0, L)), U(r, 0.04, 0.14))
    return circ(lambda y: hp(y, 100, 2), x)


@sound('gun_charge_full', 'weapon', 2, pitch=P_T10, lufs=-18, max_dur=0.8,
       notes='LEGEND charge shot full: a bright E-minor hard-light chime + a small latch')
def gun_charge_full(r, v):
    out = buf(0.7)
    add(out, click(r, 4000, 0.0015), 0, 0.5)
    add(out, hl_chord(r, [['E6', 'B6', 'E7'], ['B5', 'E6', 'G6']][v], dec=0.14, bright=1.3, stagger=0.012), 0, 0.8)
    add(out, _clank(r, 2200, 0.015), 0, 0.2)
    return hp(out, 300, 2)


@sound('bfg_spinup', 'weapon', 2, pitch=P_SOFT, lufs=-15, max_dur=0.7,
       notes='LEGEND BFG spin-up (0.5 s): a rising turbine whine + plasma crackle build + a low swell into the shot')
def bfg_spinup(r, v):
    d = 0.52
    n = ns(d)
    out = buf(d)
    f = sweep(n, [180, 150][v], [900, 800][v], 0.8)
    whine = (saw(f, n) * 0.4 + sine(f, n) + 0.5 * sine(2 * f, n)) * env_pts(n, [(0, 0), (d * 0.9, 1), (d, 0.5)], 1.5)
    add(out, lp(whine, 5000), 0, 0.55)
    add(out, _crackle(r, d, 50, 1800, 7000, grow=True, shape=1.3), 0, 0.55)
    add(out, sine(sweep(n, 40, 70, 1.0), n) * _swell(n, 2.0), 0, 0.7)
    add(out, _buzz(r, d, 60, 110, 2.0, 50) * _swell(n, 2.5), 0, 0.3)
    return hp(out, 35, 2)


@sound('charge_shot_tap', 'weapon', 3, pitch=P_MICRO, lufs=-19, max_dur=0.35,
       notes='LEGEND charge cannon tap: a quick plasma pellet - falling FM blip + snap + small thump')
def charge_shot_tap(r, v):
    out = buf(0.3)
    add(out, click(r, 3000, 0.002), 0, 0.6)
    n = ns(0.12)
    add(out, fm(sweep(n, [1800, 1500, 2100][v], 400, 0.5), 1.0, 1.8, n) * env_exp(n, 0.035), 0, 0.7)
    add(out, thump(0.15, 220, 110, 0.015, 0.035, drive=1.8), 0, 0.6)
    return hp(out, 90, 2)


@sound('charge_shot_blast', 'weapon', 2, pitch=P_SOFT, lufs=-11,
       notes='LEGEND charge cannon full blast: plasma discharge - a heavy snap, a down-sweeping roar of FM, crackle, sub')
def charge_shot_blast(r, v):
    out = buf(1.2)
    add(out, _snap(r, 1.0, 2500), 0, 1.0)
    n = ns(0.6)
    add(out, fm(sweep(n, 900, 90, 0.5), 1.0, 3.5 * env_exp(n, 0.15), n) * env_exp(n, 0.16, 0.0005), 0, 0.6)
    add(out, _crackle(r, 0.4, 45, 1800, 8000, shape=1.7), 0, 0.5)
    add(out, thump(1.0, 110 * U(r, .95, 1.05), 36, 0.05, 0.2, drive=3.2, attack=0.001), 0, 1.4)
    add(out, _air(r, 0.4, 2000, 400, 1.2, [(0, 0), (0.01, 1), (0.4, 0)], color='brown'), 0, 0.3)
    return carve_heavy(glue(out, 1.7), -3, -3)


@sound('bfg_zap', 'weapon', 4, pitch=P_MICRO, lufs=-20, max_dur=0.3,
       notes='LEGEND BFG orb tendril: a short, darker, thick plasma zap')
def bfg_zap(r, v):
    return lp(_zap(r, [0.12, 0.1, 0.15, 0.11][v], [70, 85, 60, 95][v], bright=0.7), 7000, 2)
