# POLISH: WESTERN — audit (movies / comics / TV / base Marvel · DC · cartoons)

Files: `assets/powers/movies_comics.ron`, `tv_cartoons.ron` (minus the Transformers kits: TRANSFORMERS FEEL
owns those), `base_marvel.ron`, `base_dc.ron`, `base_cartoons.ron`. Bar: Hollow Purple + Ruyi staff —
"unique flavour / control / playstyle + dramatic / cool presentation = fun".

## Method

A scratch script parsed every entry (1,102 moves in 297 powers / kits, Transformers excluded) and scored each
move on **blandness**:

| flag | meaning | weight |
|---|---|---|
| generic-ops | only Cast / Shove / Impulse / Explode / Blink / SelfStatus / Field | 1.0 |
| recipe ×N | the same op sequence (≤ 2 ops) as ≥ 6 other moves | 0.5 |
| default-fx | no move / power `fx` and no element: draws as the default hard light | 1.0 |
| ult-no-impact | an Ultimate with no `juice.impact_frame` | 1.0 |
| ult-no-title / ult-no-boom | an Ultimate with no title card / no sub-bass | 0.5 / 0.3 |
| no-juice / no-color | no juice at all / no colour | 0.7 / 0.2 |
| cast-only | the move is one Cast and nothing else | 0.3 |

**Popularity** = the best Wikidata sitelink count among the kit's names (`tools/lexicon_build/data/entities.jsonl.gz`),
sanity-checked by hand (sitelinks under-count comics characters and over-count words like "Neo"), plus the
brief's priority list (Spider-Man, Iron Man, Thor, Hulk, Cap, Strange, Wanda, Wolverine, Nightcrawler, Magneto,
Superman, Flash, Batman, Wonder Woman, Green Lantern, the Force + lightsabers, Harry Potter, Gandalf, bending,
Stranger Things, The Boys, Invincible, Ben 10, TMNT, the Witcher).

## Before — the numbers

| | count |
|---|---|
| moves audited | 1,102 |
| Ultimates | 194 |
| … with no impact frame | **190** (98 %) |
| … with no boom | 96 |
| moves drawn in the default style (no fx, no element) | **234** |
| kits / powers with no style of their own | 127 |
| moves with no juice at all | 534 |
| moves that are a single Cast | 239 |

## Systemic findings (fix once, everywhere)

1. **Ultimates had no impact frame.** Only Omni-Man carried one. Every Ultimate now gets a frame in its
   property's look (see below), plus boom / title / shake where missing. Heavy non-ult hits (Ram / Grab /
   Finisher ≥ 14 dmg or ≥ 30 kb, other hits ≥ 18 dmg) get one too.
2. **The look per property was unchosen.** Western comics now land on the halftone **comic** POW panel;
   magic light (Strange, Patronus, Gandalf, Jedi, Lanterns, He-Man, Avatar State) on **holy**; horror /
   cosmic / dark (Wanda, Thanos, Sith, Voldemort, Vecna, Eleven, Nightcrawler's BAMF) on **void**; the
   Matrix, Ghostbusters, Terminator, Rick, Doctor Who on **glitch**; bending, Samurai Jack, kung fu,
   Kill Bill, John Wick, monster films on sumi **ink**.
3. **Wrong bodies.** Magneto's `fx: "metal"` aliased to the Ruyi RIGID body — his metal storm drew a
   telescoping gold pole to the shards. Superman's power-level `holy` painted his FREEZE breath gold.
   Firebending / Azula / Iroh / Zuko lightning drew as fire. Witcher Signs were all cyan chi (Igni is fire,
   Aard a telekinetic blast, Quen a golden shield…). Homelander's / Superman's / Cyclops' eye beams came
   from the hand, not the eyes.
4. **Identity without a body.** Spider-Man's webs drew as hard-light energy; the Force as a magenta
   psychic halo; wand spells as hard light; Strange's magic as generic gold; Wanda's chaos as the same
   magenta as Jean Grey; Cap's shield / Wolverine's claws / TMNT katanas as default energy.
5. **Same verb, same feel.** Thor's hammer flight and Superman's flight were the same
   `Impulse + Float`; Force jump was an unaimed hop.

## Ranked (popularity × blandness, before) — top 45

| kit | file | sitelinks | bland / move | flags |
|---|---|---|---|---|
| Deadpool | movies_comics | 56 | 2.96 | default-fx 5, generic-ops 3, no-juice 3, cast-only 3, ult-no-impact 1 |
| Rocket Raccoon | movies_comics | 25 | 3.40 | generic-ops 3, default-fx 3, no-juice 3 |
| Gifts of the Amazons | base_dc | 84 | 2.50 | default-fx 2, no-juice 1 |
| Sokka | tv_cartoons | 25 | 3.17 | default-fx 3, generic-ops 2, ult-no-impact 1 |
| Harley Quinn | movies_comics | 46 | 2.78 | generic-ops 4, default-fx 4, no-juice 2, ult-no-impact 1 |
| The Shredder | tv_cartoons | 66 | 2.57 | no-juice 2, generic-ops 2, ult-no-impact 1 |
| The Fellowship | movies_comics | 58 | 2.56 | generic-ops 5, default-fx 5, no-juice 4, cast-only 4 |
| Catwoman | movies_comics | 56 | 2.53 | default-fx 3, no-juice 3 |
| Captain America | movies_comics | 78 | 2.37 | default-fx 6, generic-ops 2, no-juice 2, ult-no-impact 1 |
| Jurassic Park | movies_comics | 78 | 2.30 | generic-ops 4, no-color 4, default-fx 3, ult-no-impact 1 |
| **Wolverine** | movies_comics | 106 | 2.10 | default-fx 7 (the whole kit), no-juice 2, ult-no-impact 1 |
| Spidermonkey | tv_cartoons | 58 | 2.30 | no-juice 3, default-fx 2 |
| Atlantean Blood | base_dc | 49 | 2.35 | generic-ops 2, no-juice 2 |
| Aquaman | movies_comics | 49 | 2.24 | generic-ops 4, no-juice 3, ult-no-impact 2 |
| Cable | movies_comics | 35 | 2.38 | generic-ops 4, no-juice 2, ult-no-impact 1 |
| Hawkeye | movies_comics | 42 | 2.23 | default-fx 3, no-juice 2, ult-no-impact 1 |
| Toy Story | movies_comics | 44 | 2.15 | default-fx 3, no-juice 2 |
| Dracula | tv_cartoons | 61 | 1.97 | no-juice 3, generic-ops 2 |
| Jack Sparrow | movies_comics | 71 | 1.90 | default-fx 4, no-juice 3, ult-no-impact 2 |
| Gambit | movies_comics | 35 | 2.15 | generic-ops 6, cast-only 4, ult-no-impact 1 |
| Perry the Platypus | tv_cartoons | 31 | 2.20 | no-juice 3, default-fx 2 |
| Human Torch | movies_comics | 33 | 2.17 | generic-ops 3, ult-no-impact 1 |
| Queen Maeve | tv_cartoons | 30 | 2.20 | no-juice 3, default-fx 2 |
| **Green Lantern** | movies_comics | 50 | 1.97 | generic-ops 5, cast-only 5, no-juice 4, ult-no-impact 1 |
| **Gandalf** | movies_comics | 69 | 1.84 | generic-ops 6, cast-only 4, no-juice 4, ult-no-impact 2 |
| Dumbledore | movies_comics | 83 | 1.76 | generic-ops 3, ult-no-impact 2 |
| **Iron Man** | movies_comics | 83 | 1.76 | generic-ops 4, ult-no-impact 3, cast-only 3 |
| Indiana Jones | movies_comics | 62 | 1.85 | no-juice 3, default-fx 2, ult-no-impact 2 |
| Predator | movies_comics | 46 | 1.95 | generic-ops 4, no-juice 4, ult-no-impact 1 |
| **Thor** | movies_comics | 105 | 1.64 | generic-ops 5, cast-only 3, no-juice 3, ult-no-impact 1 |
| Raphael | tv_cartoons | 69 | 1.78 | no-juice 3, generic-ops 2, default-fx 2 |
| **Superman** | movies_comics | 109 | 1.33 | wrong bodies (gold freeze breath, heat vision from the hand), ult-no-impact |
| **Batman** | movies_comics | 115 | 1.25 | no-juice 4, ult-no-impact |
| **Hulk** | movies_comics | 95 | 1.32 | no impact frames anywhere (the SMASH kit) |
| **Spider-Man** | movies_comics | 128 | 1.21 | webs in hard light, no THWIP, ult-no-impact |
| **Nightcrawler** | movies_comics | 104 | 1.10 | ult-no-impact (otherwise the best kit in the pack) |
| **Wonder Woman** | movies_comics | 106 | 1.48 | bracers / shield default, ult-no-impact |
| **Jedi / Sith** | movies_comics | — | 1.3 / 1.1 | the Force drawn as magenta psychic, sabers as hard light |
| **Harry Potter** | movies_comics | — | 1.0 | spells as hard light (no jet of light), Patronus / Avada no frame |
| **Bending** (4 arts + kits) | tv_cartoons | 90+ | 1.2–1.9 | ultimates no frame; lightning drawn as fire |
| **Eleven / Vecna** | tv_cartoons | — | 1.4 | no frames on the scream / the curse |
| **Homelander** | tv_cartoons | — | 1.3 | laser eyes from the hand |
| **Invincible / Omni-Man** | tv_cartoons | — | 2.0 | no style at all, piledriver no title / frame |
| **Ben 10 / TMNT / Witcher** | tv_cartoons | — | 1.8–2.6 | default energy katanas, grey Signs, ult-no-impact |

(Bold = the brief's named headliners, which were fixed first regardless of rank.)

## After (POLISH: WESTERN pass)

| | before | after |
|---|---|---|
| moves (Transformers excluded) | 1,102 | 1,112 (+3 kits, +2 moves) |
| Ultimates with an impact frame | 4 / 194 | **195 / 195** |
| Ultimates with a boom | 98 / 194 | 195 / 195 |
| heavy non-ult hits with an impact frame | ~0 | 27 |
| moves in the default style | 234 | 106 (mostly plain weapons / gadgets where hard light is fine) |
| moves with no juice | 534 | 470 |
| new styles (`move_fx.ron`, existing bodies) | — | web, mystic, chaos, force, spell, magnetism, saber, claws, blade, shield, repulsor, hellfire, cable |
| new comic words | — | THWIP, SNIKT |

Headliners (before → after):
- **Spider-Man**: hard-light blobs → a thin white web STRAND (Rubber body at 1/3 thickness, THWIP) for zip / swing /
  shooters / net; a red POW impact frame on the spider kick and Maximum Spider.
- **Iron Man**: repulsor / unibeam in their own cyan `repulsor` look (THOOM); boot jets and micro-missiles burn
  fire; "I Am Iron Man" is a holy glory frame.
- **Thor**: hammer flight was `Impulse + Float` (the same as Superman's) → a towed, swervy `Ram` flight line that
  bats anyone on it (steer 150); Bifrost in holy light; thunder strike / slam get titles + comic frames.
- **Superman**: heat vision now fires from the EYES (`gaze`); freeze breath is ice (was painted gold by the kit's
  holy); flight is a straight, fast sonic-boom `Ram` (steer 50, keeps its momentum) — not Thor's swerve.
- **Hulk / Cap / Wolverine / Magneto / Nightcrawler / Flash / Batman / WW / GL**: impact frames on every big hit;
  Cap's kit clangs (`shield`), Wolverine's is steel streaks + SNIKT (`claws`), Magneto stops drawing the Ruyi
  pole (`magnetism`); Batman's grapnel / batclaw are steel `cable`s; Cap gains **On Your Left** (step round to their
  flank and pounce).
- **Strange / Wanda**: orange mandala `mystic` vs scarlet-wisp `chaos` (they were gold holy and Jean Grey's magenta).
- **Star Wars**: the Force is a dim pressure `force` (push / pull / lift / choke / crush), sabers hum (`saber`),
  **Force jump lands exactly where you look** (`Leap`), Unlimited Power a glitch frame; new **Clone Troopers** kit
  (Execute Order 66 = a seeking blaster salvo on the marked target, blaster, thermal detonator).
- **Harry Potter**: spells are straight coloured `spell` jets with a whip-crack; Patronus holy, Avada void; new
  **Dark Arts** kit (Crucio: short writhing lift-stun; Imperio: no powers for 2 s + dazed) and **Professor Lupin**
  kit (Riddikulus swallows an enemy construct, Alohomora steps you through the wall, Episkey mends).
- **Gandalf**: You Shall Not Pass = holy frame; Fly, You Fools / Fireworks titles.
- **Avatar**: bending ultimates in sumi ink frames; every lightning move (Azula, Zuko, Iroh, firebending) draws as
  lightning, not fire; "Lightning Bending" is now a real shout.
- **Stranger Things / The Boys / Invincible**: Eleven / Vecna void frames; Homelander's lasers from the eyes;
  Invincible and Omni-Man are `blood` with comic frames (INVINCIBLE / PILEDRIVER).
- **Witcher**: each Sign its own body and title — Igni fire, Aard force, Quen holy, Yrden / Axii psychic; + a silver sword.
- **TMNT / Ben 10 / Samurai Jack**: steel `blade` katanas, Shredder `claws`, Splinter smoke; Omnitrix atomic green.
