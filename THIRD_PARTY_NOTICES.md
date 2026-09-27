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
