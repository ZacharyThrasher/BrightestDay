# assets/powers/ — the power table (COMBAT.md §5h)

Player powers — ONE template for every iconic power system. Owned by bd_sim (`src/powers`).

**Every `*.ron` file here is a plain RON list of entries (same schema in every file).** Files load in
FILENAME order (byte order), entries in file order; a power's id on the wire is its index in that
concatenation, identical on every peer because the table is compiled into bd_sim
(`crates/bd_sim/build.rs` → `powers::SHIPPED_FILES`). The lexicon reads the same directory next to
`lexicon.bin` at runtime (`Lexicon::power_files`). Keep each file a plain list: the lexicon splits
entries at the first bracket.

Each entry: `id`, `name`, `kind`, `names` (lexicon), `body`, `graft`, `incantations` + `delivery`,
`shouts` (whole phrases → move slot), `moves` (≤ 8: slot 0 = G tap, 1 = G hold, the rest by shout /
by tag), `transform`, `companion`, `pickup` (the look of a fruit / origin pickup), `effects`,
`ascend`, `tiers`.

`kind`: `Paramecia | Logia | Zoan | MythicalZoan` (devil fruits) | `Stand` (JoJo) | `Origin` (superhero
origin kits) | `Technique` (say it: yours until KO) | `Kit` (a character move-set: its names never
grant anything — the bare name stays the costume summon — and "<kit name> <move word>" fires that
move; see docs/toolbox/KIT_DONE once it exists).

Every value is closed-set data; moves are lists of MoveOps that map onto existing world operations
(Cast, Impulse, Blink, Reel, Shove, Field, Room, Swap, Portal, Link, Explode, Carve, TimeStop,
Restore, Transform, SelfStatus, Weigh, SlamOnLand, Yank, Singularity, Tier; new generic ops are
registered in docs/toolbox/OPS.md). The lexicon scans id / name / names / incantations / kind /
companion / tiers[].names / claim / pickup (`bd_lexicon/src/powers.rs`). Shared tuning (fruit cost,
drain secs, rush tokens…) is `PowerRules` in `crates/bd_sim/src/powers/data.rs`.

## Files

| file | contents |
|---|---|
| `anime_classics.ron` | Naruto / Boruto, Dragon Ball, One Piece, Bleach, JoJo: character kits (`kit_*`) + Eight Gates, Byakugan, Observation Haki, Kido, Ninjutsu (SHONEN CLASSICS) |
| `base_cartoons.ron` | TMNT mutagen, Invincible, Ben 10, Power Rangers, Powerpuff Girls, The Incredibles, Beast Boy |
| `base_dc.ron` | Superman, the Flash, Wonder Woman, Aquaman, Martian Manhunter, Shazam, Batman, the emotional spectrum |
| `base_folklore.ron` | Journey to the West (the Great Sage's peach) |
| `base_jojo.ron` | JoJo stands |
| `base_marvel.ron` | Marvel + X-Men origins |
| `base_one_piece.ron` | devil fruits |
| `base_techniques.ron` | every `kind: Technique` (Dragon Ball, Naruto, JJK, Bleach, HxH, haki) |
| `games_adventure.ron` | single-player action / adventure / RPG / platformer / shooter games: Zelda, Mario, Kirby, Sonic, Mega Man, Metroid, Halo, DOOM, God of War, Souls / Elden Ring / Bloodborne / Sekiro, Skyrim shouts, DMC, Bayonetta, Kingdom Hearts, Final Fantasy, Genshin, Destiny, Mass Effect, Half-Life, Portal, BioShock, Dishonored, inFAMOUS, Prototype, Minecraft, Hollow Knight, Hades, … (CONTENT: ADVENTURE GAMES) |
| `movies_comics.ron` | 95 film & comics kits: X-Men, Avengers / Marvel, DC, Star Wars, Harry Potter (the spells are the shouts), LOTR / Hobbit, Matrix, Ghostbusters, MIB, Pirates, Kung Fu Panda, Karate Kid, Kill Bill, John Wick, Terminator, Predator, Alien, Jurassic Park, Godzilla / Kong, Back to the Future, Indiana Jones, Shrek, Frozen, Incredibles, Moana, Encanto, Tangled, Toy Story, Spider-Verse (CONTENT: MOVIES & COMICS; also owns `base_marvel.ron` / `base_dc.ron`) |
| `zz_derived_kits.ron` | GENERATED — do not edit: a `kind: Kit` (`dk_*`) for every character-like icon / costume no hand file names. Re-run `python tools/derive_kits/derive_kits.py` after content lands (docs/toolbox/AUTO_KITS.md). Sorts last so hand content wins every tie. |

New content goes in a new file named for its domain (e.g. `overwatch.ron`, `naruto_kits.ron`). Ids,
names, tier names and shouts must be unique across the WHOLE directory — the validator reports
clashes.

## Validate without recompiling the game

    cargo run --profile fastrel -p bd_sim --example powers_check -- assets/powers/<file>.ron [--coverage phrases.txt]

It reads the files at runtime (content edits never need a bd_sim rebuild to check), parses them with
bd_sim's `PowerTable`, runs the lexicon's own scan, reports cross-file name / shout collisions against
the whole directory, and with `--coverage` prints what each phrase (one per line) resolves to.

## Budget (TOOLBOX CORE, measured 2026-09-27 with the trimmed `zz_derived_kits.ron`)

The table is compiled into bd_sim and parsed once at startup (on the lexicon loader thread, so
nothing hitches); ids are `u16` on the wire.

| what | limit | now (3,453 powers, 2.8 MB) |
|---|---|---|
| entries in the whole directory | `MAX_POWERS` = 4000 — over it is an ERROR (validator, `PowerTable::shipped`, a stderr note at runtime), never a silent truncation | 3,453 |
| total RON size | keep ≤ 4 MB (the generated file ≤ 1 MB) | 2.8 MB (generated 0.8 MB) |
| table parse | ≤ 200 ms fastrel | 125 ms fastrel (≈ 0.5 s debug) |
| phrase index build | — | ≈ 30 ms debug |
| lexicon load (lexicon.bin + icons + this dir) | ≤ 100 ms | ≈ 70 ms fastrel |
| bd_sim rebuild after a data edit | — | ≈ 3.4 s `cargo check -p bd_sim` (the validator needs none) |
| one phrase reading (kit / borrow index) | — | ≈ 3 µs; a console line incl. lexicon checks 2–7 ms (debug) |

Raising `MAX_POWERS` is cheap (u16 space is 65,535); the real costs are the parse time and binary
size above — grow the generated file only with that headroom in mind.
