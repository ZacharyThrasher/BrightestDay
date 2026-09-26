"""Beast-mode transformers: Grimlock (T-rex), Optimus Primal (gorilla)."""
from tflib import TF
from bots_a import arm, pelvis

GOLD = (0.95, 0.74, 0.18)


# ------------------------------------------------------------------------------ Grimlock

def grimlock():
    t = TF(k=1.4)
    # legs are the same legs in both modes (the dino's legs ARE the robot legs, as on the 1985 toy)
    for side, pid in ((-1, 1), (1, 2)):
        x = side * 2.5
        b = t.panel(pid, anchor=(x, 10.0, 0.6))
        b.bx((x, 8.0, 0.6), (3.2, 4.4, 4.0), "P")                                            # thigh
        b.bx((x, 4.2, 1.0), (2.6, 4.4, 2.8), "P")                                            # shin
        b.bx((x, 6.0, -1.3), (2.4, 1.6, 0.6), "A")                                           # gold knee plate
        b.bx((x, 0.7, -0.6), (3.4, 1.4, 5.0), "D")                                           # foot
        for tx in (-1.0, 0.0, 1.0):
            b.cone((x + tx, 0.7, -3.6), 0.8, 1.2, "C", dir="-z")                             # toe claws
        role = "LegBackLeft" if side < 0 else "LegBackRight"
        t.alt_part(pid, 0, (x, 10.0, 0.6), role)
        t.place(pid, at=(side * 2.7, 10.0, 0.4), parent=0, pivot=(side * 2.7, 10.0, 0.4), role=role)
    # dino body -> barrel chest (Rx 90: the neck end goes up, the belly plate faces front)
    c = t.panel(0, anchor=(0, 12.5, 1.5))
    c.el((0, 12.5, 1.5), (7.2, 6.6, 11.0), "P")
    c.bx((0, 15.6, 1.5), (1.2, 0.8, 8.0), "A")                                               # gold spine ridge
    for z in (-2.0, 1.5, 5.0):
        c.bx((0, 16.2, z), (0.6, 1.0, 1.4), "A")                                             # back plates
    c.bx((0, 9.6, 1.0), (4.6, 1.0, 6.0), "D")                                                # belly plate -> back
    t.place(0, at=(0, 17.2, 0.6), rot=(90, 0, 0))
    # head + neck -> hangs on his back, snout up (Rx 90); jaw keeps its Jaw role (chomps in both modes)
    hd = t.panel(3, anchor=(0, 13.8, -4.0))
    hd.bx((0, 14.2, -5.0), (3.6, 3.6, 3.4), "P")                                             # neck
    hd.bx((0, 15.8, -8.6), (4.6, 4.2, 6.2), "P")                                             # skull
    hd.bx((0, 17.6, -8.0), (4.8, 0.8, 4.0), "A")                                             # gold brow crest
    hd.bx((0, 15.4, -12.0), (3.6, 2.8, 1.6), "P")                                            # snout
    for sx in (-1, 1):
        hd.bx((sx * 2.35, 16.6, -9.8), (0.4, 0.8, 1.2), "C")                                 # eyes
    t.alt_part(3, 0, (0, 13.8, -4.0), "Head")
    t.place(3, at=(0, 19.0, 4.4), rot=(90, 0, 0), parent=0, role="Other")
    jw = t.panel(4, anchor=(0, 13.6, -6.0))
    jw.bx((0, 13.0, -9.2), (4.0, 1.4, 6.2), "D")                                             # lower jaw
    for tz in (-11.4, -10.0, -8.6, -7.2):
        for sx in (-1, 1):
            jw.bx((sx * 1.5, 13.9, tz), (0.5, 0.8, 0.5), "C")                                # teeth
    t.alt_part(4, 3, (0, 13.6, -6.0), "Jaw")
    t.follow(4, 3, role="Jaw")
    # tail (3-segment chain) -> back plate + calf armour
    tails = [(5, (0, 12.8, 7.2), (5.0, 4.6, 5.0)), (6, (0, 12.0, 11.4), (3.8, 3.4, 4.6)),
             (7, (0, 11.2, 15.2), (2.6, 2.4, 4.4))]
    prev = 0
    for pid, cen, sz in tails:
        tb = t.panel(pid, anchor=(cen[0], cen[1], cen[2] - sz[2] / 2))
        tb.bx(cen, sz, "P")
        tb.bx((0, cen[1] + sz[1] / 2 + 0.2, cen[2]), (0.6, 0.6, sz[2] - 1.0), "A")          # ridge
        t.alt_part(pid, prev, (cen[0], cen[1], cen[2] - sz[2] / 2), "Tail")
        prev = pid
    t.place(5, at=(0, 15.0, 3.6), rot=(90, 0, 0), parent=0)                                 # tail root -> lower back
    t.place(6, at=(-2.7, 7.2, 2.2), rot=(90, 0, 0), parent=1)                                # -> left calf
    t.place(7, at=(2.7, 7.0, 1.9), rot=(90, 0, 0), parent=2)                                 # -> right calf
    # tiny T-rex arms -> shoulder pads
    for side, pid in ((-1, 8), (1, 9)):
        a = t.panel(pid, anchor=(side * 2.8, 12.0, -3.6))
        a.bx((side * 2.8, 11.0, -4.0), (1.2, 2.4, 1.2), "P")
        a.bx((side * 2.8, 9.8, -4.6), (1.0, 0.6, 1.4), "C")                                  # little claws
        t.alt_part(pid, 0, (side * 2.8, 12.0, -3.6), "Other")
        t.place(pid, at=(side * 7.2, 23.4, 0.4), rot=(0, 0, side * 90), parent=21 if side < 0 else 22)
    # robot-only
    pelvis(t, 20, 11.0, w=6.4, t_=("D", "A"))
    kw = dict(pad=(3.6, 3.2, 4.4), upper=(2.6, 4.0, 2.6), fore=(3.2, 5.0, 3.4), fist=(2.6, 2.4, 2.8),
              pad_t="A", up_t="D", fore_t="P", fist_t="D")
    _, hl = arm(t, 21, -1, (-6.4, 22.0, 0.4), **kw)
    _, hr = arm(t, 22, 1, (6.4, 22.0, 0.4), **kw)
    sw = t.robot_only(24, 22, hr, "Weapon")                                                  # energo sword
    sw.bx((hr[0], hr[1], hr[2]), (0.8, 0.8, 3.0), "A")                                       # guard
    sw.bx((hr[0], hr[1], hr[2] - 5.6), (0.5, 1.4, 9.0), "C")                                 # glowing blade
    hy = 22.6
    h = t.robot_only(23, 0, (0, hy, 0.6), "Head")
    h.bx((0, hy + 0.6, 0.6), (2.0, 1.2, 2.0), "D")
    h.bx((0, hy + 2.8, 0.6), (4.4, 4.0, 4.0), "D")                                           # grey helmet
    h.bx((0, hy + 5.0, 0.4), (1.0, 1.6, 4.2), "A")                                           # gold crest fin
    h.bx((0, hy + 3.2, -1.5), (3.4, 1.0, 0.4), "C")                                          # the visor
    h.bx((0, hy + 1.9, -1.45), (2.6, 1.4, 0.4), "P")                                         # faceplate
    t.finish()
    names = ["grimlock", "grimlock transformer", "grim lock", "me grimlock", "grimlock t-rex", "grimlock dinobot",
             "dinobot grimlock", "king grimlock", "g1 grimlock"]
    recipe = dict(name="Grimlock", form="Kaiju", mech=["Blade", "ArticulatedTail", "Stomp", "Breath", "Heavy",
                                                          "Transforms"],
                  elem="Fire", scale=10.0, mass=30000, accent=GOLD, icon="grimlock",
                  feat=["Spikes"], temp="Aggressive", inten=0.8)
    note = ("Grimlock (G1 Dinobot leader): T-rex that breathes fire -> robot w/ energo sword. Dino legs stay the\n"
            "legs, body -> chest, T-rex head hangs on his back snout-up, tail -> back + calf armour, tiny arms ->\n"
            "shoulder pads. An Autobot, but 'me Grimlock' is no pacifist: Aggressive.")
    return names, recipe, t, note


# ------------------------------------------------------------------------------ Optimus Primal

def optimus_primal():
    t = TF(k=1.5)
    # gorilla torso -> robot chest (Rx 90: shoulders up, the gorilla's back becomes his back)
    c = t.panel(0, anchor=(0, 10.0, 1.0))
    c.el((0, 10.0, 1.0), (8.4, 6.8, 10.0), "D")                                              # dark fur torso
    c.el((0, 11.6, -2.6), (8.8, 5.0, 4.4), "D")                                              # shoulder hump
    c.bx((0, 7.2, -1.0), (5.2, 1.2, 6.0), "A")                                               # silver chest plate (underside)
    c.bx((0, 7.0, -1.0), (1.0, 1.0, 5.0), "P")                                               # centre seam
    t.place(0, at=(0, 15.2, 0.4), rot=(90, 0, 0))
    # gorilla arms -> robot arms (same panels: LegFront in beast mode, Arm in robot mode)
    for side, pid in ((-1, 1), (1, 2)):
        x = side * 4.6
        a = t.panel(pid, anchor=(x, 11.6, -3.4))
        a.el((x, 11.0, -3.4), (3.4, 3.2, 3.4), "D")                                          # shoulder
        a.bx((x, 7.4, -3.6), (2.8, 5.0, 3.0), "D")                                           # upper arm
        a.bx((x, 3.4, -3.8), (3.2, 4.0, 3.4), "A")                                           # silver forearm
        a.bx((x, 0.9, -4.0), (3.4, 1.8, 3.6), "D")                                           # knuckle fist
        t.alt_part(pid, 0, (x, 11.6, -3.4), "LegFrontLeft" if side < 0 else "LegFrontRight")
        t.place(pid, at=(side * 6.4, 20.2, 0.6), parent=0, pivot=(side * 6.4, 20.2, 0.6),
                role="ArmLeft" if side < 0 else "ArmRight")
    # gorilla legs -> robot legs
    for side, pid in ((-1, 4), (1, 5)):
        x = side * 2.6
        b = t.panel(pid, anchor=(x, 8.6, 4.4))
        b.bx((x, 6.2, 4.4), (3.0, 4.8, 3.4), "D")                                            # thigh
        b.bx((x, 2.4, 4.2), (2.6, 3.8, 2.8), "A")                                            # silver shin
        b.bx((x, 0.5, 3.4), (3.0, 1.0, 4.2), "D")                                            # foot
        role = "LegBackLeft" if side < 0 else "LegBackRight"
        t.alt_part(pid, 0, (x, 8.6, 4.4), role)
        t.place(pid, at=(side * 2.8, 8.6, 0.4), parent=0, pivot=(side * 2.8, 8.6, 0.4), role=role)
    # gorilla head -> folds behind the robot head (a hood on his back)
    hd = t.panel(3, anchor=(0, 10.8, -5.4))
    hd.el((0, 10.4, -6.6), (4.4, 4.2, 4.0), "D")
    hd.bx((0, 11.8, -8.4), (4.4, 1.0, 1.0), "D")                                             # brow ridge
    hd.bx((0, 9.0, -8.4), (3.0, 2.0, 1.2), "A")                                              # muzzle
    for sx in (-1, 1):
        hd.bx((sx * 1.1, 11.0, -8.5), (0.8, 0.6, 0.4), "C")                                  # eyes
    t.alt_part(3, 0, (0, 10.8, -5.4), "Head")
    t.place(3, at=(0, 20.4, 4.2), rot=(0, 180, 0), parent=0, role="Other")
    pelvis(t, 20, 9.6, w=6.2, t_=("D", "A"))
    for side, pid in ((-1, 24), (1, 25)):                                                    # twin swords
        x = side * 6.4
        s = t.robot_only(pid, 1 if side < 0 else 2, (x, 10.3, 0.2), "Weapon")
        s.bx((x, 10.3, 0.2), (0.8, 0.8, 2.0), "A")
        s.bx((x, 10.3, -4.4), (0.4, 1.2, 7.4), "C")
    ln = t.robot_only(26, 0, (4.2, 20.8, 2.6), "Weapon")                                     # shoulder launcher
    ln.bx((4.2, 22.0, 2.6), (2.2, 1.8, 3.2), "D")
    ln.bx((4.2, 22.0, 0.9), (1.4, 1.0, 0.4), "C")
    hy = 20.4
    h = t.robot_only(23, 0, (0, hy, 0.4), "Head")
    h.bx((0, hy + 0.5, 0.4), (1.8, 1.0, 1.8), "D")
    h.bx((0, hy + 2.6, 0.4), (4.0, 3.8, 3.8), "P")                                           # helmet
    h.bx((0, hy + 1.8, -1.55), (2.6, 2.0, 0.4), "A")                                         # battle mask
    for mx in (-0.6, 0.6):
        h.bx((mx, hy + 1.6, -1.8), (0.3, 1.4, 0.3), "D")                                     # mask slats
    h.bx((0, hy + 3.0, -1.6), (2.8, 0.6, 0.4), "C")                                          # eyes
    h.bx((0, hy + 4.6, 0.2), (0.8, 0.8, 3.0), "A")                                           # crest
    t.finish()
    names = ["optimus primal", "primal transformer", "optimus primal gorilla", "beast wars optimus",
             "optimus gorilla", "maximal leader"]
    recipe = dict(name="Optimus Primal", form="Quadruped", mech=["Blunt", "Blade", "Charge", "Transforms"],
                  scale=3.6, mass=4000, accent=(0.72, 0.74, 0.8), icon="optimus_primal",
                  feat=["Armor"], temp="Protective", inten=0.8)
    note = ("Optimus Primal (Beast Wars / Rise of the Beasts): knuckle-walking gorilla -> Maximal leader. The\n"
            "gorilla arms/legs are the robot arms/legs (LegFront -> Arm roles), torso stands up, gorilla head\n"
            "folds behind his battle mask; twin swords + shoulder launcher.")
    return names, recipe, t, note
