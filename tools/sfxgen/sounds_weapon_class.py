"""WEAPON FEEL: per-class swing / impact sounds (bd_sim `constructs/weapon_class.rs`, voiced by bd_game
`present/weapon_feel.rs`; keys in `assets/tuning/weapons.ron` -> `feel`).

The class must be audible with your eyes closed:
  * LIGHT   - `swing_swish_light`: a thin, high, fast air cut (a knife through air) + a faint steel sing.
  * MEDIUM  - `swing_whoosh_medium`: a mid whoosh with a short doppler body (the sword / hammer default).
  * HEAVY   - `swing_whoosh_heavy`: a big body of air shoved past you, low and slow, with a heave in front
    (you hear the weight wind up); `heavy_crunch`: the connect layer (a dull bone-deep crunch + sub);
    `ground_crack`: a whiff buried in the floor (stone splitting, grit, a rolling thump).
  * REACH   - `swing_thrust`: a short rising air-dart (a spear's jab), a wooden shaft creak.
  * FLEXIBLE- `whip_crack`: the tip breaking the sound barrier: a sharp supersonic snap with a whistle lead-in.
  * SHIELD  - `shield_bash`: a flat wooden-metal slam + a push thump.

Same rules as everything else (README): dry; noise only where the source is noise (moving air) and then
pink/brown, band-limited, grained and subordinate; impacts carved by role.
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU  # noqa: F401
from materials import click, grit, material_hit, micro, shatter
from sounds import P_IMPACT, P_MICRO, P_SOFT, U, sound, swish


def _air(r, d, f0, f1, q=1.4, pts=None, color='pink'):
    """Moving air (subordinate): noise through a sweeping resonant band, grained."""
    n = ns(d)
    src = pink(n, r) if color == 'pink' else brown(n, r) + 0.4 * pink(n, r)
    y = normpk(tvf(src, sweep(n, f0, f1), q, 'bp')) * (1 + 0.35 * smooth_noise(n, r, 70))
    return y * env_pts(n, pts or [(0, 0), (d * 0.4, 1), (d, 0)])


def _sing(r, f, d, bell_c, bell_w):
    """A faint blade 'sing': two inharmonic partials riding a doppler bell (not a hum)."""
    n = ns(d)
    t = tvec(n)
    bell = np.exp(-((t - bell_c) / bell_w) ** 2)
    ff = f * (1 + 0.08 * np.tanh((bell_c - t) / (bell_w * 0.6)))
    x = sine(ff, n, r.uniform(0, TAU)) + 0.45 * sine(ff * 2.76, n, r.uniform(0, TAU))
    return normpk(x) * bell


# =============================================================================== LIGHT
@sound('swing_swish_light', 'combat', 5, pitch=P_MICRO, lufs=-22, max_dur=0.3,
       notes='WEAPON FEEL light swing: a thin high fast air cut + a faint steel sing (knife, dagger, rapier)')
def swing_swish_light(r, v):
    d = [0.14, 0.12, 0.16, 0.13, 0.15][v]
    out = buf(d + 0.02)
    c = d * U(r, 0.35, 0.5)
    add(out, _air(r, d, U(r, 2200, 2800), U(r, 5200, 6800), 2.4, [(0, 0), (c, 1), (d, 0)]), 0, 0.9)
    add(out, swish(r, d * 0.8, 3500, 8000, 2.0, 140, [(0, 0), (d * 0.3, 1), (d * 0.8, 0)]), 0.01, 0.35)
    add(out, _sing(r, U(r, 2600, 3400), d, c, d * 0.2), 0, 0.12)
    return hp(out, 900, 2)


# =============================================================================== MEDIUM
@sound('swing_whoosh_medium', 'combat', 5, pitch=P_MICRO, lufs=-20, max_dur=0.45,
       notes='WEAPON FEEL medium swing: a mid whoosh with a short doppler body (sword / hammer default)')
def swing_whoosh_medium(r, v):
    d = [0.26, 0.22, 0.3, 0.24, 0.28][v]
    out = buf(d + 0.02)
    c = d * U(r, 0.4, 0.55)
    add(out, _air(r, d, U(r, 700, 950), U(r, 2200, 2900), 1.8, [(0, 0), (c, 1), (d, 0)]), 0, 1.0)
    add(out, lp(_air(r, d, 300, 700, 1.2, [(0, 0), (c, 0.8), (d, 0)], color='brown'), 1500, 2), 0, 0.35)
    add(out, _sing(r, U(r, 1500, 1900), d, c, d * 0.18), 0, 0.06)
    return hp(out, 180, 2)


# =============================================================================== HEAVY
@sound('swing_whoosh_heavy', 'combat', 4, pitch=P_SOFT, lufs=-17, max_dur=0.8,
       notes='WEAPON FEEL heavy swing: a big body of air shoved past, low and slow, with a heave in front')
def swing_whoosh_heavy(r, v):
    d = [0.55, 0.5, 0.62, 0.58][v]
    out = buf(d + 0.05)
    c = d * U(r, 0.5, 0.62)
    # The heave: the air builds (you hear the weight coming), peaks late, dies fast.
    add(out, lp(_air(r, d, U(r, 180, 240), U(r, 700, 900), 1.2, [(0, 0), (c * 0.7, 0.45), (c, 1), (d, 0)], color='brown'), 1400, 2), 0, 1.0)
    add(out, _air(r, d, 420, 1300, 1.5, [(0, 0), (c, 0.8), (d * 0.9, 0)]), 0, 0.4)
    n = ns(d)
    t = tvec(n)
    body = sine(glide(n, U(r, 70, 85), U(r, 45, 55), d * 0.6), n) * np.exp(-((t - c) / (d * 0.22)) ** 2)
    add(out, body, 0, 0.55)  # the mass going by (felt more than heard)
    return hp(out, 35, 2)


@sound('heavy_crunch', 'combat', 4, pitch=P_IMPACT, lufs=-14, max_dur=0.9,
       notes='WEAPON FEEL heavy connect layer: a dull bone-deep crunch + a sub punch under the regular hit')
def heavy_crunch(r, v):
    out = buf(0.8)
    add(out, sat(click(r, 1600, 0.004), 2.2), 0, 0.8)
    add(out, material_hit(r, 'wood', 1.6 + 0.3 * v, 1.6), 0, 0.55)  # the crunch (gristle, not glass)
    add(out, grit(r, 22, 0.12, 900, 3200), 0.004, 0.35)
    add(out, thump(0.55, U(r, 68, 80), 32, 0.03, 0.2, drive=3.0, attack=0.002), 0, 1.2)
    return carve_heavy(glue(out, 2.0))


@sound('ground_crack', 'combat', 4, pitch=P_IMPACT, lufs=-13, max_dur=1.6,
       notes='WEAPON FEEL heavy whiff into terrain: stone splitting under the head, grit spray, a rolling thump')
def ground_crack(r, v):
    d = 1.4
    out = buf(d)
    add(out, sat(click(r, 1200, 0.006), 2.4), 0, 0.9)
    add(out, material_hit(r, 'stone', 2.4, 1.8), 0, 0.8)
    # The split running out: a few staggered cracks, each lower.
    for k in range(3 + v % 2):
        add(out, material_hit(r, 'stone', 1.0 + 0.4 * k, 1.2), 0.03 + 0.05 * k + U(r, 0, 0.02), 0.45 / (1 + k * 0.5))
    add(out, shatter(r, 'stone', [10, 14, 12, 16][v], 0.8, 0.8, 1.0), 0.02, 0.35)
    add(out, thump(1.1, U(r, 55, 66), 26, 0.08, 0.4, drive=3.0, attack=0.003), 0, 1.15)
    return carve_heavy(glue(out, 2.2))


# =============================================================================== REACH
@sound('swing_thrust', 'combat', 4, pitch=P_MICRO, lufs=-21, max_dur=0.35,
       notes='WEAPON FEEL reach thrust: a short rising air-dart (a spear jab) + a wooden shaft creak')
def swing_thrust(r, v):
    d = [0.17, 0.15, 0.19, 0.16][v]
    out = buf(d + 0.03)
    add(out, _air(r, d, U(r, 900, 1200), U(r, 3200, 4200), 2.2, [(0, 0), (d * 0.25, 1), (d, 0)]), 0, 1.0)
    # The shaft: a dry low creak (two damped wood modes).
    n = ns(0.08)
    creak = modal([U(r, 330, 420), U(r, 870, 990)], [0.03, 0.02], [1, .5], n, r, attack=0.001)
    add(out, creak, 0.0, 0.25)
    return hp(out, 250, 2)


# =============================================================================== FLEXIBLE
@sound('whip_crack', 'combat', 4, pitch=P_MICRO, lufs=-14, max_dur=0.5,
       notes='WEAPON FEEL whip tip sweet spot: a whistling lead-in, then the supersonic snap (sharp, dry)')
def whip_crack(r, v):
    out = buf(0.45)
    lead = 0.07 + 0.02 * (v % 2)
    add(out, _air(r, lead, 1800, 6500, 3.0, [(0, 0), (lead * 0.9, 1), (lead, 0.2)]), 0, 0.45)
    n = ns(0.02)
    x = np.zeros(n)
    x[1] = 1.0
    x += r.uniform(-1, 1, n) * np.exp(-np.arange(n) / ns(0.0015))
    snap = normpk(hpf(x, 1800, 2))
    add(out, sat(snap * 1.6, 3.0), lead, 1.0)
    add(out, micro(r, U(r, 3800, 5200), 0.012, 3, (1.4, 2.3)), lead + 0.002, 0.35)
    add(out, thump(0.08, 320, 160, 0.005, 0.02, drive=1.6), lead, 0.3)
    return hp(out, 300, 2)


# =============================================================================== SHIELD
@sound('shield_bash', 'combat', 3, pitch=P_IMPACT, lufs=-15, max_dur=0.8,
       notes='WEAPON FEEL shield bash: a flat wood + metal-rim slam and a shove thump')
def shield_bash(r, v):
    out = buf(1.6)
    add(out, sat(click(r, 2200, 0.003), 2.0), 0, 0.7)
    add(out, material_hit(r, 'wood', 1.8, 1.4), 0, 0.7)
    add(out, material_hit(r, 'metal', 0.9 + 0.2 * v, 0.8), 0.002, 0.3)
    add(out, thump(0.3, U(r, 110, 130), 60, 0.02, 0.08, drive=2.2), 0, 0.8)
    return carve_heavy(glue(out, 1.8))
