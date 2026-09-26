"""Creatures, mechs, vehicles. Feet centre at origin, facing -Z, left = -X."""
import math
from shapes import S, humanoid, tail_chain, fist, ALONG_Z, ALONG_X, TIP_NZ, TIP_PZ, TIP_NY, TIP_PX, TIP_NX


def shenron():
    s = S()
    # head (raised, front) + jaw
    s.part(1, 0, (0, 8, -4), 'Head')
    s.vb(-1, 1, 7, 9, -8, -4, 'P', part=1)               # skull
    s.vb(-1, 1, 7, 8, -11, -9, 'P', part=1)              # snout
    s.vb(-1, 1, 9, 9, -7, -6, 'C', part=1)               # red eyes
    s.box((0, 9, -7), (3, 1, 1), 'C', part=1)
    for sx in (-1, 1):
        s.seg((sx * 1, 10, -5), (sx * 2, 13, -2), 1, 1, 'A', part=1)     # antler horns
        s.seg((sx * 2, 12, -3), (sx * 3, 13, -4), 1, 1, 'A', part=1)
        s.seg((sx * 1, 7, -11), (sx * 5, 6, -12), 1, 1, 'C', part=1)     # whiskers
    s.part(2, 1, (0, 7, -6), 'Jaw')
    s.vb(-1, 1, 6, 6, -11, -6, 'D', part=2)
    # neck + coiled body as a tail chain (serpent gait)
    pts = [(0, 7, -3), (0, 6, 0), (1, 4, 3), (2, 2, 6), (1, 1, 9), (-1, 2, 12), (-2, 3, 15), (-1, 3, 18), (0, 2, 21)]
    sizes = [(3, 1, 3.4), (3.4, 1, 3.4), (3.2, 1, 3), (3, 1, 3), (2.8, 1, 2.8), (2.6, 1, 2.4), (2, 1, 2), (1.5, 1, 1.5)]
    s.part(3, 0, pts[0], 'Tail')
    pid = 3
    s.seg(pts[0], pts[1], sizes[0][0], sizes[0][2], 'P', part=3, kind='Cylinder')
    for i in range(1, len(pts) - 1):
        nid = 3 + i
        s.part(nid, pid, pts[i], 'Tail')
        s.seg(pts[i], pts[i + 1], sizes[i][0], sizes[i][2], 'P', part=nid, kind='Cylinder')
        s.cone((pts[i][0], pts[i][1] + 1.8, pts[i][2]), (1, 2, 1), 'A', part=nid)   # dorsal spines
        pid = nid
    s.cone((0, 2, 22.5), (2, 3, 1), 'A', TIP_PZ, part=pid)                          # tail tuft
    return s


def majin_buu():
    s = S()
    humanoid(s, leg_h=3, leg_w=3, hip_x=2, torso=(9, 6, 7), arm_w=2, arm_len=5, head=(5, 4, 5),
             tl='P', tt='A', ta='A', th='A', head_kind='ell', arm_d=2)
    s.ell((0, 5, 0), (11, 7, 9), 'A')                    # big round belly
    s.vb(-4, 4, 3, 4, -4, 4, 'D')                         # belt
    s.vb(0, 0, 3, 4, -5, -5, 'C')                         # M buckle
    s.ell((0, 11, 0), (5, 2, 5), 'P')                     # cape collar
    s.vb(-1, -1, 12, 12, -3, -3, 'D', part=5); s.vb(1, 1, 12, 12, -3, -3, 'D', part=5)   # eyes
    s.vb(-1, 1, 11, 11, -3, -3, 'C', part=5)             # grin
    s.part(6, 5, (0, 14, 0), 'Tail')                     # floppy head antenna
    s.seg((0, 14, 0), (0, 16, 2), 1, 1, 'A', part=6)
    s.part(7, 6, (0, 16, 2), 'Tail')
    s.seg((0, 16, 2), (0, 15, 5), 1, 1, 'A', part=7)
    return s


def great_ape():
    s = S()
    h = humanoid(s, leg_h=6, leg_w=3, hip_x=3, torso=(11, 9, 7), arm_w=3, arm_len=12, head=(7, 6, 6),
                 tl='P', tt='P', ta='P', th='P', arm_d=3)
    s.ell((0, 10, -1), (9, 8, 6), 'A')                    # chest
    s.ell((0, 17, -3), (5, 3, 3), 'A', part=5)            # muzzle
    s.vb(-2, 2, 16, 16, -5, -5, 'C', part=5)              # teeth
    s.vb(-2, -1, 19, 19, -4, -4, 'C', part=5); s.vb(1, 2, 19, 19, -4, -4, 'C', part=5)  # red eyes
    s.vb(-2, 2, 20, 20, -4, -3, 'D', part=5)              # brow
    for sx in (-1, 1):
        s.ell((sx * 3.5, 21, 0), (2, 2, 1), 'P', part=5)  # ears
        s.ell((sx * h['ax'], 3, -1), (4, 3, 4), 'P', part=3 if sx < 0 else 4)  # knuckle fists
    pts = [(0, 7, 3), (0, 8, 7), (1, 10, 10), (0, 13, 12), (-1, 15, 11)]
    tail_chain(s, 6, 0, pts, [(1.6, 1.6)] * 4, 'P', kind='seg')
    return s


def kurama():
    s = S()
    # body
    s.ell((0, 9, 0), (11, 9, 18), 'P')
    s.ell((0, 9, -6), (9, 8, 6), 'A')                     # chest ruff
    legs = [(1, -4, -6, 'LegFrontLeft'), (2, 4, -6, 'LegFrontRight'), (3, -4, 6, 'LegBackLeft'), (4, 4, 6, 'LegBackRight')]
    for pid, x, z, role in legs:
        s.part(pid, 0, (x, 7, z), role)
        s.box((x, 3.5, z), (3, 8, 3), 'P', part=pid)
        s.vb(int(x) - 1, int(x) + 1, 0, 0, z - 2, z, 'D', part=pid)
        s.cone((x, 0.5, z - 2.5), (1, 2, 1), 'C', TIP_NZ, part=pid)   # claws
    # head
    s.part(5, 0, (0, 13, -9), 'Head')
    s.ell((0, 15, -12), (8, 6, 7), 'P', part=5)
    s.cone((0, 13.5, -17), (4, 5, 3), 'P', TIP_NZ, part=5)          # long snout
    for sx in (-1, 1):
        s.cone((sx * 3, 20, -11), (3, 6, 2), 'P', part=5)            # tall ears
        s.cone((sx * 3, 19.5, -11.8), (1.4, 4, 1), 'D', part=5)
        s.box((sx * 2, 16, -15), (2, 1, 1), 'C', (0, 0, sx * 20), part=5)  # slit eyes
    s.part(6, 5, (0, 13, -12), 'Jaw')
    s.vb(-2, 2, 12, 12, -19, -13, 'D', part=6)
    s.vb(-2, 2, 13, 13, -18, -17, 'C', part=6)
    # nine tails fanning from the rump, each a 2-segment chain
    nid = 7
    for k in range(9):
        a = math.radians(-60 + k * 15)
        base = (0, 11, 8)
        mid = (math.sin(a) * 5, 16 + 2 * math.cos(a), 12 + math.cos(a) * 3)
        tip = (math.sin(a) * 11, 22 - abs(math.sin(a)) * 5, 15 + math.cos(a) * 2)
        s.part(nid, 0, base, 'Tail'); s.seg(base, mid, 3, 3, 'P', part=nid, kind='Ellipsoid')
        s.part(nid + 1, nid, mid, 'Tail'); s.seg(mid, tip, 3, 3, 'P' if k % 2 else 'A', part=nid + 1, kind='Ellipsoid')
        nid += 2
    return s


def susanoo():
    s = S()
    h = humanoid(s, leg_h=6, leg_w=3, hip_x=3, torso=(11, 9, 5), arm_w=3, arm_len=10, head=(5, 6, 5),
                 tl='P', tt='P', ta='P', th='P', arm_d=3)
    for y in (8, 10, 12):                                 # glowing ribcage
        s.vb(-4, 4, y, y, -3, -3, 'C')
    s.vb(0, 0, 7, 14, -3, -3, 'C')
    for sx in (-1, 1):                                    # shoulder armour
        s.ell((sx * 7, 15, 0), (5, 3, 6), 'A')
    s.cone((0, 22.5, -1), (1, 4, 2), 'A', (-20, 0, 0), part=5)       # tengu nose / crest
    s.vb(-1, -1, 18, 18, -3, -3, 'C', part=5); s.vb(1, 1, 18, 18, -3, -3, 'C', part=5)
    for sx in (-1, 1):
        s.seg((sx * 2, 21, 0), (sx * 4, 25, 2), 1, 1, 'A', part=5)   # horned helm
    s.vb(8, 8, 3, 5, -3, -1, 'D', part=4)                            # sword in right hand
    s.vb(8, 8, 4, 4, -18, -4, 'C', part=4)
    s.vb(7, 9, 4, 4, -4, -4, 'A', part=4)
    s.ell((0, 8, 3), (13, 12, 3), 'A')                    # cape / aura mantle
    return s


def ninja():
    """Shadow-clone ninja: orange jumpsuit, headband plate, spiky hair."""
    s = S()
    h = humanoid(s, leg_h=3, leg_w=1, hip_x=1, torso=(3, 3, 2), arm_w=1, arm_len=3, head=(3, 3, 3),
                 tl='A', tt='A', ta='A', th='P', head_kind='box')
    s.vb(-1, 1, 5, 5, -1, 0, 'D')                         # jacket zip/collar
    s.vb(-1, 1, 7, 7, -2, -2, 'C', part=5)                # headband plate
    s.vb(-1, 1, 7, 7, -1, 1, 'D', part=5)
    s.vb(0, 0, 7, 7, 2, 2, 'D', part=5)                   # band tails
    for x, z in ((-1, 0), (1, 0), (0, 1), (0, -1)):
        s.cone((x, 9.5, z), (1, 2, 1), 'A', part=5)       # spiky hair
    return s


def pochita():
    s = S()
    s.part(1, 0, (0, 3, -1), 'Head')
    s.ell((0, 3, 0), (5, 4, 6), 'A')                      # round orange body
    s.ell((0, 4, -2), (5, 4, 4), 'A', part=1)             # head
    s.vb(0, 0, 5, 5, -12, -4, 'D', part=1)                # chainsaw bar from the forehead
    s.vb(0, 0, 6, 6, -12, -4, 'C', part=1)                # teeth
    s.vb(0, 0, 4, 4, -12, -5, 'C', part=1)
    s.vb(-1, -1, 4, 4, -4, -4, 'D', part=1); s.vb(1, 1, 4, 4, -4, -4, 'D', part=1)
    s.cyl((0, 5.5, 2), (2, 1, 2), 'D', ALONG_X)           # pull-cord handle
    for pid, x, z, role in ((2, -1, -1, 'LegFrontLeft'), (3, 1, -1, 'LegFrontRight'), (4, -1, 2, 'LegBackLeft'), (5, 1, 2, 'LegBackRight')):
        s.part(pid, 0, (x, 1.5, z), role); s.vb(x, x, 0, 1, z, z, 'A', part=pid)
    s.part(6, 0, (0, 3, 3), 'Tail'); s.seg((0, 3, 3), (0, 3, 5), 1, 1, 'A', part=6)
    return s


def chainsaw_man():
    s = S()
    h = humanoid(s, leg_h=4, leg_w=1, hip_x=1, torso=(5, 4, 3), arm_w=1, arm_len=4, head=(3, 3, 3),
                 tl='D', tt='P', ta='P', th='A')
    s.vb(-2, 2, 7, 7, -1, 1, 'D')                         # tie / shirt
    s.vb(0, 0, 5, 7, -2, -2, 'D')
    s.vb(0, 0, 11, 11, -9, 1, 'D', part=5)                # head chainsaw bar
    s.vb(0, 0, 12, 12, -9, 0, 'C', part=5); s.vb(0, 0, 10, 10, -9, -2, 'C', part=5)
    s.vb(-1, 1, 9, 9, -2, -2, 'C', part=5)                # sharp-tooth grin
    for pid, sx in ((3, -1), (4, 1)):                     # forearm chainsaws pointing forward
        x = sx * h['ax']
        s.vb(x, x, 5, 5, -7, 0, 'D', part=pid)
        s.vb(x, x, 6, 6, -7, -1, 'C', part=pid)
    s.part(6, 0, (0, 6, -2), 'Other')                     # starter chain hanging from the chest
    s.vb(0, 0, 4, 5, -2, -2, 'A', part=6)
    s.part(7, 6, (0, 4, -2), 'Other')
    s.vb(0, 0, 2, 3, -2, -2, 'A', part=7)
    s.cyl((0, 1.5, -2), (2, 1, 2), 'D', ALONG_X, part=7)
    return s


def titan(kind='colossal'):
    """Big humanoid titan (authored ~40 units tall)."""
    s = S()
    lean = kind == 'attack'
    h = humanoid(s, leg_h=16, leg_w=5 if lean else 7, hip_x=4, torso=(13 if lean else 15, 13, 7), arm_w=4 if lean else 5,
                 arm_len=15, head=(7, 8, 7), tl='P', tt='P', ta='P', th='P', arm_d=4 if lean else 5, leg_d=5 if lean else 6)
    top = h['y_sh']
    hy = h['hy']
    if kind == 'colossal':
        for x in (-5, -2, 2, 5):                          # exposed muscle bands
            s.vb(x, x, 17, 28, -4, -4, 'A')
        for y in (18, 21, 24):
            s.vb(-5, 5, y, y, -4, -4, 'D')                # ribs
        s.vb(-3, 3, hy - 3, hy - 3, -4, -4, 'C', part=5)  # lipless teeth
        s.vb(-3, 3, hy - 2, hy - 2, -4, -4, 'C', part=5)
        s.vb(-3, -2, hy + 1, hy + 1, -4, -4, 'D', part=5); s.vb(2, 3, hy + 1, hy + 1, -4, -4, 'D', part=5)
        s.vb(-3, 3, hy + 4, hy + 4, -3, 3, 'A', part=5)   # bare skull ridge
    elif kind == 'armored':
        s.vb(-6, 6, 18, 27, -4, -4, 'C')                  # breastplate
        s.vb(-6, 6, 22, 22, -4, -4, 'A')
        for pid, sx in ((3, -1), (4, 1)):
            x = sx * h['ax']
            s.vb(int(x) - 2, int(x) + 2, 16, 22, -3, 2, 'C', part=pid)   # forearm plates
            s.vb(int(x) - 3, int(x) + 3, 26, 29, -3, 3, 'A', part=pid)   # pauldrons
        for pid, sx in ((1, -1), (2, 1)):
            s.vb(sx * 4 - 3, sx * 4 + 3, 3, 10, -4, -4, 'C', part=pid)    # greaves
        s.vb(-3, 3, hy - 1, hy + 3, -4, -4, 'C', part=5)                  # armoured face
        s.vb(-2, -1, hy + 1, hy + 1, -4, -4, 'D', part=5); s.vb(1, 2, hy + 1, hy + 1, -4, -4, 'D', part=5)
    else:  # attack titan: long hair, pointed ears, fanged grin
        s.vb(-4, 4, hy + 2, hy + 4, -3, 4, 'D', part=5)
        s.vb(-4, -4, hy - 3, hy + 3, -2, 4, 'D', part=5); s.vb(4, 4, hy - 3, hy + 3, -2, 4, 'D', part=5)
        s.vb(-3, 3, hy - 3, hy - 3, -4, -4, 'C', part=5)
        s.vb(-2, -1, hy + 1, hy + 1, -4, -4, 'C', part=5); s.vb(1, 2, hy + 1, hy + 1, -4, -4, 'C', part=5)
        for sx in (-1, 1):
            s.cone((sx * 4.5, hy + 1, 0), (1, 3, 1), 'P', (0, 0, -sx * 50), part=5)
        for x in (-4, 0, 4):
            s.vb(x, x, 17, 27, -4, -4, 'A')               # muscle lines
    return s


def mahoraga():
    s = S()
    h = humanoid(s, leg_h=6, leg_w=2, hip_x=2, torso=(7, 6, 3), arm_w=2, arm_len=6, head=(3, 3, 3),
                 tl='P', tt='P', ta='P', th='P', arm_d=2)
    s.vb(-3, 3, 9, 9, -2, -2, 'D')
    s.vb(-1, 1, 14, 14, -2, -2, 'C', part=5)              # eyes
    for sx in (-1, 1):
        s.seg((sx * 2, 14, 0), (sx * 4, 16, 1), 1, 2, 'A', part=5)   # head wings
    # the eight-handled wheel above the head
    s.cyl((0, 20, 0), (9, 1, 9), 'A', ALONG_Z, part=5)
    s.cyl((0, 20, 0), (7, 1, 7), 'A', ALONG_Z, sub=True)
    s.vb(0, 0, 16, 24, 0, 0, 'A', part=5); s.vb(-4, 4, 20, 20, 0, 0, 'A', part=5)
    for k in range(8):
        a = math.radians(k * 45 + 22.5)
        s.box((5.5 * math.cos(a), 20 + 5.5 * math.sin(a), 0), (1, 1, 1), 'C', part=5)   # the eight handles
    s.vb(0, 0, 20, 20, -1, 1, 'C', part=5)
    x = h['ax']                                            # Sword of Extermination along the right forearm
    s.vb(x, x, 6, 6, -8, 0, 'C', part=4)
    s.vb(x, x, 7, 7, -8, -2, 'P', part=4)
    return s


def eva01():
    s = S()
    h = humanoid(s, leg_h=12, leg_w=3, hip_x=2, torso=(7, 9, 4), arm_w=2, arm_len=11, head=(3, 4, 4),
                 tl='P', tt='P', ta='P', th='P', arm_d=2)
    s.vb(-1, 1, 12, 13, -2, -2, 'A')                      # green waist
    s.vb(-3, 3, 17, 18, -2, -2, 'A')                      # chest green
    s.vb(0, 0, 14, 16, -2, -2, 'C')                       # entry-plug accent
    for pid, sx in ((3, -1), (4, 1)):                     # the shoulder pylons
        x = sx * h['ax']
        s.vb(int(x) - 1, int(x) + 1, 18, 22, -2, 2, 'P', part=pid)
        s.vb(int(x) - 1, int(x) + 1, 21, 21, -3, 3, 'A', part=pid)
    for pid, sx in ((1, -1), (2, 1)):
        s.vb(sx * 2 - 1, sx * 2 + 1, 5, 6, -2, -2, 'A', part=pid)       # knees
    hy = h['hy']
    s.vb(-1, 1, hy - 1, hy - 1, -3, -3, 'A', part=5)       # green jaw
    s.vb(-1, -1, hy, hy, -3, -3, 'C', part=5); s.vb(1, 1, hy, hy, -3, -3, 'C', part=5)
    s.seg((0, hy + 1, -2), (0, hy + 4, -4), 1, 1, 'C', part=5)       # single forward horn
    s.vb(-2, 2, hy + 1, hy + 1, -1, 2, 'P', part=5)
    return s


def mech(kind='gundam'):
    s = S()
    h = humanoid(s, leg_h=10, leg_w=3, hip_x=2, torso=(7, 8, 5), arm_w=3, arm_len=9, head=(3, 3, 3),
                 tl='P', tt='P', ta='P', th='P', arm_d=3)
    hy = h['hy']
    for pid, sx in ((1, -1), (2, 1)):
        s.vb(sx * 2 - 1, sx * 2 + 1, 0, 1, -3, 1, 'A', part=pid)       # feet
        s.vb(sx * 2 - 1, sx * 2 + 1, 5, 6, -2, -2, 'D', part=pid)      # knee
    for pid, sx in ((3, -1), (4, 1)):
        x = sx * h['ax']
        s.vb(int(x) - 2, int(x) + 2, 15, 18, -2, 2, 'P', part=pid)     # big shoulder blocks
        s.vb(int(x) - 1, int(x) + 1, 9, 10, -2, 2, 'D', part=pid)      # fists
    s.vb(-3, 3, 10, 11, -3, 2, 'A')                       # waist
    if kind == 'gundam':
        s.vb(-3, 3, 15, 17, -3, -3, 'A')                  # chest
        s.vb(-2, -1, 16, 16, -4, -4, 'D'); s.vb(1, 2, 16, 16, -4, -4, 'D')   # vents
        s.vb(0, 0, 12, 14, -3, -3, 'C')
        s.vb(-1, 1, hy - 1, hy - 1, -2, -2, 'C', part=5)  # faceplate
        s.vb(-1, -1, hy, hy, -2, -2, 'C', part=5); s.vb(1, 1, hy, hy, -2, -2, 'C', part=5)
        for sx in (-1, 1):
            s.seg((0, hy + 1, -2), (sx * 3, hy + 4, -2), 1, 1, 'A', part=5)   # V-fin
        s.vb(0, 0, hy + 1, hy + 1, -2, -2, 'C', part=5)
        s.vb(-6, -6, 5, 13, -3, -3, 'A', part=3)          # shield on the left arm
        s.vb(-6, -6, 6, 12, -4, -4, 'D', part=3)
        s.vb(-6, -6, 9, 9, -5, -5, 'C', part=3)
        x = h['ax']
        s.vb(x, x, 9, 10, -7, -2, 'D', part=4)            # beam rifle in the right hand
        s.vb(x, x, 11, 11, -5, -3, 'A', part=4)
        for sx in (-1, 1):
            s.vb(sx * 2, sx * 2, 18, 21, 3, 3, 'D')      # backpack saber hilts
        s.vb(-2, 2, 12, 17, 3, 3, 'P')
    else:  # mazinger z: pilder cockpit on the head, breast-fire red fins, rocket-punch fists
        s.vb(-3, 3, 16, 17, -3, -3, 'A')
        s.box((-2, 16.5, -3), (3, 1, 1), 'C', (0, 0, -20)); s.box((2, 16.5, -3), (3, 1, 1), 'C', (0, 0, 20))
        s.vb(-1, 1, hy - 1, hy, -2, -2, 'D', part=5)
        s.vb(-1, 1, hy, hy, -2, -2, 'C', part=5)
        s.vb(-1, 1, hy + 2, hy + 2, -2, 1, 'A', part=5)   # hover pilder
        s.vb(0, 0, hy + 2, hy + 2, -3, -3, 'C', part=5)
        for sx in (-1, 1):
            s.box((sx * 2.5, hy + 1, 0), (1, 3, 2), 'A', (0, 0, -sx * 25), part=5)   # ear fins
    return s


def gurren_lagann():
    s = S()
    h = humanoid(s, leg_h=6, leg_w=3, hip_x=3, torso=(13, 11, 7), arm_w=3, arm_len=11, head=(3, 3, 3),
                 tl='P', tt='P', ta='P', th='A', arm_d=3)
    # Gurren's face on the chest: brows, eyes, toothy grin; Kamina shades across it
    s.vb(-5, 5, 13, 13, -4, -4, 'C')                      # teeth row
    s.vb(-5, 5, 12, 12, -4, -4, 'D')
    for sx in (-1, 1):
        s.box((sx * 3, 14.5, -4.5), (5, 1, 1), 'A', (0, 0, sx * 15))   # shades
        s.box((sx * 3, 15.3, -4.5), (4, 1, 1), 'A', (0, 0, sx * 15))
        s.box((sx * 3, 17, -4), (4, 1, 1), 'D', (0, 0, -sx * 20))      # angry brows
    s.vb(-5, 5, 17, 17, -3, 3, 'P')
    hy = h['hy']
    s.vb(0, 0, hy, hy, -2, -2, 'C', part=5)               # Lagann on top
    s.cone((0, hy + 3, 0), (2, 4, 2), 'C', part=5)        # core-drill horn
    x = h['ax']
    s.cone((x, 3, -6), (3, 8, 3), 'C', TIP_NZ, part=4)   # drill arm
    for sx in (-1, 1):
        s.box((sx * 9, 17, 3), (1, 4, 7), 'A', (0, 0, -sx * 30))   # shoulder spikes
    return s


def totoro():
    s = S()
    s.ell((0, 6, 0), (12, 12, 10), 'P')                   # big egg body
    s.ell((0, 5, -3), (8, 8, 5), 'A')                     # pale belly
    for x, y in ((-2, 6), (0, 6.5), (2, 6), (-1, 4.5), (1, 4.5)):
        s.box((x, y, -5.5), (1, 1, 1), 'D')               # chevron marks
    s.part(5, 0, (0, 10, 0), 'Head')
    for sx in (-1, 1):
        s.ell((sx * 2.5, 15, 0), (2, 5, 2), 'P', part=5)  # ears
        s.ell((sx * 2.5, 10.5, -4.5), (2, 2, 1), 'C', part=5)  # eyes
        s.vb(int(sx * 2.5), int(sx * 2.5), 10, 10, -5, -5, 'D', part=5)
        for dy in (-0.5, 0.5):
            s.box((sx * 4.5, 9 + dy, -4.5), (4, 1, 1), 'D', (0, 0, sx * dy * 20), part=5)  # whiskers
    s.vb(0, 0, 10, 10, -5, -5, 'D', part=5)
    for pid, sx, role in ((3, -1, 'ArmLeft'), (4, 1, 'ArmRight')):
        s.part(pid, 0, (sx * 5.5, 9, 0), role); s.ell((sx * 6, 6.5, 0), (2, 5, 3), 'P', part=pid)
    for pid, sx, role in ((1, -1, 'LegBackLeft'), (2, 1, 'LegBackRight')):
        s.part(pid, 0, (sx * 3, 1.5, 0), role); s.ell((sx * 3, 0.5, -0.5), (3, 2, 4), 'P', part=pid)
    return s


def catbus():
    s = S()
    s.ell((0, 6, 0), (8, 6, 20), 'A')                     # long furry bus body
    s.vb(-4, 4, 6, 7, -7, 6, 'D')                         # window band
    for z in (-6, -3, 0, 3, 6):
        s.vb(-4, 4, 6, 7, z, z, 'A')                      # window frames
    s.part(1, 0, (0, 7, -8), 'Head')
    s.ell((0, 8, -10), (7, 6, 5), 'A', part=1)
    s.vb(-3, 3, 6, 6, -13, -12, 'C', part=1)              # giant grin
    for sx in (-1, 1):
        s.ell((sx * 2, 9.5, -12), (2, 2, 1), 'C', part=1) # headlight eyes
        s.cone((sx * 2.5, 12, -10), (2, 3, 1), 'A', part=1)
    s.part(2, 0, (0, 8, 10), 'Tail'); s.seg((0, 8, 10), (0, 11, 13), 2, 2, 'A', part=2)
    s.part(3, 2, (0, 11, 13), 'Tail'); s.seg((0, 11, 13), (0, 14, 13), 2, 2, 'A', part=3)
    pid = 4
    for sx, role in ((-1, 'LegFrontLeft'), (1, 'LegFrontRight')):
        for z in (-7, -4, -1, 2, 5, 8):
            x = sx * 3
            s.part(pid, 0, (x, 3, z), role)
            s.vb(x, x, 0, 3, z, z, 'A', part=pid)
            pid += 1
    return s


def soot_sprite():
    s = S()
    s.part(1, 0, (0, 1, 0), 'Head')
    s.ell((0, 2, 0), (5, 5, 5), 'D', part=1)
    for k in range(10):
        a = k * 2.4; b = (k % 5) * 0.6 - 1.2
        s.box((2.6 * math.cos(a), 2 + 2.6 * math.sin(b), 2.6 * math.sin(a)), (1, 1, 1), 'D', part=1)
    for sx in (-1, 1):
        s.vb(sx, sx, 2, 3, -3, -3, 'C', part=1)
    return s


def no_face():
    s = S()
    s.part(1, 0, (-1, 1.5, 0), 'LegBackLeft'); s.vb(-1, -1, 0, 1, 0, 0, 'D', part=1)
    s.part(2, 0, (1, 1.5, 0), 'LegBackRight'); s.vb(1, 1, 0, 1, 0, 0, 'D', part=2)
    s.cyl((0, 6, 0), (5, 10, 4), 'D')                     # tall shadow robe
    s.ell((0, 11, 0), (5, 3, 4), 'D')
    s.part(5, 0, (0, 9, 0), 'Head')
    s.ell((0, 10, -2), (3, 4, 1), 'C', part=5)           # white mask
    s.vb(-1, -1, 10, 10, -3, -3, 'D', part=5); s.vb(1, 1, 10, 10, -3, -3, 'D', part=5)
    s.vb(-1, -1, 11, 11, -3, -3, 'P', part=5); s.vb(1, 1, 11, 11, -3, -3, 'P', part=5)   # purple marks
    s.vb(-1, -1, 9, 9, -3, -3, 'P', part=5); s.vb(1, 1, 9, 9, -3, -3, 'P', part=5)
    s.vb(0, 0, 8, 8, -3, -3, 'D', part=5)
    for pid, sx, role in ((3, -1, 'ArmLeft'), (4, 1, 'ArmRight')):
        s.part(pid, 0, (sx * 2, 8, 0), role); s.vb(sx * 3, sx * 3, 4, 8, 0, 0, 'D', part=pid)
    return s


def calcifer():
    s = S()
    s.part(1, 0, (0, 1, 0), 'Head')
    s.ell((0, 2, 0), (6, 5, 5), 'P', part=1)
    s.cone((0, 6, 0), (5, 5, 4), 'P', part=1)
    s.cone((-2, 5, 0), (2, 4, 2), 'A', (0, 0, 25), part=1)
    s.cone((2, 5, 0), (2, 4, 2), 'A', (0, 0, -25), part=1)
    for sx in (-1, 1):
        s.ell((sx * 1.2, 3, -2.2), (2, 2, 1), 'C', part=1)
        s.vb(sx, sx, 3, 3, -3, -3, 'D', part=1)
    s.vb(-1, 1, 1, 1, -3, -3, 'D', part=1)
    s.part(2, 1, (0, 8, 0), 'Other'); s.cone((0, 9, 0), (2, 3, 2), 'C', part=2)   # flickering tip
    return s


def howls_castle():
    s = S()
    for pid, sx, role in ((1, -1, 'LegBackLeft'), (2, 1, 'LegBackRight')):   # scrawny chicken legs
        s.part(pid, 0, (sx * 3, 9, 0), role)
        s.seg((sx * 3, 9, 0), (sx * 3, 5, 2), 1, 1, 'D', part=pid)
        s.seg((sx * 3, 5, 2), (sx * 3, 1, 0), 1, 1, 'D', part=pid)
        for dz, dx in ((-2, 0), (-1, 1), (-1, -1)):
            s.vb(sx * 3 + dx, sx * 3 + dx, 0, 0, dz, dz + 1, 'D', part=pid)
    s.ell((0, 14, 0), (14, 10, 12), 'P')                  # ramshackle bulk
    s.vb(-5, 1, 16, 21, -3, 3, 'P')
    s.vb(2, 6, 12, 18, -5, 1, 'A')
    s.cyl((-4, 21, 2), (3, 6, 3), 'D')                    # smokestacks
    s.cyl((3, 20, 3), (2, 4, 2), 'D')
    s.cone((-4, 25, 2), (4, 2, 4), 'A')
    s.vb(-3, 3, 11, 11, -7, -6, 'C')                      # mouth-hatch with teeth
    s.vb(-3, 3, 10, 10, -6, -6, 'D')
    s.cyl((-4, 15, -6), (2, 2, 2), 'C', ALONG_Z)          # eye windows
    s.cyl((3, 16, -6), (2, 2, 2), 'C', ALONG_Z)
    s.cyl((-6, 12, -5), (1, 5, 1), 'D', ALONG_Z)          # cannon barrels
    s.cyl((6, 12, -5), (1, 5, 1), 'D', ALONG_Z)
    s.vb(-1, 1, 12, 13, 6, 7, 'A')                        # tail-hut
    return s


def laputa_robot():
    s = S()
    for pid, sx, role in ((1, -1, 'LegBackLeft'), (2, 1, 'LegBackRight')):
        s.part(pid, 0, (sx * 2, 6, 0), role); s.vb(sx * 2, sx * 2, 0, 5, 0, 0, 'D', part=pid)
        s.vb(sx * 2 - 1, sx * 2 + 1, 0, 0, -1, 1, 'P', part=pid)
    s.ell((0, 9, 0), (7, 8, 6), 'P')                      # bell-shaped body
    s.vb(-3, 3, 6, 6, -3, 3, 'A')
    s.part(5, 0, (0, 13, 0), 'Head')
    s.ell((0, 13.5, -0.5), (4, 3, 4), 'P', part=5)
    s.vb(0, 0, 13, 13, -3, -3, 'C', part=5)               # single eye (beam)
    s.vb(-1, 1, 15, 15, 0, 0, 'A', part=5)
    for pid, sx, role in ((3, -1, 'ArmLeft'), (4, 1, 'ArmRight')):   # long dangling arms
        s.part(pid, 0, (sx * 3.5, 11, 0), role)
        s.vb(sx * 4, sx * 4, 3, 11, 0, 0, 'D', part=pid)
        s.vb(sx * 4 - 1 if sx > 0 else sx * 4, sx * 4 if sx > 0 else sx * 4 + 1, 2, 2, -1, 0, 'P', part=pid)
    s.vb(-1, 1, 9, 10, 3, 3, 'A')                         # moss/flower patch
    return s


def mimic():
    s = S()
    s.vb(-4, 4, 0, 4, -3, 3, 'P')                         # chest body
    s.vb(-4, 4, 2, 2, -4, 3, 'D')                         # iron band
    s.vb(-3, 3, 5, 5, -4, -4, 'C')                        # lower teeth
    s.vb(-2, 2, 4, 5, -2, 1, 'A')                         # tongue
    s.part(1, 0, (0, 5, 3), 'Head')                       # the lid, hinged at the back: it chomps
    s.vb(-4, 4, 6, 7, -3, 3, 'P', part=1)
    s.cyl((0, 7.5, 0), (9, 1, 7), 'P', ALONG_Z, part=1)
    s.vb(-4, 4, 6, 8, 0, 0, 'D', part=1)
    s.vb(-3, 3, 5, 5, -3, -3, 'C', part=1)                # upper teeth
    s.vb(-1, -1, 7, 7, -4, -4, 'C', part=1); s.vb(1, 1, 7, 7, -4, -4, 'C', part=1)   # glinting eyes
    s.vb(0, 0, 4, 5, -4, -4, 'A')                         # lock
    return s


def shadow_soldier():
    s = S()
    h = humanoid(s, leg_h=4, leg_w=1, hip_x=1, torso=(5, 4, 3), arm_w=1, arm_len=4, head=(3, 3, 3),
                 tl='D', tt='D', ta='D', th='D')
    s.vb(-2, 2, 5, 7, -2, -2, 'P')                        # armour plates
    s.vb(-1, 1, 9, 9, -2, -2, 'C', part=5)                # visor glow
    s.seg((0, 11, 0), (0, 12, 3), 1, 1, 'A', part=5)      # plume
    s.seg((0, 12, 3), (0, 10, 5), 1, 1, 'A', part=5)
    s.vb(-2, 2, 2, 7, 2, 2, 'A')                          # tattered cape
    x = h['ax']
    s.vb(x, x, 4, 4, -7, -1, 'C', part=4)                 # blade
    s.vb(x - 1, x + 1, 4, 4, -1, -1, 'P', part=4)
    return s


def kaiju8():
    s = S()
    h = humanoid(s, leg_h=4, leg_w=2, hip_x=1.5, torso=(5, 5, 3), arm_w=2, arm_len=5, head=(3, 3, 3),
                 tl='P', tt='P', ta='P', th='A', arm_d=2)
    for y in (5, 7):
        s.vb(-2, 2, y, y, -2, -2, 'D')                    # segmented plates
    s.cyl((0, 7, -2), (1, 1, 1), 'C', ALONG_Z)            # core
    hy = h['hy']
    s.vb(-1, -1, hy, hy, -2, -2, 'D', part=5); s.vb(1, 1, hy, hy, -2, -2, 'D', part=5)   # hollow eye sockets
    s.vb(-1, 1, hy - 1, hy - 1, -2, -2, 'C', part=5)       # skull grin
    s.vb(-1, 1, hy + 2, hy + 2, -1, 1, 'P', part=5)
    for sx in (-1, 1):
        s.vb(sx * 3, sx * 3, 9, 10, -1, 1, 'D')           # shoulder vents
    return s


def star_platinum():
    s = S()
    h = humanoid(s, leg_h=4, leg_w=1, hip_x=1, torso=(5, 4, 3), arm_w=2, arm_len=5, head=(3, 3, 3),
                 tl='P', tt='P', ta='P', th='P', arm_d=2, shoulder_extra=0)
    s.vb(-2, 2, 7, 7, -2, 2, 'A')                         # scarf
    s.vb(1, 2, 3, 7, 2, 2, 'A')
    s.vb(-2, 2, 4, 4, -2, 2, 'D')                         # belt
    hy = h['hy']
    s.vb(-1, 1, hy, hy, -2, -2, 'C', part=5)              # fierce eyes
    for x, y, z in ((-1, 12, 1), (1, 12, 1), (0, 12, 2), (0, 11, 3), (-1, 11, 2), (1, 11, 2)):
        s.cone((x, y, z), (1, 3, 1), 'D', (70, 0, x * 20), part=5)   # flowing hair
    for sx in (-1, 1):
        s.ell((sx * 3.5, 8, 0), (3, 2, 3), 'A')           # shoulder pads
    for pid, sx in ((3, -1), (4, 1)):
        x = sx * h['ax']
        s.vb(int(math.floor(x)), int(math.ceil(x)), 3, 4, -1, 0, 'A', part=pid)    # wrist bands
    return s


def alphonse():
    s = S()
    h = humanoid(s, leg_h=4, leg_w=2, hip_x=1.5, torso=(7, 5, 4), arm_w=2, arm_len=5, head=(3, 4, 3),
                 tl='P', tt='P', ta='P', th='P', arm_d=2)
    s.vb(-3, 3, 4, 4, -2, 1, 'D')                         # loincloth belt
    s.vb(-1, 1, 1, 3, -2, -2, 'A')
    for sx in (-1, 1):
        s.ell((sx * 4.5, 8.5, 0), (3, 3, 3), 'A')         # round pauldrons
        s.cone((sx * 4.5, 10.5, 0), (1, 2, 1), 'C')       # spikes
    hy = h['hy']
    s.vb(-1, 1, hy, hy, -2, -2, 'D', part=5)              # visor slit
    s.vb(-1, -1, hy, hy, -2, -2, 'C', part=5); s.vb(1, 1, hy, hy, -2, -2, 'C', part=5)
    s.cone((0, hy + 3, -1), (1, 3, 1), 'A', (-25, 0, 0), part=5)    # helmet spike
    s.seg((0, hy + 2, 1), (0, hy, 3), 1, 1, 'A', part=5)  # plume
    s.seg((0, hy, 3), (0, hy - 3, 3), 1, 1, 'A', part=5)
    s.vb(-2, 2, 7, 8, -2, -2, 'A')                        # chest emblem
    return s


def ramiel():
    s = S()
    s.cone((0, 13, 0), (12, 8, 12), 'P')                  # upper pyramid
    s.cone((0, 5, 0), (12, 8, 12), 'P', TIP_NY)           # lower pyramid
    s.cyl((0, 9, 0), (12.5, 1, 12.5), 'A')                # equator band
    s.box((0, 9, -6), (1, 1, 1), 'C')                     # the beam point
    return s


def guanyin():
    s = S()
    s.ell((0, 6, 0), (16, 8, 12), 'A')                    # seated lotus base
    s.box((0, 10, 1), (10, 10, 7), 'P')                   # torso
    s.ell((0, 18, 0), (6, 6, 6), 'P')                     # head
    s.ell((0, 22, 0), (3, 3, 3), 'A')                     # topknot
    s.vb(-1, 1, 18, 18, -3, -3, 'D')
    s.vb(0, 0, 10, 16, -4, -4, 'C')                       # praying hands
    s.vb(-1, 1, 12, 15, -3, -3, 'P')
    for k in range(14):                                   # the thousand-arm halo
        a = math.radians(-100 + k * (200 / 13))
        r0 = 6; r1 = 16
        base = (math.sin(a) * r0, 14 + math.cos(a) * r0, 5)
        tip = (math.sin(a) * r1, 14 + math.cos(a) * r1, 6)
        s.seg(base, tip, 2, 2, 'P')
        s.box(tip, (3, 3, 2), 'C')
    return s


def kaneda_bike():
    s = S()
    for pid, z in ((1, -6), (2, 6)):
        s.part(pid, 0, (0, 3, z), 'Other')
        s.cyl((0, 3, z), (6, 1, 6), 'D', ALONG_X, part=pid)
        s.cyl((0, 3, z), (3, 1, 3), 'A', ALONG_X, part=pid)
    s.ell((0, 5, -0.5), (3, 4, 9), 'A')                   # red fairing (wheels exposed)
    s.ell((0, 6, -3.5), (3, 3, 5), 'A')
    s.ell((0, 4.5, 3.5), (3, 3, 5), 'A')
    s.vb(0, 0, 5, 5, -9, -8, 'C')                         # headlight
    s.vb(-1, 1, 6, 6, 1, 5, 'D')                          # seat
    s.vb(-1, 1, 2, 2, -3, 3, 'P')                         # stripe / chassis
    s.box((0, 6.5, -5), (1, 1, 2), 'C')                   # windscreen glint
    return s


def tachikoma():
    s = S()
    s.ell((0, 7, 3), (7, 6, 8), 'P')                      # abdomen pod
    s.part(5, 0, (0, 7, -1), 'Head')
    s.ell((0, 7, -4), (5, 5, 5), 'P', part=5)            # head pod
    for x in (-1, 0, 1):
        s.vb(x, x, 8 if x == 0 else 7, 8 if x == 0 else 7, -7, -7, 'C', part=5)   # eye lenses
    s.vb(0, 0, 5, 5, -9, -6, 'D', part=5)                 # gun
    for pid, x, z, role in ((1, -3, -2, 'LegFrontLeft'), (2, 3, -2, 'LegFrontRight'), (3, -3, 4, 'LegBackLeft'), (4, 3, 4, 'LegBackRight')):
        s.part(pid, 0, (x, 6, z), role)
        sx = 1 if x > 0 else -1
        s.seg((x, 6, z), (x + sx * 3, 3, z), 1, 1, 'A', part=pid)
        s.seg((x + sx * 3, 3, z), (x + sx * 4, 0, z), 1, 1, 'A', part=pid)
        s.cyl((x + sx * 4, 0.5, z), (2, 1, 2), 'D', ALONG_X, part=pid)    # foot wheels
    s.vb(0, 0, 11, 12, 3, 3, 'D')                         # antenna
    return s


def sunny(kind='sunny'):
    s = S()
    s.vb(-4, 4, 1, 4, -10, 10, 'P')                       # hull
    s.cone((0, 2.5, -12), (8, 4, 4), 'P', TIP_NZ)         # bow
    s.vb(-3, 3, 0, 0, -8, 8, 'D')                         # keel
    s.vb(-4, 4, 4, 4, -10, 10, 'A')                       # gunwale stripe
    s.cyl((0, 12, 0), (1, 16, 1), 'D')                    # mast
    s.vb(-5, 5, 9, 16, 0, 0, 'P')                         # sail
    s.vb(-1, 1, 12, 13, -1, -1, 'C')                      # jolly roger skull
    s.box((0, 11, -1), (3, 1, 1), 'C', (0, 0, 35)); s.box((0, 11, -1), (3, 1, 1), 'C', (0, 0, -35))
    s.vb(-3, 3, 5, 7, 5, 9, 'A')                          # aft cabin
    if kind == 'sunny':
        s.cyl((0, 6, -13), (7, 1, 7), 'A', ALONG_Z)       # sunflower lion mane
        s.ell((0, 6, -13.5), (4, 4, 2), 'C')              # lion face
        s.vb(-1, 1, 5, 5, -15, -15, 'D')
        s.vb(-1, -1, 7, 7, -15, -15, 'D'); s.vb(1, 1, 7, 7, -15, -15, 'D')
    else:  # going merry: sheep figurehead with curled horns
        s.ell((0, 6, -13), (3, 3, 3), 'C')
        for sx in (-1, 1):
            s.cyl((sx * 2, 6.5, -12.5), (2, 1.5, 2), 'A', ALONG_X)
        s.vb(0, 0, 5, 5, -15, -15, 'D')
    return s
