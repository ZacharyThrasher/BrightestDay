"""Guns, hit confirms, bullet impacts, ring blast, KO shatter - DOOM feel, never pea shooters
(docs/COMBAT.md s4b).

Every fire sound is a DRY three-layer composite, carved by role:
  * transient snap   - highs (3-8 kHz), < 10 ms
  * mechanical body  - mids (500 Hz-3 kHz): action / hammer / bolt clanks (modal)
  * body punch       - 40-90 Hz saturated sine thump (reads on small speakers via its harmonics)
No room tail and no slapback: the mixer's reverb bus and early-reflection send place the shot.
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, grit, hl_chord, hl_ping, hl_shards, material_hit, micro, note, pent, pent_index, shatter
from sounds import (P_IMPACT, P_MICRO, P_SOFT, P_T10, P_TONAL, U, _cube_tinks, _pulse_train, sound)


def _clank(r, f, decay=0.035, amp=1.0):
    """Short steel mechanism clank (bolt/slide/hammer)."""
    ratios = [1.0, 1.73 * U(r, .97, 1.03), 2.61 * U(r, .97, 1.03), 3.9 * U(r, .97, 1.03)]
    n = ns(decay * 7)
    x = modal([f * k for k in ratios], [decay, decay * .8, decay * .6, decay * .4], [1, .7, .5, .3], n, r)
    return amp * (x + 0.5 * fit(click(r, 3000, 0.001), n))


def _snap(r, amp=1.0, fc=3000.0, d=0.006):
    """Transient snap: impulse + <= 6 ms of 3-8 kHz noise (the supersonic crack / muzzle edge)."""
    n = ns(d) + ns(0.003)
    x = np.zeros(n)
    x[1] = 1.0
    x[:ns(d)] += r.uniform(-1, 1, ns(d)) * np.exp(-np.arange(ns(d)) / ns(d / 4))
    return amp * normpk(lpf(hpf(x, fc, 2), 9000, 2))


def _report(r, lo=250.0, hi=2200.0, tau=0.006, amp=1.0):
    """Muzzle report: a short (<= 15 ms audible) pink burst in the mids, saturated."""
    n = ns(0.03)
    b = normpk(bp(pink(n, r), lo, hi)) * env_exp(n, tau, 0.0003)
    return amp * sat(b * 1.5, 2.0)


def _gunshot(r, sub=(110, 45, 0.03, 0.15), sub_amp=1.2, sub_drive=3.0, snap_amp=0.9, report=(250, 2200, 0.006),
             report_amp=0.7, punch_amp=0.35, mech=(), d=0.9):
    out = buf(d)
    add(out, _snap(r, snap_amp), 0, 1.0)
    add(out, _report(r, *report), 0, report_amp)
    f0, f1, tp, ta = sub
    add(out, thump(ta * 8, f0, f1, tp, ta, drive=sub_drive, attack=0.001), 0, sub_amp)
    add(out, thump(0.12, 260, 120, 0.012, 0.025, drive=1.5), 0, punch_amp)   # mid punch for small speakers
    for (t, f, a, dec) in mech:
        add(out, _clank(r, f, dec), t, a)
    return glue(out, 1.4)


# =============================================================================== guns
@sound('gun_handcannon', 'weapon', 4, pitch=P_MICRO, lufs=-13,
       notes='hand cannon: snap + hammer/slide clanks + 46 Hz punch; dry')
def gun_handcannon(r, v):
    return _gunshot(r, sub=(115 * U(r, .92, 1.08), 46, 0.028, 0.12), sub_amp=1.25,
                    report=(220, 2400, 0.006),
                    mech=[(U(r, .0, .004), U(r, 1500, 1900), 0.35, 0.02),
                          (U(r, .045, .065), U(r, 2200, 2800), 0.22, 0.025),
                          (U(r, .08, .1), U(r, 1300, 1700), 0.12, 0.03)])


@sound('gun_shotgun', 'weapon', 4, pitch=P_MICRO, lufs=-11, notes='super shotgun: two barrels a hair apart, huge punch; dry')
def gun_shotgun(r, v):
    out = _gunshot(r, sub=(100 * U(r, .92, 1.08), 44, 0.05, 0.25), sub_amp=1.45, sub_drive=3.5, snap_amp=1.0,
                   report=(150, 2000, 0.009), report_amp=0.9, punch_amp=0.45,
                   mech=[(U(r, .01, .02), U(r, 900, 1200), 0.25, 0.04)], d=1.4)
    t2 = U(r, 0.008, 0.016)
    add(out, _snap(r, 0.8), t2, 1.0)
    add(out, _report(r, 150, 1800, 0.008), t2, 0.6)
    add(out, thump(1.4, 85, 42, 0.05, 0.2, drive=3.0), t2, 0.7)
    n = ns(0.8)
    boom = normpk(lp(brown(n, r), 180)) * env_exp(n, 0.12, 0.004)  # physical low muzzle boom, brown & short
    add(out, boom, 0, 0.35)
    return glue(out, 1.8)


@sound('gun_shotgun_reload', 'weapon', 3, pitch=P_MICRO, lufs=-24,
       notes='break open snap ... two shells ... heavy close clack (fits the 0.9 s reload)')
def gun_shotgun_reload(r, v):
    out = buf(0.8)
    add(out, click(r, 2500, 0.0015), 0, 0.6)
    add(out, _clank(r, U(r, 1500, 1800), 0.03), 0, 0.8)
    for t in (U(r, .16, .2), U(r, .26, .3)):
        add(out, _clank(r, U(r, 2800, 3400), 0.012), t, 0.3)
        add(out, material_hit(r, 'wood', 0.35, 0.7), t + 0.004, 0.25)  # shell seated in the chamber
    t = U(r, 0.44, 0.5)
    add(out, click(r, 2000, 0.002), t, 0.8)
    add(out, _clank(r, U(r, 1100, 1400), 0.045), t, 1.0)
    add(out, _clank(r, U(r, 600, 800), 0.03), t + 0.003, 0.6)
    return carve_foley(out, 200, 11000)


_MG_F = 180.0     # motor fundamental at full spin (Hz); loop uses integer cycles
_MG_RATE = 14.0   # rounds per second (COMBAT.md)


def _mg_round(r, a=1.0):
    n = ns(0.2)
    x = np.zeros(n)
    add(x, _snap(r, 0.7, 3200, 0.003), 0, 1.0)
    add(x, _report(r, 250, 2600, 0.004), 0, 0.3)
    add(x, thump(0.15, 115 * U(r, .95, 1.05), 55, 0.015, 0.04, drive=2.8), 0, 1.0)
    add(x, _clank(r, U(r, 2300, 2900), 0.012), 0.004, 0.45)
    return a * x


def _gear_whirr(n, rot_rate, f_res=(1300, 2600)):
    """Barrel/gear whirr: a tooth pulse train through two resonators (tonal-mechanical, not hiss)."""
    tr = _pulse_train(n, rot_rate)
    return reson(tr, f_res[1], 6) + 0.6 * reson(tr, f_res[0], 4)


@sound('gun_minigun_spinup', 'weapon', 3, pitch=P_SOFT, lufs=-20, max_dur=0.5,
       notes='0.4 s spin-up; ends at the loop motor speed')
def gun_minigun_spinup(r, v):
    d = 0.42
    n = ns(d)
    u = np.linspace(0, 1, n)
    f = 35 + (_MG_F - 35) * u ** 0.8
    motor = lp(saw(f, n) + 0.6 * saw(f * 1.006, n) + 0.4 * saw(f * 2, n), 1500)
    teeth = normpk(_gear_whirr(n, 20 + 100 * u ** 0.8))
    clicks = normpk(_gear_whirr(n, 4 + 24 * u, (1300 * U(r, .95, 1.05), 2600 * U(r, .95, 1.05))))
    whine = sine(f * 8, n)
    env = env_pts(n, [(0, 0), (0.03, 0.4), (d, 1.0)])
    x = (normpk(motor) * 0.7 + 0.3 * teeth + 0.3 * clicks + 0.08 * whine) * env
    x[-ns(0.01):] *= np.linspace(1, 0, ns(0.01))
    return hpf(x, 100, 2)


@sound('gun_minigun_loop', 'weapon', 2, loop=True, pitch=P_TONAL, lufs=-16,
       notes='loop: 14 rounds/s, exactly 14 rounds per 1 s loop; dry')
def gun_minigun_loop(r, v):
    L = ns(1.0)
    shots = np.zeros(L)
    k = int(_MG_RATE)
    for i in range(k):
        accent = 1.0 if i % 2 == 0 else 0.85  # heavy rhythmic lope
        add_circ(shots, _mg_round(r, accent * U(r, 0.9, 1.05)), int((i + 0.5) * L / k))

    def motor(n):
        t = tvec(n)
        m = lp(saw(_MG_F, n) + 0.6 * saw(_MG_F + 1.0, n) + 0.4 * saw(2 * _MG_F, n), 1500)
        return 0.5 * normpk(m) * (1 + 0.2 * np.sin(TAU * _MG_RATE * t)) + 0.05 * sine(8 * _MG_F, n)
    mot = periodic(motor, L)
    teeth = periodic(lambda n: _gear_whirr(n, 126.0), L)  # 9 teeth per round
    x = normpk(shots) + 0.25 * mot + 0.1 * normpk(teeth)
    return glue(x, 1.3)


@sound('gun_minigun_spindown', 'weapon', 3, pitch=P_SOFT, lufs=-20, notes='0.3 s spin-down + settle clunk')
def gun_minigun_spindown(r, v):
    out = buf(0.7)
    d = 0.32
    n = ns(d)
    u = np.linspace(0, 1, n)
    f = _MG_F * (1 - u) ** 1.3 + 25
    motor = lp(saw(f, n) + 0.6 * saw(f * 1.006, n) + 0.4 * saw(f * 2, n), 1500)
    teeth = normpk(_gear_whirr(n, 126 * (1 - u) ** 1.3 + 3))
    clicks = normpk(_gear_whirr(n, 28 * (1 - u) + 3))
    env = env_pts(n, [(0, 1), (d * 0.7, 0.5), (d, 0)])
    add(out, (normpk(motor) * 0.7 + 0.3 * teeth + 0.3 * clicks + 0.08 * sine(f * 8, n)) * env, 0, 1.0)
    add(out, _clank(r, U(r, 500, 700), 0.05), d - 0.02, 0.6)
    add(out, thump(0.2, 140, 70, 0.015, 0.04, drive=1.5), d - 0.02, 0.4)
    return hpf(out, 100, 2)


@sound('gun_rocket_fire', 'weapon', 3, pitch=P_MICRO, lufs=-12,
       notes='launch: snap + 40 Hz punch + ignition roar (shaped, short); rocket then uses rocket_flyby_loop')
def gun_rocket_fire(r, v):
    d = 1.2
    n = ns(d)
    out = _gunshot(r, sub=(86 * U(r, .92, 1.08), 40, 0.06, 0.22), sub_amp=1.35, report=(120, 1600, 0.01),
                   mech=[(0.004, U(r, 900, 1200), 0.3, 0.04)], d=d)
    g = brown(n, r) + 0.6 * pink(n, r)
    roar = tvf(g, sweep(n, 3000, 450, 0.6), 1.4, 'lp') * (1 + 0.35 * smooth_noise(n, r, 35))
    add(out, normpk(roar) * env_pts(n, [(0, 0), (0.015, 1), (0.25, 0.5), (0.9, 0)]), 0, 0.55)
    thr = reson(g, 700 * U(r, .9, 1.1), 3) + 0.6 * reson(g, 1600 * U(r, .9, 1.1), 4)  # motor throat
    add(out, normpk(thr) * env_pts(n, [(0, 0), (0.02, 1), (0.6, 0)]), 0.01, 0.3)
    return out


@sound('rocket_flyby_loop', 'weapon', 2, loop=True, pitch=P_SOFT, lufs=-20,
       notes='loop: rocket motor (shaped roar + combustion buzz + pops); attach to the projectile')
def rocket_flyby_loop(r, v):
    Ld = 2.0
    L, X = ns(Ld), ns(0.3)
    n = L + X
    g = brown(n, r) + 0.7 * pink(n, r)
    roar = normpk(lp(g, 600)) + 0.6 * normpk(reson(g, 900 + 150 * v, 2.5)) + 0.35 * normpk(reson(g, 2300, 4))
    roar *= 1 + 0.3 * smooth_noise(n, r, 18)
    pops = np.zeros(n)
    for t in r.uniform(0, n / SR, int(40 * n / SR)):
        add(pops, lp(micro(r, U(r, 300, 900), 0.006, 2, (1.5, 2.5)), 3000), t, U(r, 0.2, 1.0))
    noise = loop_xfade(roar + 0.3 * normpk(pops), L, X)
    fb = [qf(95, Ld), qf(110, Ld)][v]
    buzz = periodic(lambda m: lp(saw(fb, m) + 0.5 * saw(fb * 1.5, m), 900), L)  # combustion buzz
    return circ(lambda y: hp(y, 40, 2), noise + 0.25 * normpk(buzz))


@sound('gun_railgun_charge', 'weapon', 3, pitch=P_TONAL, lufs=-20, max_dur=0.52,
       notes='0.5 s rising whine (tonal), ends on E; gun_railgun_fire follows')
def gun_railgun_charge(r, v):
    d = 0.5
    n = ns(d)
    u = np.linspace(0, 1, n)
    f = note(['E3', 'B2', 'E3'][v]) * 8 ** (u ** 1.5)  # rises 3 octaves, lands on E6/B5
    whine = sine(f, n) + 0.4 * sine(2 * f, n) + 0.25 * lp(saw(f / 2, n), 3000)
    spark = fm(f * 2, 1.5, 2.5 * u, n)
    hum = sine(note('E2'), n) + 0.5 * sine(note('E3'), n)
    env = u ** 1.5
    x = (0.6 * whine + 0.2 * spark + 0.3 * hum * u) * env
    for _ in range(int(30 * (v + 1) / 2)):  # arcing: sparse tiny ticks, denser towards the end
        tt = d * r.uniform() ** 0.5
        add(x, micro(r, U(r, 3500, 8000), 0.002), tt, 0.3 * (tt / d) ** 2)
    x[:ns(0.01)] *= np.linspace(0, 1, ns(0.01))
    x[-ns(0.008):] *= np.linspace(1, 0, ns(0.008))
    return x


@sound('gun_railgun_fire', 'weapon', 3, pitch=P_SOFT, lufs=-11,
       notes='snap + deep boom + FM beam zap + ringing rail (physical ring, no room)')
def gun_railgun_fire(r, v):
    d = 2.0
    n = ns(d)
    out = buf(d)
    add(out, _snap(r, 1.0, 2500, 0.006), 0, 1.0)
    add(out, _snap(r, 0.6, 4000, 0.003), 0.003, 1.0)
    nz = ns(0.3)
    add(out, fm(glide(nz, 4500, 250, 0.03), 0.5, 10 * np.exp(-tvec(nz) / 0.05), nz) * env_exp(nz, 0.07), 0, 0.5)
    add(out, thump(2.0, 78 * U(r, .93, 1.07), 40, 0.08, 0.45, drive=3.5, attack=0.001), 0, 1.3)
    add(out, _report(r, 150, 1500, 0.01), 0, 0.6)
    ring = material_hit(r, 'metal', 0.9, 1.4)  # the rails ringing
    add(out, ring, 0, 0.3)
    add(out, (sine(note('E6'), n) + sine(note('E6') * 1.0027, n)) * env_exp(n, 0.7, 0.005), 0, 0.05)
    return glue(out, 1.4)


@sound('shell_casing', 'weapon', 6, pitch=P_T10, lufs=-32,
       notes='ejected hard-light casing: glassy tinkle in key (some end in a tiny shatter)')
def shell_casing(r, v):
    out = buf(0.6)
    f = pent(pent_index('E6') + [0, 2, 4, 1, 3, 5][v])
    hits = [(t, a) for (t, a) in bounces(r, 0, 1.0, U(r, .06, .12), U(r, .5, .65), 0.55, 0.5)][:int(U(r, 2, 5))]
    for (t, a) in hits:
        add(out, hl_ping(r, f, U(r, 0.04, 0.08), 1.2, glint=0.8, shimmer=0.2), t, a)
        add(out, micro(r, U(r, 1800, 2600), 0.01, 2, (1.4, 2.2)), t, a * 0.4)  # casing body
    if v >= 4:  # last bounce shatters
        add(out, hl_shards(r, 2, 0.05, 'E8', 'B7', amp=0.4, dec=(0.015, 0.04)), hits[-1][0] + 0.02, 1.0)
    return carve_foley(out, 250, 12000)


# =============================================================================== hit confirms (feedback: bright)
@sound('hit_confirm', 'feedback', 6, pitch=P_T10, lufs=-21,
       notes='crisp dry hit-confirm tick for the shooter: FM glint 5-9 kHz + small 2-3 kHz body')
def hit_confirm(r, v):
    out = buf(0.12)
    f = [5274.0, 5920.0, 4699.0, 6272.0, 5274.0, 4978.0][v]  # E8 F#8 D8 G8 E8 D#8~ (glint pitch)
    ratio = [1.41, 2.0, 1.5, 1.41, 3.0, 2.0][v]
    n = ns(0.07)
    add(out, fm(f, ratio, 2.2 * env_exp(n, 0.004), n) * env_exp(n, 0.014, 0.0002), 0, 1.0)
    g = U(r, 2300, 3000)
    add(out, modal([g, g * 1.58, g * 2.46], [0.012, 0.008, 0.005], [1, .5, .3], ns(0.06), r), 0, 0.45)
    add(out, click(r, 5000, 0.0008), 0, 0.35)
    return carve_ui(out, 2000)


@sound('hit_confirm_heavy', 'feedback', 3, pitch=P_T10, lufs=-19,
       notes='"ka-CHUNK": high tick then a heavier (still bright) knock for big hits / kills')
def hit_confirm_heavy(r, v):
    out = buf(0.3)
    n = ns(0.06)
    add(out, fm(note('E8'), 2.0, 2 * env_exp(n, 0.004), n) * env_exp(n, 0.012), 0, 0.8)
    add(out, click(r, 5000, 0.0008), 0, 0.3)
    t = U(r, 0.04, 0.05)
    add(out, sat(click(r, 2500, 0.003), 2.0), t, 0.6)
    g = [1500, 1650, 1400][v]
    add(out, modal([g, g * 1.58, g * 2.46, g * 3.6], [0.04, 0.025, 0.015, 0.01], [1, .7, .5, .35], ns(0.2), r), t, 1.0)
    m = ns(0.1)
    add(out, fm(note('B7'), 1.41, 3 * env_exp(m, 0.006), m) * env_exp(m, 0.03), t, 0.5)
    return carve_ui(out, 1500)


@sound('hit_crit', 'feedback', 5, pitch=P_T10, lufs=-18,
       notes='critical/headshot: crisp glass-metal HF snap over a small dull low thud')
def hit_crit(r, v):
    out = buf(0.3)
    f = U(r, 3300, 4300)
    ratios = [[1, 1.52, 2.1], [1, 1.41, 1.93], [1, 1.62, 2.3], [1, 1.5, 2.0], [1, 1.37, 2.2]][v]
    snap = modal([f * k for k in ratios], [0.026, 0.016, 0.01], [1, .6, .35], ns(0.12), r)
    add(out, snap, 0, 1.3)
    add(out, _snap(r, 1.0, 3500, 0.004), 0, 1.0)
    n = ns(0.05)
    add(out, fm(f * 1.5, 2.0, 2 * env_exp(n, 0.003), n) * env_exp(n, 0.01), 0, 0.4)
    add(out, thump(0.2, 110 * U(r, .95, 1.05), 65, 0.02, 0.04, drive=1.2), 0, 0.28)
    return lpf(glue(out, 1.5), 11000, 2)


# =============================================================================== bullet impacts (material family)
def _ricochet(r, amp=0.25):
    nn = ns(0.35)
    tt = tvec(nn)
    fr = glide(nn, U(r, 3000, 3600), U(r, 1200, 1500), 0.12) * (1 + 0.01 * np.sin(TAU * 30 * tt))
    return amp * sine(fr, nn) * env_pts(nn, [(0, 0), (0.008, 1), (0.35, 0)], 2.0)


@sound('bullet_impact_stone', 'destruction', 6, pitch=P_IMPACT, lufs=-20,
       notes='slug into stone: chip + grit (variations 2, 4, 6 ricochet)')
def bullet_impact_stone(r, v):
    out = buf(0.6)
    add(out, material_hit(r, 'stone', U(r, 0.5, 0.8), 1.6), 0, 1.0)
    add(out, shatter(r, 'stone', int(U(r, 3, 6)), 0.12, 0.35, 0.8), 0.003, 0.4)
    add(out, thump(0.15, 170, 90, 0.012, 0.025, drive=1.5), 0, 0.3)
    out = glue(out, 1.6)
    if v % 2 == 1:
        add(out, _ricochet(r), 0.01, 1.0)
    return out


@sound('bullet_impact_dirt', 'destruction', 6, pitch=P_IMPACT, lufs=-20, notes='slug into dirt: soft thud + clods')
def bullet_impact_dirt(r, v):
    out = buf(0.5)
    add(out, material_hit(r, 'dirt', U(r, 0.6, 0.9), 1.4), 0, 1.0)
    add(out, click(r, 1500, 0.003), 0, 0.3)
    add(out, grit(r, int(U(r, 6, 14)), 0.03, 300, 1400, 0.006), 0.005, 0.35)
    return lpf(glue(out, 1.6), 4000, 2)


@sound('bullet_impact_wood', 'destruction', 6, pitch=P_IMPACT, lufs=-20, notes='slug into wood: knock + splinter ticks')
def bullet_impact_wood(r, v):
    out = buf(0.5)
    add(out, material_hit(r, 'wood', U(r, 0.6, 0.9), 1.5), 0, 1.0)
    add(out, grit(r, int(U(r, 8, 18)), 0.02, 2000, 6000, 0.0015), 0.004, 0.3)
    return glue(out, 1.6)


@sound('bullet_impact_metal', 'destruction', 6, pitch=P_IMPACT, lufs=-20,
       notes='slug into metal: bright ping + short ring (variations 2, 4, 6 ricochet)')
def bullet_impact_metal(r, v):
    out = buf(1.2)
    m = material_hit(r, 'metal', U(r, 0.35, 0.6), 1.8)
    add(out, m * env_exp(len(m), U(r, 0.25, 0.5), 0), 0, 1.0)
    add(out, click(r, 4000, 0.0015), 0, 0.5)
    if v % 2 == 1:
        add(out, _ricochet(r, 0.3), 0.008, 1.0)
    return out


@sound('bullet_impact_glass', 'destruction', 6, pitch=P_IMPACT, lufs=-21, notes='slug through glass: crack + a few shards')
def bullet_impact_glass(r, v):
    out = buf(0.9)
    add(out, shatter(r, 'glass', int(U(r, 5, 10)), 0.2, U(r, 0.5, 0.8), 1.4), 0, 1.0)
    return out


# =============================================================================== ring blast (hard light)
@sound('ring_blast', 'construct', 5, pitch=P_T10, lufs=-18,
       notes='empty-hand ring blast: E-tuned fist-of-light whump + FM zap landing on E')
def ring_blast(r, v):
    d = 0.6
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, click(r, 3000, 0.003), 0, 0.6)
    add(out, thump(0.5, note('E3') * [1.0, 1.0, 1.19, 1.0, 1.12][v], note('E1'), 0.03, 0.11, drive=3.0), 0, 1.2)
    top = note(['E6', 'B6', 'E6', 'G6', 'D6'][v])
    zap = fm(glide(n, top, note('E4'), 0.04), 1.0, 5 * np.exp(-t / 0.05) + 0.4, n) * env_exp(n, 0.07)
    add(out, zap, 0, 0.35)
    add(out, hl_ping(r, note(['B4', 'E5', 'G4', 'B4', 'E5'][v]), 0.1, 1.1), 0, 0.3)
    wn = ns(0.08)
    add(out, sat(normpk(bp(pink(wn, r), 90, 600)) * env_exp(wn, 0.012), 2.0), 0, 0.5)  # air push, 12 ms
    return out


@sound('ring_blast_hit', 'construct', 5, pitch=P_T10, lufs=-17,
       notes='ring blast impact: crack + E-tuned sub whump + E-minor chord splash + a few shards')
def ring_blast_hit(r, v):
    d = 0.9
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, sat(click(r, 2000, 0.004), 2.5), 0, 0.9)
    add(out, thump(0.8, note('E3'), note('E1') * [1.0, 1.0, 1.19, 1.0, 1.12][v], 0.035, 0.18, drive=3.2), 0, 1.3)
    ch = [['E5', 'B5', 'E6'], ['G5', 'B5', 'E6'], ['E5', 'G5', 'B5'], ['B4', 'E5', 'B5'], ['E5', 'A5', 'E6']][v]
    add(out, hl_chord(r, ch, dec=0.18, bright=1.2, stagger=0.002), 0, 0.5)
    add(out, fm(glide(n, note('E6'), note('E4'), 0.03), 1.0, 7 * np.exp(-t / 0.025), n) * env_exp(n, 0.045), 0, 0.3)
    add(out, hl_shards(r, 3, 0.25, 'E8', 'E6', amp=0.3), 0.01, 1.0)
    return out


# =============================================================================== KO shatter (hard light)
@sound('ko_shatter', 'combat', 3, pitch=P_TONAL, lufs=-13,
       notes='player shatters into hard-light voxels: bigger than construct_shatter + E sub drop + soul leaving')
def ko_shatter(r, v):
    d = 2.4
    out = buf(d)
    add(out, sat(click(r, 2500, 0.006), 3.0), 0, 0.9)
    add(out, sat(click(r, 4000, 0.003), 2.0), U(r, 0.01, 0.02), 0.5)
    add(out, thump(1.6, note('E3'), note('E1') * 0.75, 0.08, 0.35, drive=3.2, attack=0.001), 0, 1.4)
    ch = [['E4', 'B4', 'E5', 'G5', 'B5'], ['G4', 'B4', 'E5', 'B5', 'E6'], ['E4', 'G4', 'B4', 'E5', 'B5']][v]
    add(out, hl_chord(r, ch, dec=0.3, bright=1.3, stagger=0.003), 0, 0.6)
    add(out, hl_shards(r, 18, 1.5, 'E8', 'E6', amp=0.9), 0.01, 1.0)
    add(out, _cube_tinks(r, 14, 0.08, 1.9, 'E6', 'B7', amp=0.5), 0, 1.0)
    add(out, _snap(r, 1.0, 4000, 0.004), 0, 0.6)
    m = ns(1.2)
    fall = sum(sine(glide(m, note(nm), note(nm) / 2, 0.45), m) for nm in ('E7', 'G7', 'B7'))  # soul leaving
    add(out, fall * env_pts(m, [(0, 0), (0.15, 1), (1.2, 0)]), 0.1, 0.05)
    return out
