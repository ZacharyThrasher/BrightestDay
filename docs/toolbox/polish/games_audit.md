# POLISH: GAMES — audit of games_arena.ron + games_adventure.ron (2026-09-27, overnight polish wave)

Scope: 225 powers (222 kits), 1,033 moves, 1,848 shouts (games_arena 149 kits / 599 moves, games_adventure
76 kits / 434 moves) + the ops those packs added (Rewind Ram Hook Grab Deflect Counter LockOn Lockout Zone
Pounce Salvo Mend Evade PlacePortal Empower Capture).

Bar (owner): *unique flavor / control / playstyle + dramatic / cool presentation = fun* (Hollow Purple,
Ruyi staff). Score = **popularity** (how often a player would yell it, 1–5) × **blandness** (1–5: generic ops
only, same recipe as a sibling, default / no style, an ultimate with no impact frame / title / signature
presence, no distinct control, weak chain ending, wrong word / sound).

## Systemic findings (before)

| finding | count | verdict |
|---|---|---|
| moves carrying an `impact_frame` | **0 / 1,033** | the single biggest gap: not one Overwatch ult, dragon punch, Falcon Punch or fatality froze the frame |
| ultimates (`tags: [Ultimate]`) without a title | 7 | minor (Warthog, Noxious Trap, Spirit Rush, Moonlight Shadow, Slow Sands, Okami Thunderstorm, RE rocket launcher) |
| moves with no `juice` at all | 422 | mostly primaries / pokes — correct (contrast keeps big things big) |
| ops that were silently no-ops | 1 | Samus Screw Attack's `Zone(status: Stun)` (zones never stun: owner rule) |
| famous identity missing | 4 | Mega Mushroom / Giga Bowser never grew (only armour); Link's Spin Attack was a single flat ring; Mega Man's charge shot / Ryu's Hadouken / Chun-Li's Kikoken weren't energy balls (no palms-gather, no hold-to-charge, no fireball clash) |
| cast-based ultimates with no Signature presence | ~12 | the headline ones now cast as signatures (D.Va Self-Destruct, Nova Bomb, BFG 9000, Rennala's Full Moon) |
| **engine bug found**: `juice.impact_frame` never fires for kit / borrowed moves | all packs | `present/impact_frames` arms only on `PowerEvent::Move` (held moves); kits emit `PowerEvent::Fired`. Filed in docs/toolbox/POLISH_REQUESTS.md (ANIME filed it first; GAMES +1 and the `move_window` item). Signature-tier hits, crits (`Finisher`, `crit`) and parries (`Counter`) DO fire frames today. |
| most-reused recipes | Cast ×156, Ram ×61, Salvo ×43, Hook ×26, Impulse+SlamOnLand ×14 | acceptable: each Ram / Salvo differs in pin / through / steer / seek / track / cut and in body; the plunge family (ground pound, helm breaker, desolate dive…) differs mostly by style — low priority |

The packs themselves were strong: almost every headliner already did its famous thing with its famous
control (Reinhardt pins you into a wall, Deadeye is a breakable lock-on, Scorpion's spear is a dodgeable
skillshot, Falcon Punch is a telegraphed Salvo, Omnislash is a blink flurry, Judgment Cut End is a time-stop +
delayed cut sweep). So this pass is mostly presentation drama + a handful of identity fixes.

## Ranked list (top 60 by popularity × blandness, before → after)

P = popularity, B = blandness before. ✔ = fixed in this pass.

| # | move (kit) | P | B | what was bland | after |
|---|---|---|---|---|---|
| 1 | Hadouken (Ryu) | 5 | 4 | plain Blunt ball: no palms-gather, no charge, no fireball-vs-fireball clash, no frame | ✔ energy ball (`Explodes`): gathers between the palms, hold to grow, two fireballs burst on each other |
| 2 | Shoryuken (Ryu / Ken / Akuma) | 5 | 3 | invincible uppercut already right; no connect drama | ✔ `comic` impact frame on the connect (all three + Flash Kick, Volcanic Viper, Shoryureppa) |
| 3 | Falcon Punch | 5 | 3 | telegraphed Salvo already right; no frame | ✔ `comic` frame on the landing |
| 4 | Earthshatter | 5 | 3 | shockwave barely lifted (up 0.1): no "knocked off their feet" | ✔ up 0.55 (they're popped down in a tumble) + `ink` frame |
| 5 | Get Over Here (Scorpion spear) | 5 | 3 | right mechanic, no drama on the catch | ✔ `ink` frame on the catch; Finish Him / Fatality / Brutality `ink` |
| 6 | Fus Ro Dah | 5 | 2 | fine; ult without frame | ✔ `ink` frame (ult pass) |
| 7 | Spin Attack (Link) | 5 | 4 | one flat 360° shove | ✔ a real spin: 3-hit all-round `Barrage` (whirr, whirr, WHAM finish), wind body |
| 8 | Deadeye | 5 | 3 | no boom / frame on the shot | ✔ boom + shake + `ink` frame (needs the `move_window` request for the 4.5 s channel) |
| 9 | Dragonblade / Nanoblade | 5 | 3 | hard-light body for a green DRAGON | ✔ `spirit` body (green via juice colour) + `ink` frame |
| 10 | Graviton Surge | 5 | 2 | no world reaction | ✔ inward debris, ripple, `void` frame |
| 11 | Self-Destruct (D.Va) | 5 | 3 | an ordinary bomb cast | ✔ Signature Summon (ultimate scale, forming windup, DANGER cut-in, Signature-tier frame) + `glitch` |
| 12 | Rocket Punch / Meteor Strike (Doomfist) | 5 | 2 | no connect drama | ✔ `comic` frames (plain + charged), Meteor Strike ripple/debris + `ink` |
| 13 | Reinhardt Charge | 4 | 2 | pin + wall slam already right | ✔ `comic` frame on the pin |
| 14 | Mega Mushroom | 4 | 5 | armour + contact only — Mario never got big | ✔ `SelfResize(1.6, 8 s)` + `comic` |
| 15 | Giga Bowser | 3 | 5 | same | ✔ `SelfResize(1.6, 8 s)` + `comic` |
| 16 | Primal Rage | 4 | 2 | fine | ✔ ripple + `comic` frame on the first swat |
| 17 | Raging Demon | 4 | 2 | blink flurry right; generic frame | ✔ `black_flash` (the blackout) |
| 18 | Spinning Piledriver / Giant Swing / Muscle Buster | 4 | 3 | grabs right; no frame | ✔ `comic` |
| 19 | Knee of Justice | 3 | 3 | no juice colour / frame | ✔ electric-yellow + `comic` |
| 20 | Warlock Punch | 4 | 3 | no frame | ✔ `void` |
| 21 | Mega Man Charge Shot | 4 | 4 | plain piercing ball: "charge" was a word only | ✔ energy ball (hold to charge the buster) |
| 22 | Kikoken (Chun-Li) | 3 | 4 | as Hadouken | ✔ energy ball |
| 23 | Donkey Kong Giant Punch | 3 | 3 | no title / colour / frame | ✔ title + colour + `comic` |
| 24 | Kirby Hammer Flip / Bowser Bomb / Maximum Heat | 3 | 3 | no frame | ✔ `comic` |
| 25 | Super Star | 4 | 3 | no frame | ✔ `comic` on the first rainbow body-check |
| 26 | Nova Bomb | 4 | 3 | plain ball | ✔ Signature Summon + `void` |
| 27 | BFG 9000 | 4 | 3 | plain ball | ✔ Signature Summon + `glitch` |
| 28 | Rennala's Full Moon | 3 | 3 | plain ball | ✔ Signature Summon + `void` |
| 29 | Blood Punch / Berserk (Doom) | 4 | 3 | no title / frame | ✔ title + `ink` |
| 30 | Judgment Cut End | 4 | 1 | already the bar | ✔ `ink` frame (ult pass) |
| 31 | Omnislash (Cloud / Juggernaut) | 4 | 1 | blink flurries | ✔ `ink` frames |
| 32 | Braver / Lion's Claw / Tifa Meteor Strike / Nero Buster / Wicked Weave | 3 | 3 | no frame | ✔ `ink` / `comic` |
| 33 | Chaos Blast / Wrath of the Gods | 3 | 3 | no frame | ✔ `void` / `holy` |
| 34 | Triforce Slash | 3 | 3 | no frame | ✔ `holy` |
| 35 | Screw Attack | 4 | 3 | its Stun status was a silent no-op | ✔ removed (the shredding dps stays) |
| 36–60 | every other damaging Ultimate in both files (EMP, Pulse Bomb, Blizzard, Barrage, Rip-Tire, Whole Hog, Terra Surge, Blade Storm, Showstopper, Hunter's Fury, Black Hole ×2, Last Breath, Dragon's Rage, Death Mark, Chronosphere, Echo Slam, Reverse Polarity, Culling Blade, Mana Void, Laguna Blade, Final Spark, Tidal Wave, Urbosa's Fury, Power Bomb, Supernova, Knights of the Round, Megaflare, Golden Gun, Fist of Havoc, Thundercrash, Hammer of Dawn…) | 3–4 | 2 | no impact frame | ✔ systematic ult pass: 82 (arena) + 62 (adventure) keyed by body — void / shadow / psychic → `void`, holy / chi / light → `holy` (fighting games → `comic`), lightning / glitch / atomic → `glitch`, Mortal Kombat → `ink`, else `ink`. Non-hitting ults skipped (Zipline, Jump Pad, Photon Barrier, Care Package, Life of the Party, Skyward Dive, Valkyrie, Well of Radiance, Warthog, Titanfall, Katamari; Comet Azur / Madama Butterfly already signatures) |

## Left as-is on purpose
- Primaries / pokes / dashes (spam tools) get no frame: contrast keeps the big moments big, and the
  frame cap (≤ 3/s, 0.8 s per attacker) would eat them anyway.
- Chaos Control / Witch Time / Tiid Klo Ul (time moves): no hit of their own — a frame key would be
  stolen by the NEXT move's hit.
- Spark prices (`cost`) untouched (tier model rule). The four new signature casts price as Signature
  (6 sparks) through the cast pipeline — they are the ultimates of their kits.

## Op fixes (OPS_REVIEW LOW)
- `mend.rs`, `evade.rs`, `pounce.rs`, `place_portal.rs`: step loops now walk every slot (`0..MAX_PLAYERS`)
  and clear a slot whose owner left / died, so `adventure_active()` goes quiet after a leave.
  (`empower.rs` already did.)

## Coverage (step 4)
Probe: 371 phrases players actually yell in this domain (scratch `polish_games/phrases.txt`), read with
`powers_check --coverage` (hygiene on). **Fall-throughs 100 → 86.** Filled (all marked: Title Case famous
names or carrying the character's name, so no everyday phrase is stolen):
- `<hero> ult` for the eight heroes whose ult was hijacked by an icon alias in assets/icons/games.ron
  (zarya / junkrat / jett / hanzo / raze / horizon / gibraltar (+ gibby) / malphite ult → their kit ultimate,
  was the icon construct);
- Hunter's Fury, Dragons Rage, Electric Thunder, Meat Hook (Pudge), Hasagi / Hasaki, Ultra Sword,
  Starscourge, Limit Break (→ Omnislash), Knights Of The Round, Hammer Of Sol;
- NEW move: Mortal Kombat **Fatal Blow** ("Fatal Blow", "X-Ray", "x ray move"): armoured lunge-grab + ink frame.
Remaining fall-throughs are deliberate: curated icons of the THING ("hookshot", "master sword", "golden gun",
"energy sword", "bfg 9000", "portal gun", "warthog", "sonic boom", "black hole", "emp"…: the summon stays the
summon, the move is "<kit> <move>"), everyday words ("blizzard", "roll", "chainsaw", "wind wall",
"resurrection"), and voice lines / quotes the owner rule forbids ("nerf this", "it's high noon", "heroes never
die", "ryujin no ken wo kurae", "die die die", "rip and tear", "flawless victory").
Hygiene: 0 dead shouts in the games files (removed the pre-existing dead "pharaoh's wind"); a Title-Case
"Lion's Claw" was tried and withdrawn (it would take the myth pack's Nemean "lion's claw").

## Verification (phrase scene, frames in docs/toolbox/polish/games_after.png)
Checked in `BD_MOVEFX_TEST=phrase` (third person): Hadouken, Gou Hadouken, Yoga Fire, Samus charge shot,
Shoryuken, Falcon Punch, Get Over Here, Earthshatter, Doomfist punch, Fus Ro Dah, Graviton Surge, Raging Demon,
Self-Destruct, Mega Mushroom, Fatal Blow, Deadeye, Spin Attack, and the chain Reinhardt Charge -> Earthshatter.
Found + fixed from frames: Hadouken as a Light `Explodes` ball read as a white puff that burst at the hands —
now `Electric` (blue, hit-stun into shoryuken, energy-ball rules); D.Va's eject flung her backwards off the
island — now straight up. Impact frames confirmed firing on kit moves (comic on Shoryuken / Falcon Punch /
Mega Mushroom / Charge, ink on Earthshatter) after PRESENTATION POLISH landed GAMES 1+2.
Open: Spin Attack's 3-hit Barrage has no spinning body (POLISH_REQUESTS GAMES 3); Deadeye's release not
re-captured after the window fix (budget).
