# Transformers V2: notes (stages 1+2 done; 3–5 queued)

Rules: COMBAT.md §5d-quinquies. Code: `bd_sim/src/constructs/brawl.rs` (search for `V2`), the hero cap in
`world/actions.rs::spawn_cast`, guard break in `world/combat.rs::apply_hit` and `powers/ops/counter.rs`,
the cue in `bd_game/src/present/transformer_feel/flash.rs`, the camera in `present/camera3p.rs`.
Tests: `bd_sim/tests/transformer_v2.rs`.

## Measured (before → after)

`cargo test -p bd_sim --test transformer_v2 v2_measure -- --ignored --nocapture`: a scripted 20 s duel
against a still 1.8 m dummy (put back in front when knocked > 14 m). The *timed* player presses LMB when
the robot is ready and Transform 0.1 s after the string's ender. The *masher* presses LMB at 7.5 Hz and
Transform every 1.2 s.

| pilot | before: presses / hit | after: presses / hit | before: vehicle attacks landed | after: vehicle attacks landed | after: flash windows taken |
|---|---|---|---|---|---|
| Optimus, timed | 1.13 (43 / 38) | **0.86** (43 / 50) | 8 / 8 | 8 / 8 | 8 / 8 (100 %) |
| Optimus, masher | 5.57 (167 / 30) | 20.9 (167 / 8) | 6 / 6 | 1 / 1 | 1 / 1 |
| car, timed | 1.50 (60 / 40) | **0.86** (60 / 70) | 8 / 10 | 8 / 10 | 10 / 10 (100 %) |
| car, masher | 3.34 (167 / 50) | 10.4 (167 / 16) | 15 / 17 | 1 / 1 | 1 / 1 |

In the before build, any string hit opened a 0.7 s window, so a masher's random Transform presses became
vehicle attacks 35 % of the time. Now a random Transform press is a full plain transform (8 of them in
20 s), which takes the masher out of the fight. Intentional play lands more per press: 24 % more for
Optimus, 43 % more for the car. The gap between the timed player and the masher grew from ~5× to ~24×.
The robot also stands at 4.6 m instead of 6 m generic / 8.7 m for the real Optimus icon.

## For stage 3 (input buffer / cancels / dodge offset)

- **Stage 3's biggest win is a buffer on the flash.** A Transform press that comes early, just before
  the ender connects, is a plain transform right now. `transform_req` is used up on the tick it's
  pressed (`pilot.rs`: xattack start, or else `toggle`). Hold it for ~0.1 s while a string ender's
  swing is live (`brawl.cd > 0 && step == string_hits - 1`), then let `wants_xattack` take it.
- **Presses during the recovery are DROPPED** (`brawl::fight`: `press1` with `cd > 0` does nothing).
  LMB taps at exactly the string interval (0.3 s) lose every other hit to float timing. The piloting
  strip's script had to move to 0.33 s spacing. A ~0.12 s LMB buffer is the cheapest feel win left.
- The window opens on the *connect*. A whiffed ender never opens it (`open_flash` needs `n > 0`).
- Branch input is read during the burst by `brawl::pick_branch`: an LMB tap = rush, and an LMB held
  for `branch_hold` = the launch. A tap resolves on release, so the rush is 1–3 ticks late. Before V2,
  LMB during the burst was ignored.
- The branch window runs from the first connect until `branch_secs` runs out **or the burst ends**,
  whichever is first. If the ram connects late, the window is shorter. Past the burst, the default
  finisher runs (the Finish phase doesn't read branch input). An earlier attempt kept the burst going
  while the window was open. The victim drifted out of the finisher's reach, so that was reverted.
- The drift branch leaves `open = combo_window`, `cd = 0`, `step = 0`: the next LMB starts a new string
  right away. A cancel system can use the same state.

## For stage 4 (tiered asymmetric hitstop + crunch audio)

- Vehicle-attack hits are `HeavyMelee` with impact ≥ 2 and `HitInfo::guard_break = true`. They always
  fire the automatic HeavyWeapon impact frame and `transformer_feel::combat`'s `robot_finisher_crunch`.
  Rotor sweep / grind ticks are `Melee` + guard_break (light). Key the tiers on `guard_break` + kind.
- Balance: connected combos are stronger now. string → ram (pop) → finisher can KO a 0 % dummy off the
  96 m flat test arena (the voice test waits for the respawn). The ram's knockback is inflated by the
  melee momentum bonus: the closing speed uses the *pilot player's* velocity (they ride at ~24 m/s).
- The juggle launch numbers (`juggle_kb` 0.45 × the row's heavy kb, `juggle_rise` 14 m/s) are tuned on
  the truck row. Light rows (car / bike / jet) may want per-row values.
- The flash chime reuses `ko_streak_willpower` (a bell sequence, pitch 1.3 for the flash and 1.0 for
  the branch). A dedicated cue would be better.

## For stage 5 (PvP-safe Focus dodge)

- Guard break beats powers stances and a transformer's **guard order**, but not a boost-step's perfect
  window or real dodge intangibility (`crate::dodge`). Vehicle attacks stay dodgeable on purpose.
- Fly-by and barrage vehicle attacks are projectiles and don't carry guard break (projectiles have no
  flag).

## Hero scale

- The cap is applied host-side in `spawn_cast` after the shout bonus and before the hidden legendary
  roll: a legendary transformer is 1.2× (~5.5 m).
- The icon robot / alt ratio comes from the authored shapes. For two-mode icons, that's the alt spec's
  largest extent against the robot spec's height, so Optimus (1.45×) ends up as a ~3.2 m truck with
  a 4.6 m robot. Generic transformers use a ratio of 1 (the robot stands as tall as the alt is long).
- "Said big" means recipe scale ≥ 7. The lexicon's `big` is ×1.4, so "big optimus" (8.4) keeps its size.
  Combiners keep their size through the colossus row's `giant: true` (combat.ron).
