# Piloting / transformer frame strips

Dev frames from `--practice --scenario piloting --pilot-shot <shot>` (offscreen: `BD_HIDDEN_WINDOW=1`,
`--screenshot x.png --screenshot-at t1,t2,..`). Rules: docs/COMBAT.md §5d-quater.

TRANSFORMERS FEEL (after the feel pass, 3x3 grids read left to right):
- `feel_before_combo.png`: the old Optimus combo strip. The dummy is hidden behind the robot or knocked out of reach after one hit.
- `feel_after_combo.png`: Optimus string (hits stay connected), freight-train ram, axe finisher.
- `feel_after_grimlock.png`: T-rex bites, tail sweep and breath, then the robot bursts out into the axe (frame 2 is the finisher's impact frame).
- `feel_after_devastator.png`: colossus string, quake and smash.
- `feel_after_smallcar.png`: a 2.4 m suit transformer, fought on your own body.
- `feel_after_voice.png`: "optimus prime combo", then "ion blaster", then "transform", all said mid-fight.

TRANSFORMERS V2 (COMBAT.md §5d-quinquies; notes + measurements: `V2_NOTES.md`):
- `v2_before_after.png`: `--pilot-shot combo` from a fixed SIDE camera (`BD_SHOT_CAM=-27,44,-11,-90,-12`). The top rows are before (an 8.7 m Optimus, the 1.8 m dummy lost at its feet) and the bottom rows are after (hero scale 4.6 m: the string connects, the gold flash burst at the ender, the vehicle attack, the finisher). Same frame times.
