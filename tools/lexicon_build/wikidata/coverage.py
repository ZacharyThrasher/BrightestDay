"""Coverage check for entities.jsonl.gz against test_names.txt.

    python tools/lexicon_build/wikidata/coverage.py            # hit rate, misses, 30 example chains
    python tools/lexicon_build/wikidata/coverage.py eeyore "baby yoda"   # look up specific names

A name hits if its normalised form equals the normalised label or any alias of some entity
(normalisation: casefold, strip accents/punctuation, drop leading "the/a/an"). When several entities
share a key, the one with most sitelinks wins (as the lexicon would do).
"""

from __future__ import annotations

import gzip
import json
import re
import sys
import unicodedata
from pathlib import Path

HERE = Path(__file__).resolve().parent
ENTITIES = HERE.parent / "data" / "entities.jsonl.gz"
TESTS = HERE / "test_names.txt"

SHOW = [
    "Eeyore", "Shrek", "Donkey", "Sonic", "Totoro", "Grogu", "Groot", "Mothra", "Kraken", "Cerberus",
    "Balrog", "Nessie", "corgi", "capybara", "axolotl", "T. rex", "Millennium Falcon", "TARDIS",
    "Batmobile", "Excalibur", "Master Sword", "Keyblade", "Optimus Prime", "Gundam", "Pikachu",
    "Clippy", "Eiffel Tower", "Big Ben", "Rubik's Cube", "AK-47",
]


def norm(s: str) -> str:
    s = unicodedata.normalize("NFKD", s)
    s = "".join(c for c in s if not unicodedata.combining(c)).casefold()
    s = s.replace("&", " and ")
    s = re.sub(r"[^\w\s]", "", s)
    s = re.sub(r"\s+", " ", s).strip()
    s = re.sub(r"^(the|a|an) ", "", s)
    return s


def load_index():
    idx: dict[str, dict] = {}
    n = 0
    with gzip.open(ENTITIES, "rt", encoding="utf-8") as f:
        for line in f:
            r = json.loads(line)
            n += 1
            for k in [r["label"], *r["aliases"]]:
                key = norm(k)
                if key and (key not in idx or idx[key]["sitelinks"] < r["sitelinks"]):
                    idx[key] = r
    return idx, n


def lookup(idx, name: str):
    for v in name.split(" / "):
        r = idx.get(norm(v))
        if r:
            return r
    return None


def fmt(r) -> str:
    extra = []
    if r.get("franchise"):
        extra.append(f"franchise={r['franchise']}")
    if r.get("height_m"):
        extra.append(f"h={r['height_m']}m")
    if r.get("common_word_collision"):
        extra.append("COLLISION")
    return (f"{r['label']} [{r['qid']}, {r['family']}/{r['kind']}, sl={r['sitelinks']}"
            f"{', fict' if r['is_fictional'] else ''}] -> {' > '.join(r['classes'][:7])}"
            + (f"  ({'; '.join(extra)})" if extra else ""))


def main():
    idx, n = load_index()
    if len(sys.argv) > 1:
        for name in sys.argv[1:]:
            r = lookup(idx, name)
            print(f"{name}: {fmt(r) if r else 'MISS'}")
        return
    names = [l.strip() for l in TESTS.read_text(encoding="utf-8").splitlines()
             if l.strip() and not l.startswith("#")]
    names = list(dict.fromkeys(names))
    hits, misses = [], []
    for name in names:
        (hits if lookup(idx, name) else misses).append(name)
    print(f"entities: {n}, keys: {len(idx)}")
    print(f"coverage: {len(hits)}/{len(names)} = {100 * len(hits) / len(names):.1f}%")
    print(f"misses ({len(misses)}): {', '.join(misses)}")
    print("\nexamples:")
    for name in SHOW:
        r = lookup(idx, name)
        print(f"  {name:18s} {fmt(r) if r else 'MISS'}")


if __name__ == "__main__":
    main()
