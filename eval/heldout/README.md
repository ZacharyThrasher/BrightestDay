# Held-out evaluation corpora

These two corpora were **authored independently, without access to the implementation**
(no files under `src/`, `tools/`, or any interpreter/lexicon code were read). The only
input was `eval/EVAL_SPEC.md`. They are frozen: do not edit them, and do not tune the
interpreter against their contents.

| file | items | sha256 |
|---|---|---|
| `set_A.jsonl` | 120 | `6a2612e8f344d3baa12cd8faefcad8e6d9332c9d8e25fed7e93001b0faf80a41` |
| `set_B.jsonl` | 120 | `84f2c1d36f19c88732a4b33c73bbd5a4d02b208596bd2bc1b48ce34b4cc6031a` |

Both sets share the same distribution but contain different items (no utterance appears in both).

## Counts per category (each set)

| category | A | B |
|---|---|---|
| straightforward | 20 | 20 |
| novel_composition | 20 | 20 |
| followup | 16 | 16 |
| reference | 12 | 12 |
| ambiguous | 8 | 8 |
| impossible | 8 | 8 |
| analogy | 14 | 14 |
| asr_noise | 10 | 10 |
| correction | 6 | 6 |
| contradictory | 6 | 6 |

## Counts per scene

| scene | A | B |
|---|---|---|
| arena | 19 | 19 |
| gap | 15 | 15 |
| tower | 13 | 12 |
| barrage | 16 | 16 |
| falling | 15 | 16 |
| boulder | 14 | 14 |
| drones | 28 | 28 |

Verify integrity with `sha256sum eval/heldout/set_*.jsonl`.
