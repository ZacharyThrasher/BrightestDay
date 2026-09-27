# POLISH: ANIME — audit (Naruto · Dragon Ball · One Piece · Bleach · JoJo · JJK · MHA · Demon Slayer · HxH · OPM · CSM · AoT · FMA · Fairy Tail · …)

Files: `assets/powers/anime_classics.ron`, `anime_modern.ron`, `base_techniques.ron`, `base_one_piece.ron`,
`base_jojo.ron` (282 powers / kits, 1,317 moves before). Bar: Hollow Purple + Ruyi staff — "unique flavour /
control / playstyle + dramatic / cool presentation = fun".

## Method

A scratch parser (`<scratchpad>/polish_anime/{ron,audit,stats}.py`) read every entry and flagged each move:

| flag | meaning |
|---|---|
| generic-ops | only Cast / Shove / Impulse / Explode / Blink / Barrage, no signature cast |
| same-recipe | the identical op shape (numbers rounded, element ignored) as another move in the file |
| no-style | no move `fx`, no power `fx`, no element anywhere: draws as the default hard light |
| element-style | no `fx` at all, the body comes only from an element (sometimes the wrong one) |
| ult-no-impact-frame | Ultimate (or ≥ 20 willpower) that hits, not a signature cast, no crit, no `impact_frame` |
| ult-no-title / no-color | presentation gaps |

**Popularity** = a hand fame score for what players actually yell (Kamehameha, Rasengan, Hollow Purple, Za
Warudo, Gum-Gum Pistol… = 10; Chidori, Getsuga, Detroit Smash, Black Flash… = 9–10; deep cuts 7) + the
character's Wikidata sitelinks. Rank = popularity × (1 + blandness), then read by hand — the flags find
candidates, the ranking below is judgement.

## Before — the numbers

| | before |
|---|---|
| moves / powers | 1,317 / 282 |
| Ultimates | 241 |
| … hitting, non-signature, with no impact frame | **178** |
| moves with any impact frame | 7 |
| moves drawn in the default style (no fx, no element) | **103** |
| powers / kits with no style of their own | 50 |
| borrowable shouts | 2,534 |
| fan-yell corpus (374 phrases, `yells.txt`) | 224 borrow · 74 fall through · 68 grant |

## Systemic findings

1. **Impact frames only existed on 7 moves.** Every hitting Ultimate had none; neither did Rasengan, Chidori,
   Raikiri, Getsuga Tensho, Detroit Smash… **And the ones that existed on kit moves never fired**:
   `present/impact_frames` arms a move's frame only on `PowerEvent::Move` (held moves). Kit / borrowed moves
   send `PowerEvent::Fired`, so World Cutting Slash, United States of Smash, Serious Punch, Resonance, Death
   Note… never froze. Filed as request 1 in `docs/toolbox/POLISH_REQUESTS.md` (every pack is affected).
2. **Wrong bodies on headliners.** Hollow Purple / Blue / Red drew in the kit's `cursed` body (blue-black
   flame, red core) — Purple is a violet void, Blue implodes, Red is a clean repulse. Iron / sky / lightning
   dragon slayers wore the FIRE body of the technique. Saitama — a man with no ki — drew as cyan chi.
   Bleach's flash step, Aizen's flash step, Byakuya's senka, Yoruichi's utsusemi and Goku's afterimage all
   drew as the Flash's yellow speed-force lightning (the owner's own example: a Flash dash must not look like
   any other blink).
3. **Same verb, one body.** Every Luffy gear stretched the same rubber arm: Jet Pistol, Elephant Gun, Kong
   Gun and Gear 5 looked identical apart from size numbers.
4. **103 moves / 50 powers with no body at all**: One For All's smashes, Kaio-ken, Kurama Mode's Tailed Beast
   Bomb, the Rinnegan, all eight Titan shifts, Total Concentration Breathing's forms without an element,
   Arise, Spirit Gun, Moon Prism, the AT Field, chainsaws, life fibers, seven JoJo stands, twelve devil
   fruits.
5. **The famous line didn't do the famous thing.**
   - "ZA WARUDO" **granted the stand** (it was one of The World's names) instead of stopping time.
   - "Mugetsu" **granted Kamui Senketsu** — the lexicon compound-repaired it into "mug + senketsu".
   - "Thunderclap and flash godspeed" **split into two steps**: the Hulk's thunderclap, then a Nen grant
     (the word "godspeed" was a Nen tier name).
   - "Godspeed" granted the Transmuter tier instead of Killua's Godspeed.
   - "ka me ha me ha" (spoken syllables), "toki wo tomare", "wryyyyy", "za hando", "izanagi", "edo tensei",
     "plus ultra", "zero gravity", "set your heart ablaze", "constant flux", "dragon force", "ice make",
     "devil union", "hell flame", "straw doll technique", "summoning jutsu", "beast gohan", "room",
     "nah i'd win", "half cold half hot" fell through to generic constructs.
6. **Bland recipes on famous moves**: Road Roller Da was a plain drop with no title (The World's version had no
   juice at all); Kaio-ken Rush a generic lunge-shove; the Black Flash technique's Divergent Fist one plain
   shove (the whole point is the second, delayed impact); Itadori's Black Flash had no crit and no frame
   (the technique's version had both); Nen's Jajanken Rock a lunge-shove with no blast; Emerald Splash,
   Crossfire Hurricane, Arrivederci, Za Hando, 3 Freeze had no juice at all.

## Ranked headliners (popularity × blandness) and the verdict

| # | move (file) | pop | what was bland | verdict |
|---|---|---|---|---|
| 1 | Za Warudo (base_jojo, kit_dio) | 10 | the phrase granted the stand; no colour | FIX: fires the time stop, gold, 18 m ripple |
| 2 | Hollow Purple / Blue / Red (limitless, gojo_kit) | 10 | cursed-flame body, Purple uncoloured on the technique | FIX: void (Purple, Blue — implodes) / hard light (Red), purple, shake |
| 3 | Black Flash (itadori_kit) | 10 | no crit, no frame | FIX: guaranteed ×2.5 BlackFlash crit + inverted frame (numbers rescaled) |
| 4 | Gum-Gum moves across gears (base_one_piece, kit_luffy) | 10 | one rubber body for four gears | FIX: 4 new bodies (see after) |
| 5 | Rasengan ×6 variants, Chidori ×3, Raikiri | 10 | no impact frame | FIX: ink frame on the latch / pierce |
| 6 | Kamehameha | 10 | fine (signature beam); spoken syllables fell through | ALIAS |
| 7 | Road Roller Da (the_world, kit_dio) | 9 | plain drop, no title/colour on the stand | FIX: LEAP to the crosshair, the roller drops where you land; title, comic frame |
| 8 | Mugetsu | 9 | granted Kamui Senketsu | FIX (bare "senketsu" is no longer a grant name) |
| 9 | Thunderclap and Flash (Godspeed) | 9 | godspeed phrase split into two wrong steps | FIX |
| 10 | Detroit Smash (one_for_all) | 10 | default body | FIX: green lightning (Full Cowl) + comic frame |
| 11 | Kaio-ken Rush | 8 | generic lunge-shove, no body | FIX: a red 4-hit surge + launcher, the body pays (Strain), "KAIO-KEN!" |
| 12 | Getsuga Tensho, Wind Scar, Oni Giri, Diable Jambe, Red Hawk, Fire Fist, Galaxy Breaker, Star Finger | 9 | no frame | FIX: frames |
| 13 | Dragon slayers (iron / sky / lightning) | 7 | fire body | FIX: rigid / wind / lightning |
| 14 | Breathing forms (sound, mist, moon, stone, love, serpent, insect, beast, sun) + nichirin base | 9 | default energy / element only | FIX: per-form body; base cuts in steel `blade` |
| 15 | Flash step / senka / utsusemi / afterimage / UI silver dash | 8 | Flash's lightning body | FIX: new `shunpo` body (dark blur, one ghost, quiet) |
| 16 | Tailed Beast Bomb, Chibaku Tensei, Almighty Push | 9 | no body | FIX: void / void / psychic rings |
| 17 | Titan shifts (8) | 8 | no body | FIX: the golden shift bolt (lightning) |
| 18 | Arise, Spirit Gun, Moon Prism, AT Field, Chainsaw, Life Fibers, Spiral | 7–9 | no body | FIX: shadow / spirit / holy / hardlight / blood / blood / energy |
| 19 | Saitama's punches | 10 | cyan chi on a man with no ki | FIX: `brawl` (dusty knuckle shockwave, WHAM) |
| 20 | Zoro's sword moves | 9 | wind body on steel | FIX: `blade` (the flying slashes stay wind) |
| 21 | Divergent Fist (black_flash technique) | 8 | one plain shove | FIX: two impacts (Shove + DelayedHit) |
| 22 | Jajanken Rock (nen) | 8 | lunge-shove | FIX: + a blast, ink frame |
| 23 | Emerald Splash, Crossfire Hurricane, Arrivederci, Za Hando, 3 Freeze, SP: The World | 8 | no juice / colour | FIX: titles + colours |
| 24 | Izanagi, Edo Tensei | 7 | fell through | NEW MOVES: Obito's Izanagi = Rewind; Orochimaru's Edo Tensei = RaiseFallen |
| 25 | ~150 hitting Ultimates | — | no impact frame | FIX: frame in the property's look (void / holy / comic / ink) |
| — | Spirit Bomb, Amaterasu, Kamui, Susanoo, Instant Transmission, Domains, Serious Punch, ORA/MUDA, Bites the Dust, Killer Queen, Getsuga (op) | 10 | — | KEEP: already the famous mechanic (hold-to-grow, contagion, phase+banish, WarpTo, Domain, Rush, Rewind, Tag/TagStrike, Wave) |

## Not changed (design calls / other owners) — see the report

- "black flash" and "jajanken" still GRANT their techniques (bd_lexicon `techniques.rs` pins it: saying a
  technique's name makes it yours; saying it again fires). A first-call "say it, see it" would move those
  words to shouts — owner's call.
- "star platinum the world" still grants The World (the lexicon reads the stand name inside the phrase).
- "reaper death seal" reads as Overwatch Reaper's kit (a loose kit reading beats an exact borrowed shout — core).
- "flash step" reads as the Flash's kit move (western pack).
