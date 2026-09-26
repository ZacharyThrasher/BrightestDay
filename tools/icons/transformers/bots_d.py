"""Decepticons: Megatron (tank), Starscream (F-15), Soundwave (cassette deck), Shockwave (space gun)."""
import math

from tflib import TF, axes
from bots_a import arm, pelvis

SILVER = (0.78, 0.8, 0.86)


# ------------------------------------------------------------------------------ Megatron

def megatron():
    """Alt mode = a Cybertronian-style tank (Armada / Revenge of the Fallen / War for Cybertron). Chosen over
    the G1 handgun and the 2007 jet: the tank's main gun *is* the fusion cannon (one panel, same prims),
    a tank silhouette reads at 30 m, and the jet slot belongs to Starscream."""
    t = TF(k=1.3)
    # hull front -> chest (Rx -90: glacis faces front, front edge at the waist)
    h = t.panel(0, anchor=(0, 4.6, -0.5))
    h.bx((0, 3.2, -4.5), (6.4, 2.8, 8.0), "A")                                               # hull front
    h.bx((0, 2.4, -8.8), (6.0, 1.8, 1.4), "D", rot=(-35, 0, 0))                            # glacis / belt
    h.bx((0, 4.7, -4.0), (4.4, 0.5, 5.0), "D")                                               # engine deck -> chest plate
    h.bx((-1.6, 4.9, -6.2), (1.2, 0.4, 1.2), "C")                                            # vision slits -> chest lights
    h.bx((1.6, 4.9, -6.2), (1.2, 0.4, 1.2), "C")
    t.place(0, at=(0, 23.0, -0.8), rot=(-90, 0, 0))
    # hull rear -> back plate
    hr = t.panel(5, anchor=(0, 4.6, -0.5))
    hr.bx((0, 3.2, 3.0), (6.4, 2.8, 7.0), "A")
    hr.bx((0, 4.7, 4.0), (3.0, 0.4, 4.0), "D")                                               # exhaust grate
    t.place(5, at=(0, 23.0, 4.8), rot=(90, 0, 0), parent=0)
    # treads -> legs: tread length -> leg, tread height -> leg width, road wheels face front
    legs = {}
    for side, pid in ((-1, 1), (1, 2)):
        x = side * 4.6
        b = t.panel(pid, anchor=(x, 3.8, -7.0))
        b.bx((x, 1.9, 0.0), (2.8, 3.8, 14.0), "D")                                           # track
        for z in (-4.5, -1.5, 1.5, 4.5):
            b.cx((x + side * 0.9, 1.9, z), 2.4, 1.2, "A")                                    # road wheels -> knee/shin discs
        b.bx((x, 4.1, 0.0), (3.0, 0.6, 14.0), "A")                                           # track guard -> leg side
        b.bx((x, 1.4, 6.8), (2.8, 2.8, 1.4), "A")                                            # rear sprocket -> foot
        t.alt_part(pid, 0, (x, 3.8, -7.0), "Other")
        rot = axes((0, 0, 1), (-1, 0, 0), (0, -1, 0)) if side < 0 else axes((0, 0, -1), (1, 0, 0), (0, -1, 0))
        t.place(pid, at=(side * 4.8, 14.0, 0.2), rot=rot, parent=0, pivot=(side * 2.9, 14.0, 0.2),
                role="LegBackLeft" if side < 0 else "LegBackRight")
        legs[side] = pid
    # turret halves -> shoulder pads
    for side, pid in ((-1, 3), (1, 4)):
        x = side * 1.6
        tu = t.panel(pid, anchor=(x, 4.6, 0.5))
        tu.bx((x, 5.9, 0.5), (3.2, 2.6, 6.0), "A")
        tu.bx((x + side * 0.9, 7.3, 1.2), (1.2, 0.5, 2.0), "D")                              # hatch
        t.place(pid, at=(side * 7.9, 21.4, 0.3), rot=(0, 0, 0), parent=21 if side < 0 else 22)
    # main gun -> the FUSION CANNON on the right forearm
    g = t.panel(6, anchor=(0, 5.9, -2.5))
    g.cz((0, 5.9, -8.0), 2.0, 11.0, "D")
    g.cz((0, 5.9, -13.6), 2.6, 1.4, "D")                                                     # muzzle brake
    g.cz((0, 5.9, -14.4), 1.4, 0.4, "C")                                                     # glowing bore
    g.cz((0, 5.9, -3.6), 2.8, 2.2, "A")                                                      # mantlet -> breech
    t.place(6, at=(9.4, 14.4, 4.2), parent=22, role="Weapon")
    # robot-only: pelvis, arms, bucket helmet
    pelvis(t, 20, 14.6, w=6.4, t_=("D", "A"))
    arm(t, 21, -1, (-7.6, 22.4, 0.3), pad=(3.2, 2.4, 4.0), upper=(2.4, 4.0, 2.4), fore=(3.0, 5.2, 3.2),
        fist=(2.4, 2.2, 2.6), pad_t="A", fore_t="A")
    arm(t, 22, 1, (7.6, 22.4, 0.3), pad=(3.2, 2.4, 4.0), upper=(2.4, 4.0, 2.4), fore=(3.0, 5.2, 3.2),
        fist=(2.4, 2.2, 2.6), pad_t="A", fore_t="A")
    hy = 23.4
    hd = t.robot_only(23, 0, (0, hy, 0.4), "Head")
    hd.bx((0, hy + 0.5, 0.4), (1.8, 1.0, 1.8), "D")
    hd.bx((0, hy + 2.8, 0.5), (4.4, 4.2, 4.2), "A")                                         # bucket helmet
    hd.el((0, hy + 4.9, 0.5), (4.4, 1.6, 4.2), "A")                                         # domed top
    hd.bx((0, hy + 4.6, -0.6), (0.9, 1.8, 3.6), "D")                                         # central crest ridge
    hd.bx((0, hy + 2.2, -1.65), (2.6, 2.6, 0.4), "D")                                        # face
    hd.bx((0, hy + 2.8, -1.8), (2.4, 0.6, 0.4), "C")                                         # red eyes
    for sx in (-1, 1):
        hd.bx((sx * 2.4, hy + 2.2, 0.4), (0.6, 2.8, 2.6), "A")                               # cheek guards
    t.finish()
    names = ["megatron", "megatron transformer", "megs", "megatron tank", "lord megatron", "decepticon leader",
             "g1 megatron", "galvatron transformer"]
    recipe = dict(name="Megatron", form="Vehicle", mech=["Shoots", "Explodes", "Heavy", "Transforms"], scale=6.0,
                  mass=12000, accent=SILVER, icon="megatron",
                  payload=dict(name="fusion cannon blast", form="Ball", mech=["Explodes", "Fast"], elem="Atomic",
                               scale=0.8, mass=5.0, accent=(0.6, 0.3, 0.9), seed=1984),
                  feat=["Treads", "Turret", "Barrels", "Armor"], temp="Aggressive", inten=0.9)
    note = ("Megatron: silver Cybertronian tank -> bucket-helmeted Decepticon leader. The tank gun IS the fusion\n"
            "cannon (same panel) on his right arm; treads -> legs, turret halves -> shoulder pads.\n"
            "Tank over G1 gun / 2007 jet: the cannon maps 1:1, reads at range, and Starscream owns the jet.")
    return names, recipe, t, note


# ------------------------------------------------------------------------------ Starscream

def starscream():
    t = TF(k=1.3)
    # cockpit section + nose -> chest (canopy on the chest, nose points down as the codpiece)
    c = t.panel(0, anchor=(0, 5.6, -1.0))
    c.bx((0, 4.0, -4.5), (3.6, 3.2, 7.0), "A")                                               # forward fuselage (red chest)
    c.el((0, 5.8, -5.0), (2.4, 1.8, 4.6), "P")                                               # canopy (blue glass)
    c.cone((0, 4.0, -9.5), 3.0, 3.2, "D", dir="-z", d2=2.6)                                 # nose cone
    t.place(0, at=(0, 23.0, 1.2), rot=(-90, 0, 0))
    # intakes -> upper arms (shared), guns under the wings -> null rays on the forearms
    arms = {}
    for side, pid in ((-1, 3), (1, 4)):
        x = side * 2.9
        a = t.panel(pid, anchor=(x, 5.2, -4.0))
        a.bx((x, 3.9, -1.2), (2.4, 2.6, 5.6), "A")                                           # intake
        a.bx((x, 3.9, -4.1), (2.0, 2.0, 0.4), "D")                                           # intake mouth -> shoulder top
        t.alt_part(pid, 0, (x, 5.2, -4.0), "Other")
        t.place(pid, at=(side * 6.2, 23.4, 1.4), rot=(90, 0, 0), parent=0, pivot=(side * 6.2, 22.4, 0.2),
                role="ArmLeft" if side < 0 else "ArmRight")
        arms[side] = pid
        fi = t.robot_only(20 + (0 if side < 0 else 1), pid, (side * 6.2, 17.4, 0.2), "Other")
        fi.bx((side * 6.2, 16.2, -0.2), (2.6, 4.2, 2.8), "D")                                # forearm
        fi.bx((side * 6.2, 13.2, -0.2), (2.2, 2.0, 2.4), "A")                                # fist
        n = t.panel(10 + (0 if side < 0 else 1), anchor=(side * 6.5, 3.0, 1.0))
        n.cz((side * 6.5, 3.0, 0.5), 1.0, 5.0, "D")                                          # null ray
        n.cz((side * 6.5, 3.0, -2.2), 0.6, 0.6, "C")
        t.place(10 + (0 if side < 0 else 1), at=(side * 7.8, 16.6, 1.4), parent=20 + (0 if side < 0 else 1),
                role="Weapon")
    # rear fuselage halves -> legs: engine nozzles become the thruster feet, tail fins stick out of the calves
    legs = {}
    for side, pid in ((-1, 1), (1, 2)):
        x = side * 1.5
        b = t.panel(pid, anchor=(x, 5.2, -1.0))
        b.bx((x, 3.8, 4.8), (2.8, 2.8, 11.6), "P")                                           # blue leg / fuselage
        b.cz((x, 3.6, 11.2), 2.4, 1.4, "D")                                                  # engine nozzle -> foot
        b.cz((x, 3.6, 11.9), 1.4, 0.4, "C")                                                  # afterburner glow
        b.bx((x + side * 0.4, 7.2, 8.4), (0.5, 4.2, 3.4), "A", rot=(0, 0, side * 12))        # twin tail fin
        b.bx((x + side * 2.4, 3.8, 9.8), (3.0, 0.4, 2.6), "D")                               # stabiliser -> ankle spur
        b.bx((x, 2.2, 0.6), (2.0, 0.4, 2.2), "D")                                            # knee
        t.alt_part(pid, 0, (x, 5.2, -1.0), "Other")
        t.place(pid, at=(side * 2.0, 13.4, 1.4), rot=(90, 0, 0), parent=0, pivot=(side * 2.0, 13.4, 0.0),
                role="LegBackLeft" if side < 0 else "LegBackRight")
        legs[side] = pid
    # wings -> the iconic seeker wings on his back
    for side, pid in ((-1, 5), (1, 6)):
        w = t.panel(pid, anchor=(side * 1.8, 4.2, 0.0))
        w.bx((side * 5.6, 4.2, 3.6), (7.6, 0.5, 6.0), "D")                                   # main wing
        w.bx((side * 3.6, 4.2, -0.8), (3.6, 0.5, 3.0), "D", rot=(0, side * 35, 0))           # leading-edge root
        w.bx((side * 8.4, 4.25, 3.4), (1.6, 0.5, 4.4), "A")                                  # red wing tip stripe
        w.bx((side * 5.0, 4.25, 2.0), (1.6, 0.5, 1.6), "P")                                  # insignia patch
        t.alt_part(pid, 0, (side * 1.8, 4.2, 0.0), "Other")
        cs, sn = math.cos(math.radians(25)), math.sin(math.radians(25))                       # 25 deg up
        rot = axes((cs, side * sn, 0), (0, 0, 1), (side * sn, -cs, 0))
        t.place(pid, at=(side * 2.0, 21.6, 3.8), rot=rot, parent=0)
    pelvis(t, 22, 13.8, w=5.2, t_=("D", "A"))
    hy = 23.2
    hd = t.robot_only(23, 0, (0, hy, 0.6), "Head")
    hd.bx((0, hy + 0.5, 0.6), (1.4, 1.0, 1.4), "D")
    hd.bx((0, hy + 2.4, 0.6), (3.4, 3.4, 3.2), "P")                                          # blue helmet
    hd.bx((0, hy + 4.4, 0.4), (0.8, 2.2, 2.4), "P")                                          # tall crest
    for sx in (-1, 1):
        hd.bx((sx * 1.2, hy + 4.2, 0.6), (0.6, 1.8, 0.8), "P", rot=(0, 0, -sx * 18))         # crown points
    hd.bx((0, hy + 2.0, -1.1), (2.2, 2.0, 0.4), "D")                                         # grey face
    hd.bx((0, hy + 2.5, -1.25), (2.2, 0.5, 0.4), "C")                                        # red eyes
    t.finish()
    names = ["starscream", "starscream transformer", "screamer", "star scream", "starscream jet",
             "decepticon seeker", "g1 starscream", "air commander starscream"]
    recipe = dict(name="Starscream", form="Vehicle", mech=["Flies", "Shoots", "Fast", "Transforms"], scale=6.0,
                  mass=3500, accent=(0.86, 0.14, 0.2), icon="starscream",
                  payload=dict(name="null ray", form="Ball", mech=["Fast"], elem="Electric", scale=0.4, mass=1.0,
                               accent=(0.5, 0.7, 1.0), seed=15),
                  feat=["Jet", "Cockpit", "Fins"], temp="Sneaky", inten=0.85)
    note = ("Starscream: grey/red/blue F-15-style seeker jet. Cockpit -> chest, intakes -> arms, underwing guns ->\n"
            "arm null rays, rear fuselage -> legs w/ thruster feet + tail fins, wings -> back. Sneaky (of course).")
    return names, recipe, t, note


# ------------------------------------------------------------------------------ Soundwave

def soundwave():
    t = TF(k=1.3)
    # the deck stands upright, face -Z. Cassette door (0) -> chest (pure slide: it stays facing front)
    c = t.panel(0, anchor=(0, 11.0, 0))
    c.bx((0, 15.5, 0), (11.0, 9.0, 4.4), "A")
    c.bx((0, 15.6, -2.25), (7.0, 5.2, 0.4), "D")                                             # cassette window
    c.bx((0, 15.6, -2.4), (5.4, 1.2, 0.3), "P")                                              # tape
    for sx in (-1.6, 1.6):
        c.cy((sx, 15.6, -2.45), (1.6, 0.4, 1.6), "C", rot=(90, 0, 0))                        # reels
    c.bx((0, 12.2, -2.25), (9.0, 0.6, 0.4), "D")                                             # door seam -> belt line
    t.place(0, at=(0, 13.6, 0))
    # lower body halves (speaker grille, battery door) -> legs (pure slide down/out)
    legs = {}
    for side, pid in ((-1, 1), (1, 2)):
        x = side * 2.7
        b = t.panel(pid, anchor=(x, 11.0, 0))
        b.bx((x, 5.5, 0), (5.2, 11.0, 4.4), "A")
        for gy in (6.0, 7.2, 8.4, 9.6):
            b.bx((x, gy, -2.25), (4.0, 0.5, 0.4), "D")                                       # speaker grille -> thighs
        b.bx((x, 1.2, -2.3), (4.4, 2.4, 0.6), "P")                                           # foot plate
        t.alt_part(pid, 0, (x, 11.0, 0), "Other")
        t.place(pid, at=(side * 3.2, 12.6, 0.2), parent=0, pivot=(side * 3.2, 12.6, 0.0),
                role="LegBackLeft" if side < 0 else "LegBackRight")
        legs[side] = pid
    # top buttons -> shoulder tops
    cols = ["P", "C", "D", "P", "C", "D"]
    for side, pid in ((-1, 3), (1, 4)):
        bt = t.panel(pid, anchor=(side * 2.8, 20.0, 0))
        for i in range(3):
            bx = side * (1.0 + i * 1.5)
            bt.bx((bx, 20.4, -0.4), (1.2, 0.8, 2.6), cols[i + (0 if side < 0 else 3)])       # play/rec/stop keys
        t.place(pid, at=(side * 7.4, 23.4, 0), parent=21 if side < 0 else 22)
    # robot-only: arms, head, shoulder cannon, concussion blaster
    pelvis(t, 20, 13.2, w=6.0, t_=("D", "P"))
    _, hl = arm(t, 21, -1, (-7.4, 22.2, 0.2), pad=(3.6, 2.4, 4.2), pad_t="A", fore_t="A")
    _, hr = arm(t, 22, 1, (7.4, 22.2, 0.2), pad=(3.6, 2.4, 4.2), pad_t="A", fore_t="A")
    sc = t.robot_only(24, 21, (-7.4, 23.6, 0.2), "Weapon")                                   # shoulder cannon
    sc.bx((-7.4, 25.0, 0.8), (1.8, 1.8, 3.2), "D")
    sc.cz((-7.4, 25.0, -1.4), 1.0, 1.6, "D")
    sc.cz((-7.4, 25.0, -2.3), 0.6, 0.4, "C")
    g = t.robot_only(25, 22, hr, "Weapon")                                                   # concussion blaster
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 2.4), (1.4, 1.8, 4.2), "D")
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 4.8), (0.8, 0.8, 0.6), "C")
    hy = 20.2
    hd = t.robot_only(23, 0, (0, hy, 0.2), "Head")
    hd.bx((0, hy + 0.5, 0.2), (1.8, 1.0, 1.8), "D")
    hd.bx((0, hy + 2.6, 0.3), (4.2, 3.8, 3.8), "A")                                          # blue helmet
    for sx in (-1, 1):
        hd.bx((sx * 2.4, hy + 2.8, 0.3), (0.8, 3.8, 3.0), "A")                               # side "ear" plates
        hd.bx((sx * 1.6, hy + 4.7, 0.1), (0.8, 0.8, 2.6), "A")                               # helmet ridges
    hd.bx((0, hy + 3.0, -1.6), (3.4, 1.0, 0.4), "C")                                         # the visor
    hd.bx((0, hy + 1.8, -1.55), (2.4, 1.4, 0.4), "D")                                        # faceplate
    t.finish()
    names = ["soundwave", "soundwave transformer", "soundwave decepticon", "g1 soundwave",
             "soundwave boombox", "soundwave cassette player", "soundwave superior"]
    recipe = dict(name="Soundwave", form="Biped", mech=["Shoots", "Transforms"], scale=4.0, mass=3000,
                  accent=(0.24, 0.4, 0.92), icon="soundwave",
                  payload=dict(name="laserbeak", form="Bird", mech=["Flies", "Homing", "Shoots", "Swarm"], scale=1.0, mass=20.0,
                               count=3, accent=(0.85, 0.2, 0.2), seed=31),
                  feat=["Antenna"], temp="Sneaky", inten=0.7)
    note = ("Soundwave: G1 microcassette deck -> the Decepticon communications officer. Cassette door -> chest\n"
            "(with the reels), speaker halves -> legs, keys -> shoulders; shoulder cannon + concussion blaster.\n"
            "Payload = cassette minions (Laserbeak), 'Soundwave superior'.")
    return names, recipe, t, note


# ------------------------------------------------------------------------------ Shockwave

def shockwave():
    t = TF(k=1.3)
    # receiver (0) -> chest (Ry 90: length becomes width)
    c = t.panel(0, anchor=(0, 13.0, 1.5))
    c.bx((0, 13.6, 1.5), (4.6, 5.2, 9.0), "A")
    c.bx((0, 16.4, 1.5), (3.0, 0.5, 7.0), "D")                                               # top rail -> collar
    c.bx((2.35, 13.6, 1.5), (0.4, 2.4, 6.0), "D")                                            # side vent -> chest vent
    t.place(0, at=(0, 16.4, 0.4), rot=(0, 90, 0))
    # grip halves -> legs (pure slide)
    legs = {}
    for side, pid in ((-1, 1), (1, 2)):
        x = side * 1.35
        b = t.panel(pid, anchor=(x, 11.0, 3.6))
        b.bx((x, 5.5, 3.6), (2.6, 11.0, 3.8), "A")
        b.bx((x, 0.6, 3.2), (2.6, 1.2, 4.6), "D")                                            # grip butt -> foot
        b.bx((x, 8.5, 1.6), (2.0, 2.0, 0.4), "D")                                            # trigger guard -> knee
        t.alt_part(pid, 0, (x, 11.0, 3.6), "Other")
        t.place(pid, at=(side * 2.2, 11.0, 0.4), parent=0, pivot=(side * 2.2, 11.0, 0.4),
                role="LegBackLeft" if side < 0 else "LegBackRight")
        legs[side] = pid
    # barrel -> left-arm cannon
    br = t.panel(4, anchor=(0, 13.6, -3.0))
    br.cz((0, 13.6, -9.0), 2.4, 12.0, "D")
    br.cz((0, 13.6, -15.2), 1.6, 0.6, "C")                                                   # muzzle glow
    br.cz((0, 13.6, -4.4), 3.0, 2.0, "A")                                                    # barrel shroud -> elbow
    t.place(4, at=(-7.2, 12.6, 3.2), parent=21, role="Weapon")
    # scope -> back antenna / shoulder sensor
    sc = t.panel(3, anchor=(0, 16.9, 2.0))
    sc.cz((0, 17.3, 2.0), 1.2, 5.0, "D")
    sc.cz((0, 17.3, -0.6), 0.8, 0.3, "C")
    t.place(3, at=(0, 19.4, 3.0), rot=(-90, 0, 0), parent=0)
    pelvis(t, 20, 12.2, w=5.4, t_=("D", "A"))
    arm(t, 21, -1, (-6.6, 19.2, 0.3), pad=(3.2, 3.2, 4.0), fore=(2.6, 3.0, 2.6), fist=(1.6, 1.0, 1.6),
        pad_t="A", fore_t="A")
    arm(t, 22, 1, (6.6, 19.2, 0.3), pad=(3.2, 3.2, 4.0), pad_t="A", fore_t="A")
    hy = 19.4
    hd = t.robot_only(23, 0, (0, hy, 0.4), "Head")
    hd.bx((0, hy + 0.5, 0.4), (1.6, 1.0, 1.6), "D")
    hd.el((0, hy + 2.7, 0.4), (4.4, 4.2, 4.0), "A")                                          # rounded purple head
    hd.cy((0, hy + 2.8, -1.4), (2.6, 0.6, 2.6), "C", rot=(90, 0, 0))                         # THE single eye
    for sx in (-1, 1):
        hd.bx((sx * 1.9, hy + 5.4, 0.4), (0.6, 2.8, 0.8), "D", rot=(0, 0, -sx * 10))         # antenna horns
    t.finish()
    names = ["shockwave transformer", "shockwave decepticon", "decepticon shockwave", "g1 shockwave",
             "shockwave cyclops robot", "shockwave space gun", "shock wave transformer"]
    recipe = dict(name="Shockwave", form="Biped", mech=["Beam", "Shoots", "Transforms"], scale=4.0, mass=4000,
                  accent=(0.56, 0.24, 0.78), icon="shockwave_transformer",
                  feat=["Barrels", "Antenna"], temp="Aggressive", inten=0.5)
    note = ("Shockwave: G1 Cybertronian space gun -> one-eyed purple robot; the gun barrel becomes his left-arm\n"
            "cannon (Beam). Grip halves -> legs, receiver -> chest, scope -> back sensor. Cold logic: low-intensity\n"
            "Aggressive. Bare 'shockwave' stays the physics word.")
    return names, recipe, t, note
