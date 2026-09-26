"""TV / movie ("shows") icon authoring helpers (Brightest Day). Shape builder + RON writer.
(Adapted from tools/icons/dc/dlib.py; the cross-pack name check also covers every *_names.txt.)

Units = construct voxels (0.25 m). +Y up, -Z forward, left = -X.
Origins: held items = grip (blade/point toward -Z, strike face -Y); creatures/structures = feet centre;
vehicles = wheels-on-ground centre; objects = centre.
Icons are rescaled uniformly to `recipe.scale` along the form's characteristic axis, so authoring
resolution == voxel resolution: humanoids ~22-28 units tall, held items ~12-30 units long.
Euler convention: glam XYZ (R = Rx*Ry*Rz), degrees.
"""
import math
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

T = {"P": "Primary", "A": "Accent", "D": "Dark", "C": "Core"}
ROLES = {"Head", "Jaw", "Tail", "LegFrontLeft", "LegFrontRight", "LegBackLeft", "LegBackRight", "ArmLeft",
         "ArmRight", "WingLeft", "WingRight", "Weapon", "Seat", "Wheel", "Exhaust", "Cloth", "Other"}
SEAT = 255

ALONG_Z = (90, 0, 0)     # cylinder axis along Z
ALONG_X = (0, 0, 90)     # cylinder axis along X
TIP_NZ = (-90, 0, 0)     # cone tip toward -Z
TIP_PZ = (90, 0, 0)
TIP_NY = (180, 0, 0)
TIP_PX = (0, 0, -90)
TIP_NX = (0, 0, 90)


def fnum(x):
    x = round(float(x), 2)
    if x == 0:
        x = 0.0
    s = ("%.2f" % x).rstrip("0")
    return s + "0" if s.endswith(".") else s


def vec(t):
    return "(%s, %s, %s)" % tuple(fnum(a) for a in t)


class S:
    """A ShapeSpec under construction. `self.p` = current part id for new prims."""

    def __init__(self, k=1.5):
        self.prims = []
        self.parts = {}
        self.p = 0
        self.k = k        # uniform output scale (author at a comfortable size, emit at k x for detail)

    def on(self, pid):
        self.p = pid
        return self

    def add(self, kind, c, s, t="P", r=(0, 0, 0), sub=False, part=None):
        assert t in T, t
        assert all(a > 0 for a in s), (kind, c, s)
        self.prims.append(dict(kind=kind, c=tuple(float(v) for v in c), s=tuple(float(v) for v in s),
                               r=tuple(float(v) for v in r), t=t, part=self.p if part is None else part, sub=sub))
        return self

    def box(self, c, s, t="P", r=(0, 0, 0), sub=False):
        return self.add("Box", c, s, t, r, sub)

    def ell(self, c, s, t="P", r=(0, 0, 0), sub=False):
        return self.add("Ellipsoid", c, s, t, r, sub)

    def cyl(self, c, s, t="P", r=(0, 0, 0), sub=False):
        return self.add("Cylinder", c, s, t, r, sub)

    def cone(self, c, s, t="P", r=(0, 0, 0), sub=False):
        return self.add("Cone", c, s, t, r, sub)

    def cz(self, c, d, l, t="P", sub=False):
        return self.cyl(c, (d, l, d), t, ALONG_Z, sub)

    def cx(self, c, d, l, t="P", sub=False):
        return self.cyl(c, (d, l, d), t, ALONG_X, sub)

    def seg(self, a, b, w, h=None, t="P", kind="Box"):
        """Prim whose local Y runs from a to b (w = local x size, h = local z size)."""
        h = w if h is None else h
        ax, ay, az = a
        bx, by, bz = b
        dx, dy, dz = bx - ax, by - ay, bz - az
        L = math.sqrt(dx * dx + dy * dy + dz * dz)
        # Build rotation taking +Y to d: R = Rx(a)*Ry(0)*Rz(c) with XYZ order.
        # Use yaw about Y then tilt: rotate +Y by Rz(-tilt) toward +X, then Ry(yaw).
        # glam XYZ = Rx*Ry*Rz, so express as (0, yaw, -tilt) (Rz applied first, then Ry).
        horiz = math.sqrt(dx * dx + dz * dz)
        tilt = math.degrees(math.atan2(horiz, dy))
        yaw = math.degrees(math.atan2(-dz, dx)) if horiz > 1e-6 else 0.0
        c = ((ax + bx) / 2, (ay + by) / 2, (az + bz) / 2)
        return self.add(kind, c, (w, max(L, 0.5), h), t, (0, yaw, -tilt))

    def mirror(self, f):
        """f(sx) for sx in (-1, 1)."""
        for sx in (-1, 1):
            f(sx)
        return self

    def part(self, pid, parent, pivot, role):
        assert role in ROLES, role
        assert pid != 0
        self.parts[pid] = (parent, tuple(float(v) for v in pivot), role)
        return self

    def seat(self, parent, pivot):
        return self.part(SEAT, parent, pivot, "Seat")

    # ---------------------------------------------------------------------------- output
    def check(self, label):
        ids = {p["part"] for p in self.prims} - {0}
        for pid in ids:
            assert pid in self.parts, "%s: part %d has prims but no Part entry" % (label, pid)
        for pid, (par, _, _) in self.parts.items():
            assert par == 0 or par in self.parts, "%s: part %d parent %d missing" % (label, pid, par)
        assert 0 < len(self.prims) <= 120, "%s: %d prims" % (label, len(self.prims))

    def ron(self, ind=12):
        pad = " " * ind
        out = [pad + "prims: ["]
        for p in self.prims:
            k = self.k
            s = "(kind: %s, center: %s, size: %s" % (p["kind"], vec([a * k for a in p["c"]]), vec([a * k for a in p["s"]]))
            if any(abs(a) > 1e-6 for a in p["r"]):
                s += ", rot_deg: %s" % vec(p["r"])
            if p["t"] != "P":
                s += ", tint: %s" % T[p["t"]]
            if p["part"]:
                s += ", part: %d" % p["part"]
            if p["sub"]:
                s += ", subtract: true"
            out.append(pad + "    " + s + "),")
        out.append(pad + "],")
        if self.parts:
            out.append(pad + "parts: [")
            order, seen, pending = [], {0}, sorted(self.parts)
            while pending:
                prog = False
                for pid in list(pending):
                    if self.parts[pid][0] in seen:
                        order.append(pid)
                        seen.add(pid)
                        pending.remove(pid)
                        prog = True
                if not prog:
                    raise ValueError("part cycle: %s" % pending)
            for pid in order:
                par, piv, role = self.parts[pid]
                out.append(pad + "    (id: %d, parent: %d, pivot: %s, role: %s)," % (pid, par, vec([a * self.k for a in piv]), role))
            out.append(pad + "],")
        return "\n".join(out)


# ------------------------------------------------------------------------------------ recipes

_SRC = open(os.path.join(ROOT, "crates", "bd_core", "src", "mechanics.rs"), encoding="utf-8").read()


def _variants(name):
    body = re.search(r"pub enum %s \{(.*?)\n\}" % name, _SRC, re.S).group(1)
    return re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*[,{(]", body, re.M)


MECH_ORDER = _variants("Mechanic")
FEAT_ORDER = _variants("Feature")
FORMS = set(_variants("Form"))
TEMPS = set(_variants("Temperament"))
ELEMS = set(_variants("Element"))


def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def recipe_ron(r, ind=8):
    assert r["form"] in FORMS, r["form"]
    for m in r["mech"]:
        assert m in MECH_ORDER, m
    for f_ in r.get("feat", []):
        assert f_ in FEAT_ORDER, f_
    assert r.get("temp") in TEMPS | {None}, r.get("temp")
    assert r.get("elem", "Light") in ELEMS, r.get("elem")
    mech = sorted(set(r["mech"]), key=MECH_ORDER.index)
    feat = sorted(set(r.get("feat", [])), key=FEAT_ORDER.index)
    pad = " " * ind
    L = ["("]
    L.append(pad + "    name: %s," % q(r["name"]))
    L.append(pad + "    form: %s," % r["form"])
    L.append(pad + "    mechanics: [%s]," % ", ".join(mech))
    L.append(pad + "    element: %s," % r.get("elem", "Light"))
    L.append(pad + "    scale: %s," % fnum(r["scale"]))
    L.append(pad + "    mass: %s," % fnum(r["mass"]))
    L.append(pad + "    count: %d," % r.get("count", 1))
    acc = r.get("accent")
    L.append(pad + "    accent: %s," % ("Some(%s)" % vec(acc) if acc else "None"))
    if r.get("icon"):
        L.append(pad + "    model: Icon(%s)," % q(r["icon"]))
    else:
        L.append(pad + "    model: Procedural(seed: %d)," % r.get("seed", 1987))
    if r.get("payload"):
        L.append(pad + "    payload: Some(%s)," % recipe_ron(r["payload"], ind + 4))
    if feat:
        L.append(pad + "    features: [%s]," % ", ".join(feat))
    if r.get("grants"):
        L.append(pad + "    grants: Some(%s)," % q(r["grants"]))
    if r.get("temp"):
        L.append(pad + "    temperament: %s," % r["temp"])
        L.append(pad + "    intensity: %s," % fnum(r.get("inten", 0.7)))
    L.append(pad + ")")
    return "\n".join(L)


def entry_ron(names, recipe, shape=None, source="TV / Film", note=None, alt=None):
    for n in names:
        assert n == n.lower() and n.strip(), n
    out = []
    if note:
        for c in note.strip().split("\n"):
            out.append("    // " + c)
    out.append("    IconEntry(")
    out.append("        names: [%s]," % ", ".join(q(n) for n in names))
    out.append("        recipe: " + recipe_ron(recipe) + ",")
    if shape is not None:
        shape.check(names[0])
        out.append("        shape: Some((\n" + shape.ron(12) + "\n        )),")
    else:
        out.append("        shape: None,")
    if alt is not None:
        assert shape is not None and "Transforms" in recipe["mech"], names[0]
        alt.check(names[0] + " (alt)")
        out.append("        alt_shape: Some((\n" + alt.ron(12) + "\n        )),")
    out.append("        source: %s," % q(source))
    out.append("    ),")
    return "\n".join(out)


def R(name, form, mech, scale, mass, accent=None, icon=None, elem="Light", count=1, feat=(), temp=None, inten=0.7,
      payload=None, seed=None, grants=None, source=None):
    if seed is None:
        seed = sum((i + 1) * ord(c) for i, c in enumerate(name)) % 99991 + 7
    d = dict(name=name, form=form, mech=list(mech), scale=scale, mass=mass, accent=accent, elem=elem, count=count,
             feat=list(feat), seed=seed)
    if grants:
        d["grants"] = grants
    if source:
        d["source"] = source
    if icon:
        d["icon"] = icon
    if temp:
        d["temp"] = temp
        d["inten"] = inten
    if payload:
        d["payload"] = payload
    return d


def icon_id(name):
    return re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_")


# ------------------------------------------------------------------------ cross-pack name check

def norm_name(n):
    n = n.lower().replace("&", " and ")
    n = re.sub(r"[^a-z0-9' ]+", " ", n).replace("'", "")
    n = re.sub(r"\s+", " ", n).strip()
    for pre in ("the ", "a ", "an "):
        if n.startswith(pre):
            n = n[len(pre):]
    return n


def taken_names(exclude=("shows.ron",)):
    """name -> file for every icon name in other packs (+ powers.ron names), raw and normalised."""
    out = {}
    files = [os.path.join(ROOT, "assets", "icons.ron")]
    d = os.path.join(ROOT, "assets", "icons")
    files += [os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith(".ron") and f not in exclude]
    for f in files:
        txt = open(f, encoding="utf-8").read()
        for m in re.finditer(r"names:\s*\[(.*?)\]", txt, re.S):
            for n in re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(1)):
                out.setdefault(n.lower(), os.path.basename(f))
                out.setdefault(norm_name(n), os.path.basename(f))
    ptxt = open(os.path.join(ROOT, "assets", "powers.ron"), encoding="utf-8").read()
    for m in re.finditer(r"names:\s*\[(.*?)\]", ptxt, re.S):
        for n in re.findall(r'"([^"]*)"', m.group(1)):
            out.setdefault(n.lower(), "powers.ron")
            out.setdefault(norm_name(n), "powers.ron")
    for f in sorted(os.listdir(d)):
        if not f.endswith("_names.txt") or f.startswith("shows"):
            continue
        for line in open(os.path.join(d, f), encoding="utf-8"):
            if line.startswith("#") or "|" not in line:
                continue
            parts = [x.strip() for x in line.split("|")]
            for n in [parts[0]] + [a for a in re.split(r"[;,]", parts[1]) if a.strip()]:
                n = n.strip()
                if n:
                    out.setdefault(n.lower(), f)
                    out.setdefault(norm_name(n), f)
    return out


