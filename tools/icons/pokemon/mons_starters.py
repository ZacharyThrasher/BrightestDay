"""Starter lines (Kanto..Sinnoh) + a few Kanto favourites for the Pokemon icon pack.

Authored at ~2x in-game resolution: height in units ~= 16 * max(scale, 0.9). Creatures face -Z, feet
centre at origin, left = -X. Every limb's prims live on that limb's part so the rig moves them as a unit.
"""
import math

from plib import S, icon, tail_chain, ALONG_Z, ALONG_X, TIP_NZ, TIP_PZ, TIP_NY, TIP_NX, TIP_PX
from plib import HEAD, JAW, TAIL, LFL, LFR, LBL, LBR, ARM_L, ARM_R, WING_L, WING_R, OTHER

# accents (sRGB 0..1)
CREAM = (0.98, 0.88, 0.6)
PINK = (1.0, 0.5, 0.62)
TAN = (0.82, 0.66, 0.42)
WHITE = (0.94, 0.95, 1.0)
LEAF = (0.35, 0.8, 0.3)
RED = (0.92, 0.22, 0.16)
YELLOW = (1.0, 0.82, 0.2)
ORANGE = (1.0, 0.55, 0.12)
BROWN = (0.62, 0.44, 0.26)
GOLD = (0.95, 0.78, 0.3)
GREY = (0.62, 0.64, 0.68)


# ------------------------------------------------------------------------------------------- helpers
def unit(a, b):
    d = [b[i] - a[i] for i in range(3)]
    n = math.sqrt(sum(v * v for v in d)) or 1.0
    return [v / n for v in d]


def along(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def brows(s, x, y, z, part, w=2.0, ang=20, t='D'):
    """Angry brows: inner ends lower."""
    for sx in (-1, 1):
        s.box((sx * x, y, z), (w, 0.7, 0.9), t, (0, 0, sx * ang), part=part)


def leg(s, pid, role, hip, ankle, w, d, t='P', foot=None, foot_t=None, claws=None, claw_t='D'):
    """Leg part pivoting at the hip: thigh ellipsoid hip->ankle, foot ellipsoid (w, h, len) pointing -Z."""
    s.part(pid, 0, hip, role)
    s.seg(hip, ankle, w, d, t, part=pid, kind='Ellipsoid')
    if foot:
        fw, fh, fl = foot
        fc = (ankle[0], fh / 2, ankle[2] - fl / 2 + d / 2)
        s.ell(fc, (fw, fh, fl), foot_t or t, part=pid)
        if claws:
            n = claws
            for k in range(n):
                cx = ankle[0] + (k - (n - 1) / 2) * fw / (n + 0.5)
                s.box((cx, 0.5, fc[2] - fl / 2 - 0.1), (0.7, 1.0, 1.2), claw_t, part=pid)
    return s


def legs2(s, x, hip_y, z, w, d, foot, t='P', foot_t=None, claws=None, ids=(4, 5), claw_t='D', ankle_y=None):
    for pid, sx, role in ((ids[0], -1, LBL), (ids[1], 1, LBR)):
        ay = ankle_y if ankle_y is not None else foot[1] * 0.8
        leg(s, pid, role, (sx * x, hip_y, z), (sx * x, ay, z), w, d, t, foot, foot_t, claws, claw_t)
    return s


def legs4(s, xs, zf, zb, top, w, d, foot, t='P', foot_t=None, claws=None, ids=(4, 5, 6, 7), claw_t='D'):
    roles = (LFL, LFR, LBL, LBR)
    pos = ((-xs, zf), (xs, zf), (-xs, zb), (xs, zb))
    for pid, role, (x, z) in zip(ids, roles, pos):
        leg(s, pid, role, (x, top, z), (x, foot[1] * 0.8, z), w, d, t, foot, foot_t, claws, claw_t)
    return s


def arm(s, pid, role, sh, hand, w, t='P', hand_t=None, hand_s=None, claws=None, claw_t='D'):
    s.part(pid, 0, sh, role)
    s.seg(sh, hand, w, w, t, part=pid, kind='Ellipsoid')
    hs = hand_s or w + 0.6
    s.ell(hand, (hs, hs, hs), hand_t or t, part=pid)
    if claws:
        u = unit(sh, hand)
        tip = tuple(hand[i] + u[i] * hs * 0.5 for i in range(3))
        for k in range(claws):
            cx = (k - (claws - 1) / 2) * hs * 0.35
            s.box((tip[0] + cx, tip[1], tip[2] - 0.3), (0.7, 1.0, 0.9), claw_t, part=pid)
    return s


def arms2(s, x, y, z, hand_off, w, t='P', hand_t=None, hand_s=None, claws=None, ids=(8, 9), claw_t='D'):
    """hand_off = (outward, down, forward) offset of the hand from the shoulder."""
    ox, oy, oz = hand_off
    for pid, sx, role in ((ids[0], -1, ARM_L), (ids[1], 1, ARM_R)):
        arm(s, pid, role, (sx * x, y, z), (sx * (x + ox), y + oy, z + oz), w, t, hand_t, hand_s, claws, claw_t)
    return s


def flame(s, base, h, w, part=0, t='C'):
    """Upward flame: bulb + tongue."""
    x, y, z = base
    s.ell((x, y + h * 0.25, z), (w, h * 0.5, w), t, part=part)
    s.cone((x, y + h * 0.6, z), (w * 0.9, h * 0.8, w * 0.9), t, part=part)
    return s


def tail(s, start_id, pts, sizes, t='P', kind='Ellipsoid'):
    return tail_chain(s, start_id, 0, pts, sizes, t, kind)


# =================================================================================== Kanto starters
def ivysaur():
    s = S()
    s.ell((0, 5.5, 0.8), (8.4, 6.2, 10.0), 'P')                              # squat body
    for sx in (-1, 1):
        s.ell((sx * 3.6, 6.4, 2.0), (1.6, 2.0, 2.4), 'D')                     # spots
    legs4(s, 2.8, -2.2, 3.6, 5.0, 2.6, 2.6, (3.0, 1.6, 3.4), claws=3)
    s.part(1, 0, (0, 6.4, -3.2), HEAD)
    s.ell((0, 8.0, -5.6), (8.0, 6.2, 6.6), 'P', part=1)                      # broad head
    s.ell((0, 6.8, -7.8), (6.0, 3.0, 3.2), 'P', part=1)                      # muzzle
    for sx in (-1, 1):
        s.spike((sx * 2.8, 10.4, -4.8), (sx * 3.8, 12.8, -4.0), 2.2, 'P', part=1, d=1.4)   # pointed ears
        s.ell((sx * 2.0, 8.8, -8.7), (1.8, 1.6, 0.9), 'C', part=1)            # eye whites (red in canon)
        s.box((sx * 1.9, 8.8, -9.2), (0.8, 1.1, 0.5), 'D', part=1)            # pupil
    brows(s, 2.0, 9.9, -8.6, 1, 2.0, 18)
    s.part(2, 1, (0, 6.4, -5.6), JAW)
    s.ell((0, 5.8, -7.6), (5.4, 1.6, 3.2), 'P', part=2)
    s.box((0, 6.6, -9.2), (3.6, 0.6, 0.8), 'D', part=1)                      # mouth line
    # the bud: four broad leaves cradling a pink bulb
    s.cyl((0, 9.0, 1.6), (3.4, 2.0, 3.4), 'D')                                # stalk
    for k in range(4):
        a = math.radians(45 + k * 90)
        s.seg((0, 9.6, 1.6), (math.cos(a) * 7.4, 7.6, 1.6 + math.sin(a) * 7.4), 1.4, 5.0, 'P', kind='Ellipsoid')
    s.ell((0, 11.8, 1.6), (7.0, 5.8, 7.0), 'A')                               # plump bud
    s.cone((0, 15.2, 1.6), (3.6, 2.6, 3.6), 'A')
    for k in range(3):
        a = math.radians(30 + k * 120)
        s.seg((0, 10.4, 1.6), (math.cos(a) * 2.2, 14.4, 1.6 + math.sin(a) * 2.2), 0.8, 0.8, 'D')   # petal seams
    tail(s, 20, [(0, 5.0, 5.4), (0, 4.4, 7.0)], [(1.6, 1.6)])
    return s


def venusaur():
    s = S()
    s.ell((0, 10.0, 1.0), (17.0, 11.0, 19.0), 'P')                           # massive toad body
    for sx in (-1, 1):
        s.ell((sx * 7.6, 10.6, 3.0), (2.2, 3.0, 3.6), 'D')
        s.ell((sx * 6.8, 11.4, -3.6), (2.0, 2.6, 2.6), 'D')
    legs4(s, 6.2, -5.2, 6.4, 8.5, 4.8, 4.8, (5.4, 2.4, 5.6), claws=3)
    s.part(1, 0, (0, 11.0, -7.0), HEAD)
    s.ell((0, 12.4, -11.6), (12.0, 8.4, 9.0), 'P', part=1)
    s.ell((0, 10.6, -14.6), (9.4, 4.4, 4.0), 'P', part=1)                    # heavy muzzle
    for sx in (-1, 1):
        s.spike((sx * 4.2, 15.6, -10.6), (sx * 5.6, 18.4, -9.8), 3.0, 'P', part=1, d=2.0)
        s.ell((sx * 2.8, 13.6, -15.6), (2.4, 1.8, 1.0), 'C', part=1)
        s.box((sx * 2.6, 13.5, -16.2), (1.0, 1.3, 0.6), 'D', part=1)
    brows(s, 2.8, 15.0, -15.6, 1, 3.0, 18)
    s.box((0, 10.4, -16.6), (7.0, 0.8, 1.0), 'D', part=1)                    # wide mouth
    s.part(2, 1, (0, 10.0, -9.0), JAW)
    s.ell((0, 9.0, -13.0), (9.0, 2.4, 6.6), 'P', part=2)
    # palm trunk, drooping fronds, and the giant flower
    s.cyl((0, 17.0, 2.4), (5.6, 6.0, 5.6), 'D')
    for k in range(4):
        a = math.radians(45 + k * 90)
        tip = (math.cos(a) * 11.0, 13.0, 2.4 + math.sin(a) * 11.0)
        s.seg((0, 19.0, 2.4), tip, 1.6, 6.0, 'P', kind='Ellipsoid')           # fronds
        s.seg((0, 19.4, 2.4), along((0, 19.4, 2.4), tip, 0.8), 0.9, 1.0, 'D')  # midrib
    for k in range(5):
        a = math.radians(-90 + 36 + k * 72)
        tip = (math.cos(a) * 11.0, 19.6, 2.4 + math.sin(a) * 11.0)
        s.seg((0, 21.6, 2.4), tip, 2.2, 7.0, 'A', kind='Ellipsoid')           # petals
        m = along((0, 21.6, 2.4), tip, 0.62)
        s.ell((m[0], m[1] + 0.8, m[2]), (1.6, 1.2, 1.6), 'C')                # white petal spots
    s.ell((0, 22.4, 2.4), (4.6, 2.6, 4.6), 'C')                               # glowing centre (solar beam)
    tail(s, 20, [(0, 8.0, 10.0), (0, 6.6, 12.0)], [(3.0, 3.0)])
    return s


def charmander():
    s = S()
    s.ell((0, 5.2, 0.4), (6.0, 6.4, 5.4), 'P')                               # pear body
    s.ell((0, 4.8, -1.3), (4.4, 5.0, 3.2), 'A')                              # cream belly
    legs2(s, 1.9, 3.4, 0.4, 2.8, 3.0, (2.9, 1.5, 4.0), ankle_y=1.2)
    arms2(s, 2.7, 7.0, -0.6, (0.9, -2.6, -1.6), 1.8)
    s.part(1, 0, (0, 8.0, -0.4), HEAD)
    s.ell((0, 11.0, -0.8), (7.2, 6.4, 6.4), 'P', part=1)                     # big round head
    s.ell((0, 10.1, -3.4), (5.0, 3.6, 3.8), 'P', part=1)                     # muzzle
    s.eyes(1.8, 11.7, -3.7, 1, w=1.4, h=2.1, d=1.2)
    s.box((0, 9.5, -5.2), (3.4, 0.6, 0.8), 'D', part=1)                      # grin
    s.part(2, 1, (0, 9.4, -2.0), JAW)
    s.ell((0, 8.9, -3.2), (4.0, 1.6, 3.0), 'P', part=2)
    ids = tail(s, 20, [(0, 3.4, 2.4), (0, 2.8, 5.8), (0, 4.4, 8.4)], [(3.4, 3.4), (3.0, 3.0)])
    s.part(30, ids[-1], (0, 4.2, 8.4), TAIL)
    flame(s, (0, 4.4, 8.6), 5.0, 3.2, part=30)                               # the tail flame
    return s


def charmeleon():
    s = S()
    s.ell((0, 7.2, 0.4), (6.0, 7.6, 5.2), 'P')
    s.ell((0, 6.8, -1.3), (4.2, 6.2, 3.0), 'A')
    legs2(s, 2.0, 5.0, 0.6, 2.8, 3.2, (2.8, 1.5, 4.6), claws=3, ankle_y=1.2)
    arms2(s, 2.8, 9.4, -0.6, (1.2, -3.4, -2.0), 1.7, claws=3)
    s.part(1, 0, (0, 10.6, -0.4), HEAD)
    s.ell((0, 13.8, -0.6), (6.4, 5.6, 6.0), 'P', part=1)
    s.ell((0, 12.9, -4.0), (4.6, 3.4, 4.6), 'P', part=1)                     # longer snout
    s.spike((0, 15.4, 1.4), (0, 16.8, 5.2), 2.4, 'P', part=1, d=2.0)         # back-swept horn
    s.eyes(1.7, 14.3, -3.2, 1, w=1.2, h=1.7, d=1.2)
    brows(s, 1.7, 15.3, -3.3, 1, 2.0, 22)
    s.box((0, 12.4, -6.0), (3.2, 0.6, 1.0), 'D', part=1)
    s.part(2, 1, (0, 12.2, -2.2), JAW)
    s.ell((0, 11.7, -3.8), (3.8, 1.6, 4.0), 'P', part=2)
    for sx in (-1, 1):
        s.box((sx * 1.1, 12.4, -5.6), (0.6, 0.9, 0.6), 'C', part=2)          # fangs
    ids = tail(s, 20, [(0, 4.8, 2.4), (0, 3.4, 6.6), (0, 4.6, 10.4)], [(3.4, 3.4), (3.0, 3.0)])
    s.part(30, ids[-1], (0, 4.4, 10.4), TAIL)
    flame(s, (0, 4.6, 10.6), 6.0, 3.6, part=30)
    return s


def wartortle():
    s = S()
    s.ell((0, 6.4, 0.2), (7.4, 7.6, 6.4), 'P')                               # body
    s.ell((0, 6.8, 2.8), (8.6, 8.8, 4.8), 'D')                               # shell back
    s.ell((0, 6.8, 1.8), (9.2, 9.4, 1.6), 'A')                               # pale shell rim
    s.ell((0, 6.0, -2.4), (5.6, 6.6, 2.6), 'A')                              # plastron
    for y in (4.8, 7.2):
        s.box((0, y, -3.6), (4.4, 0.5, 0.8), 'D')
    legs2(s, 2.4, 3.8, 0.4, 2.8, 3.0, (3.0, 1.5, 3.8), ankle_y=1.2)
    arms2(s, 3.4, 8.4, -0.6, (1.2, -3.0, -1.6), 1.8)
    s.part(1, 0, (0, 10.0, -0.4), HEAD)
    s.ell((0, 12.6, -1.0), (6.6, 5.6, 6.2), 'P', part=1)
    s.eyes(1.6, 13.2, -3.8, 1, w=1.2, h=1.8, d=1.0)
    brows(s, 1.7, 14.2, -3.8, 1, 1.8, 16)
    s.box((0, 11.3, -4.0), (2.6, 0.6, 0.8), 'D', part=1)
    for sx in (-1, 1):                                                       # fluffy swept-back ears
        s.seg((sx * 2.8, 14.2, 0.2), (sx * 4.8, 17.2, 3.6), 1.8, 3.2, 'A', part=1, kind='Ellipsoid')
        s.ell((sx * 4.9, 16.8, 3.8), (2.2, 2.6, 2.6), 'A', part=1)
    # big curling fluffy tail
    ids = tail(s, 20, [(0, 3.6, 3.4), (0, 3.8, 6.6), (0, 7.0, 8.8)], [(3.4, 3.4), (4.2, 4.2)], t='A')
    s.part(30, ids[-1], (0, 7.0, 8.8), TAIL)
    s.ell((0, 9.4, 8.4), (3.8, 4.4, 4.0), 'A', part=30)
    s.ell((0, 10.0, 7.0), (2.6, 2.6, 2.6), 'A', part=30)
    return s


def blastoise():
    s = S()
    s.ell((0, 11.0, 1.0), (13.0, 14.0, 11.0), 'P')                           # torso
    s.ell((0, 11.2, 4.4), (15.4, 16.0, 9.0), 'A')                            # shell back
    s.ell((0, 11.2, 2.6), (16.2, 16.8, 2.2), 'D')                            # dark shell rim
    for y in (8.0, 14.0):
        s.box((0, y, 8.8), (10, 0.7, 1.0), 'D')                              # back plates
    s.box((0, 11.0, 8.8), (0.7, 12, 1.0), 'D')
    s.ell((0, 10.0, -3.6), (10.0, 12.0, 4.4), 'A')                           # plastron
    for y in (7.6, 11.4):
        s.box((0, y, -5.6), (8.4, 0.6, 1.0), 'D')
    s.box((0, 9.6, -5.6), (0.6, 9.4, 1.0), 'D')
    legs2(s, 3.8, 6.8, 0.6, 5.0, 5.4, (5.4, 2.0, 6.6), claws=3, ankle_y=1.6)
    arms2(s, 6.0, 14.4, -0.4, (2.2, -5.6, -2.2), 3.8, hand_s=4.4, claws=3)
    s.part(1, 0, (0, 17.4, -1.0), HEAD)
    s.ell((0, 20.2, -2.4), (8.4, 7.2, 8.4), 'P', part=1)
    for sx in (-1, 1):
        s.spike((sx * 2.8, 22.8, -1.2), (sx * 3.4, 25.0, -0.6), 1.8, 'P', part=1)   # little ears
    s.eyes(1.9, 20.8, -6.1, 1, w=1.3, h=1.7, d=1.2)
    brows(s, 1.9, 21.9, -6.0, 1, 2.2, 20)
    s.box((0, 18.8, -6.4), (4.2, 0.7, 1.0), 'D', part=1)
    s.part(2, 1, (0, 18.2, -3.0), JAW)
    s.ell((0, 17.6, -4.6), (5.8, 1.8, 4.2), 'P', part=2)
    # the shoulder water cannons
    for sx in (-1, 1):
        a, b = (sx * 5.6, 16.4, 3.6), (sx * 5.8, 18.0, -9.6)
        u = unit(a, b)
        s.ell((sx * 5.6, 16.8, 3.4), (5.2, 4.6, 5.4), 'A')                   # shell hump the cannon rises from
        s.seg(a, b, 4.4, 4.4, 'D', kind='Cylinder')
        s.seg(tuple(b[i] - u[i] * 2.2 for i in range(3)), tuple(b[i] + u[i] * 0.3 for i in range(3)), 5.4, 5.4, 'D', kind='Cylinder')
        s.seg(tuple(b[i] - u[i] * 0.8 for i in range(3)), tuple(b[i] + u[i] * 0.6 for i in range(3)), 2.8, 2.8, 'C', kind='Cylinder')
    tail(s, 20, [(0, 5.4, 5.8), (0, 3.4, 9.0)], [(3.0, 3.0)])
    return s


# =================================================================================== Johto starters
def chikorita():
    s = S()
    s.ell((0, 4.0, 1.2), (5.0, 4.4, 6.6), 'P')                               # little body
    legs4(s, 1.6, -0.6, 3.2, 3.2, 1.9, 1.9, (2.3, 1.2, 2.6))
    s.part(1, 0, (0, 5.2, -1.4), HEAD)
    s.ell((0, 7.4, -2.8), (6.4, 6.0, 6.0), 'P', part=1)                      # big round head
    s.ell((0, 6.4, -5.0), (4.2, 2.8, 2.6), 'P', part=1)
    s.eyes(1.6, 7.8, -5.5, 1, w=1.3, h=2.0, d=1.0)
    s.box((0, 5.9, -6.2), (1.8, 0.5, 0.6), 'D', part=1)
    for k in range(7):                                                       # ring of neck buds
        a = math.radians(-90 + 30 + k * 40)
        s.ell((math.cos(a) * 2.9, 5.0, -1.4 + math.sin(a) * 2.9), (1.6, 1.6, 1.6), 'A', part=1)
    # the big head leaf, broad face-on, arching back
    s.seg((0, 9.8, -3.0), (0, 11.2, -2.8), 0.8, 0.8, 'D', part=1)
    s.seg((0, 10.8, -2.8), (0, 16.0, -0.2), 1.2, 4.6, 'A', part=1, kind='Ellipsoid')
    s.seg((0, 15.4, -0.4), (0, 16.6, 1.6), 1.0, 2.2, 'A', part=1, kind='Ellipsoid')   # curled tip
    s.seg((0, 11.0, -3.3), (0, 15.4, -1.0), 0.6, 0.6, 'D', part=1)           # midrib
    tail(s, 20, [(0, 3.8, 4.2), (0, 4.4, 5.6)], [(1.4, 1.4)])
    return s


def meganium():
    s = S()
    s.ell((0, 9.0, 2.6), (10.0, 9.0, 14.0), 'P')                             # sauropod body
    legs4(s, 3.4, -1.6, 6.6, 7.6, 3.2, 3.2, (3.8, 1.8, 4.2))
    s.part(1, 0, (0, 11.6, -2.4), HEAD)
    s.seg((0, 11.0, -1.6), (0, 20.4, -4.6), 3.8, 3.8, 'P', part=1, kind='Ellipsoid')   # long neck
    s.ell((0, 21.6, -5.6), (6.0, 5.2, 6.4), 'P', part=1)
    s.ell((0, 20.6, -8.4), (4.2, 3.2, 3.2), 'P', part=1)
    s.eyes(1.6, 22.2, -8.2, 1, w=1.2, h=1.6, d=1.0)
    s.box((0, 20.0, -9.9), (2.0, 0.5, 0.6), 'D', part=1)
    for sx in (-1, 1):                                                       # antennae
        s.seg((sx * 1.2, 23.6, -5.4), (sx * 2.2, 28.0, -4.2), 1.1, 1.1, 'P', part=1)
        s.ell((sx * 2.3, 28.2, -4.1), (1.6, 1.6, 1.6), 'A', part=1)
    # the petal ruff around the base of the neck (body-fixed so it frames the neck)
    c = (0, 13.0, -1.4)
    for k in range(9):
        a = math.radians(-90 + k * 40)
        tip = (math.cos(a) * 8.2, 14.6, c[2] + math.sin(a) * 8.2)
        s.seg(c, tip, 1.5, 4.8, 'A', kind='Ellipsoid')
    for k in range(6):
        a = math.radians(-90 + 30 + k * 60)
        s.ell((math.cos(a) * 3.4, 14.4, c[2] + math.sin(a) * 3.4), (1.4, 1.6, 1.4), 'C')   # yellow stamens
    tail(s, 20, [(0, 8.4, 9.0), (0, 7.2, 11.6)], [(2.4, 2.4)])
    return s


def cyndaquil():
    s = S()
    s.ell((0, 4.2, 1.0), (5.4, 4.6, 9.0), 'P')                               # long low body, dark back
    s.ell((0, 3.3, 0.6), (4.8, 3.2, 8.0), 'A')                               # cream underside
    legs4(s, 1.8, -1.6, 3.6, 2.8, 1.7, 1.7, (2.1, 1.0, 2.4), t='A')
    s.part(1, 0, (0, 4.8, -2.6), HEAD)
    s.ell((0, 5.6, -4.4), (4.4, 4.0, 4.8), 'P', part=1)
    s.ell((0, 4.8, -4.8), (4.2, 2.6, 4.4), 'A', part=1)                      # cream face
    s.cone((0, 5.0, -7.8), (2.8, 3.8, 2.4), 'A', TIP_NZ, part=1)             # pointed snout
    s.box((0, 5.0, -9.4), (0.8, 0.8, 0.8), 'D', part=1)
    for sx in (-1, 1):
        s.box((sx * 1.3, 6.0, -6.2), (1.5, 0.5, 0.6), 'D', (0, 0, sx * 12), part=1)   # sleepy closed eyes
    # back flames (Core) erupting up and back
    for x, z, h in ((-1.2, 1.2, 3.4), (1.2, 1.2, 3.4), (0, 3.4, 3.8)):
        s.spike((x, 5.8, z), (x * 1.6, 5.8 + h, z + 2.2), 2.4, 'C')
    return s


def typhlosion():
    s = S()
    s.ell((0, 10.2, 1.0), (10.0, 12.0, 8.6), 'P')                            # hunched body
    s.ell((0, 9.4, -1.8), (7.6, 9.6, 4.4), 'A')                              # cream belly
    legs2(s, 3.0, 5.6, 1.0, 3.8, 4.0, (4.0, 1.8, 5.4), claws=3, ankle_y=1.6)
    arms2(s, 4.4, 13.0, -1.0, (1.4, -4.2, -2.8), 2.4, hand_s=2.8, claws=3)
    s.part(1, 0, (0, 15.2, -1.4), HEAD)
    s.ell((0, 17.2, -3.2), (5.4, 4.6, 6.4), 'P', part=1)
    s.ell((0, 16.4, -6.6), (3.8, 3.2, 4.4), 'P', part=1)                     # long snout
    s.ell((0, 15.8, -5.4), (4.2, 2.4, 5.6), 'A', part=1)                     # cream jaw line
    for sx in (-1, 1):
        s.box((sx * 1.5, 18.0, -6.0), (1.8, 0.6, 0.8), 'D', (0, 0, sx * 16), part=1)  # narrow eyes
    s.box((0, 16.9, -8.8), (0.9, 0.8, 0.6), 'D', part=1)
    s.part(2, 1, (0, 15.6, -4.0), JAW)
    s.ell((0, 15.0, -6.2), (3.2, 1.4, 4.0), 'A', part=2)
    # the volcanic flame collar: flames fan out around the neck and blaze up behind the head
    for k in range(9):
        a = math.radians(-20 + k * 27.5)                                    # 0 deg = +X, around the back
        bx, bz = math.cos(a) * 3.8, 0.4 + math.sin(a) * 3.2
        tx, tz = math.cos(a) * 7.4, 1.6 + math.sin(a) * 5.4
        s.spike((bx, 15.0, bz), (tx, 22.6 - abs(math.cos(a)) * 2.6, tz), 3.2, 'C')
    for sx in (-1, 1):                                                       # fire from the hips too
        s.spike((sx * 1.4, 7.2, 4.6), (sx * 2.0, 10.4, 8.2), 2.6, 'C')
    tail(s, 20, [(0, 5.8, 4.6), (0, 4.4, 7.4)], [(2.6, 2.6)])
    return s


def totodile():
    s = S()
    s.ell((0, 4.8, 0.4), (5.4, 5.6, 4.8), 'P')
    legs2(s, 1.7, 3.0, 0.4, 2.4, 2.6, (2.6, 1.3, 3.4), ankle_y=1.0)
    arms2(s, 2.4, 6.2, -0.6, (1.2, -2.2, -1.4), 1.5)
    s.part(1, 0, (0, 7.0, -0.4), HEAD)
    s.ell((0, 9.8, -0.6), (6.4, 5.2, 6.0), 'P', part=1)                      # huge head
    s.ell((0, 9.0, -4.0), (5.4, 3.0, 5.6), 'P', part=1)                      # upper jaw
    s.ell((0, 8.0, -3.6), (4.6, 1.4, 4.8), 'D', part=1)                      # open maw
    for sx in (-1, 1):
        s.box((sx * 1.9, 7.8, -5.4), (0.7, 1.0, 0.7), 'C', part=1)          # fangs
    s.eyes(1.7, 11.0, -3.0, 1, w=1.4, h=2.0, d=1.2)
    s.part(2, 1, (0, 8.0, -1.4), JAW)
    s.ell((0, 7.0, -3.6), (4.8, 1.8, 4.8), 'P', part=2)
    s.box((0, 7.8, -5.6), (2.6, 0.7, 0.7), 'C', part=2)
    s.spike((0, 11.8, 1.2), (0, 13.8, 3.2), 2.2, 'A', part=1, d=1.2)         # red crest
    for y, z in ((8.6, 2.2), (6.0, 2.8)):
        s.spike((0, y, z), (0, y + 1.0, z + 2.2), 2.2, 'A', d=1.2)          # back spikes
    ids = tail(s, 20, [(0, 3.4, 2.4), (0, 2.8, 5.4), (0, 3.2, 7.8)], [(2.4, 2.4), (1.8, 1.8)])
    s.spike((0, 3.6, 5.0), (0, 5.4, 6.4), 2.0, 'A', part=ids[0], d=1.0)
    s.spike((0, 3.6, 7.4), (0, 5.2, 8.8), 1.8, 'A', part=ids[1], d=1.0)
    return s


def feraligatr():
    s = S()
    s.ell((0, 15.0, 1.0), (12.0, 16.0, 9.0), 'P')                            # tall torso
    for y in (10.0, 13.0, 16.0):
        s.box((0, y, -3.6), (7.0, 0.8, 1.2), 'D')                            # belly bands
    legs2(s, 3.8, 8.4, 1.6, 4.6, 5.0, (5.2, 2.2, 7.4), claws=3, ankle_y=2.0)
    arms2(s, 5.8, 19.0, -0.6, (2.6, -7.6, -3.8), 3.4, hand_s=3.8, claws=3)
    s.part(1, 0, (0, 22.0, -1.0), HEAD)
    s.ell((0, 25.6, -2.4), (8.4, 6.2, 8.4), 'P', part=1)
    s.ell((0, 25.0, -8.0), (7.0, 3.6, 10.0), 'P', part=1)                    # long upper jaw
    s.ell((0, 23.4, -7.6), (6.2, 1.6, 9.0), 'D', part=1)                     # gaping maw
    for sx in (-1, 1):
        for z in (-6.0, -9.0, -11.6):
            s.box((sx * (2.6 + (z + 6) * 0.12), 23.2, z), (0.8, 1.2, 0.8), 'C', part=1)   # upper teeth
        s.ell((sx * 2.6, 27.4, -5.4), (1.6, 1.4, 1.0), 'C', part=1)          # eyes
        s.box((sx * 2.6, 27.3, -6.0), (0.6, 1.2, 0.5), 'D', part=1)
    brows(s, 2.6, 28.4, -5.6, 1, 2.6, 20)
    s.part(2, 1, (0, 23.6, -2.6), JAW)
    s.ell((0, 21.8, -7.4), (6.6, 2.6, 9.4), 'P', part=2)
    for sx in (-1, 1):
        for z in (-7.4, -10.4):
            s.box((sx * 2.0, 23.0, z), (0.8, 1.2, 0.8), 'C', part=2)
    for y, z in ((29.0, -0.6), (27.4, 2.2)):                                 # red head crest
        s.spike((0, y, z), (0, y + 3.2, z + 2.6), 3.2, 'A', part=1, d=1.4)
    for y, z in ((20.6, 4.8), (16.6, 5.4), (12.6, 5.2)):                     # back spikes
        s.spike((0, y, z), (0, y + 1.4, z + 3.8), 3.4, 'A', d=1.6)
    ids = tail(s, 20, [(0, 9.0, 4.6), (0, 5.4, 10.0), (0, 3.4, 15.6)], [(5.2, 5.2), (3.8, 3.8)])
    s.spike((0, 8.6, 8.4), (0, 11.0, 10.4), 2.8, 'A', part=ids[0], d=1.4)
    s.spike((0, 5.6, 13.6), (0, 7.6, 15.6), 2.4, 'A', part=ids[1], d=1.2)
    return s


# =================================================================================== Hoenn starters
# seg() sizing rule used below: for a segment in the sagittal (YZ) plane, `w` is its in-plane breadth and
# `h` its lateral (X) thickness — so fins/leaf tails seen broad from the side use a big w, small h.
def treecko():
    s = S()
    s.ell((0, 4.6, 0.4), (4.8, 5.2, 4.2), 'P')
    s.ell((0, 4.2, -1.2), (3.4, 3.8, 2.4), 'A')                              # red belly
    legs2(s, 1.5, 2.8, 0.4, 1.9, 2.0, (2.8, 1.2, 3.0), ankle_y=1.0)
    arms2(s, 2.2, 6.0, -0.4, (1.2, -2.2, -1.4), 1.4, hand_s=2.0)
    s.part(1, 0, (0, 6.8, -0.2), HEAD)
    s.ell((0, 9.2, -0.8), (6.4, 4.6, 5.6), 'P', part=1)                      # wide flat head
    s.ell((0, 8.4, -3.2), (4.4, 2.8, 3.2), 'P', part=1)
    s.spike((0, 10.6, 0.4), (0, 11.4, 3.2), 2.4, 'P', part=1, d=1.4)         # pointed crown
    for sx in (-1, 1):
        s.ell((sx * 2.2, 9.8, -2.6), (2.0, 2.4, 1.6), 'C', part=1)           # big yellow eyes
        s.box((sx * 2.3, 9.8, -3.4), (0.6, 1.6, 0.5), 'D', part=1)            # slit pupils
    s.box((0, 7.9, -4.6), (2.6, 0.5, 0.6), 'D', part=1)
    # huge dark leaf-tail curling up behind
    ids = tail(s, 20, [(0, 3.4, 2.2), (0, 4.6, 5.0), (0, 9.0, 6.4)], [(2.6, 1.8), (4.8, 1.8)], t='D')
    s.part(30, ids[-1], (0, 9.0, 6.4), TAIL)
    s.seg((0, 9.0, 6.4), (0, 12.4, 5.2), 4.2, 1.6, 'D', part=30, kind='Ellipsoid')
    return s


def sceptile():
    s = S()
    s.ell((0, 12.2, 0.6), (7.6, 9.6, 6.2), 'P')                              # lean torso
    s.ell((0, 13.0, -1.8), (5.4, 5.8, 2.8), 'A')                             # red chest
    for sx in (-1, 1):
        for y in (16.0, 12.8, 9.6):
            s.ell((sx * 1.6, y, 3.4), (2.4, 2.4, 2.2), 'C')                  # glowing seed pods on the back
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):                          # big thighs, thin shins, talons
        x = sx * 2.8
        s.part(pid, 0, (x, 9.4, 0.8), role)
        s.ell((x, 7.6, 0.8), (3.8, 5.6, 4.4), 'P', part=pid)
        s.seg((x, 5.4, 1.4), (x, 1.2, 1.2), 2.8, 3.0, 'P', part=pid, kind='Ellipsoid')
        s.ell((x, 0.8, -0.8), (3.6, 1.8, 5.0), 'P', part=pid)
        for k in (-1, 0, 1):
            s.box((x + k * 1.0, 0.5, -3.4), (0.7, 1.0, 1.2), 'D', part=pid)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                      # arms with the leaf blades
        sh, hand = (sx * 4.0, 15.4, -0.4), (sx * 5.8, 9.6, -2.8)
        arm(s, pid, role, sh, hand, 2.8, hand_s=3.0, claws=3)
        for t0, ln in ((0.4, 7.4), (0.7, 6.0)):
            b = along(sh, hand, t0)
            s.spike(b, (b[0] + sx * 2.0, b[1] + 3.0, b[2] + ln), 3.6, 'D', part=pid, d=2.2)
    s.part(1, 0, (0, 17.0, -0.6), HEAD)
    s.ell((0, 19.4, -1.4), (5.0, 4.2, 6.4), 'P', part=1)
    s.cone((0, 18.8, -5.4), (3.2, 3.6, 2.6), 'P', TIP_NZ, part=1)            # pointed snout
    s.spike((0, 20.8, 0.4), (0, 21.8, 5.2), 2.8, 'P', part=1, d=1.4)         # back-swept head frond
    for sx in (-1, 1):
        s.ell((sx * 1.7, 20.0, -4.0), (1.6, 1.4, 1.0), 'C', part=1)
        s.box((sx * 1.8, 20.0, -4.5), (0.5, 1.1, 0.5), 'D', part=1)
    brows(s, 1.7, 20.9, -4.1, 1, 1.8, 18)
    s.box((0, 18.2, -6.2), (2.0, 0.5, 0.6), 'D', part=1)
    # long drooping frond tail with leaflets
    ids = tail(s, 20, [(0, 9.4, 3.2), (0, 7.4, 8.0), (0, 5.2, 13.0)], [(3.2, 3.2), (2.6, 2.6)], t='D')
    for i, pid in enumerate(ids):
        z0 = 5.0 + i * 5.0
        for sx in (-1, 1):
            s.spike((0, 8.2 - i * 2.2, z0), (sx * 3.4, 5.4 - i * 2.2, z0 + 1.8), 3.0, 'D', part=pid, d=2.0)
    return s


def torchic():
    s = S()
    s.ell((0, 5.8, 0.4), (7.6, 7.0, 7.2), 'P')                               # round chick body
    legs2(s, 1.5, 3.0, 0.4, 1.7, 1.7, (2.6, 1.0, 3.0), t='A', ankle_y=0.8)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                      # stubby wings
        s.part(pid, 0, (sx * 3.4, 7.0, 0.2), role)
        s.ell((sx * 4.0, 5.8, 0.6), (1.8, 3.4, 3.2), 'A', part=pid)
    s.part(1, 0, (0, 8.2, -0.2), HEAD)
    s.ell((0, 10.8, -0.4), (6.8, 6.0, 6.4), 'P', part=1)
    s.cone((0, 10.0, -4.2), (2.8, 2.6, 2.0), 'A', TIP_NZ, part=1)            # beak
    s.box((0, 10.0, -4.0), (2.8, 0.4, 2.0), 'D', part=1)
    s.eyes(1.8, 11.2, -3.2, 1, w=1.3, h=1.9, d=1.0)
    for base, tip in (((0, 13.2, -0.6), (0, 16.0, -2.2)), ((-1.3, 13.2, 0.2), (-2.4, 15.6, 1.2)),
                      ((1.3, 13.2, 0.2), (2.4, 15.6, 1.2))):
        s.spike(base, tip, 1.8, 'A', part=1)                                 # three-feather crest
    tail(s, 20, [(0, 6.0, 3.8), (0, 7.4, 5.4)], [(2.2, 2.2)])
    return s


def blaziken():
    s = S()
    s.ell((0, 17.0, 0.4), (7.6, 8.4, 5.6), 'P')                              # torso
    s.ell((0, 12.8, 0.6), (6.4, 4.0, 5.0), 'A')                              # feathered hips
    for sx in (-1, 1):
        s.box((sx * 1.3, 17.2, -2.6), (1.4, 5.6, 1.0), 'A', (0, 0, sx * 24))  # chest V
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        x = sx * 2.6
        s.part(pid, 0, (x, 12.0, 0.8), role)
        s.ell((x, 9.6, 0.8), (4.6, 6.4, 5.0), 'A', part=pid)                 # big feathered thighs
        s.seg((x, 7.0, 1.2), (x, 1.6, 1.6), 3.0, 3.0, 'P', part=pid, kind='Ellipsoid')
        s.ell((x, 0.8, -0.4), (3.2, 1.6, 4.6), 'D', part=pid)
        for k in (-1, 0, 1):
            s.box((x + k * 1.0, 0.5, -2.8), (0.7, 1.0, 1.4), 'D', part=pid)  # talons
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh, hand = (sx * 4.2, 19.6, -0.4), (sx * 5.8, 13.2, -2.8)
        arm(s, pid, role, sh, hand, 2.8, hand_s=3.0, claws=3)
        s.spike(along(sh, hand, 0.75), (sx * 7.0, 10.6, 2.2), 3.4, 'C', part=pid)   # wrist flames
    s.part(1, 0, (0, 21.2, -0.6), HEAD)
    s.ell((0, 23.4, -1.0), (4.6, 4.6, 5.0), 'P', part=1)
    s.cone((0, 22.6, -4.4), (2.2, 2.6, 1.8), 'A', TIP_NZ, part=1)            # beak
    for sx in (-1, 1):
        s.box((sx * 1.4, 24.0, -3.4), (1.4, 0.8, 0.8), 'D', (0, 0, sx * 18), part=1)
        s.spike((sx * 1.0, 25.2, -1.4), (sx * 2.4, 29.0, -0.4), 1.8, 'P', part=1)   # horn feathers
    s.seg((0, 24.6, 1.0), (0, 15.6, 5.0), 4.8, 3.6, 'A', part=1, kind='Ellipsoid')    # long cream mane
    s.seg((0, 23.4, 1.6), (0, 19.0, 4.8), 4.6, 5.0, 'A', part=1, kind='Ellipsoid')
    return s


def mudkip():
    s = S()
    s.ell((0, 4.0, 1.4), (5.6, 4.8, 7.4), 'P')
    legs4(s, 2.0, -0.8, 3.6, 3.2, 1.9, 1.9, (2.4, 1.2, 2.6))
    s.part(1, 0, (0, 5.0, -1.6), HEAD)
    s.ell((0, 6.8, -3.0), (7.0, 5.6, 6.0), 'P', part=1)                      # big round head
    s.box((0, 5.6, -5.9), (3.4, 0.5, 0.6), 'D', part=1)                      # smile
    s.eyes(2.0, 7.6, -5.3, 1, w=1.0, h=1.4, d=1.0)
    s.seg((0, 8.6, -3.8), (0, 13.6, -2.2), 4.4, 1.2, 'P', part=1, kind='Ellipsoid')   # tall head fin
    s.seg((0, 13.0, -2.4), (0, 13.6, -2.2), 1.6, 1.4, 'D', part=1)
    for sx in (-1, 1):                                                       # orange cheek gills
        for dy, dz in ((1.2, 0.4), (0.0, 0.8), (-1.2, 0.4)):
            s.spike((sx * 3.0, 6.4, -2.6), (sx * 5.4, 6.4 + dy, -2.6 + dz), 1.4, 'A', part=1)
    ids = tail(s, 20, [(0, 4.4, 4.6), (0, 5.4, 8.6)], [(4.8, 1.2)])          # fan tail fin
    s.seg((0, 5.6, 7.6), (0, 7.2, 9.4), 2.4, 1.0, 'D', part=ids[0])
    return s


def swampert():
    s = S()
    s.ell((0, 9.0, 0.8), (11.0, 11.0, 8.0), 'P')                             # barrel body
    legs2(s, 3.4, 5.0, 0.8, 4.0, 4.4, (4.6, 1.8, 5.4), claws=3, ankle_y=1.4)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                      # huge forearms
        sh, hand = (sx * 5.8, 12.4, -0.2), (sx * 8.2, 5.4, -2.6)
        s.part(pid, 0, sh, role)
        s.seg(sh, along(sh, hand, 0.45), 3.2, 3.2, 'P', part=pid, kind='Ellipsoid')
        s.seg(along(sh, hand, 0.35), hand, 4.6, 4.6, 'P', part=pid, kind='Ellipsoid')
        s.ell(hand, (4.6, 3.4, 4.6), 'P', part=pid)
        for k in (-1, 0, 1):
            s.box((hand[0] + k * 1.2, hand[1] - 1.6, hand[2] - 1.4), (0.8, 1.4, 0.8), 'D', part=pid)
    s.part(1, 0, (0, 14.2, -0.6), HEAD)
    s.ell((0, 16.2, -1.8), (7.6, 5.2, 7.2), 'P', part=1)                     # broad flat head
    s.box((0, 15.0, -5.4), (4.6, 0.6, 1.0), 'D', part=1)
    s.eyes(2.2, 17.2, -4.8, 1, w=1.2, h=1.2, d=1.0)
    brows(s, 2.2, 18.0, -4.9, 1, 2.0, 18)
    s.seg((0, 18.2, -2.2), (0, 22.8, 1.0), 4.6, 1.4, 'D', part=1, kind='Ellipsoid')   # black head fin
    for sx in (-1, 1):                                                       # big orange gills
        for dy, dz in ((2.0, 0.8), (0.2, 1.2), (-1.6, 0.6)):
            s.spike((sx * 3.4, 16.4, -1.4), (sx * 7.0, 16.4 + dy, -1.4 + dz), 2.0, 'A', part=1)
    tail(s, 20, [(0, 6.4, 4.4), (0, 7.2, 8.8)], [(5.4, 1.4)], t='D')
    return s


# =================================================================================== Sinnoh starters
def turtwig():
    s = S()
    s.ell((0, 3.8, 1.2), (5.4, 4.0, 7.0), 'P')
    s.ell((0, 5.4, 1.8), (6.4, 3.6, 6.8), 'A')                               # brown shell
    s.ell((0, 4.6, 1.8), (6.9, 1.2, 7.3), 'D')                               # shell rim
    legs4(s, 2.2, -1.0, 3.8, 3.0, 2.0, 2.0, (2.4, 1.2, 2.6))
    s.part(1, 0, (0, 4.6, -1.8), HEAD)
    s.ell((0, 5.8, -4.0), (6.0, 5.0, 5.8), 'P', part=1)                      # big head
    s.ell((0, 4.6, -4.6), (5.0, 2.2, 4.6), 'A', part=1)                      # yellow-brown jaw
    s.box((0, 5.4, -6.9), (3.0, 0.5, 0.6), 'D', part=1)
    s.eyes(1.7, 6.6, -6.5, 1, w=1.1, h=1.5, d=1.0)
    s.seg((0, 7.8, -4.0), (0, 10.0, -4.0), 1.0, 1.0, 'D', part=1)             # sprout stem
    for sx in (-1, 1):
        s.seg((0, 9.8, -4.0), (sx * 3.0, 11.2, -4.2), 1.2, 2.4, 'P', part=1, kind='Ellipsoid')   # twin leaves
    tail(s, 20, [(0, 3.6, 4.4), (0, 3.4, 5.8)], [(1.4, 1.4)])
    return s


def torterra():
    s = S()
    s.ell((0, 9.5, 1.4), (15.0, 10.0, 20.0), 'P')                            # massive body
    s.ell((0, 13.4, 2.4), (17.0, 8.0, 20.0), 'D')                            # earthen shell
    legs4(s, 6.0, -5.2, 8.0, 9.0, 5.2, 5.2, (6.0, 2.4, 6.4), claws=3)
    s.part(1, 0, (0, 10.6, -7.4), HEAD)
    s.ell((0, 11.8, -11.8), (8.4, 7.0, 8.4), 'P', part=1)
    s.ell((0, 10.4, -14.4), (6.6, 3.6, 4.4), 'P', part=1)                    # beaky muzzle
    s.box((0, 10.2, -16.6), (5.0, 0.7, 0.8), 'D', part=1)
    for sx in (-1, 1):
        s.ell((sx * 2.4, 13.2, -15.4), (1.4, 1.2, 0.8), 'C', part=1)
        s.box((sx * 2.3, 13.1, -15.8), (0.6, 0.9, 0.5), 'D', part=1)
        s.spike((sx * 3.6, 12.0, -10.4), (sx * 7.2, 12.8, -9.2), 2.6, 'D', part=1)   # rocky cheek spikes
    brows(s, 2.4, 14.2, -15.3, 1, 2.6, 20)
    # the living landscape on its back: a tree and three rock mountains with snowy peaks
    s.cyl((0, 19.4, -1.6), (2.8, 7.0, 2.8), 'D')                              # trunk
    s.ell((0, 24.2, -1.6), (10.0, 5.0, 9.0), 'A')                            # canopy
    s.ell((-2.8, 22.6, 0.8), (5.4, 3.6, 5.4), 'A')
    s.ell((2.8, 22.6, -3.6), (5.4, 3.6, 5.4), 'A')
    for base, tip, w in (((-4.8, 15.0, 5.0), (-5.2, 24.4, 5.8), 6.4), ((4.8, 15.0, 5.0), (5.2, 22.8, 5.8), 6.0),
                         ((0, 15.4, 9.4), (0, 21.6, 10.4), 5.6)):
        s.spike(base, tip, w, 'D')
        s.spike(along(base, tip, 0.72), tip, w * 0.34, 'C')                  # snow caps
    tail(s, 20, [(0, 8.4, 11.0), (0, 7.2, 13.4)], [(2.6, 2.6)])
    return s


def chimchar():
    s = S()
    s.ell((0, 4.8, 0.4), (4.8, 5.2, 4.2), 'P')
    s.ell((0, 4.6, -1.0), (3.4, 3.8, 2.6), 'A')                              # cream belly
    legs2(s, 1.5, 2.8, 0.4, 1.7, 1.8, (2.4, 1.1, 2.8), foot_t='A', ankle_y=0.8)
    arms2(s, 2.2, 6.2, -0.4, (1.6, -2.4, -1.0), 1.3, hand_t='A', hand_s=1.8)
    s.part(1, 0, (0, 7.0, -0.2), HEAD)
    s.ell((0, 9.6, -0.4), (6.0, 5.6, 5.6), 'P', part=1)
    s.ell((0, 9.2, -2.2), (4.6, 4.0, 2.8), 'A', part=1)                      # cream face
    for sx in (-1, 1):
        s.ell((sx * 3.5, 9.8, -0.2), (1.6, 2.6, 2.2), 'A', part=1)           # round ears
        s.ell((sx * 1.2, 10.0, -3.4), (1.1, 1.7, 0.8), 'D', part=1)
        s.box((sx * 1.2, 10.4, -3.8), (0.5, 0.5, 0.5), 'C', part=1)
    s.box((0, 8.2, -3.5), (1.8, 0.5, 0.6), 'D', part=1)
    s.spike((0, 12.0, -0.8), (0, 13.6, 1.2), 1.8, 'D', part=1)               # head swirl
    s.ell((0, 5.0, 2.4), (2.8, 2.8, 2.4), 'C')                               # the rump flame
    s.spike((0, 5.4, 2.8), (0, 10.2, 5.6), 3.2, 'C')
    tail(s, 20, [(0, 3.4, 2.0), (0, 2.8, 4.2)], [(1.2, 1.2)])
    return s


def infernape():
    s = S()
    s.ell((0, 11.0, 0.4), (8.0, 7.8, 5.8), 'P')                               # broad chest
    s.ell((0, 8.0, 0.6), (5.6, 3.6, 4.6), 'P')                               # waist
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        x = sx * 2.2
        leg(s, pid, role, (x, 7.4, 0.6), (x, 1.0, 0.8), 3.0, 3.2, 'P', (3.2, 1.4, 4.0))
        s.ell((x, 4.4, -0.6), (2.6, 2.2, 1.6), 'A', part=pid)                # gold knee pads
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        sh, hand = (sx * 4.2, 13.2, -0.2), (sx * 6.4, 7.8, -2.0)
        arm(s, pid, role, sh, hand, 2.6, hand_s=2.8)
        s.ell((sx * 4.2, 13.4, -0.2), (3.0, 2.4, 3.0), 'A', part=pid)        # gold shoulder guards
        w = along(sh, hand, 0.8)
        s.ell(w, (2.8, 1.4, 2.8), 'A', part=pid)                             # gold wrist bands
    s.part(1, 0, (0, 14.6, -0.4), HEAD)
    s.ell((0, 16.8, -0.8), (5.6, 5.2, 5.2), 'P', part=1)
    s.ell((0, 16.0, -3.0), (3.6, 2.6, 2.0), 'P', part=1)
    for sx in (-1, 1):
        s.ell((sx * 1.4, 17.4, -3.2), (1.0, 1.3, 0.8), 'D', part=1)
        s.ell((sx * 2.6, 16.6, -2.6), (1.0, 1.6, 1.0), 'A', part=1)          # gold face marks
        s.ell((sx * 3.0, 17.2, -0.4), (1.2, 1.6, 1.2), 'A', part=1)          # ears
    brows(s, 1.4, 18.4, -3.2, 1, 1.6, 22)
    s.box((0, 15.4, -4.0), (2.0, 0.5, 0.6), 'D', part=1)
    for x, z, h in ((0, -0.6, 7.2), (-1.6, 0.4, 5.6), (1.6, 0.4, 5.6), (0, 1.6, 6.0)):
        s.spike((x, 18.6, z), (x * 1.8, 18.6 + h, z + 2.4), 3.0, 'C', part=1)   # flame crown
    tail(s, 20, [(0, 8.4, 2.6), (0, 7.2, 6.0), (0, 9.4, 8.8)], [(1.6, 1.6), (1.4, 1.4)])
    return s


def piplup():
    s = S()
    s.ell((0, 5.0, 0.2), (6.4, 7.2, 5.8), 'P')                               # penguin body
    for sx in (-1, 1):
        s.ell((sx * 1.2, 5.6, -2.6), (1.6, 1.6, 1.0), 'A')                   # white chest spots
    legs2(s, 1.6, 2.2, 0.2, 1.8, 1.8, (2.6, 1.0, 3.2), foot_t='D', ankle_y=0.6)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                      # flippers
        s.part(pid, 0, (sx * 3.0, 6.8, 0.2), role)
        s.ell((sx * 3.8, 5.0, 0.0), (1.8, 5.0, 3.0), 'D', (0, 0, sx * 24), part=pid)
    s.part(1, 0, (0, 8.2, -0.2), HEAD)
    s.ell((0, 10.8, -0.4), (6.4, 6.0, 6.0), 'D', part=1)                     # dark-blue head
    s.ell((0, 10.0, -2.2), (5.2, 4.2, 2.6), 'A', part=1)                     # white face mask
    s.eyes(1.4, 10.8, -3.4, 1, w=1.0, h=1.4, d=0.8)
    s.cone((0, 9.6, -3.9), (2.6, 2.4, 1.6), 'D', TIP_NZ, part=1)             # beak
    s.spike((0, 13.4, 0.0), (0, 14.6, 1.6), 1.6, 'D', part=1)                # little crest
    tail(s, 20, [(0, 2.6, 2.6), (0, 1.8, 4.2)], [(2.0, 1.4)])
    return s


def empoleon():
    s = S()
    s.ell((0, 10.8, 0.4), (10.0, 15.0, 8.0), 'P')                            # tall emperor body
    s.ell((0, 10.0, -2.2), (6.4, 10.0, 3.6), 'D')                            # dark shirt front
    for y in (13.2, 10.6, 8.0):
        s.ell((0, y, -3.9), (1.4, 1.4, 0.8), 'C')                            # white 'buttons'
    legs2(s, 2.6, 4.4, 0.6, 3.0, 3.2, (3.8, 1.4, 4.6), foot_t='A', claws=3, ankle_y=1.2)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                      # steel flipper blades
        sh, tip = (sx * 4.8, 15.2, 0.0), (sx * 8.0, 5.4, -1.2)
        s.part(pid, 0, sh, role)
        s.seg(sh, tip, 4.0, 1.4, 'P', part=pid, kind='Ellipsoid')
        s.seg((sh[0] + sx * 1.6, sh[1] - 1.0, sh[2]), (tip[0] + sx * 0.8, tip[1], tip[2]), 1.0, 1.6, 'C', part=pid)
        s.spike((sx * 5.0, 15.8, 0.0), (sx * 7.2, 18.0, 0.6), 2.0, 'A', part=pid)   # shoulder spurs
    s.part(1, 0, (0, 17.6, -0.4), HEAD)
    s.ell((0, 20.4, -0.8), (6.2, 5.6, 6.0), 'P', part=1)
    s.cone((0, 19.8, -4.6), (2.8, 3.8, 2.2), 'A', TIP_NZ, part=1)            # gold beak
    s.eyes(1.8, 21.0, -3.3, 1, w=1.0, h=1.2, d=0.8)
    brows(s, 1.8, 21.8, -3.4, 1, 1.8, 16)
    # the golden trident crest rising from the beak
    s.seg((0, 20.6, -3.8), (0, 27.4, -2.4), 1.8, 1.6, 'A', part=1)
    s.spike((0, 27.0, -2.4), (0, 29.2, -2.0), 2.2, 'A', part=1)
    for sx in (-1, 1):
        s.seg((0, 22.8, -3.4), (sx * 3.2, 24.6, -2.8), 1.6, 1.6, 'A', part=1)
        s.seg((sx * 3.2, 24.2, -2.8), (sx * 3.4, 27.2, -2.4), 1.6, 1.6, 'A', part=1)
        s.spike((sx * 3.4, 26.8, -2.4), (sx * 3.5, 28.6, -2.2), 2.0, 'A', part=1)
    for k, sx in enumerate((-1, 1)):                                         # tailcoat tails
        tail_chain(s, 20 + k * 2, 0, [(sx * 1.4, 6.8, 3.6), (sx * 1.8, 3.0, 6.0), (sx * 2.0, 1.2, 7.4)],
                   [(2.6, 1.6), (2.0, 1.4)], 'D', 'Ellipsoid')
    return s


# =================================================================================== Kanto favourites
# Wing rule: a segment running mostly along X has w = its vertical thickness and h = its Z chord
# (horizontal wings); for a wing that should face the camera (butterfly) swap them.
def pidgeot():
    s = S()
    s.seg((0, 6.2, 3.4), (0, 10.4, -1.8), 6.4, 6.0, 'P', kind='Ellipsoid')  # chest-up body
    s.ell((0, 7.6, -1.0), (4.6, 4.6, 3.6), 'P')
    s.part(1, 0, (0, 10.6, -1.8), HEAD)
    s.ell((0, 12.4, -3.4), (4.6, 4.4, 5.0), 'P', part=1)
    s.cone((0, 11.8, -6.8), (1.8, 3.0, 1.6), 'D', TIP_NZ, part=1)            # hooked beak
    for sx in (-1, 1):
        s.box((sx * 2.0, 12.8, -4.4), (0.8, 1.0, 2.6), 'D', part=1)          # black eye streak
        s.box((sx * 2.1, 13.0, -5.0), (0.6, 0.6, 0.6), 'C', part=1)
    s.seg((0, 13.4, -3.8), (0, 16.6, 2.4), 3.6, 1.8, 'A', part=1, kind='Ellipsoid')   # the long crest plume
    s.seg((0, 16.4, 2.0), (0, 15.4, 7.8), 3.0, 1.4, 'A', part=1, kind='Ellipsoid')
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):                   # great spread wings
        root = (sx * 2.4, 9.6, 0.6)
        s.part(pid, 0, root, role)
        s.seg(root, (sx * 9.0, 12.4, 1.4), 2.0, 7.0, 'P', part=pid, kind='Ellipsoid')
        s.seg((sx * 7.6, 12.0, 1.4), (sx * 15.0, 13.4, 3.0), 1.6, 5.4, 'P', part=pid, kind='Ellipsoid')
        for k in range(3):
            s.spike((sx * (11.0 + k * 1.4), 12.8, 2.6), (sx * (13.4 + k * 1.6), 12.6, 7.4 - k * 0.8), 1.8, 'D', part=pid, d=1.0)
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):                         # talons
        leg(s, pid, role, (sx * 1.6, 7.0, 1.0), (sx * 1.6, 1.4, 0.8), 2.8, 2.8, 'P', (3.0, 1.4, 3.4), 'D', claws=3)
    ids = tail(s, 20, [(0, 6.0, 2.6), (0, 4.8, 11.6)], [(1.6, 6.0)], t='A')  # fanned tail feathers
    s.seg((0, 4.8, 10.6), (0, 4.6, 12.6), 1.4, 6.4, 'D', part=ids[0], kind='Ellipsoid')
    return s


def beedrill():
    s = S()
    s.ell((0, 10.0, 0.2), (4.2, 4.6, 4.4), 'P')                              # thorax
    s.part(1, 0, (0, 11.4, -1.6), HEAD)
    s.ell((0, 12.6, -3.0), (4.4, 4.0, 4.0), 'P', part=1)
    for sx in (-1, 1):
        s.ell((sx * 1.6, 13.0, -4.2), (2.0, 2.6, 1.8), 'A', part=1)          # big red eyes
        s.seg((sx * 0.8, 14.2, -3.4), (sx * 2.0, 17.4, -2.2), 1.2, 1.2, 'D', part=1)   # antennae
    s.box((0, 11.4, -5.0), (1.8, 0.8, 0.8), 'D', part=1)
    ids = tail(s, 20, [(0, 9.0, 1.8), (0, 3.8, 7.6)], [(5.4, 5.4)])          # striped abdomen
    a, b = (0, 9.0, 1.8), (0, 3.8, 7.6)
    for t0 in (0.38, 0.66):
        s.seg(along(a, b, t0 - 0.06), along(a, b, t0 + 0.06), 5.8, 5.8, 'D', part=ids[0], kind='Ellipsoid')
    s.spike(along(a, b, 0.9), (0, 1.6, 10.4), 2.4, 'C', part=ids[0])        # tail stinger
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                     # the twin drill arms
        sh, el = (sx * 2.0, 10.4, -0.6), (sx * 3.6, 8.0, -3.2)
        s.part(pid, 0, sh, role)
        s.seg(sh, el, 2.0, 2.0, 'P', part=pid, kind='Ellipsoid')
        s.spike(el, (sx * 4.0, 7.4, -10.4), 3.2, 'C', part=pid)
        s.cyl(el, (2.6, 1.0, 2.6), 'D', ALONG_Z, part=pid)
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        leg(s, pid, role, (sx * 1.4, 8.4, 0.8), (sx * 2.0, 5.0, 0.6), 1.4, 1.4, 'D')
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):                 # two pairs of glassy wings
        root = (sx * 1.4, 11.8, 1.2)
        s.part(pid, 0, root, role)
        s.seg(root, (sx * 8.0, 15.4, 4.2), 0.9, 4.2, 'C', part=pid, kind='Ellipsoid')
        s.seg((sx * 1.4, 11.0, 1.8), (sx * 6.4, 11.8, 5.6), 0.9, 3.2, 'C', part=pid, kind='Ellipsoid')
    return s


def butterfree():
    s = S()
    s.ell((0, 9.0, 0.6), (3.6, 4.4, 3.6), 'P')                               # thorax
    s.ell((0, 6.4, 1.4), (2.8, 3.8, 2.8), 'P')                               # abdomen
    s.part(1, 0, (0, 10.6, -0.4), HEAD)
    s.ell((0, 12.0, -0.8), (4.8, 4.2, 4.2), 'P', part=1)
    for sx in (-1, 1):
        s.ell((sx * 1.7, 12.4, -2.2), (2.4, 3.0, 2.2), 'D', part=1)          # big compound eyes
        s.box((sx * 1.6, 13.2, -3.3), (0.6, 0.6, 0.5), 'C', part=1)
        s.seg((sx * 0.8, 13.8, -1.2), (sx * 2.4, 17.4, -0.4), 1.0, 1.0, 'D', part=1)   # antennae
        s.ell((sx * 2.5, 17.6, -0.4), (1.6, 1.6, 1.6), 'D', part=1)
    s.box((0, 10.8, -2.8), (1.6, 0.6, 0.6), 'D', part=1)
    arms2(s, 1.6, 9.2, -0.8, (0.8, -1.6, -1.2), 1.4)                        # little blue hands
    legs2(s, 1.0, 5.8, 1.0, 1.4, 1.4, (1.8, 1.0, 1.8), ankle_y=3.2)
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):                 # big white wings, black edges
        root = (sx * 1.4, 10.2, 1.4)
        s.part(pid, 0, root, role)
        up, lo = (sx * 10.0, 15.8, 2.6), (sx * 7.4, 4.4, 2.6)
        s.seg(root, up, 7.2, 1.2, 'A', part=pid, kind='Ellipsoid')
        s.seg((sx * 1.4, 9.4, 1.6), lo, 5.8, 1.2, 'A', part=pid, kind='Ellipsoid')
        s.ell((up[0] - sx * 1.6, up[1] - 0.8, up[2]), (3.2, 3.2, 1.4), 'D', part=pid)   # black wingtips
        s.ell((lo[0] - sx * 1.2, lo[1] + 0.8, lo[2]), (2.6, 2.6, 1.4), 'D', part=pid)
        s.seg(root, along(root, up, 0.8), 0.8, 1.4, 'D', part=pid)           # veins
    return s


def nidoking():
    s = S()
    s.ell((0, 10.0, 0.8), (10.0, 11.0, 8.4), 'P')                            # hulking torso
    s.ell((0, 9.4, -2.2), (7.0, 9.0, 4.0), 'A')                              # cream belly
    for y in (7.4, 10.4):
        s.box((0, y, -4.1), (5.4, 0.7, 0.8), 'D')
    legs2(s, 3.4, 6.2, 1.2, 4.2, 4.6, (4.8, 1.8, 6.0), claws=3, ankle_y=1.6)
    arms2(s, 5.0, 13.0, -1.2, (1.8, -4.6, -2.8), 2.8, hand_s=3.0, claws=3)
    s.part(1, 0, (0, 14.6, -2.4), HEAD)
    s.ell((0, 16.6, -4.2), (5.6, 5.0, 6.4), 'P', part=1)
    s.ell((0, 15.8, -7.4), (4.4, 3.2, 4.0), 'P', part=1)                     # snout
    s.spike((0, 18.2, -5.8), (0, 23.8, -6.8), 2.8, 'C', part=1)              # the great horn
    for sx in (-1, 1):
        s.spike((sx * 2.4, 18.8, -2.8), (sx * 4.0, 22.4, -1.4), 2.6, 'P', part=1, d=1.4)   # tall ears
        s.ell((sx * 1.6, 17.4, -7.0), (1.2, 1.0, 0.8), 'C', part=1)
        s.box((sx * 1.6, 17.4, -7.4), (0.6, 0.8, 0.5), 'D', part=1)
    brows(s, 1.6, 18.2, -7.0, 1, 1.8, 24)
    s.part(2, 1, (0, 15.2, -5.0), JAW)
    s.ell((0, 14.4, -7.4), (3.8, 1.6, 3.8), 'P', part=2)
    for sx in (-1, 1):
        s.box((sx * 1.2, 15.2, -8.8), (0.7, 1.0, 0.7), 'C', part=2)          # fangs
    for base, tip in (((0, 16.6, 1.6), (0, 19.6, 4.2)), ((0, 13.4, 3.8), (0, 15.6, 7.2)),
                      ((-2.4, 12.0, 3.6), (-3.2, 13.6, 6.8)), ((2.4, 12.0, 3.6), (3.2, 13.6, 6.8))):
        s.spike(base, tip, 2.8, 'P')                                        # back spines
    tail(s, 20, [(0, 6.6, 4.2), (0, 4.2, 9.4), (0, 2.6, 14.4)], [(4.4, 4.4), (3.2, 3.2)])
    return s


# ========================================================================================== register
def register():
    icon(["ivysaur", "ivysar", "ivysuar", "ivy saur"], "Ivysaur", "Quadruped", ["Pull", "Blade"], 1.0, 13.0,
         element="Earth", accent=PINK, temperament="Calm", intensity=0.4, shape=ivysaur())
    icon(["venusaur", "venasaur", "venusuar", "venosaur"], "Venusaur", "Quadruped", ["Beam", "Heavy", "Pull"], 2.0, 100.0,
         element="Earth", accent=PINK, temperament="Calm", intensity=0.6, shape=venusaur())
    icon(["charmander", "charmandar", "charmender", "char mander"], "Charmander", "Biped", ["Breath", "ArticulatedTail"],
         0.6, 8.5, element="Fire", accent=CREAM, temperament="Playful", intensity=0.5, shape=charmander())
    icon(["charmeleon", "charmelion", "charmilion", "charmileon"], "Charmeleon", "Biped",
         ["Breath", "Blade", "ArticulatedTail"], 1.1, 19.0, element="Fire", accent=CREAM, temperament="Aggressive",
         intensity=0.6, shape=charmeleon())
    icon(["wartortle", "wartortel", "war tortle", "wartotle"], "Wartortle", "Biped", ["Guard", "Shoots", "ArticulatedTail"],
         1.0, 22.5, element="Water", accent=WHITE, temperament="Playful", intensity=0.4, shape=wartortle())
    icon(["blastoise", "blastoys", "blastiose", "blastoice"], "Blastoise", "Biped", ["Shoots", "Guard", "Heavy"],
         1.6, 85.5, element="Water", accent=TAN, temperament="Protective", intensity=0.6, shape=blastoise())
    icon(["chikorita", "chickorita", "chikorida", "chicorita"], "Chikorita", "Quadruped", ["Blade", "Pull"], 0.9, 6.4,
         element="Earth", accent=LEAF, temperament="Playful", intensity=0.5, shape=chikorita())
    icon(["meganium", "meganeum", "meganuim", "megaanium"], "Meganium", "Quadruped", ["Blade", "Stomp", "Guard"], 1.8, 100.5,
         element="Earth", accent=PINK, temperament="Protective", intensity=0.6, shape=meganium())
    icon(["cyndaquil", "cindaquil", "cyndaquill", "cyndequil"], "Cyndaquil", "Quadruped", ["Breath", "Fast"], 0.5, 7.9,
         element="Fire", accent=CREAM, temperament="Fearful", intensity=0.4, shape=cyndaquil())
    icon(["typhlosion", "typhlosian", "typhosion", "typholsion"], "Typhlosion", "Biped", ["Breath", "Charge"], 1.7, 79.5,
         element="Fire", accent=CREAM, temperament="Aggressive", intensity=0.5, shape=typhlosion())
    icon(["totodile", "totodial", "totadile", "toto dile"], "Totodile", "Biped", ["Grab", "Fast"], 0.6, 9.5,
         element="Water", accent=RED, temperament="Playful", intensity=0.8, shape=totodile())
    icon(["feraligatr", "feraligator", "feraligater", "ferraligatr"], "Feraligatr", "Biped", ["Grab", "ArticulatedTail", "Heavy"],
         2.3, 88.8, element="Water", accent=RED, temperament="Aggressive", intensity=0.8, shape=feraligatr())
    icon(["treecko", "treeko", "treecco", "trecko"], "Treecko", "Biped", ["Fast", "ArticulatedTail"], 0.5, 5.0,
         element="Earth", accent=RED, temperament="Sneaky", intensity=0.5, shape=treecko())
    icon(["sceptile", "sceptil", "septile", "sceptyle"], "Sceptile", "Biped", ["Blade", "Fast", "ArticulatedTail"], 1.7, 52.2,
         element="Earth", accent=RED, temperament="Sneaky", intensity=0.6, shape=sceptile())
    icon(["torchic", "torchick", "torchik", "tor chic"], "Torchic", "Biped", ["Breath", "Fast"], 0.4, 2.5,
         element="Fire", accent=YELLOW, temperament="Playful", intensity=0.7, shape=torchic())
    icon(["blaziken", "blazikan", "blazekin", "blaziken pokemon"], "Blaziken", "Biped", ["Charge", "Blunt", "Fast"], 1.9, 52.0,
         element="Fire", accent=CREAM, temperament="Aggressive", intensity=0.7, shape=blaziken())
    icon(["mudkip", "mudkipz", "mud kip", "mudkipp"], "Mudkip", "Quadruped", ["Shoots", "Fast"], 0.4, 7.6,
         element="Water", accent=ORANGE, temperament="Playful", intensity=0.6, shape=mudkip())
    icon(["swampert", "swamper", "swampurt", "swampert pokemon"], "Swampert", "Biped", ["Stomp", "Heavy", "Blunt"], 1.5, 81.9,
         element="Water", accent=ORANGE, temperament="Calm", intensity=0.5, shape=swampert())
    icon(["turtwig", "turtwigg", "turtwik", "tortwig"], "Turtwig", "Quadruped", ["Guard", "Blade"], 0.4, 10.2,
         element="Earth", accent=BROWN, temperament="Calm", intensity=0.4, shape=turtwig())
    icon(["torterra", "toterra", "torterrah", "tortera"], "Torterra", "Quadruped", ["Heavy", "Stomp", "Guard"], 2.2, 310.0,
         element="Earth", accent=LEAF, temperament="Calm", intensity=0.6, shape=torterra())
    icon(["chimchar", "chimchaar", "chim char", "chimcar"], "Chimchar", "Biped", ["Fast", "Breath"], 0.5, 6.2,
         element="Fire", accent=CREAM, temperament="Playful", intensity=0.8, shape=chimchar())
    icon(["infernape", "infernap", "inferape", "infernapee"], "Infernape", "Biped", ["Blunt", "Charge", "Fast"], 1.2, 55.0,
         element="Fire", accent=GOLD, temperament="Aggressive", intensity=0.6, shape=infernape())
    icon(["piplup", "pip lup", "piplupp", "piplop"], "Piplup", "Biped", ["Shoots", "Fast"], 0.4, 5.2,
         element="Water", accent=WHITE, temperament="Playful", intensity=0.5, shape=piplup())
    icon(["empoleon", "empolean", "empoleonn", "emperoleon"], "Empoleon", "Biped", ["Blade", "Charge"], 1.7, 84.5,
         element="Water", accent=GOLD, temperament="Protective", intensity=0.5, shape=empoleon())
    # Kanto favourites
    icon(["pidgeot", "pigeot", "pidgot", "pidgeott"], "Pidgeot", "Bird", ["Flies", "Fast", "Repel"],
         2.0, 39.5, element="Wind", accent=(0.95, 0.38, 0.2), temperament="Aggressive", intensity=0.4, shape=pidgeot())
    icon(["beedrill", "beedril", "bee drill", "beadrill"], "Beedrill", "Insect", ["Swarm", "Pierce", "Flies"], 1.0, 29.5,
         element="Poison", count=3, accent=RED, temperament="Aggressive", intensity=0.9, shape=beedrill())
    icon(["butterfree", "butterfree pokemon", "buterfree", "butter free"], "Butterfree", "Insect", ["Flies", "Beam"], 1.3, 32.0,
         element="Psychic", accent=WHITE, temperament="Calm", intensity=0.4, shape=butterfree())
    icon(["nidoking", "nidokng", "nido king", "nidokin"], "Nidoking", "Biped", ["Pierce", "Stomp", "ArticulatedTail"], 1.4, 62.0,
         element="Poison", accent=CREAM, temperament="Aggressive", intensity=0.8, shape=nidoking())
