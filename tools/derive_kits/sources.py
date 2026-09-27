"""Read the repo's character data for derive_kits.py (stdlib only, deterministic).

Sources (all checked in, so every run on every machine sees the same thing):
  * assets/icons.ron + assets/icons/*.ron      hand-authored IconEntry recipes (best data)
  * assets/icons/*_names.txt                  `name | aliases | QID | hint` scanner lists
  * assets/costumes/characters.ron            costume looks (weapons on the back, colours)
  * assets/powers/*.ron (minus our output)    what hand-made kits / powers already claim
  * crates/bd_core/src/mechanics.rs           the closed enums (Form, Mechanic, Element, ...)
  * crates/bd_sim/src/powers/data.rs          which MoveOps exist today
  * assets/costumes/parts.ron + costume/grammar.rs   words the costume grammar owns
"""

from __future__ import annotations

import glob
import os
import re
from dataclasses import dataclass, field

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def rel(*p: str) -> str:
    return os.path.join(ROOT, *p)


def read(path: str) -> str:
    with open(path, encoding="utf-8") as f:
        return f.read()


# ------------------------------------------------------------------------------------ enums

def enum_variants(src: str, name: str) -> list[str]:
    m = re.search(r"pub enum " + name + r"\s*\{", src)
    if not m:
        return []
    i, depth, out = m.end(), 1, []
    body_start = i
    while i < len(src) and depth:
        c = src[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        i += 1
    body = src[body_start:i - 1]
    # top-level variant identifiers (skip comments, attributes and nested field blocks)
    depth = 0
    for line in body.splitlines():
        s = line.split("//")[0].strip()
        if depth == 0:
            mm = re.match(r"([A-Z][A-Za-z0-9]*)\b", s)
            if mm and not s.startswith("#"):
                out.append(mm.group(1))
        depth += s.count("{") - s.count("}")
        depth += s.count("(") - s.count(")") if depth > 0 else 0
    return out


class Enums:
    def __init__(self) -> None:
        src = read(rel("crates", "bd_core", "src", "mechanics.rs"))
        self.forms = set(enum_variants(src, "Form"))
        self.mechanics = set(enum_variants(src, "Mechanic"))
        self.elements = set(enum_variants(src, "Element"))
        self.temperaments = set(enum_variants(src, "Temperament"))
        self.features = set(enum_variants(src, "Feature"))
        data = read(rel("crates", "bd_sim", "src", "powers", "data.rs"))
        self.ops = set(enum_variants(data, "MoveOp"))


# ------------------------------------------------------------------------------------ RON scanning

def balanced(s: str, i: int) -> int:
    """Index just past the bracket group opening at s[i] (strings and // comments skipped)."""
    opener = s[i]
    closer = {"(": ")", "[": "]", "{": "}"}[opener]
    depth = 0
    n = len(s)
    while i < n:
        c = s[i]
        if c == '"':
            i += 1
            while i < n and s[i] != '"':
                if s[i] == "\\":
                    i += 1
                i += 1
        elif c == "/" and i + 1 < n and s[i + 1] == "/":
            while i < n and s[i] != "\n":
                i += 1
            continue
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def top_items(list_src: str) -> list[str]:
    """Top-level `( ... )` items of a `[ ... ]` list text."""
    out = []
    i = list_src.find("[")
    if i < 0:
        return out
    end = balanced(list_src, i)
    j = i + 1
    while j < end - 1:
        c = list_src[j]
        if c == '"':
            j = balanced_string(list_src, j)
            continue
        if c == "/" and list_src[j + 1:j + 2] == "/":
            while j < end and list_src[j] != "\n":
                j += 1
            continue
        if c == "(":
            k = balanced(list_src, j)
            out.append(list_src[j:k])
            j = k
            continue
        j += 1
    return out


def balanced_string(s: str, i: int) -> int:
    i += 1
    while i < len(s) and s[i] != '"':
        if s[i] == "\\":
            i += 1
        i += 1
    return i + 1


def fields(entry: str) -> dict[str, str]:
    """Top-level `key: value` pairs of one `( ... )` struct text (values raw)."""
    out: dict[str, str] = {}
    s = entry.strip()
    if not s.startswith("("):
        k = s.find("(")
        s = s[k:]
    n = len(s)
    i = 1
    depth = 1
    key_start = None
    cur_key = None
    val_start = 0
    while i < n:
        c = s[i]
        if c == '"':
            i = balanced_string(s, i)
            continue
        if c == "/" and s[i + 1:i + 2] == "/":
            while i < n and s[i] != "\n":
                i += 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                if cur_key is not None:
                    out[cur_key] = s[val_start:i].strip()
                break
        elif c == "," and depth == 1:
            if cur_key is not None:
                out[cur_key] = s[val_start:i].strip()
                cur_key = None
            key_start = None
        elif c == ":" and depth == 1 and cur_key is None and key_start is not None:
            cur_key = s[key_start:i].strip()
            val_start = i + 1
        elif depth == 1 and cur_key is None and (c.isalnum() or c == "_"):
            if key_start is None:
                key_start = i
        elif depth == 1 and cur_key is None and not c.isspace():
            key_start = None
        i += 1
    return out


def strings(v: str | None) -> list[str]:
    if not v:
        return []
    return [m.group(1).replace('\\"', '"') for m in re.finditer(r'"((?:[^"\\]|\\.)*)"', v)]


def idents(v: str | None) -> list[str]:
    if not v:
        return []
    v = re.sub(r'"(?:[^"\\]|\\.)*"', "", v)
    return re.findall(r"\b[A-Z][A-Za-z0-9]*\b", v)


def number(v: str | None, default: float) -> float:
    try:
        return float(v) if v is not None else default
    except ValueError:
        return default


def rgb_some(v: str | None) -> tuple[float, float, float] | None:
    if not v or not v.strip().startswith("Some"):
        return None
    nums = re.findall(r"-?\d+(?:\.\d+)?", v)
    if len(nums) < 3:
        return None
    return tuple(max(0.0, min(1.0, float(x))) for x in nums[:3])  # type: ignore[return-value]


def hex_rgb(h: str) -> tuple[float, float, float] | None:
    h = h.strip().lstrip("#")
    if not re.fullmatch(r"[0-9a-fA-F]{6}", h):
        return None
    return tuple(int(h[k:k + 2], 16) / 255.0 for k in (0, 2, 4))  # type: ignore[return-value]


def norm_words(s: str) -> str:
    """bd_sim `powers::norm_words`: lower-case, punctuation stripped (apostrophes kept), single spaces."""
    out = []
    for w in re.split(r"[\s\-_]+", s):
        w = "".join(ch for ch in w if ch.isalnum() or ch == "'").lower()
        if w:
            out.append(w)
    return " ".join(out)


# ------------------------------------------------------------------------------------ characters

@dataclass
class Char:
    key: str                      # canonical normalised name (first name)
    display: str                  # title as authored
    names: list[str]              # normalised aliases (key first)
    pack: str                     # "marvel", "pokemon", ... ("costume" for costume-only)
    source: str = ""
    form: str = "Biped"
    mech: set[str] = field(default_factory=set)
    element: str = "Light"
    temperament: str = "Calm"
    scale: float = 2.2
    mass: float = 80.0
    intensity: float = 0.5
    features: set[str] = field(default_factory=set)
    accent: tuple[float, float, float] | None = None
    payload: tuple[str, str, str] | None = None      # (name, form, element) of what it carries/fires
    notes: str = ""               # free text (names.txt notes, source) — keyword hints only
    weapons: set[str] = field(default_factory=set)   # costume accessories ("BigSword", "Bow", ...)
    colors: list[tuple[float, float, float]] = field(default_factory=list)  # costume palette
    origin: str = "icon"          # icon | names | costume
    order: int = 0


CREATURE_FORMS = {"Biped", "Kaiju", "Quadruped", "Serpent", "Bird", "Fish", "Arachnid", "Insect", "Blob"}


def is_character(form: str, mech: set[str], element: str, temperament: str) -> bool:
    if "Transforms" in mech:
        return True
    if form not in CREATURE_FORMS:
        return False
    if form == "Blob":
        # squishmallows and play-doh are blobs too: only blobs that DO something are characters
        fighty = {"Shoots", "Beam", "Breath", "Grab", "Blunt", "Blade", "Explodes", "Charge", "Constrict", "Swarm", "Pierce",
                  "Stomp", "Pull", "Attract", "Repel", "Flies", "Spin", "Homing"}
        return bool(mech & fighty) or element != "Light" or temperament in ("Aggressive", "Chaotic", "Sneaky")
    return True


def parse_icon_file(path: str, pack: str, en: Enums, order0: int) -> list[Char]:
    src = read(path)
    out = []
    pos = 0
    k = order0
    while True:
        i = src.find("IconEntry(", pos)
        if i < 0:
            break
        j = src.find("(", i)
        end = balanced(src, j)
        entry = src[j:end]
        pos = end
        f = fields(entry)
        names = [norm_words(n) for n in strings(f.get("names"))]
        names = [n for n in names if n]
        rec = f.get("recipe")
        if not names or not rec:
            continue
        rf = fields(rec)
        form = (idents(rf.get("form")) or ["Ball"])[0]
        mech = set(idents(rf.get("mechanics"))) & en.mechanics
        element = (idents(rf.get("element")) or ["Light"])[0]
        temper = (idents(rf.get("temperament")) or ["Calm"])[0]
        if not is_character(form, mech, element, temper):
            continue
        payload = None
        pv = rf.get("payload")
        if pv and pv.strip().startswith("Some"):
            pf = fields(pv[pv.find("(", 4):])
            pname = (strings(pf.get("name")) or [""])[0]
            payload = (pname, (idents(pf.get("form")) or ["Ball"])[0], (idents(pf.get("element")) or ["Light"])[0])
        c = Char(
            key=names[0],
            display=(strings(rf.get("name")) or [names[0].title()])[0],
            names=names,
            pack=pack,
            source=(strings(f.get("source")) or [""])[0],
            form=form,
            mech=mech,
            element=element if element in en.elements else "Light",
            temperament=temper if temper in en.temperaments else "Calm",
            scale=number(rf.get("scale"), 2.2),
            mass=number(rf.get("mass"), 80.0),
            intensity=number(rf.get("intensity"), 0.5),
            features=set(idents(rf.get("features"))) & en.features,
            accent=rgb_some(rf.get("accent")),
            payload=payload,
            origin="icon",
            order=k,
        )
        c.notes = c.source
        out.append(c)
        k += 1
    return out


def parse_names_file(path: str, pack: str, en: Enums, order0: int) -> list[Char]:
    out = []
    k = order0
    for line in read(path).splitlines():
        if not line.strip() or line.lstrip().startswith("#") or "|" not in line:
            continue
        parts = [p.strip() for p in line.split("|")]
        name = norm_words(parts[0])
        if not name:
            continue
        aliases = [norm_words(a) for a in parts[1].split(";")] if len(parts) > 1 else []
        hint = parts[-1] if len(parts) >= 4 else (parts[3] if len(parts) > 3 else parts[-1])
        notes = " ".join(re.findall(r"\[([^\]]*)\]", hint))
        body = re.sub(r"\[[^\]]*\]", " ", hint).replace("hint:", " ")
        toks = re.findall(r"[A-Za-z]+", body)
        form = next((t for t in toks if t in en.forms), None)
        if form is None:
            continue
        mech = {t for t in toks if t in en.mechanics}
        element = next((t for t in toks if t in en.elements), "Light")
        temper = next((t for t in toks if t in en.temperaments), "Calm")
        if not is_character(form, mech, element, temper):
            continue
        feats = {t for t in toks if t in en.features}
        size = {"Kaiju": (8.0, 2000.0), "Quadruped": (1.8, 150.0), "Bird": (1.4, 20.0), "Insect": (1.0, 5.0),
                "Arachnid": (1.4, 20.0), "Fish": (1.6, 60.0), "Serpent": (3.0, 200.0), "Blob": (2.0, 80.0)}.get(form, (2.4, 85.0))
        mass = size[1] * (2.5 if "Heavy" in mech else 1.0)
        c = Char(
            key=name,
            display=parts[0].strip(),
            names=[name] + [a for a in aliases if a and a != name],
            pack=pack,
            source="",
            form=form,
            mech=mech,
            element=element,
            temperament=temper,
            scale=size[0],
            mass=mass,
            features=feats,
            origin="names",
            order=k,
        )
        c.notes = notes
        out.append(c)
        k += 1
    return out


COSTUME_WEAPON_ACC = {
    "BackSword": "sword", "BigSword": "greatsword", "TwinSwords": "twin swords", "HipSwords": "katana", "Saber": "lightsaber",
    "Staff": "staff", "Hammer": "hammer", "BigHammer": "greathammer", "Axe": "axe", "Polearm": "polearm", "Bow": "bow",
    "Quiver": "bow", "Holsters": "guns", "ArmCannon": "arm cannon", "Claws": "claws", "Gauntlet": "gauntlet",
    "RoundShield": "shield", "KiteShield": "shield", "Jetpack": "jetpack", "Wings": "wings", "Shell": "shell", "Gourd": "gourd",
    "Tail": "tail", "Cape": "cape", "ChestLight": "reactor",
}


def parse_costumes(en: Enums, order0: int) -> list[Char]:
    src = read(rel("assets", "costumes", "characters.ron"))
    out = []
    k = order0
    for item in top_items(src[src.find("["):]):
        f = fields(item)
        names = [norm_words(n) for n in strings(f.get("names"))]
        names = [n for n in names if n]
        if not names:
            continue
        look = f.get("look", "")
        lf = fields(look) if look.strip().startswith("(") else {}
        accs = set(re.findall(r"\b([A-Z][A-Za-z]+)\(", lf.get("acc", "")))
        cols = []
        for key in ("top", "legs", "hair"):
            v = lf.get(key, "")
            for h in re.findall(r'color:\s*"([0-9a-fA-F]{6})"', v)[:1] + re.findall(r'trim:\s*"([0-9a-fA-F]{6})"', v)[:1]:
                rgb = hex_rgb(h)
                if rgb:
                    cols.append(rgb)
        c = Char(
            key=names[0],
            display=names[0].title(),
            names=names,
            pack="costume",
            source=(strings(f.get("source")) or [""])[0],
            weapons={COSTUME_WEAPON_ACC[a] for a in accs if a in COSTUME_WEAPON_ACC},
            colors=cols,
            origin="costume",
            order=k,
        )
        c.notes = c.source
        # variants: words the costume grammar owns after this name
        vwords = []
        for v in top_items(f.get("variants", "[]")):
            vf = fields(v)
            vwords += strings(vf.get("name")) + strings(vf.get("words"))
        c.variant_words = [norm_words(w) for w in vwords if norm_words(w)]  # type: ignore[attr-defined]
        out.append(c)
        k += 1
    return out


# ------------------------------------------------------------------------------------ what hand content claims

@dataclass
class Claims:
    names: set[str] = field(default_factory=set)       # every power / tier / kit name + id
    shouts: set[str] = field(default_factory=set)
    kit_names: dict[str, str] = field(default_factory=dict)  # kit name -> id (hand kits)
    grant_names: set[str] = field(default_factory=set)
    files: list[str] = field(default_factory=list)
    kits: list[dict] = field(default_factory=list)      # hand kits: {id, names, moves:[{name, tags}], shouts}
    entries: int = 0                                    # every hand power entry (the table budget)


def parse_claims(skip_file: str) -> Claims:
    cl = Claims()
    for path in sorted(glob.glob(rel("assets", "powers", "*.ron"))):
        base = os.path.basename(path)
        if base == skip_file:
            continue
        cl.files.append(base)
        src = read(path)
        i = src.find("[")
        if i < 0:
            continue
        for item in top_items(src[i:]):
            f = fields(item)
            pid = (strings(f.get("id")) or [""])[0].lower()
            if not pid:
                continue
            cl.entries += 1
            kind = " ".join(idents(f.get("kind")))
            own = [norm_words(pid.replace("_", " "))]
            own += [norm_words(n) for n in strings(f.get("name"))[:1]]
            own += [norm_words(n) for n in strings(f.get("names"))]
            own += [norm_words(n) for n in strings(f.get("claim"))]
            tiers = top_items(f.get("tiers", "[]")) if f.get("tiers") else []
            tier_names = []
            for t in tiers:
                tf = fields(t)
                tier_names += [norm_words(n) for n in strings(tf.get("names"))]
                for s in re.findall(r'\(\s*"((?:[^"\\]|\\.)*)"\s*,\s*\d+\s*\)', tf.get("shouts", "")):
                    cl.shouts.add(norm_words(s))
            own = [n for n in own + tier_names if n]
            cl.names.update(own)
            for s in re.findall(r'\(\s*"((?:[^"\\]|\\.)*)"\s*,\s*\d+\s*\)', f.get("shouts", "")):
                cl.shouts.add(norm_words(s))
            if kind == "Kit":
                kn = [n for n in own if n not in tier_names]
                for n in kn:
                    cl.kit_names.setdefault(n, pid)
                moves = []
                for mv in top_items(f.get("moves", "[]")):
                    mf = fields(mv)
                    moves.append({"name": norm_words((strings(mf.get("name")) or [""])[0]), "tags": idents(mf.get("tags"))})
                shouts = [(norm_words(s), int(n)) for s, n in re.findall(r'\(\s*"((?:[^"\\]|\\.)*)"\s*,\s*(\d+)\s*\)', f.get("shouts", ""))]
                cl.kits.append({"id": pid, "names": sorted(set(kn)), "moves": moves, "shouts": shouts})
            else:
                cl.grant_names.update(own)
    return cl


# ------------------------------------------------------------------------------------ words other systems own

def costume_grammar_words() -> set[str]:
    """Words the costume grammar reads after a character name (parts, colours, size, clauses)."""
    words: set[str] = set()
    parts = read(rel("assets", "costumes", "parts.ron"))
    for m in re.finditer(r"names:\s*\[([^\]]*)\]", parts):
        for n in strings(m.group(1)):
            words.update(norm_words(n).split())
    g = rel("crates", "bd_shapes", "src", "costume", "grammar.rs")
    if os.path.exists(g):
        for s in re.findall(r'"([a-z][a-z ]*)"', read(g)):
            words.update(s.split())
    return {w for w in words if len(w) >= 3}


def fx_keys() -> set[str]:
    p = rel("assets", "tuning", "move_fx.ron")
    if not os.path.exists(p):
        return set()
    src = read(p)
    keys = set(re.findall(r'^\s*"([a-z ]+)":\s*\(', src, re.M))
    aliases = set(re.findall(r'"([a-z ]+)":\s*"[a-z ]+"', src))
    return keys | aliases


def icon_names_all() -> set[str]:
    """Every name any icon / names list / costume answers to (for the collision check)."""
    out: set[str] = set()
    for path in [rel("assets", "icons.ron")] + sorted(glob.glob(rel("assets", "icons", "*.ron"))):
        for m in re.finditer(r"names:\s*\[([^\]]*)\]", read(path)):
            out.update(norm_words(n) for n in strings(m.group(1)))
    for path in sorted(glob.glob(rel("assets", "icons", "*_names.txt"))):
        for line in read(path).splitlines():
            if line.lstrip().startswith("#") or "|" not in line:
                continue
            p = line.split("|")
            out.add(norm_words(p[0]))
            if len(p) > 1:
                out.update(norm_words(a) for a in p[1].split(";"))
    sig = rel("assets", "signatures.ron")
    if os.path.exists(sig):
        for m in re.finditer(r"names:\s*\[([^\]]*)\]", read(sig)):
            out.update(norm_words(n) for n in strings(m.group(1)))
    out.discard("")
    return out


# ------------------------------------------------------------------------------------ popularity

CHAR_FAMILIES = {"fiction_character", "pokemon", "mecha", "kaiju", "robot", "myth_creature", "fiction_other"}
GENERIC_KINDS = {"type", "taxon"}


def popularity(names: set[str]) -> dict[str, dict]:
    """Wikidata harvest (tools/lexicon_build/data/entities.jsonl.gz, checked in): for every normalised name
    in `names`, the best CHARACTER match's sitelinks (`char`), the best GENERIC-type match's (`generic`:
    "dragon", "werewolf") and the best match overall's family (`family`). Missing file = empty map."""
    import gzip
    import json

    path = rel("tools", "lexicon_build", "data", "entities.jsonl.gz")
    out: dict[str, dict] = {}
    if not os.path.exists(path):
        return out
    with gzip.open(path, "rt", encoding="utf-8") as fh:
        for line in fh:
            try:
                e = json.loads(line)
            except ValueError:
                continue
            keys = {norm_words(e.get("label", ""))} | {norm_words(a) for a in e.get("aliases", []) or []}
            hit = keys & names
            if not hit:
                continue
            sl = int(e.get("sitelinks", 0) or 0)
            fam = e.get("family", "")
            kind = e.get("kind", "")
            for k in hit:
                r = out.setdefault(k, {"char": 0, "generic": 0, "best": 0, "family": ""})
                if fam in CHAR_FAMILIES and kind == "instance":
                    r["char"] = max(r["char"], sl)
                if kind in GENERIC_KINDS or fam in ("taxon", "breed"):
                    r["generic"] = max(r["generic"], sl)
                if sl > r["best"]:
                    r["best"], r["family"] = sl, fam
    return out


def max_powers() -> int:
    m = re.search(r"pub const MAX_POWERS: usize = (\d+);", read(rel("crates", "bd_sim", "src", "powers", "data.rs")))
    return int(m.group(1)) if m else 4000
