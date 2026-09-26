# Architecture

```
 keyboard ring console ──keystrokes──► ghost preview + Tab completion ───┐
 (momentum + aim live while typing; cast timer, combo, quick-cast 1–9)   │
                        └─Enter─────────┐                                │
                                        ▼                                │
                         ┌──────────────────── Director ─────────────────┴──┐
                         │ System 1  (≈1 ms)                                 │
                         │   normalise → typo repair → clause parser →       │
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
                         Inspiration.price()  ← novelty of the cast (creativity economy)
                         validateSpec()  ← THE authority boundary
                         size/count/speed/force/energy/lifetime clamps,
                         malformed proposals rejected, every clamp reported
                         then strength ×inspiration, still within LIMITS
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
| `src/sim/world.ts`, `levels.ts` | Rapier world, entities (drones, turret, civilian, boulder…), scenes, goals, race rings; PvP: `world.players` (ids `p1`, `p2` …; `world.player` = the local alias), `damagePlayer(target, amount, cause, by)`, death / respawn at the spawn farthest from the opponent, `lockTarget`, `snapshotMatch()` / `stateHash()` |
| `src/sim/match.ts` | duel rules: warmup → live → round_end → match_end, first to 3, 3-min cap + sudden death, rematch (deterministic, serializable) |
| `src/sim/bots.ts`, `src/lang/practice.ts` | `BotController` (per-tick PlayerInput + typed commands through that player's own Director, `commitSync`), the scripted `PracticeBot`, `attachPracticeBot(world)` |
| `src/sim/constructs/spec.ts` | the declarative construct representation |
| `src/sim/constructs/forms.ts` | 90+ parametric shapes |
| `src/sim/constructs/validate.ts` | the authority boundary (limits, energy budget) |
| `src/sim/constructs/runtime.ts` | placement, bodies, 22 behaviours, cargo, impacts, launches; every construct has an OWNER (player id) whose point of view resolves 'self' / 'aim' / placement / hold / follow / mount; per-player willpower (`energyOf(id)`); strikes, grabs, throws, crushes, blasts and thrown bodies hit opposing lanterns and their constructs |
| `src/sim/constructs/joints.ts`, `machines.ts` | articulation (joint poses, synced colliders), machine locomotion (kinematic ground/hover/air controllers with their own collision queries), brains, weapons, riding, construct damage |
| `src/sim/constructs/machine-spec.ts`, `combat.ts` | machine rigging/clamps/costs/claims; combat tuning table + predictive intercept + turn-limited chasers |
| `src/sim/constructs/catalog/` | named articulated machines & creatures (tank, car, motorcycle, helicopter, airplane, mech, robot, dog, dragon, spider) registered as forms + lexicon concepts |
| `src/lang/parser.ts`, `words.ts`, `lexicon.ts`, `fuzzy.ts` | System 1 |
| `src/lang/interpreter.ts` | references, edits, actions, goals, situational defaults, S2 compile, effect report |
| `src/lang/semantic.ts`, `src/semantic/embed.mjs` | System 1.5 |
| `src/lang/escalate.ts` | System 2 prompt + providers (Anthropic, OpenAI-compatible, record/replay) |
| `src/lang/director.ts` | the interface the app uses (preview / commit / onCorrection) |
| `src/sim/inspiration.ts` | creativity economy: novelty signatures, inspiration meter, power scaling within limits, creative-solution bonuses |
| `src/app/typing.ts` | typing as a skill: focus time, cast clock/bests, combo, Tab completion, quick-cast slots |
| `src/render/`, `src/app/`, `src/electron/` | renderer, HUD/input, Electron shell |
| `eval/` | contract, dev set, frozen held-out sets, runners, results |
| `test/` | movement + construct scenario tests |
