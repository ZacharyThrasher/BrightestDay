# tools/lexicon_build — vocabulary knowledge → `assets/lexicon.bin`

Brightest Day's rule: **external resources explain words; the game alone decides mechanics.** This
pipeline turns ConceptNet, WordNet, dariusk/corpora, PokeAPI and the Wikidata harvest into a compact
index that `crates/bd_lexicon` loads at runtime. Only classic graph/lexical processing — no ML models.

## One command

```sh
pip install nltk wordfreq          # once (numpy not required)
python tools/lexicon_build/build.py            # add --release to pack with an optimised build
```

Everything is cached in `tools/lexicon_build/cache/` (gitignored). Typical runtimes on the dev box:

| step | first run | cached |
|---|---|---|
| downloads (ConceptNet 475 MB, WordNet, corpora, PokeAPI CSVs) | ~1 min | 0 |
| ConceptNet filter (34 M lines → 3 M English edges) | ~90 s | 0 |
| WordNet / ConceptNet / corpora / Pokémon / Wikidata / modifiers | ~100 s | ~100 s |
| `cargo run --example pack` (JSON → bin + reload smoke test) | ~10 s (+compile) | ~10 s |

Outputs: `assets/lexicon.bin` (runtime index), `assets/lexicon_preview.json` (~600 sampled entries for
review), `cache/lexicon.json` (full intermediate, ~170 MB).

Options: `--no-pack` (stop after JSON), `--entities <path>` (default `data/entities.jsonl.gz`, the
Wikidata harvest from `tools/lexicon_build/wikidata/`; skipped if absent).

## Pinned sources (`sources.py`)
* ConceptNet 5.7 assertions: `s3.amazonaws.com/conceptnet/downloads/2019/edges/conceptnet-assertions-5.7.0.csv.gz`
* WordNet 3.0 via nltk_data @ `550b6625…` (`packages/corpora/wordnet.zip`)
* dariusk/corpora @ `2e7adec1…` (zip)
* PokeAPI CSVs @ `a003ae37…` (`data/v2/csv/{pokemon,pokemon_species,pokemon_types,types,pokemon_shapes,pokemon_colors,pokemon_species_names}.csv`)
* word frequencies: the `wordfreq` package (data, not a model)

## How words become recipes
1. **Anchors** (`anchors.py`) are the *only* place vocabulary touches the closed sets: `FORM_SYNSETS`
   (hammer.n.02 → Hammer…), `MECH_SYNSETS`, `ELEM_SYNSETS`, `FEATURE_SYNSETS`, `SIZE_SYNSETS`,
   `WORD_ANCHORS` (exact words), `MECH_CONCEPTS`/`ELEM_CONCEPTS`/`PART_MECH` (ConceptNet phrases like
   "slow descent through air" → SlowFall), modifier tables (adjectives, verbs, intensifiers,
   collectives, temperament words), `POP_CULTURE`, Pokémon shape/type maps, `denylist.txt` (slurs only).
2. **WordNet propagation**: anchors flow down hypernym trees (nearest anchor wins for form, decaying
   confidence 0.95/level; mechanics/features are inherited unions; lexicographer-file fallbacks make
   every concrete noun resolve). Each lemma picks its best sense (sense order × tag counts ×
   resolvability; literal senses beat vulgar ones, slur senses are never used).
   `maul → sledgehammer → hammer (WordNet hypernym)`.
3. **ConceptNet evidence**: UsedFor/CapableOf/HasProperty into anchor phrases, HasA/PartOf parts
   (scorpion RelatedTo "stinging tail" → ArticulatedTail), elements; strong relations
   (IsA/Synonym/DerivedFrom/SimilarTo) resolve words WordNet lacks (lightsaber DerivedFrom saber).
4. **Curated data**: corpora lists (dinosaurs, monsters, Lovecraft, weapons, instruments…), every
   Pokémon from PokeAPI (shape → body plan, types → element + mechanics, real height/weight, colour
   accent), pop-culture aliases, abstract nouns → *spirits* (Blob + Flies + temperament).
5. **Wikidata entities** (~300k): the most informative class wins (generic "character" classes are
   down-weighted; "fictional"/"anthropomorphic" stripped: Eeyore → *anthropomorphic mule* → mule),
   then the description head noun, then the harvest's `family`. Common nouns beat entity collisions
   unless the word is weak and the entity popular. Real-world religious figures are excluded.
6. **`assets/icons/*_names.txt`** (pop-culture scanners) merge last as high-priority curated names.
7. **Modifiers**: adjectives (element/size/mechanic/feature/temperament), verbs with generated
   inflections (shoots/shooting/shootin/shot), payload verbs, intensifiers (incl. profanity: owner
   policy allows it), collectives; expanded one hop through WordNet similar_to / verb synonyms.

## Runtime (`crates/bd_lexicon`)
`Lexicon::load("assets/lexicon.bin")` (+ `assets/icons.ron`, `assets/icons/*.ron`) → `resolve`,
`resolve_spoken`, `complete`, `bias_prompt`, `is_blocked_name`, `random_phrase`. See the crate docs.

```sh
CARGO_TARGET_DIR=target/lexicon cargo run -p bd_lexicon --example probe -- "giant eeyore shootin laser beams"
CARGO_TARGET_DIR=target/lexicon cargo test -p bd_lexicon -- --nocapture     # incl. coverage report
```

## Maintenance
* New bd_core enum variant → `python tools/lexicon_build/gen_tables.py` (regenerates
  `crates/bd_lexicon/src/tables.rs`; exhaustive matches make a stale table a compile error), then map
  words to it in `anchors.py` and rebuild.
* Wrong reading for a word? Prefer a `WORD_ANCHORS` entry or a synset anchor over special cases in code.
