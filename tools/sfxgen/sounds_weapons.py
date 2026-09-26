"""Guns and the ring blast — DOOM feel, never pea shooters (docs/COMBAT.md §4b).

Every fire sound = sharp transient + heavy sub thump (40-90 Hz) + mechanical body + short
distance/room tail (slapback + reverb).
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from sounds import U, _pulse_train, _shards, sound


def _slap(x, r, delay=0.11, gain=0.3, fc=1500.0):
    """Distance cue: one dark echo off far terrain."""
    out = fit(x, len(x) + ns(delay))
    return add(out, lp(x, fc, 2), delay, gain)


def _clank(r, f, decay=0.035, amp=1.0):
    """Short steel mechanism clank (bolt/slide/hammer)."""
    ratios = [1.0, 1.73 * U(r, .97, 1.03), 2.61 * U(r, .97, 1.03), 3.9 * U(r, .97, 1.03)]
    n = ns(decay * 7)
    x = modal([f * k for k in ratios], [decay, decay * .8, decay * .6, decay * .4], [1, .7, .5, .3], n, r)
    return amp * (x + 0.8 * crack(r, 3000, 0.0012, d=n / SR)[:n])


def _gunshot(r, d=1.2, crack_fc=2000, crack_tau=0.003, blast=(200, 3000, 0.035), blast_amp=0.9,
             sub=(110, 45, 0.03, 0.15), sub_amp=1.2, sub_drive=3.0, punch_amp=0.35,
             mech=(), rt=0.9, wet=0.22, slap=(0.11, 0.3)):
    out = buf(d)
    add(out, sat(crack(r, crack_fc, crack_tau, d=0.05), 3.0), 0, 1.0)
    lo, hi, tau = blast
    nb = ns(max(0.2, tau * 8))
    bl = normpk(bp(white(nb, r) + 2 * pink(nb, r), lo, hi)) * env_exp(nb, tau, 0.0005)
    add(out, sat(bl * 1.5, 2.0), 0, blast_amp)
    f0, f1, tp, ta = sub
    add(out, thump(ta * 8, f0, f1, tp, ta, drive=sub_drive, attack=0.001), 0, sub_amp)
    add(out, thump(0.15, 260, 120, 0.012, 0.03, drive=1.5), 0, punch_amp)   # mid punch for small speakers
    for (t, f, a) in mech:
        add(out, _clank(r, f), t, a)
    if slap:
        out = _slap(out, r, *slap)
    out = reverb(out, r, rt, wet, bright=0.7)
    return sat(out * 1.1, 1.4)


# =============================================================================== guns
@sound('gun_handcannon', 'weapon', 4, volume=0.9, pitch=(0.94, 1.05),
       notes='hand cannon / pistol / revolver / rifle; 0.35 s cadence')
def gun_handcannon(r, v):
    return _gunshot(r, d=1.0, crack_fc=U(r, 1800, 2400), blast=(180, 3200, 0.03),
                    sub=(115 * U(r, .92, 1.08), 46, 0.028, 0.13), sub_amp=1.25,
                    mech=[(U(r, .012, .02), U(r, 1900, 2500), 0.28), (U(r, .05, .07), U(r, 2600, 3200), 0.15)],
                    rt=0.8, wet=0.2, slap=(U(r, .09, .14), 0.3))


@sound('gun_shotgun', 'weapon', 4, volume=1.0, pitch=(0.95, 1.04), lufs=-11,
       notes='super shotgun: huge double-barrel boom')
def gun_shotgun(r, v):
    d = 2.0
    out = _gunshot(r, d=d, crack_fc=1500, crack_tau=0.005, blast=(80, 4000, 0.075), blast_amp=1.1,
                   sub=(100 * U(r, .92, 1.08), 44, 0.06, 0.3), sub_amp=1.45, sub_drive=3.5, punch_amp=0.45,
                   mech=[], rt=1.4, wet=0.28, slap=(U(r, .12, .18), 0.35))
    # second barrel a hair later + rumble tail
    t2 = U(r, 0.008, 0.016)
    add(out, sat(crack(r, 1300, 0.005, d=0.05), 3.0), t2, 0.8)
    add(out, thump(1.6, 85, 42, 0.05, 0.25, drive=3.0), t2, 0.7)
    n = ns(1.8)
    rum = normpk(lp(brown(n, r), 200)) * env_exp(n, 0.4, 0.02) * (1 + 0.4 * smooth_noise(n, r, 8))
    add(out, rum, 0, 0.45)
    return out


@sound('gun_shotgun_reload', 'weapon', 3, volume=0.75, pitch=(0.96, 1.04), lufs=-16,
       notes='two-part snap (break open) ... clack (close), fits the 0.9 s reload')
def gun_shotgun_reload(r, v):
    out = buf(0.8)
    # part 1: break-open snap
    add(out, crack(r, 2500, 0.0015), 0, 0.8)
    add(out, _clank(r, U(r, 1500, 1800), 0.03), 0, 0.7)
    add(out, thump(0.1, 220, 130, 0.01, 0.02, drive=1.2), 0, 0.35)
    # shells: two quick brass-ish inserts
    for t in (U(r, .16, .2), U(r, .26, .3)):
        add(out, _clank(r, U(r, 2800, 3400), 0.012), t, 0.25)
        add(out, burst(0.03, r, 600, 2500, 0.006), t, 0.15)
    # part 2: heavy close clack
    t = U(r, 0.44, 0.5)
    add(out, crack(r, 2000, 0.002), t, 0.9)
    add(out, _clank(r, U(r, 1100, 1400), 0.045), t, 1.0)
    add(out, thump(0.3, 160, 70, 0.015, 0.05, drive=2.0), t, 0.8)
    return reverb(out, r, 0.4, 0.1)


_MG_F = 180.0     # motor fundamental at full spin (Hz); loop uses integer cycles
_MG_RATE = 14.0   # rounds per second (COMBAT.md)


def _mg_round(r, a=1.0):
    n = ns(0.2)
    x = np.zeros(n)
    add(x, sat(crack(r, 2200, 0.0018, d=0.03), 2.5), 0, 0.8)
    b = normpk(bp(white(n, r) + pink(n, r), 250, 3000)) * env_exp(n, 0.013)
    add(x, sat(b * 1.4, 2.0), 0, 0.7)
    add(x, thump(0.15, 115 * U(r, .95, 1.05), 55, 0.015, 0.042, drive=2.8), 0, 1.0)
    add(x, _clank(r, U(r, 2300, 2900), 0.01), 0.004, 0.18)
    return a * x


@sound('gun_minigun_spinup', 'weapon', 3, volume=0.8, pitch=(0.98, 1.02), lufs=-16, max_dur=0.5,
       notes='0.4 s spin-up; ends at the loop motor speed')
def gun_minigun_spinup(r, v):
    d = 0.42
    n = ns(d)
    u = np.linspace(0, 1, n)
    f = 35 + (_MG_F - 35) * u ** 0.8
    motor = lp(saw(f, n) + 0.6 * saw(f * 1.006, n) + 0.4 * saw(f * 2, n), 1500)
    rot = np.cumsum(2 + 12 * u ** 0.8) / SR
    whirr = normpk(bp(white(n, r), 700, 3500)) * (0.55 + 0.45 * np.sin(TAU * rot))
    clicks = normpk(reson(_pulse_train(n, 4 + 24 * u), 2600, 6) + 0.6 * reson(_pulse_train(n, 4 + 24 * u), 1300, 4))
    whine = sine(f * 8, n)
    env = env_pts(n, [(0, 0), (0.03, 0.4), (d, 1.0)])
    x = (normpk(motor) * 0.7 + 0.35 * whirr + 0.3 * clicks + 0.08 * whine) * env
    x[-ns(0.01):] *= np.linspace(1, 0, ns(0.01))
    return x


@sound('gun_minigun_loop', 'weapon', 2, loop=True, volume=0.9, pitch=(1.0, 1.0), lufs=-14,
       notes='loop: 14 rounds/s, exactly 14 rounds per 1 s loop')
def gun_minigun_loop(r, v):
    L = ns(1.0)
    X = ns(0.2)
    shots = np.zeros(L)
    k = int(_MG_RATE)
    for i in range(k):
        accent = 1.0 if i % 2 == 0 else 0.85  # heavy rhythmic lope
        add_circ(shots, _mg_round(r, accent * U(r, 0.9, 1.05)), int((i + 0.5) * L / k))
    room = reverb_circ(shots, r, 0.7, 0.22, bright=0.7)
    room = room + 0.25 * np.roll(circ(lambda y: lp(y, 1500), shots), ns(0.1))  # slapback, circular

    def motor(n):
        t = tvec(n)
        m = lp(saw(_MG_F, n) + 0.6 * saw(_MG_F + 1.0, n) + 0.4 * saw(2 * _MG_F, n), 1500)
        return 0.5 * normpk(m) * (1 + 0.2 * np.sin(TAU * _MG_RATE * t)) + 0.05 * sine(8 * _MG_F, n)
    mot = periodic(motor, L)
    wn = normpk(bp(white(L + X, r), 700, 3500))
    whirr = loop_xfade(wn, L, X) * (0.6 + 0.4 * np.sin(TAU * _MG_RATE * tvec(L)))
    x = normpk(room) + 0.25 * mot + 0.12 * whirr
    return sat(x * 1.2, 1.3)


@sound('gun_minigun_spindown', 'weapon', 3, volume=0.75, pitch=(0.97, 1.03), lufs=-17,
       notes='0.3 s spin-down + settle clunk')
def gun_minigun_spindown(r, v):
    out = buf(0.8)
    d = 0.32
    n = ns(d)
    u = np.linspace(0, 1, n)
    f = _MG_F * (1 - u) ** 1.3 + 25
    motor = lp(saw(f, n) + 0.6 * saw(f * 1.006, n) + 0.4 * saw(f * 2, n), 1500)
    rot = np.cumsum(_MG_RATE * (1 - u) ** 1.3 + 0.5) / SR
    whirr = normpk(bp(white(n, r), 700, 3500)) * (0.55 + 0.45 * np.sin(TAU * rot))
    tr = _pulse_train(n, 28 * (1 - u) + 3)
    clicks = normpk(reson(tr, 2600, 6) + 0.6 * reson(tr, 1300, 4))
    env = env_pts(n, [(0, 1), (d * 0.7, 0.5), (d, 0)])
    add(out, (normpk(motor) * 0.7 + 0.35 * whirr + 0.3 * clicks + 0.08 * sine(f * 8, n)) * env, 0, 1.0)
    add(out, _clank(r, U(r, 500, 700), 0.05), d - 0.02, 0.6)
    add(out, thump(0.2, 140, 70, 0.015, 0.04, drive=1.5), d - 0.02, 0.4)
    return reverb(out, r, 0.4, 0.12)


@sound('gun_rocket_fire', 'weapon', 3, volume=1.0, pitch=(0.94, 1.05), lufs=-11,
       notes='launch whoosh-thump; rocket then uses rocket_flyby_loop')
def gun_rocket_fire(r, v):
    d = 1.6
    n = ns(d)
    out = _gunshot(r, d=d, crack_fc=1500, crack_tau=0.004, blast=(100, 2000, 0.05), blast_amp=0.8,
                   sub=(86 * U(r, .92, 1.08), 40, 0.06, 0.25), sub_amp=1.35, sub_drive=3.0,
                   mech=[(0.005, U(r, 900, 1200), 0.3)], rt=1.2, wet=0.25, slap=(U(r, .13, .18), 0.3))
    hiss = normpk(bp(white(n, r), 2000, 7000)) * (1 + 0.3 * smooth_noise(n, r, 30))
    add(out, hiss * env_pts(n, [(0, 0), (0.012, 1), (0.25, 0.5), (1.1, 0)]), 0, 0.45)
    roar = tvf(brown(n, r) + pink(n, r), sweep(n, 3500, 500, 0.6), 0.9, 'lp')
    add(out, normpk(roar) * env_pts(n, [(0, 0), (0.02, 1), (1.2, 0)]), 0, 0.65)
    add(out, whoosh(1.0, r, 1600, 350, 1.4, [(0, 0), (0.05, 1), (1.0, 0)]), 0.02, 0.35)
    return out


@sound('rocket_flyby_loop', 'weapon', 2, loop=True, volume=0.7, pitch=(0.97, 1.03), lufs=-18,
       notes='loop: rocket motor roar; attach to the projectile (spatial + doppler)')
def rocket_flyby_loop(r, v):
    L, X = ns(2.0), ns(0.3)
    n = L + X
    roar = normpk(lp(brown(n, r), 500) + 0.6 * normpk(bp(pink(n, r), 400, 2500)))
    roar *= 1 + 0.25 * smooth_noise(n, r, 18)
    hiss = normpk(bp(white(n, r), 3000, 8000)) * (1 + 0.3 * smooth_noise(n, r, 25))
    pops = np.zeros(n)
    for t in r.uniform(0, n / SR, int(40 * n / SR)):
        add(pops, lp(grain(r, U(r, .002, .006), 300, 4000), 3000), t, U(r, 0.2, 1.0))
    sub = normpk(lp(brown(n, r), 90))
    x = roar + 0.3 * hiss + 0.25 * normpk(pops) + 0.5 * sub
    return loop_xfade(x, L, X)


@sound('gun_railgun_charge', 'weapon', 3, volume=0.8, pitch=(1.0, 1.0), lufs=-16, max_dur=0.52,
       notes='0.5 s rising whine; gun_railgun_fire follows')
def gun_railgun_charge(r, v):
    d = 0.5
    n = ns(d)
    u = np.linspace(0, 1, n)
    f = 300 * 8 ** (u ** 1.5) * U(r, .97, 1.03)
    whine = sine(f, n) + 0.4 * sine(2 * f, n) + 0.25 * lp(saw(f / 2, n), 3000)
    spark = fm(f * 2, 1.5, 2.5 * u, n)
    hum = sine(100, n) + 0.5 * sine(200, n)
    riser = normpk(tvf(white(n, r), sweep(n, 500, 8000), 2.0, 'bp'))
    crackle = bp(white(n, r), 3000, 9000) * (white(n, r) > 1 - 0.08 * u ** 2) * 3
    env = u ** 1.5
    x = (0.6 * whine + 0.2 * spark + 0.35 * hum * u + 0.2 * riser + 0.3 * crackle) * env
    x[:ns(0.01)] *= np.linspace(0, 1, ns(0.01))
    x[-ns(0.008):] *= np.linspace(1, 0, ns(0.008))
    return x


@sound('gun_railgun_fire', 'weapon', 3, volume=1.0, pitch=(0.96, 1.04), lufs=-11,
       notes='crack + deep boom + ringing tail (lingering beam)')
def gun_railgun_fire(r, v):
    d = 2.4
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, sat(crack(r, 1500, 0.004, d=0.05), 3.5), 0, 1.1)
    add(out, sat(crack(r, 3000, 0.002, d=0.03), 2.0), 0.003, 0.6)
    nz = ns(0.3)
    add(out, fm(glide(nz, 4500, 250, 0.03), 0.5, 10 * np.exp(-tvec(nz) / 0.05), nz) * env_exp(nz, 0.08), 0, 0.55)
    add(out, thump(2.0, 78 * U(r, .93, 1.07), 40, 0.08, 0.5, drive=3.5, attack=0.001), 0, 1.3)
    add(out, burst(0.5, r, None, 1500, 0.06), 0, 0.6)
    f0 = U(r, 1100, 1400)
    add(out, modal([f0, f0 * 2.76, f0 * 5.4, f0 * 1.5], [1.2, 0.7, 0.4, 0.9], [.5, .3, .15, .3], ns(5.0), r), 0, 0.3)
    add(out, (sine(1760, n) + sine(1763.5, n)) * env_exp(n, 1.0, 0.005), 0, 0.06)
    siz = bp(white(n, r), 3000, 9000) * (white(n, r) > 0.9) * 3 * env_exp(n, 0.5, 0.01)
    add(out, siz, 0, 0.15)
    out = _slap(out, r, U(r, .12, .17), 0.3)
    return reverb(out, r, 2.0, 0.3, bright=0.9)


@sound('shell_casing', 'weapon', 6, volume=0.4, pitch=(0.9, 1.12), lufs=-24,
       notes='ejected hard-light casing: metallic-glass tinkle (some end in a tiny shatter)')
def shell_casing(r, v):
    out = buf(0.6)
    f = U(r, 3500, 6500)
    hits = [(t, a) for (t, a) in bounces(r, 0, 1.0, U(r, .06, .12), U(r, .5, .65), 0.55, 0.5)][:int(U(r, 2, 5))]
    for (t, a) in hits:
        add(out, tink(r, f * U(r, .97, 1.03), U(r, 1.5, 2.5), bright=1.2), t, a)
        add(out, modal([f * 0.47], [0.04], [0.4], ns(0.2), r), t, a * 0.5)  # brass body
    if v >= 4:  # hard light: last bounce shatters
        tl = hits[-1][0] + 0.02
        tmp = np.zeros(ns(0.3))
        _shards(tmp, r, 8, 0.02, 5000, 10000, 0.3)
        add(out, tmp, tl, 0.6)
    return reverb(out, r, 0.25, 0.08, bright=1.4)


# =============================================================================== hit confirms
def _thock(r, f, amp=1.0, dec=1.0):
    n = ns(0.2 * dec)
    x = modal([f, f * 1.58, f * 2.46], np.array([0.045, 0.025, 0.015]) * dec, [1, .5, .3], n, r)
    return amp * x


@sound('hit_confirm', 'combat', 4, volume=0.85, pitch=(0.98, 1.02), lufs=-17,
       notes='crisp dry hit-confirm "thock" for the shooter; cuts through the mix (1-3 kHz)')
def hit_confirm(r, v):
    out = buf(0.25)
    f = 1080 * (1 + 0.025 * (v - 1.5))
    add(out, crack(r, 3000, 0.0012, d=0.02), 0, 0.35)
    add(out, _thock(r, f, dec=1.3), 0, 1.0)
    add(out, thump(0.1, 260, 160, 0.01, 0.025, drive=1.5), 0, 0.45)
    add(out, sine(4200, ns(0.02)) * env_exp(ns(0.02), 0.004), 0, 0.15)
    return out


@sound('hit_confirm_heavy', 'combat', 3, volume=0.9, pitch=(0.98, 1.02), lufs=-13,
       notes='"ka-CHUNK": high tick then heavy thock (big hits / kills)')
def hit_confirm_heavy(r, v):
    out = buf(0.5)
    add(out, crack(r, 3000, 0.0012, d=0.02), 0, 0.5)
    add(out, _thock(r, 1500 * U(r, .98, 1.02), 0.6), 0, 1.0)
    t = U(r, 0.04, 0.05)
    add(out, sat(crack(r, 2000, 0.003, d=0.03), 2.0), t, 0.7)
    add(out, _thock(r, 650 * U(r, .97, 1.03), 1.0, dec=1.8), t, 1.0)
    add(out, thump(0.4, 150, 58, 0.02, 0.08, drive=2.5), t, 1.0)
    return reverb(out, r, 0.3, 0.1)


# =============================================================================== bullet impacts
@sound('bullet_impact_stone', 'destruction', 4, volume=0.8, pitch=(0.9, 1.1), lufs=-15,
       notes='slug into stone: chip + dust (variations 2 and 4 ricochet)')
def bullet_impact_stone(r, v):
    out = buf(0.7)
    add(out, sat(crack(r, 2500, 0.003), 2.0), 0, 0.6)
    for _ in range(16):
        tt = r.exponential(0.015)
        f = U(r, 1500, 5000)
        add(out, 2 * reson(grain(r, U(r, .002, .006), 1200, 7000), f, 5), tt, 0.45)
    add(out, thump(0.3, 170, 80, 0.015, 0.035, drive=2.0), 0, 0.8)
    add(out, burst(0.4, r, None, 2000, 0.08, color='pink'), 0.003, 0.5)
    if v % 2 == 1:
        nn = ns(0.35)
        tt = tvec(nn)
        fr = glide(nn, U(r, 3000, 3600), U(r, 1200, 1500), 0.12) * (1 + 0.01 * np.sin(TAU * 30 * tt))
        add(out, sine(fr, nn) * env_pts(nn, [(0, 0), (0.008, 1), (0.35, 0)], 2.0), 0.01, 0.3)
    return reverb(out, r, 0.4, 0.12)


@sound('bullet_impact_dirt', 'destruction', 4, volume=0.75, pitch=(0.9, 1.1), lufs=-16)
def bullet_impact_dirt(r, v):
    out = buf(0.5)
    add(out, thump(0.2, 130, 60, 0.02, 0.04, drive=2.0), 0, 0.9)
    add(out, burst(0.2, r, None, 900, 0.05, color='brown'), 0, 0.8)
    add(out, lp(crack(r, 1500, 0.004), 3000), 0, 0.5)
    for _ in range(22):
        tt = r.exponential(0.06)
        add(out, lp(grain(r, U(r, .003, .01), 300, 3000), 1600), tt, 0.3 * np.exp(-tt / 0.12))
    return reverb(out, r, 0.3, 0.08)


@sound('bullet_impact_wood', 'destruction', 4, volume=0.8, pitch=(0.9, 1.1), lufs=-15)
def bullet_impact_wood(r, v):
    out = buf(0.6)
    add(out, sat(crack(r, 1500, 0.003), 2.0), 0, 1.0)
    f0 = U(r, 220, 320)
    add(out, modal([f0, f0 * 2.3, f0 * 3.9], [0.08, 0.05, 0.03], [1, .6, .3], ns(0.5), r), 0, 0.7)
    add(out, thump(0.15, 220, 130, 0.01, 0.02, drive=1.5), 0, 0.6)
    for _ in range(20):
        tt = 0.005 + r.gamma(2.0, 0.025)
        nn = ns(U(r, 0.003, 0.01))
        add(out, bp(white(nn, r), 2000, 7000) * env_exp(nn, U(r, .001, .003)), tt, U(r, .1, .35) * np.exp(-tt / 0.1))
    return reverb(out, r, 0.35, 0.1)


# =============================================================================== ring blast (heavier redo)
@sound('ring_blast', 'construct', 4, volume=0.9, pitch=(0.94, 1.06), lufs=-13,
       notes='empty-hand ring blast: meaty fist-of-light whump + zap (0.4 s cadence)')
def ring_blast(r, v):
    d = 0.55
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, crack(r, 2500, 0.003), 0, 0.7)
    wn = ns(0.3)
    add(out, sat(normpk(bp(pink(wn, r), 90, 700)) * env_exp(wn, 0.045) * 1.5, 2.0), 0, 1.0)
    add(out, thump(0.5, 130 * U(r, .92, 1.08), 45, 0.03, 0.12, drive=3.0), 0, 1.2)
    zap = fm(glide(n, 1600 * U(r, .92, 1.08), 300, 0.05), 0.5, 6 * np.exp(-t / 0.07) + 0.5, n) * env_exp(n, 0.08)
    add(out, zap, 0, 0.45)
    add(out, lp(supersaw(glide(n, 240, 90, 0.05), n, r, 3, 25), 2500) * env_exp(n, 0.08), 0, 0.3)
    for _ in range(15):
        tt = r.exponential(0.06)
        add(out, grain(r, 0.002, 3000, 9000), tt, 0.1 * np.exp(-tt / 0.1))
    out = _slap(out, r, 0.09, 0.2)
    return reverb(out, r, 0.5, 0.15, bright=1.1)


@sound('ring_blast_hit', 'construct', 4, volume=0.95, pitch=(0.93, 1.07), lufs=-12,
       notes='ring blast impact: light-punch landing, bright crack + sub whump + energy splash')
def ring_blast_hit(r, v):
    d = 0.9
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    add(out, sat(crack(r, 2000, 0.005, d=0.05), 2.5), 0, 1.0)
    add(out, thump(0.8, 110 * U(r, .92, 1.08), 44, 0.035, 0.2, drive=3.2), 0, 1.3)
    wn = ns(0.4)
    add(out, sat(normpk(bp(pink(wn, r), 100, 900)) * env_exp(wn, 0.06) * 1.5, 2.0), 0, 0.8)
    add(out, fm(glide(n, 1200, 180, 0.03), 0.5, 9 * np.exp(-t / 0.03), n) * env_exp(n, 0.05), 0, 0.45)
    crackle = bp(white(n, r), 3000, 9000) * (white(n, r) > 0.93) * 3
    add(out, crackle * env_exp(n, 0.15), 0.01, 0.3)
    for _ in range(8):
        nn = ns(0.012)
        add(out, sine(U(r, 3500, 8000), nn) * np.hanning(nn), U(r, 0.01, 0.25), 0.08)
    return reverb(out, r, 0.8, 0.2, bright=1.1)


# =============================================================================== crits / KO shatter
@sound('hit_crit', 'combat', 4, volume=0.9, pitch=(0.97, 1.03), lufs=-17,
       notes='critical/headshot confirm: short glass-metal snap (3-8 kHz) over a dull 60-120 Hz thud')
def hit_crit(r, v):
    out = buf(0.3)
    f = U(r, 3300, 4300)
    snap = modal([f, f * 1.52, f * 2.1 if f * 2.1 < 8000 else f * 1.83], [0.026, 0.016, 0.01], [1, .6, .35],
                 ns(0.12), r)
    add(out, snap, 0, 1.4)
    add(out, sat(burst(0.05, r, 3000, 8000, 0.005), 1.5), 0, 1.2)
    add(out, thump(0.3, 120 * U(r, .95, 1.05), 65, 0.02, 0.055, drive=1.2), 0, 0.6)
    return lp(out, 9000, 4)  # nothing fatiguing above 9 kHz


@sound('ko_shatter', 'combat', 3, volume=1.0, pitch=(0.96, 1.04), lufs=-11,
       notes='player shatters into hard-light voxels: bigger and more final than construct_shatter + sub drop')
def ko_shatter(r, v):
    d = 2.6
    n = ns(d)
    out = buf(d)
    add(out, sat(crack(r, 2500, 0.008, d=0.08), 3.0), 0, 1.0)
    add(out, sat(crack(r, 4000, 0.004, d=0.05), 2.0), U(r, 0.01, 0.02), 0.7)
    add(out, thump(1.6, 150 * U(r, .95, 1.05), 27, 0.08, 0.35, drive=3.2, attack=0.001), 0, 1.4)
    _shards(out, r, 90, 0.12, 1800, 10000, 0.4)
    nb = ns(0.6)
    add(out, normpk(hp(white(nb, r), 3500)) * (1 + 0.8 * smooth_noise(nb, r, 400)) * env_exp(nb, 0.1), 0, 0.5)
    f = glide(n, 440 * U(r, .95, 1.05), 55, 0.2)
    pw = tvf(supersaw(f, n, r, 7, 25), glide(n, 10000, 200, 0.25), 1.5, 'lp') * env_exp(n, 0.3, 0.002)
    add(out, normpk(pw), 0, 0.4)
    from sounds import _cascade
    _cascade(out, r, 30, 0.08, 2.0, 2000, 7000, 0.3)
    m = ns(1.2)
    fall = sum(sine(glide(m, fr, fr / 2, 0.5), m) for fr in (2637.0, 3136.0, 3951.0))  # soul leaving
    add(out, fall * env_pts(m, [(0, 0), (0.15, 1), (1.2, 0)]), 0.1, 0.06)
    add(out, burst(1.5, r, None, 250, 0.4, attack=0.02, color='brown'), 0, 0.5)
    return reverb(out, r, 1.8, 0.28, bright=1.5)
