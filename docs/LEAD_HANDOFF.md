# Lead orchestrator handoff (keep updated)

Owner: wants autonomous progress; this is a research comparison of orchestration harnesses. Read
docs/DESIGN.md first (NORTH STAR, MYSTERY tenet, FEATURE FREEZE, snappiness gates), then docs/COMBAT.md
(§1a Smash model supersedes health; §5h player powers incl. Origin kind; §5i balance), docs/PLAYING.md,
docs/CLIENT_INTEGRATION.md, docs/TESTING.md, docs/BALANCE_REPORT.md. Memory: brightest-day-direction.

## Process
- Workers = Opus subagents with strict file ownership + their own CARGO_TARGET_DIR; they never commit.
- Lead verifies (tests / galleries / screenshots / benches) and commits with the Co-Authored-By trailer.
- bd_core contract v1.3 (Transforms, Recipe.grants). Change only via lead; patch literal call sites.
- Gate: `tools/check.ps1` (fmt/clippy/tests/soak/bench/smoke). Perf gates: frame p99 < 7 ms (quiet
  machine), sim tick p99 < 1 ms, no hitch > 16 ms after load. Never taskkill all cargo (kill own PIDs).
- Usage limits have interrupted workers before: on resume, check `git status` + `cargo check
  --workspace`, then resume each worker with a short "you were cut off; resume" message.

## State (2026-09-27 morning)
Everything below is committed and green unless noted.
- Engine/sim: core, voxel (Archipelago, Tutorial, Miami + city kit), movement (generous bhop), net
  (tunnel/LAN, prediction, reconnect, event timing), sim world (Smash model, punch-through, constructs,
  powers 95 incl. fruits/stands/mutagens/hero origins, party mode 59 effects, balance tuning + telemetry),
  shapes (forms, machine kit, costumes, Transformers rigs, icon normalisation), fx (hard light, VFX,
  onomatopoeia), sfx (83 sounds), voice (experimental PTT), lexicon (552k entries, ~1,800 icons, 9 packs).
- Client: practice, host/join lobby, all menus/rules/presets, console/voice/oath/viewmodel, party UI,
  presentation of every event, director camera + `--scenario showcase`, `--record`.
- Playable: `target\game\release\brightest_day.exe` (build: `cargo build --release -p bd_game`);
  `--map tutorial|miami|orbit`, `--host`, `--join <code|ip:port>`, `--scenario showcase`, `--soak-view`.
- Videos: target\recordings\showcase.mp4 (1080p60), showcase_720p.mp4, soak_director.mp4.

## In flight at time of writing (their reports come to the session that launched them)
- Client perf/polish (brawl p99 7.1–7.4 → < 7; terrain-graft rendering; neon emissive channel; viewmodel
  size; stock/timed scoring UI; title flake). Owns bd_game present/render/debris, bd_fx, bd_voxel mesher.
- Shapes weight pass (mass-scaled anticipation/overshoot, exaggerated hit reactions, secondary follow-
  through, kaiju footfall events) in bd_shapes — then call sites in bd_sim held.rs/creature.rs + client.
- (DONE + committed: shows/Dune pack, Pokémon pass + moves grammar, lexicon compounds/everyday anchors.
  lexicon.bin is gitignored — rebuild with `python tools/lexicon_build/build.py` (~3.5 min).
  Open: tools/icons/games/dsl.py was never committed, so games/gen.py can't run.)
- Orbit map (bd_voxel maps/orbit.rs, gravity-by-altitude profile, space/re-entry KOs, present/space.rs).
- Hitscan guns + bot aim realism + balance trim (bd_sim weapons, COMBAT.md §4b).
- AUDIO WORLD (owner: "feels like we are just throwing noise at the user, it doesn't feel like a world").
  Phase A: new crate bd_audio (real-time mixer on the audio thread: buses, shared aux reverb driven by
  voxel-raycast room estimation, occlusion/air-absorption LPF, HDR loudness window, voice limits +
  virtual voices, snapshots in assets/audio/mix.ron incl. Danger/RingOpen/Space), sfxgen redesign
  (modal synthesis, layered composites, spectral carving, dry assets, variation pools, loudness
  hierarchy), per-map reactive ambience, offline render_mix before/after WAV + spectrogram + report.
  Phase B (after client perf worker releases audio.rs/pcm.rs/present/sound.rs): collapse PlaySfx +
  Sfx3d into one bd_audio path, settings UI. Absorbs "heavy-hit ducking" from the weight pass.
- MUSIC (owner: chiptune classical like "Chiptune Mozart: 8-BIT Classical Music"): new crate bd_music —
  own arrangements of PUBLIC-DOMAIN pieces only (never copy modern arrangements), band-limited chip synth,
  layered stems + intensity, beat-quantised stingers, tempo-up for sudden death; per-map/menu casting;
  render example -> target/music/medley.wav (send to owner). Plays via a Music bus in bd_audio.
- SHOUT WORDS (owner: "when they yell they get big dynamic juicy words announcing their construct"):
  OnoStyle::Shout / bd_fx shout.rs + present/shout.rs — phrase erupts as voxel letters from the ring
  hand, size from mic loudness / CAPS / "!!!" / construct scale, letters shatter and stream into the
  construct; replicated (filtered, <=48 chars) so others read it; comfort + "On/Mine only/Off" setting.
- Weight pass: bd_shapes half COMMITTED (bb3269e). Call sites still to switch together: bd_sim
  held.rs:252 swing_curve_weighted, ai.rs:495 attack_timing_hw, creature.rs:820, tests/creatures.rs:303;
  client present/constructs.rs:137/456, viewmodel.rs:428, bd_fx model weight + SecondaryMotion +
  footfalls + AnimState::Launched. Do after client perf + hitscan workers finish (file overlap).
If you are a fresh lead and these reports never reached you: inspect `git status`/`git diff`, run tests,
review galleries/screenshots, and commit what's green; re-launch anything half-done with a fresh brief.

## Queue (do next, in order)
1. **Weight pass (presentation half)** once client perf is done: DESIGN.md "Weight & power fantasy (pass
   2)" — tiered + asymmetric hit-stop, victim hit-shake, nudge-zoom, finisher flash (comfort-gated),
   directional sparks, scorch/crack decals, heavy-hit ducking, final-blow slow-mo, construct weight
   (footstep shake, summon events by size, held-weapon lag/aim shove), `assets/tuning/juice.ron` hot
   reload; wire the weighted timing fns into bd_sim held.rs/creature.rs so hitboxes match.
2. (Deprioritised by owner: no more showcase/video work — the game is the focus.)
3. **Soak investigation:** one soak failure at tick 156001 during concurrent edits didn't reproduce —
   run `soak --seed 1..8 --ticks 200000` on a quiet tree; reproduce by seed if any fails.
4. Full `tools/check.ps1 -Soak -Bench -Smoke` green on a quiet machine; fix regressions.
5. `pwsh tools/package.ps1` → dist/BrightestDay-win64.zip; smoke-test the zip on a clean folder
   (host + join over LAN); send the owner the zip path + a morning summary (what's playable, commands,
   known issues, what needs hands-on feel feedback: movement generosity, knockback/KO %, construct
   costs, hit weight — all hot-reloadable in assets/tuning/*.ron).
6. Powers backlog (docs/POWERS_BACKLOG.md): Naruto, DBZ, JJK, Zelda runes, HxH, misc — data-only.
7. Balance: re-run tournaments after humans play; bot data says KO% median ~70 (target 110–170) and
   99% bottom KOs — likely arena/void geometry + bot behaviour, NOT knockback; wait for playtest data.
