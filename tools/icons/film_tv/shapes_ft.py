"""Hand-authored ShapeSpecs for the film/TV pack. Units: construct voxels (0.25 m)."""
from ftlib import Shape

SIDES = (-1, 1)


def check(s):
    ids = {0} | {p[0] for p in s.parts}
    for pr in s.prims:
        assert pr[5] in ids, ("undefined part", pr)
    return s


# ------------------------------------------------------------------ rig helpers

def humanoid(s, leg_h, torso_h, head_h, tw, td, lw, aw, arm_l, tl="P", tt="P", ta="P", th="P", hw=None,
             hd=None, head=True):
    hip = leg_h
    lx = tw / 2 - lw / 2
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * lx, hip, 0), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * lx, hip / 2, 0), (lw, hip + 0.4, lw), tl, part=pid)
    s.bx((0, hip + torso_h / 2, 0), (tw, torso_h, td), tt)
    shy = hip + torso_h - aw / 2
    ax = tw / 2 + aw / 2
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * ax, shy, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.bx((sx * ax, shy - arm_l / 2 + aw / 2, 0), (aw, arm_l, aw), ta, part=pid)
    top = hip + torso_h
    if head:
        s.part(5, 0, (0, top, 0), "Head")
        s.bx((0, top + head_h / 2, 0), (hw or head_h, head_h, hd or head_h), th, part=5)
    return dict(hip=hip, top=top, shy=shy, ax=ax, lx=lx, hc=top + head_h / 2)


def quad(s, leg_h, leg_w, lx, lz, t="P", foot=None):
    """Four legs as parts 1..4 (FL, FR, BL, BR). Front is -Z."""
    roles = [("LegFrontLeft", -1, -1), ("LegFrontRight", 1, -1), ("LegBackLeft", -1, 1), ("LegBackRight", 1, 1)]
    for i, (role, sx, sz) in enumerate(roles):
        pid = s.part(1 + i, 0, (sx * lx, leg_h, sz * lz), role)
        s.bx((sx * lx, leg_h / 2, sz * lz), (leg_w, leg_h + 0.4, leg_w), t, part=pid)
        if foot:
            s.bx((sx * lx, foot[1] / 2, sz * lz - 0.3), (foot[0], foot[1], foot[2]), "D", part=pid)


def tail_chain(s, first_id, parent, pts, widths, t="P", tip=None):
    """Chain of Tail parts through points pts[0..n]; widths per segment (ellipsoids)."""
    prev = parent
    for i in range(len(pts) - 1):
        pid = s.part(first_id + i, prev, pts[i], "Tail")
        s.seg(pts[i], pts[i + 1], widths[i], widths[i] * 0.9, t, part=pid, kind="Ellipsoid", over=widths[i] * 0.6)
        prev = pid
    if tip:
        tip(s, prev)
    return prev


# ------------------------------------------------------------------ held items

def mjolnir():
    s = Shape()
    s.cz((0, 0, -2), 1.2, 6, "D")
    s.bx((0, 0, 0), (1.5, 1.5, 0.6), "A")
    s.bx((0, 0, -1.6), (1.5, 1.5, 0.6), "A")
    s.bx((0, 0, 1.4), (1.7, 1.7, 0.8), "A")
    s.seg((0, 0, 1.8), (0, -1.4, 2.8), 0.6, 0.6, "D")
    s.bx((0, 0, -6.6), (6.2, 3.6, 3.6), "P")
    s.bx((0, 1.9, -6.6), (5.6, 0.5, 3.0), "D")
    s.bx((0, -1.9, -6.6), (5.6, 0.5, 3.0), "D")
    s.cx((-3.2, 0, -6.6), 2.4, 0.5, "C")
    s.cx((3.2, 0, -6.6), 2.4, 0.5, "C")
    return check(s)


def stormbreaker():
    s = Shape()
    s.cz((0, 0, -3.0), 1.0, 9.5, "A")
    s.bx((0, 0, 0.4), (1.3, 1.3, 2.4), "D")
    s.bx((0, 0, 2.0), (1.5, 1.5, 0.8), "D")
    s.bx((0, 0, -6.0), (1.7, 1.7, 2.6), "D")
    s.bx((0, -2.4, -6.0), (1.0, 3.6, 3.4), "P")
    s.bx((0, -2.8, -6.0), (1.0, 3.2, 3.2), "P", rot=(45, 0, 0))
    s.bx((0, -4.6, -6.0), (1.2, 1.0, 4.6), "C")
    s.bx((0, 1.5, -6.0), (1.7, 2.0, 2.0), "P")
    s.cone((0, 0, -8.4), 1.1, 2.4, "P", "-z")
    s.bx((0, 0, -6.0), (1.9, 0.8, 0.8), "C")
    return check(s)


def cap_shield():
    s = Shape()
    s.bx((0, 0, 0), (1.0, 3.0, 1.0), "D")
    s.cz((0, 0, -1.5), 9.0, 1.0, "P")
    s.cz((0, 0, -2.0), 7.0, 1.0, "A")
    s.cz((0, 0, -2.5), 5.0, 1.0, "P")
    s.cz((0, 0, -3.0), 3.2, 1.0, "D")
    # star: two crossed diamonds on the boss
    s.bx((0, 0, -3.4), (1.4, 1.4, 0.6), "C", rot=(0, 0, 45))
    s.bx((0, 0.4, -3.4), (0.6, 1.6, 0.6), "C")
    return check(s)


def lightsaber():
    s = Shape()
    s.cz((0, 0, 0.2), 1.2, 3.4, "D")
    s.bx((0, 0, 0.8), (1.4, 1.4, 0.4), "A")
    s.bx((0, 0, 0.0), (1.4, 1.4, 0.4), "A")
    s.bx((0, 0, 2.0), (1.4, 1.4, 0.5), "A")
    s.bx((0, 0, -1.7), (1.7, 1.7, 0.9), "A")
    s.bx((0, 0.8, -0.8), (0.5, 0.5, 0.7), "C")
    s.cz((0, 0, -6.4), 1.4, 8.6, "P")
    s.cz((0, 0, -6.2), 0.7, 8.2, "C")
    s.cone((0, 0, -11.0), 1.3, 1.2, "C", "-z")
    return check(s)


def darksaber():
    s = Shape()
    s.cz((0, 0, 0.4), 1.2, 3.2, "D")
    s.bx((0, 0, 1.0), (1.4, 1.4, 0.4), "A")
    s.bx((0, 0, -1.4), (1.6, 1.6, 0.8), "A")
    s.bx((0, 0.2, -5.6), (0.6, 1.8, 7.6), "D")
    s.bx((0, -0.9, -5.6), (0.7, 0.5, 7.6), "C")
    s.bx((0, 0.2, -9.6), (0.6, 1.9, 1.6), "D", rot=(40, 0, 0))
    s.bx((0, -0.6, -9.9), (0.7, 0.5, 1.8), "C", rot=(40, 0, 0))
    return check(s)


def double_saber():
    s = Shape()
    s.cz((0, 0, 0), 1.2, 5.4, "D")
    s.bx((0, 0, 0), (1.5, 1.5, 0.6), "A")
    for sz in (-1, 1):
        s.bx((0, 0, sz * 2.8), (1.6, 1.6, 0.6), "A")
        s.cz((0, 0, sz * 7.2), 1.4, 8.4, "P")
        s.cz((0, 0, sz * 7.2), 0.7, 8.0, "C")
        s.cone((0, 0, sz * 11.8), 1.3, 1.2, "C", "-z" if sz < 0 else "+z")
    return check(s)


def sting():
    s = Shape()
    s.cz((0, 0, 0.4), 1.0, 2.6, "D")
    s.bx((0, 0, 2.0), (1.2, 1.6, 0.8), "A")
    s.seg((0, 0, -1.0), (0, 1.8, -1.9), 0.9, 0.9, "A")
    s.seg((0, 0, -1.0), (0, -1.8, -1.9), 0.9, 0.9, "A")
    s.el((0, 0, -5.0), (1.0, 2.6, 7.6), "P")
    s.bx((0, -1.0, -5.0), (1.0, 0.5, 6.0), "C")
    s.bx((0, 1.0, -5.0), (1.0, 0.5, 6.0), "C")
    s.cone((0, 0, -9.0), 0.7, 1.8, "C", "-z", d2=1.2)
    return check(s)


def elder_wand():
    s = Shape()
    s.cz((0, 0, -2.6), 0.9, 9.0, "P")
    s.bx((0, 0, 1.1), (1.3, 1.3, 2.2), "D")
    for z in (0.3, -1.6, -3.6, -5.6):
        s.el((0, 0, z), (1.7, 1.7, 0.9), "A")
    s.cone((0, 0, -7.6), 0.9, 1.4, "P", "-z")
    s.bx((0, 0, -8.4), (0.7, 0.7, 0.7), "C")
    return check(s)


def neutrona_wand():
    s = Shape()
    s.bx((0, -0.9, 0.4), (1.2, 3.0, 1.8), "D", rot=(-14, 0, 0))
    s.bx((0, 0.9, -1.4), (1.8, 2.0, 5.4), "D")
    s.bx((0, -0.9, -3.4), (1.0, 2.4, 1.0), "D")
    s.bx((0, 2.2, -1.2), (0.7, 0.7, 2.4), "A")
    s.cz((0, 0.9, -5.2), 1.3, 3.2, "P")
    s.bx((0, 0.9, -7.0), (1.5, 1.5, 0.6), "C")
    s.bx((1.0, 0.9, -0.2), (0.3, 0.6, 0.6), "C")
    s.bx((1.0, 0.9, -1.2), (0.3, 0.6, 0.6), "A")
    s.seg((0, -0.4, 1.6), (0, -2.2, 3.4), 0.9, 0.9, "A", kind="Cylinder")
    return check(s)


def portal_gun():
    s = Shape()
    s.bx((0, -0.9, 0.8), (1.4, 2.8, 1.6), "D", rot=(-14, 0, 0))
    s.bx((0, 1.2, 0.0), (2.4, 2.4, 4.2), "A")
    s.cz((0, 2.8, 0.2), 1.5, 3.4, "P")
    s.el((0, 1.2, -2.6), (2.6, 2.6, 2.0), "C")
    for x, y in ((-0.9, 2.0), (0.9, 2.0), (0, 0.2)):
        s.cone((x, y, -3.9), 0.6, 1.8, "D", "-z")
    s.bx((0, 1.2, 2.3), (1.8, 1.8, 0.6), "D")
    return check(s)


def web_shooter():
    s = Shape()
    s.cz((0, 0, 0.6), 3.0, 3.6, "D")
    for z in (-0.4, 1.6):
        s.cz((0, 0, z), 3.4, 0.8, "A")
    s.bx((0, 2.0, 0.0), (2.0, 1.2, 3.0), "P")
    s.bx((0, 2.0, -2.0), (1.0, 1.0, 1.2), "C")
    s.bx((0, 2.8, 0.6), (1.0, 0.6, 1.0), "A")
    s.cz((0, 2.0, -3.4), 1.0, 1.6, "C")
    return check(s)


def wolverine_claws():
    s = Shape()
    s.cz((0, 0, 2.6), 4.0, 2.6, "D")
    s.bx((0, 0, 0), (5.0, 3.0, 3.0), "P")
    s.bx((0, 0.6, -1.8), (5.2, 1.4, 1.0), "A")
    for x in (-2, 0, 2):
        s.bx((x, 0.6, -5.4), (1.0, 1.0, 6.4), "C")
        s.cone((x, 0.6, -9.2), 1.0, 1.6, "C", "-z")
    return check(s)


def infinity_gauntlet():
    s = Shape()
    s.cz((0, 0, 3.2), 5.0, 2.6, "A")
    s.cz((0, 0, 4.6), 5.4, 0.8, "D")
    s.bx((0, 0, 0), (7.0, 3.0, 4.0), "A")
    s.el((0, 1.8, 0.4), (2.0, 1.0, 2.0), "C")
    for x in (-3, -1, 1, 3):
        s.bx((x, 0.2, -3.6), (1.0, 2.0, 3.4), "A")
        s.bx((x, 1.4, -2.2), (1.0, 0.6, 1.0), "C")
    s.bx((4.2, -0.4, -1.4), (1.4, 1.4, 3.0), "A", rot=(0, -20, 0))
    s.bx((4.2, 0.6, -1.4), (1.0, 0.6, 1.0), "C")
    return check(s)


def trident():
    s = Shape()
    s.cz((0, 0, -3.0), 0.9, 12.4, "A")
    s.bx((0, 0, 3.6), (1.1, 1.1, 0.8), "D")
    s.bx((0, 0, -9.6), (1.1, 5.0, 1.1), "P")
    s.bx((0, 0, -9.6), (1.3, 1.3, 1.3), "C")
    s.cz((0, 0, -11.8), 0.9, 4.0, "P")
    s.cone((0, 0, -14.6), 1.1, 1.8, "C", "-z")
    for sy in (-1, 1):
        s.bx((0, sy * 2.2, -11.2), (0.9, 0.9, 3.2), "P")
        s.cone((0, sy * 2.2, -13.4), 1.0, 1.6, "C", "-z")
        s.bx((0, sy * 2.8, -10.4), (0.7, 0.9, 0.7), "P")
    return check(s)


def batarang():
    s = Shape()
    c = -3.0
    s.bx((0, 0, c), (2.2, 0.6, 2.2), "P")
    for sx in (-1, 1):
        s.bx((sx * 2.4, 0, c + 0.2), (3.2, 0.6, 1.8), "P", rot=(0, sx * 12, 0))
        s.bx((sx * 4.2, 0, c - 0.5), (1.4, 0.6, 1.4), "P", rot=(0, 45, 0))
        s.el((sx * 1.9, 0, c + 1.3), (1.6, 1.0, 1.4), "P", sub=True)
        s.el((sx * 3.3, 0, c + 1.0), (1.2, 1.0, 1.2), "P", sub=True)
        s.bx((sx * 0.6, 0, c - 1.5), (0.5, 0.6, 1.0), "P")
    s.bx((0, 0.3, c), (0.8, 0.4, 0.8), "C")
    return check(s)


def acme_anvil():
    s = Shape()
    s.bx((0, 0.6, 0), (4.6, 1.2, 3.4), "D")
    s.bx((0, 2.0, 0), (2.4, 1.8, 2.0), "P")
    s.bx((0, 3.8, 0), (5.4, 1.8, 2.8), "P")
    s.cone((-4.2, 3.9, 0), 1.6, 3.2, "P", "-x", d2=1.4)
    s.bx((3.4, 3.8, 0), (1.4, 1.8, 2.4), "P")
    s.bx((0, 4.8, 0), (5.0, 0.3, 2.4), "A")
    s.bx((0, 2.0, -1.05), (1.8, 0.8, 0.2), "C")
    return check(s)


def maui_hook():
    s = Shape()
    s.cz((0, 0, -2.6), 1.3, 10.0, "P")
    s.bx((0, 0, 0.6), (1.5, 1.5, 2.4), "D")
    for z in (-1.8, -3.6, -5.4):
        s.bx((0, 0, z), (1.5, 1.5, 0.5), "A")
    s.seg((0, 0, -7.4), (0, 2.2, -9.0), 1.3)
    s.seg((0, 2.2, -9.0), (0, 4.4, -8.8), 1.3)
    s.seg((0, 4.4, -8.8), (0, 5.0, -7.0), 1.3)
    s.cone((0, 4.8, -5.8), 1.2, 1.8, "C", "+z")
    return check(s)


def power_sword():
    s = Shape()
    s.cz((0, 0, 0.3), 1.2, 3.0, "D")
    s.el((0, 0, 2.2), (1.9, 1.9, 1.4), "A")
    s.bx((0, 0, -1.7), (1.4, 5.0, 1.2), "A")
    for sy in (-1, 1):
        s.el((0, sy * 2.8, -2.0), (1.4, 1.6, 1.6), "A")
    s.bx((0, 0, -1.8), (1.6, 1.0, 1.0), "C")
    s.bx((0, 0, -7.4), (0.8, 2.4, 10.4), "P")
    s.bx((0, 0, -7.2), (1.0, 0.5, 9.0), "C")
    s.cone((0, 0, -13.4), 0.8, 2.0, "P", "-z", d2=2.4)
    return check(s)


def harley_mallet():
    s = Shape()
    s.cz((0, 0, -3.0), 1.1, 9.0, "D")
    s.bx((0, 0, 1.4), (1.4, 1.4, 0.8), "A")
    s.cx((0, 0, -8.6), 4.4, 7.0, "P")
    for x in (-1.8, 1.8):
        s.cx((x, 0, -8.6), 4.7, 1.0, "A")
    for x in (-3.6, 3.6):
        s.cx((x, 0, -8.6), 4.0, 0.5, "D")
    s.bx((0, 2.3, -8.6), (1.2, 0.6, 1.2), "C", rot=(0, 45, 0))
    return check(s)


def lantern_fist():
    s = Shape()
    s.cz((0, 0, 3.0), 4.4, 2.2, "D")
    s.bx((0, 0, -0.4), (5.2, 4.6, 4.6), "P")
    for x in (-1.9, -0.63, 0.63, 1.9):
        s.bx((x, 0.5, -3.3), (1.2, 3.2, 1.8), "P")
        s.el((x, 1.7, -2.9), (1.4, 1.4, 1.4), "A")
    s.bx((0.4, -1.5, -3.2), (3.4, 1.3, 1.3), "P")
    s.bx((-0.63, 0.5, -4.35), (1.5, 1.5, 0.5), "C")
    s.cy((0, 2.4, -0.4), (2.4, 0.4, 2.4), "C")
    s.cy((0, 2.5, -0.4), (1.4, 0.6, 1.4), "P", sub=True)
    return check(s)


def boxing_glove():
    s = Shape()
    s.cz((0, 0, 4.0), 1.6, 2.4, "D")
    s.cz((0, 0, 1.6), 3.6, 2.8, "A")
    s.el((0, 0, -2.0), (4.8, 4.4, 5.4), "P")
    s.el((2.2, -0.5, -1.6), (1.8, 2.2, 3.2), "P")
    s.bx((0, 1.9, 1.4), (0.6, 0.6, 2.6), "C")
    return check(s)


def sonic_screwdriver():
    s = Shape()
    s.cz((0, 0, 0.4), 1.3, 4.4, "D")
    for z in (1.0, 0.0, -0.8):
        s.bx((0, 0, z), (1.5, 1.5, 0.3), "A")
    s.bx((0, 0, -1.9), (1.8, 1.8, 0.5), "A")
    s.cz((0, 0, -2.8), 0.8, 1.6, "P")
    for sy in (-1, 1):
        s.bx((0, sy * 0.9, -3.0), (0.4, 0.4, 1.6), "A")
    s.bx((0, 0, -3.8), (0.9, 0.9, 0.7), "C")
    return check(s)


def kong_axe():
    s = Shape()
    s.cz((0, 0, -3.2), 1.4, 11.0, "D")
    s.bx((0, 0, 2.6), (1.8, 1.8, 1.0), "A")
    s.bx((0, 0, -7.0), (2.0, 2.0, 3.2), "D")
    s.el((0, -3.6, -7.0), (1.2, 6.6, 6.4), "P")
    for z, h in ((-9.4, 2.4), (-7.0, 3.0), (-4.6, 2.4)):
        s.cone((0, -6.6 + (0.8 if z != -7.0 else 0), z), 1.2, h, "C", "-y", d2=1.8)
    s.el((0, -2.6, -7.0), (1.4, 2.0, 2.0), "A")
    return check(s)


# ------------------------------------------------------------------ vehicles

def wheel(s, pid, c, d, w, parent=0):
    s.part(pid, parent, c, "Other")
    s.cx(c, d, w, "D", part=pid)
    s.cx(c, d * 0.45, w + 0.2, "A", part=pid)


def batmobile():
    s = Shape()
    s.bx((0, 2.4, 0.6), (8.4, 2.4, 15.0), "D")
    s.bx((0, 2.9, -6.6), (7.2, 1.2, 6.0), "P", rot=(-10, 0, 0))
    s.bx((0, 4.2, 1.4), (4.4, 1.8, 5.2), "P")
    s.bx((0, 4.6, -1.0), (4.0, 1.2, 1.4), "D", rot=(-35, 0, 0))
    for sx in (-1, 1):
        s.bx((sx * 3.2, 4.4, 5.6), (0.6, 2.6, 3.8), "P", rot=(0, 0, sx * -12))
        s.bx((sx * 2.7, 2.3, -9.0), (1.1, 0.6, 0.5), "C")
    wheel(s, 1, (-4.8, 2.3, 5.4), 4.6, 2.6)
    wheel(s, 2, (4.8, 2.3, 5.4), 4.6, 2.6)
    wheel(s, 3, (-4.4, 1.6, -5.6), 3.2, 1.8)
    wheel(s, 4, (4.4, 1.6, -5.6), 3.2, 1.8)
    s.cz((0, 2.8, 8.4), 2.2, 1.2, "C")
    s.seat(0, (0, 3.4, 1.0))
    return check(s)


def x_wing():
    s = Shape()
    s.bx((0, 3.0, 0.4), (2.4, 2.4, 14.0), "P")
    s.cone((0, 3.0, -9.6), 2.3, 5.4, "P", "-z")
    s.bx((0, 4.4, -2.2), (1.8, 1.0, 3.6), "D")
    s.el((0, 4.4, 1.8), (1.4, 1.2, 1.4), "A")
    for sx in (-1, 1):
        for sy in (-1, 1):
            y = 3.0 + sy * 1.5
            s.cz((sx * 2.2, y, 4.4), 1.6, 5.0, "D")
            s.cz((sx * 2.2, y, 7.0), 1.1, 0.4, "C")
            ang = 12 * sx * (1 if sy > 0 else -1)
            s.bx((sx * 5.6, y, 4.0), (8.6, 0.5, 3.6), "P", rot=(0, 0, ang))
            s.cz((sx * 9.8, y + sy * 0.95, 0.0), 0.6, 8.6, "A")
            s.bx((sx * 5.6, y + sy * 0.1, 2.6), (7.0, 0.6, 0.6), "A", rot=(0, 0, ang))
    s.seat(0, (0, 4.2, -2.2))
    return check(s)


def tie_fighter():
    s = Shape()
    s.el((0, 5.0, 0), (5.0, 5.0, 5.0), "D")
    s.cz((0, 5.0, -2.3), 2.6, 0.6, "C")
    for sx in (-1, 1):
        s.bx((sx * 3.3, 5.0, 0), (2.4, 1.3, 1.3), "P")
        for a in (0, 60, -60):
            s.bx((sx * 5.0, 5.0, 0), (0.6, 5.0, 8.66), "P", rot=(a, 0, 0))
        s.bx((sx * 5.3, 5.0, 0), (0.3, 10.0, 0.8), "D")
        s.bx((sx * 5.3, 5.0, 0), (0.3, 0.8, 8.66), "D")
    s.seat(0, (0, 5.0, 0))
    return check(s)


def millennium_falcon():
    s = Shape()
    s.cy((0, 2.0, 0), (20, 2.4, 20), "P")
    s.cy((0, 3.4, 0.6), (12, 1.0, 12), "A")
    for sx in (-1, 1):
        s.bx((sx * 3.2, 2.0, -12.0), (3.6, 2.0, 8.0), "P")
    s.bx((0, 2.0, -13.0), (2.8, 3.2, 8.0), "P", sub=True)
    s.bx((8.6, 2.4, -3.4), (3.2, 1.6, 3.2), "D")
    s.cz((10.6, 2.4, -5.4), 2.4, 6.0, "D")
    s.bx((10.6, 2.8, -8.5), (1.8, 1.2, 0.4), "C")
    s.cy((-4.0, 4.2, -1.6), (3.2, 0.5, 3.2), "D", rot=(-25, 0, 0))
    s.cy((0, 4.2, 0.6), (2.4, 1.0, 2.4), "D")
    s.bx((0, 2.0, 9.9), (14.0, 1.2, 0.6), "C")
    s.seat(0, (10.6, 2.4, -6.0))
    return check(s)


def delorean():
    s = Shape()
    s.bx((0, 2.2, 0), (7.0, 2.2, 17.0), "A")
    s.bx((0, 3.5, -5.2), (6.8, 0.6, 6.6), "A", rot=(-6, 0, 0))
    s.bx((0, 4.2, 1.4), (6.0, 2.2, 6.0), "D")
    s.bx((0, 5.4, 1.4), (5.6, 0.4, 5.0), "A")
    for sx in (-1, 1):
        s.bx((sx * 4.6, 6.4, 1.4), (3.6, 0.4, 5.0), "A", rot=(0, 0, sx * 30))
    wheel(s, 1, (-3.6, 1.4, -5.6), 2.8, 1.4)
    wheel(s, 2, (3.6, 1.4, -5.6), 2.8, 1.4)
    wheel(s, 3, (-3.6, 1.4, 5.6), 2.8, 1.4)
    wheel(s, 4, (3.6, 1.4, 5.6), 2.8, 1.4)
    s.bx((0, 3.4, 8.6), (5.6, 0.8, 0.4), "C")
    s.cy((0, 4.2, 7.4), (1.6, 1.8, 1.6), "P")
    for sx in (-1, 1):
        s.bx((sx * 2.4, 2.6, -8.6), (1.4, 0.6, 0.3), "C")
    s.seat(0, (-1.2, 3.8, 1.4))
    return check(s)


def tardis():
    s = Shape()
    s.bx((0, 0.4, 0), (7.0, 0.8, 7.0), "D")
    s.bx((0, 5.4, 0), (6.0, 9.2, 6.0), "P")
    for sx in (-1, 1):
        for sz in (-1, 1):
            s.bx((sx * 3.1, 5.4, sz * 3.1), (0.8, 9.4, 0.8), "D")
    s.bx((0, 4.4, 0), (6.2, 7.0, 0.4), "D")
    s.bx((0, 4.4, 0), (0.4, 7.0, 6.2), "D")
    s.bx((0, 3.2, 0), (6.2, 0.4, 6.2), "D")
    s.bx((0, 5.8, 0), (6.2, 0.4, 6.2), "D")
    s.bx((0, 8.0, 0), (6.2, 1.4, 4.4), "A")
    s.bx((0, 8.0, 0), (4.4, 1.4, 6.2), "A")
    s.bx((0, 9.7, 0), (6.8, 1.1, 6.8), "D")
    s.bx((0, 9.7, 0), (7.0, 0.5, 5.0), "C")
    s.bx((0, 9.7, 0), (5.0, 0.5, 7.0), "C")
    s.bx((0, 10.7, 0), (5.8, 1.0, 5.8), "P")
    s.bx((0, 11.4, 0), (4.4, 0.6, 4.4), "P")
    s.cy((0, 12.4, 0), (1.1, 1.6, 1.1), "C")
    return check(s)


def broom():
    s = Shape()
    s.cz((0, 2.4, -2.2), 0.9, 10.4, "A")
    s.cone((0, 2.4, 6.0), 4.4, 6.0, "P", "-z")
    s.bx((0, 2.4, 3.2), (1.6, 1.6, 0.8), "D")
    s.bx((0, 1.2, 1.8), (2.4, 0.4, 0.4), "D")
    s.bx((0, 2.4, -7.6), (0.8, 0.8, 0.6), "C")
    s.seat(0, (0, 3.0, 0.8))
    return check(s)


def surfboard():
    s = Shape()
    s.el((0, 1.0, 0), (5.2, 0.8, 16.0), "A")
    s.el((0, 1.3, 0), (1.0, 0.4, 14.0), "C")
    s.el((0, 0.9, 0), (4.2, 0.9, 12.0), "P")
    s.bx((0, 0.4, 6.0), (0.4, 1.0, 1.6), "D")
    s.bx((0, 1.1, 7.4), (1.8, 0.4, 0.6), "C")
    s.seat(0, (0, 1.6, 1.0))
    return check(s)


def at_at():
    s = Shape()
    s.bx((0, 34, 0), (12, 10, 24), "P")
    s.bx((0, 39.5, 0), (10, 1.4, 20), "A")
    s.bx((0, 30, 0), (10, 2, 20), "D")
    s.part(5, 0, (0, 33, -12), "Head")
    s.cz((0, 33, -14), 4.4, 4.5, "D", part=5)
    s.bx((0, 32, -19.5), (6.4, 6, 8), "P", part=5)
    for sx in (-1, 1):
        s.cz((sx * 2.2, 29.2, -22.4), 1.0, 4.4, "A", part=5)
    s.bx((0, 33.4, -23.6), (4.4, 1.0, 0.4), "C", part=5)
    roles = [("LegFrontLeft", -1, -1), ("LegFrontRight", 1, -1), ("LegBackLeft", -1, 1), ("LegBackRight", 1, 1)]
    for i, (role, sx, sz) in enumerate(roles):
        pid = s.part(1 + i, 0, (sx * 6.6, 30, sz * 8), role)
        x, z = sx * 7.0, sz * 8
        s.bx((x, 24, z), (2.6, 12, 3.2), "P", part=pid)
        s.el((x, 17.6, z), (3.4, 3.4, 3.4), "D", part=pid)
        s.bx((x, 9.8, z), (2.4, 14, 2.4), "P", part=pid)
        s.cy((x, 1.3, z), (5.0, 2.6, 5.0), "D", part=pid)
    s.seat(5, (0, 33, -20))
    return check(s)


def death_star():
    s = Shape()
    s.el((0, 32, 0), (64, 64, 64), "P")
    s.cy((0, 32, 0), (64.6, 1.6, 64.6), "D")
    for y in (-14, 14):
        r = (32 ** 2 - y ** 2) ** 0.5 * 2
        s.cy((0, 32 + y, 0), (r + 0.5, 0.6, r + 0.5), "A")
    s.el((0, 45, -30), (22, 22, 12), "P", sub=True)
    s.el((0, 45, -23.0), (5, 5, 3), "C")
    s.cz((0, 45, -24.4), 12, 1.0, "D")
    return check(s)


# ------------------------------------------------------------------ creatures

def godzilla(k=1.0):
    s = Shape()
    K = lambda t: tuple(a * k for a in t)
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, K((sx * 7, 21, 3)), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.el(K((sx * 7.6, 16, 3)), K((9, 14, 10)), "P", part=pid)
        s.bx(K((sx * 7.6, 7, 4)), K((7, 12, 7)), "P", part=pid)
        s.bx(K((sx * 7.6, 1.5, 1)), K((8, 3, 11)), "D", part=pid)
    s.el(K((0, 31, 2)), K((18, 26, 16)), "P", rot=(-12, 0, 0))
    s.el(K((0, 28, -3.5)), K((12, 18, 6)), "A", rot=(-12, 0, 0))
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, K((sx * 8.5, 37, -3)), "ArmLeft" if sx < 0 else "ArmRight")
        s.seg(K((sx * 9.5, 36, -4)), K((sx * 9.5, 29, -9)), 3.6 * k, 3.6 * k, "P", part=pid)
        s.el(K((sx * 9.5, 28, -10)), K((3.8, 3.4, 3.8)), "D", part=pid)
    s.part(5, 0, K((0, 41, -3)), "Head")
    s.el(K((0, 42, -5)), K((9, 9, 9)), "P", part=5)
    s.bx(K((0, 46, -9.5)), K((7, 5.6, 11)), "P", part=5)
    s.bx(K((0, 48.6, -9)), K((7.6, 1.4, 9)), "D", part=5)
    for sx in SIDES:
        s.bx(K((sx * 3.5, 47.4, -12.4)), K((0.8, 1.0, 1.6)), "C", part=5)
    s.part(6, 5, K((0, 44, -6)), "Jaw")
    s.bx(K((0, 42.8, -10.6)), K((6.2, 2.4, 9.6)), "D", part=6)
    # dorsal plates
    for (y, z, h) in ((46, 1, 5), (43, 5, 7), (38, 8.5, 8), (32, 11, 7)):
        s.bx(K((0, y, z)), K((1.4, h, h * 0.9)), "A", rot=(45, 0, 0))
    def tip(s, prev):
        s.cone(K((0, 5.5, 55)), 3.4 * k, 9 * k, "P", "+z", part=prev)
    last = tail_chain(s, 7, 0, [K((0, 24, 10)), K((0, 19, 22)), K((0, 12, 34)), K((0, 6.5, 47))],
                      [11 * k, 8 * k, 5.4 * k], "P", tip)
    s.bx(K((0, 27, 16)), K((1.2, 5, 5)), "A", rot=(45, 0, 0), part=7)
    s.bx(K((0, 20.5, 28)), K((1.0, 4, 4)), "A", rot=(45, 0, 0), part=8)
    return check(s)


def king_kong():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 6, 17, 5), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * 6.5, 9, 5.5), (7, 17, 7.4), "P", part=pid)
        s.bx((sx * 6.5, 1.5, 3.6), (7.4, 3, 9.4), "D", part=pid)
    s.el((0, 29, 2), (22, 26, 16), "P", rot=(-18, 0, 0))
    s.el((0, 29, -4.6), (14, 13, 6), "A", rot=(-18, 0, 0))
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 12, 37, -1), "ArmLeft" if sx < 0 else "ArmRight")
        s.seg((sx * 13.6, 37, -1.5), (sx * 14.5, 22, -5), 7.4, 7.4, "P", part=pid)
        s.seg((sx * 14.5, 22, -5), (sx * 14.8, 7, -7), 7.0, 7.0, "P", part=pid)
        s.el((sx * 14.8, 4.2, -7.6), (8.4, 7.4, 8.4), "D", part=pid)
    s.part(5, 0, (0, 40, -5), "Head")
    s.el((0, 44.5, -9), (11, 11, 10), "P", part=5)
    s.bx((0, 47.4, -13.2), (10, 2, 3), "D", part=5)
    s.el((0, 42.4, -13.4), (8, 6, 5), "A", part=5)
    for sx in SIDES:
        s.bx((sx * 2.6, 45.6, -14.6), (1.4, 0.9, 0.6), "C", part=5)
    s.part(6, 5, (0, 40.5, -10), "Jaw")
    s.bx((0, 39.4, -12.6), (7, 2.6, 5), "D", part=6)
    return check(s)


def t_rex():
    s = Shape()
    s.el((0, 24, 0), (14, 16, 26), "P")
    s.el((0, 20.5, -3), (9, 8, 16), "A")
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 6, 21, 4), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.el((sx * 7, 16, 4), (7, 14, 11), "P", part=pid)
        s.seg((sx * 7, 11, 5), (sx * 7, 2.4, 7.5), 4.2, 4.4, "P", part=pid)
        s.bx((sx * 7, 1.2, 3.4), (5, 2.4, 9), "D", part=pid)
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 5, 22, -10), "ArmLeft" if sx < 0 else "ArmRight")
        s.seg((sx * 5.4, 22, -10), (sx * 5.4, 17.5, -13), 1.6, 1.6, "P", part=pid)
    s.part(5, 0, (0, 28, -11), "Head")
    s.el((0, 30.5, -15), (8, 11, 9), "P", rot=(30, 0, 0), part=5)
    s.bx((0, 33.4, -23), (8.4, 8, 13), "P", part=5)
    s.bx((0, 37.0, -21.4), (8.8, 1.2, 4.4), "D", part=5)
    for sx in SIDES:
        s.bx((sx * 4.3, 35.2, -20.6), (0.5, 1.2, 1.4), "C", part=5)
    s.bx((0, 30.2, -24.4), (7.2, 0.8, 9.6), "C", part=5)
    s.part(6, 5, (0, 31, -17), "Jaw")
    s.bx((0, 28.6, -23.2), (7.2, 3, 12), "D", part=6)
    def tip(s, prev):
        s.cone((0, 19.5, 45), 3.6, 11, "P", "+z", part=prev)
    tail_chain(s, 7, 0, [(0, 25, 12), (0, 23, 24), (0, 21, 36)], [10, 6.4], "P", tip)
    return check(s)


def xenomorph():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 1.5, 6.4, 0.2), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.seg((sx * 1.5, 6.4, 0.2), (sx * 1.5, 3.8, -1.2), 1.2, 1.3, "D", part=pid)
        s.seg((sx * 1.5, 3.8, -1.2), (sx * 1.5, 0.8, 0.6), 1.0, 1.0, "D", part=pid)
        s.bx((sx * 1.5, 0.4, -0.3), (1.4, 0.8, 2.2), "D", part=pid)
    s.bx((0, 6.4, 0.3), (3.0, 1.2, 1.8), "D")
    s.bx((0, 8.4, 0), (3.6, 3.4, 2.4), "P", rot=(-15, 0, 0))
    for y in (7.6, 8.6, 9.6):
        s.bx((0, y, -0.6), (3.8, 0.4, 2.2), "A", rot=(-15, 0, 0))
    for sx in SIDES:
        s.seg((sx * 0.9, 9.4, 1.0), (sx * 1.1, 11.2, 2.6), 0.7, 0.7, "D", kind="Cylinder")
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 2.2, 9.6, -0.2), "ArmLeft" if sx < 0 else "ArmRight")
        s.seg((sx * 2.4, 9.6, -0.2), (sx * 2.6, 5.6, -1.4), 0.8, 0.8, "P", part=pid)
        s.cone((sx * 2.6, 4.8, -1.8), 1.0, 1.4, "C", "-y", part=pid)
    s.part(5, 0, (0, 10.2, -0.8), "Head")
    s.el((0, 11.2, 1.0), (2.3, 2.4, 7.2), "P", rot=(18, 0, 0), part=5)
    s.bx((0, 10.4, -2.4), (1.6, 1.2, 1.6), "D", part=5)
    s.part(6, 5, (0, 10.2, -2.6), "Jaw")
    s.bx((0, 9.9, -3.6), (0.8, 0.8, 1.4), "C", part=6)
    def tip(s, prev):
        s.cone((0, 2.8, 15.4), 1.6, 3.2, "C", "+z", part=prev, d2=0.5)
    tail_chain(s, 7, 0, [(0, 6.2, 1.2), (0, 5.0, 6.0), (0, 3.8, 10.6), (0, 3.0, 13.6)], [1.2, 1.0, 0.8], "D", tip)
    return check(s)


def demogorgon():
    s = Shape()
    h = humanoid(s, 5.6, 3.8, 0.1, 3.0, 1.8, 1.0, 0.8, 6.6, tl="P", tt="P", ta="P", head=False)
    for sx in SIDES:
        s.bx((sx * 1.0, 0.4, -0.6), (1.2, 0.8, 2.0), "D", part=1 if sx < 0 else 2)
        s.bx((sx * h["ax"], h["shy"] - 6.6, -0.6), (1.3, 1.4, 1.4), "D", part=3 if sx < 0 else 4)
    s.part(5, 0, (0, h["top"], 0), "Head")
    cy_ = h["top"] + 1.8
    s.el((0, cy_, -0.6), (1.8, 1.8, 1.8), "C", part=5)
    import math
    for kk in range(5):
        a = math.radians(kk * 72)
        s.bx((1.9 * math.sin(a), cy_ + 1.9 * math.cos(a), -0.9), (1.5, 3.0, 0.5), "P", rot=(-20, 0, -kk * 72), part=5)
        s.bx((1.5 * math.sin(a), cy_ + 1.5 * math.cos(a), -1.2), (0.5, 1.6, 0.3), "C", rot=(-20, 0, -kk * 72), part=5)
    return check(s)


def stay_puft():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 5.5, 15, 0), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.el((sx * 5.5, 7.6, 0), (9, 16, 9), "P", part=pid)
    s.el((0, 24, 0), (22, 22, 17), "P")
    s.bx((0, 33.5, 0.6), (14, 2.6, 12), "A")
    for sx in SIDES:
        s.el((sx * 1.8, 32, -7.4), (3.2, 2.6, 1.8), "D")
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 10, 31, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.el((sx * 12.4, 25, 0), (5.6, 13, 5.6), "P", rot=(0, 0, sx * 18), part=pid)
        s.el((sx * 14, 18.4, -0.6), (5.2, 5.2, 5.2), "P", part=pid)
    s.part(5, 0, (0, 34, 0), "Head")
    s.el((0, 39.6, 0), (14, 12, 12), "P", part=5)
    s.cy((0, 45.4, 0), (12.4, 1.8, 12.4), "A", part=5)
    s.cy((0, 47.0, 0.4), (9.6, 2.2, 9.6), "A", part=5)
    s.bx((0, 45.6, -6.2), (3.2, 1.2, 0.6), "D", part=5)
    for sx in SIDES:
        s.el((sx * 2.4, 41, -5.8), (1.3, 1.8, 0.8), "D", part=5)
        s.el((sx * 4.4, 38.2, -4.8), (2.0, 1.6, 1.0), "A", part=5)
    s.bx((0, 37.2, -6.0), (4.0, 0.6, 0.6), "D", part=5)
    return check(s)


def groot():
    s = Shape()
    h = humanoid(s, 5.2, 5.0, 3.6, 3.4, 2.0, 1.4, 1.2, 6.2, tl="A", tt="A", ta="A", th="A", hw=2.8, hd=2.6)
    for sx in SIDES:
        s.bx((sx * h["lx"], 0.4, -0.6), (2.0, 0.8, 2.6), "D", part=1 if sx < 0 else 2)
        pid = 3 if sx < 0 else 4
        for dz in (-0.5, 0.5):
            s.cone((sx * h["ax"], h["shy"] - 6.6, dz), 0.5, 1.6, "A", "-y", part=pid)
    s.bx((0, 7.0, -1.0), (2.6, 3.0, 0.4), "D")
    for sx in SIDES:
        s.bx((sx * 0.6, h["hc"] + 0.3, -1.3), (0.5, 0.6, 0.3), "C", part=5)
        s.seg((sx * 0.8, h["top"] + 3.2, 0), (sx * 1.8, h["top"] + 5.0, 0), 0.5, 0.5, "A", part=5)
        s.el((sx * 1.9, h["top"] + 5.2, 0.2), (1.6, 1.0, 1.4), "P", part=5)
    s.el((0.4, h["top"] + 4.2, 0.6), (1.4, 1.0, 1.4), "P", part=5)
    s.el((-1.8, 8.4, 0.4), (1.2, 0.9, 1.2), "P")
    return check(s)


def grogu():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 0.6, 1.0, 0), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * 0.6, 0.5, -0.2), (0.8, 1.0, 1.0), "D", part=pid)
    s.bx((0, 2.2, 0), (3.0, 3.0, 2.6), "A")
    s.bx((0, 3.5, 0), (3.4, 0.8, 2.8), "A")
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 1.6, 3.3, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.bx((sx * 1.7, 2.6, -0.5), (0.8, 1.8, 0.8), "A", part=pid)
        s.bx((sx * 1.7, 1.6, -0.6), (0.6, 0.6, 0.6), "P", part=pid)
    s.part(5, 0, (0, 3.8, 0), "Head")
    s.el((0, 5.2, 0), (3.4, 2.8, 2.8), "P", part=5)
    for sx in SIDES:
        s.el((sx * 3.3, 5.6, 0.2), (3.8, 1.2, 0.8), "P", rot=(0, 0, sx * 12), part=5)
        s.el((sx * 3.2, 5.6, -0.1), (2.4, 0.6, 0.4), "A", rot=(0, 0, sx * 12), part=5)
        s.bx((sx * 0.7, 5.3, -1.35), (0.8, 0.8, 0.3), "D", part=5)
    return check(s)


def krypto():
    s = Shape()
    s.el((0, 4.4, 0.2), (3.4, 3.2, 7.4), "P")
    quad(s, 3.6, 1.1, 1.1, 2.4)
    s.part(5, 0, (0, 5.2, -3.2), "Head")
    s.bx((0, 6.4, -4.4), (2.6, 2.4, 2.6), "P", part=5)
    s.bx((0, 5.7, -6.0), (1.4, 1.2, 1.6), "P", part=5)
    s.bx((0, 6.1, -6.9), (0.6, 0.5, 0.3), "D", part=5)
    for sx in SIDES:
        s.bx((sx * 1.0, 7.9, -3.9), (0.6, 1.2, 0.6), "D", part=5)
        s.bx((sx * 0.6, 6.9, -5.75), (0.4, 0.4, 0.2), "C", part=5)
    s.bx((0, 4.4, -3.9), (1.2, 1.0, 0.4), "C")
    s.part(7, 0, (0, 5.8, -1.6), "Other")
    s.bx((0, 5.9, 2.2), (3.4, 0.4, 6.4), "A", rot=(10, 0, 0), part=7)
    s.part(8, 0, (0, 5.2, 3.6), "Tail")
    s.seg((0, 5.2, 3.6), (0, 7.2, 5.0), 0.7, 0.7, "P", part=8)
    return check(s)


def stitch():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 0.9, 1.5, 0), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * 0.9, 0.7, 0), (1.1, 1.5, 1.2), "P", part=pid)
    s.el((0, 2.7, 0), (3.2, 3.0, 2.6), "P")
    s.el((0, 2.5, -0.9), (2.0, 2.0, 0.9), "A")
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 1.5, 3.5, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.seg((sx * 1.7, 3.5, 0), (sx * 2.3, 1.9, -0.5), 0.9, 0.9, "P", part=pid)
    s.part(5, 0, (0, 3.9, 0), "Head")
    s.el((0, 5.1, -0.1), (4.2, 3.0, 3.0), "P", part=5)
    for sx in SIDES:
        s.el((sx * 2.9, 5.9, 0.5), (2.8, 1.8, 0.5), "P", rot=(0, 0, sx * -30), part=5)
        s.el((sx * 2.8, 5.9, 0.3), (1.6, 0.9, 0.3), "A", rot=(0, 0, sx * -30), part=5)
        s.el((sx * 1.0, 5.3, -1.45), (1.1, 1.2, 0.4), "D", part=5)
    s.el((0, 4.7, -1.65), (1.3, 0.8, 0.5), "D", part=5)
    s.bx((0, 4.0, -1.45), (2.0, 0.4, 0.3), "C", part=5)
    return check(s)


def minion():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 0.7, 0.9, 0), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * 0.7, 0.4, -0.1), (0.9, 0.9, 1.2), "D", part=pid)
    s.cy((0, 3.0, 0), (3.2, 3.6, 3.2), "P")
    s.el((0, 4.7, 0), (3.2, 2.2, 3.2), "P")
    s.el((0, 1.3, 0), (3.2, 1.4, 3.2), "A")
    s.cy((0, 1.7, 0), (3.4, 1.2, 3.4), "A")
    s.bx((0, 2.5, -1.4), (1.6, 1.4, 0.6), "A")
    s.cy((0, 3.9, 0), (3.4, 0.6, 3.4), "D")
    s.cz((0, 3.9, -1.6), 1.9, 0.8, "A")
    s.cz((0, 3.9, -1.8), 1.2, 0.6, "C")
    s.bx((0, 3.9, -2.1), (0.5, 0.5, 0.2), "D")
    s.bx((0, 2.9, -1.65), (1.2, 0.3, 0.2), "D")
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 1.7, 2.8, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.bx((sx * 2.0, 2.1, 0), (0.5, 1.6, 0.5), "P", part=pid)
        s.bx((sx * 2.0, 1.2, 0), (0.7, 0.6, 0.7), "D", part=pid)
    s.bx((0, 6.1, 0), (0.25, 1.0, 0.25), "D")
    s.bx((0.4, 6.0, 0), (0.25, 0.9, 0.25), "D", rot=(0, 0, -25))
    return check(s)


def dragon(k, horns=False, wing_span=9.0, fin=None):
    """Four-legged winged dragon. k scales everything."""
    s = Shape()
    K = lambda t: tuple(a * k for a in t)
    s.el(K((0, 5.0, 0)), K((5, 4.2, 11)), "P")
    s.el(K((0, 4.2, -0.6)), K((3.4, 2.6, 8)), "A")
    roles = [("LegFrontLeft", -1, -1), ("LegFrontRight", 1, -1), ("LegBackLeft", -1, 1), ("LegBackRight", 1, 1)]
    for i, (role, sx, sz) in enumerate(roles):
        pid = s.part(1 + i, 0, K((sx * 1.9, 4.2, sz * 3.0)), role)
        s.bx(K((sx * 2.0, 2.1, sz * 3.0)), K((1.4, 4.4, 1.6)), "P", part=pid)
        s.bx(K((sx * 2.0, 0.3, sz * 3.0 - 0.5)), K((1.6, 0.6, 2.2)), "D", part=pid)
    s.part(5, 0, K((0, 6.0, -5.2)), "Head")
    s.seg(K((0, 6.0, -5.2)), K((0, 7.0, -7.8)), 2.4 * k, 2.4 * k, "P", part=5, kind="Ellipsoid", over=1.4 * k)
    s.el(K((0, 7.2, -9.2)), K((3.4, 2.6, 4.2)), "P", part=5)
    for sx in SIDES:
        s.bx(K((sx * 1.5, 7.6, -10.2)), K((0.4, 0.8, 1.0)), "C", part=5)
        if horns:
            s.cone(K((sx * 1.2, 8.8, -7.6)), 0.8 * k, 2.6 * k, "D", "+z", part=5)
        else:
            s.bx(K((sx * 1.4, 8.6, -7.8)), K((0.4, 1.8, 0.9)), "D", rot=(0, 0, sx * -30), part=5)
            s.bx(K((sx * 0.8, 8.8, -8.0)), K((0.4, 1.4, 0.8)), "D", rot=(0, 0, sx * -15), part=5)
    s.part(6, 5, K((0, 6.6, -8.0)), "Jaw")
    s.bx(K((0, 6.1, -9.8)), K((2.8, 0.8, 3.2)), "D", part=6)
    for i, sx in enumerate(SIDES):
        pid = s.part(9 + i, 0, K((sx * 2.2, 6.6, -1.4)), "WingLeft" if sx < 0 else "WingRight")
        w = wing_span
        s.bx(K((sx * (2.2 + w / 2), 7.0, 0.2)), K((w, 0.5, 6.0)), "P", part=pid)
        s.bx(K((sx * (2.2 + w / 2), 7.4, -2.6)), K((w + 0.4, 0.8, 0.8)), "D", part=pid)
        s.bx(K((sx * (2.2 + w * 0.8), 7.0, 3.2)), K((w * 0.4, 0.5, 2.4)), "P", part=pid)
    # dorsal spikes
    for z in (-3.0, -0.5, 2.0):
        s.cone(K((0, 7.4, z)), 0.8 * k, 1.4 * k, "A", "+y", part=0)
    def tip(s, prev):
        if fin:
            s.bx(K((0.9, 4.1, 15.2)), K((1.8, 0.3, 2.0)), fin[0], part=prev)
            s.bx(K((-0.9, 4.1, 15.2)), K((1.8, 0.3, 2.0)), fin[1], part=prev)
        else:
            s.cone(K((0, 4.1, 15.4)), 1.8 * k, 2.4 * k, "D", "+z", part=prev, d2=0.5 * k)
    tail_chain(s, 11, 0, [K((0, 5.0, 5.4)), K((0, 4.6, 10.2)), K((0, 4.2, 14.0))], [2.0 * k, 1.4 * k], "P", tip)
    return s


def toothless():
    s = dragon(1.0, fin=("A", "P"))
    s.seat(0, (0, 7.2, -1.2))
    return check(s)


def smaug():
    s = dragon(3.6, horns=True, wing_span=11.0)
    return check(s)


def sandworm():
    s = Shape()
    k = 1.4
    K = lambda t: tuple(a * k for a in t)
    s.cz(K((0, 10, -4)), 16 * k, 10 * k, "P")
    s.cz(K((0, 10, -9.2)), 14.5 * k, 1.2 * k, "C")
    s.cz(K((0, 10, -9.6)), 10 * k, 5 * k, "P", sub=True)
    for sx in SIDES:
        s.bx(K((sx * 5.6, 10, -10.4)), K((4.0, 11, 1.4)), "P", rot=(0, sx * 25, 0))
    s.bx(K((0, 16.2, -10.2)), K((10, 3.4, 1.4)), "P", rot=(-25, 0, 0))
    pts = [K((0, 10, 1)), K((0, 9, 11)), K((0, 8, 21)), K((0, 7, 31)), K((0, 6, 41)), K((0, 5.5, 51))]
    ds = [15, 14, 12.6, 11, 9]
    prev = 0
    for i in range(5):
        pid = s.part(1 + i, prev, pts[i], "Tail")
        s.seg(pts[i], pts[i + 1], ds[i] * k, ds[i] * k, "P", part=pid, kind="Cylinder", over=0.4 * k)
        mid = tuple((pts[i][j] + pts[i + 1][j]) / 2 for j in range(3))
        s.cz(tuple(mid[j] - (0 if j != 2 else 4.2 * k) for j in range(3)), (ds[i] + 0.7) * k, 1.0 * k, "D", part=pid)
        prev = pid
    return check(s)


def ghidorah():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 7, 20, 3), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.el((sx * 7.6, 15, 3), (8.6, 13, 9.6), "P", part=pid)
        s.bx((sx * 7.6, 6.5, 4), (6.4, 11, 6.4), "P", part=pid)
        s.bx((sx * 7.6, 1.5, 1.6), (7.6, 3, 10), "D", part=pid)
    s.el((0, 28, 2), (18, 20, 16), "P", rot=(-10, 0, 0))
    s.el((0, 27, -4), (11, 14, 5), "A")
    for i, sx in enumerate(SIDES):
        pid = s.part(9 + i, 0, (sx * 8, 34, 5), "WingLeft" if sx < 0 else "WingRight")
        s.bx((sx * 22, 38, 6), (28, 1.2, 14), "P", rot=(0, 0, sx * 12), part=pid)
        s.bx((sx * 22, 39.6, 0), (30, 2.0, 2.0), "D", rot=(0, 0, sx * 12), part=pid)
        for z in (4, 9, 13):
            s.bx((sx * 22, 38, z), (26, 1.4, 0.8), "A", rot=(0, 0, sx * 12), part=pid)
    heads = [(5, 0, "Head"), (12, -1, "Other"), (13, 1, "Other")]
    for pid, sx, role in heads:
        base = (sx * 4.5, 36, -2)
        top = (sx * 9.5, 52, -8) if sx else (0, 55, -8)
        s.part(pid, 0, base, role)
        mid = ((base[0] + top[0]) / 2, (base[1] + top[1]) / 2 + 1, base[2] - 3)
        s.seg(base, (mid[0], mid[1], base[2]), 4.2, 4.2, "P", part=pid, kind="Ellipsoid", over=3)
        s.seg((mid[0], mid[1], base[2]), (top[0], top[1], base[2]), 3.6, 3.6, "P", part=pid, kind="Ellipsoid", over=3)
        s.bx((top[0], top[1] + 1, -6), (4.6, 4.0, 8), "P", part=pid)
        s.cone((top[0] - 1.2, top[1] + 4, -3.6), 1.0, 4.0, "A", "+z", part=pid)
        s.cone((top[0] + 1.2, top[1] + 4, -3.6), 1.0, 4.0, "A", "+z", part=pid)
        s.bx((top[0], top[1] + 1.6, -10.1), (3.8, 0.8, 0.4), "C", part=pid)
    prev = 0
    for chain, sx in ((0, -1), (1, 1)):
        pts = [(sx * 2.5, 22, 10), (sx * 4, 16, 22), (sx * 6, 10, 34), (sx * 7, 5, 46)]
        def tip(s, p, sx=sx):
            s.cone((sx * 7.4, 4.4, 50), 2.6, 7, "A", "+z", part=p)
        tail_chain(s, 20 + chain * 5, 0, pts, [7, 5, 3.4], "P", tip)
    return check(s)


def mothra():
    s = Shape()
    s.el((0, 10, 0), (5.4, 5.4, 15), "P")
    for z in (2, 4.5):
        s.cz((0, 10, z), 5.8, 0.8, "A")
    s.part(5, 0, (0, 11, -7), "Head")
    s.el((0, 11.5, -9), (4.6, 4.2, 4.2), "P", part=5)
    for sx in SIDES:
        s.el((sx * 1.9, 12, -10.2), (1.8, 2.2, 1.8), "C", part=5)
        s.seg((sx * 1, 13.4, -10), (sx * 3, 17, -12), 0.6, 0.6, "D", part=5)
    for i, sx in enumerate(SIDES):
        pid = s.part(9 + i, 0, (sx * 2.6, 11, -2), "WingLeft" if sx < 0 else "WingRight")
        s.bx((sx * 13.5, 11, -3.5), (22, 0.6, 12), "P", rot=(0, sx * -8, 0), part=pid)
        s.el((sx * 13, 11.4, -3.5), (9, 0.7, 6), "A", part=pid)
        s.el((sx * 17, 11.6, -4.5), (3.2, 0.8, 3.2), "C", part=pid)
        s.bx((sx * 11, 10.6, 6), (18, 0.6, 9), "P", rot=(0, sx * 10, 0), part=pid)
        s.el((sx * 12, 11.0, 6.4), (6, 0.7, 4), "D", part=pid)
        s.bx((sx * 13.5, 11.3, -9.2), (22.6, 0.9, 1.0), "D", rot=(0, sx * -8, 0), part=pid)
    return check(s)


def dalek():
    s = Shape()
    s.bx((0, 0.6, 0), (6.6, 1.2, 7.6), "D")
    s.bx((0, 3.2, 0.3), (5.6, 4.2, 6.6), "P")
    s.bx((0, 3.2, -0.2), (4.8, 4.2, 6.6), "P", rot=(-8, 0, 0))
    for sx in SIDES:
        for y in (2.2, 3.8):
            for z in (-1.8, 0.4, 2.6):
                s.el((sx * 2.9, y, z), (0.9, 0.9, 0.9), "A")
    s.bx((0, 5.9, 0), (4.4, 1.3, 5.0), "D")
    s.cz((1.1, 5.9, -3.3), 0.6, 2.2, "D")
    s.cz((-1.1, 5.9, -3.4), 0.5, 2.4, "D")
    s.cz((-1.1, 5.9, -4.7), 1.3, 0.4, "A")
    s.bx((0, 7.0, 0), (4.0, 1.0, 4.4), "A")
    s.el((0, 8.0, 0), (3.8, 2.8, 3.8), "P")
    s.cz((0, 8.2, -2.6), 0.4, 2.2, "D")
    s.bx((0, 8.2, -3.8), (0.9, 0.9, 0.5), "C")
    for sx in SIDES:
        s.bx((sx * 1.9, 9.0, 0), (0.4, 0.7, 0.4), "C")
    return check(s)


def iron_man():
    s = Shape()
    h = humanoid(s, 5.0, 3.8, 2.2, 3.6, 2.2, 1.4, 1.3, 3.8, th="P", hw=2.0, hd=2.2)
    for i, sx in enumerate(SIDES):
        s.bx((sx * h["lx"], 0.6, -0.2), (1.6, 1.2, 2.0), "A", part=1 + i)
        s.bx((sx * h["lx"], 0.1, 0.2), (1.0, 0.25, 1.0), "C", part=1 + i)
        s.bx((sx * h["ax"], h["shy"] - 3.4, -0.1), (1.5, 1.6, 1.5), "A", part=3 + i)
        s.bx((sx * h["ax"], h["shy"] - 4.0, -0.4), (0.7, 0.4, 0.7), "C", part=3 + i)
        s.bx((sx * 2.1, h["top"] - 0.4, 0), (1.5, 1.2, 2.2), "P")
    s.bx((0, 6.0, -0.4), (2.6, 1.6, 1.8), "A")
    s.cz((0, 7.6, -1.15), 1.2, 0.4, "C")
    s.bx((0, h["hc"] - 0.1, -1.05), (1.6, 1.8, 0.3), "A", part=5)
    for sx in SIDES:
        s.bx((sx * 0.45, h["hc"] + 0.2, -1.25), (0.6, 0.3, 0.2), "C", part=5)
    s.seat(0, (0, 6.8, 0))
    return check(s)


def gipsy_danger():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 6, 26, 0), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * 6.5, 20, 0), (6, 12, 7), "P", part=pid)
        s.el((sx * 6.5, 14, 0), (6.4, 5, 6.4), "D", part=pid)
        s.bx((sx * 6.5, 7, 0), (6.6, 10, 7.4), "P", part=pid)
        s.bx((sx * 6.5, 1.5, -1.5), (8, 3, 11), "D", part=pid)
    s.bx((0, 27, 0), (12, 5, 8), "D")
    s.bx((0, 31, 0), (10, 6, 8), "A")
    s.bx((0, 39, 0), (20, 12, 12), "P")
    s.bx((0, 44, 1), (16, 3, 10), "A")
    s.cz((0, 39, -6.4), 6.4, 1.4, "C")
    s.cz((0, 39, -6.8), 3.2, 1.2, "A")
    for i, sx in enumerate(SIDES):
        s.el((sx * 12, 43, 0), (8, 7, 9), "P")
        pid = s.part(3 + i, 0, (sx * 13, 42, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.bx((sx * 14, 36, 0), (5, 12, 5), "P", part=pid)
        s.bx((sx * 14, 25, -0.5), (7, 11, 7), "A", part=pid)
        s.cz((sx * 14, 28, 4.0), 3.0, 1.6, "C", part=pid)
        s.bx((sx * 14, 17, -0.5), (6.4, 6, 6.4), "D", part=pid)
    s.part(5, 0, (0, 45, 0), "Head")
    s.bx((0, 48, -1), (6, 5, 6), "P", part=5)
    s.bx((0, 48.5, -4.2), (5, 1.4, 0.4), "C", part=5)
    s.bx((0, 50.8, 0), (1.2, 1.2, 5), "A", part=5)
    s.seat(5, (0, 48, -1))
    return check(s)


# ------------------------------------------------------------------ structures

def power_battery():
    s = Shape()
    s.bx((0, 0.6, 0), (6.4, 1.2, 6.4), "D")
    s.cy((0, 1.7, 0), (5.2, 1.0, 5.2), "P")
    s.cy((0, 5.5, 0), (4.8, 6.4, 4.8), "C")
    for a in (0, 90):
        s.bx((0, 5.5, 0), (6.0, 6.6, 1.0), "P", rot=(0, a + 45, 0))
    s.cy((0, 2.6, 0), (6.4, 0.8, 6.4), "P")
    s.cy((0, 8.4, 0), (6.4, 0.8, 6.4), "P")
    s.cone((0, 10.0, 0), 5.4, 2.6, "P", "+y")
    s.bx((0, 12.0, 0), (0.8, 2.6, 4.0), "D")
    s.bx((0, 11.8, 0), (1.2, 1.4, 2.4), "D", sub=True)
    for sx in SIDES:
        s.bx((sx * 3.7, 5.5, 0), (1.0, 3.6, 0.8), "D")
    return check(s)


def sling_ring_portal():
    s = Shape()
    import math
    s.cz((0, 7, 0), 13, 1.0, "C")
    s.cz((0, 7, 0), 10.6, 1.2, "P")
    s.cz((0, 7, 0), 9.0, 3.0, "P", sub=True)
    for kk in range(12):
        a = math.radians(kk * 30 + 15)
        s.bx((6.8 * math.sin(a), 7 + 6.8 * math.cos(a), 0), (0.8, 1.4, 0.8), "A", rot=(0, 0, -kk * 30 - 15))
    s.bx((0, 0.3, 0), (4, 0.6, 2), "D")
    return check(s)


SHAPES = dict(
    mjolnir=mjolnir, stormbreaker=stormbreaker, cap_shield=cap_shield, lightsaber=lightsaber,
    darksaber=darksaber, double_saber=double_saber, sting=sting, elder_wand=elder_wand,
    neutrona_wand=neutrona_wand, portal_gun=portal_gun, web_shooter=web_shooter,
    wolverine_claws=wolverine_claws, infinity_gauntlet=infinity_gauntlet, trident=trident,
    batarang=batarang, acme_anvil=acme_anvil, maui_hook=maui_hook, power_sword=power_sword,
    harley_mallet=harley_mallet, lantern_fist=lantern_fist, boxing_glove=boxing_glove,
    sonic_screwdriver=sonic_screwdriver, kong_axe=kong_axe, batmobile=batmobile, x_wing=x_wing,
    tie_fighter=tie_fighter, millennium_falcon=millennium_falcon, delorean=delorean, tardis=tardis,
    broom=broom, surfboard=surfboard, at_at=at_at, death_star=death_star, godzilla=godzilla,
    king_kong=king_kong, t_rex=t_rex, xenomorph=xenomorph, demogorgon=demogorgon, stay_puft=stay_puft,
    groot=groot, grogu=grogu, krypto=krypto, stitch=stitch, minion=minion, toothless=toothless,
    smaug=smaug, sandworm=sandworm, ghidorah=ghidorah, mothra=mothra, dalek=dalek, iron_man=iron_man,
    gipsy_danger=gipsy_danger, power_battery=power_battery, sling_ring_portal=sling_ring_portal,
)
