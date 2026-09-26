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
