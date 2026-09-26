# Brightest Day — Design (north star)

A native (Rust + Bevy) **1–8 player free-for-all voxel arena fighter**. Every player wears a lantern
ring. You move like Quake/Source (air-strafe, bunny-hop, slide) and swing like the Attack on Titan fan
game (dual tethers, reel, gas boost). You fight by **constructing**: type a word — `hammer`, `mjolnir`,
`parachute`, `godzilla`, `pikachu`, `captain america's shield` — and your ring builds it out of hard
light voxels in your corps colour. Everything shatters into cubes.

> **Owner's standing priority:** gameplay, mechanics, animation and functionality >>> art fidelity.
> This is a fighting/party game that must primarily **FEEL** good, and **player skill expression is the
> highlight**. When time is short, spend it on feel, responsiveness, readable animation (anticipation,
> follow-through, squash), and mechanical depth — never on visual detail. Simple voxel shapes + great
> motion beat detailed models + stiff motion.

> **NORTH STAR (owner):** "The user experience has to feel good; we are trying to create the illusion
> that the only thing limiting them is their mind — not the game. I want them excitedly yelling out
> anything they can think of." Consequences: the ring **never says no** (every phrase yields something
> sensible, instantly — forming starts on the same frame); failures are invisible or charming, never
> an error; the "≈" note is brief and delightful, not a correction; voice (push-to-talk) is a
> first-class way to play, not a gimmick; unresolved/fallback phrases are logged locally
> (`logs/unresolved.txt`) so every playtest grows the vocabulary.
>
> **FEATURE FREEZE (2026-09-26).** No new *systems*. Growth only as data (icons, powers, words) that
> composes existing closed-set pieces. Core = movement + ring constructs + destruction + FFA; every other
> layer (Transformers, player powers, party mode, voice, costumes, onomatopoeia) is a lobby toggle and is
> hidden rather than shown half-done. Priority from here: vertical slice → playtest → polish → breadth.
> Any proposed new verb must pass the mechanic admission test (COMBAT.md §5f).

## Pillars (in priority order)
1. **Feel.** Snappy, tight, low latency. Mouse look is applied every rendered frame from raw input,
   never tick-quantised. Movement is simulated at a fixed 120 Hz with render interpolation. No
   floatiness: heavy gravity, crisp acceleration, weighty impacts (hit-stop, shake, FOV punch).
2. **Freedom of expression — two co-equal axes: movement and construct creativity.** Mechanically
   skilled movement is the strongest movement (flying constructs are slower than a well-executed
   bhop → tether → swing chain). Equally, *what you build* is skill: phrases compose mechanics
   ("a flaming hammer with rockets that returns"), unusual combinations produce genuinely new
   behaviour (never just bigger numbers), counters have qualitative outcomes, and systemic
   interactions let players invent combos. Neither axis dominates (see COMBAT.md §0).
   Any phrase can become a construct; the same phrase is always the same construct (learnable).
3. **Impact.** Terrain, trees, towers and constructs are voxels and break into physical debris cubes.
   Explosions leave craters. Clashes should look like a fight between Green Lanterns.
4. **Focus.** Minimal on-screen text. Health and willpower are shown diegetically (ring glow,
   first-person hand, screen-edge light fractures; other players' aura). No AI/LLM anywhere.

## Vocabulary vs. mechanics (the core architectural rule)
- **External knowledge explains words.** ConceptNet, WordNet/OMW, dariusk/corpora, and Pokémon data
  (PokeAPI CSVs) are processed *offline* by `tools/lexicon_build` into `assets/lexicon.bin`. They tell
  us that *maul ≈ hammer*, *parachutes slow falls*, *claws grasp*, *scorpions have tails*,
  *Charizard is a fire/flying dragon-shaped thing, 1.7 m, 90 kg*.
- **Brightest Day alone decides physics.** `bd_core::mechanics` defines a closed set of mechanic tags
  (`Hammer`, `SlowFall`, `Grab`, `ArticulatedTail`, …) and their parameters. `bd_sim` implements what
  each tag *does*. The lexicon never contains behaviour, only tags + parameters.
- **We construct like the player.** A construct is a `Recipe`: a body plan + mechanic tags + scale +
  element + name. Voxel shapes are generated procedurally from the recipe (plus a small set of curated
  icon models for pop-culture items). Unknown words fall back gracefully via WordNet hypernyms
  ("sledge" → hammer) and ConceptNet relations; truly unknown words make a generic glowing blob with a
  flash of "?" — never nothing.
- **Vast by default.** Goal: nearly any phrase a player types or says resolves to *something
  sensible*. Layers: curated icons → Wikidata named entities (~150k–500k characters, creatures, species,
  breeds, vehicles, weapons, landmarks, each mapped via its "is a" chain: Eeyore → fictional donkey →
  donkey → Quadruped) → ConceptNet/WordNet words (every concrete WordNet noun gets a default body from
  its category) → morphology/slang ("shootin", "GIANTTT") → compound split ("sharknado") → phonetic/
  fuzzy → description head noun → blob. Phrases compose: size, count, element, "with/that/of" parts
  and mechanics, and a recursive **payload** ("eeyore shooting laser beams", "cannon that shoots
  anvils", "godzilla breathing bees"). A tiny note ("eeyore ≈ donkey") teaches players what the ring
  understood.
- **Pop culture is data.** `assets/icons.ron` curates iconic things (Mjolnir: Hammer+Returning+Electric;
  Cap's shield: Shield+Returning+Ricochet; Godzilla: Kaiju biped+ArticulatedTail+Breath(atomic);
  lightsaber, batarang, web-shooter, Pokéball…). All ~1000 Pokémon come in automatically from
  PokeAPI data (shape → body plan, types → element, height/weight → scale/mass).

## Impact (designed from scratch — every hit is a small event)
A hit communicates on five channels at once, scaled by a single `impact` scalar (≈ momentum
transferred) so small hits stay small and big hits are *huge*:
1. **Time** — hit-stop freezes attacker + victim (and their constructs) for 30–140 ms scaled by impact;
   the rest of the world keeps running so it reads as *contact*, not lag. Local-only, render-side.
2. **Body** — victim gets knockback along the hit vector with guaranteed lift; high-impact hits put the
   victim into a *tumble* state (reduced air control, spinning debris trail) until they touch ground or
   tether. Tumbling into a wall = *wall splat*: bonus damage, a voxel crater at the contact, a bounce.
3. **World** — anything the hit or the flying victim touches breaks: voxels are carved and become
   physical debris cubes that inherit the hit's velocity (not a generic puff). Big hits shockwave
   loose debris outward. Explosions carve craters; kaiju footsteps crack the ground.
4. **Light** — constructs are hard light. On clash, both constructs flash white, emit a ring of
   their-colour shards, and lose integrity; a construct at 0 integrity *shatters* into its own voxels,
   which tumble, glow, and fade to nothing over ~1.5 s. Two constructs colliding is itself a clash.
5. **Camera/audio** — directional screen shake (along hit vector, very short), brief FOV punch, a
   chromatic/white flash for the victim, low-frequency thump + crack layered by material. Your own
   hits must be as satisfying to *give* as they are dramatic to *receive*.

### Juicy text (hard-light onomatopoeia)
Grounded in "Juicy Text" (ICMI '24; vault: JuicyText_SupplementaryMaterials): text + particles is the
most appealing feedback, text alone is dull, words convey material well. So big impacts spawn a
comic word at the contact point, built from voxel letters in the attacker's corps colour — pop,
wobble, hold ~0.4 s, then **shatter into cubes** with the impact debris. Word chosen by material /
mechanic / element / weight (KRAK, SPLOOSH, ZZAK, VWOOP, KRANG, DOOM…; data table
`assets/fx/onomatopoeia.ron`), size by impact. Performance words where timing is skill: PERFECT (perfect
bhop, perfect oath), GOOD / SLOW (oath grades), PARRY!. Manga mode (ドドド, ゴゴゴ). Settings: Off /
Big hits only (default) / Full comic; rate-limited; world-space, never HUD clutter.

### Hit & KO confirmation (lessons from Valorant's feedback loop)
- **Crit layer:** players have a head zone (×1.5 for guns, ring blast, thrown objects; melee exempt);
  construct weak points share it. Crit = a crisp high-frequency snap layered over a dull low thud —
  cuts through any mix without fatigue — plus a stronger crosshair flare.
- **Clean KO:** the KO'd player shatters instantly into their corps-coloured voxels (no lingering
  ragdoll; threat silhouette gone at once); their construct loops cut off abruptly.
- **Negative space:** on a KO you dealt, the rest of the mix ducks ~250 ms so the streak note rings.
- **Release:** KO blows get ~80 ms extra hit-stop on top of the normal amount.
- **Crosshair flare:** subtle pulse on hit, stronger on crit, corps-coloured bloom on KO — the only
  2D confirmation. No kill banners/skulls; no long finishers.

### KO streak sounds (per-corps "skinline")
Each KO without dying plays the next step of a rising 5-step minor-pentatonic phrase (step 5 resolves
with a flourish and repeats from then on); dying resets. Same musical structure for everyone, **distinct
timbre per corps** (Willpower bell-synth, Rage distorted saw, Greed coin chimes, Fear detuned glass, Hope
choir pad, Compassion mallet chime, Love FM harp, Life sine choir) — you can hear *who* is streaking.
Accents: void KO, double KO (≤ 1 s), shutdown (ending someone's 3+ streak). Visual stays minimal: ring
flares brighter per step + one corps-emblem pulse near the crosshair; no banner text.

## Game rules (FFA)
- 1–8 players, one arena of floating Cube-World-style voxel islands over a void.
- **Health 100.** As health drops, **knockback taken increases** (Smash-style). KO by health reaching 0
  **or** falling into the void / out of bounds. Respawn after 3 s at the spawn farthest from enemies
  with 1.5 s spawn shield. Match: first to 10 KOs or 8 minutes; last hitter within 5 s gets void KOs.
- **Willpower** (0–100) powers constructs: cast cost scales with recipe size/power; summons and held
  weapons drain upkeep. At low willpower, recite your **corps oath** (a typing test, the Lantern oaths
  from the vault image) to refill; speed and accuracy set the refill. You're vulnerable while reciting.
- **Corps = player colour.** Willpower green, Rage red, Greed orange, Fear yellow, Hope blue,
  Compassion indigo, Love violet, Life white (8 players ⇒ 8 corps). Constructs glow in your colour.
- **Always-available ring blast** (LMB with no construct held): quick light bolt, low damage, some
  knockback — so nobody is ever helpless.

## Controls (defaults, rebindable later)
| input | action |
|---|---|
| mouse | look (raw, per-frame) |
| WASD | move (Quake/Source accel; air-strafe gains speed) |
| Space | jump (hold = auto-hop within buffer window) |
| Ctrl | crouch / slide at speed |
| Shift | gas boost (meter) |
| Q / E (and Mouse4/5) | left / right tether — hold to stay attached, release to fling |
| Mouse wheel / R | reel in while tethered |
| LMB / RMB | use construct primary / secondary (ring blast when empty) |
| Enter or T | open ring console: type a construct, Enter to cast (mouse aim + tethers stay live; momentum preserved) |
| 1–4 | quick-cast loadout slots (words chosen in lobby) |
| V (hold) | *experimental* voice cast: say the phrase, release to cast (local Whisper on CPU → same lexicon resolver) |
| F | recite oath (willpower recharge typing test) |
| X | dismiss your constructs |
| Tab | scoreboard |

## Motion comfort (a playtester gets motion sick)
Defaults for everyone: the camera **never rolls** (tumble spins the body, not the view), no motion blur,
no head-bob, stable high fps, an always-visible small centre dot. Settings → **Comfort preset** (one
click; each item individually adjustable): camera shake off/slider, speed-FOV and FOV punches off,
landing dip/camera kick off, **comfort vignette** (edge darkening scaled by speed/swing angular
velocity, strength slider), wider default FOV, screen flashes off (also photosensitivity), portal
transition fade, and a **third-person over-the-shoulder camera** (shares the piloting camera rig;
crosshair raycast from the camera, projectiles corrected toward the camera aim point; shows the
character costume).

## Robustness (how the big structure stays solid)
- **Contract freeze:** `bd_core` is v1. Changes only via the lead with an explicit version note.
- **Validate at every boundary:** recipes clamped before the sim; network messages size-limited and
  decoded defensively (malformed ⇒ kick, never crash); sim `check_invariants()` every tick in debug
  (finite, in bounds, counts ≤ caps, ids consistent).
- **No panics in sim/net/lexicon runtime paths** (no `unwrap`/indexing on untrusted data; clippy lints).
  A misbehaving construct is removed, never the game.
- **Chaos soak:** headless worlds with bots casting random recipes from the full combination space
  (mechanics × forms × elements × payloads × features × temperaments × grafts × no-limits) for
  millions of ticks; asserts no panic, invariants, tick budget, determinism; failures print a seed.
- **Property tests:** lexicon (any string ⇒ valid recipe < 1 ms), voxels (carve/collapse invariants),
  net (random bytes never crash).
- **Graceful degradation:** unknown word ⇒ blob; no clang ⇒ voice stub; no cloudflared ⇒ LAN + hint.
- **Watchable soak:** `brightest_day --soak-view --seed N` renders a live chaos world (bots casting
  random combinations, auto-director camera, overlay with each cast's phrase + tick time + invariant
  counter; freezes on a failure with its seed). Headless soak writes `--stats` JSON → coverage
  dashboard (mechanic-pair matrix, tick-time histogram, failures).
- **One gate:** `tools/check.ps1` (fmt, clippy, tests, 2-min soak, stress bench thresholds, client
  smoke: launch + screenshot + `--bench`). Nothing integrates unless green.
- **Vertical slice before breadth:** move → bots + ring blast → few constructs → 2-player net →
  breadth.

## Character costumes (built with the same ring vocabulary)
Lobby → "Costume": type any phrase ("knight with a fox head", "robot pirate", "shark", "toaster",
"eeyore", "goku"); the lexicon resolves it and `bd_shapes::costume(recipe) -> ShapeSpec` **dresses a
fixed humanoid rig**: creatures donate their head (+ cosmetic tail stub/ears/fin/horns), objects become
helmets/props at head scale, features/mechanics become accessories (Armor → pauldrons, Wings → small
wings, Cape, Spikes, Antenna, Smokestack hat), icons/Wikidata characters use their curated/category look.
Colours = corps + accent. Guardrails: **cosmetic only** (identical hitbox/movement/abilities), cosmetic
volume ≤ ~1.2× the player hull, always a humanoid core so all animations work, first-person hands show
the costume's gloves + ring. UI: turntable preview, 3–4 seed variants per phrase, randomise, simple
tweaks (accent colour, head scale, accessory toggles), preset gallery for non-typers. The costume
phrase + seed + tweaks are sent once at join.

## Party mode UI (client)
Lobby: mode toggle (FFA / Party) + a settings panel exposing every `GameSettings` knob with presets
(Classic, Party, Chaos, Movement Gods). Between rounds: standings strip (corps-coloured, minimal), then
the **pick screen** — 2/3/4 hard-light cards floating in front of the camera (name ≤ 3 words, one-line
description, a small looping voxel illustration of the behaviour); number keys or click to pick; a
visible timer; locked picks reveal around the table; **Boom or Bust** is a spinning coin that lands on a
gold (Legendary) or purple-black (Cursed) face with its own sting. Party end: podium on the Heart island
with everyone's stacked effects listed.

## Movement tutorial
Main menu → Tutorial: a compact floating course, station by station — bunny-hop, air-strafe, slide,
tether swing, dual tether + reel, boost + release fling — then the **Peak Run** that chains them all.
Each station shows a translucent **ghost** running the ideal line first (recorded from the real
movement code driven by a scripted bot, so every station is provably doable), a 3–5 word hint, and
(tutorial only) a speedometer. Peak Run has bronze/silver/gold times based on the bot's time and
remembers your best. Minimal text; the ghost teaches by showing.

## Tech architecture
Cargo workspace in `game/`:

| crate | role | depends on |
|---|---|---|
| `bd_core` | shared types: math (glam), ids, `VoxelQuery` trait, `mechanics` (tags, `Recipe`, `BodyPlan`, `Element`), corps/colours, constants | glam, serde |
| `bd_voxel` | chunked voxel world, materials, DDA raycast, carve ops (sphere/box → removed voxels), chunk mesher (culled faces + baked AO → plain vertex arrays), arena generator (seeded) | bd_core |
| `bd_sim` | deterministic 120 Hz simulation: player movement + tethers, constructs (recipe → voxel model + behaviour), combat, damage/knockback, match rules. Headless, unit-tested. | bd_core, bd_voxel |
| `bd_lexicon` | runtime resolver: text → `Recipe` using `assets/lexicon.bin` + `assets/icons.ron` | bd_core |
| `bd_net` | protocol (serde+bincode), WebSocket transport on a tokio thread, host/client sessions, cloudflared quick-tunnel launcher | bd_core |
| `bd_game` | the Bevy 0.19 app (bin `brightest_day`): render, input, camera, VFX/debris, audio, menus, glue | all |

Offline tools (`tools/`): `lexicon_build` (Python: downloads + processes ConceptNet/WordNet/corpora/PokeAPI
→ `assets/lexicon.bin` + a human-readable `assets/lexicon_preview.json`), `sfxgen` (procedural synthesis → OGG in assets/sfx + manifest.ron).

### Units & constants
- 1 world unit = 1 m. **Voxel = 0.5 m** for terrain; construct models use 0.25 m voxels.
- Y up. Player collision hull: AABB 0.8 × 1.8 × 0.8 m (crouch 1.1 m); eye at 1.6 m.
- Sim tick 120 Hz (`bd_core::TICK_HZ`). Network send rate: client input every tick batched to 60 Hz,
  host snapshots at 30–60 Hz.

### Netcode
Listen-server, **fully host-authoritative with client-side prediction and reconciliation** (Quake/Source
model). Movement is a pure deterministic function `step_player(state, input, world, dt)` shared by host
and client. Clients send sequenced inputs (with redundancy); they predict their own player immediately,
and when a snapshot arrives with `last_processed_input_seq` they reset to the authoritative state and
replay unacknowledged inputs (smoothing any residual error visually over ~100 ms). Knockback, grabs and
launches therefore "just work". Remote players are interpolated ~2 snapshots in the past; hitscan and
melee are lag-compensated on the host by rewinding other players' hulls to the shooter's view time.
Casts are sent as text + aim; the host resolves and broadcasts the spawn. Voxel destruction is
replicated as deterministic carve ops (`center, radius, seed`), so every client applies identical edits
and spawns its own cosmetic debris. Transport: WebSocket; host listens on localhost and launches
`cloudflared tunnel --url http://localhost:PORT`; the `*.trycloudflare.com` words are the lobby code.
Direct `ip:port` also works for LAN.

### Performance budget & snappiness rules
Targets (dev box, RTX 5080 / i7-12700K), measured by a stress scenario (8 bots at construct limits,
kaiju, constant explosions): **frame time p99 < 7 ms**, **host sim tick p99 < 4 ms** (120 Hz), no
single-frame hitch > 16 ms after load. Judge by p99/1%-low, never averages.
- Free *what*, bounded *how much*: per-player construct limits (COMBAT.md §3) cap entity counts.
- Construct collision = a few OBBs/capsules per part (from prims), never per voxel.
- Recipe → ShapeSpec → voxels → mesh runs on worker threads and is **cached by recipe** (same phrase =
  same model). Loadout slots are pre-built at match start.
- Destruction: carve ops merged + rate-limited per tick; remesh on worker threads with a per-frame
  budget, nearest-to-camera first; collapse search bounded; debris cosmetic, client-only, GPU-instanced.
- Network: creatures/constructs replicate `anim state + time`; clients compute part poses with the
  shared deterministic `bd_shapes` animation code. Delta-compressed snapshots.
- **Latency hiding**: own swings, ring blasts, hit-stop and cast *forming* animations start locally on
  input; the host confirms damage/spawns. Forming time (≥ 150 ms) covers the round trip.
- **Input latency**: `AutoNoVsync`, raw mouse applied per frame, `desired_maximum_frame_latency = 1`.
- **No first-use hitches**: pre-warm every pipeline/material/mesh type during load (render them once
  offscreen); no asset loading mid-match; no per-frame allocations in hot paths.
