"""Legendary + classic Pokemon: Kaiju-scale legends (Rayquaza, Groudon, Kyogre, Wailord, Dialga, Palkia, Giratina)
and regular-size favourites (Metagross, Mew, Lugia, Ho-Oh, the birds, Steelix, Alakazam, ...).

Conventions (plib): feet centre at origin (fliers/floaters: bottom centre), face -Z, left = -X, +Y up.
Kaiju are authored ~35-50 units tall (Kaiju extent = height); regular creatures ~16 x max(scale, 0.9) units on
their characteristic extent (Biped/Kaiju = height, Blob = width, others = largest dimension).
"""
import math

from plib import S, icon, ALONG_Z, ALONG_X, TIP_NZ, TIP_PZ, TIP_NY, TIP_PX, TIP_NX
from plib import HEAD, JAW, TAIL, LFL, LFR, LBL, LBR, ARM_L, ARM_R, WING_L, WING_R, OTHER
from plib import RED, ORANGE, YELLOW, GOLD, BLUE, CYAN, PURPLE, PINK, WHITE, BLACK, GREEN, ICE, CREAM, BROWN, GREY, LILAC


# ------------------------------------------------------------------------------------------------ helpers
def lerp(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def vsub(a, b):
    return tuple(a[i] - b[i] for i in range(3))


def vadd(a, b):
    return tuple(a[i] + b[i] for i in range(3))


def vmul(a, k):
    return tuple(x * k for x in a)


def unit(v):
    n = math.sqrt(sum(x * x for x in v)) or 1.0
    return tuple(x / n for x in v)


def mx(p, sx):
    """Mirror a point to side sx (+-1) — author on +X, pass sx."""
    return (p[0] * sx, p[1], p[2])


def chain(s, sid, parent, pts, rads, t='P', role=TAIL, kind='Cylinder', joints=True):
    """Tail-style chain through pts (part i spans pts[i]..pts[i+1], pivot pts[i]); rads = diameter per point.
    Joint spheres keep the chain solid while it bends. Returns the part ids."""
    ids, pid = [], parent
    for i in range(len(pts) - 1):
        nid = sid + i
        s.part(nid, pid, pts[i], role)
        w = (rads[i] + rads[i + 1]) / 2
        s.seg(pts[i], pts[i + 1], w, w, t, part=nid, kind=kind)
        if joints and i < len(pts) - 2:
            r = rads[i + 1]
            s.ell(pts[i + 1], (r, r, r), t, part=nid)
        ids.append(nid)
        pid = nid
    return ids


def band(s, a, b, frac, w, t, part, thick=1.2):
    """A ring around the segment a..b at fraction frac (w = ring diameter)."""
    p = lerp(a, b, frac)
    d = unit(vsub(b, a))
    s.seg(vsub(p, vmul(d, thick / 2)), vadd(p, vmul(d, thick / 2)), w, w, t, part=part, kind='Cylinder')


def plate(s, a, b, width, thick, t='P', part=0, vertical=True, kind='Box'):
    """Flat plate from a to b. vertical=True: the plate lies in the vertical plane through a..b (fins, crests);
    False: the plate is horizontal-ish (wing membranes, flukes)."""
    if vertical:
        return s.seg(a, b, width, thick, t, part=part, kind=kind)
    return s.seg(a, b, thick, width, t, part=part, kind=kind)


def claws3(s, x, y, z, t='D', part=0, spread=0.9, size=(0.8, 0.8, 1.6), dz=-1.0):
    for k in (-1, 0, 1):
        s.cone((x + k * spread, y, z + dz), size, t, TIP_NZ, part=part)


# ================================================================================================ KAIJU
def rayquaza():
    """Sky serpent rearing in an S: the whole body is a Tail chain from the neck back; head rides the first link."""
    s = S()
    pts = [(0, 34, -7), (0, 29, -5), (0, 23.5, -2.5), (1, 18, 0), (2, 12.5, 1.5), (1.5, 7, 3.5), (-0.5, 3.8, 7.5),
           (-2.5, 3.2, 12.5), (-3, 4.2, 17.5), (-1.5, 6.5, 22.5), (0.5, 9.5, 26.5), (1, 13, 29.5), (0.5, 17, 31.5)]
    rads = [6.0, 6.4, 6.6, 6.6, 6.6, 6.4, 6.2, 5.8, 5.2, 4.6, 3.8, 3.0, 2.2]
    ids = chain(s, 10, 0, pts, rads, 'P')
    # yellow ring markings every link + glowing dots on the flanks
    for i, pid in enumerate(ids):
        a, b = pts[i], pts[i + 1]
        w = (rads[i] + rads[i + 1]) / 2 + 0.7
        band(s, a, b, 0.5, w, 'A', pid, thick=1.3)
    # paired side fins (red-tipped in canon) on alternate links, swept back
    for i in (1, 3, 5, 7, 9):
        a, b = pts[i], pts[i + 1]
        m = lerp(a, b, 0.4)
        d = unit(vsub(b, a))
        r = rads[i] / 2
        for sx in (-1, 1):
            base = (m[0] + sx * (r - 0.3), m[1], m[2])
            tip = (m[0] + sx * (r + 4.2), m[1] + 0.8 + d[1] * 2.5, m[2] + 2.0 + d[2] * 2.5)
            plate(s, base, tip, 2.6, 1.2, 'P', ids[i], vertical=False)
            s.ell(tip, (1.6, 1.4, 1.6), 'A', part=ids[i])
    # tail rudder fin
    tp = pts[-1]
    s.box((tp[0], tp[1] + 1.2, tp[2] + 1.0), (1.2, 5.0, 5.0), 'P', (45, 0, 0), part=ids[-1])
    s.box((tp[0], tp[1] + 1.2, tp[2] + 1.0), (1.4, 2.6, 2.6), 'A', (45, 0, 0), part=ids[-1])
    # little clawed arms on the chest link
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh = (sx * 2.6, 25.5, -4.0)
        s.part(pid, ids[1], sh, role)
        s.seg(sh, (sx * 4.2, 22.5, -7.2), 1.6, 1.6, 'P', part=pid, kind='Ellipsoid')
        claws3(s, sx * 4.3, 22.0, -7.6, 'C', part=pid, spread=0.6, size=(0.6, 0.6, 1.4))
    # head (rides the neck link)
    s.part(1, ids[0], (0, 34, -7), HEAD)
    s.ell((0, 37, -10.5), (6.2, 5.2, 8), 'P', part=1)                    # skull
    s.ell((0, 36.4, -15.5), (4.6, 3.4, 7.2), 'P', part=1)                # long muzzle
    s.box((0, 38.0, -13.5), (1.4, 0.8, 4.8), 'A', part=1)                # yellow muzzle stripe
    s.box((0, 35.3, -15.8), (4.2, 0.8, 6.4), 'D', part=1)                # mouth line
    for sx in (-1, 1):
        s.box((sx * 2.5, 37.6, -13.2), (1.3, 1.1, 1.8), 'C', (0, sx * 20, 0), part=1)     # eyes
        s.box((sx * 2.9, 37.6, -13.6), (0.6, 0.8, 0.6), 'D', part=1)
        # the two long swept-back head blades + shorter ear fins
        plate(s, (sx * 1.5, 39.2, -9.5), (sx * 3.2, 41.0, 3.0), 2.4, 1.2, 'P', 1, vertical=True)
        s.ell((sx * 3.2, 41.0, 3.0), (1.4, 1.6, 2.0), 'A', part=1)
        plate(s, (sx * 2.8, 37.0, -8.0), (sx * 6.5, 36.0, -3.0), 1.2, 2.0, 'P', 1, vertical=False)
        s.ell((sx * 6.5, 36.0, -3.0), (1.4, 1.4, 1.4), 'A', part=1)
    s.part(2, 1, (0, 35.2, -10), JAW)
    s.ell((0, 34.4, -14.8), (4.0, 1.8, 6.4), 'P', part=2)
    s.box((0, 35.0, -17.6), (2.8, 0.8, 0.8), 'C', part=2)                # teeth
    return s


def groudon():
    s = S()
    # legs (massive, clawed toes, grey knee plates)
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 7, 15, 2), role)
        s.ell((sx * 7.5, 11, 2), (8.5, 11, 9.5), 'P', part=pid)
        s.box((sx * 7.8, 4.6, 1), (6.8, 8, 7), 'P', part=pid)
        s.ell((sx * 7.8, 1.6, -1.2), (9, 3.2, 11), 'P', part=pid)
        s.ell((sx * 11.4, 11.5, 1), (2.4, 6.5, 6.5), 'A', part=pid)       # grey thigh plate
        for k in (-1, 0, 1):
            s.cone((sx * 7.8 + k * 2.6, 1.3, -7.2), (1.6, 1.6, 2.6), 'C', TIP_NZ, part=pid)
    # torso: hunched barrel with black body lines
    s.ell((0, 21, 1), (20, 20, 17), 'P')
    s.ell((0, 16, -5.5), (12, 10, 6), 'P')                                # belly bulge
    for y, w in ((15.5, 12.4), (19.5, 12.8)):
        s.box((0, y, -8.2), (w, 1.0, 1.4), 'D')                           # belly lines
    s.box((0, 25, -7.4), (9, 1.0, 1.4), 'D')
    for sx in (-1, 1):
        s.seg((sx * 9.6, 18, -3), (sx * 9.2, 26, 4), 1.2, 1.2, 'D', kind='Box')    # flank lines
        s.ell((sx * 9.5, 27.5, 0), (7.5, 5.5, 9), 'A')                   # shoulder plates
    # grey back plates (big ridge)
    for k, (y, z, h) in enumerate(((31, -1, 6.5), (30, 4, 6.0), (26.5, 8.5, 5.0))):
        for sx in (-1, 1):
            s.spike((sx * 3.5, y - 2, z), (sx * 5.5, y + h * 0.6, z + 2.5), 4.2, 'A', d=2.2)
        s.spike((0, y - 1, z + 1), (0, y + h * 0.5, z + 3.5), 3.4, 'A', d=2.0)
    # arms: short upper arm, huge plated forearm, spikes, white claws
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh = (sx * 10.5, 25, -2)
        s.part(pid, 0, sh, role)
        s.seg(sh, (sx * 13, 19.5, -4.5), 5.5, 5.5, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 13.6, 16, -5.5), (7, 8.5, 7.5), 'P', part=pid)
        s.ell((sx * 16.6, 17, -4.5), (2.6, 7.5, 6.5), 'A', part=pid)      # forearm plate
        s.spike((sx * 16.5, 19, -3), (sx * 21, 22.5, 1), 3.2, 'A', part=pid)
        s.spike((sx * 16.5, 16, -2), (sx * 21, 16.5, 2.5), 2.8, 'A', part=pid)
        claws3(s, sx * 13.6, 12.3, -8.0, 'C', part=pid, spread=1.8, size=(1.4, 1.4, 2.8))
    # head: blunt reptile head, yellow eyes under a heavy brow, spiky ridges
    s.part(1, 0, (0, 29, -7), HEAD)
    s.ell((0, 31.5, -11.5), (9.5, 7.5, 11), 'P', part=1)
    s.ell((0, 30.8, -16.2), (7.4, 5.5, 5), 'P', part=1)                    # muzzle
    for sx in (-1, 1):
        s.box((sx * 3.2, 33.0, -15.4), (2.2, 1.1, 1.4), 'C', (0, 0, sx * -12), part=1)   # eyes
        s.box((sx * 3.2, 34.2, -15.2), (3.2, 1.2, 1.6), 'D', (0, 0, sx * -15), part=1)  # brow
        s.spike((sx * 3.0, 34.5, -9.5), (sx * 4.2, 36.5, -4.5), 2.4, 'A', part=1)
        s.box((sx * 4.4, 31.5, -12), (1.0, 1.0, 7), 'D', (0, sx * 12, 0), part=1)        # head lines
    s.box((0, 29.4, -17.2), (6.4, 1.0, 2.8), 'D', part=1)                 # mouth
    s.part(2, 1, (0, 29.5, -9), JAW)
    s.ell((0, 28.2, -14), (7.4, 3.2, 8.5), 'P', part=2)
    s.box((0, 29.4, -17.8), (5, 0.8, 0.8), 'C', part=2)                    # teeth
    # tail: thick, plated, spiked tip
    pts = [(0, 16, 9), (0, 12, 16), (0, 8.5, 22), (0, 5.5, 27.5), (0, 4.2, 32)]
    ids = chain(s, 20, 0, pts, [10, 8, 6.2, 4.4, 2.6])
    for i, pid in enumerate(ids):
        a, b = pts[i], pts[i + 1]
        m = lerp(a, b, 0.5)
        h = (10, 8, 6.2, 4.4)[i] / 2
        s.spike((m[0], m[1] + h - 1, m[2]), (m[0], m[1] + h + 2.5, m[2] + 2), 3.0, 'A', part=pid, d=1.6)
    tip = pts[-1]
    for sx in (-1, 0, 1):
        s.spike((tip[0], tip[1], tip[2] - 1), (tip[0] + sx * 3.5, tip[1] + 2 - abs(sx), tip[2] + 4), 2.6, 'A', part=ids[-1])
    return s


def kyogre():
    s = S()
    # rear body (root) — the whale hovers, bottom at y ~2
    s.ell((0, 9.5, 3), (16, 11.5, 20), 'P')
    s.ell((0, 5.5, 1), (13, 4, 18), 'P')                                  # flat belly
    for sx in (-1, 1):
        s.box((sx * 7.6, 10.5, 1.5), (1.4, 1.6, 15), 'A', (0, sx * -3, 0))   # red side lines
        s.box((sx * 3.2, 15.0, 5.0), (1.4, 1.2, 9), 'A', (0, sx * 12, 0))    # red back V
        for z in (-3.5, 3.5):
            s.ell((sx * 7.8, 10.5, z), (1.6, 2.0, 2.0), 'C')                 # glowing spots
        s.spike((sx * 2, 14.5, 7), (sx * 2.5, 16.5, 10), 2.2, 'P', d=1.4)    # little back fins
    # head: broad rounded front, huge mouth, eyes set high on the sides
    s.part(1, 0, (0, 9, -5), HEAD)
    s.ell((0, 10.2, -9.5), (16.4, 12, 15), 'P', part=1)
    s.box((0, 7.2, -16.8), (12.5, 1.6, 3), 'D', part=1)                   # mouth slit
    for sx in (-1, 1):
        s.ell((sx * 5.6, 12.6, -14.2), (3.6, 3.0, 2.6), 'A', part=1)      # red eye ring
        s.ell((sx * 5.8, 12.7, -15.0), (2.0, 1.8, 1.4), 'C', part=1)      # yellow eye
        s.box((sx * 7.3, 12.0, -8.5), (1.4, 1.4, 9), 'A', (0, sx * -8, 0), part=1)   # head lines
        for x in (2.2, 4.8):
            s.ell((sx * x, 8.8, -16.4), (1.6, 1.4, 1.2), 'C', part=1)     # white lip spots
    s.part(2, 1, (0, 8, -6), JAW)
    s.ell((0, 6.2, -11), (13.5, 3.8, 11.5), 'P', part=2)
    # huge pectoral fins (two short slabs each, so the bounds stay honest) with four white nails
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 6.5, 7.5, -3)
        mid = (sx * 13, 5.2, 0.5)
        s.part(pid, 0, root, role)
        plate(s, root, mid, 10, 2.0, 'P', pid, vertical=False, kind='Ellipsoid')
        plate(s, mid, (sx * 19, 3.6, 3.5), 9, 2.0, 'P', pid, vertical=False, kind='Ellipsoid')
        plate(s, (sx * 8, 7.2, -4.6), (sx * 13.5, 5.8, -2.4), 2.2, 2.2, 'A', pid, vertical=False)   # leading edge
        for k in range(4):
            tip = (sx * (13.5 + k * 1.9), 4.2 - k * 0.3, 6.5 - k * 0.4)
            s.ell(tip, (1.8, 1.6, 2.6), 'C', part=pid)
    # tail + flukes
    pts = [(0, 9.5, 12), (0, 10, 18), (0, 10.5, 23)]
    ids = chain(s, 20, 0, pts, [8.5, 5.5, 3.4])
    for sx in (-1, 1):
        plate(s, (0, 10.5, 22), (sx * 7.5, 11, 27.5), 5, 1.8, 'P', ids[-1], vertical=False, kind='Ellipsoid')
        s.ell((sx * 7.5, 11, 27.5), (2.2, 1.8, 2.2), 'C', part=ids[-1])
    return s


def wailord():
    """Colossal blimp-whale: blue top, cream grooved underside, tiny eyes, blowhole, upswept twin fluke."""
    s = S()
    s.ell((0, 12, 2), (24, 22, 42), 'P')                                  # body
    s.ell((0, 5.5, 0), (24.4, 11, 44), 'A')                              # cream underside
    for k, y in enumerate((3.5, 6.0, 8.5)):
        s.box((0, y, -18.5 + k * 0.6), (14 - k * 2, 0.8, 5), 'D')          # belly grooves (front)
    # head: tall rounded front with the smile line where blue meets cream
    s.part(1, 0, (0, 12, -8), HEAD)
    s.ell((0, 12.5, -14), (23.4, 22.6, 22), 'P', part=1)
    s.ell((0, 6.2, -14.5), (23.8, 11, 22.6), 'A', part=1)
    s.box((0, 9.6, -24.6), (12, 1.0, 2.0), 'D', part=1)                   # smile
    for sx in (-1, 1):
        s.box((sx * 8.5, 10.4, -23.2), (5, 1.0, 2.0), 'D', (0, sx * 30, sx * -18), part=1)
        s.ell((sx * 9.8, 15.5, -20.2), (2.2, 3.2, 2.0), 'D', part=1)       # small eyes
        s.box((sx * 9.9, 16.4, -21.2), (0.8, 0.8, 0.8), 'C', part=1)
    s.cyl((0, 23.2, -11), (3.4, 1.6, 3.4), 'D', part=1)                   # blowhole
    s.cone((0, 25.5, -11), (3.0, 3.6, 3.0), 'C', part=1)                  # spout puff
    for z in (-6, -1, 4):
        for sx in (-1, 1):
            s.ell((sx * 4.5, 22.2 - abs(z) * 0.08, z), (2.2, 1.0, 2.2), 'A')   # back spots
    # little flippers near the belly
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 11, 7, -8)
        s.part(pid, 0, root, role)
        plate(s, root, (sx * 17.5, 3.5, -2), 6, 1.8, 'P', pid, vertical=False, kind='Ellipsoid')
    # tail sweeping up into a twin fluke
    pts = [(0, 12, 19), (0, 13.5, 25), (0, 17, 30)]
    ids = chain(s, 20, 0, pts, [15, 9, 5])
    for sx in (-1, 1):
        plate(s, (0, 17, 29.5), (sx * 9, 21.5, 34), 7, 2.0, 'P', ids[-1], vertical=False, kind='Ellipsoid')
        s.ell((sx * 9, 21.5, 34), (2.2, 2.0, 2.6), 'A', part=ids[-1])
    return s


def dialga():
    """Steel temporal quadruped: chest diamond, head crest, fan fins on the back."""
    s = S()
    s.ell((0, 17.5, 2.5), (16, 15, 26), 'P')                              # body
    s.ell((0, 20, -8.5), (14.5, 15, 8), 'P')                              # armoured chest
    s.box((0, 22, -12.6), (4.2, 4.2, 1.6), 'C', (0, 0, 45))                # the diamond
    s.box((0, 22, -12.2), (6.4, 6.4, 1.2), 'A', (0, 0, 45))
    for sx in (-1, 1):
        s.box((sx * 5.2, 18, -11.4), (1.2, 9, 1.2), 'A', (0, 0, sx * 12))    # chest rim lines
        s.cyl((sx * 3.2, 26, -2), (2.2, 12, 2.2), 'A', (80, 0, 0))           # back pipes
        s.box((sx * 8.1, 17, 3), (1.0, 1.2, 16), 'A')                        # flank line
    # four armoured legs
    legs = ((4, -1, -6.5, LFL), (5, 1, -6.5, LFR), (6, -1, 9, LBL), (7, 1, 9, LBR))
    for pid, sx, z, role in legs:
        s.part(pid, 0, (sx * 6.5, 16, z), role)
        s.ell((sx * 7.4, 12.5, z), (7.5, 10.5, 8.5), 'P', part=pid)
        s.box((sx * 7.6, 5.2, z - 0.8), (6, 8.5, 6.2), 'P', part=pid)
        s.box((sx * 10.6, 12.5, z), (1.0, 5, 5), 'A', part=pid)              # shoulder plate line
        s.ell((sx * 7.6, 1.6, z - 2.2), (7.4, 3.2, 9), 'P', part=pid)
        claws3(s, sx * 7.6, 1.2, z - 6.2, 'C', part=pid, spread=2.0, size=(1.2, 1.2, 2.2))
    # tall fan fins on the back
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 4.5, 25, 3)
        s.part(pid, 0, root, role)
        plate(s, root, (sx * 9.5, 37, 9), 7.5, 1.6, 'P', pid, vertical=True)
        plate(s, (sx * 7.4, 32, 8.5), (sx * 10.2, 38.5, 10), 2.0, 2.0, 'A', pid, vertical=True)
        s.spike((sx * 8, 31, 9), (sx * 11.5, 35, 15), 2.6, 'P', part=pid)
    # neck + head with the crest
    s.part(1, 0, (0, 24, -9), HEAD)
    s.seg((0, 23, -9), (0, 31.5, -13.5), 6.4, 6.4, 'P', part=1, kind='Cylinder')
    s.ell((0, 33.5, -16), (7.4, 6.4, 9.5), 'P', part=1)
    s.ell((0, 32.6, -20.8), (5.2, 4.2, 5.4), 'P', part=1)
    s.box((0, 36.8, -15), (1.6, 1.6, 6), 'A', part=1)                     # crest base line
    plate(s, (0, 36.5, -18), (0, 42, -8.5), 5, 1.8, 'P', 1, vertical=True)   # crest
    s.ell((0, 41.5, -9.2), (1.8, 2.4, 2.4), 'A', part=1)
    for sx in (-1, 1):
        s.box((sx * 2.8, 34.2, -19.3), (1.5, 1.1, 1.4), 'C', (0, sx * 18, 0), part=1)   # eyes
        s.spike((sx * 3.2, 35.5, -13), (sx * 4.5, 38, -7), 2.2, 'P', part=1)
        s.box((sx * 3.8, 30.5, -12), (1.2, 7, 1.2), 'A', (-25, 0, 0), part=1)          # neck lines
    s.part(2, 1, (0, 31.5, -15), JAW)
    s.ell((0, 30.4, -19.8), (4.6, 2.0, 6.2), 'P', part=2)
    s.box((0, 31.2, -22.4), (3.2, 0.8, 0.8), 'C', part=2)
    # short spiked tail
    pts = [(0, 16, 14), (0, 12.5, 20.5), (0, 9.5, 26)]
    ids = chain(s, 20, 0, pts, [8, 5.5, 3])
    for i, pid in enumerate(ids):
        m = lerp(pts[i], pts[i + 1], 0.5)
        s.spike((m[0], m[1] + 1.5, m[2]), (m[0], m[1] + 4.5, m[2] + 2.5), 2.6, 'A', part=pid, d=1.4)
    return s


def palkia():
    """Space dragon biped: pearl shoulders, back fins, crest; pink stripes."""
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 5, 15, 2), role)
        s.ell((sx * 5.6, 11, 2), (7.2, 10.5, 8), 'P', part=pid)
        s.box((sx * 6, 4.8, 1.2), (5.2, 8, 5.4), 'P', part=pid)
        s.box((sx * 8.7, 10.5, 2), (1.0, 7, 1.4), 'A', part=pid)             # thigh stripe
        s.ell((sx * 6, 1.6, -0.8), (6.6, 3.2, 8.5), 'P', part=pid)
        claws3(s, sx * 6, 1.2, -4.6, 'D', part=pid, spread=1.8, size=(1.1, 1.1, 2.0))
    # slim upright torso + chest plate with the pink line
    s.ell((0, 20, 1.5), (12.5, 14, 10.5), 'P')
    s.ell((0, 26, 1), (14, 9, 10), 'P')
    s.box((0, 22, -3.8), (1.4, 10, 1.4), 'A')
    for sx in (-1, 1):
        s.box((sx * 3, 25.5, -3.4), (1.2, 5, 1.2), 'A', (0, 0, sx * 35))
    # the pearl shoulders
    for sx in (-1, 1):
        s.ell((sx * 9, 28, 1), (8.5, 8, 9), 'P')
        s.cyl((sx * 11.8, 28.5, 0.5), (1.4, 6.4, 6.4), 'A', ALONG_X)
        s.ell((sx * 12.2, 28.5, 0.5), (3.0, 4.6, 4.6), 'C')
    # arms with claw hands
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh = (sx * 9.5, 25.5, 1)
        s.part(pid, 0, sh, role)
        s.seg(sh, (sx * 12, 18, -1), 4.8, 4.8, 'P', part=pid, kind='Ellipsoid')
        s.seg((sx * 12, 18, -1), (sx * 12.3, 13, -5), 4.2, 4.2, 'P', part=pid, kind='Ellipsoid')
        s.seg((sx * 13.4, 18, -1), (sx * 13.4, 13.5, -4.5), 1.0, 1.0, 'A', part=pid)     # arm stripe
        claws3(s, sx * 12.3, 11.8, -6.2, 'D', part=pid, spread=0.9, size=(0.9, 1.8, 0.9), dz=0)
    # two big fins on the back
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 4, 29, 5)
        s.part(pid, 0, root, role)
        plate(s, root, (sx * 14, 39, 11), 6, 1.6, 'P', pid, vertical=True)
        plate(s, (sx * 6, 31.5, 6.3), (sx * 14.5, 39.5, 11.4), 1.6, 1.8, 'A', pid, vertical=True)
        s.spike((sx * 13, 38, 10.5), (sx * 17, 42, 13), 2.4, 'P', part=pid)
    # long neck + head with crest horn
    s.part(1, 0, (0, 29.5, -1), HEAD)
    s.seg((0, 29, -1), (0, 35, -4.5), 5, 5, 'P', part=1, kind='Cylinder')
    s.ell((0, 36.5, -6.5), (6, 5.5, 8), 'P', part=1)
    s.ell((0, 36, -10.8), (4.2, 3.6, 5), 'P', part=1)
    plate(s, (0, 38.5, -8), (0, 43.5, -1), 4, 1.6, 'P', 1, vertical=True)      # crest
    s.box((0, 39.5, -6.5), (1.4, 1.4, 4), 'A', (-30, 0, 0), part=1)
    for sx in (-1, 1):
        s.box((sx * 2.4, 37.2, -9.8), (1.3, 1.0, 1.3), 'C', (0, sx * 18, 0), part=1)
        s.spike((sx * 2.8, 35.5, -5.5), (sx * 5.5, 37.5, -1.5), 1.8, 'P', part=1)   # cheek horns
    s.part(2, 1, (0, 34.5, -6), JAW)
    s.ell((0, 34.4, -10.4), (3.8, 1.8, 5), 'P', part=2)
    # tail
    pts = [(0, 14, 6), (0, 9.5, 12), (0, 6, 17)]
    ids = chain(s, 20, 0, pts, [7, 5, 2.8])
    for i, pid in enumerate(ids):
        band(s, pts[i], pts[i + 1], 0.5, (7, 5, 2.8)[i] - 1 + 0.8, 'A', pid)
    return s


def giratina():
    """Altered Forme: six legs, gold crown and chest ribs, black stabbing wings, striped body."""
    s = S()
    s.ell((0, 16, 2), (14, 12.5, 30), 'P')
    for z in (-4, 3, 10):
        s.ell((0, 16, z), (14.4, 12.9, 1.6), 'D')                           # black body rings
    for k, z in enumerate((-10, -7, -4)):
        s.ell((0, 11.5 + k * 0.3, z), (10 - k * 0.6, 2.2, 3.2), 'A')        # gold chest ribs
    # six legs, three a side (hexapod gait)
    for pid, sx, z, role in ((4, -1, -8, LFL), (5, 1, -8, LFR), (12, -1, 1, LFL), (13, 1, 1, LFR),
                             (6, -1, 10, LBL), (7, 1, 10, LBR)):
        top = (sx * 6, 13, z)
        s.part(pid, 0, top, role)
        s.seg(top, (sx * 9, 5, z - 1), 4.8, 4.8, 'P', part=pid, kind='Ellipsoid')
        s.seg((sx * 9, 5, z - 1), (sx * 9.5, 1, z - 2), 3.8, 3.8, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 8.2, 7.5, z - 0.6), (3.8, 1.2, 3.8), 'D', part=pid)
        claws3(s, sx * 9.5, 0.8, z - 2.6, 'A', part=pid, spread=0.9, size=(0.9, 0.9, 1.8))
    # wings: black shadows with three stabbing red spikes
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 5, 21, -3)
        s.part(pid, 0, root, role)
        s.seg(root, (sx * 15, 31, 6), 3.2, 3.2, 'P', part=pid, kind='Ellipsoid')        # wing arm
        s.ell((sx * 15, 31, 6), (3.4, 3.4, 3.4), 'A', part=pid)
        for k, (tx, ty, tz) in enumerate(((22, 36, 12), (22, 26, 17), (17, 19, 20))):
            plate(s, (sx * 11, 26, 4 + k * 2), (sx * tx, ty, tz), 5.5 - k, 1.6, 'D', pid, vertical=True)
            s.spike((sx * (tx - 2), ty - 1, tz - 1.5), (sx * (tx + 2.5), ty + 1.5, tz + 3), 2.2, 'C', part=pid)
    # neck + gold-crowned head
    s.part(1, 0, (0, 20, -11), HEAD)
    s.seg((0, 19, -11), (0, 27.5, -15.5), 5.6, 5.6, 'P', part=1, kind='Cylinder')
    band(s, (0, 19, -11), (0, 27.5, -15.5), 0.45, 6.2, 'D', 1, thick=1.4)
    s.ell((0, 30, -17.5), (6.4, 6, 8), 'P', part=1)
    s.ell((0, 31.6, -17.8), (7.2, 3.2, 8.6), 'A', part=1)                  # crown band
    s.ell((0, 28.2, -18.5), (6.8, 2.4, 7.4), 'A', part=1)                  # jaw guard
    for sx in (-1, 1):
        s.spike((sx * 3.2, 32.5, -15.5), (sx * 6, 35.5, -10.5), 2.2, 'A', part=1)      # crown horns
        s.spike((sx * 3.4, 28.4, -15.5), (sx * 6.5, 27, -11.5), 1.8, 'A', part=1)      # jaw spikes
        s.box((sx * 2.1, 30.4, -21.3), (1.4, 1.0, 0.8), 'C', part=1)                   # red eyes
    # striped tail
    pts = [(0, 17, 16), (0, 15.5, 22), (0, 13, 27.5)]
    ids = chain(s, 20, 0, pts, [7.5, 5, 3])
    for i, pid in enumerate(ids):
        band(s, pts[i], pts[i + 1], 0.5, (7.5, 5)[i] - 1 + 0.8, 'D', pid)
    for sx in (-1, 1):
        s.spike((0, 13, 27), (sx * 2.5, 14.5, 31), 1.8, 'A', part=ids[-1])
    return s


# ================================================================================================ REGULAR
def metagross():
    """Flat iron disc with the silver X face and four thick legs in an X."""
    s = S()
    s.ell((0, 11, 1.5), (20, 9, 18), 'P')                                  # disc body
    s.ell((0, 15, 2.5), (13, 4, 12), 'P')
    s.part(1, 0, (0, 11, -3), HEAD)
    s.ell((0, 11, -5.5), (15, 9.5, 8), 'P', part=1)                        # the face
    for sx in (-1, 1):
        for sy in (-1, 1):                                                  # silver X, four arms from the centre
            d = (sx * math.cos(math.radians(48)), sy * math.sin(math.radians(48)))
            s.box((d[0] * 3.6, 11.5 + d[1] * 3.6, -9.6), (3.6, 7.6, 3.0), 'A', (0, 0, -sx * sy * 42), part=1)
        s.ell((sx * 4.2, 8.2, -9.0), (3.2, 2.6, 2.2), 'C', part=1)             # red eyes
    s.box((0, 11.5, -9.6), (3.2, 3.2, 2.4), 'A', part=1)
    legs = ((4, -1, -1, LFL), (5, 1, -1, LFR), (6, -1, 1, LBL), (7, 1, 1, LBR))
    for pid, sx, sz, role in legs:
        hip = (sx * 6, 10, sz * 5)
        knee = (sx * 12, 14.5, sz * 10)
        foot = (sx * 13.5, 1.8, sz * 12)
        s.part(pid, 0, hip, role)
        s.seg(hip, knee, 6, 6, 'P', part=pid, kind='Ellipsoid')
        s.ell(knee, (6.4, 6.4, 6.4), 'A', part=pid)
        s.seg(knee, foot, 5.6, 5.6, 'P', part=pid, kind='Ellipsoid')
        for k in (-1, 0, 1):
            ang = math.atan2(sz, sx) + k * 0.7
            s.spike((foot[0], 1.2, foot[2]), (foot[0] + math.cos(ang) * 3.2, 0.6, foot[2] + math.sin(ang) * 3.2), 2.4,
                    'A', part=pid, d=2.0)
    return s


def mew():
    """Tiny floating kitten: big head, small body, big feet, very long thin tail with an oval tip."""
    s = S()
    s.ell((0, 5.6, 0.4), (4.4, 5.2, 4.2), 'P')                             # body
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 1.2, 3.6, 0.6), role)
        s.seg((sx * 1.2, 3.6, 0.6), (sx * 1.6, 1.4, 0.2), 2.0, 2.0, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 1.7, 1.0, -0.6), (2.4, 2.0, 3.4), 'P', part=pid)       # big feet
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        s.part(pid, 0, (sx * 1.8, 7.2, -0.4), role)
        s.seg((sx * 1.8, 7.2, -0.4), (sx * 3.0, 5.6, -1.8), 1.8, 1.8, 'P', part=pid, kind='Ellipsoid')
    s.part(1, 0, (0, 7.6, 0), HEAD)
    s.ell((0, 10.8, 0), (8.0, 6.8, 6.8), 'P', part=1)
    for sx in (-1, 1):
        s.cone((sx * 2.6, 14.4, 0.4), (2.6, 2.8, 2.0), 'P', (0, 0, sx * -18), part=1)   # ears
        s.ell((sx * 1.7, 11.2, -3.0), (2.2, 3.0, 1.2), 'A', part=1)            # big blue eyes
        s.ell((sx * 1.6, 11.0, -3.4), (1.0, 1.6, 0.6), 'D', part=1)
        s.box((sx * 1.5, 12.0, -3.6), (0.6, 0.6, 0.5), 'C', part=1)
    s.box((0, 9.0, -3.3), (1.2, 0.5, 0.5), 'D', part=1)                    # smile
    pts = [(0, 4.2, 2.2), (0, 3.0, 5.5), (0, 4.5, 9.0), (0, 8.0, 10.5), (0, 11.5, 9.5), (0, 14.0, 7.5)]
    ids = chain(s, 20, 0, pts, [2.2, 2.0, 2.0, 2.0, 2.0, 2.0])
    s.ell((0, 15.2, 6.3), (2.4, 4.0, 3.2), 'P', (40, 0, 0), part=ids[-1])   # oval tail tip
    return s


def lugia():
    """Diving guardian: long neck, blue eye masks, back spines, raised five-fingered wing-hands, twin tail fins."""
    s = S()
    s.ell((0, 14, 1.5), (11, 14, 11), 'P')                                 # body
    s.ell((0, 13, -3.2), (8, 11, 4.4), 'A')                               # blue belly
    for k in range(4):
        y, z = 21 - k * 3.2, 5 + k * 1.2
        s.spike((0, y, z), (0, y - 1.5, z + 4), 3.4, 'A', d=2.2)          # back spines
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 3, 9.5, 1.5), role)
        s.seg((sx * 3, 9.5, 1.5), (sx * 3.4, 1.8, 0.6), 4.4, 4.4, 'P', part=pid, kind='Ellipsoid')
        claws3(s, sx * 3.4, 0.9, 0.2, 'D', part=pid, spread=1.1, size=(1.1, 1.1, 2.2))
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 5, 20.5, 1.5)
        s.part(pid, 0, root, role)
        wing_hand(s, pid, sx, root, (sx * 10, 25.5, 2.5), (sx * 12, 29.5, 3),
                  [(78, 6.5), (48, 7.5), (18, 7.5), (-12, 7), (-42, 6.5)], arm_w=5, fw=4.4, ft=2.6, tip_r=3.2)
        plate(s, (sx * 6, 20, 2.5), (sx * 15, 23, 3.5), 5.5, 2.4, 'P', pid, vertical=True, kind='Ellipsoid')   # membrane
    s.part(1, 0, (0, 19.5, -0.5), HEAD)
    s.seg((0, 19, -0.5), (0, 26.5, -3), 5, 5, 'P', part=1, kind='Cylinder')
    s.ell((0, 28.2, -5), (6.2, 5.4, 8.6), 'P', part=1)
    s.ell((0, 27.4, -9.2), (4.4, 3.6, 4), 'P', part=1)                     # rounded beak
    for sx in (-1, 1):
        s.ell((sx * 2.4, 28.8, -7.2), (2.6, 2.4, 4), 'A', (0, sx * 12, 0), part=1)   # eye masks
        s.box((sx * 2.9, 29.0, -8.4), (0.8, 1.0, 1.0), 'D', part=1)
    pts = [(0, 9.5, 5.5), (0, 7, 10.5), (0, 5.5, 15.5), (0, 5.2, 19.5)]
    ids = chain(s, 20, 0, pts, [5.5, 4.4, 3.4, 2.8])
    for sx in (-1, 1):
        plate(s, (0, 5.2, 19), (sx * 4.2, 5.8, 23), 4, 2.0, 'A', ids[-1], vertical=False, kind='Ellipsoid')
    return s


def ho_oh():
    """Rainbow phoenix: gold crest + beak, raised wings with banded white tips, long plumed tail."""
    s = S()
    s.ell((0, 13, 1), (10, 12, 10), 'P')                                   # body
    s.ell((0, 12.5, -2.8), (7.4, 9, 4.6), 'A')                            # gold breast
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 2.6, 8.5, 1), role)
        s.seg((sx * 2.6, 8.5, 1), (sx * 3, 1.6, 0.5), 3.0, 3.0, 'A', part=pid, kind='Ellipsoid')
        claws3(s, sx * 3, 0.9, 0.2, 'D', part=pid, spread=1.0, size=(1.0, 1.0, 2.2))
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 4.5, 18, 1.5)
        s.part(pid, 0, root, role)
        wing_hand(s, pid, sx, root, (sx * 8, 23, 2.5), (sx * 10, 27, 3),
                  [(82, 7.5), (55, 8), (28, 8), (2, 7.5), (-24, 7)], arm_w=5, fw=4.4, ft=2.4, tip_t='C', tip_r=3.2,
                  band_t='A')
        plate(s, root, (sx * 11, 14, 3.5), 5.5, 2.4, 'P', pid, vertical=True, kind='Ellipsoid')   # inner coverts
    s.part(1, 0, (0, 18, -0.5), HEAD)
    s.seg((0, 17.5, -0.5), (0, 23, -2.5), 4.6, 4.6, 'P', part=1, kind='Cylinder')
    s.ell((0, 24.8, -3.6), (5.4, 5, 6.4), 'P', part=1)
    s.spike((0, 24.6, -6), (0, 22.6, -9.4), 3.0, 'A', part=1, d=2.4)         # hooked beak
    s.ell((0, 24.0, -5.2), (5.6, 2.6, 3.2), 'C', part=1)                    # white cheeks
    for sx in (-1, 1):
        s.box((sx * 2.0, 25.6, -6.2), (1.0, 1.0, 1.0), 'D', part=1)
    for k, (dy, dz) in enumerate(((6, 3), (7, 0.5), (4.5, 6))):
        s.spike((0, 26.5, -3), (0, 26.5 + dy, -3 + dz), 3.2 - k * 0.4, 'A', part=1, d=2.0)   # crest plumes
    pts = [(0, 8.5, 5), (0, 6.5, 10), (0, 4.5, 15), (0, 3.5, 19.5)]
    ids = chain(s, 20, 0, pts, [4.6, 4.0, 3.6, 3.0])
    for sx in (-1, 1):
        plate(s, (0, 6, 11), (sx * 5, 3, 19), 3.6, 2.0, 'P', ids[1], vertical=False, kind='Ellipsoid')
        s.ell((sx * 5, 3, 19), (2.8, 2.0, 3.0), 'C', part=ids[1])
    s.ell((0, 3.5, 20), (3.2, 2.2, 3.4), 'C', part=ids[-1])
    return s


def zapdos():
    """Thunder bird: jagged lightning wings, spiky crest and tail, long straight beak."""
    s = S()
    s.ell((0, 11, 1), (10, 11, 10), 'P')
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 2.4, 7, 1), role)
        s.seg((sx * 2.4, 7, 1), (sx * 2.8, 1.4, 0.5), 3.0, 3.0, 'A', part=pid, kind='Box')
        claws3(s, sx * 2.8, 0.8, 0.2, 'A', part=pid, spread=1.0, size=(1.0, 1.0, 2.4))
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 4, 14, 1.5)
        s.part(pid, 0, root, role)
        plate(s, root, (sx * 11, 16.5, 2.5), 5.5, 3.0, 'P', pid, vertical=True, kind='Ellipsoid')
        for k, (tx, ty) in enumerate(((18, 22.5), (18.5, 15.5), (14.5, 9))):
            base = (sx * (8 + k * 0.5), 15.5 - k * 1.5, 2.5 + k)
            tip = (sx * tx, ty, 3 + k * 1.5)
            s.spike(base, tip, 5.0, 'P', part=pid, d=3.0)
            s.spike(lerp(base, tip, 0.5), tip, 3.2, 'D', part=pid, d=2.4)
    s.part(1, 0, (0, 14, -1), HEAD)
    s.ell((0, 16.8, -2.5), (5.6, 5.6, 6), 'P', part=1)
    s.spike((0, 16.4, -5.2), (0, 15.2, -11), 3.0, 'A', part=1, d=2.4)       # beak
    for sx in (-1, 1):
        s.box((sx * 2.0, 17.6, -5.0), (1.2, 0.9, 1.0), 'D', (0, 0, sx * -20), part=1)
    for k, (dx, dy, dz) in enumerate(((0, 6, 1), (2.8, 4.6, 2.8), (-2.8, 4.6, 2.8), (0, 3.8, 5))):
        base = (dx * 0.3, 18.8, -2)
        tip = (dx, 18.8 + dy, -2 + dz)
        s.spike(base, tip, 3.0, 'P', part=1, d=2.2)
        s.spike(lerp(base, tip, 0.5), tip, 2.0, 'D', part=1, d=1.6)
    s.part(20, 0, (0, 8.5, 4.5), TAIL)
    for k in (-1, 0, 1):
        tip = (k * 4, 9.5 - abs(k) * 1.5, 13)
        s.spike((0, 8.5, 4), tip, 4.0, 'P', part=20, d=2.2)
        s.spike(lerp((0, 8.5, 4), tip, 0.55), tip, 2.4, 'D', part=20, d=1.6)
    return s


def moltres():
    """Flame bird: long neck, flame crest, wings trailing fire, flame tail."""
    s = S()
    s.ell((0, 11, 1), (9, 10, 10), 'P')
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 2.2, 7.5, 1), role)
        s.seg((sx * 2.2, 7.5, 1), (sx * 2.4, 1.4, 0.5), 2.4, 2.4, 'P', part=pid, kind='Box')
        claws3(s, sx * 2.4, 0.8, 0.2, 'D', part=pid, spread=0.9, size=(0.9, 0.9, 2.2))
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 4, 14, 1.5)
        s.part(pid, 0, root, role)
        wrist = (sx * 10, 20, 3)
        s.seg(root, wrist, 4.6, 3.2, 'P', part=pid, kind='Ellipsoid')
        s.seg(wrist, (sx * 17, 18, 4), 4.0, 3.0, 'P', part=pid, kind='Ellipsoid')
        plate(s, root, (sx * 14, 15, 4), 6, 2.4, 'P', pid, vertical=True, kind='Ellipsoid')
        for k in range(4):                                                  # flames along the trailing edge
            b = (sx * (6 + k * 3.2), 15.5 + k * 0.8, 4.5)
            t = (sx * (6.5 + k * 3.6), 20 + k * 1.2, 9.5 + k * 0.4)
            s.spike(b, t, 4.0, 'A', part=pid, d=2.6)
            s.spike(lerp(b, t, 0.35), vadd(t, (0, 1.2, 1)), 2.6, 'C', part=pid, d=2.0)
    s.part(1, 0, (0, 14, -1.5), HEAD)
    s.seg((0, 13.5, -1.5), (0, 19.5, -4), 3.8, 3.8, 'P', part=1, kind='Cylinder')
    s.ell((0, 21, -4.8), (4.6, 4.4, 5.4), 'P', part=1)
    s.spike((0, 20.8, -7), (0, 20.0, -10.5), 2.6, 'A', part=1, d=2.2)       # beak
    for sx in (-1, 1):
        s.box((sx * 1.8, 21.8, -6.6), (1.0, 0.8, 0.8), 'D', (0, 0, sx * -20), part=1)
    for k in range(3):                                                        # flame crest
        b = (0, 22.5 + k * 0.5, -5 + k * 1.5)
        t = (0, 27.5 - k * 0.8, -2 + k * 3)
        s.spike(b, t, 3.6, 'A', part=1, d=2.4)
        s.spike(lerp(b, t, 0.35), vadd(t, (0, 1.2, 0.6)), 2.2, 'C', part=1, d=1.8)
    pts = [(0, 9, 5), (0, 8, 9.5), (0, 8.5, 13.5)]
    ids = chain(s, 20, 0, pts, [3.8, 3.2, 2.6])
    for k in (-1, 0, 1):
        b = (0, 8.5, 12.5)
        tip = (k * 4, 10.5 + (1 - abs(k)) * 2.5, 19)
        s.spike(b, tip, 4.0, 'A', part=ids[-1], d=2.6)
        s.spike(lerp(b, tip, 0.4), tip, 2.6, 'C', part=ids[-1], d=2.0)
    return s


def articuno():
    """Ice bird: three-plume crest, pale chest, broad wings, extremely long streaming tail."""
    s = S()
    s.ell((0, 11, 1), (9, 10.5, 10), 'P')
    s.ell((0, 11.5, -2.8), (6.6, 8, 4.6), 'A')                            # pale chest
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 2.2, 7.5, 1), role)
        s.seg((sx * 2.2, 7.5, 1), (sx * 2.4, 1.4, 0.5), 2.4, 2.4, 'P', part=pid, kind='Box')
        claws3(s, sx * 2.4, 0.8, 0.2, 'D', part=pid, spread=0.9, size=(0.9, 0.9, 2.2))
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 4, 14.5, 1.5)
        s.part(pid, 0, root, role)
        wing_hand(s, pid, sx, root, (sx * 7.5, 18.5, 2.5), (sx * 10, 21, 3),
                  [(60, 7), (30, 7.5), (0, 7.5), (-30, 7)], arm_w=4.6, fw=4.2, ft=2.4, tip_t='A', tip_r=3.0)
        plate(s, root, (sx * 11, 12, 3.5), 5.5, 2.4, 'P', pid, vertical=True, kind='Ellipsoid')
    s.part(1, 0, (0, 14, -1.5), HEAD)
    s.seg((0, 13.5, -1.5), (0, 18.5, -3.5), 3.8, 3.8, 'P', part=1, kind='Cylinder')
    s.ell((0, 20, -4.2), (4.6, 4.6, 5.4), 'P', part=1)
    s.spike((0, 19.6, -6.6), (0, 19.0, -9.6), 2.2, 'D', part=1, d=2.0)
    for sx in (-1, 1):
        s.box((sx * 1.8, 20.8, -6.2), (1.0, 0.8, 0.8), 'C', (0, 0, sx * -20), part=1)
    for k, (dx, dz) in enumerate(((0, 0), (-1.4, 1.2), (1.4, 1.2))):       # three-plume crest
        s.spike((dx * 0.5, 22, -4 + dz), (dx * 1.6, 26.5 - k * 0.8, -2 + dz * 2), 2.8, 'D', part=1, d=2.0)
    for n, sx in enumerate((-1, 1)):                                          # two long streamers
        pts = [(sx * 0.8, 9, 5), (sx * 1.8, 7.5, 10), (sx * 2.8, 7, 15), (sx * 3.6, 7.8, 20), (sx * 4.4, 9.5, 24)]
        ids = chain(s, 20 + n * 10, 0, pts, [3.8, 3.6, 3.4, 3.2, 3.0])
        s.ell((sx * 4.5, 9.8, 24.8), (3.2, 3.6, 3.4), 'A', part=ids[-1])
    return s


def aerodactyl():
    """Fossil pterosaur: membrane wings with thumb claws, toothy open jaw, horns, spade tail."""
    s = S()
    s.ell((0, 11, 1), (8.5, 9, 11), 'P')
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 2.4, 8, 2.5), role)
        s.seg((sx * 2.4, 8, 2.5), (sx * 2.8, 1.6, 2), 3.0, 3.0, 'P', part=pid, kind='Ellipsoid')
        claws3(s, sx * 2.8, 0.9, 1.6, 'D', part=pid, spread=1.0, size=(1.0, 1.0, 2.2))
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        root = (sx * 3.5, 13.5, 0)
        s.part(pid, 0, root, role)
        wrist = (sx * 10, 19, 1)
        tip = (sx * 17.5, 13, 4)
        s.seg(root, wrist, 3.6, 3.6, 'P', part=pid, kind='Ellipsoid')
        s.seg(wrist, tip, 3.0, 3.0, 'P', part=pid, kind='Ellipsoid')
        s.spike(wrist, vadd(wrist, (sx * -0.6, 3.2, -1.0)), 2.0, 'D', part=pid)       # thumb claw
        for k, (ex, ey, ez) in enumerate(((16, 8.5, 6), (12, 7.5, 6), (7.5, 8, 4.5))):
            plate(s, lerp(wrist, tip, 0.9 - k * 0.35), (sx * ex, ey, ez), 6.5, 2.4, 'A', pid, vertical=True,
                  kind='Ellipsoid')
        plate(s, root, (sx * 6.5, 8.5, 3), 5, 2.4, 'A', pid, vertical=True, kind='Ellipsoid')
    s.part(1, 0, (0, 13, -3.5), HEAD)
    s.seg((0, 12.5, -3.5), (0, 16, -6), 3.8, 3.8, 'P', part=1, kind='Cylinder')
    s.ell((0, 17.4, -7.2), (5.4, 4.6, 6), 'P', part=1)
    s.box((0, 17.8, -11.2), (3.6, 2.6, 5.5), 'P', (8, 0, 0), part=1)        # upper jaw
    s.box((0, 16.4, -11.4), (3.0, 1.0, 5), 'C', part=1)                     # teeth
    s.spike((0, 19.2, -6), (0, 20.2, -0.5), 3.0, 'P', part=1, d=2.2)         # head horn
    for sx in (-1, 1):
        s.box((sx * 2.0, 18.6, -9.2), (1.0, 1.0, 1.2), 'C', part=1)
        s.spike((sx * 1.8, 18.6, -6.5), (sx * 2.8, 20, -3.5), 2.0, 'P', part=1)
    s.part(2, 1, (0, 15.8, -6.5), JAW)
    s.box((0, 14.2, -10.5), (3.2, 2.2, 5.5), 'P', (-18, 0, 0), part=2)
    s.box((0, 15.1, -11.2), (2.6, 0.9, 4.4), 'C', (-18, 0, 0), part=2)
    pts = [(0, 10, 6), (0, 9.5, 10.5), (0, 9.5, 15), (0, 10, 18.5)]
    ids = chain(s, 20, 0, pts, [3.4, 2.8, 2.4, 2.2])
    s.box((0, 10.2, 19.8), (4.2, 1.8, 4.2), 'P', (0, 45, 0), part=ids[-1])   # spade tip
    return s


def steelix():
    """Iron snake: huge square-jawed head, barrel segments with side crystals, rearing pose (not Onix's boulders)."""
    s = S()
    pts = [(0, 22, -5), (0, 17, -3.5), (0, 12, -1.5), (0, 7.5, 1.5), (0, 4.5, 6), (-1.5, 3.8, 11), (-3, 4, 16),
           (-3.5, 4.5, 21), (-2.5, 5, 25.5)]
    rads = [7.5, 8, 8.2, 8, 7.4, 6.6, 5.6, 4.6, 3.4]
    ids = chain(s, 10, 0, pts, rads, 'P', kind='Cylinder', joints=False)
    for i, pid in enumerate(ids):
        a, b = pts[i], pts[i + 1]
        r = (rads[i] + rads[i + 1]) / 2
        s.ell(b, (r * 0.95, r * 0.95, r * 0.95), 'P', part=pid)           # rounded joint
        band(s, a, b, 0.08, r + 0.8, 'D', pid, thick=1.0)                   # segment seam
        if i in (0, 2, 4, 6):                                               # paired side crystals
            m = lerp(a, b, 0.5)
            for sx in (-1, 1):
                s.spike((m[0] + sx * r * 0.35, m[1], m[2]), (m[0] + sx * (r * 0.5 + 3.2), m[1] + 1.8, m[2] + 1.5),
                        2.8, 'A', part=pid, d=2.0)
    s.spike(vadd(pts[-1], (0, 0, 1)), vadd(pts[-1], (0, 1.5, 5.5)), 3.4, 'A', part=ids[-1], d=1.2)   # tail blade
    s.part(1, ids[0], (0, 22, -5), HEAD)
    s.box((0, 26, -8), (8, 6.5, 10), 'P', part=1)                          # blocky skull
    s.box((0, 23.2, -9), (8.6, 2.0, 10.5), 'A', part=1)                     # upper jaw plate
    for sx in (-1, 1):
        s.box((sx * 2.4, 27.4, -13.1), (2.2, 1.2, 0.8), 'C', (0, 0, sx * -12), part=1)   # red eyes
        s.spike((sx * 3.8, 28.5, -7), (sx * 6.5, 31, -2), 2.6, 'A', part=1, d=1.6)       # head spikes
        s.spike((sx * 4.2, 25, -6), (sx * 8, 25.5, -3), 2.4, 'A', part=1, d=1.6)
    s.spike((0, 29, -8), (0, 32.5, -4.5), 2.8, 'A', part=1, d=1.6)
    s.part(2, 1, (0, 22.5, -5), JAW)
    s.box((0, 20.3, -10), (9, 3.4, 11), 'P', part=2)                         # massive underbite
    for x in (-3, -1, 1, 3):
        s.box((x, 22.4, -15.0), (1.3, 1.4, 1.2), 'C', part=2)               # square teeth
    return s


def alakazam():
    """Thin psychic with a huge head, swept-back horns, long moustache, a spoon in each hand."""
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 2, 9.5, 0), role)
        s.seg((sx * 2, 9.5, 0), (sx * 2.4, 1.6, 0.2), 3.0, 3.2, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 2.3, 5.5, -0.5), (3.4, 3.0, 3.4), 'A', part=pid)          # brown knee
        s.ell((sx * 2.5, 1.0, -1.0), (3.2, 2.0, 4.6), 'P', part=pid)
    s.ell((0, 12.6, 0), (6.6, 8.4, 5), 'P')                                 # torso
    s.ell((0, 14, -1.2), (7, 5, 3.6), 'A')                                  # brown chest armour
    s.ell((0, 9.6, 0), (6, 2.6, 4.8), 'A')                                  # belt
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh = (sx * 3.4, 15.2, 0)
        s.part(pid, 0, sh, role)
        s.seg(sh, (sx * 5, 11.5, -1.6), 2.6, 2.6, 'P', part=pid, kind='Ellipsoid')
        s.seg((sx * 5, 11.5, -1.6), (sx * 5.4, 9, -3.6), 2.4, 2.4, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 5.4, 8.8, -3.8), (3.0, 3.0, 3.0), 'P', part=pid)
        s.seg((sx * 5.4, 8.6, -3.8), (sx * 5.6, 13, -5.6), 1.8, 1.8, 'C', part=pid, kind='Box')   # spoon
        s.ell((sx * 5.7, 14.4, -6.1), (2.8, 3.6, 2.0), 'C', (-20, 0, 0), part=pid)
    s.part(1, 0, (0, 16.5, 0), HEAD)
    s.ell((0, 19.6, 0), (7, 6.6, 6.4), 'P', part=1)                         # big head
    s.ell((0, 18, -3), (4.2, 3.4, 3.6), 'P', part=1)                        # snout
    for sx in (-1, 1):
        s.box((sx * 1.6, 20.2, -3.1), (2.0, 0.9, 0.8), 'D', (0, 0, sx * -18), part=1)    # narrow eyes
        s.spike((sx * 2.6, 22, 0.5), (sx * 5.2, 26, 4.2), 2.8, 'P', part=1, d=2.2)         # swept horns
        s.seg((sx * 1.0, 17.4, -4.4), (sx * 3.2, 11.8, -4.4), 2.0, 1.6, 'A', part=1, kind='Ellipsoid')   # moustache
    return s


def gardevoir():
    """Gown-swept psychic: bell skirt, slim waist, chest horn, bobbed hair with cheek points."""
    s = S()
    s.cone((0, 7.5, 0.5), (12.5, 15, 10.5), 'P')                          # the gown
    s.box((0, 5.5, -4.6), (3, 11, 3), 'P', sub=True)                      # front slit
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 0.9, 13, 0), role)
        s.seg((sx * 0.9, 13, 0), (sx * 1.0, 0.8, -1.8), 1.8, 1.8, 'P', part=pid, kind='Ellipsoid')
    s.ell((0, 16, 0), (4.8, 6.4, 4), 'P')                                  # slim torso
    s.ell((0, 18.4, 0), (5.6, 3, 4.2), 'A')                                # green shoulders
    s.spike((0, 16.6, -1.6), (0, 15.8, -5.6), 2.6, 'C', d=1.8)            # chest horn (front)
    s.spike((0, 16.6, 1.6), (0, 15.8, 5.0), 2.2, 'C', d=1.6)              # and back
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh = (sx * 2.4, 18.4, 0)
        s.part(pid, 0, sh, role)
        s.seg(sh, (sx * 4.2, 14.6, -1), 2.0, 2.0, 'A', part=pid, kind='Ellipsoid')
        s.seg((sx * 4.2, 14.6, -1), (sx * 3.8, 11, -2.4), 1.8, 1.8, 'A', part=pid, kind='Ellipsoid')
    s.part(1, 0, (0, 19.5, 0), HEAD)
    s.ell((0, 21.8, -0.2), (4.4, 4.6, 4.2), 'P', part=1)                   # face
    s.ell((0, 22.8, 0.6), (6, 4.6, 5.6), 'A', part=1)                      # bob
    s.box((0, 21.4, -1.9), (3.4, 3, 1.6), 'P', part=1)                     # face under the fringe
    for sx in (-1, 1):
        s.spike((sx * 2.4, 22, -0.6), (sx * 3.2, 18.4, -2), 2.2, 'A', part=1, d=1.8)     # cheek points
        s.box((sx * 0.9, 21.6, -2.7), (1.2, 1.2, 0.6), 'C', part=1)                     # red eyes
    return s


def absol():
    """Disaster fox-goat: white fluff, dark face, sickle horn on one side, blade tail."""
    s = S()
    s.ell((0, 9, 1.5), (7.5, 7.5, 13), 'P')
    s.ell((0, 9.5, -4), (7, 8.5, 5.5), 'P')                                # chest fluff
    s.spike((-1.5, 9, -5.5), (-4.2, 4.5, -6.5), 4.2, 'P', d=2.8)           # tuft sweep
    for pid, sx, z, role in ((4, -1, -3.5, LFL), (5, 1, -3.5, LFR), (6, -1, 5.5, LBL), (7, 1, 5.5, LBR)):
        s.part(pid, 0, (sx * 2.2, 7, z), role)
        s.seg((sx * 2.2, 7, z), (sx * 2.3, 0.9, z - 0.3), 2.8, 2.8, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 2.3, 0.8, z - 0.7), (2.8, 1.6, 3.0), 'A', part=pid)
    s.part(1, 0, (0, 11.5, -4.5), HEAD)
    s.ell((0, 14, -6.5), (5.6, 5.4, 6), 'A', part=1)                       # dark face
    s.ell((0, 13.2, -9.4), (3.4, 3.0, 3.4), 'A', part=1)
    s.ell((0, 16, -5.6), (6, 3, 5), 'P', part=1)                           # white brow fluff
    for sx in (-1, 1):
        s.box((sx * 1.5, 14.4, -9.2), (1.8, 1.0, 1.0), 'C', (0, 0, sx * -18), part=1)   # red eyes
    for a, b, w in (((2, 15.5, -5.5), (4.5, 20, -3.5), 3.0), ((4.5, 20, -3.5), (5, 22, 0.5), 2.6),
                    ((5, 22, 0.5), (4, 21.5, 3.5), 2.2)):
        s.seg(a, b, w, 2.0, 'D', part=1, kind='Box')                      # sickle horn (right side)
    s.spike((-1.8, 16, -4.5), (-3.4, 17.5, -1.5), 2.2, 'D', part=1)          # little left horn
    pts = [(0, 10, 7.5), (0, 13.5, 10), (0, 17, 11)]
    ids = chain(s, 20, 0, pts, [3.0, 2.6, 2.2], 'A')
    plate(s, (0, 13, 10), (0, 19.5, 12.5), 4.4, 2.0, 'A', ids[-1], vertical=True)
    plate(s, (0, 18, 11.5), (0, 17.5, 15), 3.2, 2.0, 'A', ids[-1], vertical=True)
    return s


def scizor():
    """Steel mantis: slim red body, huge pincer claws with eye markings, small wings, horned head."""
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 1.8, 10, 0.5), role)
        s.seg((sx * 1.8, 10, 0.5), (sx * 2.4, 5.5, -0.5), 2.4, 2.4, 'P', part=pid, kind='Ellipsoid')
        s.seg((sx * 2.4, 5.5, -0.5), (sx * 2.4, 1.0, 0.6), 2.2, 2.2, 'A', part=pid, kind='Ellipsoid')
        s.ell((sx * 2.4, 0.7, -0.8), (2.2, 1.4, 3.6), 'A', part=pid)
        s.cone((sx * 2.4, 1.2, 2.4), (1.2, 2.0, 1.2), 'A', TIP_PZ, part=pid)          # heel spike
    s.ell((0, 11, 1.2), (5, 5, 6), 'P')                                     # abdomen
    s.box((0, 11, 1.2), (5.2, 0.8, 6.2), 'D')
    s.ell((0, 15, 0), (5, 5.4, 4.2), 'P')                                   # thorax
    s.ell((0, 14.8, -1.6), (3.6, 3.4, 1.6), 'A')
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):
        s.part(pid, 0, (sx * 1.2, 16, 2), role)
        plate(s, (sx * 1.2, 16, 2), (sx * 4, 12.5, 6.5), 3.6, 1.6, 'A', pid, vertical=True, kind='Ellipsoid')
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh = (sx * 2.8, 16.5, 0)
        s.part(pid, 0, sh, role)
        s.seg(sh, (sx * 5, 14.5, -1), 2.4, 2.4, 'P', part=pid, kind='Ellipsoid')
        s.seg((sx * 5, 14.5, -1), (sx * 5.8, 13.5, -3.5), 2.4, 2.4, 'P', part=pid, kind='Ellipsoid')
        c = (sx * 6, 13.5, -6.8)
        s.ell(c, (5.4, 5.8, 7), 'P', part=pid)                              # pincer
        s.box((c[0], c[1], c[2] - 2.4), (5.8, 1.2, 4.2), 'D', part=pid, sub=True)   # the claw's open jaw
        s.box((c[0], c[1], c[2] - 2.4), (5.2, 0.8, 3.8), 'D', part=pid)
        for dy in (1.6, -1.6):
            s.ell((c[0] + sx * 2.4, c[1] + dy, c[2] - 0.2), (1.4, 1.8, 1.8), 'D', part=pid)   # eye markings
            s.box((c[0] + sx * 2.9, c[1] + dy, c[2] - 0.3), (0.6, 0.8, 0.8), 'C', part=pid)
    s.part(1, 0, (0, 17.5, 0), HEAD)
    s.ell((0, 19.6, -0.5), (4.2, 3.8, 4), 'P', part=1)
    for sx in (-1, 1):
        s.box((sx * 1.2, 19.9, -2.4), (1.6, 1.0, 0.8), 'C', (0, 0, sx * -15), part=1)
        s.spike((sx * 1.3, 21.2, -0.5), (sx * 1.8, 23.6, 0.8), 1.6, 'P', part=1)
    s.spike((0, 21, -1.8), (0, 22.4, -3.6), 1.4, 'P', part=1)
    return s


def ball_mon(big, face):
    """Voltorb / Electrode: two-tone sphere (P top, A bottom) with a centre seam and a face."""
    s = S()
    r = 7.5 if not big else 8
    s.part(1, 0, (0, r, 0), HEAD)
    s.ell((0, r, 0), (2 * r, 2 * r, 2 * r), 'P', part=1)
    for y in range(0, int(r)):
        rr = max(0.6, (r * r - (y + 0.5 - r) ** 2) ** 0.5)
        s.cyl((0, y + 0.5, 0), (2 * rr + 0.1, 1, 2 * rr + 0.1), 'A', part=1)
    s.cyl((0, r, 0), (2 * r + 0.3, 0.8, 2 * r + 0.3), 'D', part=1)          # seam
    face(s, r)
    return s


def voltorb_face(s, r):
    for sx in (-1, 1):
        s.box((sx * 2.7, r + 3.0, -r + 1.4), (3.6, 1.4, 1.6), 'D', (0, sx * -18, sx * 24), part=1)   # angry brows
        s.ell((sx * 2.6, r + 1.3, -r + 0.9), (3.0, 2.8, 1.6), 'C', part=1)
        s.box((sx * 2.3, r + 1.1, -r + 0.3), (1.2, 1.8, 0.8), 'D', part=1)


def electrode_face(s, r):
    for sx in (-1, 1):
        s.box((sx * 2.8, r + 3.4, -r + 1.6), (3.6, 1.4, 1.6), 'D', (0, sx * -18, sx * 25), part=1)
        s.ell((sx * 2.7, r + 1.8, -r + 1.0), (3.0, 2.8, 1.6), 'C', part=1)
        s.box((sx * 2.4, r + 1.6, -r + 0.4), (1.2, 1.8, 0.8), 'D', part=1)
    s.ell((0, r - 1.8, -r + 1.2), (8.5, 3.0, 1.6), 'D', part=1)             # wide grin
    s.box((0, r - 1.1, -r + 0.6), (7.0, 1.0, 1.0), 'C', part=1)


def cubone():
    """Lonely skull-helmet kid with a bone club."""
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 1.8, 3.2, 0.5), role)
        s.seg((sx * 1.8, 3.2, 0.5), (sx * 2.0, 1.0, 0.2), 2.2, 2.2, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 2.0, 0.8, -0.6), (2.4, 1.6, 3.4), 'P', part=pid)
        claws3(s, sx * 2.0, 0.5, -2.3, 'A', part=pid, spread=0.6, size=(0.5, 0.6, 0.8), dz=0)
    s.ell((0, 6.0, 0.4), (6.0, 6.4, 5.4), 'P')                              # round body
    s.ell((0, 5.6, -1.6), (4.2, 4.4, 2.4), 'A')                             # cream belly
    pts = [(0, 3.4, 2.8), (0, 2.6, 5.6)]
    ids = chain(s, 20, 0, pts, [2.2, 1.6])
    s.spike((0, 3.6, 4.2), (0, 5.0, 5.0), 1.0, 'A', part=ids[-1])
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh = (sx * 2.8, 7.6, 0)
        s.part(pid, 0, sh, role)
        s.seg(sh, (sx * 4.2, 5.8, -1.2), 1.6, 1.6, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 4.3, 5.6, -1.4), (1.6, 1.6, 1.6), 'P', part=pid)
    # the bone club in the right hand, raised
    s.seg((4.3, 5.0, -1.4), (4.6, 12.0, -1.0), 1.2, 1.2, 'A', part=9, kind='Box')
    for dx in (-0.8, 0.8):
        s.ell((4.6 + dx, 12.4, -1.0), (1.6, 1.6, 1.6), 'A', part=9)
        s.ell((4.3 + dx, 4.4, -1.4), (1.4, 1.4, 1.4), 'A', part=9)
    s.part(1, 0, (0, 8.5, 0), HEAD)
    s.ell((0, 10.6, -0.2), (6.2, 5.2, 5.6), 'P', part=1)                    # head
    s.ell((0, 11.6, 0.2), (6.6, 4.6, 6.2), 'A', part=1)                     # skull cap
    s.ell((0, 11.0, -3.0), (3.2, 2.8, 3.2), 'A', part=1)                    # skull snout
    for sx in (-1, 1):
        s.ell((sx * 1.6, 11.4, -3.0), (1.6, 1.6, 1.0), 'D', part=1)          # eye sockets
        s.box((sx * 1.6, 11.1, -3.4), (0.5, 0.5, 0.4), 'C', part=1)
        s.spike((sx * 2.4, 13.2, 1.6), (sx * 3.6, 14.2, 3.6), 1.4, 'A', part=1)    # skull horns
    return s


def meowth():
    """Scratch cat: wide head, gold charm coin, whiskers, curly tail."""
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 1.3, 4.0, 0.2), role)
        s.seg((sx * 1.3, 4.0, 0.2), (sx * 1.4, 1.0, 0.0), 1.8, 1.8, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 1.5, 0.8, -0.6), (2.2, 1.6, 3.0), 'D', part=pid)        # brown paws
    s.ell((0, 5.6, 0.2), (4.0, 5.0, 3.4), 'P')                              # body
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh = (sx * 1.8, 7.0, 0)
        s.part(pid, 0, sh, role)
        s.seg(sh, (sx * 3.0, 5.2, -1.2), 1.6, 1.6, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 3.1, 5.0, -1.3), (1.8, 1.8, 1.8), 'P', part=pid)
    s.part(1, 0, (0, 7.8, 0), HEAD)
    s.ell((0, 10.4, 0), (8.6, 6.2, 6.0), 'P', part=1)                      # wide head
    s.ell((0, 12.4, -2.5), (3.2, 2.8, 1.4), 'A', part=1)                   # charm coin
    for sx in (-1, 1):
        s.cone((sx * 2.8, 14.2, 0.2), (2.8, 3.6, 1.8), 'P', (0, 0, sx * -20), part=1)     # ears
        s.cone((sx * 2.8, 14.0, -0.5), (1.6, 2.4, 0.6), 'D', (0, 0, sx * -20), part=1)
        s.ell((sx * 1.7, 10.6, -2.7), (1.6, 2.0, 0.8), 'C', part=1)                         # eyes
        s.box((sx * 1.7, 10.6, -3.1), (0.5, 1.4, 0.4), 'D', part=1)
        for dy in (0.6, -0.6):
            s.box((sx * 4.8, 9.6 + dy, -1.4), (3.6, 0.9, 0.9), 'D', (0, 0, sx * dy * 20), part=1)   # whiskers
    s.box((0, 9.1, -3.0), (1.6, 0.6, 0.5), 'D', part=1)
    pts = [(0, 4.0, 1.6), (0, 3.0, 4.5), (0, 5.5, 6.5), (0, 8.5, 6.0), (0, 9.8, 4.5)]
    ids = chain(s, 20, 0, pts, [1.8, 1.8, 1.8, 1.8, 1.8])
    s.ell((0, 9.2, 3.6), (2.0, 2.2, 2.2), 'P', part=ids[-1])                 # curled tip
    return s


def farfetchd():
    """Wild duck brandishing a leek like a sword."""
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 1.6, 3.5, 0.5), role)
        s.seg((sx * 1.6, 3.5, 0.5), (sx * 1.7, 0.8, 0.2), 1.8, 1.8, 'D', part=pid, kind='Box')
        s.ell((sx * 1.7, 0.7, -0.9), (2.6, 1.2, 3.2), 'D', part=pid)
    s.ell((0, 6.5, 0.6), (7.4, 7.2, 8.6), 'P')
    s.spike((0, 7.5, 4), (0, 9.5, 7.5), 3.6, 'P', d=2.4)                    # tail feathers
    s.part(1, 0, (0, 9, -1.4), HEAD)
    s.ell((0, 11.8, -1.6), (5.6, 5.6, 5.6), 'P', part=1)
    s.box((0, 11.2, -5.4), (3.2, 1.6, 3.6), 'A', part=1)                    # flat duck bill
    for sx in (-1, 1):
        s.box((sx * 1.6, 12.9, -4.2), (2.0, 0.9, 0.9), 'D', (0, 0, sx * -20), part=1)    # black brows
    s.part(10, 0, (-3.2, 8.5, 0), WING_L)
    plate(s, (-3.2, 8.5, 0), (-4.2, 5, 3.4), 3.6, 1.8, 'P', 10, vertical=True, kind='Ellipsoid')
    s.part(11, 0, (3.2, 8.5, 0), WING_R)
    s.seg((3.2, 8.5, 0), (4.6, 7.2, -2.8), 2.8, 2.0, 'P', part=11, kind='Ellipsoid')
    s.seg((4.6, 4.4, -2.2), (5.0, 14.5, -5.4), 2.2, 2.2, 'C', part=11, kind='Box')    # white stalk
    for dx, dz in ((-0.9, 0.6), (0.9, 0.6), (0, -0.9)):
        s.spike((5.0, 13.8, -5.2), (5.0 + dx * 2.4, 18.2, -5.8 + dz * 2), 2.4, 'A', part=11, d=1.6)   # green leaves
    return s


def tauros():
    """Wild bull: big horns, shaggy mane, three whip tails."""
    s = S()
    s.ell((0, 10, 1), (9.5, 8.5, 15), 'P')
    s.ell((0, 11.5, -4), (10.5, 9.5, 7), 'A')                              # mane over the shoulders
    for sx in (-1, 1):
        s.ell((sx * 4.2, 12, 5.5), (2.4, 2.4, 2.4), 'D')                   # hip knobs
    for pid, sx, z, role in ((4, -1, -3.5, LFL), (5, 1, -3.5, LFR), (6, -1, 6, LBL), (7, 1, 6, LBR)):
        s.part(pid, 0, (sx * 3, 8, z), role)
        s.seg((sx * 3, 8, z), (sx * 3.2, 1.4, z - 0.3), 3.2, 3.2, 'P', part=pid, kind='Ellipsoid')
        s.box((sx * 3.2, 0.8, z - 0.4), (3.0, 1.6, 3.0), 'D', part=pid)
    s.part(1, 0, (0, 12, -6), HEAD)
    s.ell((0, 12, -9.2), (6, 5.6, 6), 'P', part=1)
    s.ell((0, 10.6, -12), (4.4, 3.6, 3), 'P', part=1)                      # muzzle
    s.ell((0, 15, -8.4), (6.4, 2.8, 4.6), 'A', part=1)                     # forelock
    for x in (-1.4, 0, 1.4):
        s.ell((x, 15.2, -11.0), (1.4, 1.4, 1.4), 'D', part=1)               # forehead knobs
    for sx in (-1, 1):
        s.box((sx * 1.8, 12.8, -12.0), (1.6, 0.9, 0.8), 'C', (0, 0, sx * -20), part=1)
        s.seg((sx * 2.6, 13.4, -8.5), (sx * 6.8, 14.4, -9.5), 2.6, 2.6, 'C', part=1, kind='Ellipsoid')   # horns out
        s.spike((sx * 6.4, 14.2, -9.4), (sx * 6.6, 18.8, -12.5), 2.6, 'C', part=1, d=2.2)                # and up
    for n, sx in enumerate((-1, 0, 1)):                                      # three tails
        base = (sx * 1.4, 11, 8)
        mid = (sx * 3.2, 10.5, 11.5)
        tip = (sx * 4.6, 7.5, 13.8)
        s.part(20 + n * 2, 0, base, TAIL)
        s.seg(base, mid, 1.8, 1.8, 'D', part=20 + n * 2)
        s.part(21 + n * 2, 20 + n * 2, mid, TAIL)
        s.seg(mid, tip, 1.8, 1.8, 'D', part=21 + n * 2)
        s.ell(tip, (2.4, 3.0, 2.4), 'A', part=21 + n * 2)
    return s


def golem():
    """Rock-shell boulder with stubby limbs poking out; it rolls and blows itself up."""
    s = S()
    s.ell((0, 8.5, 0.5), (15, 13.5, 13.5), 'P')                            # boulder shell
    for (x, y, z) in ((-5, 13, -3), (4, 14, -2), (0, 15, 3), (-6, 9, 4), (6, 10, 4), (-2, 12, -6), (5, 6, -5)):
        s.ell((x, y, z), (4, 3, 4), 'P')                                    # lumpy rocks
        s.box((x, y + 1.2, z), (2, 0.6, 2), 'D')
    s.ell((0, 6.5, -5.2), (8.5, 7.5, 3.4), 'A')                            # skin belly
    s.part(1, 0, (0, 9, -5), HEAD)
    s.ell((0, 10, -7), (5, 4.6, 4), 'A', part=1)                           # face poking out
    s.box((0, 9.0, -8.9), (3.2, 0.6, 0.5), 'D', part=1)
    for sx in (-1, 1):
        s.box((sx * 1.3, 10.8, -8.9), (1.1, 0.9, 0.5), 'C', (0, 0, sx * 15), part=1)
        s.box((sx * 1.3, 11.8, -8.7), (1.6, 0.6, 0.6), 'D', (0, 0, sx * -15), part=1)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh = (sx * 6.5, 9.5, -2)
        s.part(pid, 0, sh, role)
        s.seg(sh, (sx * 9.5, 6.5, -3.5), 2.6, 2.6, 'A', part=pid, kind='Ellipsoid')
        s.ell((sx * 9.8, 6.2, -3.8), (2.6, 2.6, 2.6), 'A', part=pid)
        claws3(s, sx * 9.8, 6.0, -5.0, 'D', part=pid, spread=0.7, size=(0.6, 0.6, 1.2), dz=0)
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 3.8, 3.2, -1), role)
        s.seg((sx * 3.8, 3.2, -1), (sx * 4.2, 0.9, -1.5), 3.2, 3.2, 'A', part=pid, kind='Ellipsoid')
        s.ell((sx * 4.2, 0.8, -2.4), (3.4, 1.6, 3.8), 'A', part=pid)
    return s


def weezing():
    """Twin toxic spheres with skull-and-crossbones marks and smoke vents."""
    s = S()
    s.part(1, 0, (0, 8, 0), HEAD)
    s.ell((-2.5, 9, 0), (11, 11, 11), 'P', part=1)                         # big head
    s.ell((-2.5, 5.2, -4.8), (3.4, 2.6, 1.2), 'A', part=1)                 # skull mark
    s.box((-2.5, 4.4, -5.2), (4.6, 0.8, 0.8), 'A', (0, 0, 30), part=1)
    s.box((-2.5, 4.4, -5.2), (4.6, 0.8, 0.8), 'A', (0, 0, -30), part=1)
    for sx in (-1, 1):
        s.box((-2.5 + sx * 1.8, 11.2, -5.1), (1.8, 1.2, 0.8), 'C', (0, 0, sx * -12), part=1)
    s.ell((-2.5, 8.3, -5.2), (4.2, 1.4, 0.8), 'D', part=1)                 # mouth
    for (x, y, z) in ((-7.5, 12, 0), (-4, 15, -1), (1.5, 14, 1)):
        s.cyl((x, y, z), (2.4, 2.4, 2.4), 'P', (0, 0, (x + 2.5) * -6), part=1)   # vents
        s.ell((x * 1.05, y + 1.4, z), (2.8, 2.6, 2.8), 'C', part=1)            # puff of gas
    s.part(2, 1, (3, 9, 0), OTHER)
    s.ell((6, 7, 1), (7.5, 7.5, 7.5), 'P', part=2)                          # small head
    s.box((6, 8.2, -2.8), (1.2, 0.8, 0.8), 'C', part=2)
    s.box((4.4, 8.2, -2.6), (1.2, 0.8, 0.8), 'C', part=2)
    s.ell((5.4, 5.5, -2.9), (2.2, 1.0, 0.6), 'D', part=2)
    s.ell((5.4, 4.2, -2.6), (2.0, 1.2, 0.8), 'A', part=2)
    s.cyl((8.5, 10.5, 1), (2, 2, 2), 'P', (0, 0, -30), part=2)
    s.ell((9.2, 11.8, 1), (2.4, 2.2, 2.4), 'C', part=2)
    s.cyl((3, 8, 0), (4, 3, 4), 'P', ALONG_X)                               # the joining tube
    return s


def starmie():
    """Double star standing on a point, gold setting, glowing jewel core; it spins."""
    s = S()
    c = 9.2
    s.part(1, 0, (0, c, 0), HEAD)
    for k in range(5):                                                      # front star
        a = math.radians(90 + k * 72)
        tip = (math.cos(a) * 9.2, c + math.sin(a) * 9.2, -0.8)
        s.spike((0, c, -0.8), tip, 6.4, 'P', part=1, d=3.4)
    for k in range(5):                                                      # rear star, offset 36 deg
        a = math.radians(126 + k * 72)
        tip = (math.cos(a) * 8.6, c + math.sin(a) * 8.6, 1.8)
        s.spike((0, c, 1.8), tip, 5.6, 'P', part=1, d=3.0)
    s.cyl((0, c, -2.2), (8, 2.4, 8), 'A', ALONG_Z, part=1)                  # gold setting
    s.box((0, c, -3.2), (4.8, 4.8, 2.0), 'C', (0, 0, 45), part=1)          # the jewel
    return s


def tentacruel():
    """Jellyfish: dome head with two red gems, beak mouth, a skirt of long tentacles."""
    s = S()
    s.part(1, 0, (0, 9, 0), HEAD)
    s.ell((0, 14, 0), (11, 10, 10), 'P', part=1)                           # dome
    s.ell((0, 10, 0), (10.5, 3.5, 9.5), 'P', part=1)
    for sx in (-1, 1):
        s.ell((sx * 3.2, 14.5, -3.8), (3.0, 3.4, 2.6), 'A', part=1)        # red gems
        s.ell((sx * 3.4, 15.2, -4.6), (1.0, 1.0, 0.8), 'C', part=1)
        s.box((sx * 2.4, 10.8, -4.8), (1.6, 0.8, 0.8), 'D', (0, 0, sx * 20), part=1)   # eyes under the gems
    s.ell((0, 17.5, -3.4), (1.8, 1.8, 1.8), 'A', part=1)                   # small top gem
    s.cone((0, 8.5, -3.8), (2.0, 2.2, 1.6), 'D', TIP_NY, part=1)           # beak
    nid = 20
    for k in range(6):                                                      # tentacles all round
        a = math.radians(k * 60 + 30)
        dx, dz = math.cos(a), math.sin(a)
        pts = [(dx * 3.5, 8.5, dz * 3.5), (dx * 6, 5, dz * 6), (dx * 8, 2, dz * 8), (dx * 9.5, 0.8, dz * 9.5)]
        ids = chain(s, nid, 0, pts, [2.4, 2.0, 1.6, 1.4], 'P', kind='Ellipsoid', joints=False)
        nid += 4
    return s


def wing_hand(s, pid, sx, root, elbow, wrist, fingers, arm_w=4.6, fw=4.2, ft=2.4, tip_t='A', tip_r=3.0, band_t=None):
    """Arm (root->elbow->wrist) + fanned finger feathers; fingers = [(angle_deg_from_outward, length)]."""
    s.seg(root, elbow, arm_w, arm_w * 0.7, 'P', part=pid, kind='Ellipsoid')
    s.seg(elbow, wrist, arm_w * 0.9, arm_w * 0.65, 'P', part=pid, kind='Ellipsoid')
    s.ell(wrist, (arm_w, arm_w, arm_w * 0.7), 'P', part=pid)
    for k, (a, ln) in enumerate(fingers):
        r = math.radians(a)
        tip = (wrist[0] + sx * math.cos(r) * ln, wrist[1] + math.sin(r) * ln, wrist[2] + 0.4 + k * 0.5)
        s.seg(wrist, tip, fw, ft, 'P', part=pid, kind='Ellipsoid')
        if band_t:
            s.seg(lerp(wrist, tip, 0.5), lerp(wrist, tip, 0.78), fw, ft + 0.2, band_t, part=pid, kind='Ellipsoid')
        if tip_t:
            s.ell(lerp(wrist, tip, 0.9), (tip_r, tip_r, tip_r * 0.7), tip_t, part=pid)


def vulpix():
    """Six-tailed fox cub: big head with the red forelock curls, tails curling up behind like a fan."""
    s = S()
    s.ell((0, 6, 1), (5.6, 5.6, 9), 'P')
    for pid, sx, z, role in ((4, -1, -2.2, LFL), (5, 1, -2.2, LFR), (6, -1, 3.8, LBL), (7, 1, 3.8, LBR)):
        s.part(pid, 0, (sx * 1.8, 5, z), role)
        s.seg((sx * 1.8, 5, z), (sx * 1.9, 0.9, z - 0.2), 2.4, 2.4, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 1.9, 0.8, z - 0.6), (2.6, 1.6, 3.0), 'P', part=pid)
    s.part(1, 0, (0, 8, -3), HEAD)
    s.ell((0, 10.5, -4.5), (7.4, 6.4, 6.4), 'P', part=1)
    s.ell((0, 9.4, -7.6), (3.4, 2.8, 3.4), 'P', part=1)                    # snout
    s.box((0, 9.8, -9.3), (1.4, 1.0, 0.8), 'D', part=1)                    # nose
    for sx in (-1, 1):
        s.spike((sx * 2.2, 12.5, -4), (sx * 3.8, 17, -3.2), 3.4, 'P', part=1, d=2.0)       # ears
        s.spike((sx * 2.3, 12.8, -4.6), (sx * 3.5, 15.8, -4.0), 1.8, 'D', part=1, d=1.0)
        s.ell((sx * 1.8, 11, -7.3), (1.6, 2.0, 1.0), 'D', part=1)                         # eyes
    for dx, dy, dz in ((0, 13.8, -6.4), (-1.8, 13.2, -6.8), (1.8, 13.2, -6.8)):
        s.ell((dx, dy, dz), (2.8, 2.8, 2.8), 'A', part=1)                  # forelock curls
    base = (0, 7.5, 5)
    for i in range(6):
        a = math.radians(-70 + 28 * i)
        mid = (math.sin(a) * 2.4, 7.5 + math.cos(a) * 3.5, 7.5)
        tip = (math.sin(a) * 5.6, 8.5 + math.cos(a) * 7.0, 9.6)
        s.part(20 + 2 * i, 0, base, TAIL)
        s.seg(base, mid, 2.4, 2.4, 'P', part=20 + 2 * i, kind='Ellipsoid')
        s.part(21 + 2 * i, 20 + 2 * i, mid, TAIL)
        s.seg(mid, tip, 2.8, 2.8, 'P', part=21 + 2 * i, kind='Ellipsoid')
        s.ell(vadd(tip, (0, -0.6, 0.8)), (3.2, 3.2, 3.2), 'A', part=21 + 2 * i)     # curled tip
    return s


def ninetales():
    """Nine-tailed fox: elegant long legs, chest ruff, tall crest, a peacock fan of nine tails."""
    s = S()
    s.ell((0, 9.5, 1), (6.4, 6.4, 12), 'P')
    s.ell((0, 11, -4.2), (7, 8, 5.4), 'A')                                 # chest ruff
    for pid, sx, z, role in ((4, -1, -3.2, LFL), (5, 1, -3.2, LFR), (6, -1, 5, LBL), (7, 1, 5, LBR)):
        s.part(pid, 0, (sx * 2, 8, z), role)
        s.seg((sx * 2, 8, z), (sx * 2.2, 0.9, z - 0.3), 2.8, 2.8, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 2.2, 0.8, z - 0.8), (2.8, 1.6, 3.2), 'P', part=pid)
    s.part(1, 0, (0, 13, -5), HEAD)
    s.ell((0, 15.5, -6.5), (6, 5.4, 6), 'P', part=1)
    s.spike((0, 14.8, -8.5), (0, 14.2, -12.5), 3.4, 'P', part=1, d=2.6)     # snout
    for sx in (-1, 1):
        s.spike((sx * 1.9, 17.5, -6), (sx * 3.2, 21.5, -5.2), 3.0, 'P', part=1, d=1.8)       # ears
        s.box((sx * 1.6, 16.0, -9.2), (1.8, 1.0, 0.8), 'C', (0, 0, sx * -15), part=1)       # red eyes
    s.spike((0, 17.8, -7.5), (0, 22.8, -4), 3.4, 'A', part=1, d=2.0)        # tall crest tuft
    s.spike((0, 17.4, -6.5), (0, 20.2, -1.5), 2.6, 'A', part=1, d=1.6)
    base = (0, 11, 6.5)
    for i in range(9):
        a = math.radians(-80 + 20 * i)
        mid = (math.sin(a) * 4, 11 + math.cos(a) * 5, 9.5)
        tip = (math.sin(a) * 9.5, 11 + math.cos(a) * 10.5 - abs(math.sin(a)) * 1.0, 11.5)
        s.part(20 + 2 * i, 0, base, TAIL)
        s.seg(base, mid, 3.2, 3.2, 'P', part=20 + 2 * i, kind='Ellipsoid')
        s.part(21 + 2 * i, 20 + 2 * i, mid, TAIL)
        s.seg(mid, tip, 3.6, 3.6, 'P', part=21 + 2 * i, kind='Ellipsoid')
        s.ell(lerp(mid, tip, 0.88), (3.2, 3.2, 3.2), 'A', part=21 + 2 * i)          # pale tips
    return s


# ================================================================================================ register
def register():
    # ---------------- Kaiju-scale legends
    icon(["rayquaza", "rayquasa", "ray quaza", "rayqueza", "raquaza"], "Rayquaza", "Kaiju",
         ["Flies", "Breath", "ArticulatedTail"], 14.0, 25000, element="Wind", accent=YELLOW,
         temperament="Aggressive", intensity=0.6, shape=rayquaza())
    icon(["groudon", "groundon", "grudon", "groudan"], "Groudon", "Kaiju",
         ["Stomp", "Heavy", "Breath", "ArticulatedTail"], 16.0, 50000, element="Fire", accent=GREY,
         temperament="Aggressive", intensity=0.7, shape=groudon())
    icon(["kyogre", "kyorge", "kiogre", "kyogr"], "Kyogre", "Kaiju",
         ["Flies", "Beam", "Repel"], 10.0, 40000, element="Water", accent=RED, shape=kyogre())
    icon(["wailord", "whailord", "wail lord", "wailor"], "Wailord", "Kaiju",
         ["Heavy", "Bounce", "Stomp"], 20.0, 60000, element="Water", accent=CREAM,
         temperament="Playful", intensity=0.4, shape=wailord())
    icon(["dialga", "dialgia", "dielga", "dyalga"], "Dialga", "Kaiju",
         ["Beam", "Stomp", "Heavy"], 16.0, 45000, element="Atomic", accent=CYAN, shape=dialga())
    icon(["palkia", "palkya", "palika", "pal kia"], "Palkia", "Kaiju",
         ["Portal", "Blade", "Beam"], 15.0, 35000, element="Water", accent=PINK, shape=palkia())
    icon(["giratina", "giritina", "giratena", "girateena"], "Giratina", "Kaiju",
         ["Flies", "Portal", "Stomp"], 16.0, 45000, element="Shadow", accent=GOLD,
         temperament="Aggressive", intensity=0.7, shape=giratina())
    # ---------------- regular
    icon(["metagross", "meta gross", "metagros", "metagrass"], "Metagross", "Quadruped",
         ["Heavy", "Stomp", "Guard"], 1.6, 550, element="Psychic", accent=GREY, shape=metagross())
    icon(["mew", "mew pokemon", "mu pokemon"], "Mew", "Biped",
         ["Flies", "Beam", "Barrier"], 0.4, 4.0, element="Psychic", accent=BLUE,
         temperament="Playful", intensity=0.8, shape=mew())
    icon(["lugia", "lugiya", "lujia", "lugea"], "Lugia", "Bird",
         ["Flies", "Beam", "Repel"], 5.2, 216, element="Wind", accent=(0.3, 0.45, 0.8), shape=lugia())
    icon(["ho-oh", "ho oh", "hooh", "ho-o", "hoho"], "Ho-Oh", "Bird",
         ["Flies", "Breath", "Glide"], 3.8, 199, element="Fire", accent=GOLD, shape=ho_oh(), id="ho_oh")
    icon(["zapdos", "zapados", "zap dos", "zapdoss"], "Zapdos", "Bird",
         ["Flies", "Shoots", "Pierce"], 1.6, 52.6, element="Electric", accent=ORANGE, shape=zapdos())
    icon(["moltres", "moltress", "moltris", "moltres bird"], "Moltres", "Bird",
         ["Flies", "Breath", "Glide"], 2.0, 60, element="Fire", accent=ORANGE, shape=moltres())
    icon(["articuno", "articono", "arctikuno", "articuno bird"], "Articuno", "Bird",
         ["Flies", "Breath", "SlowFall"], 1.7, 55.4, element="Ice", accent=ICE, shape=articuno())
    icon(["aerodactyl", "aerodactl", "aero dactyl", "arodactyl"], "Aerodactyl", "Bird",
         ["Flies", "Grab", "Pierce"], 1.8, 59, element="Earth", accent=LILAC,
         temperament="Aggressive", intensity=0.7, shape=aerodactyl())
    icon(["steelix", "steelics", "steel onix", "steelex"], "Steelix", "Serpent",
         ["Heavy", "Constrict", "Blunt", "Guard"], 9.2, 400, element="Light", accent=(0.45, 0.48, 0.56),
         shape=steelix())
    icon(["alakazam", "alakazan", "alakazaam", "alacazam"], "Alakazam", "Biped",
         ["Beam", "Portal", "Barrier"], 1.5, 48, element="Psychic", accent=BROWN, shape=alakazam())
    icon(["gardevoir", "gardevoire", "gardivoir", "gardevwar"], "Gardevoir", "Biped",
         ["Barrier", "Beam", "SlowFall"], 1.6, 48.4, element="Psychic", accent=GREEN,
         temperament="Protective", intensity=0.8, shape=gardevoir())
    icon(["absol", "absoll", "abseol", "absol pokemon"], "Absol", "Quadruped",
         ["Blade", "Charge", "Fast"], 1.2, 47, element="Shadow", accent=(0.25, 0.3, 0.45),
         temperament="Sneaky", intensity=0.5, shape=absol())
    icon(["scizor", "scissor pokemon", "sizor", "scyzor"], "Scizor", "Biped",
         ["Grab", "Blade", "Fast"], 1.8, 118, element="Light", accent=GREY,
         temperament="Aggressive", intensity=0.5, shape=scizor())
    icon(["voltorb", "voltorbe", "volt orb", "voltourb"], "Voltorb", "Blob",
         ["Explodes", "Bounce", "Spin"], 0.5, 10.4, element="Electric", accent=WHITE,
         temperament="Chaotic", intensity=0.8, shape=ball_mon(False, voltorb_face))
    icon(["electrode", "electrode pokemon", "electrod", "elektrode"], "Electrode", "Blob",
         ["Explodes", "Fast", "Bounce"], 1.2, 66.6, element="Electric", accent=RED,
         temperament="Chaotic", intensity=0.7, shape=ball_mon(True, electrode_face))
    icon(["cubone", "cubbone", "qbone", "cuboné"], "Cubone", "Biped",
         ["Blunt", "Returning"], 0.4, 6.5, element="Earth", accent=CREAM,
         temperament="Fearful", intensity=0.3, shape=cubone())
    icon(["vulpix", "vulpicks", "vulpex", "volpix"], "Vulpix", "Quadruped",
         ["Breath", "Fast"], 0.6, 9.9, element="Fire", accent=ORANGE,
         temperament="Playful", intensity=0.4, shape=vulpix())
    icon(["ninetales", "nine tales", "ninetails", "nine tails"], "Ninetales", "Quadruped",
         ["Breath", "Fast", "Homing"], 1.1, 19.9, element="Fire", accent=CREAM,
         temperament="Sneaky", intensity=0.4, shape=ninetales())
    icon(["meowth", "meouth", "meowf", "meowth pokemon"], "Meowth", "Biped",
         ["Shoots", "Blade"], 0.4, 4.2, element="Light", accent=GOLD,
         temperament="Playful", intensity=0.6, shape=meowth())
    icon(["farfetch'd", "farfetchd", "farfetched", "far fetched", "farfetch"], "Farfetch'd", "Bird",
         ["Blade", "Glide", "Charge"], 0.8, 15, element="Wind", accent=GREEN, shape=farfetchd(), id="farfetchd")
    icon(["tauros", "taurus pokemon", "tauros bull", "toros"], "Tauros", "Quadruped",
         ["Charge", "Heavy", "Stomp"], 1.4, 88.4, element="Light", accent=BROWN,
         temperament="Aggressive", intensity=0.9, shape=tauros())
    # ---------------- stretch
    icon(["golem", "golem pokemon", "gollem pokemon", "golom"], "Golem", "Biped",
         ["Heavy", "Explodes", "Charge"], 1.4, 300, element="Earth", accent=BROWN, shape=golem())
    icon(["weezing", "weezin", "wheezing", "weesing"], "Weezing", "Blob",
         ["Breath", "Explodes", "Flies"], 1.2, 9.5, element="Poison", accent=CREAM,
         temperament="Chaotic", intensity=0.5, shape=weezing())
    icon(["starmie", "starme", "star mie", "starmy"], "Starmie", "Blob",
         ["Spin", "Beam"], 1.1, 80, element="Water", accent=GOLD, shape=starmie())
    icon(["tentacruel", "tentacruel pokemon", "tentacrual", "tentacruell"], "Tentacruel", "Blob",
         ["Constrict", "Grab"], 1.6, 55, element="Poison", accent=RED, shape=tentacruel())
