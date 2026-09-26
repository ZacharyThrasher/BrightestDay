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
