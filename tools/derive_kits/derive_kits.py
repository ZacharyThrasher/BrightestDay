#!/usr/bin/env python3
"""DERIVED KITS — a Kit for every character nobody hand-built (docs/toolbox/BRIEF.md, AUTO-KITS).

    python tools/derive_kits/derive_kits.py            # (re)write assets/powers/zz_derived_kits.ron
    python tools/derive_kits/derive_kits.py --check    # exit 1 if the checked-in file is stale
    python tools/derive_kits/derive_kits.py --show "heat wave" [--show kilowog ...]   # print kits
    python tools/derive_kits/derive_kits.py --sample 50 [--seed 7]                   # spot-check
    python tools/derive_kits/derive_kits.py --stats                                  # coverage only

Re-run it whenever hand content lands (any assets/powers/*.ron) or the icon / costume data changes:
a character that any hand-made power or kit already names is SKIPPED, so hand kits always win (the
output file also sorts LAST — `zz_` — so even a stale copy loses every name tie to hand content).

WHAT IT READS (checked-in data only — deterministic on every machine, stdlib Python only):
  assets/icons.ron, assets/icons/*.ron (hand recipes), assets/icons/*_names.txt (scanner hints),
  assets/costumes/characters.ron (weapons on the back, colours), assets/powers/*.ron (claims),
  bd_core's enums and bd_sim's MoveOp list (only ops that exist are used), the costume grammar's
  words (never used in move names: "goku sword" stays a costume), assets/tuning/move_fx.ron.

HOW A KIT IS DERIVED: each character gets a profile (effective element, flavour, weapon, size,
temperament, role scores from its mechanics + name keywords + pack) and every slot (Primary,
Movement, Defense+Counter, Combo, Ultimate, + one bonus move from a signature mechanic) picks the
best-scoring TEMPLATE for that profile, with a stable per-character jitter so two fire brutes don't
get the same kit. Names come from element / flavour / temperament vocabularies (vocab.py) and are
validated so "<name> ultimate / attack / dash / block / combo / counter / special" always finds the
right move and no word steals a costume / icon / shout phrase.
"""

from __future__ import annotations

import argparse
import glob
import os
import random
import re
import sys
import zlib
from dataclasses import dataclass, field

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import sources as S  # noqa: E402
import vocab as V  # noqa: E402

OUT_NAME = "zz_derived_kits.ron"
OUT = S.rel("assets", "powers", OUT_NAME)
PACK_ORDER = ["base", "anime", "dc", "film_tv", "folklore_cn", "games", "internet", "marvel", "military", "pokemon", "shows",
              "tabletop", "tmnt", "transformers", "costume"]

# bd_sim powers/toolbox.rs
SLOT_WORDS = {
    "ultimate": "Ultimate", "ult": "Ultimate", "ulti": "Ultimate", "ultimates": "Ultimate", "super": "Ultimate", "special": "Ultimate",
    "final": "Ultimate", "finisher": "Ultimate", "finishing": "Ultimate", "fatality": "Ultimate",
    "combo": "Combo", "combos": "Combo", "string": "Combo", "flurry": "Combo", "rush": "Combo", "barrage": "Combo",
    "attack": "Primary", "punch": "Primary", "hit": "Primary", "strike": "Primary", "basic": "Primary", "primary": "Primary", "light": "Primary",
    "dash": "Movement", "teleport": "Movement", "tp": "Movement", "port": "Movement", "jump": "Movement", "leap": "Movement",
    "blink": "Movement", "dodge": "Movement", "mobility": "Movement", "movement": "Movement",
    "block": "Defense", "shield": "Defense", "guard": "Defense", "parry": "Defense", "defense": "Defense", "defence": "Defense", "barrier": "Defense",
    "counter": "Counter", "reversal": "Counter", "counterattack": "Counter", "riposte": "Counter",
}
FILLER = {"use", "uses", "do", "does", "the", "his", "her", "their", "its", "a", "an", "my", "s"}


def h32(*parts: str) -> int:
    return zlib.crc32("\x1f".join(parts).encode("utf-8")) & 0xFFFFFFFF


def fmt(x: float) -> str:
    """Compact float literal: one decimal from 1 up, two below (RON needs the point for f32)."""
    x = float(x)
    x = round(x, 1) if abs(x) >= 1 else round(x, 2)
    s = f"{x:.2f}".rstrip("0")
    return s + "0" if s.endswith(".") else s


# ============================================================================ phrase matching (port of toolbox.rs)

def within_one(a: str, b: str) -> bool:
    if a == b:
        return True
    s, l = (a, b) if len(a) <= len(b) else (b, a)
    if len(l) - len(s) > 1:
        return False
    i = 0
    while i < len(s) and s[i] == l[i]:
        i += 1
    return s[i + 1:] == l[i + 1:] if len(s) == len(l) else s[i:] == l[i + 1:]


def word_hits(w: str, m: str) -> bool:
    return w == m or (len(w) >= 4 and m.startswith(w)) or (len(m) >= 4 and w.startswith(m)) or (len(w) >= 5 and len(m) >= 5 and within_one(w, m))


def kitform(s: str) -> str:
    """How a phrase's words reach toolbox.rs kit matching (normalised, possessives stripped)."""
    return " ".join(strip_possessive(w) for w in S.norm_words(s).split(" ") if w)


def strip_possessive(w: str) -> str:
    for suf in ("'s", "s'", "'"):
        if w.endswith(suf):
            return w[: -len(suf)]
    return w


class KitIndex:
    """toolbox.rs MoveIndex::kit_move over (kit name → kit) for hand + derived kits."""

    def __init__(self, kits: list[dict]):
        # kits: {pid, names, moves: [{name, tags}], shouts: [(s, slot)], derived}
        self.kits = kits
        pairs = []
        for k in kits:
            for n in k["names"]:
                pairs.append((n, k["pid"]))
        pairs = sorted(set(pairs), key=lambda p: (-len(p[0]), p[0], p[1]))
        self.pairs = pairs
        self.by_pid = {k["pid"]: k for k in kits}
        self.first = {}
        for n, pid in pairs:
            self.first.setdefault(n.split(" ")[0], []).append((n, pid))

    def kit_move(self, n: str):
        words = [strip_possessive(w) for w in n.split(" ") if w]
        if len(words) < 2:
            return None
        cands = []
        for key in {words[0], words[0][:-1] if words[0].endswith("s") else words[0]}:
            cands += self.first.get(key, [])
        cands.sort(key=lambda p: (-len(p[0]), p[0], p[1]))
        for kit, pid in cands:
            kw = kit.split(" ")
            if len(kw) >= len(words):
                continue
            fits = True
            for i, w in enumerate(kw):
                last = i + 1 == len(kw)
                if w != words[i] and not (last and len(words[i]) == len(w) + 1 and words[i].endswith("s") and words[i].startswith(w)):
                    fits = False
                    break
            if not fits:
                continue
            hit = pick_kit_move(self.by_pid[pid], kit, words[len(kw):])
            return (pid, hit[0], hit[1], kit) if hit else None
        return None


def pick_kit_move(k: dict, kit: str, rest: list[str]):
    moves = k["moves"]
    if not moves:
        return None
    rest = [w for w in rest if w not in FILLER]
    if not rest:
        return None
    joined = " ".join(rest)
    for i, m in enumerate(moves):
        if m["name"] == joined:
            return (i, "exact")
    for s, slot in k.get("shouts", []):
        if slot < len(moves) and (s == joined or s == f"{kit} {joined}"):
            return (slot, "exact")
    b = best_overlap(k, rest, True)
    if b is not None:
        return (b, "fuzzy")
    tag = next((SLOT_WORDS[w] for w in rest if w in SLOT_WORDS), None)
    if tag:
        for i, m in enumerate(moves):
            if tag in m["tags"]:
                return (i, "tag")
    b = best_overlap(k, rest, False)
    if b is not None:
        return (b, "fuzzy")
    if tag:
        if tag == "Primary":
            return (0, "fallback")
        if tag == "Ultimate" and len(moves) > 1:
            return (len(moves) - 1, "fallback")
    return None


def move_words(k: dict, i: int) -> list[str]:
    words = k["moves"][i]["name"].split(" ")
    for s, slot in k.get("shouts", []):
        if slot == i:
            words += s.split(" ")
    return words


def best_overlap(k: dict, rest: list[str], whole: bool):
    best = None
    for i in range(len(k["moves"])):
        mw = move_words(k, i)
        s = sum(1 for w in rest if len(w) >= 3 and any(len(m) >= 3 and (w == m if whole else word_hits(w, m)) for m in mw))
        if s > 0 and (best is None or s > best[0]):
            best = (s, i)
    return None if best is None else best[1]


# ============================================================================ characters

def load_characters(en: S.Enums) -> list[S.Char]:
    chars: list[S.Char] = []
    icon_paths = [("base", S.rel("assets", "icons.ron"))] + [(os.path.basename(p)[:-4], p) for p in sorted(glob.glob(S.rel("assets", "icons", "*.ron")))]
    for pack, p in icon_paths:
        chars += S.parse_icon_file(p, pack, en, len(chars))
    names_paths = [(os.path.basename(p)[: -len("_names.txt")], p) for p in sorted(glob.glob(S.rel("assets", "icons", "*_names.txt")))]
    listed: list[S.Char] = []
    for pack, p in names_paths:
        listed += S.parse_names_file(p, pack, en, 100000 + len(listed))
    costumes = S.parse_costumes(en, 200000)

    by_alias: dict[str, S.Char] = {}
    out: list[S.Char] = []

    def adopt(c: S.Char) -> None:
        out.append(c)
        for n in c.names:
            by_alias.setdefault(n, c)

    for c in chars:
        if c.key in by_alias:  # same character in two icon packs: first keeps it, aliases merge
            host = by_alias[c.key]
            host.names += [n for n in c.names if n not in host.names]
            for n in c.names:
                by_alias.setdefault(n, host)
            continue
        adopt(c)
    for c in listed:
        host = next((by_alias[n] for n in c.names if n in by_alias), None)
        if host is not None:
            host.names += [n for n in c.names if n not in host.names and n not in by_alias]
            for n in c.names:
                by_alias.setdefault(n, host)
            if not host.notes:
                host.notes = c.notes
            continue
        adopt(c)
    for c in costumes:
        host = next((by_alias[n] for n in c.names if n in by_alias), None)
        vw = getattr(c, "variant_words", [])
        if host is not None:
            host.names += [n for n in c.names if n not in host.names and n not in by_alias]
            for n in c.names:
                by_alias.setdefault(n, host)
            host.weapons |= c.weapons
            host.colors = host.colors or c.colors
            host.variant_words = getattr(host, "variant_words", []) + vw  # type: ignore[attr-defined]
            if not host.source:
                host.source = c.source
            continue
        c.variant_words = vw  # type: ignore[attr-defined]
        adopt(c)
    return out


# ============================================================================ profile

def tokens(text: str) -> list[str]:
    return re.findall(r"[a-z0-9][a-z0-9'\-]*", text.lower())


def kw_score(words: list[str], toks: set[str], text: str) -> int:
    """Whole-word hits (plural -s too); a word ending in `*` is a stem ("regenerat*")."""
    n = 0
    for w in words:
        if " " in w or "-" in w:
            if re.search(r"(^|\W)" + re.escape(w) + r"(\W|$)", text):
                n += 1
        elif w.endswith("*"):
            if any(t.startswith(w[:-1]) for t in toks):
                n += 1
        elif w in toks or (w + "s") in toks:
            n += 1
    return n


@dataclass
class Prof:
    elem: str
    elem2: str | None
    flav: str | None
    weapon: str | None
    size: float
    beast: bool
    humanoid: bool
    heavy: bool
    small: bool
    vehicle: str | None
    r: dict[str, float] = field(default_factory=dict)
    kw: dict[str, int] = field(default_factory=dict)


BEAST_FORMS = {"Quadruped", "Bird", "Serpent", "Fish", "Insect", "Arachnid"}


def profile(c: S.Char) -> Prof:
    m = c.mech
    titles = getattr(c, "titles", set())
    name_text = " ".join([n for n in c.names if n not in titles] + [c.display.lower()])
    # names-list notes are descriptions ("[radiates kryptonite]"); an icon / costume "note" is its franchise
    note_text = (c.notes or "").lower() if c.origin == "names" else ""
    if note_text and (is_title(S.norm_words(note_text), FRANCHISE_TITLES) or S.norm_words(note_text) in SHARED_NOTES):
        note_text = ""
    ntok, otok = set(tokens(name_text)), set(tokens(note_text))

    def kw(words: list[str]) -> float:
        return 2.0 * kw_score(words, ntok, name_text) + 1.0 * kw_score(words, otok, note_text)

    # element: the recipe's, or (Light) the one the name / notes speak of
    elem = c.element
    elem2 = None
    ek = sorted(((kw(ws), e) for e, ws in V.ELEMENT_WORDS.items()), key=lambda x: (-x[0], x[1]))
    if ek and ek[0][0] > 0:
        if elem == "Light":
            elem = ek[0][1]
        elif ek[0][1] != elem:
            elem2 = ek[0][1]
    if c.payload and c.payload[2] not in ("Light", elem):
        elem2 = elem2 or c.payload[2]
    # flavour of a Light character (words, then shape of the data)
    fk = {f: kw(ws) for f, ws in V.FLAVOUR_WORDS.items()}
    mount = "Piloted" in m and c.form in BEAST_FORMS  # a ridden beast (pegasus, chocobo) is not a machine
    if "Transforms" in m or ("Piloted" in m and not mount) or c.features & {"Jet", "Turbine", "Cockpit", "Armor", "Gears", "Antenna", "Turret", "Barrels"}:
        fk["tech"] = fk.get("tech", 0) + 2
    if "Fast" in m and "Heavy" not in m:
        fk["speed"] = fk.get("speed", 0) + 1.5
    if c.pack == "internet":
        fk["glitch"] = fk.get("glitch", 0) + 0.5
    if c.pack == "folklore_cn":
        fk["chi"] = fk.get("chi", 0) + 1.5
    flav = None
    best = sorted(fk.items(), key=lambda x: (-x[1], x[0]))
    if best and best[0][1] >= 1.5:
        flav = best[0][0]
    # weapon
    weapon = None
    wk = {w: kw(ws) for w, ws in V.WEAPON_WORDS.items()}
    for cw in c.weapons:
        cls = V.COSTUME_WEAPON.get(cw)
        if cls:
            wk[cls] = wk.get(cls, 0) + 3
    if c.payload and c.payload[1] in V.PAYLOAD_WEAPON:
        wk[V.PAYLOAD_WEAPON[c.payload[1]]] = wk.get(V.PAYLOAD_WEAPON[c.payload[1]], 0) + 2
    if "Blade" in m:
        wk["sword"] = wk.get("sword", 0) + 1
    if "Pierce" in m:
        wk["spear"] = wk.get("spear", 0) + 0.5
    if "Blunt" in m:
        wk["fists"] = wk.get("fists", 0) + 0.5
    wbest = sorted(wk.items(), key=lambda x: (-x[1], x[0]))
    if wbest and wbest[0][1] >= 1.0:
        weapon = wbest[0][0]
    if weapon == "claws" and c.form not in BEAST_FORMS and c.form != "Kaiju" and "Blade" not in m and not (c.weapons & {"claws"}):
        # "cat" in a name makes a catburglar, not a clawed beast — unless the data says claws
        weapon = None if wbest[0][1] < 3 else weapon

    heavy = "Heavy" in m or c.form == "Kaiju" or c.mass >= 250 or c.scale >= 4.0
    small = c.scale < 1.2 or c.form == "Insect"
    beast = c.form in BEAST_FORMS or (c.form == "Kaiju" and not {"Shoots", "Beam", "Piloted"} & m and flav != "tech"
                                       and kw(V.ROLE_WORDS["knight"]) == 0)
    humanoid = c.form == "Biped"
    size = 1.25 if c.form == "Kaiju" else 1.12 if heavy else 0.85 if small else 1.0
    vehicle = None
    if c.form == "Vehicle" or ("Transforms" in m and c.pack == "transformers"):  # a shapeshifter is not a car
        t = note_text + " " + name_text
        if re.search(r"jet|plane|eagle|falcon|concorde|helicopter|chopper|bomber|fighter|shuttle|f-\d|mig|harrier|seeker|aerial", t) or "Flies" in m:
            vehicle = "jet"
        elif re.search(r"tank|artillery|cannon|howitzer|missile", t):
            vehicle = "tank"
        elif re.search(r"dino|t-rex|rex|stego|ptera|raptor|beast|wolf|cat|lion|gorilla|rhino|bat|scorpion|spider|insect|fuzor|dragon", t) or c.form in BEAST_FORMS:
            vehicle = "beast"
        else:
            vehicle = "car"

    kwr = {k: int(kw(ws)) for k, ws in V.ROLE_WORDS.items()}
    melee = bool(m & {"Blade", "Blunt", "Pierce", "Grab", "Constrict", "Charge", "Stomp", "Spin"}) or weapon in ("sword", "axe", "hammer", "spear", "claws", "fists", "knives", "whip")
    r = {
        "speed": 3 * ("Fast" in m) + 1.5 * (flav == "speed") - 3 * heavy,
        "brute": 3 * heavy + 2 * ("Stomp" in m) + 1 * ("Blunt" in m) + 1 * ("Charge" in m) + 1.5 * min(kwr["brute"], 2) + ("Grow" in m),
        "blade": 3 * ("Blade" in m) + 3 * (weapon in ("sword", "axe", "knives")) + 2 * (weapon == "spear") + ("Pierce" in m),
        "blast": 2 * ("Shoots" in m) + 2 * ("Beam" in m) + 3 * (weapon in ("gun", "bow")) + ("Explodes" in m) + ("Homing" in m) + 1.5 * min(kwr["gunner"], 2),
        "mage": 2 * (elem == "Psychic") + 2 * ("Portal" in m) + ("Homing" in m) + 2 * (weapon == "staff") + 1.5 * min(kwr["mage"], 2)
        + ("Shoots" in m and not melee) + 2 * (flav in ("mystic", "holy", "spirit")),
        "grab": 2 * ("Grab" in m) + 2 * ("Constrict" in m) + ("Pull" in m) + ("Link" in m) + 2 * (weapon == "whip") + ("Sticky" in m),
        "fly": 2 * ("Flies" in m) + ("Glide" in m) + ("Thrust" in m) + ("SlowFall" in m) + (c.form == "Bird"),
        "sneak": 2 * (c.temperament == "Sneaky") + (elem == "Shadow") + 1.5 * min(kwr["sneak"], 2),
        "guard": 2 * ("Guard" in m) + 2 * ("Barrier" in m) + (c.temperament == "Protective") + 3 * (weapon == "shield") + ("Armor" in c.features),
        "beast": 3 * beast,
        "trick": 2 * (c.temperament == "Chaotic") + (c.temperament == "Playful") + ("Bounce" in m) + ("Spin" in m) + 1.5 * min(kwr["trick"], 2),
        "swarm": 3 * ("Swarm" in m),
        "civ": 2 * (c.temperament == "Fearful") + 1.5 * min(kwr["civilian"], 2) + (not m) * 1.0 - 2 * melee,
        "tech": 3 * (flav == "tech") + ("Piloted" in m),
    }
    return Prof(elem=elem, elem2=elem2, flav=flav, weapon=weapon, size=size, beast=beast, humanoid=humanoid, heavy=heavy,
                small=small, vehicle=vehicle, r=r, kw=kwr)


# ============================================================================ moves

@dataclass
class Move:
    names: list[str]              # preferred first; the first one that validates is used
    ops: list[str]
    tags: list[str] = field(default_factory=list)
    cd: float = 1.0
    cost: float = 3.0
    fx: str = ""
    juice: str | None = None
    family: str = ""
    crit: str | None = None
    name: str = ""
    tpl: str = ""


class Ctx:
    def __init__(self, c: S.Char, p: Prof, en: S.Enums):
        self.c, self.p, self.en = c, p, en
        self.m = c.mech
        self.E = p.elem
        self.ev = V.ELEM.get(p.elem)
        self.fv = V.FLAVOUR.get(p.flav) if p.flav else None
        self.used_adj: set[str] = set()
        self.agg = {"Aggressive": 1.1, "Chaotic": 1.05, "Calm": 1.0, "Protective": 0.95, "Lazy": 1.05, "Fearful": 0.9,
                    "Sneaky": 1.0, "Playful": 0.95}.get(c.temperament, 1.0)
        self.cdm = {"Aggressive": 0.9, "Lazy": 1.2, "Fearful": 1.05, "Playful": 0.95}.get(c.temperament, 1.0)

    def has(self, *ops: str) -> bool:
        return all(o in self.en.ops for o in ops)

    def rnd(self, salt: str, n: int) -> int:
        return h32(self.c.key, salt) % max(n, 1)

    def pick(self, salt: str, xs: list[str]) -> str:
        return xs[self.rnd(salt, len(xs))]

    # numbers: damage / knockback scale with size and temperament; radii with size
    def d(self, x: float) -> str:
        return fmt(x * self.agg * (0.92 + 0.08 * self.p.size))

    def kb(self, x: float) -> str:
        return fmt(x * self.p.size * (1.05 if self.c.temperament == "Aggressive" else 1.0))

    def rad(self, x: float) -> str:
        return fmt(x * (0.85 + 0.15 * self.p.size))

    def el(self) -> str:
        return "" if self.E == "Light" else f", element: {self.E}"

    def st(self):
        return self.ev["status"] if self.ev else None

    def adjs(self, physical: bool) -> list[str]:
        if not physical and self.ev:
            base = list(self.ev["adj"])
        elif not physical and self.fv:
            base = list(self.fv["adj"])
        else:
            base = list(V.TEMPER_ADJ.get(self.c.temperament, V.TEMPER_ADJ["Calm"]))
            if self.p.beast:
                base = ["feral", "savage", "primal", "wild", "ravening"] if self.c.temperament in ("Aggressive", "Chaotic", "Calm") else base
            if self.ev and self.p.elem != "Light":
                base = list(self.ev["adj"]) + base
        k = self.rnd("adj", len(base))
        return base[k:] + base[:k]

    def adj(self, physical: bool = True) -> list[str]:
        """Adjectives in preference order, the ones not yet used in this kit first."""
        a = self.adjs(physical)
        return [x for x in a if x not in self.used_adj] + [x for x in a if x in self.used_adj]

    def vocab(self, key: str) -> list[str]:
        if self.ev:
            v = self.ev[key]
        elif self.fv:
            v = self.fv[key]
        else:
            v = {"bolt": ["energy blast", "force bolt"], "ray": ["energy beam", "force beam"], "storm": ["all out assault", "last resort", "full force"],
                 "ward": ["force field", "iron will"], "step": ["quick step", "sidestep"]}[key]
        v = list(v)
        k = self.rnd("v" + key, len(v))
        return v[k:] + v[:k]

    def color(self) -> str | None:
        rgb = self.c.accent or (self.c.colors[1] if len(self.c.colors) > 1 else self.c.colors[0] if self.c.colors else None)
        if rgb is None and self.ev:
            rgb = self.ev["color"]
        if rgb is None:
            return None
        return f"Some(({fmt(rgb[0])}, {fmt(rgb[1])}, {fmt(rgb[2])}))"


def juice(ctx: Ctx, title: str | None = None, boom=False, punch=0.0, shake=0.0, at: str | None = None, dist=0.0, ripple=0.0,
          debris=0.0, impact: str | None = None, color=False) -> str:
    parts = []
    if title:
        parts.append(f'title: Some("{title.upper()}")')
    if boom:
        parts.append("boom: true")
    if punch:
        parts.append(f"punch: {fmt(punch)}")
    if shake:
        parts.append(f"shake: {fmt(shake)}")
    if at:
        parts.append(f"at: {at}")
    if dist:
        parts.append(f"dist: {fmt(dist)}")
    if ripple:
        parts.append(f"ripple: {fmt(ripple)}")
    if debris:
        parts.append(f"debris: {fmt(debris)}")
    col = ctx.color() if color else None
    if col:
        parts.append(f"color: {col}")
    if impact:
        parts.append(f'impact_frame: Some("{impact}")')
    return f"Some(({', '.join(parts)}))" if parts else ""


def recipe(name: str, form: str, mech: list[str], elem: str, scale: float, mass: float = 8.0, count: int = 1, extra: str = "") -> str:
    parts = [f'name: "{name}"', f"form: {form}"]
    if mech:
        parts.append(f"mech: [{', '.join(sorted(set(mech)))}]")
    if elem != "Light":
        parts.append(f"element: {elem}")
    parts.append(f"scale: {fmt(scale)}")
    parts.append(f"mass: {fmt(mass)}")
    if count > 1:
        parts.append(f"count: {count}")
    if extra:
        parts.append(extra)
    return f"({', '.join(parts)})"


# ---------------------------------------------------------------------------- PRIMARY templates
# Each template: (family, score(ctx) -> float, build(ctx) -> Move). Score <= 0 = not applicable.

def t_slash(ctx: Ctx) -> Move:
    w = ctx.p.weapon
    a = ctx.adj(ctx.E == "Light")
    noun = {"axe": ["cleave", "hew"], "knives": ["knife fan", "quick cut"], "spear": ["sweep", "arc"]}.get(w or "", ["slash", "cut", "edge"])
    names = [f"{a[0]} {noun[0]}", f"{a[1]} {noun[-1]}", f"{a[2]} {noun[0]}"]
    heavy = w == "axe" or ctx.p.heavy
    ops = [f"Impulse(fwd: {fmt(5 if not heavy else 3)}, up: 0.0)",
           f"Shove(range: {ctx.rad(4.2 if heavy else 3.6)}, angle: {fmt(110 if heavy else 80)}, damage: {ctx.d(13 if heavy else 10)}, knockback: {ctx.kb(15 if heavy else 10)}{ctx.el()})"]
    return Move(names, ops, cd=1.1 if heavy else 0.8, cost=3.0, family="slash")


def t_thrust(ctx: Ctx) -> Move:
    a = ctx.adj(ctx.E == "Light")
    names = [f"{a[0]} thrust", f"{a[1]} lunge", f"piercing {('thrust' if a[0] != 'piercing' else 'lunge')}"]
    ops = ["Impulse(fwd: 9.0, up: 0.0)",
           f"Shove(range: {ctx.rad(5.4)}, angle: 22.0, damage: {ctx.d(12)}, knockback: {ctx.kb(14)}{ctx.el()})"]
    return Move(names, ops, cd=0.9, cost=3.0, family="thrust")


def t_smash(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    noun = ["smash", "haymaker", "crush"] if ctx.p.weapon != "hammer" else ["smash", "crush", "pound"]
    if ctx.p.beast:
        noun = ["maul", "headbutt", "crush"]
    names = [f"{a[0]} {noun[0]}", f"{a[1]} {noun[1]}", f"{a[2]} {noun[2]}"]
    ops = ["Impulse(fwd: 4.0, up: 0.0)",
           f"Shove(range: {ctx.rad(3.8)}, angle: 90.0, damage: {ctx.d(12)}, knockback: {ctx.kb(16)}, up: 0.25{ctx.el()})"]
    return Move(names, ops, cd=1.0, cost=3.0, family="smash")


BRAWL = {
    # style: (names, lunge fwd, lunge up, range, angle, damage, knockback, up, cooldown)
    "jab": (["{a} jab", "one two", "quick jab"], 5.0, 0.0, 3.0, 50.0, 7.0, 9.0, 0.0, 0.5),
    "haymaker": (["{a} haymaker", "haymaker", "wind up swing"], 6.0, 0.0, 3.4, 70.0, 12.0, 18.0, 0.15, 1.1),
    "kick": (["{a} kick", "roundhouse", "spin kick"], 4.0, 0.0, 3.8, 40.0, 10.0, 13.0, 0.25, 0.8),
    "flying kick": (["flying kick", "{a} flying kick", "leaping kick"], 9.0, 3.0, 3.4, 40.0, 10.0, 14.0, 0.1, 0.9),
    "palm": (["palm thrust", "{a} palm", "open palm"], 3.0, 0.0, 3.4, 40.0, 9.0, 20.0, 0.1, 0.9),
    "headbutt": (["headbutt", "{a} headbutt", "skull bash"], 6.0, 0.0, 2.8, 40.0, 11.0, 14.0, 0.2, 1.0),
    "slap": (["{a} slap", "wild swing", "flailing swing"], 4.0, 0.0, 3.0, 80.0, 6.0, 14.0, 0.1, 0.6),
    "elbow": (["{a} elbow", "elbow smash", "spinning elbow"], 5.0, 0.0, 2.8, 60.0, 11.0, 12.0, 0.0, 0.8),
    "punch": (["{a} cross", "straight right", "one two cross"], 5.0, 0.0, 3.2, 60.0, 9.0, 11.0, 0.0, 0.7),
}


def brawl_style(ctx: Ctx) -> str:
    t = ctx.c.temperament
    pool = {"Aggressive": ["haymaker", "headbutt", "elbow", "punch"], "Calm": ["jab", "palm", "kick", "punch"],
            "Protective": ["punch", "jab", "haymaker"], "Sneaky": ["kick", "elbow", "jab"], "Playful": ["flying kick", "slap", "kick"],
            "Chaotic": ["headbutt", "flying kick", "slap", "haymaker"], "Fearful": ["slap", "jab"], "Lazy": ["haymaker", "slap", "headbutt"]}[t]
    if ctx.p.flav == "chi":
        pool = ["palm", "kick", "flying kick"]
    if ctx.p.weapon == "fists":
        pool = ["punch", "haymaker", "jab"]
    return ctx.pick("brawl", pool)


def t_punch(ctx: Ctx) -> Move:
    a = ctx.adj(ctx.E == "Light")
    style = brawl_style(ctx)
    names0, fwd, up, rng, ang, dmg, kb, lift, cd = BRAWL[style]
    names = [n.format(a=a[k % len(a)]) for k, n in enumerate(names0)] + [f"{a[1]} cross", f"{a[2]} swing"]
    upo = f", up: {fmt(lift)}" if lift else ""
    ops = [f"Impulse(fwd: {fmt(fwd)}, up: {fmt(up)})",
           f"Shove(range: {ctx.rad(rng)}, angle: {fmt(ang)}, damage: {ctx.d(dmg)}, knockback: {ctx.kb(kb)}{upo}{ctx.el()})"]
    return Move(names, ops, cd=cd, cost=2.0, family="punch")


def t_stretch(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = ["rubber fist", f"{a[0]} stretch", "elastic jab"]
    ops = [f"Shove(range: 7.5, angle: 16.0, damage: {ctx.d(10)}, knockback: {ctx.kb(14)}{ctx.el()})"]
    return Move(names, ops, cd=0.8, cost=3.0, fx="rubber", family="stretch")


def t_stomp(ctx: Ctx) -> Move:
    a = ctx.adj(False if ctx.E != "Light" else True)
    names = [f"{a[0]} stomp", f"{a[1]} stomp", "ground pound"]
    ops = [f"Explode(radius: {ctx.rad(4.2)}, damage: {ctx.d(10)}, knockback: {ctx.kb(14)}, carve: 0.8{ctx.el()})"]
    return Move(names, ops, cd=1.3, cost=3.0, fx="earth" if ctx.E == "Light" else "", juice=juice(ctx, shake=0.15, color=False), family="stomp")


def t_bolt(ctx: Ctx) -> Move:
    E = ctx.E
    v = ctx.vocab("bolt")
    if ctx.c.pack == "pokemon":
        v = [V.POKEMON.get(E, V.POKEMON["Light"])["bolt"]] + v
    if E == "Electric":
        ops = [f"Explode(radius: 2.6, damage: {ctx.d(10)}, knockback: {ctx.kb(12)}, element: Electric, at: Aim)"]
    elif E == "Psychic":
        ops = [f"Afflict(range: 30.0, cone: 10.0, status: Some(Float), secs: 1.0, damage: {ctx.d(8)}, up: 5.0, element: Psychic)"]
        v = ["telekinetic lift", "mind grip"] + v if ctx.c.pack != "pokemon" else v
    elif E in ("Atomic", "Water") and ctx.has("BounceBeam"):
        ops = [f"BounceBeam(length: 28.0, radius: 0.45, damage: {ctx.d(11)}, knockback: {ctx.kb(14)}, element: {E})"]
    else:
        form, mech, scale, mass, count = {
            "Fire": ("Ball", ["Explodes", "Fast"], 0.55, 6.0, 1), "Ice": ("Spear", ["Pierce", "Fast"], 0.6, 5.0, 1),
            "Wind": ("Disc", ["Blade", "Fast"], 0.6, 4.0, 1), "Earth": ("Boulder", ["Heavy"], 0.8, 40.0, 1),
            "Poison": ("Blob", ["Sticky"], 0.4, 4.0, 1), "Shadow": ("Ball", ["Homing"], 0.5, 5.0, 1),
        }.get(E, ("Ball", ["Fast", "Swarm"], 0.25, 3.0, 3) if ctx.p.flav == "tech" or ctx.p.weapon == "gun" else ("Ball", ["Fast"], 0.45, 5.0, 1))
        if ctx.p.weapon == "bow" and E == "Light":
            form, mech, scale, mass, count = "Spear", ["Pierce", "Fast"] + (["Homing"] if "Homing" in ctx.m else []), 0.55, 3.0, 1
            v = ["quick shot", "piercing shot", "true shot"]
        if ctx.p.weapon == "gun" and E == "Light":
            v = ["blaster shot", "rapid fire", "quick draw"] if ctx.p.flav != "tech" else v
        if ctx.p.weapon == "knives" and E == "Light":
            form, mech, scale, mass, count = "Disc", ["Blade", "Fast"], 0.3, 1.0, 3
            v = ["knife toss", "throwing stars", "fan of knives"]
        ops = [f"Cast(recipe: {recipe(v[0], form, mech, E, scale, mass, count)})"]
    return Move(v, ops, cd=0.8, cost=2.0, family="bolt")


def t_beam(ctx: Ctx) -> Move:
    v = ctx.vocab("ray")
    a = ctx.adj(False)
    if ctx.E == "Light" and ctx.p.humanoid and not ctx.fv:
        v = ["optic blast", "eye beams"] + v
    bounces = 2 if "Ricochet" in ctx.m else 0
    ops = [f"BounceBeam(length: 34.0, bounces: {bounces}, radius: 0.45, damage: {ctx.d(12)}, knockback: {ctx.kb(12)}{ctx.el()})"]
    return Move(v + [f"{a[0]} beam"], ops, cd=1.1, cost=3.0, family="beam")


def t_breath(ctx: Ctx) -> Move:
    v = ctx.ev["breath"] if ctx.ev else ["roar", "bellow"]
    v = list(v)
    st = ctx.st()
    ops = [f"Shove(range: {ctx.rad(8)}, angle: 32.0, damage: {ctx.d(9)}, knockback: {ctx.kb(11)}{ctx.el()})"]
    if st:
        ops.append(f"Zone(radius: {ctx.rad(3.5)}, secs: 2.0, status: Some({st}), at: Front, dist: 5.0)")
    return Move(v, ops, cd=1.3, cost=3.0, family="breath")


def t_bite(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = [f"{a[0]} bite", "lunging bite", f"{a[1]} fangs"]
    if ctx.c.form in ("Bird",):
        names = [f"{a[0]} peck", "diving talon", f"{a[1]} swoop"]
    if ctx.p.weapon == "claws":
        names = [f"{a[0]} swipe", "rending swipe", f"{a[1]} rake"]
    ops = [f"Pounce(range: 12.0, speed: 36.0, cone: 12.0, damage: {ctx.d(9)}, knockback: {ctx.kb(10)}, bounce: 3.0{ctx.el()})"]
    return Move(names, ops, cd=0.9, cost=3.0, family="bite")


def t_whip(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = [f"{a[0]} lash", "crack of the lash", f"{a[1]} snare"]
    if ctx.c.form in ("Arachnid",) or "Sticky" in ctx.m:
        names = ["web shot", f"{a[0]} web", "sticky strand"]
    ops = [f"Hook(range: 16.0, speed: 70.0, damage: {ctx.d(8)}, status: Some(Stun), secs: 0.25)"]
    return Move(names, ops, cd=1.0, cost=3.0, family="whip")


def t_spin(ctx: Ctx) -> Move:
    a = ctx.adj(ctx.E == "Light")
    names = [f"{a[0]} spin", "whirlwind", f"{a[1]} cyclone"]
    ops = [f"Shove(range: {ctx.rad(3.6)}, angle: 179.0, damage: {ctx.d(9)}, knockback: {ctx.kb(12)}{ctx.el()})"]
    return Move(names, ops, cd=0.9, cost=3.0, family="spin")


def t_hex(ctx: Ctx) -> Move:
    a = ctx.adj(False)
    v = [f"{a[0]} hex", f"{a[1]} curse"] if not ctx.ev else ctx.vocab("bolt")
    st = ctx.st() or "Stun"
    secs = "0.5" if st == "Stun" else "1.4"
    ops = [f"Afflict(range: 30.0, cone: 10.0, status: Some({st}), secs: {secs}, damage: {ctx.d(8)}{ctx.el()})"]
    return Move(v, ops, cd=1.0, cost=3.0, family="hex")


PRIMARY = [
    ("stretch", lambda x: 9 * (x.kwr("stretch") > 0 or "Extends" in x.m), t_stretch),
    ("slash", lambda x: 2 * ("Blade" in x.m) + 3 * (x.p.weapon in ("sword", "axe", "knives")) + x.r("blade") * 0.3, t_slash),
    ("thrust", lambda x: 4 * (x.p.weapon == "spear") + 1.5 * ("Pierce" in x.m and x.p.humanoid), t_thrust),
    ("smash", lambda x: 2 * ("Blunt" in x.m) + 3 * (x.p.weapon == "hammer") + (x.p.heavy and x.p.humanoid) * 1.5, t_smash),
    ("stomp", lambda x: 3 * ("Stomp" in x.m) + 1 * (x.c.form == "Kaiju"), t_stomp),
    ("bolt", lambda x: 2.5 * ("Shoots" in x.m) + 3 * (x.p.weapon in ("gun", "bow")) + 2 * (x.p.weapon == "knives" and "Blade" not in x.m)
     + 2.2 * (x.E != "Light" and not x.p.beast) + (x.p.flav in ("tech", "mystic", "holy")) + 2 * (x.c.pack == "pokemon"), t_bolt),
    ("beam", lambda x: 3.2 * ("Beam" in x.m) - 1 * (x.c.pack == "pokemon"), t_beam),
    ("breath", lambda x: 5 * ("Breath" in x.m), t_breath),
    ("bite", lambda x: 2.4 * x.p.beast + 1.5 * (x.p.weapon == "claws" and x.p.beast) + 0.6 * (x.p.beast and "Shoots" not in x.m), t_bite),
    ("whip", lambda x: 4 * (x.p.weapon == "whip") + 2.5 * (x.c.form == "Arachnid" or ("Sticky" in x.m and "Pull" in x.m)), t_whip),
    ("spin", lambda x: 3 * ("Spin" in x.m), t_spin),
    ("hex", lambda x: 1.5 * x.r("mage") - 1 * ("Shoots" in x.m), t_hex),
    ("punch", lambda x: 0.8 + 1 * x.p.humanoid + 1 * (x.p.weapon == "fists"), t_punch),
]


# ---------------------------------------------------------------------------- MOVEMENT templates

def m_blur(ctx: Ctx) -> Move:
    v = V.FLAVOUR["speed"]["step"] if ctx.E == "Light" else [f"{ctx.adj(False)[0]} surge"]
    names = list(v) + ["blur", "afterimage"]
    ops = ["Blink(dist: 12.0, afterimage: true)"]
    return Move(names, ops, cd=2.5, cost=3.0, fx="speedforce" if ctx.E in ("Light", "Electric") else "", family="blink")


def m_streak(ctx: Ctx) -> Move:
    a = ctx.adj(False)
    names = [f"{a[0]} streak", "blitz", f"{a[1]} streak"]
    ops = [f"Ram(speed: 36.0, secs: 0.35, damage: {ctx.d(6)}, knockback: {ctx.kb(10)}, through: true, keep: 0.4{ctx.el()})"]
    return Move(names, ops, cd=3.0, cost=4.0, fx="speedforce" if ctx.E in ("Light", "Electric") else "", family="streak")


def m_teleport(ctx: Ctx) -> Move:
    v = ctx.vocab("step")
    fx = ""
    if "Portal" in ctx.m:
        v = ["portal hop", "portal step"] + v
    if ctx.p.r["sneak"] >= 2 and ctx.E in ("Light", "Shadow"):
        v = ["vanish", "shadow step"] + v
        fx = "smoke"
    return Move(v, ["Blink(dist: 14.0)"], cd=3.0, cost=3.0, fx=fx, family="blink")


def m_ambush(ctx: Ctx) -> Move:
    names = ["ambush", "backstab step", "from the shadows"]
    return Move(names, ["WarpTo(range: 30.0, cone: 12.0, behind: 2.0)"], cd=5.0, cost=4.0, fx="smoke" if ctx.E == "Light" else "", family="warp")


def m_fly(ctx: Ctx) -> Move:
    if ctx.c.form == "Bird" or "Glide" in ctx.m:
        names = ["swoop", "take wing", "sky dive"]
        ops = ["Impulse(fwd: 18.0, up: 6.0)", "SelfStatus(status: Float, secs: 0.8)"]
    else:
        names = ["take flight", "sky surge", "soar"]
        ops = ["Impulse(fwd: 14.0, up: 12.0)", "SelfStatus(status: Float, secs: 1.2)"]
    if ctx.c.pack == "pokemon":
        names = ["fly"] + names
    return Move(names, ops, cd=3.0, cost=3.0, family="fly")


def m_boost(ctx: Ctx) -> Move:
    names = ["thruster boost", "rocket boost", "afterburner"]
    return Move(names, ["Impulse(fwd: 20.0, up: 5.0)"], cd=2.5, cost=3.0, fx="hardlight" if ctx.E == "Light" else "", family="boost")


def m_leap(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = ["ground slam", "crater drop", f"{a[0]} bound"]
    ops = ["Leap(range: 24.0, apex: 8.0)", f"SlamOnLand(radius: {ctx.rad(4.2)}, damage: {ctx.d(10)}, knockback: {ctx.kb(14)}, carve: 1.0)"]
    return Move(names, ops, cd=4.0, cost=4.0, fx="earth" if ctx.E == "Light" else "", family="leap")


def m_charge(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = [f"{a[0]} charge", "bull charge", "stampede charge"] if ctx.p.beast else [f"{a[0]} charge", "shoulder charge", "bulldozer"]
    ops = [f"Ram(speed: 24.0, secs: 0.7, damage: {ctx.d(9)}, knockback: {ctx.kb(16)}, up: 0.2, through: true, keep: 0.5{ctx.el()})"]
    return Move(names, ops, cd=3.5, cost=4.0, family="charge")


def m_pounce(ctx: Ctx) -> Move:
    names = ["pounce", "spring", "leaping pounce"]
    ops = [f"Pounce(range: 20.0, speed: 40.0, cone: 12.0, damage: {ctx.d(7)}, knockback: {ctx.kb(8)}, bounce: 7.0)"]
    return Move(names, ops, cd=3.0, cost=3.0, family="pounce")


def m_swing(ctx: Ctx) -> Move:
    names = ["web swing", "web zip", "swing"] if ctx.c.form == "Arachnid" or "Sticky" in ctx.m else ["grapple", "grapple line", "zip line"]
    return Move(names, ["Reel(range: 26.0, speed: 30.0)"], cd=2.5, cost=3.0, family="reel")


EVASIVE = {
    # style: (names, impulse fwd, impulse up, evade secs (0 = none), blink dist (0 = none))
    "roll": (["combat roll", "tumble", "roll away"], 11.0, 3.0, 0.3, 0.0),
    "slide": (["power slide", "slide", "baseball slide"], 15.0, 0.0, 0.2, 0.0),
    "vault": (["vault", "parkour", "wall run"], 7.0, 11.0, 0.0, 0.0),
    "backflip": (["springback", "reverse somersault", "withdraw"], -9.0, 10.0, 0.35, 0.0),
    "sprint": (["sprint", "burst of speed", "hustle"], 17.0, 1.0, 0.0, 0.0),
    "sidestep": (["sidestep", "quickstep", "shuffle"], 0.0, 0.0, 0.25, 5.0),
    "flee": (["run away", "scramble", "flee"], -12.0, 4.0, 0.3, 0.0),
    "cartwheel": (["cartwheel", "somersault", "handspring"], 10.0, 6.0, 0.25, 0.0),
    "flop": (["lazy roll", "flop", "belly flop"], 8.0, 2.0, 0.3, 0.0),
}


def evasive_style(ctx: Ctx) -> str:
    t = ctx.c.temperament
    pool = {"Fearful": ["flee"], "Playful": ["cartwheel", "backflip", "vault"], "Lazy": ["flop"], "Sneaky": ["sidestep", "roll", "slide"],
            "Aggressive": ["sprint", "slide", "vault"], "Chaotic": ["cartwheel", "slide", "backflip"],
            "Protective": ["roll", "sprint", "sidestep"]}.get(t, ["roll", "vault", "backflip", "sidestep", "slide"])
    if ctx.p.flav == "chi":
        pool = ["backflip", "vault", "sidestep"]
    return ctx.pick("evasive", pool)


def m_roll(ctx: Ctx) -> Move:
    names, fwd, up, ev, blink = EVASIVE[evasive_style(ctx)]
    ops = []
    if blink:
        ops.append(f"Blink(dist: {fmt(blink)}, afterimage: true)")
    if fwd or up:
        ops.append(f"Impulse(fwd: {fmt(fwd)}, up: {fmt(up)})")
    if ev and ctx.has("Evade"):
        ops.append(f"Evade(secs: {fmt(ev)})")
    return Move(list(names), ops, cd=2.5, cost=3.0, family="roll")


def m_burrow(ctx: Ctx) -> Move:
    names = ["burrow", "tunnel", "dig"] if ctx.c.pack != "pokemon" else ["dig", "burrow"]
    return Move(names, ["Blink(dist: 11.0)", f"Explode(radius: 2.5, damage: {ctx.d(6)}, knockback: {ctx.kb(10)}, carve: 0.6, element: Earth)"],
                cd=3.5, cost=4.0, fx="earth", family="burrow")


def m_surf(ctx: Ctx) -> Move:
    v = V.POKEMON["Water"]["step"] if ctx.c.pack == "pokemon" else "riptide surge"
    names = [v, "riptide surge", "wave ride", "undertow"]
    ops = [f"Ram(speed: 26.0, secs: 0.6, damage: {ctx.d(6)}, knockback: {ctx.kb(12)}, through: true, keep: 0.5, element: Water)"]
    return Move(names, ops, cd=3.0, cost=4.0, fx="water", family="surf")


def m_rollout(ctx: Ctx) -> Move:
    names = ["roll out", "floor it", "burn rubber"]
    ops = [f"Ram(speed: 30.0, secs: 0.9, damage: {ctx.d(10)}, knockback: {ctx.kb(18)}, through: true, steer: 40.0, keep: 0.6)"]
    return Move(names, ops, cd=3.5, cost=4.0, fx="hardlight", family="rollout")


def m_bounce(ctx: Ctx) -> Move:
    return Move(["bounce", "spring up", "boing"], ["Impulse(fwd: 8.0, up: 17.0)"], cd=2.5, cost=3.0, fx="rubber", family="bounce")


def m_phase(ctx: Ctx) -> Move:
    return Move(["phase through", "intangible", "fade"], ["Phase(secs: 1.2)", "Impulse(fwd: 12.0, up: 0.0)"], cd=5.0, cost=4.0,
                fx="spirit" if ctx.E == "Light" else "", family="phase")


def m_elem_step(ctx: Ctx) -> Move:
    v = ctx.vocab("step")
    if ctx.c.pack == "pokemon":
        v = [V.POKEMON.get(ctx.E, V.POKEMON["Light"])["step"]] + v
    return Move(v, ["Blink(dist: 12.0, afterimage: true)"], cd=3.0, cost=3.0, family="blink")


MOVEMENT = [
    ("blur", lambda x: 4 * ("Fast" in x.m) + 2 * (x.p.flav == "speed") - 4 * x.p.heavy, m_blur),
    ("streak", lambda x: 2 * ("Fast" in x.m) + 2 * ("Charge" in x.m and "Fast" in x.m) - 3 * x.p.heavy, m_streak),
    ("teleport", lambda x: 5 * ("Portal" in x.m) + 1.5 * x.r("mage") + 1.2 * (x.r("sneak") >= 2) + 2 * (x.E == "Psychic") + 1 * (x.E == "Shadow"), m_teleport),
    ("warp", lambda x: (1.2 * x.r("sneak") + 0.5 * (x.E == "Shadow")) * x.has("WarpTo") - 1, m_ambush),
    ("fly", lambda x: 4 * ("Flies" in x.m) + (x.c.form == "Bird") + ("Glide" in x.m), m_fly),
    ("boost", lambda x: 2.5 * (("Thrust" in x.m) or bool(x.c.features & {"Jet", "Turbine"}) or ("jetpack" in x.c.weapons)) + 1 * (x.p.flav == "tech"), m_boost),
    ("leap", lambda x: 2 * x.p.heavy + 2 * ("Stomp" in x.m) + 1 * (x.r("brute") >= 3) - 1 * x.p.small, m_leap),
    ("charge", lambda x: 3 * ("Charge" in x.m) + 0.5 * x.p.beast - 1 * ("Fast" in x.m), m_charge),
    ("pounce", lambda x: 3 * (x.c.form in ("Quadruped", "Arachnid", "Insect") and not x.p.heavy) + 1 * (x.p.weapon == "claws"), m_pounce),
    ("reel", lambda x: 3 * ("Pull" in x.m or "Link" in x.m) + 3 * (x.c.form == "Arachnid" or "Sticky" in x.m) + 1.5 * (x.p.weapon == "whip"), m_swing),
    ("burrow", lambda x: 2.5 * (x.E == "Earth") + 1 * (x.c.form in ("Serpent", "Insect")) * (x.E == "Earth"), m_burrow),
    ("surf", lambda x: 2.5 * (x.E == "Water") + 1 * (x.c.form in ("Fish", "Serpent")), m_surf),
    ("rollout", lambda x: 6 * (x.p.vehicle == "car") + 3 * (x.p.vehicle == "tank"), m_rollout),
    ("bounce", lambda x: 3 * ("Bounce" in x.m), m_bounce),
    ("phase", lambda x: (3 * (x.p.flav == "spirit") + 1 * ("Shrink" in x.m)) * x.has("Phase"), m_phase),
    ("elemstep", lambda x: 1.2 * (x.E != "Light") + 1 * (x.c.pack == "pokemon"), m_elem_step),
    ("roll", lambda x: 1.0 + 1.5 * (x.c.temperament in ("Fearful", "Playful", "Sneaky", "Lazy")), m_roll),
]


# ---------------------------------------------------------------------------- DEFENSE (+ Counter) templates

def d_parry(ctx: Ctx) -> Move:
    names = ["en garde", "turn the blade", "deflecting stance"]
    ops = [f"Counter(secs: 0.6, damage: {ctx.d(11)}, knockback: {ctx.kb(15)}, up: 0.2{ctx.el()})"]
    return Move(names, ops, cd=6.0, cost=4.0, family="counter")


def d_guard(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = ["bulwark stance"] if ctx.p.weapon == "shield" else []
    if ctx.p.beast:
        names += ["hackles up", "defensive crouch", "bristling stance"]
    names += [f"{a[0]} stance", "iron wall", f"{a[1]} stance"]
    ops = ["Counter(secs: 1.0)"]
    if ctx.has("Deflect"):
        ops.append("Deflect(secs: 1.0, angle: 80.0)")
    return Move(names, ops, cd=6.0, cost=5.0, fx="hardlight" if ctx.E == "Light" else "", family="guard")


def d_deflect(ctx: Ctx) -> Move:
    names = ["deflect", "bullet time", "turn aside"]
    return Move(names, ["Deflect(secs: 1.1, angle: 90.0)", "Counter(secs: 0.3)"], cd=6.0, cost=4.0, family="deflect")


def d_ward(ctx: Ctx) -> Move:
    v = ctx.vocab("ward")
    if ctx.c.pack == "pokemon":
        v = [V.POKEMON.get(ctx.E, V.POKEMON["Light"])["ward"]] + v
    st = ctx.st()
    ops = ["Field(strength: -45.0, radius: 4.5, secs: 1.0)", "Counter(secs: 0.4)"]
    if st and ctx.has("Zone"):
        ops.append(f"Zone(radius: 4.0, secs: 2.5, status: Some({st}), follow: true)")
    return Move(v, ops, cd=7.0, cost=5.0, family="ward")


def d_brace(ctx: Ctx) -> Move:
    names = ["thick hide", "brace", "unmovable"] if ctx.p.beast else ["brace", "unmovable", "hold the line"]
    ops = ["Empower(secs: 3.0, damage: 1.0, armor: 0.5)", "Counter(secs: 0.5)"]
    return Move(names, ops, cd=8.0, cost=5.0, family="brace")


def d_evade(ctx: Ctx) -> Move:
    names = {"Fearful": ["duck and cover", "flinch", "cower"], "Sneaky": ["sixth sense", "slip", "misdirection"]}.get(
        ctx.c.temperament, ["perfect evasion", "sixth sense", "slip"])
    return Move(names, ["Evade(secs: 0.5, slow: 0.6, speed: 1.4, blink: 3.0)"], cd=6.0, cost=4.0, family="evade")


def d_smoke(ctx: Ctx) -> Move:
    names = ["smoke veil", "smoke screen", "vanishing act"]
    ops = ["Zone(radius: 5.0, secs: 3.0, status: Some(Shadow))", "Evade(secs: 0.4)"]
    return Move(names, ops, cd=8.0, cost=5.0, fx="smoke", family="smoke")


def d_heal(ctx: Ctx) -> Move:
    names = ["regenerate", "healing factor", "second wind"] if ctx.kwr("healing_factor") else ["second wind", "mend", "restoration"]
    if ctx.p.flav == "holy":
        names = ["healing glow", "blessing"] + names
    ops = ["Mend(pct: 18.0, per_s: 3.0, secs: 3.0)", "Counter(secs: 0.3)"]
    return Move(names, ops, cd=9.0, cost=5.0, fx="holy" if ctx.E == "Light" else "", family="heal")


def d_shell(ctx: Ctx) -> Move:
    names = ["hunker down", "curl up", "bristle"]
    ops = [f"Counter(secs: 1.2, damage: {ctx.d(6)}, knockback: {ctx.kb(10)})"]
    return Move(names, ops, cd=6.0, cost=4.0, family="shell")


def d_phase(ctx: Ctx) -> Move:
    return Move(["go intangible", "ghost out", "unseen"], ["Phase(secs: 1.4)"], cd=7.0, cost=4.0, fx="spirit" if ctx.E == "Light" else "", family="phase")


DEFENSE = [
    ("counter", lambda x: 2 * ("Blade" in x.m) + 2.5 * (x.p.weapon in ("sword", "spear", "staff")) + 1.5 * (x.p.flav == "chi") + 1 * (x.c.temperament == "Calm" and x.p.humanoid), d_parry),
    ("guard", lambda x: 0.8 + 3 * ("Guard" in x.m) + 4 * (x.p.weapon == "shield") + 1.5 * (x.c.temperament == "Protective") + 1 * ("Armor" in x.c.features), d_guard),
    ("deflect", lambda x: (2 * ("Fast" in x.m) + 1.5 * (x.p.weapon == "sword") + 1 * (x.p.weapon == "gun")) * x.has("Deflect"), d_deflect),
    ("ward", lambda x: 1.6 * (x.E != "Light") + 2 * ("Barrier" in x.m) + 1.5 * ("Repel" in x.m) + 1 * (x.r("mage") >= 2) + 1 * (x.p.flav in ("tech", "mystic", "holy")), d_ward),
    ("brace", lambda x: 2.5 * x.p.heavy + 1 * ("Armor" in x.c.features) + 0.5 * x.p.beast, d_brace),
    ("evade", lambda x: (1.5 * (x.c.temperament in ("Fearful", "Sneaky")) + 1 * ("Fast" in x.m) + 1 * x.p.small) * x.has("Evade"), d_evade),
    ("smoke", lambda x: 2.2 * (x.r("sneak") >= 2) * x.has("Zone", "Evade") + 1 * (x.kwr("sneak") > 0), d_smoke),
    ("heal", lambda x: (4 * (x.kwr("healer") > 0) + 4 * (x.kwr("healing_factor") > 0) + 1.5 * (x.p.flav == "holy" and x.c.temperament == "Protective")) * x.has("Mend"), d_heal),
    ("shell", lambda x: 2 * ("Spikes" in x.c.features) + 2 * (x.c.form in ("Insect", "Arachnid") and "Guard" in x.m) + 1.5 * ("shell" in x.c.weapons), d_shell),
    ("phase", lambda x: 3 * (x.p.flav == "spirit") * x.has("Phase"), d_phase),
]


# ---------------------------------------------------------------------------- COMBO templates

def c_flurry(ctx: Ctx) -> Move:
    a = ctx.adj(ctx.E == "Light")
    names = [f"{a[0]} onslaught", "thousand cuts", f"{a[1]} onslaught"]
    ops = [f"Barrage(hits: 6, interval: 0.08, range: {ctx.rad(3.4)}, angle: 55.0, damage: {ctx.d(4)}, knockback: 2.0, finish: {ctx.kb(16)}, advance: 3.0{ctx.el()})"]
    return Move(names, ops, cd=6.0, cost=8.0, juice=juice(ctx, punch=2.5), family="barrage")


def c_barrage(ctx: Ctx) -> Move:
    a = ctx.adj(ctx.E == "Light")
    names = [f"{a[0]} pummeling", "hundred fists", f"{a[1]} pummeling"]
    if ctx.p.beast:
        names = [f"{a[0]} mauling", "frenzy", f"{a[1]} frenzy"]
    ops = [f"Barrage(hits: 8, interval: 0.07, range: {ctx.rad(3.0)}, angle: 50.0, damage: {ctx.d(3)}, knockback: 2.0, finish: {ctx.kb(18)}, advance: 2.0{ctx.el()})"]
    return Move(names, ops, cd=6.0, cost=8.0, juice=juice(ctx, punch=2.5), family="barrage")


def c_rush(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = [f"{a[0]} fists", "fist storm", "rapid fists"]
    ops = [f"Rush(hits: 10, rate: 16.0, damage: {ctx.d(2.5)}, knockback: {ctx.kb(14)}, range: 2.8)"]
    return Move(names, ops, cd=6.0, cost=8.0, juice=juice(ctx, punch=2.5), family="rush")


def c_blitz(ctx: Ctx) -> Move:
    a = ctx.adj(ctx.E == "Light")
    names = [f"{a[0]} blitz", "phantom blitz", "afterimage blitz"]
    ops = [f"Barrage(hits: 5, interval: 0.12, range: 3.2, angle: 60.0, damage: {ctx.d(5)}, knockback: 3.0, finish: {ctx.kb(18)}, blink: true{ctx.el()})"]
    fx = "speedforce" if "Fast" in ctx.m and ctx.E in ("Light", "Electric") else "smoke" if ctx.E == "Light" else ""
    return Move(names, ops, cd=7.0, cost=8.0, fx=fx, juice=juice(ctx, punch=3.0), family="blitz")


def c_launcher(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = ["launcher", f"{a[0]} uppercut", "skyfall"]
    ops = [f"Shove(range: 3.4, angle: 60.0, damage: {ctx.d(8)}, knockback: 4.0, up: 1.4{ctx.el()})", "Impulse(fwd: 2.0, up: 13.0)",
           f"SlamOnLand(radius: {ctx.rad(3.6)}, damage: {ctx.d(11)}, knockback: {ctx.kb(16)}, carve: 0.8)"]
    return Move(names, ops, cd=7.0, cost=8.0, juice=juice(ctx, shake=0.2), family="launcher")


def c_grab(ctx: Ctx) -> Move:
    if "Constrict" in ctx.m:
        names = ["constrict", "coil crush", "crushing coils"]
        ops = [f"Grab(range: 3.4, hold: 0.45, damage: {ctx.d(12)}, knockback: {ctx.kb(16)}, up: 0.2)"]
    elif ctx.p.beast or ctx.c.form == "Kaiju":
        names = ["seize and toss", "shake and throw", "savage toss"]
        ops = [f"Grab(range: 3.4, hold: 0.3, damage: {ctx.d(10)}, knockback: {ctx.kb(24)}, up: 0.4)"]
    else:
        names = ["suplex", "body slam", "grapple throw"]
        ops = [f"Grab(range: 3.2, hold: 0.3, damage: {ctx.d(10)}, knockback: {ctx.kb(22)}, up: 0.35, back: true)"]
    return Move(names, ops, cd=6.0, cost=8.0, juice=juice(ctx, punch=3.0, shake=0.15), family="grab")


def c_volley(ctx: Ctx) -> Move:
    a = ctx.adj(False)
    names = [f"{a[0]} volley", "suppressing fire" if ctx.p.weapon == "gun" or ctx.p.flav == "tech" else f"{a[1]} volley", f"{a[1]} salvo"]
    if ctx.c.pack == "pokemon":
        names = [V.POKEMON.get(ctx.E, V.POKEMON["Light"])["combo"]] + names
    ops = [f"Salvo(count: 6, delay: 0.25, interval: 0.08, radius: 2.2, damage: {ctx.d(6)}, knockback: {ctx.kb(8)}{ctx.el()}, at: Aim, spread: 3.5)"]
    return Move(names, ops, cd=7.0, cost=8.0, juice=juice(ctx, punch=2.0), family="volley")


def c_sweep(ctx: Ctx) -> Move:
    a = ctx.adj(False)
    names = [f"{a[0]} sweep", "sweeping beam", f"{a[1]} sweep"]
    ops = [f"Carve(length: 24.0, radius: 1.0, damage: {ctx.d(12)})"]
    if ctx.E != "Light":
        ops.append(f"Explode(radius: 3.0, damage: {ctx.d(6)}, knockback: {ctx.kb(10)}{ctx.el()}, at: Aim)")
    return Move(names, ops, cd=7.0, cost=8.0, juice=juice(ctx, punch=2.0), family="sweep")


def maul_style(ctx: Ctx) -> str:
    return ctx.pick("maul", ["maul", "frenzy", "rend", "pack hunt"] if ctx.c.form in ("Quadruped", "Kaiju") else ["maul", "frenzy", "rend"])


def c_maul(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    style = maul_style(ctx)
    if style == "frenzy":
        names = ["frenzy", f"{a[0]} frenzy", "feeding frenzy"]
        ops = [f"Barrage(hits: 7, interval: 0.07, range: 3.0, angle: 80.0, damage: {ctx.d(3)}, knockback: 2.0, finish: {ctx.kb(16)}, advance: 3.0{ctx.el()})"]
    elif style == "rend":
        names = ["rend", "tear apart", f"{a[0]} rending"]
        ops = [f"Barrage(hits: 3, interval: 0.16, range: 3.4, angle: 70.0, damage: {ctx.d(7)}, knockback: 4.0, finish: {ctx.kb(18)}, advance: 5.0{ctx.el()})"]
    elif style == "pack hunt":
        names = ["pack hunt", "run them down", "circle and strike"]
        ops = [f"Barrage(hits: 4, interval: 0.14, range: 3.2, angle: 60.0, damage: {ctx.d(5)}, knockback: 3.0, finish: {ctx.kb(16)}, blink: true{ctx.el()})"]
    else:
        names = ["maul", f"{a[0]} mauling", "pounce and maul"]
        ops = [f"Pounce(range: 12.0, speed: 36.0, cone: 12.0, damage: {ctx.d(5)}, knockback: 2.0)",
               f"Barrage(hits: 4, interval: 0.1, range: 3.0, angle: 70.0, damage: {ctx.d(4)}, knockback: 2.0, finish: {ctx.kb(14)}{ctx.el()})"]
    if ctx.c.pack == "pokemon":
        names = [V.POKEMON.get(ctx.E, V.POKEMON["Light"])["combo"]] + names
    return Move(names, ops, cd=6.0, cost=8.0, juice=juice(ctx, punch=2.5), family="maul")


def c_lash(ctx: Ctx) -> Move:
    names = ["sweeping lash", "whiplash", "lashing sweep"]
    ops = [f"Barrage(hits: 3, interval: 0.15, range: {ctx.rad(4.5)}, angle: 179.0, damage: {ctx.d(6)}, knockback: 6.0, finish: {ctx.kb(16)}{ctx.el()})"]
    return Move(names, ops, cd=6.0, cost=8.0, juice=juice(ctx, punch=2.0), family="lash")


def c_curse(ctx: Ctx) -> Move:
    a = ctx.adj(False)
    names = [f"{a[0]} curse", f"{a[1]} hex", "doom mark"]
    ops = ["Afflict(range: 30.0, cone: 10.0, status: Some(Float), secs: 1.0, up: 6.0)",
           f"DelayedHit(range: 30.0, cone: 10.0, secs: 0.7, damage: {ctx.d(13)}, knockback: {ctx.kb(16)}{ctx.el()})"]
    return Move(names, ops, cd=8.0, cost=8.0, juice=juice(ctx, at="Aim", ripple=3.0), family="curse")


def c_wave(ctx: Ctx) -> Move:
    a = ctx.adj(False)
    names = [f"{a[0]} crescent", "air slash" if ctx.E == "Light" else f"{a[1]} crescent", f"{a[1]} wave"]
    ops = [f"Wave(length: 22.0, speed: 32.0, width: 3.0, carve: 0.6, damage: {ctx.d(12)}, knockback: {ctx.kb(12)}{ctx.el()})"]
    return Move(names, ops, cd=6.0, cost=8.0, juice=juice(ctx, punch=2.5), family="wave")


def c_drill(ctx: Ctx) -> Move:
    a = ctx.adj(False)
    names = [f"{a[0]} drill", "drill charge", "corkscrew"]
    ops = [f"Drill(speed: 30.0, secs: 0.5, grind: 0.6, damage: {ctx.d(12)}, knockback: {ctx.kb(20)}{ctx.el()})"]
    return Move(names, ops, cd=7.0, cost=8.0, juice=juice(ctx, punch=3.0), family="drill")


def c_follow(ctx: Ctx) -> Move:
    names = ["follow through", "decisive blow", "coup de grace"]
    ops = [f"Finisher(lunge: 18.0, range: 3.5, angle: 60.0, damage: {ctx.d(12)}, knockback: {ctx.kb(18)}{ctx.el()}, window: 1.2, mult: 2.0)"]
    return Move(names, ops, cd=6.0, cost=8.0, juice=juice(ctx, punch=3.0), family="finisher")


def c_swarm(ctx: Ctx) -> Move:
    names = ["overwhelm", "dogpile", "gang up"]
    ops = [f"Salvo(count: 8, delay: 0.3, interval: 0.07, radius: 1.8, damage: {ctx.d(5)}, knockback: {ctx.kb(6)}{ctx.el()}, spread: 5.0, seek: true)"]
    return Move(names, ops, cd=7.0, cost=8.0, juice=juice(ctx, punch=2.0), family="swarm")


COMBO = [
    ("barrage", lambda x: 2 * ("Blade" in x.m) + 2.5 * (x.p.weapon in ("sword", "axe", "knives", "spear")), c_flurry),
    ("fists", lambda x: 1.5 + 1 * (x.p.weapon == "fists") + 1 * ("Blunt" in x.m) * x.p.humanoid - 2 * x.p.beast, c_barrage),
    ("rush", lambda x: (2 * (x.p.weapon == "fists") + 1.5 * (x.p.flav == "chi") + 1 * (x.c.pack == "anime")) * x.has("Rush") * x.p.humanoid, c_rush),
    ("blitz", lambda x: 2.5 * ("Fast" in x.m) + 1.5 * (x.r("sneak") >= 2) + 1 * ("Portal" in x.m) - 2 * x.p.heavy, c_blitz),
    ("launcher", lambda x: 2 * ("Blunt" in x.m) + 1.5 * (x.p.heavy and x.p.humanoid) + 1 * ("Launch" in x.m), c_launcher),
    ("grab", lambda x: 3 * ("Grab" in x.m) + 3 * ("Constrict" in x.m) + 1 * (x.p.weapon == "whip"), c_grab),
    ("volley", lambda x: 2 * ("Shoots" in x.m) + 2.5 * (x.p.weapon in ("gun", "bow")) + 1 * (x.p.flav == "tech"), c_volley),
    ("sweep", lambda x: 2.5 * ("Beam" in x.m), c_sweep),
    ("maul", lambda x: 3 * x.p.beast, c_maul),
    ("lash", lambda x: 3 * ("ArticulatedTail" in x.m) + 2 * (x.p.weapon == "whip"), c_lash),
    ("curse", lambda x: 1.5 * max(0.0, x.r("mage") - 1) + 1.5 * (x.p.flav in ("mystic", "spirit")), c_curse),
    ("wave", lambda x: (2 * (x.p.weapon in ("sword", "axe")) + 1 * (x.p.flav == "chi") + 1 * (x.c.pack == "anime")) * x.has("Wave"), c_wave),
    ("drill", lambda x: (3 * ("Spin" in x.m and "Pierce" in x.m) + 1.5 * ("Spin" in x.m)) * x.has("Drill"), c_drill),
    ("finisher", lambda x: (1.5 * (x.c.temperament == "Calm" and x.p.humanoid) + 1 * (x.p.flav == "chi")) * x.has("Finisher"), c_follow),
    ("swarm", lambda x: 3 * ("Swarm" in x.m), c_swarm),
]


# ---------------------------------------------------------------------------- ULTIMATE templates

def ult_juice(ctx: Ctx, title: str, **kw) -> str:
    imp = V.PACK_IMPACT.get(ctx.c.pack)
    if ctx.p.flav == "holy":
        imp = "holy"
    elif ctx.E == "Shadow" or ctx.p.flav == "spirit":
        imp = "void" if imp is None else imp
    elif ctx.p.flav == "glitch":
        imp = "glitch"
    return juice(ctx, title=title, boom=True, punch=kw.pop("punch", 5.0), shake=kw.pop("shake", 0.3), impact=imp, color=True, **kw)


def u_storm(ctx: Ctx) -> Move:
    E = ctx.E
    v = ctx.vocab("storm")
    if ctx.c.pack == "pokemon":
        v = [V.POKEMON.get(E, V.POKEMON["Light"])["storm"]] + v
    el = ctx.el()
    if E == "Water" and ctx.has("Surge"):
        ops = [f"Surge(width: 12.0, dist: 40.0, speed: 15.0, push: 18.0, damage: {ctx.d(10)}, element: Water)"]
    elif E == "Ice":
        ops = [f"Zone(radius: {ctx.rad(10)}, secs: 4.0, status: Some(Chill), slow: 0.5, at: Aim)",
               f"Salvo(count: 6, delay: 0.9, interval: 0.15, radius: 3.0, damage: {ctx.d(12)}, knockback: {ctx.kb(14)}, element: Ice, carve: 0.8, at: Aim, spread: 7.0, seek: true)"]
    elif E == "Wind":
        ops = [f"Field(strength: 40.0, radius: {ctx.rad(9)}, secs: 2.5, at: Aim)", "Zone(radius: 6.0, secs: 2.5, slow: 0.3, at: Aim)",
               f"Salvo(count: 4, delay: 1.2, interval: 0.3, radius: 3.5, damage: {ctx.d(10)}, knockback: {ctx.kb(18)}, element: Wind, at: Aim, spread: 3.0, up: 1.0)"]
    elif E == "Earth":
        if ctx.has("Erupt"):
            ops = [f"Explode(radius: {ctx.rad(7)}, damage: {ctx.d(10)}, knockback: {ctx.kb(18)}, carve: 1.5, element: Earth)",
                   f"Erupt(length: 20.0, width: 3.0, height: 4.0, count: 7, element: Earth, damage: {ctx.d(12)}, launch: 14.0)"]
        else:
            ops = [f"Explode(radius: {ctx.rad(7)}, damage: {ctx.d(10)}, knockback: {ctx.kb(18)}, carve: 1.5, element: Earth)",
                   f"Carve(length: 22.0, radius: 1.3, damage: {ctx.d(12)})"]
    elif E == "Poison":
        ops = [f"Zone(radius: {ctx.rad(9)}, secs: 5.0, status: Some(Poison), dps: 4.0, slow: 0.2, at: Aim)"]
        if ctx.has("Contagion"):
            ops.append("Contagion(range: 30.0, cone: 12.0, status: Poison, secs: 5.0, spread: 5.0)")
    elif E == "Psychic" and ctx.has("Singularity"):
        ops = [f"Singularity(dist: 14.0, rise: 3.0, secs: 2.2, radius: {ctx.rad(10)}, strength: 36.0, capture: 2.0, chunks: 6, damage: {ctx.d(12)}, knockback: {ctx.kb(20)})"]
    elif E == "Shadow":
        ops = [f"Zone(radius: {ctx.rad(10)}, secs: 4.0, status: Some(Shadow))",
               f"DelayedHit(radius: {ctx.rad(10)}, secs: 1.2, damage: {ctx.d(16)}, knockback: {ctx.kb(16)}, element: Shadow)"]
    elif E == "Atomic":
        ops = [f"Explode(radius: {ctx.rad(9)}, damage: {ctx.d(22)}, knockback: {ctx.kb(30)}, carve: 2.5, element: Atomic)",
               f"Zone(radius: {ctx.rad(8)}, secs: 3.0, status: Some(Burn), dps: 3.0)"]
        if ctx.has("Strain"):
            ops.append("Strain(pct: 6.0)")
    else:  # Fire, Electric, Light flavours
        ops = [f"Salvo(count: 8, delay: 0.8, interval: 0.12, radius: 3.2, damage: {ctx.d(13)}, knockback: {ctx.kb(16)}{el}, carve: 1.2, at: Aim, spread: 8.0, seek: true)"]
    return Move(v, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, v[0], at="Aim", ripple=8.0), family="storm")


def u_awaken(ctx: Ctx) -> Move:
    a = ctx.adj(False if ctx.E != "Light" else True)
    names = [f"{a[0]} awakening", "rampage", "berserk"] if ctx.c.temperament in ("Aggressive", "Chaotic") else [f"{a[0]} awakening", "unleashed", "full power"]
    ops = []
    if ctx.has("SelfResize"):
        ops.append("SelfResize(scale: 1.5, secs: 8.0)")
    ops.append(f"Empower(secs: 8.0, damage: 1.25, armor: 0.6, contact: {ctx.d(6)}, contact_kb: {ctx.kb(14)}, contact_cd: 0.5{ctx.el()})")
    ops.append(f"Explode(radius: {ctx.rad(5)}, damage: {ctx.d(8)}, knockback: {ctx.kb(18)}, carve: 1.0{ctx.el()})")
    return Move(names, ops, cd=24.0, cost=40.0, juice=ult_juice(ctx, names[0], ripple=6.0, debris=14.0), family="awaken")


def u_meteor(ctx: Ctx) -> Move:
    names = ["meteor dive", "skyfall", "falling star"] if ctx.E != "Fire" else ["meteor dive", "falling star", "skyfall"]
    ops = ["Counter(secs: 1.2)", "Impulse(fwd: 10.0, up: 24.0)", f"SlamOnLand(radius: {ctx.rad(7.5)}, damage: {ctx.d(22)}, knockback: {ctx.kb(28)}, carve: 3.0)"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0]), family="meteor")


def u_blades(ctx: Ctx) -> Move:
    a = ctx.adj(False)
    names = ["thousand cuts", f"{a[0]} tempest", "last cut"]
    if ctx.has("Wave"):
        ops = [f"Barrage(hits: 8, interval: 0.09, range: 3.6, angle: 70.0, damage: {ctx.d(5)}, knockback: 2.0, finish: {ctx.kb(26)}, blink: true{ctx.el()})",
               f"Wave(length: 30.0, speed: 36.0, width: 7.0, carve: 1.0, damage: {ctx.d(14)}, knockback: {ctx.kb(18)}{ctx.el()})"]
    else:
        ops = [f"Barrage(hits: 8, interval: 0.09, range: 3.6, angle: 70.0, damage: {ctx.d(5)}, knockback: 2.0, finish: {ctx.kb(26)}, blink: true{ctx.el()})"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0]), family="blades")


def u_cannon(ctx: Ctx) -> Move:
    a = ctx.adj(False)
    names = ["hyper beam"] if ctx.c.pack == "pokemon" else []
    names += [f"{a[0]} cannon", f"{a[1]} annihilator", "maximum output"]
    ops = [f"Carve(length: 44.0, radius: 1.8, damage: {ctx.d(20)})", f"Explode(radius: {ctx.rad(5)}, damage: {ctx.d(14)}, knockback: {ctx.kb(20)}, carve: 1.5{ctx.el()}, at: Aim)"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0], at="Aim", ripple=6.0), family="cannon")


def u_lockon(ctx: Ctx) -> Move:
    names = ["dead aim", "target lock", "marked for death"]
    ops = [f"LockOn(secs: 1.6, range: 55.0, damage: {ctx.d(30)}, knockback: {ctx.kb(12)}, angle: 30.0)"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0], punch=3.0, shake=0.15), family="lockon")


def u_bombard(ctx: Ctx) -> Move:
    names = ["bombardment", "missile salvo", "carpet bomb"] if ctx.p.flav == "tech" or ctx.p.vehicle else ["arrow storm" if ctx.p.weapon == "bow" else "hail of fire", "rain of shots", "bombardment"]
    ops = [f"Salvo(count: 14, delay: 0.35, interval: 0.1, radius: 2.2, damage: {ctx.d(6)}, knockback: {ctx.kb(6)}{ctx.el()}, spread: 11.0, seek: true)"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0]), family="bombard")


def u_summon(ctx: Ctx) -> Move:
    c = ctx.c
    form = {"Insect": "Insect", "Arachnid": "Arachnid", "Bird": "Bird", "Fish": "Fish", "Serpent": "Serpent", "Quadruped": "Quadruped"}.get(c.form, "Biped")
    minion = {"Insect": "swarm", "Arachnid": "brood", "Bird": "flock", "Fish": "school", "Serpent": "nest", "Quadruped": "pack"}.get(form, "horde")
    mech = ["Swarm"] + (["Flies"] if "Flies" in ctx.m or form in ("Insect", "Bird") else []) + ([m for m in ("Blade", "Blunt", "Shoots", "Grab", "Breath") if m in ctx.m][:1])
    names = [f"call the {minion}", f"unleash the {minion}", "legion"]
    rec = recipe(f"{c.display} {minion}".lower(), form, mech, ctx.E, 0.7, 30.0, 6, "temperament: Aggressive, intensity: 1.0")
    ops = [f"Cast(recipe: {rec}, signature: Some(Summon))"]
    return Move(names, ops, cd=24.0, cost=40.0, juice=ult_juice(ctx, names[0]), family="summon")


def u_stampede(ctx: Ctx) -> Move:
    names = ["stampede", "rampage", "unstoppable"]
    ops = [f"Empower(secs: 5.0, damage: 1.2, armor: 0.7, contact: {ctx.d(7)}, contact_kb: {ctx.kb(16)}, contact_cd: 0.4{ctx.el()})",
           f"Ram(speed: 28.0, secs: 1.4, damage: {ctx.d(12)}, knockback: {ctx.kb(20)}, through: true, steer: 60.0, keep: 0.5{ctx.el()})"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0], debris=12.0), family="stampede")


def u_overdrive(ctx: Ctx) -> Move:
    names = V.FLAVOUR["speed"]["storm"] if ctx.E == "Light" else [f"{ctx.adj(False)[0]} overdrive", "overdrive"]
    ops = ["TimeStop(secs: 1.2, speed: 1.6)",
           f"Barrage(hits: 7, interval: 0.1, range: 3.4, angle: 70.0, damage: {ctx.d(5)}, knockback: 2.0, finish: {ctx.kb(24)}, blink: true{ctx.el()})"]
    return Move(list(names), ops, cd=20.0, cost=40.0, fx="speedforce" if ctx.E in ("Light", "Electric") else "",
                juice=ult_juice(ctx, names[0], ripple=5.0), family="overdrive")


def u_gamble(ctx: Ctx) -> Move:
    names = ["wild card", "chaos gambit", "roll the dice"]
    e = ctx.E if ctx.E != "Light" else "Fire"
    picks = [
        f"[Explode(radius: 7.0, damage: {ctx.d(20)}, knockback: {ctx.kb(26)}, carve: 2.0, element: {e}, at: Aim)]",
        f"[Salvo(count: 10, delay: 0.5, interval: 0.1, radius: 2.4, damage: {ctx.d(8)}, knockback: {ctx.kb(10)}, at: Aim, spread: 9.0, seek: true)]",
        "[Field(strength: 45.0, radius: 10.0, secs: 2.0, at: Aim), Zone(radius: 7.0, secs: 3.0, status: Some(Stun), at: Aim)]",
        f"[Empower(secs: 6.0, damage: 1.4, armor: 0.5, contact: 6.0, contact_kb: 12.0, contact_cd: 0.4), Mend(pct: 25.0)]",
    ]
    ops = [f"Roulette(picks: [{', '.join(picks)}])"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0]), family="gamble")


def u_guardian(ctx: Ctx) -> Move:
    names = ["unbreakable", "bulwark", "rallying cry"]
    ops = ["Empower(secs: 6.0, damage: 1.15, armor: 0.4)", "Mend(pct: 20.0)" if ctx.has("Mend") else "Counter(secs: 1.0)",
           f"Explode(radius: {ctx.rad(6)}, damage: {ctx.d(8)}, knockback: {ctx.kb(22)}{ctx.el()})"]
    return Move(names, ops, cd=22.0, cost=40.0, fx="holy" if ctx.E == "Light" else "", juice=ult_juice(ctx, names[0], ripple=6.0), family="guardian")


def u_piledriver(ctx: Ctx) -> Move:
    names = ["piledriver", "death grip", "crushing embrace"] if "Constrict" not in ctx.m else ["death coil", "crushing embrace", "piledriver"]
    if "Flies" in ctx.m or ctx.c.form == "Bird":
        names = ["snatch and drop", "sky drop", "carried off"]
    ops = [f"Grab(range: 3.4, hold: 0.45, damage: {ctx.d(14)}, knockback: 8.0, up: 1.4)", "Impulse(fwd: 2.0, up: 14.0)",
           f"SlamOnLand(radius: {ctx.rad(5)}, damage: {ctx.d(18)}, knockback: {ctx.kb(22)}, carve: 1.5)"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0]), family="piledriver")


def u_assassinate(ctx: Ctx) -> Move:
    names = ["assassinate", "death from above", "silent kill"]
    ops = ["WarpTo(range: 40.0, cone: 15.0, behind: 2.0)", f"Shove(range: 3.2, angle: 60.0, damage: {ctx.d(18)}, knockback: {ctx.kb(18)}{ctx.el()})"]
    return Move(names, ops, cd=20.0, cost=40.0, fx="smoke" if ctx.E == "Light" else "", juice=ult_juice(ctx, names[0], punch=4.0),
                crit="Some((2.0, Spark))", family="assassinate")


def u_strafe(ctx: Ctx) -> Move:
    names = ["strafing run", "air strike", "bombing run"]
    ops = ["Impulse(fwd: 18.0, up: 10.0)", "SelfStatus(status: Float, secs: 1.0)",
           f"Salvo(count: 8, delay: 0.3, interval: 0.08, radius: 2.4, damage: {ctx.d(8)}, knockback: {ctx.kb(10)}, at: Front, dist: 12.0, spread: 6.0, seek: true)"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0]), family="strafe")


def u_orbital(ctx: Ctx) -> Move:
    names = ["orbital strike", "full payload", "overclock"]
    ops = [f"Salvo(count: 1, delay: 1.2, radius: {ctx.rad(7)}, damage: {ctx.d(26)}, knockback: {ctx.kb(28)}{ctx.el()}, carve: 2.5, at: Aim)"]
    return Move(names, ops, cd=20.0, cost=40.0, fx="hardlight" if ctx.E == "Light" else "", juice=ult_juice(ctx, names[0], at="Aim", ripple=8.0), family="orbital")


def u_courage(ctx: Ctx) -> Move:
    names = ["burst of courage", "hero moment", "adrenaline"]
    ops = ["Empower(secs: 6.0, damage: 1.35, armor: 0.6)", "Mend(pct: 20.0)" if ctx.has("Mend") else "Counter(secs: 0.8)",
           f"Barrage(hits: 6, interval: 0.09, range: 3.0, angle: 60.0, damage: {ctx.d(4)}, knockback: 2.0, finish: {ctx.kb(20)}, advance: 3.0)"]
    return Move(names, ops, cd=22.0, cost=40.0, juice=ult_juice(ctx, names[0]), family="courage")


def u_knockout(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = [f"{a[0]} knockout", "knockout blow", "lights out"]
    ops = [f"Drill(speed: 34.0, secs: 0.5, grind: 0.0, damage: {ctx.d(24)}, knockback: {ctx.kb(34)}, up: 0.3{ctx.el()})"] if ctx.has("Drill") else \
        [f"Ram(speed: 34.0, secs: 0.5, damage: {ctx.d(24)}, knockback: {ctx.kb(34)}, up: 0.3{ctx.el()})"]
    return Move(names, ops, cd=18.0, cost=40.0, juice=ult_juice(ctx, names[0], punch=6.0), family="knockout")


def u_beatdown(ctx: Ctx) -> Move:
    a = ctx.adj(True)
    names = ["beatdown", f"{a[0]} beatdown", "no mercy"]
    ops = [f"Barrage(hits: 10, interval: 0.07, range: 3.2, angle: 60.0, damage: {ctx.d(3)}, knockback: 2.0, finish: {ctx.kb(26)}, advance: 4.0{ctx.el()})"]
    return Move(names, ops, cd=18.0, cost=40.0, juice=ult_juice(ctx, names[0], punch=4.0), family="beatdown")


def u_toprope(ctx: Ctx) -> Move:
    names = ["from the top rope", "diving finale", "high flyer"]
    ops = ["Leap(range: 18.0, apex: 7.0)", f"SlamOnLand(radius: {ctx.rad(5)}, damage: {ctx.d(18)}, knockback: {ctx.kb(24)}, carve: 1.2)"]
    return Move(names, ops, cd=18.0, cost=40.0, juice=ult_juice(ctx, names[0]), family="toprope")


def u_rage(ctx: Ctx) -> Move:
    names = ["blind rage", "berserker", "lose it"]
    ops = [f"Empower(secs: 6.0, damage: 1.4, armor: 0.7, contact: {ctx.d(5)}, contact_kb: {ctx.kb(12)}, contact_cd: 0.5{ctx.el()})",
           f"Ram(speed: 26.0, secs: 0.8, damage: {ctx.d(12)}, knockback: {ctx.kb(22)}, through: true, keep: 0.4{ctx.el()})"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0], debris=10.0), family="rage")


def u_nap(ctx: Ctx) -> Move:
    names = ["nap time", "snooze", "hibernate"]
    ops = ["Mend(pct: 30.0, per_s: 4.0, secs: 4.0)", "Zone(radius: 7.0, secs: 3.0, slow: 0.55)", "Counter(secs: 1.0)"]
    return Move(names, ops, cd=24.0, cost=40.0, juice=ult_juice(ctx, names[0], punch=2.0, shake=0.1), family="nap")


def u_freakout(ctx: Ctx) -> Move:
    names = ["freak out", "total panic", "scream and flail"]
    ops = ["Evade(secs: 0.6, slow: 0.8, speed: 1.5)", "Empower(secs: 5.0, damage: 1.3, armor: 0.6)",
           f"Barrage(hits: 9, interval: 0.07, range: 3.0, angle: 90.0, damage: {ctx.d(3)}, knockback: 3.0, finish: {ctx.kb(22)}, advance: 2.0)"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0], punch=3.0), family="freakout")


def u_showstopper(ctx: Ctx) -> Move:
    names = ["showstopper", "grand finale", "ta da"]
    ops = [f"Explode(radius: {ctx.rad(6)}, damage: {ctx.d(8)}, knockback: {ctx.kb(14)}{ctx.el()})",
           "Zone(radius: 6.0, secs: 1.5, status: Some(Float))",
           f"Salvo(count: 5, delay: 0.9, interval: 0.12, radius: 2.4, damage: {ctx.d(9)}, knockback: {ctx.kb(12)}, spread: 6.0, seek: true)"]
    return Move(names, ops, cd=20.0, cost=40.0, juice=ult_juice(ctx, names[0], ripple=6.0), family="showstopper")


def u_throttle(ctx: Ctx) -> Move:
    names = ["full throttle", "ramming speed", "road rage"]
    ops = [f"Empower(secs: 4.0, damage: 1.2, armor: 0.6, contact: {ctx.d(6)}, contact_kb: {ctx.kb(16)}, contact_cd: 0.4)",
           f"Ram(speed: 40.0, secs: 1.2, damage: {ctx.d(16)}, knockback: {ctx.kb(26)}, up: 0.2, through: true, steer: 40.0, keep: 0.6)"]
    return Move(names, ops, cd=20.0, cost=40.0, fx="hardlight" if ctx.E == "Light" else "", juice=ult_juice(ctx, names[0], debris=12.0), family="throttle")


def u_timestop(ctx: Ctx) -> Move:
    names = ["time stop", "stopwatch", "borrowed time"]
    ops = ["TimeStop(secs: 2.0, speed: 2.0)"] + (["Rewind(secs: 0.0)"] if False else [])
    return Move(names, ops, cd=24.0, cost=40.0, fx="void" if ctx.E == "Light" else "", juice=ult_juice(ctx, names[0], ripple=12.0), family="timestop")


ULTIMATE = [
    ("storm", lambda x: 3.5 * (x.E != "Light") + 1.5 * (x.p.flav in ("mystic", "holy", "tech", "spirit", "glitch", "ink")) + 3 * (x.c.pack == "pokemon"), u_storm),
    ("awaken", lambda x: (3 * (x.c.form == "Kaiju") + 2 * ("Grow" in x.m) + 1.5 * (x.p.heavy and x.p.humanoid) + 0.5 * (x.c.temperament == "Aggressive")), u_awaken),
    ("meteor", lambda x: 2 * ("Flies" in x.m) * (("Blunt" in x.m) + x.p.heavy + ("Stomp" in x.m)) + 1 * ("DropFromSky" in x.m), u_meteor),
    ("blades", lambda x: 2 * ("Blade" in x.m) + 2.5 * (x.p.weapon in ("sword", "axe", "knives", "spear")), u_blades),
    ("cannon", lambda x: 3 * ("Beam" in x.m) + 1 * (x.p.weapon == "gun" and x.p.flav == "tech"), u_cannon),
    ("lockon", lambda x: (2.5 * (x.p.weapon in ("gun", "bow")) + 1.2 * ("Shoots" in x.m) + 1 * (x.kwr("gunner") > 0)
                          + 1 * (x.c.temperament == "Calm")) * x.has("LockOn"), u_lockon),
    ("bombard", lambda x: 1.8 * ("Shoots" in x.m) + 1.5 * ("Explodes" in x.m) + 1.5 * (x.p.vehicle == "tank") + 1 * (x.p.weapon == "bow"), u_bombard),
    ("summon", lambda x: 3.5 * ("Swarm" in x.m) + 2 * (x.kwr("summoner") > 0 and ("Swarm" in x.m or x.c.temperament == "Aggressive")), u_summon),
    ("stampede", lambda x: 2.5 * ("Charge" in x.m) * (x.p.beast or x.p.heavy) + 1 * x.p.beast, u_stampede),
    ("overdrive", lambda x: (4 * ("Fast" in x.m) + 2 * (x.p.flav == "speed") - 4 * x.p.heavy) * x.has("TimeStop") * (x.p.vehicle is None), u_overdrive),
    ("throttle", lambda x: 5 * (x.p.vehicle == "car"), u_throttle),
    ("gamble", lambda x: (3 * (x.c.temperament == "Chaotic") + 2 * (x.kwr("trick") > 0)) * x.has("Roulette"), u_gamble),
    ("guardian", lambda x: 2 * (x.c.temperament == "Protective") + 1.5 * ("Guard" in x.m) + 2 * (x.p.weapon == "shield"), u_guardian),
    ("piledriver", lambda x: 2.5 * ("Grab" in x.m) + 3 * ("Constrict" in x.m), u_piledriver),
    ("assassinate", lambda x: (2 * (x.r("sneak") >= 2) + 1 * (x.p.weapon == "knives")) * x.has("WarpTo"), u_assassinate),
    ("strafe", lambda x: 5 * (x.p.vehicle == "jet"), u_strafe),
    ("orbital", lambda x: 2 * (x.p.flav == "tech") + 1 * ("Piloted" in x.m) + 1 * (x.p.vehicle in ("tank", "car")), u_orbital),
    ("courage", lambda x: 2.5 * (x.r("civ") >= 2), u_courage),
    ("timestop", lambda x: 5 * (x.kwr("time") > 0) * x.has("TimeStop"), u_timestop),
    ("knockout", lambda x: 1.2 + 1 * x.p.humanoid + 0.5 * (x.p.weapon == "fists"), u_knockout),
    ("beatdown", lambda x: 0.6 + 0.8 * x.p.humanoid + 0.6 * (x.c.temperament in ("Aggressive", "Calm")), u_beatdown),
    ("toprope", lambda x: (0.6 + 1 * x.p.humanoid + 1.2 * (x.c.temperament in ("Playful", "Chaotic"))) * x.has("Leap"), u_toprope),
    ("rage", lambda x: (1.5 * (x.c.temperament == "Aggressive") + 0.8 * x.p.humanoid + 0.5 * x.p.beast), u_rage),
    ("nap", lambda x: 3.5 * (x.c.temperament == "Lazy") * x.has("Mend"), u_nap),
    ("freakout", lambda x: 3.2 * (x.c.temperament == "Fearful") * x.has("Evade"), u_freakout),
    ("showstopper", lambda x: 2.2 * (x.c.temperament == "Playful"), u_showstopper),
]


# ---------------------------------------------------------------------------- BONUS (one signature move, untagged)

def b_grow(ctx):
    return Move(["grow", "super size", "growth spurt"], ["SelfResize(scale: 1.4, secs: 6.0)"], cd=12.0, cost=6.0, family="grow")


def b_shrink(ctx):
    return Move(["shrink", "shrinking", "go small"], ["SelfResize(scale: 0.6, secs: 5.0)", "Impulse(fwd: 10.0, up: 4.0)"], cd=10.0, cost=4.0, family="shrink")


def b_yank(ctx):
    return Move(["reel in", "get over here", "yank"], ["Yank(range: 28.0, speed: 26.0, cone: 12.0)"], cd=6.0, cost=4.0, family="yank")


def b_web(ctx):
    return Move(["web snare", "sticky web", "entangle"], [f"Zone(radius: 4.0, secs: 3.0, status: Some(Stun), slow: 0.6, at: Aim)"], cd=8.0, cost=5.0, family="web")


def b_seek(ctx):
    a = ctx.adj(False)
    rec = recipe("homing shot", "Spear" if ctx.p.weapon == "bow" else "Ball", ["Homing", "Fast"], ctx.E, 0.5, 4.0)
    return Move(["homing shot", f"{a[0]} seeker", "never misses"],
                [f"Seeker(recipe: {rec}, range: 45.0, cone: 20.0, speed: 34.0, damage: {ctx.d(10)}, knockback: {ctx.kb(12)}{ctx.el()}, hits: 2, chain: 10.0)"],
                cd=6.0, cost=5.0, family="seek")


def b_bomb(ctx):
    a = ctx.adj(False)
    rec = recipe("bomb", "Bomb", ["Explodes"], ctx.E if ctx.E != "Light" else "Fire", 0.6, 6.0)
    return Move([f"{a[0]} bomb", "explosive", "kaboom"], [f"Cast(recipe: {rec})"], cd=4.0, cost=3.0, family="bomb")


def b_gravity(ctx):
    return Move(["gravity well", "pull in", "attraction"], ["Field(strength: 40.0, radius: 8.0, secs: 1.5, at: Aim)"], cd=8.0, cost=5.0, family="attract")


def b_repulse(ctx):
    return Move(["repulse", "force push", "shockwave"], [f"Explode(radius: {ctx.rad(5)}, damage: {ctx.d(6)}, knockback: {ctx.kb(24)}{ctx.el()})"], cd=6.0, cost=4.0, family="repel")


def b_trap(ctx):
    return Move(["set a trap", "snare", "ambush trap"], [f"Zone(radius: 3.5, secs: 6.0, status: Some(Stun), slow: 0.5, at: Aim)"], cd=8.0, cost=4.0, family="trap")


def b_rewind(ctx):
    return Move(["rewind", "undo", "time loop"], ["Rewind(secs: 3.0)"], cd=12.0, cost=5.0, fx="void", family="rewind")


def b_portal(ctx):
    return Move(["portal", "open a portal", "gateway"], ["PlacePortal(range: 60.0, secs: 15.0)"], cd=3.0, cost=3.0, family="portal")


def b_drop(ctx):
    return Move(["drop from above", "death from the sky", "incoming"], [f"Salvo(count: 1, delay: 0.8, radius: 4.0, damage: {ctx.d(16)}, knockback: {ctx.kb(20)}, carve: 1.5, at: Aim)"],
                cd=8.0, cost=5.0, family="drop")


def b_breath(ctx):
    m = t_breath(ctx)
    m.cd, m.cost = 5.0, 4.0
    return m


def b_beam(ctx):
    m = t_beam(ctx)
    m.cd, m.cost = 4.0, 4.0
    return m


def b_roar(ctx):
    return Move(["roar", "war cry", "bellow"], ["GazeStun(range: 14.0, cone: 70.0, secs: 0.8, status: Stun, aimed: false)"] if ctx.has("GazeStun")
                else ["Zone(radius: 7.0, secs: 1.0, status: Some(Stun))"], cd=10.0, cost=5.0, family="roar")


def b_stomp(ctx):
    m = t_stomp(ctx)
    m.cd = 4.0
    return m


def b_bolt(ctx):
    m = t_bolt(ctx)
    m.cd = 2.0
    return m


BONUS = [
    ("grow", lambda x: 4 * ("Grow" in x.m or x.kwr("size") > 0 and "Shrink" not in x.m) * x.has("SelfResize"), b_grow),
    ("shrink", lambda x: 4 * ("Shrink" in x.m) * x.has("SelfResize"), b_shrink),
    ("yank", lambda x: 3 * ("Pull" in x.m) + 1.5 * ("Link" in x.m) + 1 * (x.p.weapon == "whip"), b_yank),
    ("web", lambda x: 3.5 * (x.c.form == "Arachnid" or "Sticky" in x.m), b_web),
    ("seek", lambda x: (3 * ("Homing" in x.m) + 1.5 * ("Returning" in x.m)) * x.has("Seeker"), b_seek),
    ("bomb", lambda x: 3 * ("Explodes" in x.m), b_bomb),
    ("attract", lambda x: 3 * ("Attract" in x.m), b_gravity),
    ("repel", lambda x: 3 * ("Repel" in x.m), b_repulse),
    ("trap", lambda x: 3 * ("Trap" in x.m), b_trap),
    ("rewind", lambda x: 4 * (x.kwr("time") > 0) * x.has("Rewind"), b_rewind),
    ("portal", lambda x: 2.5 * ("Portal" in x.m) * x.has("PlacePortal"), b_portal),
    ("drop", lambda x: 3 * ("DropFromSky" in x.m) * x.has("Salvo"), b_drop),
    ("breath", lambda x: 2.5 * ("Breath" in x.m), b_breath),
    ("beam", lambda x: 2.5 * ("Beam" in x.m), b_beam),
    ("roar", lambda x: 2 * (x.c.form == "Kaiju") + 1 * (x.p.beast and x.c.temperament == "Aggressive"), b_roar),
    ("stomp", lambda x: 2 * ("Stomp" in x.m), b_stomp),
    ("bolt", lambda x: 1.8 * ("Shoots" in x.m) + 1.2 * (x.E != "Light"), b_bolt),
]


# ============================================================================ kit assembly

class Rules:
    """Everything a move name / kit name must not collide with."""

    def __init__(self, claims: S.Claims):
        self.costume_words = S.costume_grammar_words()
        self._memo: dict[str, list[str]] = {}
        self.claims = claims
        self.fx = S.fx_keys()

    def clashes(self, w: str) -> list[str]:
        """Costume-grammar words that a spoken `w` would be confused with (memoised)."""
        hit = self._memo.get(w)
        if hit is None:
            hit = [f for f in self.costume_words if f == w or word_hits(f, w)] if len(w) >= 3 else []
            self._memo[w] = hit
        return hit

    def name_ok(self, name: str, tag: str | None, taken_words: set[str]) -> bool:
        ws = name.split(" ")
        if not name or len(ws) > 4:
            return False
        for w in ws:
            # no slot word at all in a derived move name: "<name> combo" must reach the Combo move by its
            # TAG, so a hand kit sharing the name (a stale file) always wins the tie (toolbox.rs ranks a
            # whole-word name hit above a tag)
            if w in SLOT_WORDS:
                return False
            for f in self.clashes(w):
                if not (f in SLOT_WORDS and (SLOT_WORDS[f] == tag or (tag == "Defense" and SLOT_WORDS[f] == "Counter"))):
                    return False
        return True


def slot_tag(slot: str) -> str | None:
    return {"primary": "Primary", "movement": "Movement", "defense": "Defense", "combo": "Combo", "ultimate": "Ultimate"}.get(slot)


class X:
    """Scoring view of a character (templates' lambdas read this)."""

    def __init__(self, ctx: Ctx):
        self.c, self.p, self.m, self.E = ctx.c, ctx.p, ctx.m, ctx.E
        self._ctx = ctx

    def r(self, k: str) -> float:
        return self.p.r.get(k, 0.0)

    def kwr(self, k: str) -> int:
        return self.p.kw.get(k, 0)

    def has(self, *ops: str) -> bool:
        return self._ctx.has(*ops)


Ctx.r = lambda self, k: self.p.r.get(k, 0.0)  # type: ignore[attr-defined]
Ctx.kwr = lambda self, k: self.p.kw.get(k, 0)  # type: ignore[attr-defined]

SLOTS = [("primary", PRIMARY, ["Primary"], (0.8, 3.0)), ("movement", MOVEMENT, ["Movement"], (3.0, 4.0)),
         ("defense", DEFENSE, ["Defense", "Counter"], (6.0, 5.0)), ("combo", COMBO, ["Combo"], (6.0, 8.0)),
         ("ultimate", ULTIMATE, ["Ultimate"], (20.0, 40.0))]


def choose(ctx: Ctx, x: X, slot: str, table, used_families: set[str]):
    scored = []
    for fam, score, build in table:
        s = float(score(x))
        if s <= 0:
            continue
        if fam in used_families:
            s -= 2.5
        jitter = (h32(ctx.c.key, slot, fam) % 1000) / 1000.0 * 1.4
        scored.append((s + jitter, fam, build))
    scored.sort(key=lambda t: (-t[0], t[1]))
    return scored


def build_kit(c: S.Char, en: S.Enums, rules: Rules) -> dict | None:
    p = profile(c)
    ctx = Ctx(c, p, en)
    x = X(ctx)
    moves: list[Move] = []
    families: set[str] = set()
    taken_names: set[str] = set()
    taken_words: set[str] = set()
    trace = []

    def settle(mv: Move, tags: list[str]) -> bool:
        for n in mv.names:
            n = S.norm_words(n)
            if n in taken_names or not rules.name_ok(n, tags[0] if tags else None, taken_words):
                continue
            banned = getattr(c, "banned_words", set())
            if banned and any(word_hits(b, w) for b in banned for w in n.split(" ") if len(w) >= 3):
                continue
            # distinctive words must not be shared with another move of the kit (fuzzy picks stay clean)
            ws = {w for w in n.split(" ") if len(w) >= 3}
            if ws & taken_words and len(ws - taken_words) == 0:
                continue
            mv.name = n
            taken_names.add(n)
            taken_words.update(ws)
            for w in n.split(" "):
                ctx.used_adj.add(w)
            return True
        return False

    for slot, table, tags, (cd0, cost0) in SLOTS:
        placed = False
        for _s, fam, build in choose(ctx, x, slot, table, families):
            mv = build(ctx)
            if mv is None:
                continue
            if not settle(mv, tags):
                continue
            mv.tags = tags
            mv.cd = round(mv.cd * ctx.cdm, 2)
            mv.tpl = tpl_for(slot, fam, ctx)
            families.add(mv.family or fam)
            moves.append(mv)
            trace.append(f"{slot}={fam}")
            placed = True
            break
        if not placed:
            return None
    # one bonus move: the strongest signature mechanic not already expressed
    for _s, fam, build in choose(ctx, x, "bonus", BONUS, families):
        if fam in families or _s < 2.4:
            continue
        mv = build(ctx)
        if mv.family in families:
            continue
        if settle(mv, []):
            mv.tags = []
            mv.tpl = tpl_for("bonus", fam, ctx)
            moves.append(mv)
            trace.append(f"bonus={fam}")
        break
    kit_fx = ""
    if ctx.ev:
        kit_fx = ctx.ev["fx"]
    elif ctx.fv:
        kit_fx = ctx.fv["fx"]
    if kit_fx and rules.fx and kit_fx not in rules.fx:
        kit_fx = ""
    for mv in moves:
        if mv.fx == kit_fx or (rules.fx and mv.fx not in rules.fx):
            mv.fx = ""
    return {"char": c, "prof": p, "moves": moves, "fx": kit_fx, "trace": trace, "ctx": ctx}


# ============================================================================ template ids (bd_sim powers/derive.rs)

FAMILY_TPL = {
    ("primary", "stretch"): "Stretch", ("primary", "thrust"): "Thrust", ("primary", "smash"): "Smash", ("primary", "stomp"): "Stomp",
    ("primary", "breath"): "Breath", ("primary", "bite"): "Bite", ("primary", "whip"): "Lash", ("primary", "spin"): "Spin",
    ("primary", "hex"): "Hex",
    ("movement", "blur"): "Blur", ("movement", "streak"): "Streak", ("movement", "warp"): "Ambush", ("movement", "boost"): "Boost",
    ("movement", "leap"): "Leap", ("movement", "charge"): "Charge", ("movement", "pounce"): "Pounce", ("movement", "reel"): "Reel",
    ("movement", "burrow"): "Burrow", ("movement", "surf"): "Surf", ("movement", "rollout"): "RollOut", ("movement", "bounce"): "Bounce",
    ("movement", "phase"): "PhaseDash", ("movement", "elemstep"): "ElemStep",
    ("defense", "counter"): "Parry", ("defense", "guard"): "Guard", ("defense", "deflect"): "Deflect", ("defense", "ward"): "Ward",
    ("defense", "brace"): "Brace", ("defense", "evade"): "Evade", ("defense", "smoke"): "Smoke", ("defense", "heal"): "Heal",
    ("defense", "shell"): "Shell", ("defense", "phase"): "Intangible",
    ("combo", "barrage"): "Flurry", ("combo", "fists"): "FistBarrage", ("combo", "rush"): "Rush", ("combo", "blitz"): "Blitz",
    ("combo", "launcher"): "Launcher", ("combo", "volley"): "Volley", ("combo", "sweep"): "Sweep", ("combo", "lash"): "TailLash",
    ("combo", "curse"): "Curse", ("combo", "wave"): "Crescent", ("combo", "drill"): "Drill", ("combo", "finisher"): "FollowThrough",
    ("combo", "swarm"): "Dogpile",
    ("ultimate", "storm"): "Storm", ("ultimate", "awaken"): "Awaken", ("ultimate", "meteor"): "Meteor", ("ultimate", "blades"): "Blades",
    ("ultimate", "cannon"): "Cannon", ("ultimate", "lockon"): "DeadAim", ("ultimate", "bombard"): "Bombard",
    ("ultimate", "stampede"): "Stampede", ("ultimate", "overdrive"): "Overdrive", ("ultimate", "gamble"): "Gamble",
    ("ultimate", "guardian"): "Guardian", ("ultimate", "piledriver"): "Piledriver", ("ultimate", "assassinate"): "Assassinate",
    ("ultimate", "strafe"): "Strafe", ("ultimate", "orbital"): "Orbital", ("ultimate", "courage"): "Courage",
    ("ultimate", "knockout"): "Knockout", ("ultimate", "beatdown"): "Beatdown", ("ultimate", "toprope"): "TopRope",
    ("ultimate", "rage"): "Rage", ("ultimate", "nap"): "Nap", ("ultimate", "freakout"): "FreakOut",
    ("ultimate", "showstopper"): "Showstopper", ("ultimate", "throttle"): "Throttle", ("ultimate", "timestop"): "TimeStop",
    ("bonus", "grow"): "Grow", ("bonus", "shrink"): "Shrink", ("bonus", "yank"): "Yank", ("bonus", "web"): "WebSnare",
    ("bonus", "seek"): "Seek", ("bonus", "bomb"): "Bomb", ("bonus", "attract"): "Gravity", ("bonus", "repel"): "Repulse",
    ("bonus", "trap"): "Trap", ("bonus", "rewind"): "Rewind", ("bonus", "portal"): "Portal", ("bonus", "drop"): "Drop",
    ("bonus", "breath"): "Breath", ("bonus", "roar"): "Roar", ("bonus", "stomp"): "Stomp",
}
BRAWL_TPL = {"jab": "Jab", "haymaker": "Haymaker", "kick": "Kick", "flying kick": "FlyingKick", "palm": "Palm", "headbutt": "Headbutt",
             "slap": "Slap", "elbow": "Elbow", "punch": "Punch"}
EVASIVE_TPL = {"roll": "Roll", "slide": "Slide", "vault": "Vault", "backflip": "Springback", "sprint": "Sprint", "sidestep": "Sidestep",
               "flee": "Flee", "cartwheel": "Cartwheel", "flop": "Flop"}
MAUL_TPL = {"maul": "Maul", "frenzy": "Frenzy", "rend": "Rend", "pack hunt": "PackHunt"}


def bolt_kind(ctx: Ctx) -> str:
    E, w = ctx.E, ctx.p.weapon
    if E == "Electric":
        return "Thunder"
    if E == "Psychic":
        return "Lift"
    if E in ("Atomic", "Water") and ctx.has("BounceBeam"):
        return "Ray"
    k = {"Fire": "Fireball", "Ice": "Shard", "Wind": "Cutter", "Earth": "Rock", "Poison": "Spit", "Shadow": "Orb"}.get(E)
    if k:
        return k
    if w == "bow":
        return "HomingArrow" if "Homing" in ctx.m else "Arrow"
    if w == "knives":
        return "Knives"
    if ctx.p.flav == "tech" or w == "gun":
        return "Pellets"
    return "Energy"


def tpl_for(slot: str, fam: str, ctx: Ctx) -> str:
    """The Rust `Tpl` of the template `fam` picked for `slot` (same picks as the name builders)."""
    m = ctx.m
    if fam == "slash":
        return "Cleave" if ctx.p.weapon == "axe" or ctx.p.heavy else "Slash"
    if fam == "bolt":
        return f"Bolt({bolt_kind(ctx)})"
    if fam == "beam":
        return "BeamBounce" if "Ricochet" in m else "Beam"
    if fam == "punch":
        return f"Brawl({BRAWL_TPL[brawl_style(ctx)]})"
    if fam == "teleport":
        return "Vanish" if ctx.p.r["sneak"] >= 2 and ctx.E in ("Light", "Shadow") else "Teleport"
    if fam == "fly":
        return "Swoop" if ctx.c.form == "Bird" or "Glide" in m else "Fly"
    if fam == "roll":
        return f"Evasive({EVASIVE_TPL[evasive_style(ctx)]})"
    if fam == "grab":
        return "Grab(Constrict)" if "Constrict" in m else "Grab(Toss)" if ctx.p.beast or ctx.c.form == "Kaiju" else "Grab(Suplex)"
    if fam == "maul":
        return f"Maul({MAUL_TPL[maul_style(ctx)]})"
    if fam == "summon":
        form = ctx.c.form if ctx.c.form in ("Insect", "Arachnid", "Bird", "Fish", "Serpent", "Quadruped") else "Biped"
        flies = "Flies" in m or form in ("Insect", "Bird")
        mech = next((x for x in ("Blade", "Blunt", "Shoots", "Grab", "Breath") if x in m), None)
        return f"Summon({form}, {'true' if flies else 'false'}, {f'Some({mech})' if mech else 'None'})"
    t = FAMILY_TPL.get((slot, fam))
    if t is None:
        raise KeyError(f"no template id for {slot}/{fam}")
    return t


# ============================================================================ names, collisions, output

def slug(s: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", s.lower()).strip("_")


FRANCHISE_TITLES: set[str] = set()
SHARED_NOTES: set[str] = set()  # a names-list note several characters share is their franchise, not a description


def franchise_titles(chars: list[S.Char]) -> set[str]:
    """Franchise / source titles seen in the data ("game of thrones", "cat returns"): never kit names."""
    out: set[str] = set()
    seen: dict[str, int] = {}
    for c in chars:
        parts = [S.norm_words(p) for p in re.split(r"[/;,()\[\]]| - ", c.source or "")]
        out.update(p for p in parts if p and len(p.split(" ")) >= 2)
        for p in re.split(r"[/;,()\[\]]| - ", c.notes or ""):
            n = S.norm_words(p)
            if n:
                seen[n] = seen.get(n, 0) + 1
    # a names-list note shared by several characters is their franchise ("turtles in time"), not a description
    out.update(n for n, k in seen.items() if k >= 2 and len(n.split(" ")) >= 2)
    return out


def is_title(n: str, titles: set[str]) -> bool:
    """A franchise title, or the start of one ("red dead" of "red dead redemption 2")."""
    if n in titles:
        return True
    ws = n.split(" ")
    return len(ws) >= 2 and any(t.startswith(n + " ") for t in titles_by_first(titles).get(ws[0], ()))


_TBF: dict[int, dict[str, list[str]]] = {}


def titles_by_first(titles: set[str]) -> dict[str, list[str]]:
    key = id(titles)
    if key not in _TBF:
        d: dict[str, list[str]] = {}
        for t in titles:
            d.setdefault(t.split(" ")[0], []).append(t)
        _TBF[key] = d
    return _TBF[key]


def word_spread(chars: list[S.Char]) -> dict[str, int]:
    """How many different things (characters, icons, names-list entries) use each word in a name."""
    owners: dict[str, set[str]] = {}
    for c in chars:
        for n in c.names:
            for w in n.split(" "):
                owners.setdefault(w, set()).add("c:" + c.key)
    for n in S.icon_names_all():
        for w in n.split(" "):
            owners.setdefault(w, set()).add("i:" + n)
    return {w: len(o) for w, o in owners.items()}


DERIVED_BUDGET = 2500   # most derived kits (agreed cap; the table's MAX_POWERS budget may shrink it further)
HEADROOM = 300          # table slots always left free for hand content that lands before a re-run
VARIANT_WORDS = {"super", "saiyan", "baby", "young", "old", "kid", "teen", "adult", "future", "past", "evil", "dark", "ultimate", "prime",
                 "classic", "modern", "zombie", "robot", "mecha", "robo", "cyborg", "clone", "golden", "shadow", "armored", "armoured",
                 "giant", "mini", "chibi", "beast", "true", "final", "perfect", "god", "mode", "form", "suit", "version", "noir", "2099"}


def priorities(chars: list[S.Char], claims: S.Claims) -> tuple[dict[str, float], dict[str, str]]:
    """How likely players are to NAME each character (Wikidata sitelinks of its best character match +
    curation bonuses), and the ones that are not characters at all (reason per key)."""
    names = {n for c in chars for n in c.names}
    pop = S.popularity(names)
    # sorted (longest first, then by text): iterating a set here made the variant check depend on
    # PYTHONHASHSEED ("cyborg superman" kept or dropped run to run) — TOOLBOX QA
    hand = sorted({kitform(n) for n in claims.names if len(n) >= 5}, key=lambda h: (-len(h), h))
    score: dict[str, float] = {}
    drop: dict[str, str] = {}
    for c in chars:
        charpop = max((pop[n]["char"] for n in c.names if n in pop), default=0)
        generic = pop.get(c.key, {}).get("generic", 0)  # the canonical name itself names a TYPE ("werewolf")
        ws = c.key.split(" ")
        if c.pack == "military":
            drop[c.key] = "not a character (military unit)"
            continue
        if c.pack == "tabletop" and charpop < 10:
            drop[c.key] = "generic creature (tabletop monster type)"
            continue
        if generic >= 20 and charpop == 0 and c.origin == "names" and c.pack not in ("pokemon", "transformers"):
            drop[c.key] = f"generic creature type ({generic} sitelinks as a type)"
            continue
        # a variant of a hand-built character ("zombie captain america", "super saiyan blue vegeta")
        base = next((h for h in hand if h in c.key and h != c.key and (" " in h or len(h) >= 6)
                     and re.search(r"(^| )" + re.escape(h) + r"( |$)", c.key)), None)
        if base and charpop < 25 and (set(ws) - set(base.split(" "))) <= VARIANT_WORDS | {"the", "of", "marvel", "dc"}:
            drop[c.key] = f"variant of the hand-built {base!r}"
            continue
        bonus = 30 if (c.origin == "costume" or c.colors) else 15 if c.origin == "icon" else 0
        score[c.key] = charpop + bonus + 0.01 * len(c.names)
    return score, drop


def assign(chars: list[S.Char], claims: S.Claims, rules: Rules, rank: dict[str, float] | None = None):
    """Pick each character's kit names: skip characters hand content names; unique aliases.

    A one-word alias that many other things also use ("fire", "robot", "bishop", "war") is not a kit
    name unless it is the character's own canonical name: "<common word> attack" must stay free."""
    hand = claims.names | claims.shouts | set(claims.kit_names)
    hand |= {kitform(n) for n in claims.names}
    # A hand kit name with an apostrophe ("pharaoh's mummy") is matched possessive-stripped
    # ("pharaoh mummy ..."): no derived alias may be a word-prefix of it ("pharaoh") or it steals it.
    for n in claims.kit_names:
        ws = kitform(n).split(" ")
        if "'" in n:
            hand |= {" ".join(ws[:k]) for k in range(1, len(ws))}
    titles = franchise_titles(chars)
    FRANCHISE_TITLES.clear()
    FRANCHISE_TITLES.update(titles)
    counts: dict[str, int] = {}
    for c in chars:
        if c.notes:
            n = S.norm_words(c.notes)
            counts[n] = counts.get(n, 0) + 1
    SHARED_NOTES.clear()
    SHARED_NOTES.update(n for n, k in counts.items() if k >= 2)
    spread = word_spread(chars)
    kept = []
    skipped_hand = []
    taken: dict[str, str] = {}
    order = lambda c: (-(rank or {}).get(c.key, 0.0), PACK_ORDER.index(c.pack) if c.pack in PACK_ORDER else 99, c.order)  # noqa: E731
    for c in sorted(chars, key=order):
        if any(n in hand for n in c.names):
            skipped_hand.append(c)
            continue
        c.titles = {n for n in c.names if n != c.key and is_title(n, titles)}  # type: ignore[attr-defined]
        names = []
        extra = []
        for n in c.names:
            ws = n.split(" ")
            if len(ws) > 1 and ws[-1] in V.STRIP_SUFFIXES:
                extra.append(" ".join(ws[:-1]))
        for n in c.names + extra:
            stripped = n in extra
            n = " ".join(strip_possessive(w) for w in n.split(" "))
            if len(n) < 3 or n in hand or n in taken or n in SLOT_WORDS or n in FILLER or n in names:
                continue
            if is_title(n, titles) and n != c.key:
                continue
            if " " not in n and n != c.key and spread.get(n, 0) >= (2 if stripped else 4):
                continue
            if all(w in FILLER or w in SLOT_WORDS for w in n.split(" ")):
                continue
            names.append(n)
        if not names:
            continue
        for n in names:
            taken[n] = c.key
        c.kit_names = names  # type: ignore[attr-defined]
        if c.display == c.display.lower():
            c.display = c.display.title()
        canon = {c.key, kitform(c.display)}
        ks = c.key.split(" ")
        if len(ks) > 1 and ks[-1] in V.STRIP_SUFFIXES:
            canon.add(" ".join(ks[:-1]))
        c.canonical = canon & set(names)  # type: ignore[attr-defined]
        if kitform(c.display) not in names:
            # the kit's `name` is a kit name too (toolbox.rs indexes it): never let it claim another's
            c.display = names[0].title()
        kept.append(c)
    return kept, skipped_hand


def impact_frame(ctx: Ctx) -> str | None:
    imp = V.PACK_IMPACT.get(ctx.c.pack)
    if ctx.p.flav == "holy":
        imp = "holy"
    elif ctx.E == "Shadow" or ctx.p.flav == "spirit":
        imp = "void" if imp is None else imp
    elif ctx.p.flav == "glitch":
        imp = "glitch"
    return imp


def ron_kit(k: dict, pid: str) -> str:
    """One compact entry: `derived: Some((el, tm, sz, wp, col, fr, mv: [(Tpl, "name")..]))`."""
    c: S.Char = k["char"]
    ctx: Ctx = k["ctx"]
    names = ", ".join(f'"{n}"' for n in c.kit_names)  # type: ignore[attr-defined]
    fx = f', fx: "{k["fx"]}"' if k["fx"] else ""
    disp = c.display.replace('"', "'")
    d = []
    if ctx.E != "Light":
        d.append(f"el: {ctx.E}")
    if c.temperament != "Calm":
        d.append(f"tm: {c.temperament}")
    if abs(ctx.p.size - 1.0) > 1e-6:
        d.append(f"sz: {fmt(ctx.p.size)}")
    if ctx.p.weapon == "bow":
        d.append("wp: Some(Bow)")
    col = ctx.color()
    if col:
        d.append(f"col: {col}")
    fr = impact_frame(ctx)
    if fr:
        d.append(f'fr: Some("{fr}")')
    mv = ", ".join(f'({m.tpl}, "{m.name}")' for m in k["moves"])
    d.append(f"mv: [{mv}]")
    return f'    (id: "{pid}", name: "{disp}", kind: Kit{fx}, names: [{names}],\n     derived: Some(({", ".join(d)}))),\n'


def generate(verbose: bool = True):
    en = S.Enums()
    claims = S.parse_claims(OUT_NAME)
    rules = Rules(claims)
    chars = load_characters(en)
    rank, not_chars = priorities(chars, claims)
    budget = max(0, min(DERIVED_BUDGET, S.max_powers() - claims.entries - HEADROOM))
    generate.budget = budget  # type: ignore[attr-defined]
    generate.not_chars = not_chars  # type: ignore[attr-defined]
    cands = [c for c in chars if c.key not in not_chars]
    kept, skipped_hand = assign(cands, claims, rules, rank)
    generate.no_name = len(cands) - len(skipped_hand) - len(kept)  # type: ignore[attr-defined]
    generate.cut = kept[budget + 200:]  # type: ignore[attr-defined]
    kept = kept[: budget + 200]  # a margin: the collision pass may drop a few
    kits = []
    failed = []
    for c in kept:
        k = build_kit(c, en, rules)
        if k is None:
            failed.append(c)
            if os.environ.get("DK_DEBUG"):
                print("FAILED", c.key, c.pack, c.form, c.element, c.temperament, sorted(c.mech))
            continue
        kits.append(k)
    # ids
    seen: set[str] = set()
    for k in kits:
        pid = "dk_" + slug(k["char"].key)
        if pid in seen or pid in {kk["id"] for kk in claims.kits}:
            pid = f"{pid}_{k['char'].pack}"
        n = 2
        base = pid
        while pid in seen:
            pid = f"{base}_{n}"
            n += 1
        seen.add(pid)
        k["pid"] = pid
    # collision pass: no known phrase (icon names, costume phrases, shouts, power names) may read as a derived kit move
    generate.all_chars = chars  # type: ignore[attr-defined]
    dropped, accepted = collision_pass(kits, claims, en, rules)
    generate.accepted = accepted  # type: ignore[attr-defined]
    kits = [k for k in kits if k["char"].kit_names]  # type: ignore[attr-defined]
    generate.cut = [k["char"] for k in kits[budget:]] + generate.cut  # type: ignore[attr-defined]
    kits = kits[:budget]
    return en, claims, chars, kept, skipped_hand, failed, kits, dropped


def known_phrases(claims: S.Claims, chars: list[S.Char]) -> set[str]:
    out = set(S.icon_names_all()) | claims.names | claims.shouts
    for c in chars:
        for n in c.names:
            out.add(n)  # every character's name (costume looks included) stays its bare summon
            for w in getattr(c, "variant_words", []):
                out.add(f"{n} {w}")
    return {p for p in out if " " in p}


def index_for(kits: list[dict], claims: S.Claims) -> KitIndex:
    ks = []
    for hk in claims.kits:
        ks.append({"pid": "H:" + hk["id"], "names": hk["names"], "moves": hk["moves"], "shouts": hk["shouts"]})
    for k in kits:
        c = k["char"]
        names = list(c.kit_names)  # type: ignore[attr-defined]
        names += [kitform(c.display), k["pid"].replace("_", " ")]
        ks.append({"pid": k["pid"], "names": sorted(set(n for n in names if n)), "moves": [{"name": m.name, "tags": m.tags} for m in k["moves"]], "shouts": []})
    return KitIndex(ks)


def collision_pass(kits: list[dict], claims: S.Claims, en: S.Enums, rules: Rules):
    """No known phrase (an icon / costume / power name, a hand shout) may read as a derived kit move.

    * the phrase belongs to hand content (a power name, a shout): the alias is dropped — hand wins;
    * an alias that isn't what people call the character: dropped;
    * the character's own name + a word of one of its move names ("electro bolt" is Electro's bolt
      icon): that move is renamed (the word is banned for the kit, which is rebuilt);
    * the character's own name + a slot word ("zelda shield", "zerg rush"): kept and logged — the
      kit move is the in-character reading (see the report's accepted list)."""
    phrases = sorted(known_phrases(claims, getattr(generate, "all_chars", [k["char"] for k in kits])))
    hand_phr = claims.names | claims.shouts
    # every character's own name must stay its bare name (the costume / icon summon)
    char_names = {kitform(n) for c in getattr(generate, "all_chars", [k["char"] for k in kits]) for n in c.names}
    dropped = []
    accepted = []
    by_pid = {k["pid"]: k for k in kits}
    for _round in range(6):
        ix = index_for(kits, claims)
        bad = []
        for ph in phrases:
            hit = ix.kit_move(ph)
            if hit and not hit[0].startswith("H:"):
                bad.append((ph, hit))
        changed = False
        for ph, (pid, slot, why, kitname) in bad:
            k = by_pid[pid]
            c = k["char"]
            canon = kitname in getattr(c, "canonical", set()) or kitname == kitform(c.display)
            if not canon or (why != "fuzzy" and (ph in hand_phr or kitform(ph) in char_names)):
                if kitname in c.kit_names:  # type: ignore[attr-defined]
                    c.kit_names.remove(kitname)  # type: ignore[attr-defined]
                    dropped.append((ph, c.display, kitname, k["moves"][slot].name, why))
                    changed = True
            elif why == "fuzzy":
                rest = ph[len(kitname):].split()
                ban = {w for w in rest if len(w) >= 3}
                old = getattr(c, "banned_words", set())
                if not ban - old:
                    continue
                c.banned_words = old | ban  # type: ignore[attr-defined]
                nk = build_kit(c, en, rules)
                if nk is not None:
                    k["moves"], k["trace"], k["fx"] = nk["moves"], nk["trace"], nk["fx"]
                    changed = True
            else:
                accepted.append((ph, c.display, k["moves"][slot].name))
            if kitform(c.display) not in c.kit_names and c.kit_names:  # type: ignore[attr-defined]
                c.display = c.kit_names[0].title()  # type: ignore[attr-defined]
        if not changed:
            break
    return dropped, sorted(set(accepted))


def render(kits: list[dict], claims: S.Claims) -> str:
    head = [
        "// GENERATED by tools/derive_kits/derive_kits.py — DO NOT EDIT BY HAND (re-run the generator).\n",
        "// DERIVED KITS (AUTO-KITS, docs/toolbox/AUTO_KITS.md): a `kind: Kit` for the most-named characters no hand-made\n",
        "// power or kit names. \"<character> attack / dash / block / counter / combo / ultimate\" fires an in-character\n",
        "// move derived from its element, weapon, mechanics, size, temperament and pack. The bare name still summons\n",
        "// the costume construct (a kit never grants). Hand content always wins: the generator skips anything a hand\n",
        "// file names, and this file sorts LAST (ties go to the earlier file).\n",
        f"// {len(kits)} kits, {sum(len(k['moves']) for k in kits)} moves; budget {getattr(generate, 'budget', 0)} (MAX_POWERS - hand entries - headroom).\n",
        "// COMPACT: `derived` = element / temperament / size / colour / impact frame + (template, move name) per slot\n",
        "// (Primary, Movement, Defense+Counter, Combo, Ultimate, bonus); bd_sim powers/derive.rs expands them into moves.\n",
        "[\n",
    ]
    body = [ron_kit(k, k["pid"]) for k in kits]
    return "".join(head) + "".join(body) + "]\n"


def show(k: dict) -> str:
    c = k["char"]
    lines = [f"{c.display}  [{c.pack}]  names={c.kit_names}  fx={k['fx'] or '-'}",
             f"   data: form={c.form} elem={c.element}->{k['prof'].elem} temper={c.temperament} mech={sorted(c.mech)} weapon={k['prof'].weapon} flav={k['prof'].flav} notes={c.notes[:60]!r}"]
    for mv in k["moves"]:
        lines.append(f"   {('/'.join(mv.tags) or 'bonus'):16} {mv.name:24} {mv.tpl}")
    return "\n".join(lines)


def self_check(kits: list[dict], claims: S.Claims) -> list[str]:
    """Every derived kit answers the slot words with the right move (simulated toolbox.rs)."""
    ix = index_for(kits, claims)
    errs = []
    probe = [("attack", "Primary"), ("punch", "Primary"), ("dash", "Movement"), ("teleport", "Movement"), ("dodge", "Movement"),
             ("block", "Defense"), ("parry", "Defense"), ("counter", "Counter"), ("combo", "Combo"), ("flurry", "Combo"),
             ("ultimate", "Ultimate"), ("special", "Ultimate"), ("super", "Ultimate"), ("ult", "Ultimate")]
    for k in kits:
        name = k["char"].kit_names[0]
        for w, tag in probe:
            hit = ix.kit_move(f"{name} {w}")
            if not hit or hit[0] != k["pid"]:
                errs.append(f"{name} {w}: reads as {hit}")
                continue
            mv = k["moves"][hit[1]]
            if tag not in mv.tags:
                errs.append(f"{name} {w}: fired {mv.name} {mv.tags}, wanted {tag}")
    return errs


def main() -> int:
    try:
        sys.stdout.reconfigure(encoding="utf-8")  # type: ignore[attr-defined]
    except Exception:
        pass
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="exit 1 if the checked-in file differs from a fresh generation")
    ap.add_argument("--show", action="append", default=[], help="print the kit of this character (any alias)")
    ap.add_argument("--sample", type=int, default=0, help="print N random kits (spot-check)")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--stats", action="store_true")
    ap.add_argument("--dry", action="store_true", help="don't write the file")
    ap.add_argument("--out", default=OUT, help="write here instead of assets/powers/ (review / timing)")
    a = ap.parse_args()

    en, claims, chars, kept, skipped_hand, failed, kits, dropped = generate()
    text = render(kits, claims)
    errs = self_check(kits, claims)

    print(f"characters found: {len(chars)} (icons + names lists + costumes, merged)")
    print(f"  skipped (a hand-made power / kit names them): {len(skipped_hand)}")
    print(f"  no usable name left: {generate.no_name}   no kit (template failure): {len(failed)}")
    print(f"derived kits: {len(kits)}  moves: {sum(len(k['moves']) for k in kits)}  names: {sum(len(k['char'].kit_names) for k in kits)}")
    print(f"hand kits / powers: {len(claims.kits)} kits in {len(claims.files)} files; total table ≈ {len(kits)} derived + hand")
    print(f"collision drops (alias removed because a known phrase would read as its move): {len(dropped)}")
    print(f"not characters (dropped): {len(generate.not_chars)}   over budget {generate.budget} (least named, cut): {len(generate.cut)}")
    if errs:
        print(f"SELF-CHECK FAILURES: {len(errs)}")
        for e in errs[:30]:
            print("   ", e)
    if a.stats:
        from collections import Counter
        for slot in ("primary", "movement", "defense", "combo", "ultimate", "bonus"):
            cnt = Counter(t.split("=")[1] for k in kits for t in k["trace"] if t.startswith(slot + "="))
            print(f"  {slot:9}", ", ".join(f"{f}:{n}" for f, n in cnt.most_common()))
        for d in dropped[:40]:
            print("   drop:", d)
    for q in a.show:
        qn = S.norm_words(q)
        for k in kits:
            if qn in k["char"].kit_names or qn == k["char"].key:
                print(show(k))
                break
        else:
            print(f"(no derived kit answers to {q!r})")
    if a.sample:
        rng = random.Random(a.seed)
        for k in rng.sample(kits, min(a.sample, len(kits))):
            print(show(k))
    if a.check:
        cur = S.read(OUT) if os.path.exists(OUT) else ""
        if cur != text:
            print(f"STALE: {OUT_NAME} differs from a fresh generation — run tools/derive_kits/derive_kits.py")
            return 1
        print("up to date")
        return 1 if errs else 0
    if not a.dry and not a.show and not a.sample:
        with open(a.out, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print(f"wrote {os.path.relpath(a.out, S.ROOT)} ({len(text) // 1024} KiB)")
    return 1 if errs else 0


if __name__ == "__main__":
    sys.exit(main())
