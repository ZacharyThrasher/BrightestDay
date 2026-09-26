"""Two-mode (Transforms) icon authoring for Brightest Day.

A transformer is a set of rigid PANELS. Each panel is authored once, in ALT-mode coordinates (the
vehicle / beast), and then *placed* in ROBOT mode by a rigid motion: rotate about the panel's alt anchor,
then move the anchor to its robot position. So every shared panel is literally the same prims in both
modes (same part id, same prim order) -- the transformation can fold/slide/rotate each panel rigidly.

Parts that exist in only one mode (the robot head that "unfolds from inside", fists, the alt-mode hidden
chassis frame...) get their own ids (schema: docs/ICONS_SCHEMA.md).

Units: construct voxels (0.25 m); +Y up, -Z forward; origin = feet / wheels-on-ground centre.
Euler convention: glam XYZ (R = Rx*Ry*Rz), degrees.
"""
import math

import numpy as np

TINTS = {"P": "Primary", "A": "Accent", "D": "Dark", "C": "Core"}
ROLES = {"Head", "Jaw", "Tail", "LegFrontLeft", "LegFrontRight", "LegBackLeft", "LegBackRight", "ArmLeft",
         "ArmRight", "WingLeft", "WingRight", "Weapon", "Seat", "Wheel", "Exhaust", "Cloth", "Other"}


def fnum(x):
    x = round(float(x), 2)
    if x == 0:
        x = 0.0
    s = ("%.2f" % x).rstrip("0")
    return s + "0" if s.endswith(".") else s


def vec(t):
    return "(%s, %s, %s)" % tuple(fnum(a) for a in t)


def rmat(deg):
    x, y, z = [math.radians(d) for d in deg]
    rx = np.array([[1, 0, 0], [0, math.cos(x), -math.sin(x)], [0, math.sin(x), math.cos(x)]])
    ry = np.array([[math.cos(y), 0, math.sin(y)], [0, 1, 0], [-math.sin(y), 0, math.cos(y)]])
    rz = np.array([[math.cos(z), -math.sin(z), 0], [math.sin(z), math.cos(z), 0], [0, 0, 1]])
    return rx @ ry @ rz


def euler_xyz(R):
    """Inverse of rmat (R = Rx*Ry*Rz) -> degrees."""
    sb = max(-1.0, min(1.0, R[0, 2]))
    b = math.asin(sb)
    if abs(math.cos(b)) > 1e-6:
        a = math.atan2(-R[1, 2], R[2, 2])
        c = math.atan2(-R[0, 1], R[0, 0])
    else:
        a = math.atan2(R[2, 1], R[1, 1])
        c = 0.0
    out = []
    for d in (math.degrees(a), math.degrees(b), math.degrees(c)):
        r = round(d)
        out.append(float(r) if abs(d - r) < 1e-3 else d)
    return tuple(out)


def axes(xto, yto, zto):
    """Euler (deg) of the rotation sending alt X, Y, Z to the given robot directions (must be proper)."""
    R = np.array([xto, yto, zto], float).T
    assert abs(np.linalg.det(R) - 1) < 1e-4, (xto, yto, zto)
    return euler_xyz(R)


def _selftest():
    rng = np.random.default_rng(1)
    for _ in range(200):
        e = rng.uniform(-180, 180, 3)
        e[1] = rng.uniform(-89, 89)
        R = rmat(e)
        assert np.allclose(rmat(euler_xyz(R)), R, atol=1e-6)
    assert np.allclose(rmat(euler_xyz(rmat((90, 90, 0)))), rmat((90, 90, 0)), atol=1e-6)


_selftest()


class Prim:
    __slots__ = ("kind", "c", "s", "rot", "t", "part", "sub")

    def __init__(self, kind, c, s, rot, t, part, sub):
        assert all(a > 0 for a in s), (kind, c, s)
        assert t in TINTS, t
        self.kind, self.c, self.s, self.rot, self.t, self.part, self.sub = kind, np.array(c, float), tuple(s), tuple(rot or (0, 0, 0)), t, part, sub

    def moved(self, R, anchor, at):
        c = R @ (self.c - anchor) + at
        rot = euler_xyz(R @ rmat(self.rot))
        return Prim(self.kind, c, self.s, rot, self.t, self.part, self.sub)

    def ron(self):
        s = "(kind: %s, center: %s, size: %s" % (self.kind, vec(self.c), vec(self.s))
        if any(abs(a) > 1e-6 for a in self.rot):
            s += ", rot_deg: %s" % vec(self.rot)
        if self.t != "P":
            s += ", tint: %s" % TINTS[self.t]
        if self.part:
            s += ", part: %d" % self.part
        if self.sub:
            s += ", subtract: true"
        return s + ")"


class Builder:
    """Prim helpers writing into a list with a fixed part id."""

    def __init__(self, sink, part):
        self.sink, self.part = sink, part

    def add(self, kind, c, s, t="P", rot=None, sub=False):
        self.sink.append(Prim(kind, c, s, rot, t, self.part, sub))
        return self

    def bx(self, c, s, t="P", rot=None, sub=False):
        return self.add("Box", c, s, t, rot, sub)

    def el(self, c, s, t="P", rot=None, sub=False):
        return self.add("Ellipsoid", c, s, t, rot, sub)

    def cy(self, c, s, t="P", rot=None, sub=False):
        return self.add("Cylinder", c, s, t, rot, sub)

    def cn(self, c, s, t="P", rot=None, sub=False):
        return self.add("Cone", c, s, t, rot, sub)

    def cx(self, c, d, l, t="P", sub=False):
        """cylinder along X (wheels)"""
        return self.cy(c, (d, l, d), t, (0, 0, 90), sub)

    def cz(self, c, d, l, t="P", sub=False):
        """cylinder along Z"""
        return self.cy(c, (d, l, d), t, (90, 0, 0), sub)

    def cone(self, c, d, l, t="P", dir="-z", d2=None):
        rot = {"-z": (-90, 0, 0), "+z": (90, 0, 0), "-x": (0, 0, 90), "+x": (0, 0, -90), "+y": None,
               "-y": (180, 0, 0)}[dir]
        return self.cn(c, (d, l, d2 if d2 else d), t, rot)

    def wheel(self, c, d=3.2, w=1.4, hub="A"):
        self.cx(c, d, w, "D")
        return self.cx(c, d * 0.45, w + 0.2, hub)


class Mode:
    def __init__(self):
        self.prims = []
        self.parts = {}  # id -> (parent, pivot, role)

    def part(self, pid, parent, pivot, role):
        assert role in ROLES, role
        assert pid != 0
        self.parts[pid] = (parent, tuple(pivot), role)

    def ron(self, ind):
        pad = " " * ind
        out = [pad + "prims: ["]
        out += [pad + "    " + p.ron() + "," for p in self.prims]
        out.append(pad + "],")
        if self.parts:
            out.append(pad + "parts: [")
            # parents before children
            order, seen = [], {0}
            pending = sorted(self.parts)
            while pending:
                prog = False
                for pid in list(pending):
                    if self.parts[pid][0] in seen:
                        order.append(pid)
                        seen.add(pid)
                        pending.remove(pid)
                        prog = True
                if not prog:
                    raise ValueError("part parent cycle / missing parent: %s" % pending)
            for pid in order:
                par, piv, role = self.parts[pid]
                out.append(pad + "    (id: %d, parent: %d, pivot: %s, role: %s)," % (pid, par, vec(piv), role))
            out.append(pad + "],")
        return "\n".join(out)


class TF:
    """Two-mode model. `panel()` = shared rigid panel; `alt_only()` / `robot_only()` = one-mode parts."""

    def __init__(self, k=1.0):
        """k = uniform output scale (author in ~robot-28-units-tall space, emit at k x for detail)."""
        self.alt = Mode()
        self.robot = Mode()
        self.k = k
        self._panels = {}  # id -> (prims list, anchor)

    # -- shared panels ---------------------------------------------------------------------------
    def panel(self, pid, anchor=(0, 0, 0)):
        """Author a shared panel in ALT coordinates. Returns a Builder. Part 0 allowed (root)."""
        assert pid not in self._panels, pid
        prims = []
        self._panels[pid] = (prims, np.array(anchor, float))
        return Builder(prims, pid)

    def alt_part(self, pid, parent=0, pivot=None, role="Other"):
        piv = pivot if pivot is not None else self._panels[pid][1]
        self.alt.part(pid, parent, piv, role)

    def place(self, pid, at, rot=(0, 0, 0), parent=0, pivot=None, role="Other"):
        """Robot placement: rotate panel `pid` by `rot` about its alt anchor and move the anchor to `at`."""
        self._placed = getattr(self, "_placed", {})
        self._placed[pid] = (np.array(at, float), rmat(rot))
        self._rot = getattr(self, "_rot", {})
        self._rot[pid] = rot
        if pid != 0:
            self.robot.part(pid, parent, pivot if pivot is not None else at, role)

    def robot_pt(self, pid, p):
        """Where alt-mode point p of panel `pid` ends up in robot mode."""
        at, R = self._placed[pid]
        return tuple(R @ (np.array(p, float) - self._panels[pid][1]) + at)

    def follow(self, pid, leader, parent=None, pivot=None, role="Other"):
        """Place panel `pid` with the same rigid motion as `leader` (e.g. wheels riding a leg)."""
        at_l, R = self._placed[leader]
        anchor_l = self._panels[leader][1]
        anchor = self._panels[pid][1]
        at = R @ (anchor - anchor_l) + at_l
        self.place(pid, at, self._rot[leader], parent=leader if parent is None else parent,
                   pivot=pivot if pivot is not None else tuple(at), role=role)

    # -- one-mode parts ------------------------------------------------------------------------
    def alt_only(self, pid, parent=0, pivot=(0, 0, 0), role="Other"):
        if pid != 0:
            self.alt.part(pid, parent, pivot, role)
        return Builder(self.alt.prims, pid)

    def robot_only(self, pid, parent=0, pivot=(0, 0, 0), role="Other"):
        if pid != 0:
            self.robot.part(pid, parent, pivot, role)
        return Builder(self.robot.prims, pid)

    def finish(self):
        placed = getattr(self, "_placed", {})
        for pid, (_, anchor) in self._panels.items():
            if pid != 0 and pid not in self.alt.parts:
                self.alt.part(pid, 0, tuple(anchor), "Other")
        alt_extra, rob_extra = list(self.alt.prims), list(self.robot.prims)
        self.alt.prims, self.robot.prims = [], []
        for pid in sorted(self._panels):
            prims, anchor = self._panels[pid]
            assert pid in placed, "panel %d has no robot placement" % pid
            at, R = placed[pid]
            self.alt.prims += prims
            self.robot.prims += [p.moved(R, anchor, at) for p in prims]
        # subtractive prims must come after what they carve: keep one-mode extras after panels, but
        # hoist additive extras before subtractive panel prims is not needed (panels rarely subtract).
        self.alt.prims += alt_extra
        self.robot.prims += rob_extra
        for m, name in ((self.alt, "alt"), (self.robot, "robot")):
            ids = {p.part for p in m.prims}
            for pid in ids - {0}:
                assert pid in m.parts, "%s: part %d has prims but no Part entry" % (name, pid)
            assert len(m.prims) <= 120, "%s: %d prims > 120" % (name, len(m.prims))
            if self.k != 1.0:
                for p in m.prims:
                    p.c = p.c * self.k
                    p.s = tuple(a * self.k for a in p.s)
                m.parts = {i: (par, tuple(a * self.k for a in piv), r) for i, (par, piv, r) in m.parts.items()}
        return self


def extent(mode):
    """(lo, hi) like bd_sim shape_extent (rotation-agnostic)."""
    lo = np.full(3, np.inf)
    hi = np.full(3, -np.inf)
    for p in mode.prims:
        if p.sub:
            continue
        h = np.array(p.s) * 0.5 if not any(abs(a) > 1e-6 for a in p.rot) else np.full(3, max(p.s) * 0.5)
        lo = np.minimum(lo, p.c - h)
        hi = np.maximum(hi, p.c + h)
    return lo, hi


# characteristic axis per form (bd_sim form table: dims component == 1.0)
FORM_AXIS = {"Vehicle": 2, "Kaiju": 1, "Biped": 1, "Quadruped": 2, "Bird": None, "Gun": 2, "Cannon": 2,
             "Sword": 2, "Axe": 2, "Ball": None, "Staff": 2, "Insect": None, "Arachnid": None, "Serpent": None}


def robot_height_m(tf, form, scale):
    """Robot height in metres if the alt shape's uniform factor (to recipe.scale) is used for both modes."""
    lo, hi = extent(tf.alt)
    ax = FORM_AXIS.get(form)
    size = (hi - lo) * 0.25
    ext = size.max() if ax is None else size[ax]
    f = scale / ext
    rlo, rhi = extent(tf.robot)
    return (rhi[1] - rlo[1]) * 0.25 * f, (hi - lo) * 0.25 * f


# ---------------------------------------------------------------------------------- RON writer

import os as _os
import re as _re

_SRC = open(_os.path.join(_os.path.dirname(_os.path.abspath(__file__)), "..", "..", "..", "crates", "bd_core", "src",
                          "mechanics.rs"), encoding="utf-8").read()


def _variants(name):
    body = _re.search(r"pub enum %s \{(.*?)\n\}" % name, _SRC, _re.S).group(1)
    return _re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*[,{(]", body, _re.M)


MECH_ORDER = _variants("Mechanic")
FEAT_ORDER = _variants("Feature")
FORMS = set(_variants("Form"))
TEMPS = set(_variants("Temperament"))
ELEMS = set(_variants("Element"))


def recipe_ron(r, ind=8):
    assert r["form"] in FORMS, r["form"]
    for m in r["mech"]:
        assert m in MECH_ORDER, m
    for f_ in r.get("feat", []):
        assert f_ in FEAT_ORDER, f_
    assert r.get("temp") in TEMPS | {None}, r.get("temp")
    assert r.get("elem", "Light") in ELEMS, r.get("elem")
    r = dict(r, mech=sorted(set(r["mech"]), key=MECH_ORDER.index), feat=sorted(set(r.get("feat", [])), key=FEAT_ORDER.index))
    pad = " " * ind
    lines = ["("]
    lines.append(pad + "    name: %s," % q(r["name"]))
    lines.append(pad + "    form: %s," % r["form"])
    lines.append(pad + "    mechanics: [%s]," % ", ".join(r["mech"]))
    lines.append(pad + "    element: %s," % r.get("elem", "Light"))
    lines.append(pad + "    scale: %s," % fnum(r["scale"]))
    lines.append(pad + "    mass: %s," % fnum(r["mass"]))
    lines.append(pad + "    count: %d," % r.get("count", 1))
    acc = r.get("accent")
    lines.append(pad + "    accent: %s," % ("Some(%s)" % vec(acc) if acc else "None"))
    if r.get("icon"):
        lines.append(pad + "    model: Icon(%s)," % q(r["icon"]))
    else:
        lines.append(pad + "    model: Procedural(seed: %d)," % r.get("seed", 1))
    if r.get("payload"):
        lines.append(pad + "    payload: Some(%s)," % recipe_ron(r["payload"], ind + 4))
    if r.get("feat"):
        lines.append(pad + "    features: [%s]," % ", ".join(r["feat"]))
    if r.get("temp"):
        lines.append(pad + "    temperament: %s," % r["temp"])
        lines.append(pad + "    intensity: %s," % fnum(r.get("inten", 0.7)))
    lines.append(pad + ")")
    return "\n".join(lines)


def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def entry_ron(names, recipe, tf=None, shape=None, source="Transformers", comment=None):
    """tf = two-mode TF (alt shape + robot alt_shape); shape = single-mode Mode (items)."""
    out = []
    if comment:
        for c in comment.strip().split("\n"):
            out.append("    // " + c)
    out.append("    IconEntry(")
    out.append("        names: [%s]," % ", ".join(q(n) for n in names))
    out.append("        recipe: " + recipe_ron(recipe) + ",")
    if tf is not None:
        out.append("        shape: Some((\n" + tf.alt.ron(12) + "\n        )),")
        out.append("        alt_shape: Some((\n" + tf.robot.ron(12) + "\n        )),")
    elif shape is not None:
        out.append("        shape: Some((\n" + shape.ron(12) + "\n        )),")
    else:
        out.append("        shape: None,")
    out.append("        source: %s," % q(source))
    out.append("    ),")
    return "\n".join(out)
