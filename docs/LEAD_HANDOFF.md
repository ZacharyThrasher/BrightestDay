# Lead orchestrator handoff (keep updated)

Owner: wants autonomous progress; research comparison of orchestration harnesses. Read docs/DESIGN.md
(north star, mystery tenet, feature freeze, snappiness), docs/COMBAT.md (§1a Smash model supersedes
health; §5i balance), docs/CLIENT_INTEGRATION.md, docs/TESTING.md. Memory: brightest-day-direction.

## Process
- Workers = Opus subagents with strict file ownership + their own CARGO_TARGET_DIR; they never commit.
- Lead verifies (tests/gallery/screenshots) and commits with the Co-Authored-By trailer.
- bd_core contract: v1.3 (Transforms, Recipe.grants). Change only via lead.
- Gate: `tools/check.ps1` (fmt/clippy/tests/soak/bench/smoke). Perf gates: frame p99 < 7 ms (quiet
  machine), sim tick p99 < 1 ms, no hitches > 16 ms after load.

## Done (committed)
core, voxel (+arena, tutorial), movement, net, voice, sfx (83 sounds), shapes (forms, anim, machine kit,
costumes, Transformers rigs), lexicon (550k + packs: games, anime, film/TV, internet, transformers),
Wikidata, sim world, constructs (all), powers (24 fruits, 13 stands), party mode (59 effects), client
shell (arena, movement, tutorial playable: `target\game\release\brightest_day.exe --map tutorial`),
hardening gate + property tests.

## In flight at time of writing (2026-09-26 night)
- World engineer: construct follow-ups (portal gun, pierce flag, generic reflect, transform snapshots,
  fire events, own-structure damage, sliced halves, off-tick arena regen, alt_shape through cast),
  then **Smash model** (§1a) + **punch-through**.
- Balance engineer: CombatTuning (combat.ron), telemetry, tournaments (waits for Smash model).
- Movement feel: generous bhop (buffer, landing grace, W-strafe assist, hop_chain events), re-record ghosts.
- Client SIM / UI / NET engineers (bd_game): bridge + presentation + practice + soak viewer + recording /
  console + voice + oath + viewmodel + lobby + party UI / host-join-tunnel + prediction (+ predicted
  construct spawns).
- bd_fx (hard-light rendering, VFX, onomatopoeia, gun FX, transform FX).
- Content: TMNT pack (+ mutagen power); Miami map + city kit; lexicon lazy-loading + TMNT load check.

- Superhero packs: Marvel/X-Men (assets/icons/marvel.ron) and DC/Invincible/others (assets/icons/dc.ron);
  their power entries are staged in `assets/powers_pending/{marvel,dc}.ron` — **the lead must merge
  them into assets/powers.ron** (append; check ids unique; TMNT's mutagen already appended) and run
  bd_sim powers tests + bd_lexicon tests (sim embeds powers.ron via include_str).

- Lexicon bug (next lexicon pass): an exact multi-word icon/entity name must win over phrase parsing
  ("man of steel" currently → "of steel" material → steel sword). Also check other "X of Y" names
  (lord of the rings items, "eye of sauron", "hand of doom", "sword of omens").

- Lexicon (next pass): add phrases for `random:origin` ("mystery origin", "superpower", "random
  superpower", "super serum"?) — sim supports it; "compound v" already points there.
- tools/icons/{dc,marvel} checker scripts reference the deleted assets/powers_pending files — point
  them at assets/powers.ron (low priority).

- **Investigate:** one soak run reported 1 failure at tick 156001 (world engineer, while the balance
  engineer was editing concurrently); didn't reproduce. After balance lands, run the gate soak with
  several fixed seeds (`--seed 1..8 --ticks 200000`) on a quiet tree; if any fails, reproduce by seed.
- UI polish: stock/timed scoring on rules page + scoreboard; first-person held items too large (giant
  hammer); title occasionally missing on first frames.

## Queue (do next)
1. When world engineer finishes: dedicated **sim perf pass** on a quiet machine (FFA soak p99 briefly
   4.3 ms reported under load; kaiju/creature work suspected).
2. When client engineers finish: integrate Miami into map picker; client **perf pass** on a quiet machine
   (`--bench`, `--scenario brawl`); fix anything > budget.
3. Record the **chaos soak video** (`--soak-view` + `--record`) and a brawl clip; encode with ffmpeg
   (installed); send to the owner via SendUserFile.
4. Resolve duplicate icon names between games.ron and internet.ron (among us, banana peel, …).
5. Powers backlog (docs/POWERS_BACKLOG.md) as data-only content workers: Pokémon moves, Naruto, DBZ,
   JJK, Zelda runes, HxH, misc.
6. Full `tools/check.ps1 -Soak -Bench -Smoke` green; package a release zip (exe + assets + lexicon.bin +
   cloudflared note); write README "how to play with friends".
7. Morning summary for the owner: what's playable, exact commands, known issues, what needs their
   hands-on feel feedback.
