"""Pinned external sources + download/cache helpers for the Brightest Day lexicon build.

Every URL is pinned (a fixed release file or a git commit SHA) so the build is reproducible.
All downloads land in tools/lexicon_build/cache/ (gitignored). Nothing here contains behaviour: these
sources only *explain words*; mechanics are decided by bd_core/bd_sim.
"""

from __future__ import annotations

import gzip
import json
import os
import sys
import time
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CACHE = HERE / "cache"

CONCEPTNET_URL = (
    "https://s3.amazonaws.com/conceptnet/downloads/2019/edges/conceptnet-assertions-5.7.0.csv.gz"
)
CORPORA_SHA = "2e7adec11a0561696c236ae1fca82e88be173d42"
CORPORA_URL = f"https://github.com/dariusk/corpora/archive/{CORPORA_SHA}.zip"
NLTK_DATA_SHA = "550b6625bcef1f2abff2ff770a5a0d272c9c6b2a"
WORDNET_URL = (
    f"https://raw.githubusercontent.com/nltk/nltk_data/{NLTK_DATA_SHA}/packages/corpora/wordnet.zip"
)
POKEAPI_SHA = "a003ae375b69a99907ec273fe100d97e7f36321c"
POKEAPI_BASE = f"https://raw.githubusercontent.com/PokeAPI/pokeapi/{POKEAPI_SHA}/data/v2/csv"
POKEAPI_FILES = [
    "pokemon",
    "pokemon_species",
    "pokemon_types",
    "types",
    "pokemon_shapes",
    "pokemon_colors",
    "pokemon_species_names",
    "moves",
    "move_names",
    "move_damage_classes",
    "move_targets",
    "move_meta",
]

# ConceptNet relations we keep, with the propagation weight used by build.py.
CN_RELATIONS = {
    "IsA", "Synonym", "FormOf", "RelatedTo", "UsedFor", "CapableOf", "HasA", "PartOf", "MadeOf",
    "HasProperty", "AtLocation", "SimilarTo", "DerivedFrom", "ReceivesAction", "MannerOf",
}
CN_FILTERED = CACHE / "conceptnet_en.tsv.gz"


def log(*a):
    print(*a, file=sys.stderr, flush=True)


def download(url: str, dest: Path) -> Path:
    if dest.exists() and dest.stat().st_size > 0:
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_suffix(dest.suffix + ".part")
    log(f"download {url}")
    t = time.time()
    with urllib.request.urlopen(url) as r, open(tmp, "wb") as f:
        while True:
            b = r.read(1 << 20)
            if not b:
                break
            f.write(b)
    tmp.replace(dest)
    log(f"  -> {dest.name} {dest.stat().st_size / 1e6:.1f} MB in {time.time() - t:.1f}s")
    return dest


def fetch_all():
    download(CONCEPTNET_URL, CACHE / "conceptnet-assertions-5.7.0.csv.gz")
    download(CORPORA_URL, CACHE / "corpora.zip")
    download(WORDNET_URL, CACHE / "wordnet.zip")
    for f in POKEAPI_FILES:
        download(f"{POKEAPI_BASE}/{f}.csv", CACHE / "pokeapi" / f"{f}.csv")
    # NLTK expects <data>/corpora/wordnet.zip
    nd = CACHE / "nltk_data" / "corpora"
    nd.mkdir(parents=True, exist_ok=True)
    wz = nd / "wordnet.zip"
    if not wz.exists():
        wz.write_bytes((CACHE / "wordnet.zip").read_bytes())


def _term(uri: str):
    # /c/en/sledge_hammer/n/wn/artifact -> ("sledge hammer", "n")
    p = uri.split("/")
    if len(p) < 4 or p[2] != "en":
        return None, None
    return p[3].replace("_", " "), (p[4] if len(p) > 4 else "")


def filter_conceptnet(force: bool = False) -> Path:
    """Stream the 475 MB assertions dump once and keep English<->English edges of useful relations."""
    if CN_FILTERED.exists() and not force:
        return CN_FILTERED
    src = CACHE / "conceptnet-assertions-5.7.0.csv.gz"
    t = time.time()
    n_in = n_out = 0
    tmp = CN_FILTERED.with_suffix(".part")
    with gzip.open(src, "rt", encoding="utf-8") as fi, gzip.open(tmp, "wt", encoding="utf-8", compresslevel=5) as fo:
        for line in fi:
            n_in += 1
            parts = line.split("\t", 4)
            if len(parts) < 5:
                continue
            rel = parts[1][3:]
            if rel not in CN_RELATIONS:
                continue
            if not (parts[2].startswith("/c/en/") and parts[3].startswith("/c/en/")):
                continue
            a, ap = _term(parts[2])
            b, bp = _term(parts[3])
            if not a or not b or a == b:
                continue
            if a.count(" ") > 3 or b.count(" ") > 3:
                continue
            try:
                meta = json.loads(parts[4])
                w = float(meta.get("weight", 1.0))
            except Exception:
                w = 1.0
            fo.write(f"{rel}\t{a}\t{ap}\t{b}\t{bp}\t{w:.3f}\n")
            n_out += 1
            if n_in % 5_000_000 == 0:
                log(f"  conceptnet: {n_in / 1e6:.0f}M lines scanned, {n_out} kept")
    tmp.replace(CN_FILTERED)
    log(f"conceptnet filtered: {n_out} edges from {n_in} lines in {time.time() - t:.0f}s")
    return CN_FILTERED


def corpora_zip() -> zipfile.ZipFile:
    return zipfile.ZipFile(CACHE / "corpora.zip")


def corpora_json(z: zipfile.ZipFile, rel: str):
    name = f"corpora-{CORPORA_SHA}/data/{rel}"
    with z.open(name) as f:
        return json.load(f)


if __name__ == "__main__":
    fetch_all()
    filter_conceptnet(force="--force" in sys.argv)
