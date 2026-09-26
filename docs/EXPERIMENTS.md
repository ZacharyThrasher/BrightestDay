# Experiment log

Everything here was measured in this repository; commands to reproduce are listed per experiment.
Negative and partial results are kept on purpose. UNKNOWN = not measured (usually: needs a real Windows
PC with an RTX 3080, a real microphone, or a human player).

## 0. Environment constraints (affect every result below)

- Built in a Linux cloud container: 4 vCPU, no GPU, no audio device. The target is Windows 10/11 + RTX 3080-class.
- Network policy blocks `huggingface.co`, `typesafe.ai` (Jev), `registry.ollama.ai`, `cdn.jsdelivr.net`, `unpkg.com`.
  GitHub release assets, npm and PyPI work. So: no Jev, no HF-hosted models; models come from GitHub releases.
- No Anthropic API key in the container, so System 2 was evaluated by **record/replay** (§6).

## 1. Movement before AI (Experiment A)

`npx tsx --test test/movement.test.ts` — headless, deterministic, fixed 60 Hz.

| check | result |
|---|---|
| ground run speed | 11.0 m/s (cap) |
| land + immediate jump (bhop) keeps speed | 11.0 → 11.0 m/s over 5 hops |
| air strafing (Source AirAccelerate, 1.4 m/s wish cap) | 11.0 → 24.6 m/s in 8 s with a simple scripted strafe bot |
| ramp (8 m tall, 34°) at 30 m/s | launched to 11.3 m feet height (clip-velocity keeps tangential speed) |
| tether swing from 40 m | peaks 33.6 m/s; release keeps momentum (35.0 m/s 0.4 s later) |
| accessible flight | cruise capped at 17 m/s; boost reaches 63.7 m/s; overspeed decays slowly (53.7 after 2 s) |
| wings (glide mount) | 159 m horizontal from an 80 m drop vs 43 m falling |

Design decisions this validated: velocity is state (input only redirects it); landing never zeroes speed
(jump buffer skips friction on the jump tick); flight is deliberately slower than momentum tech so it
does not flatten skill (the "flight-flattens-skill trap"); construct surfaces (ramps, launchers, conveyors,
mounts, tethers) feed the SAME velocity/collision model as level geometry.

Found by evaluation, fixed: the player could not step over a 0.4 m ledge (bridge ends), so a Quake-style
step-slide move was added. UNKNOWN: whether it is *fun* — needs human play.

## 2. Construct substrate (Experiment B/C)

`npx tsx --test test/constructs.test.ts` — all scenario tests pass:

| scenario | without construct | with construct |
|---|---|---|
| turret barrage | 4–5 hits in 4 s | wall *between*: 0 hits |
| reflecting shield | – | 9 shots reflected; turret damaged |
| rocket glove punch | – | drone disabled |
| grab → orbit → throw into wall | – | drone thrown 18 m, damaged |
| falling civilian (60 m) | hurt | cushioned trampoline under predicted landing: safe |
| 25 m canyon | – | span-placed bridge, player walks across, goal reached |
| 40 m tower | – | launch pad: 47 m apex |
| "10,000 planet-sized anvils" | – | clamped: count 10000→1 (willpower), size ×1e7→×10, all clamps reported |

The representation that survived: **parts (8 primitive shapes) + motion mode + placement relation +
behaviour list**. 90+ forms, 22 behaviour types. Behaviours are orthogonal to forms, e.g. `strike` works
on a glove, a wall or a wrecking ball; `thrust` means "flies forward" on a dynamic object, "hits harder"
on a strike, and "propels the player" on a mount.

## 3. Semantic layer v1 (rules + lexicon) and the frozen held-out protocol

- `eval/EVAL_SPEC.md` (functional contract) was written **before** the interpreter existed.
- Two 120-item held-out sets were written by an independent agent that could only read EVAL_SPEC.md
  (`eval/heldout/`, committed at `385cfb5` before any interpreter code).
- Protocol: build + tune on my own dev set (78 items) → freeze → run A (inspect failures) and B
  (aggregate only, never inspected) → revise using A only → re-run both.

`npx tsx eval/run.ts <file> [--quiet] [--no-sem] [--s2 record:<f>|replay:<f>] [--s2mode hybrid|always]`

| build | dev (own) | held-out A | held-out B (unseen) | B confident-wrong |
|---|---|---|---|---|
| frozen `3d002f2` | 100% | 68.3% | **55.0%** | 26.7% |
| revision 1 (A-driven) | 100% | 90.8% | **62.5%** | 18.3% |
| revision 1, System 1.5 off | – | 90.0% | 61.7% | 20.8% |

**Findings**
1. Dev 100% vs unseen 55%: a rule/lexicon system tuned on its author's own phrasings overfits badly.
   (The "benchmark overfitting" failure mode from the brief — observed, not hypothetical.)
2. A-driven revision: +22.5 points on A, +7.5 on B. The missing abstractions found in A's failures were
   real (see commit `a4c5dfc`), but most fixes were pattern-shaped and transferred poorly.
3. Weakest categories on unseen data: follow-ups, references, contradictions, impossible requests,
   ASR noise. Strongest: straightforward, novel composition, analogy.
4. System 1.5 (word vectors) is **not** the bottleneck: +1 item on B. Failures are compositional /
   pragmatic, not unknown nouns.
5. Calibration: after revision 1, S1's confidence bins on A are 0.85+ → 90% correct, 0.7–0.85 → 83%.
   A coverage-based "I'm unsure" signal flagged 26/120 A items for escalation — **none of which were
   failing**; the 11 failures all looked confident. Escalation-by-uncertainty alone would not catch
   S1's errors (the "confidence theater" risk, measured).

Latency of the rule path (in-process, Node, this container): p50 ≈ 1.2 ms, p95 ≈ 10 ms, max ≈ 40 ms.

## 4. System 1.5: word vectors for unknown words

Built by a sub-agent (`docs/experiments/SEMANTIC_MODELS.md`): ConceptNet Numberbatch 19.08, pruned to
127k terms, 200-d int8 (27 MB), 0.78 top-1 / 0.87 top-3 on 120 unknown-object phrases, ~0.1–0.2 ms/query,
loads in ~90 ms, runs in Node and the renderer with no native deps. License CC BY-SA 4.0 (attribution
in README); an MIT spaCy-based fallback (0.61 top-1) is also built.

## 5. Small local LLMs as typed-decision engines (SemIf/OpenJev idea)

Sub-agent result (`docs/experiments/SEMANTIC_MODELS.md`): Qwen2.5-0.5B/1.5B-Instruct Q4_K_M via llama.cpp,
direct option-logit scoring on 108 typed questions: **57% (1.5B), 34% (0.5B) — below the 65%
majority-class baseline**; few-shot and calibration did not rescue it. JSON generation valid but often
wrong. Conclusion: at ≤1.5B, generation-free typed decisions are not viable zero-shot for this workload;
Laya-style fine-tuning on logged utterance→decision pairs remains the hypothesis worth testing later.

## 6. System 2: a stronger model compiling into the same bounded ops

**Setup.** `src/lang/escalate.ts` defines one system prompt (vocabulary: concept ids, add-ons, 22 behaviour
types, placements, refs; rules: compose, abstain on unsupported/contradictory, request absurd sizes and
let the validator clamp) and a per-request user message (scene summary + utterance + System-1's attempt).
The model's JSON is compiled by `Interpreter.compileS2` into ordinary ops — unknown ids/behaviours are
dropped and noted, never guessed — and then validated like everything else.

**Emulation (no API key in the container).** Requests for all 240 held-out items were recorded
(`--s2 record:`), answered by a fast model (Claude Haiku-class, i.e. the runtime default
`claude-haiku-4-5`) playing System 2, and replayed (`--s2 replay:`).
- *Negative result:* the first attempt gave each of two agents 120 requests; both wrote keyword-heuristic
  scripts instead of answering (only ~29 distinct responses per 120). Detected by response-diversity
  checks, quarantined in `eval/s2/invalid/`, redone with 12 agents × 20 requests and an explicit
  no-scripts rule (119–120 distinct responses per set). Lesson for autonomous evaluation: *check that an
  evaluator actually did the work.*
- Caveats: batching 20 requests in one context is not identical to 20 independent API calls; latency
  is not measured by replay (UNKNOWN on a real network; Haiku-class models typically take ~1–2 s for
  ~300 output tokens). One emulation agent's summary quoted two short B fragments into the implementer's
  context despite instructions; no change was made in response to them.

| policy | A (inspected) | **B (unseen)** | B confident-wrong |
|---|---|---|---|
| System 1 only (rules, rev 1) | 90.8% | **62.5%** | 18.3% |
| System 2 on every utterance | 87.5% | **80.8%** | 15.8% |
| Hybrid: S2 only when S1 unsure | 90.0% | 71.7% | 25.0% |
| Speculative: act on S1, S2 overrides on disagreement | 88% | **80%** | – |

S1/S2 agreement analysis (`--compare`, aggregate only for B):

| | A | B |
|---|---|---|
| agree | 104/120 | 80/120 |
| accuracy when they agree | 96% / 95% | **89% / 90%** |
| when they disagree: S1 right / S2 right | 56% / 38% | **10% / 63%** |
| oracle (either right) | 94% | 85% |

**Conclusions**
1. On unseen phrasing a small fast LLM compiling into the bounded vocabulary beats the hand-built rule
   system by ~18 points, while the rules win on the phrasings they were tuned on (A). The rules are
   still valuable: 1 ms, offline, deterministic, and correct ~90% of the time *when the model agrees*.
2. "Escalate when uncertain" underperforms because System 1's errors are confident; **agreement between
   two different systems** is the useful confidence signal.
3. Adopted policy (implemented in `Interpreter.commitSpeculative` / `Director.onCorrection`): act on
   System 1 immediately (instant feedback), run System 2 in parallel, and visibly reform the construct if
   System 2 disagrees ("the ring reconsidered"). Without a model configured, the game degrades to the
   System-1 numbers above.
4. Distillation hook: System-2 compilations for unknown phrases are stored as recipes and reused by
   System 1 next time (`Interpreter.recipes`). Not yet evaluated over a play session (UNKNOWN).

## 7. Solution diversity (world-state judged)

`npx tsx eval/diversity.ts` — 6 challenges × 9 deliberately different requests (written by me, so this
is a *dev* measurement), judged only by physics outcomes; a naive autopilot walks/jumps/boosts toward the
objective where a person would. Do-nothing baselines fail all six.

| challenge | first run | after substrate fixes |
|---|---|---|
| cross a 25 m canyon | 4/9 | 5/9 |
| reach a 40 m roof | 2/9 | 6/9 |
| falling civilian lands unhurt | 7/9 | 8/9 |
| barrage: ≤1 hit in 5 s | 8/9 | 8/9 |
| disable/capture aimed drone | 9/9 | 9/9 |
| clear the boulder door | 2/9 | 6/9 |
| **total** | **32/54** | **42/54** |

The failures were mostly substrate gaps, not language: ladders couldn't be climbed; grapples didn't winch;
launches aimed at a wall face instead of its top; elevators stopped short; kinematic strikes used visual
(non-solid) extents; the boulder was wedged into a door narrower than itself; "grab the rock and throw
it" created two hands (no cross-clause sequencing). Each fix is general (e.g. every ballistic launch now
samples the path and clears the tallest obstacle). Remaining failures are mostly honest physics (wings from
standing height can't cross a 25 m gap; orbiting shields leave gaps).
