"""KO streak: 5 ascending steps per corps (E minor pentatonic: E5 G5 A5 B5, then D6 -> E6 resolve),
the same melody for every corps, a distinct timbre per corps, plus a punchy transient on every step.
Files: ko_streak_<corps>_<step>.ogg (sequence, not random variations)."""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from sounds import U, sound

NOTE = {'E5': 659.26, 'G5': 783.99, 'A5': 880.00, 'B5': 987.77, 'D6': 1174.66, 'E6': 1318.51}
STEPS = ['E5', 'G5', 'A5', 'B5', 'E6']  # step 5 = D6 grace -> E6 resolve
CORPS = ['willpower', 'rage', 'greed', 'fear', 'hope', 'compassion', 'love', 'life']
CORPS_TIMBRE = {
    'willpower': 'clean bright bell-synth', 'rage': 'distorted growling saw', 'greed': 'metallic coin chimes',
    'fear': 'eerie detuned glass', 'hope': 'airy choir pad', 'compassion': 'soft mallet chime',
    'love': 'warm FM harp', 'life': 'pure sine choir',
}


def voice(corps: str, f: float, d: float, r) -> np.ndarray:
    """One note of the corps' instrument. Fundamental f is always the strongest partial."""
    n = ns(d)
    t = tvec(n)
    if corps == 'willpower':
        x = (fm_bell(n, f, 1.0, 2.0, 0.08, 0.35) + 0.6 * sine(f, n) * env_exp(n, 0.5)
             + 0.25 * fm_bell(n, 2 * f, 1.0, 1.0, 0.05, 0.2) + 0.12 * sine(3 * f, n) * env_exp(n, 0.1))
    elif corps == 'rage':
        growl = 1 + 0.45 * np.sin(TAU * 31 * t)
        src = supersaw(f, n, r, 5, 22) + 0.35 * supersaw(f / 2, n, r, 3, 15)
        x = lp(sat(src * growl * 2.0, 4.0), 4200) * env_exp(n, 0.3, 0.004)
        x = x + 0.6 * sine(f, n) * env_exp(n, 0.3, 0.004)
    elif corps == 'greed':
        x = modal([f, 2.0 * f * 1.003, 2.76 * f, 4.07 * f, 5.9 * f], [0.6, 0.35, 0.25, 0.15, 0.1],
                  [1, .35, .3, .2, .12], n, r)
        for k, dt in enumerate((0.025, 0.055)):  # coins jingling
            add(x, modal([2.76 * f * U(r, 1.0, 1.02), 5.4 * f], [0.05, 0.03], [1, .5], ns(0.3), r), dt, 0.25 - 0.08 * k)
            add(x, crack(r, 6000, 0.004), dt, 0.08)
    elif corps == 'fear':
        vib = 1 + 0.006 * np.sin(TAU * 4 * t)
        x = (sine(f * 2 ** (-16 / 1200) * vib, n) + sine(f * 2 ** (16 / 1200) * vib, n) + 0.8 * sine(f * vib, n)
             + 0.25 * sine(2.76 * f * 1.01 * vib, n) + 0.12 * sine(f * 1.414, n))
        x *= (0.8 + 0.2 * np.sin(TAU * 7 * t)) * env_exp(n, 0.45, 0.03)
    elif corps == 'hope':
        src = supersaw(f, n, r, 5, 12) + 0.5 * pink(n, r)
        x = reson(src, 800, 4) + 0.8 * reson(src, 1150, 5) + 0.4 * reson(src, 2900, 6)
        x = normpk(x) * 0.6 + 0.8 * sine(f, n)
        x *= env_pts(n, [(0, 0), (0.03, 1), (d, 0)], 1.6)
    elif corps == 'compassion':
        x = modal([f, 3.93 * f, 9.2 * f], [0.4, 0.09, 0.03], [1, .35, .1], n, r)
        x += 0.2 * lp(crack(r, 200, 0.004, d=d)[:n], 2500)
    elif corps == 'love':
        x = (fm(f, 1.0, 3 * np.exp(-t / 0.04) + 0.3, n) * env_exp(n, 0.5, 0.001)
             + 0.4 * fm(f * 1.002, 2.0, 1.5 * np.exp(-t / 0.03), n) * env_exp(n, 0.3, 0.001))
        x = lp(x, 5000)
    elif corps == 'life':
        vib = 1 + 0.005 * np.sin(TAU * 5.5 * t)
        x = (sine(f * vib, n) + 0.6 * sine(f * 1.003 * vib, n) + 0.6 * sine(f * 0.997 * vib, n)
             + 0.2 * sine(2 * f * vib, n) + 0.2 * sine(f / 2, n))
        x *= env_pts(n, [(0, 0), (0.04, 1), (d, 0)], 1.6)
    else:
        raise ValueError(corps)
    rel = ns(min(0.12, d * 0.35))  # note-off release
    x[-rel:] *= 0.5 + 0.5 * np.cos(np.linspace(0, np.pi, rel))
    return x


def _step(corps, r, s):
    """s = 0..4"""
    out = buf(1.5)
    punch = 1.0 + 0.12 * s
    add(out, crack(r, 3000, 0.002), 0, 0.45 * punch)
    add(out, thump(0.2, 160, 75, 0.015, 0.04 + 0.005 * s, drive=2.0), 0, 0.6 * punch)
    if s < 4:
        f = NOTE[STEPS[s]]
        d = 0.36 + 0.04 * s
        add(out, voice(corps, f, d, r), 0.004, 1.0)
        add(out, voice(corps, 2 * f, d * 0.6, r), 0.004, 0.08 * s)       # escalating shimmer
        return reverb(out, r, 0.5, 0.18, bright=1.2)
    # step 5: grace D6 -> resolve E6 with the E5/B5 underneath and a sparkle run
    add(out, voice(corps, NOTE['D6'], 0.1, r), 0.0, 0.55)
    t0 = 0.07
    add(out, sat(crack(r, 2000, 0.003, d=0.04), 2.0), t0, 0.6)
    add(out, thump(0.5, 120, 45, 0.03, 0.12, drive=2.5), t0, 0.9)
    add(out, voice(corps, NOTE['E6'], 1.1, r), t0, 1.0)
    add(out, voice(corps, NOTE['B5'], 0.9, r), t0, 0.3)
    add(out, voice(corps, NOTE['E5'], 0.9, r), t0, 0.35)
    for i, k in enumerate([2.0, 2.378, 2.67, 3.0, 4.0]):  # E6 G6 A6 B6 E7 run
        add(out, voice(corps, NOTE['E5'] * k, 0.14, r), t0 + 0.09 + 0.035 * i, 0.16)
    return reverb(out, r, 1.2, 0.3, bright=1.2)


def _make(corps):
    def fn(r, v):
        return _step(corps, r, v)
    fn.__name__ = f'ko_streak_{corps}'
    sound(f'ko_streak_{corps}', 'ko_streak', 5, volume=0.9, pitch=(1.0, 1.0), lufs=-15.0, lufs_ramp=0.5,
          sequence=True, max_dur=[0.6, 0.6, 0.6, 0.6, 1.4],
          notes=f'{CORPS_TIMBRE[corps]}; steps 1-5 = E5 G5 A5 B5 (D6->)E6')(fn)


for _c in CORPS:
    _make(_c)


@sound('ko_accent_void', 'ko_streak', 2, volume=0.9, pitch=(0.97, 1.03), lufs=-14,
       notes='accent: KO by void / ring-out: whoosh down + thud')
def ko_accent_void(r, v):
    out = buf(1.2)
    d = 0.55
    n = ns(d)
    add(out, whoosh(d, r, 3000, 150, 1.2, [(0, 0), (0.08, 1), (d, 0.3)]), 0, 0.8)
    add(out, lp(sine(glide(n, 900, 55, 0.15), n), 2000) * env_pts(n, [(0, 0), (0.03, 1), (d, 0.2)]), 0, 0.4)
    add(out, thump(0.8, 90, 32, 0.04, 0.15, drive=3.0), d - 0.03, 1.2)
    add(out, lp(crack(r, 800, 0.006, d=0.05), 3000), d - 0.03, 0.6)
    return reverb(out, r, 1.0, 0.25)


@sound('ko_accent_double', 'ko_streak', 2, volume=0.85, pitch=(1.0, 1.0), lufs=-15,
       notes='accent: multi-KO in quick succession: stacked sparkle')
def ko_accent_double(r, v):
    out = buf(0.8)
    add(out, crack(r, 3000, 0.002), 0, 0.5)
    add(out, thump(0.2, 180, 90, 0.015, 0.04, drive=1.5), 0, 0.4)
    for i, f in enumerate([1318.51, 1975.53, 2637.02]):
        add(out, fm_bell(ns(0.6), f, 1.0, 1.5, 0.06, 0.2), 0.055 * i, 0.5)
    for _ in range(22):
        nn = ns(0.01)
        add(out, sine(U(r, 3500, 9500), nn) * np.hanning(nn), U(r, 0.0, 0.35), 0.1)
    return reverb(out, r, 0.7, 0.25, bright=1.5)


@sound('ko_streak_end', 'ko_streak', 2, volume=0.95, pitch=(1.0, 1.0), lufs=-13,
       notes="'shutdown' sting for the player who ends someone else's streak: falling cue")
def ko_streak_end(r, v):
    out = buf(1.6)
    add(out, sat(crack(r, 1800, 0.004, d=0.05), 2.5), 0, 0.9)
    add(out, thump(0.9, 100, 40, 0.04, 0.2, drive=3.0), 0, 1.1)
    for i, nm in enumerate(['B5', 'A5', 'G5', 'E5']):
        add(out, lp(voice('willpower', NOTE[nm], 0.35, r), 5000), 0.02 + 0.07 * i, 0.55)
    n = ns(0.7)
    pd = tvf(supersaw(glide(n, 500, 80, 0.2), n, r, 5, 20), glide(n, 4000, 200, 0.2), 1.5, 'lp')
    add(out, normpk(pd) * env_exp(n, 0.25, 0.005), 0, 0.4)
    add(out, sine(82.41, ns(1.2)) * env_exp(ns(1.2), 0.4, 0.005), 0.3, 0.5)
    add(out, thump(0.5, 110, 45, 0.03, 0.1, drive=2.0), 0.3, 0.6)
    return reverb(out, r, 1.3, 0.3)


def manifest_section() -> list[str]:
    lines = ['    ko_streak: (',
             '        key: "E minor pentatonic",',
             '        steps: ["E5", "G5", "A5", "B5", "D6->E6"],',
             '        corps: {']
    for c in CORPS:
        lines.append(f'            "{c}": "ko_streak_{c}", // {CORPS_TIMBRE[c]}')
    lines += ['        },',
              '        accents: { "void": "ko_accent_void", "double": "ko_accent_double", "shutdown": "ko_streak_end" },',
              '    ),']
    return lines
