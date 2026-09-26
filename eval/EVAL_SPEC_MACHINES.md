# Evaluation contract v2: machines, creatures, creativity (frozen before implementation)

Extends [`EVAL_SPEC.md`](EVAL_SPEC.md) (same item format, scenes, aims, intents). Written **before**
articulated constructs, locomotion, autonomy, weapons or riding exist, so held-out corpora written
against it cannot be tuned to the implementation.

Everything here is judged from **world state over time** (the harness steps the simulation after the
utterance), never from the interpreter's internal representation.

## New scenes

| scene | layout |
|---|---|
| `sandbox` | large open ground with scattered crates, a stack of loose blocks, a low wall, two ground robots (hostile, walk toward the player and punch), two hostile drones, one turret, one civilian standing still. |
| `race` | a long straight road (200 m) with a finish line; nothing hostile. |

Existing scenes remain valid. New aim values: `robot`, `crate`, `construct` (the player's most recent construct).

## New functions (observable effects, measured over the scored window, default 8 s)

| function | observable meaning |
|---|---|
| `moves_autonomously` | a construct created/modified by the utterance travels ≥ 6 m on its own (player not riding) |
| `drives_on_ground` | a construct moves ≥ 6 m while staying within 1.5 m of the ground |
| `flies` | a construct moves ≥ 6 m while ≥ 3 m above the ground for most of the window |
| `walks` | `drives_on_ground` and it has swinging leg parts |
| `wheels_spin` | the construct has parts that rotate in proportion to its travel |
| `has_moving_parts` | at least one part of the construct moves relative to the construct body (rotor, turret, legs, arms, jaws…) |
| `aims_at_target` | a part group turns to point at the target (≤ 20° error at some point in the window) |
| `shoots` | the construct emits projectiles/beams |
| `damages_target` | the target entity loses health or is destroyed |
| `destroys_target` | the target entity reaches 0 health / is removed |
| `attacks_enemies` | ≥ 1 hostile entity (not necessarily the aimed one) takes damage from the construct |
| `follows_player` | the construct stays within 12 m of the player while the player moves ≥ 15 m |
| `guards_area` | the construct stays within 8 m of its spot and damages a hostile that comes near |
| `fetches` | a target object ends up within 4 m of the player, carried by the construct |
| `rideable` | the player can occupy it (the harness issues "get in" / movement input) and it moves with the player's driving input |
| `carries_player` | the player is transported ≥ 8 m while on/in the construct |
| `pushes_objects` | a loose object (crate/block/boulder) is moved ≥ 2 m by the construct |
| `returns` | the construct comes back to within 4 m of the player after acting |
| `multiple_agents` | ≥ 2 independently moving constructs exist at once |
| `creative_bonus` | the game reports a creativity/novelty reward for the utterance |
| `stale_penalty` | the game reports reduced power/increased cost for repeating the same idea |

Old functions (`block_projectiles`, `grab_target`, `throw_target`, …) keep their meanings and may be combined.

## New categories

| category | meaning |
|---|---|
| `vehicle` | cars, tanks, bikes, boats-on-land, planes, helicopters, trains, mechs… that should move and work |
| `creature` | animals, monsters, robots, golems that should move/behave like one |
| `weapon_machine` | constructs that fire, swing, or otherwise attack on their own |
| `companion` | constructs given ongoing orders (follow, guard, fetch, attack) |
| `order` | commands to an existing construct (“attack that drone”, “come back”, “stop”, “get in”) |
| `mashup` | compositions of unrelated things (“a shark with helicopter blades”, “a flying bathtub with cannons”) |
| `creative_economy` | novelty vs repetition (same idea repeatedly vs a new idea) |

## Scoring

An item passes when all `functions_all` and at least one of `functions_any` are observed, the
intent matches, and no `must_*` condition is violated. `approximation_ok: true` means an approximation
(“closest thing I can make is …”) is acceptable if it still produces the functions.
