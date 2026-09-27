# AUTO-KITS — a derived Kit for the characters nobody hand-built

"<character> attack / dash / block / counter / combo / ultimate / special" does something in character for the
~2,500 most-named characters (icon packs, names lists, costumes) that no content worker wrote a kit for.

## What ships

* `assets/powers/zz_derived_kits.ron` — GENERATED, checked in, never edited by hand. COMPACT: each entry is a
  normal `kind: Kit` PowerDef (ids `dk_*`, names, kit `fx`) whose moves are spelled as template ids + a few
  character params:

      (id: "dk_garchomp", name: "Garchomp", kind: Kit, fx: "earth", names: ["garchomp", ...],
       derived: Some((el: Earth, tm: Aggressive, sz: 1.12, col: Some((0.3, 0.4, 0.8)),
        mv: [(Slash, "boulder slash"), (Blur, "tectonic surge"), (Parry, "en garde"), (Flurry, "stone onslaught"), (Storm, "earthquake")]))),

* `crates/bd_sim/src/powers/derive.rs` — the templates (`Tpl`): `PowerTable::sanitize` expands `derived` into
  ordinary `MoveDef`s when the table is built (pure, deterministic, every peer identical), so derived kits take
  the exact hand-kit path afterwards: `MoveIndex::kit_move`, `ToolStep::Move { kit: true }`, spark pricing,
  chains, presentation (`fx` style keys; ultimates get title letters, boom, the character's colour, the pack's
  impact frame). Ops are written as RON text and parsed (memoised), so ops that gain fields never break it.
* Slot order = tags: Primary, Movement, Defense+Counter, Combo, Ultimate, then at most one untagged bonus move
  from a signature mechanic (web snare, roar, grow, rewind, portal, homing shot, bomb…).
* No shouts (derived kits add nothing borrowable), no tiers / transforms / companions, never a grant; the bare
  name still summons the costume construct. No slot word ever appears in a derived move name, so "<name>
  combo" reaches the Combo move by its TAG and a hand kit sharing the name wins the tie.

## Budget (MAX_POWERS stays 4000)

`DERIVED_BUDGET = 2500`, further capped at `MAX_POWERS − hand entries − 300 headroom` (read from data.rs and the
hand files at generation time). Candidates are ranked by how likely players are to NAME them: Wikidata sitelinks
of the best character match (`tools/lexicon_build/data/entities.jsonl.gz`, checked in) + curation bonuses
(costume look +30, hand-shaped icon +15). Dropped before ranking: non-characters (military units, generic
creature TYPES — "werewolf", "spinosaurus", D&D monster types — unless Wikidata knows them as a character) and
variants of hand-built characters ("spider man noir", "zombie captain america"). Aliases are deduped (first,
most-named character keeps a shared alias), franchise titles and common single words are never kit names.

## Cost (fastrel, best of N — `cargo run --profile fastrel -p bd_sim --example derived_kits_bench`)

| | without | with | delta |
|---|---|---|---|
| file compiled in | — | 784 KiB | (was 6.3 MB fully spelled out) |
| power table parse + expand | 28 ms (987 powers) | 63 ms (3,487 powers / 17k moves) | +35 ms |
| phrase index build | 3.6 ms | 8.2 ms | +4.6 ms |
| kit phrase reading | 4.1 µs | 7.7 µs | +3.6 µs / phrase |
| lexicon load (power scan inside) | 81 ms | 93 ms | +12 ms |

## Hand kits always win

* The generator SKIPS every character any hand file names (power / tier / kit names, shouts, claims), and
  no derived alias may be a word-prefix of a hand kit name with an apostrophe ("pharaoh's mummy").
* The file is named `zz_…` so it loads LAST: hand ids never move, and every name tie goes to hand content.
* Stale file (a hand kit landed for a character that still has a derived kit): hand still wins at runtime;
  `cargo test -p bd_sim --test derived_kits` prints a STALE warning; `--check` exits 1 — re-run the generator.

## Run it (the lead / anyone after content lands)

    python tools/derive_kits/derive_kits.py            # rewrite assets/powers/zz_derived_kits.ron (~10 s, stdlib only)
    python tools/derive_kits/derive_kits.py --check    # exit 1 if the checked-in file is stale
    python tools/derive_kits/derive_kits.py --show "killer frost" --show krang     # print kits (template per slot)
    python tools/derive_kits/derive_kits.py --sample 50 --seed 7                  # spot-check
    python tools/derive_kits/derive_kits.py --stats                               # template histogram + collision log
    cargo test -p bd_sim --test derived_kits

Deterministic: only checked-in inputs (icons, names lists, costumes, power files, the Wikidata harvest, bd_core's
enums, bd_sim's `MoveOp` list — a template whose op doesn't exist is never picked), CRC hashing, sorted output.

## How a kit is derived (tools/derive_kits/)

`sources.py` reads the data; `derive_kits.py` profiles each character — effective element (the recipe's, or for
Light characters the one its names speak of: "Captain Cold" → Ice), flavour (tech / holy / mystic / spirit / chi /
glitch / speed / ink), weapon (costume accessories, icon payload, name keywords, Blade / Pierce / Blunt), size,
temperament, form (humanoid / beast / kaiju / vehicle kind), pack — and every slot picks the best-scoring
template with a stable per-character jitter (two fire brutes don't get the same kit). Names come from `vocab.py`
(element / flavour / temperament vocabularies; Pokémon get Pokémon move names) and are validated: no slot word,
no word the costume grammar reads after a name ("goku sword", "gojo sunglasses", "red goku", "giant goku"), and
a collision pass simulates `toolbox.rs` over every known phrase (icon / names-list / costume names + variants,
power names, shouts): a colliding alias is dropped, or the colliding move renamed.

| slot | templates (`Tpl`) |
|---|---|
| Primary | Slash / Cleave, Thrust, Smash, Stomp, Stretch, Brawl(Jab / Haymaker / Kick / FlyingKick / Palm / Headbutt / Slap / Elbow / Punch), Bolt(Fireball / Thunder = instant strike at the crosshair / Lift / Ray / Shard / Cutter / Rock / Spit / Orb / Pellets / Arrow / HomingArrow / Knives / Energy), Beam / BeamBounce, Breath (cone + lingering status), Bite, Lash, Spin, Hex |
| Movement | Blur, Streak, Teleport, Vanish, Ambush (warp behind), Fly, Swoop, Boost, Leap (+ slam), Charge, Pounce, Reel, Burrow, Surf, RollOut, Bounce, PhaseDash, ElemStep, Evasive(Roll / Slide / Vault / Springback / Sprint / Sidestep / Flee / Cartwheel / Flop) |
| Defense + Counter | Parry, Guard (+ deflect), Deflect, Ward (repel + status aura), Brace, Evade, Smoke, Heal, Shell, Intangible |
| Combo | Flurry, FistBarrage, Rush, Blitz, Launcher (→ slam), Grab(Constrict / Toss / Suplex), Volley, Sweep, Maul(Maul / Frenzy / Rend / PackHunt), TailLash, Curse (lift + delayed burst), Crescent, Drill, FollowThrough (combo-timing crit), Dogpile |
| Ultimate | Storm (meteor storm, thunderstorm, blizzard, tidal wave, tornado, earthquake + spikes, plague + contagion, psychic singularity, eclipse, meltdown), Awaken (grow + empower), Meteor, Blades, Cannon, DeadAim, Bombard, Summon(the horde), Stampede, Overdrive, Gamble (roulette), Guardian, Piledriver / sky drop, Assassinate, Strafe, Orbital, Throttle; personality finishers: Knockout, Beatdown, TopRope, Rage, Nap, FreakOut, Showstopper, Courage, TimeStop |

Numbers: ultimate cost 40 (Titan), combo 8, defense 4-5, movement 3-4, primary 2-3 — the presence floors hand
kits use; damage / knockback / radius shade a little with size and temperament (Aggressive hits harder, Lazy
cools down slower), all inside the hand kits' ranges.

## Edits AUTO-KITS made in other workers' files (each under its lock)

* `bd_sim/src/powers/data.rs` (TOOLBOX CORE): `PowerDef::derived: Option<derive::DerivedKit>` (serde default,
  skipped when `None`) + the expansion at the top of `PowerTable::sanitize`'s per-power loop. `MAX_POWERS` was
  briefly 12000 during this work and is back to 4000.
* `bd_sim/src/powers/mod.rs` (TOOLBOX CORE): `pub mod derive;`.
* `bd_sim/src/powers/compose/mod.rs` (VERB GRAMMAR): `derived: None` in `composed_power()`'s literal.
* `bd_sim/src/powers/toolbox.rs` (TOOLBOX CORE): `MoveIndex::build` dedupes a kit's names per power (was a scan
  of every kit: quadratic), kit names are possessive-stripped like phrases ("pharaoh's mummy" was unreachable),
  and `kit_first` buckets kit names by first word so a phrase no longer scans every name.

Open for TOOLBOX CORE: `powers_check` lints every derived kit with "no shouts" (by design — derived kits add
nothing borrowable); consider skipping that lint for `dk_*` ids.
