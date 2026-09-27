# COMPOSED MOVES — the describe-a-power grammar (VERB GRAMMAR worker, 2026-09-27)

Named moves (assets/powers/*.ron) are the famous shortcuts. This is the open end of the toolbox: a phrase
whose head is a **self-action verb** becomes a real move of the player's own, even though nobody wrote it:

    verb × style/element × modifier × place × target × count × duration

"shadow teleport behind ethan", "lightning dash through him", "gravity punch", "ice clone", "swap with the
dragon", "rewind 3 seconds", "fire uppercut", "phase through the wall", "pull everyone to me", "leap onto the
tower", "blink above him and slam", "turn invisible", "absorb his attack", "freeze everyone", "hex him",
"summon a dragon, shadow teleport behind it, fire uppercut".

Code: `crates/bd_sim/src/powers/compose/` (`mod.rs` spec + table, `grammar.rs` phrase → spec, `build.rs`
spec → ops, `exec.rs` host firing), ops `powers/ops/relocate.rs` + `pass_through.rs`. Vocabulary:
`assets/verbs.ron` (compiled into bd_sim). Client: bd_game `toolbox::command_for` / `composed`.
Tests: `cargo test -p bd_sim --test compose` (golden: `crates/bd_sim/tests/data/composed_phrases.{txt,golden}`,
454 phrases typed + spoken), `cargo test -p bd_game --bin brightest_day toolbox::` (precedence with the real
lexicon).

## 1. Precedence (per chain step)

1. a HELD power's phrase (shout / tier / incantation) — TOOLBOX CORE
2. a KIT move ("<kit> <move word>") — TOOLBOX CORE
3. a power NAME (grant) — the lexicon
4. a BORROWED shout (exact shout of any power / tier / kit) — TOOLBOX CORE
5. **a DESCRIBED move** — this grammar, when (a) every word is understood, (b) there is exactly one self-action
   verb, and (c) the lexicon does not know the exact phrase as something curated: a signature ("kamehameha"),
   a grant / stand rush, or an icon whose name IS the whole phrase (`compose::curated_by_lexicon`)
6. a construct — the lexicon, as ever

So "a dragon", "giant sword", "hammer", "drop a meteor on ethan", "throw a fireball", "hit mia with a meteor",
"cage ethan" stay constructs / deliveries (an unknown noun is never swallowed), "almighty push" / "bamf" /
"ground pound" stay the named moves that shout them, and "dash through him" is a described move. Many plain
verbs are ALSO somebody's shout today ("dash", "teleport", "phase", "smash", "flurry", "deflect"…): those
borrow the famous version first — that is the named-move shortcut the brief asks for. Any extra word ("fire
dash", "dash through him", "teleport behind ethan") makes it a described move.

Mixed chains work: every separator the chain splitter knows ("->", "then", ",", "and") splits described
steps too ("blink above him and slam" = 2 steps). An untargeted aimable link after a targeted one aims at
the SAME target (`Target::Same`): "teleport behind ethan, fire uppercut" uppercuts Ethan;
"summon a dragon, shadow teleport behind it, fire uppercut" teleports behind the new dragon.

## 2. Grammar (`grammar.rs`)

Word classes (all data in `assets/verbs.ron`, longest multi-word match first, free word order):

| class | examples |
|---|---|
| verb (57 entries, ~715 words, 48 kinds) | teleport / blink / warp / bamf / flash step / shunpo · dash / zip / lunge · charge / ram / tackle · leap / jump · fly / hover · dodge / roll · phase / intangible / walk through · swap / switch places · rewind / recall / go back in time · vanish / turn invisible / smoke bomb · grow / shrink · punch / hit / jab / haymaker · kick · uppercut / launcher · slam / ground pound / dive · stomp / quake · smash · slash / cut · flurry / barrage / beat up · spin · grab / choke · throw / suplex · pull / yank / get over here · push / force push · lift · crush / pin down · explode / self destruct · freeze / burn / shock / poison / blind / stun · block / guard · parry / counter · reflect / deflect · absorb · copy / mimic · clone / decoy · heal · harden / armor up · stop time / slow time · disarm · silence / hack / emp · lock on · hex / doom / curse · finish (him) · blast / laser / eye beams |
| style / element (28 styles, 291 words) | fire, ice, lightning, water, wind, earth (iron, steel…), sand, poison, psychic, gravity, shadow, smoke, void, cursed, blood, holy, chi (ki, aura…), atomic, speed (speed force, sonic…), ghost, glitch, tech, rubber, ink, steam, paper, cards, hardlight |
| place | behind, in front of, above / over / on top of, below / under, beside / next to, around, through, to / toward / at, away / back, onto, into, up, down, forward |
| target | player names · him / her / them / that guy / you → `Pronoun` · it / that → `It` · "the/my/that <thing>" → `Thing` · everyone / all of them → `Everyone` · the nearest / whoever's closest → `Nearest` · me / myself → `Me` · the wall / the tower / the ground / over there → `Ground` |
| count | twice, three times, 3x, double, triple, "3 punches" |
| duration | 3 seconds, for 2 seconds, 1.5s, half a second |
| modifier | starter list (MODIFIERS & FUSION owns it, §5): big (giant, mega, super…), quick (fast, swift…), long |
| filler | please, now, the, i'll, let me, go, use, move, jutsu, "in the face", "as hard as i can"… |

Rules:
* **Full coverage**: a word that is none of the above ⇒ not a described move (it stays a construct).
  Exception: a word right after an article / possessive / place word is a `Thing` ("the dragon", "behind
  dragon"); a guess costs like a filler word, so any known reading wins.
* **Exactly one verb.** Two verbs ⇒ not a single described move ("blink above him and slam" splits into two).
* **Noun-verbs** (`noun_words`: hook, meteor, spike, slide, rocket, ram, duck, turtle, tank, trap, belt,
  cloak, flame, ice…): bare (or "a hook") they stay the THING; with any complement they are the verb
  ("hook him", "ice him", "cloak myself").
* **Voice parity**: typed and spoken go through the same parser. Whisper output (capitals, punctuation, curly
  apostrophes) is normalised; `respell` repairs mishearings ("tele port", "up her cut", "faze", "in visible",
  "shun po", "rewined", "swop"…); inflections fold ("teleporting", "slammed", "punches", "phased"); one typo is
  forgiven on words ≥ 5 letters ("teleprot", "uppercutt", "electricute").
* **Names**: exact, possessive ("ethan's"), joined ("alex99" = Alex_99), one word of a multi-word name
  ("tony" = Big Tony), one typo ("ehtan"), and — in the game — the lexicon's own sound-alike matcher (voice:
  "eaton" → Ethan, the same one "missile at Jake" uses). Never special-cased: any display name works.
* Search: a small exhaustive segmentation search (budget 20k nodes) picking the parse with the fewest filler
  segments, then fewest segments. ~0.03 ms per phrase.

## 3. Kinds → ops (`build.rs`) — the closed set in code

Every kind has default numbers for a FAIR, satisfying move and a chain-friendly ending, scaled by the verb's
`power` / `reach` / `cd` (verbs.ron), the style (element on every hit + a rider), modifiers, place, target,
count and duration. Explicit target ⇒ the host aims the move at it (§4); "everyone" ⇒ the area form.

| kind | untargeted | targeted / place | ending |
|---|---|---|---|
| Teleport | Blink 12 m (×count, ≤ 30) · Up = 7 m straight up · Away = 9 m back · Through = PassThrough | Relocate: behind (1.8 m + their width, from THEIR look), front, above (5 m over their top), onto, beside / around | airborne beside / above them; elemental styles burst on arrival (fire, lightning…), smoke leaves a blinding puff, gravity pulls things in |
| Dash | Impulse 24 m/s (elemental: brush-past Shove) | at X: Pounce (homing lunge-hit, bounce) · through X: Ram through (hits everyone on the line) · behind X: Relocate | airborne |
| Charge | Ram 0.55 s (into = pin to the wall, through = pass through) | | grounded |
| Leap | Leap to the crosshair point · Up / Away = jumps | onto X / the tower: Leap there; heavy / gravity / big: + SlamOnLand | lands |
| Fly | Impulse up + Float 2.5 s (duration ≤ 4) | | airborne |
| Dodge / Phase | Evade 0.45 s (+ step) / Phase 1.2 s (intangible; firing ends it) | through the wall: PassThrough · through him: Ram through + Evade | |
| Swap | Transpose (players or constructs; "it", "the dragon") | | |
| Rewind | Rewind 2 s (duration ≤ 3) | | |
| Vanish | smoke Zone (blinds enemies inside) + Relocate 7 m back | | (true invisibility is an open issue) |
| Grow / Shrink | SelfResize 1.5 / 0.6 for 6 s | | |
| Punch / Kick / Uppercut / Smash | Shove cone (+ lunge step; uppercut rises with it) | X: Pounce (the lunge IS the hit) · ×count: Barrage · everyone: 360° | uppercut ends rising |
| Slam | dive (Impulse −34) + SlamOnLand | X: Pounce + SlamOnLand · onto X: Relocate above + dive | grounded |
| Stomp / Explode | Explode at you | everyone: bigger radius | grounded |
| Slash | close cut + travelling Wave crescent | ×count: Barrage · everyone: 360° | |
| Flurry / Spin | Barrage 6 hits / 360° Shove + hop | | |
| Grab / Throw | Grab (hold, throw along aim; up / behind) | "grab him" from afar: Hook reels them in | |
| Pull | Yank | everyone: attract Field · myself to the wall: Reel · the wall: Haul (tears a chunk out) | |
| Push | Shove cone | X: Afflict knockback · everyone: Explode push (Almighty-Push style) | |
| Lift / Crush | Afflict Float lift / Weigh down | everyone: Float / Chill zone | |
| Freeze / Burn / Shock / Poison / Blind / Stun | Afflict with the status (capped: stun ≤ 1.2 s…) | everyone: Explode + lingering Zone | |
| Guard / Counter / Absorb / Reflect / Copy | Counter guard / Counter answer / Counter / Deflect / CopyLast | | |
| Clone | casts N hard-light biped clones (count ≤ 3) + sidestep | | |
| Heal / Harden / TimeStop | Mend / Empower (armor) / TimeStop 1.5 s (Titan) | | |
| Disarm / Silence / LockOn / Hex / Finisher / Blast | Disarm / Lockout ≤ 2.5 s / LockOn channel / DelayedHit / Finisher (combo crit) / BounceBeam (×count = bounces) | everyone: area forms | |

Fairness: every status is capped (the `ops/afflict.rs` caps), TimeStop never freezes anyone, cooldowns per
kind (0.35 s jab … 12 s time stop), and every move pays sparks.

## 4. The spec and transport (host-authoritative, like a borrowed move)

```rust
// bd_sim::powers::compose — ~16 bytes on the wire (bincode), closed set, quantised
pub struct ComposedMove {
    pub verb: u16,          // id16(verbs.ron `id`) — FNV folded, never 0; unknown ⇒ rejected
    pub style: u16,         // id16(style id), 0 = none; unknown ⇒ dropped
    pub place: Place,       // enum (15)
    pub target: Target,     // Aim | Me | Pronoun | It | Same | Nearest | Everyone | Player(PlayerId) | Thing(u16) | Ground
    pub count: u8,          // 1..=5
    pub secs: u8,           // tenths of a second, 0 = verb default, ≤ 120 (each kind clamps further)
    pub mods: [u8; 4],      // modifier ids (MODIFIERS & FUSION), sorted, unique, unknown ⇒ dropped
}
```

* Client: bd_game `toolbox::command_for(lex, held, powers_on, text, spoken, roster)` → `Cmd::Toolbox { steps:
  [ToolStep::Composed(spec), …] }` → `NetCmd::Toolbox` (**protocol v41**) → host `World::toolbox` → the chain
  runner fires `ToolStep::Composed` through `World::fire_composed` (compose/exec.rs): the SAME gates as a
  borrowed move (alive, powers on, not stunned / reciting / charging / seated / locked out), then
  `spec.sanitized()` → `spec.expand()` (pure, identical on every peer) → cooldown → price → target → ops.
* **Cooldown / staleness**: `Toolbox::cds` slot keyed by `spec.cd_key()` = hash of the MECHANICAL identity
  (verb + place + area form) — re-wording ("fire teleport behind him" vs "shadow teleport behind ethan") never
  dodges a cooldown or the staleness price. Key layout: bit 31 set, bits 8..24 = 0xFFFF (so
  `MoveRef::unpack(key − 1)` names power 0xFFFF, which never resolves), 15 hash bits.
* **Price**: `chain::move_sparks(tuning, &expanded, uses, bar)` = the presence tier of what the ops do
  (`chain::move_tier`) → the spark table (Tiny 1 / Normal 2 / Big 3 / Titan 5), +1 per repeat inside the
  staleness window. A clone's Cast also pays the cast pipeline's own price. Paid only if the first op did
  something (a teleport with nowhere to go costs nothing).
* **Targets** (resolved by the host at fire time): `Player(id)` (alive, ≤ 90 m; else the aimed enemy),
  `Pronoun` (the last player a composed move went for, ≤ 4 s; else the enemy nearest the crosshair),
  `Same` (the last target ≤ 8 s), `Nearest`, `It` (your newest construct, else the construct under the
  crosshair), `Thing(h)` (the nearest live construct whose name has that word; else the aimed construct; else
  the aimed enemy — so a misheard name still does something sensible). The host swings the move's aim ray
  (`powers.move_aim`) at the target, so EVERY aimed op (lunges, spells, pulls, swaps, beams) goes for it;
  `Relocate` reads the target directly (`OpState::compose[i].now`).
* **Events**: `PowerEvent::Composed { player, spec, from, to, dir, step, sparks }` then
  `PowerEvent::Fired { mv: compose::exec::COMPOSED_REF (power 0xFFFF), how: FireHow::Composed, .. }`.
  Presentation: `compose::interned(&spec)` → `&'static MoveDef` (name, ops, `fx` style key) — bd_game movefx
  already turns `Composed` into a `MoveFxFired` (borrowed chip, style beats). `ToolboxSnap::queue` shows a
  pending composed step as `chain::QUEUE_COMPOSED` (0xA000_0000).
* Prediction: like borrowed moves — the host moves you, the client reconciles (no special path).

## 5. For MODIFIERS & FUSION and AUTO-KITS

* **Modifiers** (MODIFIERS & FUSION's `powers/twist/`, `assets/move_modifiers.ron`, ids `n` 1 big / 2 quick /
  3 long match the composed starter list in `assets/verbs.ron` `modifiers:`): the composed grammar consumes
  ONLY those three starter words itself ("giant punch", "quick dash", "long blink") and applies them in
  `compose/build.rs::knobs_in` (× power / reach / cooldown). Everything else ("homing", "rapid fire",
  "reverse", "delayed", counts beyond `count`) is the twist reader's: the planned `TwistBase::Composed(spec)`
  wraps a composed core, and its TwistSide carries the modifier bytes. **Rule to avoid double application:**
  a twisted composed base keeps `spec.mods` EMPTY (the twist side owns them) — or, if you prefer the spec to
  carry them, replace the `knobs_in` match with a call into `twist::ops::twist` on the built `MoveDef` and
  drop the starter list (one-line change in `build_in`; tell me and I'll do it). `sanitized_in` keeps only
  ids `VerbTable::modifier` knows (the starter three) until then.
* **Fusions** ("fire and ice punch"): the grammar keeps the FIRST style today (a second style word is read but
  dropped). A fusion byte in `mods` is the natural home.
* **AUTO-KITS** may emit derived moves as specs: `ComposedMove { verb: compose::id16("teleport"), style:
  compose::id16("smoke"), place: Place::Behind, target: Target::Pronoun, count: 1, ..Default::default() }` →
  `ToolStep::Composed`. `spec.expand()` gives the ops if you want to bake them into a `.ron` kit instead.

## 6. Tools

* `cargo run --profile fastrel -p bd_sim --example powers_check -- <files> --coverage phrases.txt` prints
  `COMPOSED <reading> → [ops]` for described moves (sample roster Ethan / Mia / Sam / Nova); only real
  constructs are left in the FALLTHROUGH gap list.
* Phrase preview (`bd_shapes --example phrase`): a described move prints `DESCRIBED move "…" (reading;
  target) - style - tier / sparks - cooldown - active phase - ops`, chains included.

## 7. Data editing (`assets/verbs.ron`)

* `id`s are wire identity (16-bit hash): never rename; add freely. `cargo test -p bd_sim --test compose`
  checks id collisions, noun words, style fx keys against `assets/tuning/move_fx.ron`, that every verb × place
  × target builds a sane, priced move, the 454-phrase golden, and that every headline verb DOES something in a
  real world.
* New BEHAVIOUR = a new `VerbKind` in code (a builder arm), never a special case for a word or a name.

## 8. Open issues

* True invisibility: "turn invisible" is a smoke-bomb vanish (blinding puff + reposition). A real Cloak needs
  a replicated player flag + a render hook (a new system — out of the feature freeze; left for the lead).
* Many bare verbs are borrowed shouts ("teleport" → a named teleport, "dash", "phase", "flurry"…): correct
  per precedence, but it means the bare word never reads as the generic version. Styled / targeted forms do.
* The host can't turn the player's camera: after "teleport behind him" the crosshair still points where it
  did — chains solve it (`Same`), a lone follow-up "punch" aims at the crosshair (say "punch him").
* Named targets need the roster: the console passes it; the preview / validator use a sample roster.
* A second style word is dropped (fusion is MODIFIERS & FUSION's).

## 9. MODIFIERS & FUSION — twisted and fused moves (MODIFIERS & FUSION worker, integrated)

Code: `crates/bd_sim/src/powers/twist/` (`mod.rs` table + wire, `read.rs` phrase reading, `ops.rs` the
generic transform, `fuse.rs` fusion rules, `exec.rs` host firing). Data: `assets/move_modifiers.ron`
(compiled into bd_sim: every peer twists identically). Tests: `cargo test -p bd_sim --test twist`
(golden: `crates/bd_sim/tests/data/twist_golden.txt`, 380+ phrases typed + spoken; every modifier word ×
3 cores; transforms, caps, pricing, fusion, host, wire, determinism) and
`cargo test -p bd_game --bin brightest_day toolbox::` (the console path with the real lexicon).

**Wire** (`chain::ToolStep::Twist(Box<TwistStep>)`, protocol v42):
```rust
pub struct TwistStep { pub a: TwistSide, pub b: Option<TwistSide> /* fused with */, pub mods: Vec<u8> /* on the whole */, pub shot: u8 /* host-only follow-ups; reset to 0 on anything a client sends */ }
pub struct TwistSide { pub base: TwistBase, pub mods: Vec<u8> /* modifier wire bytes, ≤ 8 */ }
pub enum TwistBase { Move { mv: MoveRef, kit: bool }, Held(u8), Cast { recipe, shape, signature } /* a famous cast the lexicon knows */, Composed(ComposedMove) }
```
Modifier **wire bytes** are `move_modifiers.ron` `n` (stable, never reused; 1 giant = "big", 2 quick, 3
long are the composed starter ids, so `ComposedMove::mods` bytes mean the same thing). The host
rebuilds everything from refs + bytes; nothing numeric travels.

**Rule: a modifier is applied ONCE.** A twisted described move carries its spec with `mods` EMPTY:
the spec's own bytes are moved into the twist side (`twist::split_composed`, done by the client AND
again by the host). Plain (untwisted) described moves still apply their starter bytes in
`compose/build.rs::knobs_in`, exactly as before — nothing changes for them.

**Reading precedence** (`twist::read::Reader`, called first per chain step by bd_game
`toolbox::twist_step`): a line is a twist only when a MOVE sits under the modifiers. Cores are tried
over every prefix / suffix split, fewest stripped words first, in three passes: (1) a named move (held
/ kit / borrowed shout; a borrowed shout that names an everyday thing — "hammer", "fireball" — is
refused, the lexicon's construct wins), (2) a DESCRIBED move (your `composed()`; only vocabulary the
grammar shares — styles / elements and `composed_words` 1 2 3 6 7 — may stay inside it, any other
modifier word is stripped out: "rapid fire punch at ethan" = the twist `rapid` on "punch at ethan"),
(3) a famous cast (a lexicon signature: "spirit bomb"). A phrase that is already a move as said is
never twisted ("giant rasengan" is the authored one; "giant punch", "fire punch", "triple shadow
teleport" stay yours).

**Second style word** ("fire and ice punch", "punch with fire and ice", "fire and ice rasengan"): the
reader splits it into the same move once per element and FUSES them (`fuse` rule T, "one move, both
elements": the first element's hits + the second element's status / burst on whoever they're aimed
at, a Merge windup of both). The grammar can keep dropping the second style — the twist reads it first.

**Host**: a twisted described move resolves its target with your `compose_resolve` / `forced_point`,
swings every copy at it, sets `OpState::compose[i].now` for `Relocate`, remembers `last` for "him" /
`Same`, cools `spec.cd_key()` in the toolbox table and fires `OpEvent::Twisted { a: BaseRef::Composed(spec), .. }`
then `PowerEvent::Fired { mv: COMPOSED_REF, how: FireHow::Composed }` (no `PowerEvent::Composed`: the
movefx intake plays the twisted def from `twist::exec::interned`).
