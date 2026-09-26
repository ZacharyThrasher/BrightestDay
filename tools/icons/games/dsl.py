"""Tiny DSL that emits the Brightest Day video-game IconEntry pack (assets/icons/games.ron).

    python tools/icons/games/gen.py assets/icons/games.ron [gallery.txt]

R() builds a recipe, E() registers an entry, Shape builds a ShapeSpec (see shapes.py), emit() writes the
RON. Schema: docs/ICONS_SCHEMA.md. Euler convention for rot_deg: glam XYZ (R = Rx*Ry*Rz), degrees.
Procedural seeds are crc32(recipe name) % 1000; an Icon model's id is the entry's first name as authored
(it survives gen.py's name dedupe, so the id can outlive the name)."""
import math
import zlib

MECH_ORDER = ["Blunt", "Blade", "Pierce", "Guard", "Returning", "Ricochet", "Shoots", "Beam", "Explodes",
              "Grab", "Pull", "SlowFall", "Glide", "Thrust", "Bounce", "Launch", "Barrier", "Trap", "Homing",
              "ArticulatedTail", "Flies", "Charge", "Stomp", "Breath", "Swarm", "Spin", "DropFromSky", "Sticky",
              "Fast", "Heavy", "Constrict", "Walks", "Piloted", "Portal", "Attract", "Repel", "Shrink", "Grow",
              "Link"]
FEAT_ORDER = ["Wheels", "Treads", "Pistons", "Gears", "Boiler", "Smokestack", "Pipes", "Exhaust", "Propeller",
              "Rotor", "Jet", "Turbine", "Sails", "Mast", "Cockpit", "Cab", "Windows", "Turret", "Barrels", "Drill",
              "Saw", "CraneArm", "Chains", "Antenna", "Dish", "Headlight", "Plow", "Fins", "Spikes", "Armor", "Bell",
              "Cage"]
FORMS = set("""Hammer Sword Axe Spear Shield Gauntlet Whip Gun Bow Cannon Staff Disc Wings Parachute Jetpack Cape
Biped Kaiju Quadruped Serpent Bird Fish Arachnid Insect Blob Wall Ramp Pillar Dome Cage Platform Trampoline
Ball Anvil Bomb Boulder Rocket Vehicle""".split())
ELEMENTS = set("Light Fire Electric Ice Water Wind Earth Poison Psychic Shadow Atomic".split())
TEMPS = set("Calm Aggressive Fearful Protective Playful Lazy Chaotic Sneaky".split())
ROLES = set("Head Jaw Tail LegFrontLeft LegFrontRight LegBackLeft LegBackRight ArmLeft ArmRight WingLeft WingRight "
            "Weapon Other".split())
TINTS = {"P": "Primary", "A": "Accent", "D": "Dark", "C": "Core"}
KINDS = {"Box", "Ellipsoid", "Cylinder", "Cone"}

# Aliases too generic (or too quote-y) to own: E() drops them from entries and names.py never proposes them
# for games_names.txt. Recovered from the shipped pack (every authored name gen.py lists but games.ron lacks).
BLOCK = {n.strip() for n in """
guardian|shell|red mushroom|giant mushroom|tiny mushroom|blue mushroom|barrel|wooden barrel|aw man|pickaxe
|wither|warden|mace|glider|shockwave|rift|grappler|hog|puma|pelican|banshee|spartan|grunt|grunts|chainsaw
|chain saw|crucible|strider|barnacle|moonlight|rob|let me solo her|hunting horn|big bird|behemoth|leviathan
|jormungandr|world serpent|jörmungandr|rex|ray|bt|titan|stand by for titanfall|nevermind the black hole
|nerf this|ice wall|ryuu ga waga teki wo kurau|fishbones|baron|the bomb has been planted|rhino|minigun
|florida gator|nail|needle|hornet|git gud|aegis|cerberus|three-headed dog|three headed dog|zenith|yoyo|yo-yo
|ops|reinforce|charger|rover|anubis|vent|red bird|the goose|honk|octane|spring|rush|met|sonic boom|tyrant
|stars|get over here|ice ball|final destination|holy water|bouncer|clicker|leaper|rocketeer|tether|symbiote
|owlbear|owl bear|mind flayer|illithid|nautiloid""".replace("\n", "").split("|")}

HEADER = "// Brightest Day — video game icon pack (domain: games). Generated; see docs/ICONS_SCHEMA.md."


# ------------------------------------------------------------------ number / vector formatting
def f(x):
    s = f"{float(x):.2f}".rstrip("0")
    if s.endswith("."):
        s += "0"
    if s == "-0.0":
        s = "0.0"
    return s


def v3(t):
    return f"({f(t[0])}, {f(t[1])}, {f(t[2])})"


def seed_of(name):
    return zlib.crc32(name.encode("utf-8")) % 1000


# ------------------------------------------------------------------ rotation maths (glam XYZ)
def _mm(A, B):
    return [[sum(A[i][k] * B[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def _mv(A, v):
    return tuple(sum(A[i][k] * v[k] for k in range(3)) for i in range(3))


def rot_mat(rot):
    ax, ay, az = (math.radians(r) for r in rot)
    cx, sx, cy, sy, cz, sz = math.cos(ax), math.sin(ax), math.cos(ay), math.sin(ay), math.cos(az), math.sin(az)
    Rx = [[1, 0, 0], [0, cx, -sx], [0, sx, cx]]
    Ry = [[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]
    Rz = [[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]]
    return _mm(_mm(Rx, Ry), Rz)


def mat_euler(M):
    """Inverse of rot_mat: degrees (x, y, z) with M = Rx*Ry*Rz."""
    sy = max(-1.0, min(1.0, M[0][2]))
    y = math.asin(sy)
    if abs(sy) < 0.9999999:
        x = math.atan2(-M[1][2], M[2][2])
        z = math.atan2(-M[0][1], M[0][0])
    else:  # gimbal lock: fold everything into z
        x = 0.0
        z = math.atan2(M[1][0], M[1][1])
    return tuple(math.degrees(a) for a in (x, y, z))


def _norm(v):
    l = math.sqrt(sum(c * c for c in v))
    return tuple(c / l for c in v)


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def y_to(d):
    """Rotation matrix taking +Y onto direction d along the shortest arc."""
    d = _norm(d)
    c = max(-1.0, min(1.0, d[1]))
    axis = _cross((0.0, 1.0, 0.0), d)
    s = math.sqrt(sum(a * a for a in axis))
    if s < 1e-9:
        if c > 0:
            return [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
        axis, s = (1.0, 0.0, 0.0), 0.0  # straight down: half turn about X
    else:
        axis = tuple(a / s for a in axis)
    ang = math.atan2(s, c)
    x, y, z = axis
    ca, sa = math.cos(ang), math.sin(ang)
    t = 1 - ca
    return [[t * x * x + ca, t * x * y - sa * z, t * x * z + sa * y],
            [t * x * y + sa * z, t * y * y + ca, t * y * z - sa * x],
            [t * x * z - sa * y, t * y * z + sa * x, t * z * z + ca]]


# ------------------------------------------------------------------ shape builder
class Shape:
    """ShapeSpec builder. Tints: 'P' Primary, 'A' Accent, 'D' Dark, 'C' Core. `p` = part id, `r` = rot_deg.
    turn=True authors a held item lying in the XZ plane and stands it on edge (every prim is turned -90
    degrees about Z at emit time), so a flat blade is authored as a width-by-length box. Part pivots are
    NOT turned (as shipped: blades_of_chaos' chain pivots sit half a voxel off their links)."""

    def __init__(self, turn=False):
        self.turn = turn
        self.prims = []  # (kind, center, size, rot, tint, part, sub)
        self.parts = []  # (id, parent, pivot, role)

    def _p(self, kind, c, sz, t="P", p=0, r=(0, 0, 0), sub=False):
        assert kind in KINDS, kind
        assert min(sz) > 0, (kind, c, sz)
        self.prims.append((kind, tuple(c), tuple(sz), tuple(r), TINTS[t], p, sub))
        return self

    def box(self, c, sz, t="P", p=0, r=(0, 0, 0), sub=False):
        return self._p("Box", c, sz, t, p, r, sub)

    def ell(self, c, sz, t="P", p=0, r=(0, 0, 0), sub=False):
        return self._p("Ellipsoid", c, sz, t, p, r, sub)

    def cyl(self, c, sz, t="P", p=0, r=(0, 0, 0), sub=False):
        return self._p("Cylinder", c, sz, t, p, r, sub)

    def cone(self, c, sz, t="P", p=0, r=(0, 0, 0), sub=False):
        return self._p("Cone", c, sz, t, p, r, sub)

    def point(self, c, sz, d, t="P", p=0, sub=False):
        """Cone at c whose tip (+Y in local space) points along d."""
        return self._p("Cone", c, sz, t, p, mat_euler(y_to(d)), sub)

    def seg(self, a, b, w, t="P", p=0, d=None, ext=0.0, kind="Box", sub=False):
        """Prim spanning a -> b (local +Y along the segment), w wide, d deep (default w); ext lengthens
        both ends."""
        v = tuple(b[i] - a[i] for i in range(3))
        ln = math.sqrt(sum(x * x for x in v))
        c = tuple((a[i] + b[i]) / 2 for i in range(3))
        return self._p(kind, c, (w, ln + 2 * ext, w if d is None else d), t, p, mat_euler(y_to(v)), sub)

    def part(self, pid, parent, pivot, role):
        assert role in ROLES, role
        self.parts.append((pid, parent, tuple(pivot), role))
        return self

    def final(self):
        """(prims, parts) in emitted space (turn applied)."""
        if not self.turn:
            return self.prims, self.parts
        T = rot_mat((0, 0, -90))
        prims = [(k, _mv(T, c), sz, mat_euler(_mm(T, rot_mat(r))), t, p, sub)
                 for (k, c, sz, r, t, p, sub) in self.prims]
        return prims, self.parts

    def ron(self, ind):
        prims, parts = self.final()
        pad = " " * ind
        out = [f"{pad}prims: ["]
        for kind, c, sz, rot, tint, part, sub in prims:
            extra = ""
            if any(f(x) != "0.0" for x in rot):
                extra += f", rot_deg: {v3(rot)}"
            extra += f", tint: {tint}"
            if part:
                extra += f", part: {part}"
            if sub:
                extra += ", subtract: true"
            out.append(f"{pad}    (kind: {kind}, center: {v3(c)}, size: {v3(sz)}{extra}),")
        out.append(f"{pad}],")
        if parts:
            out.append(f"{pad}parts: [")
            for pid, parent, pivot, role in parts:
                out.append(f"{pad}    (id: {pid}, parent: {parent}, pivot: {v3(pivot)}, role: {role}),")
            out.append(f"{pad}],")
        return "\n".join(out)


# ------------------------------------------------------------------ recipes / entries
def R(name, form, mech=(), el="Light", scale=1.0, mass=10.0, count=1, accent=None, payload=None, feat=(),
      temp=None, inten=None):
    assert form in FORMS, (name, form)
    assert el in ELEMENTS, (name, el)
    for m in mech:
        assert m in MECH_ORDER, (name, m)
    for x in feat:
        assert x in FEAT_ORDER, (name, x)
    if temp:
        assert temp in TEMPS, (name, temp)
    return dict(name=name, form=form, mech=sorted(set(mech), key=MECH_ORDER.index), el=el, scale=scale,
                mass=mass, count=count, accent=accent, payload=payload,
                feat=sorted(set(feat), key=FEAT_ORDER.index), temp=temp, inten=inten)


def recipe_ron(r, ind, model):
    pad = " " * ind
    L = [f'{pad}name: "{r["name"]}",', f'{pad}form: {r["form"]},',
         f'{pad}mechanics: [{", ".join(r["mech"])}],', f'{pad}element: {r["el"]},',
         f'{pad}scale: {f(r["scale"])},', f'{pad}mass: {f(r["mass"])},', f'{pad}count: {int(r["count"])},']
    a = r["accent"]
    L.append(f"{pad}accent: " + (f"Some({v3(a)})" if a else "None") + ",")
    L.append(f"{pad}model: {model},")
    if r["payload"]:
        p = r["payload"]
        L.append(f"{pad}payload: Some((")
        L.append(recipe_ron(p, ind + 4, f"Procedural(seed: {seed_of(p['name'])})"))
        L.append(f"{pad})),")
    if r["feat"]:
        L.append(f'{pad}features: [{", ".join(r["feat"])}],')
    if r["temp"]:
        L.append(f'{pad}temperament: {r["temp"]},')
        L.append(f'{pad}intensity: {f(r["inten"] if r["inten"] is not None else 0.6)},')
    return "\n".join(L)


# (names, recipe, source, shape, icon id). gen.py's dedupe rewrites names only.
ENTRIES = []


def E(names, r, source, shape=None):
    names = [n.strip().lower() for n in names]
    assert len(set(names)) == len(names), names
    names = [n for n in names if n not in BLOCK]
    assert names, r["name"]
    ENTRIES.append((names, r, source, shape, names[0]))


def render():
    out = [HEADER, "["]
    seen = {}
    for names, r, source, shape, cid in ENTRIES:
        for n in names:
            assert n not in seen, f"dup name {n!r} ({seen[n]} / {names[0]})"
            seen[n] = names[0]
        model = f'Icon("{cid}")' if shape is not None else f"Procedural(seed: {seed_of(r['name'])})"
        out.append("    IconEntry(")
        out.append("        names: [" + ", ".join(f'"{n}"' for n in names) + "],")
        out.append("        recipe: (")
        out.append(recipe_ron(r, 12, model))
        out.append("        ),")
        if shape is not None:
            assert 1 <= len(shape.prims) <= 60, (cid, len(shape.prims))
            out.append("        shape: Some((")
            out.append(shape.ron(12))
            out.append("        )),")
        else:
            out.append("        shape: None,")
        out.append(f'        source: "{source}",')
        out.append("    ),")
    out.append("]")
    return "\n".join(out) + "\n"


def emit(path):
    """Write the pack; returns (entries, entries with shapes)."""
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(render())
    return len(ENTRIES), sum(1 for e in ENTRIES if e[3] is not None)


def emit_gallery(path=None):
    """Plain-text index of the hand-shaped entries (icon id, form, prim/part counts) for reviewing shapes
    with bd_shapes' gallery example. No-op without a path."""
    if not path:
        return 0
    rows = []
    for names, r, source, shape, cid in ENTRIES:
        if shape is None:
            continue
        prims, parts = shape.final()
        rows.append(f"{cid}\t{r['form']}\t{len(prims)} prims\t{len(parts)} parts\t{source}")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(rows) + "\n")
    return len(rows)
