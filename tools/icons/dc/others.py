"""Other superhero IP: Power Rangers (zords combine!), Ben 10, Powerpuff Girls, Hellboy, The Incredibles, Megamind,
Static Shock, Kick-Ass, The Umbrella Academy."""
import math

from dlib import (S, R, PR_RED, OMNI_GREEN, HELL_RED, INCRED_RED, MEGA_BLUE, PPG_PINK, TIP_NZ, ALONG_Z, ALONG_X)
from hero import hero, head, brute, cape, humanoid, tail


# ------------------------------------------------------------------------------ Power Rangers

def tyrannozord():
    s = S()
    # a mechanical T-rex: body tilted forward, big head with jaw, thick legs, tail, tiny arms, cockpit in the head
    s.box((0, 13.0, 0), (7.0, 7.0, 11.0), "A", (12, 0, 0))                                   # torso
    s.box((0, 11.0, -3.8), (5.6, 6.0, 1.0), "D", (12, 0, 0))                                 # chest plate
    s.box((0, 13.0, -0.4), (7.4, 1.0, 11.4), "C", (12, 0, 0))                                # silver trim band
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):
        s.on(pid)
        s.box((sx * 3.8, 9.0, 1.4), (3.2, 7.0, 5.0), "A", (-15, 0, 0))                      # thigh
        s.box((sx * 3.8, 4.0, 0.6), (2.6, 6.0, 2.8), "D")                                    # shin
        s.box((sx * 3.8, 0.8, -1.2), (3.6, 1.6, 6.0), "A")                                   # foot
        for dx in (-1.0, 0.0, 1.0):
            s.cone((sx * 3.8 + dx, 0.8, -4.8), (0.8, 1.6, 0.8), "C", TIP_NZ)                # toe claws
        s.part(pid, 0, (sx * 3.8, 12.0, 1.4), role)
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):
        s.on(pid)
        s.box((sx * 3.0, 11.0, -6.0), (1.4, 3.6, 1.4), "D", (30, 0, 0))                     # tiny arms
        s.part(pid, 0, (sx * 3.0, 12.4, -5.0), role)
    s.on(5)
    s.box((0, 19.0, -8.0), (5.6, 5.0, 8.0), "A")                                             # head
    s.box((0, 21.2, -7.6), (5.0, 1.4, 6.0), "D")                                             # brow crest
    for sx in (-1, 1):
        s.box((sx * 2.0, 20.0, -11.6), (1.4, 1.0, 0.6), "C")                                # eyes
        for zz in (-9.0, -10.6):
            s.cone((sx * 1.8, 17.0, zz), (0.6, 1.2, 0.6), "C", (180, 0, 0))                 # teeth
    s.box((0, 19.6, -5.0), (3.0, 2.0, 2.0), "C")                                             # cockpit windows
    s.part(5, 0, (0, 16.0, -4.0), "Head")
    s.on(6)
    s.box((0, 16.0, -8.6), (4.8, 1.6, 6.4), "A")                                             # jaw
    s.part(6, 5, (0, 17.0, -5.0), "Jaw")
    s.on(0)
    tail(s, [(0, 12.4, 5.0), (0, 11.0, 10.0), (0, 9.0, 15.0), (0, 7.0, 19.0)],
         [(5.0, 4.6, 6.0), (3.6, 3.4, 5.6), (2.4, 2.2, 5.0)], t="A")
    s.seat(5, (0, 19.6, -5.0))
    names = ["tyrannosaurus dinozord", "tyrannosaurus zord", "red dinozord", "t-rex dinozord", "red ranger zord",
             "tyrannosaurus dinozord mmpr"]
    r = R("Tyrannosaurus Dinozord", "Biped", ["Transforms", "Stomp", "Grab", "ArticulatedTail", "Piloted"], 8.0,
          6000.0, PR_RED, feat=["Cockpit", "Armor"], temp="Aggressive", inten=0.7, source="Power Rangers")
    return names, r, s, ("The red Tyrannosaurus Dinozord: ride it (Piloted) or send it. It Transforms - own 3+ Transforms\n"
                         "constructs and say 'combine' to form a Megazord.")


def red_ranger():
    s = S()
    b = hero(s, suit="A", legs="A", arms="A", boots="C", gloves="C", glove_h=3.0, belt="D", buckle="C")
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    for i, x in enumerate((-2.0, -1.0, 0.0, 1.0, 2.0)):                                       # the white diamond band
        s.box((x, ys - 2.0 - abs(x) * 0.3, -td / 2 - 0.25), (1.0, 1.0, 0.4), "C", (0, 0, 45))
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.box((hx, hy, hz + 0.3), (0.8, 0.8, 2.0), "D")                                           # power sword
    s.box((hx, hy, hz - 0.8), (2.2, 1.0, 0.6), "C")
    s.seg((hx, hy + 0.3, hz - 1.1), (hx, hy + 2.0, hz - 11.0), 0.4, 1.4, "C")
    s.on(5)
    hy2 = b["y_neck"] + 2.1
    s.ell((0, hy2, 0), (3.9, 4.4, 4.2), "A")                                                  # helmet
    s.box((0, hy2 + 0.2, -1.8), (3.0, 1.4, 0.8), "D", (0, 0, 0))                              # black visor
    s.box((0, hy2 - 0.6, -1.9), (1.2, 1.4, 0.6), "D")                                         # T-rex tooth visor
    s.box((0, hy2 - 1.6, -1.9), (2.2, 0.8, 0.5), "C")                                         # silver mouthplate
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["red ranger", "red power ranger", "mighty morphin red ranger", "jason lee scott", "red mighty morphin ranger"]
    r = R("Red Ranger", "Biped", ["Blade", "Fast", "Bounce"], 1.8, 75.0, PR_RED, temp="Aggressive", inten=0.8,
          source="Power Rangers")
    return names, r, s, "The Red Ranger: diamond-banded red suit, T-rex visor, Power Sword. Acrobatic (Bounce) swordsman."


# ------------------------------------------------------------------------------------ Ben 10

def four_arms():
    s = S()
    b = brute(s, bulk=1.4, leg_h=8.6, torso=(10.4, 11.0, 6.2), arm_len=10.0, arm_spread=16, hunch=False)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, (yh + ys) / 2 + 1.0, -td / 2 - 0.3), (0.9, b["th"] * 0.8, 0.4), "D")            # black center stripe
    s.box((0, yh + 0.6, 0), (tw * 0.8, 1.6, td * 1.02), "D")
    s.cyl((0, ys - 2.0, -td / 2 - 0.5), (2.2, 0.4, 2.2), "C", (90, 0, 0))                      # omnitrix symbol
    for pid, sx in ((3, -1), (4, 1)):                                                         # the second (lower) pair
        s.on(pid)
        x = sx * (b["arm_x"] - 0.8)
        top = ys - 4.6
        s.seg((x, top, 0.4), (x + sx * 2.8, top - 8.0, -0.6), b["arm_w"] * 0.9, b["arm_w"] * 0.9, "P")
        s.box((x + sx * 2.8, top - 9.2, -0.8), (3.4, 3.4, 3.4), "P")
    s.on(5)
    hy = b["y_neck"] + 2.0
    s.ell((0, hy, -0.4), (4.4, 4.2, 4.2), "P")
    for i, x in enumerate((-1.4, -0.5, 0.5, 1.4)):                                            # FOUR eyes
        s.box((x, hy + 0.5 - (0.4 if i in (0, 3) else 0), -2.4), (0.8, 0.5, 0.4), "C")
    s.box((0, hy - 1.3, -2.2), (2.2, 0.6, 0.4), "D")
    s.ell((0, hy + 1.8, 0.8), (3.6, 1.6, 3.2), "D")                                            # top-knot hair
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["four arms", "fourarms", "four arms ben 10", "tetramand", "four-arms"]
    r = R("Four Arms", "Biped", ["Grab", "Blunt", "Heavy", "Stomp"], 3.2, 700.0, (0.85, 0.15, 0.15), temp="Aggressive",
          inten=0.9, source="Ben 10")
    return names, r, s, "Four Arms: red Tetramand, four eyes, FOUR fists. Grabs, pummels, stomps."


def heatblast():
    s = S()
    b = hero(s, bulk=0.95, suit="A", legs="A", arms="A", boots="D", gloves="D", arm_spread=12)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    for (x, y) in ((-1.8, ys - 2.0), (1.6, ys - 3.4), (-0.6, yh + 3.4), (1.8, yh + 1.2), (-2.0, yh + 0.6)):
        s.box((x, y, -td / 2 - 0.3), (1.8, 1.4, 0.5), "D", (0, 0, 20))                          # rocky plates
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        hx, hy, hz = b["hands"][sx]
        s.cone((hx, hy + 0.6, hz - 1.4), (2.6, 3.4, 2.6), "C", TIP_NZ)                         # fireball palms
    s.on(5)
    hy2 = b["y_neck"] + 2.2
    s.ell((0, hy2, 0), (3.6, 4.0, 3.8), "D")                                                   # rock face
    for sx in (-1, 1):
        s.box((sx * 0.8, hy2 + 0.2, -1.8), (1.0, 0.6, 0.4), "C")
    for i, (x, z, h, rz) in enumerate(((-1.2, 0.4, 4.6, 20), (0.0, 0.0, 5.8, 0), (1.2, 0.4, 4.6, -20), (0.0, 1.2, 4.0, 0))):
        s.cone((x, hy2 + 2.0 + h / 2, z), (2.0, h, 2.0), "C" if i % 2 else "A", (0, 0, rz))      # the flame head
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["heatblast", "heat blast", "heatblast ben 10", "pyronite"]
    r = R("Heatblast", "Biped", ["Shoots", "Breath", "Flies"], 1.8, 90.0, (1.0, 0.5, 0.1), elem="Fire", temp="Aggressive",
          inten=0.8, payload=R("fireball", "Ball", ["Fast", "Explodes"], 0.6, 2.0, elem="Fire"), source="Ben 10")
    return names, r, s, "Heatblast: a Pyronite - rocky body, head of fire. Fireballs and flame breath."


def cannonbolt():
    s = S()
    s.ell((0, 6.0, 0), (12.0, 12.0, 12.0), "A")                                               # the armour ball
    for rz in (0, 60, 120):
        s.cyl((0, 6.0, 0), (12.4, 1.0, 12.4), "C", (90, 0, rz))                              # plate seams
    s.ell((0, 6.0, -5.6), (3.0, 3.0, 1.4), "D")                                               # face peeking out
    for sx in (-1, 1):
        s.box((sx * 0.8, 6.6, -6.2), (0.8, 0.6, 0.4), "C")
    s.cyl((0, 11.6, 0), (2.4, 0.6, 2.4), "C")                                                  # omnitrix on top
    names = ["cannonbolt", "cannon bolt", "cannonbolt ben 10", "arburian pelarota"]
    r = R("Cannonbolt", "Ball", ["Bounce", "Charge", "Heavy", "Ricochet"], 2.4, 500.0, (0.95, 0.9, 0.7), source="Ben 10")
    return names, r, s, "Cannonbolt: curls into an armoured ball and rolls through everything - bounces, ricochets."


# ------------------------------------------------------------------------- Powerpuff Girls

def powerpuff():
    s = S()
    b = humanoid(s, leg_h=3.0, leg_w=1.4, leg_d=1.4, hip_x=1.0, foot=(1.6, 1.0, 2.0), torso=(4.4, 4.0, 3.0),
                 torso_t="A", arm_w=1.2, arm_len=3.4, arm_x=2.6, hand=1.4, arm_t="P", leg_t="P", hand_t="P",
                 foot_t="D", neck=0.4, arm_spread=30)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, yh + 0.4, 0), (4.6, 0.8, 3.2), "D")                                               # the black belt stripe
    s.box((0, yh - 0.4, 0), (5.2, 1.6, 3.6), "A")                                               # dress hem
    s.on(5)
    hy = b["y_neck"] + 4.2
    s.ell((0, hy, 0), (9.0, 8.4, 8.0), "P")                                                     # the giant head
    for sx in (-1, 1):
        s.ell((sx * 2.0, hy - 0.2, -3.4), (3.4, 3.8, 1.4), "C")                                 # huge eyes
        s.ell((sx * 2.0, hy - 0.4, -4.0), (2.0, 2.4, 0.6), "A")
        s.ell((sx * 2.2, hy + 0.4, -4.3), (0.7, 0.7, 0.3), "C")
    s.box((0, hy - 2.8, -3.6), (1.2, 0.4, 0.4), "D")
    s.ell((0, hy + 2.6, 0.8), (9.4, 4.2, 8.0), "D")                                             # hair cap
    s.box((0, hy + 4.4, 1.0), (3.4, 1.6, 1.6), "A")                                             # Blossom's bow
    for sx in (-1, 1):
        s.box((sx * 2.4, hy + 4.4, 1.0), (2.4, 2.4, 1.0), "A", (0, 0, sx * 20))
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(7)
    s.box((0, hy - 3.0, 4.2), (7.0, 9.0, 1.6), "D")                                             # long hair down the back
    s.part(7, 5, (0, hy, 3.4), "Cloth")
    s.on(0)
    names = ["powerpuff girls", "the powerpuff girls", "powerpuff girl", "blossom bubbles and buttercup",
             "blossom powerpuff", "bubbles powerpuff", "buttercup powerpuff", "ppg"]
    r = R("The Powerpuff Girls", "Biped", ["Flies", "Fast", "Swarm", "Blunt"], 1.0, 20.0, PPG_PINK, count=3,
          temp="Playful", inten=0.9, source="The Powerpuff Girls")
    return names, r, s, "Sugar, spice, everything nice... and Chemical X: three tiny big-headed fliers (count 3), fast punches."


# ---------------------------------------------------------------------------------- Hellboy

def _doom_hand(s, c, k=1.0):
    x, y, z = c
    s.box((x, y, z), (3.6 * k, 3.6 * k, 3.8 * k), "D")                                          # the stone fist
    for i, xx in enumerate((-1.2, 0.0, 1.2)):
        s.box((x + xx * k, y - 0.6 * k, z - 2.0 * k), (1.1 * k, 2.0 * k, 1.2 * k), "D")        # knuckles
    s.box((x, y + 2.6 * k, z + 0.2), (3.0 * k, 2.2 * k, 3.0 * k), "D")                          # stone forearm
    for yy in (0.8, 2.2):
        s.box((x, y + yy * k, z + 0.2), (3.9 * k, 0.4 * k, 3.9 * k), "C")                        # rune bands


def hellboy():
    s = S()
    b = hero(s, bulk=1.2, suit="P", legs="D", arms="P", boots="D", arm_spread=10)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, yh + 1.0, 0), (tw * 0.8, 1.2, td * 1.04), "A")                                    # belt
    s.on(7)                                                                                      # trenchcoat tails
    s.box((0, yh - 2.0, td / 2 + 0.6), (tw + 1.0, 8.0, 0.6), "D", (-6, 0, 0))
    for sx in (-1, 1):
        s.box((sx * (tw / 2 + 0.3), yh - 1.4, 0), (0.6, 7.0, td), "D")
    s.part(7, 0, (0, yh + 2.0, td / 2 + 0.4), "Cloth")
    s.on(0)
    s.box((0, ys - 2.4, 0), (tw + 0.6, 5.2, td + 0.6), "D")                                      # coat upper
    s.box((0, ys - 2.4, -td / 2 - 0.35), (2.8, 5.0, 0.3), "P")                                   # open front
    s.on(4)
    hx, hy, hz = b["hands"][1]
    _doom_hand(s, (hx, hy, hz), 1.1)
    s.on(3)
    hx, hy, hz = b["hands"][-1]                                                                  # the Samaritan
    s.box((hx, hy - 0.6, hz + 0.2), (0.9, 1.8, 1.0), "D")
    s.box((hx, hy + 0.5, hz - 2.4), (1.4, 1.4, 5.6), "D")
    s.cyl((hx, hy + 0.5, hz - 1.4), (2.0, 1.6, 2.0), "A", ALONG_Z)
    s.on(5)
    hy2 = b["y_neck"] + 2.2
    s.ell((0, hy2, 0), (4.2, 4.4, 4.2), "P")
    s.box((0, hy2 - 1.2, -1.2), (3.2, 2.0, 2.4), "P")                                            # square jaw
    for sx in (-1, 1):
        s.box((sx * 0.9, hy2 + 0.3, -2.0), (1.0, 0.5, 0.4), "C")
        s.cyl((sx * 1.3, hy2 + 1.8, -0.8), (1.6, 0.6, 1.6), "C")                                 # filed horn stumps
    s.box((0, hy2 + 1.9, 1.4), (1.6, 1.4, 1.6), "D")                                             # top-knot
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    tail(s, [(0, yh + 0.4, td / 2 + 0.4), (0.6, yh - 3.0, td / 2 + 2.2), (1.6, yh - 5.6, td / 2 + 3.2)],
         [(1.0, 1.0, 3.8), (0.8, 0.8, 3.2)], t="P")
    names = ["hellboy", "anung un rama", "hellboy bprd", "the world's greatest paranormal investigator", "red hellboy"]
    r = R("Hellboy", "Biped", ["Blunt", "Heavy", "Shoots", "ArticulatedTail"], 2.2, 180.0, HELL_RED, elem="Earth",
          temp="Lazy", inten=0.4, source="Hellboy")
    return names, r, s, ("Hellboy: red, filed-down horns, trenchcoat, tail, the Samaritan revolver and the stone Right Hand\n"
                         "of Doom. Unhurried (Lazy) but hits like a landslide.")


def right_hand_of_doom():
    s = S()
    _doom_hand(s, (0, 0, -1.6), 1.6)
    s.box((0, 5.6, 0.4), (3.4, 3.4, 3.4), "D")
    names = ["right hand of doom", "hellboy's right hand", "hellboy's stone hand", "the right hand of doom",
             "hand of doom"]
    r = R("Right Hand of Doom", "Gauntlet", ["Blunt", "Heavy"], 0.9, 60.0, (0.45, 0.35, 0.3), elem="Earth",
          source="Hellboy")
    return names, r, s, "Hellboy's Right Hand of Doom: a stone gauntlet. Blunt + Heavy, Earth (craters, big terrain carve)."


# ------------------------------------------------------------------------- The Incredibles

def mr_incredible():
    s = S()
    b = hero(s, bulk=1.4, tall=-0.6, suit="A", legs="A", arms="A", boots="D", gloves="D", belt="D", buckle="C",
             arm_spread=14)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.ell((0, (yh + ys) / 2 + 1.0, -0.2), (tw * 1.02, b["th"] * 1.1, td * 1.2), "A")          # barrel chest
    s.cyl((0, ys - 2.4, -td / 2 - 1.0), (3.2, 0.5, 3.2), "C", (90, 0, 0))                     # the i emblem
    s.cyl((0, ys - 2.4, -td / 2 - 1.1), (2.4, 0.5, 2.4), "D", (90, 0, 0))
    s.box((0, ys - 2.6, -td / 2 - 1.4), (0.5, 1.4, 0.3), "C")
    s.box((0, ys - 1.6, -td / 2 - 1.4), (0.5, 0.5, 0.3), "C")
    head(s, b, hair="C", style="slick", mask="D", w=3.8, h=4.4)
    names = ["mr. incredible", "mr incredible", "bob parr", "mister incredible", "mr incredible the incredibles"]
    r = R("Mr. Incredible", "Biped", ["Blunt", "Heavy", "Charge", "Grab"], 2.2, 250.0, INCRED_RED, temp="Protective",
          inten=0.8, source="The Incredibles")
    return names, r, s, "Mr. Incredible: barrel chest, the 'i' emblem, domino mask. Strongman: grabs, charges, throws."


def omnidroid():
    s = S()
    s.ell((0, 14.0, 0), (14.0, 12.0, 14.0), "P")                                                # the sphere
    s.cyl((0, 14.0, 0), (14.4, 1.0, 14.4), "D")                                                  # equator
    s.ell((0, 15.0, -6.6), (4.0, 3.0, 1.6), "C")                                                  # sensor eye
    s.ell((0, 15.0, -7.2), (1.6, 1.2, 0.6), "D")
    for i in range(6):                                                                            # claw legs
        a = math.radians(i * 60 + 30)
        dx, dz = math.cos(a), math.sin(a)
        pid = 1 + (i % 4) if i < 4 else 8 + (i - 4)
        role = ("LegFrontLeft", "LegFrontRight", "LegBackLeft", "LegBackRight")[i % 4] if i < 4 else "Other"
        s.on(pid)
        hip = (dx * 5.4, 11.0, dz * 5.4)
        knee = (dx * 12.0, 14.0, dz * 12.0)
        foot = (dx * 15.0, 0.6, dz * 15.0)
        s.seg(hip, knee, 1.8, 1.8, "D")
        s.seg(knee, foot, 1.4, 1.4, "A")
        s.cone(foot, (1.8, 2.4, 1.8), "C", (180, 0, 0))
        s.part(pid, 0, hip, role)
    s.on(0)
    names = ["omnidroid", "the omnidroid", "omnidroid 9000", "omnidroid v.10", "syndrome's robot", "omnidroid robot"]
    r = R("Omnidroid", "Arachnid", ["Grab", "Heavy", "Beam", "Charge"], 9.0, 20000.0, (0.3, 0.3, 0.35), temp="Aggressive",
          inten=0.9, source="The Incredibles")
    return names, r, s, "Syndrome's Omnidroid: a black sphere on six clawed legs. Grabs, rolls over things, beams. It learns."


def ice_path():
    s = S()
    s.box((0, 4.4, -10.0), (6.0, 1.2, 28.0), "C", (18, 0, 0))                                    # the ice slide ramp
    for i in range(6):
        z = 2.0 - i * 5.0
        h = max(0.8, 4.4 + (z + 10.0) * -0.325)
        s.box((0, h / 2, z), (4.6, h, 1.6), "A")                                                 # frozen supports
    for sx in (-1, 1):
        s.box((sx * 3.2, 5.0, -10.0), (0.6, 1.4, 28.0), "P", (18, 0, 0))                          # frosted rails
    names = ["frozone's ice path", "frozone ice path", "ice path", "ice slide", "frozone's ice slide", "frozone"]
    r = R("Frozone's Ice Path", "Ramp", ["Launch", "Fast"], 7.0, 400.0, (0.6, 0.85, 1.0), elem="Ice",
          source="The Incredibles")
    return names, r, s, "Frozone's ice path: a slick Ice ramp that launches whoever surfs it - build your own movement tech."


# ---------------------------------------------------------------------------------- Megamind

def megamind():
    s = S()
    b = hero(s, bulk=0.9, suit="D", legs="D", arms="D", boots="D", gloves="D", belt="C", buckle="C")
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.on(7)                                                                                        # spiked collar cape
    s.box((0, ys + 1.6, td / 2 + 1.6), (8.4, 4.4, 0.6), "D", (-15, 0, 0))
    for x in (-3.4, -1.7, 0.0, 1.7, 3.4):
        s.cone((x, ys + 4.4, td / 2 + 2.2), (1.0, 2.4, 0.6), "D", (-15, 0, 0))
    s.box((0, ys - 4.0, td / 2 + 1.2), (8.0, 11.0, 0.6), "D", (-6, 0, 0))
    s.part(7, 0, (0, ys, td / 2), "Cloth")
    s.on(5)
    hy = b["y_neck"] + 4.2
    s.ell((0, hy + 0.6, 0.4), (9.4, 8.6, 9.0), "P")                                               # the HUGE blue head
    s.box((0, hy - 2.8, -1.8), (3.4, 2.0, 3.2), "P")                                               # narrow chin
    s.box((0, hy - 3.6, -3.2), (1.2, 1.0, 0.6), "D")                                               # goatee
    for sx in (-1, 1):
        s.ell((sx * 1.4, hy - 0.8, -3.6), (2.0, 1.6, 1.0), "C")                                    # big green eyes
    s.box((0, hy + 0.4, -3.9), (3.6, 0.5, 0.5), "D")                                               # brow
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(4)
    hx, hy2, hz = b["hands"][1]
    s.box((hx, hy2, hz - 1.4), (1.6, 1.6, 3.4), "C")                                               # the dehy gun
    s.on(0)
    names = ["megamind", "megamind villain", "megamind the villain", "master of all villainy"]
    r = R("Megamind", "Biped", ["Shoots", "Shrink"], 1.8, 60.0, MEGA_BLUE, elem="Psychic", temp="Fearful", inten=0.7,
          source="Megamind")
    return names, r, s, "Megamind: huge blue head, spiked-collar cape. Keeps his distance and zaps (Shrink = dehydrated)."


def dehy_gun():
    s = S()
    s.box((0, -1.4, 0.6), (1.2, 3.2, 1.6), "D", (15, 0, 0))                                        # grip
    s.ell((0, 0.8, -1.6), (4.0, 4.0, 7.0), "P")                                                    # bulbous body
    for i in range(3):
        s.cyl((0, 0.8, -4.6 - i * 1.3), (3.4 - i * 0.8, 0.5, 3.4 - i * 0.8), "C", ALONG_Z)          # emitter rings
    s.cz((0, 0.8, -8.6), 0.8, 2.6, "C")
    s.ell((0, 3.0, -1.0), (1.8, 1.8, 1.8), "A")                                                    # blue dehydrator tank
    names = ["dehydration gun", "dehy gun", "megamind's dehydration gun", "de-gun", "megamind dehy gun",
             "dehydrator ray"]
    r = R("Dehydration Gun", "Gun", ["Shrink", "Shoots"], 0.8, 3.0, MEGA_BLUE, elem="Water", source="Megamind")
    return names, r, s, "Megamind's dehydration gun: whoever it hits shrinks into a tiny cube-ish self (Shrink)."


def P(names, r, note):
    return lambda: (names, r, None, note)


PROC = [
    # Power Rangers
    P(["dinozords", "the dinozords", "power rangers zords", "mighty morphin zords", "zords"],
      R("Dinozords", "Quadruped", ["Transforms", "Swarm", "Charge", "Stomp"], 5.0, 3000.0, PR_RED, count=5,
        feat=["Armor", "Cockpit"], source="Power Rangers"),
      "All five Dinozords at once (count 5, each Transforms): then say 'combine' - five zords make one Megazord."),
    P(["mastodon dinozord", "mastodon zord", "black dinozord"],
      R("Mastodon Dinozord", "Quadruped", ["Transforms", "Charge", "Breath"], 6.0, 5000.0, (0.2, 0.2, 0.22), elem="Ice",
        feat=["Armor"], source="Power Rangers"), "The black Mastodon Dinozord: freezing trunk blast."),
    P(["triceratops dinozord", "triceratops zord", "blue dinozord"],
      R("Triceratops Dinozord", "Quadruped", ["Transforms", "Charge", "Grab"], 6.0, 5000.0, (0.2, 0.4, 0.9),
        feat=["Treads", "Armor"], source="Power Rangers"), "The blue Triceratops Dinozord: charging horns, grappling lines."),
    P(["sabertooth tiger dinozord", "sabertooth tiger zord", "yellow dinozord", "saber-toothed tiger zord"],
      R("Sabertooth Tiger Dinozord", "Quadruped", ["Transforms", "Fast", "Shoots"], 5.0, 3000.0, (1.0, 0.85, 0.1),
        feat=["Armor"], source="Power Rangers"), "The yellow Sabertooth Tiger Dinozord: fast, tail cannon."),
    P(["pterodactyl dinozord", "pterodactyl zord", "pink dinozord"],
      R("Pterodactyl Dinozord", "Bird", ["Transforms", "Flies", "Beam"], 6.0, 2000.0, (1.0, 0.45, 0.7),
        feat=["Armor"], source="Power Rangers"), "The pink Pterodactyl Dinozord: flying beam strafes."),
    P(["dragonzord", "the dragonzord", "green ranger's dragonzord"],
      R("Dragonzord", "Kaiju", ["Transforms", "Stomp", "Shoots", "ArticulatedTail"], 14.0, 60000.0, (0.2, 0.7, 0.3),
        source="Power Rangers"), "The Dragonzord: summoned by the Dragon Dagger's song; missile fingers, drill tail."),
    P(["dragon dagger", "green ranger's dagger", "dragon flute"],
      R("Dragon Dagger", "Sword", ["Blade", "Repel"], 0.7, 1.0, (0.2, 0.7, 0.3), source="Power Rangers"),
      "The Green Ranger's Dragon Dagger (also a flute): stab, and the song shoves."),
    P(["green ranger", "tommy oliver", "green power ranger"],
      R("Green Ranger", "Biped", ["Blade", "Fast", "Guard"], 1.8, 75.0, (0.2, 0.7, 0.3), temp="Aggressive", inten=0.8,
        source="Power Rangers"), "The Green Ranger: gold shield-plate and Dragon Dagger."),
    P(["power rangers", "mighty morphin power rangers", "the power rangers", "mmpr"],
      R("Power Rangers", "Biped", ["Blade", "Fast", "Swarm", "Bounce"], 1.8, 75.0, PR_RED, count=5, temp="Aggressive",
        inten=0.8, source="Power Rangers"), "Five Power Rangers, one squad."),
    P(["putty patrollers", "putties", "putty patrol"],
      R("Putty Patrollers", "Biped", ["Swarm", "Blunt"], 1.8, 50.0, (0.6, 0.6, 0.65), count=6, temp="Playful", inten=0.6,
        source="Power Rangers"), "Rita's Putty Patrollers: six clay goons, easy to shatter."),
    P(["goldar", "goldar power rangers"],
      R("Goldar", "Biped", ["Flies", "Blade", "Charge"], 2.4, 150.0, (0.95, 0.75, 0.15), temp="Aggressive", inten=0.9,
        source="Power Rangers"), "Goldar: winged golden gryphon-knight."),
    P(["rita's wand", "rita repulsa", "rita repulsa's wand", "make my monster grow"],
      R("Rita's Wand", "Staff", ["Grow", "DropFromSky"], 1.8, 3.0, (0.7, 0.6, 0.2), elem="Psychic",
        source="Power Rangers"), "Rita's wand, thrown to earth: 'make my monster grow!' (Grow)."),
    P(["lord zedd", "zedd", "lord zedd's staff"],
      R("Lord Zedd's Staff", "Staff", ["Beam", "Grow"], 2.0, 4.0, (0.8, 0.1, 0.1), elem="Electric",
        source="Power Rangers"), "Lord Zedd's Z-staff: lightning that grows monsters."),
    P(["zordon", "zordon's tube"],
      R("Zordon", "Pillar", ["Guard", "Beam"], 4.0, 500.0, (0.4, 0.7, 1.0), source="Power Rangers"),
      "Zordon in his time warp tube (a beaming pillar)."),
    P(["alpha 5", "alpha five", "ay yi yi"],
      R("Alpha 5", "Biped", ["Fast"], 1.5, 40.0, (1.0, 0.85, 0.2), temp="Fearful", inten=1.0, source="Power Rangers"),
      "Alpha 5: panicking robot. Ay-yi-yi!"),
    # Ben 10
    P(["xlr8", "xlr8 ben 10", "kineceleran"],
      R("XLR8", "Biped", ["Fast", "Charge", "Blade"], 1.6, 50.0, (0.2, 0.5, 0.9), elem="Wind", temp="Aggressive",
        inten=0.8, source="Ben 10"), "XLR8: the Kineceleran speedster."),
    P(["diamondhead", "diamond head", "petrosapien"],
      R("Diamondhead", "Biped", ["Blade", "Shoots", "Guard"], 2.2, 300.0, (0.3, 1.0, 0.6), temp="Protective",
        inten=0.7, payload=R("crystal shard", "Spear", ["Pierce", "Fast"], 0.8, 3.0), source="Ben 10"),
      "Diamondhead: crystal body, fires crystal shards."),
    P(["upgrade ben 10", "galvanic mechamorph"],
      R("Upgrade", "Blob", ["Sticky", "Piloted", "Shoots"], 1.8, 80.0, (0.2, 0.2, 0.2), elem="Electric",
        source="Ben 10"), "Upgrade: the living tech-goo that merges with machines (wear it like a suit)."),
    P(["grey matter ben 10", "gray matter ben 10", "galvan"],
      R("Grey Matter", "Biped", ["Fast", "Shrink"], 0.4, 3.0, (0.6, 0.6, 0.6), temp="Sneaky", inten=0.9,
        source="Ben 10"), "Grey Matter: five inches of genius."),
    P(["stinkfly", "stink fly", "lepidopterran"],
      R("Stinkfly", "Insect", ["Flies", "Breath", "Sticky"], 1.6, 40.0, (0.5, 0.8, 0.2), elem="Poison",
        temp="Aggressive", inten=0.7, source="Ben 10"), "Stinkfly: flying goo-spitter."),
    P(["wildvine", "wild vine", "florauna"],
      R("Wildvine", "Serpent", ["Constrict", "Grab", "Pull"], 2.4, 80.0, (0.3, 0.7, 0.2), elem="Earth", temp="Aggressive",
        inten=0.7, source="Ben 10"), "Wildvine: stretching vine-alien."),
    P(["ripjaws", "rip jaws", "piscciss volann"],
      R("Ripjaws", "Fish", ["Grab", "Charge", "Fast"], 2.0, 90.0, (0.4, 0.6, 0.8), elem="Water", temp="Aggressive",
        inten=0.9, source="Ben 10"), "Ripjaws: the angler-jawed swimmer."),
    P(["ghostfreak", "ghost freak", "ectonurite"],
      R("Ghostfreak", "Biped", ["Flies", "Grab"], 1.9, 50.0, (0.85, 0.85, 0.9), elem="Shadow", temp="Sneaky", inten=1.0,
        source="Ben 10"), "Ghostfreak: flanks from the shadows (never actually invisible here)."),
    P(["way big", "waybig", "to'kustar"],
      R("Way Big", "Kaiju", ["Beam", "Stomp", "Blunt"], 20.0, 150000.0, (0.95, 0.95, 0.95), elem="Atomic",
        temp="Protective", inten=0.7, source="Ben 10"), "Way Big: the To'kustar - a kaiju with a cosmic ray."),
    P(["upchuck", "gourmand"],
      R("Upchuck", "Biped", ["Shoots", "Explodes"], 1.2, 30.0, (0.3, 0.8, 0.3), temp="Playful", inten=0.8,
        source="Ben 10"), "Upchuck: eats things, spits them back as bombs."),
    P(["vilgax", "vilgax the conqueror"],
      R("Vilgax", "Biped", ["Blunt", "Heavy", "Grab", "Beam"], 2.8, 400.0, (0.3, 0.6, 0.3), temp="Aggressive", inten=1.0,
        source="Ben 10"), "Vilgax: squid-faced warlord."),
    P(["rust bucket", "the rust bucket", "grandpa max's rv"],
      R("Rust Bucket", "Vehicle", ["Piloted", "Heavy", "Shoots"], 6.0, 6000.0, (0.9, 0.9, 0.85), feat=["Wheels", "Windows"],
        source="Ben 10"), "Grandpa Max's alien-tech RV."),
    # Powerpuff Girls
    P(["mojo jojo", "mojo jojo's robot", "mojo robot"],
      R("Mojo Jojo's Robot", "Kaiju", ["Piloted", "Stomp", "Beam"], 12.0, 20000.0, (0.4, 0.2, 0.6), feat=["Cockpit"],
        source="The Powerpuff Girls"), "Mojo Jojo's giant robot: climb in and stomp Townsville."),
    P(["him powerpuff", "the villain him"],
      R("HIM", "Biped", ["Constrict", "Shoots"], 2.0, 50.0, (0.9, 0.1, 0.2), elem="Shadow", temp="Chaotic", inten=0.9,
        source="The Powerpuff Girls"), "HIM: claws and creepy falsetto."),
    P(["professor utonium", "utonium's lab"],
      R("Professor Utonium", "Biped", ["Shoots"], 1.8, 70.0, (0.9, 0.9, 0.95), temp="Fearful", inten=0.5,
        source="The Powerpuff Girls"), "Professor Utonium."),
    P(["fuzzy lumpkins", "fuzzy lumpkins' gun"],
      R("Fuzzy Lumpkins", "Biped", ["Blunt", "Heavy", "Charge"], 2.0, 150.0, (1.0, 0.5, 0.7), temp="Aggressive", inten=0.8,
        source="The Powerpuff Girls"), "Fuzzy Lumpkins: get off my property!"),
    # Hellboy & BPRD
    P(["samaritan revolver", "hellboy's samaritan", "hellboy's gun"],
      R("The Samaritan", "Gun", ["Shoots", "Heavy"], 0.8, 4.0, (0.4, 0.4, 0.45), source="Hellboy"),
      "Hellboy's Samaritan: a massive hand cannon."),
    P(["abe sapien", "abraham sapien"],
      R("Abe Sapien", "Biped", ["Fast", "Grab"], 1.9, 70.0, (0.3, 0.6, 0.7), elem="Water", temp="Protective", inten=0.6,
        source="Hellboy"), "Abe Sapien: amphibious BPRD agent."),
    P(["liz sherman", "elizabeth sherman"],
      R("Liz Sherman", "Biped", ["Breath", "Explodes"], 1.7, 60.0, (1.0, 0.45, 0.1), elem="Fire", temp="Fearful",
        inten=0.6, source="Hellboy"), "Liz Sherman: pyrokinetic blasts."),
    P(["kroenen", "karl ruprecht kroenen"],
      R("Kroenen", "Biped", ["Blade", "Spin", "Fast"], 1.9, 70.0, (0.3, 0.3, 0.3), temp="Sneaky", inten=0.9,
        source="Hellboy"), "Kroenen: clockwork assassin with spinning tonfa blades."),
    P(["sammael", "sammael the desolate one", "hound of resurrection"],
      R("Sammael", "Quadruped", ["Pull", "Grab", "Breath"], 3.0, 500.0, (0.6, 0.3, 0.3), elem="Fire", count=2,
        temp="Aggressive", inten=0.9, source="Hellboy"), "Sammael: tongue-lashing hellhounds (two of them - they multiply)."),
    # Incredibles
    P(["elastigirl", "helen parr", "mrs. incredible", "mrs incredible"],
      R("Elastigirl", "Biped", ["Grab", "Pull", "Constrict", "Glide"], 1.8, 60.0, INCRED_RED, temp="Protective",
        inten=0.8, source="The Incredibles"), "Elastigirl: stretch to grab, pull and wrap; turn into a parachute."),
    P(["dash parr", "dash incredibles"],
      R("Dash", "Biped", ["Fast", "Charge"], 1.2, 30.0, INCRED_RED, temp="Playful", inten=1.0, source="The Incredibles"),
      "Dash: the fastest kid alive."),
    P(["violet parr", "violet's force field", "violet incredibles"],
      R("Violet's Force Field", "Dome", ["Barrier", "Guard", "Bounce"], 3.0, 50.0, (0.6, 0.3, 0.9),
        source="The Incredibles"), "Violet's force field bubble (the invisibility half is never-list)."),
    P(["jack-jack", "jack jack", "jack-jack parr"],
      R("Jack-Jack", "Biped", ["Explodes", "Fast", "Bounce"], 0.8, 10.0, INCRED_RED, elem="Fire", temp="Chaotic",
        inten=1.0, source="The Incredibles"), "Jack-Jack: 17 powers, zero self-control. Chaotic."),
    P(["frozone character", "lucius best", "where's my super suit"],
      R("Frozone", "Biped", ["Shoots", "Fast"], 1.9, 75.0, (0.6, 0.85, 1.0), elem="Ice", temp="Protective", inten=0.7,
        source="The Incredibles"), "Frozone: ice blasts (his ramp is 'frozone's ice path')."),
    P(["syndrome", "buddy pine", "incrediboy"],
      R("Syndrome", "Biped", ["Grab", "Thrust", "Shoots"], 1.8, 70.0, (0.9, 0.9, 0.95), elem="Psychic", temp="Fearful",
        inten=0.8, feat=["Jet"], source="The Incredibles"), "Syndrome: rocket boots and a zero-point hold (a short Grab)."),
    P(["incredibile", "the incredibile", "incredibles car"],
      R("Incredibile", "Vehicle", ["Piloted", "Fast", "Charge"], 5.0, 2000.0, INCRED_RED, feat=["Wheels", "Jet"],
        source="The Incredibles"), "The Incredibile: Mr. Incredible's car."),
    P(["the underminer", "underminer", "underminer's drill"],
      R("The Underminer's Drill", "Vehicle", ["Charge", "Spin", "Heavy"], 8.0, 20000.0, (0.4, 0.35, 0.3), elem="Earth",
        feat=["Drill", "Treads"], source="The Incredibles"), "The Underminer's tunneller: drills through terrain."),
    P(["edna mode", "no capes", "edna"],
      R("Edna Mode", "Biped", ["Fast"], 1.0, 20.0, (0.2, 0.2, 0.2), temp="Aggressive", inten=1.0, source="The Incredibles"),
      "Edna Mode. NO CAPES."),
    # Megamind
    P(["brainbots", "megamind's brainbots", "brain bots", "brainbot"],
      R("Brainbots", "Insect", ["Flies", "Swarm", "Grab"], 0.8, 5.0, MEGA_BLUE, count=5, feat=["Antenna"],
        temp="Playful", inten=0.8, source="Megamind"), "Megamind's brainbots: five flying brains in jars that grab."),
    P(["metro man", "metroman"],
      R("Metro Man", "Biped", ["Flies", "Blunt", "Fast"], 2.0, 110.0, (0.95, 0.95, 1.0), temp="Protective", inten=0.7,
        source="Megamind"), "Metro Man: the defender of Metro City."),
    P(["tighten", "titan megamind", "hal stewart"],
      R("Tighten", "Biped", ["Flies", "Beam", "Charge"], 2.0, 110.0, (0.9, 0.2, 0.2), temp="Chaotic", inten=0.9,
        source="Megamind"), "Tighten: Megamind's accidental super-villain."),
    P(["minion megamind", "megamind's minion", "fish in a robot suit"],
      R("Minion", "Biped", ["Piloted", "Blunt", "Heavy"], 2.4, 400.0, (0.4, 0.4, 0.45), feat=["Armor"],
        source="Megamind"), "Minion's gorilla robot suit: wear it (Piloted)."),
    # Static Shock, Kick-Ass, Umbrella Academy
    P(["static shock", "virgil hawkins", "static shock hero"],
      R("Static Shock", "Biped", ["Shoots", "Pull"], 1.7, 60.0, (0.2, 0.4, 0.9), elem="Electric", temp="Playful",
        inten=0.8, source="Static Shock"), "Static: electromagnetic bolts, magnetic Pull."),
    P(["static's disc", "static shock disc", "static's manhole cover", "flying manhole cover"],
      R("Static's Disc", "Vehicle", ["Piloted", "Flies", "Fast"], 1.2, 30.0, (0.6, 0.6, 0.65), elem="Electric",
        source="Static Shock"), "Static's electromagnetically-surfed manhole cover: ride it."),
    P(["kick-ass", "kick ass", "dave lizewski", "kick-ass batons"],
      R("Kick-Ass' Batons", "Sword", ["Blunt"], 0.7, 1.0, (0.2, 0.6, 0.3), count=2, source="Kick-Ass"),
      "Kick-Ass's twin batons."),
    P(["hit-girl", "hit girl", "mindy macready"],
      R("Hit-Girl", "Biped", ["Blade", "Fast", "Shoots"], 1.3, 40.0, (0.6, 0.2, 0.7), temp="Aggressive", inten=1.0,
        source="Kick-Ass"), "Hit-Girl."),
    P(["big daddy kick-ass", "damon macready"],
      R("Big Daddy", "Biped", ["Shoots", "Guard"], 1.9, 90.0, (0.3, 0.25, 0.2), temp="Protective", inten=0.8,
        source="Kick-Ass"), "Big Daddy."),
    P(["spaceboy", "space boy", "luther hargreeves"],
      R("Spaceboy", "Biped", ["Blunt", "Heavy", "Grab"], 2.4, 300.0, (0.3, 0.3, 0.35), temp="Protective", inten=0.7,
        source="The Umbrella Academy"), "Spaceboy (Luther): ape-strong."),
    P(["diego hargreeves", "the kraken umbrella academy", "diego's knives"],
      R("Diego's Knives", "Spear", ["Homing", "Pierce", "Fast"], 0.6, 0.5, (0.3, 0.3, 0.3), count=3,
        source="The Umbrella Academy"), "Diego's curving throwing knives (Homing)."),
    P(["number five", "five hargreeves", "the boy umbrella academy"],
      R("Number Five", "Biped", ["Fast", "Portal"], 1.4, 40.0, (0.3, 0.3, 0.5), temp="Sneaky", inten=0.9,
        source="The Umbrella Academy"), "Number Five: short spatial jumps (Portal)."),
    P(["white violin", "vanya hargreeves", "viktor hargreeves"],
      R("White Violin", "Staff", ["Repel", "Beam"], 1.0, 1.0, (0.95, 0.95, 1.0), elem="Wind",
        source="The Umbrella Academy"), "The White Violin: sound shockwaves."),
    P(["the umbrella academy", "umbrella academy"],
      R("The Umbrella Academy", "Biped", ["Swarm", "Blunt", "Fast"], 1.8, 70.0, (0.3, 0.3, 0.35), count=5,
        temp="Chaotic", inten=0.6, source="The Umbrella Academy"), "The Hargreeves siblings: five, bickering (Chaotic)."),
]


def build_all():
    return [tyrannozord, red_ranger, four_arms, heatblast, cannonbolt, powerpuff, hellboy, right_hand_of_doom,
            mr_incredible, omnidroid, ice_path, megamind, dehy_gun] + PROC
