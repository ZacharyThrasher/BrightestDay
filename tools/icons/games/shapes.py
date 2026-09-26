"""Hand-authored ShapeSpecs (construct-voxel units, 0.25 m). Held: grip at origin, points -Z.
Creatures/structures/objects: feet-centre origin, +Y up, face -Z. Odd sizes on integer centres."""
import math
from dsl import Shape

S = {}


def shape(fn):
    S[fn.__name__] = fn
    return fn


# ============================================================== ZELDA
@shape
def master_sword():
    s = Shape(turn=True)
    s.box((0, 0, 2), (3, 1, 1), 'A')            # pommel
    s.box((0, 0, 0.5), (1, 1, 2), 'D')          # grip
    s.box((0, 0, -1), (7, 1, 1), 'A')           # guard
    s.box((-3, 0, -2), (1, 1, 1), 'A')          # swept wings
    s.box((3, 0, -2), (1, 1, 1), 'A')
    s.box((0, 0, -1), (1, 3, 1), 'C')           # gem
    s.box((0, 0, -5.5), (3, 1, 8), 'P')         # blade z -9..-2
    s.box((0, 0, -5), (1, 1, 7), 'C')           # fuller glow
    s.box((0, 0, -10), (1, 1, 1), 'P')          # tip
    return s


@shape
def hylian_shield():
    s = Shape()
    s.box((0, 1, -1), (9, 9, 1), 'A')           # rim
    s.box((0, -4, -1), (5, 3, 1), 'A')          # lower taper
    s.box((0, -6, -1), (3, 1, 1), 'A')
    s.box((0, 1, -1), (7, 7, 1), 'P')           # field
    s.box((0, -4, -1), (3, 3, 1), 'P')
    # triforce
    s.box((0, 4, -2), (1, 1, 1), 'C')
    s.box((0, 3, -2), (3, 1, 1), 'C')
    # crest bird
    s.box((0, 0, -2), (5, 1, 1), 'A')
    s.box((0, -1, -2), (1, 3, 1), 'A')
    s.box((-2, 1, -2), (1, 1, 1), 'A')
    s.box((2, 1, -2), (1, 1, 1), 'A')
    s.box((0, 0, 0), (1, 3, 1), 'D')            # handle
    return s


@shape
def hookshot():
    s = Shape()
    s.box((0, -2, 0.5), (1, 3, 2), 'D')                       # grip
    s.cyl((0, 0, -1), (3, 5, 3), 'P', r=(90, 0, 0))           # barrel body z -3..1
    s.cyl((0, 0, -3), (3, 1, 3), 'A', r=(90, 0, 0))           # front ring
    s.box((0, 0, -5), (1, 1, 3), 'D')                         # chain stub
    s.point((0, 0, -7), (3, 2, 3), (0, 0, -1), 'C')          # hook head
    s.seg((0, 0, -7), (-2, 0, -6), 1, 'C')                    # prongs
    s.seg((0, 0, -7), (2, 0, -6), 1, 'C')
    s.seg((0, 0, -7), (0, 2, -6), 1, 'C')
    return s


@shape
def guardian():
    s = Shape()
    s.cyl((0, 5, 0), (7, 3, 7), 'D')                               # hip ring
    s.ell((0, 8, 0), (9, 6, 9), 'P', p=1)                          # dome head
    s.box((0, 5.5, 0), (9, 1, 9), 'A', p=1)                        # brim
    s.ell((0, 8, -4), (3, 3, 1), 'A', p=1)                         # eye ring
    s.box((0, 8, -4.5), (1, 1, 1), 'C', p=1)                       # eye
    s.box((0, 11, 0), (1, 1, 1), 'C', p=1)
    s.part(1, 0, (0, 6, 0), 'Head')
    legs = [('LegFrontLeft', -1, -2.5), ('LegFrontRight', 1, -2.5), ('LegFrontLeft', -1, 0.5),
            ('LegFrontRight', 1, 0.5), ('LegBackLeft', -1, 3), ('LegBackRight', 1, 3)]
    for i, (role, sx, z) in enumerate(legs):
        pid = 2 + i
        hip = (sx * 3, 5, z)
        knee = (sx * 6, 6, z * 1.6)
        foot = (sx * 8, 0, z * 2)
        s.seg(hip, knee, 1.6, 'D', pid)
        s.seg(knee, foot, 1.4, 'P', pid)
        s.box((foot[0], 0, foot[2]), (1, 1, 1), 'C', pid)
        s.part(pid, 0, hip, role)
    return s


# ============================================================== MARIO
def _shell(spiny=False, wings=False):
    s = Shape()
    s.ell((0, 2, 0), (8, 6, 9), 'P')
    s.box((0, -1, 0), (11, 4, 11), 'P', sub=True)
    s.cyl((0, 1, 0), (9, 1, 10), 'C')                        # white rim
    s.ell((0, 5, 0), (3, 1, 3), 'A')                          # hex plates
    for (x, z) in [(-2, -2), (2, -2), (-2, 2), (2, 2)]:
        s.ell((x, 4, z), (3, 1, 3), 'A')
    if spiny:
        for (x, z) in [(0, -2), (-2, 1), (2, 1), (0, 0)]:
            s.cone((x, 6, z), (1.6, 3, 1.6), 'C')
    if wings:
        s.box((-5.5, 3, 1), (2, 1, 4), 'C', r=(0, 0, 20))
        s.box((5.5, 3, 1), (2, 1, 4), 'C', r=(0, 0, -20))
    return s


@shape
def green_shell():
    return _shell()


@shape
def red_shell():
    return _shell()


@shape
def spiny_shell():
    return _shell(spiny=True, wings=True)


@shape
def bob_omb():
    s = Shape()
    s.ell((0, 4, 0), (7, 7, 7), 'P')
    s.box((-1, 5, -3), (1, 2, 1), 'C')
    s.box((1, 5, -3), (1, 2, 1), 'C')
    s.cyl((0, 8, 0), (2, 1, 2), 'D')
    s.cyl((0, 9, 0), (1, 2, 1), 'D')
    s.box((0, 10, 0), (1, 1, 1), 'C')                      # spark
    s.box((0, 4, 4), (1, 1, 2), 'D')                       # key stem
    s.box((0, 4, 5), (5, 3, 1), 'A')                       # key bow
    for sx, role in [(-1, 'LegBackLeft'), (1, 'LegBackRight')]:
        pid = 1 if sx < 0 else 2
        s.box((sx * 1.5, 0, -0.5), (2, 1, 4), 'A', pid)
        s.part(pid, 0, (sx * 1.5, 1.5, 0), role)
    return s


@shape
def bullet_bill():
    s = Shape()
    s.cyl((0, 4, 1), (7, 7, 7), 'P', r=(90, 0, 0))         # body z -2.5..4.5
    s.ell((0, 4, -2), (7, 7, 7), 'P')                      # rounded nose
    s.cyl((0, 4, 5), (8, 1, 8), 'D', r=(90, 0, 0))         # rear rim
    s.cyl((0, 4, 6), (5, 1, 5), 'C', r=(90, 0, 0))         # exhaust glow
    s.box((-1.5, 5.5, -5), (1, 2, 1), 'C')                 # eyes
    s.box((1.5, 5.5, -5), (1, 2, 1), 'C')
    s.box((-1.5, 6.5, -4.5), (2, 1, 1), 'D')               # angry brows
    s.box((1.5, 6.5, -4.5), (2, 1, 1), 'D')
    s.box((-4, 3, 0), (1, 2, 2), 'A')                      # little arms
    s.box((4, 3, 0), (1, 2, 2), 'A')
    return s


@shape
def pow_block():
    s = Shape()
    s.box((0, 4, 0), (9, 9, 9), 'P')
    s.box((0, 4, 0), (9.2, 7, 7), 'A')
    s.box((0, 4, 0), (7, 9.2, 7), 'A')
    s.box((0, 4, 0), (7, 7, 9.2), 'A')
    # "POW" glyph bars on front/back
    for z in (-4.6, 4.6):
        for x in (-2.5, 0, 2.5):
            s.box((x, 4, z), (1, 3, 1), 'C')
        s.box((0, 6, z), (7, 1, 1), 'C')
        s.box((0, 2, z), (7, 1, 1), 'C')
    return s


@shape
def super_mushroom():
    s = Shape()
    s.ell((0, 4, 0), (9, 7, 9), 'P')
    s.box((0, 1, 0), (11, 5, 11), 'P', sub=True)
    s.ell((0, 7, 0), (3, 2, 3), 'A')
    s.ell((0, 5.5, -4), (3, 3, 2), 'A')
    s.ell((-4, 5.5, 0), (2, 3, 3), 'A')
    s.ell((4, 5.5, 0), (2, 3, 3), 'A')
    s.ell((0, 5.5, 4), (3, 3, 2), 'A')
    s.cyl((0, 2, 0), (5, 4, 5), 'C')                       # stem
    s.box((-1, 2.5, -2.2), (1, 2, 1), 'D')                 # eyes
    s.box((1, 2.5, -2.2), (1, 2, 1), 'D')
    return s


@shape
def super_star():
    s = Shape()
    c = (0, 7, 0)
    s.ell(c, (7, 7, 3), 'P')
    for k in range(5):
        a = math.radians(k * 72)
        d = (math.sin(a), math.cos(a), 0)
        s.seg(c, (c[0] + d[0] * 5, c[1] + d[1] * 5, 0), 3, 'P', d=3)
        s.seg(c, (c[0] + d[0] * 7, c[1] + d[1] * 7, 0), 1.2, 'P', d=1.5)
    s.box((-1, 7.5, -1.5), (1, 2, 1), 'D')
    s.box((1, 7.5, -1.5), (1, 2, 1), 'D')
    return s


@shape
def bowser():
    s = Shape()
    # torso + shell (root)
    s.ell((0, 8, 0.5), (9, 8, 8), 'P')
    s.ell((0, 7.5, -2.5), (6, 6, 3), 'A')                  # belly
    s.ell((0, 9, 3), (10, 9, 5), 'D')                      # shell
    s.cyl((0, 9, 3), (10.5, 1, 6), 'C', r=(0, 0, 0))       # shell rim
    for (x, y) in [(0, 11), (-2.5, 8.5), (2.5, 8.5), (0, 6.5), (-2.5, 12), (2.5, 12)]:
        s.point((x, y, 6), (1.6, 2.5, 1.6), (0, 0, 1), 'C')
    # legs
    for sx, role, pid in [(-1, 'LegBackLeft', 1), (1, 'LegBackRight', 2)]:
        s.box((sx * 2.5, 2.5, 0), (3, 4, 3), 'P', pid)
        s.box((sx * 2.5, 0, -1), (3, 1, 5), 'D', pid)
        s.box((sx * 2.5, 0, -3.5), (3, 1, 1), 'C', pid)
        s.part(pid, 0, (sx * 2.5, 5, 0), role)
    # arms
    for sx, role, pid in [(-1, 'ArmLeft', 3), (1, 'ArmRight', 4)]:
        s.box((sx * 5.5, 9, -0.5), (2, 5, 2), 'P', pid)
        s.box((sx * 5.5, 8, -0.5), (3, 1, 3), 'D', pid)       # spiked cuff
        s.box((sx * 5.5, 6, -0.5), (2, 1, 3), 'C', pid)       # claws
        s.part(pid, 0, (sx * 4.5, 11, 0), role)
    # head
    s.ell((0, 13.5, -1.5), (6, 5, 6), 'P', 5)
    s.box((0, 13, -4.5), (5, 3, 3), 'P', 5)                   # snout
    s.box((0, 14, -6), (1, 1, 1), 'D', 5)                     # nostril
    s.box((-1.5, 14.5, -4), (1, 1, 1), 'C', 5)
    s.box((1.5, 14.5, -4), (1, 1, 1), 'C', 5)
    s.box((0, 15.5, 0.5), (5, 2, 3), 'D', 5)                  # hair
    s.point((-2.5, 16, -1), (1.6, 3, 1.6), (-0.3, 1, 0), 'C', 5)
    s.point((2.5, 16, -1), (1.6, 3, 1.6), (0.3, 1, 0), 'C', 5)
    s.part(5, 0, (0, 12, -1), 'Head')
    s.box((0, 11.5, -4), (5, 1, 4), 'P', 6)                   # jaw
    s.box((-1.5, 12, -5.5), (1, 1, 1), 'C', 6)
    s.box((1.5, 12, -5.5), (1, 1, 1), 'C', 6)
    s.part(6, 5, (0, 12, -2), 'Jaw')
    # tail
    s.point((0, 5, 6), (3, 5, 3), (0, -0.3, 1), 'P', 7)
    s.part(7, 0, (0, 5.5, 4), 'Tail')
    return s


@shape
def chain_chomp():
    s = Shape()
    s.ell((0, 5, 0), (9, 9, 9), 'D')
    s.box((0, 4, -3.5), (7, 1, 3), 'P', sub=True)              # mouth slit
    for x in (-3, -1, 1, 3):
        s.box((x, 4.5, -4), (1, 1, 1), 'C')
    s.ell((-2, 7, -3.5), (2, 3, 1), 'C')                        # eyes
    s.ell((2, 7, -3.5), (2, 3, 1), 'C')
    s.box((-2, 7, -4.2), (1, 1, 1), 'D')
    s.box((2, 7, -4.2), (1, 1, 1), 'D')
    s.ell((0, 8, -2), (1, 1, 1), 'A')
    # jaw part (lower half snaps)
    s.ell((0, 2.5, -1), (7, 3, 7), 'D', 1)
    for x in (-2, 0, 2):
        s.box((x, 3.5, -4), (1, 1, 1), 'C', 1)
    s.part(1, 0, (0, 4, 1), 'Jaw')
    # chain links behind (Other chain)
    prev = 0
    for i, z in enumerate([5.5, 7.5, 9.5]):
        pid = 2 + i
        s.cyl((0, 1.5, z), (1, 1, 2), 'D', pid, r=(0, 0, 90) if i % 2 else (90, 0, 0))
        s.part(pid, prev, (0, 1.5, z - 1), 'Other')
        prev = pid
    s.cyl((0, 1, 11), (1, 3, 1), 'A', 4)
    return s


@shape
def thwomp():
    s = Shape()
    s.box((0, 5.5, 0), (9, 11, 5), 'P')
    s.box((0, 5.5, 0), (7, 9, 5.4), 'A')
    s.box((-2, 7, -2.8), (3, 1, 1), 'D', r=(0, 0, -15))         # brows
    s.box((2, 7, -2.8), (3, 1, 1), 'D', r=(0, 0, 15))
    s.box((-2, 6, -2.8), (1, 1, 1), 'C')
    s.box((2, 6, -2.8), (1, 1, 1), 'C')
    s.box((0, 3, -2.8), (5, 2, 1), 'D')                          # mouth
    for x in (-2, 0, 2):
        s.box((x, 3, -3), (1, 1, 1), 'C')
    for y in (2, 5.5, 9):
        s.point((-5.5, y, 0), (2, 2, 2), (-1, 0, 0), 'C')
        s.point((5.5, y, 0), (2, 2, 2), (1, 0, 0), 'C')
    for x in (-3, 0, 3):
        s.point((x, 11.5, 0), (2, 2, 2), (0, 1, 0), 'C')
    return s


# ============================================================== DONKEY KONG / KIRBY / METROID / STAR FOX
@shape
def dk_barrel():
    s = Shape()
    s.ell((0, 3.5, 0), (9, 8, 8), 'P')
    s.cyl((0, 3.5, 0), (7, 9, 7), 'P', r=(0, 0, 90))
    s.cyl((-2.5, 3.5, 0), (8.4, 1, 8.4), 'D', r=(0, 0, 90))
    s.cyl((2.5, 3.5, 0), (8.4, 1, 8.4), 'D', r=(0, 0, 90))
    s.cyl((-4.5, 3.5, 0), (5, 1, 5), 'A', r=(0, 0, 90))
    s.cyl((4.5, 3.5, 0), (5, 1, 5), 'A', r=(0, 0, 90))
    return s


@shape
def kirby():
    s = Shape()
    s.ell((0, 3.5, 0), (7, 7, 7), 'P')
    s.box((-1, 4, -3.1), (1, 2, 1), 'D')
    s.box((1, 4, -3.1), (1, 2, 1), 'D')
    s.box((-1, 5, -3.2), (1, 1, 1), 'C')
    s.box((1, 5, -3.2), (1, 1, 1), 'C')
    s.box((-2.5, 3, -2.6), (1, 1, 1), 'A')
    s.box((2.5, 3, -2.6), (1, 1, 1), 'A')
    s.box((0, 2.5, -3.2), (1, 1, 1), 'D')
    for sx, role, pid in [(-1, 'LegBackLeft', 1), (1, 'LegBackRight', 2)]:
        s.ell((sx * 1.5, 0.5, -0.5), (3, 2, 4), 'A', pid)
        s.part(pid, 0, (sx * 1.5, 1.5, 0), role)
    for sx, role, pid in [(-1, 'ArmLeft', 3), (1, 'ArmRight', 4)]:
        s.ell((sx * 3.8, 3.5, 0), (2, 2, 2), 'P', pid)
        s.part(pid, 0, (sx * 3, 3.5, 0), role)
    return s


@shape
def warp_star():
    s = Shape()
    c = (0, 2, 0)
    s.ell(c, (7, 3, 7), 'P')
    for k in range(5):
        b = -90 + k * 72
        d = (-math.cos(math.radians(b)), 0, math.sin(math.radians(b)))
        s.seg((0, 2, 0), (d[0] * 5, 2, d[2] * 5), 3, 'P', d=2)
        s.seg((0, 2, 0), (d[0] * 7, 2, d[2] * 7), 1.2, 'P', d=1.2)
    s.box((0, 3, 0), (3, 1, 3), 'C')
    s.box((0, 2, 7.5), (1, 1, 3), 'A')                     # trail sparkles
    s.box((-1.5, 2, 9), (1, 1, 1), 'C')
    s.box((1.5, 2, 9), (1, 1, 1), 'C')
    return s


@shape
def arm_cannon():
    s = Shape()
    s.cyl((0, 0, -1.5), (5, 8, 5), 'P', r=(90, 0, 0))      # z -5.5..2.5
    s.cyl((0, 0, 2.5), (5.4, 1, 5.4), 'D', r=(90, 0, 0))   # arm opening
    s.cyl((0, 0, 2.5), (3, 1.2, 3), 'D', r=(90, 0, 0), sub=True)
    s.cyl((0, 0, -3), (5.6, 1, 5.6), 'A', r=(90, 0, 0))    # bands
    s.cyl((0, 0, 0), (5.6, 1, 5.6), 'A', r=(90, 0, 0))
    s.cyl((0, 0, -6), (3, 1, 3), 'D', r=(90, 0, 0))        # muzzle
    s.cyl((0, 0, -6.5), (1.5, 1, 1.5), 'C', r=(90, 0, 0))
    s.box((0, 3, -1), (1, 1, 5), 'A')                      # top ridge
    return s


@shape
def metroid():
    s = Shape()
    s.ell((0, 6, 0), (9, 7, 9), 'A')                       # membrane
    s.ell((0, 6.5, 0), (7, 5, 7), 'P', sub=False)
    for (x, z) in [(-1.5, -1), (1.5, -1), (0, 1.5)]:
        s.ell((x, 6, z), (3, 3, 3), 'C')                    # nuclei
    s.cyl((0, 3, 0), (7, 1, 7), 'D')                       # rim
    fangs = [(-2, -2, 'Jaw'), (2, -2, 'Jaw'), (-2, 2, 'Other'), (2, 2, 'Other')]
    for i, (x, z, role) in enumerate(fangs):
        pid = 1 + i
        s.seg((x, 3, z), (x * 0.6, 0, z * 1.3), 1.4, 'D', pid, kind='Cone')
        s.part(pid, 0, (x, 3, z), role)
    return s


@shape
def arwing():
    s = Shape()
    s.box((0, 2, 0.5), (3, 3, 9), 'P')                                     # fuselage
    s.point((0, 2, -6), (3, 4, 3), (0, 0, -1), 'P')                         # nose
    s.ell((0, 3.5, -1), (3, 2, 4), 'C')                                     # canopy
    s.seg((-1.5, 2, 1), (-7, 1.5, 4), 2, 'P', d=4)                          # swept wings
    s.seg((1.5, 2, 1), (7, 1.5, 4), 2, 'P', d=4)
    s.seg((-7, 1, 4), (-6, 6, 5), 1, 'A', d=4)                              # blade fins
    s.seg((7, 1, 4), (6, 6, 5), 1, 'A', d=4)
    s.box((-4, 1, 1), (1, 1, 5), 'D')                                       # laser pods
    s.box((4, 1, 1), (1, 1, 5), 'D')
    s.cyl((0, 2, 5.5), (3, 1, 3), 'C', r=(90, 0, 0))                        # engine glow
    s.box((0, 4, 3.5), (1, 2, 3), 'A')                                      # tail fin
    return s


# ============================================================== MINECRAFT
@shape
def tnt():
    s = Shape()
    s.box((0, 3, 0), (7, 7, 7), 'P')
    s.box((0, 3, 0), (7.2, 3, 7.2), 'C')                    # label band
    for x in (-2, 0, 2):
        for z in (-3.6, 3.6):
            s.box((x, 3, z), (1, 1, 1), 'D')
    for z in (-2, 0, 2):
        for x in (-3.6, 3.6):
            s.box((x, 3, z), (1, 1, 1), 'D')
    s.box((0, 6.6, 0), (5, 1, 5), 'D')
    s.box((0, 7, 0), (1, 1, 1), 'C')
    for (x, z) in [(-2, -2), (2, 2), (-2, 2), (2, -2)]:
        s.box((x, 1, z), (1, 1, 1), 'A')
    return s


@shape
def creeper():
    s = Shape()
    s.box((0, 6, 0), (5, 7, 3), 'P')                                        # body y 3..9
    for (x, y) in [(-1, 4), (1, 7), (2, 5), (-2, 8), (0, 6)]:
        s.box((x, y, -1), (1, 1, 1), 'A')
    s.box((0, 12, 0), (5, 5, 5), 'P', 5)                                    # head y 10..14
    s.box((-1.5, 12.5, -2), (2, 2, 1), 'D', 5)                              # face
    s.box((1.5, 12.5, -2), (2, 2, 1), 'D', 5)
    s.box((0, 11, -2), (1, 2, 1), 'D', 5)
    s.box((-1, 10.5, -2), (1, 2, 1), 'D', 5)
    s.box((1, 10.5, -2), (1, 2, 1), 'D', 5)
    s.box((0, 14, 1), (3, 1, 1), 'A', 5)
    s.part(5, 0, (0, 9.5, 0), 'Head')
    legs = [('LegFrontLeft', -1.5, -2.5), ('LegFrontRight', 1.5, -2.5), ('LegBackLeft', -1.5, 1.5),
            ('LegBackRight', 1.5, 1.5)]
    for i, (role, x, z) in enumerate(legs):
        pid = 1 + i
        s.box((x, 1, z), (2, 3, 2), 'P', pid)
        s.box((x, 0, z - 0.5 if z < 0 else z + 0.5), (2, 1, 1), 'D', pid)
        s.part(pid, 0, (x, 2.5, z), role)
    return s


@shape
def diamond_sword():
    s = Shape(turn=True)
    s.box((0, 0, 2), (1, 1, 1), 'D')
    s.box((0, 0, 1), (1, 1, 1), 'A')
    s.box((0, 0, 0), (1, 1, 1), 'D')
    s.box((0, 0, -1), (5, 1, 1), 'D')                         # guard
    s.box((-2, 0, -1), (1, 1, 1), 'A')
    s.box((2, 0, -1), (1, 1, 1), 'A')
    s.box((0, 0, -5.5), (3, 1, 8), 'D')                       # outline
    s.box((0, 0, -5.5), (1, 1, 8), 'P')                       # diamond body
    s.box((-0.5, 0, -6), (1, 1.2, 5), 'A')
    s.box((0, 0, -10), (1, 1, 1), 'D')
    s.box((0.5, 0, -4), (1, 1.2, 3), 'C')
    return s


@shape
def ender_dragon():
    s = Shape()
    s.box((0, 7, 0), (5, 5, 13), 'P')                                    # body z -6..6
    for z in (-4, -1, 2, 5):
        s.box((0, 10, z), (1, 1, 1), 'C')                                # spine plates
    # head + neck
    s.box((0, 8, -9), (3, 3, 6), 'P', 1)
    s.box((0, 8.5, -14), (5, 4, 6), 'P', 1)
    s.box((-2, 10, -14), (1, 1, 2), 'A', 1)                             # eyes
    s.box((2, 10, -14), (1, 1, 2), 'A', 1)
    s.box((-1.5, 11, -12), (1, 1, 1), 'C', 1)                           # horns
    s.box((1.5, 11, -12), (1, 1, 1), 'C', 1)
    s.part(1, 0, (0, 8, -6), 'Head')
    s.box((0, 6, -15), (3, 1, 5), 'D', 2)
    s.part(2, 1, (0, 7, -12), 'Jaw')
    # wings
    for sx, role, pid in [(-1, 'WingLeft', 3), (1, 'WingRight', 4)]:
        s.box((sx * 9.5, 9, -2), (15, 1, 1), 'D', pid)
        s.box((sx * 9.5, 8.5, 2), (15, 1, 7), 'P', pid)
        s.box((sx * 14, 8.5, 5.5), (5, 1, 1), 'A', pid)
        s.part(pid, 0, (sx * 2.5, 9, -2), role)
    # legs (short, tucked)
    for sx, role, pid, z in [(-1, 'LegBackLeft', 5, 3), (1, 'LegBackRight', 6, 3)]:
        s.box((sx * 2, 3, z), (1, 4, 1), 'D', pid)
        s.box((sx * 2, 1, z - 1), (1, 1, 3), 'C', pid)
        s.part(pid, 0, (sx * 2, 5, z), role)
    # tail chain
    prev = 0
    for i, (z, w) in enumerate([(9, 3), (14, 3), (19, 1)]):
        pid = 7 + i
        s.box((0, 7, z), (w, w, 5), 'P', pid)
        s.box((0, 7 + (w + 1) / 2, z), (1, 1, 1), 'C', pid)
        s.part(pid, prev, (0, 7, z - 2.5), 'Tail')
        prev = pid
    return s


# ============================================================== FORTNITE
@shape
def battle_bus():
    s = Shape()
    s.box((0, 5, 0), (7, 5, 15), 'P')                                   # body y 3..7
    s.box((0, 3.5, -8.5), (7, 3, 2), 'P')                               # hood
    s.box((0, 5.5, -7.6), (5, 2, 1), 'C')                               # windshield
    for z in (-5, -2, 1, 4):
        for x in (-3.6, 3.6):
            s.box((x, 6, z), (1, 2, 2), 'C')                            # windows
    s.box((0, 3, 0), (7.4, 1, 15.4), 'A')                               # stripe
    s.box((-2, 3.5, -9.6), (1, 1, 1), 'C')                              # headlights
    s.box((2, 3.5, -9.6), (1, 1, 1), 'C')
    s.ell((0, 16, 0), (11, 9, 11), 'A')                                 # balloon
    s.box((0, 12, 0), (3, 1, 3), 'D')
    for (x, z) in [(-3, -6), (3, -6), (-3, 6), (3, 6)]:
        s.seg((x, 7.5, z), (x * 0.4, 12, z * 0.2), 1, 'D')              # ropes
    for i, (x, z) in enumerate([(-4, -5), (4, -5), (-4, 5), (4, 5)]):
        pid = 1 + i
        s.cyl((x, 1.5, z), (3, 1, 3), 'D', pid, r=(0, 0, 90))
        s.part(pid, 0, (x, 1.5, z), 'Other')
    return s


# ============================================================== HALO
@shape
def energy_sword():
    s = Shape(turn=True)
    s.box((0, 0, 1), (1, 1, 3), 'D')                                   # handle z 0..2
    s.box((0, 0, -0.5), (3, 1, 1), 'A')
    for sx in (-1, 1):
        s.seg((sx * 1, 0, -0.5), (sx * 2, 0, -3), 1.2, 'P')
        s.seg((sx * 2, 0, -3), (sx * 2, 0, -7), 1.4, 'C', ext=0.5)
        s.box((sx * 2, 0, -5), (1, 3, 5), 'P')
        s.box((sx * 2, 0, -5), (1, 1, 5), 'C')
        s.seg((sx * 2, 0, -7), (sx * 1, 0, -10), 1.2, 'P')
        s.box((sx * 3, 0, -3), (1, 1, 1), 'P')                         # back hooks
    return s


@shape
def gravity_hammer():
    s = Shape(turn=True)
    s.box((0, 0, -3.5), (1, 1, 12), 'D')                               # shaft z -9..2
    s.box((0, 0, 2.5), (1, 1, 1), 'A')
    s.box((0, 0, -11), (5, 3, 4), 'P')                                 # head
    s.box((0, 0, -11), (5.4, 1, 1), 'C')                               # core
    s.box((3, 0, -11), (1, 5, 5), 'A')                                 # blunt face +X
    s.seg((-2.5, 0, -11), (-5.5, 0, -11), 3, 'P', d=3.5)               # blade side
    s.seg((-5, 0, -11), (-6.5, 0, -11), 1.2, 'C', d=4)
    s.box((0, 0, -8), (3, 1, 1), 'A')
    return s


@shape
def warthog():
    s = Shape()
    s.box((0, 3, 0.5), (7, 2, 15), 'P')                                 # chassis z -6.5..7.5
    s.box((0, 4.5, -4.5), (7, 2, 5), 'P')                               # hood
    s.box((0, 3.5, -8), (7, 3, 1), 'D')                                 # grill
    s.box((-2, 4, -8.6), (1, 1, 1), 'C')
    s.box((2, 4, -8.6), (1, 1, 1), 'C')
    s.box((0, 6.5, -1.5), (7, 1, 1), 'D')                               # roll bar
    s.box((-3, 5, -1.5), (1, 3, 1), 'D')
    s.box((3, 5, -1.5), (1, 3, 1), 'D')
    s.cyl((0, 5, 4.5), (3, 2, 3), 'A')                                  # turret base
    s.box((0, 7, 4), (3, 2, 3), 'A', 5)
    for x in (-0.5, 0.5):
        s.box((x * 2, 7, 1), (1, 1, 4), 'C', 5)                         # triple barrels
    s.box((0, 8, 1), (1, 1, 4), 'C', 5)
    s.part(5, 0, (0, 6, 4.5), 'Weapon')
    for i, (x, z) in enumerate([(-4.5, -4.5), (4.5, -4.5), (-4.5, 5), (4.5, 5)]):
        pid = 1 + i
        s.cyl((x, 2, z), (4, 2, 4), 'D', pid, r=(0, 0, 90))
        s.part(pid, 0, (x, 2, z), 'Other')
    return s


@shape
def plasma_grenade():
    s = Shape()
    s.ell((0, 2, 0), (3, 3, 3), 'C')
    for d in [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)]:
        s.seg((d[0] * 1.5, 0, d[2] * 1.5), (d[0] * 2, 4, d[2] * 2), 1, 'P')
    s.box((0, 4, 0), (3, 1, 3), 'A')
    s.box((0, 0, 0), (3, 1, 3), 'A')
    return s


@shape
def needler():
    s = Shape()
    s.box((0, -2, 1), (1, 3, 1), 'D')                                  # grip
    s.box((0, 0, -1), (3, 3, 6), 'P')                                  # body z -3.5..1.5
    s.point((0, 0, -4.5), (3, 2, 3), (0, 0, -1), 'P')
    s.box((0, 1, 2), (3, 1, 2), 'D')
    for z in (-2.5, -0.5, 1.5):
        for x in (-1, 1):
            s.point((x, 3, z), (1, 3, 1), (0, 1, 0.5), 'C')
    return s


# ============================================================== DOOM
@shape
def bfg_9000():
    s = Shape()
    s.box((0, 0, -2.5), (5, 5, 9), 'P')                                  # body z -6.5..1.5
    s.box((0, 0, -2.5), (5.4, 1, 7), 'A')                                # side stripe
    s.cyl((0, 0, -7), (5, 1, 5), 'D', r=(90, 0, 0))
    s.ell((0, 0, -7.5), (3, 3, 2), 'C')                                  # plasma mouth
    s.box((0, 3, -3), (1, 1, 5), 'D')                                    # carry handle
    s.box((0, 3.5, -1), (1, 1, 1), 'C')
    s.box((0, -3.5, 1), (1, 3, 2), 'D')                                  # grip
    s.box((0, 0, 2.5), (3, 3, 2), 'D')                                   # stock
    for sx in (-1, 1):
        s.box((sx * 3, 0, -4), (1, 3, 3), 'C')                           # cells
    return s


@shape
def doom_chainsaw():
    s = Shape()
    s.box((0, 0, 1.5), (3, 3, 4), 'P')                                   # engine z 0..3
    s.box((0, 2, 1.5), (1, 1, 3), 'D')                                   # top handle
    s.box((0, 0, 4), (1, 1, 1), 'D')
    s.box((0, 0, -4.5), (1, 3, 10), 'C')                                 # chain loop
    s.box((0, 0, -4.5), (1.2, 1, 8), 'D')                                # bar
    s.box((0, 1, 0.5), (3.4, 1, 1), 'A')
    return s


@shape
def cacodemon():
    s = Shape()
    s.ell((0, 6, 0), (11, 11, 11), 'P')
    s.ell((0, 8, -4.5), (3, 3, 2), 'C')                                  # eye
    s.box((0, 8, -5.5), (1, 1, 1), 'A')
    s.box((0, 4, -4), (7, 2, 4), 'D', sub=True)                         # maw
    for x in (-3, -1, 1, 3):
        s.box((x, 4.5, -4.5), (1, 1, 1), 'C')
    for (x, z) in [(-2, -1), (2, -1), (0, 1), (-3, 2), (3, 2)]:
        s.point((x, 11.5, z), (1.6, 2.5, 1.6), (x * 0.15, 1, 0), 'C')
    s.ell((0, 2.5, -2.5), (7, 3, 5), 'P', 1)
    for x in (-2, 0, 2):
        s.box((x, 3.5, -4.5), (1, 1, 1), 'C', 1)
    s.part(1, 0, (0, 4, 0), 'Jaw')
    return s


# ============================================================== PORTAL / HALF-LIFE
@shape
def portal_gun():
    s = Shape()
    s.box((0, 0, 0.5), (3, 3, 6), 'P')                                 # white body z -2..3
    s.ell((0, 0, 3.5), (3, 3, 3), 'D')                                 # rear
    s.box((0, 2, 0), (1, 1, 4), 'C')                                   # glass tube
    s.box((0, -2, 1), (1, 2, 2), 'D')                                  # grip
    s.cyl((0, 0, -2.5), (3, 1, 3), 'D', r=(90, 0, 0))
    s.ell((0, 0, -3.5), (1, 1, 1), 'C')                                # emitter
    for d in [(0, 1), (-1, -0.6), (1, -0.6)]:
        s.seg((d[0] * 1.4, d[1] * 1.4, -2.5), (d[0] * 1, d[1] * 1, -5), 1, 'D')
    s.box((0, 0, -1), (3.4, 1, 1), 'A')
    return s


@shape
def companion_cube():
    s = Shape()
    s.box((0, 3, 0), (7, 7, 7), 'P')
    for x in (-2.5, 2.5):
        for y in (0.5, 5.5):
            for z in (-2.5, 2.5):
                s.box((x, y, z), (2, 2, 2), 'D')
    for (ax, sgn) in [('z', -1), ('z', 1), ('x', -1), ('x', 1), ('y', 1)]:
        def at(u, w, off=3.6):
            if ax == 'z':
                return (u, 3 + w, sgn * off)
            if ax == 'x':
                return (sgn * off, 3 + w, u)
            return (u, 3 + off, w)
        big = (3, 3, 1) if ax == 'z' else ((1, 3, 3) if ax == 'x' else (3, 1, 3))
        s.box(at(0, 0, 3.4), big, 'C')
        for (u, w) in [(-1, 1), (1, 1), (0, 0), (0, -1)]:
            s.box(at(u, w), (1, 1, 1), 'A')
    return s


@shape
def gravity_gun():
    s = Shape()
    s.box((0, 0, 1), (3, 3, 6), 'D')                                   # body z -1.5..3.5
    s.box((0, 0, 1), (3.4, 1, 4), 'A')                                 # orange stripes
    s.box((0, 1.8, 0), (1, 1, 5), 'A')
    s.box((0, -2, 2), (1, 2, 2), 'D')                                  # grip
    s.ell((0, 0, -2.5), (3, 3, 3), 'C')                                # core
    for d in [(0, 1), (-0.87, -0.5), (0.87, -0.5)]:
        s.seg((d[0] * 1.5, d[1] * 1.5, -1.5), (d[0] * 3, d[1] * 3, -5), 1, 'P')
        s.box((d[0] * 3, d[1] * 3, -5.5), (1, 1, 1), 'A')
    return s


@shape
def crowbar():
    s = Shape()
    s.box((0, 0, -3), (1, 1, 11), 'P')                                  # shaft z -8..2
    s.seg((0, 0, -8), (0, 2, -9.5), 1, 'P')                             # front bend
    s.box((0, 2.5, -10), (1, 1, 1), 'A')
    s.seg((0, 0, 2), (0, 1.5, 3.5), 1, 'P')                              # hook
    s.seg((0, 1.5, 3.5), (0, 2.5, 2.5), 1, 'P')
    s.box((0, 3, 2), (1, 1, 1), 'A')
    s.box((0, 0, 0), (1.2, 1.2, 2), 'D')                                 # grip wear
    return s


@shape
def headcrab():
    s = Shape()
    s.ell((0, 2, 0.5), (6, 3, 6), 'P')
    s.box((0, 1, -1.5), (3, 1, 3), 'D', sub=False)                     # beak underside
    s.box((0, 1, -2.5), (1, 1, 1), 'C')
    legs = [('LegFrontLeft', -1, -1, (-2, 3, -5)), ('LegFrontRight', 1, -1, (2, 3, -5)),
            ('LegBackLeft', -1, 1, (-3, 0, 3)), ('LegBackRight', 1, 1, (3, 0, 3))]
    for i, (role, sx, sz, tip) in enumerate(legs):
        pid = 1 + i
        hip = (sx * 2, 2, sz * 1)
        knee = (sx * 3, 3, sz * 2.5)
        s.seg(hip, knee, 1, 'D', pid)
        s.seg(knee, (tip[0], 0 if sz > 0 else 0, tip[2]), 1, 'A' if sz < 0 else 'D', pid)
        s.part(pid, 0, hip, role)
    return s


# ============================================================== SOULS / MONSTER HUNTER
@shape
def moonlight_greatsword():
    s = Shape(turn=True)
    s.box((0, 0, 2.5), (1, 1, 1), 'A')
    s.box((0, 0, 1), (1, 1, 2), 'D')
    s.box((0, 0, -0.5), (5, 1, 1), 'A')
    s.box((-2, 0, -1.5), (1, 1, 1), 'A')
    s.box((2, 0, -1.5), (1, 1, 1), 'A')
    s.box((0, 0, -8), (3, 1, 14), 'P')                                  # z -14.5..-1.5
    s.box((0, 0, -8), (1, 1.2, 14), 'C')
    s.box((0, 0, -15.5), (1, 1, 2), 'C')
    for z in (-4, -9):
        s.box((0, 0, z), (5, 1, 1), 'C')                               # moonlight runes
    return s


@shape
def rivers_of_blood():
    s = Shape()
    s.box((0, 0, 1), (1, 1, 3), 'D')                                    # grip
    s.cyl((0, 0, -0.5), (3, 1, 3), 'A', r=(90, 0, 0))                   # tsuba
    n = 6
    pts = [(0, 0.08 * 14 * (i / n) ** 2 * 1.4, -1 - 14 * i / n) for i in range(n + 1)]
    for i in range(n):
        s.seg(pts[i], pts[i + 1], 1, 'P', d=1, ext=0.7)
        s.seg((pts[i][0], pts[i][1] - 1, pts[i][2]), (pts[i + 1][0], pts[i + 1][1] - 1, pts[i + 1][2]), 1, 'C', ext=0.7)
    s.box((0, pts[-1][1] - 0.5, pts[-1][2] - 1), (1, 1, 1), 'C')
    return s


@shape
def rathalos():
    s = Shape()
    s.ell((0, 7, 0), (7, 6, 12), 'P')                                   # body
    for z in (-3, 0, 3):
        s.box((0, 10, z), (1, 1, 1), 'C')
    s.seg((0, 8, -5), (0, 10, -9), 3, 'P', 1)                            # neck
    s.box((0, 10.5, -11), (3, 3, 5), 'P', 1)                             # head
    s.box((-1.5, 11.5, -11), (1, 1, 1), 'C', 1)
    s.box((1.5, 11.5, -11), (1, 1, 1), 'C', 1)
    s.point((-1, 12.5, -9), (1, 3, 1), (0, 0.5, 1), 'A', 1)             # horns back
    s.point((1, 12.5, -9), (1, 3, 1), (0, 0.5, 1), 'A', 1)
    s.part(1, 0, (0, 8, -5), 'Head')
    s.box((0, 9, -12.5), (3, 1, 4), 'D', 2)
    s.part(2, 1, (0, 9.5, -10), 'Jaw')
    for sx, role, pid in [(-1, 'WingLeft', 3), (1, 'WingRight', 4)]:
        s.seg((sx * 3, 10, -2), (sx * 12, 13, 0), 1.4, 'D', pid)          # wing arm
        s.seg((sx * 12, 13, 0), (sx * 15, 8, 4), 1.2, 'D', pid)
        s.seg((sx * 4, 9.5, 1.5), (sx * 12, 10.5, 3.5), 1, 'P', pid, d=7)  # membrane
        s.box((sx * 12, 13.5, -0.5), (1, 1, 1), 'C', pid)
        s.part(pid, 0, (sx * 3, 10, -2), role)
    for sx, role, pid in [(-1, 'LegBackLeft', 5), (1, 'LegBackRight', 6)]:
        s.seg((sx * 3, 6, 1), (sx * 3, 3, -1), 2.4, 'P', pid)
        s.seg((sx * 3, 3, -1), (sx * 3, 0.5, 1), 1.6, 'D', pid)
        s.box((sx * 3, 0, -0.5), (3, 1, 3), 'C', pid)
        s.part(pid, 0, (sx * 3, 6.5, 1), role)
    prev = 0
    for i, (a, b, w) in enumerate([((0, 7, 6), (0, 6, 11), 3), ((0, 6, 11), (0, 5, 16), 2.2), ((0, 5, 16), (0, 4.5, 20), 1.6)]):
        pid = 7 + i
        s.seg(a, b, w, 'P', pid)
        s.part(pid, prev, a, 'Tail')
        prev = pid
    s.ell((0, 4.5, 20.5), (3, 2, 3), 'C', 9)                               # tail club
    return s


# ============================================================== FINAL FANTASY / KINGDOM HEARTS
@shape
def buster_sword():
    s = Shape(turn=True)
    s.box((0, 0, 3.5), (1, 1, 1), 'A')                                    # pommel
    s.box((0, 0, 1.5), (1, 1, 4), 'D')                                    # long grip z 0..3
    s.box((0, 0, -0.5), (5, 3, 1), 'A')                                   # collar
    s.box((0, 0, -7.5), (5, 1, 13), 'P')                                  # blade z -13.5..-1.5
    s.box((0, 0, -15), (6, 3, 6), 'P', r=(0, 45, 0), sub=True)            # angled tip cut (left)
    s.box((-2, 0, -12), (1, 1.2, 3), 'P', sub=False)
    s.box((2, 0, -7), (1, 1.2, 13), 'C')                                  # edge
    s.cyl((0, 0, -2.5), (1, 3, 1), 'D', sub=True)                          # two holes
    s.cyl((0, 0, -4.5), (1, 3, 1), 'D', sub=True)
    return s


@shape
def gunblade():
    s = Shape()
    s.box((0, -2, 1.5), (1, 3, 1), 'D', r=(20, 0, 0))                    # pistol grip
    s.cyl((0, -0.5, 0.5), (3, 2, 3), 'D', r=(90, 0, 0))                    # revolver cylinder
    s.box((0, -1.5, -0.5), (1, 1, 1), 'A')                                # trigger guard
    s.box((0, 0.5, -0.5), (1, 4, 1), 'A')
    s.box((0, 1, -5.5), (1, 3, 9), 'P')                                   # blade z -9.5..-1.5
    s.box((0, 0, -5), (1.2, 1, 8), 'C')
    s.box((0, 1.5, -10.5), (1, 2, 1), 'P')
    s.box((0, 0.8, -1.5), (1, 1, 1), 'D')                                 # barrel
    return s


@shape
def chocobo():
    s = Shape()
    s.ell((0, 7, 0), (5, 5, 7), 'P')
    s.seg((0, 8.5, -2), (0, 11, -3), 2, 'P', 1)                           # neck
    s.ell((0, 12, -3), (3, 3, 4), 'P', 1)
    s.box((0, 11.5, -5.5), (1, 1, 2), 'A', 1)                             # beak
    s.box((-1.5, 12.5, -4), (1, 1, 1), 'D', 1)
    s.box((1.5, 12.5, -4), (1, 1, 1), 'D', 1)
    s.seg((0, 13, -2), (0, 14.5, 0), 1, 'P', 1)                           # crest
    s.seg((0, 13, -2.5), (0, 15, -1.5), 1, 'P', 1)
    s.part(1, 0, (0, 9, -2), 'Head')
    for sx, role, pid in [(-1, 'WingLeft', 2), (1, 'WingRight', 3)]:
        s.ell((sx * 2.8, 7.5, 0.5), (1, 3, 4), 'P', pid)
        s.part(pid, 0, (sx * 2.2, 8.5, 0), role)
    s.seg((0, 7.5, 3), (0, 10, 5), 3, 'P', 4, d=1)                        # tail fan
    s.part(4, 0, (0, 7.5, 3), 'Tail')
    for sx, role, pid in [(-1, 'LegBackLeft', 5), (1, 'LegBackRight', 6)]:
        s.seg((sx * 1.5, 5, 0), (sx * 1.5, 3, 1), 1.6, 'P', pid)
        s.seg((sx * 1.5, 3, 1), (sx * 1.5, 0.5, 0), 1, 'A', pid)
        s.box((sx * 1.5, 0, -0.5), (1, 1, 3), 'A', pid)
        s.part(pid, 0, (sx * 1.5, 5.5, 0), role)
    return s


@shape
def keyblade():
    s = Shape(turn=True)
    s.box((0, 0, 0.5), (1, 1, 2), 'D')                                    # grip
    s.box((-1.5, 0, 0.5), (1, 1, 3), 'A')                                 # guard frame
    s.box((1.5, 0, 0.5), (1, 1, 3), 'A')
    s.box((0, 0, 2.5), (3, 1, 1), 'A')
    s.box((0, 0, -1), (5, 1, 1), 'A')
    s.box((0, 0, -5), (1, 1, 8), 'P')                                     # shaft z -9..-1
    s.box((1.5, 0, -8.5), (2, 1, 2), 'P')                                 # teeth
    s.box((1, 0, -6.5), (1, 1, 1), 'P')
    s.box((2, 0, -7), (1, 1, 1), 'C')
    s.box((0, 0, 3.5), (1, 1, 1), 'D')                                    # keychain
    s.box((0, 0, 4.5), (1, 1, 1), 'C')
    return s


# ============================================================== GOD OF WAR
@shape
def leviathan_axe():
    s = Shape(turn=True)
    s.box((0, 0, -1.5), (1, 1, 8), 'D')                                   # haft z -5..2
    for z in (-3, -1, 1):
        s.box((0, 0, z), (1.2, 1.2, 1), 'A')
    s.box((1, 0, -5.5), (3, 1, 3), 'P')                                   # socket
    s.ell((3, 0, -5.5), (4, 1, 7), 'C')                                   # edge glow
    s.ell((2.5, 0, -5.5), (4, 1, 6), 'P')                                 # blade
    s.box((2, 0, -5.5), (1, 1.2, 1), 'A')                                 # rune
    s.box((-1, 0, -5.5), (1, 1, 1), 'D')                                  # back spike
    s.box((0, 0, 2.5), (1, 1, 1), 'A')
    return s


@shape
def blades_of_chaos():
    s = Shape(turn=True)
    s.box((0, 0, 0.5), (1, 1, 2), 'D')
    s.box((0, 0, -1), (3, 1, 1), 'A')
    s.box((0, 0, -3.5), (3, 1, 5), 'P')
    s.box((0, 0, -3.5), (1, 1.2, 4), 'C')
    s.seg((1, 0, -6), (0, 0, -8), 1.4, 'P')                               # hooked tip
    s.seg((-1, 0, -6), (-2, 0, -7.5), 1, 'P')
    prev = 0
    for i, z in enumerate([2.5, 4.5, 6.5]):
        pid = 1 + i
        s.box((0, -0.5 * i, z), (1, 1, 2), 'D', pid, r=(0, 0, 45 * (i % 2)))
        s.part(pid, prev, (0, -0.5 * i, z - 1), 'Other')
        prev = pid
    return s


# ============================================================== METAL GEAR / TITANFALL
@shape
def metal_gear_rex():
    s = Shape()
    s.box((0, 17, 2), (9, 6, 11), 'P')                                    # body
    s.box((0, 17, 2), (9.4, 2, 9), 'A')
    s.box((0, 21, 5), (5, 3, 5), 'D')                                     # dorsal radome
    for sx, role, pid in [(-1, 'LegBackLeft', 1), (1, 'LegBackRight', 2)]:
        s.seg((sx * 5, 15, 2), (sx * 6, 9, -3), 3.4, 'P', pid)            # thigh
        s.box((sx * 6, 9, -3), (3, 3, 3), 'C', pid)                       # knee (weak)
        s.seg((sx * 6, 9, -3), (sx * 6, 2, 2), 3, 'D', pid)                # reverse shin
        s.box((sx * 6, 1, -0.5), (5, 2, 9), 'P', pid)                     # foot
        s.box((sx * 6, 0.5, -5), (5, 1, 1), 'C', pid)
        s.part(pid, 0, (sx * 5, 15.5, 2), role)
    s.box((0, 19, -6), (5, 4, 9), 'P', 3)                                 # head / cockpit
    s.point((0, 19, -12), (5, 4, 4), (0, 0, -1), 'P', 3)
    s.ell((0, 21, -6), (3, 2, 5), 'C', 3)                                 # cockpit canopy
    s.part(3, 0, (0, 19, -2), 'Head')
    s.cyl((6, 20, -7), (1.6, 18, 1.6), 'A', 4, r=(90, 0, 0))              # railgun
    s.box((6, 20, 1), (3, 3, 5), 'D', 4)
    s.part(4, 0, (5, 19, 0), 'Weapon')
    s.box((-6, 20, 0), (3, 4, 6), 'D', 5)                                 # missile pod
    for z in (-2, 0, 2):
        s.box((-6, 22.4, z), (1, 1, 1), 'C', 5)
    s.part(5, 0, (-5, 19, 0), 'Weapon')
    return s


@shape
def titan_bt():
    s = Shape()
    s.box((0, 13, 0), (9, 7, 7), 'P')                                     # hull
    s.box((0, 13, -3.6), (7, 5, 1), 'A')                                  # hatch
    s.ell((0, 15.5, -3.8), (2, 2, 1), 'C')                                # cyclops eye
    s.box((0, 9, 0), (5, 2, 5), 'D')                                      # waist
    s.box((-3, 16.6, 2), (1, 3, 1), 'D')                                  # antenna
    for sx, role, pid in [(-1, 'LegBackLeft', 1), (1, 'LegBackRight', 2)]:
        s.box((sx * 2.5, 6, 0), (3, 5, 3), 'P', pid)
        s.box((sx * 2.5, 2.5, 0), (3, 3, 3), 'D', pid)
        s.box((sx * 2.5, 0.5, -0.5), (3, 1, 5), 'P', pid)
        s.part(pid, 0, (sx * 2.5, 8.5, 0), role)
    for sx, role, pid in [(-1, 'ArmLeft', 3), (1, 'ArmRight', 4)]:
        s.box((sx * 6, 15, 0), (3, 3, 3), 'D', pid)                        # shoulder
        s.box((sx * 6, 11.5, 0), (3, 4, 3), 'P', pid)
        s.box((sx * 6, 8, -1), (3, 3, 3), 'D', pid)                        # fist
        s.part(pid, 0, (sx * 5, 15, 0), role)
    s.box((6, 8, -5), (2, 3, 7), 'A', 5)                                   # XO-16 rifle
    s.box((6, 8, -9), (1, 1, 2), 'C', 5)
    s.part(5, 4, (6, 8, -1), 'Weapon')
    return s


# ============================================================== AMONG US / RETRO
@shape
def crewmate():
    s = Shape()
    s.ell((0, 5.5, 0), (5, 5, 5), 'P')
    s.box((0, 3.5, 0), (5, 4, 5), 'P')                                     # body y 1.5..5.5
    s.ell((0, 5.5, -2.2), (3, 2, 2), 'A')                                  # visor
    s.box((0.5, 6, -3), (1, 1, 1), 'C')
    s.box((0, 4, 3), (3, 4, 1), 'P')                                       # backpack
    for sx, role, pid in [(-1, 'LegBackLeft', 1), (1, 'LegBackRight', 2)]:
        s.box((sx * 1.5, 0.5, 0), (2, 2, 4), 'P', pid)
        s.part(pid, 0, (sx * 1.5, 2, 0), role)
    return s


@shape
def pac_man_ghost():
    s = Shape()
    s.ell((0, 5, 0), (7, 6, 7), 'P')
    s.box((0, 2.5, 0), (7, 4, 7), 'P')
    for x in (-2, 0, 2):
        s.box((x, 0.5, 0), (1, 1, 9), 'P', sub=True)                       # wavy skirt
    for sx in (-1, 1):
        s.ell((sx * 1.5, 5, -3), (2, 3, 1), 'C')
        s.box((sx * 1.5, 4.5, -3.5), (1, 1, 1), 'A')
    return s


@shape
def tetris_t():
    s = Shape()
    for (x, y) in [(-3, 5), (0, 5), (3, 5), (0, 2)]:
        s.box((x, y, 0), (3, 3, 3), 'P')
        s.box((x, y, -1.2), (1, 1, 1), 'C')
        s.box((x, y, 1.2), (1, 1, 1), 'C')
        s.box((x, y + 1, 0), (1, 1.2, 1), 'A')
    return s


# ============================================================== registry helper
def get(name):
    return S[name]()
