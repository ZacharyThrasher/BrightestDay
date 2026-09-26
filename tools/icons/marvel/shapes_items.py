"""Hand-authored ShapeSpecs for the Marvel + X-Men pack: items, vehicles, structures.

Held items: origin = grip, -Z = the way the item points. Vehicles/structures: origin = ground centre.
"""
from mvlib import Shape
from shapes_heroes import check, SIDES


def wheel(s, pid, c, d, w, parent=0):
    s.part(pid, parent, c, "Wheel")
    s.cx(c, d, w, "D", part=pid)
    s.cx(c, d * 0.45, w + 0.2, "A", part=pid)


# ------------------------------------------------------------------ held

def wakandan_spear():
    s = Shape()
    s.cz((0, 0, -3.5), 0.8, 15.0, "D")
    s.bx((0, 0, 0.0), (1.0, 1.0, 1.6), "A")
    for z in (-2.0, -5.0):
        s.bx((0, 0, z), (1.0, 1.0, 0.5), "A")
    s.bx((0, 0, -11.6), (0.5, 2.2, 3.2), "C")          # broad leaf blade
    s.bx((0, 0, -11.6), (0.5, 1.6, 3.2), "C", rot=(45, 0, 0))
    s.cone((0, 0, -14.4), 0.5, 2.6, "C", "-z", d2=1.9)
    s.bx((0, 0, -10.0), (1.1, 1.1, 0.6), "A")
    s.cone((0, 0, 4.6), 0.7, 1.2, "A", "+z")
    return check(s)


def cap_heater_shield():
    s = Shape()
    s.bx((0, 0, 0), (1.0, 2.6, 1.0), "D")
    z = -1.2
    s.bx((0, 1.2, z), (8.0, 4.2, 0.8), "P")
    s.bx((0, -1.6, z), (5.6, 3.0, 0.8), "P", rot=(0, 0, 0))
    s.bx((0, -3.6, z), (3.0, 3.0, 0.8), "P", rot=(0, 0, 45))
    for x in (-2.4, 0.0, 2.4):      # the stripes
        s.bx((x, -1.2, z - 0.45), (1.0, 4.4, 0.2), "A")
    s.bx((0, 2.2, z - 0.45), (7.6, 2.0, 0.2), "A")
    s.bx((0, 2.2, z - 0.6), (1.2, 1.2, 0.2), "C", rot=(0, 0, 45))   # the star
    return check(s)


def wakandan_shield():
    s = Shape()
    s.bx((0, 0, 0), (0.9, 2.4, 0.9), "D")
    s.el((0, 0, -1.3), (6.4, 9.0, 1.2), "P")
    s.el((0, 0, -1.7), (5.2, 7.8, 0.9), "A")
    for y in (-2.4, 0.0, 2.4):
        s.bx((0, y, -2.2), (4.4, 0.4, 0.3), "C")
    s.bx((0, 0, -2.3), (0.4, 7.0, 0.3), "D")
    return check(s)


def gambit_card():
    s = Shape()
    s.bx((0, 0, -1.4), (0.2, 2.2, 3.2), "P")
    s.bx((0, 0, -1.4), (0.25, 1.8, 2.8), "C")
    s.bx((0.1, 0, -1.4), (0.3, 0.8, 0.8), "A", rot=(45, 0, 0))
    s.el((0, 0, -1.4), (1.2, 3.0, 4.0), "A")  # the kinetic glow
    s.el((0, 0, -1.4), (1.0, 2.6, 3.6), "A", sub=True)
    return check(s)


def yaka_arrow():
    s = Shape()
    s.cz((0, 0, -3.0), 0.35, 9.0, "A")
    s.cone((0, 0, -8.2), 0.5, 1.4, "C", "-z")
    s.bx((0, 0.5, 0.2), (0.15, 1.0, 3.2), "P")          # the fin crest
    s.bx((0, 0.25, 1.9), (0.3, 0.6, 1.2), "D")
    s.bx((0, 0, -2.0), (0.4, 0.4, 0.4), "C")
    s.bx((0, 0.3, -0.8), (0.2, 0.8, 0.6), "C")
    return check(s)


def element_gun():
    s = Shape()
    s.bx((0, -1.0, 0.8), (1.0, 2.4, 1.2), "D", rot=(-12, 0, 0))
    s.el((0, 0.4, 0.4), (2.0, 2.0, 2.2), "P")
    s.cz((0, 0.4, -1.8), 1.3, 3.0, "A")
    s.cz((0, 0.4, -3.6), 0.9, 1.0, "C")
    s.cx((0, 0.4, 0.4), 0.8, 2.3, "C")
    s.bx((0, 1.4, -1.0), (0.4, 0.4, 2.0), "D")
    return check(s)


def groot_branches():
    s = Shape()
    s.bx((0, 0, 0.4), (1.4, 1.4, 1.8), "D")
    pts = [(0, 0, 0), (0.4, 0.6, -3.0), (-0.3, 1.2, -6.0), (0.3, 0.8, -9.0), (0.0, 0.0, -12.0)]
    w = [1.3, 1.1, 0.9, 0.7]
    for i, (a, b) in enumerate(zip(pts, pts[1:])):
        s.seg(a, b, w[i], w[i], "P", kind="Cylinder", over=0.4)
    for (a, b) in (((0.4, 0.6, -3.0), (1.8, 2.2, -4.4)), ((-0.3, 1.2, -6.0), (-1.8, 2.6, -7.0)),
                   ((0.3, 0.8, -9.0), (1.6, -0.6, -10.0))):
        s.seg(a, b, 0.45, 0.45, "D", kind="Cylinder", over=0.2)
        s.el(b, (0.9, 0.9, 0.9), "A")
    s.el((0, 0, -12.2), (1.2, 1.2, 1.2), "A")
    return check(s)


def photon_gauntlet():
    s = Shape()
    s.cz((0, 0, 2.4), 3.6, 2.4, "A")
    s.bx((0, 0, -0.2), (4.2, 3.4, 3.4), "P")
    s.bx((0, 1.8, -0.2), (1.4, 0.3, 1.4), "C", rot=(0, 45, 0))      # the star
    s.el((0, 0, -3.2), (4.8, 4.8, 4.6), "C")                        # the photon blast
    s.el((0, 0, -3.2), (3.4, 3.4, 3.6), "A")
    return check(s)


def jarnbjorn():
    s = Shape()
    s.cz((0, 0, -3.4), 0.9, 10.0, "D")
    s.bx((0, 0, 1.6), (1.2, 1.2, 0.8), "A")
    s.bx((0, -2.2, -7.2), (0.7, 3.6, 3.2), "P")
    s.bx((0, -3.8, -7.2), (0.8, 1.0, 4.4), "C")
    s.bx((0, 1.4, -7.4), (0.7, 1.8, 1.6), "P")
    s.cone((0, 2.6, -7.4), 0.7, 1.4, "C", "+y")
    s.bx((0, 0, -7.2), (1.3, 1.3, 1.6), "A")
    return check(s)


def gungnir():
    s = Shape()
    s.cz((0, 0, -4.0), 0.7, 16.0, "A")
    s.bx((0, 0, 3.8), (1.0, 1.0, 0.8), "D")
    s.el((0, 0, -11.2), (1.6, 1.6, 1.6), "D")
    for sy in (-1, 1):
        s.bx((0, sy * 1.0, -12.0), (0.4, 0.5, 2.2), "C", rot=(sy * 20, 0, 0))
    s.bx((0, 0, -14.0), (0.35, 1.4, 3.4), "C")
    s.cone((0, 0, -16.4), 0.35, 1.6, "C", "-z", d2=1.4)
    return check(s)


def hofund():
    s = Shape()
    s.cz((0, 0, 0.6), 1.0, 2.8, "D")
    s.el((0, 0, 2.2), (1.2, 1.2, 1.0), "A")
    s.bx((0, 0, -1.0), (1.0, 4.4, 0.8), "A")
    s.cone((0, 2.6, -1.0), 0.8, 1.2, "A", "+y")
    s.cone((0, -2.6, -1.0), 0.8, 1.2, "A", "-y")
    s.bx((0, 0, -6.6), (0.5, 1.8, 10.4), "P")
    s.bx((0, 0, -6.6), (0.6, 0.5, 10.0), "C")
    s.cone((0, 0, -12.6), 0.5, 1.8, "C", "-z", d2=1.8)
    return check(s)


def twilight_sword():
    s = Shape()
    s.cz((0, 0, 0.8), 1.4, 4.4, "D")
    s.bx((0, 0, -1.8), (1.6, 6.4, 1.6), "D")
    for sy in (-1, 1):
        s.cone((0, sy * 3.8, -2.6), 1.2, 2.4, "A", "+y" if sy > 0 else "-y")
    s.bx((0, 0, -11.0), (0.9, 3.4, 17.0), "P")
    s.bx((0, 0, -11.0), (1.0, 1.0, 16.0), "C")
    for z in (-6.0, -10.0, -14.0):
        s.cone((0, 2.2, z), 1.0, 1.6, "A", "+y")
    s.cone((0, 0, -20.6), 0.9, 2.6, "P", "-z", d2=3.4)
    return check(s)


def widows_bite():
    s = Shape()
    s.cz((0, 0, 0.6), 3.0, 3.6, "D")
    for z in (-0.6, 0.6, 1.8):
        s.cz((0, 0, z), 3.3, 0.6, "A")
    for a in (0, 120, 240):
        s.bx((0, 0, -1.6), (0.4, 0.4, 1.2), "C", rot=(0, 0, a))
    s.bx((0, 1.8, 0.6), (1.2, 0.4, 1.2), "C", rot=(0, 45, 0))   # the hourglass
    s.el((0, 0, -2.8), (2.4, 2.4, 1.6), "C")
    return check(s)


def vibranium_arm():
    s = Shape()
    s.cz((0, 0, 3.4), 3.0, 4.4, "P")
    for z in (2.0, 3.2, 4.4):
        s.cz((0, 0, z), 3.2, 0.3, "D")
    s.bx((0, 1.6, 4.0), (1.0, 0.2, 1.0), "A", rot=(0, 45, 0))   # the star
    s.bx((0, 0, -0.2), (3.6, 3.0, 3.2), "P")
    for x in (-1.2, -0.4, 0.4, 1.2):
        s.bx((x, 0.4, -2.4), (0.7, 1.8, 1.8), "P")
    s.bx((2.0, -0.4, -1.0), (0.9, 0.9, 2.2), "P", rot=(0, -20, 0))
    return check(s)


def elektra_sai():
    s = Shape()
    s.cz((0, 0, 0.6), 0.8, 2.8, "D")
    s.el((0, 0, 2.2), (0.9, 0.9, 0.9), "A")
    for sy in (-1, 1):
        s.seg((0, 0, -0.9), (0, sy * 1.2, -1.4), 0.4, 0.4, "A", kind="Cylinder", over=0.2)
        s.seg((0, sy * 1.2, -1.4), (0, sy * 1.3, -2.8), 0.4, 0.4, "A", kind="Cylinder", over=0.2)
    s.cz((0, 0, -4.0), 0.5, 6.0, "C")
    s.cone((0, 0, -7.4), 0.5, 0.9, "C", "-z")
    return check(s)


def iron_fist():
    s = Shape()
    s.cz((0, 0, 2.6), 2.8, 2.4, "D")
    s.bx((0, 0, 0), (3.6, 3.0, 3.2), "P")
    for x in (-1.2, -0.4, 0.4, 1.2):
        s.bx((x, 0.4, -2.0), (0.7, 1.6, 1.4), "P")
    s.bx((0, 1.6, 0.2), (2.2, 0.3, 1.6), "A")                    # the dragon tattoo
    s.bx((0.8, 1.6, 1.0), (0.4, 0.3, 1.2), "A", rot=(0, 30, 0))
    s.el((0, 0, -1.2), (5.2, 4.6, 5.4), "C")                      # chi glow shell
    s.el((0, 0, -1.2), (4.8, 4.2, 5.0), "C", sub=True)
    return check(s)


def clobberin_fist():
    s = Shape()
    s.cz((0, 0, 2.8), 3.4, 2.6, "A")
    s.bx((0, 0, 0), (4.6, 4.0, 4.0), "P")
    for (x, y, z) in ((-1.4, 1.2, 1.0), (1.2, 1.4, 0.2), (0.0, -1.2, 1.2), (1.6, -0.6, -1.0), (-1.2, 0.6, -1.2)):
        s.bx((x, y, z), (1.6, 1.4, 1.6), "D", rot=(15, 25, 10))
    for x in (-1.5, -0.5, 0.5, 1.5):
        s.bx((x, 0.4, -2.6), (1.0, 2.0, 1.6), "P", rot=(0, 0, 8))
    return check(s)


def ronan_hammer():
    s = Shape()
    s.cz((0, 0, -4.0), 1.0, 12.0, "D")
    s.bx((0, 0, 2.2), (1.4, 1.4, 1.0), "A")
    s.cy((0, 0, -10.4), (3.4, 3.2, 3.4), "P", rot=(90, 0, 0))
    s.cy((0, 0, -10.4), (3.8, 0.6, 3.8), "A", rot=(90, 0, 0))
    s.cone((0, -2.8, -10.4), 2.4, 2.4, "P", "-y")
    s.cone((0, 2.6, -10.4), 1.6, 1.6, "A", "+y")
    s.cz((0, 0, -12.2), 1.4, 0.6, "C")
    return check(s)


def crescent_dart():
    s = Shape()
    c = -1.6
    s.cy((0, 0, c), (3.2, 0.4, 3.2), "C")
    s.cy((0.9, 0, c - 0.5), (2.8, 0.6, 2.8), "C", sub=True)
    s.bx((0, 0, c), (0.8, 0.5, 0.8), "A")
    return check(s)


def falcon_wings():
    s = Shape()
    s.bx((0, 0, 0.6), (2.6, 3.2, 1.2), "D")                 # backpack
    s.cz((0, -1.4, 1.4), 0.8, 1.0, "C")
    for i, sx in enumerate(SIDES):
        pid = s.part(9 + i, 0, (sx * 1.3, 1.0, 0.8), "WingLeft" if sx < 0 else "WingRight")
        s.bx((sx * 4.4, 1.4, 0.8), (6.4, 0.3, 2.0), "P", rot=(0, 0, sx * -8), part=pid)
        for n in range(4):
            s.bx((sx * (2.4 + n * 1.6), 0.2 - n * 0.2, 1.0), (1.2, 2.4 + n * 0.4, 0.25), "A", rot=(0, 0, sx * -6),
                 part=pid)
        s.bx((sx * 7.8, 1.8, 0.8), (1.2, 0.3, 1.6), "C", rot=(0, 0, sx * -8), part=pid)
    return check(s)


def infinity_stone():
    s = Shape()
    s.bx((0, 0, -1.2), (1.6, 2.0, 1.2), "A", rot=(0, 0, 45))
    s.bx((0, 0, -1.2), (1.1, 1.4, 1.5), "C", rot=(0, 0, 45))
    s.el((0, 0, -1.2), (2.6, 2.6, 2.6), "A")
    s.el((0, 0, -1.2), (2.4, 2.4, 2.4), "A", sub=True)
    s.cy((0, 0, -1.2), (3.4, 0.2, 3.4), "C", rot=(90, 0, 0))
    s.cy((0, 0, -1.2), (3.0, 0.3, 3.0), "C", rot=(90, 0, 0), sub=True)
    return check(s)


def tesseract():
    s = Shape()
    s.bx((0, 0, -1.8), (3.0, 3.0, 3.0), "A")
    s.bx((0, 0, -1.8), (2.6, 2.6, 3.2), "A", sub=True)
    s.bx((0, 0, -1.8), (2.6, 3.2, 2.6), "A", sub=True)
    s.bx((0, 0, -1.8), (3.2, 2.6, 2.6), "A", sub=True)
    s.bx((0, 0, -1.8), (1.6, 1.6, 1.6), "C", rot=(35, 45, 0))
    s.el((0, 0, -1.8), (1.0, 1.0, 1.0), "C")
    return check(s)


def storm_lightning():
    """A lightning strike (Pillar): zigzag from the sky, ~6 m tall."""
    s = Shape()
    pts = [(0, 24, 0), (2.4, 19, 0.4), (-1.2, 15, -0.3), (2.0, 10, 0.2), (-1.6, 5.5, 0), (0.4, 0.5, 0)]
    for a, b in zip(pts, pts[1:]):
        s.seg(a, b, 1.1, 1.1, "C", over=0.8)
        s.seg(a, b, 1.8, 0.5, "A", over=0.4)
    s.seg((2.0, 10, 0.2), (4.6, 6.8, 0.6), 0.6, 0.6, "C", over=0.3)
    s.seg((2.4, 19, 0.4), (5.2, 17.4, -0.2), 0.6, 0.6, "C", over=0.3)
    s.cy((0.4, 0.3, 0), (5.0, 0.6, 5.0), "A")
    s.cy((0.4, 0.3, 0), (3.0, 0.8, 3.0), "C")
    return check(s)


def magneto_helmet():
    """Dome: the helmet as a psychic-proof bubble."""
    s = Shape()
    s.el((0, 6, 0), (16, 14, 17), "A")
    s.el((0, 6, 0), (14.6, 12.6, 15.6), "A", sub=True)
    s.bx((0, 0, 0), (18, 2.0, 18), "P", sub=True)
    s.bx((0, 11.5, -5.5), (2.2, 5.0, 6.0), "A")                 # front crest
    for sx in SIDES:
        s.bx((sx * 8.2, 4.0, -1.0), (1.4, 8.0, 6.0), "P")       # side flares
    s.bx((0, 4.0, -8.2), (7.0, 6.0, 2.4), "P", sub=True)        # the face opening
    s.bx((0, 1.2, 0), (16.4, 1.2, 16.4), "D")
    return check(s)


def web_net():
    """Cage: an impact-web ball."""
    s = Shape()
    c = (0, 4, 0)
    for rx, ry in ((0, 0), (90, 0), (0, 90), (45, 0), (0, 45), (-45, 0)):
        s.cy(c, (8.0, 0.4, 8.0), "C", rot=(rx, ry, 0))
        s.cy(c, (7.4, 0.6, 7.4), "C", rot=(rx, ry, 0), sub=True)
    s.cy((0, 0.2, 0), (5.0, 0.4, 5.0), "A")
    s.el(c, (1.4, 1.4, 1.4), "A")
    return check(s)


def pumpkin_bomb():
    s = Shape()
    s.el((0, 1.4, 0), (2.8, 2.4, 2.8), "P")
    for a in (0, 60, 120):
        s.el((0, 1.4, 0), (3.0, 2.3, 1.2), "P", rot=(0, a, 0))
    for sx in SIDES:
        s.bx((sx * 0.55, 1.8, -1.35), (0.5, 0.5, 0.4), "C", rot=(0, 0, 45))
    s.bx((0, 0.9, -1.35), (1.4, 0.35, 0.4), "C")
    s.cy((0, 2.9, 0), (0.4, 0.8, 0.4), "D")
    return check(s)


# ------------------------------------------------------------------ vehicles

def goblin_glider():
    s = Shape()
    s.bx((0, 1.0, 0), (2.4, 0.8, 4.0), "P")
    s.cone((0, 1.0, -2.8), 1.6, 1.8, "P", "-z", d2=0.8)
    for sx in SIDES:
        s.bx((sx * 2.8, 1.1, 0.4), (3.6, 0.4, 2.6), "P", rot=(0, sx * -12, sx * 8))
        s.bx((sx * 4.6, 1.5, 1.0), (1.6, 0.4, 1.6), "D", rot=(0, 45, sx * 8))
        s.cone((sx * 5.6, 1.9, 1.4), 0.6, 1.2, "D", "+y")
        s.bx((sx * 0.9, 0.6, -2.0), (0.4, 0.4, 0.6), "C")
    s.part(8, 0, (0, 1.0, 2.0), "Exhaust")
    s.cone((0, 1.0, 3.0), 1.2, 1.6, "C", "+z", part=8)
    s.bx((0, 1.5, 0.3), (1.4, 0.2, 1.6), "D")
    s.seat(0, (0, 1.6, 0.3))
    return check(s)


def spider_mobile():
    s = Shape()
    s.bx((0, 2.4, 0.6), (7.0, 2.0, 12.0), "P")
    s.bx((0, 2.8, -6.2), (6.0, 1.2, 3.0), "P", rot=(-10, 0, 0))
    s.bx((0, 4.2, 1.8), (4.4, 1.6, 4.0), "A")                  # the cab
    s.bx((0, 4.2, -0.3), (4.0, 1.2, 0.4), "C", rot=(-30, 0, 0))
    # the spider on the hood
    s.el((0, 3.5, -3.6), (1.4, 0.4, 2.0), "D")
    for sx in SIDES:
        for dz in (-0.9, 0.9):
            s.bx((sx * 1.2, 3.5, -3.6 + dz), (1.8, 0.25, 0.25), "D", rot=(0, sx * dz * 30, 0))
        s.bx((sx * 2.8, 2.4, -6.8), (1.0, 0.6, 0.4), "C")
    s.cz((0, 3.2, -7.3), 1.4, 0.6, "C")                         # web-shooter nozzle
    wheel(s, 1, (-3.9, 1.8, 4.4), 3.6, 1.8)
    wheel(s, 2, (3.9, 1.8, 4.4), 3.6, 1.8)
    wheel(s, 3, (-3.9, 1.6, -4.0), 3.2, 1.6)
    wheel(s, 4, (3.9, 1.6, -4.0), 3.2, 1.6)
    s.seat(0, (0, 3.4, 1.8))
    return check(s)


def blackbird():
    s = Shape()
    s.el((0, 2.0, 0), (3.0, 2.2, 34.0), "P")                    # fuselage
    s.cone((0, 2.0, -19.0), 2.2, 5.0, "P", "-z")
    s.el((0, 3.0, -9.0), (2.0, 1.2, 5.0), "C")                  # cockpit
    for sx in SIDES:
        s.bx((sx * 3.2, 1.8, -8.0), (4.0, 0.3, 14.0), "P", rot=(0, sx * -10, 0))   # the chines
        s.bx((sx * 6.0, 1.8, 5.0), (10.0, 0.35, 10.0), "P", rot=(0, sx * 30, 0))   # delta wings
        s.cz((sx * 5.0, 1.8, 4.0), 2.4, 16.0, "D")                                   # engine nacelles
        s.cz((sx * 5.0, 1.8, 12.4), 1.8, 0.8, "C")
        s.bx((sx * 5.0, 4.2, 9.0), (0.3, 4.2, 3.4), "A", rot=(0, 0, sx * -15))     # canted tails
    s.bx((0, 2.8, 2.0), (1.0, 0.4, 1.0), "A", rot=(0, 45, 0))  # the X
    s.part(8, 0, (0, 1.8, 13.0), "Exhaust")
    for sx in SIDES:
        s.cone((sx * 5.0, 1.8, 13.8), 1.6, 1.8, "C", "+z", part=8)
    s.seat(0, (0, 2.8, -9.0))
    return check(s)
