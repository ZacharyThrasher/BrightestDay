# ANIMATION PASS — what was built (2026-09-27)

Ranked audit: [AUDIT.md](AUDIT.md). Everything here is presentation only: no sim state, no new
network traffic, no hitbox changes (the FFA golden hash can't move).

## Frame sheets
- `body_before.png` — every player moment as the client drew it before this pass (same layer stack as
  `present/players.rs` had). Note: air, crouch, typing, oath, charge, landing and respawn were all the
  same one or two poses.
- `body_after.png` — the same moments with the new layers.
- `body_sequences.png` — the one-shot actions over time (landing soft / hard, air-jump flip forward /
  sideways, dodge, fast tether let-go flip, respawn re-form).
- Regenerate (CPU raster, no GPU, ~2 s):
  `cargo run -p bd_shapes --example body_sheet --release -- [--before | --seq | --probe] [--names naruto,knight]`
  (`--probe` prints the lowest posed voxel per moment — feet must stay near 0).
- In-game: `brightest_day --practice --bots 4 --scenario bodyanim [--record DIR --record-secs 12]` —
  four players in front of the camera: strafing bunny-hopper with air jumps, typist → crouch walk,
  jump + gas boost → dodge, charger → hops.

## Body layers (`bd_shapes::anim_body`, new `AnimState` variants)
| layer | drives | reads as |
|---|---|---|
| `Motion(BodyMotion)` | body-space velocity, vertical speed, yaw rate, air / crouch / boost weights | tuck rising, legs reach + arms rise falling, flail plunging (by fall speed); bank into turns (air-strafe reads as a lean), strafe roll, backpedal lean-back, sprint lean at bhop speeds; crouch (silhouette matches the 1.1 m hull); gas-boost flight lean (legs trail, arms back) |
| `Action { Land, AirJump, Dodge, Release }` | touchdown, `AIR_JUMP`, `DODGE`, `TETHER_RELEASE` | landing squash scaled by fall speed (big drops: lead foot forward, back knee down, ring fist planted); air-jump flip about the axis ⟂ travel (front flip / cartwheel / back flip); dodge dive + shoulder twist; let-go arms fling open + knee tuck (+ forward flip ≥ 20 m/s) |
| `Channel { Typing, Oath, Charge }` | `TYPING` (+ your own push-to-talk), `RECITING`, `CHARGING` | ring fist up at the face, off hand at the wrist, head bowed (a tap per beat); ring fist raised overhead; wide planted stance, fists at the hips, trembling harder the longer it charges |
| `Forming` (existing) | `GameEvent::Respawn` | the ring re-forms the body part by part (0.45 s) |

`bd_shapes::MAX_LAYERS` = 8 (was 4): `pose_layered` and `bd_fx::AnimLayers` take up to 8, so the head-aim
layer / Ascend pose / move-grammar pose no longer get dropped when a cast + tether + hit overlap.

## Body effects (`bd_game::present::body_motion`, pooled bd_fx particles)
- Landing: dust burst above 9 m/s impact; a corps chip ring racing out along the ground above 16 m/s.
- Gas boost: pale, corps-tinted gas puffs from the hips, blown back against the flight.
- Slide: heel sparks, rate by speed.
- High speed (airborne > 17 m/s): thin corps wind streaks peeling off the shoulders.
- Typing / oath: corps motes curling up off the ring fist.
- Respawn: corps light chips rain in from above and converge on the body while it re-forms; a ring shock
  races out at the feet when it locks (+ the existing flash / sound).
- Your own body in first person gets none of these except the landing ring (nothing near the crosshair).

## 1P hands (`viewmodel_anim.rs`, keys in `assets/tuning/viewmodel.ron`)
`air_jump_push` (both hands shove down off the step), `dodge_sweep` / `dodge_roll` (hands sweep against
the dodge, roll into it), `wall_slap` (the hand on the wall's side slaps toward it on a wall refresh).

## Tuning — `assets/tuning/anim.ron` (hot-reloaded)
`enabled` (false = the pre-pass body), ease rates (`air_rate`, `crouch_rate`, `boost_rate`, `channel_in`,
`channel_out`, `turn_rate_smooth`), landing strength range (`land_min_speed`, `land_full_speed`),
`air_jump_flip`, `release_flip_speed`, `respawn_form`, and every effect's threshold / rate
(`land_dust_speed`, `land_ring_speed`, `land_ring_chips`, `boost_puffs`, `slide_sparks`, `streak_speed`,
`streak_full`, `streak_rate`, `channel_motes`, `respawn_chips`, `fx_range`). Pose amplitudes are code
constants in `crates/bd_shapes/src/anim_body.rs` (shared with the frame-sheet tool).

## Perf
- Pose: +1–3 layers per player body (≤ 8 bodies × ~6 parts for costumes); each layer is a few trig calls
  per part. Constructs are unaffected (they never get the new layers). `AnimLayers` grew from 4 to 8
  entries (a Copy array inside `ConstructVisual`; no heap).
- `body_motion` system: one `HashMap<PlayerId, _>` entry per player (allocated once per player), no
  per-frame allocation; particles go through the existing `FxParticles` pool (budget / per-frame cap
  respected, `particle_budget` scales every continuous emitter); effects skip players beyond 90 m.
- Worst case continuous emission per player: boost 38/s + streaks 22/s (both only in the air), slide
  sparks ≤ 68/s (on the ground) — ~500 particles/s with 8 players all boosting at once.
