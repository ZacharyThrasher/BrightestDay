# Third-party notices

Brightest Day is a private research project (not for sale or distribution). It bundles or derives data
from the following sources; names of characters, franchises and products belong to their owners and are
used only as words a player may type.

| Component | Use | License |
|---|---|---|
| ConceptNet 5.7 (commonsense/conceptnet5) | offline vocabulary relations → `assets/lexicon.bin` | CC BY-SA 4.0 (attribution: ConceptNet, conceptnet.io) |
| WordNet 3.0 (Princeton) / Open Multilingual Wordnet | offline synonyms, hypernyms | WordNet License (BSD-style); OMW data per-wordnet licenses |
| Wikidata | offline named-entity classes → `assets/lexicon.bin` | CC0 |
| dariusk/corpora | offline word lists | CC0 |
| PokeAPI data (PokeAPI/pokeapi CSVs) | offline creature attributes | BSD-3-Clause (data), Pokémon names © Nintendo/Game Freak |
| wordfreq | offline word frequencies | Apache-2.0 (data CC BY-SA 4.0) |
| Bevy and the Rust crate ecosystem | engine and libraries | MIT / Apache-2.0 (see each crate) |
| whisper.cpp / ggml-base.en model (optional voice) | local speech recognition | MIT |
| cloudflared | quick-tunnel hosting (bundled exe) | Apache-2.0 |
| System Reference Document 5.1 (Wizards of the Coast) | spell / monster / magic-item NAMES as trigger words in `assets/icons/tabletop.ron` (no rules text) | CC BY 4.0 (attribution below) |

Because `assets/lexicon.bin` includes material derived from ConceptNet (CC BY-SA 4.0), that file is
shared under CC BY-SA 4.0. All sound effects are procedurally synthesised (`tools/sfxgen`) and all
models are procedural or hand-authored voxel primitives in this repository.

## Soundtrack (`assets/music`, `crates/bd_music`)

All music is synthesised live by `bd_music`. There are no recordings, samples or audio files. Every
arrangement, counter-melody, harmonisation, episode, stinger fanfare and instrument patch is original
to Brightest Day. The underlying compositions are in the public domain worldwide: every composer died
in or before 1925, and every work was published before 1929. The melodies were written from memory of
the original scores. No modern arrangement, recording, MIDI file or third-party score file (Mutopia,
IMSLP or other) was used or consulted. Each track file's `source` field lists which parts follow
the score.

| Track | Composition | Composer (died) |
|---|---|---|
| menu | Serenade No. 13 "Eine kleine Nachtmusik" K. 525, I (1787) | W. A. Mozart (1791) |
| lobby | Gymnopédie No. 1 (1888) | Erik Satie (1925) |
| tutorial | Minuet in G, BWV Anh. 114 (1725) | Christian Petzold (1733), attr. J. S. Bach |
| archipelago_explore | The Four Seasons, "Spring" RV 269, I (1725) | Antonio Vivaldi (1741) |
| archipelago_combat | The Four Seasons, "Summer" RV 315, III (1725) | Antonio Vivaldi (1741) |
| miami | Piano Sonata K. 331, III "Rondo alla Turca" (1783) | W. A. Mozart (1791) |
| orbit_float | "An der schönen blauen Donau" Op. 314 (1867) | Johann Strauss II (1899) |
| orbit_combat | Die Walküre, "Ride of the Valkyries" (1856/1870) | Richard Wagner (1883) |
| sudden_death | Peer Gynt, "In the Hall of the Mountain King" (1875) | Edvard Grieg (1907) |
| final_stock | The Tale of Tsar Saltan, "Flight of the Bumblebee" (1899–1900) | Nikolai Rimsky-Korsakov (1908) |
| party | Orphée aux enfers, "Galop infernal" (1858) | Jacques Offenbach (1880) |
| party_results | Guillaume Tell, Overture finale (1829) | Gioachino Rossini (1868) |
| results | Symphony No. 9, IV, "Ode to Joy" theme (1824) | Ludwig van Beethoven (1827) |
| stinger: ko | Symphony No. 5, I, opening motif (1808) | Ludwig van Beethoven (1827) |
| stinger: match_point | "Dies irae" sequence (Gregorian chant, 13th c.) | anonymous |
| stinger: last_standing | Toccata and Fugue in D minor BWV 565, opening (c. 1704) | attr. J. S. Bach (1750) |
| stingers: streak, victory, defeat | original | Brightest Day |

Remix variants (`assets/music/remix/*.ron`): our own arrangements of the same public-domain themes in
genre styles (trap, synthwave, house, drum & bass, lo-fi, phonk, chiptune EDM). The drum patterns,
bass lines, chord voicings, builds and drops are original, and they follow generic genre conventions
only. No existing recording, remix, producer's track or sample was used, imitated or referenced. The
808s, supersaws, drum kit, risers and vinyl crackle are all synthesised by `bd_music`.

## Maps

Sakura Keep's layout is modelled on Blizzard's Overwatch map Hanamura for private use; replace the layout before any public release.
(Only the floor plan is followed — `crates/bd_voxel/src/maps/sakura_layout.rs`, traced by `tools/maps/sakura_trace.py`.
The look, palette, props and name are Brightest Day's own; no Blizzard assets, logos, signage or text are used.)

Ember map is inspired by Star Wars' Mustafar for private use; replace before any public release.
(Only the mood is referenced — a lava world at dusk with a dark twin-spired tower and a mining
facility over a lava river. Layout, geometry, props and the name are Brightest Day's own; no
Lucasfilm/Disney assets, names, logos or text are used — `crates/bd_voxel/src/maps/ember.rs`.)

TMNT Lair map uses Teenage Mutant Ninja Turtles places/props (Paramount/Nickelodeon) for private use; replace before any public release.
(The Turtle Lair — `crates/bd_voxel/src/maps/lair.rs` — is our own layout, geometry and voxel art evoking the turtles'
sewer lair: the bean-bag pit, dojo, weapon rack, pizza sign, party wagon, Shellraiser, Krang's body and other props are
baked from the private-use TMNT icon pack `assets/icons/tmnt.ron`. No Paramount/Nickelodeon assets, logos or text are used.)

Valley of the End map is modelled on the Naruto location for private use; replace before any public release.
(Naruto is Shueisha/Pierrot IP. The gorge, falls and the two facing statues — `crates/bd_voxel/src/maps/valley.rs`,
`valley_statues.rs` — are Brightest Day's own voxel geometry evoking the location; no Shueisha/Pierrot assets, logos or text are used.)

Mirrorfold map is inspired by Inception and Doctor Strange's Mirror Dimension for private use; replace before any public release.
(Only the idea is referenced — a city folding up over itself, kaleidoscope-cut buildings, a bright rift where the fold
meets. Layout, geometry, props and the name are Brightest Day's own; no Warner Bros./Legendary or Marvel/Disney assets,
logos or text are used — `crates/bd_voxel/src/maps/mirror.rs`, `crates/bd_game/src/present/mirror.rs`.)

Cybertron map uses Transformers places/insignia (Hasbro) for private use; replace before any public release.
(Iacon, the Hall of Records, Kaon's gladiator pit, the Well of AllSparks, the space bridge, the Sea of Rust and the Autobot /
Decepticon insignia mosaics — `crates/bd_voxel/src/maps/cybertron.rs` — are our own layout, geometry and voxel art; the Prime
and Megatron statues, the Matrix and the AllSpark are baked from the private-use Transformers icon pack
`assets/icons/transformers.ron`. The transforming-terrain sounds are our own panel clicks, not the trademarked transformation
sound. No Hasbro assets, logos or text are used.)

Hyperline map is inspired by Spider-Man: Across the Spider-Verse for private use; replace before any public release.
(Only the mood of the train chase is referenced — a sleek white train racing through a vertical city shaft past giant ring
gates. The name, layout, geometry, backdrop and sounds are Brightest Day's own; no Sony/Marvel assets, logos, names or text are
used — `crates/bd_voxel/src/maps/hyperline.rs`, `crates/bd_game/src/present/hyperline.rs`.)

The Doorman's door (vocabulary entry "doorman door" / "doorman's door" / "doorway" / "a door to …" in `assets/icons.ron`)
names Valve's Deadlock character for private use; replace the name before any public release.
(Name only, as words a player may type — the doorway portal, its wooden frame, warm light and threshold are Brightest Day's
own voxel shapes and mechanics (`crates/bd_fx/src/extras.rs` `door_frame_spec`, `crates/bd_sim/src/world/actions.rs`); no Valve
assets, logos, text or sounds are used.)

## Tabletop vocabulary (`assets/icons/tabletop.ron`, `tools/icons/tabletop/gen.py`)

This work includes material taken from the System Reference Document 5.1 ("SRD 5.1") by Wizards of
the Coast LLC and available at https://dnd.wizards.com/resources/systems-reference-document. The SRD 5.1
is licensed under the Creative Commons Attribution 4.0 International License available at
https://creativecommons.org/licenses/by/4.0/legalcode.
(Only spell, monster and magic-item names are used, as words a player may type; every in-game effect is
Brightest Day's own mapping onto its existing mechanics. No SRD rules text is reproduced.)

Non-SRD Dungeons & Dragons names (beholder, mind flayer / illithid, displacer beast, umber hulk, githyanki,
hook horror, and the Bigby's / Tasha's / Otto's / Otiluke's / Evard's / Melf's / Mordenkainen's / Leomund's /
Tenser's spell prefixes) are Wizards of the Coast product identity, used only as trigger words for private
use; replace them before any public release. (Entries tagged "Dungeons & Dragons (name only; private use)"
in `assets/icons/tabletop.ron`; the older D&D entries in `assets/icons/internet.ron` fall under the same note.)

## Video-game power vocabulary (`assets/powers/games_adventure.ron`)

Character, move, spell and item NAMES from these single-player game franchises are used only as trigger
words a player may type or say, for private use; replace them before any public release: The Legend of
Zelda, Super Mario, Donkey Kong, Kirby, Metroid, Star Fox, EarthBound / Mother, Xenoblade Chronicles
(Nintendo); Sonic the Hedgehog (SEGA); Mega Man, Devil May Cry, Resident Evil, Ōkami (Capcom); Final
Fantasy, Kingdom Hearts (Square Enix); Halo, Gears of War (Microsoft / Xbox Game Studios); DOOM, The Elder
Scrolls V: Skyrim, Dishonored (Bethesda / id Software / Arkane / ZeniMax); God of War, Ratchet & Clank,
inFAMOUS, Ghost of Tsushima, Katamari (Sony Interactive / Bandai Namco); Dark Souls, Elden Ring, Sekiro
(FromSoftware / Bandai Namco); Bloodborne (FromSoftware / Sony); Bayonetta (PlatinumGames / SEGA / Nintendo);
Genshin Impact (HoYoverse); Destiny (Bungie); Mass Effect (BioWare / EA); Half-Life, Portal (Valve);
BioShock (2K); Prototype (Radical / Activision); Minecraft (Mojang / Microsoft); Hollow Knight (Team Cherry);
Hades (Supergiant Games); Celeste (Maddy Makes Games / Extremely OK Games); Cuphead (Studio MDHR); Undertale
(Toby Fox); Assassin's Creed, Prince of Persia (Ubisoft); Metal Gear (Konami); Castlevania (Konami); Pac-Man
(Bandai Namco); Crash Bandicoot, Spyro (Activision); Max Payne (Remedy / Rockstar); Titanfall (Respawn / EA);
Control (Remedy). Every in-game effect is Brightest Day's own mapping onto its generic move operations; no
game assets, text, dialogue, lyrics or sounds are used.

## Arena / competitive game power vocabulary (`assets/powers/games_arena.ron`)

Hero, champion, agent, legend and fighter NAMES and their ability / move NAMES from these multiplayer and
fighting-game franchises are used only as trigger words a player may type or say, for private use; replace
them before any public release: Overwatch (Blizzard Entertainment); Valorant, League of Legends (Riot
Games); Apex Legends (Respawn / EA); Deadlock, Dota 2, Team Fortress 2 (Valve); Marvel Rivals (NetEase /
Marvel); Street Fighter (Capcom); Mortal Kombat (NetherRealm / Warner Bros. Games); Tekken (Bandai Namco);
Super Smash Bros., Fire Emblem (Nintendo / HAL Laboratory / Intelligent Systems); Pokémon (Nintendo /
Game Freak / Creatures / The Pokémon Company); The Legend of Zelda's Ganondorf as fought in Smash (Nintendo);
Guilty Gear, BlazBlue (Arc System Works); Fortnite (Epic Games); Brawl Stars (Supercell). Every in-game
effect is Brightest Day's own mapping onto its generic move operations; no game assets, text, voice lines,
dialogue or sounds are used.

## Film & comics power vocabulary (`assets/powers/movies_comics.ron`, `base_marvel.ron`, `base_dc.ron`)

Character, move, spell, gadget and catchphrase NAMES from these film and comics franchises are used only
as trigger words a player may type or say, for private use; replace them before any public release:
Marvel Comics / Marvel Studios / 20th Century Studios' X-Men films and Sony's Spider-Man and Spider-Verse
films (Marvel / Disney / Sony); DC Comics and DC films (DC / Warner Bros.); Star Wars (Lucasfilm / Disney);
Harry Potter / Wizarding World (J.K. Rowling / Warner Bros.); The Lord of the Rings and The Hobbit
(Tolkien Estate / Middle-earth Enterprises / New Line / Warner Bros.); The Matrix (Warner Bros.);
Ghostbusters (Columbia / Sony); Men in Black (Columbia / Sony / Amblin); Pirates of the Caribbean, Frozen,
Moana, Encanto, Tangled (Disney); The Incredibles, Toy Story (Disney / Pixar); Kung Fu Panda, Shrek, Puss in
Boots (DreamWorks / Universal); The Karate Kid and Cobra Kai (Columbia / Sony); Kill Bill (Miramax / A Band
Apart); John Wick (Lionsgate); The Terminator (StudioCanal / Skydance); Predator and Alien (20th Century
Studios / Disney); Jurassic Park / Jurassic World (Universal / Amblin); Godzilla and Kong (Toho / Legendary
/ Warner Bros.); Back to the Future (Universal / Amblin); Indiana Jones (Lucasfilm / Disney). Every in-game
effect is Brightest Day's own mapping onto its generic move operations; no film or comics assets, logos,
dialogue beyond short move names, lyrics or sounds are used.

## Modern anime power vocabulary (`assets/powers/anime_modern.ron`; JJK / Hunter x Hunter entries in `base_techniques.ron`)

Character, technique, move and catchphrase NAMES from these anime / manga franchises are used only as
trigger words a player may type or say, for private use; replace them before any public release:
Jujutsu Kaisen (Gege Akutami / Shueisha / MAPPA); My Hero Academia (Kohei Horikoshi / Shueisha / Bones);
Hunter x Hunter (Yoshihiro Togashi / Shueisha / Madhouse); Demon Slayer: Kimetsu no Yaiba (Koyoharu Gotouge /
Shueisha / ufotable); Chainsaw Man (Tatsuki Fujimoto / Shueisha / MAPPA); Mob Psycho 100 and One-Punch Man
(ONE / Yusuke Murata / Shogakukan / Shueisha / Bones / Madhouse / J.C.Staff); Fullmetal Alchemist (Hiromu
Arakawa / Square Enix / Bones); Attack on Titan (Hajime Isayama / Kodansha / Wit Studio / MAPPA); Solo
Leveling (Chugong / D&C Media / A-1 Pictures); Yu Yu Hakusho (Yoshihiro Togashi / Shueisha / Studio
Pierrot); Inuyasha (Rumiko Takahashi / Shogakukan / Sunrise); Fairy Tail (Hiro Mashima / Kodansha / A-1
Pictures); Black Clover (Yūki Tabata / Shueisha / Studio Pierrot); Gurren Lagann and Kill la Kill (Gainax /
Trigger / Aniplex); Sailor Moon (Naoko Takeuchi / Kodansha / Toei Animation); Neon Genesis Evangelion
(khara / Gainax); Frieren: Beyond Journey's End (Kanehito Yamada / Tsukasa Abe / Shogakukan / Madhouse);
Dandadan (Yukinobu Tatsu / Shueisha / Science SARU); Blue Lock (Muneyuki Kaneshiro / Yusuke Nomura /
Kodansha / 8bit); Haikyu!! (Haruichi Furudate / Shueisha / Production I.G); Cowboy Bebop (Sunrise); Death
Note (Tsugumi Ohba / Takeshi Obata / Shueisha / Madhouse). Every in-game effect is Brightest Day's own
mapping onto its generic move operations; no anime assets, logos, dialogue beyond short move names,
incantations, lyrics or sounds are used.

## Shonen classics power vocabulary (`assets/powers/anime_classics.ron`; the Naruto / Dragon Ball / Bleach entries in `base_techniques.ron`, `base_one_piece.ron`, `base_jojo.ron`)

Character, technique, move and catchphrase NAMES from these anime / manga franchises are used only as
trigger words a player may type or say, for private use; replace them before any public release:
Naruto, Naruto Shippuden and Boruto (Masashi Kishimoto / Mikio Ikemoto / Shueisha / Studio Pierrot);
Dragon Ball, Dragon Ball Z, Dragon Ball GT and Dragon Ball Super (Akira Toriyama / Toyotarou / Shueisha /
Toei Animation); One Piece (Eiichiro Oda / Shueisha / Toei Animation); Bleach (Tite Kubo / Shueisha / Studio
Pierrot); JoJo's Bizarre Adventure (Hirohiko Araki / Shueisha / David Production). Every in-game effect is
Brightest Day's own mapping onto its generic move operations; no anime assets, logos, dialogue beyond short
move names and shouts, incantations, lyrics or sounds are used.

## TV & cartoon power vocabulary (`assets/powers/tv_cartoons.ron`, `base_cartoons.ron`)

Character, technique, move, gadget and catchphrase NAMES from these television / animation franchises are
used only as trigger words a player may type or say, for private use; replace them before any public
release: Avatar: The Last Airbender and The Legend of Korra (Nickelodeon / Paramount); Transformers
(Hasbro / Takara Tomy); Teenage Mutant Ninja Turtles (Paramount / Nickelodeon); Ben 10 (Cartoon Network /
Warner Bros. Discovery); Power Rangers and Voltron (Hasbro / Toei / DreamWorks / World Events Productions);
Invincible (Robert Kirkman / Skybound / Amazon); The Boys (Garth Ennis / Darick Robertson / Sony / Amazon);
Stranger Things (Netflix); Teen Titans (DC / Warner Bros.); Rick and Morty, Samurai Jack, Dexter's
Laboratory, The Powerpuff Girls, Adventure Time (Cartoon Network / Adult Swim / Warner Bros. Discovery);
Doctor Who (BBC); Gravity Falls, Kim Possible, Phineas and Ferb, The Owl House (Disney); Steven Universe
(Rebecca Sugar / Cartoon Network); Danny Phantom and SpongeBob SquarePants (Nickelodeon / Paramount);
Miraculous: Tales of Ladybug & Cat Noir (ZAG / Method Animation); Looney Tunes and Scooby-Doo (Warner
Bros.); Arcane (Riot Games / Fortiche); Castlevania (Konami / Netflix); The Witcher (Andrzej Sapkowski / CD
Projekt / Netflix); Buffy the Vampire Slayer (20th Century Studios / Disney); Supernatural (Warner Bros.);
He-Man and the Masters of the Universe, She-Ra (Mattel); ThunderCats (Warner Bros.); Ninjago (LEGO). Every
in-game effect is Brightest Day's own mapping onto its generic move operations; no show assets, logos,
dialogue beyond short move names and catchphrases, incantations, lyrics or sounds are used.
