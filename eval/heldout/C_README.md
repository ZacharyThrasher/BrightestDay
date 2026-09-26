# Held-out set C (machines, creatures, creativity)

Two frozen corpora for the v2 contract (`eval/EVAL_SPEC.md` + `eval/EVAL_SPEC_MACHINES.md`).

| file | items | ids | who may read it |
|---|---|---|---|
| `set_C_dev.jsonl` | 90 | `CD-001`..`CD-090` | developers (inspect freely to guide work) |
| `set_C.jsonl` | 150 | `C-001`..`C-150` | nobody; report **aggregate scores only** |

## How they were built

- Written by hand by an independent author who read only `eval/EVAL_SPEC.md`,
  `eval/EVAL_SPEC_MACHINES.md` and `README.md`. No source code, tests, dev sets, results, or other
  held-out sets were opened. No generator script was used.
- Item format is exactly that of `EVAL_SPEC.md`. Every function, scene, aim, intent, target, and
  modification value comes from the two spec files.
- Utterances are written the way a player types mid-action: mostly lowercase, little punctuation,
  shorthand (`w/`, `2 tanks`, `u`, `plz`, `urself`, `lil`), slang (`sic em`, `heel`), a mix of
  one-word requests (`tnak`, `horse`, `jetpack`) and longer creative sentences. About 21% of the
  items (19/90 dev, 34/150 test) contain 1–2 realistic keyboard typos (for example `helicoptor`,
  `dragn`, `shoooot`, `catapault`, `refridgerator`, `pheonix`). The note field says `typo` on those
  items. There are no speech-recognition-style errors.
- The expectations are **functional**: what a reasonable player would expect to see within about
  8 s. `functions_any` lists acceptable alternatives. `functions_all` is used only when something
  is truly required: `rideable` when the player says they will ride it, `shoots` when shooting is
  the point, `multiple_agents` for explicit groups, `follows_player`/`fetches` for those orders, and
  `stale_penalty`/`creative_bonus` for the economy items. `target` is set only when it is
  unambiguous (usually `aimed_entity`) and is otherwise `null`. Counts are set only for explicit
  numbers or plurals.
- `approximation_ok: true` is used on most mash-ups and on exotic creature or vehicle requests.
- `must_abstain` items (3 dev, 4 test, about 3%) are keyboard mashes or requests that are genuinely
  outside what hard-light can do (time travel, deleting gravity, lottery numbers). `must_clamp`
  items (2 dev, 3 test, about 2%) ask for absurd scale or counts (moon-sized tank, a million dogs,
  ten billion hornets).
- Order items put the construct in `context` first (for example `["a tank"]` and then `get in`).
  Intent `act|modify` is accepted for these. Stop and stay orders expect `remove_behavior`.
- Creative-economy items repeat one idea 3–4 times in `context`. The scored utterance either
  repeats it again (`stale_penalty`) or is a new combination (`creative_bonus`).
- Regression items use the old EVAL_SPEC categories (shield, bridge, grab-and-crush, net, mirror,
  launch, parachute, zipline, bubble).
- The test set is a separate sample drawn from the same distribution. None of its utterances
  matches a dev utterance.

## Distribution by category

| category | dev | test |
|---|---|---|
| vehicle | 15 | 26 |
| creature | 14 | 23 |
| weapon_machine | 13 | 21 |
| companion | 10 | 17 |
| order | 13 | 21 |
| mashup | 13 | 23 |
| creative_economy | 5 (3 stale, 2 bonus) | 8 (4 stale, 4 bonus) |
| old categories: straightforward / followup | 4 | 7 |
| old category: impossible (abstain) | 3 | 4 |
| **total** | **90** | **150** |

## Distribution by scene

| scene | dev | test |
|---|---|---|
| sandbox | 37 | 60 |
| drones | 14 | 27 |
| race | 9 | 14 |
| arena | 7 | 10 |
| barrage | 7 | 9 |
| boulder | 5 | 7 |
| gap | 4 | 9 |
| falling | 4 | 5 |
| tower | 3 | 9 |

## Integrity (sha256)

```
bec7c16c6edf947b41019998f56982dc901ee7be50c126b453cf21cf09bc0b9f  set_C_dev.jsonl
4ed2587d505d62c153d3f6f8f8757382d0ab05fff1213e91ace6db2aed8bdcc3  set_C.jsonl
```

Do not edit these files. If an item turns out to be unfair, record it in a separate errata file
and keep the frozen hashes unchanged.
