# TOOLBOX QA notes (2026-09-27)

## Shout hygiene — the rule (bd_sim `powers/toolbox.rs`, bd_game `toolbox.rs`, `powers_check`)

Shouts are GLOBAL: anyone saying one fires that move. So a borrowed shout must be DISTINCTIVE, or it
steals ordinary phrases from the core promise ("say anything and it's built").

A shout is **MARKED** (always a move) when
- the data writes it with a capital letter — `("Spirit Bomb", 2)` = a famous PROPER move name, or
- it carries its own character's / power's name — `("kirby flaming hammer", 2)`.

Any other shout **yields to the lexicon** (reads as `Plain`: the console builds / grants / composes) when
1. it is exactly a curated icon's name ("mjolnir", "lightsaber", "zelda shield"), or
2. it has no coined / proper word (every word is an everyday word, `Lexicon::is_everyday_word`) and
   the lexicon composes a clean ordinary thing from it ("flaming hammer", "sand shield"), or
3. it is ONE everyday word ("dash", "smoke", "float", "clap") — never distinctive enough to be global
   (the described-move grammar / the lexicon reads it).

A coined shout the lexicon would only improvise ("fus ro dah", "bamf", "expelliarmus") stays a move.
A LOOSE kit reading (slot word / word overlap) yields to an icon's exact name ("zelda shield"), and —
when the kit name matched is itself everyday words and the phrase has no coined word — to a clean
ordinary thing ("frozen shield"). Exact kit moves and HELD moves are never touched (a holder always
fires their own shouts). `MoveIndex::read` (raw) is unchanged; the game, voice, HUD preview and the
validator read through `MoveIndex::read_judged` with the lexicon's `LexJudge`.

`powers_check --all --hygiene` lists every DEAD shout (yields: prefix it, or Title Case it if famous),
every one-word coined shout the lexicon also builds as a thing (COINED: e.g. "lasso", "rapier"), and
every shout made only of everyday words that still borrows (PLAINWORDS, review list). Kit dead shouts
are warnings; a held power's dead shout is a note (its holder still fires it).

### Authoring
- A famous move's own name made of dictionary words ("Spirit Bomb", "Almighty Push", "Falcon Punch")
  → write it Title Case. Only for names people shout for THAT move.
- A descriptive / generic shout ("sand shield", "summon the dogs") → prefix the character
  ("gaara sand shield"); "<kit> <move>" reaches it anyway.
- Never a bare everyday word as a shout.

## Numbers (corpora in the QA scratch; ~1,263 construct phrases, 520 famous-move phrases)
| | before | after |
|---|---|---|
| construct phrases read as a move (hijacked) | 88 | 25 (famous proper names now marked + exact "<character> <thing>" kit moves) |
| famous-move phrases read as a move | 425 | 402 (the rest were icons of the THING: "fire flower", "adamantium claws", "odm gear"…) |
| shouts that collide with a construct / icon / one everyday word | 1,418 | 203 (all held-power, holder-only) |
| everyday-word shouts that still borrow (review) | 1,808 | 1,611 |

## Notes for other workers
- VERB GRAMMAR: bare everyday verbs ("dash", "smash", "leap", "slam") are no longer hijacked by a kit's
  borrowed shout — `composed()` now gets them in the console. `compose::curated_by_lexicon` and the
  hygiene's `plain_thing` agree on signatures / grants; icons differ (hygiene counts any exact icon
  alias, you compare the icon id) — fine, just be aware.
- MODIFIERS & FUSION: `twist::read::Reader::move_core` uses the raw `MoveIndex::read`; your own judge
  (`Lex::Everyday`) already refuses everyday borrowed cores. With the data sweep, most generic bare
  shouts are prefixed now, so "giant flaming hammer" can't find a borrowed core anyway.
- MYTHOLOGY data (myth_folklore.ron, now QA-owned): 197 generic shouts were prefixed with the god /
  hero ("poseidon tsunami", "hermes caduceus"…); famous names restored Title Case where tests named
  them ("Gungnir", "Ding", "Lift The Mountain", "Wave Of The Iron Fan").
- AUTO-KITS: `derive_kits.py` was NOT deterministic (a set iterated in `priorities()` →
  "cyborg superman" / "lizardfolk" kept or dropped by PYTHONHASHSEED; `--check` flip-flopped STALE).
  Fixed (sorted). Regenerated; `--check` is up to date.

## Open
- Held powers' generic shouts (203, e.g. alchemy "earth wall", earthbending "rock wall") only work for
  their holder now; borrowing them needs the power name first. Intended.
- `amazon` "lasso", `silver_chariot` "rapier", `earthbending` "sinkhole": one-word coined-for-the-lexicon
  held shouts that still borrow globally (validator COINED warnings) — their moves ARE those things.
- 8 cross-file shout clashes remain (anime_modern kits vs base_techniques nen / black flash: the kit
  keeps it; the technique's holder still has it held). Pre-existing, harmless.
