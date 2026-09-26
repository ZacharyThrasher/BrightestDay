"""Helpers for authoring Brightest Day icon RON (film/TV pack)."""
import math, os, re, zlib

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))

SRC = open(os.path.join(ROOT, "crates", "bd_core", "src", "mechanics.rs"), encoding="utf-8").read()


def _variants(name):
    body = re.search(r"pub enum %s \{(.*?)\n\}" % name, SRC, re.S).group(1)
    return re.findall(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*[,{(]", body, re.M)


FORM_ORDER = _variants("Form")
MECH_ORDER = _variants("Mechanic")
FEAT_ORDER = _variants("Feature")
TEMPS = set(_variants("Temperament"))
ELEMS = set(_variants("Element"))
FORMS = set(FORM_ORDER)
MECHS = set(MECH_ORDER)
FEATS = set(FEAT_ORDER)
ROLES = {"Head", "Jaw", "Tail", "LegFrontLeft", "LegFrontRight", "LegBackLeft", "LegBackRight", "ArmLeft",
         "ArmRight", "WingLeft", "WingRight", "Weapon", "Other"}
TINTS = {"P": "Primary", "A": "Accent", "D": "Dark", "C": "Core"}
SEAT = 255


def f(x):
    x = round(float(x), 2)
    if x == 0:
        x = 0.0
    s = ("%.2f" % x).rstrip("0")
    if s.endswith("."):
        s += "0"
    return s


def v(t):
    return "(%s, %s, %s)" % tuple(f(a) for a in t)


class Shape:
    def __init__(self):
        self.prims = []
        self.parts = []

    def add(self, kind, c, s, t="P", rot=None, part=0, sub=False):
        assert all(a > 0 for a in s), (kind, c, s)
        assert t in TINTS, t
        self.prims.append((kind, tuple(c), tuple(s), tuple(rot) if rot else None, t, part, sub))
        return self

    def bx(self, c, s, t="P", rot=None, part=0, sub=False):
        return self.add("Box", c, s, t, rot, part, sub)

    def el(self, c, s, t="P", rot=None, part=0, sub=False):
        return self.add("Ellipsoid", c, s, t, rot, part, sub)

    def cy(self, c, s, t="P", rot=None, part=0, sub=False):
        return self.add("Cylinder", c, s, t, rot, part, sub)

    def cn(self, c, s, t="P", rot=None, part=0, sub=False):
        return self.add("Cone", c, s, t, rot, part, sub)

    def cz(self, c, d, l, t="P", part=0, sub=False):
        """cylinder along Z"""
        return self.cy(c, (d, l, d), t, (90, 0, 0), part, sub)

    def cx(self, c, d, l, t="P", part=0, sub=False):
        """cylinder along X"""
        return self.cy(c, (d, l, d), t, (0, 0, 90), part, sub)

    def cone(self, c, d, l, t="P", dir="-z", part=0, d2=None):
        """cone with tip pointing along dir; d = base diameter (d2 = second base axis)"""
        rot = {"-z": (-90, 0, 0), "+z": (90, 0, 0), "-x": (0, 0, 90), "+x": (0, 0, -90), "+y": None,
               "-y": (180, 0, 0)}[dir]
        return self.cn(c, (d, l, d2 if d2 else d), t, rot, part)

    def seg(self, a, b, w, h=None, t="P", part=0, kind="Box", over=0.6):
        """Prim spanning a->b. a,b must differ in only two axes. w = width, h = depth."""
        h = h if h is not None else w
        d = [b[i] - a[i] for i in range(3)]
        L = math.sqrt(sum(x * x for x in d)) + over
        c = [(a[i] + b[i]) / 2 for i in range(3)]
        # q = Rx(th) * Rz(ps) takes local +Y to d (bd_shapes: Euler XYZ intrinsic = Rx*Ry*Rz)
        th = math.degrees(math.atan2(d[2], d[1]))
        ps = math.degrees(math.atan2(-d[0], math.hypot(d[1], d[2])))
        return self.add(kind, c, (w, L, h), t, (th, 0, ps), part)

    def part(self, id, parent, pivot, role):
        assert role in ROLES
        assert id not in [p[0] for p in self.parts]
        self.parts.append((id, parent, tuple(pivot), role))
        return id

    def seat(self, parent, pos):
        self.parts.append((SEAT, parent, tuple(pos), "Other"))

    def ron(self, ind):
        p = " " * ind
        out = ["Some((", p + "    prims: ["]
        for kind, c, s, rot, t, part, sub in self.prims:
            items = ["kind: %s" % kind, "center: %s" % v(c), "size: %s" % v(s)]
            if rot and any(abs(r) > 1e-6 for r in rot):
                items.append("rot_deg: %s" % v(rot))
            items.append("tint: %s" % TINTS[t])
            if part:
                items.append("part: %d" % part)
            if sub:
                items.append("subtract: true")
            out.append(p + "        (" + ", ".join(items) + "),")
        out.append(p + "    ],")
        if self.parts:
            out.append(p + "    parts: [")
            for id, parent, piv, role in self.parts:
                out.append(p + "        (id: %d, parent: %d, pivot: %s, role: %s)," % (id, parent, v(piv), role))
            out.append(p + "    ],")
        out.append(p + "))")
        return "\n".join(out)

    def extent(self):
        lo = [1e9] * 3
        hi = [-1e9] * 3
        for kind, c, s, rot, t, part, sub in self.prims:
            if sub:
                continue
            h = [x / 2 for x in s] if not rot else [max(s) / 2] * 3
            for i in range(3):
                lo[i] = min(lo[i], c[i] - h[i])
                hi[i] = max(hi[i], c[i] + h[i])
        return lo, hi


def slug(n):
    return re.sub(r"[^a-z0-9]+", "_", n.lower()).strip("_")


def seed_of(n):
    return zlib.crc32(n.encode()) % 100000


class Recipe:
    def __init__(self, name, form, mech=(), el="Light", scale=1.0, mass=10.0, count=1, accent=None,
                 feat=(), temp="Calm", inten=0.0, payload=None, seed=None):
        assert form in FORMS, form
        for m in mech:
            assert m in MECHS, m
        for x in feat:
            assert x in FEATS, x
        assert temp in TEMPS, temp
        assert el in ELEMS, el
        assert len(set(mech)) == len(mech), (name, mech)
        if temp != "Calm":
            assert inten > 0, name
        self.name, self.form, self.el = name, form, el
        self.mech = sorted(mech, key=MECH_ORDER.index)
        self.feat = sorted(feat, key=FEAT_ORDER.index)
        self.scale, self.mass, self.count, self.accent = scale, mass, count, accent
        self.temp, self.inten, self.payload = temp, inten, payload
        self.seed = seed if seed is not None else seed_of(name)
        self.icon = None

    def ron(self, ind):
        p = " " * ind
        lines = ["(",
                 p + '    name: "%s",' % self.name,
                 p + "    form: %s," % self.form,
                 p + "    mechanics: [%s]," % ", ".join(self.mech),
                 p + "    element: %s," % self.el,
                 p + "    scale: %s," % f(self.scale),
                 p + "    mass: %s," % f(self.mass),
                 p + "    count: %d," % self.count,
                 p + "    accent: %s," % (("Some(%s)" % v(self.accent)) if self.accent else "None"),
                 p + "    model: %s," % (('Icon("%s")' % self.icon) if self.icon else "Procedural(seed: %d)" % self.seed)]
        if self.payload:
            lines.append(p + "    payload: Some(%s)," % self.payload.ron(ind + 4))
        if self.feat:
            lines.append(p + "    features: [%s]," % ", ".join(self.feat))
        if self.temp != "Calm" or self.inten:
            lines.append(p + "    temperament: %s," % self.temp)
            lines.append(p + "    intensity: %s," % f(self.inten))
        lines.append(p + ")")
        return "\n".join(lines)


ENTRIES = []


def E(names, recipe, source, shape=None):
    names = [n.lower().strip() for n in names]
    assert len(set(names)) == len(names), names
    if shape is not None:
        recipe.icon = slug(names[0])
        lo, hi = shape.extent()
        ext = max(hi[i] - lo[i] for i in range(3)) * 0.25
        if recipe.scale is None:
            cat_creature = recipe.form in ("Biped", "Kaiju", "Quadruped", "Bird", "Insect", "Arachnid", "Blob")
            recipe.scale = round((hi[1] - lo[1]) * 0.25 if cat_creature else ext, 1)
        # recipe.scale should roughly match the authored extent
        if not (0.6 * ext <= recipe.scale <= 1.6 * ext):
            print("WARN scale %s: recipe %.2f vs shape extent %.2f m" % (names[0], recipe.scale, ext))
        n = len(shape.prims)
        if n < 5 or n > 45:
            print("WARN prim count %s: %d" % (names[0], n))
    ENTRIES.append((names, recipe, shape, source))


def R(*a, **k):
    return Recipe(*a, **k)


def taken_names(exclude="film_tv"):
    """Names already owned by the base set and the other domain packs (they load first / own them)."""
    import glob
    root = os.path.join(ROOT, "assets")
    files = [os.path.join(root, "icons.ron")] + [g for g in glob.glob(os.path.join(root, "icons", "*.ron")) if exclude not in g]
    out = {}
    for fp in files:
        t = open(fp, encoding="utf-8").read()
        for m in re.finditer(r"names:\s*\[(.*?)\]", t, re.S):
            ns = re.findall(r'"([^"]*)"', m.group(1))
            for n in ns:
                out.setdefault(n.lower(), os.path.basename(fp) + ":" + ns[0])
    return out


def emit(path, drop_same=()):
    taken = taken_names()
    kept = []
    for names, r, s, src in ENTRIES:
        if names[0] in taken or names[0] in drop_same:
            print("DROP [%s] (owned by %s)" % (names[0], taken.get(names[0], "manual")))
            continue
        nn = [n for n in names if n not in taken]
        if len(nn) != len(names):
            print("  trim [%s]: %s" % (names[0], [n for n in names if n in taken]))
        kept.append((nn, r, s, src))
    ENTRIES[:] = kept
    seen = {}
    for names, r, s, src in ENTRIES:
        for n in names:
            assert n not in seen, "dup name %r in %s and %s" % (n, seen[n], names[0])
            seen[n] = names[0]
    out = ["// Brightest Day curated icons: film, TV & comics pack. Generated; see docs/ICONS_SCHEMA.md.",
           "// Names/titles only (no quotes/lyrics). Recipes use the closed bd_core sets; shapes are in",
           "// construct voxels (0.25 m), origin = grip (held) or feet-centre (creatures/structures), -Z forward.",
           "["]
    for names, r, s, src in ENTRIES:
        out.append("    IconEntry(")
        out.append("        names: [%s]," % ", ".join('"%s"' % n.replace('"', '\\"') for n in names))
        out.append("        recipe: %s," % r.ron(8))
        out.append("        shape: %s," % (s.ron(8) if s else "None"))
        out.append('        source: "%s",' % src)
        out.append("    ),")
    out.append("]")
    open(path, "w", encoding="utf-8", newline="\n").write("\n".join(out) + "\n")
    return len(ENTRIES), sum(1 for e in ENTRIES if e[2])
