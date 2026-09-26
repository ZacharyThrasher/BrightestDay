"""Harvest the Wikidata named-entity layer for the Brightest Day lexicon.

    python tools/lexicon_build/wikidata/harvest.py            # online (uses cache where present)
    BD_WIKIDATA_OFFLINE=1 python tools/lexicon_build/wikidata/harvest.py   # cache only

Output: tools/lexicon_build/data/entities.jsonl.gz (+ entities_sample.json). See README.md for schema.

Stages (every SPARQL response cached under tools/lexicon_build/cache/wikidata/sparql/):
  1. closures   -- for every family root: all P279* subclasses (+ their sitelink counts)
  2. candidates -- instances (P31 of any subclass, sitelinks >= inst_min) and kinds (subclass with
                   sitelinks >= type_min); taxa via English common name / sitelinks
  3. details    -- labels (en, fallback mul), aliases, description, sitelinks, P31/P279, franchise,
                   height/mass (SI-normalised), colour, taxon parent/rank/common names
  4. graph      -- labels + P279 parents for classes (4 levels), P171 parents for taxa (to the root)
  5. assemble   -- class chains, is_fictional, common-word collision flag, write JSONL.gz
"""

from __future__ import annotations

import gzip
import json
import re
import statistics
import sys
import time
import zipfile
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import families as F  # noqa: E402
import wdqs  # noqa: E402
from wdqs import QueryTimeout, log, qid  # noqa: E402

DATA = HERE.parent / "data"
OUT = DATA / "entities.jsonl.gz"
SAMPLE = DATA / "entities_sample.json"
WORDNET_ZIP = HERE.parent / "cache" / "wordnet.zip"
WORKERS = 3
BREED_SUFFIX = {"dog breed": "dog", "cat breed": "cat", "horse breed": "horse", "cattle breed": "cattle",
                "chicken breed": "chicken", "pig breed": "pig", "sheep breed": "sheep",
                "goat breed": "goat", "rabbit breed": "rabbit"}


def vals(qids) -> str:
    return " ".join("wd:" + q for q in qids)


def chunks(xs, n):
    for i in range(0, len(xs), n):
        yield xs[i:i + n]


def pmap(fn, items, workers=WORKERS):
    with ThreadPoolExecutor(workers) as ex:
        return list(ex.map(fn, items))


# ---------------------------------------------------------------- 1. closures


def closure(root: str) -> dict[str, int]:
    rows = wdqs.query(
        "SELECT ?c ?sl WHERE { ?c wdt:P279* wd:%s . OPTIONAL { ?c wikibase:sitelinks ?sl } }" % root,
        tag=f"closure {root}",
    )
    out = {}
    for r in rows:
        out[qid(r["c"])] = int(r.get("sl", 0))
    for b in F.CLASS_BLOCKLIST:
        out.pop(b, None)
    return out


def class_set(root: str) -> set[str]:
    """Plain P279* closure (no sitelinks) -- used for huge exclusion trees like `occurrence`."""
    rows = wdqs.query("SELECT ?c WHERE { ?c wdt:P279* wd:%s }" % root, tag=f"classes {root}")
    return {qid(r["c"]) for r in rows}


def excluded_classes() -> set[str]:
    ex: set[str] = set()
    for root in F.EXCLUDE_ROOTS:
        ex |= class_set(root)
    for root in F.EXCLUDE_KEEP_ROOTS:
        ex -= class_set(root)
    return ex


# ---------------------------------------------------------------- 2. candidates


def instances(classes: list[str], min_sl: int, tag: str) -> dict[str, int]:
    """Items with P31 in `classes` and >= min_sl sitelinks. Splits batches on timeout."""
    found: dict[str, int] = {}
    gaps: list[str] = []

    def run(batch: list[str]):
        q = (
            "SELECT ?i ?sl WHERE { VALUES ?c { %s } ?i wdt:P31 ?c . "
            "?i wikibase:sitelinks ?sl . FILTER(?sl >= %d) }" % (vals(batch), min_sl)
        )
        try:
            return [(qid(r["i"]), int(r["sl"])) for r in wdqs.query(q, timeout_ok=True, tag=tag)]
        except QueryTimeout:
            if len(batch) == 1:
                gaps.append(batch[0])
                log(f"  TIMEOUT single class {batch[0]} ({tag}); skipped")
                return []
            h = len(batch) // 2
            return run(batch[:h]) + run(batch[h:])

    batches = list(chunks(sorted(classes), 400))
    for res in pmap(run, batches):
        for i, sl in res:
            found[i] = sl
    if gaps:
        log(f"  {tag}: {len(gaps)} classes skipped by timeout: {gaps[:10]}")
    return found


def taxa_candidates() -> dict[str, int]:
    found: dict[str, int] = {}
    rows = wdqs.query(
        "SELECT ?i ?sl WHERE { ?i wdt:P1843 ?cn FILTER(lang(?cn)='en') ?i wikibase:sitelinks ?sl "
        "FILTER(?sl>=1) }",
        tag="taxa common-name",
    )
    for r in rows:
        if int(r["sl"]) >= F.TAXON_CN_MIN:
            found[qid(r["i"])] = int(r["sl"])
    n_cn = len(found)
    q20 = "SELECT ?i ?sl WHERE { ?i wdt:P31 wd:Q16521 . ?i wikibase:sitelinks ?sl FILTER(?sl>=20) }"
    try:
        rows = wdqs.query(q20, tag="taxa sl>=20")
    except QueryTimeout:
        log("  taxa sl>=20 timed out; partitioning by rank")
        rows = []
        for rank in ["Q7432", "Q34740", "Q35409"]:
            try:
                rows += wdqs.query(
                    "SELECT ?i ?sl WHERE { ?i wdt:P105 wd:%s . ?i wdt:P31 wd:Q16521 . "
                    "?i wikibase:sitelinks ?sl FILTER(?sl>=%d) }" % (rank, F.TAXON_MIN),
                    tag=f"taxa rank {rank}", timeout_ok=True)
            except QueryTimeout:
                log(f"  taxa rank {rank} timed out too")
    for r in rows:
        if int(r["sl"]) >= F.TAXON_MIN:
            found[qid(r["i"])] = int(r["sl"])
    for r in wdqs.query(
        "SELECT ?i ?sl WHERE { ?i wdt:P31 wd:Q23038290 . ?i wikibase:sitelinks ?sl FILTER(?sl>=%d) }"
        % F.TAXON_FOSSIL_MIN, tag="fossil taxa"):
        found[qid(r["i"])] = int(r["sl"])
    log(f"  taxa: {n_cn} with English common name, {len(found)} total")
    return found


# ---------------------------------------------------------------- 3/4. details + graph

DETAIL_Q = """SELECT ?item ?p ?v WHERE { VALUES ?item { %s }
 { ?item rdfs:label ?v . FILTER(LANG(?v) IN ("en","mul")) BIND(CONCAT("L", LANG(?v)) AS ?p) }
 UNION { ?item skos:altLabel ?v . FILTER(LANG(?v) IN ("en","mul")) BIND("A" AS ?p) }
 UNION { ?item schema:description ?v . FILTER(LANG(?v) = "en") BIND("D" AS ?p) }
 UNION { ?item wikibase:sitelinks ?v . BIND("S" AS ?p) }
 UNION { ?item wdt:P31 ?v . BIND("P31" AS ?p) }
 UNION { ?item wdt:P279 ?v . BIND("P279" AS ?p) }
 UNION { ?item wdt:P8345 ?v . BIND("P8345" AS ?p) }
 UNION { ?item wdt:P1080 ?v . BIND("P1080" AS ?p) }
 UNION { ?item wdt:P1441 ?v . BIND("P1441" AS ?p) }
 UNION { ?item wdt:P462 ?v . BIND("P462" AS ?p) }
 UNION { ?item p:P2048/psn:P2048/wikibase:quantityAmount ?v . BIND("P2048" AS ?p) }
 UNION { ?item p:P2067/psn:P2067/wikibase:quantityAmount ?v . BIND("P2067" AS ?p) }
 UNION { ?item wdt:P1843 ?v . FILTER(LANG(?v) = "en") BIND("CN" AS ?p) }
 UNION { ?item wdt:P225 ?v . BIND("TN" AS ?p) }
 UNION { ?item wdt:P171 ?v . BIND("P171" AS ?p) }
 UNION { ?item wdt:P105 ?v . BIND("P105" AS ?p) }
}"""

NODE_Q = """SELECT ?item ?p ?v WHERE { VALUES ?item { %s }
 { ?item rdfs:label ?v . FILTER(LANG(?v) IN ("en","mul")) BIND(CONCAT("L", LANG(?v)) AS ?p) }
 UNION { ?item wikibase:sitelinks ?v . BIND("S" AS ?p) }
 UNION { ?item wdt:P279 ?v . BIND("P279" AS ?p) }
 UNION { ?item wdt:P171 ?v . BIND("P171" AS ?p) }
 UNION { ?item wdt:P105 ?v . BIND("P105" AS ?p) }
}"""

ENTITY_PROPS = {"P31", "P279", "P8345", "P1080", "P1441", "P462", "P171", "P105"}


def fetch(template: str, qids: list[str], batch: int, tag: str) -> dict[str, dict[str, list]]:
    out: dict[str, dict[str, list]] = {}

    def run(b):
        try:
            return wdqs.query(template % vals(b), tag=tag)
        except QueryTimeout:
            if len(b) <= 5:
                log(f"  {tag}: timeout on tiny batch {b}")
                return []
            h = len(b) // 2
            return run(b[:h]) + run(b[h:])

    bs = list(chunks(sorted(qids), batch))
    t = time.time()
    done = 0
    with ThreadPoolExecutor(WORKERS) as ex:
        for rows in ex.map(run, bs):
            done += 1
            if done % 100 == 0:
                log(f"  {tag}: {done}/{len(bs)} batches ({time.time() - t:.0f}s) {wdqs.stats()}")
            for r in rows:
                i = qid(r["item"])
                p = r["p"]
                v = r["v"]
                if p in ENTITY_PROPS:
                    v = qid(v)
                d = out.setdefault(i, {})
                lst = d.setdefault(p, [])
                if v not in lst:
                    lst.append(v)
    return out


def label_of(d: dict) -> str | None:
    for k in ("Len", "Lmul"):
        if d.get(k):
            return d[k][0]
    return None


# ---------------------------------------------------------------- collision check


def wordnet_nouns() -> dict[str, set[str]]:
    """lowercase-lemma (as written in WordNet, i.e. a common noun) -> set of lexfile names."""
    out: dict[str, set[str]] = defaultdict(set)
    if not WORDNET_ZIP.exists():
        log("  WARNING: wordnet.zip missing; collision flag disabled")
        return out
    with zipfile.ZipFile(WORDNET_ZIP) as z:
        lexnames = {}
        for line in z.read("wordnet/lexnames").decode().splitlines():
            p = line.split()
            if len(p) >= 2:
                lexnames[int(p[0])] = p[1]
        for line in z.read("wordnet/data.noun").decode("utf-8", "replace").splitlines():
            if line.startswith("  "):
                continue
            p = line.split(" ")
            lex = lexnames.get(int(p[1]), "")
            n = int(p[3], 16)
            for k in range(n):
                w = p[4 + 2 * k].replace("_", " ")
                if w == w.lower():
                    out[w].add(lex)
    return out


# ---------------------------------------------------------------- main


def main():
    t0 = time.time()
    DATA.mkdir(parents=True, exist_ok=True)

    # 1+2: closures and candidates
    fam_of: dict[str, str] = {}          # entity -> family
    kind_of: dict[str, str] = {}         # entity -> instance|type|taxon
    fam_counts_raw = Counter()
    all_closures: dict[str, dict[str, int]] = {}
    excl = excluded_classes()
    built: set[str] = set()
    for r in F.BUILT_ROOTS:
        built |= class_set(r)
    log(f"excluded classes: {len(excl)}; built classes: {len(built)}")
    for fam, roots in F.FAMILIES:
        for root, rlabel, inst_min, type_min in roots:
            t = time.time()
            cl = closure(root)
            all_closures[root] = cl
            if fam in F.PRUNED_FAMILIES:
                cl = {c: s for c, s in cl.items() if c not in excl}
            n_i = n_t = 0
            if inst_min is not None:
                for i in instances(list(cl), inst_min, f"{fam}/{rlabel}"):
                    if i not in fam_of:
                        fam_of[i] = fam
                        kind_of[i] = "instance"
                        n_i += 1
            if type_min is not None:
                for c, sl in cl.items():
                    if c != root and sl >= type_min and c not in fam_of:
                        fam_of[c] = fam
                        kind_of[c] = "type"
                        n_t += 1
            fam_counts_raw[fam] += n_i + n_t
            log(f"[{fam}] {rlabel} ({root}): {len(cl)} subclasses -> +{n_i} instances, +{n_t} kinds "
                f"({time.time() - t:.1f}s)")
    for fam, kind, q in F.EXTRA_QUERIES:
        n = 0
        for r in wdqs.query(q, tag=f"extra {fam}"):
            i = qid(r["i"])
            if i not in fam_of:
                fam_of[i] = fam
                kind_of[i] = kind
                n += 1
        log(f"[{fam}] extra query -> +{n}")
    for i in taxa_candidates():
        if i not in fam_of:
            fam_of[i] = "taxon"
            kind_of[i] = "taxon"
    log(f"candidates: {len(fam_of)} ({time.time() - t0:.0f}s)")
    if "--candidates-only" in sys.argv:
        log(str(Counter(fam_of.values()).most_common()))
        return

    fictional_classes: set[str] = set()
    for r in F.FICTIONAL_ROOTS:
        fictional_classes |= set(all_closures.get(r) or closure(r))

    character_classes = set(all_closures["Q95074"])

    # 3: details
    det = fetch(DETAIL_Q, list(fam_of), 300, "details")
    log(f"details: {len(det)} entities ({time.time() - t0:.0f}s)")

    # 4: class graph (P279, 4 levels) and taxon graph (P171 to root)
    nodes: dict[str, dict] = {}
    frontier = set()
    aux = set()
    for i, d in det.items():
        frontier |= set(d.get("P31", [])) | set(d.get("P279", []))
        aux |= set(d.get("P8345", [])[:3]) | set(d.get("P1080", [])[:3]) | set(d.get("P1441", [])[:3])
        aux |= set(d.get("P462", []))
    for level in range(5):
        need = [c for c in frontier if c not in nodes and c not in det]
        if not need:
            break
        nodes.update(fetch(NODE_Q, need, 500, f"classes L{level}"))
        nxt = set()
        for c in frontier:
            nxt |= set((nodes.get(c) or det.get(c) or {}).get("P279", []))
        frontier = nxt
    tax_frontier = set()
    for i, d in det.items():
        if kind_of[i] == "taxon" or d.get("P171"):
            tax_frontier |= set(d.get("P171", []))
    for level in range(80):
        need = [c for c in tax_frontier if c not in nodes and c not in det]
        if not need:
            break
        nodes.update(fetch(NODE_Q, need, 500, f"taxa L{level}"))
        nxt = set()
        for c in tax_frontier:
            nxt |= set((nodes.get(c) or det.get(c) or {}).get("P171", []))
        tax_frontier = nxt
    need = [c for c in aux if c not in nodes and c not in det]
    nodes.update(fetch(NODE_Q, need, 500, "aux labels"))
    log(f"graph: {len(nodes)} nodes ({time.time() - t0:.0f}s)")

    def info(x):
        return nodes.get(x) or det.get(x) or {}

    def lab(x):
        return label_of(info(x))

    # 5: assemble
    wn = wordnet_nouns()
    allowed_lex = {
        "taxon": {"noun.animal", "noun.plant"},
        "breed": {"noun.animal"},
    }

    def class_chain(i, d, kind) -> list[str]:
        out: list[str] = []
        seen = set()
        level = list(d.get("P31", [])) if kind != "type" else []
        if kind == "type" or not level or fam == "taxon":
            level = list(d.get("P279", [])) + level
        for depth in range(5):
            nxt = []
            for c in level:
                if c in seen:
                    continue
                seen.add(c)
                lb = lab(c)
                if lb and lb not in F.GENERIC_LABELS and lb not in out and lb != "Wikimedia category":
                    out.append(lb)
                nxt.extend(info(c).get("P279", []))
            level = nxt
            if len(out) >= 16:
                break
        return out[:16]

    def taxon_chain(i, d) -> tuple[list[str], bool]:
        out: list[str] = []
        dino = False
        aves = False
        cur = d.get("P171", [])
        seen = set()
        steps = 0
        while cur and steps < 80:
            p = cur[0]
            if p in seen:
                break
            seen.add(p)
            steps += 1
            n = info(p)
            if p == F.DINOSAURIA:
                dino = True
            if p == F.AVES:
                aves = True
            rank = (n.get("P105") or [None])[0]
            sl = int((n.get("S") or [0])[0])
            lb = label_of(n)
            if lb and len(out) < F.TAXON_CHAIN_MAX and (rank in F.KEY_RANKS or sl >= F.CLADE_MIN_SITELINKS):
                if lb not in out:
                    out.append(lb)
            cur = n.get("P171", [])
        return out, dino and not aves

    def num(xs):
        try:
            v = [float(x) for x in xs]
            v = [x for x in v if x > 0]
            return round(statistics.median(v), 4) if v else None
        except ValueError:
            return None

    records = []
    dropped = Counter()
    for i, d in det.items():
        fam = fam_of.get(i)
        if not fam:
            continue
        kind = kind_of[i]
        label = label_of(d)
        if not label or re.fullmatch(r"Q\d+", label):
            dropped["no_label"] += 1
            continue
        sl = int((d.get("S") or [0])[0])
        aliases = []
        seen = {label.casefold()}
        extra = (d.get("CN", []) + d.get("TN", [])) if kind == "taxon" or d.get("P171") else []
        for a in d.get("A", []) + extra + d.get("Lmul", []) + d.get("Len", []):
            a = a.strip()
            if a and len(a) <= 60 and a.casefold() not in seen:
                seen.add(a.casefold())
                aliases.append(a)
        p31 = set(d.get("P31", []))
        if (fam in F.PRUNED_FAMILIES and kind == "instance" and p31 & excl
                and not (p31 & built)):
            dropped["excluded_class"] += 1
            continue
        if fam == "fiction_other" and p31 & character_classes:
            fam = "fiction_character"
        for p in p31:
            suf = BREED_SUFFIX.get(lab(p) or "")
            if suf and suf not in label.lower() and f"{label} {suf}".casefold() not in seen:
                seen.add(f"{label} {suf}".casefold())
                aliases.append(f"{label} {suf}")
        is_fict = bool(p31 & fictional_classes) or fam in (
            "fiction_character", "fiction_other", "myth_creature", "myth_other", "kaiju", "pokemon", "franchise")
        if kind == "type":
            is_fict = is_fict or bool(set(d.get("P279", [])) & fictional_classes)
        if kind == "taxon" or (d.get("P171") and p31 & {"Q16521", "Q23038290"}):
            classes, dino = taxon_chain(i, d)
            classes = ["taxon"] + classes
            if dino:
                fam = "dinosaur"
            kind = "taxon"
        else:
            classes = class_chain(i, d, kind)
        rec = {
            "qid": i,
            "label": label,
            "aliases": aliases,
            "sitelinks": sl,
            "description": (d.get("D") or [""])[0],
            "classes": classes,
            "is_fictional": is_fict,
        }
        for prop in ("P8345", "P1080", "P1441"):
            vs = [lab(x) for x in d.get(prop, [])[:3]]
            vs = [v for v in vs if v]
            if vs:
                rec["franchise"] = vs[0]
                break
        h = num(d.get("P2048", []))
        if h:
            rec["height_m"] = h
        m = num(d.get("P2067", []))
        if m:
            rec["mass_kg"] = m
        cols = [lab(c) for c in d.get("P462", [])]
        cols = [c for c in cols if c]
        if cols:
            rec["color"] = cols
        # common-word collision
        low = label.lower()
        coll = False
        if low in wn:
            if kind == "type":
                coll = False
            elif fam in allowed_lex or kind == "taxon":
                coll = not (wn[low] & allowed_lex.get(fam, allowed_lex["taxon"]))
            else:
                coll = True
        if coll:
            if sl < (2 if is_fict else 5):
                dropped["collision_unpopular"] += 1
                continue
            rec["common_word_collision"] = True
        rec["family"] = fam
        rec["kind"] = kind
        if kind == "taxon":
            rk = lab((d.get("P105") or [None])[0]) if d.get("P105") else None
            if rk:
                rec["taxon_rank"] = rk
        records.append(rec)

    # A franchise named like one of its constructs ("Shrek", "Gundam", "Godzilla") must not shadow the
    # character/mecha itself: keep franchise records only for names nothing else answers to.
    def nkey(s):
        return re.sub(r"[^\w]+", " ", s.casefold()).strip()
    taken = set()
    for r in records:
        if r["family"] != "franchise":
            taken.add(nkey(r["label"]))
            taken.update(nkey(a) for a in r["aliases"])
    before = len(records)
    records = [r for r in records if r["family"] != "franchise" or nkey(r["label"]) not in taken]
    dropped["franchise_shadowing"] = before - len(records)

    records.sort(key=lambda r: (-r["sitelinks"], r["qid"]))
    tmp = OUT.with_suffix(".part")
    with gzip.open(tmp, "wt", encoding="utf-8", compresslevel=9) as f:
        for r in records:
            f.write(json.dumps(r, ensure_ascii=False, separators=(",", ":")) + "\n")
    for attempt in range(20):  # Windows: AV/indexer may briefly hold the fresh file
        try:
            tmp.replace(OUT)
            break
        except PermissionError:
            time.sleep(1.5)
    else:
        tmp.replace(OUT)

    fams = Counter(r["family"] for r in records)
    # sample: top 8 per family + some from the long tail
    sample = []
    by_fam = defaultdict(list)
    for r in records:
        by_fam[r["family"]].append(r)
    for fam, rs in sorted(by_fam.items()):
        sample += rs[:8]
        sample += rs[len(rs) // 2: len(rs) // 2 + 2]
        sample += rs[-2:]
    SAMPLE.write_text(json.dumps(sample, ensure_ascii=False, indent=1), encoding="utf-8")

    log(f"\nwrote {len(records)} entities -> {OUT} ({OUT.stat().st_size / 1e6:.1f} MB)")
    log(f"dropped: {dict(dropped)}")
    for fam, n in fams.most_common():
        log(f"  {fam:20s} {n}")
    log(f"fictional: {sum(r['is_fictional'] for r in records)}; "
        f"collision-flagged: {sum(1 for r in records if r.get('common_word_collision'))}")
    log(f"wdqs: {wdqs.stats()}; total {time.time() - t0:.0f}s")


if __name__ == "__main__":
    main()
