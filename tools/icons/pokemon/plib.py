"""Shared builders + RON emitter for the Pokemon icon pack (assets/icons/pokemon.ron).

Units = construct voxels (author proportions; icons are normalised to recipe.scale before voxelising, so
author at a comfortable detail — creatures ~14-24 units tall). Voxel centres on the integer lattice.
Creatures: feet centre at origin, face -Z, left = -X, +Y up. Held: grip at origin, points -Z.
Objects: centre-ish, forward -Z. Worn/vehicles: see docs/ICONS_SCHEMA.md.

Tints: 'P' Primary (corps colour — the body), 'A' Accent (recipe accent, e.g. Charmander orange belly vs
flame), 'D' Dark (edges, mouths, claws, pupils), 'C' Core (near-white: eyes, flames, energy, teeth).
"""
import math
import zlib

T = {'P': 'Primary', 'A': 'Accent', 'D': 'Dark', 'C': 'Core'}
ALONG_Z = (90, 0, 0)   # cylinder axis along Z
ALONG_X = (0, 0, 90)   # cylinder axis along X
TIP_NZ = (-90, 0, 0)   # cone tip toward -Z (forward)
TIP_PZ = (90, 0, 0)    # cone tip toward +Z (back)
TIP_NY = (180, 0, 0)   # cone tip down
TIP_PX = (0, 0, -90)   # cone tip toward +X
TIP_NX = (0, 0, 90)    # cone tip toward -X

# part-role names (bd_core::shape::PartRole)
HEAD, JAW, TAIL = 'Head', 'Jaw', 'Tail'
LFL, LFR, LBL, LBR = 'LegFrontLeft', 'LegFrontRight', 'LegBackLeft', 'LegBackRight'
ARM_L, ARM_R, WING_L, WING_R = 'ArmLeft', 'ArmRight', 'WingLeft', 'WingRight'
WEAPON, SEAT, WHEEL, EXHAUST, CLOTH, OTHER = 'Weapon', 'Seat', 'Wheel', 'Exhaust', 'Cloth', 'Other'


class S:
    def __init__(self):
        self.prims = []
        self.parts = []

    def add(self, kind, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        assert t in T, t
        self.prims.append(dict(kind=kind, center=tuple(float(v) for v in c), size=tuple(float(max(v, 0.5)) for v in s),
                               rot=tuple(float(v) for v in r), tint=T[t], part=part, sub=sub))
        return self

    def box(self, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        return self.add('Box', c, s, t, r, part, sub)

    def ell(self, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        return self.add('Ellipsoid', c, s, t, r, part, sub)

    def cyl(self, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        return self.add('Cylinder', c, s, t, r, part, sub)

    def cone(self, c, s, t='P', r=(0, 0, 0), part=0, sub=False):
        return self.add('Cone', c, s, t, r, part, sub)

    def part(self, id, parent, pivot, role):
        assert 1 <= id <= 255 and all(p['id'] != id for p in self.parts), id
        self.parts.append(dict(id=id, parent=parent, pivot=tuple(float(v) for v in pivot), role=role))
        return self

    def seg(self, a, b, w, h, t='P', part=0, kind='Box'):
        """Prim whose local Y runs from a to b (w = local x size, h = local z size). kind Box/Ellipsoid/Cylinder."""
        ax, ay, az = a
        bx, by, bz = b
        dx, dy, dz = bx - ax, by - ay, bz - az
        L = math.sqrt(dx * dx + dy * dy + dz * dz)
        horiz = math.sqrt(dx * dx + dz * dz)
        tilt = math.degrees(math.atan2(horiz, dy))
        yaw = math.degrees(math.atan2(-dz, dx)) if horiz > 1e-6 else 0.0
        c = ((ax + bx) / 2, (ay + by) / 2, (az + bz) / 2)
        return self.add(kind, c, (w, max(L, 1), h), t, (0, yaw, -tilt), part)

    def spike(self, base, tip, w, t='P', part=0, d=None):
        """Cone from base (wide end) to tip."""
        ax, ay, az = base
        bx, by, bz = tip
        dx, dy, dz = bx - ax, by - ay, bz - az
        L = math.sqrt(dx * dx + dy * dy + dz * dz)
        horiz = math.sqrt(dx * dx + dz * dz)
        tilt = math.degrees(math.atan2(horiz, dy))
        yaw = math.degrees(math.atan2(-dz, dx)) if horiz > 1e-6 else 0.0
        c = ((ax + bx) / 2, (ay + by) / 2, (az + bz) / 2)
        return self.add('Cone', c, (w, max(L, 1), d if d is not None else w), t, (0, yaw, -tilt), part)

    def mirror(self, fn):
        """Call fn(sx) for sx in (-1, 1) — handy for symmetric features."""
        for sx in (-1, 1):
            fn(sx)
        return self

    def eyes(self, x, y, z, part, w=1.0, h=1.4, d=0.6, t='D', shine=True):
        """A pair of eyes at +-x, facing -Z at depth z (front surface)."""
        for sx in (-1, 1):
            self.ell((sx * x, y, z), (w, h, d), t, part=part)
            if shine:
                self.box((sx * x + sx * 0.0, y + h * 0.25, z - 0.2), (0.5, 0.5, 0.5), 'C', part=part)
        return self

    def spec(self):
        return dict(prims=self.prims, parts=self.parts)


def tail_chain(s, start_id, parent, pts, sizes, t='P', kind='Ellipsoid', role='Tail'):
    """Chain of parts through pts; part i spans pts[i]..pts[i+1], pivot at pts[i]. Returns ids."""
    pid = parent
    ids = []
    for i in range(len(pts) - 1):
        a, b = pts[i], pts[i + 1]
        nid = start_id + i
        s.part(nid, pid, a, role)
        w, h = sizes[i][0], sizes[i][-1]
        s.seg(a, b, w, h, t, part=nid, kind=kind)
        pid = nid
        ids.append(nid)
    return ids


def quad_legs(s, xs, zf, zb, top, foot=0, w=2.0, d=2.0, t='P', paw='P', ids=(4, 5, 6, 7), claws=None):
    """Four legs from y=top down to y=foot at x=+-xs, z=zf (front) / zb (back). Parts LFL/LFR/LBL/LBR."""
    roles = (LFL, LFR, LBL, LBR)
    pos = ((-xs, zf), (xs, zf), (-xs, zb), (xs, zb))
    for pid, role, (x, z) in zip(ids, roles, pos):
        s.part(pid, 0, (x, top, z), role)
        s.box((x, (top + foot) / 2 + 0.5, z), (w, top - foot, d), t, part=pid)
        s.ell((x, foot + 0.6, z - 0.3), (w + 0.6, 1.4, d + 1.0), paw, part=pid)
        if claws:
            for cx in (-0.5, 0.5):
                s.box((x + cx * (w * 0.6), foot + 0.4, z - d / 2 - 0.6), (0.5, 0.6, 0.8), claws, part=pid)
    return s


def biped_legs(s, x, top, foot=0, w=2.2, d=2.4, t='P', feet='P', ids=(4, 5), claws=None, foot_len=None):
    """Two legs (LegBackLeft/Right) from y=top down, feet slightly forward."""
    for pid, sx, role in ((ids[0], -1, LBL), (ids[1], 1, LBR)):
        px = sx * x
        s.part(pid, 0, (px, top, 0), role)
        s.box((px, (top + foot) / 2 + 0.5, 0), (w, top - foot, d), t, part=pid)
        fl = foot_len or d + 1.5
        s.ell((px, foot + 0.6, -fl / 2 + d / 2), (w + 0.8, 1.4, fl), feet, part=pid)
        if claws:
            for cx in (-0.6, 0, 0.6):
                s.box((px + cx * w * 0.5, foot + 0.4, -fl + d / 2 - 0.3), (0.5, 0.6, 0.8), claws, part=pid)
    return s


def arms(s, x, y, length, w=1.4, d=1.4, t='P', ids=(8, 9), down=(0, -1, -0.35), hand=None, claws=None):
    """Two arms (ArmLeft/Right) hanging from shoulders at (+-x, y, 0) along direction `down`."""
    dx, dy, dz = down
    n = math.sqrt(dx * dx + dy * dy + dz * dz)
    dx, dy, dz = dx / n, dy / n, dz / n
    for pid, sx, role in ((ids[0], -1, ARM_L), (ids[1], 1, ARM_R)):
        a = (sx * x, y, 0)
        b = (sx * (x + abs(dx) * length), y + dy * length, dz * length)
        s.part(pid, 0, a, role)
        s.seg(a, b, w, d, t, part=pid, kind='Ellipsoid')
        if hand:
            s.ell(b, (w + 0.6, w + 0.6, w + 0.6), hand, part=pid)
        if claws:
            for k in (-1, 0, 1):
                s.box((b[0] + k * 0.5, b[1] - 0.6, b[2] - 0.6), (0.5, 0.8, 0.5), claws, part=pid)
    return s


def wing_pair(s, root_x, root_y, root_z, span, chord, t='A', ids=(10, 11), tilt=25, sweep=0, thick=0.6, bone='P',
              kind='Box'):
    """Flat wings (WingLeft/Right) hinged at (+-root_x, root_y, root_z), extending outward, raised by `tilt` deg."""
    for pid, sx, role in ((ids[0], -1, WING_L), (ids[1], 1, WING_R)):
        s.part(pid, 0, (sx * root_x, root_y, root_z), role)
        cx = sx * (root_x + span / 2 * math.cos(math.radians(tilt)))
        cy = root_y + span / 2 * math.sin(math.radians(tilt))
        s.add(kind, (cx, cy, root_z + chord * 0.15), (span, thick, chord), t, (0, sx * sweep, sx * tilt), pid)
        if bone:
            s.box((cx, cy + 0.2, root_z - chord * 0.3), (span, thick + 0.3, 0.8), bone, (0, sx * sweep, sx * tilt), pid)
    return s


# ======================================================================================= registry + RON
E = []
SOURCE = "Pokemon (Game Freak / Nintendo)"


def icon(names, display, form, mech, scale, mass, *, element='Light', count=1, accent=None, features=(),
         temperament=None, intensity=None, shape=None, source=SOURCE, payload=None, id=None):
    """Register an icon. names: lowercase lookup keys (first = canonical id). scale = height (creatures) in m —
    use the canonical Pokedex height (min 0.3 so the sim's readability floor lifts tiny ones to 0.9 m)."""
    assert names and all(n == n.lower() and n.strip() for n in names), names
    E.append(dict(names=list(names), display=display, form=form, mech=sorted(set(mech)), scale=scale, mass=mass,
                  element=element, count=count, accent=accent, features=list(features), temperament=temperament,
                  intensity=intensity, shape=shape, source=source, payload=payload,
                  id=id or names[0].replace(' ', '_').replace("'", '').replace('-', '_').replace('.', '')))


def f(v):
    v = round(float(v), 3)
    return repr(v) if v != int(v) else f"{int(v)}.0"


def vec(t):
    return "(" + ", ".join(f(x) for x in t) + ")"


def prim_ron(p):
    s = f"(kind: {p['kind']}, center: {vec(p['center'])}, size: {vec(p['size'])}"
    if any(abs(r) > 1e-9 for r in p['rot']):
        s += f", rot_deg: {vec(p['rot'])}"
    if p['tint'] != 'Primary':
        s += f", tint: {p['tint']}"
    if p['part']:
        s += f", part: {p['part']}"
    if p['sub']:
        s += ", subtract: true"
    return s + ")"


def recipe_ron(e, model, indent="        "):
    acc = f"Some({vec(e['accent'])})" if e.get('accent') else "None"
    mech = "[" + ", ".join(e['mech']) + "]"
    out = ["(",
           f"{indent}    name: \"{e['display']}\",",
           f"{indent}    form: {e['form']},",
           f"{indent}    mechanics: {mech},",
           f"{indent}    element: {e.get('element', 'Light')},",
           f"{indent}    scale: {f(e['scale'])},",
           f"{indent}    mass: {f(e['mass'])},",
           f"{indent}    count: {e.get('count', 1)},",
           f"{indent}    accent: {acc},",
           f"{indent}    model: {model},"]
    if e.get('features'):
        out.append(f"{indent}    features: [{', '.join(sorted(e['features']))}],")
    if e.get('temperament'):
        out.append(f"{indent}    temperament: {e['temperament']},")
        out.append(f"{indent}    intensity: {f(e['intensity'] if e.get('intensity') is not None else 0.6)},")
    if e.get('payload'):
        p = e['payload']
        pe = dict(display=p['name'], form=p['form'], mech=p['mech'], element=p.get('element', 'Light'), scale=p['scale'],
                  mass=p['mass'], count=1, accent=None)
        seed = zlib.crc32(p['name'].encode()) & 0xffff
        out.append(f"{indent}    payload: Some({recipe_ron(pe, f'Procedural(seed: {seed})', indent + '    ')}),")
    out.append(f"{indent})")
    return "\n".join(out)


def entry_ron(e):
    names = ", ".join('"' + n + '"' for n in e['names'])
    if e['shape'] is not None:
        model = f"Icon(\"{e['id']}\")"
    else:
        model = f"Procedural(seed: {zlib.crc32(e['id'].encode()) & 0xffff})"
    lines = ["    IconEntry(", f"        names: [{names}],", f"        recipe: {recipe_ron(e, model)},"]
    if e['shape'] is not None:
        sp = e['shape'].spec()
        lines.append("        shape: Some((")
        lines.append("            prims: [")
        for p in sp['prims']:
            lines.append("                " + prim_ron(p) + ",")
        lines.append("            ],")
        if sp['parts']:
            lines.append("            parts: [")
            for q in sp['parts']:
                lines.append(f"                (id: {q['id']}, parent: {q['parent']}, pivot: {vec(q['pivot'])}, role: {q['role']}),")
            lines.append("            ],")
        lines.append("        )),")
    else:
        lines.append("        shape: None,")
    lines.append(f"        source: \"{e['source']}\",")
    lines.append("    ),")
    return "\n".join(lines)


# accents (sRGB 0..1)
RED = (0.9, 0.18, 0.15); ORANGE = (1.0, 0.55, 0.1); YELLOW = (1.0, 0.85, 0.2); GOLD = (0.95, 0.78, 0.3)
BLUE = (0.35, 0.6, 1.0); CYAN = (0.55, 0.9, 1.0); PURPLE = (0.65, 0.3, 1.0); PINK = (1.0, 0.55, 0.75)
WHITE = (0.95, 0.95, 0.97); BLACK = (0.08, 0.08, 0.1); GREEN = (0.35, 0.9, 0.35); ICE = (0.7, 0.9, 1.0)
CREAM = (0.98, 0.9, 0.7); BROWN = (0.6, 0.4, 0.25); TEAL = (0.2, 0.75, 0.7); NAVY = (0.15, 0.2, 0.5)
GREY = (0.62, 0.64, 0.68); LILAC = (0.75, 0.6, 0.95)
