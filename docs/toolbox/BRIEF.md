# THE TOOLBOX — shared brief for the franchise-powers wave (2026-09-27)

## What the owner wants (their words, paraphrased tightly)
- Players should feel they carry a **toolbox of every power they've ever seen in fiction** and can
  call any of it with their voice or keyboard at any moment. The skill ceiling is **how many powers
  they can chain with their imagination**: "nightcrawler teleport -> reinhardt ultimate -> doomfist
  punch -> optimus prime combo" should just work as a sequence.
- Players should be **constantly cycling** through everything they can think of.
- **No more "I say sharingan and a projectile comes out."** A famous power must do the thing it is
  famous for, the way it's famous for doing it.
- Everything should feel as **unique / powerful / special as possible** — "the illusion that we've
  thought of anything they could possibly say."
- The quality bar the owner named: **Hollow Purple** (coolest attack) and **Sun Wukong's Ruyi staff**
  (coolest weapon). Formula, verbatim: *"unique flavor/control/playstyle + dramatic/cool presentation = fun"*.
- Same-mechanic moves must still look and feel different: *"for things like stretchy punch it would be
  cool if it looked/felt different than stretchy pole"* — Gum-Gum Pistol is a rubber arm that visibly
  stretches from the shoulder, wobbles, and snaps back like a band; the Ruyi staff is a rigid golden
  pole that telescopes with a metallic ring. Same verb, different body, different sound, different
  timing. Apply that thinking everywhere (a Flash dash ≠ a Nightcrawler blink ≠ an Instant
  Transmission ≠ a Minato Flying Thunder God).

## Standing owner rules you must not break (docs/DESIGN.md, docs/COMBAT.md, memory)
- No AI/LLM in the game. Vocabulary is data; mechanics are a closed set in code. Content ships as data.
- **Generalize over special-casing.** Famous moves are showcases of GENERIC rules keyed off data/
  mechanic combos (Spirit Bomb → generic hold-to-grow energy ball). If you need new behaviour, add a
  generic MoveOp named for its behaviour (`MarkRecall`, `CopyLast`, `GazeStun`), never for a franchise.
- Mystery tenet: never list options, no codex, no autocomplete, no explaining text. Titles/cut-ins
  when a move fires are fine. Never take control away from players (no freezes; slows/stuns must be
  short and escapable; TimeStop-like moves slow others, never freeze input).
- Creativity → new *behaviour*, never bigger numbers. Contrast keeps big things big (presence tiers).
- Voice == typing: every phrase must work spoken. Add Whisper-style mishearings/phonetic spellings to
  names/shouts the way existing entries do ("super sayan", "rinne gan", "night crawler").
- Costumes: a bare character name ("doomfist") summons a construct wearing that costume — keep that.
  "<character> <move>" ("doomfist punch", "reinhardt ultimate") must fire THAT character's move.
- Willpower is spent in SPARKS (host rule, default 8). Costs come from the presence tier model the
  readability worker is landing (Tiny 1 / Normal 2 / Big 3 / Titan 5 / Signature 6; repeats cost +1
  spark per staleness step — this is what pushes players to cycle). Don't hand-tune spark prices.
- Profanity OK; slurs/hate never. No copyrighted text beyond move/character NAMES (no quotes, no
  lyrics, no long incantations copied from scripts). Franchise names are private-use; add the property
  to THIRD_PARTY_NOTICES.md if it's new there.

## Where things live
- Powers: `assets/powers.ron` today (2,476 lines, 111 entries). **The core worker is splitting it into
  `assets/powers/*.ron`** (one plain list per domain file, same schema). Until
  `docs/toolbox/SPLIT_DONE` exists, write NEW entries in your own new file (named below) and do NOT
  edit existing entries in assets/powers.ron. After it exists, existing entries for your domain live in
  the split files — you may deepen/rework them there (check the file isn't owned by another worker).
- Schema: `crates/bd_sim/src/powers/data.rs` (PowerDef, MoveDef, MoveOp, tiers, effects, ascend,
  juice), executed by `exec.rs`; phrase lookup `mod.rs::parse_phrase`; lexicon scan
  `crates/bd_lexicon/src/powers.rs`. Read the header comment of assets/powers.ron and the TECHNIQUES
  section (ultra_instinct, rinnegan = "the flagship", gomu_gomu gears) for idiom.
- Other data you may extend: `assets/signatures.ron` (Hollow-Purple-style signature tags, Merge
  windups), `crates/bd_lexicon/data/relations.ron` (named-move words), `assets/icons/*.ron` (things you
  can summon — see docs/ICONS_SCHEMA.md), `assets/costumes/characters.ron` (looks),
  `assets/tuning/combat.ron` (`legendary:` generic weapon moves), `assets/tuning/presence.ron`.
- Reference implementations of the bar: Hollow Purple (signatures.ron Merge windup + bd_sim
  signature/), Ruyi staff (Mechanic::Extends, folklore_cn.ron + combat rules), palm beams/orbs
  (bd_sim signature/palm.rs, orb.rs), legendary weapons (bd_sim legend.rs), Ascend forms.

## New kinds / features the core worker (TOOLBOX CORE) is landing — author against them
1. **Borrowed moves**: saying/typing ANY move's shout — of any power, tier or kit, held or not —
   fires that move right now as a one-shot. Held powers still take priority (their tier version).
2. **`kind: Kit`**: a character move-set (Nightcrawler, Doomfist, Reinhardt, Scorpion…). A Kit's
   `names` NEVER grant or transform you and never steal the bare name from the costume summon; they
   are prefixes: "<kit name> <move word>" fires that move. Moves can carry
   `tags: [Primary | Movement | Defense | Ultimate | Combo | Counter]` so "<name> ultimate / ult /
   super / special / combo / dodge / attack" finds the right move without you listing every combo.
   Kits may have up to 8 moves. Everything else (effects, ascend, costume look) as in other entries.
3. **Chains**: one line / one utterance with separators ("->", ">", ",", "then", "into", "and then")
   runs a sequence of moves, each firing as the previous one's active phase ends, re-aimed at the
   crosshair when it fires. So each move's feel of *ending* matters (a dash that ends airborne sets
   up an aerial move, a slam that ends grounded sets up a launcher…). Design for chains.
4. A validator you can run without recompiling bd_sim:
   `cargo run --profile fastrel -p bd_sim --example powers_check -- assets/powers/<file>.ron`
   (until it lands, validate with a scratch test calling `bd_sim::powers::PowerTable::parse`
   and `bd_lexicon`'s `parse_powers` on your file's contents).
5. `docs/toolbox/OPS.md`: the registry of MoveOps. **Before adding a new generic op, claim it there**
   (append a line: name, one-sentence behaviour, your worker name, status). If someone already claimed
   it, use theirs. New ops go in their own file `crates/bd_sim/src/powers/ops/<snake_name>.rs` with only
   minimal registration lines in data.rs / exec.rs, edited under the lock
   `target\locks\powers_ops.lock` (see docs/COORDINATION.md). Ops must be deterministic, alloc-free
   per tick, and change nothing when unused (the FFA golden hash 0x4d16_0c05_399e_513e must not move;
   if it does, find out why before reporting).
6. The PRESENTATION worker is building a generic move-presentation grammar (Hollow Purple grammar
   for every move: gather → release → world reaction → clean exit, per op kind, per element/style;
   distinct bodies like "rubber arm" vs "rigid pole"; per-op sound). It will add optional juice/style
   keys — watch `docs/toolbox/PRESENTATION.md` and use them in your data when they appear.

## What "done" looks like for a content worker
- Coverage breadth: the properties and moves players would **actually yell**, deepest for the most
  popular. Think "what would a 14-year-old and a 35-year-old both shout mid-fight". Aim for several
  hundred callable moves per domain across kits/techniques/tiers, with every alias/mishearing.
- Uniqueness depth: every headline move has (a) a distinct control feel (how you aim/hold/release/
  steer/combo it), (b) the thing it is famous for mechanically, (c) presentation keys (title, colour,
  style, sound), (d) a sensible chain ending. No two headline moves in a property should share a
  recipe. Generic projectile reskins are a failure unless the move genuinely IS a thrown thing — and
  even then make it the famous version (Amaterasu: black flames that stick and spread and don't go
  out; Rasengan: a hand-held grinding sphere you drive INTO someone; Chidori: a lightning lunge).
- Audit the existing entries of your domain (after SPLIT_DONE) for "projectile comes out" laziness
  and fix them (sharingan's G-tap fireball is the owner's example: its identity is reading/copying
  and genjutsu, fireball can stay as a shout).
- Tests: golden phrase tests (phrase → which move/kit fires, including chains and mishearings) and a
  behaviour assertion per new op. Keep `cargo test -p bd_sim --test powers` and the lexicon power
  tests green; name collisions with other files are YOUR bug (the lexicon warns "taken over").
- Report: counts (kits, moves, aliases), the 10 moves you're proudest of and why they feel unique,
  new ops, anything you needed but couldn't build, and a frame sheet / screenshots of 3-4 headliners
  (offscreen/record modes; the owner may be playtesting).

## Process (docs/COORDINATION.md)
- Shared tree, shared `CARGO_TARGET_DIR=target\play`, `CARGO_BUILD_JOBS=8`, `BD_HIDDEN_WINDOW=1`.
  Read `target\build_status.txt` before building; stay green; lock hot files; never commit (the lead
  commits green checkpoints). Don't touch files another worker owns without a lock + note.
- Don't build zips or release builds.
