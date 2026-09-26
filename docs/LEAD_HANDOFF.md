# Lead orchestrator handoff (keep updated)

Owner: wants autonomous progress; this is a research comparison of orchestration harnesses. Read
docs/DESIGN.md first (NORTH STAR, MYSTERY tenet, FEATURE FREEZE, snappiness gates), then docs/COMBAT.md
(§1a Smash model supersedes health; §5h player powers incl. Origin kind; §5i balance), docs/PLAYING.md,
docs/CLIENT_INTEGRATION.md, docs/TESTING.md, docs/BALANCE_REPORT.md. Memory: brightest-day-direction.

## Process
- Workers = Opus subagents with strict file ownership; they never commit. BUILD POLICY (changed
  2026-09-26 midday): all workers build the SAME working tree, so they share ONE target dir
  (`target\play`, default features, CARGO_BUILD_JOBS=8; cargo's lock queues builds). Per-worker target
  dirs caused 5-6 concurrent full Bevy builds (~70 rustc) and hour-long waits. Small leaf crates
  (bd_music, bd_audio-only) may keep their own dir. Copy exes out before running them.
  Playable build for the owner: target\playable\brightest_day.exe.
- Lead: commit a checkpoint whenever `cargo check --workspace --tests` + core tests are green, so HEAD
  always builds (in-progress worker files included).
- Lead verifies (tests / galleries / screenshots / benches) and commits with the Co-Authored-By trailer.
- bd_core contract v1.4 (v1.3 Transforms, Recipe.grants; v1.4 `input::buttons::CHARGE`). Change only via lead; patch literal call sites.
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
  PHASE A COMMITTED (46b688a); the worker's full Phase B plan is in its report — resume the audio
  worker for Phase B. Check: brawl_after spectrogram shows near-constant saturated low band (wind/bed
  loops?) — verify by ear/metrics during Phase B.
- NOTE (2026-09-26 ~11:30-11:40): the owner accidentally played in a game window a worker had
  launched (perf/screenshot run). Treat perf numbers/screenshots from that window as possibly
  perturbed; owner asked NOT to rerun just for this.
- WARNING: HEAD does not build on its own — commit 36de637 picked up `mod alloc_count; mod pcm;`
  (main.rs) and `pub mod animated_terrain;` (present/mod.rs) whose files (+ rodio dep, bd_fx
  VoxelColors) belong to the uncommitted client perf work. Resolved when the perf work is committed.
- Oaths: owner supplied the Lantern Corps oath texts (assets/oaths.ron; DC's text — private use only,
  swap before any public release). Loader now takes string keys (Death kept for a future corps).
- MUSIC (owner: chiptune classical like "Chiptune Mozart: 8-BIT Classical Music"): new crate bd_music —
  own arrangements of PUBLIC-DOMAIN pieces only (never copy modern arrangements), band-limited chip synth,
  layered stems + intensity, beat-quantised stingers, tempo-up for sudden death; per-map/menu casting;
  render example -> target/music/medley.wav (send to owner). Plays via a Music bus in bd_audio.
- SHOUT WORDS (owner: "when they yell they get big dynamic juicy words announcing their construct"):
  OnoStyle::Shout / bd_fx shout.rs + present/shout.rs — phrase erupts as voxel letters from the ring
  hand, size from mic loudness / CAPS / "!!!" / construct scale, letters shatter and stream into the
  construct; replicated (filtered, <=48 chars) so others read it; comfort + "On/Mine only/Off" setting.
- CHARGE (owner: anime power-up): hold a key to slowly refill willpower (slower than the oath, ramps,
  can't act, slowed, interrupted by hits); replicated flag+level (bd_core v1.4); big corps-coloured
  aura, lifting debris, pressure ring, world dims <=6-10% (never HUD/chargers), comfort-gated; bots use it.
- CONSTRUCT ANIMATION (owner: "everything should give the illusion of being premade"): part ROLES
  (hatch/canopy, turret, barrel, rotor, piston, wing, jaw, head/eye, blade, thruster + wheel/seat/
  exhaust/cloth) tagged at generation + conservative inference for icons; role x semantic-event
  behaviours (aim/fire/move/pilot approach+enter/hit/charge/dismiss), role-ordered summon assembly
  (visual only, gameplay live from tick 1). SAME WORKER also switches the weight-pass call sites below.
  HANDS worker: viewmodel procedural springs + "disconnected" root cause (viewmodel.rs, viewmodel_anim.rs).
- BOUNDS RULES (owner): an attached tether prevents side/bottom/void KOs (only the star/top KO still
  applies); constructs leaving the map RECALL next to their summoner (anti-loop valve), plus an audit of
  every early-despawn path (worker: new bd_sim world/bounds.rs or recall.rs).
- TRACKING + MELEE ASSIST: ring-compass filament, lantern beacons >60 m, loud-action reveal pulses
  through walls, speed trails; melee lunge magnetism (sim, range/cone/LOS, cooldown), in-range weapon
  glow, hook-and-strike (tether hooks players, reel + one-shot slam; escapable; rules toggle).
  Same bounds worker also does visible summon placement + off-screen ping + tiny live preview camera.
- SIGNATURE MOVES (owner: "domain expansion called by name should give a dopamine spike"): invocation
  grammar + data `signature` tags; generic Domain mode (15-20 m sealed dome, element-transformed
  interior, sure-hit homing, big willpower cost, charge synergy); ultimate beams with windup.
  NO FREEZE (owner rejected taking control away) -> fighting-game "cut-in" DANGER alert via the shared
  PiP camera API (CutIn message in present/summon_ping.rs, built by the bounds worker); title letters
  via shout words.
- CAMERA: 1st/3rd person toggle as first-class modes (over-shoulder, collision, crosshair-truth aim
  convergence, "Allow third person" rule) + ADS in both (RMB with guns, accuracy only — movement
  untouched; ADS input bit after charge's in bd_core v1.4; AdsState resource read by the hands worker).
- Weight pass: bd_shapes half COMMITTED (bb3269e). Call sites still to switch together: bd_sim
  held.rs:252 swing_curve_weighted, ai.rs:495 attack_timing_hw, creature.rs:820, tests/creatures.rs:303;
  client present/constructs.rs:137/456, viewmodel.rs:428, bd_fx model weight + SecondaryMotion +
  footfalls + AnimState::Launched. Do after client perf + hitscan workers finish (file overlap).
If you are a fresh lead and these reports never reached you: inspect `git status`/`git diff`, run tests,
review galleries/screenshots, and commit what's green; re-launch anything half-done with a fresh brief.

## FEATURE FREEZE after the current wave (lead recommendation, 2026-09-26 afternoon)
Owner asked whether complexity is getting out of hand. Answer: finish in-flight work, then NO new
features — new ideas go to a backlog list. Order: (1) land + commit the wave, (2) stabilisation:
quiet-machine perf, `tools/check.ps1 -Soak -Bench -Smoke`, soak seeds 1..8, cross-feature bugs
(charge x hook x signature, recall x Orbit gravity, shout x assembly), (3) fresh build + short
"try these" list for an owner playtest (ideally 2+ players over the tunnel), (4) tune from feel.
Signature moves were RE-SCOPED to an amplifier over existing mechanics (no Domain mode in the sim).
Owner is actively playtesting the target\playable build and likes it; ideas from play (feel/polish,
like the gun grip, summon visibility, tether saves) are welcome anytime — the freeze is for new SYSTEMS.

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
