# Architecture

```
 mic ──► ASR (utility process, sherpa-onnx) ──partials──► ghost preview ─┐
                                              └─final───┐                │
 keyboard text console ─────────────────────────────────┤                │
                                                        ▼                │
                         ┌──────────────────── Director ─────────────────┴──┐
                         │ System 1  (≈1 ms)                                 │
                         │   normalise → ASR repair → clause parser →        │
                         │   concept lexicon (120 concepts / 700 synonyms)   │
                         │   + 22 add-on parts + reference resolution        │
                         │ System 1.5 (≈0.2 ms): Numberbatch word vectors    │
                         │   for unknown nouns/verbs (always flagged)        │
                         │ System 2 (≈1–2 s, optional, async):               │
                         │   LLM compiles to the SAME bounded op JSON;       │
                         │   runs in parallel, corrects S1 on disagreement   │
                         │ Recipe cache: S2 results for unknown phrases      │
                         └──────────────── proposals (data only) ───────────┘
                                                        │
                                                        ▼
                         validateSpec()  ← THE authority boundary
                         size/count/speed/force/energy/lifetime clamps,
                         malformed proposals rejected, every clamp reported
                                                        │
                                                        ▼
            ConstructRuntime (Rapier) — parts → colliders, behaviours → motion
                                                        │
      Player (Source-style momentum, AoTTG tethers, flight) ◄── same physics ──► entities
                                                        │
                                                        ▼
                  Three.js renderer (cel shading, hard-light material, bloom, HUD)
```

## Principles that survived testing

1. **Probabilistic intelligence proposes; deterministic machinery disposes.** Nothing a parser or model
   outputs is executable. `src/sim/constructs/validate.ts` is the only gate into physics, and it doesn't
   trust any proposer's confidence.
2. **Compose, don't enumerate.** A construct is `parts + motion + placement + behaviours`. There is no
   per-phrase handler; "a rocket glove that punches the drone" = glove form × rocket add-on × strike
   behaviour × entity reference.
3. **One physical vocabulary.** Ramps, launchers, conveyors, mounts (wings, jetpacks, boards) and
   grapple constructs act on the *same* player velocity/collision model as level geometry and the
   manual tethers, so constructs extend movement rather than replacing it.
4. **Failures are visible.** Clamps, abstentions, approximations ("‘hoverboard’ ≈ board"), assumptions
   ("assumed: catch the falling civilian") and System-2 corrections are all surfaced in the HUD.
5. **Adaptive computation, chosen by evidence.** Rules are instant and accurate on familiar phrasing;
   a model generalises better to unfamiliar phrasing; agreement between them is the best confidence
   signal we found. So: act on System 1 immediately, let System 2 confirm or correct.

## Code map

| path | what |
|---|---|
| `src/sim/player.ts` | movement: accel/friction, air-strafe, bhop buffer, slide, step-up, flight, dual tethers, reel |
| `src/sim/world.ts`, `levels.ts` | Rapier world, entities (drones, turret, civilian, boulder…), scenes, goals, race rings |
| `src/sim/constructs/spec.ts` | the declarative construct representation |
| `src/sim/constructs/forms.ts` | 90+ parametric shapes |
| `src/sim/constructs/validate.ts` | the authority boundary (limits, energy budget) |
| `src/sim/constructs/runtime.ts` | placement, bodies, 22 behaviours, cargo, impacts, launches |
| `src/lang/parser.ts`, `words.ts`, `lexicon.ts`, `fuzzy.ts` | System 1 |
| `src/lang/interpreter.ts` | references, edits, actions, goals, situational defaults, S2 compile, effect report |
| `src/lang/semantic.ts`, `src/semantic/embed.mjs` | System 1.5 |
| `src/lang/escalate.ts` | System 2 prompt + providers (Anthropic, OpenAI-compatible, record/replay) |
| `src/lang/director.ts` | the interface the app uses (preview / commit / onCorrection) |
| `src/voice/` | ASR engine (sherpa-onnx) |
| `src/render/`, `src/app/`, `src/electron/` | renderer, HUD/input, Electron shell |
| `eval/` | contract, dev set, frozen held-out sets, runners, results |
| `test/` | movement + construct scenario tests |
