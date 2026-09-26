"""Autobots (and friends): hand-authored two-mode models. Each function returns (names, recipe, TF, comment)."""
from tflib import TF

RED = (0.86, 0.1, 0.1)


# ------------------------------------------------------------------------------ shared robot kit

def arm(t, pid, side, sh, up_t="D", fore_t="A", pad_t="A", fist_t="D", pad=(3.4, 3.4, 4.0), upper=(2.4, 4.0, 2.4),
        fore=(3.0, 5.0, 3.2), fist=(2.4, 2.2, 2.6), parent=0):
    """Robot-only arm hanging from shoulder point `sh` (x sign = side). Returns (builder, hand centre)."""
    x, y, z = sh
    b = t.robot_only(pid, parent, sh, "ArmLeft" if side < 0 else "ArmRight")
    b.bx((x, y - pad[1] * 0.3, z), pad, pad_t)
    yu = y - pad[1] * 0.6 - upper[1] * 0.5 + 0.4
    b.bx((x, yu, z), upper, up_t)
    yf = yu - upper[1] * 0.5 - fore[1] * 0.5 + 0.3
    b.bx((x, yf, z - 0.2), fore, fore_t)
    yh = yf - fore[1] * 0.5 - fist[1] * 0.5 + 0.2
    b.bx((x, yh, z - 0.2), fist, fist_t)
    return b, (x, yh, z - 0.2)


def pelvis(t, pid, y, w=6.0, d=3.6, t_=("D", "P")):
    b = t.robot_only(pid, 0, (0, y, 0), "Other")
    b.bx((0, y, 0.3), (w, 2.4, d), t_[0])
    b.bx((0, y - 0.6, -1.6), (2.0, 2.2, 0.6), t_[1])
    return b


# ------------------------------------------------------------------------------ Optimus Prime (G1)

def optimus_prime():
    t = TF(k=1.35)
    # ---- part 0: the cab -> chest (windshield = chest windows, grille = abdomen, bumper = belt)
    c = t.panel(0, anchor=(0, 4, -9.5))
    c.bx((0, 8.5, -9.5), (9, 9, 7), "A")                       # red cab
    c.bx((0, 13.1, -9.8), (8.4, 0.5, 6), "A")                  # roof lip
    c.bx((-2.15, 11.2, -13.05), (3.7, 2.8, 0.6), "P")          # windshield L (blue glass)
    c.bx((2.15, 11.2, -13.05), (3.7, 2.8, 0.6), "P")           # windshield R
    c.bx((0, 6.4, -13.05), (5.2, 3.6, 0.6), "D")               # silver grille
    for gx in (-1.8, -0.6, 0.6, 1.8):
        c.bx((gx, 6.4, -13.3), (0.5, 3.2, 0.4), "P")           # grille bars
    c.bx((-3.5, 5.6, -13.1), (1.4, 1.0, 0.5), "C")             # headlights
    c.bx((3.5, 5.6, -13.1), (1.4, 1.0, 0.5), "C")
    c.bx((0, 4.2, -13.4), (9.4, 1.2, 1.0), "D")                # bumper -> belt
    c.bx((-4.6, 11.0, -10.2), (0.4, 2.4, 3.2), "P")            # side windows
    c.bx((4.6, 11.0, -10.2), (0.4, 2.4, 3.2), "P")
    for lx in (-3.2, 3.2):
        c.cz((lx, 13.6, -11.8), 0.6, 2.4, "D")                 # roof air horns
    t.place(0, at=(0, 15.2, 0.3))
    # ---- legs: the two halves of the chassis behind the cab swing down (Rx 90)
    legs = {}
    for side, pid in ((-1, 1), (1, 2)):
        x = 1.9 * side
        b = t.panel(pid, anchor=(x, 5.5, -6))
        b.bx((x, 3.9, 1.0), (3.6, 3.4, 14.0), "P")             # blue leg / chassis rail
        b.bx((x, 5.7, 4.0), (3.0, 0.4, 5.0), "D")              # fifth-wheel plate -> calf back
        b.bx((x, 2.2, 1.0), (2.8, 0.6, 2.4), "D")              # knee pad (under chassis)
        b.bx((x, 2.2, -4.2), (3.0, 0.6, 3.0), "D")             # thigh plate
        b.bx((x, 1.3, 7.2), (3.4, 2.4, 1.6), "P")              # foot (rear mud-flap block)
        b.bx((x + 0.9 * side, 3.4, 7.9), (1.2, 0.8, 0.4), "A")  # tail light -> heel light
        b.cz((x + 1.6 * side, 4.6, -3.2), 1.3, 3.2, "D")       # fuel tank -> thigh tank
        t.alt_part(pid, 0, (x, 5.5, -6), "Other")
        t.place(pid, at=(2.7 * side, 14, 1.4), rot=(90, 0, 0), parent=0, pivot=(2.7 * side, 14, 0.2),
                role="LegBackLeft" if side < 0 else "LegBackRight")
        legs[side] = pid
    # ---- rear wheels ride the legs (calves)
    wid = 5
    for side in (-1, 1):
        for z in (1.8, 5.2):
            w = t.panel(wid, anchor=(4.35 * side, 2, z))
            w.wheel((4.35 * side, 2, z), 4.0, 1.6)
            t.alt_part(wid, 0, (4.35 * side, 2, z), "Wheel")
            t.follow(wid, legs[side], role="Wheel")
            wid += 1
    # ---- front wheels -> hips
    for side in (-1, 1):
        w = t.panel(wid, anchor=(4.0 * side, 2, -10.5))
        w.wheel((4.0 * side, 2, -10.5), 4.0, 1.4)
        t.alt_part(wid, 0, (4.0 * side, 2, -10.5), "Wheel")
        t.place(wid, at=(2.3 * side, 18.0, 4.4), rot=(0, 90, 0), parent=0, role="Wheel")  # on the back
        wid += 1
    # ---- smokestacks behind the cab -> behind the shoulders
    for side, pid in ((-1, 11), (1, 12)):
        s = t.panel(pid, anchor=(4.0 * side, 10, -5.4))
        s.cy((4.0 * side, 10, -5.4), (1.1, 10, 1.1), "D")
        s.cy((4.0 * side, 15.2, -5.4), (1.3, 0.5, 1.3), "P")
        t.place(pid, at=(4.7 * side, 24.5, 3.4), parent=0)
    # ---- robot-only: pelvis, arms, head, weapons (unfold from inside the cab)
    pelvis(t, 20, 14.6)
    _, hl = arm(t, 21, -1, (-6.4, 23.2, 0))
    _, hr = arm(t, 22, 1, (6.4, 23.2, 0))
    h = t.robot_only(23, 0, (0, 24.6, 0.2), "Head")
    h.bx((0, 25.0, 0.4), (1.8, 1.2, 1.8), "D")                 # neck
    h.bx((0, 27.2, 0.2), (4.2, 4.2, 3.8), "P")                 # blue helmet
    h.bx((0, 26.2, -1.9), (2.6, 2.0, 0.6), "D")                # silver faceplate
    h.bx((0, 27.6, -2.0), (2.8, 0.7, 0.4), "C")                # eyes
    h.bx((0, 29.6, -1.2), (0.8, 1.2, 1.6), "D")                # crest
    for sx in (-1, 1):
        h.bx((2.5 * sx, 28.9, 0.3), (0.8, 4.0, 1.0), "P")      # antenna "ears"
    g = t.robot_only(24, 22, hr, "Weapon")                     # ion blaster (right hand)
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 2.6), (1.3, 1.6, 4.6), "D")
    g.bx((hr[0], hr[1] - 1.0, hr[2] - 1.0), (0.9, 1.4, 0.9), "D")
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 5.1), (0.8, 0.8, 0.6), "C")
    a = t.robot_only(25, 21, hl, "Weapon")                     # energon axe (left hand)
    a.bx((hl[0], hl[1] - 2.0, hl[2]), (0.6, 3.8, 0.6), "D")
    a.bx((hl[0], hl[1] - 3.2, hl[2] - 1.4), (0.5, 2.8, 2.4), "C")
    t.finish()
    names = ["optimus prime", "optimus", "optimus prime truck", "autobot truck", "prime transformer",
             "g1 optimus", "optimus prime g1", "autobot leader"]
    recipe = dict(name="Optimus Prime", form="Vehicle", mech=["Shoots", "Blade", "Charge", "Transforms"],
                  scale=6.0, mass=9000, accent=RED, icon="optimus_prime",
                  payload=dict(name="ion blaster bolt", form="Ball", mech=["Fast", "Pierce"], elem="Electric",
                               scale=0.5, mass=1.0, accent=(0.55, 0.8, 1.0), seed=1984),
                  feat=["Wheels", "Smokestack", "Cab", "Headlight"], temp="Protective", inten=0.85)
    note = ("Optimus Prime (G1): red cab-over semi. Cab -> chest (windshield = chest windows, grille = abs),\n"
            "chassis halves -> legs w/ wheels on calves, smokestacks -> behind shoulders. Ion blaster + energon axe.")
    return names, recipe, t, note
