"""Tiny DSL that emits Brightest Day IconEntry RON."""
import math

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
Ball Anvil Bomb Boulder Rocket Vehicle Blob0""".split())
ELEMENTS = set("Light Fire Electric Ice Water Wind Earth Poison Psychic Shadow Atomic".split())
TEMPS = set("Calm Aggressive Fearful Protective Playful Lazy Chaotic Sneaky".split())
ROLES = set("Head Jaw Tail LegFrontLeft LegFrontRight LegBackLeft LegBackRight ArmLeft ArmRight WingLeft WingRight Weapon Other".split())
TINTS = {"P": "Primary", "A": "Accent", "D": "Dark", "C": "Core"}


def f(x):
    s = f"{float(x):.3f}".rstrip("0")
    if s.endswith("."):
        s += "0"
    if s == "-0.0":
        s = "0.0"
    return s


def v3(t):
    return f"({f(t[0])}, {f(t[1])}, {f(t[2])})"


def fnv(s):
    h = 0x811C9DC5
    for b in s.encode("utf-8"):
        h ^= b
        h = (h * 0x01000193) & 0xFFFFFFFF
    return h % 100000


class S:
    """Shape builder."""

    def __init__(self):
        self.prims = []
        self.parts = []

    def _p(self, kind, c, s, t="P", part=0, rot=(0, 0, 0), sub=False):
        assert min(s) > 0, (kind, c, s)
        self.prims.append((kind, c, s, rot, TINTS[t], part, sub))
        return self

    def box(self, c, s, t="P", part=0, rot=(0, 0, 0), sub=False):
        return self._p("Box", c, s, t, part, rot, sub)

    def ell(self, c, s, t="P", part=0, rot=(0, 0, 0), sub=False):
        return self._p("Ellipsoid", c, s, t, part, rot, sub)

    def cyl(self, c, s, t="P", part=0, rot=(0, 0, 0), sub=False):
        return self._p("Cylinder", c, s, t, part, rot, sub)

    def cone(self, c, s, t="P", part=0, rot=(0, 0, 0), sub=False):
        return self._p("Cone", c, s, t, part, rot, sub)

    def part(self, pid, parent, pivot, role):
        assert role in ROLES
        self.parts.append((pid, parent, pivot, role))
        return self

    # mirrored helpers (x -> -x); rot z / y flip sign
    def mbox(self, c, s, t="P", parts=(0, 0), rot=(0, 0, 0)):
        self.box((-c[0], c[1], c[2]), s, t, parts[0], (rot[0], -rot[1], -rot[2]))
        self.box((c[0], c[1], c[2]), s, t, parts[1], rot)
        return self

    def mell(self, c, s, t="P", parts=(0, 0), rot=(0, 0, 0)):
        self.ell((-c[0], c[1], c[2]), s, t, parts[0], (rot[0], -rot[1], -rot[2]))
        self.ell((c[0], c[1], c[2]), s, t, parts[1], rot)
        return self

    def mcyl(self, c, s, t="P", parts=(0, 0), rot=(0, 0, 0)):
        self.cyl((-c[0], c[1], c[2]), s, t, parts[0], (rot[0], -rot[1], -rot[2]))
        self.cyl((c[0], c[1], c[2]), s, t, parts[1], rot)
        return self

    def mcone(self, c, s, t="P", parts=(0, 0), rot=(0, 0, 0)):
        self.cone((-c[0], c[1], c[2]), s, t, parts[0], (rot[0], -rot[1], -rot[2]))
        self.cone((c[0], c[1], c[2]), s, t, parts[1], rot)
        return self

    def ron(self, ind):
        pad = " " * ind
        out = [f"{pad}prims: ["]
        for kind, c, s, rot, tint, part, sub in self.prims:
            extra = ""
            if any(abs(r) > 1e-9 for r in rot):
                extra += f", rot_deg: {v3(rot)}"
            extra += f", tint: {tint}"
            if part:
                extra += f", part: {part}"
            if sub:
                extra += ", subtract: true"
            out.append(f"{pad}    (kind: {kind}, center: {v3(c)}, size: {v3(s)}{extra}),")
        out.append(f"{pad}],")
        if self.parts:
            out.append(f"{pad}parts: [")
            for pid, parent, pivot, role in self.parts:
                out.append(f"{pad}    (id: {pid}, parent: {parent}, pivot: {v3(pivot)}, role: {role}),")
            out.append(f"{pad}],")
        return "\n".join(out)


def R(name, form, mech=(), el="Light", scale=1.0, mass=10.0, count=1, accent=None, payload=None, feat=(),
      temp=None, inten=None, seed=None):
    assert form in FORMS, form
    assert el in ELEMENTS, el
    for m in mech:
        assert m in MECH_ORDER, m
    for x in feat:
        assert x in FEAT_ORDER, x
    if temp:
        assert temp in TEMPS, temp
    return dict(name=name, form=form, mech=sorted(set(mech), key=MECH_ORDER.index), el=el, scale=scale, mass=mass,
                count=count, accent=accent, payload=payload, feat=sorted(set(feat), key=FEAT_ORDER.index), temp=temp,
                inten=inten, seed=seed)


def recipe_ron(r, ind, model):
    pad = " " * ind
    L = [f'{pad}name: "{r["name"]}",', f'{pad}form: {r["form"]},',
         f'{pad}mechanics: [{", ".join(r["mech"])}],', f'{pad}element: {r["el"]},',
         f'{pad}scale: {f(r["scale"])},', f'{pad}mass: {f(r["mass"])},', f'{pad}count: {int(r["count"])},']
    a = r["accent"]
    L.append(f'{pad}accent: ' + (f'Some(({f(a[0])}, {f(a[1])}, {f(a[2])}))' if a else "None") + ",")
    L.append(f"{pad}model: {model},")
    if r["payload"]:
        p = r["payload"]
        seed = p["seed"] if p["seed"] is not None else fnv(p["name"])
        L.append(f"{pad}payload: Some((")
        L.append(recipe_ron(p, ind + 4, f"Procedural(seed: {seed})"))
        L.append(f"{pad})),")
    if r["feat"]:
        L.append(f'{pad}features: [{", ".join(r["feat"])}],')
    if r["temp"]:
        L.append(f'{pad}temperament: {r["temp"]},')
        L.append(f'{pad}intensity: {f(r["inten"] if r["inten"] is not None else 0.6)},')
    return "\n".join(L)


ENTRIES = []


def E(names, source, r, shape=None, note=None):
    names = [n.strip().lower() for n in names]
    assert len(set(names)) == len(names), names
    ENTRIES.append((names, source, r, shape, note))


def emit():
    out = ["// Brightest Day icon pack: internet culture, memes, toys, sports, everyday objects,",
           "// mythology & folklore, D&D, tech mascots. Schema: docs/ICONS_SCHEMA.md.",
           "// Mechanic combos are chosen to act out the joke (see per-entry comments).", "["]
    seen = {}
    for names, source, r, shape, note in ENTRIES:
        for n in names:
            assert n not in seen, f"dup name {n} ({seen[n]} / {names[0]})"
            seen[n] = names[0]
        cid = names[0]
        if shape is not None:
            model = f'Icon("{cid}")'
        else:
            seed = r["seed"] if r["seed"] is not None else fnv(cid)
            model = f"Procedural(seed: {seed})"
        if note:
            out.append(f"    // {note}")
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
        out.append(f'        source: "{source}",')
        out.append("    ),")
    out.append("]")
    return "\n".join(out) + "\n", seen


def _rot(rot):
    ax, ay, az = (math.radians(r) for r in rot)
    cx, sx, cy, sy, cz, sz = math.cos(ax), math.sin(ax), math.cos(ay), math.sin(ay), math.cos(az), math.sin(az)
    Rx = [[1, 0, 0], [0, cx, -sx], [0, sx, cx]]
    Ry = [[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]
    Rz = [[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]]
    mm = lambda A, B: [[sum(A[i][k] * B[k][j] for k in range(3)) for j in range(3)] for i in range(3)]
    return mm(mm(Rx, Ry), Rz)


def samples(kind, c, s, rot):
    """Integer lattice points inside a prim (mirrors bd_shapes PrimEval)."""
    eff = lambda h: 0.51 if h < 0.51 else h - 1e-3
    e = [eff(abs(v) * 0.5) for v in s]
    R = _rot(rot)
    r = math.ceil(max(e) * 1.8) + 1
    n = 0
    for x in range(int(math.floor(c[0] - r)), int(math.ceil(c[0] + r)) + 1):
        for y in range(int(math.floor(c[1] - r)), int(math.ceil(c[1] + r)) + 1):
            for z in range(int(math.floor(c[2] - r)), int(math.ceil(c[2] + r)) + 1):
                d = (x - c[0], y - c[1], z - c[2])
                l = [sum(R[k][i] * d[k] for k in range(3)) for i in range(3)]  # R^T d
                if kind == "Box":
                    ok = all(abs(l[i]) <= e[i] for i in range(3))
                elif kind == "Ellipsoid":
                    ok = sum((l[i] / e[i]) ** 2 for i in range(3)) <= 1
                elif kind == "Cylinder":
                    ok = abs(l[1]) <= e[1] and (l[0] / e[0]) ** 2 + (l[2] / e[2]) ** 2 <= 1
                else:
                    ok = False
                    if abs(l[1]) <= e[1]:
                        f_ = (l[1] + e[1]) / (2 * e[1])
                        rx, rz = max(e[0] * (1 - f_), 0.45), max(e[2] * (1 - f_), 0.45)
                        ok = (l[0] / rx) ** 2 + (l[2] / rz) ** 2 <= 1
                n += ok
    return n


def scaled(shape, k):
    s = S()
    s.prims = [(kind, tuple(v * k for v in c), tuple(v * k for v in sz), rot, t, p, sub)
               for (kind, c, sz, rot, t, p, sub) in shape.prims]
    s.parts = [(i, par, tuple(v * k for v in pv), role) for (i, par, pv, role) in shape.parts]
    return s


def _rhaz(v):
    return math.floor(v + 0.5) if v >= 0 else -math.floor(-v + 0.5)


def check(shape, cid):
    """Snap prims that would sample no voxel to a 1-voxel-min box on the lattice (keeps eyes/ears)."""
    fixed = 0
    for i, (kind, c, sz, rot, t, p, sub) in enumerate(shape.prims):
        if not sub and samples(kind, c, sz, rot) == 0:
            nc = tuple(_rhaz(v) for v in c)
            nsz = tuple(max(1.0, min(v, 2.0)) for v in sz)
            shape.prims[i] = ("Box", nc, nsz, (0, 0, 0), t, p, sub)
            fixed += 1
            assert samples("Box", nc, nsz, (0, 0, 0)) > 0
    if fixed:
        print(f"  [{cid}] snapped {fixed} sub-voxel prims")
    return fixed
