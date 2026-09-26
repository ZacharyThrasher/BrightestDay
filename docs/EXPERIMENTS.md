# Experiment log

Everything here was measured in this repository; commands to reproduce are listed per experiment.
Negative and partial results are kept on purpose. UNKNOWN = not measured (usually: needs a real Windows
PC with an RTX 3080, a real microphone, or a human player).

> **Voice input was dropped by design decision** (typing is part of the skill: play as fast as you can
> type). The ASR measurements are summarised in [`experiments/ASR.md`](experiments/ASR.md); the code and
> bench are in git history (≤ `c644bb9`). Mentions of ASR below are kept as history; the interpreter's
> "ASR repair" now serves as typo repair.

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

`npx tsx eval/run.ts <file> [--quiet [--by-category]] [--no-sem] [--s2 record:<f>|replay:<f>] [--s2mode hybrid|always]`
`[--window 8] [--phases expected|all|none] [--score observed|claimed] [--jobs N] [--verbose]` — machine functions
(EVAL_SPEC_MACHINES.md) are scored from world state by `eval/observe.ts` (see its header for thresholds and phases).

| build | dev (own) | held-out A | held-out B (unseen) | B confident-wrong |
|---|---|---|---|---|
| frozen `3d002f2` | 100% | 68.3% | **55.0%** | 26.7% |
| revision 1 (A-driven) | 100% | 90.8% | **62.5%** | 18.3% |
| revision 1, System 1.5 off | – | 90.0% | 61.7% | 20.8% |
| revision 2 (trained on dev2) | 100% | 91.7% | **70.0%** | 17.5% |

*Revision 2*: a second independent agent (no access to code, held-out sets or results) wrote `eval/dev2.jsonl`
— 180 items concentrated on the hard categories. The frozen rev-1 build scored **60%** on it (consistent with
the B estimate), 72 failures were analysed into general abstractions (commentary stripping, NP-swap and
count corrections, ordinal references with group splitting, "N more of those", idioms such as "let him go",
recipient "give it wings", ram-vs-place verbs, entity-into-construct placement, antonym-pair contradiction
detection, noun-slot sound repair, merged-word splitting, guarded aggressive repair) → dev2 **85.6%**,
dev 100%, A 91.7%, and **B 70.0%** (+7.5). Broader, independently written training data generalised about as
well as the A-driven revision did; neither closes the gap to System 2 (80.8%).

B has now been evaluated (aggregate-only) 7 times across all builds/policies; its item text was never shown
to the implementer, but repeated aggregate looks carry mild selection risk, so these numbers are reported as
measured and not tuned against.

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

## 8. Typing as a skill + the creativity economy (tuning notes)

Voice was dropped (see the note at the top); the typed console became part of the skill. Everything below
is scripted/simulated in this container: **no human has played it yet** (feel is UNKNOWN).

**Typing while moving.** Opening the console no longer releases the mouse or kills momentum: pointer lock
stays (mouse look keeps aiming, so "that" = what's under the crosshair at Enter), movement keys pause,
velocity is untouched, a held tether (Q/E or mouse button) stays attached and a slide in progress keeps
sliding (`Input.startTyping`). Checked by `tools/ui-smoke.mjs` in the real Electron build (airborne 24.0 →
24.0 m/s after 0.7 s typing; a held Q-tether survives the console; the previous code released it).

**Focus time — now offline practice only, off by default.** The project moved to 1v1 online duels
(`docs/PVP_DESIGN.md`): both players share one clock, so the world can't slow for one typist. The
mechanism stays as a practice option (`focusTimeScale`, menu "Focus time (practice)") and is forced off
whenever `App.sharedClock` is set (the netcode's job); the smoke test checks both. What makes typing a
PvP-fair skill instead: you never stop (momentum, held tethers, slides and mouse aim continue while you
type), casting speed (Tab completion, empty-Enter recast, Shift+Enter chaining, 1–9 quick-cast), and the
combo feeding the per-player inspiration curve — while stale spam of quick-casts is self-limiting. The
measurements below were made before the pivot and still describe the practice option.
While the console is open the fixed 60 Hz sim gets fewer ticks per real second (dt never
changes, so the sim stays deterministic). A meter holds 5 real seconds of slow time and refills in 10 s
while the console is closed; once dry, time runs normally until it's back to 25 %. Chosen with a scripted
typist on the `falling` scene (`npx tsx tools/focus-sweep.ts`: 0.5 s reaction, then N chars at a constant
rate; world-state judged — was the civilian unhurt?). Cells: outcome (world seconds elapsed at the cast).

| typist | "catch him" ×1.0 | ×0.5 | **×0.35** | ×0.2 | "a trampoline under him" ×1.0 | ×0.5 | **×0.35** | ×0.2 |
|---|---|---|---|---|---|---|---|---|
| 3 cps (~35 wpm) | hurt (3.5) | saved | saved | saved | hurt | hurt | hurt | hurt (meter ran dry) |
| 5 cps (~60 wpm) | saved (2.3) | saved | saved | saved | hurt (4.9) | hurt (2.5) | **saved (1.8)** | saved |
| 8 cps (~95 wpm) | saved | saved | saved | saved | hurt (3.3) | saved (1.7) | saved (1.2) | saved |
| 12 cps | saved | saved | saved | saved | hurt (2.4) | saved | saved | saved |

×0.35 is where the gradient is right: without focus only a terse command in time works; at ×0.35 a ~60 wpm
typist can type a descriptive rescue, slower typists must be concise, and ×0.2 is so forgiving that speed
stops mattering. Median dev-set command = 19 chars (p75 28), i.e. ~3.3 s at 60 wpm: one median command per
meter for an average typist, two for a fast one. `settings.json`: `focusTimeScale` (1 = off), `focusSeconds`;
the menu has off / 50 % / 35 % / 20 %.

**Speed tools.** Tab / → accept a faint inline completion (history words > saved designs > concept ids >
add-ons > verbs > synonyms; Tab again cycles); ↑/↓ history; empty Enter recasts the last command;
Shift/Ctrl+Enter forms and keeps the console open; 1–9 quick-cast (saved designs, then the latest distinct
typed commands; scenes moved to Alt+1–9). Typos stay fine: the interpreter's repair (`fuzzy.ts`) turns
"a tnak with wigns" into "a tank with wings". Cast timer (console open → Enter), chars/s and personal bests
are shown after each typed cast; successful casts within 6 s build a combo (×1.15 per step on inspiration
gains, max ×1.6).

**Creativity economy** (`src/sim/inspiration.ts`, `test/inspiration.test.ts`). Each cast's signature =
noun (label head), concept id, grafted add-on parts (roles after the base form's prefix), label adjectives,
physical materials, size bucket, count/arrangement, behaviours (with mode), joint motions, placement,
targets (entity types), verbs — plus all pairs of the "idea" kinds (pairs share a saturating weight of 4, so
combinations dominate). Freshness of a feature = exp(-1.2 × recent uses) (uses decay ×0.85 per cast and
with a 120 s time constant), ×1.3 if never used before. Novelty = fresh share of the weight × (0.55 + 0.45 ×
richness). Measured sequences (drones scene, through the Director):

| cast | novelty | power × | cost × | note |
|---|---|---|---|---|
| a wall | 0.65 | 1.31 | 0.76 | "✦ Inspired — new: wall" |
| a wall (2nd) | 0.20 | 1.15 | 0.87 | quiet |
| a wall (3rd) | 0.07 | 0.85 | 1.17 | "Stale — you've made 3 walls in a row" |
| a wall (4th) | 0.03 | 0.77 | 1.31 | stale |
| a tank ×3 | 0.84 → 0.26 → 0.10 | | | third is stale |
| a tank with wings | 0.46 | 1.22 | 0.82 | "new: wing + tank" — new combination on a stale idea |
| three hammers orbiting me | 0.89 | 1.50 | 0.67 | |
| three swords orbiting me | 0.63 | 1.50 | 0.67 | variation: "new: sword" |
| a shark with helicopter blades | 0.91 | 1.84 | 0.54 | mash-up |

Meter: +0.3 × (novelty − 0.3) per cast (×combo when positive), relaxes toward 0.3 with a 75 s time
constant; start 0.4. Power × = (0.8 + 0.8 × meter) × (1 + 0.25 × (novelty − 0.3)); stale casts (novelty <
0.18) get 0.7–0.85 and never the meter's bonus; clamped to 0.5–2. It scales behaviour speeds/forces
(strike, throw, home, shoot, field, launch, thrust, blast; weapon/drive only downwards until the validator
has caps for them) and lifetime, **after** validation and never beyond `LIMITS`; energy cost × 1/power
(0.5–1.6). Willpower cap 85–125 and regen ×0.7–1.6 follow the meter. Felt difference in the test: the
same "punch that drone" went from strike speed 59 / cost 5.2 (fresh) to 31 / 10.1 (4th in a row).

The economy is **per player** (`world.inspiration.of(playerId)`, one Director per player sets the caster),
so in a duel each lantern has their own usage history, meter and streaks: player 1 spamming walls goes stale
(0.65 → 0.20 → 0.07 → 0.03) while player 2's first wall is fresh (0.65).

Creative solutions: an outcome is attributed to a construct (explicit `by`, else the construct that last
touched / shot / reflected / blasted the target) and pays its OWNER inspiration (+ willpower) × unusualness
(conventional tool for the job ×0.35; halves each time the same noun solves the same job; never for hurting
yourself). Duel jobs: take the rival down +0.15, first hit on them per construct +0.04, break one of their
constructs +0.08, block their shot +0.05 (once per construct). Test (synthetic duel events): rival taken
down with a piano +0.15, hit with a piano +0.04, hit with a cannon +0.01, their wall broken with a whale
+0.08. PvE jobs kept for the eval scenes: a drone downed by a dropped piano +0.12, by a cannon +0.04, the
piano again +0.06. The duel events (`player_damage`, `player_killed`, `construct_destroyed`, `blocked.from`)
are the contract for whoever implements PvP damage.

The eval harness now records `creative_bonus` / `stale_penalty` as OBSERVED functions from the game's own
`inspiration` / `creative_solution` events (dev set unchanged: 78/78).
