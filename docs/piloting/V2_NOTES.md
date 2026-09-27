# Transformers V2: notes (stages 1–3 done; 4–5 queued)

Rules: COMBAT.md §5d-quinquies. Code: `bd_sim/src/constructs/brawl.rs` (search for `V2`), the hero cap in
`world/actions.rs::spawn_cast`, guard break in `world/combat.rs::apply_hit` and `powers/ops/counter.rs`,
the cue in `bd_game/src/present/transformer_feel/flash.rs`, the camera in `present/camera3p.rs`.
Tests: `bd_sim/tests/transformer_v2.rs`.

## Measured (before stage 1 → after stage 2 → after stage 3)

`cargo test -p bd_sim --test transformer_v2 v2_measure -- --ignored --nocapture`: a scripted 20 s duel
against a still 1.8 m dummy (put back in front when knocked > 14 m). The *timed* player presses LMB when
the robot is ready and Transform 0.1 s after the string's ender. The *masher* presses LMB at 7.5 Hz and
Transform every 1.2 s.

| pilot | before: presses / hit | stage 2: presses / hit | stage 3: presses / hit | stage 2: vehicle attacks landed | stage 3: vehicle attacks landed | stage 3: flash windows taken |
|---|---|---|---|---|---|---|
| Optimus, timed | 1.13 (43 / 38) | 0.86 (43 / 50) | **0.86** (43 / 50) | 8 / 8 | 8 / 8 | 8 / 8 (100 %) |
| Optimus, masher | 5.57 (167 / 30) | 20.9 (167 / 8) | **13.9** (167 / 12) | 1 / 1 | 1 / 1 | 1 / 1 |
| car, timed | 1.50 (60 / 40) | 0.86 (60 / 70) | **0.83** (60 / 72) | 8 / 10 | 9 / 10 | 10 / 10 (100 %) |
| car, masher | 3.34 (167 / 50) | 10.4 (167 / 16) | **7.26** (167 / 23) | 15 / 17 | 2 / 3 | 3 / 3 |

Stage 3's input buffer rescues mistimed taps for everyone, timed or not: the masher's own hit count rose
(8 → 12 for Optimus) simply because more of its 7.5 Hz taps now land instead of being dropped by the
0.3 s string interval. The timed player's numbers are unchanged (its script was already frame-perfect,
so there was nothing to buffer). Vehicle-attack conversion (`xattacks_landed / transform_presses`, not
in the table) stayed pinned near where stage 2 left it — 6 % for Optimus's masher, 12 % for the car's,
both ~17 transform presses over 20 s — because the early-transform buffer only ever holds a press for
`early_xform_secs` (0.1 s) *from the press itself*, not from whenever the string entered recovery; see
the re-arming bug below. The timed/masher gap stays wide (0.86 vs 13.9 presses/hit for Optimus).

## Stage 3 (input buffer, early-transform buffer, branch window fix, dodge offset) — done

New tunables in `combat.ron` → `transformer`: `buffer_secs` (0.12 s), `early_xform_secs` (0.1 s),
`dodge_grace_secs` (0.5 s). New state in `BrawlState`: `press1_buf`, `press2_buf`, `xform_buf`. Tests:
`transformer_v2.rs` STAGE 3 section (buffered tap, early transform, late-connect branch, dodge offset,
masher regression guard).

- **Input buffer** (`brawl::fight`): an LMB/RMB press while its own cooldown (`cd` / `blaster_cd`) is
  still running is remembered in `press1_buf` / `press2_buf` for `buffer_secs` and fires the instant
  the cooldown clears, instead of being silently dropped. The buffer is only *set* when the raw press
  arrives (never re-armed on later ticks) and only *cleared* once it actually fires — an earlier draft
  cleared it unconditionally on the press tick, which meant a press arriving before the cooldown ended
  never actually got remembered. Caught by `a_buffered_lmb_tap_fires_at_the_first_legal_frame`.
- **Early-transform buffer** (`brawl::ender_pending` + `pilot.rs`): a Transform press while a string's
  about to land its ender (`cd > 0 && !string_done && step == string_hits - 1`) or a Launcher heavy is
  winding up (`hold > 0 && !heavy_done`) is held in `c.brawl.xform_buf` for `early_xform_secs` instead
  of firing an immediate plain transform; `wants_xattack` becoming true while the buffer is still alive
  consumes it into `start_xattack`. If the swing whiffs and the buffer runs out first, it falls back to
  the plain transform it would have been (no press is silently eaten).
  - **The re-arming bug (the one that mattered):** the first version used `s.transform_req` — a sticky
    flag also set synthetically when the buffer expires unconverted, so the code could fall through to
    the SAME "arm a fresh buffer" branch it had just come from, as long as `ender_pending` was still
    true. Since `ender_pending`'s window is the *whole* recovery (up to ~0.4 s for some rows), far
    longer than `early_xform_secs`, this let the buffer keep re-arming itself for the entire recovery —
    any masher Transform press landing anywhere in that window (not just the last 0.1 s) eventually
    converted once the ender connected. Measured effect: the masher's vehicle-attack conversion jumped
    from ~6 % to ~24 % (Optimus) before the fix. The fix separates "a fresh press this tick"
    (`fresh_press`, captured before the buffer-expiry logic touches `s.transform_req`) from "the buffer
    just expired, fall through to the plain toggle" — only `fresh_press` may arm a new buffer.
  - Applies to the main piloted path (`step_piloted`) only, not the small-suit path (`suit_transformer`,
    ≤ 2.6 m — "you are the robot"); its Transform handling is a separate, simpler flow.
- **Branch window fix** (`step_xattack`): the fallback finish (`begin_finish`) used to trigger on
  `xatk_t >= xattack_secs` unconditionally, which cut the branch window short whenever the vehicle
  attack connected late in the burst (little of `xattack_secs` left to run out the window's own
  length). Now that fallback is gated on `branch <= 0.0` too, so a still-open branch window always gets
  its full `branch_secs`, connect early or late. Whiffed attacks (branch never opens) are unaffected.
- **Dodge offset** (`brawl::fight`, the boost-step trigger): starting a boost-step mid-string
  (`step > 0 && open > 0.0`) tops `open` up to at least `dodge_grace_secs` (0.5 s) instead of letting
  the dash's own time (and any repositioning after it) eat into the ordinary 0.7 s `combo_window`. The
  next LMB continues the string (`step` increments) instead of restarting it, as long as it lands
  within the grace.
- The masher regression guard (`masher_input_still_cant_farm_free_vehicle_attacks`) asserts
  `xattacks_landed / transform_presses < 0.15`; measured ~0.06–0.12. Worth re-checking after any future
  change to `buffer_secs` or `early_xform_secs` — the two combine multiplicatively (more landed enders
  × a wider conversion window), so small increases to either can swing the masher rate a lot.

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

## Transformers V3: mobility parity, chase tools, alt-mode weapons

Rules: COMBAT.md §5d-sexies. Code: `bd_sim/src/constructs/kit.rs`, hooked in `pilot::step_piloted`.
Tests: `bd_sim/tests/transformer_v3.rs`.

### Measured: the chase (`transformer_v3::chase_piloted_closes_as_fast_as_on_foot`)

The runner is a bhopping strafer. It runs up for 0.5 s, then hops with jump held, weaving ±40° off
straight away every 0.6 s. It gains from ~10 to ~16 m/s over 15 s. The chaser starts 30 m behind. It
uses the same technique: it runs up, hops, weaves ±25° around the bearing, and presses boost-step /
dodge every 0.5 s. The vehicle drives in, pops out at 12 m, then chases on as the robot. The number is
the time until the hull gap is ≤ 2 m (capped at 15 s).

| chaser | V2 (before) | V3 (after) |
|---|---|---|
| on foot (reference; the on-foot dodge adds no sustained speed) | 13.5 s | 13.5 s |
| piloted robot (Optimus, 4.6 m) | 13.7 s | **4.7 s** |
| vehicle → pop-out | 15 s (never) | **2.8 s** |

The test asserts robot ≤ on foot, vehicle ≤ on foot, and V3 robot < V2 robot. Top speed with W + A/D
air strafing (`robot_bhops_and_air_strafes_like_the_pilot`): on foot 10.5 m/s, V3 robot 9.5 m/s, V2
robot 5.9 m/s. A skilled on-foot player (tethers, real strafe timing) is faster than this script.
Tethers also work from the robot now.

The robot beats on foot mostly because of the boost-step. It holds `dash_speed` (20 m/s) for
`dash_secs`, and in the air the kit keeps that speed (Source air rules). The on-foot dodge never raises
sustained speed. If robots out-chase people too hard in playtests, look at `dodge_cd` / `dash_speed`
first. The kit itself is at parity.

### Vehicle weapons: events for presentation (V3 SAUCE)

- Every shot: `GameEvent::ShotFired { construct: Some(id), archetype, muzzle, dir, .. }`. The
  archetypes are Cannon for the tank shell, Minigun for the coax / nose / chin / car gun, and Rocket
  for missiles / rocket pods. Shells and missiles are `ProjectileKind::CannonBall` with an explosion.
- Plus `GameEvent::HeldFx { fx: HeldFx::VehicleFire (61), pos: muzzle, dir, strength: code }` with
  these codes (`kit::VehicleGun`): 1 tank shell, 2 coax MG, 3 jet nose cannon (salvo alternates
  sides), 4 missile, 5 chin gun, 6 rocket pod (4-round salvo), 7 car gun, 8 boost-ram (`pos` = the
  tail, `dir` = backward; a `HeldFx::Thrust` fires with it).
- The tank turret aims at the pilot's crosshair, so the turret's yaw / pitch is the pilot's look (the
  replicated player yaw / pitch). There is no separate turret state on the wire.
- Pop-out: `HeldFx::Transform` with strength ≈ +4 (`TRANSFORM_SECS / pop_secs`), the same
  speed-scaled cue as combat snaps.

### For the lead

- The masher guard (`masher_input_still_cant_farm_free_vehicle_attacks`) is green in the final state:
  0 / 17 masher transform presses become vehicle attacks. The timed player is unchanged at 8 / 8
  (Optimus) and 9 / 10 (car). The 3 / 17 seen mid-work most likely came from an interim pop-out that set the
  body's velocity straight at the dummy. That put every random vehicle → robot press right on the
  target, so more enders connected. The final pop-out is a short lunge that stops `pop_gap` off the
  target, and the early-Transform buffer is untouched. So the guard stays as it is: the rise was a
  side effect of the interim build, not of timed-play-like hits going up.
- `Construct::kit` is host-only (not replicated). Client prediction of cockpit seats is unchanged
  (none, except titans).
