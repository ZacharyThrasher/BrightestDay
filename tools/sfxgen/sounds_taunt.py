"""TAUNT & VOICE TELL (bd_game present/taunt.rs): one tiny, wordless, funny sting per body taunt.

Soft and short (feedback bus, ~-28 LUFS): a taunt is a wink, never a callout, and never masks combat.
All in the hard-light E-minor family (E G A B D) so they sit with the rest of the ring's voice:
  taunt_flex    "ba-DING": two plucks up a fourth + a sparkle, a little rubbery boing under it
  taunt_beckon  "c'mere, c'mere": two soft upward whoop chirps
  taunt_shrug   "meh": two notes sagging down + a small brush-off swish
  taunt_bow     "ta-da": a quick rising run into a soft open chord
  taunt_dance   a bouncy four-tick groove over a tiny kick
"""
from __future__ import annotations

from dsp import *  # noqa: F401,F403
from materials import click, hl_chord, hl_ping, note
from sounds import P_T10, sound


def boop(r, f0, f1, d, tau=None, amp=1.0):
    """A soft sine chirp gliding f0 -> f1 (cartoon voice-less 'whoop')."""
    n = ns(d)
    f = sweep(n, f0, f1, 0.7)
    x = sine(f, n) + 0.18 * sine(f * 2, n)
    e = env_pts(n, [(0, 0), (d * 0.12, 1), (d * 0.55, 0.8), (d, 0)])
    return amp * x * e


def boing(r, f, d=0.22, amp=1.0):
    """A rubbery spring: a sine whose pitch wobbles down and settles."""
    n = ns(d)
    t = tvec(n)
    wob = 1.0 + 0.35 * np.exp(-t / 0.05) * np.sin(TAU * 22 * t)
    x = sine(f * wob, n)
    return amp * x * env_exp(n, d * 0.35, 0.002)


@sound('taunt_flex', 'feedback', 2, pitch=P_T10, lufs=-28, max_dur=0.9,
       notes='TAUNT flex: "ba-DING" - two hard-light plucks up a fourth + sparkle, a little rubbery boing under it')
def taunt_flex(r, v):
    out = buf(0.8)
    a, b = [('B5', 'E6'), ('E5', 'A5')][v]
    add(out, hl_ping(r, note(a), 0.07, 1.0, glint=0.6), 0.0, 0.7)
    add(out, hl_ping(r, note(b), 0.18, 1.1, glint=1.0), 0.13, 1.0)
    add(out, hl_ping(r, note('E7'), 0.1, 1.2, glint=0.8, shimmer=0.7), 0.15, 0.35)
    add(out, boing(r, [190, 220][v], 0.24), 0.12, 0.45)
    add(out, click(r, 5200, 0.0008), 0.13, 0.12)
    return carve_ui(out, 260)


@sound('taunt_beckon', 'feedback', 2, pitch=P_T10, lufs=-29, max_dur=0.8,
       notes='TAUNT beckon: "c\'mere, c\'mere" - two soft upward whoop chirps (wordless)')
def taunt_beckon(r, v):
    out = buf(0.7)
    lo, hi = [(note('B4'), note('E5')), (note('A4'), note('D5'))][v]
    add(out, boop(r, lo, hi, 0.13), 0.0, 0.8)
    add(out, boop(r, lo * 1.06, hi * 1.12, 0.16), 0.26, 0.95)
    add(out, hl_ping(r, hi * 2, 0.05, 1.0, glint=0.4), 0.3, 0.2)
    return carve_ui(out, 300)


@sound('taunt_shrug', 'feedback', 2, pitch=P_T10, lufs=-29, max_dur=0.9,
       notes='TAUNT shrug: "meh" - two notes sagging down, then a small brush-off swish')
def taunt_shrug(r, v):
    out = buf(0.85)
    a, b = [('G5', 'E5'), ('B5', 'G5')][v]
    add(out, boop(r, note(a), note(a) * 0.97, 0.14), 0.0, 0.8)
    add(out, boop(r, note(b), note(b) * 0.9, 0.24), 0.17, 0.85)
    n = ns(0.12)
    sw = bp(pink(n, r), 1800, 5200) * env_pts(n, [(0, 0), (0.02, 1), (0.12, 0)])
    add(out, sw, 0.55, 0.25)
    add(out, click(r, 3800, 0.001), 0.56, 0.08)
    return carve_ui(out, 280)


@sound('taunt_bow', 'feedback', 2, pitch=P_T10, lufs=-28, max_dur=1.2,
       notes='TAUNT bow: "ta-da" - a quick rising E-minor run into a soft open chord')
def taunt_bow(r, v):
    out = buf(1.1)
    run = [['E5', 'G5', 'B5'], ['B4', 'E5', 'G5']][v]
    for i, nm in enumerate(run):
        add(out, hl_ping(r, note(nm), 0.06, 1.0, glint=0.5), i * 0.065, 0.55)
    add(out, hl_chord(r, [['E6', 'B6'], ['B5', 'E6']][v], dec=0.28, bright=1.0, stagger=0.01, glint=0.8), 0.22, 0.9)
    return carve_ui(out, 260)


@sound('taunt_dance', 'feedback', 2, pitch=P_T10, lufs=-29, max_dur=0.9,
       notes='TAUNT dance: a bouncy four-tick groove (132 bpm) over a tiny kick')
def taunt_dance(r, v):
    out = buf(0.8)
    beat = 60.0 / 132.0 / 2.0
    notes = [['E5', 'B5', 'E5', 'G5'], ['E5', 'D5', 'E5', 'B5']][v]
    for i, nm in enumerate(notes):
        add(out, hl_ping(r, note(nm), 0.045, 1.0, glint=0.5, shimmer=0.3), i * beat, 0.6 if i % 2 else 0.8)
    for i in (0, 2):
        k = thump(0.09, 160, 70, 0.02, 0.05, drive=1.2)
        add(out, k, i * beat, 0.35)
    return carve_ui(out, 200)
