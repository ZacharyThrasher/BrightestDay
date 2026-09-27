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
- `v2_before_after.png`: `--pilot-shot combo` from a fixed SIDE camera (`BD_SHOT_CAM=-27,44,-11,-90,-12`). The top rows are before (an 8.7 m Optimus, the 1.8 m dummy lost at its feet) and the bottom rows are after (hero scale, a 4.8 m robot: the string connects, then the ender, the vehicle attack taken in the flash window, the finisher). Same frame times.

TRANSFORMERS V3 SAUCE (`present/transformer_feel/sauce.rs`; sounds `tools/sfxgen/sounds_tf_sauce.py`):
- `v3_sauce_before_after.png`: `BD_SAUCE_STRIP=<dir> --practice --scenario piloting --pilot-shot sauce_truck|sauce_jet|sauce_tank|sauce_dino`
  (a fixed side camera + frames at the script's beats: light hit, heavy coil, heavy release, blaster, string ender, vehicle attack x2,
  finisher x2). BEFORE rows = the same exe with `BD_TF_SAUCE=0`. Tiles are cropped around the robot; the 1.8 m dummy is the human-sized target.
- The move grammar per archetype weapon (Blade truck/train/boat, Blaster car, Cannon jet, ChinGun heli, Shell tank/colossus, Grinder
  construction, Spin bike, Jaws dino, Claw beast): gather (heavy-hold servo coil + motes/energy lines + `tfs_charge_hum`; muzzle glow while
  the blaster recharges; chest gather on the finisher), release (smear arcs in the robot colour, lunge/twist/stretch pose held through the
  hit-stop then a damped overshoot, re-chamber between string hits, air tear on heavies; muzzle flash + tracer + casings/smoke ring + recoil
  rock on shots, vehicle weapons included; turret swivel cue), world reaction (bouncing metal sparks, shards, shockwave rings, ground cracks +
  dust walls, skid scars/tyre burns, rotor disc, grinder spray, stampede dust), tiered ASYMMETRIC hit-stop (light 3-6 frames, heavy 10-18,
  vehicle attack 15-22, finisher 20-25; attacker x0.75), impact frames + camera kick on heavy+, finisher crunch + bass drop (`tfs_finisher_drop`).
