"""TRANSFORMER FEEL (bd_game `present/transformer_feel/`): the sounds that make ANY transformer feel
awesome. Original synthesis only, in the family of `sounds_transform.py` (tuned panel clicks, servo
whirs, plate clunks, a solid lock, a hydraulic hiss) — never an imitation of any franchise's sounds.

* Transform: size-classed servo BEDS (the click cascade is played by the game, one click per panel
  lock, in sync with the picture), combat SNAPs, size-classed final LOCKs, a heavy plate clunk for big
  bodies, the mid-air whoosh, the visor zing.
* Weight: robot footsteps (light / mid; heavy = `robot_footstep_heavy`), the knee servo, the landing.
* Vehicle: engine loops per archetype (V8, twin, turbine, tank treads), tyre screech loop, the
  drift-boost burst.
* Combat: robot swings (heavy / fast), metal hit layers (heavy / fast), the finisher crunch, the
  perfect-dodge sting; beasts: chomp, tail whoosh, pounce.
Dry (no baked room); noise only as shaped, subordinate texture or where the source is air/gas."""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, grit, material_hit, micro
from sounds import P_IMPACT, P_MICRO, P_SOFT, P_TONAL, U, sound
from sounds_transform import clunk, hiss, lock, panel_click, servo


# ------------------------------------------------------------------------------------------ helpers
def _ratchet(r, d, n, k0, k1, amp=1.0, accel=1.4):
    """A dense run of panel clicks accelerating over d (pitch factor k0 -> k1)."""
    out = np.zeros(ns(d + 0.15))
    ts = d * np.linspace(0, 1, n) ** accel
    for i, t in enumerate(ts):
        k = k0 * (k1 / k0) ** (i / max(1, n - 1))
        add(out, panel_click(r, k * U(r, 0.95, 1.05), U(r, 0.5, 0.9)), t, amp)
    return out


def _gear_grind(r, d, f0, f1, amp=1.0):
    """Big slow gearbox: low saw through a moving band + tooth chatter (the mass of a giant moving)."""
    n = ns(d)
    f = sweep(n, f0, f1, 0.9)
    tooth = 1 + 0.5 * np.sign(np.sin(TAU * np.cumsum(f * 0.25) / SR))
    body = lp(saw(f, n) + 0.6 * saw(f * 1.51, n), 900) * tooth
    grind = normpk(tvf(pink(n, r), f * 5, 4.0, 'bp')) * 0.25 * (1 + 0.5 * smooth_noise(n, r, 25))
    e = env_pts(n, [(0, 0), (0.04, 1), (d * 0.85, 0.85), (d, 0)])
    return amp * (normpk(body) + grind) * e


def _mass_groan(r, d, f0, f1):
    """Sub-heavy groan of big plates shifting (brown noise through a gliding low resonance)."""
    n = ns(d)
    g = tvf(brown(n, r), sweep(n, f0, f1), 6.0, 'bp')
    return normpk(g) * env_pts(n, [(0, 0), (d * 0.25, 1), (d * 0.8, 0.7), (d, 0)])


def _seq(r, up, cls):
    """0.9 s transformation bed: unlatch pop, the main actuator carrying the pitch contour, two
    secondary servos, a pneumatic breath; no click cascade (the game plays one click per panel lock)
    and no final lock (played on the stand-up)."""
    d = 0.9
    out = buf(d)
    # (actuator f range, secondary base, voice gains) per size class: small zippy/high, big deep.
    lo, hi = [(520, 1700), (240, 860), (80, 300)][cls]
    a0, a1 = (lo, hi) if up else (hi, lo)
    # Unlatch: a pneumatic tchk + a micro chirp.
    add(out, sat(click(r, [3200, 2200, 1400][cls], 0.002), 2.0), 0.0, 0.7)
    add(out, panel_click(r, [1.5, 1.0, 0.6][cls], 0.8), 0.004)
    add(out, servo(r, 0.07, a0 * 1.6, a0 * (2.2 if up else 1.2)), 0.01, 0.35)
    # The main actuator: the contour.
    add(out, servo(r, 0.72, a0, a1), 0.06, [0.7, 0.8, 0.75][cls])
    # Secondary servos, staggered (limbs / doors).
    for i in range(2):
        t = 0.14 + 0.2 * i + U(r, -0.03, 0.03)
        b = U(r, 0.85, 1.2) * (lo * 1.4 if up else hi * 0.9)
        add(out, servo(r, U(r, 0.18, 0.28), b, b * (1.35 if up else 0.72)), t, 0.3)
    # Pneumatic breath across the rearrange beat.
    n = ns(0.6)
    breath = normpk(tvf(pink(n, r), sweep(n, *( (1800, 4200) if up else (4200, 1600)), 0.8), 2.5, 'bp'))
    add(out, breath * env_pts(n, [(0, 0), (0.2, 1), (0.6, 0)]), 0.18, [0.12, 0.1, 0.08][cls])
    if cls == 2:
        # Giants: gearbox grind + the groan of mass moving + two limb drops (sub).
        add(out, _gear_grind(r, 0.75, a0 * 0.5, a1 * 0.5), 0.05, 0.55)
        add(out, _mass_groan(r, 0.8, 70 if up else 140, 140 if up else 60), 0.08, 0.5)
        for t in (0.3, 0.56):
            add(out, thump(0.5, 95, 34, 0.05, 0.16, drive=2.2), t + U(r, -0.02, 0.02), 0.8)
            add(out, clunk(r, 0.45, 0.6), t, 1.0)
        return carve_heavy(glue(out, 1.5), -3, -3)
    if cls == 1:
        for t in (0.32, 0.55):
            add(out, clunk(r, U(r, 0.8, 1.0), 0.4), t + U(r, -0.03, 0.03))
    else:
        add(out, _ratchet(r, 0.12, 5, 1.5, 1.9 if up else 1.2, 0.35), 0.4)
    return glue(out, 1.4)


# ------------------------------------------------------------------------------------------ transform
for _up, _dir in ((True, 'robot'), (False, 'alt')):
    for _cls, _sz in enumerate(('s', 'm', 'l')):
        def _mk(up=_up, cls=_cls):
            return lambda r, v: _seq(r, up, cls)
        sound(f'tf_seq_{_dir}_{_sz}', 'transform', 2, pitch=P_SOFT, lufs=[-17, -16, -14][_cls],
              notes=f'0.9 s {"rising" if _up else "falling"} servo bed, {["small/zippy", "mid/weighty", "giant/deep + sub"][_cls]}; '
                    'clicks come from the game (one per panel lock), lock on the stand-up')(_mk())


def _snap(r, up):
    d = 0.34
    out = buf(d)
    add(out, _ratchet(r, 0.16, 11, 1.1 if up else 1.9, 1.9 if up else 1.0, 0.7, accel=0.8), 0.0)
    f0, f1 = (420, 1500) if up else (1500, 380)
    add(out, servo(r, 0.2, f0, f1), 0.0, 0.55)
    add(out, whoosh(0.22, r, 900 if up else 2600, 2600 if up else 700, 1.8), 0.0, 0.3)
    add(out, lock(r, 1.15 if up else 0.9), 0.19, 1.0)
    add(out, hiss(r, 0.14), 0.22, 0.3)
    return glue(out, 1.8)


@sound('tf_snap_robot', 'transform', 3, pitch=P_MICRO, lufs=-14, notes='0.3 s combat transform to robot: rising ratchet zip + hard lock')
def tf_snap_robot(r, v):
    return _snap(r, True)


@sound('tf_snap_alt', 'transform', 3, pitch=P_MICRO, lufs=-14, notes='0.3 s combat transform to vehicle: falling ratchet zip + hard lock')
def tf_snap_alt(r, v):
    return _snap(r, False)


@sound('tf_lock_s', 'transform', 3, pitch=P_MICRO, lufs=-17, notes='small body final lock: bright crack + small clunk + ping + puff')
def tf_lock_s(r, v):
    out = buf(0.5)
    add(out, sat(click(r, 3000, 0.002), 2.0), 0, 0.8)
    add(out, clunk(r, 1.5, 0.8), 0)
    f = U(r, 1700, 2100)
    add(out, modal([f, f * 2.76, f * 5.4], [0.12, 0.06, 0.03], [1, .5, .2], ns(0.5), r), 0.002, 0.25)
    add(out, hiss(r, 0.12), 0.03, 0.25)
    return glue(out, 1.5)


@sound('tf_lock_m', 'transform', 3, pitch=P_MICRO, lufs=-14, notes='6 m final lock: crack + clunk + sub knock + ring + hydraulic hiss')
def tf_lock_m(r, v):
    out = buf(0.9)
    add(out, lock(r, U(r, 0.95, 1.05)), 0, 1.0)
    add(out, hiss(r, 0.25), 0.04, 0.35)
    return glue(out, 1.6)


@sound('tf_lock_l', 'transform', 3, pitch=P_SOFT, lufs=-12, notes='giant final lock: slam + sub drop + long low ring + long hiss + grit')
def tf_lock_l(r, v):
    d = 1.8
    out = buf(d)
    add(out, lock(r, 0.6), 0, 1.0)
    add(out, thump(1.5, 70, 26, 0.08, 0.45, drive=2.6, attack=0.002), 0, 1.3)
    f = U(r, 170, 210)
    add(out, modal([f, f * 2.02, f * 2.93, f * 4.4], [0.9, 0.55, 0.35, 0.2], [1, .6, .4, .2], ns(1.6), r), 0.005, 0.3)
    add(out, hiss(r, 0.6), 0.06, 0.45)
    add(out, grit(r, 14, 0.05, 700, 2600, 0.004), 0.02, 0.25)
    return carve_heavy(glue(out, 1.8), -4, -3)


@sound('tf_panel_clunk', 'transform', 6, pitch=P_IMPACT, lufs=-18, notes='heavy plate clunk (per panel lock on big bodies)')
def tf_panel_clunk(r, v):
    out = buf(0.5)
    add(out, clunk(r, U(r, 0.5, 0.7)), 0, 1.0)
    add(out, thump(0.4, 120 * U(r, 0.9, 1.1), 48, 0.03, 0.09, drive=2.0), 0, 0.8)
    f = U(r, 300, 420)
    add(out, modal([f, f * 2.3, f * 3.9], [0.2, 0.1, 0.05], [1, .5, .25], ns(0.5), r), 0.003, 0.18)
    return glue(out, 1.5)


@sound('tf_air_whoosh', 'transform', 3, pitch=P_MICRO, lufs=-18, notes='mid-air transformation: a spinning-panel whoosh with rotor-like flutter')
def tf_air_whoosh(r, v):
    d = 0.6
    n = ns(d)
    out = buf(d)
    g = pink(n, r)
    band = tvf(g, env_pts(n, [(0, 500), (0.25, 2200), (0.6, 700)]), 2.2, 'bp')
    rate = env_pts(n, [(0, 9), (0.3, 22), (0.6, 12)])
    flutter = 0.6 + 0.4 * np.sin(TAU * np.cumsum(rate) / SR)
    add(out, normpk(band) * flutter * env_pts(n, [(0, 0), (0.18, 1), (0.6, 0)]), 0, 1.0)
    for _ in range(5):
        add(out, panel_click(r, U(r, 1.0, 1.5), U(r, 0.2, 0.4)), U(r, 0.05, 0.4))
    return carve_foley(out, 180, 12000)


@sound('tf_eye_zing', 'transform', 3, pitch=P_MICRO, lufs=-20, notes='visor ignites: a short rising FM zing + electric buzz')
def tf_eye_zing(r, v):
    d = 0.4
    n = ns(d)
    t = tvec(n)
    f = sweep(n, U(r, 900, 1100), U(r, 2200, 2600), 0.5)
    z = fm(f, 2.0, 1.8 * np.exp(-t / 0.12), n) * env_exp(n, 0.14, 0.004)
    buzz = normpk(lp(square(f / 8, n, duty=0.2), 3500)) * env_exp(n, 0.06, 0.002) * 0.3
    return carve_ui(z + buzz, 900, 13000)


# ------------------------------------------------------------------------------------------ weight
@sound('robot_step_light', 'movement', 6, pitch=P_IMPACT, lufs=-24, notes='small robot foot: metal clank + tiny knock')
def robot_step_light(r, v):
    out = buf(0.3)
    add(out, click(r, 2500, 0.002), 0, 0.6)
    f = U(r, 850, 1300)
    add(out, modal([f, f * 1.62, f * 2.71, f * 4.1], [0.05, 0.035, 0.02, 0.012], [1, .6, .4, .2], ns(0.3), r), 0.001, 0.6)
    add(out, thump(0.2, 260, 150, 0.01, 0.025, drive=1.3), 0, 0.6)
    add(out, grit(r, 4, 0.01, 1500, 3500, 0.002), 0.004, 0.3)
    return carve_foley(out, 180)


@sound('robot_step_mid', 'combat', 6, pitch=P_IMPACT, lufs=-18, notes='6 m robot foot: metal clang + thud + grit')
def robot_step_mid(r, v):
    out = buf(0.6)
    add(out, sat(click(r, 1800, 0.003), 1.8), 0, 0.6)
    f = U(r, 380, 560)
    add(out, modal([f, f * 1.83, f * 2.9, f * 4.4], [0.12, 0.08, 0.05, 0.03], [1, .55, .35, .2], ns(0.6), r), 0.002, 0.35)
    add(out, thump(0.5, 130 * U(r, .92, 1.08), 46, 0.03, 0.11, drive=2.2), 0, 1.0)
    add(out, grit(r, 8, 0.02, 900, 2800, 0.003), 0.004, 0.25)
    return carve_heavy(glue(out, 1.4), -4, -3)


@sound('robot_servo_step', 'movement', 4, pitch=P_MICRO, lufs=-27, notes='knee servo per robot step: a short falling whir + tiny hiss')
def robot_servo_step(r, v):
    out = buf(0.25)
    b = U(r, 480, 620)
    add(out, servo(r, 0.13, b * 1.3, b * 0.8), 0, 1.0)
    add(out, hiss(r, 0.06), 0.1, 0.25)
    return carve_foley(out, 200)


@sound('robot_land', 'combat', 3, pitch=P_SOFT, lufs=-13, notes='robot landing: crack + metal clang + big thud + shock-absorber hiss')
def robot_land(r, v):
    out = buf(1.2)
    add(out, sat(click(r, 1500, 0.004), 2.2), 0, 0.8)
    add(out, material_hit(r, 'metal', 1.0, 1.2), 0.001, 0.3)
    add(out, thump(1.0, 95 * U(r, .92, 1.08), 30, 0.05, 0.22, drive=2.8, attack=0.002), 0, 1.3)
    add(out, hiss(r, 0.3), 0.05, 0.35)
    add(out, grit(r, 12, 0.03, 700, 2600, 0.004), 0.01, 0.3)
    return carve_heavy(glue(out, 1.8), -5, -3)


# ------------------------------------------------------------------------------------------ vehicle loops
def _engine(r, Ld, fire_hz, pattern, body_lp, exhaust_f, exhaust_q, rasp_f, rasp_amt, whine_f, whine_amt, clatter=0.05, air=0.25):
    """Pulse engine loop: firings on a (possibly uneven) pattern per cycle, exhaust resonance, intake
    rasp, a quantised whine. fire_hz * Ld must give whole cycles."""
    L = ns(Ld)
    cyc = int(round(fire_hz * Ld))
    pulses = np.zeros(L)
    clat = np.zeros(L)
    per = L / cyc
    for i in range(cyc):
        for off, a in pattern:
            pos = int(round((i + off) * per)) % L
            pn = ns(0.02)
            p = sine(U(r, 60, 90), pn) * env_exp(pn, 0.006, 0.0008) + air * pink(pn, r) * env_exp(pn, 0.003, 0.0005)
            add_circ(pulses, p, pos, a * U(r, 0.85, 1.0))
            if clatter > 0:
                add_circ(clat, micro(r, U(r, 1800, 3400), 0.003, 3, (1.4, 2.3)), pos + ns(0.003), a * U(r, 0.4, 1.0))
    body = circ(lambda y: lp(y, body_lp, 4), pulses)
    exh = circ(lambda y: reson(y, exhaust_f, exhaust_q), pulses)
    rasp = circ(lambda y: reson(y, rasp_f, 3.0), pulses)
    wf = qf(whine_f, Ld)
    ton = periodic(lambda m: sine(wf, m) + 0.35 * sine(qf(whine_f * 2, Ld), m), L)
    x = normpk(body) + 0.7 * normpk(exh) + rasp_amt * normpk(rasp) + whine_amt * ton + clatter * normpk(circ(lambda y: hp(y, 1500), clat))
    return sat(x, 1.6)


@sound('engine_car_loop', 'vehicle', 2, loop=True, pitch=P_TONAL, lufs=-22, notes='loop: cross-plane V8 burble; the game pitches by speed (~0.7-1.8x)')
def engine_car_loop(r, v):
    # 4 firings per crank rev with the V8 "lope": uneven spacing / level.
    pattern = [(0.0, 1.0), (0.22, 0.75), (0.5, 0.95), (0.74, 0.7)]
    return _engine(r, 2.0, [24.0, 22.0][v], pattern, 420, 120, 3.0, 650, 0.35, 1900, 0.012, 0.03, 0.12)


@sound('engine_bike_loop', 'vehicle', 2, loop=True, pitch=P_TONAL, lufs=-22, notes='loop: V-twin (uneven pair firing) with a bright rasp; pitched by speed (~0.8-2.2x)')
def engine_bike_loop(r, v):
    pattern = [(0.0, 1.0), (0.31, 0.85)]
    return _engine(r, 2.0, [26.0, 24.0][v], pattern, 700, 210, 3.5, 1350, 0.5, 2600, 0.02, 0.04, 0.08)


@sound('engine_tank_loop', 'vehicle', 2, loop=True, pitch=P_TONAL, lufs=-22, notes='loop: deep diesel + tread links clanking + a squeal; pitched by speed')
def engine_tank_loop(r, v):
    Ld = 2.0
    L = ns(Ld)
    eng = _engine(r, Ld, [15.0, 14.0][v], [(0.0, 1.0), (0.5, 0.8)], 260, 70, 2.5, 420, 0.2, 1400, 0.02, 0.03)
    links = np.zeros(L)
    k = int(round(9.0 * Ld))
    for i in range(k):
        f = U(r, 650, 1100)
        clank = modal([f, f * 1.7, f * 2.6], [0.03, 0.02, 0.012], [1, .5, .3], ns(0.12), r)
        add_circ(links, clank, int(round((i + U(r, -0.08, 0.08)) * L / k)), U(r, 0.6, 1.0))
    sq = periodic(lambda m: sine(qf(2300, Ld), m) * (0.5 + 0.5 * np.sin(TAU * qf(1.5, Ld) * tvec(m))), L)
    return sat(normpk(eng) + 0.45 * normpk(links) + 0.03 * sq, 1.3)


@sound('engine_jet_loop', 'vehicle', 2, loop=True, pitch=P_TONAL, lufs=-22, notes='loop: turbine whine partials + a shaped roar; pitched by speed (~0.8-1.4x)')
def engine_jet_loop(r, v):
    Ld = 2.0
    L, X = ns(Ld), ns(0.3)
    n = L + X
    g = pink(n, r)
    roar = normpk(lp(g, 700, 2) + 0.6 * reson(g, 1700 + 200 * v, 2.5) + 0.25 * reson(g, 3800, 4))
    roar *= 1 + 0.15 * smooth_noise(n, r, 8)
    noise = loop_xfade(roar, L, X)
    w = [2900.0, 3300.0][v]
    ton = periodic(lambda m: sine(qf(w, Ld), m) + 0.6 * sine(qf(w * 1.005, Ld), m) + 0.4 * sine(qf(w * 2.0, Ld), m)
                   + 0.25 * sine(qf(w * 0.5, Ld), m), L)
    rumble = periodic(lambda m: sine(qf(55, Ld), m) + 0.5 * sine(qf(82.5, Ld), m), L)
    return circ(lambda y: hp(y, 30), noise + 0.18 * normpk(ton) + 0.25 * normpk(rumble))


@sound('tyre_screech_loop', 'vehicle', 2, loop=True, pitch=P_TONAL, lufs=-22, notes='loop: tyre squeal (stick-slip tonal + a scrub band); gain by slip')
def tyre_screech_loop(r, v):
    Ld = 1.5
    L, X = ns(Ld), ns(0.25)
    n = L + X
    g = pink(n, r)
    scrub = normpk(reson(g, 2600 + 300 * v, 3.0) + 0.6 * reson(g, 4100, 4.0)) * (1 + 0.5 * smooth_noise(n, r, 35))
    noise = loop_xfade(scrub, L, X)
    f0 = [1250.0, 1480.0][v]

    def squeal(m):
        t = tvec(m)
        wob = 1 + 0.012 * np.sin(TAU * qf(7.0, Ld) * t) + 0.006 * np.sin(TAU * qf(23.0, Ld) * t)
        return (sine(qf(f0, Ld) * wob, m) + 0.5 * sine(qf(f0 * 2, Ld) * wob, m) + 0.25 * sine(qf(f0 * 3, Ld) * wob, m)) \
            * (0.75 + 0.25 * np.sin(TAU * qf(11.0, Ld) * t))
    ton = periodic(squeal, L)
    return circ(lambda y: carve_foley(y, 400, 12000), 0.6 * normpk(ton) + 0.4 * noise)


@sound('drift_boost_burst', 'vehicle', 3, pitch=P_MICRO, lufs=-15, notes='drift boost / boost-step: turbo pop + flame whoosh + rising whistle + thump')
def drift_boost_burst(r, v):
    d = 0.7
    n = ns(d)
    out = buf(d)
    f = U(r, 300, 420)
    add(out, modal([f, f * 1.9, f * 3.1], [0.04, 0.025, 0.012], [1, .5, .3], ns(0.2), r), 0, 0.7)
    add(out, sat(click(r, 1500, 0.003), 2.0), 0, 0.6)
    add(out, thump(0.4, 140, 55, 0.02, 0.08, drive=2.0), 0, 0.8)
    flame = normpk(tvf(pink(n, r), sweep(n, 3500, 700, 0.5), 2.0, 'bp')) * (1 + 0.3 * smooth_noise(n, r, 50))
    add(out, flame * env_exp(n, 0.18, 0.006), 0.005, 0.8)
    wn = ns(0.35)
    add(out, sine(sweep(wn, 1800, 4200, 0.6), wn) * env_pts(wn, [(0, 0), (0.08, 1), (0.35, 0)]), 0.02, 0.12)
    return glue(out, 1.5)


# ------------------------------------------------------------------------------------------ combat
@sound('robot_swing_heavy', 'combat', 4, pitch=P_MICRO, lufs=-18, notes='heavy robot swing: servo rise + low heavy whoosh + hydraulic puff')
def robot_swing_heavy(r, v):
    out = buf(0.6)
    b = U(r, 220, 300)
    add(out, servo(r, 0.16, b, b * 1.6), 0, 0.45)
    add(out, whoosh(0.4, r, 380, 140, 1.4, [(0, 0), (0.14, 1), (0.4, 0)]), 0.05, 0.9)
    add(out, thump(0.3, 90, 50, 0.04, 0.08, drive=1.4, attack=0.02), 0.1, 0.4)
    add(out, hiss(r, 0.1), 0.22, 0.25)
    return glue(out, 1.3)


@sound('robot_swing_fast', 'combat', 4, pitch=P_MICRO, lufs=-19, notes='fast robot strike: sharp whoosh + servo chirp + snap')
def robot_swing_fast(r, v):
    out = buf(0.35)
    b = U(r, 700, 900)
    add(out, servo(r, 0.07, b, b * 1.8), 0, 0.4)
    add(out, whoosh(0.2, r, 1100, 3200, 1.6, [(0, 0), (0.06, 1), (0.2, 0)]), 0.02, 1.0)
    add(out, panel_click(r, 1.4, 0.5), 0.0)
    return carve_foley(out, 220, 13000)


@sound('robot_punch_heavy', 'combat', 5, pitch=P_IMPACT, lufs=-14, notes='heavy metal fist on body: crack + low steel clang + thump + grit')
def robot_punch_heavy(r, v):
    out = buf(0.8)
    add(out, sat(click(r, 1800, 0.004), 2.2), 0, 0.9)
    f = U(r, 170, 250)
    add(out, modal([f, f * 2.32, f * 3.7, f * 5.2], [0.18, 0.1, 0.06, 0.035], [1, .6, .35, .2], ns(0.8), r), 0.002, 0.45)
    add(out, thump(0.6, 125 * U(r, .92, 1.08), 38, 0.04, 0.14, drive=2.6, attack=0.0015), 0, 1.2)
    add(out, grit(r, 8, 0.02, 900, 3000, 0.003), 0.003, 0.3)
    return carve_heavy(glue(out, 1.8), -4, -3)


@sound('robot_punch_fast', 'combat', 5, pitch=P_IMPACT, lufs=-16, notes='fast metal strike: snap + bright clank + small thump')
def robot_punch_fast(r, v):
    out = buf(0.4)
    add(out, sat(click(r, 2600, 0.003), 2.0), 0, 0.9)
    f = U(r, 600, 900)
    add(out, modal([f, f * 1.9, f * 3.1], [0.06, 0.035, 0.02], [1, .5, .3], ns(0.4), r), 0.001, 0.5)
    add(out, thump(0.3, 170, 70, 0.02, 0.06, drive=1.8), 0, 0.8)
    return glue(out, 1.6)


@sound('robot_finisher_crunch', 'combat', 3, pitch=P_SOFT, lufs=-11, notes='finisher: crack + bending-plate groan + crunch cluster + sub slam + ring')
def robot_finisher_crunch(r, v):
    d = 1.4
    n = ns(d)
    out = buf(d)
    add(out, sat(click(r, 1500, 0.005), 2.5), 0, 1.0)
    add(out, thump(1.1, 90, 26, 0.06, 0.3, drive=3.0, attack=0.002), 0, 1.3)
    # The plate bending: a gliding inharmonic groan.
    gn = ns(0.5)
    gf = sweep(gn, U(r, 420, 520), U(r, 160, 200), 0.6)
    groan = (saw(gf, gn) + 0.6 * saw(gf * 1.37, gn)) * env_pts(gn, [(0, 0), (0.02, 1), (0.5, 0)])
    add(out, reson(groan, 700, 2.0) + 0.5 * lp(groan, 400), 0.01, 0.25)
    add(out, grit(r, 26, 0.04, 600, 3200, 0.004), 0.004, 0.55)
    f = U(r, 210, 260)
    add(out, modal([f, f * 2.4, f * 3.9], [0.5, 0.25, 0.12], [1, .5, .25], ns(1.2), r), 0.004, 0.25)
    return carve_heavy(glue(out, 2.2), -4, -3)


@sound('tf_focus_sting', 'feedback', 2, pitch=P_TONAL, lufs=-19, notes='perfect dodge: a short air swell into a bright chrome chime (E minor)')
def tf_focus_sting(r, v):
    d = 0.6
    out = buf(d)
    sn = ns(0.09)
    swell = normpk(tvf(pink(sn, r), sweep(sn, 3000, 9000, 0.7), 2.0, 'bp')) * env_pts(sn, [(0, 0), (0.085, 1), (0.09, 0)])
    add(out, swell, 0, 0.5)
    for f, a in ((1318.5, 1.0), (1975.5, 0.7), (2637.0, 0.45)):  # E6 B6 E7
        n = ns(0.5)
        add(out, fm_bell(n, f, 1.41, 2.0, 0.05, 0.22), 0.085, a)
    return carve_ui(out, 1300, 14000)


# ------------------------------------------------------------------------------------------ beasts
@sound('beast_chomp', 'combat', 4, pitch=P_IMPACT, lufs=-14, notes='metal jaws snapping shut: two clanks + crunch + thump')
def beast_chomp(r, v):
    out = buf(0.6)
    for t, k in ((0.0, 1.0), (U(r, 0.012, 0.02), 0.8)):
        f = U(r, 380, 520) * (1.15 if t else 1.0)
        add(out, modal([f, f * 1.8, f * 2.9], [0.07, 0.04, 0.02], [1, .5, .3], ns(0.3), r), t, k * 0.6)
        add(out, sat(click(r, 2000, 0.003), 2.0), t, k * 0.8)
    add(out, thump(0.4, 120, 45, 0.03, 0.1, drive=2.2), 0.01, 1.0)
    add(out, grit(r, 12, 0.02, 800, 2800, 0.003), 0.015, 0.4)
    return carve_heavy(glue(out, 1.8), -4, -3)


@sound('beast_tail_whoosh', 'combat', 3, pitch=P_MICRO, lufs=-16, notes='tail sweep: long heavy whoosh + segment clatter')
def beast_tail_whoosh(r, v):
    d = 0.8
    out = buf(d)
    add(out, whoosh(0.7, r, 200, 480, 1.3, [(0, 0), (0.3, 1), (0.7, 0)]), 0, 1.0)
    add(out, whoosh(0.5, r, 900, 400, 2.0, [(0, 0), (0.25, 1), (0.5, 0)]), 0.12, 0.35)
    for i in range(7):
        add(out, panel_click(r, U(r, 0.6, 0.9), U(r, 0.25, 0.45)), 0.1 + i * 0.06 + U(r, -0.01, 0.01))
    return glue(out, 1.3)


@sound('pounce_whoosh', 'combat', 3, pitch=P_MICRO, lufs=-17, notes='pounce / lunge: rising whoosh + servo growl')
def pounce_whoosh(r, v):
    out = buf(0.55)
    add(out, whoosh(0.45, r, 300, 1500, 1.5, [(0, 0), (0.3, 1), (0.45, 0)]), 0, 1.0)
    b = U(r, 160, 220)
    add(out, servo(r, 0.3, b, b * 1.5), 0.02, 0.35)
    return glue(out, 1.3)
