"""TRANSFORMERS V3 SAUCE (bd_game `present/transformer_feel/sauce.rs`): per-archetype WEAPON IDENTITY
for piloted robots + the move grammar's gather / release / finisher layers. Original synthesis only
(modal metal, FM, thumps, shaped noise) - never an imitation of any franchise's sounds.

* Weapons: energon blade hum (trucks / trains / boats), blaster zip (cars / bikes), cannon chatter
  (jets), chin gun (helicopters), shell thump (tanks / colossi), grinder screech (construction),
  spin whirr (bikes' spin-out), bite crunch (dinos), claw rake (beasts), missile whoosh, turret swivel.
* Grammar: the gather hum (a muzzle charging / a heavy winding up), the air tear on a heavy's release,
  the finisher's metallic crunch + BASS DROP.
Dry (no baked room)."""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, grit
from sounds import P_IMPACT, P_MICRO, P_SOFT, U, sound
from sounds_transform import hiss, panel_click, servo


def _zap(r, d, f0, f1, idx=3.0):
    """Descending FM zap (an energy bolt leaving a barrel)."""
    n = ns(d)
    f = sweep(n, f0, f1, 0.5)
    mod = np.sin(TAU * np.cumsum(f * 1.5) / SR) * idx * env_pts(n, [(0, 1), (d, 0)])
    car = np.sin(TAU * np.cumsum(f) / SR + mod)
    return car * env_pts(n, [(0, 0), (0.002, 1), (d * 0.4, 0.5), (d, 0)])


# ------------------------------------------------------------------------------------------ weapons
@sound('tfs_blade_hum', 'combat', 4, pitch=P_MICRO, lufs=-17, notes='energon blade swing: E-minor hum doppler + heavy whoosh + servo')
def tfs_blade_hum(r, v):
    d = 0.55
    n = ns(d)
    out = buf(d)
    dop = sweep(n, 1.0, 1.25, 0.5) * env_pts(n, [(0, 1), (0.18, 1.3), (d, 0.9)])
    hum = np.zeros(n)
    for k, a in ((1, 1.0), (2, 0.55), (3, 0.3), (6, 0.12)):
        f = 82.4 * 2 * k * dop  # E3 family
        hum += a * (np.sin(TAU * np.cumsum(f) / SR) + 0.5 * np.sin(TAU * np.cumsum(f * 1.004) / SR))
    add(out, sat(hum * env_pts(n, [(0, 0), (0.05, 0.6), (0.17, 1), (0.42, 0.25), (d, 0)]), 1.6), 0, 0.45)
    add(out, whoosh(0.4, r, 320, 1400, 1.6, [(0, 0), (0.15, 1), (0.4, 0)]), 0.02, 0.8)
    b = U(r, 300, 380)
    add(out, servo(r, 0.1, b, b * 1.7), 0, 0.3)
    return glue(out, 1.4)


@sound('tfs_blaster_zip', 'weapon', 5, pitch=P_MICRO, lufs=-15, notes='robot blaster: bright FM zap + snap + punch + capacitor whine tail')
def tfs_blaster_zip(r, v):
    out = buf(0.35)
    add(out, sat(click(r, 4200, 0.003), 2.0), 0, 0.8)
    f0 = U(r, 2400, 3000)
    add(out, _zap(r, 0.16, f0, f0 * 0.28, 3.5), 0.001, 0.7)
    add(out, thump(0.2, 140, 60, 0.02, 0.05, drive=1.8), 0, 0.8)
    add(out, _zap(r, 0.25, 5200, 3900, 0.5), 0.02, 0.08)
    return glue(out, 1.5)


@sound('tfs_cannon_chatter', 'weapon', 4, pitch=P_MICRO, lufs=-13, notes='jet cannon burst: 4 rapid snaps + mechanism clanks + punch')
def tfs_cannon_chatter(r, v):
    out = buf(0.45)
    gap = U(r, 0.045, 0.055)
    for i in range(4):
        t = i * gap + U(r, -0.003, 0.003)
        add(out, sat(click(r, U(r, 3000, 3800), 0.003), 2.4), t, 0.9)
        add(out, thump(0.08, 180, 90, 0.01, 0.03, drive=2.0), t, 0.7)
        f = U(r, 900, 1300)
        add(out, modal([f, f * 2.1], [0.02, 0.012], [1, .5], ns(0.05), r), t + 0.006, 0.25)
    return glue(out, 1.8)


@sound('tfs_chin_gun', 'weapon', 4, pitch=P_MICRO, lufs=-14, notes='helicopter chin gun: rotary whir under 3 thudding rounds')
def tfs_chin_gun(r, v):
    d = 0.4
    out = buf(d)
    n = ns(d)
    whir = saw(sweep(n, 180, 260), n)
    add(out, lp(whir, 900) * env_pts(n, [(0, 0), (0.03, 1), (0.3, 0.8), (d, 0)]), 0, 0.2)
    for i in range(3):
        t = 0.02 + i * U(r, 0.07, 0.08)
        add(out, sat(click(r, 2200, 0.004), 2.2), t, 0.8)
        add(out, thump(0.12, 130, 55, 0.015, 0.05, drive=2.2), t, 0.9)
    return glue(out, 1.7)


@sound('tfs_shell_thump', 'weapon', 4, pitch=P_SOFT, lufs=-11, notes='tank shell: deep chest boom + breech clank + casing ring + short blast air')
def tfs_shell_thump(r, v):
    d = 1.0
    out = buf(d)
    add(out, sat(click(r, 1400, 0.006), 2.6), 0, 1.0)
    add(out, thump(0.8, 70, 24, 0.06, 0.25, drive=3.0, attack=0.0015), 0, 1.4)
    n = ns(0.3)
    blast = normpk(tvf(brown(n, r), sweep(n, 900, 200), 1.2, 'lp')) * env_pts(n, [(0, 1), (0.3, 0)])
    add(out, blast, 0.002, 0.5)
    f = U(r, 300, 360)
    add(out, modal([f, f * 2.6, f * 4.1], [0.12, 0.06, 0.03], [1, .5, .3], ns(0.3), r), 0.09, 0.35)
    add(out, panel_click(r, 0.7, 0.6), 0.12)
    f = U(r, 2400, 2900)
    add(out, modal([f, f * 1.6, f * 2.7], [0.3, 0.2, 0.1], [1, .5, .3], ns(0.45), r), 0.32, 0.08)
    return carve_heavy(glue(out, 2.0), -4, -3)


@sound('tfs_grind_screech', 'combat', 3, pitch=P_MICRO, lufs=-16, notes='construction grinder: toothed metal grind + spark hiss')
def tfs_grind_screech(r, v):
    d = 0.6
    n = ns(d)
    out = buf(d)
    f = sweep(n, U(r, 160, 200), U(r, 240, 280))
    tooth = 1 + 0.6 * np.sign(np.sin(TAU * np.cumsum(f * 0.5) / SR))
    body = reson(saw(f, n) * tooth, 1800, 3.0) + 0.5 * lp(saw(f, n), 700)
    add(out, normpk(body) * env_pts(n, [(0, 0), (0.03, 1), (0.5, 0.8), (d, 0)]), 0, 0.7)
    sp = normpk(tvf(pink(n, r), sweep(n, 4000, 6500), 3.0, 'bp')) * (0.6 + 0.4 * smooth_noise(n, r, 40))
    add(out, sp * env_pts(n, [(0, 0), (0.05, 1), (d, 0)]), 0, 0.25)
    add(out, grit(r, 20, 0.5, 1500, 5000, 0.002), 0.02, 0.3)
    return glue(out, 1.6)


@sound('tfs_spin_whirr', 'combat', 3, pitch=P_MICRO, lufs=-16, notes='bike spin-out: rising rotor whirr + tyre chirp + whoosh')
def tfs_spin_whirr(r, v):
    d = 0.6
    n = ns(d)
    out = buf(d)
    f = sweep(n, 120, 520, 0.7)
    am = 0.6 + 0.4 * np.sin(TAU * np.cumsum(sweep(n, 8, 30)) / SR)
    add(out, lp(saw(f, n), 2200) * am * env_pts(n, [(0, 0), (0.05, 1), (0.5, 0.8), (d, 0)]), 0, 0.5)
    add(out, whoosh(0.5, r, 400, 2400, 1.5, [(0, 0), (0.3, 1), (0.5, 0)]), 0.05, 0.7)
    cn = ns(0.12)
    chirp = normpk(tvf(pink(cn, r), sweep(cn, 2200, 3200), 8.0, 'bp')) * env_pts(cn, [(0, 0), (0.01, 1), (0.12, 0)])
    add(out, chirp, 0.0, 0.35)
    return glue(out, 1.4)


@sound('tfs_bite_crunch', 'combat', 4, pitch=P_IMPACT, lufs=-13, notes='dino jaws: growl onset + double jaw clank + crunch cluster + sub thump')
def tfs_bite_crunch(r, v):
    d = 0.7
    out = buf(d)
    gn = ns(0.18)
    growl = lp(saw(sweep(gn, 90, 70) * (1 + 0.08 * smooth_noise(gn, r, 30)), gn), 600)
    add(out, growl * env_pts(gn, [(0, 0), (0.06, 1), (0.18, 0)]), 0, 0.4)
    for t, k in ((0.1, 1.0), (0.1 + U(r, 0.012, 0.018), 0.8)):
        f = U(r, 260, 340)
        add(out, modal([f, f * 1.7, f * 2.8], [0.1, 0.05, 0.03], [1, .5, .3], ns(0.3), r), t, 0.7 * k)
        add(out, sat(click(r, 1800, 0.004), 2.2), t, 0.9 * k)
    add(out, grit(r, 30, 0.06, 500, 2600, 0.004), 0.105, 0.6)
    add(out, thump(0.4, 100, 34, 0.04, 0.12, drive=2.6), 0.1, 1.2)
    return carve_heavy(glue(out, 2.0), -4, -3)


@sound('tfs_claw_rake', 'combat', 4, pitch=P_MICRO, lufs=-16, notes='beast claws: three quick metallic rakes over a short whoosh')
def tfs_claw_rake(r, v):
    out = buf(0.4)
    add(out, whoosh(0.25, r, 800, 2600, 1.8, [(0, 0), (0.08, 1), (0.25, 0)]), 0, 0.6)
    for i in range(3):
        t = 0.05 + i * 0.022
        f = U(r, 1400, 2000)
        add(out, modal([f, f * 1.5, f * 2.3], [0.04, 0.03, 0.02], [1, .6, .3], ns(0.12), r), t, 0.4)
        add(out, sat(click(r, 3500, 0.002), 1.8), t, 0.5)
    return glue(out, 1.5)


@sound('tfs_missile_whoosh', 'weapon', 3, pitch=P_MICRO, lufs=-15, notes='missile launch: ignition pop + rocket hiss whoosh')
def tfs_missile_whoosh(r, v):
    d = 0.8
    n = ns(d)
    out = buf(d)
    add(out, sat(click(r, 1600, 0.005), 2.2), 0, 0.8)
    add(out, thump(0.2, 110, 60, 0.02, 0.06, drive=2.0), 0, 0.8)
    roar = normpk(tvf(pink(n, r), sweep(n, 1800, 700), 1.4, 'bp')) * (0.8 + 0.2 * smooth_noise(n, r, 60))
    add(out, roar * env_pts(n, [(0, 0), (0.03, 1), (0.35, 0.6), (d, 0)]), 0.01, 0.7)
    return glue(out, 1.4)


@sound('tfs_turret_swivel', 'weapon', 3, pitch=P_MICRO, lufs=-22, notes='turret swivel: short heavy servo whine + gear tick')
def tfs_turret_swivel(r, v):
    out = buf(0.35)
    b = U(r, 180, 230)
    add(out, servo(r, 0.28, b, b * 1.35), 0, 0.8)
    for i in range(4):
        add(out, panel_click(r, 0.6, 0.25), 0.03 + i * 0.06)
    return glue(out, 1.3)


# ------------------------------------------------------------------------------------------ grammar
@sound('tfs_charge_hum', 'combat', 3, pitch=P_SOFT, lufs=-19, notes='gather: rising energon hum + servo tighten (a muzzle charging / a heavy winding up)')
def tfs_charge_hum(r, v):
    d = 0.5
    n = ns(d)
    out = buf(d)
    f = sweep(n, 164.8, 329.6, 0.8)  # E3 -> E4
    tone = np.sin(TAU * np.cumsum(f) / SR) + 0.4 * np.sin(TAU * np.cumsum(f * 3) / SR) + 0.2 * np.sin(TAU * np.cumsum(f * 2.004) / SR)
    add(out, tone * env_pts(n, [(0, 0), (0.4, 1), (d, 0)], 1.5), 0, 0.5)
    b = U(r, 400, 500)
    add(out, servo(r, 0.3, b, b * 1.8), 0.05, 0.25)
    return glue(out, 1.3)


@sound('tfs_air_tear', 'combat', 4, pitch=P_MICRO, lufs=-15, notes='heavy release: a ripping air tear (sharp band sweep) + pressure thump')
def tfs_air_tear(r, v):
    d = 0.4
    n = ns(d)
    out = buf(d)
    tear = normpk(tvf(pink(n, r), sweep(n, 5200, 900, 0.5), 2.5, 'bp')) * (0.7 + 0.3 * smooth_noise(n, r, 120))
    add(out, tear * env_pts(n, [(0, 0), (0.008, 1), (0.12, 0.5), (d, 0)]), 0, 0.8)
    add(out, thump(0.25, 110, 45, 0.02, 0.08, drive=2.0), 0, 0.7)
    return glue(out, 1.5)


@sound('tfs_finisher_drop', 'combat', 3, pitch=P_SOFT, lufs=-10, notes='finisher: metallic crunch + bending groan into a long saturated BASS DROP (80 -> 26 Hz)')
def tfs_finisher_drop(r, v):
    d = 1.8
    out = buf(d)
    add(out, sat(click(r, 1300, 0.006), 2.8), 0, 1.0)
    add(out, grit(r, 36, 0.05, 500, 3000, 0.005), 0.002, 0.7)
    f = U(r, 180, 230)
    add(out, modal([f, f * 2.3, f * 3.8, f * 5.1], [0.4, 0.2, 0.1, 0.05], [1, .6, .3, .2], ns(1.0), r), 0.003, 0.4)
    gn = ns(0.4)
    gf = sweep(gn, U(r, 380, 460), U(r, 120, 150), 0.6)
    groan = (saw(gf, gn) + 0.6 * saw(gf * 1.41, gn)) * env_pts(gn, [(0, 0), (0.02, 1), (0.4, 0)])
    add(out, reson(groan, 600, 2.0), 0.02, 0.3)
    # The drop: a deep sine glide with a saturated edge, long enough to be FELT.
    dn = ns(1.5)
    df = sweep(dn, 80, 26, 0.6)
    drop = np.sin(TAU * np.cumsum(df) / SR)
    drop = sat(drop * 1.8, 2.2) * env_pts(dn, [(0, 0), (0.01, 1), (0.5, 0.8), (1.5, 0)], 1.3)
    add(out, drop, 0.03, 1.1)
    return carve_heavy(glue(out, 2.2), -4, -3)
