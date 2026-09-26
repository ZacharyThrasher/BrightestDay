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

## 1. Players
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
- Cast cost = `category_base · size_factor · (1 + 0.15·#mechanics) · count_factor · staleness`.
  - bases: Held 12, Worn 15, Structure 10, Object 14, Creature 25 (Kaiju 60).
  - `size_factor = clamp((scale/default_scale)^1.3, 0.5, 3)`; `staleness = 1 + 0.3·(uses of this exact
    word in the last 30 s)` — repetition gets pricier; variety is free (expression!).
- Upkeep: held/worn constructs drain 0.6/s, creatures 1.5/s (Kaiju 4/s), structures 0.3/s. At 0
  willpower your constructs start losing integrity quickly and shatter.
- Can't cast if cost > current willpower (the ring flickers — diegetic "no").

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
Ring blast (empty hand): a meaty fist of light — 75 m/s chunky bolt, 12 dmg, 9 kb, 0.4 s cadence, same
fire/hit juice as guns (§4b).

### 4b. Guns — DOOM feel, never pea shooters
Principles: (1) every shot is an event — low cadence, high damage, big knockback; (2) **guns are
movement tools** — recoil impulse opposite the aim on the shooter (air ×1.0, ground ×0.35), enabling
shotgun-jumps, rocket-jumps and minigun hovering; projectiles inherit 100% of the shooter's velocity;
the bounded momentum bonus (§0) uses projectile-vs-target closing speed; (3) visible chunky voxel
projectiles with tracers (only the railgun is hitscan, lag-compensated); (4) fire is instant and
client-predicted, the host confirms hits.
| archetype (phrase → recipe) | cadence | per shot | recoil (m/s) | notes |
|---|---|---|---|---|
| Hand cannon (gun/pistol/revolver/rifle) | 0.35 s | 16 dmg, 11 kb, 140 m/s slug | 2.5 | small terrain chip |
| Super shotgun (shotgun/blunderbuss; Gun + Shoots + count≥3) | 2 shots, 0.9 s reload-snap | 9 pellets × 8 dmg, 18 kb total point blank, falloff > 12 m | 9 | shotgun-jump; pairs with tethers (meathook combo) |
| Minigun (minigun/gatling/machine gun; Gun + Spin) | 0.4 s spin-up, then 14/s | 5 dmg, 3 kb, 120 m/s | 1.2/s continuous | point down = hover; spin-down 0.3 s |
| Rocket (rocket launcher/bazooka; Gun/Cannon + Explodes) | 0.8 s | 30 dmg center, r=3.5 m carve, 22 kb | 4 | rocket-jump (self kb, no self damage) |
| Railgun (railgun/sniper/laser rifle; Gun + Pierce/Beam) | 0.5 s charge, 1.1 s cycle | 45 dmg, 20 kb, pierces 2 players + thin walls | 6 | hitscan, lag-compensated, lingering beam |
Caliber scales with `scale` (giant gun: slower, heavier, more recoil); `payload` swaps the round
(sharks, anvils: slow heavy novelty rounds); count 2 = dual wield (alternating).
**Feel pipeline** — fire: viewmodel kick (pitch + push back, spring recovery ~150 ms; aim itself never
drifts — no random recoil), 1-frame muzzle light that lights the world, voxel muzzle burst, ejected
hard-light casings that tinkle and shatter, short camera punch, layered sound (transient + bass thump +
distance tail). Hit: 20–50 ms hit-stop on heavy hits, crisp hit-confirm "thock", victim flinch + shove,
voxel chips from terrain, onomatopoeia on big hits (BLAM, KA-CHUNK). Ring shove (empty RMB): 4 m cone push
14 kb, 0 dmg, 1.5 s cd — a defensive reset.

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
caps (Normal / Chaos). "Throw a moon": colossal Objects form above the caster while the cast is held,
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
**Settings (lobby, all gameplay knobs):** mode (FFA / Party), rounds, round KO target, round time,
points per KO / round win, pick timer, Boom-or-Bust on/off, first-place-2-choices on/off, effect pools
(common/legendary/cursed toggles), plus every MatchRules knob (scale, willpower, caps, respawn time,
spawn shield, gravity scale, gas regen, knockback scale, void on/off, islands regen), saved as presets.

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
