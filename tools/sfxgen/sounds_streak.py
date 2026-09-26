"""KO streak: 5 ascending steps per corps (E minor pentatonic: E5 G5 A5 B5, then D6 -> E6 resolve),
the same melody for every corps, a distinct timbre per corps, a bright transient and a shared in-key
"sheen" on every step (feedback sounds own 5-12 kHz). Dry: the mixer supplies the space.
Files: ko_streak_<corps>_<step>.ogg (sequence, not random variations)."""
from __future__ import annotations

import numpy as np

from dsp import *  # noqa: F401,F403
from dsp import SR, TAU
from materials import click, hl_chord, hl_ping, hl_shards, note, pent, pent_index
from sounds import P_TONAL, U, sound

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


def sheen(r, f, d, amt=1.0):
    """Shared bright 'hard light' layer on every streak note: in-key upper harmonics (3, 4, 6, 8, 12)
    decaying with the note, plus an FM glint - this is what lets the streak cut through at a modest
    loudness. Never adds energy in the 450-1500 Hz melody band."""
    n = ns(d)
    t = tvec(n)
    x = np.zeros(n)
    for k, a, dec in ((3, 0.5, 0.5), (4, 0.6, 0.45), (6, 0.45, 0.35), (8, 0.35, 0.28), (12, 0.18, 0.15)):
        if f * k < 15000:
            x += a * np.exp(-t / (dec * d)) * np.sin(TAU * f * k * t + r.uniform(0, TAU))
    m = min(n, ns(0.05))
    x[:m] += 0.4 * fm(f * 8, 1.0, 2.0 * np.exp(-tvec(m) / 0.005), m) * env_exp(m, 0.012, 0.0003)
    return amt * x * np.clip(t / 0.0005, 0, 1)


def _step(corps, r, s):
    """s = 0..4. Dry; bright transient + note + sheen; no sub (the KO itself carries the weight)."""
    body = buf(1.5)
    bright = buf(1.5)
    punch = 1.0 + 0.12 * s
    add(bright, click(r, 4000, 0.002), 0, 0.5 * punch)
    if s < 4:
        f = NOTE[STEPS[s]]
        d = 0.36 + 0.04 * s
        add(body, voice(corps, f, d, r), 0.004, 1.0)
        add(body, voice(corps, 2 * f, d * 0.6, r), 0.004, 0.08 * s)       # escalating shimmer
        add(bright, sheen(r, f, d), 0.004, 1.0)
    else:
        # step 5: grace D6 -> resolve E6 with the E5/B5 underneath and a sparkle run
        add(body, voice(corps, NOTE['D6'], 0.1, r), 0.0, 0.55)
        t0 = 0.07
        add(bright, sat(click(r, 3000, 0.003), 2.0), t0, 0.6)
        add(body, voice(corps, NOTE['E6'], 1.1, r), t0, 1.0)
        add(bright, sheen(r, NOTE['E6'], 1.1), t0, 1.0)
        add(body, voice(corps, NOTE['B5'], 0.9, r), t0, 0.3)
        add(body, voice(corps, NOTE['E5'], 0.9, r), t0, 0.3)
        for i, k in enumerate([2.0, 2.378, 2.67, 3.0, 4.0]):  # E6 G6 A6 B6 E7 run
            add(body, voice(corps, NOTE['E5'] * k, 0.14, r), t0 + 0.09 + 0.035 * i, 0.2)
    # the note keeps its body; the shared sheen is mixed in until >= 42 % of the energy is above 2 kHz
    return hp(balance_hi(body, bright, 0.42), 400, 2)


def _make(corps):
    def fn(r, v):
        return _step(corps, r, v)
    fn.__name__ = f'ko_streak_{corps}'
    sound(f'ko_streak_{corps}', 'ko_streak', 5, pitch=P_TONAL, lufs=-17.0, lufs_ramp=0.5,
          sequence=True, max_dur=[0.6, 0.6, 0.6, 0.6, 1.4],
          notes=f'{CORPS_TIMBRE[corps]}; steps 1-5 = E5 G5 A5 B5 (D6->)E6')(fn)


for _c in CORPS:
    _make(_c)


@sound('ko_accent_void', 'ko_streak', 2, pitch=P_TONAL, lufs=-16,
       notes='accent: KO by void / ring-out: falling hard-light glissando into the dark + bright pop')
def ko_accent_void(r, v):
    out = buf(1.0)
    k0 = pent_index('E8')
    for j in range(10):  # a fast falling pentatonic cascade: the player dropping away
        add(out, hl_ping(r, pent(k0 - j - v), 0.06, 1.1, glint=0.5), 0.04 * j, 0.5 * (1 - 0.05 * j))
    d = 0.5
    n = ns(d)
    f = sweep(n, note('E7'), note('E5'), 0.7)
    add(out, (sine(f, n) + 0.4 * sine(2 * f, n) + 0.25 * sine(3 * f, n)) * env_pts(n, [(0, 0), (0.03, 1), (d, 0)], 1.5), 0, 0.3)
    add(out, sat(click(r, 3500, 0.003), 2.0), d - 0.02, 0.7)
    add(out, hl_chord(r, ['E6', 'B6'], dec=0.15, bright=1.2), d - 0.02, 0.45)
    return carve_ui(out, 1500)


@sound('ko_accent_double', 'ko_streak', 2, pitch=P_TONAL, lufs=-16,
       notes='accent: multi-KO in quick succession: stacked sparkle')
def ko_accent_double(r, v):
    out = buf(0.8)
    add(out, click(r, 4000, 0.002), 0, 0.5)
    for i, f in enumerate([1318.51, 1975.53, 2637.02]):
        add(out, fm_bell(ns(0.6), f, 1.0, 1.5, 0.06, 0.2), 0.055 * i, 0.5)
        add(out, sheen(r, f, 0.4, 0.8), 0.055 * i, 1.0)
    add(out, hl_shards(r, 4, 0.3, 'E8', 'E7', amp=0.3), 0.1, 1.0)
    return carve_ui(out, 1500)


@sound('ko_streak_end', 'ko_streak', 2, pitch=P_TONAL, lufs=-15,
       notes="'shutdown' sting for the player who ends someone else's streak: falling B6 A6 G6 E6 power-down")
def ko_streak_end(r, v):
    out = buf(1.4)
    add(out, sat(click(r, 3000, 0.004), 2.5), 0, 0.8)
    for i, nm in enumerate(['B5', 'A5', 'G5', 'E5']):
        f = NOTE[nm] * 2
        add(out, voice('willpower', f, 0.35, r), 0.02 + 0.07 * i, 0.5)
        add(out, sheen(r, f, 0.3, 0.9), 0.02 + 0.07 * i, 1.0)
    n = ns(0.7)
    f = sweep(n, note('E7'), note('E6'), 0.6)  # power-down glide, an octave, lands in key
    add(out, (sine(f, n) + 0.4 * sine(2 * f, n)) * env_exp(n, 0.25, 0.005), 0.3, 0.3)
    return carve_ui(out, 1200)


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
