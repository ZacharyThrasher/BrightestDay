# Lead orchestrator handoff (keep updated)

## >>> CURRENT SNAPSHOT (2026-09-26 ~18:00) — read this first <<<
**Committed & green (HEAD builds):** feature wave (audio mixer + music/remixes, shout words, charge,
hands, camera/ADS, construct animation + assembly, bounds/recall/placement + cut-in preview, tracking/
lunge/hook + compass, double jump/dodge/wall refresh/surf, oath sections + voice parity, freedom bundle,
lexicon relations, signature moves + Hollow Purple, combat.ron live + host sync, outlines, Orbit,
Backrooms, Sakura Keep), Stabilisation A (keymap/settings tabs/settings-save fix) + B (3P/aim/readability),
release-to-fire fix, costume preview rotation. bd_core v1.6; bd_net PROTOCOL_VERSION ~14 (map workers keep
bumping — just keep it compiling).
**Owner builds:** target\playable\0926_1654_voice\brightest_day.exe (fastrel + VOICE); zip
dist\BrightestDay-win64.zip (146 MB, voice); target\playable\LATEST.txt = newest verified. Voice builds
need `--features voice` in their own dir (target\voice).
**Lead background processes (restart after reboot):** `pwsh -File target\buildwatch.ps1` (GREEN/RED →
target\build_status.txt every 90 s) and `pwsh -File target\promote.ps1` (every 30 min: GREEN → fastrel
+voice build in target\voice → startup smoke → target\playable\<stamp>\). Lead checks: CARGO_TARGET_DIR
target\lead. Workers: target\play + `--profile fastrel`, BD_HIDDEN_WINDOW=1. Protocol: docs/COORDINATION.md.
**In flight (subagent ids; reports return to this session):**
- Connect feel a6409611d34036099 — hit assist (melee volume+magnetism, projectile/hitscan magnetism),
  weight-pass-2 juice (hit-stop, contact resistance, camera punch, sparks, KO slow-mo; juice.ron),
  LUNGE fix (owner can't feel it: measure trigger rate, loosen, make it felt), railgun early-release shot.
- Anti-juggle a98da5aecac02be4b — knockback decay + short immunity; also owns the FFA hash A/B
  (currently 0xd1a37fca9b34c73d failing — cause is juggle or casino edits).
- Signature a67f94557763852e1 — ENERGY ATTACKS = HAND BEAMS (final flash/kamehameha/galick… no gun
  model; 2-hand charge orb + palm beam 1P/3P; canonical title when recognised).
- Intent aafbfbcb5ee31d5c1 — name targeting bug ("drop a meteor on ethan"), whisper bias w/ player
  names, delivery verbs, ~1,000-phrase golden corpus, stage order, local logs/intent.log + intent_report.
- Giants a01a81cbdbb91cd1e — weak points, tether+ride giants (moving platforms), limb sever/topple,
  dodgeable attacks, summoner command; optional hijack.
- Powers a8211d8337e51dbfe — DBZ/Naruto/JJK/Bleach/HxH powers + generic effects (AutoDodge, slow field,
  self-drain, crit, read cue, tiers) + ONE generic "Ascend" transformation sequence driven by data.
- Character costumes abe83e88bc70188b0 — style library + ~150 iconic looks (gojo/vader/...) + icon-derived
  fallback; gallery docs/costume_gallery.png is the bar.
- Maps (isolated behind bd_voxel features; register at the end): Cybertron a2b9a832591c61404 (transforms
  ADD routes only), Turtle Lair adf4a2d95abe323d2, Giant's Room ae3814d9e0a766ccd (static), Valley of the
  End a39d259b62dd1312d (water-walk as flavour), Ember a12534acdde2f59c8 (lava = void), Hyperline
  a2bf846d827b78dcb (train, slipstream), Mirrorfold a319a5a9539efed5b, High Roller casino
  a668e5a109aacf9fa (optional slots/roulette; its routes example was missing mid-work).
**Known follow-ups:** 3P ADS head fills ~1/3 of screen; Alt+Enter also dodges; bots don't navigate
mazes; possible tutorial first-construct pipeline hitch; bd_net InputQueue trim can drop a 1-tick tap;
Sakura's bd_sim `sakura` feature not default; menu backdrop may ignore prefs map.
**Owner rules:** maps = flavour that ENHANCES, never restricts; voice == typing; never freeze players;
never say no (sputter); mystery tenet. Owner playtests with friends and sends feel notes — prioritise them.
**Next after in-flight lands:** quiet-machine perf + `tools/check.ps1 -Soak -Bench -Smoke`, soak seeds
1..8, fresh voice zip + "try these" list, then run intent_report on the owner's logs/intent.log.
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
- With many workers: follow docs/COORDINATION.md — lead runs `target\buildwatch.ps1` (writes
  target\build_status.txt GREEN/RED + owning worker every ~90 s), hot shared files are claimed via
  target\locks\*.lock, sequence integration-heavy work behind the features it depends on.
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
- DOUBLE JUMP: one air jump (restored on land / tether attach / being hit), keeps momentum with a small
  wish-direction redirect but NEVER adds horizontal speed; never fires from held-jump bhop; hard-light
  step burst; bots use it for recovery.
- OATH SECTIONS + VOICE: each authored line of oaths.ron is a section; client rotates sections,
  OathBegin{section} (sim looks up length from its own table). Voice: hold V in the overlay, or just
  yell your oath via push-to-talk (>=0.7 similarity -> recital, checked before casting). OWNER RULE:
  voice == typing in skill ceiling (speed bonus from speaking time, same formula). FIXES a bug: sim OathTable parser failed on the new string-keyed oaths.ron
  and silently fell back to 110 chars.
- DODGE (double-jump worker): short burst, ~0.12-0.15 s intangible, ground cooldown, one air dodge per
  airtime, never raises sustained speed, breaks hook-and-strike.
- PLAYER OUTLINES + chaos readability audit (inverted hull, distance-scaled, LOS only, player/construct
  separation, subtle high-% pulse, settings).
- CONSTRUCT ANIMATION DONE (bd_core contract now v1.5: PartRole + Hatch/Turret/Barrel/Rotor/Piston/Fin/
  Thruster/Eye/Blade/Grip/Support). Weight call sites switched (viewmodel.rs line left to hands worker).
  TODO lead: wire shout letter voxels into the assembly beam — `bd_fx::ConstructAssembly{ring,
  next_world,next_in,...}` + shout's `Shout::stream_to`. SFX wishlist: assemble_click, assemble_lock,
  hatch_open, hatch_close, rotor_spinup, exhaust_puff, kaiju_step, footstep_heavy.
  Two bd_sim tests fail from in-flight world/* work (flying_cage_*, opposing_constructs_clash*) — recheck.
  SHOUT WORDS DONE; open design call: remote shouts draw on top through walls (~1 s) — world_on_top.
- LEXICON RELATIONS (lexicon worker resumed): size relations ("T-rex sized duck", "size of a house"),
  magnitude table (nuclear/planetary/micro), mount/riding ("King Kong on a skateboard" -> mount attached
  + locomotion; maybe bd_core v1.6), EMITTER relation ("satellite death beam" -> satellite is the
  construct, beam is its attack), sky delivery + volley words (meteor shower, rain of arrows).
  Sim side of the orbital strike (hover high above aim, ground telegraph, tracking beam with slew cap)
  -> signature worker. Resolve cost is per-cast (~0.1-10 ms), not per-frame.
- FREEDOM BUNDLE (owner-approved): "it" = last construct ("make it bigger", "again!", graft path),
  player-name targeting ("missile at Jake"), never refuse for willpower (sputtering scaled-down version),
  yell/CAPS energy -> up to ~+15% (voice == typing), HIDDEN legendary rolls whose chance scales with
  novelty (match-wide unseen phrase + composition richness + personal variety; repeats ~0; never shown),
  kill feed with the exact phrase, end-of-match highlights. Leftover words -> light grafts (lexicon worker).
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

## Third-person gaps (camera worker report) — fix in stabilisation
(a) costume head fills the view when the camera is < ~1 m (fade or auto-first-person); (b) no 3P
ADS/gun-raise body pose, held item on a fixed hand offset, railgun oversized at head height;
(c) "Allow third person" is client-enforced only — enforce on host; (d) power moves (G) use the
uncorrected look in 3P; (e) projectile muzzle parallax ~0.3 m point-blank; (f) tutorial map doesn't
draw the local body in 3P; (g) DESIGN.md still frames 3P as comfort-only; (h) re-test `--bench` with
a hidden window. Protocol is v7 (S2C::Tuning).

## Integration cleanups to do ONCE after the wave lands (don't let 8 workers fight over them now)
- Keybind audit (charge, dodge, view toggle, ADS, shoulder swap, hook) — no conflicts; update PLAYING.md.
- Settings menu reorganisation into tabs (Gameplay / Controls / Video / Audio / Comfort / HUD) — ~8
  workers each added rows to menu.rs/prefs.rs.
- bd_core contract + bd_net PROTOCOL_VERSION reconciliation (multiple bumps: v1.4 charge/ADS bits,
  v1.5 roles, maybe v1.6 mount); CLIENT_INTEGRATION.md contract notes.
- SFX: docs/SFX_WISHLIST.md is the consolidated cue list (audio worker, one sfxgen pass).
- Owner "try these" list + fresh build. Lead runs target\promote.ps1 (every 30 min: GREEN -> release
  build -> startup smoke -> copy to target\playable\<stamp>\, LATEST.txt, promote.log).

## Queue (do next, in order)
0. After the wave + stabilisation: BACKROOMS map (bd_voxel maps/backrooms.rs): 4-6 stacked levels of liminal room kits (office maze, poolrooms, parking garage, hotel corridor, dark playground, impossible sky room), pits to the void = KO, roof = star KO, punch-through floors; perf via room/interior culling + haze, emissive ceiling panels instead of many point lights, capped instanced decor props from the icon library; fluorescent hum ambience (bd_audio maps.ron). Same process as Orbit (preview, routes, bench).
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

- RESOLVED: FFA hash 0x1fab383f88dfc29f re-recorded by the double-jump worker (bots air-jump/dodge; A/B verified). STILL OPEN: flying_cage_* and opposing_constructs_clash* failures — recheck in stabilisation.
- DOUBLE JUMP + DODGE DONE: Left Alt = dodge (lead moved gun alt-fire to C; Caps Lock rejected). No air jump while tethered. OWNER: wall touch DOES refresh the air jump/air dodge (surf/wall-kick emergent play) with a tunable same-wall lockout. TODO: regenerate tutorial peak_run ghost (examples/tutorial_ghosts.rs) — its apex jump press is now an air jump.
- LEXICON RELATIONS DONE: data in crates/bd_lexicon/data/relations.ron (compiled in, no bin rebuild); bd_core v1.6 Recipe.mount (one level); T-Rex now 12 m; sky-emitter / strike / volley recipe shapes documented in the worker report (see signature worker).
- SURFING (owner: 'go with what feels best'): surf-only air-accel on unwalkable steep surfaces (off-surf bit-identical), real surf lines on Archipelago/Miami/Orbit (indestructible), 'surf ramp/half pipe' constructs; key risk = 0.5 m voxel staircase vs smooth slope collision. Double-jump worker resumed.
- HOLLOW PURPLE = flagship signature acceptance test (owner: 'key one'): red+blue merge windup, 3-4 m erasing sphere, tunnel, huge knockback, cut-in; scripted scenario + frame strip required. Also bare 'red'/'blue' -> element tints (not a Bolshevik biped). BACKROOMS map: IN FLIGHT in isolation (bd_voxel feature ackrooms, own target dir target\backrooms; registers MapKind only at the end).
- BOUNDS/RECALL/PLACEMENT DONE (22 tests). Stabilisation note: thrown non-returning held items are still 'Spent on landing' (held.rs) — consider lingering/recall-to-hand per the owner's 'summons shouldn't disappear right away'. Client helper (summon ping/preview/CutIn) has not reported perf/screenshots — verify.
- SURF DONE: lines are smooth hard-light colliders (ArenaInfo::surf_lines), not voxels. surf_air_accel has NO effect at 120 Hz (normal accel already saturates the 0.8 m/s wish cap); the real feel knob if owner wants more carving = a surf-only wish cap. Orbit has no line yet. Ridge lip ~0.75 m at the very top.
- AUDIO PHASE B DONE (e07e1e7): all sound via bd_audio; Settings -> Sound (master/music/world/ambience/feedback, style, reduced range, mono); BD_PERF_OFF=audio A/B. Unwired cues to hook in stabilisation: summon_ping (summon_ping.rs), view_toggle (camera toggle), rotor_loop (construct_anim after rotor_spinup); sudden_death music flag has no overtime signal yet. Output latency limited by WASAPI shared mode (~22 ms callbacks).
- FREEDOM BUNDLE DONE. Open: power pickups (powers/exec.rs power_pay) still refuse for willpower; viewmodel NotEnoughWillpower flicker is now dead for casts; note a sputter at 0 willpower starves other constructs (existing upkeep rule) — watch in playtest.
- OUTLINES DONE. Readability follow-ups: (1) fade/shrink opaque shatter debris within ~1.5 m of the camera and cap shatter cube size near players; (2) lower daylight construct alpha (0.92) near player bodies so players inside enemy domes read; (3) judge dark flecks on high-damage bodies. Uncapped --bench for outlines still owed. target/outline/rec is ~550 MB (prunable).
- TRACKING/MELEE DONE (f87b4f0) incl. a real bug fix: melee sweeps ignored the hand frame (hitboxes near world origin) — all melee changed. CRITICAL FOUND: combat.ron was never loaded by bd_game (only move.ron) — worker launched to load + hot-reload + host->client sync + whip sweep density. Design notes: hooking a PLAYER while falling counts as a tether save (kept as emergent rescue); victim tug arc at screen edge is on (tracking.ron tug:false to remove); ring-face glow dropped (needs hands' ring position).
- COMBAT TUNING NOW LIVE: crates/bd_game/src/combat_tuning.rs loads assets/tuning/combat.ron (practice/host/headless/soak), hot-reloads every 0.5 s, host pushes S2C::Tuning to clients; defaults == shipped (strict parity test). Guns had been running at x1 cost until now. Whip samples <=0.75 m. Note: held.rs/tests/held.rs/tests/tuning.rs are now LF (harmless).
- CUT-IN PREVIEW PERF FIXED (8 Hz offscreen image, +0.13 ms avg). TODO: summon_ping.rs module doc still describes the old viewport approach.
- HOLLOW PURPLE JUICE DONE. TODO: in 3P the 3.5 m orb gathers only 3 m + radius ahead of the eye and fills the screen during windup — push the gather point further out (one line in bd_sim signature orb).
- BACKROOMS DONE (--map backrooms, host picker): 5 storeys, 10 spawns, void pits L0, breakable roof/windows, sky room, 1.00M tris, ~+1 ms p99 vs Archipelago on a busy box. Protocol v8. Gaps: bots don't navigate mazes (beeline through walls); no pool-drip sound; ambient light is the default blue (per-map ambient would need client code).
- SAKURA KEEP map (owner: 'based on hanamura'): OWNER: follow Hanamura's LAYOUT closely (private use; note in THIRD_PARTY_NOTICES to replace before public release) with OUR OWN look/name/props (no Blizzard assets/signage) — gatehouse streets -> bridge chokepoint -> sakura courtyard/shrine -> temple -> castle keep with tall lantern-lit interior; rooftop routes, surf lines, void ravine. Isolated behind bd_voxel feature sakura, own target dir; registers at the end (protocol bump).
- CYBERTRON map (birthday-gift fan target): Iacon (Hall of Records with Prime statues) vs Kaon (gladiator pit), Well of AllSparks (void KO), space bridge (Portal pair), transforming bridges/platforms (terrain animation, telegraphed), vehicle-mode highways, Sea of Rust edge, fallen colossal robot, energon everywhere, space sky + low gravity up high. Private-use IP note in THIRD_PARTY_NOTICES. Isolated behind feature cybertron.
- TURTLE LAIR map (--map lair): vaulted sewer lair (sunken bean-bag pit under a manhole light shaft, dojo/Splinter corner, lab/garage, arcade + skate half-pipe), sewer tunnels with water + pipes, manholes up to NYC rooftops (void/star KOs). Private-use IP note. Isolated behind feature lair. Three map workers (sakura, cybertron, lair) will each register MapKind + bump protocol — reconcile PROTOCOL_VERSION when they land.
- GIANT'S ROOM map (--map giant): toy-scale (~x30) kid's bedroom — carpet tufts, pencil bridges, bed trampoline, bookshelf climb, desk plateau, toy race track with banked surf curves, block castle, toy box pit, rotating ceiling fan platforms, curtains to swing, sleeping giant cat; sunbeam + dust + haze for scale. Isolated behind feature giant.
- VALLEY OF THE END map (--map valley): gorge + central waterfall (plunge = void), two ~55 m facing statues (destructible limbs, indestructible cores), map-level water-walking (ArenaInfo water_walk; sim change w/ tests), waterfall face refreshes air jump, canyon surf line, mist. Private-use note. Isolated behind feature alley. FIVE map workers now in flight — reconcile protocol bumps + MapKind registration as they land.
- EMBER map (--map ember, Mustafar-esque): lava = burn + upward launch (Smash Norfair-style, not a KO), periodic telegraphed lava surge, water/ice -> temporary obsidian bridges, drifting mining platform, timed geysers, twin-spire fortress, lavafall void edges. Generic hazards module in bd_sim (world/hazards.rs) any map can declare. Private-use note. Isolated behind feature mber. SIX map workers in flight.
- OWNER DIRECTION (maps): no map-specific mechanics ('gimmick mechanics can feel bad if they are the central point of the map'; 'competitive is good'). All 6 map workers told: drop lava surge/geysers/obsidian/drifting platform (Ember: lava = void KO), water-walking (Valley), transforming terrain + portals (Cybertron), fan/trampoline/cat sim (Giant's Room); no new sim code; focus on fair spawns, routes, sightlines, no camping perches.
- OWNER UPDATE: Cybertron transforming bridges + space bridge and Valley water-walking are BACK as flavour (optional, off main lanes, telegraphed, deterministic). Ember lava surge/geysers/obsidian/drifting platform and Giant's Room fan/trampoline/cat stay cut.
- MAP RULE (owner, verbatim): 'unique flavor = good, gimmick mechanic that forces the players to play around it = bad; the flavor should ENHANCE their current abilities, not restrict them.' Cybertron transforms must only ADD routes.
- HYPERLINE map (--map train, Spider-Verse chase-inspired): static train (loco + ~8 cars, hollow interiors, gaps), infinite scrolling backdrop (vertical city shaft, ring gates, speed lines; visual only, comfort-gated), SLIPSTREAM rule (none on roof, gentle drag airborne, strong sweep outside the footprint -> KO; tether save applies) — enhances knockback. Isolated behind feature hyperline. SEVEN map workers in flight.
- MIRRORFOLD map (--map mirror; Inception/Mirror Dimension-inspired): normal gravity; streets curl up into huge surf-line half-pipes, inverted skyscrapers hang within tether range, floating kaleidoscope fragments, rolled-up city backdrop (visual). No new sim code. Isolated behind feature mirror. EIGHT map workers in flight — suggest owner pauses new maps until these land.
- HIGH ROLLER casino map (--map casino): Vegas floor, giant roulette-table arena (static, surf lap), chandeliers, pool-deck rooftop. OPTIONAL gambling via a small generic interactables module: slot machines (hit/tether the lever, willpower stake, party-effect payout / jackpot = guaranteed legendary next cast / harmless bust; odds hidden) + roulette 'house spin' bonus pocket. Isolated behind feature casino. NINE map workers in flight.
- In flight: ANTI-JUGGLE worker a98da5aecac02be4b (knockback decay within a window + short knockback immunity after N hits; tunable in combat.ron); COSTUME ROTATE worker a6035c30f82aca87b (drag/wheel orbit of the costume preview). Voice-enabled fastrel building in target\voice (owner saw no voice options: the playable/zip builds lacked --features voice). Zip: drop the unused 148 MB assets\models\ggml-base.en.bin (only q5_1 is used).
- In flight: RELEASE-TO-FIRE worker (owner: charged hit needs an extra click; sim intends release — root-cause in input/net/prediction or other charge actions).
- VOICE BUILDS: build with `--features voice` in its own CARGO_TARGET_DIR (target\voice, ~14 min cold). dist\BrightestDay-win64.zip (146 MB) = voice fastrel + assets (unused ggml-base.en.bin dropped; q5_1 kept) + cloudflared. promote.ps1 now builds fastrel WITH --features voice in target\voice (restarted 16:55).
- In flight: CHARACTER COSTUMES worker (owner: 'if I type gojo I should look like gojo'): voxel style library (hair/face/helmets/masks/outfit cuts/accessories), ~150 hand-authored iconic character looks with variants in assets/costumes/characters.ron, auto-derived costumes from character icons as fallback; gallery docs/costume_gallery.png is the quality bar.
- COSTUME ROTATE DONE (lobby/costume_view.rs; drag/wheel/R/double-click, idle turntable off in comfort). Note: menu backdrop may not follow prefs map. Lead asked sakura worker to restore menu.rs LF endings; commit after that.
- In flight: CONNECT FEEL worker (owner: 'slapping noodles, hard to hit; getting hit feels good, make connecting easier + more rewarding'): hit assistance (wider melee volume + swing magnetism, projectile/thrown light homing, hitscan magnetism, lunge range) + the never-built WEIGHT PASS 2 presentation half (tiered asymmetric hit-stop, contact resistance on the viewmodel/swing, attacker camera punch, directional sparks, victim flash/shake, layered impact audio, KO slow-mo; assets/tuning/juice.ron).
- STABILISATION B DONE (host aim clamp when 3P disallowed, G powers aim, projectile muzzle convergence, 3P held items in costume hand + gun raise + reach cap, near-clip fade, tutorial 3P body, HP gather 8.5 m, debris near-camera shrink, dome alpha around bodies, words off the sight in ADS, unaffordable pickups sputter, thrown items linger 4 s, peak_run ghost regenerated, ring sputter flicker). FOLLOW-UPS: 3P ADS — the head/body fills ~1/3 of the screen (fade body during ADS or widen shoulder offset); possible first-construct pipeline hitch in the tutorial (warm up); DESIGN.md 3P framing (worker A may have fixed); FFA hash moved (juggle worker A/B-ing). Protocol now v12.
- In flight: signature worker a67f94557763852e1 resumed — ENERGY ATTACKS ARE HAND BEAMS (owner: 'final flash gives me a super powered gun, not the iconic beam'): no gun model for kamehameha/final flash/galick gun/special beam cannon/spirit gun/hadouken/etc.; two-hand charge orb + thick palm beam in 1P and 3P; release-to-fire; canonical title letters when recognised. TODO: menu.rs committed with CRLF in d357e20 — normalise to LF when its lock is free.
- Owner: 'I don't even feel the lunge / I just end up spam clicking' — sent to the connect-feel worker: measure trigger rate + failing condition, loosen (cone ~30-35 deg, sword ~6 m, combos, 0.5 s cooldown reset on hit), make it FELT (FOV punch, speed lines, whoosh, viewmodel thrust, dash streak).
- SAKURA KEEP DONE (--map sakura): Hanamura floor plan traced (tools/maps/sakura_trace.py -> sakura_layout.rs), ~1.4x scale, keep hall + tenshu, ravine void, 2 surf lines, 877k tris, 10 spawns. bd_sim sakura feature NOT default yet (route example needs --features sakura). No chimes/bell/fountain SFX yet (wishlist). practice.rs --pos fix.
- RELEASE-TO-FIRE DONE (root cause: charge release never emitted SwingStart -> viewmodel/3P froze until next click; fixed held.rs + viewmodel local release + 3P hold_at). Railgun early release -> weaker scaled shot (sent to connect-feel worker). Rare bd_net InputQueue trim can drop a 1-tick tap (not fixed). casino_routes example missing broke cargo test -p bd_sim examples build (casino worker in progress).
- In flight: INTENT worker aafbfbcb5ee31d5c1 — name targeting root cause + delivery (drop/throw/summon/rain onto the named player), whisper bias with player names, delivery verb table, ~1,000-phrase golden intent corpus with per-category accuracy, enforced stage order, local-only logs/intent.log + intent_report tool.
- In flight: GIANTS worker (owner: summons are 'giant death balls'): AoT-style weak points (nape/core/eye) + stagger, tether onto and RIDE giants (moving anchors + moving-platform carry, parity), per-limb damage/sever/topple, readable dodgeable attacks (stomp shockwave jumpable, swipes dodgeable, no ambient contact launches), summoner command via crosshair/name target; optional hijack.
- In flight: POWERS BACKLOG worker (owner approved): generic power effects (AutoDodge, ProjectileSlowField, SelfDrain+PowerMult, CritChance, ReadSlow cue, tiered transforms) + DBZ (ultra instinct, super saiyan tiers, kaio-ken), Naruto (sage/kurama/sharingan/rinnegan/clones), JJK (limitless/infinity, black flash, shrine, ten shadows), Bleach (bankai, getsuga), HxH (nen types, jajanken) in powers.ron; phrases resolve to POWERS not constructs.
- Powers worker also builds ONE generic 'Ascend' transformation sequence (power-up pose, aura in colour/intensity, burst/crater/flash, title letters, cosmetic hair/eye tint hook, linger), driven by scend: data on every transform power (incl. existing Zoan/gamma/phoenix/symbiote/shazam/lantern).
- EMBER DONE (--map ember; lava = void KO sheet, facility + collector arms, fortress + spires, 2 surf lines, 614k tris, 9 spawns). Follow-ups: lava hard to read from low plains at distance; ash/heat shimmer/lava splash KO FX need client code; ArenaInfo has no Default so every new field patches every map literal (lead: add Default + ..Default::default()).
