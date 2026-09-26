"""Held items, objects, structures, worn."""
import math
from shapes import S, katana, fist, ALONG_Z, ALONG_X, TIP_NZ, TIP_PZ, TIP_NY, TIP_PX, TIP_NX, octagon_fill, octagon_ring


def kamehameha():
    s = S()
    s.cyl((0, 0, -4), (7, 1, 7), 'A', ALONG_Z)          # aura ring behind the ball
    s.ell((0, 0, -4), (5, 5, 5), 'C')                    # the ball
    s.ell((0, 0, -5), (3, 3, 3), 'C')
    for sx in (-1, 1):                                   # cupped palms
        s.box((sx * 3, 0, -3), (1, 4, 4), 'D', (0, sx * 20, 0))
        s.box((sx * 2, 2, -4.5), (1, 1, 3), 'D')         # fingers over the top
        s.box((sx * 2, -2, -4.5), (1, 1, 3), 'D')
        s.seg((sx * 3, 0, -1), (sx * 1, -1, 2), 1, 1, 'D')  # wrists
    return s


def spirit_bomb():
    s = S()
    s.ell((0, 0, 0), (13, 13, 13), 'C')
    s.ell((0, 0, 0), (14, 11, 14), 'A', (0, 0, 20))       # swirling energy band
    s.ell((0, 0, 0), (13, 13, 13), 'C')
    for k in range(8):                                    # gathered motes streaming in
        a = k * math.pi / 4
        r = 9.5
        s.ell((r * math.cos(a), 2.5 * math.sin(3 * a), r * math.sin(a)), (2, 2, 2), 'A' if k % 2 else 'P')
    return s


def nimbus():
    s = S()
    s.ell((0, 0, 0), (9, 3, 6), 'A')
    s.ell((-3, 1, 0.5), (5, 3, 5), 'A')
    s.ell((3, 1, -0.5), (5, 3, 5), 'A')
    s.ell((0, 1, -2), (4, 3, 4), 'A')
    s.ell((0, 0.5, 4), (4, 2, 5), 'A')                     # trailing wisp
    s.ell((1, 1, 7), (2, 2, 4), 'A')
    s.ell((1.5, 1.5, 9), (1, 1, 2), 'A')
    s.ell((-2, 2, 0), (2, 1, 2), 'C'); s.ell((2, 2, -1), (2, 1, 2), 'C')
    return s


def power_pole():
    s = S()
    s.cyl((0, 0, -4), (1, 17, 1), 'A', ALONG_Z)
    s.cyl((0, 0, 4.5), (1.6, 1.4, 1.6), 'C', ALONG_Z)
    s.cyl((0, 0, -12.5), (1.6, 1.4, 1.6), 'C', ALONG_Z)
    return s


def dragon_ball():
    s = S()
    s.ell((0, 0, 0), (5, 5, 5), 'A')
    for c in ((0, 0, -2), (0, 1, -2), (0, -1, -2), (1, 0, -2), (-1, 0, -2)):
        s.box(c, (1, 1, 1), 'C')
    return s


def destructo_disc():
    s = S()
    s.cyl((0, 0, -5), (9, 1, 9), 'A')
    s.cyl((0, 0, -5), (7, 1, 7), 'C')
    for k in range(8):
        a = k * 45
        s.box((4.6 * math.cos(math.radians(a)), 0, -5 + 4.6 * math.sin(math.radians(a))), (2, 1, 1), 'C', (0, -a + 30, 0))
    return s


def special_beam_cannon():
    s = S()
    s.vb(0, 0, -1, -1, -2, 1, 'D')                        # index finger
    s.vb(1, 1, -1, -1, -2, 1, 'D')                        # middle finger
    s.vb(0, 1, -2, -2, 0, 2, 'D')                         # hand
    s.vb(0, 0, -1, -1, -10, -3, 'C')                      # beam core
    for i in range(16):
        a = i * 0.9
        s.box((0.5 * 0 + 1.6 * math.cos(a), -1 + 1.6 * math.sin(a), -3 - i * 0.5), (1, 1, 1), 'A')
    return s


def rasengan():
    s = S()
    s.ell((0, 0, 0), (5, 5, 5), 'C')
    s.cyl((0, 0, 0), (8, 1, 8), 'A', (20, 0, 0))
    s.cyl((0, 0, 0), (8, 1, 8), 'A', (0, 0, 60))
    s.cyl((0, 0, 0), (8, 1, 8), 'P', (-50, 0, -40))
    s.ell((0, 0, 0), (5, 5, 5), 'C')
    return s


def rasenshuriken():
    s = S()
    c = (0, 0, -7)
    for k in range(4):
        yaw = k * 90 + 20
        a = math.radians(yaw)
        dx, dz = math.cos(a), -math.sin(a)
        s.cone((c[0] + dx * 4.5, 0, c[2] + dz * 4.5), (1, 8, 4), 'P', (0, yaw, -90))
        s.cone((c[0] + dx * 4.5, 0, c[2] + dz * 4.5), (1, 6, 2), 'A', (0, yaw, -90))
    s.cyl(c, (7, 1, 7), 'A')
    s.ell(c, (5, 5, 5), 'C')
    return s


def chidori():
    s = S()
    s.vb(-1, 1, -1, 1, 1, 4, 'D')                         # forearm
    s.ell((0, 0, -1), (4, 4, 4), 'C')                    # crackling fist
    dirs = [(1, 1, -1), (-1, 1, -1), (1, -1, -1), (-1, -1, -1), (0, 1.4, 0), (1.4, 0, 0.3), (-1.4, 0, 0.3), (0, -1.4, 0.2), (0, 0, -1.6)]
    for d in dirs:
        n = math.sqrt(sum(v * v for v in d))
        a = (0, 0, -1)
        b = (d[0] / n * 5, d[1] / n * 5 + 0, -1 + d[2] / n * 5)
        mid = ((a[0] + b[0]) / 2 + d[1] * 0.6, (a[1] + b[1]) / 2 - d[0] * 0.6, (a[2] + b[2]) / 2)
        s.seg(a, mid, 1, 1, 'A'); s.seg(mid, b, 1, 1, 'C')
    return s


def kunai(three_prong=False):
    s = S()
    s.cyl((0, 0, 3.5), (3, 1, 3), 'A', ALONG_X)            # ring pommel
    s.cyl((0, 0, 3.5), (1, 1, 1), 'A', ALONG_X, sub=True)
    s.vb(0, 0, 0, 0, -1, 2, 'D')                          # wrapped handle
    s.box((0, 0, -3), (1, 3, 3), 'P', (45, 0, 0))         # blade diamond
    s.cone((0, 0, -5.5), (1, 5, 2.5), 'P', TIP_NZ)
    s.vb(0, 0, 0, 0, -7, -2, 'C')
    if three_prong:
        for sy in (-1, 1):
            s.seg((0, sy * 1, -2), (0, sy * 3, -5), 1, 1, 'P')
            s.cone((0, sy * 3.2, -5.6), (1, 2, 1.2), 'C', TIP_NZ)
        s.vb(0, 0, -1, 1, 0, 0, 'A')
    return s


def shuriken():
    s = S()
    c = (0, 0, -5)
    for k in range(4):
        yaw = k * 90 + 45
        a = math.radians(yaw)
        s.cone((math.cos(a) * 3, 0, c[2] - math.sin(a) * 3), (1, 6, 3), 'P', (0, yaw, -90))
    s.cyl(c, (4, 1, 4), 'A')
    s.cyl(c, (1, 1, 1), 'A', sub=True)
    return s


def gum_gum_pistol():
    s = S()
    s.cyl((0, 0, 1), (3, 2, 3), 'A', ALONG_Z)             # red sleeve cuff
    s.cyl((0, 0, -5), (1.6, 11, 1.6), 'P', ALONG_Z)       # the stretched arm
    s.vb(-1, 1, -1, 1, -11, -11, 'P')                     # wrist flare
    fist(s, -13, 3, 3, 3)
    s.box((0, 1, -9), (3, 1, 1), 'D'); s.box((0, -1, -6), (3, 1, 1), 'D')  # motion rings
    return s


def gear5():
    s = S()
    s.ell((0, 0, -6), (8, 7, 7), 'P')                    # cartoon fist
    for x in (-2.5, -0.8, 0.8, 2.5):
        s.ell((x, 2.5, -8.5), (2, 2, 2), 'P')             # knuckles
    s.ell((-4, 0, -5), (2, 3, 4), 'P')                   # thumb
    s.box((0, -0.5, -9.5), (6, 1, 1), 'D')                # finger crease
    s.ell((0, 0, -1), (5, 4, 4), 'P')                    # wrist
    for k in range(6):                                    # cloud cuff (Nika's steam)
        a = k * math.pi / 3
        s.ell((3 * math.cos(a), 3 * math.sin(a), 1), (3, 3, 3), 'C')
    s.ell((0, 0, 2), (4, 4, 2), 'C')
    return s


def rocket_punch():
    s = S()
    fist(s, -3, 5, 5, 5, 'P')
    s.vb(-2, 2, -2, 2, -1, 0, 'A')                        # wrist ring
    s.cyl((0, 0, 3), (4, 6, 4), 'P', ALONG_Z)             # forearm
    for sx in (-1, 1):
        s.box((sx * 2.5, 0, 4.5), (1, 3, 3), 'D')          # fins
    s.box((0, 2.5, 4.5), (1, 1, 3), 'D')
    s.cone((0, 0, 7.5), (3, 3, 3), 'C', TIP_PZ, part=1)  # exhaust flame (flickers)
    s.part(1, 0, (0, 0, 6), 'Other')
    return s


def bakugo_gauntlet():
    s = S()
    s.ell((0, 0, 3), (5, 5, 7), 'A')                     # grenade bracer
    for z in (1, 3, 5):
        s.cyl((0, 0, z), (5.4, 1, 5.4), 'D', ALONG_Z)
    s.vb(0, 0, 3, 4, 5, 6, 'C'); s.cyl((1, 4, 6), (2, 1, 2), 'C', ALONG_X)  # pin + ring
    s.vb(0, 0, 3, 3, 1, 4, 'D')                           # lever
    s.vb(-1, 1, -1, 1, -1, 0, 'D')
    fist(s, -2, 3, 3, 3)
    return s


def killua_yoyo():
    s = S()
    s.vb(0, 0, 0, 0, -5, 0, 'C')                          # string
    for x in (-1, 1):
        s.cyl((x, 0, -8), (5, 1, 5), 'P', ALONG_X)
        s.cyl((x * 1.0, 0, -8), (3, 1, 3), 'A', ALONG_X)
    s.cyl((0, 0, -8), (1, 1, 1), 'D', ALONG_X)
    s.box((0, 0, -8), (3, 1, 1), 'C')
    return s


def lance_of_longinus():
    s = S()
    s.cyl((0, 0, -2), (1, 13, 1), 'A', ALONG_Z)           # shaft z 4..-8
    for i in range(4):                                    # twisted wrap before the fork
        s.box((0, 0, -6 - i), (2, 1, 1), 'A', (0, 0, 45 * i))
    for sy in (-1, 1):                                    # the two prongs
        s.seg((0, 0, -9), (0, sy * 2, -13), 1, 1, 'A')
        s.seg((0, sy * 2, -13), (0, sy * 1, -19), 1, 1, 'A')
        s.cone((0, sy * 0.8, -20), (1, 2.5, 1), 'C', TIP_NZ)
    return s


def giga_drill():
    s = S()
    s.cyl((0, 0, 1), (1, 5, 1), 'D', ALONG_Z)
    s.cyl((0, 0, -1.5), (5, 1, 5), 'A', ALONG_Z)
    s.cone((0, 0, -9), (6, 14, 6), 'P', TIP_NZ)
    for k in range(5):                                    # spiral flutes
        z = -3.5 - k * 2.4
        r = 5.5 * (1 - (k * 2.4 + 1) / 14)
        s.cyl((0, 0, z), (r + 1, 1, r + 1), 'A', (90, 0, 0))
    s.cone((0, 0, -15.5), (1.5, 3, 1.5), 'C', TIP_NZ)
    return s


def beam_saber():
    s = S()
    s.cyl((0, 0, 0.5), (1.4, 5, 1.4), 'D', ALONG_Z)
    s.cyl((0, 0, -2.2), (2, 1, 2), 'A', ALONG_Z)
    s.cyl((0, 0, -7.5), (1.2, 10, 1.2), 'C', ALONG_Z)
    return s


def zangetsu(dark=False):
    s = S()
    s.vb(0, 0, 0, 0, -1, 3, 'D')                          # cloth-wrapped hilt
    s.vb(0, 0, -1, 1, -2, -2, 'D')                        # collar
    s.vb(0, 0, -3, 1, -11, -3, 'D' if dark else 'P')      # the cleaver slab
    s.vb(0, 0, -3, -3, -11, -3, 'C')                      # edge
    s.box((0, -3, -11), (3, 3, 3), 'P', (45, 0, 0), sub=True)  # angled tip
    s.vb(0, 0, 1, 1, -11, -4, 'A')                        # spine
    s.cyl((0, 0, -2.5), (1, 1, 1), 'P', sub=True)
    # hanging cloth tail from the pommel (animated chain)
    s.part(1, 0, (0, 0, 3), 'Other'); s.vb(0, 0, -2, 0, 4, 4, 'A', part=1)
    s.part(2, 1, (0, -2, 4), 'Other'); s.vb(0, 0, -5, -3, 4, 4, 'A', part=2)
    return s


def tensa_zangetsu():
    s = S()
    s.vb(0, 0, 0, 0, -1, 2, 'D')
    for dy, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)):     # manji-style guard
        s.vb(0, 0, dy, dy + (1 if dy > 0 else -1 if dy < 0 else 0), -3 + dz, -3 + dz, 'A')
    s.vb(0, 0, -2, 2, -3, -3, 'A'); s.vb(0, 0, 2, 2, -4, -4, 'A'); s.vb(0, 0, -2, -2, -2, -2, 'A')
    s.vb(0, 0, -1, 0, -11, -4, 'D')
    s.vb(0, 0, -1, -1, -11, -4, 'C')
    s.vb(0, 0, -1, -1, -12, -12, 'C')
    # broken chain from the pommel
    s.part(1, 0, (0, 0, 3), 'Other'); s.vb(0, 0, -1, 0, 3, 3, 'A', part=1)
    s.part(2, 1, (0, -1, 3), 'Other'); s.vb(0, 0, -3, -2, 3, 3, 'D', part=2)
    s.part(3, 2, (0, -3, 3), 'Other'); s.vb(0, 0, -5, -4, 3, 3, 'A', part=3)
    return s


def santoryu():
    s = S()
    s.vb(0, 0, 0, 0, -1, 2, 'D')
    for ang, t in ((-14, 'P'), (0, 'A'), (14, 'P')):     # three fanned katanas
        a = math.radians(ang)
        tip = (0, -math.sin(a) * 10 - 0.5, -2 - math.cos(a) * 10)
        s.seg((0, -0.5, -2), tip, 1, 1, t)
        s.seg((0, -math.sin(a) * 2 - 1, -2 - math.cos(a) * 2), (tip[0], tip[1] - 0.8, tip[2]), 1, 1, 'C')
    s.cyl((0, 0, -1.5), (3, 1, 3), 'A', ALONG_Z)
    return s


def yoru():
    s = S()
    s.vb(0, 0, 0, 0, -1, 3, 'D')
    s.ell((0, 0, 4), (2, 2, 2), 'A')                     # jewelled pommel
    s.vb(0, 0, -4, 4, -2, -2, 'A')                        # cross guard (ornate)
    s.ell((0, 0, -2), (1, 3, 3), 'C')
    s.cone((0, 4.5, -2), (1, 2, 1), 'A'); s.cone((0, -4.5, -2), (1, 2, 1), 'A', TIP_NY)
    s.vb(0, 0, -1, 1, -15, -3, 'D')                       # long black blade
    s.vb(0, 0, -1, -1, -15, -3, 'C'); s.vb(0, 0, 1, 1, -15, -3, 'C')
    s.cone((0, 0, -16.5), (1, 3, 3), 'D', TIP_NZ)
    return s


def dragonslayer():
    s = S()
    s.vb(0, 0, 0, 0, -1, 3, 'D')
    s.vb(0, 0, 4, 4, 4, 4, 'A')
    s.vb(0, 1, -2, 2, -2, -2, 'A')                        # plain guard
    s.vb(0, 1, -2, 1, -16, -3, 'P')                       # huge slab, 2 thick
    s.vb(0, 1, -2, -2, -16, -3, 'D')                      # nicked iron edge
    s.box((0.5, -2.5, -17), (2, 3, 3), 'P', (45, 0, 0), sub=True)
    s.box((0.5, 1.5, -17), (2, 3, 3), 'P', (45, 0, 0), sub=True)
    s.vb(0, 1, -1, -1, -15, -4, 'A')
    for z in (-6, -10):
        s.vb(0, 1, -2, -2, z, z, 'P', sub=True)          # battle notches
    return s


def scissor_blade():
    s = S()
    s.cyl((0, 1, 1), (5, 1, 5), 'A', ALONG_X)             # big loop handle around the hand
    s.cyl((0, 1, 1), (3, 1, 3), 'A', ALONG_X, sub=True)
    s.vb(0, 0, 0, 0, 0, 0, 'D')
    s.vb(0, 0, -1, 1, -3, -2, 'D')                        # pivot boss
    s.cyl((0, 0, -2.5), (1, 1, 1), 'C', ALONG_X)
    s.vb(0, 0, -1, 1, -13, -4, 'A')                       # long red half-scissor blade
    s.box((0, 2, -13), (1, 3, 10), 'A', (-12, 0, 0), sub=True)  # taper the spine to a point
    s.vb(0, 0, -1, -1, -13, -4, 'C')
    return s


def tessaiga():
    s = S()
    s.vb(0, 0, 0, 0, -1, 2, 'D')
    s.ell((0, 0, -2.5), (4, 4, 3), 'A')                  # fur ruff at the guard
    s.vb(0, 0, -2, 1, -12, -4, 'P')                       # broad fang blade
    s.ell((0, -0.5, -12.5), (1, 4, 4), 'P')              # rounded fang tip
    s.vb(0, 0, -2, -2, -12, -4, 'C')
    s.box((0, 2.5, -8), (1, 3, 10), 'P', (8, 0, 0), sub=True)  # fang curve on the spine
    return s


def asta_sword():
    s = S()
    s.vb(0, 0, 0, 0, -1, 3, 'D')
    s.vb(0, 0, -1, 1, -2, -2, 'D')
    s.vb(0, 1, -3, 1, -13, -3, 'D')                       # black, chipped cleaver
    s.vb(0, 1, -3, -3, -13, -3, 'P')
    for z, y in ((-5, -3), (-8, 1), (-11, -3), (-13, 1)):
        s.vb(0, 1, y, y, z, z, 'D', sub=True)            # chips
    s.box((0.5, -3, -13.5), (2, 3, 3), 'P', (45, 0, 0), sub=True)
    s.vb(0, 1, 0, 0, -12, -4, 'A')                        # grimoire-rune line
    return s


def beyblade():
    s = S()
    c = (0, 0, -4)
    s.cone((0, -1.5, -4), (3, 3, 3), 'D', TIP_NY)         # performance tip
    s.cyl((0, 0, -4), (8, 1, 8), 'P')                     # fusion wheel
    for k in range(3):                                    # pegasus-style wing tabs
        a = math.radians(k * 120)
        s.box((4 * math.cos(a), 0, -4 + 4 * math.sin(a)), (3, 1, 2), 'A', (0, -k * 120 + 30, 0))
    s.cyl((0, 1, -4), (5, 1, 5), 'A')                     # clear wheel
    s.cyl((0, 2, -4), (3, 1, 3), 'C')                     # bit chip
    return s


def frieren_staff():
    s = S()
    s.cyl((0, 0, -2), (1, 13, 1), 'P', ALONG_Z)           # white shaft 4..-8
    s.vb(0, 0, 0, 0, 4, 5, 'A')
    s.cyl((0, 0, -11), (6, 1, 6), 'A', ALONG_X)           # golden ring head
    s.cyl((0, 0, -11), (4, 1, 4), 'A', ALONG_X, sub=True)
    for sy in (-1, 1):                                    # wing-like flourishes
        s.seg((0, sy * 1, -9), (0, sy * 4, -8), 1, 1, 'A')
    s.ell((0, 0, -10), (1.6, 2, 2), 'C')                 # red jewel
    return s


def moon_rod():
    s = S()
    s.cyl((0, 0, -2), (1, 13, 1), 'A', ALONG_Z)           # pink shaft
    s.ell((0, 0, 4.5), (2, 2, 2), 'A')
    s.cyl((0, 0, -10.5), (7, 1, 7), 'P', ALONG_X)         # crescent moon
    s.cyl((0, 1.5, -11.5), (6, 1, 6), 'P', ALONG_X, sub=True)
    s.ell((-0.0, 0.5, -10), (1, 2, 2), 'C')               # heart gem
    s.ell((0, 0.5, -9.2), (1, 2, 2), 'C')
    for sy in (-1, 1):
        s.seg((0, sy * 1, -8.5), (0, sy * 3.5, -7), 1, 2, 'C')  # little wings
    return s


def moon_tiara():
    s = S()
    s.cyl((0, 0, -4), (8, 1, 8), 'A')
    s.cyl((0, 0, -4), (6, 1, 6), 'A', sub=True)
    s.ell((0, 0, -7.5), (2, 1, 2), 'C')
    s.box((0, 0, -7.5), (1, 1, 3), 'C', (0, 45, 0))
    return s


def kiki_broom():
    s = S()
    s.cyl((0, 0, -2), (1, 13, 1), 'D', ALONG_Z)
    s.cone((0, 0, 7), (4, 6, 4), 'A', TIP_NZ)            # bristles behind the hand
    s.cyl((0, 0, 4.5), (2, 1, 2), 'D', ALONG_Z)
    s.box((0, 1, 4.5), (3, 1, 1), 'C')                    # red ribbon bow
    return s


def small_light(big=False):
    s = S()
    s.vb(0, 0, -2, 0, 0, 1, 'D')                          # grip
    r = 5 if big else 3
    s.cyl((0, 1, -2), (2, 5, 2), 'P', ALONG_Z)
    s.cone((0, 1, -5.5), (r, 3, r), 'P', TIP_PZ)          # flared head
    s.cyl((0, 1, -7), (r, 1, r), 'C', ALONG_Z)            # lens
    s.cyl((0, 1, -1), (2.4, 1, 2.4), 'A', ALONG_Z)
    return s


def thunder_spear():
    s = S()
    s.cone((0, 0, -5), (2, 3, 2), 'C', TIP_NZ)            # piercing head
    s.cyl((0, 0, -1), (2, 6, 2), 'P', ALONG_Z)
    for z in (-3, 0):
        s.cyl((0, 0, z), (2.6, 1, 2.6), 'A', ALONG_Z)
    s.cyl((0, 0, 3), (1, 3, 1), 'D', ALONG_Z)
    for sx, sy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        s.box((sx * 1.2, sy * 1.2, 4), (1 if sx == 0 else 2, 1 if sy == 0 else 2, 2), 'D')
    return s


# ------------------------------------------------------------------------- structures / objects

def at_field():
    s = S()
    octagon_fill(s, (0, 10, 0), 9.5, 1, 'A')
    for r, t in ((7, 'C'), (4.5, 'C'), (2, 'P')):
        octagon_ring(s, (0, 10, -1), r, 1, t)
    s.vb(-1, 1, 0, 0, -1, 1, 'D')
    return s


def anywhere_door():
    s = S()
    s.vb(-3, 3, 0, 0, -1, 1, 'D')                         # little base
    s.vb(-2, 2, 1, 9, 0, 0, 'A')                          # pink door
    s.vb(-3, 3, 10, 10, 0, 0, 'A'); s.vb(-3, -3, 1, 10, 0, 0, 'A'); s.vb(3, 3, 1, 10, 0, 0, 'A')  # frame
    s.vb(1, 1, 5, 5, -1, -1, 'C')                         # knob
    s.vb(-1, 1, 7, 8, 0, 0, 'P')                          # panels
    s.vb(-1, 1, 2, 4, 0, 0, 'P')
    return s


def malevolent_shrine():
    s = S()
    s.vb(-7, 7, 0, 1, -6, 6, 'D')                         # skull-strewn base
    for x in (-5, -2, 2, 5):
        s.ell((x, 2, -6), (2, 2, 2), 'C')                  # skulls
    for x in (-5, 5):
        for z in (-4, 4):
            s.vb(x, x, 2, 9, z, z, 'P')                   # ribbed pillars
            for y in (4, 6, 8):
                s.vb(x - 1, x + 1, y, y, z, z, 'D')
    s.vb(-4, 4, 3, 8, -3, 3, 'D')                         # inner sanctum
    s.ell((0, 6, -3.5), (7, 5, 2), 'P')                   # the maw
    s.vb(-3, 3, 6, 6, -5, -4, 'D')                        # mouth opening
    for x in (-3, -1, 1, 3):
        s.vb(x, x, 7, 7, -5, -5, 'C'); s.vb(x, x, 5, 5, -5, -5, 'C')  # teeth
    s.box((0, 10.5, 0), (18, 1, 16), 'P')                 # tiered roofs
    s.box((0, 11.5, 0), (14, 1, 12), 'D')
    s.box((0, 13, 0), (12, 2, 10), 'P')
    s.box((0, 14.5, 0), (8, 1, 6), 'D')
    for sx in (-1, 1):                                    # horns
        s.seg((sx * 3, 15, 0), (sx * 6, 19, -1), 1, 1, 'A')
        s.cone((sx * 6.6, 20, -1), (1, 2, 1), 'C', (0, 0, -sx * 30))
    return s


def road_roller():
    s = S()
    s.part(1, 0, (0, 3, -6), 'Other')                    # front drum (spins)
    s.cyl((0, 3, -6), (6, 9, 6), 'D', ALONG_X, part=1)
    s.cyl((0, 3, -6), (6.6, 1, 6.6), 'A', ALONG_X, part=1)
    s.vb(-4, 4, 5, 6, -9, -3, 'A')                        # drum yoke
    s.vb(-3, 3, 2, 7, -3, 6, 'A')                         # body
    s.vb(-2, 2, 8, 11, 1, 5, 'P')                         # cab
    s.vb(-2, 2, 9, 10, 0, 0, 'C')                         # window
    s.vb(-3, 3, 12, 12, 0, 6, 'D')                        # roof
    for x in (-4, 4):
        s.cyl((x, 2, 4), (4, 1, 4), 'D', ALONG_X)          # rear wheels
    s.vb(-3, -3, 8, 12, 6, 6, 'D'); s.vb(3, 3, 8, 12, 6, 6, 'D')
    return s


# --------------------------------------------------------------------------------------- worn

def odm_gear():
    s = S()
    s.vb(-1, 1, -6, -5, 0, 1, 'D')                        # central device at the lower back
    s.cyl((0, -5.5, 2), (1, 2, 1), 'C', ALONG_Z)          # gas nozzle
    for sx in (-1, 1):
        s.cyl((sx * 3, -5, 1), (2, 5, 2), 'P', ALONG_Z)    # gas canisters on the hips
        s.vb(sx * 3, sx * 3, -7, -6, -3, 3, 'D')          # blade scabbards
        s.vb(sx * 4, sx * 4, -7, -6, -3, 3, 'A')
        s.cone((sx * 2.5, -5, -2), (1, 2, 1), 'C', TIP_NZ) # grapple anchors
        s.vb(sx * 2, sx * 2, -4, 0, 0, 0, 'D')            # harness straps
    return s


def hollow_purple():
    s = S()
    s.vb(0, 0, -1, 0, -1, 1, 'D')                         # two raised fingers
    s.vb(1, 1, -1, 1, 0, 1, 'D')
    s.cyl((0, 0, -7), (10, 1, 10), 'P', (90, 0, 25))      # imaginary-mass ring
    s.ell((0, 0, -7), (7, 7, 7), 'A')                    # the purple mass
    s.ell((0, 0, -9.5), (4, 4, 3), 'C')                  # erasing core, front
    s.ell((-4.5, 2, -3), (3, 3, 3), 'P')                 # red + blue converging
    s.ell((4.5, -2, -3), (3, 3, 3), 'D')
    return s
