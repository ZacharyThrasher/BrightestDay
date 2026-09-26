# Evaluation contract (frozen before the interpreter exists)

This file defines the *functional* vocabulary used to score the construct interpreter.
It deliberately describes **what should happen in the world**, not how the interpreter
represents it internally. It was written before the parser/lexicon, so held-out
corpora written against it cannot be tuned to the implementation.

## Item format (JSON Lines, one item per line)

```json
{
  "id": "A-017",
  "category": "novel_composition",
  "scene": "drones",
  "aim": "drone",
  "context": ["make a big hand"],
  "utterance": "now squeeze it until it pops",
  "expect": {
    "intent": "act",
    "functions_any": ["crush_target", "restrain_target"],
    "functions_all": [],
    "target": "aimed_entity",
    "count_min": null,
    "count_max": null,
    "modifications_any": [],
    "approximation_ok": true,
    "must_clamp": false,
    "must_abstain": false
  },
  "note": "free text for humans; never used for scoring"
}
```

`context` utterances are executed first (unscored) in the same scene, in order.
Only `utterance` is scored.

## Scenes

| scene | layout |
|---|---|
| `arena` | open flat ground, some walls/buildings, player standing. Nothing hostile. |
| `gap` | player on a cliff edge; a 25 m wide canyon; goal platform across at same height. |
| `tower` | a 40 m tall building/ledge next to the player; goal on top. |
| `barrage` | a turret 30 m ahead continuously fires projectiles at the player. |
| `falling` | a civilian is falling from ~60 m, 15 m ahead of the player. |
| `boulder` | a very heavy boulder blocks a doorway 10 m ahead; goal behind it. |
| `drones` | three hostile flying drones 15–30 m away. |

## Aim (what the crosshair is on when the utterance is spoken)

`drone`, `ground_ahead`, `wall`, `sky`, `civilian`, `boulder`, `turret`, `far_ledge`, `none`

## Intents

| intent | meaning |
|---|---|
| `create` | a new construct appears |
| `modify` | an existing construct changes (size, speed, parts, behaviour, count, orientation…) |
| `act` | an existing or newly-made construct performs an action on something (punch, throw, grab…) |
| `control` | meta: dismiss/clear, release/let go, save/recall a design, undo, cancel |
| `abstain` | the system should visibly refuse/ask/say it does not understand rather than guess |

`act` and `create` may overlap ("punch that drone with a giant fist" is either). Scorers accept
either when `intent` is given as `"create|act"`.

## Functions (effects in the world)

| function | meaning |
|---|---|
| `block_projectiles` | stops/deflects incoming projectiles |
| `shield_self` | protective barrier around/in front of the player |
| `shield_other` | protective barrier around someone/something else |
| `support_player` | a surface the player can stand on |
| `bridge_gap` | a surface spanning a gap |
| `raise_player` | moves/lets the player get significantly higher |
| `launch_player` | an impulse that throws the player |
| `propel_player` | sustained movement for the player (vehicle, wings, rocket, board…) |
| `slow_fall` | reduces falling speed (parachute, glider, umbrella…) |
| `tether_player` | rope/cable/grapple connecting player to a point for swinging/pulling |
| `grab_target` | seizes and holds a target |
| `restrain_target` | traps/immobilises a target (cage, net, bubble…) |
| `strike_target` | hits a target (punch, hammer, bat, ram…) |
| `throw_target` | flings a held/other object |
| `crush_target` | drops/presses something heavy onto a target |
| `pull_target` | brings a target toward the player/a point |
| `push_target` | pushes a target away |
| `lift_target` | raises a target |
| `catch_target` | arrests a falling target safely |
| `move_obstacle` | displaces a heavy obstacle |
| `projectile_attack` | fires things at a target |
| `area_attack` | hits an area / multiple targets |
| `orbit` | construct(s) circle around something |
| `spin` | construct/target rotates |
| `attract` | pulls things in over a region (magnet, vacuum) |
| `repel` | pushes things out over a region (fan, blast) |
| `sweep` | moves across an area clearing it |

## Targets

`self`, `aimed_entity` (the thing under the crosshair), `nearest_enemy`, `previous_construct`,
`location` (a point/area), `other` (explicitly named thing not under the crosshair), `none`.

## Modifications (for `modify` intents)

`scale_up`, `scale_down`, `faster`, `slower`, `heavier`, `lighter`, `add_part`, `add_behavior`,
`remove_behavior`, `duplicate`, `count_change`, `reorient`, `retarget`, `release`, `dismiss`,
`save`, `recall`, `undo`, `attach`.

## Categories

`straightforward`, `novel_composition`, `followup`, `reference`, `ambiguous`, `impossible`,
`analogy` (unusual object comparisons, e.g. "like a baggage carousel"), `asr_noise`
(transcription-style errors), `correction` (mid-utterance "no, wait…"), `contradictory`.

## Scoring

An item passes when every non-null expectation holds:
- `intent` matches (or one of `a|b`),
- at least one of `functions_any` is exhibited (if non-empty), all of `functions_all` are exhibited,
- `target` matches (if given),
- count within bounds (if given),
- at least one of `modifications_any` happened (if non-empty),
- `must_clamp` → the system visibly reduced/limited the request,
- `must_abstain` → the system visibly abstained/asked rather than silently acting.

Additionally tracked (not pass/fail): **confident wrong** = the system acted with high confidence
but failed the item; **approximation** = system flagged it as an approximation.
