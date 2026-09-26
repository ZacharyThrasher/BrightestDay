"""Transformers items (single shape) + Soundwave's cassettes (two-mode: cassette <-> beast)."""
import math

from tflib import TF, Mode, Builder

PINK = (1.0, 0.36, 0.78)


def _mode():
    m = Mode()
    return m, Builder(m.prims, 0)


def _k(m, k=3.0):
    """Items are authored at 1 m = 4 units; emit at k x for voxel detail (icons rescale to recipe.scale)."""
    for p in m.prims:
        p.c = p.c * k
        p.s = tuple(a * k for a in p.s)
    return m


def energon_cube():
    m, b = _mode()
    b.bx((0, 1.6, 0), (3.2, 3.2, 3.2), "A")                                                  # pink cube
    b.bx((0, 1.6, 0), (2.2, 2.2, 3.4), "C")                                                  # glowing core (x-ray faces)
    b.bx((0, 1.6, 0), (3.4, 2.2, 2.2), "C")
    b.bx((0, 1.6, 0), (2.2, 3.4, 2.2), "C")
    for sx in (-1, 1):
        for sz in (-1, 1):
            b.bx((sx * 1.6, 1.6, sz * 1.6), (0.5, 3.4, 0.5), "D")                            # frame edges
            b.bx((sx * 1.6, 1.6 + sz * 1.6, 0), (0.5, 0.5, 3.4), "D")
            b.bx((0, 1.6 + sx * 1.6, sz * 1.6), (3.4, 0.5, 0.5), "D")
    names = ["energon cube", "energon", "transformers energon", "energon transformers", "energon block",
             "energon cubes"]
    recipe = dict(name="Energon Cube", form="Ball", mech=["Explodes", "Bounce"], elem="Electric", scale=0.8,
                  mass=20.0, accent=PINK, icon="energon_cube")
    return names, recipe, _k(m), "Energon cube: the Transformers' fuel. Volatile: Explodes (Electric)."


def matrix_of_leadership():
    m, b = _mode()
    b.el((0, 2.6, 0), (2.4, 2.8, 2.0), "C")                                                  # the light of Primus
    b.bx((0, 2.6, 0), (3.0, 3.6, 1.2), "D")                                                  # casing
    b.el((0, 2.6, -0.2), (2.0, 2.4, 1.8), "C")
    for sx in (-1, 1):
        b.bx((sx * 2.0, 2.6, 0), (0.6, 4.2, 0.8), "A")                                       # handles
        b.bx((sx * 1.6, 4.6, 0), (1.2, 0.6, 0.8), "A")
        b.bx((sx * 1.6, 0.6, 0), (1.2, 0.6, 0.8), "A")
    b.cone((0, 5.4, 0), 1.2, 1.4, "A", dir="+y")                                             # top point
    b.cone((0, -0.2, 0), 1.2, 1.4, "A", dir="-y")
    names = ["matrix of leadership", "autobot matrix of leadership", "the matrix of leadership",
             "leadership matrix", "creation matrix", "matrix transformers"]
    recipe = dict(name="Matrix of Leadership", form="Ball", mech=["Guard", "Barrier"], elem="Light", scale=0.7,
                  mass=10.0, accent=(0.55, 0.85, 1.0), icon="matrix_of_leadership", temp="Protective", inten=0.9)
    return names, recipe, _k(m), "Matrix of Leadership: 'light our darkest hour'. A glowing core that guards (Ball + Guard)."


def star_saber():
    m, b = _mode()
    b.bx((0, 0, 0.6), (0.9, 0.9, 2.4), "D")                                                  # grip
    b.bx((0, 0, 2.0), (1.3, 1.3, 0.6), "A")                                                  # pommel
    b.bx((0, 0, -0.9), (4.0, 0.9, 0.7), "A")                                                 # guard bar
    for a in (45, -45):
        b.bx((0, 0, -0.9), (3.0, 0.8, 0.6), "A", rot=(0, a, 0))                              # star guard
    b.bx((0, 0, -1.0), (1.4, 1.2, 1.2), "C")                                                 # star gem
    b.bx((0, 0, -6.0), (1.4, 0.5, 9.0), "C")                                                 # blade
    b.bx((0, 0, -6.0), (0.5, 0.7, 9.0), "A")                                                 # fuller
    b.cone((0, 0, -11.0), 1.4, 1.4, "C", dir="-z", d2=0.5)
    names = ["star saber", "victory saber", "star saber sword", "star saber victory", "star sabre"]
    recipe = dict(name="Star Saber", form="Sword", mech=["Blade", "Beam"], elem="Light", scale=2.4, mass=12.0,
                  accent=(0.3, 0.5, 1.0), icon="star_saber")
    return names, recipe, _k(m), "Star Saber (Transformers: Victory): the legendary glowing sword. Blade + Beam."


def energon_axe():
    m, b = _mode()
    b.bx((0, 0, -1.6), (0.8, 0.8, 6.4), "D")                                                 # haft
    b.bx((0, 0, 1.6), (1.1, 1.1, 0.6), "A")                                                  # butt
    b.bx((0, 1.8, -4.4), (0.5, 3.0, 2.6), "C")                                               # energon blade
    b.bx((0, 3.4, -4.6), (0.5, 0.8, 3.4), "C", rot=(-15, 0, 0))
    b.bx((0, 0.4, -4.4), (0.7, 1.0, 2.0), "A")                                               # socket
    b.cone((0, 0, -5.4), 0.9, 1.2, "A", dir="-z")                                            # spike
    names = ["energon axe", "optimus axe", "optimus prime axe", "energon battle axe"]
    recipe = dict(name="Energon Axe", form="Axe", mech=["Blade"], elem="Electric", scale=1.6, mass=15.0,
                  accent=(1.0, 0.45, 0.3), icon="energon_axe")
    return names, recipe, _k(m), "Energon axe: Optimus Prime's hand-formed glowing axe (G1). Blade, Electric."


def fusion_cannon():
    m, b = _mode()
    b.cz((0, 0, -3.5), 2.6, 9.0, "D")                                                        # barrel
    b.cz((0, 0, -8.4), 3.2, 1.2, "A")                                                        # muzzle ring
    b.cz((0, 0, -9.1), 1.8, 0.4, "C")                                                        # glowing bore
    b.cz((0, 0, 0.8), 3.2, 2.4, "A")                                                         # breech
    b.bx((0, -1.8, 0.4), (1.6, 1.6, 3.0), "D")                                               # arm clamp / grip
    for z in (-2.0, -4.5):
        b.cz((0, 0, z), 2.9, 0.4, "A")                                                       # cooling rings
    names = ["fusion cannon", "megatron's fusion cannon", "megatron fusion cannon", "megatron cannon",
             "fusion cannon megatron"]
    recipe = dict(name="Fusion Cannon", form="Cannon", mech=["Explodes", "Heavy"], elem="Atomic", scale=2.6,
                  mass=60.0, accent=(0.8, 0.82, 0.88), icon="fusion_cannon",
                  payload=dict(name="fusion blast", form="Ball", mech=["Explodes", "Fast"], elem="Atomic",
                               scale=0.8, mass=5.0, accent=(0.6, 0.3, 0.9), seed=1986))
    return names, recipe, _k(m), "Fusion cannon: Megatron's arm cannon. Explodes, Atomic."


def allspark():
    m, b = _mode()
    b.bx((0, 2.2, 0), (4.4, 4.4, 4.4), "A")                                                  # the cube
    for i, y in enumerate((0.9, 2.2, 3.5)):
        b.bx((0, y, -2.25), (3.6 - i * 0.6, 0.3, 0.3), "C")                                  # glowing glyphs
        b.bx((-2.25, y, 0), (0.3, 0.3, 3.0 + i * 0.3), "C")
        b.bx((2.25, y, 0), (0.3, 0.3, 3.4 - i * 0.4), "C")
    b.bx((0, 2.2, 2.25), (0.3, 3.6, 0.3), "C")
    b.bx((0, 4.45, 0), (2.6, 0.3, 0.3), "C")
    names = ["allspark", "all spark", "the allspark", "allspark cube", "the cube transformers"]
    recipe = dict(name="AllSpark", form="Ball", mech=["Grow"], elem="Electric", scale=0.9, mass=40.0,
                  accent=(0.7, 0.75, 0.85), icon="allspark")
    return names, recipe, _k(m), "AllSpark (live-action films): the cube that brings machines to life -> Grow, Electric."


# ------------------------------------------------------------------------------ cassettes

def _cassette(t, pid_body, pid_l, pid_r, label_t="A"):
    """Microcassette lying flat (x 8, y 1.2, z 5): left/right halves + centre label strip."""
    body = t.panel(pid_body, anchor=(0, 0.6, 0))
    body.bx((0, 0.6, 0), (2.8, 1.2, 5.0), "D")                                               # centre
    body.bx((0, 1.25, -0.4), (2.4, 0.2, 2.4), label_t)                                       # label window
    for sx in (-0.7, 0.7):
        body.cy((sx, 1.3, -0.4), (0.8, 0.3, 0.8), "C")                                       # reels
    halves = {}
    for side, pid in ((-1, pid_l), (1, pid_r)):
        h = t.panel(pid, anchor=(side * 1.4, 0.6, 0))
        h.bx((side * 2.7, 0.6, 0), (2.6, 1.2, 5.0), "A")
        h.bx((side * 2.7, 1.25, 1.6), (1.6, 0.2, 1.0), "D")                                  # screw plate
        halves[side] = pid
    return halves


def laserbeak():
    t = TF(k=3.0)
    halves = _cassette(t, 0, 1, 2)
    t.place(0, at=(0, 6.0, 0.4), rot=(-10, 0, 0))                                            # body
    for side, pid in ((-1, 1), (1, 2)):
        # cassette halves swing out as the wings (with a little dihedral)
        t.alt_part(pid, 0, (side * 1.4, 0.6, 0), "Other")
        t.place(pid, at=(side * 1.4, 6.2, 0.4), rot=(0, 0, side * 12), parent=0,
                pivot=(side * 1.4, 6.2, 0.4), role="WingLeft" if side < 0 else "WingRight")
        wt = t.robot_only(10 + (0 if side < 0 else 1), pid, (side * 4.0, 6.6, 0.4), "Other")
        wt.bx((side * 5.6, 6.8, 1.2), (3.0, 0.5, 3.2), "A")                                  # wing tip feathers
    hd = t.robot_only(3, 0, (0, 6.4, -2.6), "Head")
    hd.el((0, 7.0, -3.4), (1.8, 1.8, 2.0), "A")
    hd.cone((0, 6.8, -4.8), 1.0, 1.6, "D", dir="-z")                                         # beak
    hd.bx((0, 7.3, -4.1), (1.6, 0.4, 0.4), "C")                                              # eyes
    tl = t.robot_only(4, 0, (0, 6.0, 2.8), "Tail")
    tl.bx((0, 5.8, 3.8), (2.4, 0.4, 2.2), "D")
    for side, pid in ((-1, 5), (1, 6)):                                                      # talons
        lg = t.robot_only(pid, 0, (side * 0.8, 5.4, 0.6), "Other")
        lg.bx((side * 0.8, 4.4, 0.6), (0.5, 2.0, 0.5), "D")
        lg.bx((side * 0.8, 3.4, 0.2), (1.0, 0.4, 1.4), "C")
    t.finish()
    names = ["laserbeak", "laser beak", "laserbeak cassette", "soundwave's laserbeak", "laserbeak condor"]
    recipe = dict(name="Laserbeak", form="Bird", mech=["Flies", "Shoots", "Homing", "Transforms"], scale=1.4,
                  mass=30.0, accent=(0.86, 0.18, 0.18), icon="laserbeak", temp="Sneaky", inten=0.8,
                  payload=dict(name="laserbeak shot", form="Ball", mech=["Fast"], elem="Light", scale=0.3,
                               mass=1.0, seed=33))
    note = "Laserbeak: Soundwave's red condor cassette. Cassette halves -> wings, label strip -> body. Sneaky spy."
    return names, recipe, t, note


def ravage():
    t = TF(k=3.0)
    _cassette(t, 0, 1, 2, label_t="P")
    t.place(0, at=(0, 4.2, 0.0), rot=(0, 0, 0))                                              # back/spine
    for side, pid in ((-1, 1), (1, 2)):
        t.alt_part(pid, 0, (side * 1.4, 0.6, 0), "Other")
        t.place(pid, at=(side * 1.2, 4.2, 0.0), rot=(0, 0, side * -70), parent=0)            # flanks fold down
    legs = [(10, -1, -1.6, "LegFrontLeft"), (11, 1, -1.6, "LegFrontRight"), (12, -1, 1.8, "LegBackLeft"),
            (13, 1, 1.8, "LegBackRight")]
    for pid, sx, z, role in legs:
        lg = t.robot_only(pid, 0, (sx * 1.3, 3.6, z), role)
        lg.bx((sx * 1.3, 2.0, z), (0.8, 3.4, 0.9), "D")
        lg.bx((sx * 1.3, 0.3, z - 0.3), (1.0, 0.6, 1.4), "D")
    hd = t.robot_only(14, 0, (0, 4.4, -2.6), "Head")
    hd.bx((0, 4.6, -3.6), (2.2, 1.8, 2.0), "D")
    hd.bx((0, 4.2, -4.8), (1.4, 1.0, 1.0), "D")                                              # muzzle
    hd.bx((0, 5.0, -4.65), (1.8, 0.4, 0.4), "C")                                             # red eyes
    for sx in (-1, 1):
        hd.cone((sx * 0.7, 5.8, -3.4), 0.6, 0.8, "D", dir="+y")                              # ears
    tl = t.robot_only(15, 0, (0, 4.4, 2.6), "Tail")
    tl.bx((0, 4.6, 4.2), (0.5, 0.5, 3.4), "D", rot=(20, 0, 0))
    for sx in (-1, 1):
        ms = t.robot_only(16 + (0 if sx < 0 else 1), 0, (sx * 1.9, 4.8, 0.4), "Weapon")
        ms.cz((sx * 1.9, 5.0, 0.0), 0.7, 3.0, "A")                                           # flank missiles
        ms.cz((sx * 1.9, 5.0, -1.6), 0.5, 0.3, "C")
    t.finish()
    names = ["ravage transformer", "ravage cassette", "soundwave's ravage", "ravage jaguar",
             "decepticon ravage"]
    recipe = dict(name="Ravage", form="Quadruped", mech=["Pierce", "Fast", "Shoots", "Transforms"], scale=1.6,
                  mass=120.0, accent=(0.3, 0.3, 0.36), icon="ravage", temp="Sneaky", inten=0.9)
    note = "Ravage: Soundwave's black jaguar cassette. Cassette -> back + flanks, legs/head unfold; flank missiles."
    return names, recipe, t, note
