"""Humanoid rig + dressing for the TV / movie pack (adapted from tools/icons/dc/hero.py).

Part ids (all creatures, same as the TMNT rig): 1 leg L, 2 leg R, 3 arm L, 4 arm R, 5 head, 6 jaw,
7 cloth (cape / coat tails / hair streamers), 8..11 tail chain, 12 wing L, 13 wing R, 255 seat.
Held weapons are authored *inside* the arm parts so they follow the swing. Creatures face -Z, left = -X,
feet at y = 0. Heroic proportions: long legs, broad shoulders, small head (~24 units tall before k).
"""
import math

from slib import S, TIP_NZ, ALONG_Z, ALONG_X


def humanoid(s, leg_h=9.0, leg_w=2.6, leg_d=2.8, hip_x=1.9, foot=(2.7, 1.3, 3.8), torso=(7.6, 9.0, 4.4),
             torso_t="P", torso_kind="box", arm_w=2.3, arm_len=9.2, arm_x=4.9, hand=2.3, arm_t="P", leg_t="P",
             hand_t="P", foot_t="P", neck=1.2, arm_spread=0.0):
    """Legs (1,2), torso (root), arms (3,4). Returns key points; the head is authored by the caller (part 5).
    `arm_spread` (deg) swings the arms outward from the body (fists-at-hips / bulky brutes)."""
    y_hip = leg_h
    tw, th, td = torso
    y_sh = leg_h + th
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):
        x = sx * hip_x
        s.on(pid)
        s.box((x, (leg_h + 0.6) / 2 + 0.4, 0), (leg_w, leg_h - 0.2, leg_d), leg_t)
        s.box((x, foot[1] / 2, -(foot[2] - leg_d) / 2 - 0.3), foot, foot_t)
        s.part(pid, 0, (x, y_hip, 0), role)
    s.on(0)
    if torso_kind == "box":
        s.box((0, y_hip + th / 2, 0), (tw, th, td), torso_t)
    elif torso_kind == "v":      # heroic V: broad chest, narrow waist
        s.box((0, y_hip + th * 0.62, 0), (tw, th * 0.62, td), torso_t)
        s.box((0, y_hip + th * 0.22, 0), (tw * 0.66, th * 0.5, td * 0.9), torso_t)
        s.ell((0, y_hip + th * 0.68, -td * 0.18), (tw * 0.92, th * 0.5, td * 0.8), torso_t)   # pecs
    else:
        s.ell((0, y_hip + th / 2, 0), (tw, th * 1.08, td), torso_t)
        s.box((0, y_hip + 0.8, 0), (tw * 0.8, 1.6, td * 0.85), torso_t)
    s.box((0, y_sh + neck / 2, 0), (tw * 0.3, neck + 0.4, td * 0.45), torso_t)   # neck
    hands = {}
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):
        x = sx * arm_x
        s.on(pid)
        top = y_sh - 0.4
        a = math.radians(arm_spread)
        hx = x + sx * math.sin(a) * arm_len
        hy_ = top - math.cos(a) * arm_len
        s.seg((x, top, 0), (hx, hy_, 0), arm_w, arm_w, arm_t)
        s.ell((x, top - 0.3, 0), (arm_w * 1.25, arm_w * 1.1, arm_w * 1.2), arm_t)            # deltoid
        hy = hy_ - hand / 2 + 0.3
        s.box((hx, hy, -0.2), (hand, hand, hand), hand_t)
        hands[sx] = (hx, hy, -0.2)
        s.part(pid, 0, (x, y_sh - 0.6, 0), role)
    s.on(0)
    return dict(y_hip=y_hip, y_sh=y_sh, y_neck=y_sh + neck, hands=hands, td=td, tw=tw, th=th, leg_h=leg_h,
                hip_x=hip_x, arm_x=arm_x, arm_len=arm_len, arm_w=arm_w, leg_w=leg_w, leg_d=leg_d)


def hero(s, bulk=1.0, tall=0.0, fem=False, suit="P", legs=None, arms=None, boots=None, boot_h=3.6, gloves=None,
         glove_h=2.8, trunks=None, belt=None, buckle=None, torso_kind="v", arm_spread=4.0, shoulder=None):
    """A caped-crusader body: V torso, boots/gloves/trunks/belt as separate tinted shells."""
    k = bulk
    if fem:
        b = humanoid(s, leg_h=9.6 + tall, leg_w=2.2 * k, leg_d=2.4 * k, hip_x=1.6 * k, foot=(2.2, 1.2, 3.4),
                     torso=(6.2 * k, 8.2 + tall * 0.4, 3.8 * k), torso_t=suit, torso_kind=torso_kind,
                     arm_w=1.8 * k, arm_len=8.8, arm_x=4.0 * k, hand=1.9 * k, arm_t=arms or suit,
                     leg_t=legs or suit, hand_t=gloves or arms or suit, foot_t=boots or legs or suit,
                     neck=1.3, arm_spread=arm_spread)
    else:
        b = humanoid(s, leg_h=9.0 + tall, leg_w=2.7 * k, leg_d=2.9 * k, hip_x=1.9 * k, foot=(2.8 * k, 1.3, 3.9),
                     torso=(7.8 * k, 9.0 + tall * 0.4, 4.6 * k), torso_t=suit, torso_kind=torso_kind,
                     arm_w=2.4 * k, arm_len=9.2 + tall * 0.3, arm_x=5.0 * k, hand=2.4 * k, arm_t=arms or suit,
                     leg_t=legs or suit, hand_t=gloves or arms or suit, foot_t=boots or legs or suit,
                     neck=1.2, arm_spread=arm_spread)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    if boots:
        for pid, sx in ((1, -1), (2, 1)):
            s.on(pid)
            s.box((sx * b["hip_x"], boot_h / 2 + 0.6, 0), (b["leg_w"] + 0.35, boot_h, b["leg_d"] + 0.35), boots)
            s.box((sx * b["hip_x"], boot_h + 0.6, -0.1), (b["leg_w"] + 0.6, 0.6, b["leg_d"] + 0.6), boots)   # cuff
    if gloves:
        for pid, sx in ((3, -1), (4, 1)):
            s.on(pid)
            hx, hy, hz = b["hands"][sx]
            s.box((hx, hy + glove_h / 2 + 0.6, 0), (b["arm_w"] + 0.35, glove_h, b["arm_w"] + 0.35), gloves)
    s.on(0)
    if trunks:
        s.box((0, yh + 0.4, 0), (tw * 0.72, 2.2, td + 0.3), trunks)
        for sx in (-1, 1):
            s.box((sx * b["hip_x"], yh - 0.6, 0), (b["leg_w"] + 0.3, 1.4, b["leg_d"] + 0.3), trunks)
    if belt:
        s.box((0, yh + 1.6, 0), (tw * 0.72, 1.0, td * 0.95), belt)
        if buckle:
            s.box((0, yh + 1.6, -td * 0.48 - 0.1), (1.4, 1.2, 0.4), buckle)
    if shoulder:
        for pid, sx in ((3, -1), (4, 1)):
            s.on(pid)
            s.ell((sx * b["arm_x"], ys - 0.2, 0), (b["arm_w"] * 1.7, b["arm_w"] * 1.1, b["arm_w"] * 1.6), shoulder)
        s.on(0)
    b["chest"] = (0, yh + b["th"] * 0.68, -td * 0.18 - td * 0.4)   # front of the pecs (z)
    return b


def head(s, b, skin="P", hair=None, style="short", eyes="C", mask=None, jaw_t=None, w=3.7, h=4.2, d=4.0, dy=0.0,
         brows=None):
    """A human head on part 5. style: short | long | slick | curl (Superman) | bald | pony | spiky | none."""
    s.on(5)
    hy = b["y_neck"] + h / 2 - 0.1 + dy
    s.ell((0, hy, 0), (w, h, d), skin)
    s.box((0, hy - h * 0.3, -d * 0.12), (w * 0.72, h * 0.34, d * 0.72), jaw_t or skin)      # jaw
    if mask:
        s.box((0, hy + h * 0.08, -d * 0.2), (w * 1.04, h * 0.26, d * 0.72), mask)           # domino mask
    for sx in (-1, 1):
        s.box((sx * w * 0.2, hy + h * 0.08, -d / 2 + 0.05), (w * 0.2, h * 0.1, 0.4), eyes)
    if brows:
        s.box((0, hy + h * 0.22, -d / 2 + 0.1), (w * 0.7, 0.4, 0.4), brows)
    if hair and style != "bald":
        if style in ("short", "curl", "slick", "spiky", "long", "pony"):
            s.ell((0, hy + h * 0.3, d * 0.16), (w * 1.1, h * 0.56, d * 0.98), hair)
            s.box((0, hy + h * 0.02, d * 0.36), (w * 1.02, h * 0.5, d * 0.36), hair)       # back of the head
        if style == "curl":
            s.ell((w * 0.12, hy + h * 0.24, -d * 0.5), (1.0, 1.4, 0.7), hair, (0, 0, 25))   # the S-curl
        if style == "spiky":
            for i, (x, z) in enumerate(((-1.0, 0.4), (0.0, 0.0), (1.0, 0.4), (-0.5, 1.2), (0.6, 1.2))):
                s.cone((x * w * 0.3, hy + h * 0.5, z - 0.4), (1.4, 2.2, 1.4), hair, (15 * z, 0, -x * 25))
        if style == "long":
            s.box((0, hy - h * 0.3, d * 0.4), (w * 1.05, h * 1.0, d * 0.4), hair)
            for sx in (-1, 1):
                s.box((sx * w * 0.46, hy - h * 0.1, 0.1), (0.5, h * 0.8, d * 0.7), hair)
        if style == "pony":
            s.seg((0, hy + h * 0.2, d * 0.5), (0, hy - h * 0.8, d * 0.9), 1.0, 1.0, hair)
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    b["hy"] = hy
    b["head_h"] = h
    b["head_w"] = w
    b["head_d"] = d
    return hy


def cowl(s, b, t="P", ears=2.4, eyes="C", chin="P", w=4.0, h=4.6, d=4.2):
    """Batman cowl: helmet head with pointed ears, white eye slits, open chin."""
    s.on(5)
    hy = b["y_neck"] + h / 2 - 0.1
    s.ell((0, hy + 0.2, 0.1), (w, h, d), t)
    s.box((0, hy - h * 0.3, -d * 0.12), (w * 0.7, h * 0.32, d * 0.72), chin)
    s.box((0, hy + h * 0.12, -d * 0.22), (w * 1.02, h * 0.36, d * 0.66), t)     # angular brow plate
    for sx in (-1, 1):
        s.box((sx * w * 0.2, hy + h * 0.08, -d * 0.55), (w * 0.24, 0.45, 0.3), eyes, (0, 0, -sx * 10))
        s.cone((sx * w * 0.3, hy + h * 0.5 + ears / 2, 0.2), (1.0, ears, 1.0), t)
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    b["hy"] = hy
    b["head_h"] = h
    b["head_w"] = w
    b["head_d"] = d
    return hy


def cape(s, b, t="A", length=None, flare=1.5, width=None, collar=None, scallops=0, split=False):
    """A cape on part 7 (Cloth): hangs from the shoulders behind the back, flared at the hem."""
    ys, td, tw = b["y_sh"], b["td"], b["tw"]
    L = length if length is not None else ys - 1.2
    W = width if width is not None else tw * 1.02
    s.on(7)
    z0 = td / 2 + 0.35
    top = ys + 0.2
    n = 3
    for i in range(n):
        f = (i + 0.5) / n
        yy = top - L * f
        ww = W + flare * f * 2.0
        zz = z0 + 0.35 + f * 1.4
        s.box((0, yy, zz), (ww, L / n + 0.3, 0.7), t, (-6 - f * 4, 0, 0))
    s.box((0, top, z0 - 0.1), (W * 0.95, 1.0, 1.0), t)                                     # shoulder drape
    if collar:
        s.box((0, top + 0.9, z0 - 0.5), (W * 0.8, 2.0, 0.6), collar, (12, 0, 0))
    if scallops:
        hem = top - L
        for i in range(scallops):
            x = -(W + flare * 2) / 2 + (i + 0.5) * (W + flare * 2) / scallops
            s.cone((x, hem - 0.5, z0 + 1.9), (1.4, 1.6, 0.6), t, (180, 0, 0))
    if split:
        s.box((0, top - L * 0.6, z0 + 1.6), (0.3, L * 0.8, 0.9), "D")
    s.part(7, 0, (0, top, z0), "Cloth")
    s.on(0)


def wings(s, b, t="P", span=12.0, h=9.0, feathers=5, feather_t="A", y=None, z=None, angle=25):
    """Feathered wings on parts 12/13 from the upper back (Hawkman)."""
    ys, td = b["y_sh"], b["td"]
    y0 = ys - 1.0 if y is None else y
    z0 = td / 2 + 0.6 if z is None else z
    for pid, sx, role in ((12, -1, "WingLeft"), (13, 1, "WingRight")):
        s.on(pid)
        s.box((sx * span * 0.25, y0 + 1.5, z0 + 0.8), (span * 0.5, 2.0, 0.8), t, (0, 0, sx * angle))
        for i in range(feathers):
            f = i / max(1, feathers - 1)
            x = sx * (1.2 + f * span * 0.46)
            s.box((x, y0 + 1.6 - h * (0.35 + 0.35 * (1 - abs(f - 0.6))) / 2 + f * 3.0, z0 + 0.9),
                  (span / feathers * 0.9, h * (0.35 + 0.4 * (1 - abs(f - 0.6))), 0.5),
                  feather_t if i % 2 else t, (0, 0, sx * (angle - 10 * f)))
        s.part(pid, 0, (sx * 1.0, y0, z0), role)
    s.on(0)


# --------------------------------------------------------------------------------- chest emblems

def emblem_s(s, c, t="A", inner="P", k=1.0):
    """Superman shield: a diamond (rotated box) with an S hint."""
    x, y, z = c
    s.box((x, y, z), (3.0 * k, 3.0 * k, 0.4), t, (0, 0, 45))
    s.box((x, y + 0.6 * k, z - 0.15), (1.8 * k, 0.45, 0.3), inner)
    s.box((x, y, z - 0.15), (1.4 * k, 0.45, 0.3), inner, (0, 0, -20))
    s.box((x, y - 0.6 * k, z - 0.15), (1.8 * k, 0.45, 0.3), inner)


def emblem_bat(s, c, t="D", k=1.0, oval=None):
    x, y, z = c
    if oval:
        s.ell((x, y, z + 0.05), (5.2 * k, 2.6 * k, 0.4), oval)
    s.box((x, y, z - 0.1), (1.0 * k, 1.4 * k, 0.4), t)                                    # body
    for sx in (-1, 1):
        s.box((x + sx * 1.3 * k, y + 0.1, z - 0.1), (1.8 * k, 0.9 * k, 0.4), t, (0, 0, sx * 12))
        s.box((x + sx * 2.1 * k, y - 0.1, z - 0.1), (1.0 * k, 0.8 * k, 0.4), t, (0, 0, -sx * 20))
        s.box((x + sx * 0.3 * k, y + 0.8 * k, z - 0.1), (0.3, 0.6, 0.4), t)             # ears


def emblem_lantern(s, c, t="C", k=1.0, bg="D"):
    """Green Lantern symbol: circle between two bars."""
    x, y, z = c
    s.cyl((x, y, z), (3.0 * k, 0.4, 3.0 * k), bg, (90, 0, 0))
    s.cyl((x, y, z - 0.1), (2.2 * k, 0.4, 2.2 * k), t, (90, 0, 0))
    s.cyl((x, y, z - 0.2), (1.3 * k, 0.4, 1.3 * k), bg, (90, 0, 0))
    for sy in (-1, 1):
        s.box((x, y + sy * 1.3 * k, z - 0.15), (3.2 * k, 0.45, 0.35), t)


def emblem_bolt(s, c, t="A", k=1.0):
    x, y, z = c
    s.box((x - 0.3 * k, y + 0.8 * k, z), (0.8 * k, 2.0 * k, 0.4), t, (0, 0, -25))
    s.box((x, y, z), (1.6 * k, 0.5 * k, 0.4), t, (0, 0, -10))
    s.box((x + 0.4 * k, y - 0.9 * k, z), (0.8 * k, 2.0 * k, 0.4), t, (0, 0, -25))


def emblem_disc(s, c, t="A", k=1.0, inner=None):
    x, y, z = c
    s.cyl((x, y, z), (2.6 * k, 0.4, 2.6 * k), t, (90, 0, 0))
    if inner:
        s.cyl((x, y, z - 0.12), (1.4 * k, 0.4, 1.4 * k), inner, (90, 0, 0))


def fist_glow(s, hand, t="C", r=1.0):
    x, y, z = hand
    s.ell((x, y, z), (2.6 * r, 2.6 * r, 2.6 * r), t)


def ring_on(s, hand, t="C"):
    """A lantern ring on a fist (right hand by convention)."""
    x, y, z = hand
    s.box((x, y + 0.2, z - 1.3), (1.4, 0.8, 0.5), t)


def tail(s, pts, sizes, t="P", first_id=8, parent=0, kind="ell"):
    pid = parent
    for i in range(len(pts) - 1):
        nid = first_id + i
        s.part(nid, pid, pts[i], "Tail")
        s.on(nid)
        a, b = pts[i], pts[i + 1]
        if kind == "seg":
            s.seg(a, b, sizes[i][0], sizes[i][1], t)
        else:
            c = tuple((a[k] + b[k]) / 2 for k in range(3))
            s.ell(c, sizes[i], t)
        pid = nid
    s.on(0)


def brute(s, bulk=1.4, skin="P", leg_h=7.5, torso=(10.0, 10.0, 6.4), arm_len=10.5, hunch=True, arm_spread=10):
    """Hulking brute body (Doomsday, Grundy, Killer Croc, Bane, Allen): huge torso and arms, short legs."""
    k = bulk
    b = humanoid(s, leg_h=leg_h, leg_w=3.2 * k, leg_d=3.4 * k, hip_x=2.2 * k, foot=(3.4 * k, 1.5, 4.4),
                 torso=(torso[0] * k / 1.4, torso[1], torso[2] * k / 1.4), torso_t=skin, torso_kind="v",
                 arm_w=3.2 * k, arm_len=arm_len, arm_x=(torso[0] * k / 1.4) / 2 + 1.6 * k, hand=3.4 * k,
                 arm_t=skin, leg_t=skin, hand_t=skin, foot_t=skin, neck=1.8, arm_spread=arm_spread)
    if hunch:
        s.ell((0, b["y_sh"] - 1.4, b["td"] * 0.38), (b["tw"] * 0.86, 3.4, b["td"] * 0.8), skin)   # trapezius hump
    return b
