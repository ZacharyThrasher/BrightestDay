"""Extra small fan favourites: Togepi, Clefairy, Magnemite, Koffing, Porygon, Marill.
Authored ~2x in-game resolution (creatures under 0.9 m are shown at 0.9 m = ~7 voxels: keep them BOLD)."""
from plib import S, icon, tail_chain, ALONG_X, ALONG_Z, TIP_NZ, TIP_PZ, TIP_NY, TIP_PX, TIP_NX
from plib import HEAD, TAIL, LFL, LFR, LBL, LBR, ARM_L, ARM_R, WING_L, WING_R, OTHER
from plib import RED, BLUE, YELLOW, PINK, CREAM, GREY, PURPLE, CYAN, WHITE, BROWN


def togepi():
    s = S()
    s.ell((0, 5, 0), (9, 10, 9), 'C')                                   # egg-shell body
    for x, y, t in ((-2.5, 3.5, 'A'), (2.2, 6, 'P'), (0, 2.5, 'P'), (-3.2, 7, 'P'), (3.4, 3.2, 'A'), (0.5, 7.5, 'A')):
        s.ell((x, y, -3.9), (2.2, 2.0, 1.2), t)                          # red/blue triangle patches
    s.part(1, 0, (0, 9, 0), HEAD)
    s.ell((0, 11.5, 0), (7, 5.5, 6.5), 'A', part=1)                      # cream head
    for x, h in ((-2.4, 3.0), (0, 3.8), (2.4, 3.0)):                     # crown spikes
        s.spike((x, 13.0, 0.2), (x * 1.25, 13.0 + h, 0.4), 2.0, 'A', part=1)
    s.eyes(1.4, 11.6, -3.1, 1, w=1.2, h=1.8)
    s.box((0, 10.2, -3.2), (1.4, 0.6, 0.6), 'D', part=1)                 # smile
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                # stubby arms
        s.part(pid, 0, (sx * 3.8, 8.5, 0), role)
        s.seg((sx * 3.8, 8.5, 0), (sx * 5.6, 9.5, -0.6), 2.0, 2.0, 'A', part=pid, kind='Ellipsoid')
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):                    # tiny feet
        s.part(pid, 0, (sx * 2, 1.5, 0), role)
        s.ell((sx * 2, 0.8, -0.8), (2.4, 1.6, 3.0), 'A', part=pid)
    return s


def clefairy():
    s = S()
    s.ell((0, 5.5, 0), (9, 9, 8))                                       # round pink body
    s.part(1, 0, (0, 9, 0), HEAD)
    s.ell((0, 11, -0.3), (8.4, 6.5, 7), part=1)                          # head
    for sx in (-1, 1):
        s.spike((sx * 2.8, 13.2, 0), (sx * 4.2, 18, 0.6), 3.4, 'P', part=1)          # pointy ears
        s.spike((sx * 3.3, 15.8, 0.1), (sx * 4.2, 18, 0.6), 1.5, 'D', part=1)        # dark ear tips
    s.seg((0, 13.8, -1.0), (0, 15.8, -0.4), 1.2, 1.2, 'P', part=1)       # forehead curl
    s.ell((0, 16.0, 0.3), (2.0, 2.0, 1.2), 'P', part=1)
    s.eyes(1.7, 11.4, -3.6, 1, w=1.4, h=1.4)
    for sx in (-1, 1):
        s.ell((sx * 2.9, 10.3, -3.1), (1.2, 0.8, 0.6), 'A', part=1)      # cheek blush
    for pid, sx, role in ((10, -1, WING_L), (11, 1, WING_R)):            # tiny back wings
        s.part(pid, 0, (sx * 1.5, 8, 3.5), role)
        s.ell((sx * 3.0, 9, 4.3), (3.6, 3.0, 0.8), 'A', (0, 0, sx * -25), part=pid)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        s.part(pid, 0, (sx * 4.2, 7.5, 0), role)
        s.seg((sx * 4.2, 7.5, 0), (sx * 5.8, 5.8, -0.8), 2.0, 2.0, 'P', part=pid, kind='Ellipsoid')
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 2.2, 2.2, 0), role)
        s.ell((sx * 2.2, 1.0, -0.8), (2.8, 2.2, 3.4), 'P', part=pid)
    tail_chain(s, 12, 0, [(0, 4, 3.8), (0, 5.5, 6.0), (0, 7.5, 6.4)], [(1.4, 1.4), (1.4, 1.4)], 'P', kind='Ellipsoid')
    s.ell((0, 8.2, 5.8), (1.6, 1.6, 1.6), 'P', part=13)                  # tail curl
    return s


def magnemite():
    s = S()
    y = 7
    s.ell((0, y, 0), (8, 8, 8), 'P')                                      # steel ball body
    s.cyl((0, y, -3.6), (5.0, 1.0, 5.0), 'C', ALONG_Z)                   # big single eye
    s.cyl((0, y, -4.0), (2.0, 0.8, 2.0), 'D', ALONG_Z)                   # pupil
    s.part(1, 0, (0, y + 3.5, 0), HEAD)                                   # top screw
    s.cyl((0, y + 5.0, 0), (1.4, 3.0, 1.4), 'A', part=1)
    s.cyl((0, y + 6.6, 0), (2.8, 0.8, 2.8), 'A', part=1)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                # horseshoe magnets (U, poles out)
        s.part(pid, 0, (sx * 4, y, 0), role)
        x = sx * 6.5
        s.box((x, y, 0), (1.8, 1.8, 7.0), 'P', part=pid)                  # bend
        s.box((x + sx * 2.0, y, -2.8), (4.0, 1.8, 1.6), 'A', part=pid)   # prong front (red pole)
        s.box((x + sx * 2.0, y, 2.8), (4.0, 1.8, 1.6), 'A', part=pid)    # prong back (blue pole)
        s.box((x + sx * 3.6, y, -2.8), (0.8, 1.8, 1.6), 'C', part=pid)   # pole tips
        s.box((x + sx * 3.6, y, 2.8), (0.8, 1.8, 1.6), 'D', part=pid)
        s.box((sx * 4.3, y, 0), (1.4, 1.4, 1.4), 'D', part=pid)          # mount
    for sx in (-1, 1):
        s.cyl((sx * 2.2, y - 3.6, 0.8), (1.0, 1.4, 1.0), 'A', (0, 0, sx * 30))   # little under-screws
    return s


def koffing():
    s = S()
    y = 8
    s.ell((0, y, 0), (12, 12, 12))                                       # gas ball
    for x, yy, z, d in ((-4.5, y + 3, -2, 3.0), (4.0, y + 4.2, 1, 3.4), (5.2, y - 2, -1.5, 2.6), (-3.5, y - 4, 2.5, 3.0),
                        (0.5, y + 5.6, -1.5, 2.4), (-5.8, y, 1.5, 2.6)):
        s.cyl((x, yy, z), (d, 1.6, d), 'A', (0, 0, -x * 12))             # crater vents
        s.cyl((x * 1.05, yy * 1.0 + (0.4 if yy > y else -0.4), z), (d * 0.5, 1.2, d * 0.5), 'D', (0, 0, -x * 12))
    s.part(1, 0, (0, y, -4), HEAD)                                        # the face (tilts as head)
    s.box((-2.2, y + 1.6, -5.6), (2.2, 1.6, 0.8), 'D', (0, 0, -15), part=1)   # scowling brows/eyes
    s.box((2.2, y + 1.6, -5.6), (2.2, 1.6, 0.8), 'D', (0, 0, 15), part=1)
    s.box((-2.1, y + 1.3, -5.9), (1.0, 0.8, 0.6), 'C', part=1)
    s.box((2.1, y + 1.3, -5.9), (1.0, 0.8, 0.6), 'C', part=1)
    s.box((0, y - 2.2, -5.6), (4.4, 1.2, 0.8), 'D', part=1)              # grin
    s.box((-1.0, y - 1.7, -5.9), (0.8, 0.6, 0.5), 'C', part=1)           # fangs
    s.box((1.0, y - 1.7, -5.9), (0.8, 0.6, 0.5), 'C', part=1)
    s.box((0, y - 4.0, -5.3), (2.6, 2.2, 0.8), 'C')                      # skull-and-crossbones mark
    s.box((0, y - 4.0, -5.4), (4.2, 0.6, 0.6), 'C', (0, 0, 35))
    s.box((0, y - 4.0, -5.4), (4.2, 0.6, 0.6), 'C', (0, 0, -35))
    return s


def porygon():
    s = S()
    s.box((0, 5.5, 0.8), (6, 5, 7))                                      # blocky body
    s.box((0, 4.5, 3.8), (4, 3, 2.5), 'A')                               # blue tail block
    s.box((0, 4.2, 5.4), (2.4, 2.0, 1.4), 'P')
    s.part(1, 0, (0, 7, -2.5), HEAD)
    s.box((0, 8.6, -3.8), (5, 4.2, 4.4), 'P', part=1)                    # head
    s.box((0, 8.0, -7.2), (3, 2.2, 3.0), 'A', (0, 45, 0), part=1)        # beak (diamond)
    s.box((0, 8.0, -7.2), (2.0, 1.8, 2.0), 'A', part=1)
    s.eyes(1.6, 9.6, -6.1, 1, w=1.2, h=1.2, d=0.5)
    for pid, sx, zz, role in ((4, -1, -1.5, LFL), (5, 1, -1.5, LFR), (6, -1, 3.0, LBL), (7, 1, 3.0, LBR)):
        s.part(pid, 0, (sx * 2.5, 3.2, zz), role)
        s.box((sx * 2.6, 1.6, zz), (2.4, 3.2, 2.6), 'A' if pid < 6 else 'P', part=pid)   # block feet
    return s


def marill():
    s = S()
    s.ell((0, 5.5, 0), (10, 10, 9))                                      # round blue body
    s.ell((0, 4.8, -2.8), (6.4, 6.4, 3.6), 'A')                          # cream belly
    s.part(1, 0, (0, 9, 0), HEAD)
    s.ell((0, 10.6, -0.4), (7.4, 6.0, 6.6), part=1)                      # head
    for sx in (-1, 1):
        s.cyl((sx * 3.3, 14.2, 0.3), (3.6, 1.4, 3.6), 'P', (90, 0, sx * -20), part=1)   # round mouse ears
        s.cyl((sx * 3.3, 14.2, -0.1), (2.2, 0.8, 2.2), 'D', (90, 0, sx * -20), part=1)
    s.eyes(1.6, 11.2, -3.6, 1, w=1.2, h=1.6)
    s.ell((0, 9.8, -3.8), (1.0, 0.7, 0.5), 'D', part=1)                  # nose
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        s.part(pid, 0, (sx * 4.5, 7, -0.5), role)
        s.seg((sx * 4.5, 7, -0.5), (sx * 5.8, 5.6, -1.6), 1.8, 1.8, 'P', part=pid, kind='Ellipsoid')
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 2.4, 2, 0), role)
        s.ell((sx * 2.4, 0.9, -1.0), (3.0, 1.8, 3.6), 'P', part=pid)
    # zig-zag tail ending in the bouncy water ball
    ids = tail_chain(s, 12, 0, [(0, 3.5, 4.0), (0, 5.2, 6.5), (0, 3.8, 8.6), (0, 6.2, 10.2)], [(0.9, 0.9)] * 3, 'D', kind='Box')
    s.ell((0, 7.6, 11.4), (4.2, 4.2, 4.2), 'A', part=ids[-1])
    return s


def register():
    icon(["togepi", "togapi", "toge pi", "togeppi"],
         "Togepi", "Blob", ["Bounce", "Shrink"], 0.3, 1.5, element="Psychic", accent=CREAM, shape=togepi(),
         temperament="Playful", intensity=0.8)
    icon(["clefairy", "clefary", "clefferie", "cleffairy"],
         "Clefairy", "Biped", ["Bounce", "Attract"], 0.6, 7.5, element="Psychic", accent=PINK, shape=clefairy(),
         temperament="Playful", intensity=0.6)
    icon(["magnemite", "magnamite", "magnemight", "magnet mite"],
         "Magnemite", "Blob", ["Flies", "Attract", "Shoots"], 0.3, 6.0, element="Electric", accent=RED, shape=magnemite())
    icon(["koffing", "kofing", "coffing", "koffin"],
         "Koffing", "Blob", ["Flies", "Breath", "Explodes"], 0.6, 1.0, element="Poison", accent=PURPLE, shape=koffing(),
         temperament="Chaotic", intensity=0.6)
    icon(["porygon", "porigon", "polygon pokemon", "porygun"],
         "Porygon", "Quadruped", ["Portal", "Beam"], 0.8, 36.5, element="Psychic", accent=BLUE, shape=porygon())
    icon(["marill", "maril", "marrill", "marill pokemon"],
         "Marill", "Biped", ["Bounce", "Shoots"], 0.4, 8.5, element="Water", accent=CREAM, shape=marill(),
         temperament="Playful", intensity=0.6)
