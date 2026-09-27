"""Power HUD (bd_game power_hud): the soft "ready" chime of a held power coming back off cooldown.

One struck hard-light note on E5 (the key's root). The game plays it pitched to an E-minor
pentatonic step per power (E G A B D E'), so a player's powers ring as a little chord and are
learned by ear. Soft and short: it must never mask combat.
"""
from __future__ import annotations

from dsp import *  # noqa: F401,F403
from materials import click, hl_ping, note
from sounds import P_TONAL, sound


@sound('power_ready', 'ui', 1, pitch=P_TONAL, lufs=-27, max_dur=0.9,
       notes='a held power is ready again: soft E5 hard-light bell + faint octave shimmer (pitched per power in game)')
def power_ready(r, v):
    out = buf(0.85)
    add(out, hl_ping(r, note('E5'), 0.16, 0.85, glint=0.35, shimmer=0.5), 0, 1.0)
    add(out, hl_ping(r, note('E6'), 0.09, 1.0, glint=0.2, shimmer=0.6), 0.012, 0.28)
    add(out, click(r, 4200, 0.0006), 0, 0.12)
    return carve_ui(out, 420)
