# POLISH: MYTH & DERIVED — audit and fixes (2026-09-27, overnight polish wave)

Scope: `assets/powers/myth_folklore.ron`, `base_folklore.ron`, and the derived-kit system
(`tools/derive_kits/*.py` → `assets/powers/zz_derived_kits.ron`, expanded by `crates/bd_sim/src/powers/derive.rs`).
Bar: Hollow Purple / Ruyi staff — "unique flavor/control/playstyle + dramatic/cool presentation = fun".

## 1. Audit

### Derived kits (2,500 characters) — where "as many as possible" is won or lost

Measured with a scratch script over `derive_kits.generate()` (before = the checked-in file at 06:45).

| | before | after |
|---|---|---|
| kits with NO presentation style (every move drawn as plain default hard light) | **1,285 (51 %)** | **31** (gunslingers / archers keep the clean energy default on purpose) |
| kits sharing all 5 slot NAMES with another kit | 391 | 40 |
| distinct 5-name signatures | 2,281 | 2,480 |
| most repeated move names | en garde ×535, crater drop ×317, unmovable ×234, overdrive ×224, blur ×209, sky surge ×202, thick hide ×198, force bolt ×185 | maul ×133, slide ×131, springback ×116 (the most repeated name now appears in 5 % of kits) |
| ultimate templates in use | 26 | 32 |
| ultimates with an impact frame | ~1,100 (only packs dc / marvel / anime / tmnt / shows / internet / folklore_cn, holy / shadow / spirit / glitch) | **all 2,500** |
| ultimates with a gather / windup | 0 signature casts; the 338 "knockouts" were a plain lunge | 165 signature palm beams (gathered at the hands, hold = telegraph; SolarBeam for plants), 236 windup knockouts (armoured, marked, then it lands), gather-before-strike storms (the eye drags them in first) |
| grass-type Pokémon / plant characters | on **Earth**: Chikorita "stone slash / quake onslaught / earthquake", Leafeon "quake slash", Treecko "stone bolt / boulder overdrive" | **nature** flavour: bullet seed / vine whip / petal blizzard / bramble ward / synthesis / solar beam / frenzy plant, in a leaf style |

Template monotony (same 5 templates, different names/style/numbers): 1,282 → 1,298 kits share a template
signature with at least one other — structurally unchanged in count, but the largest clusters now split by
temperament (a calm swordsman's ultimate is the iaido "sheathe", an angry one's is the blade tempest) and the
shared kits now differ in body, names and finisher. The biggest cluster is 37 (fast blade users: slash / blur /
parry / flurry / overdrive), was 38.

Why it looked generic in play: half the kits drew every move in the default energy body (no fx), and the
most common ultimate (Knockout, 338 kits) was a 0.5 s lunge with a title — no windup, no frame.

### Myth & folklore (181 entries, 741 moves)

The myth file was already mechanically rich (Seeker Gungnir, Haul Māui, Surge Poseidon, Leap Hanuman, GazeStun
Medusa, Ding, Ruyi extend). Popularity × blandness found presentation gaps, not recipe gaps:

* 10 impact frames in 741 moves; Zeus / Poseidon / Ra / Shiva / Hanuman / Wukong / Māui / Anubis ultimates had none.
* 9 kits used the `rigid` body — the Ruyi staff's telescoping golden pole — for spears, swords and a
  smith's hammer (Achilles, Tyr, Ogun, Lancelot, Ilmarinen, Beowulf, William Tell). The owner's favourite
  weapon look was being diluted.
* King Arthur had no Excalibur move (the sword is an icon summon; his ultimate was "sword in the stone").
* Bigfoot's ultimate was an untitled Explode + Zone.
* Zeus's wrath is the headline storm but played like any Salvo.

## 2. Fixes

### Derived templates (derive.rs + derive_kits.py + vocab.py)

New templates (all generic, all data-driven from the character profile):

| Tpl | who gets it | what it does |
|---|---|---|
| `SigBeam` (ult) | elemental / holy / tech / mystic / chi casters and blasters (155) | a SIGNATURE palm beam in the character's element: gathered at the hands (hold = telegraph), bores terrain, can clash; a second element (`e2`) fuses two orbs in the windup (the Hollow Purple merge, 6 kits). Names per element (inferno beam, railgun, absolute zero, psycho beam, umbral beam, judgement ray…) |
| `Pressure` (ult) | calm fighters (148); calm / sneaky blade users get it as the IAIDO ("sheathe", "last draw") | one quiet touch — then 1.3 s later it lands (DelayedHit): you walk away while they burst |
| `Apex` (ult) | beasts (203) | a roar that locks up whoever is watching, the pounce, the mauling |
| `Titanfall` (ult) | brutes / kaiju (100) | armoured, a huge leap onto the crosshair, a crater (world breaker, earth shatter, meteor fall) |
| `SolarBeam` / `Overgrowth` (ult) | plants | the gathered sunlight beam / a ring of thorn walls + spores + roots bursting under them |
| `Vine` (primary) | plants | a skillshot vine that stings and tows them to arm's length (-> the combo's range) |
| `Thorns` (defense) | plants | bramble hedge: repel + a thorny slowing aura + counter |
| `Roots` (combo) | plants | roots erupt along the aim (the ground's own material) + a grasping patch at the crosshair |
| `Bolt(Leaf)` | plants | a fan of razor petals / bullet seed |

Reworked:
* **Knockout** (236 kits): Drill lunge → the WINDUP haymaker — armoured while the fist cocks back, a step in,
  a marked spot in front of you, then it lands (26 dmg class, 36 kb). -> ends planted, fist out.
* **Storm** (503): the storm GATHERS first — a pull at the crosshair drags them into the eye while the marks
  bloom; a thunderstorm also soaks them (Wet: bolts arc on).
* **Beatdown**: armoured through the whole string.
* **Streak** for blade wielders: the swordsman's pass (through them, cut lands as you pass), steel look.
* Every ultimate: an impact frame (pack / flavour, else by element: dark → void, lightning → glitch,
  fire → holy, the rest ink; playful / chaotic / fearful / lazy → comic), plus per-template juice
  (debris pulled IN for the beam gather, a crater ripple for Titanfall…).

Presentation bodies for the 1,285 unstyled kits (new `move_fx.ron` styles, data only): `brawl` (dusty
shockwave off the knuckles, WHAM — 800 kits), `feral` (amber claw-streaks, THWACK — 232 beasts), `toon`
(confetti paper, POW — 144 playful / panicky / chaotic), blade users → the existing `blade` style (77), plants
→ `nature` (whirling leaves, FWSSH — 20). Mystic flavour now uses the `mystic` spell-ring style.

Names: every fixed name list rotates per character (`Ctx.rot`) and grew; every candidate was checked against
the costume grammar (many old names were silently rejected: anything with "the", "cannon", "leaf", "blast",
"power", "big", "ground" — which is why "crater drop" and "en garde" swamped everything).

Budget: 808 KiB (≤ 1 MB), 2,500 kits, table 3,377 powers < MAX_POWERS 4,000 (−300 headroom kept).

### Myth (myth_folklore.ron)

* Impact frames on 21 headline moves: wrath of zeus / descent from olympus (holy), tidal wave, open the
  underworld (void), ragnarok, eye of ra, weighing of the heart (void + boom), Shiva's third eye + tandava,
  leap to lanka, lift the mountain, fury of kali, raijin's storm (glitch), storm of the sea, seventy two
  transformations + havoc in heaven (Wukong: ink, debris), fish up the islands (debris pulled in), avalanche,
  red eyes, sudarshan chakra, Erlang's third eye.
* **Wrath of Zeus**: the bolts TRACK the live crosshair — sweep your aim and you conduct the storm across them.
* **Excalibur** (King Arthur's new ultimate): the blade flares (armoured, a flash that throws back anyone
  close), then a golden 9 m crescent of light rolls 46 m across the map carving a trench. "king arthur
  excalibur", "light of excalibur", "swing excalibur", "excalibur strike"; bare "excalibur" still summons
  the sword. "sword in the stone" stays callable by name.
* **Sasquatch roar**: whoever is LOOKING at Bigfoot freezes 0.7 s (they saw him), the shock throws the close
  ones, and he crashes out of the treeline through them. Titled, comic frame.
* Bodies: the golden telescoping pole is Wukong's (and Pinocchio's nose's) alone again — Achilles, Tyr, Ogun,
  Lancelot → `blade`; Ilmarinen → `fire` (the forge); Beowulf → `brawl` (Naegling keeps `blade`); William Tell →
  element default.
* Sun Wukong: not regressed (ruyi extend / ding / somersault untouched); the ultimate and Havoc in Heaven
  gained frames and debris.

## 3. Open

* Kit / borrowed `impact_frame` (POLISH_REQUESTS.md ANIME #1): the 08:5x build already fires it for kit
  moves (the comic frame on Sasquatch roar in the sheet) — confirm with the presentation worker it's final.
* Frame sheet: `myth_derived_after.png` (observer view, 3 beats each). Signature palm beams (SolarBeam,
  SigBeam, Eye of Ra) show the DANGER cut-in + the gather; the beam itself fires on the holder's release,
  which the dev scene's actor never presses — not visible in the sheet.
* Poseidon's tidal wave wall draws as a red hard-light construct (corps colour) — the Surge wall should take
  the move's water look (presentation; noted, not in my files).
* The phrase scene refuses Hanuman's leap / Māui's haul (no landing / terrain under the scene's aim) — the
  moves are fine in play (they refuse with the cost refunded when there's nothing to land on / hook).
* Template-signature clusters (same 5 templates) remain for ~1,300 kits; the next lever is more Primary /
  Movement variants per weapon (e.g. a whip family, a gun family beyond pellets).
* Wukong elevation idea not done (risk of regressing the favourite): the giant form should bring a giant
  Ruyi with it (needs a check of what a second held-weapon Cast does while one is held).
