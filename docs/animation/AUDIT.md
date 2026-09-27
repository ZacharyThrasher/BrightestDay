# Animation & effects audit (ANIMATION PASS, 2026-09-27)

Owner ask: "different animations / additional animations / effects for various things". Priority: gameplay
feel >>> art fidelity; presentation must make things feel powerful and **readable in a fast FFA**.

Method: read every presentation path (`present/players.rs`, `bd_shapes::anim`, `viewmodel_anim.rs`,
`present/{air_moves,construct_anim,constructs,impact,charge,pickups,water,connect,giants_fx}.rs`,
`bd_shapes::behaviours`) against what the sim replicates (`replicate::pflags`, `events::move_flags`,
`MoveEvents`), rendered the player rig through every state offline (`cargo run -p bd_shapes --example
body_sheet`, CPU raster → `docs/animation/body_before.png`) and ran the existing scenarios.

Score = **F** (how often a player sees it: 5 = every second, 1 = rare) × **W** (how weak it is now:
5 = nothing at all, 1 = good). Sorted by score. **Built** = done in this pass (see the end).

| # | thing | F | W | score | now | verdict |
|---|---|---|---|---|---|---|
| 1 | **3P body in the air** (hop, fall, air-strafe) | 5 | 5 | 25 | one static pose (`airborne_local`) whatever the vertical speed, turn or strafe; bhop chains look like a statue sliding through the air | **Built**: `Motion` layer — tuck rising, legs reach + arms spread falling (by fall speed), bank into the turn (yaw rate = air-strafe), lean into strafes |
| 2 | **3P crouch** | 4 | 5 | 20 | no pose at all: a crouched player (1.1 m hull) is drawn standing (1.8 m) — the silhouette lies about the hitbox | **Built**: crouch in `Motion` (hips drop, knees out, torso forward) |
| 3 | **3P ground strafe / backpedal lean** | 5 | 4 | 20 | the body just yaws to the look; the gait is the same forwards, sideways, backwards | **Built**: roll into the strafe, lean back when backpedalling, a harder forward lean at bhop speeds |
| 4 | **Landing by fall height** (3P body + world) | 5 | 4 | 20 | remote: a sound on `LAND_HARD`; local: a camera dip; the body never squashes; no dust, no ring | **Built**: `Action::Land` squash + recover scaled by impact speed (super-hero crouch on big drops), dust/chip ring at the feet (light → hard tiers) |
| 5 | **Typing / voice / oath / charge body language** | 4 | 5 | 20 | `TYPING` and `RECITING` are replicated but nothing reads them: a caster is invisible until the construct appears (typing is *the* vulnerable window in an FFA) | **Built**: `Channel` layer — typing: ring fist up at the face, off hand at the wrist, head bowed to the ring + ring motes; oath: ring fist raised high; charge: planted power stance, fists at the hips, trembling |
| 6 | **Gas boost** (3P) | 4 | 5 | 20 | `BOOSTING` replicated, unused: nobody can see you're on gas | **Built**: boost lean in `Motion` (legs trail, arms back) + a pale gas plume from the hips |
| 7 | **Respawn** | 3 | 5 | 15 | a flash + sound; the body pops in fully formed | **Built**: the ring re-forms you — body plays `Forming` (parts pop in with overshoot) under a column of corps light chips raining in, a ring shock at the feet on lock |
| 8 | **Air jump / dodge body** (3P) | 4 | 4 | 16 | great world FX (step, streak, afterimages) but the body itself does nothing | **Built**: `Action::AirJump` — flip in the travel direction (front / back / cartwheel); `Action::Dodge` — low twisted lean into the dodge |
| 9 | **Tether release** (3P) | 4 | 3 | 12 | swing pose just ends → air pose | **Built**: `Action::Release` — arms fling open, knees tuck, a forward flip when released fast |
| 10 | **Slide** (world) | 4 | 3 | 12 | pose is good; no contact effect (a sound only) | **Built**: sparks / chips kicked from the heels, rate by speed |
| 11 | **High speed** (3P, remote) | 4 | 3 | 12 | nothing tells you someone is at 25 m/s except their position change | **Built**: corps-coloured wind streaks peeling off the shoulders above 17 m/s |
| 12 | **1P hands: dodge / air jump / wall refresh** | 4 | 3 | 12 | jump, land, boost, tethers, cast are all animated; dodge, air jump and wall refresh don't touch the hands | **Built** (viewmodel_anim, shared with WEAPON FEEL — locked): dodge sweeps the hands against the dodge, air jump pushes them down, wall refresh slaps the near hand |
| 13 | Layer budget | — | — | — | player bodies were capped at 4 anim layers: base + cast + tether + hit already dropped the head-aim layer and the Ascend pose | **Built**: `AnimLayers` / `pose_layered` take 8 |
| 14 | Wordless taunt / emote | 2 | 5 | 10 | none | **Built (TAUNT & VOICE TELL, owner-approved)**: `Taunt` action (default **Y**): tap = flex → beckon → shrug → bow rotation, hold = dance; any input cancels, rate-limited on the host, no gameplay effect. See BUILT.md "Taunts + voice tell". |
| 15 | Voice casting (3P, remote) | 3 | 4 | 12 | push-to-talk isn't replicated, so others see nothing until the shout word | **Built (TAUNT & VOICE TELL)**: `buttons::VOICE` → `PlayerSnap::tell`; everyone sees the typing pose + motes, plus corps sound-wave rings off the mouth. |
| 16 | Flying constructs / vehicles banking | 3 | 3 | 9 | `Flyer` gait pitches forward with speed, never banks | Not built (next): bank = yaw rate from the interpolated transform, same as the players' air bank. `construct_anim.rs` already has yaw rate in `AnimDrive`. |
| 17 | Creature landing squash | 2 | 3 | 6 | leaping creatures land with no squash | Not built (next): same `Action::Land` layer works on any rig (root squash is rig-generic). |
| 18 | Footstep dust for players | 5 | 1 | 5 | footstep sounds + 1P bob exist; dust would be noise at 8 players | Leave. |
| 19 | Construct summon assembly | 3 | 1 | 3 | excellent (staged bricks, clicks, lock) | Leave. |
| 20 | Creature attack wind-ups | 3 | 1 | 3 | telegraph anticipation from `attack_timing`, jaw roar, stomp | Leave. |
| 21 | Construct death / despawn | 3 | 1 | 3 | shatter into own voxels / dissolve-to-ring / collapse | Leave. |
| 22 | KO shatter | 3 | 1 | 3 | instant corps-coloured shatter, star KO poof | Leave. |
| 23 | Pickups | 2 | 1 | 2 | beacon, per-kind idle motion, carrier hand | Leave. |
| 24 | Water | 2 | 2 | 4 | splashes on entry (Miami) | Leave. |
| 25 | Terrain destruction / debris | 4 | 1 | 4 | voxel carving + physical debris cubes | Leave. |

### 1P hands (already strong, `viewmodel_anim.rs`)
Look sway, footstep bob, strafe lean, speed tuck, jump / land by impact, slide brace, per-hand tethers
(throw, grip, reel tremble, whip back), ring raise while typing / talking + per-letter twitch, cast thrust,
oath steady, held weight, gun kick, hit flinch, launch flail, KO drop, charge clench, palm beams. Gaps were
only #12.

## Routed to other workers (not built here)

- **TOOLBOX PRESENTATION** (per-MoveOp visuals, `move_fx.ron`): blink / dash / shove / slam body poses for
  power moves — the new `Action` layer kinds (`Dodge`, `Land`) are generic and could be reused by the move
  grammar for dash / slam bodies (e.g. `Dodge` for a Flash dash lean, `Land` with strength 1 for a slam
  touchdown). Borrowed-move HUD chips / chain pips are theirs.
- **WEAPON FEEL** (`weapons.ron`, 1P/3P swings): 3P held-weapon swings still ride `constructs::place`
  (`sc.swing` curve) with the arm locked by `gun_pose`; light vs heavy 3P arm anticipation is theirs. I
  touched `viewmodel_anim.rs` only to add three impulse methods + their calls (dodge / air jump / wall
  refresh); lock note left in the file header.
- **IMPACT FRAMES** (`docs/impact/API.md`): victim hit reactions (`AnimState::HitReact`, players.rs
  `react`) are theirs — untouched. The `Action::Land` squash is *not* a hit reaction (landings only).
  Wall splat / tumble recovery poses ("get-up") are theirs too (hit aftermath).
- **Piloting** (`brawl.rs`, `pilot.rs`): transformer combat bodies — untouched.
- **Legendary visuals** (`present/legend/`): the `LegendPose` arm override — untouched; still wins over the
  gun raise.

## What was built — see `docs/animation/BUILT.md` for the tuning keys, frame sheets and perf.
