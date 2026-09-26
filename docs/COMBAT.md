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

**Axis 2 — Construction (head).** The ring rewards imagination and cleverness, mechanically:
1. **Composition is power.** Phrases compose closed-set pieces: `a flaming hammer with rockets`,
   `electric shark swarm`, `giant bouncy anvil`, `returning axe of ice`. Each extra mechanic costs a
   bit more but stacks real behaviour (Thrust makes the hammer lunge you forward; Explodes makes every
   hit a crater). Designing a good combo *is* the skill.
2. **Freshness ("Spark").** Your first cast of a word this life gets +20% power and −30% cost; a
   *combination* (form+mechanics+element signature) you haven't used this match gets it too. Repeats
   go stale (§2). Variety is always the efficient play; spamming one weapon is the inefficient one.
3. **Counters pay ("Clever!").** Answering what's in front of you is rewarded: a construct that hard-
   counters the thing it hits/blocks (see §8 counter table) refunds 50% of its cost and deals ×1.5.
   Shield vs projectile spam, cage vs kaiju, ice vs fire, water vs fire, parachute vs a launch, pillar
   over the void, mirror/Guard vs Beam, Blade vs structures, Pierce vs Guard.
4. **Systemic interactions** (§9): elements and constructs interact with each other and the world —
   fire burns leaves/wood voxels, water douses fire, electric through water chains wider, ice + Blunt
   shatters frozen things for bonus damage, bombs on trampolines, anvils on ramps, boulders down
   slopes. Players discover combos we didn't script.
5. **Readable to the victim.** Every construct telegraphs (forming glow, silhouette, sound) so the
   *opponent's* creativity — reading and countering — is also skill.

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
| Gun | 5 / 3 per shot | auto fire 10/s | aim-down: tighter spread |
| Bow | 6–22 charge | hold to draw, release arrow | — |
| Cannon | 20 / 22 | slow heavy ball, recoil pushes you back (rocket-jump tech!) | — |
| Staff | 8 / 8 | element bolt | hold = Beam if tagged |
| Disc | 8 / 8 | throw (always Returning, Ricochet ×2) | — |
Ring blast (empty hand): 90 m/s bolt, 7 dmg, 5 kb, 0.25 s cd. Ring shove (empty RMB): 4 m cone push
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

## 6. Elements (status on hit; always short, always visible)
Light (none), Fire (burn 3 dmg/s 2.5 s, orange embers), Electric (chains to 1 extra target within 6 m
at 50%; 0.12 s micro-stun), Ice (accel ×0.6 for 1.5 s, frost voxels), Water (+40% kb), Wind (+kb, more
lift), Earth (×2 terrain carve), Poison (2 dmg/s 4 s), Psychic (victim gravity ×0.3 for 1 s — juggle),
Shadow (victim vision vignette 1 s), Atomic (burn + bigger breath/explosions).

## 8. Counter table ("Clever!" bonus: ×1.5 effect, 50% refund, small gold flash + chime)
| the construct you use | counters |
|---|---|
| Guard / Barrier / Dome | incoming projectiles, Beam, Breath (blocked while facing) |
| Cage / Trap / Constrict | Creatures (esp. Kaiju: a cage the Kaiju's size roots it 2 s), Charge |
| Blade | Structures, Barrier, Cage (cuts through) |
| Pierce | Guard (ignores block) |
| SlowFall / Glide / Parachute | being launched (tumble cancel) — "saved it" bonus if within 1.5 s of a big knockback |
| Pillar / Platform / Whip-to-terrain | falling into the void (rescue) |
| Ice | Fire, Fast creatures |  
| Water | Fire, Explodes (dud chance) |
| Electric | Water creatures/constructs, Swarm (chains through) |
| Heavy / DropFromSky | Flies (knocks fliers down), Barrier |
| Returning / Ricochet | targets behind cover |
| Pull / Grab | fleeing/fast players, fliers |
The table is data (`bd_sim` counter rules), easy to tune; the lexicon never touches it.

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
