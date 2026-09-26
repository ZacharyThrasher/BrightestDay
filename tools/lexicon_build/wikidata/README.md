# Wikidata named-entity layer

Builds `tools/lexicon_build/data/entities.jsonl.gz` (+ `entities_sample.json` for review): ~300k things a
player might name (Eeyore, Grogu, Mothra, Millennium Falcon, Excalibur, corgi, axolotl, T. rex, Big Ben,
AK-47, Rubik's Cube ...). Each record says what the thing *is*. It never says how the thing behaves:
the lexicon build maps `classes`/`family`/size to Forms and Mechanics.

```
python tools/lexicon_build/wikidata/harvest.py                         # ~5-8 min cold, ~30 s cached
BD_WIKIDATA_OFFLINE=1 python tools/lexicon_build/wikidata/harvest.py   # cache only, fails if a query is missing
python tools/lexicon_build/wikidata/coverage.py                        # hit rate on test_names.txt
python tools/lexicon_build/wikidata/coverage.py eeyore "baby yoda"     # look up names
```

## Method
Everything comes from the public SPARQL endpoint (`query.wikidata.org`). Queries run serially or 3 at
a time, with a descriptive User-Agent. The client honours 429 Retry-After and splits a batch that
times out. Each response is cached as gzip JSON under `cache/wikidata/sparql/`, keyed by the hash of
the query text, so a rerun makes no network calls. Dumps and the REST API were not needed: the
largest query (taxa with 20 or more sitelinks) takes about 50 s.

1. **Closures.** For each root in `families.py`, fetch every `P279*` subclass with its sitelink count.
2. **Candidates.**
   - *instances*: `P31` of any subclass, with at least `inst_min` sitelinks.
   - *kinds*: subclasses with at least `type_min` sitelinks.
   - *taxa*: an English common name (P1843) and 3 or more sitelinks, or 20 or more sitelinks, or a
     fossil taxon with 8 or more sitelinks.
   - *extra*: anything with "from narrative universe" (P1080), including 0-sitelink fan-universe items
     such as the keyblade.

   The structure, tool, appliance, toy and landform trees are pruned of settlement, admin-area,
   organisation and event classes, because bad P279 edges otherwise let in universities and papal
   journeys.
3. **Details.** Fetched in batches of 300 using one UNION query per batch.
4. **Graph.** Class labels plus 4 levels of `P279`, and taxon parents (`P171`) up to the root.
5. **Assemble.** Build the class chains and flags, then drop franchise records that would shadow
   their own character ("Shrek", "Gundam").

## Record schema (one JSON object per line, sorted by sitelinks desc)
| field | type | notes |
|---|---|---|
| `qid` | str | Wikidata id |
| `label` | str | English label, or the `mul` label if there is no English one. Original casing. |
| `aliases` | [str] | en/mul aliases. Taxa also get P1843 common names and the P225 scientific name. Breeds get "<label> cat/dog/horse". |
| `sitelinks` | int | popularity; 0 is allowed only for narrative-universe items |
| `description` | str | English description, may be "" |
| `classes` | [str] | Direct P31 labels (P279 for kinds), then up to 4 levels of P279 ancestors. Deduplicated, most specific first, generic top-level classes removed, at most 16. For taxa: `"taxon"` then parent taxa at key ranks (genus, family, order, class, phylum, kingdom) or well-known clades (80 or more sitelinks). |
| `is_fictional` | bool | a P31/P279 in the fictional/mythical/cryptid trees, or a fiction family |
| `franchise` | str? | from P8345 media franchise, else P1080 narrative universe, else P1441 present in work |
| `height_m` / `mass_kg` | float? | median of the SI-normalised P2048/P2067 values |
| `color` | [str]? | P462 labels |
| `common_word_collision` | true? | Set when the lowercase label is a WordNet common noun (for example Venom, Hulk, Shrek's Donkey). Taxa and breeds are only flagged if WordNet has no animal or plant sense. Collisions with fewer than 5 sitelinks are dropped (fewer than 2 if fictional). |
| `family` | str | pokemon, kaiju, mecha, robot, myth_creature, fiction_character, fiction_other, myth_other, breed, weapon, spacecraft, aircraft, ship, vehicle, structure, instrument, food, toy, sports_equipment, appliance, tool, product, landform, franchise, taxon, dinosaur |
| `kind` | str | `instance` (an individual, e.g. Eeyore or Big Ben), `type` (a kind, e.g. AK-47 or Batmobile), or `taxon` |
| `taxon_rank` | str? | taxa only |
