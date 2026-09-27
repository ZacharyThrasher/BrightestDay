# Move presentation grammar (TOOLBOX PRESENTATION worker)

Every power move — held, borrowed, kit, chained — plays a full beat: **gather → release → world
reaction → clean exit**, shaped per op kind, in a per-move **style** (the BODY the move is made of).
Same verb, different body: a rubber arm is not a golden pole; a Nightcrawler BAMF is not a Flash dash.

Code: `crates/bd_game/src/present/powers/movefx/` (grammar, bodies, poses, dev harness),
`crates/bd_game/src/power_hud/borrowed.rs` (chips + chain pips). Data: `assets/tuning/move_fx.ron`
(hot-reloaded). Sounds: `tools/sfxgen/sounds_moves.py` (34 `mv_*` sounds in `assets/sfx`).

## Authoring (data only — no code)

Two optional fields (bd_sim `powers/data.rs`, presentation only, never read by the sim, never on
the wire):

```ron
(
    id: "teleport_bamf",
    kind: Origin,
    fx: "smoke",                      // default style for every move of this power / kit
    moves: [
        (name: "bamf", ops: [Blink(dist: 12.0)]),                         // → smoke
        (name: "bamf strike", fx: "shadow", ops: [Blink(...), Shove(...)]), // a move's own fx wins
    ],
)
```

**Resolution**: move `fx` → power `fx` → the move's element (Shove / Explode / Surge / Pounce /
Counter / Salvo / Empower `element`, a Cast recipe's element) → the power's graft / pickup element
→ `"energy"`. **Colour**: `juice.color` → the style's own colour → the element's → the power's
identity colour → the caster's corps colour. Keys ignore case, spaces, `_` and `-`
("Speed Force" = "speedforce").

Unknown keys fall back silently in play and FAIL the test
`move_fx_every_fx_key_in_the_powers_data_exists` (`cargo test -p bd_game --bin brightest_day movefx`).
Add a missing key to `move_fx.ron` (`styles` or `aliases`) rather than renaming your data.

Keep using `juice` (title, punch, shake, boom, ripple, color, debris) — it layers on top.

**Words**: a style's `word` (an `assets/fx/onomatopoeia.ron` key) is what its blasts SAY: a
smoke BAMF says "BAMF" (not "KA-BOOM"), rubber "BOING", rigid "KRANG", speedforce "ZWIP", void /
shadow / cursed "VWUMP", holy "SHING", elements their material word; `"none"` = silent. A smoke /
styled blink with no blast of its own says its word where it lands (never in your own face).

**Adding a style** (no code): add an entry to `styles` in `move_fx.ron`: pick a `body` (the closed
set of SHAPES below), colours, numbers, sounds. TV & CARTOONS' `"putty"` (Jake the Dog) is a good
example: a Rubber body, fatter, floppier, slower, lower-pitched.

## Style keys (usable now)

| key | body | reads as | sound (release / impact / exit) |
|---|---|---|---|
| `energy` (default) | Energy | clean hard light in the move's colour | shing |
| `hardlight` (`tech`, `construct`) | Energy | crisper, cyan-white core | glassy strike, shard run |
| `rubber` (`gum`, `elastic`, `stretchy`) | Rubber | limb stretches FROM THE SHOULDER, thins as it stretches, wobbles, band-snaps back | creaky stretch / SNAP + boing |
| `putty` | Rubber | fat, floppy, slow dog-putty | low doughy stretch, sloppy flop |
| `rigid` (`staff`, `pole`, `metal`, `iron`) | Rigid | iron shaft, gold bands, telescopes out in segments, dead straight (the Ruyi staff's look) | segment clacks / temple-bell ring / collapse tink |
| `smoke` (`bamf`, `sulfur`) | Smoke | purple-black sulphur puffs where you were AND where you land, embers | BAMF crack + puff + hiss |
| `speedforce` (`speed`, `flash`) | Streak | yellow lightning streaks, 5 afterimages, crackle along the path | doppler zip + crackle |
| `lightning` (`electric`, `thunder`) | Bolt | jagged white-blue bolts re-struck every frame | close thunder crack |
| `wind` (`air`, `gust`, `storm`) | Gust | spiralling thin streaks | swirling gust |
| `shadow` (`dark`, `darkness`) | Smoke | dark violet smoke that sinks | breathy dark swoosh |
| `ink` | Liquid | black drops and tendrils | low splash |
| `void` (`gravity`, `black hole`, `cosmic`) | Void | implodes first; dark core, violet rim | suck-in / hollow thump |
| `cursed` (`curse`, `demon`) | Flame | blue-black flame with red core | distorted growl + crackle |
| `blood` | Liquid | red drops, arcs, splash crowns | wet splash |
| `holy` (`light`, `divine`, `radiant`) | Halo | gold rings, rising light | choir chord bloom |
| `chi` (`ki`, `aura`, `chakra`) | Halo | cyan rings | resonant thrum |
| `psychic` (`mind`, `telekinesis`) | Halo | magenta rings | psychic hum |
| `fire` (`flame`, `magma`, `lava`) | Flame | rising embers, hot core, smoke | WHOOMP + crackle |
| `ice` (`frost`, `snow`, `cold`) | Shard | shards that hold then shatter | crystalline crack-freeze |
| `crystal` (`gem`) | Shard | violet gem shards | ringing gem chord |
| `water` (`liquid`, `ocean`) | Liquid | blue drops, splash | splash |
| `steam` (`vapor`, `mist`) | Smoke | white, fast-rising | pressurised hiss |
| `sand` (`dust`, `ash`) | Grain | pouring grit that falls | grain hiss |
| `earth` (`stone`, `rock`) | Grain | heavy chunks, dust | ground crack |
| `poison` (`toxic`, `acid`) | Smoke | green clouds | hiss |
| `atomic` (`radiation`, `nuclear`) | Energy | green-white glare | atomic hum |
| `glitch` (`digital`, `pixel`) | Glitch | offset blinking cubes | bit-crushed stutter |
| `paper` (`cards`, `petals`) | Paper | fluttering sheets that linger | flutters + snap |
| `spirit` (`ghost`, `soul`) | Smoke | pale cyan wisps rising | airy swoosh |

Elements with no `fx` get: Fire→fire, Electric→lightning, Ice→ice, Water→water, Wind→wind,
Earth→earth, Poison→poison, Psychic→psychic, Shadow→shadow, Atomic→atomic.

**Bodies** (code, closed set): Energy, Rubber, Rigid, Smoke, Bolt, Streak, Liquid, Grain, Flame,
Shard, Gust, Glitch, Paper, Halo, Void. A body shapes every piece of the grammar: its motes, its
traces between two points, its limbs / lines, its bursts and rings.

## Grammar per op (what every move gets, in its body)

| op | gather | release | world reaction | exit | pose (1P / 3P) |
|---|---|---|---|---|---|
| **Blink** (+ BlinkCarry) | implosion where you were | burst + flash where you land, a trace between (bolt re-struck, smoke tube, streaks…), a ground ring | Smoke: a lingering puff at BOTH ends | trace fades | lunge |
| **Impulse** (+ Ram, Pounce, Leap) | — | air tears open behind you (ring ⟂ the dash), backward burst; a ground ring on launches | streak ribbons at shoulders + feet, body motes, afterimages (style `afterimages`) | landing SKID: grit spray + a scar in the ground + skid sound | lunge (1P: speed lines rush past the lens + FOV punch) |
| **Shove** (+ Surge) | fist flash | cone shockwave (3 rings sweeping out along the cone) + the cone's wireframe racing out | ground crack under the cone (two for big ones), debris flung | Rubber / Rigid: the LIMB stretches the whole range (Gum-Gum Pistol: shoulder → fist, wobble, band-snap + boing; rigid: telescoping banded pole, clacks, bell ring); other bodies at range ≥ 4 m: a spectral fist flies out and back | punch |
| **SlamOnLand** | — | a falling whistle | a DREAD RING on the ground under you that tightens + spins faster as you fall, a dashed drop line, grit lifting | crater ring, dust wall, 6 radial cracks, flash, debris, shake nearby | arms up |
| **Reel** (+ Haul) | stretch sound | the limb / line shoots to the anchor | anchor burst + thunk | when you arrive: band snap-back (exit sound) | grab |
| **Cast** | gather at the hand | flash + shing | Rubber / Rigid: the ARM stretches to the new construct and follows it (a returning pistol pulls the fist home) | band-snap / telescoping collapse | punch |
| **Field / Room** (+ Zone) | attract: pulled in / repel: pushed out | ground ring racing to the edge, flash | the boundary rim shimmers; anyone crossing it flares where they cross (+ a membrane bloop) | fades | palms |
| **Swap** | — | both ends flash at once, bursts + rings at both | an arc between them | fades | palms |
| **Explode** | Void: collapse first | body-shaped bloom (embers + smoke, shards, bolts, splash…) + ground ring | shake nearby | — | palms |
| **TimeStop** | implosion | two shockwave rings, flash, clock tick | a ripple off you twice a second, two clock hands sweeping round you | fades | palms |
| **Carve** | — | trace along the line | ground crack along it, end burst | — | punch |
| **Link / Weigh / Yank** (+ Grab, LockOn, Seeker, Afflict, Lockout) | — | a trace from the hand to the aim | burst at the end | — | grab |
| borrowed move of a power with a companion (a Stand) | the companion's ghost steps out behind you | — | — | fades ~0.6 s | — |
| everything else | — | body pulse: ring rising off the feet, chest burst | — | — | clench |

Incanted deliveries with `arm: true` (gomu gomu no <anything>) draw the power's `fx` limb (default
`rubber`) from the shoulder (your first-person fist) to the delivered construct.

**Readability**: everything is drawn for every player (it is how you read what an enemy just did).
Nothing blooms in your face: motes shrink away inside ~2 m of the camera, flashes and puffs at the
lens are dropped, beams through the lens are pulled back. Your own blink / dash in first person
shows speed lines rushing past the lens in the body's shape.

Poses: first person via `MovePoses.local` (viewmodel: punch / palms / grab / lunge); third person
via `MovePoses::body` in `present/players.rs` (Cast thrust, TetherFire reach, PowerUp palms, and the
ANIMATION PASS `Dodge` dive for blinks / dashes; its `Land` squash plays on touchdown by itself).

Generic-op event visuals (op rays, target markers — DelayedHit / Tag / LockOn —, hook tethers,
telegraphs, strikes, state auras) belong to the OP VISUALS worker (`present/powers/op_visuals/`);
this grammar presents the ops through their classic beat (aliases above) and never draws those.

Other presentation guards landed with this pass: a ripple shell around a bystander (a 22 m snap)
stays a faint shimmer instead of washing their screen (`juice.rs`), and your own blast is judged at
your sim position (no explosion word / flash in your lens after a blink, `impact.rs`).

## HUD (wordless)

* **Borrowed-move chips** to the right of the power icons: the last 4 borrowed / kit moves you
  fired — the move's colour, the STYLE glyph (`glyph` in `move_fx.ron`), a clockwise cooldown sweep
  (host `ToolboxSnap` cooldowns), a flare when fired and when ready. Repeats shrink the chip a step
  each (staleness = +1 spark each). Idle chips fade.
* **Chain pips**: one per link of a pending chain; each pops as its link fires, the next breathes;
  they vanish at once if the chain is cut (`ChainStopped`).
* **Tab**: the move's name over its chip (with the power cards).
* **Spark gauge preview**: typing a borrowed / kit move previews its sparks (tier + staleness);
  typing a chain previews the whole line.

## Dev

`BD_MOVEFX_TEST=<blink_smoke|dash_speed|shove|slam|rubber_pistol|rubber_rocket|rigid_pistol|hud>`
`BD_MOVEFX_VIEW=fp|obs` `BD_MOVEFX_STYLE=<key>` `BD_MOVEFX_SHOTS=<dir>` `BD_MOVEFX_SPAM=<rounds>` —
see `movefx/dev.rs`. Frame sheet: `docs/toolbox/presentation_sheet.png`.

## Note from TEXT CLUTTER (world-text budget, 2026-09-27)
- `present/powers/juice.rs`: the move-title `Shout` now sets `sh.title = true` (one line). Titles are
  hidden when Settings → Audio → WORDS → COMIC TEXT is OFF, and one caster's titles stack in a short
  column (newest on the base line, older shrunk above, ≤ 2) — `assets/fx/text_budget.ron` `titles`.
  Set `title = true` on any other title `Shout` you add.
- All comic words go through the placement/budget policy in `bd_fx::text_budget` (never in the camera's
  lap or over the crosshair, screen-size clamp, own words small and to the side, a priority cap near
  you, every-Nth word on rapid light hits). Style words keep working as-is; set
  `SpawnOnomatopoeia::source` (caster id + 1) and `accent` (finishers) when you spawn words.
- The `electric` word key now reads KRAK / ZZAK / KRAKOW (lightning); new hit keys `thoom`, `thrust`,
  `thwack`, `whip_crack`, `hit_punch` are available for `word:` in move_fx.ron styles.
