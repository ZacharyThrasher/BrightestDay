# Combat & constructs — what Brightest Day decides the words *mean*

This is the game-owned half of "vocabulary vs. mechanics". The lexicon hands us a `Recipe`
(form + mechanic tags + element + scale/mass/count). Everything below is ours.

## 0. The skill thesis — two co-equal axes
Player expression comes from **how you move** AND **what you construct**. Neither may dominate: a
creative player with average movement must be able to beat a movement god with a boring kit, and
vice versa. Every playstyle (swinging duelist, kaiju summoner, trap architect, sniper, trickster) wins.

**Axis 1 — Movement (hands).** Melee/held hits get a *bounded* momentum bonus from closing speed:
`dmg = base · (1 + clamp((v_close − 8) / 30, 0, 0.6))` (≤ +60%), knockback likewise. Swinging in off a
tether feels great (bigger hit-stop, streak VFX when `v_close > 20`) but isn't required to do damage.
Movement is mostly *defence and positioning*: dodging, closing gaps, escaping kaiju, recovering from
ring-outs.

**Axis 2 — Construction (head).** Owner rule: *creativity must make creative things HAPPEN — never
the same result with bigger numbers.* There are no "creativity multipliers". Instead the rules are
built so that unusual ideas produce genuinely different behaviour:
1. **Mechanics are components, not stat bumps.** Every `Mechanic` is a behaviour component with a
   generic implementation that works on ANY form (§5b). The form gives shape + default control; each
   mechanic adds a *verb*. So combinations produce new behaviour we never enumerated:
   `flying hammer` hunts enemies on its own and bonks them; `bouncy wall` is a rebound surface that
   sends players and projectiles back; `homing trampoline` slides under the nearest enemy and launches
   them; `charging wall` is a bulldozer; `anvil swarm` is a hailstorm; `flying cage` scoops someone up
   and carries them over the void; `spinning pillar` is a lighthouse sweep; `exploding boomerang`
   craters on the way out and on the way back; `sticky rocket` rides the target; a `shark with wings`
   is a dive-bomber; `returning godzilla`... returns.
2. **Grafting.** Casting while your crosshair is on one of *your own* live constructs grafts the new
   phrase's mechanics/element/parts onto it instead of making a new one: throw a boulder, then
   `wings` → it takes flight; then `fire` → it's a flaming meteor; aim at your wall, `legs` → it
   walks toward the enemy. Constructs become things you *build up* mid-fight.
3. **Counters are qualitative** (§8): a cage around a kaiju roots it; ice on a fire construct
   freezes and shatters it; a mirror/Guard sends a Beam back along its path; a parachute cancels a
   launch; a pillar over the void is a lifeline; water makes an electric chain arc to the caster.
4. **Chemistry and physics** (§9): elements interact with each other, with constructs and with the
   world — fire spreads through leaves/wood, water douses, ice makes things brittle, constructs have
   mass and push/stack/topple each other, bombs on trampolines, anvils on ramps, boulders down slopes.
   Players discover combos we didn't script.
5. **Readable to the victim.** Every construct telegraphs (forming glow, silhouette, sound) so the
   *opponent's* creativity — reading and countering — is also skill.
The only economy nudge is mild staleness on repeating the exact same phrase (§2), so variety isn't
*punished* — but the *reward* for creativity is always new behaviour, never numbers.

Other skill axes: timing (parry windows, dodging telegraphs), typing under pressure (casting
mid-fight, reciting the oath), and loadout design (the 4 quick-cast slots are a build).

Nothing ever takes control fully away from a player for more than ~0.35 s. Stuns are micro; tumble
reduces air control but you can always tether out of it (a skill save).

## 1a. SMASH MODEL (owner decision — supersedes health-based rules below and in §5i)
- **No health bar.** Each player has **damage %** ("strain"), starting at 0%, uncapped (display to 999).
  Damage numbers elsewhere in this doc now mean **% added** (same values).
- **KO only by leaving the arena:** blast zones = void/water below (existing `void_y`/`water_y`), plus
  side and top bounds (arena bounds + margin ~25 m sides, ~40 m top). Top KO = "star KO" (a twinkle in
  the sky, delayed poof). Last attacker within 6 s gets credit (existing rule).
- **Blast zones exactly** (`bd_sim::world::bounds::Zones`, one rule on every map; `World::blast_zones()`):
  **Bottom** `y < ArenaInfo.void_y`; **Top** (= the **star KO**) `y > voxel-world top + smash.blast_margin_top`
  (40 m); **Side** `x`/`z` beyond the voxel world's sides `± smash.blast_margin_side` (25 m); checked in that
  order. Per map: **Archipelago** (192×96×192 m world) bottom −4 (the sea), sides ±121, star KO above 136 m;
  **Miami** (same size) bottom `WATER_Y − 1` = 2, sides ±121, star KO above 136 m; **Orbit** (192×112×192 m)
  bottom 2 = re-entry burn-up, sides ±121, star KO above 152 m = "launched into space" (gravity fades to
  0.08 g above 104 m, so whatever drifts up there keeps going); flat lab (96 m) bottom −4, sides ±73, star
  KO above 104 m.
- **A tether saves you (owner rule, `tuning.bounds`).** While at least one hook is **attached** (anchored, not
  merely flying) to an anchor that is itself inside the zones, crossing the **Side or Bottom** zone does not
  KO: you hang, swing and reel back in. **Only the star KO (Top) still applies while tethered.** No timers:
  hang as long as the anchor holds. The rope stops holding, and the normal rules apply on that same tick,
  when you let go, when the anchor breaks (voxel destroyed; construct shattered, gone or recalled), when the
  anchor is itself out of the zones (a seabed, a boulder falling into the void), or when you are more than
  `tether_hold_depth` (40 m) past a side / bottom zone (a sanity clamp that keeps bodies inside the world;
  a normal swing never gets that deep). The host decides KOs; movement (and so client prediction) is
  unchanged, and prediction matches the host while hanging. The Orbit re-entry trail and the practice-mode
  void respawn are skipped while a rope holds. Bots hold on and reel while a rope is all that keeps them in.
- **Knockback (Smash-style):** `kb = (base + growth · (pct_after / 100) · (dmg / 10 + 1)) · (1 / weight) · kb_scale`
  applied along the hit vector with guaranteed lift; weight 1.0 default (Grow ×1.6 heavier, Shrink ×0.5
  lighter, suits/pilots heavier). Tuned so ~120–160% + a heavy hit launches off a mid-size island.
  High knockback → **tumble** + a launch trail that intensifies with speed.
- **DI (directional influence):** during launch, holding a movement direction rotates the launch
  vector up to ±15° (skill expression). **Tech:** pressing Jump within 0.15 s of a wall/ground contact
  while tumbling cancels the splat/bounce (skill). **Tethers + recovery:** tether fires out of tumble
  as now (the best recovery in any platform fighter). Wall splat still adds %.
- **Juggle protection** (owner: "a cooldown after a player has been hit so they can't just be knocked
  around endlessly"; `bd_sim::juggle`, `combat.ron` → `juggle`, hot reload). Per victim: a **chain hit**
  is one whose knockback (before decay) is ≥ `chain_min_kb` (14 m/s = a hit that tumbles you); every hit a
  victim takes in one tick is one step (9 shotgun pellets = one). The first chain hit is untouched (single
  big hits and star KOs work as before). Later hits within `window` (1.0 s, refreshed by each chain hit):
  knockback ×0.6 then ×0.35 (`kb_decay`, last entry repeats), damage ×0.75 (`dmg_decay`). After `max_hits`
  (3) chain hits the victim is **guarded** for `immune_secs` (0.7 s): hits still register (event, statuses,
  damage ×0.5) but push nothing and never launch. The chain and the guard end on timeout, or when the
  victim is back in control: standing out of tumble, hanging on their own tether, or a tech. Hook-and-strike
  slams, orbs, explosions and wall splats (their knockback grows with % too — no splat ping-pong; `splats`)
  are ordinary hits in the chain; your own blasts are not.
  Readability: a guarded body's outline turns hard-light white and shimmers (no text). Host-side only;
  knockback reaches prediction through `OwnState`; replicated as bit 7 of `PlayerSnap::status`.
- **Punch-through:** a launched, tumbling player hitting destructible terrain above ~22 m/s (scaled by
  material hardness) plows THROUGH it — a capsule is carved along the path each tick, speed bleeds per
  metre by hardness (glass/leaves ≪ wood ≪ stone), they burst out the other side; below threshold
  inside → splat; indestructible cores always stop. Constructs in the path take integrity damage.
- **Respawn** at 0% with full willpower; brief spawn shield. No health regen.
- **Scoring (lobby):** Stock (N lives, last standing) or Timed KOs (default FFA) — party rounds use Timed.
- **Readability:** everyone's hard-light body glow shifts white → gold → orange → red with visible
  cracks as % rises (readable at 40 m); your own % shown diegetically on the ring + screen-edge
  fractures; an optional small numeric % (setting, off by default — mystery/minimal UI).

## 1. Players (legacy health model — superseded by §1a)
- Health 100. `knock_mult = 1 + 1.25 · (1 − hp/100)` (low hp flies further).
- Hit reaction: knockback impulse with minimum upward component; if impulse > 14 m/s → **tumble**
  (air control ×0.25, spin VFX) until ground contact, tether attach, or 1.2 s.
- **Wall splat**: tumbling into terrain/constructs at > 15 m/s → `(v − 15) · 1.4` dmg (cap 30), carve a
  small crater at contact, bounce at 30% speed, big juice.
- Void/out-of-bounds = KO; credited to last attacker within 6 s.
- Respawn 3 s at the spawn farthest from living enemies; 1.5 s spawn shield (can't deal or take damage).

## 2. Willpower economy
- Max 100, passive regen 3/s (paused 1.5 s after casting). Oath recital (F): typing test of your corps
  oath; while open, movement keys are ignored (TYPING bit) but momentum/tethers/aim stay live.
  Refill = `100 · accuracy² · clamp(target_time / time_taken, 0.5, 1.2)`; cancel anytime (no refill).
  Taking damage while reciting interrupts it (partial refill for progress so far ×0.5).
  **Sections:** you recite one *section* (an authored line of `assets/oaths.ron`; a line over 95 chars is
  split at its middle clause — `bd_sim::willpower::oath_sections`, shared by sim and client), the next one
  in rotation per corps each time (never the same twice in a row). `OathBegin { section }`: the host looks
  the length up in its own `OathTable` (the client never sends a length; bad index → section 0), so a
  half-length section earns the full refill in half the time and `oath_min_secs` (20 cps) still scales.
  **Voice:** in the overlay, hold the voice key ≥ 0.2 s before the first letter to speak the section
  (a tap still types its letter); outside it, a push-to-talk utterance that matches a section of your own
  oath (≥ 0.7, `oath::voice_match`) is a recital, not a cast. Voice has the same skill ceiling as
  typing (owner): accuracy = the word similarity, `seconds` = the speaking time (voice key down → release,
  never Whisper's latency), then the same refill formula, speed bonus and PERFECT/GOOD/SLOW grade. The
  host floor still applies (`OATH_MAX_CPS` 20 chars/s; fast speech is ~12–16). The result waits until
  `oath_min_secs` has passed since `OathBegin` (you stay reciting — vulnerable, can't cast — until then).
  Whisper's bias prompt carries your corps oath's distinctive words (`OathTexts::bias_words`).
  Clock parity: the client re-sends `OathBegin` at the first typed key / at voice-key down, which restarts the
  host's clock if no progress has been reported (only ever stricter), so a pause after opening the overlay
  costs neither. `OathResult { voice }`: a spoken result gets `tuning.willpower.oath_voice_slack` (2 s)
  instead of the 0.6 s network slack before the host's own elapsed time floors the claimed speaking time
  (Whisper's latency isn't the player's); the 20 cps plausibility check is the same.
- Cast cost = `category_base · size_factor · (1 + 0.15·#mechanics) · count_factor · staleness`.
  - bases: Held 12, Worn 15, Structure 10, Object 14, Creature 25 (Kaiju 60).
  - autonomous non-creatures (Held / Structure / Object that Flies or Walks: "flying hammer", "wall with legs") are priced like creatures (base 25, upkeep 1.5/s), judged on the phrase as typed — party/power grafts never raise a price; a typed graft that makes your construct hunt pays the difference. Behaviour unchanged (balance pass 2, `tuning.willpower.autonomous_*`).
  - `size_factor = clamp((scale/default_scale)^1.3, 0.5, 3)`; `staleness = 1 + 0.3·(uses of this exact
    word in the last 30 s)` — repetition gets pricier; variety is free (expression!).
- Upkeep: held/worn constructs drain 0.6/s, creatures 1.5/s (Kaiju 4/s), structures 0.3/s. At 0
  willpower your constructs start losing integrity quickly and shatter.
- **Never refused for willpower** (FREEDOM bundle, `bd_sim::freedom::afford`): a cast that costs more than
  you have *sputters* — count first, then scale, shrink toward what's left (floor `tuning.freedom.sputter_floor`
  of the asked scale / the form minimum) and it drains **everything** left; at ~0 it is a tiny fizzle, never
  nothing. Flickers (`cflags::SPUTTER`, the low-integrity shader flicker) and is fragile (integrity from what was
  paid). An unaffordable graft still applies, drains what's left and gains integrity only for what was paid.
- Shout energy (typed CAPS / "!!!" or a real yell — equal) scales a fresh cast up to +15 % (`energy_max_bonus`),
  paid as usual. "it" = your newest construct ("make it bigger" → graft path, resize priced like buying the
  bigger one); "again!" / "two more" re-cast; "missile at Jake" aims at Jake. Hidden legendary rolls: see the
  `bd_sim::legendary` module docs (never shown to players).
- **Charging (hold Z, bd_core v1.4 `buttons::CHARGE`; `bd_sim::charge`, `tuning.charge`):** the anime power-up —
  the easy, loud, exposed refill (the oath stays the skilful, faster one). Refill `rate_per_sec` 4.5/s (× the
  willpower mode's regen multiplier) on top of passive regen, ramping in quadratically over 0.8 s (tapping earns
  ~nothing); ignores the post-cast regen pause. While charging you can't attack, cast or use powers; on the
  ground speed ×0.2 (feet planted), in the air air-control ×0.3, jump ×0.5 (pure function of the input, so
  prediction matches). Any hit / DoT breaks it; still holding, it restarts cold after a 0.6 s stagger. Releasing
  ends it. Replicated as `pflags::CHARGING` + `PlayerSnap::charge` (hold time, 1/20 s) + `GameEvent::Charge`
  (start / full / broken / released). Bots charge when low and nobody is within ~16 m.

## 3. Construct lifecycle
cast → **validate/clamp** (scale per form min/max, mass, count ≤ 6, swarm ≤ 12) → pay → **forming**
(voxels stream from the ring to their slots; held 0.15 s, worn 0.2 s, structures 0.25 s, creatures
0.6 s, kaiju 1.2 s) → active → dismissed (X; voxels dissolve back, refund 25%) / expired / **shattered**.
- **Integrity** = `20 + 4·cost`. Any hit on a construct subtracts damage × (Blade ×2 vs constructs).
  At 0 → shatter into its own voxels with the hit's impulse.
- **Clash**: two opposing constructs (or a construct and an enemy held weapon) in contact exchange
  integrity damage by relative momentum; both flash; shard ring VFX. Clash of two swings = both
  attackers knocked back a little (sword-lock moment).
- Limits per player: 1 held, 1 worn, 3 structures, 2 objects in flight, 2 creatures (1 Kaiju).
  Casting over a limit replaces the oldest in that slot.
  **Lobby-set** (`MatchRules::limits` / `ConstructLimits`, host Match Rules page → CONSTRUCTS; the
  numbers above are the defaults, Chaos = ×3 structures / objects / creatures / kaiju): held, worn,
  structures, objects, creatures, kaiju, colossal (1; at the cap a new colossal is refused), a
  per-player TOTAL of all kinds (unlimited) and a match-wide GIANTS ON MAP (kaiju + colossal;
  unlimited). Over a cap → replace the caster's oldest of that kind (the map-wide cap only ever
  swaps the caster's own giant). **0 = that kind is off**, never a "no": a kaiju becomes the biggest
  creature allowed (Biped at its max size), a colossal just big (under 24 m); the same downsizing
  happens when the map is full of giants and the caster has none to swap; a kind that is off with
  nothing near it fizzles (`CastError::Fizzled`, nothing paid, the shout fizzles).
- **Lifetimes** (`tuning.summon`): creatures 20 s (was 14), kaiju 24 s (was 18), structures 30 s, objects
  10 s (was 6), domes 3 s, piloted ≥ 30 s, held / worn until dismissed. A thrown object that came to rest
  stays 4 s more (was 1 s), one that landed with a crater 4 s (was 2 s). Upkeep (§2) still bounds how long
  you can afford them.
- **Recall — summons are never lost to the void** (`world/bounds.rs`, `tuning.bounds`). A free construct
  (not riding its owner) that leaves the map (the player blast zones, with the bottom 6 m under the void) is
  **recalled**: it reappears next to its summoner in a free spot (slightly in front and above; not in
  terrain, players or other constructs; walkers / structures stand on the ground; a summoner who is
  hanging off the map gets it at their last solid footing), velocity zeroed, upright, with a hard-light
  blink-in (`GameEvent::ConstructRecalled`). A thrown held item comes back to the hand. A seated pilot is
  ejected where the vehicle was lost (tether / KO rules decide their fate); the vehicle recalls. Ropes and
  chains tied to it snap. It is despawned instead (the old rule) when the summoner is KO'd / respawning /
  gone, for children (swarm copies, slice halves, payloads), power fruits (they respawn on their own), and
  by an invisible anti-loop valve: a construct recalled `recall_max` (3) times within `recall_window`
  (10 s) is despawned on its next exit.
- **Placement — you see what you summon** (`World::place_visible`, `tuning.summon`). Fresh creatures,
  structures (not domes / cages / platforms) and vehicles (not suits) go along the aim ray: on the surface
  under the crosshair when it is within `snap_max` (40 m) and not closer than `near_min + footprint`, backed
  off walls by their footprint; otherwise at the fit distance `clamp(2.4 · radius, 3, 26 m)` (the whole
  thing fits a ~90° view); walkers and structures stand on the ground below (≤ 12 m drop, so a cast at the
  sky lands on the ground ahead, a cast at your feet lands in front of you, a cast off a ledge walks back to
  solid ground). The spot must have a line of sight from the eye (terrain and constructs block) and be clear
  of terrain and players; otherwise it is walked back toward the caster, then swung ±18° / ±34°; a second
  pass drops the "within 40° of the aim" requirement (looking straight up); the old crosshair spot is the
  last resort. Huge creatures (radius ≥ 3.5 m: kaiju) and colossal things go further out
  (`clamp(3 · radius, 3, 80 m)`) and 16° to the right so you watch them rise. Held / worn / thrown objects and
  drop-from-sky are unaffected. Deterministic (host only; clients get the spawn). The client adds a short
  off-screen ping + preview window when your fresh summon is still out of view.

## 3c. Animating the world (grafting onto terrain)
Casting a graft phrase ("legs", "wings", "rockets", "fire", "angry") while aiming at terrain lifts the
connected voxel piece there (a tree, a boulder, a pillar top, a chunk of ruin — extraction bounded to
~6k voxels, cut at a deterministic plane like structural collapse) out of the island, wraps it in the
caster's hard light, and makes it a construct with that mechanic: a tree with legs walks at your enemy,
a boulder with wings flies, a pillar with rockets launches. Its voxel model *is* the extracted terrain
(original colours tinted by corps light). When it shatters, terrain-coloured debris falls. Cost: like a
Creature/Object of that size. Replicated as a deterministic `CarveOp` + construct spawn.

## 3d. Power suits
A human-scale construct with `Piloted` ("iron man suit", "power armor", "mech suit", "exosuit") is a
suit: it wraps the pilot (first-person kept, slight hard-light overlay on the view edges), keeps the
pilot's own movement mostly (bhop/tether still work at ×0.85 speed) and replaces LMB/RMB with the
suit's mechanics; it absorbs hits with its integrity first, shattering off in chunks.

## 4. Forms (default behaviour; mechanics layer on top)
### Held (LMB primary / RMB secondary; empty hand = ring blast + ring shove)
Every melee held item: tap LMB = quick attack (anticipation 70 ms, active 90 ms, recovery 160 ms);
hold LMB = charge (≤ 0.7 s, can move) then release = heavy (×2.2 dmg, ×1.8 kb). Swing hitboxes are
swept arcs sampled every tick (no misses from low tick sampling). Animation drives the hitbox.
| form | base dmg / kb | primary | secondary |
|---|---|---|---|
| Hammer | 14 / 16 | wide overhead arc; airborne + looking down → **dive slam** (crater r=2.5 m, radial kb) | throw if Returning, else ground pound |
| Sword | 9 / 8 | fast 3-hit combo (third hit ×1.5) | parry stance (150 ms perfect window reflects/staggers) |
| Axe | 12 / 11 | horizontal cleave (hits all in arc, great vs constructs) | throw (spinning) |
| Spear | 10 / 10 | long-reach thrust | **lunge**: 18 m/s dash along look (a movement tool, 1.2 s cd) |
| Shield | 6 / 12 (bash) | bash | hold = block 120° front (absorb; perfect-parry 150 ms reflects projectiles) |
| Gauntlet | 11 / 13 | punch combo; with Grab: seize | uppercut launcher |
| Whip | 7 / 6 | 12 m lash; hit player → yank toward you | lash terrain → acts as a **third tether** |
| Gun | see §4b | archetype by phrase (hand cannon / super shotgun / minigun / rocket / railgun) | archetype alt-fire |
| Bow | 8–30 charge | hold to draw, release a heavy arrow | — |
| Cannon | 28 / 24 | slow heavy ball, big recoil (rocket-jump tech) | — |
| Staff | 8 / 8 | element bolt | hold = Beam if tagged |
| Disc | 8 / 8 | throw (always Returning, Ricochet ×2) | — |
Ring blast (empty hand): a meaty fist of light — a fat, fast chunky bolt (180 m/s, 0.35 m swept sphere:
players move 20–50 m/s, so it has to arrive), 12 dmg, 9 kb, 0.4 s cadence, same fire/hit juice as guns (§4b).

### 4b. Guns — DOOM feel, never pea shooters
Principles: (1) every shot is an event — low cadence, high damage, big knockback; (2) **guns are
movement tools** — recoil impulse opposite the aim on the shooter (air ×1.0, ground ×0.35), enabling
shotgun-jumps, rocket-jumps and minigun hovering; projectiles inherit 100% of the shooter's velocity;
the bounded momentum bonus (§0) uses projectile-vs-target closing speed (hitscan: the shooter's own
velocity along the aim); (3) **hitscan where speed would be unfair** (owner decision: players move
20–50 m/s, so slow slugs feel weak) — hand cannon, super shotgun (one ray per pellet, fixed learnable
spread + falloff), minigun and railgun are instant rays, lag-compensated by rewinding other players to
the shooter's `view_tick` (≤ 250 ms). Each round still reads as a physical slug: the client draws a
chunky voxel tracer streaking muzzle → impact in a few frames (`ShotTrace`), then impact chips. Rockets,
cannons, bows, payload rounds (sharks, anvils), explosive / ricochet / portal rounds and thrown things
stay projectiles (splash, arcs, rocket-jumps). A raised guard blocks a hitscan round; a perfect parry
fires it back at the shooter; (4) fire is instant and client-predicted, the host confirms hits.
| archetype (phrase → recipe) | delivery | cadence | per shot | recoil (m/s) | notes |
|---|---|---|---|---|---|
| Hand cannon (gun/pistol/revolver/rifle) | **hitscan** (lag-comp), chunky slug tracer | 0.35 s | 13 dmg, 11 kb | 2.5 | small terrain chip; alt = fan-the-hammer |
| Super shotgun (shotgun/blunderbuss; Gun + Shoots + count≥3) | **hitscan**, 9 rays (fixed 4° pattern), pellet tracers | 2 shots, 0.9 s reload-snap | 9 pellets × 8 dmg, 18 kb total point blank, falloff > 12 m (→ 30 % at 30 m) | 9 | shotgun-jump; pairs with tethers (meathook combo) |
| Minigun (minigun/gatling/machine gun; Gun + Spin) | **hitscan**, small slug tracers | 0.4 s spin-up, then 14/s | 4.5 dmg, 3 kb | 1.2/s continuous | point down = hover; spin-down 0.3 s |
| Rocket (rocket launcher/bazooka; Gun/Cannon + Explodes) | projectile 45 m/s | 0.8 s | 30 dmg center, r=3.5 m carve, 22 kb | 4 | rocket-jump (self kb, no self damage) |
| Railgun (railgun/sniper/laser rifle; Gun + Pierce/Beam) | **hitscan**, lingering beam | 0.5 s charge, 1.1 s cycle | 40 dmg, 20 kb, pierces 2 players + thin walls | 6 | — |
| Palm beam (`Form::Hands`, v1.7: kamehameha, final flash, repulsor, ki blast…; §5j) | **hitscan**, a thick sustained beam from both palms (no weapon model) | hold to charge 0.6 s; fires on **release** (≥ 20 % charge) or by itself at full | 40 dmg × (0.35 + 0.65 × charge), 20 kb, pierces 2 | 2 | caliber by scale |
| Bow / Cannon / Staff / Disc | projectile | see §4 table | | | arcs, splash, ricochet |
Hitscan rounds keep the gun's feel: recoil, the bounded momentum bonus (shooter velocity along the aim),
head-zone crits, knockback, terrain chips. A hitscan gun whose round *is* the point stays a projectile:
`payload` rounds, Explodes, Ricochet and Portal variants. Damage numbers after the hitscan balance pass
(hand cannon 16 → 13, minigun 5 → 4.5, railgun 45 → 40; docs/BALANCE_REPORT.md).
Caliber scales with `scale` (giant gun: slower, heavier, more recoil); `payload` swaps the round
(sharks, anvils: slow heavy novelty rounds); count 2 = dual wield (alternating).
**Feel pipeline** — fire: viewmodel kick (pitch + push back, spring recovery ~150 ms; aim itself never
drifts — no random recoil), 1-frame muzzle light that lights the world, voxel muzzle burst, ejected
hard-light casings that tinkle and shatter, short camera punch, layered sound (transient + bass thump +
distance tail). Hit: 20–50 ms hit-stop on heavy hits, crisp hit-confirm "thock", victim flinch + shove,
voxel chips from terrain, onomatopoeia on big hits (BLAM, KA-CHUNK). Ring shove (empty RMB): 4 m cone push
14 kb, 0 dmg, 1.5 s cd — a defensive reset.

### 4c. Melee lunge — no lock-on (owner: "Destiny sword is a great target"; `bd_sim::melee_assist`)
A lock-on would steal the camera and flatten movement skill. Instead every melee swing has a **lunge**:
- A held **melee** swing that *starts* (primary, combo steps, a released heavy — not secondaries, not guns,
  not the whip) looks for an enemy inside the weapon's **lunge range** (eye to their hull) and a **32° cone**
  around the aim, with **clear sight** (terrain and solid constructs block it). Hulls are the ones the
  swinger saw (lag compensation, `view_tick`).
- Beyond natural reach (item length + 0.9 m): a committed **dash** (30 m/s, ≤ 0.2 s: 4–6 m in ~0.17 s) at the target's
  intercept point that stops at arm's length so the swing's active window lands on arrival — on the ground
  and **in the air**. It only raises your speed *toward them* to the dash speed (a faster bhop / swing keeps
  its own; nothing stacks), ends in a **committed stop** (5 m/s), and has a **0.5 s cooldown**, cleared when the
  lunge swing connects (chains stay fluid; a whiff still waits). Combo swings lunge too.
- Range scales with the weapon: sword ≈ 6 m, hammer ≈ 5.3 m, Heavy / giant (long) weapons dash shorter and
  slower (range = clamp(reach + 4.0 · form · weight ÷ length penalty, 2.5, 6.5)). (CONNECT pass, owner: "I don't
  even feel the lunge / I just end up spam clicking": the old 20° / 4.6 m / 0.7 s lunge fired on ~23 % of
  spam-clicked swing starts at a strafing target, now ~46 %; `tests/connect.rs` prints the verdicts.)
- Connecting lunge swings get heavier juice (impact ×1.4, Heavy ×1.25 more: hit-stop, shake); the momentum
  bonus keeps using your closing speed from *before* the dash (the lunge carries you, movement earns the bonus).
- The swing **hitbox** (never the camera) turns toward the target by ≤ 15°, easing in over the windup (swing
  magnetism).
- Skill stays: out-move it (boost / tether / dodge out of the cone or past the range), space, bait the cooldown.
  Bots swing through the same code. Client: the held weapon brightens while a swing would lunge-connect (same
  query) — no reticle, no text. The dash is part of the movement state, so prediction replays it exactly.
Numbers: `assets/tuning/combat.ron` → `assist.lunge`. Feel (presentation, `assets/tuning/juice.ron`): FOV punch out,
speed lines, whoosh, the viewmodel thrusts forward; others see a hard-light dash streak; the in-range glow is bright.

### 4c-bis. CONNECT assist — easier to connect, never through walls (`bd_sim::aim_assist`, `assist.aim`)
Owner: "I feel like I'm slapping noodles around and it is hard to hit anyone." Invisible, host-side, lag-compensated
like the hit tests it helps; all numbers in `combat.ron` → `assist.aim` (neutral = `AimTuning::neutral()`).
- **Melee hit volume:** the swing capsule radius vs players ×1.5 (whip ×1.15, shield ×1.25), × (1 + 0.3 · (swing
  weight − 1)), the extra capped at 0.45 m; the extra only counts with a clear line from the blade to the body.
- **Projectile homing** (ring blast, bolts, arrows ×0.8, cannonballs ×0.5, discs ×0.8; explosive rounds none — their
  splash is their assist): at release the enemy nearest the crosshair inside 12° / 80 m (clear sight) becomes the
  target; the round turns toward them at ≤ 220°/s, falling linearly to 0 over 0.3 s, lets go past 35° off its flight
  line, needs a clear line every tick. The `Homing` mechanic stays far stronger.
- **Thrown constructs:** the same with 14° / 45 m, ≤ 160°/s over 0.5 s, release 45°.
- **Hitscan magnetism:** a round missing every body by ≤ 1.6° (hand cannon), 0.9° (minigun), 0.5° (railgun / beams),
  0 (shotgun pellets) within 90 m is bent onto the nearest body if the bent ray is clear of terrain and constructs.
- **Railgun early release** (owner: "never require another click"): letting go before full charge fires a weaker
  shot — damage, knockback, recoil and beam width from 35 % at a tap to 100 % (`guns[Railgun].release_min`).
- Hit rates vs a strafing target (scripted human-ish aimer, `tests/connect.rs`): sword 70 → 100 %, hammer 51 → 89 %,
  hand cannon 21 → 53 %, thrown axe 7 → 69 %.

### 4d. Hook-and-strike — tethers hook players (`bd_sim::hook`; lobby rule "Tethers hook players", default on)
One rule set:
- **Hook:** a tether that meets a player hooks them (hull + 0.3 m box, same hook projectile and crosshair
  assist; lag-compensated per shooter). A dodging / spawn-shielded / cockpit-seated player can't be hooked.
- **Reel:** a rope on a player pulls **you** (rope pull + reel, tilted up so it yanks you off the ground in a
  flat arc); it is never rigid — the victim is never towed and never tows you.
- **Slam:** arriving within 3 m holding a melee weapon lets go of the rope; your next swing started within 1 s
  is a **slam** (knockback ×1.5, +lift, juice ×1.6) — one swing, consumed.
- **Escape — the rope breaks when:** the victim **dodges** or **boosts**, the victim **fires a tether** of
  their own, the victim's **swing cuts** through the rope, the rope is **longer than 40 m**, or it is **out of
  sight for 0.25 s**. (And the usual: either player KO'd, the hooker lets go.)
- The victim sees the hard-light rope binding them and a subtle tug arc at the screen edge toward the hooker.
Numbers: `assets/tuning/combat.ron` → `assist.hook` (+ `move.ron` `hook_player_lift`). Bots with a melee weapon
try it now and then. Events: `GameEvent::Assist` (lunge / hooked / broken + why / slam ready / slam).

### Worn (movement modifiers — must stay below skilled movement)
- Wings: hold Jump in air = glide (fall → forward speed, cap 22 m/s); tap Jump = flap (+6 m/s up, 3 flaps
  then ground to recharge). Parachute: hold Jump = slow fall (terminal 3 m/s) + **cancels tumble** (a
  ring-out save). Jetpack: hold Jump = thrust up/forward, cap 16 m/s, drains willpower 6/s.
  Cape: glide-lite + one mid-air dash.

### Creatures (autonomous; hunt the enemy the owner last aimed at / hit, else nearest)
Spawn at the aim point on ground (fliers in the air). Size clamps: normal 0.5–4 m, Kaiju 10–22 m.
Locomotion: steering toward target over voxel heightfield, hop ≤ 1 m ledges, fliers path in 3D.
Speed `≈ 9 · (scale/2)^-0.25` (Fast ×1.6, Heavy ×0.7). Duration 14 s (Kaiju 18 s). Attacks by tags:
- default bite/swipe (8 dmg), **Charge** (telegraph 0.4 s, dash 25 m/s, 16 kb), **Stomp** (ring
  shockwave, carves ground under Kaiju feet every step), **Breath** (0.6 s telegraph glow, cone of the
  element, 4 dmg/tick), **ArticulatedTail** (tail sweep 360° behind, big kb, smashes terrain),
  **Grab** (pincers/jaws hold a player 1 s then fling), **Flies** (swoop dive), **Swarm** (`count` small
  homing units, 3 dmg each, fragile), **Constrict** (wrap & squeeze DoT, tether breaks it).
- Players can tether onto creatures (swing around a Kaiju!). Creatures can be clashed and shattered.

### Structures (placed at aim, facing you)
Wall (blocks everything, 6×4 m default), Ramp (**Launch**; a surfable slope — lets you build your own
movement tech), Pillar (a fresh tether anchor anywhere, e.g. over the void), Dome (shield bubble around
you 3 s), Cage (Trap: encloses the aimed player 1.5 s unless they break it), Platform (floating ledge),
Trampoline (Bounce: restitution 1.1, cap 35 m/s).

### Objects
Ball (thrown, bouncy, 8 dmg), Anvil/**DropFromSky** (appears 12 m above the aimed spot with a shadow
telegraph 0.5 s, crushes: 30 dmg, crater), Bomb (**Explodes**: 1.2 s fuse or impact, r=4 m crater,
30 dmg at center), Boulder (rolls, keeps momentum, flattens things), Rocket (**Homing**, turn-rate
limited, explodes), Vehicle (Charge along the ground in your look direction, rams), Blob0 (unknown word:
thrown lump, 6 dmg — "never nothing").

## 5. Mechanic modifiers (compose onto any form)
Returning (comes back after hit/1.2 s; catch it; Mjolnir: RMB-hold after throw = it drags you along —
Thor flight, drains willpower fast), Ricochet (+2 bounces, seeks enemies within 25°), Homing, Explodes
(on impact), Sticky (sticks; bomb-like attach), Spin (continuous contact dmg), Heavy (×1.5 dmg & kb,
×0.8 speed), Fast (×1.4 speed, ×0.8 dmg), Pierce (ignores shields' block, lunge), Blade (×2 vs
constructs), Blunt (×1.3 kb, craters), Guard (adds block), Pull, Grab, SlowFall/Glide/Thrust (adds the
worn behaviour to held items: an umbrella is a Staff + SlowFall), Bounce, Launch, Barrier, Trap, Beam,
Shoots, Breath, Stomp, Charge, ArticulatedTail, Flies, Swarm, Constrict, DropFromSky.

## 5b. Mechanics as generic components (the composition engine)
Every construct is a physical body (voxel shape → mass, collider) plus a **controller** chosen by its
form category, plus one **component per mechanic**. Components only talk through shared body state
(velocity, contacts, target, owner, trigger events), so any combination works. Precedence: if two
components both want to drive motion, the "drivers" blend in this order: Held/worn attachment >
Grab/Constrict (while holding) > Charge (while dashing) > Homing/Flies/Thrust (steering) > physics.
| component | generic meaning on ANY construct |
|---|---|
| Flies | body ignores gravity and becomes self-propelled; with no other driver it hovers after its target and bumps it (a flying hammer bonks, a flying wall hunts you as a moving barrier) |
| Homing | continuously steers toward its target (turn-rate limited) |
| Thrust | pushes along its facing (on held items: pulls the wielder — rocket hammer lunges you; on objects: rocket-propelled) |
| Charge | periodic telegraphed dash along facing toward target, rams |
| Returning | after contact/timeout, flies back to its owner (catchable); if it has a wielder slot it re-equips |
| Ricochet | on contact, reflects off the surface and re-aims at the nearest enemy (N bounces) |
| Explodes | detonates on its trigger (impact / fuse / owner re-cast / death) — carves, knocks back |
| Bounce | its surfaces are super-elastic for anything that touches them (players, projectiles, constructs) |
| Launch | anything that touches its top/front is flung along its facing |
| Sticky | attaches to whatever it touches first and rides it |
| Grab | when its front touches a player/construct, seizes it and carries it; owner's RMB (or AI) throws |
| Constrict | wraps what it touches; holds + squeezes |
| Pull | on contact, yanks the touched thing toward its owner |
| Spin | continuous rotation about its up axis; contact = repeated hits, deflects projectiles |
| Swarm | the construct is split into `count` smaller copies that act independently |
| DropFromSky | spawns high above the aim point and falls (telegraphed shadow) |
| Barrier / Guard | its front face blocks and reflects (see §8) |
| Trap | if it can enclose its target (cage-like), it snaps shut around it |
| SlowFall / Glide | on held/worn: modifies the wielder; on anything else: the body itself drifts/glides |
| Breath | periodically emits its element as a cone from its front |
| Beam | emits a continuous beam from its front along its facing |
| Shoots | periodically fires small projectiles from its front at its target |
| Stomp | when it lands/steps, emits a ground shockwave + carve |
| ArticulatedTail | adds a tail part that sweeps anything behind it |
| Blade / Pierce / Blunt / Heavy / Fast | physical character of its contacts (cut, penetrate, crater, mass, speed) |
Autonomous constructs (creatures, or anything with Flies/Homing/Charge) pick targets by the owner's
last aim/hit, else nearest enemy. Voxel shapes gain visible parts for mechanics (wings, rockets,
spikes, tail, jaw, horns) on any form, so the victim can *read* what a combination will do.

## 5c. Temperament (abstract/emotion words → behaviour)
Autonomous constructs have a `Temperament` + intensity from the phrase ("very angry bees"). It changes
*behaviour and animation*, not stats: Aggressive (relentless pursuit across the map, faster attack
rhythm, twitchy), Fearful (kites at range, uses cover, fires its payload), Protective (orbits owner,
body-blocks projectiles, intercepts attackers), Playful (bouncy erratic pathing, knockback pranks,
random targets), Lazy (slow, sits down, heavy roadblock), Chaotic (attacks anyone but its owner,
including constructs), Sneaky (flanks from out of view, dim glow). Abstract nouns alone ("rage",
"sadness") make a **spirit**: a flying wisp creature carrying the word's temperament + element.
Profanity is allowed and useful (intensifiers: "fucking huge hammer"; tone: "pissed off shark" →
Aggressive); crude words resolve through their physical sense like any word. Only slurs/hate speech are
blocked (in phrases and player names). Ambiguous words prefer the literal physical sense ("horny" =
horned). The closed mechanic set means the ring can only ever express what we designed.

## 5d. Piloting (Mechanic::Piloted)
"a giant mech I can pilot", "ride a dragon", "drive a tank", "mount a horse". Mech/Gundam/Jaeger-like
bipeds and classic mounts get it by default when cast big. The caster is seated at the cockpit/saddle
part; camera pulls back to third person; WASD drives the construct's own locomotion (walk, fly, roll —
by form/features), mouse aims its head/turret, LMB/RMB fire its mechanics (punch/stomp/payload/breath),
Space = jump/thrust, **E = eject** (big upward launch — also a movement tech). Drains willpower while
piloted (Kaiju-tier upkeep for colossal things). Balance = the Attack-on-Titan loop: piloted giants
are devastating but slow; **weak points** (joints, cockpit, "nape") take ×3 from Blade/Pierce, and
swingers can tether onto the giant and ride its limbs. Shattered → pilot ejected tumbling.

## 5d-bis. Transformers (Mechanic::Transforms) — a birthday-present feature, make it great
- **Two modes, one body.** A Transforms construct has an alt mode (usually a vehicle/animal) and a robot
  mode built from the *same panels*: bd_shapes splits the alt model into ~10–24 rigid panels and maps them
  to robot roles (cab/hood → chest, doors → back-wings, wheels → shoulders/calves, rear → legs, grille →
  abdomen, headlights → eyes, a small head unfolds from inside). Transformation = a staggered
  mechanical sequence (~0.9 s): each panel does 1–3 discrete moves (rotate on a hinge, slide, flip) with
  snappy easing and tiny overshoot, panels ordered outside-in, then the robot "stands up". Reverse for
  robot → vehicle. Readable silhouette change; sparks + an original mechanical cascade sound.
- **Be the transformer:** human-scale Transforms + Piloted = a power suit (§3d) with a transform key
  (default: Mouse3 / V-tap when voice is off / "transform" graft). Vehicle mode: drive (ground speed
  cap 24 m/s — below skilled bhop/swing chains; ramps launch; ram = Charge), no ring attacks. Robot mode:
  normal movement ×0.9 + the suit's weapons (Optimus: ion blaster + energon axe; Megatron: fusion cannon;
  Bumblebee: arm blaster + stinger). Transforming mid-air from vehicle → robot = **landing slam**
  (Stomp at the landing point, scaled by fall speed) — a skill combo.
- **Autonomous transformers** (summoned) drive fast toward the target in alt mode, transform ~12 m away
  and fight in robot mode, transform back to reposition.
- **Works on anything:** "a tank that transforms", "transformer shark", "robot in disguise", grafting
  "transform" onto your own vehicle/creature. Procedural robot modes are generated from any alt model.
- **Combiners:** with ≥ 3 owned Transforms vehicles, "combine"/"devastator"/"form devastator" merges them
  into one Kaiju-scale robot (each becomes a limb/torso) — ~2 s spectacular sequence, one per player.
- **Phrases:** "roll out" transforms all your Transforms constructs; "autobots"/"decepticons" summon a
  squad of 3 mini transformers; "transform" alone = graft/toggle.
- **Icons with two hand-authored modes:** Optimus Prime (semi truck), Bumblebee (muscle car), Megatron
  (tank/cannon), Starscream (jet), Soundwave (boombox → robot; spits Swarm minions), Grimlock (T-rex,
  Breath fire), Shockwave, Jazz, Ironhide, Ratchet, Arcee (motorbike), Devastator (combiner), Unicron
  (planet ⇄ robot, no-limits only).

## 5e. No-limits mode (lobby option)
Lobby toggles: Scale (Normal / Huge / **No limits**), Willpower (Normal / Double / Infinite), Construct
caps (Normal / Chaos, or per kind: §3). "Throw a moon": colossal Objects form above the caster while the cast is held,
then hurl on release; they cross the sky slowly (a telegraph everyone can dodge) and carve huge craters.
Performance stays bounded because **construct voxel size scales with construct size** (0.25 m for
hand-held → 1–2 m for colossal), keeping per-construct voxel counts within budget; huge carves are
progressive (spread over ~0.5 s, capped per tick); debris is sampled to the 20k budget; ≤ 1 colossal
construct per player. Islands regenerate between rounds in no-limits.

## 5f. Mechanic budget — how we avoid slop
Players learn ~20 **verbs** + 7 **traits**, grouped: Emitters (Shoots/Beam/Breath), Holds (Grab/
Constrict/Pull), Descent (SlowFall/Glide), Blocking (Guard/Barrier), Motion (Flies/Walks/Homing/Charge/
Thrust/Returning/Ricochet/Spin/DropFromSky), Area (Explodes/Stomp/Bounce/Launch/Trap), Body
(ArticulatedTail/Swarm/Piloted); traits = Blunt/Blade/Pierce/Heavy/Fast/Sticky (+Bouncy via Bounce).
**Admission test** for any new mechanic (all six): (1) a genuinely new verb, not a combination;
(2) readable by the victim in ~0.5 s via a unique silhouette/VFX/sound tell; (3) has a construct
counter; (4) generic on any form; (5) never removes control > 0.35 s; (6) players naturally say words
that map to it. **Never**: invisibility, time stop, mind control, instant kills, player healing, long
stuns. **Second wave (approved, in progress)** — groups: Space (Portal), Fields (Attract/Repel),
Resize (Shrink/Grow), Holds (+Link):
- **Portal**: linked pair (aim point + next aim / near caster), orange-ring/blue-ring style in corps
  colour; players, projectiles, constructs and debris pass through keeping speed, rotated to the exit
  facing (Portal-game momentum tech: fall into a floor portal, fly out of a wall portal). Lasts 12 s;
  either ring can be shot/broken. Counter: break a ring.
- **Attract / Repel**: a field (radius ∝ scale, ~6–14 m) with a swirling particle tell; accel falls off
  with distance, Heavy things resist ×0.3, tethered players resist. Black hole = Attract + Explodes at
  end of life. On a held item: Attract = pull targets to your swing, Repel = shove wave.
- **Shrink / Grow** (applied to what it hits, 6 s, clear shrinking/growing animation + sound): Shrink
  → hull/visual ×0.5, speed ×1.2, knockback taken ×1.6, damage dealt ×0.6; Grow → ×1.6 hull, speed
  ×0.85, knockback taken ×0.5, melee reach/damage ×1.4. Works on constructs too (grow your own hammer).
  Counter: Guard reflects the ray; it wears off.
- **Link**: a glowing chain (distance constraint, max length ~8 m, some slack) between the first two
  things it touches — or the target and the ground if it only hits one. Chained players drag each
  other; chain a player to an anvil and push the anvil off a cliff. 6 s or until cut (any Blade hit,
  or 25 dmg). Counter: Blade.
Playtests log per-mechanic usage + win rate; unused → merge, dominant → counter/nerf.

## 5g. Party mode (round-based, fully configurable)
**Structure:** N rounds (default 7). A round ends at the round KO target or time limit; points per KO
(+ round-win bonus) accumulate into party standings. Between rounds: a ~12 s **pick** — each player picks
**1 of 3** random party effects that apply to *their* constructs for the rest of the party (effects
stack across rounds). **First place** (party standings) gets only **2** choices. **Last place** gets
their 3 plus a 4th card, **Boom or Bust**: a 50/50 coin flip → a random **Legendary** or a random
**Cursed** effect. Ties: every tied player gets that rule. Picks lock in with a reveal (everyone sees
what everyone took); unpicked on timeout = random. No duplicate picks of the same effect per player.
**Effects are behaviour, never numbers** (owner rule): most are a *standing graft* merged into every
recipe that player casts (reusing the composition engine), some add event hooks. Examples:
- Common/wacky: **Bouncy Castle** (everything you build Bounces), **Clingy** (all your projectiles/
  objects are Sticky), **Heat Seekers** (everything Homes), **Spicy** (Fire on everything), **Shocking**
  (Electric), **Party Popper** (your constructs Explode into confetti when they shatter), **Mini-Me**
  (every cast also makes a tiny copy that follows it), **Clown Car** (constructs Swarm ×3 but tiny),
  **Growth Spurt** (constructs Grow over their lifetime), **Chain Gang** (whatever your constructs hit
  gets Linked to them), **Portal Pals** (shattered constructs leave a portal pair), **Magnet Brain**
  (constructs Attract debris + enemy projectiles), **Rebound** (constructs Ricochet), **Legs Day**
  (everything gets Walks), **Wings Day** (everything Flies), **Boomerang** (everything Returns), **Road
  Rage** (every construct Charges), **Friendly Fire-ant** (Aggressive temperament on everything).
- **Legendary** (Boom): **Worthy** (every held item becomes Returning + Electric and can Thor-fly),
  **Kaiju Heart** (your creatures grow a size class with each KO you get), **Echo** (each cast repeats
  1 s later as a free second copy), **Roll Out** (all your constructs Transform), **Main Character**
  (your constructs can't be shattered by a single hit — they break in two stages, splitting first).
- **Cursed** (Bust): **Butterfingers** (held items sometimes slip and fly off), **Piñata** (when hit
  you drop small random constructs other players can grab), **Wild Magic** (25% of your casts resolve as
  a random phrase), **Reverse Card** (your projectiles curve back toward you after 1 s), **Loud**
  (your constructs honk and glow extra bright — no sneaking), **Slippery** (ice under your feet:
  low friction everywhere — secretly great for bhop players).
Effects are data (`assets/party_effects.ron`), easy to extend.
**Outside party mode** the same effect grafts can be won for a while from High Roller's slot machines
(`bd_sim::world::interactables`): only common, graft-only effects (hook effects need the party state),
applied in `cast_inner` after the party graft and priced as typed, for `tuning.casino.effect_secs`.
**Settings (lobby, all gameplay knobs):** mode (FFA / Party), rounds, round KO target, round time,
points per KO / round win, pick timer, Boom-or-Bust on/off, first-place-2-choices on/off, effect pools
(common/legendary/cursed toggles), plus every MatchRules knob (scale, willpower, caps, respawn time,
spawn shield, gravity scale, gas regen, knockback scale, void on/off, islands regen), saved as presets.

## 5h. Player powers (ONE framework for every iconic power system)
A **Power** (data in `assets/powers.ron`, executed by bd_sim) is a fixed template — nothing else:
1. `body`: passive traits from closed sets — MoveModifiers, element immunities/weaknesses, "scatter"
   (Logia: on a hit, dissolve into the element and reform ~1 m away; 3 s cooldown), bounce-body, etc.;
2. `graft`: a standing graft merged into every cast (like party effects);
3. `incantation`: an optional prefix ("gomu gomu no", "mera mera no") — `<prefix> <phrase>` resolves the
   phrase and applies the power's **delivery** (e.g. rubber: launched on a stretching arm that snaps back
   = Returning + Bounce; fire: Fire element + flame trail) — works only if you hold that power;
4. `moves`: up to 2 signature moves on the power key (G tap / G hold), each just a Recipe (+ optional
   self-impulse/blink);
5. `transform`: optional beast/hybrid form using the Transforms panel system on the player's costume
   (Zoan fruits);
6. `companion`: optional persistent construct (JoJo stands: floats behind you, Protective; damage to the
   stand is dealt to the user; stand rush = each "ORA"/"MUDA"/"DORA" typed in the console is one punch —
   typing speed = rush speed).
**Devil fruits:** cast "<name> fruit"/"<name> no mi" → an edible hard-light fruit (expensive, anyone can
grab & eat it); held until KO, then **the fruit respawns where you died**; eating a second fruit →
you explode (self-KO, no credit). Weaknesses: Water hits drain the power for 3 s; "seastone" Links
disable it while linked; fruit users sink faster in the void.
**Stands:** cast the stand's name (or "stand arrow" → random): held until KO; a second stand replaces
the first. "Time stop" stands never freeze others (the never-list): the world greys and the *user*
moves at ×2 for 1.5 s instead.
**Origins** (`kind: Origin`: radioactive spider, gamma, kryptonian, speed force, power rings,
viltrumite, omnitrix…): cast the origin's name → a small themed **pickup** at the aim point (`pickup`:
name + an inert form/element look — a radioactive spider, a gamma canister, a power ring); anyone can
grab it; held until KO, then it drops where you fell, like a fruit (the fight over it is emergent). None
of the devil-fruit rules: no Water drain, no seastone, no faster void sinking. An authored `weak`
element still drains any power for 3 s (kryptonite = Poison). Gated only by the powers toggle.
Transforms and companions are data on any kind (a Zoan-style origin is still `Origin`).
**One power slot:** a new power always replaces the one you hold (a replaced fruit/origin drops at your
feet; a replaced stand is gone), **except fruit on fruit → you explode**. **Random pools never cross:**
"devil fruit" rolls devil fruits only (never origins or mutagens), "stand arrow" only stands, "mutagen"
only `mutagen_*`, `random:origin` only origins.
Other franchises' power systems (Naruto jutsu, MHA quirks, bending, Nen, Pokémon moves…) are added
later **as data in this template only**.

## 5i. Balance philosophy
1. **Balance with willpower cost, not damage** — a strong combo keeps its behaviour and gets pricier
   (preserves "creativity changes behaviour, not numbers").
2. **Counterplay over parity** — the bar is "nothing unanswerable", not "everything equal".
3. **Soft anti-snowball (FFA):** respawn at 0% with full willpower; streakers glow brighter + their streak
   sound announces them; shutdown (ending a 3+ streak) = +1 point. (Smash model: no regen.)
4. **Hidden** (mystery tenet): no stats, tooltips or DPS displays.
5. **Party / no-limits are deliberately unbalanced** — fun > fair; the lobby opts in.
6. **Data-driven:** per-match local telemetry (`logs/matches/*.json`), bot-vs-bot tournaments over random
   recipes, a balance report flagging outliers (e.g. damage-per-willpower > 2× median, win-rate skews,
   never-used mechanics), and all combat numbers in hot-reloadable `assets/tuning/combat.ron`.
Targets (Smash model): typical KO % ≈ 110–170; a focused fight from 0% to KO ≈ 8–15 s; heavy/creative
burst setups faster; strong recoveries (tethers/constructs) save ~40% of launches.

## 5j. Signature casts (owner: "a domain expansion called by name should give a dopamine spike")
**An amplifier over the existing composition — no new mode, no new mechanic.** Vocabulary
(`assets/signatures.ron`, `bd_lexicon` invocation.rs): invocations `domain expansion <rest>`, `bankai
<rest>`, `final / ultimate / full power / limit break … <rest>`, suffixes `<rest> domain`; tagged famous
ultimates (`kamehameha` Beam, `unlimited void` Domain, `spirit bomb` Summon, `tensa zangetsu`
Transformation…, matched on the phrase or the construct it resolves to); a Beam yelled with ≥ 3 "!".
`<rest>` resolves like any phrase (element, mechanics, icon). The sim (`bd_sim::signature`,
`tuning.signature`) then rewrites the recipe with mechanics that already exist:
- **Domain** → `Dome` + Barrier + Trap of the phrase's element around the caster, radius 16 m
  (+ up to 4 m by shout power), 6 s; keeps the rest's emitters / fields / traits (a shrine that Shoots, a
  bouncy-castle domain that Bounces); drops movers (Flies, Walks, Charge, Swarm, Piloted…). Everything
  else is the ordinary Dome/Trap/Barrier behaviour: an enemy inside is snapped up by the Trap, projectiles
  stop at the shell, enough damage from inside or outside breaks it (integrity from its cost).
- **Beam** → energy from the PALMS (owner: "final flash gave me a super powered gun, not the iconic
  beam"): `Form::Hands` + Beam at caliber ×2.5 — no weapon model; hold PRIMARY to charge (both hands draw
  back around a growing orb in the element colour), release to fire (or it fires itself at full): a thick
  0.5–1.0 s beam from the hands, damage by charge. A creature / sky emitter keeps its body and gains Beam;
  a named wand / sword keeps it (`delivery: Held`); a piercing thrown thing is an erasing orb.
  **Summon** → the thing at ×2–2.6 scale. **Transformation** → ×2–2.6 scale + Heavy. Caps: form max ×
  2.5, ≤ 30 m.
- **Delivery (data, `signatures.ron`):** a Beam name comes from the palms unless tagged `delivery: Held`
  (avada kedavra's wand, getsuga tensho's sword) or `Thrown` (rasengan, hollow purple). Plain energy
  casts come from the palms too (`energy.hands`: repulsor, unibeam, ki blast…; or an `energy_words` word
  resolving to a gun model with no `gun_words` word: "laser beam" palms, "laser gun" a gun). A yelled gun
  ("RAILGUN!!!") stays a gun. The title letters show a recognised signature's canonical name
  ("FINAL FLASH!!"), anything else exactly what was heard.
- Cost = the existing cast cost × `cost_mult` (1.35), never more than a full bar; unaffordable → the plain
  phrase is cast instead (the ring never says no). Forming ≥ 0.6 s (a readable windup; the caster keeps
  moving). Held / worn ultimates expire after 10 s. Bots at ≥ 85 willpower shout one 20% of the time.
- **Erasing orbs** (`signature::orb`, `tuning.signature.orb`; the flagship is HOLLOW PURPLE): a Beam signature whose
  phrase Pierces and is fired / thrown becomes a hard-light Ball, 3.5 m (+0.6 by shout, x1.15 from a full bar). It
  gathers 0.6 s ahead of the caster (a data `windup: Merge(Water, Fire)` shows a blue and a red orb pulled together
  from the hands and merging in a flash), then flies straight along the aim at 50 m/s for 120 m and stops for
  nothing: a 2 m-radius tunnel carve every 1.2 m (power 3), every construct it touches (not its caster's) shattered,
  every player it touches launched once (28 %, base knockback 36: ~40 m/s at 0 %, ~54 at 90 %, ~63 at 150 %), a
  7 m Attract wake riding it. Dodge i-frames and movement are the counterplay.
- **Orbital strikes** (`signature::orbital`, `tuning.signature.orbital`; "SATELLITE DEATH BEAM", "UFO
  abduction beam", "dragon breath from the sky"): not a new verb — an emitter (Shoots / Beam / Breath)
  that Flies and is placed with DropFromSky hovers ~30 m (22–40, under the map ceiling, nudged to within
  ~32° of the caster's view) above the aim point. A ground TARGET MARKER (corps-coloured ring, closing and
  pulsing; `GameEvent::OrbitalStrike { Telegraph }`) shows 0.7 s before the beam comes down; the beam
  lives 2.5 s and **tracks** at ≤ 6.5 m/s (Homing: the nearest enemy near the point; else the caster's
  crosshair ground point) — good movement outruns it; pulses every 0.1 s (3 dmg ×Heavy/Fast, element
  status), carves every 0.3 s; 3 s cooldown, then the next strike while the construct lives. Attract =
  abduction (lifts, no damage); Breath = a wider column (fire ignites); Shoots = a hail of bolts. Cost =
  the existing (autonomous + scale) price × 1.25, never more than a full bar. Meteor showers need nothing new (count + DropFromSky).
- **Presentation, no freeze, no control taken:** any THREAT (`signature::is_threat`: a hostile mechanic —
  Explodes, Beam, Shoots, Breath, Blade, Pierce, Blunt, Stomp, Charge, Spin, Trap, Grab, Constrict,
  DropFromSky, tail — a kaiju, an aggressive / chaotic creature or swarm, palms, orbs, orbital strikes)
  costing ≥ 45 or ≥ 12 m (signatures, kaiju, no-limits moons — one generic path, `signature::spectacle`;
  an "ULTIMATE PARACHUTE", wall, ramp or weaponless vehicle gets none) gets a picture-in-picture cut-in DANGER
  alert for everyone (side of the threat, corps colour, title, severity by distance / inside the radius),
  the shout words as the title letters, and audio/music cues. Nothing explains any of it (mystery tenet).

## 5k. Power-ups (lobby mode, `MatchRules::powerups`, off by default)
Owner: "a mode that has certain mechanics behind powerups on the map — a kaiju summon power up, an
awakening, etc." `bd_sim::pickups`; every number and the gated list in `combat.ron` → `pickups`.
- **Gated** (short list, `gates`): **Giant** (kaiju / colossal / creatures ≥ `giant_height`),
  **Awakening** (a power's transform, tiers ≥ `awakening_min_tier`), **Ultimate** (signature kinds in
  `ultimates` — domains, beams — orbital strikes, sky drops ≥ `drop_scale`). Everything else is free.
- **Never a no**: without the pickup the phrase still makes something through the existing downgrade
  paths — GODZILLA comes out a person-sized lizard (Biped, `giant_weak_scale`), a meteor a fist-sized
  rock (form minimum), a signature the plain phrase (the signature's can't-afford path), Gear 5 the
  plain power-up flare (no form). With the pickup: FULL size, and the pickup is spent.
- **Pickups** are deterministic host entities on one shared timer (`every`, first after
  `first_after`): a beacon pillar for `telegraph` s, then it lies there `ground_life` s. Touch = carry
  (one at a time, `held_life` s); KO'd → it drops at your last footing; spent by the next gated cast.
  Spots are derived on every map (the centre, midpoints between spawns, halfway spawn→centre; solid
  ground with head room, `spawn_clear` m from spawns, `spot_spacing` apart); `spots` overrides per map id
  (`World::set_pickup_map`). Replicated as `WorldSnapshot::pickups` + `GameEvent::Pickup`.
- **No text**: a thumping crimson heart-egg with a sub-bass heartbeat (Giant), a golden orb in
  swirling rings (Awakening), a spinning violet star (Ultimate). Carried, it glows in the carrier's
  hand (the local player sees it low in the right of the view) under a faint beacon: the carrier is
  the target. Host rules page → CONSTRUCTS → POWER-UPS.

## 6. Elements (status on hit; always short, always visible)
Light (none), Fire (burn 3 dmg/s 2.5 s, orange embers), Electric (chains to 1 extra target within 6 m
at 50%; 0.12 s micro-stun), Ice (accel ×0.6 for 1.5 s, frost voxels), Water (+40% kb), Wind (+kb, more
lift), Earth (×2 terrain carve), Poison (2 dmg/s 4 s), Psychic (victim gravity ×0.3 for 1 s — juggle),
Shadow (victim vision vignette 1 s), Atomic (burn + bigger breath/explosions).

## 8. Counters — qualitative outcomes, not multipliers
| your construct | against | what HAPPENS |
|---|---|---|
| Guard / Barrier / Dome | projectiles | stopped; a perfectly-timed Guard *reflects* them back to their owner |
| Guard (mirror-ish / Shield) | Beam, Breath | the beam bends off the shield along your aim — you redirect it |
| Cage / Trap | Creature, Kaiju | creature rooted in place, thrashing (it can break out); a kaiju cage topples it |
| Constrict / Grab | Creature | you can *steer* the grabbed creature (drag it, throw it at people) |
| Blade | Structure, Barrier, Cage | sliced cleanly in two along the swing plane; both halves fall physically |
| Pierce | Guard | goes through; the shield cracks with a hole where it hit |
| SlowFall / Glide | being launched | tumble cancelled mid-air; you drift down safely |
| Pillar / Platform / Whip | falling into the void | a new anchor/ledge — the rescue |
| Ice | Fire construct/creature | fire snuffed; the thing freezes solid and becomes brittle (next hit shatters it) |
| Water | Fire; Explodes | fire hisses out into steam (vision cloud); bombs fizzle into duds |
| Electric | Water / wet targets | current arcs through everything wet, including the water's owner |
| Heavy / DropFromSky | Flies | fliers are slammed out of the sky and pinned briefly |
| Returning / Ricochet | targets behind cover | reaches around corners |
| Pull / Grab | fleeing / fast players, fliers | reeled in to you |
| Charge / Heavy | Wall, Barrier | smashes through; wall voxels scatter as debris |
The table is data (`bd_sim` counter rules), easy to extend; the lexicon never touches it.

## 9. Systemic interactions (world + construct chemistry)
- Fire ignites Leaves/Wood voxels: burning spreads slowly (deterministic cellular step at 10 Hz,
  bounded), voxels burn away into ember debris. Water/Ice extinguish.
- Ice freezes Water-element constructs and creatures (brittle: next Blunt hit shatters them ×2).
- Electric + Water construct/creature/wet player: chain radius ×2.
- Explodes + Bounce/Trampoline: explosion kb ×1.5 upward (launch combos).
- DropFromSky onto a Ramp slides and keeps going; Boulder follows slopes and terrain holes.
- Wind pushes light constructs/projectiles and Swarms; Heavy things ignore Wind.
- Every construct has mass: constructs push, block, stack and topple each other and debris (players
  can build a wall, then knock it onto someone).

## 7. Destruction rules
Explosions, dive slams, kaiju stomps/tails, wall splats, heavy hits on terrain and Earth elements carve
terrain via deterministic carve ops (replicated). Constructs are hard light — they never damage the
voxel *they are made of*, but when they shatter their voxels become glowing debris that fades in ~1.5 s.
Terrain debris is physical-looking and lingers ~4 s. Structural collapse (bd_voxel) drops unsupported
chunks as falling bodies that can hit players (dmg by mass·speed).
