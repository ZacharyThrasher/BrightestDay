"""Brightest Day lexicon build: external vocabulary knowledge -> assets/lexicon.bin.

    python tools/lexicon_build/build.py            # fetch (cached) + process + pack (one command)
    python tools/lexicon_build/build.py --no-pack  # stop after cache/lexicon.json + preview

Pipeline (all classic graph/lexical processing, no ML):
 1. sources.py downloads pinned ConceptNet 5.7 / WordNet 3.0 (NLTK) / dariusk corpora / PokeAPI CSVs and
    filters ConceptNet to English edges of useful relations (cached).
 2. WordNet: hand anchors on synsets (anchors.py) are propagated down hypernym trees with decaying
    confidence: form (nearest anchor wins), mechanics/features (inherited unions), element, size hints.
    Every noun lemma picks its best sense (sense order + tag counts x resolvability).
 3. ConceptNet: UsedFor/CapableOf/HasA/PartOf/HasProperty/... edges into anchor concept phrases add
    mechanics/elements/features; IsA/Synonym/DerivedFrom/SimilarTo (strong) and RelatedTo (weak, voted)
    resolve words WordNet doesn't know.
 4. Curated lists (corpora), Pokemon (PokeAPI), pop-culture aliases, optional Wikidata entities.
 5. Modifier tables (adjectives, verbs + inflections, collectives), expanded through WordNet.
 6. cache/lexicon.json + assets/lexicon_preview.json; then `cargo run --example pack` -> assets/lexicon.bin.
"""

from __future__ import annotations

import argparse
import collections
import gzip
import json
import math
import os
import re
import subprocess
import sys
import time
import unicodedata
from pathlib import Path

import sources
import anchors as A
import everyday

# Everyday concepts (compounds, idioms, foods, rides, toys, brands) are hand anchors too; they win over
# the general table for the few keys both define.
A.WORD_ANCHORS.update(everyday.EVERYDAY)

HERE = sources.HERE
CACHE = sources.CACHE
GAME = HERE.parent.parent
ASSETS = GAME / "assets"
ENTITIES_DEFAULT = HERE / "data" / "entities.jsonl.gz"

T0 = time.time()


def log(*a):
    print(f"[{time.time() - T0:6.1f}s]", *a, file=sys.stderr, flush=True)


# ------------------------------------------------------------------------------------------------
# normalisation (MUST match bd_lexicon::normalize_key)
# ------------------------------------------------------------------------------------------------
_POSS = re.compile(r"(?<=[a-z0-9])['’]s(?![a-z0-9])")
_NONALNUM = re.compile(r"[^a-z0-9]+")


def norm(s: str) -> str:
    s = s.lower().replace("♀", " f ").replace("♂", " m ")
    s = unicodedata.normalize("NFKD", s)
    s = "".join(c for c in s if not unicodedata.combining(c))
    s = s.replace("ß", "ss").replace("æ", "ae").replace("œ", "oe").replace("ø", "o")
    s = _POSS.sub("", s)
    s = s.replace("'", "").replace("’", "")
    s = _NONALNUM.sub(" ", s).strip()
    return s


# ------------------------------------------------------------------------------------------------
# frequency
# ------------------------------------------------------------------------------------------------
import wordfreq  # noqa: E402

_zipf_cache: dict[str, float] = {}


def zipf(k: str) -> float:
    v = _zipf_cache.get(k)
    if v is None:
        try:
            v = wordfreq.zipf_frequency(k, "en")
        except Exception:
            v = 0.0
        _zipf_cache[k] = v
    return v


# ------------------------------------------------------------------------------------------------
# entry model
# ------------------------------------------------------------------------------------------------
F_PART, F_POKEMON, F_ENTITY, F_POP, F_ANCHOR, F_PURE, F_CN, F_PERSON = 1, 2, 4, 8, 16, 32, 64, 128
F_SPIRIT = 256
F_MOVE = 512      # Pokemon move (tools/lexicon_build/moves.py): a bare cast resolves as a graft
F_COMPOUND = 1024  # established multi-word unit ("hot dog"): the parser never splits it into modifier + noun
F_COMMON = 2048    # everyday single word (wordfreq zipf >= 3.3): curated icon aliases may not steal it
COMMON_ZIPF = 3.3
CN_COMPOUND_DEGREE = 6


class Entry:
    __slots__ = ("key", "display", "form", "fconf", "mech", "elem", "feat", "scale", "mass", "count",
                 "accent", "graft", "gelem", "flags", "why", "rank", "size_src", "lexname", "sense", "temper")

    def __init__(self, key, display=None):
        self.key = key
        self.display = display or key
        self.form = None
        self.fconf = 0.0
        self.mech: dict[str, float] = {}
        self.elem: dict[str, float] = {}
        self.feat: dict[str, float] = {}
        self.scale = None
        self.mass = None
        self.count = 1
        self.accent = None
        self.graft = None
        self.gelem = None
        self.flags = 0
        self.why: list[str] = []
        self.rank = 0.0
        self.size_src = None
        self.lexname = None
        self.sense = None
        self.temper = None

    def add_mech(self, m, c):
        if c > self.mech.get(m, 0.0):
            self.mech[m] = c

    def add_elem(self, e, c):
        # noisy-or accumulation of element evidence
        self.elem[e] = 1.0 - (1.0 - self.elem.get(e, 0.0)) * (1.0 - min(c, 1.0))

    def add_feat(self, f, c):
        if c > self.feat.get(f, 0.0):
            self.feat[f] = c


def noisy_or(a, b):
    return 1.0 - (1.0 - a) * (1.0 - b)


# ------------------------------------------------------------------------------------------------
# WordNet
# ------------------------------------------------------------------------------------------------
def load_wordnet():
    import nltk

    nltk.data.path.insert(0, str(CACHE / "nltk_data"))
    from nltk.corpus import wordnet as wn

    wn.ensure_loaded()
    return wn


PHYSICAL_LEX = {"noun.artifact", "noun.animal", "noun.plant", "noun.food", "noun.object", "noun.substance",
                "noun.body", "noun.person", "noun.phenomenon", "noun.shape", "noun.Tops", "noun.event",
                "noun.process", "noun.state", "noun.location"}

FORM_DECAY = 0.95
SLUR_DOMAINS = {"ethnic_slur.n.01"}
VULGAR_DOMAINS = {"obscenity.n.01", "vulgarism.n.01"}
VULGAR_VERB_LEMMAS = {"fuck", "screw", "hump", "get_laid", "have_sex", "make_love", "sleep_together", "bed",
                      "eff", "jazz", "have_intercourse", "sleep_with", "bang", "shag"}


class WNProp:
    """Synset-level propagation of anchors through the hypernym DAG."""

    def __init__(self, wn):
        self.wn = wn
        self.form_anchor = dict(A.FORM_SYNSETS_DEFAULT)
        self.form_anchor.update(A.FORM_SYNSETS)
        self._form = {}
        self._mech = {}
        self._elem = {}
        self._size = {}
        self._feat = {}

    def parents(self, s):
        return s.hypernyms() + s.instance_hypernyms()

    def form(self, s):
        """-> (form, conf, anchor_name, parent_name|None) — nearest anchor wins."""
        n = s.name()
        if n in self._form:
            return self._form[n]
        self._form[n] = None  # cycle guard
        res = None
        if n in self.form_anchor:
            f, c = self.form_anchor[n]
            res = (f, c, n, None)
        else:
            for h in self.parents(s):
                r = self.form(h)
                if r and (res is None or r[1] * FORM_DECAY > res[1]):
                    res = (r[0], r[1] * FORM_DECAY, r[2], h.name())
        self._form[n] = res
        return res

    def chain(self, s):
        out = [s.name()]
        n = s.name()
        for _ in range(12):
            r = self._form.get(n)
            if not r or not r[3]:
                break
            n = r[3]
            out.append(n)
        return out

    def mech(self, s):
        n = s.name()
        if n in self._mech:
            return self._mech[n]
        self._mech[n] = {}
        acc: dict[str, float] = {}
        for h in self.parents(s):
            for m, c in self.mech(h).items():
                acc[m] = max(acc.get(m, 0.0), c * 0.95)
        for m in A.MECH_SYNSETS.get(n, []):
            if m.startswith("-"):
                acc.pop(m[1:], None)
            else:
                acc[m] = 1.0
        self._mech[n] = acc
        return acc

    def feat(self, s):
        n = s.name()
        if n in self._feat:
            return self._feat[n]
        self._feat[n] = {}
        acc: dict[str, float] = {}
        for h in self.parents(s):
            for m, c in self.feat(h).items():
                acc[m] = max(acc.get(m, 0.0), c * 0.95)
        for m in A.FEATURE_SYNSETS.get(n, []):
            if m.startswith("-"):
                acc.pop(m[1:], None)
            else:
                acc[m] = 1.0
        self._feat[n] = acc
        return acc

    def elem(self, s):
        n = s.name()
        if n in self._elem:
            return self._elem[n]
        self._elem[n] = None
        res = None
        if n in A.ELEM_SYNSETS:
            res = (A.ELEM_SYNSETS[n], 1.0)
        else:
            for h in self.parents(s):
                r = self.elem(h)
                if r and (res is None or r[1] * 0.9 > res[1]):
                    res = (r[0], r[1] * 0.9)
        self._elem[n] = res
        return res

    def size(self, s):
        n = s.name()
        if n in self._size:
            return self._size[n]
        self._size[n] = None
        res = A.SIZE_SYNSETS.get(n)
        if res is None:
            for h in self.parents(s):
                r = self.size(h)
                if r:
                    res = r
                    break
        self._size[n] = res
        return res


GLOSS_RX = [(re.compile(p), tgt, w, cat) for p, tgt, w, cat in A.GLOSS_PATTERNS]


def lemma_display(synname: str) -> str:
    return synname.split(".")[0].replace("_", " ")


def wordnet_entries(wn, prop: WNProp):
    """One Entry per noun lemma key, choosing the best sense."""
    by_key: dict[str, list] = collections.defaultdict(list)
    for syn in wn.all_synsets("n"):
        if syn.instance_hypernyms():
            continue  # proper nouns (cities, people); pop culture comes from curated data instead
        for lem in syn.lemmas():
            name = lem.name()
            if name[:1].isupper() or any(ch.isdigit() for ch in name):
                continue
            key = norm(name.replace("_", " "))
            if not key or len(key) < 2 or key.count(" ") > 3:
                continue
            by_key[key].append((syn, lem.count(), name))
    log(f"wordnet: {len(by_key)} noun lemma keys")
    out = {}
    for key, senses in by_key.items():
        name = senses[0][2]
        order = {s.name(): i for i, s in enumerate(wn.synsets(name, "n"))}
        tot = sum(c for _, c, _ in senses) + 0.5 * len(senses)
        has_concrete = any(sy.lexname() in ("noun.artifact", "noun.animal", "noun.food", "noun.plant", "noun.object")
                           for sy, _, _ in senses)
        best = None
        for syn, cnt, _ in senses:
            lex = syn.lexname()
            dom = {u.name() for u in syn.usage_domains()}
            if dom & SLUR_DOMAINS:
                continue
            r = prop.form(syn)
            fb = A.LEXNAME_FALLBACK.get(lex)
            if r is None and fb is None:
                continue
            if r is None or (fb and fb[1] > r[1]):
                form, conf, anchor, via = fb[0], fb[1], None, None
            else:
                form, conf, anchor, via = r
            if lex not in PHYSICAL_LEX and conf < 0.5:
                continue
            i = order.get(syn.name(), len(order))
            share = (cnt + 0.5) / tot
            score = conf * (0.55 + 0.45 * share) * (0.78 ** i)
            if lex == "noun.person" and has_concrete:
                score *= 0.55  # a toaster is a kitchen appliance before it is a person proposing a toast
            if dom & VULGAR_DOMAINS:
                score *= 0.3  # prefer the literal physical sense
            if best is None or score > best[0]:
                best = (score, syn, form, conf, anchor)
        if best is None:
            continue
        score, syn, form, conf, anchor = best
        e = Entry(key)
        e.form, e.fconf, e.lexname, e.sense = form, conf, syn.lexname(), syn.name()
        if syn.lexname() == "noun.person":
            e.flags |= F_PERSON
        # explanation
        if anchor is None:
            e.why.append(f"{key}: {syn.lexname()[5:]} ({lemma_display(syn.name())}) -> {form} (WordNet class default)")
        elif anchor == syn.name():
            head = lemma_display(anchor)
            if head != key:
                e.why.append(f"{key} ≈ {head} (WordNet synonym)")
            else:
                e.why.append(f"{key} (anchor -> {form})")
        else:
            ch = [lemma_display(x) for x in prop.chain(syn)]
            ch = [c for i, c in enumerate(ch) if i == 0 or c != ch[i - 1]]
            if ch and ch[0] != key:
                ch.insert(0, key)
            if len(ch) > 4:
                ch = ch[:2] + ["…"] + ch[-1:]
            e.why.append(" → ".join(ch) + " (WordNet hypernym)")
        for m, c in prop.mech(syn).items():
            e.add_mech(m, c * 0.95)
        el = prop.elem(syn)
        if el:
            e.add_elem(el[0], el[1] * 0.9)
        for f, c in prop.feat(syn).items():
            e.add_feat(f, c)
        sz = prop.size(syn)
        if sz:
            e.scale, e.mass, e.size_src = sz[0], sz[1], "wordnet"
        gloss = syn.definition().lower()
        cat = A.CATEGORY[form]
        for rx, tgt, w, gcat in GLOSS_RX:
            if gcat and gcat != cat:
                continue
            if rx.search(gloss):
                if tgt.startswith("E:"):
                    e.add_elem(tgt[2:], w)
                elif tgt.startswith("-"):
                    e.mech.pop(tgt[1:], None)
                else:
                    e.add_mech(tgt, w)
        # WordNet part meronyms -> features/mechanics
        for mer in syn.part_meronyms():
            for ln in mer.lemma_names():
                pk = norm(ln.replace("_", " "))
                bio = pk in BIO_PARTS
                if pk in A.PART_FEATURE and (cat == "Creature" or not bio):
                    e.add_feat(A.PART_FEATURE[pk], 0.8)
                if pk in A.PART_MECH and (bio == (cat == "Creature")):
                    for m in A.PART_MECH[pk]:
                        e.add_mech(m, 0.6)
        out[key] = e
    return out


# ------------------------------------------------------------------------------------------------
# ConceptNet
# ------------------------------------------------------------------------------------------------
STRONG_FORM_REL = {"IsA": 0.85, "Synonym": 0.9, "FormOf": 0.95, "SimilarTo": 0.75, "DerivedFrom": 0.6}
MECH_REL = {"UsedFor": 0.75, "CapableOf": 0.8, "HasProperty": 0.7}
# Verbs whose appearance as the first word of a UsedFor/CapableOf phrase is reliable evidence.
VERB_FIRST_OK = {"cut", "chop", "slice", "stab", "pierce", "shoot", "explode", "blast", "smash", "pound",
                 "crush", "bash", "fly", "glide", "float", "bounce", "spin", "drill", "grab", "grasp", "grip",
                 "pull", "tow", "trap", "protect", "block", "launch", "stick", "glue", "sting", "constrict",
                 "squeeze", "strangle", "charge", "ram", "stomp", "trample", "breathe", "spray", "squirt"}
BIO_PARTS = {"tail", "long tail", "stinger", "stinging tail", "spiked tail", "claw", "claws", "pincer", "pincers",
             "talon", "talons", "tentacle", "tentacles", "wing", "wings", "horn", "horns", "tusk", "tusks",
             "antler", "antlers", "trunk", "shell", "carapace", "spine", "spines", "quill", "quills", "thorn",
             "thorns", "fang", "fangs", "venom", "web", "silk", "hoof", "hooves", "sharp teeth", "fin", "fins",
             "dorsal fin", "leg", "legs", "armor", "armour"}
_ART = re.compile(r"^(to |be |a |an |the )+")


def concept_key(s: str) -> str:
    s = _ART.sub("", s)
    s = re.sub(r"\b(a|an|the|your|their|its|his|her|someone|something|things?)\b", " ", s)
    return " ".join(s.split())


def load_conceptnet(path: Path):
    t = time.time()
    out = collections.defaultdict(list)
    partof_in = collections.defaultdict(list)
    intern = sys.intern
    with gzip.open(path, "rt", encoding="utf-8") as f:
        for line in f:
            rel, a, ap, b, bp, w = line.rstrip("\n").split("\t")
            if ap and ap not in ("n", "a", "v", "s", "r"):
                ap = ap[:1]
            a = intern(a)
            b = intern(b)
            w = float(w)
            if rel == "PartOf":
                partof_in[b].append((a, w))
            out[a].append((intern(rel), b, w, ap))
            if rel in ("Synonym", "SimilarTo", "RelatedTo"):
                out[b].append((intern(rel), a, w, bp))
    log(f"conceptnet loaded: {len(out)} terms in {time.time() - t:.0f}s")
    return out, partof_in


def build_concept_index():
    mech_idx = collections.defaultdict(set)
    for m, phrases in A.MECH_CONCEPTS.items():
        for p in phrases:
            mech_idx[concept_key(p)].add(m)
    elem_idx = collections.defaultdict(set)
    for e, phrases in A.ELEM_CONCEPTS.items():
        for p in phrases:
            elem_idx[concept_key(p)].add(e)
    verb_first = {}
    for v, ms in A.VERBS.items():
        if " " not in v:
            verb_first[v] = ms
    return mech_idx, elem_idx, verb_first


def conceptnet_evidence(e: Entry, cn_out, cn_partof, mech_idx, elem_idx, verb_first):
    edges = cn_out.get(e.key)
    cat = A.CATEGORY.get(e.form, "Object")
    mech_ev = collections.defaultdict(float)
    mech_why = {}
    elem_ev = collections.defaultdict(float)
    elem_why = {}
    big = small = 0.0
    creature = cat == "Creature"
    if edges:
        for rel, other, w, pos in edges:
            if pos in ("v", "a", "r", "s"):
                continue  # verb/adjective senses of the word say nothing about the thing
            wf = min(1.0, 0.55 + 0.25 * w)
            ck = concept_key(other)
            if rel in ("HasA", "RelatedTo", "MadeOf"):
                pm = A.PART_MECH.get(ck)
                bio = ck in BIO_PARTS
                if pm and ((bio and creature) or (not bio and rel == "HasA" and not creature)):
                    for m in pm:
                        c = (0.7 if rel == "HasA" else 0.25) * wf
                        mech_ev[m] = noisy_or(mech_ev[m], c)
                        mech_why.setdefault(m, f"{e.key} {rel} {other} → {m} (ConceptNet)")
                pf = A.PART_FEATURE.get(ck)
                if pf and rel == "HasA" and (creature or not bio):
                    e.add_feat(pf, 0.7)
            rw = MECH_REL.get(rel, 0.0)
            ms = mech_idx.get(ck) if rw else None
            if ms:
                for m in ms:
                    c = rw * wf
                    if c > 0.0:
                        mech_ev[m] = noisy_or(mech_ev[m], c)
                        mech_why.setdefault(m, f"{e.key} {rel} '{other}' → {m} (ConceptNet)")
            elif rel in ("UsedFor", "CapableOf") and ck:
                first = ck.split(" ")[0]
                for m in (verb_first.get(first, []) if first in VERB_FIRST_OK else []):
                    mech_ev[m] = noisy_or(mech_ev[m], rw * wf * 0.45)
                    mech_why.setdefault(m, f"{e.key} {rel} '{other}' → {m} (ConceptNet)")
            ev = elem_idx.get(ck)
            if ev:
                ew = {"RelatedTo": 0.15, "HasProperty": 0.6, "MadeOf": 0.7, "IsA": 0.3, "AtLocation": 0.25,
                      "UsedFor": 0.45, "CapableOf": 0.55, "HasA": 0.4, "Synonym": 0.5, "SimilarTo": 0.45,
                      "DerivedFrom": 0.4}.get(rel, 0.2)
                for el in ev:
                    elem_ev[el] = noisy_or(elem_ev[el], ew * wf)
                    elem_why.setdefault(el, f"{e.key} {rel} {other} → {el} (ConceptNet)")
            if rel in ("HasProperty", "RelatedTo", "IsA"):
                if ck in ("big", "large", "huge", "giant", "enormous", "massive", "tall", "gigantic"):
                    big += 0.7 if rel != "RelatedTo" else 0.3
                elif ck in ("small", "tiny", "little", "short", "miniature"):
                    small += 0.7 if rel != "RelatedTo" else 0.3
    for part, w in cn_partof.get(e.key, ()):  # "<part> PartOf <e.key>"
        pk = concept_key(part)
        pm = A.PART_MECH.get(pk)
        bio = pk in BIO_PARTS
        if pm and (bio == creature):
            for m in pm:
                mech_ev[m] = noisy_or(mech_ev[m], 0.6)
                mech_why.setdefault(m, f"{part} PartOf {e.key} → {m} (ConceptNet)")
        pf = A.PART_FEATURE.get(pk)
        if pf and (creature or not bio):
            e.add_feat(pf, 0.7)
    for m, c in mech_ev.items():
        prev = e.mech.get(m, 0.0)
        e.mech[m] = noisy_or(prev, c)
        if c >= 0.5 and m in mech_why and prev < 0.5:
            e.why.append(mech_why[m])
    for el, c in elem_ev.items():
        e.add_elem(el, c)
        if c >= 0.6 and el in elem_why:
            e.why.append(elem_why[el])
    return big - small


GENERIC_TARGETS = {"person", "human", "man", "woman", "people", "individual", "someone", "thing", "object",
                   "agent", "worker", "organism", "being", "entity", "artifact", "item", "unit", "male", "female"}


def conceptnet_new_words(entries, cn_out, cn_partof, idx, wn):
    """Resolve ConceptNet terms WordNet doesn't know at all, via strong relations to physical WordNet words
    (and heavily-voted RelatedTo for common words). No chaining through other ConceptNet guesses."""
    new = {}
    for rnd in range(1):
        known = entries
        added = 0
        for term, edges in cn_out.items():
            if term in known or term in new:
                continue
            if len(term) < 3 or len(term) > 40 or term.count(" ") > 2:
                continue
            if not re.fullmatch(r"[a-z0-9 ]+", term):
                continue
            z = zipf(term)
            multi = " " in term
            if (not multi and z < 2.5) or (multi and z < 2.3):
                continue
            if wn.synsets(term.replace(" ", "_")):
                continue  # WordNet knows it and found no physical reading (adjective, verb, abstract noun)
            if multi:
                head = entries.get(term.split(" ")[-1])
                if head is None or head.form is None:
                    continue
            if any(rel == "FormOf" for rel, *_ in edges):
                continue  # inflections: Rust morphology handles these
            ev = collections.defaultdict(float)
            why = {}
            rt_votes = collections.defaultdict(list)
            for rel, other, w, pos in edges:
                if pos and pos != "n":
                    continue
                t = known.get(other)
                if t is None or t.form is None or other in GENERIC_TARGETS or t.flags & F_CN:
                    continue
                if t.fconf < 0.45:
                    continue
                if rel in STRONG_FORM_REL:
                    c = STRONG_FORM_REL[rel] * t.fconf * min(1.0, 0.6 + 0.2 * w)
                    if c > ev[t.form]:
                        ev[t.form] = c
                        why[t.form] = (f"{term} ≈ {other} (ConceptNet {rel})", other)
                elif rel == "RelatedTo":
                    rt_votes[t.form].append((0.3 * t.fconf, other))
            for f, votes in rt_votes.items():
                if len(votes) >= 3 and z >= 3.0:
                    c = 0.0
                    for v, _ in votes:
                        c = noisy_or(c, v)
                    c = min(c, 0.5)
                    if c > ev[f]:
                        ev[f] = c
                        why[f] = (f"{term} ~ {', '.join(o for _, o in votes[:3])} (ConceptNet RelatedTo)", votes[0][1])
            if not ev:
                continue
            f, c = max(ev.items(), key=lambda kv: (kv[1], kv[0]))
            if c < 0.3:
                continue
            e = Entry(term)
            e.form, e.fconf = f, c
            e.flags |= F_CN
            e.why.append(why[f][0])
            src = known.get(why[f][1])
            if src is not None:
                for m, mc in src.mech.items():
                    e.add_mech(m, mc * 0.8)
                for el, ec in src.elem.items():
                    e.add_elem(el, ec * 0.7)
                for ft, fc in src.feat.items():
                    e.add_feat(ft, fc * 0.8)
                if src.scale:
                    e.scale, e.mass, e.size_src = src.scale, src.mass, "cn"
            new[term] = e
            added += 1
        log(f"conceptnet round {rnd}: +{added} words")
    return new


# ------------------------------------------------------------------------------------------------
# curated sources
# ------------------------------------------------------------------------------------------------
def entry_from(base: Entry, key: str, display: str | None = None) -> Entry:
    e = Entry(key, display)
    e.form, e.fconf = base.form, base.fconf
    e.mech = dict(base.mech)
    e.elem = dict(base.elem)
    e.feat = dict(base.feat)
    e.scale, e.mass, e.count, e.accent = base.scale, base.mass, base.count, base.accent
    e.graft = list(base.graft) if base.graft else None
    e.gelem = base.gelem
    e.size_src = base.size_src
    e.temper = base.temper
    return e


def apply_word_anchor(e: Entry, spec: dict):
    creature_size = (e.scale, e.mass) if (e.size_src and A.CATEGORY.get(spec["f"]) == "Creature") else (None, None)
    e.form = spec["f"]
    e.fconf = 1.0
    e.mech = {}
    e.feat = {k: v for k, v in e.feat.items() if v >= 0.9}
    for m in spec.get("m", []):
        e.mech[m] = 1.0  # "-X" entries are kept as negative markers and handled in finalize()
    e.elem = {spec["e"]: 1.0} if spec.get("e") else {}
    e.scale, e.mass = creature_size
    e.size_src = "wordnet" if creature_size[0] else None
    if spec.get("s"):
        e.scale = spec["s"]
        e.size_src = "anchor"
        if not spec.get("w"):
            d = A.FORM_DEFAULTS[e.form]
            e.mass = d[1] * (spec["s"] / d[0]) ** 3
    if spec.get("w"):
        e.mass = spec["w"]
    if spec.get("count"):
        e.count = spec["count"]
    if spec.get("t"):
        e.temper = spec["t"]
    if spec.get("ft"):
        e.feat = {f: 1.0 for f in spec["ft"]}
    if "g" in spec:
        e.graft = list(spec["g"])
    if spec.get("part"):
        e.flags |= F_PART
    e.flags |= F_ANCHOR


def load_corpora(entries):
    z = sources.corpora_zip()
    cj = lambda p: sources.corpora_json(z, p)  # noqa: E731
    added = 0

    def add(name, default_form, mechs=(), elem=None, flag=0, why="", base_ok=True, conf=0.75, fill_only=False):
        nonlocal added
        k = norm(name)
        if not k or len(k) < 3 or k.count(" ") > 3:
            return
        e = entries.get(k)
        if e is not None and fill_only and e.form is not None:
            return
        if e is not None and base_ok and e.fconf >= 0.5:
            return
        if e is None:
            e = Entry(k, name if name[:1].isupper() else None)
            entries[k] = e
            added += 1
        if e.form is None or e.fconf < conf:
            e.form, e.fconf = default_form, conf
        for m in mechs:
            e.add_mech(m, 0.9)
        if elem:
            e.add_elem(elem, 0.9)
        e.flags |= flag
        if why:
            e.why.insert(0, f"{k}: {why} (dariusk/corpora)")

    for n in cj("animals/common.json")["animals"]:
        add(n, "Quadruped", why="common animal", fill_only=True)
    for n in cj("animals/dinosaurs.json")["dinosaurs"]:
        add(n, "Kaiju", ["ArticulatedTail", "Stomp"], why="dinosaur", flag=F_POP, conf=0.8)
    for n in cj("mythology/monsters.json")["names"]:
        add(n, "Biped", why="mythic monster", flag=F_POP)
    for n in cj("mythology/greek_monsters.json")["greek_monsters"]:
        add(n, "Kaiju", why="Greek monster", flag=F_POP)
    lc = cj("mythology/lovecraft.json")
    for n in lc["deities"]:
        add(n, "Kaiju", ["Constrict", "Grab"], "Psychic", why="Lovecraftian deity", flag=F_POP, conf=0.85)
    for n in lc["supernatural_creatures"]:
        add(n, "Blob", ["Grab"], "Shadow", why="Lovecraftian creature", flag=F_POP, conf=0.8)
    pw = cj("objects/premodern_weapons.json")["data"]
    for n in pw.get("melee", []):
        add(n, "Sword", why="premodern melee weapon", fill_only=True)
    for n in pw.get("ranged", []):
        add(n, "Bow", ["Shoots"], why="premodern ranged weapon", fill_only=True)
    for n in cj("objects/objects.json")["objects"]:
        add(n, "Ball", why="throwable object", conf=0.5, fill_only=True)
    for n in cj("music/instruments.json")["instruments"]:
        add(n, "Hammer", ["Blunt"], why="musical instrument (swing it)", conf=0.55, fill_only=True)
    for n in cj("technology/appliances.json")["appliances"]:
        add(n, "Anvil", ["DropFromSky", "Heavy"], why="appliance (drop it)", conf=0.55, fill_only=True)
    for n in cj("objects/clothing.json")["clothes"]:
        add(n, "Cape", why="clothing", conf=0.5, fill_only=True)
    gods = {
        "zeus": ("Electric", ["Shoots", "Flies"]), "poseidon": ("Water", ["Pierce"]), "hades": ("Shadow", []),
        "ares": ("Fire", ["Blade", "Charge"]), "hephaestus": ("Fire", ["Blunt"]), "apollo": ("Fire", ["Shoots"]),
        "artemis": ("Light", ["Shoots", "Homing"]), "athena": ("Psychic", ["Guard"]), "hermes": ("Wind", ["Fast"]),
        "thor": ("Electric", ["Blunt", "Returning"]), "odin": ("Psychic", ["Pierce"]), "loki": ("Shadow", ["Swarm"]),
        "freyr": ("Earth", ["Blade"]), "heimdallr": ("Light", ["Guard"]), "atlas": ("Earth", ["Heavy", "Stomp"]),
        "helios": ("Fire", ["Beam"]), "cronus": ("Shadow", ["Blade"]), "hyperion": ("Fire", ["Beam"]),
    }
    greek = cj("mythology/greek_gods.json")
    names = []
    for v in greek.values():
        if isinstance(v, list):
            names += [x for x in v if isinstance(x, str)]
    ng = cj("mythology/norse_gods.json")["norse_deities"]
    for v in ng.values():
        names += v
    names += cj("mythology/greek_titans.json")["greek_titans"]
    for n in names:
        k = norm(n)
        el, ms = gods.get(k, (None, []))
        add(n, "Biped", ["Flies"] + ms, el, why="deity", flag=F_POP, conf=0.7)
    log(f"corpora: +{added} entries")


def load_pokemon(entries):
    import csv

    d = CACHE / "pokeapi"
    rd = lambda n: list(csv.DictReader(open(d / f"{n}.csv", encoding="utf-8")))  # noqa: E731
    types = {r["id"]: r["identifier"] for r in rd("types")}
    shapes = {r["id"]: r["identifier"] for r in rd("pokemon_shapes")}
    colors = {r["id"]: r["identifier"] for r in rd("pokemon_colors")}
    names = {r["pokemon_species_id"]: r["name"] for r in rd("pokemon_species_names") if r["local_language_id"] == "9"}
    ptypes = collections.defaultdict(list)
    for r in rd("pokemon_types"):
        ptypes[r["pokemon_id"]].append((int(r["slot"]), types[r["type_id"]]))
    default_poke = {}
    for r in rd("pokemon"):
        if r["is_default"] == "1":
            default_poke[r["species_id"]] = r
    n = 0
    for sp in rd("pokemon_species"):
        sid = sp["id"]
        name = names.get(sid) or sp["identifier"].title()
        p = default_poke.get(sid)
        if p is None:
            continue
        key = norm(name)
        shape = shapes.get(sp["shape_id"], "blob")
        form, smech = A.POKEMON_SHAPE.get(shape, ("Blob", []))
        h = int(p["height"]) / 10.0
        w = int(p["weight"]) / 10.0
        tys = [t for _, t in sorted(ptypes[p["id"]])]
        if form == "Biped" and (h >= 2.5 or w >= 300):
            form = "Kaiju"
        e = Entry(key, name)
        e.form, e.fconf = form, 0.95
        for m in smech:
            e.mech[m] = 1.0
        els = [A.POKEMON_TYPE[t][0] for t in tys]
        el = next((x for x in els if x not in ("Light", "Wind")), None) or next((x for x in els if x != "Light"), "Light")
        e.elem = {el: 1.0}
        for t in tys:
            for m in A.POKEMON_TYPE[t][1]:
                e.mech[m] = 0.95
        if form in ("Kaiju",) or w >= 200:
            e.mech["Heavy"] = 0.9
        if w >= 400:
            e.mech["Stomp"] = 0.9
        e.scale, e.mass, e.size_src = max(h, 0.1), max(w, 0.1), "pokeapi"
        e.accent = A.POKEMON_COLOR.get(colors.get(sp["color_id"], ""))
        e.flags |= F_POKEMON | F_POP
        gen = int(sp["generation_id"] or 9)
        e.rank = 2.5 + (1.0 if gen == 1 else 0.0) + (0.5 if sp["is_legendary"] == "1" or sp["is_mythical"] == "1" else 0.0)
        e.why.append(f"{name}: Pokémon, {shape} body, {'/'.join(tys)}, {h:g} m, {w:g} kg (PokeAPI)")
        old = entries.get(key)
        if old is not None and old.flags & F_ANCHOR:
            continue
        entries[key] = e
        n += 1
    log(f"pokemon: {n} species")


# single-word move names that override an existing entry (they read as the move first)
MOVE_OVERRIDE = {"thunderbolt"}


def load_moves(entries):
    """Pokemon moves (PokeAPI) -> (a) entries for distinctive move names (bare cast = graft), (b) the full move
    table for the runtime grammar "<pokemon> use <move>" (Meta-independent MOVS section). Multi-word move names
    override weak existing readings (pop-culture scanners, WordNet phrases); single words keep their everyday
    reading unless in MOVE_OVERRIDE; anchors / Pokemon / Wikidata entities are never overridden."""
    import moves as MV

    table = MV.build(norm, CACHE / "pokeapi", A.FORM_DEFAULTS)
    added = 0
    for m in table:
        k = m["k"]
        old = entries.get(k)
        multi = " " in k
        if old is not None and k not in MOVE_OVERRIDE:
            if not multi or old.flags & (F_POKEMON | F_ENTITY | F_ANCHOR):
                continue
        if not m["m"] and not m["t"]:
            continue                        # element-only status moves stay grammar-only
        e = Entry(k, m["d"])
        e.form, e.fconf = m["f"], 0.9
        e.mech = {x: 1.0 for x in m["m"]}
        e.elem = {m["e"]: 1.0} if m["e"] else {}
        e.graft = list(m["m"])
        e.temper = m["t"]
        e.scale, e.mass, e.size_src = m["s"], m["w"], "anchor"
        e.flags |= F_MOVE | F_POP | (F_COMPOUND if multi else 0)
        e.rank = 2.4 if m["famous"] else 2.0
        what = "+".join(m["m"]) or (m["t"] or "flop")
        e.why = [f"{k}: Pokémon move ({m['ty']} {m['cls']}) → {what}{' ' + m['e'] if m['e'] else ''} (PokeAPI)"]
        entries[k] = e
        added += 1
    log(f"moves: {len(table)} in the move table, {added} as entries")
    return [dict(k=m["k"], d=m["d"], f=m["f"], m=m["m"], e=m["e"], t=m["t"], s=m["s"], w=m["w"]) for m in table]


GENERIC_CLASS_TOKENS = {"character", "characters", "agent", "mascot", "entity", "person", "human", "humanoid",
                        "people", "protagonist", "hero", "heroine", "villain", "enemy", "being", "figure",
                        "individual", "organism", "object", "thing", "work", "creature", "concept", "item",
                        "member", "group", "role", "persona", "incarnation", "version", "extraterrestrial",
                        "alien", "boss", "npc", "player", "fighter", "deity", "god", "goddess", "spirit"}
CLASS_PREFIX = re.compile(r"^(fictional|legendary|mythical|mythological|giant|anthropomorphic|animated|cartoon|"
                          r"comic|comics|video game|film|literary|anime|manga|television|tv|fantasy|extinct|"
                          r"domestic|wild|species of|breed of|type of|kind of|genus of|class of|model of|family of|"
                          r"imaginary|norse|greek|japanese|cgi|tokusatsu|starter|electric type|fire type|"
                          r"water type|grass type|mario franchise|sonic the hedgehog|disney|marvel|dc|"
                          r"anthropomorphised|anthropomorphized|talking|magical|robotic|mechanical)\s+")
CLASS_SUFFIX = re.compile(r"\s+(breed|type|species|character|in a work of fiction|in fiction|in fiction legend or myth|"
                          r"from disney|model|series|class|variety|cultivar|family)$")


QID_KEY: dict[str, str] = {}
HINT_FILLER = {"hint", "element", "elements", "form", "mechanic", "mechanics", "feature", "features", "and", "with",
               "plus", "or", "a", "an", "the", "of", "temperament", "mood", "e", "g", "eg", "like", "is", "it"}


def load_names_txt(entries, icons_dir: Path):
    """Merge assets/icons/*_names.txt (pop-culture scanners): `name | alias; alias | QID | hint`.
    High priority: overrides Wikidata/ConceptNet guesses, never hand anchors."""
    n = 0
    files = sorted(icons_dir.glob("*_names.txt")) if icons_dir.exists() else []
    for f in files:
        for ln, line in enumerate(open(f, encoding="utf-8"), 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split("|")]
            while len(parts) < 4:
                parts.append("")
            name, aliases, qid, hint = parts[:4]
            key = norm(name)
            if not key or is_profane(key):
                continue
            base = None
            why = f"{key} (curated name, {f.name})"
            if qid and qid in QID_KEY and QID_KEY[qid] in entries:
                base = entries[QID_KEY[qid]]
                why = f"{key} = {QID_KEY[qid]} ({qid}, {f.name})"
            form, mechs, elem, feats, temper = None, [], None, [], None
            hint_clean = re.sub(r"\[[^\]]*\]|\([^)]*\)", " ", hint)
            hint_clean = re.sub(r"^\s*hint\s*:", " ", hint_clean, flags=re.I)
            enum_ci = {x.lower(): x for x in A.FORMS + A.MECHANICS + A.ELEMENTS + A.FEATURES + A.TEMPERAMENTS}
            base_words = []
            for raw in re.split(r"[^A-Za-z0-9_-]+", hint_clean):
                if not raw or raw.lower() in HINT_FILLER:
                    continue
                tok = enum_ci.get(raw.lower(), raw)
                if tok in A.FORMS:
                    form = tok
                elif tok in A.MECHANICS:
                    mechs.append(tok)
                elif tok in A.ELEMENTS:
                    elem = tok
                elif tok in A.FEATURES:
                    feats.append(tok)
                elif tok in A.TEMPERAMENTS:
                    temper = tok
                else:
                    base_words.append(raw)
            if base is None and form is None:
                for w in base_words:
                    b = entries.get(norm(w))
                    if b is not None and b.form and not (b.flags & F_SPIRIT) and norm(w) not in A.STOP_NOUNS:
                        base = b
                        why = f"{key} \u2248 {norm(w)} (curated hint, {f.name})"
                        break
            if base is None and form is None:
                log(f"  names: {f.name}:{ln} '{name}' has no resolvable QID/hint (skipped)")
                continue
            e = entry_from(base, key, name) if base is not None else Entry(key, name)
            if form:
                e.form = form
                if base is None or base.form != form:
                    d = A.FORM_DEFAULTS[form]
                    e.scale = e.scale if base is not None and A.CATEGORY.get(base.form) == A.CATEGORY[form] else None
                    e.mass = e.mass if e.scale else None
            e.fconf = 0.95
            for m in mechs:
                e.mech[m] = 1.0
            if elem:
                e.elem = {elem: 1.0}
            for ft in feats:
                e.feat[ft] = 1.0
            if temper:
                e.temper = temper
            e.flags = (e.flags & ~(F_PART | F_CN | F_PERSON | F_SPIRIT)) | F_POP
            e.rank = max(e.rank, 3.5)
            e.why = [why] + (base.why[:1] if base is not None else [])
            keys = [key] + [norm(a) for a in aliases.split(";") if a.strip()]
            for k in keys:
                if not k:
                    continue
                old = entries.get(k)
                if old is not None and old.flags & (F_ANCHOR | F_POKEMON):
                    continue
                # a curated name/alias can't take over an everyday word ("table", "goose", "shovel")
                if old is not None and " " not in k and zipf(k) >= COMMON_ZIPF and old.fconf >= 0.3                         and not old.flags & F_CN:
                    continue
                ek = e if k == key else entry_from(e, k, a if (a := next((x for x in aliases.split(";") if norm(x) == k), None)) else k)
                if k != key:
                    ek.fconf, ek.flags, ek.rank, ek.why = e.fconf, e.flags, e.rank - 0.2, [f"{k} = {name} (curated alias)"] + e.why
                    ek.temper = e.temper
                entries[k] = ek
                n += 1
    log(f"names.txt: +{n} curated names from {len(files)} files")
    return n


def load_entities(entries, path: Path):
    """Consume tools/lexicon_build/data/entities.jsonl.gz (Wikidata harvest), if present."""
    if not path.exists():
        log(f"entities: {path} not found (skipped)")
        return 0
    opener = gzip.open if path.suffix == ".gz" else open
    n = skipped = unresolved = skipped_rel = overrides = 0
    religion = re.compile(A.RELIGION_RX)
    clause = re.compile(r"\b(from|in|of|by|that|who|which|created|appearing|featured|used|with|for|owned)\b|[,;(]")
    new = {}
    ent_links: dict[str, int] = {}
    memo: dict[str, tuple] = {}

    def usable(t):
        return (t is not None and t.form is not None and not (t.flags & (F_ENTITY | F_SPIRIT))
                and t.key not in A.STOP_NOUNS)

    def resolve_label(lbl):
        """-> (entry, key, conf, generic) best physical reading of a class label / description phrase."""
        k0 = norm(lbl)
        if k0 in memo:
            return memo[k0]
        best = (None, None, 0.0, True)
        cands = []
        k = k0
        for _ in range(4):
            cands.append((k, 1.0))
            k2 = CLASS_SUFFIX.sub("", CLASS_PREFIX.sub("", k))
            if k2 == k or not k2:
                break
            k = k2
        toks = k.split()
        for i in range(len(toks) - 1, -1, -1):
            cands.append((toks[i], 0.9 if i == len(toks) - 1 else 0.6))
            if toks[i].endswith("s") and len(toks[i]) > 3:
                cands.append((toks[i][:-1], 0.85 if i == len(toks) - 1 else 0.55))
        for ck, w in cands:
            t = entries.get(ck)
            if not usable(t):
                continue
            generic = any(tok in GENERIC_CLASS_TOKENS for tok in ck.split()) or bool(t.flags & F_PERSON and ck in ("human", "person", "man", "woman"))
            c = t.fconf * w
            if c > best[2]:
                best = (t, ck, c, generic)
        memo[k0] = best
        return best

    with opener(path, "rt", encoding="utf-8") as f:
        for line in f:
            try:
                ent = json.loads(line)
            except Exception:
                continue
            label = ent.get("label") or ""
            key = norm(label)
            if not key or len(key) > 48 or key.count(" ") > 5:
                continue
            fam = ent.get("family")
            if fam in A.ENTITY_SKIP_FAMILIES:
                continue
            desc = ent.get("description") or ""
            if religion.search((label + " " + desc).lower()):
                skipped_rel += 1
                continue
            if is_profane(key):
                continue
            latin = fam == "taxon" and bool(ent.get("taxon_rank")) and not ent.get("aliases")
            links = int(ent.get("sitelinks") or 0)
            old = entries.get(key) or new.get(key)
            override = False
            if old is not None:
                if old.flags & F_ENTITY:
                    if links <= ent_links.get(key, 0):
                        continue
                else:
                    weak = (old.flags & F_CN) or old.fconf < 0.5 or (old.flags & F_PERSON and not old.flags & F_ANCHOR)
                    # everyday words ("cheese", "bucket", "window") keep their WordNet reading even when a
                    # popular Wikidata item's class chain is odd
                    everyday = " " not in key and zipf(key) >= COMMON_ZIPF
                    if old.flags & (F_ANCHOR | F_POKEMON | F_POP) or not weak or links < 30 or everyday:
                        skipped += 1
                        continue
                    override = True
            # choose the most informative class: specific beats generic ("fictional donkey" > "character")
            best = None
            for i, c in enumerate((ent.get("classes") or [])[:14]):
                t, k, conf, generic = resolve_label(c)
                if t is None:
                    continue
                sc = conf * (0.3 if generic else 1.0) * (0.97 ** i)
                if best is None or sc > best[0]:
                    best = (sc, t, c, generic)
            if desc:
                first = re.split(clause, desc.lower())[0]
                t, k, conf, generic = resolve_label(first)
                if t is not None:
                    sc = conf * (0.3 if generic else 0.85)
                    if best is None or sc > best[0]:
                        best = (sc, t, desc, generic)
            if (best is None or best[0] < 0.25) and fam in A.ENTITY_FAMILY_FORM:
                f0, ms = A.ENTITY_FAMILY_FORM[fam]
                base = Entry(fam)
                base.form, base.fconf = f0, 0.5
                for m in ms:
                    base.mech[m] = 1.0
                if best is None or 0.25 > best[0]:
                    best = (0.25, base, fam.replace("_", " "), False)
            if best is None:
                unresolved += 1
                continue
            sc, base, via, generic = best
            e = entry_from(base, key, label)
            if generic:
                e.elem = {}
            e.fconf = min(0.95, max(0.3, sc * 0.95))
            e.flags = (base.flags & ~(F_PART | F_ANCHOR | F_CN | F_PERSON | F_SPIRIT)) | F_ENTITY | F_POP
            if ent.get("height_m"):
                try:
                    e.scale = float(ent["height_m"])
                    e.size_src = "wikidata"
                except Exception:
                    pass
            if ent.get("mass_kg"):
                try:
                    e.mass = float(ent["mass_kg"])
                except Exception:
                    pass
            col = ent.get("color")
            if isinstance(col, str):
                rgb = A.POKEMON_COLOR.get(col.lower())
                if rgb is None and re.fullmatch(r"#?[0-9a-fA-F]{6}", col):
                    h = col.lstrip("#")
                    rgb = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
                e.accent = rgb or e.accent
            e.rank = 1.2 * math.log10(links + 1) + 0.3 * zipf(key) - (1.0 if latin else 0.0)
            e.why = [f"{key} \u2248 {norm(via)[:48]} (Wikidata)"] + base.why[:1]
            if latin and links < 12:
                continue  # obscure Latin binomial with no common name
            new[key] = e
            # a well-known named thing ("big mac") is a unit — but only when its reading is specific:
            # a taxon whose best class is plain "animal" ("flying snake") should stay compositional
            if " " in key and (ent.get("kind") == "type" or links >= 25) and fam not in ("taxon", "taxa") and e.fconf >= 0.55:
                e.flags |= F_COMPOUND
            ent_links[key] = links
            if ent.get("qid"):
                QID_KEY[ent["qid"]] = key
            overrides += override
            for al in (ent.get("aliases") or [])[:6]:
                ak = norm(al)
                if not ak or ak == key or len(ak) < 3 or len(ak) > 48 or ak in entries or ak in new or is_profane(ak):
                    continue
                a2 = entry_from(e, ak, al)
                a2.fconf, a2.flags, a2.rank = e.fconf * 0.95, e.flags, e.rank - 0.3
                a2.why = [f"{ak} = {label} (Wikidata alias)"] + e.why
                new[ak] = a2
            n += 1
    entries.update(new)
    log(f"entities: +{n} (+aliases => {len(new)} keys), {skipped} collisions kept the common word, "
        f"{overrides} entities beat a weak word, {unresolved} unresolved, {skipped_rel} religious figures excluded")
    return n


# ------------------------------------------------------------------------------------------------
# spirits: abstract nouns become Blob creatures (+Flies) with a temperament
# ------------------------------------------------------------------------------------------------
ABSTRACT_LEX = {"noun.feeling": 1.0, "noun.attribute": 3.2, "noun.state": 3.2, "noun.cognition": 3.4,
                "noun.act": 3.6, "noun.event": 3.4, "noun.phenomenon": 3.0, "noun.motive": 2.5,
                "noun.time": 3.4, "noun.communication": 3.8, "noun.process": 3.4}


def temper_of(syn, memo={}):
    n = syn.name()
    if n in memo:
        return memo[n]
    memo[n] = None
    r = A.TEMPERAMENT_SYNSETS.get(n)
    if r is None:
        for h in syn.hypernyms():
            r = temper_of(h)
            if r:
                break
    memo[n] = r
    return r


def add_spirits(wn, entries):
    n = 0
    for k, spec in A.SPIRITS.items():
        if spec is None:
            continue
        el, tp = spec
        e = Entry(k)
        e.form, e.fconf = "Blob", 0.9
        e.mech = {"Flies": 1.0}
        e.elem = {el: 1.0}
        e.temper = tp
        e.flags |= F_SPIRIT | F_ANCHOR
        e.why = [f"{k}: abstract → spirit ({el}, {tp})"]
        old = entries.get(k)
        if old is not None and old.flags & F_ANCHOR:
            old.temper = old.temper or tp  # keep the concrete anchor (e.g. "cold" stays a spirit only if unanchored)
            continue
        entries[k] = e
        n += 1
    # every remaining abstract noun with a feeling/attitude reading (or common enough) becomes a spirit
    for syn in wn.all_synsets("n"):
        lex = syn.lexname()
        need = ABSTRACT_LEX.get(lex)
        if need is None or syn.instance_hypernyms():
            continue
        tr = temper_of(syn)
        for lem in syn.lemmas()[:3]:
            name = lem.name()
            if name[:1].isupper() or "_" in name:
                continue
            k = norm(name)
            if k in entries or is_profane(k) or len(k) < 3 or k in A.STOP_NOUNS:
                continue
            z = zipf(k)
            if z < need and not tr:
                continue
            if z < 1.5:
                continue
            e = Entry(k)
            e.form, e.fconf = "Blob", 0.45 if tr else 0.32
            e.mech = {"Flies": 0.9}
            if tr:
                e.temper = tr[0]
                if tr[1]:
                    e.elem = {tr[1]: 0.8}
            e.flags |= F_SPIRIT
            e.why = [f"{k}: abstract ({lex[5:]}) → spirit" + (f", {tr[0]}" if tr else "")]
            entries[k] = e
            n += 1
    log(f"spirits: +{n}")


# ------------------------------------------------------------------------------------------------
# finalisation
# ------------------------------------------------------------------------------------------------
BLOCKED: set[str] = set()      # slur/hate tokens (denylist.txt)
BLOCKED_SUB: set[str] = set()  # entries marked * : also match inside longer words (names)


def load_denylist():
    for line in open(HERE / "denylist.txt", encoding="utf-8"):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        star = line.startswith("*")
        w = norm(line.lstrip("*"))
        if not w:
            continue
        BLOCKED.add(w)
        if star:
            BLOCKED_SUB.add(w.replace(" ", ""))


def is_profane(key: str) -> bool:
    """Only slurs / hate speech are blocked (owner policy); profanity is allowed."""
    if key in BLOCKED:
        return True
    return any(t in BLOCKED for t in key.split())


def finalize(e: Entry):
    form = e.form
    if e.flags & F_MOVE:
        # moves carry exactly their graft mechanics (the sim grafts a recipe by merging its mechanics)
        el = next(iter(e.elem), "Light")
        mech = sorted(e.mech)
        return dict(form=form, mech=mech, el=el, scale=round(float(e.scale), 3), mass=round(float(e.mass), 3),
                    feats=[], graft=mech, gelem=el if el != "Light" else None)
    cat = A.CATEGORY[form]
    d_scale, d_mass, d_mechs, d_elem = A.FORM_DEFAULTS[form]
    plaus = A.PLAUSIBLE.get(cat, set())
    # mechanics: intrinsic + inferred (thresholded, plausibility-weighted), max 4 inferred
    mech = set(d_mechs)
    inferred = []
    anchored = bool(e.flags & (F_ANCHOR | F_POKEMON))
    for m, c in e.mech.items():
        if m.startswith("-"):
            continue
        need = 0.5 if (m in plaus or anchored) else 0.9
        if c >= need:
            inferred.append((c, m))
    inferred.sort(key=lambda x: (-x[0], x[1]))
    for c, m in (inferred if anchored else inferred[:3]):
        mech.add(m)
    for m in e.mech:
        if m.startswith("-"):
            mech.discard(m[1:])
    if "Swarm" in mech and e.count <= 1:
        e.count = 6
    # element
    el = d_elem
    if e.elem:
        be, bc = max(e.elem.items(), key=lambda kv: (kv[1], kv[0]))
        if bc >= 0.6:
            el = be
    # size
    scale = e.scale if e.scale else d_scale
    mass = e.mass if e.mass else d_mass
    if form == "Kaiju" and scale < 6 and e.size_src not in ("pokeapi", "anchor", "wikidata"):
        scale, mass = max(scale, d_scale), max(mass, d_mass)
    # features
    feats = sorted(f for f, c in e.feat.items() if c >= 0.5)
    fw = A.FEATURE_WORDS.get(e.key)
    if fw is not None:
        feats = sorted(set(fw))
    # graft
    if e.graft is None:
        g = set(A.FORM_GRAFT.get(form, []))
        distinct = [m for c, m in inferred if m not in d_mechs and m != "Swarm"][:2]
        g.update(distinct)
        graft = sorted(g)
    else:
        graft = sorted(set(e.graft))
    gelem = el if el != "Light" else None
    return dict(form=form, mech=sorted(mech), el=el, scale=round(float(scale), 3), mass=round(float(mass), 3),
                feats=feats, graft=graft, gelem=gelem)


def build_modifiers(wn, entries):
    """Adjective / verb / collective tables (+ WordNet expansions)."""
    adj: dict[str, dict] = {}

    def put(word, conf=1.0, **kw):
        k = norm(word)
        if not k or k.endswith("_") or word.endswith("_"):
            return
        a = adj.setdefault(k, dict(k=k, e=None, m=[], s=1.0, w=1.0, f=[], c=conf, x=[], t=None))
        if kw.get("t") and not a.get("t"):
            a["t"] = kw["t"]
        if kw.get("e") and (a["e"] is None or conf > a.get("ec", 0)):
            a["e"] = kw["e"]
            a["ec"] = conf
        for m in kw.get("m", []):
            if m not in a["m"]:
                a["m"].append(m)
        for f in kw.get("f", []):
            if f not in a["f"]:
                a["f"].append(f)
        if kw.get("s"):
            a["s"] = kw["s"]
            a["w"] = kw.get("w", 1.0)
        if kw.get("x"):
            a["x"].append(kw["x"])
        a["c"] = max(a["c"], conf)

    for el, words in A.ELEMENT_ADJ.items():
        for w in words:
            put(w, e=el, x=f"{norm(w)} → {el}")
    for w, (s, m, ms) in A.SIZE_ADJ.items():
        put(w, s=s, w=m, m=ms, x=f"{w} → ×{s:g} size")
    for mech, words in A.MECH_ADJ.items():
        for w in words:
            put(w, m=[mech], x=f"{norm(w)} → {mech}")
    for w, fs in A.FEATURE_ADJ.items():
        put(w, f=fs, x=f"{w} → {'+'.join(fs)}")
    for w, meters in A.ABS_SIZE_ADJ.items():
        put(w, x=f"{w} → {meters:g} m")
        adj[norm(w)]["abs"] = meters
    for tp, words in A.TEMPERAMENT_ADJ.items():
        for w in words:
            put(w, t=tp, x=f"{norm(w)} → {tp}")
    for w, el in A.TEMPERAMENT_WEAK_ELEMENT.items():
        k = norm(w)
        if k in adj and not adj[k]["e"]:
            adj[k]["we"] = el
    for w, spec in A.SAFE_ADJ.items():
        k = norm(w)
        adj[k] = dict(k=k, e=spec.get("e"), m=list(spec.get("m", [])), s=1.0, w=1.0, f=list(spec.get("f", [])),
                      c=1.0, x=[spec["x"]], t=spec.get("t"), safe=True)
    # WordNet adjective expansion: similar_tos / also_sees of anchored adjectives (1 hop, decayed).
    seeds = list(adj.items())
    added = 0
    for k, a in seeds:
        if " " in k or a.get("safe"):
            continue
        for s in wn.synsets(k, pos="a")[:2] + wn.synsets(k, pos="s")[:2]:
            for rel in (s.similar_tos(), s.also_sees()):
                for s2 in rel:
                    for lem in s2.lemma_names():
                        k2 = norm(lem.replace("_", " "))
                        if k2 in adj or " " in k2 or zipf(k2) < 2.0 or is_profane(k2):
                            continue
                        if {u.name() for u in s2.usage_domains()} & (SLUR_DOMAINS | VULGAR_DOMAINS):
                            continue
                        put(k2, conf=0.7, e=a["e"], m=a["m"], f=a["f"], t=a.get("t"),
                            s=(a["s"] if a["s"] != 1.0 else None), w=a["w"],
                            x=f"{k2} ≈ {k} (WordNet similar)")
                        added += 1
    log(f"adjectives: {len(adj)} ({added} via WordNet)")

    # verbs
    verbs: dict[str, dict] = {}
    exc = {}
    try:
        for infl, bases in wn._exception_map["v"].items():
            for b in bases:
                exc.setdefault(b, set()).add(infl)
    except Exception:
        pass

    def inflect(v: str):
        forms = {v}
        if v.endswith(("s", "sh", "ch", "x", "z", "o")):
            forms.add(v + "es")
        elif v.endswith("y") and len(v) > 2 and v[-2] not in "aeiou":
            forms.add(v[:-1] + "ies")
        else:
            forms.add(v + "s")
        if v.endswith("e") and not v.endswith("ee"):
            ing, ed = v[:-1] + "ing", v + "d"
        elif v.endswith("y") and len(v) > 2 and v[-2] not in "aeiou":
            ing, ed = v + "ing", v[:-1] + "ied"
        elif (len(v) >= 3 and v[-1] not in "aeiouwxy" and v[-2] in "aeiou" and v[-3] not in "aeiou"
              and len(v) <= 5):
            ing, ed = v + v[-1] + "ing", v + v[-1] + "ed"
        else:
            ing, ed = v + "ing", v + "ed"
        forms.update({ing, ed, ing[:-1], ing[:-1] + "'"})  # "shootin"
        forms.update(exc.get(v, ()))
        return forms

    def putv(lemma, mechs, el=None, payload=None, conf=1.0, why=None):
        parts = lemma.split(" ")
        head, rest = parts[0], parts[1:]
        for form in inflect(head):
            k = norm(" ".join([form] + rest))
            if not k:
                continue
            v = verbs.get(k)
            if v is None or v["c"] < conf:
                verbs[k] = dict(k=k, lemma=lemma, m=sorted(set(mechs)), e=el, p=payload, c=conf,
                                x=why or f"{k} → {'+'.join(mechs) or (el or 'verb')}")
            else:
                for m in mechs:
                    if m not in v["m"]:
                        v["m"] = sorted(set(v["m"]) | {m})
                if payload and not v["p"]:
                    v["p"] = payload
                if el and not v["e"]:
                    v["e"] = el

    for v, ms in A.VERBS.items():
        putv(v, ms, A.VERB_ELEMENT.get(v))
    for v, (hm, pm) in A.PAYLOAD_VERBS.items():
        putv(v, [hm], A.VERB_ELEMENT.get(v), payload=[hm, pm])
    for v, el in A.VERB_ELEMENT.items():
        if v not in A.VERBS:
            putv(v, [], el)
    # WordNet verb synonym expansion (1 hop, first two senses, common words only)
    base = list(A.VERBS.items())
    added = 0
    for v, ms in base:
        if " " in v:
            continue
        for s in wn.synsets(v, pos="v")[:2]:
            names = set(s.lemma_names())
            if names & VULGAR_VERB_LEMMAS or {u.name() for u in s.usage_domains()} & (SLUR_DOMAINS | VULGAR_DOMAINS):
                continue  # e.g. bonk/bang/screw sexual senses must not map "love" to Blunt
            for lem in s.lemma_names():
                k2 = norm(lem.replace("_", " "))
                if not k2 or " " in k2 or k2 in verbs or zipf(k2) < 2.8 or is_profane(k2):
                    continue
                putv(k2, ms, A.VERB_ELEMENT.get(v), conf=0.7, why=f"{k2} ≈ {v} (WordNet verb synonym)")
                added += 1
    log(f"verbs: {len(verbs)} forms ({added} lemmas via WordNet)")

    colls = [dict(k=k, n=n, swarm=sw) for k, (n, sw) in A.COLLECTIVES.items()]
    return list(adj.values()), list(verbs.values()), colls


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--no-pack", action="store_true")
    ap.add_argument("--entities", default=str(ENTITIES_DEFAULT))
    ap.add_argument("--release", action="store_true", help="pack with cargo --release")
    args = ap.parse_args()

    sources.fetch_all()
    cn_path = sources.filter_conceptnet()
    load_denylist()

    wn = load_wordnet()
    prop = WNProp(wn)
    entries = wordnet_entries(wn, prop)
    log(f"wordnet entries: {len(entries)}")

    cn_out, cn_partof = load_conceptnet(cn_path)
    # lexicalised multi-word units: WordNet multi-word lemmas + well-connected ConceptNet concepts
    wn_multi = {norm(l.replace("_", " ")) for l in wn.all_lemma_names("n") if "_" in l}
    cn_multi = {t for t, ed in cn_out.items() if " " in t and len(ed) >= CN_COMPOUND_DEGREE}
    log(f"compounds: {len(wn_multi)} WordNet multi-word lemmas, {len(cn_multi)} ConceptNet multi-word concepts")
    idx = build_concept_index()
    new = conceptnet_new_words(entries, cn_out, cn_partof, idx, wn)
    entries.update(new)

    load_corpora(entries)

    # pop culture aliases (procedural, no icon)
    for k, v in A.POP_CULTURE.items():
        if v is None:
            continue
        base_k, extra, el, disp = v
        base = entries.get(norm(base_k))
        if base is None or base.form is None:
            spec = A.WORD_ANCHORS.get(norm(base_k))
            if spec:
                base = Entry(norm(base_k))
                apply_word_anchor(base, spec)
            else:
                log(f"  WARN pop culture base '{base_k}' for '{k}' unresolved")
                continue
        e = entry_from(base, norm(k), disp)
        e.fconf = 0.95
        for m in extra:
            e.mech[m] = 1.0
        if el:
            e.elem = {el: 1.0}
        e.flags |= F_POP
        e.rank = 3.0
        e.why = [f"{norm(k)} ≈ {base_k} (pop culture)"]
        entries[e.key] = e

    # word anchors (override)
    for k, spec in A.WORD_ANCHORS.items():
        if spec is None:
            continue
        k = norm(k)
        e = entries.get(k)
        if e is None:
            e = Entry(k)
            entries[k] = e
        prev_why = [w for w in e.why[:1] if ("hypernym" in w or "synonym" in w) and "anchor" not in w]
        apply_word_anchor(e, spec)
        e.why = [f"{k} (hand anchor → {spec['f']})"] + prev_why

    add_spirits(wn, entries)
    load_pokemon(entries)
    # re-assert anchors that pokemon may not override (handled inside) and pure flags
    for k in A.PURE:
        e = entries.get(norm(k))
        if e is not None:
            e.flags |= F_PURE

    # ConceptNet mechanics / elements for everything (after forms are known)
    t = time.time()
    size_adj = {}
    for k, e in entries.items():
        if e.form is None or e.flags & (F_POKEMON | F_ENTITY | F_ANCHOR):
            continue
        size_adj[k] = conceptnet_evidence(e, cn_out, cn_partof, *idx)
    log(f"conceptnet evidence applied in {time.time() - t:.0f}s")
    del cn_out

    n_ent = load_entities(entries, Path(args.entities))
    load_names_txt(entries, ASSETS / "icons")
    move_table = load_moves(entries)

    # ---------- filter + finalise ----------
    out = []
    dropped = collections.Counter()
    for k, e in entries.items():
        if e.form is None:
            dropped["noform"] += 1
            continue
        if k in A.STOP_NOUNS or is_profane(k):
            dropped["stop/profane"] += 1
            continue
        if not re.fullmatch(r"[a-z0-9]+( [a-z0-9]+){0,5}", k):
            dropped["charset"] += 1
            continue
        if re.match(r"^(genus|family|order|class|phylum|subfamily|superfamily|tribe|suborder|subclass|division) ", k) \
                or re.search(r" (family|genus|order)$", k):
            dropped["taxon"] += 1
            continue
        z = zipf(k)
        keep_flags = F_ANCHOR | F_POKEMON | F_POP | F_ENTITY
        if not (e.flags & keep_flags):
            multi = " " in k
            if e.flags & F_PERSON:
                if z < 2.8 and k not in A.FUN_PERSONS:
                    dropped["rare person"] += 1
                    continue
            elif (not multi and z < 1.0) or (multi and z < 0.5 and e.fconf < 0.6):
                dropped["rare"] += 1
                continue
            if e.fconf < 0.28:
                dropped["lowconf"] += 1
                continue
        if e.size_src is None and k in size_adj:
            f = size_adj[k]
            if f >= 0.6:
                e.scale = A.FORM_DEFAULTS[e.form][0] * 1.8
                e.mass = A.FORM_DEFAULTS[e.form][1] * 4
            elif f <= -0.6:
                e.scale = A.FORM_DEFAULTS[e.form][0] * 0.55
                e.mass = A.FORM_DEFAULTS[e.form][1] * 0.2
        if " " in k:
            if e.flags & F_ANCHOR or k in wn_multi or k in cn_multi:
                e.flags |= F_COMPOUND
        elif z >= COMMON_ZIPF:
            e.flags |= F_COMMON
        r = finalize(e)
        rank = e.rank if e.rank else 0.0
        rank = max(rank, z + (1.5 if e.flags & F_ANCHOR else 0.0) + (0.5 if e.flags & F_POP else 0.0))
        conf = round(min(1.0, e.fconf), 3)
        why = []
        for w in e.why:
            if w not in why:
                why.append(w)
        out.append(dict(k=k, d=e.display if e.display != k else None, f=r["form"], m=r["mech"], e=r["el"],
                        s=r["scale"], w=r["mass"], n=int(e.count), a=e.accent, c=conf, r=round(rank, 3),
                        g=r["graft"], ge=r["gelem"], ft=r["feats"], fl=e.flags, x=why[:4], t=e.temper))
    out.sort(key=lambda d: d["k"])
    log(f"final entries: {len(out)}; dropped {dict(dropped)}")

    adjs, verbs, colls = build_modifiers(wn, entries)
    infl = []
    keys = {d["k"] for d in out}
    try:
        for inflected, bases in wn._exception_map["n"].items():
            for b in bases:
                bk, ik = norm(b.replace("_", " ")), norm(inflected.replace("_", " "))
                if bk in keys and ik not in keys and ik != bk:
                    infl.append([ik, bk])
    except Exception:
        pass
    infl += [["pokemon", "pokemon"], ["teeth", "tooth"], ["feet", "foot"], ["geese", "goose"], ["mice", "mouse"],
             ["dice", "die"], ["octopi", "octopus"], ["cacti", "cactus"], ["fungi", "fungus"], ["wolves", "wolf"],
             ["knives", "knife"], ["leaves", "leaf"], ["elves", "elf"], ["dwarves", "dwarf"], ["staves", "staff"],
             ["oxen", "ox"], ["people", "person"], ["men", "man"], ["women", "woman"], ["children", "child"],
             ["lice", "louse"], ["loaves", "loaf"], ["hooves", "hoof"], ["halves", "half"], ["thieves", "thief"]]
    infl = sorted({tuple(x) for x in infl if x[1] in keys})

    # voice-bias candidates: pop culture + pokemon by rank
    pop = sorted((d for d in out if d["fl"] & (F_POKEMON | F_POP)), key=lambda d: (-d["r"], d["k"]))
    bias = [d["d"] or d["k"] for d in pop[:400]]

    blocked = sorted(("*" if b.replace(" ", "") in BLOCKED_SUB else "") + b for b in BLOCKED)
    intens = sorted([norm(k), v] for k, v in A.INTENSIFIERS.items())
    known = [w for w in wordfreq.top_n_list("en", 40000) if re.fullmatch(r"[a-z]{2,}", w) and not is_profane(w)]
    lex = dict(version=5, generated=time.strftime("%Y-%m-%d"), entries=out, adjectives=adjs, verbs=verbs,
               known=known,
               intensifiers=intens, blocked=blocked,
               collectives=colls, inflections=[list(x) for x in infl], bias=bias,
               payload_hosts=sorted({norm(h) for h in A.PAYLOAD_HOSTS}),
               pure=[[norm(k), v[0], v[1]] for k, v in sorted(A.PURE.items())],
               moves=move_table,
               sources=dict(conceptnet=sources.CONCEPTNET_URL, corpora=sources.CORPORA_URL,
                            wordnet=sources.WORDNET_URL, pokeapi=sources.POKEAPI_BASE,
                            entities=str(args.entities) if n_ent else None))
    jpath = CACHE / "lexicon.json"
    with open(jpath, "w", encoding="utf-8") as f:
        json.dump(lex, f, ensure_ascii=False, separators=(",", ":"))
    log(f"wrote {jpath} ({jpath.stat().st_size / 1e6:.1f} MB)")

    # human-readable preview: a deterministic sample stratified by form + the curated highlights
    by_form = collections.defaultdict(list)
    for d in out:
        by_form[d["f"]].append(d)
    must = {"maul", "parachute", "claw", "scorpion", "sledgehammer", "umbrella", "boomerang", "pikachu",
            "charizard", "snorlax", "gyarados", "tornado", "anvil", "piano", "jetpack", "lightsaber", "excalibur",
            "kraken", "electric eel", "trampoline", "python", "bee", "dragon", "train", "tank", "helicopter"}
    prev = [d for d in out if d["k"] in must]
    for f, ds in sorted(by_form.items()):
        ds = sorted(ds, key=lambda d: (-d["r"], d["k"]))
        prev += ds[:12]
    step = max(1, len(out) // 150)
    prev += out[::step]
    seen = set()
    prev2 = []
    for d in prev:
        if d["k"] not in seen:
            seen.add(d["k"])
            prev2.append(d)
    pv = dict(note="Sample of assets/lexicon.bin for review. Full data: tools/lexicon_build/cache/lexicon.json",
              counts=dict(entries=len(out), adjectives=len(adjs), verbs=len(verbs),
                          by_form={f: len(v) for f, v in sorted(by_form.items())}),
              entries=prev2[:600])
    with open(ASSETS / "lexicon_preview.json", "w", encoding="utf-8") as f:
        json.dump(pv, f, ensure_ascii=False, indent=1)
    log(f"wrote assets/lexicon_preview.json ({len(pv['entries'])} sample entries)")

    if not args.no_pack:
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(GAME / "target" / "lexicon")
        cmd = ["cargo", "run", "-q", "-p", "bd_lexicon", "--features", "builder", "--example", "pack"]
        if args.release:
            cmd.insert(2, "--release")
        cmd += ["--", str(jpath), str(ASSETS / "lexicon.bin")]
        log("packing: " + " ".join(cmd))
        subprocess.run(cmd, cwd=GAME, env=env, check=True)
    log("done")


if __name__ == "__main__":
    main()
