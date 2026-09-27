"""TOOLBOX PRESENTATION: the move grammar's sounds (bd_game present::powers::movefx, assets/tuning/move_fx.ron).

Two layers per move: the op's VERB (a blink implodes and pops, a dash tears the air, a shove is a
pressure wave, a slam whistles down and craters, a swap claps twice, time ticks) and the style's BODY
(rubber creaks and boings, rigid telescopes in clacks and rings like a bell, smoke BAMFs, speedforce
zips with crackle, lightning cracks, holy chimes, void thumps, glitch bit-crushes, paper flutters…).
Each body has its own spectrum so two moves with the same verb never sound alike.
"""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, grit, hl_chord, hl_ping, hl_shards, material_hit, micro, note, shatter
from sounds import P_IMPACT, P_MICRO, P_SOFT, P_T10, P_TONAL, U, sound, string_twang, swish


def _env_ar(n, a, d):
    """Attack (s) then exponential-ish release to the end."""
    t = tvec(n)
    return np.clip(t / max(a, 1e-4), 0, 1) * np.exp(-np.maximum(t - a, 0) / max(d, 1e-4))


# =============================================================================== verbs
@sound('mv_blink_implode', 'movement', 3, pitch=P_MICRO, lufs=-22, max_dur=0.35,
       notes='blink departure: air sucked into a point (reverse swell of band noise + a rising pinch)')
def mv_blink_implode(r, v):
    d = 0.28
    n = ns(d)
    u = np.linspace(0, 1, n)
    out = buf(d + 0.02)
    air = normpk(tvf(pink(n, r), sweep(n, 500, [3800, 4400, 3200][v], 2.0), 3.0, 'bp')) * u ** 2.6
    add(out, air, 0, 0.7)
    add(out, sine(sweep(n, 180, 900, 2.5), n) * u ** 3 * 0.5, 0, 0.4)
    add(out, click(r, 5000, 0.002), d - 0.004, 0.5)
    out[n - ns(0.004):n] *= np.linspace(1, 0.2, ns(0.004))
    return carve_foley(out, 220, 12000)


@sound('mv_blink_pop', 'movement', 4, pitch=P_MICRO, lufs=-20, max_dur=0.4,
       notes='blink arrival: air displaced outward — a hollow pop + short burst')
def mv_blink_pop(r, v):
    out = buf(0.4)
    add(out, thump(0.2, [420, 380, 460, 400][v], 140, 0.02, 0.04, drive=1.4), 0, 0.8)
    add(out, burst(0.2, r, 700, 6000, 0.035, color='pink'), 0, 0.6)
    add(out, whoosh(0.18, r, 3000, 600, 1.2, [(0, 1), (0.02, 0.7), (0.18, 0)]), 0.004, 0.35)
    return carve_foley(out, 160, 12000)


@sound('mv_air_tear', 'movement', 4, pitch=P_MICRO, lufs=-20, max_dur=0.6,
       notes='dash: the air tears open — a ripping band sweep + low push')
def mv_air_tear(r, v):
    d = 0.5
    n = ns(d)
    out = buf(d)
    rip = normpk(tvf(white(n, r), sweep(n, [900, 1200, 800, 1000][v], 5200, 0.5), 2.2, 'bp'))
    rip *= 1 + 0.8 * smooth_noise(n, r, 140)
    add(out, rip * env_pts(n, [(0, 0), (0.015, 1), (0.12, 0.5), (d, 0)]), 0, 0.6)
    add(out, thump(0.3, 160, 60, 0.04, 0.07, drive=1.6), 0, 0.55)
    add(out, swish(r, 0.35, 2200, 400, 1.4), 0.02, 0.35)
    return carve_foley(out, 90, 12000)


@sound('mv_skid', 'movement', 4, pitch=P_IMPACT, lufs=-23, max_dur=0.7,
       notes='landing skid: gravel scrape decelerating + a scuff')
def mv_skid(r, v):
    d = 0.6
    out = buf(d)
    rate = [380, 320, 420, 300][v]
    n = ns(d)
    scrape = np.zeros(n)
    for t in np.sort(r.uniform(0, d, int(rate * d))):
        add(scrape, micro(r, U(r, 900, 3800), U(r, 0.0015, 0.004), 2, (1.3, 2.2)), t, U(r, 0.2, 1.0) * (1 - t / d) ** 1.5)
    add(out, scrape, 0, 0.9)
    add(out, material_hit(r, 'stone', 1.2, 0.8), 0, 0.4)
    add(out, swish(r, 0.4, 2600, 700, 1.8), 0, 0.25)
    return carve_foley(out, 180, 9000)


@sound('mv_shove_wave', 'combat', 4, pitch=P_MICRO, lufs=-16, max_dur=0.8,
       notes='shove: a pressure wave off the fist — sub punch + air slap + dragging wind tail')
def mv_shove_wave(r, v):
    out = buf(0.8)
    add(out, thump(0.5, [140, 120, 160, 130][v], 42, 0.05, 0.12, drive=2.2), 0, 1.0)
    add(out, burst(0.25, r, 250, 3000, 0.03, color='pink'), 0, 0.8)
    add(out, whoosh(0.55, r, 2400, 300, 0.9, [(0, 1), (0.05, 0.6), (0.55, 0)]), 0.01, 0.45)
    add(out, click(r, 2500, 0.004), 0, 0.4)
    return carve_heavy(out)


@sound('mv_ground_crack', 'destruction', 4, pitch=P_IMPACT, lufs=-19, max_dur=0.9,
       notes='the ground splits: a rolling stone crackle run + a low rip')
def mv_ground_crack(r, v):
    out = buf(0.9)
    for k in range(18):
        t = 0.02 + 0.5 * (k / 18) ** 1.4 + U(r, -0.01, 0.01)
        add(out, material_hit(r, 'stone', U(r, 0.8, 1.6), U(r, 0.3, 0.8)), max(t, 0), U(r, 0.2, 0.5) * (1 - k / 22))
    add(out, burst(0.6, r, 80, 900, 0.2, color='brown'), 0, 0.6)
    add(out, shatter(r, 'stone', [6, 9, 5, 8][v], 0.4, 0.5, 0.6), 0.05, 0.3)
    return carve_heavy(out)


@sound('mv_slam_dread', 'combat', 2, pitch=P_T10, lufs=-21, max_dur=1.3,
       notes='slam windup: a falling whistle that rises in tension (downward glide + growing sub)')
def mv_slam_dread(r, v):
    d = 1.2
    n = ns(d)
    u = np.linspace(0, 1, n)
    out = buf(d)
    f = sweep(n, [1900, 1700][v], 700, 0.8)
    add(out, sine(f, n) * (0.3 + 0.7 * u) * (1 - u ** 8), 0, 0.35)
    add(out, normpk(tvf(pink(n, r), f * 1.6, 5.0, 'bp')) * u ** 1.5, 0, 0.35)
    add(out, sine(sweep(n, 70, 45, 1.0), n) * u ** 2.5, 0, 0.5)
    return hp(out, 30, 2)


@sound('mv_field_bloom', 'construct', 3, pitch=P_T10, lufs=-22, max_dur=1.0,
       notes='a field / room opens: a bowed hard-light chord blooming outward with a soft whoosh')
def mv_field_bloom(r, v):
    out = buf(1.0)
    add(out, hl_chord(r, [['E4', 'B4', 'E5'], ['D4', 'A4', 'D5'], ['G4', 'D5', 'G5']][v], 0.5, 0.8, 0.02), 0.03, 0.6)
    add(out, whoosh(0.7, r, 300, 2600, 1.0, [(0, 0), (0.1, 1), (0.7, 0)]), 0, 0.4)
    add(out, thump(0.4, 90, 55, 0.05, 0.12, drive=1.2), 0, 0.3)
    return carve_ui(out, 120, 14000)


@sound('mv_field_cross', 'construct', 4, pitch=P_T10, lufs=-26, max_dur=0.35,
       notes='someone crosses a field boundary: a glassy bloop through a membrane')
def mv_field_cross(r, v):
    out = buf(0.35)
    n = ns(0.25)
    f = sweep(n, note(['E6', 'B5', 'G6', 'D6'][v]), note(['E5', 'B4', 'G5', 'D5'][v]), 0.6)
    add(out, fm(f, 1.5, 1.2 * env_exp(n, 0.05), n) * env_exp(n, 0.07, 0.002), 0, 0.5)
    add(out, hl_ping(r, note(['B6', 'E7', 'D7', 'A6'][v]), 0.06, 1.1, glint=0.5), 0.01, 0.3)
    return carve_ui(out, 400, 13000)


@sound('mv_swap_clap', 'combat', 4, pitch=P_MICRO, lufs=-18, max_dur=0.5,
       notes='swap: two linked claps (both ends at once) with a shared ringing tail')
def mv_swap_clap(r, v):
    out = buf(0.5)
    for k, t in enumerate((0.0, [0.055, 0.045, 0.06, 0.05][v])):
        add(out, burst(0.08, r, 900, 7000, 0.012), t, 0.9 - 0.2 * k)
        add(out, click(r, 3200 + 800 * k, 0.003), t, 0.6)
    add(out, hl_ping(r, note(['E6', 'G6', 'B6', 'D6'][v]), 0.12, 1.2, glint=0.7), 0.05, 0.25)
    return carve_foley(out, 300, 13000)


@sound('mv_time_tick', 'combat', 2, pitch=P_TONAL, lufs=-18, max_dur=1.6,
       notes='time stops: a huge clock tick, then the world pulled out of phase (reverse swell + low drone dip)')
def mv_time_tick(r, v):
    d = 1.5
    out = buf(d)
    add(out, material_hit(r, 'metal', 0.5, 1.0), 0, 0.6)
    add(out, click(r, 2200, 0.006), 0, 0.9)
    add(out, hl_ping(r, note(['E3', 'D3'][v]), 0.6, 0.6, glint=0.3), 0, 0.6)
    n = ns(0.9)
    u = np.linspace(0, 1, n)
    rev = normpk(tvf(pink(n, r), sweep(n, 4000, 300, 0.6), 2.0, 'bp')) * (1 - u) ** 2
    add(out, rev, 0.05, 0.35)
    add(out, sine(sweep(n, 110, 55, 1.0), n) * np.sin(np.pi * u) ** 2, 0.1, 0.5)
    add(out, click(r, 2600, 0.005), 0.75, 0.4)
    return hp(out, 30, 2)


# =============================================================================== bodies
@sound('mv_hardlight', 'construct', 4, pitch=P_T10, lufs=-22, max_dur=0.4,
       notes='hard-light body: a crisp glassy strike + quick shard run')
def mv_hardlight(r, v):
    out = buf(0.4)
    add(out, hl_ping(r, note(['E6', 'B5', 'G6', 'D6'][v]), 0.1, 1.3, glint=1.0), 0, 0.6)
    add(out, hl_shards(r, 2, 0.12, amp=0.4), 0.01, 0.5)
    add(out, click(r, 6500, 0.0015), 0, 0.3)
    return carve_ui(out, 800)


@sound('mv_rubber_stretch', 'movement', 4, pitch=P_MICRO, lufs=-20, max_dur=0.6,
       notes='rubber stretching: a creaky rising elastic groan (rubbed band + rising pitch)')
def mv_rubber_stretch(r, v):
    d = 0.45
    n = ns(d)
    u = np.linspace(0, 1, n)
    out = buf(d)
    f0 = [90, 110, 80, 100][v]
    f = sweep(n, f0, f0 * 3.2, 0.7)
    # rubbing: a saw through a wobbling band + stick-slip pulses
    creak = saw(f, n) * (1 + 0.6 * np.sign(np.sin(TAU * np.cumsum(38 + 25 * u) / SR)))
    creak = tvf(creak, 600 + 1600 * u, 3.0, 'bp')
    add(out, normpk(creak) * env_pts(n, [(0, 0), (0.03, 1), (d * 0.8, 0.8), (d, 0)]), 0, 0.8)
    add(out, swish(r, d, 400, 1800, 2.0), 0, 0.25)
    return carve_foley(out, 120, 9000)


@sound('mv_rubber_snap', 'movement', 4, pitch=P_MICRO, lufs=-17, max_dur=0.7,
       notes='rubber snap-back: band SNAP + a wobbling boing (pitch-bent twang with vibrato decay)')
def mv_rubber_snap(r, v):
    out = buf(0.7)
    add(out, crack(r, 2500, 0.003), 0, 0.9)
    add(out, burst(0.04, r, 400, 5000, 0.006), 0, 0.5)
    n = ns(0.6)
    t = tvec(n)
    f0 = [150, 180, 130, 165][v]
    f = f0 * (1.9 * np.exp(-t / 0.035) + 1) * (1 + 0.08 * np.sin(TAU * 11 * t) * np.exp(-t / 0.25))
    boing = sine(f, n) + 0.35 * sine(2 * f, n)
    add(out, boing * env_exp(n, 0.2, 0.002), 0.004, 0.8)
    add(out, thump(0.2, 220, 90, 0.02, 0.05, drive=1.3), 0, 0.4)
    return carve_foley(out, 90, 9000)


@sound('mv_rigid_extend', 'combat', 4, pitch=P_MICRO, lufs=-19, max_dur=0.6,
       notes='telescoping pole: a fast run of metal clacks (segments locking) + a whoosh')
def mv_rigid_extend(r, v):
    out = buf(0.6)
    t = 0.0
    for k in range(4):
        add(out, material_hit(r, 'metal', 0.35 + 0.08 * k, 0.9), t, 0.55 + 0.1 * k)
        add(out, click(r, 4200 - 400 * k, 0.002), t, 0.5)
        t += [0.028, 0.024, 0.032, 0.026][v] * (1 - 0.1 * k)
    add(out, whoosh(0.35, r, 800, 3000, 1.2, [(0, 0), (0.05, 1), (0.35, 0)]), 0, 0.35)
    add(out, string_twang(r, note(['E5', 'G5', 'D5', 'B4'][v]), 0.25, 6, 0.004), t, 0.25)
    return carve_foley(out, 200, 13000)


@sound('mv_rigid_ring', 'combat', 3, pitch=P_T10, lufs=-18, max_dur=1.4,
       notes='rigid strike: a struck iron bar ringing like a temple bell (inharmonic modes, long ring)')
def mv_rigid_ring(r, v):
    out = buf(1.4)
    f = note(['E4', 'G4', 'D4'][v])
    n = ns(1.3)
    ratios = [1.0, 2.76, 5.40, 8.93, 13.34]
    decs = [0.9, 0.5, 0.3, 0.16, 0.08]
    add(out, modal([f * k for k in ratios], np.array(decs), [1.0, 0.6, 0.4, 0.25, 0.12], n, r), 0, 0.8)
    add(out, material_hit(r, 'metal', 0.7, 1.0), 0, 0.6)
    add(out, click(r, 3800, 0.003), 0, 0.5)
    return hp(out, 60, 2)


@sound('mv_rigid_collapse', 'combat', 3, pitch=P_MICRO, lufs=-22, max_dur=0.5,
       notes='pole collapsing back: reverse clacks, a tink')
def mv_rigid_collapse(r, v):
    out = buf(0.5)
    t = 0.0
    for k in range(4):
        add(out, material_hit(r, 'metal', 0.55 - 0.06 * k, 0.6), t, 0.4)
        t += 0.022
    add(out, tink(r, note(['E7', 'B6', 'G7'][v]), 1.2), t, 0.4)
    return carve_foley(out, 300, 13000)


@sound('mv_bamf', 'combat', 4, pitch=P_MICRO, lufs=-17, max_dur=0.8,
       notes='BAMF: a sharp air crack + a sulphurous puff (low whump, hissing smoke tail)')
def mv_bamf(r, v):
    out = buf(0.8)
    add(out, crack(r, 3000, 0.002), 0, 1.0)
    add(out, thump(0.4, [200, 180, 220, 190][v], 60, 0.03, 0.08, drive=2.0), 0, 0.9)
    add(out, burst(0.6, r, 1500, 9000, 0.18, color='white') * 0.5, 0.01, 0.35)  # hiss
    add(out, whoosh(0.5, r, 900, 200, 0.8, [(0, 1), (0.05, 0.5), (0.5, 0)]), 0.005, 0.4)
    return carve_heavy(out)


@sound('mv_speed_zip', 'movement', 4, pitch=P_MICRO, lufs=-18, max_dur=0.6,
       notes='speedforce: a doppler zip with electric crackle riding it')
def mv_speed_zip(r, v):
    d = 0.5
    n = ns(d)
    out = buf(d)
    f = sweep(n, [2400, 2000, 2800, 2200][v], 500, 0.5)
    add(out, normpk(tvf(white(n, r), f, 6.0, 'bp')) * env_pts(n, [(0, 0), (0.02, 1), (d, 0)]), 0, 0.7)
    add(out, saw(f * 0.5, n) * env_exp(n, 0.08) * 0.3, 0, 0.35)
    add(out, grit(r, 40, 0.35, 3000, 9000, 0.002, 0.8), 0.01, 0.45)
    return carve_foley(out, 250, 14000)


@sound('mv_bolt_crack', 'combat', 4, pitch=P_MICRO, lufs=-16, max_dur=0.9,
       notes='lightning body: a close thunder crack (sharp tear + rumble tail)')
def mv_bolt_crack(r, v):
    out = buf(0.9)
    add(out, crack(r, 1800, 0.004), 0, 1.0)
    add(out, grit(r, 30, 0.08, 2000, 8000, 0.002, 1.0), 0, 0.6)
    add(out, burst(0.8, r, 40, 600, 0.25, color='brown'), 0.02, 0.6)
    return carve_heavy(out)


@sound('mv_gust', 'movement', 4, pitch=P_MICRO, lufs=-21, max_dur=0.7,
       notes='wind body: a swirling gust (two crossing band sweeps + flutter)')
def mv_gust(r, v):
    out = buf(0.7)
    add(out, whoosh(0.6, r, 500, 2400, 1.6, [(0, 0), (0.15, 1), (0.6, 0)]), 0, 0.6)
    add(out, whoosh(0.5, r, 2800, 700, 2.2, [(0, 0), (0.1, 1), (0.5, 0)]), 0.05, 0.45)
    return carve_foley(out, 200, 10000)


@sound('mv_shadow', 'combat', 4, pitch=P_MICRO, lufs=-20, max_dur=0.7,
       notes='shadow body: a dark breathy swoosh with a low whisper-like formant')
def mv_shadow(r, v):
    d = 0.6
    n = ns(d)
    out = buf(d)
    br = normpk(tvf(pink(n, r), sweep(n, 400, 1100, 1.0), 4.0, 'bp')) * env_pts(n, [(0, 0), (0.12, 1), (d, 0)])
    add(out, br, 0, 0.7)
    add(out, sine(sweep(n, [70, 60, 80, 65][v], 45, 1.0), n) * env_exp(n, 0.25, 0.03), 0, 0.5)
    return hp(out, 40, 2)


@sound('mv_splash', 'combat', 4, pitch=P_MICRO, lufs=-19, max_dur=0.7,
       notes='liquid body: a splash (burst + many droplet plinks)')
def mv_splash(r, v):
    out = buf(0.7)
    add(out, burst(0.3, r, 300, 5000, 0.06, color='pink'), 0, 0.7)
    for _ in range(14):
        t = U(r, 0.02, 0.45)
        n = ns(0.04)
        f = sweep(n, U(r, 900, 2200), U(r, 1800, 4000), 0.5)
        add(out, sine(f, n) * env_exp(n, 0.012), t, U(r, 0.08, 0.25))
    add(out, thump(0.2, 120, 60, 0.02, 0.05), 0, 0.4)
    return carve_foley(out, 120, 11000)


@sound('mv_void_thump', 'combat', 3, pitch=P_SOFT, lufs=-16, max_dur=1.0,
       notes='void body: an inverted boom — a suck-in then a deep hollow thump')
def mv_void_thump(r, v):
    out = buf(1.0)
    n = ns(0.18)
    u = np.linspace(0, 1, n)
    add(out, normpk(tvf(pink(n, r), sweep(n, 3000, 200, 1.0), 2.0, 'bp')) * u ** 2, 0, 0.5)
    add(out, thump(0.8, [70, 60, 80][v], 28, 0.1, 0.25, drive=2.4), 0.18, 1.0)
    add(out, hl_ping(r, note('E2'), 0.4, 0.4, glint=0.0), 0.18, 0.3)
    return hp(out, 22, 2)


@sound('mv_cursed', 'combat', 4, pitch=P_MICRO, lufs=-19, max_dur=0.8,
       notes='cursed energy: a distorted dark flare (saturated low growl + crackle)')
def mv_cursed(r, v):
    d = 0.7
    n = ns(d)
    out = buf(d)
    f = sweep(n, [95, 85, 110, 100][v], 60, 1.0)
    add(out, sat(saw(f, n) + saw(f * 1.01, n), 4.0) * env_exp(n, 0.22, 0.01), 0, 0.5)
    add(out, grit(r, 25, 0.4, 1500, 6000, 0.003, 0.7), 0, 0.4)
    add(out, burst(0.4, r, 200, 2500, 0.1, color='pink'), 0, 0.4)
    return carve_heavy(out)


@sound('mv_holy', 'construct', 3, pitch=P_T10, lufs=-20, max_dur=1.3,
       notes='holy body: a bright choir-like chord bloom with bell glints')
def mv_holy(r, v):
    out = buf(1.3)
    add(out, hl_chord(r, [['E5', 'G#5', 'B5', 'E6'], ['A4', 'C#5', 'E5', 'A5'], ['B4', 'D#5', 'F#5', 'B5']][v], 0.7, 1.0, 0.03), 0, 0.6)
    add(out, hl_shards(r, 3, 0.4, amp=0.3), 0.05, 0.4)
    return carve_ui(out, 200, 15000)


@sound('mv_chi', 'combat', 4, pitch=P_MICRO, lufs=-19, max_dur=0.6,
       notes='chi / ki body: a resonant thrum pulse (pulsed low tone + air)')
def mv_chi(r, v):
    d = 0.5
    n = ns(d)
    t = tvec(n)
    out = buf(d)
    f = [110, 98, 123, 104][v]
    thr = sine(f, n) * (0.6 + 0.4 * np.sin(TAU * 28 * t)) * env_exp(n, 0.2, 0.005)
    add(out, thr, 0, 0.7)
    add(out, whoosh(0.4, r, 600, 1800, 1.4, [(0, 0), (0.04, 1), (0.4, 0)]), 0, 0.4)
    add(out, thump(0.2, 180, 80, 0.02, 0.05), 0, 0.4)
    return hp(out, 50, 2)


@sound('mv_fire_whoomp', 'combat', 4, pitch=P_MICRO, lufs=-17, max_dur=0.9,
       notes='fire body: a WHOOMP ignition (low roar swell + crackle)')
def mv_fire_whoomp(r, v):
    out = buf(0.9)
    n = ns(0.7)
    roar = normpk(tvf(brown(n, r) + 0.3 * pink(n, r), sweep(n, 250, 900, 0.6), 1.2, 'bp'))
    add(out, roar * env_pts(n, [(0, 0), (0.05, 1), (0.7, 0)]), 0, 0.8)
    add(out, grit(r, 30, 0.6, 1500, 5000, 0.003, 0.6), 0.03, 0.4)
    add(out, thump(0.3, 110, 50, 0.04, 0.08, drive=1.8), 0, 0.5)
    return carve_heavy(out)


@sound('mv_ice', 'combat', 4, pitch=P_MICRO, lufs=-19, max_dur=0.7,
       notes='ice body: a crystalline crack-freeze (glassy creak + bright shard tinkle)')
def mv_ice(r, v):
    out = buf(0.7)
    add(out, material_hit(r, 'glass', 0.8, 0.9), 0, 0.6)
    add(out, hl_shards(r, 4, 0.3, top='E8', low='B6', amp=0.5), 0.01, 0.5)
    add(out, grit(r, 20, 0.2, 3000, 9000, 0.002, 0.6), 0, 0.4)
    return carve_ui(out, 400, 15000)


@sound('mv_crystal', 'construct', 3, pitch=P_T10, lufs=-20, max_dur=0.9,
       notes='crystal body: a ringing gem chord (pure pings) + a glassy chime run')
def mv_crystal(r, v):
    out = buf(0.9)
    add(out, hl_chord(r, [['E6', 'B6', 'G#7'], ['D6', 'A6', 'F#7'], ['G6', 'D7', 'B7']][v], 0.35, 1.3, 0.02), 0, 0.6)
    add(out, hl_shards(r, 2, 0.25, amp=0.35), 0.03, 0.4)
    return carve_ui(out, 600, 16000)


@sound('mv_steam', 'movement', 4, pitch=P_MICRO, lufs=-21, max_dur=0.8,
       notes='steam body: a pressurised hiss burst')
def mv_steam(r, v):
    out = buf(0.8)
    add(out, burst(0.7, r, 2500, 11000, 0.09, color='white') * env_pts(ns(0.7), [(0, 0), (0.02, 1), (0.7, 0)]), 0, 0.7)
    add(out, whoosh(0.5, r, 1200, 4000, 1.0), 0, 0.3)
    return carve_foley(out, 600, 14000)


@sound('mv_sand', 'movement', 4, pitch=P_MICRO, lufs=-21, max_dur=0.8,
       notes='sand body: a hissing grain pour (dense grit + low rush)')
def mv_sand(r, v):
    out = buf(0.8)
    add(out, grit(r, 220, 0.6, 1500, 7000, 0.0015, 1.0), 0, 0.7)
    add(out, whoosh(0.6, r, 300, 1200, 0.8), 0, 0.4)
    return carve_foley(out, 200, 10000)


@sound('mv_glitch', 'feedback', 4, pitch=P_MICRO, lufs=-21, max_dur=0.5,
       notes='glitch body: bit-crushed stutter chirps')
def mv_glitch(r, v):
    out = buf(0.5)
    t = 0.0
    for k in range([6, 8, 5, 7][v]):
        n = ns(U(r, 0.015, 0.04))
        f = U(r, 300, 2400)
        x = square(f, n) * env_exp(n, 0.02)
        x = np.round(x * 4) / 4  # crush
        add(out, x, t, U(r, 0.25, 0.5))
        t += U(r, 0.02, 0.06)
    return carve_ui(out, 250, 12000)


@sound('mv_paper', 'movement', 4, pitch=P_MICRO, lufs=-23, max_dur=0.6,
       notes='paper body: a flurry of flutters and a crisp snap')
def mv_paper(r, v):
    out = buf(0.6)
    for _ in range(10):
        add(out, swish(r, 0.05, 2500, 5500, 3.0, 220, [(0, 0), (0.01, 1), (0.05, 0)]), U(r, 0, 0.45), U(r, 0.2, 0.5))
    add(out, crack(r, 4000, 0.0015), 0, 0.5)
    return carve_foley(out, 500, 13000)


# =============================================================================== PRESENTATION POLISH
@sound('mv_time_stop', 'combat', 2, pitch=P_TONAL, lufs=-16, max_dur=2.4,
       notes='ZA WARUDO: the world sucked into a point (reverse swell), a huge low boom as the bubble '
             'bursts out, a clock-bell strike, then a drained ringing drone that sinks')
def mv_time_stop(r, v):
    d = 2.3
    out = buf(d)
    # 1. the suck-in (0 .. 0.35 s): band noise swelling up to a pinch, a rising sine.
    n = ns(0.35)
    u = np.linspace(0, 1, n)
    air = normpk(tvf(pink(n, r), sweep(n, 300, [3600, 3000][v], 2.2), 2.4, 'bp')) * u ** 2.4
    add(out, air, 0, 0.6)
    add(out, sine(sweep(n, 90, 420, 2.0), n) * u ** 3, 0, 0.35)
    # 2. the burst (0.35 s): a saturated sub boom + a pressure whoosh out.
    add(out, thump(0.9, [70, 62][v], 28, 0.08, 0.35, drive=2.0), 0.35, 1.0)
    add(out, whoosh(0.7, r, 2400, 300, 1.1, [(0, 1), (0.05, 0.8), (0.7, 0)]), 0.35, 0.4)
    # 3. the clock bell (0.38 s): metal + an E-minor hard-light ping, long.
    add(out, material_hit(r, 'metal', 0.8, 1.0), 0.38, 0.55)
    add(out, hl_ping(r, note(['E3', 'B2'][v]), 1.4, 0.7, glint=0.5), 0.38, 0.6)
    # 4. the drained world: a low drone gliding down, beating slowly.
    m = ns(1.8)
    w = np.linspace(0, 1, m)
    drone = sine(sweep(m, 82.4, 55.0, 1.0), m) + 0.5 * sine(sweep(m, 83.4, 55.6, 1.0), m)
    add(out, drone * np.sin(np.pi * np.minimum(w * 1.6, 1.0)) * (1 - w) ** 1.5, 0.45, 0.35)
    add(out, click(r, 2400, 0.006), 1.3, 0.35)
    return hp(glue(out, 1.3), 28, 2)


@sound('mv_time_resume', 'combat', 2, pitch=P_TONAL, lufs=-18, max_dur=1.2,
       notes='time resumes: a glassy crack, the world rushing back in (a fast forward whoosh + rising '
             'noise), a short low thud as everything moves again')
def mv_time_resume(r, v):
    out = buf(1.1)
    add(out, shatter(r, 'glass', 10, 0.25, 0.5, 0.8), 0, 0.45)
    add(out, crack(r, 3800, 0.002), 0, 0.6)
    add(out, whoosh(0.6, r, 300, [3200, 2800][v], 1.2, [(0, 0), (0.35, 1), (0.6, 0)]), 0.02, 0.55)
    add(out, thump(0.4, 120, 45, 0.04, 0.15, drive=1.6), 0.36, 0.7)
    return hp(out, 35, 2)


@sound('mv_ult_gather', 'combat', 3, pitch=P_TONAL, lufs=-19, max_dur=0.5,
       notes='an ultimate gathers: a fast rising charge whine with sucked-in air, cut dead at the release')
def mv_ult_gather(r, v):
    d = 0.2
    n = ns(d)
    u = np.linspace(0, 1, n)
    out = buf(d + 0.01)
    add(out, sine(sweep(n, [220, 247, 196][v], [1760, 1976, 1568][v], 1.6), n) * u ** 1.5, 0, 0.45)
    air = normpk(tvf(pink(n, r), sweep(n, 600, 5000, 1.8), 2.0, 'bp')) * u ** 2
    add(out, air, 0, 0.5)
    out[n - ns(0.003):n] *= np.linspace(1, 0, ns(0.003))
    return hp(out, 120, 2)
