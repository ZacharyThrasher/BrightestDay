# AUTO-KITS — a derived Kit for every character nobody hand-built

"<any character> attack / dash / block / counter / combo / ultimate / special" does something in character,
even for the ~4.8k icon / names-list / costume characters no content worker wrote a kit for.

## What ships

`assets/powers/zz_derived_kits.ron` — GENERATED, checked in, never edited by hand. Plain `kind: Kit` entries
(ids `dk_*`), so derived kits take the exact hand-kit path: `MoveIndex::kit_move`, `ToolStep::Move { kit: true }`,
spark pricing, chains, presentation (`fx` style keys + ultimate `juice` titles / costume colour / impact frame).

* 5 tagged moves per kit — `Primary`, `Movement`, `Defense+Counter`, `Combo`, `Ultimate` — plus at most one
  untagged bonus move from a signature mechanic (web snare, roar, grow, rewind, portal…).
* No shouts (derived kits add nothing borrowable), no tiers / transforms / companions, never a grant.
* The bare name still summons the costume construct (a kit name needs a move word after it).

## Hand kits always win

* The generator SKIPS every character any hand file names (power / tier / kit names, shouts, claims).
* The file is named `zz_…` so it loads LAST: hand ids never move, and every name tie goes to hand content.
* A derived alias may never be a word-prefix of a hand kit name with an apostrophe ("pharaoh's mummy").
* Stale file (a hand kit landed for a character that still has a derived kit): hand still wins at runtime;
  `cargo test -p bd_sim --test derived_kits` prints a STALE warning — re-run the generator.

## Run it (lead / anyone after content lands)

    python tools/derive_kits/derive_kits.py            # rewrite assets/powers/zz_derived_kits.ron (~8 s, stdlib only)
    python tools/derive_kits/derive_kits.py --check    # exit 1 if the checked-in file is stale
    python tools/derive_kits/derive_kits.py --show "killer frost" --show krang     # print kits
    python tools/derive_kits/derive_kits.py --sample 50 --seed 7                  # spot-check
    python tools/derive_kits/derive_kits.py --stats                               # template histogram + collision log
    cargo test -p bd_sim --test derived_kits

Deterministic: only checked-in inputs (icons, names lists, costumes, power files, bd_core enums, bd_sim's
`MoveOp` list — an op that doesn't exist is never emitted, a new op is picked up automatically), stable CRC
hashing, sorted output.

## How a kit is derived (tools/derive_kits/)

`sources.py` reads the data; `derive_kits.py` builds a profile per character — effective element (the recipe's,
or for Light characters the one its names speak of: "Captain Cold" → Ice), flavour (tech / holy / mystic /
spirit / chi / glitch / speed / ink), weapon (costume accessories, icon payload, name + note keywords, Blade /
Pierce / Blunt), size, temperament, form (humanoid / beast / kaiju / vehicle kind), pack — and every slot picks
the best-scoring TEMPLATE with a stable per-character jitter (two fire brutes don't get the same kit):

| slot | templates (examples) |
|---|---|
| Primary | weapon slash / cleave / thrust, smash, stomp, element bolt (fireball, thunderbolt = instant strike at the crosshair, telekinetic lift, ice shard, rock throw…), beam, breath cone + lingering status, bite / peck / swipe pounce, web / lash hook, spin, hex, stretch punch; plain fighters get a brawling STYLE (jab, haymaker, kick, flying kick, palm, headbutt, slap, elbow) |
| Movement | blur dash (speedforce), streak, teleport / portal hop / vanish, ambush (warp behind), take flight / swoop, thruster boost, leap slam, charge, pounce, web swing / grapple, burrow, riptide surge, roll out (vehicles), bounce, phase, element step; evasive styles (dodge roll, slide, vault, springback, sprint, sidestep, run away, cartwheel, flop) |
| Defense + Counter | parry / riposte, guard (+ deflect), deflect, element ward (repel + status aura), brace / thick hide, perfect dodge, smoke veil, regenerate / healing light, bristle, go intangible |
| Combo | blade flurry, fist barrage, stand-style rush, blink blitz, launcher → slam, suplex / constrict / seize-and-toss, volley, beam sweep, beast maul / frenzy / rend / pack hunt, tail lash, curse (lift + delayed burst), crescent wave, drill, follow-through crit, swarm dogpile |
| Ultimate | element storm (meteor storm, thunderstorm, blizzard, tidal wave, tornado, earthquake + spikes, plague + contagion, psychic singularity, eclipse, meltdown), awakening (grow + empower), meteor dive, thousand cuts, beam cannon, dead aim lock-on, bombardment, summon the horde, stampede, overdrive (time slow + blink blitz), wild card roulette, unbreakable, piledriver / sky drop, assassinate, strafing run, orbital strike, full throttle; personality finishers: beatdown, from the top rope, blind rage, power nap, freak out, showstopper, burst of courage, time stop |

Names come from `vocab.py` (element / flavour / temperament vocabularies; Pokémon get Pokémon move names) and are
VALIDATED: a slot word may only appear in its own slot's move ("<name> attack" can't hit the ultimate), no word
the costume grammar reads after a name ("goku sword", "gojo sunglasses", "red goku", "giant goku"), and a
collision pass simulates `toolbox.rs` over every known phrase (icon / names-list / costume names + variants,
power names, shouts) — a colliding alias is dropped, or the colliding move renamed.

## Numbers

Ultimate cost 40 (Titan), combo 8, defense 4-5, movement 3-4, primary 2-3 — the presence-tier floors the hand
kits use; damage / knockback / radius scale a little with size and temperament (Aggressive hits harder, Lazy
cools down slower). Everything stays in the hand kits' ranges.

## Table size

~4.8k kits / ~25k moves, ~6 MB of RON. `MAX_POWERS` is 12000 (was 4000; ids are u16 on the wire).
`MoveIndex` indexes kit names by first word (a phrase no longer scans every kit name).
