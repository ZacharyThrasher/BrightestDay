"""TMNT villains: Shredder, Krang, Bebop & Rocksteady, Baxter, Foot Clan, Mousers, mutants, aliens."""
import math

from tlib import S, R, KRANG_PINK, FOOT_RED, SHRED_SILVER, TIP_NZ, ALONG_Z, ALONG_X
from body import humanoid, turtle, tail, katana_in, blaster_in, beast_head


# ------------------------------------------------------------------------------------ Shredder

def _shredder_body(s, bulk=1.0, spikes=0):
    k = bulk
    b = humanoid(s, leg_h=8.6 * k, leg_w=2.4 * k, leg_d=2.6 * k, hip_x=1.7 * k, foot=(2.6 * k, 1.4, 3.6 * k),
                 torso=(6.8 * k, 8.4 * k, 4.2 * k), torso_t="D", arm_w=2.1 * k, arm_len=8.4 * k, arm_x=4.5 * k,
                 hand=2.2 * k, leg_t="D", arm_t="D")
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, ys - 3.0 * k, -td / 2 - 0.2), (5.6 * k, 5.2 * k, 0.8), "A")        # chest plate
    for dy in (-1.4, 0.2, 1.8):
        s.box((0, ys - 3.0 * k + dy * k, -td / 2 - 0.7), (5.0 * k, 0.4, 0.4), "D")
    s.box((0, yh + 0.6, 0), (7.0 * k, 1.2, td + 0.6), "A")                        # armoured belt
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        s.box((sx * 1.7 * k, 3.6 * k, -0.2), (2.8 * k, 4.4 * k, 3.0 * k), "A")    # greaves
        s.cone((sx * 1.7 * k, 6.0 * k, -1.6 * k), (1.2, 1.8, 1.2), "C", (-60, 0, 0))   # knee spike
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        x = sx * 4.5 * k
        s.box((x + sx * 0.4, ys, 0), (3.6 * k, 2.4 * k, 4.0 * k), "A")              # pauldron
        for dz in (-1.2, 0.2, 1.6):
            s.cone((x + sx * 0.8, ys + 1.8 * k, dz * k), (1.0 * k, 2.6 * k, 1.0 * k), "C", (0, 0, -sx * 20))
        s.box((x, ys - 6.4 * k, -0.2), (2.6 * k, 3.6 * k, 2.8 * k), "A")           # gauntlet
        for dx in (-0.7, 0.0, 0.7):
            s.seg((x + dx * k, ys - 5.6 * k, -1.2 * k), (x + dx * k, ys - 3.2 * k, -5.0 * k), 0.4, 0.9, "C")   # blades
    for i in range(spikes):
        a = (i + 0.5) / spikes * math.pi
        s.cone((math.cos(a) * 3.0 * k, ys - 1.0 * k, 2.4 * k + math.sin(a) * 0.5), (1.2 * k, 3.4 * k, 1.2 * k), "C", (40, 0, (i - spikes / 2) * 18))
    # helmet (part 5)
    s.on(5)
    hy = b["y_neck"] + 2.3 * k
    s.box((0, hy, 0), (4.4 * k, 4.8 * k, 4.8 * k), "A")
    s.box((0, hy - 0.4 * k, -2.5 * k), (3.8 * k, 3.4 * k, 0.6), "D")              # face mask
    for sx in (-1, 1):
        s.box((sx * 0.9 * k, hy + 0.4 * k, -2.85 * k), (1.2 * k, 0.5, 0.3), "C")   # eye slits
    for dy in (-1.0, -1.6):
        s.box((0, hy + dy * k, -2.85 * k), (2.6 * k, 0.3, 0.3), "A")              # mouth grille
    s.box((0, hy + 2.8 * k, 0.2), (0.6, 1.6 * k, 5.2 * k), "A")                   # crest fin
    s.cone((0, hy + 2.6 * k, -2.4 * k), (0.8, 2.4 * k, 1.2), "C", (-30, 0, 0))
    for sx in (-1, 1):
        s.cone((sx * 2.4 * k, hy + 1.8 * k, 0), (0.9, 2.8 * k, 0.9), "C", (0, 0, -sx * 30))   # side blades
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(7)
    s.box((0, (yh + ys) / 2 - 1.5, td / 2 + 0.6), (6.8 * k, ys - yh + 6.0, 0.7), "P")   # cape
    s.part(7, 0, (0, ys, td / 2 + 0.6), "Cloth")
    s.on(0)
    return b


def shredder():
    s = S()
    _shredder_body(s)
    names = ["shredder", "the shredder", "oroku saki", "shredder tmnt", "oroku saki shredder", "master shredder",
             "shredder foot clan leader"]
    r = R("Shredder", "Biped", ["Blade", "Fast", "Guard"], 1.9, 110.0, SHRED_SILVER, "shredder", temp="Aggressive",
          inten=0.95, feat=["Armor", "Spikes"])
    return names, r, s, "Oroku Saki, the Shredder: bladed armour, helmet, cape, forearm blades. Aggressive."


def super_shredder():
    s = S(k=2.0)
    _shredder_body(s, bulk=1.25, spikes=6)
    names = ["super shredder", "mutant shredder", "super shredder tmnt", "shredder mutant"]
    r = R("Super Shredder", "Kaiju", ["Blade", "Stomp", "Heavy", "Charge"], 12.0, 20000.0, SHRED_SILVER,
          "super_shredder", elem="Poison", temp="Aggressive", inten=1.0, feat=["Armor", "Spikes"])
    return names, r, s, "Super Shredder (Secret of the Ooze): mutated with the last of the ooze. Kaiju-sized, spiked."


def shredder_gauntlet():
    s = S(k=2.0)
    s.box((0, 0, 3.0), (3.2, 3.2, 7.0), "A")                                    # vambrace
    s.box((0, 1.6, 3.0), (3.4, 0.6, 6.4), "D")
    s.box((0, 0, -1.0), (3.4, 3.0, 2.6), "D")                                   # fist
    for dx in (-1.0, 0.0, 1.0):
        s.seg((dx, 1.0, 1.0), (dx, 1.4, -8.0), 0.4, 1.2, "C")                   # the three blades
        s.cone((dx, 1.45, -8.6), (0.4, 1.4, 1.2), "C", TIP_NZ)
    for dz in (1.0, 4.5):
        s.cone((1.9, 0.6, dz), (0.8, 1.6, 0.8), "C", (0, 0, -90))
    names = ["shredder's gauntlet", "shredder's gauntlets", "shredder gauntlet", "shredder's claws",
             "shredder claws", "shredder's blades", "shredder's tekko kagi", "tekko kagi"]
    r = R("Shredder's Gauntlet", "Gauntlet", ["Blade", "Pierce"], 0.7, 4.0, SHRED_SILVER, "shredder_s_gauntlet")
    return names, r, s, "Shredder's bladed gauntlet (tekko-kagi): punch combo that cuts (Blade + Pierce)."


def shredder_helmet():
    s = S(k=2.0)
    s.box((0, 0, 0), (6.0, 7.0, 2.0), "A")                                      # front of the helm
    s.box((0, -0.6, -1.1), (5.0, 4.6, 0.6), "D")                                # face mask
    for sx in (-1, 1):
        s.box((sx * 1.2, 0.6, -1.5), (1.6, 0.6, 0.4), "C")
        s.cone((sx * 3.4, 2.4, 0), (1.0, 3.6, 1.0), "C", (0, 0, -sx * 35))      # side blades
    for dy in (-1.6, -2.4):
        s.box((0, dy, -1.5), (3.4, 0.35, 0.4), "A")
    s.box((0, 4.6, 0.4), (0.8, 2.2, 3.0), "A")                                  # crest
    s.cone((0, 4.0, -1.2), (1.0, 3.0, 1.4), "C", (-30, 0, 0))
    s.box((0, 1.0, 1.6), (6.4, 6.0, 1.4), "D")                                  # back shell
    names = ["shredder's helmet", "shredder helmet", "shredder's mask", "shredder mask", "shredder's kabuto"]
    r = R("Shredder's Helmet", "Shield", ["Guard", "Blade", "Returning"], 0.9, 6.0, SHRED_SILVER, "shredder_s_helmet",
          feat=["Spikes"])
    return names, r, s, "Shredder's helmet as a bladed bashing shield you can hurl back and forth."


# ---------------------------------------------------------------------------------------- Krang

def _brain(s, w=8.0, h=6.6, d=8.0, y=6.0, one_eye=False, face=True):
    s.ell((0, y, 0), (w, h, d), "A")
    for i in range(3):                                                           # gyri
        s.ell((0, y + h * 0.18 - i * 0.4, -d * 0.1 + i * 1.8), (w * 1.02, 0.6, d * 0.3), "D", (10, 0, 0))
    s.box((0, y + h * 0.3, 0), (0.5, h * 0.5, d * 0.9), "D")                     # fissure
    if face:
        if one_eye:
            s.ell((0, y - h * 0.05, -d * 0.46), (w * 0.34, w * 0.34, 1.2), "C")
            s.ell((0, y - h * 0.05, -d * 0.52), (w * 0.14, w * 0.14, 0.6), "D")
        else:
            for sx in (-1, 1):
                s.ell((sx * w * 0.16, y - h * 0.02, -d * 0.45), (1.6, 2.0, 1.0), "C")
                s.box((sx * w * 0.13, y - h * 0.02, -d * 0.52), (0.6, 0.8, 0.3), "D")
            s.box((0, y + h * 0.16, -d * 0.44), (w * 0.4, 0.5, 0.6), "D")        # angry brow


def krang():
    s = S(k=1.8)
    y = 6.2
    _brain(s, y=y)
    s.on(6)
    s.box((0, y - 2.2, -3.4), (3.0, 1.0, 1.4), "D")                              # yelling mouth
    s.box((0, y - 2.0, -3.9), (2.2, 0.4, 0.4), "C")
    s.part(6, 0, (0, y - 1.8, -2.4), "Jaw")
    roles = {(-1, -1): "LegFrontLeft", (1, -1): "LegFrontRight", (-1, 1): "LegBackLeft", (1, 1): "LegBackRight"}
    pid = 1
    for (sx, sz), role in roles.items():
        s.on(pid)
        a = (sx * 2.2, y - 2.0, sz * 1.8)
        m = (sx * 3.6, y - 3.8, sz * 3.0)
        e = (sx * 4.4, 0.4, sz * 3.4)
        s.seg(a, m, 1.3, 1.3, "A")
        s.seg(m, e, 1.0, 1.0, "A")
        s.part(pid, 0, a, role)
        pid += 1
    for sx, aid, role in ((-1, 5, "ArmLeft"), (1, 7, "ArmRight")):              # shaking tentacle fists
        s.on(aid)
        a = (sx * 3.4, y - 0.6, -1.0)
        s.seg(a, (sx * 5.4, y + 1.6, -2.2), 1.0, 1.0, "A")
        s.ell((sx * 5.6, y + 2.2, -2.4), (1.4, 1.4, 1.4), "A")
        s.part(aid, 0, a, role)
    s.on(0)
    names = ["krang", "krang tmnt", "krang the brain", "krang brain", "krang from dimension x", "utrom krang",
             "kraang brain"]
    r = R("Krang", "Blob", ["Grab", "Pull"], 0.8, 8.0, KRANG_PINK, "krang", elem="Psychic",
          temp="Sneaky", inten=0.8)
    return names, r, s, ("Krang: the warlord brain from Dimension X, out of his body - scuttles on tentacles, grabs.\n"
                          "Sneaky: goes looking for a body (cast \"krang's android body\" to give him one).")


def krang_android():
    s = S(k=1.4)
    b = humanoid(s, leg_h=10.0, leg_w=3.4, leg_d=3.6, hip_x=2.6, foot=(3.8, 1.6, 5.0), torso=(10.0, 11.0, 6.4),
                 arm_w=3.4, arm_len=10.4, arm_x=6.8, hand=3.4)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, yh + 0.6, 0), (10.4, 3.4, 6.8), "D")                               # the famous boxer shorts
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        s.box((sx * 2.6, yh - 1.4, 0), (3.8, 3.4, 4.0), "D")
        s.box((sx * 2.6, 1.6, 0), (4.0, 2.8, 4.2), "D")                          # boots
    s.on(0)
    by = yh + 5.0
    s.box((0, by, -td / 2 - 0.3), (7.4, 6.0, 1.2), "D")                          # belly window frame
    s.box((0, by, -td / 2 - 0.9), (6.0, 4.6, 0.4), "C")                          # glass rim glow
    s.ell((0, by, -td / 2 - 1.2), (5.6, 4.4, 2.4), "A")                          # Krang inside
    for sx in (-1, 1):
        s.ell((sx * 0.9, by - 0.2, -td / 2 - 2.3), (0.9, 1.1, 0.4), "C")
    s.box((0, by + 1.2, -td / 2 - 2.3), (2.4, 0.3, 0.3), "D")
    s.box((0, ys - 1.8, -td / 2 - 0.1), (8.4, 2.8, 0.4), "P")                    # pecs
    s.on(5)
    hy = b["y_neck"] + 3.0
    s.box((0, hy, 0), (5.2, 6.0, 5.2), "P")                                      # square android head
    s.box((0, hy - 1.8, -0.4), (5.6, 2.2, 5.0), "P")                             # lantern jaw
    s.box((0, hy + 3.0, 0.2), (5.4, 0.9, 5.4), "D")                              # crew cut
    for sx in (-1, 1):
        s.box((sx * 1.2, hy + 0.6, -2.7), (1.3, 0.8, 0.4), "C")
    s.box((0, hy - 1.6, -2.9), (2.2, 0.4, 0.4), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    s.seat(0, (0, by, 0))
    names = ["krang's android body", "krang android", "krang's android", "krang android body", "krang's body",
             "krang body", "android body", "krang robot body", "krang's exo-suit"]
    r = R("Krang's Android Body", "Biped", ["Piloted", "Blunt", "Heavy", "Grab"], 2.4, 400.0, KRANG_PINK,
          "krang_s_android_body", feat=["Windows", "Cockpit"])
    return names, r, s, ("Krang's android body: a human-scale power suit (Piloted, <= 2.6 m). You wear it; the big\n"
                          "pink brain rides in the belly window (the cockpit weak point). Heavy grabbing punches.")


# --------------------------------------------------------------------------- Bebop & Rocksteady

def _brute(s, belly=True):
    b = humanoid(s, leg_h=8.0, leg_w=3.2, leg_d=3.4, hip_x=2.4, foot=(3.6, 1.6, 4.6), torso=(9.0, 9.4, 6.0),
                 torso_kind="ell", arm_w=3.2, arm_len=9.4, arm_x=6.0, hand=3.2, leg_t="D")
    if belly:
        s.ell((0, b["y_hip"] + 3.0, -1.6), (7.6, 6.4, 4.8), "P")
    s.box((0, b["y_hip"] + 0.4, 0), (9.2, 1.4, 6.4), "D")
    return b


def bebop():
    s = S(k=1.5)
    b = _brute(s)
    ys, td = b["y_sh"], b["td"]
    s.box((0, ys - 3.2, 0.4), (9.4, 6.2, 6.0), "D")                               # leather vest
    s.box((0, ys - 3.2, -2.7), (3.0, 6.3, 0.4), "P")                              # open front
    x, y, z = b["hands"][1]
    s.on(4)
    blaster_in(s, (x, y, z), length=7.0)
    s.box((x, y + 2.0, 0), (3.6, 1.2, 3.6), "C")                                  # spiked wristband
    s.on(3)
    s.box((-x, y + 2.0, 0), (3.6, 1.2, 3.6), "C")
    s.on(0)
    hy = beast_head(s, b["y_neck"], snout=3.4, snout_w=5.2, head=(7.6, 7.0, 7.0), ears="pointy", eye_t="D",
                    nose_t="C")
    s.on(5)
    s.box((0, hy + 0.9, -2.9), (5.6, 1.1, 0.8), "D")                              # shades
    s.box((0, hy + 0.9, -3.4), (5.0, 0.6, 0.3), "A")
    for sx in (-1, 1):
        s.cone((sx * 2.0, hy - 0.4, -5.2), (0.9, 2.8, 0.9), "C", (-20, 0, sx * 15))   # tusks
        s.cyl((sx * 3.2, hy - 0.6, 0.2), (0.9, 0.3, 0.9), "C", ALONG_X)              # earrings
    for i in range(6):
        s.box((0, hy + 3.2 + (0.6 if 1 <= i <= 4 else 0), -2.0 + i * 1.0), (1.0, 2.6, 1.0), "A")   # mohawk
    s.on(0)
    names = ["bebop", "bebop tmnt", "bebop the warthog", "bebop warthog", "anton zeck", "bebop mutant"]
    r = R("Bebop", "Biped", ["Charge", "Shoots", "Blunt"], 2.0, 180.0, (0.6, 0.2, 0.85), "bebop", temp="Aggressive",
          inten=0.8, payload=R("laser blast", "Ball", ["Fast"], 0.4, 1.0, elem="Fire", seed=12))
    return names, r, s, "Bebop: punk warthog - purple mohawk, shades, tusks, leather vest, blaster. Charges and shoots."


def _rocksteady_shape(k=1.5):
    s = S(k=k)
    b = _brute(s)
    ys, td = b["y_sh"], b["td"]
    s.box((0, ys - 3.2, 0.4), (9.6, 6.4, 6.2), "A")                              # camo vest
    for (x, y) in ((-2.4, ys - 2.0), (2.0, ys - 4.4), (0.4, ys - 1.2)):
        s.box((x, y, -2.9), (1.8, 1.2, 0.4), "D")
    x, y, z = b["hands"][1]
    s.on(4)
    blaster_in(s, (x, y, z), length=9.0)
    s.on(0)
    hy = beast_head(s, b["y_neck"], snout=5.0, snout_w=5.2, head=(7.6, 7.0, 7.4), ears="pointy", eye_t="C")
    s.on(5)
    s.cone((0, hy + 1.2, -8.2), (2.8, 5.6, 2.8), "C", (-25, 0, 0))               # the big horn
    s.cone((0, hy + 2.2, -5.0), (1.6, 2.6, 1.6), "C", (-10, 0, 0))
    s.ell((0, hy + 2.9, 0.4), (8.2, 3.4, 8.2), "A")                               # army helmet
    s.box((0, hy + 1.4, 0.4), (8.6, 0.6, 8.6), "D")
    s.on(0)
    return s


def rocksteady():
    s = _rocksteady_shape()
    names = ["rocksteady", "rocksteady tmnt", "rocksteady the rhino", "rocksteady rhino", "ivan steranko",
             "rocksteady mutant"]
    r = R("Rocksteady", "Biped", ["Charge", "Shoots", "Heavy", "Blunt"], 2.2, 260.0, (0.45, 0.5, 0.3), "rocksteady",
          temp="Aggressive", inten=0.8, payload=R("laser blast", "Ball", ["Fast"], 0.4, 1.0, elem="Fire", seed=12))
    return names, r, s, "Rocksteady: army-helmeted rhino with a big horn and a bigger blaster. Heavy charger."


def bebop_and_rocksteady():
    s = _rocksteady_shape()
    names = ["bebop and rocksteady", "rocksteady and bebop", "bebop & rocksteady", "bebop n rocksteady",
             "bebop and rocksteady tmnt", "shredder's henchmen"]
    r = R("Bebop and Rocksteady", "Biped", ["Charge", "Shoots", "Heavy", "Swarm"], 2.1, 220.0, (0.6, 0.2, 0.85),
          "bebop_and_rocksteady", count=2, temp="Chaotic", inten=0.8,
          payload=R("laser blast", "Ball", ["Fast"], 0.4, 1.0, elem="Fire", seed=12))
    return names, r, s, "The dimwitted duo as a count-2 squad: Chaotic chargers with blasters."


# ---------------------------------------------------------------------------------- Baxter & co

def baxter():
    s = S(k=1.5)
    b = humanoid(s, leg_h=9.0, leg_w=1.9, leg_d=2.1, hip_x=1.4, foot=(2.1, 1.2, 3.2), torso=(5.2, 8.0, 3.2),
                 arm_w=1.7, arm_len=8.4, arm_x=3.5, hand=1.8, leg_t="D", arm_t="A")
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, (yh + ys) / 2 + 0.4, 0), (5.8, ys - yh + 0.8, 3.6), "A")          # lab coat
    s.box((0, (yh + ys) / 2, -1.9), (1.4, ys - yh - 1.0, 0.3), "D")              # shirt + tie
    s.on(7)
    s.box((0, yh - 2.0, 0.6), (5.8, 5.0, 2.4), "A")                              # coat tails
    s.part(7, 0, (0, yh + 0.4, 0.6), "Cloth")
    x, y, z = b["hands"][1]
    s.on(4)
    s.box((x, y + 0.2, z - 1.2), (1.8, 1.0, 2.6), "D")                           # Mouser remote
    s.cyl((x, y + 1.6, z - 1.8), (0.3, 2.0, 0.3), "C")
    s.on(5)
    hy = b["y_neck"] + 2.3
    s.ell((0, hy, 0), (3.8, 4.8, 4.2), "P")
    s.ell((0, hy + 1.6, 0.4), (4.4, 2.6, 4.4), "D")                              # hair
    s.box((0, hy - 0.8, -2.0), (2.4, 0.5, 0.4), "D")                             # moustache
    for sx in (-1, 1):
        s.box((sx * 0.9, hy + 0.3, -2.1), (1.3, 1.1, 0.4), "C")                  # big glasses
    s.box((0, hy + 0.3, -2.2), (0.6, 0.3, 0.3), "C")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    payload = R("mouser", "Biped", ["Grab", "Fast", "Swarm"], 0.5, 6.0, (0.6, 0.62, 0.7), count=6, temp="Aggressive",
                seed=85)
    names = ["baxter stockman", "baxter", "dr baxter stockman", "baxter stockman tmnt", "doctor stockman",
             "stockman"]
    r = R("Baxter Stockman", "Biped", ["Shoots", "Fast"], 1.8, 70.0, (0.92, 0.92, 0.96), "baxter_stockman",
          temp="Fearful", inten=0.7, payload=payload)
    return names, r, s, "Baxter Stockman: the scientist with the remote - keeps away and unleashes Mousers."


def baxter_fly():
    s = S(k=1.5)
    b = humanoid(s, leg_h=8.4, leg_w=1.6, leg_d=1.8, hip_x=1.6, foot=(2.6, 1.0, 3.2), torso=(5.6, 7.4, 4.0),
                 torso_kind="ell", arm_w=1.5, arm_len=8.6, arm_x=3.6, hand=2.0, leg_t="D", arm_t="D")
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.ell((0, yh + 0.4, 1.4), (5.0, 5.4, 5.6), "D")                              # fly abdomen
    for dy in (-1.0, 0.6):
        s.ell((0, yh + 0.4 + dy, 1.4), (5.2, 0.5, 5.8), "P")
    s.box((0, ys - 3.0, -1.9), (4.0, 4.4, 0.6), "A")                              # tattered lab coat remnants
    for pid, sx, role in ((12, -1, "WingLeft"), (13, 1, "WingRight")):
        s.on(pid)
        s.ell((sx * 5.0, ys - 1.0, 3.4), (8.0, 0.4, 4.0), "C", (0, sx * -20, sx * 20))
        s.box((sx * 5.0, ys - 1.0, 3.4), (7.4, 0.5, 0.4), "D", (0, sx * -20, sx * 20))
        s.part(pid, 0, (sx * 1.6, ys - 0.6, 2.2), role)
    s.on(5)
    hy = b["y_neck"] + 2.4
    s.ell((0, hy, 0), (4.4, 4.6, 4.4), "P")
    for sx in (-1, 1):
        s.ell((sx * 2.2, hy + 0.6, -0.6), (3.2, 3.8, 3.4), "A")                  # compound eyes
        s.box((sx * 2.4, hy + 0.6, -2.2), (1.2, 1.2, 0.4), "C")
        s.seg((sx * 0.6, hy + 2.0, -0.4), (sx * 1.8, hy + 4.6, -1.6), 0.4, 0.4, "D")   # antennae
    s.seg((0, hy - 1.6, -1.6), (0, hy - 3.6, -3.2), 1.0, 1.0, "D")                # proboscis
    s.ell((0, hy - 3.8, -3.4), (1.6, 1.0, 1.6), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["baxter stockman fly", "baxter fly", "fly baxter", "mutant baxter", "baxter the fly", "stockman fly",
             "baxter mutant fly"]
    r = R("Baxter Stockman (Fly)", "Insect", ["Flies", "Shoots", "Sticky"], 1.9, 60.0, (0.9, 0.12, 0.12),
          "baxter_stockman_fly", elem="Poison", temp="Chaotic", inten=0.8,
          payload=R("fly spit", "Ball", ["Sticky"], 0.35, 0.5, elem="Poison", seed=4))
    return names, r, s, "Mutant fly Baxter: compound eyes, wings, proboscis. Flies, spits sticky Poison goo."


def mousers():
    s = S(k=1.6)
    s.box((0, 6.2, -0.4), (5.4, 3.4, 6.2), "P")                                  # upper jaw / body
    s.box((0, 7.9, 0.4), (4.4, 0.8, 4.4), "D")
    for sx in (-1, 1):
        s.box((sx * 1.4, 7.4, -3.4), (1.2, 0.8, 0.4), "C")                        # eyes
    for i in range(5):
        s.cone((-2.0 + i * 1.0, 4.2, -3.2), (0.6, 1.0, 0.6), "C", (180, 0, 0))    # top fangs
    s.on(6)
    s.box((0, 3.6, -0.6), (5.0, 1.4, 5.8), "P")                                   # the huge bottom jaw
    for i in range(4):
        s.cone((-1.5 + i * 1.0, 4.6, -3.1), (0.6, 1.0, 0.6), "C")
    s.part(6, 0, (0, 4.6, 2.2), "Jaw")
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):
        s.on(pid)
        s.box((sx * 1.4, 1.8, 0.8), (0.8, 3.2, 0.8), "D")
        s.box((sx * 1.4, 0.4, 0.0), (1.8, 0.8, 3.0), "D")
        s.part(pid, 0, (sx * 1.4, 3.4, 0.8), role)
    s.on(0)
    names = ["mousers", "mouser robots", "mouser robot", "baxter's mousers", "stockman's mousers", "mousers tmnt",
             "mouser tmnt"]
    r = R("Mousers", "Biped", ["Swarm", "Grab", "Fast"], 0.5, 6.0, (0.6, 0.62, 0.7), "mousers", count=10,
          temp="Aggressive", inten=0.9)
    return names, r, s, "Mousers: Baxter's tiny walking jaws. A swarm that bites and holds on."


# ---------------------------------------------------------------------------------- Foot Clan

def _foot(s, bot=False):
    b = humanoid(s, leg_h=8.6, leg_w=2.2, leg_d=2.4, hip_x=1.6, foot=(2.4, 1.2, 3.4), torso=(6.0, 8.2, 3.6),
                 arm_w=2.0, arm_len=8.4, arm_x=4.0, hand=2.0)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, yh + 0.6, 0), (6.2, 1.2, 3.8), "D")                                # obi
    s.box((0, ys - 3.4, -td / 2 - 0.25), (2.6, 2.6, 0.4), "A")                   # Foot emblem
    for dx in (-0.9, 0.0, 0.9):
        s.box((dx, ys - 1.7, -td / 2 - 0.25), (0.6, 1.0, 0.4), "A")               # the toes
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        s.box((sx * 1.6, 2.4, 0), (2.5, 2.2, 2.6), "D")                           # tabi wraps
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        s.box((sx * 4.0, ys - 6.0, 0), (2.3, 2.2, 2.3), "D")
    x, y, z = b["hands"][1]
    s.on(4)
    katana_in(s, (x, y, z), length=8.0, rise=3.0, guard="D")
    s.on(0)
    s.box((0, ys - 3.0, td / 2 + 0.5), (0.8, 9.0, 0.8), "D", (0, 0, 35))          # saya on the back
    s.on(5)
    hy = b["y_neck"] + 2.3
    if bot:
        s.box((0, hy, 0), (4.0, 4.6, 4.2), "P")
        s.box((0, hy + 0.4, -2.2), (3.6, 1.0, 0.4), "C")                          # visor
        s.cyl((1.4, hy + 3.0, 0), (0.3, 1.8, 0.3), "C")
    else:
        s.ell((0, hy, 0), (4.2, 4.8, 4.4), "P")                                   # hood
        s.box((0, hy + 0.4, -2.0), (3.0, 1.0, 0.5), "D")                          # eye slit
        for sx in (-1, 1):
            s.box((sx * 0.7, hy + 0.4, -2.3), (0.8, 0.5, 0.3), "C")
        s.box((0, hy + 1.2, 2.0), (1.2, 1.0, 1.0), "D")                           # hood knot
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)


def foot_soldiers():
    s = S(k=1.5)
    _foot(s)
    names = ["foot clan soldiers", "foot clan soldier", "foot soldiers", "foot clan", "the foot clan", "foot ninja",
             "foot ninjas", "foot clan ninjas", "the foot"]
    r = R("Foot Soldiers", "Biped", ["Swarm", "Blade", "Fast"], 1.8, 75.0, FOOT_RED, "foot_soldiers", count=6,
          temp="Sneaky", inten=0.8)
    return names, r, s, "The Foot Clan: a squad of ninja with the red foot emblem. Sneaky swarm of blades."


def foot_bots():
    s = S(k=1.5)
    _foot(s, bot=True)
    names = ["foot bots", "footbots", "foot bot", "footbot", "foot robots", "foot clan robots", "foot soldier robots"]
    r = R("Foot Bots", "Biped", ["Swarm", "Blade", "Shoots"], 1.8, 120.0, FOOT_RED, "foot_bots", count=5,
          elem="Electric", temp="Aggressive", inten=0.8, feat=["Armor"])
    return names, r, s, "Foot Bots: the Foot's robot soldiers (1987). Aggressive, armoured, Electric."


# ------------------------------------------------------------------------ mutants of the films

def tokka():
    s = S(k=1.4)
    b = turtle(s, big=1.4, shell_spikes=True, mask="P", beak=True, eyes="A", mask_tails=False, brow=True)
    s.on(5)
    for sx in (-1, 1):
        s.cone((sx * 1.4, b["hy"] - 3.2, -4.8), (0.8, 1.6, 0.8), "C", (180, 0, 0))   # fangs
    s.on(0)
    names = ["tokka", "tokka tmnt", "tokka the snapping turtle", "tokka snapping turtle", "tokka and rahzar"]
    r = R("Tokka", "Biped", ["Charge", "Heavy", "Blunt", "Guard"], 2.8, 500.0, (0.55, 0.45, 0.3), "tokka",
          temp="Chaotic", inten=0.8, feat=["Spikes"])
    return names, r, s, "Tokka (Secret of the Ooze): a hulking baby snapping turtle. Chaotic, heavy charger."


def rahzar():
    s = S(k=1.5)
    b = humanoid(s, leg_h=8.4, leg_w=2.8, leg_d=3.0, hip_x=2.0, foot=(3.0, 1.4, 4.4), torso=(7.6, 8.8, 5.0),
                 torso_kind="ell", arm_w=2.6, arm_len=9.6, arm_x=5.0, hand=2.8)
    yh, ys = b["y_hip"], b["y_sh"]
    s.ell((0, ys - 1.0, 0.4), (8.4, 4.0, 6.0), "D")                               # shaggy mane
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        x, y, z = b["hands"][sx]
        for dx in (-0.8, 0.0, 0.8):
            s.cone((x + dx, y - 1.6, z - 0.6), (0.5, 1.8, 0.5), "C", (180, 0, 0))    # claws
    s.on(0)
    beast_head(s, b["y_neck"] - 0.4, snout=5.0, snout_w=3.0, head=(5.2, 5.0, 5.4), ears="pointy", jaw=True)
    tail(s, [(0, yh + 0.6, 2.4), (0, yh - 1.4, 5.6), (0, yh - 2.4, 8.4)], [(2.4, 2.4, 3.8), (2.0, 2.0, 3.4)], t="D")
    names = ["rahzar", "rahzar tmnt", "rahzar the wolf", "rahzar wolf", "razhar"]
    r = R("Rahzar", "Biped", ["Blade", "Fast", "Repel"], 2.6, 220.0, (0.45, 0.4, 0.35), "rahzar", temp="Aggressive",
          inten=0.9)
    return names, r, s, "Rahzar (Secret of the Ooze): the baby wolf mutant. Clawing Blade, Fast; his howl Repels."


# ------------------------------------------------------------------------------- human villains

def karai():
    s = S(k=1.5)
    b = humanoid(s, leg_h=8.8, leg_w=1.9, leg_d=2.1, hip_x=1.4, foot=(2.1, 1.2, 3.0), torso=(5.0, 7.6, 3.2),
                 torso_t="D", arm_w=1.7, arm_len=8.2, arm_x=3.5, hand=1.8, leg_t="D", arm_t="D")
    ys, td = b["y_sh"], b["td"]
    s.box((0, ys - 2.4, -td / 2 - 0.1), (4.4, 3.4, 0.6), "A")                     # chest guard
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        s.box((sx * 3.7, ys - 0.2, 0), (2.8, 1.8, 2.8), "A")                       # shoulder guards
    x, y, z = b["hands"][1]
    s.on(4)
    katana_in(s, (x, y, z), length=7.0, rise=1.0)
    s.on(5)
    hy = b["y_neck"] + 2.2
    s.ell((0, hy, 0), (3.6, 4.4, 4.0), "P")
    s.ell((0, hy + 0.6, 0.6), (4.2, 3.8, 4.2), "D")                               # bob cut
    s.box((0, hy - 0.4, 1.6), (3.8, 3.0, 1.2), "D")
    for sx in (-1, 1):
        s.box((sx * 0.8, hy + 0.1, -1.9), (0.7, 0.5, 0.3), "C")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["karai", "karai tmnt", "oroku karai", "karai foot clan", "miwa"]
    r = R("Karai", "Biped", ["Blade", "Fast"], 1.7, 60.0, (0.85, 0.12, 0.12), "karai", temp="Sneaky", inten=0.9)
    return names, r, s, "Karai: Shredder's adopted daughter, kunoichi of the Foot. Sneaky, Fast blade."


def hun():
    s = S(k=1.4)
    b = humanoid(s, leg_h=9.0, leg_w=3.4, leg_d=3.4, hip_x=2.4, foot=(3.6, 1.6, 4.6), torso=(10.4, 10.0, 6.2),
                 arm_w=3.6, arm_len=10.0, arm_x=7.0, hand=3.6, leg_t="D")
    ys, td = b["y_sh"], b["td"]
    s.box((0, ys - 4.0, 0.2), (10.8, 8.0, 6.4), "D")                              # jacket
    s.box((0, ys - 4.0, -3.0), (4.0, 8.1, 0.4), "P")
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        s.seg((sx * 7.0, ys - 2.0, -1.9), (sx * 7.0, ys - 8.0, -1.4), 1.2, 0.4, "A")   # dragon tattoos
    s.on(5)
    hy = b["y_neck"] + 2.6
    s.box((0, hy, 0), (4.8, 5.4, 5.0), "P")
    s.box((0, hy + 2.8, 0.6), (4.8, 1.0, 4.4), "C")                               # blond hair
    s.box((0, hy + 1.2, 3.0), (1.4, 3.4, 1.2), "C")                               # ponytail
    for sx in (-1, 1):
        s.box((sx * 1.1, hy + 0.6, -2.6), (1.0, 0.5, 0.3), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["hun", "hun tmnt", "hun purple dragons", "hun the purple dragon", "purple dragons leader"]
    r = R("Hun", "Biped", ["Blunt", "Charge", "Heavy", "Grab"], 2.2, 180.0, (0.55, 0.2, 0.75), "hun_tmnt",
          temp="Aggressive", inten=0.9)
    return names, r, s, "Hun: the Purple Dragons' giant, blond, tattooed enforcer. Grabs and charges."


def purple_dragons():
    s = S(k=1.4)
    b = humanoid(s, leg_h=8.8, leg_w=2.4, leg_d=2.4, hip_x=1.7, foot=(2.6, 1.4, 3.4), torso=(6.6, 8.0, 3.8),
                 arm_w=2.2, arm_len=8.6, arm_x=4.4, hand=2.2, leg_t="D")
    ys, td = b["y_sh"], b["td"]
    s.box((0, ys - 3.6, 0.1), (6.9, 7.0, 4.0), "D")
    s.seg((-1.6, ys - 1.4, -td / 2 - 0.3), (1.6, ys - 5.4, -td / 2 - 0.3), 1.0, 0.4, "A")   # dragon logo
    x, y, z = b["hands"][1]
    s.on(4)
    s.box((x, y - 0.2, z - 3.0), (0.9, 0.9, 6.0), "C")                            # crowbar
    s.on(5)
    hy = b["y_neck"] + 2.3
    s.ell((0, hy, 0), (4.0, 4.8, 4.2), "P")
    s.box((0, hy + 2.2, 0.4), (1.0, 1.8, 4.0), "A")                               # dyed mohawk
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["purple dragons", "purple dragon gang", "purple dragons gang", "purple dragon thugs"]
    r = R("Purple Dragons", "Biped", ["Swarm", "Blunt"], 1.8, 80.0, (0.55, 0.2, 0.75), "purple_dragons", count=5,
          temp="Chaotic", inten=0.7)
    return names, r, s, "The Purple Dragons street gang: a Chaotic swarm with crowbars."


def bishop():
    s = S(k=1.5)
    b = humanoid(s, leg_h=9.0, leg_w=2.2, leg_d=2.2, hip_x=1.5, foot=(2.3, 1.2, 3.4), torso=(6.0, 8.2, 3.4),
                 torso_t="D", arm_w=1.9, arm_len=8.6, arm_x=4.0, hand=1.9, leg_t="D", arm_t="D")
    ys, td = b["y_sh"], b["td"]
    s.box((0, ys - 2.0, -td / 2 - 0.1), (1.6, 3.6, 0.3), "C")                     # white shirt
    s.box((0, ys - 2.4, -td / 2 - 0.3), (0.6, 3.2, 0.3), "D")                      # tie
    x, y, z = b["hands"][1]
    s.on(4)
    blaster_in(s, (x, y, z), length=5.0)
    s.on(5)
    hy = b["y_neck"] + 2.3
    s.ell((0, hy, 0), (3.8, 5.0, 4.2), "P")                                       # bald head
    s.box((0, hy + 0.4, -2.0), (3.4, 0.9, 0.4), "D")                              # shades
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["agent bishop", "bishop tmnt", "john bishop", "agent john bishop", "bishop earth protection force"]
    r = R("Agent Bishop", "Biped", ["Shoots", "Fast"], 1.85, 80.0, (0.2, 0.2, 0.22), "agent_bishop", temp="Sneaky",
          inten=0.8, payload=R("stun blast", "Ball", ["Fast"], 0.3, 0.5, elem="Electric", seed=5))
    return names, r, s, "Agent Bishop (2003): black suit, shades, alien tech blaster. Sneaky marksman."


def savanti():
    s = S(k=1.5)
    b = humanoid(s, leg_h=8.4, leg_w=2.0, leg_d=2.2, hip_x=1.4, foot=(2.2, 1.2, 3.2), torso=(5.6, 8.4, 3.4),
                 torso_t="A", arm_w=2.0, arm_len=8.4, arm_x=3.8, hand=2.0, arm_t="A", leg_t="D")
    yh, ys = b["y_hip"], b["y_sh"]
    s.box((0, yh - 2.6, 0), (6.4, 5.8, 4.2), "A")                                 # robe
    x, y, z = b["hands"][1]
    s.on(4)
    s.cyl((x, 8.0, z - 1.2), (0.7, 16.0, 0.7), "D")
    s.ell((x, 16.6, z - 1.2), (1.8, 1.8, 1.8), "C")                               # scepter orb
    s.on(5)
    hy = b["y_neck"] + 2.3
    s.ell((0, hy, 0), (3.8, 4.8, 4.2), "P")
    for sx in (-1, 1):
        s.cone((sx * 1.4, hy + 3.0, 0.4), (1.0, 2.8, 1.0), "D", (-20, 0, -sx * 25))   # horns
        s.box((sx * 0.8, hy + 0.3, -2.0), (0.7, 0.5, 0.3), "C")
    s.cone((0, hy - 2.8, -1.2), (1.2, 2.0, 1.2), "D", (180, 0, 0))                # goatee
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["savanti romero", "savanti", "savanti romero tmnt"]
    r = R("Savanti Romero", "Biped", ["Portal", "Shoots"], 1.9, 70.0, (0.5, 0.1, 0.2), "savanti_romero", elem="Shadow",
          temp="Sneaky", inten=0.8)
    return names, r, s, "Savanti Romero: the time-meddling demon sorcerer. Opens portals, casts Shadow bolts."


# ----------------------------------------------------------------------- rock soldiers & aliens

def _rock(s, bulk=1.0, visor=False):
    k = bulk
    b = humanoid(s, leg_h=8.0 * k, leg_w=3.4 * k, leg_d=3.4 * k, hip_x=2.4 * k, foot=(3.8 * k, 1.6, 4.4 * k),
                 torso=(9.6 * k, 9.0 * k, 6.4 * k), torso_kind="ell", arm_w=3.6 * k, arm_len=9.0 * k, arm_x=6.6 * k,
                 hand=4.0 * k)
    ys = b["y_sh"]
    for (x, y, z, sz) in ((-3.0, ys - 2.0, -2.6, 3.6), (2.6, ys - 5.4, -2.8, 3.0), (0.8, ys + 0.2, 2.4, 4.0),
                          (-2.4, ys - 6.4, 2.6, 3.2)):
        s.ell((x * k, y, z * k), (sz * k, sz * 0.8 * k, sz * k), "D")              # boulders
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        s.ell((sx * 6.8 * k, ys - 0.4, 0), (4.4 * k, 3.6 * k, 4.4 * k), "D")      # rock shoulders
    s.on(5)
    hy = b["y_neck"] + 1.8 * k
    s.ell((0, hy, 0), (4.4 * k, 4.0 * k, 4.4 * k), "P")
    if visor:
        s.box((0, hy + 0.3, -2.1 * k), (3.6 * k, 0.9, 0.5), "C")
    else:
        for sx in (-1, 1):
            s.box((sx * 0.9 * k, hy + 0.3, -2.1 * k), (0.9, 0.7, 0.5), "C")
    s.box((0, hy + 2.0 * k, 0), (4.8 * k, 1.0, 4.8 * k), "A")                      # helmet rim
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)


def traag():
    s = S(k=1.3)
    _rock(s, bulk=1.2)
    names = ["general traag", "traag", "traag tmnt", "general traag rock soldier"]
    r = R("General Traag", "Biped", ["Blunt", "Heavy", "Stomp"], 2.8, 1200.0, (0.45, 0.3, 0.6), "general_traag",
          elem="Earth", temp="Aggressive", inten=0.8, feat=["Armor"])
    return names, r, s, "General Traag: the rock soldier general from Dimension X. Heavy Earth stomps."


def granitor():
    s = S(k=1.3)
    _rock(s, bulk=1.05, visor=True)
    names = ["granitor", "granitor tmnt", "lieutenant granitor"]
    r = R("Granitor", "Biped", ["Blunt", "Heavy", "Charge"], 2.6, 1000.0, (0.45, 0.3, 0.6), "granitor", elem="Earth",
          temp="Aggressive", inten=0.7, feat=["Armor"])
    return names, r, s, "Granitor: Traag's rock-soldier lieutenant. Heavy Earth charger."


def rock_soldiers():
    s = S(k=1.3)
    _rock(s, bulk=0.9)
    names = ["rock soldiers", "rock soldier", "rock soldiers tmnt", "stone warriors tmnt", "stone warriors"]
    r = R("Rock Soldiers", "Biped", ["Swarm", "Blunt", "Heavy"], 2.2, 600.0, (0.45, 0.3, 0.6), "rock_soldiers",
          count=4, elem="Earth", temp="Aggressive", inten=0.7)
    return names, r, s, "Rock soldiers of Dimension X: a heavy Earth squad."


def _triceraton():
    s = S(k=1.4)
    b = humanoid(s, leg_h=9.0, leg_w=3.0, leg_d=3.2, hip_x=2.2, foot=(3.4, 1.6, 4.4), torso=(8.4, 9.0, 5.4),
                 arm_w=3.0, arm_len=9.4, arm_x=6.0, hand=3.0)
    ys, td = b["y_sh"], b["td"]
    s.box((0, ys - 3.2, 0), (8.8, 6.4, 5.8), "D")                                 # armour
    s.box((0, ys - 3.0, -td / 2 - 0.3), (4.0, 3.0, 0.4), "A")
    x, y, z = b["hands"][1]
    s.on(4)
    blaster_in(s, (x, y, z), length=8.0)
    s.on(0)
    hy = beast_head(s, b["y_neck"], snout=3.2, snout_w=3.6, head=(5.2, 5.0, 5.6), eye_t="C")
    s.on(5)
    s.box((0, hy + 2.6, 2.6), (8.0, 6.4, 1.0), "A", (-25, 0, 0))                  # frill
    for sx in (-1, 1):
        s.cone((sx * 1.4, hy + 2.6, -2.4), (1.0, 4.0, 1.0), "C", (-60, 0, 0))    # brow horns
    s.cone((0, hy + 0.2, -5.6), (1.0, 2.2, 1.0), "C", (-40, 0, 0))                # nose horn
    s.on(0)
    return s


def triceratons():
    s = _triceraton()
    names = ["triceratons", "triceraton army", "triceraton soldiers", "triceratons tmnt", "triceraton republic"]
    r = R("Triceratons", "Biped", ["Swarm", "Charge", "Shoots", "Heavy"], 2.3, 300.0, (0.85, 0.45, 0.2),
          "triceratons", count=3, temp="Aggressive", inten=0.8, feat=["Armor"],
          payload=R("fugitoid-grade blast", "Ball", ["Fast"], 0.4, 1.0, elem="Fire", seed=21))
    return names, r, s, "Triceratons: dinosaur-headed alien soldiers, a charging squad with blasters."


def triceraton():
    s = _triceraton()
    names = ["triceraton", "triceraton soldier", "triceraton warrior", "zanramon", "traximus"]
    r = R("Triceraton", "Biped", ["Charge", "Shoots", "Heavy"], 2.4, 320.0, (0.85, 0.45, 0.2), "triceraton",
          temp="Aggressive", inten=0.8, feat=["Armor"],
          payload=R("triceraton blast", "Ball", ["Fast"], 0.4, 1.0, elem="Fire", seed=21))
    return names, r, s, "A single Triceraton warrior (horns, frill, blaster)."


def _kraang_droid(k=1.5):
    s = S(k=k)
    b = humanoid(s, leg_h=9.0, leg_w=2.2, leg_d=2.4, hip_x=1.6, foot=(2.4, 1.2, 3.2), torso=(6.0, 8.6, 4.0),
                 arm_w=1.9, arm_len=9.0, arm_x=4.0, hand=2.0, leg_t="P", arm_t="P")
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    by = yh + 3.6
    s.box((0, by, -td / 2 - 0.1), (4.4, 3.8, 1.0), "D")                           # belly bay
    s.ell((0, by, -td / 2 - 0.6), (3.6, 3.0, 1.8), "A")                           # the Kraang
    s.box((0, by + 1.4, -td / 2 - 1.2), (1.8, 0.3, 0.3), "D")
    x, y, z = b["hands"][1]
    s.on(4)
    blaster_in(s, (x, y, z), length=6.0, t="P")
    s.on(5)
    hy = b["y_neck"] + 2.3
    s.ell((0, hy, 0), (3.8, 4.8, 4.0), "P")                                       # blank face
    for sx in (-1, 1):
        s.box((sx * 0.8, hy + 0.3, -1.9), (0.8, 0.8, 0.4), "C")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    return s


def kraang_droids():
    s = _kraang_droid()
    names = ["kraang droids", "kraang droid", "kraang", "kraang robots", "kraang bots", "kraang tmnt"]
    r = R("Kraang Droids", "Biped", ["Swarm", "Shoots"], 1.9, 110.0, KRANG_PINK, "kraang_droids", count=4,
          temp="Calm", payload=R("kraang blast", "Ball", ["Fast"], 0.35, 0.5, elem="Electric", seed=9))
    return names, r, s, "Kraang droids (2012): androids with a Kraang in the belly, blasters. \"Kraang is...\""


def utroms():
    s = S(k=1.6)
    _brain(s, w=6.0, h=5.0, d=6.0, y=3.6)
    for i, (sx, sz, role) in enumerate(((-1, -1, "LegFrontLeft"), (1, -1, "LegFrontRight"), (-1, 1, "LegBackLeft"),
                                        (1, 1, "LegBackRight"))):
        s.on(i + 1)
        s.seg((sx * 1.6, 2.0, sz * 1.4), (sx * 3.0, 0.3, sz * 2.4), 0.9, 0.9, "A")
        s.part(i + 1, 0, (sx * 1.6, 2.0, sz * 1.4), role)
    s.on(0)
    names = ["utroms", "utrom", "utroms tmnt", "utrom aliens", "utrom brains"]
    r = R("Utroms", "Blob", ["Swarm", "Grab"], 0.6, 4.0, KRANG_PINK, "utroms", count=5, elem="Psychic", temp="Sneaky",
          inten=0.6)
    return names, r, s, "Utroms (Mirage/2003): little brain-aliens on tentacles. A sneaky grabbing swarm."


def kraang_prime():
    s = S(k=1.2)
    y = 20.0
    _brain(s, w=26.0, h=20.0, d=24.0, y=y, one_eye=True)
    roles = ((-1, -1, "LegFrontLeft"), (1, -1, "LegFrontRight"), (-1, 1, "LegBackLeft"), (1, 1, "LegBackRight"))
    for i, (sx, sz, role) in enumerate(roles):
        s.on(i + 1)
        a = (sx * 7.0, y - 7.0, sz * 6.0)
        m = (sx * 12.0, y - 10.0, sz * 10.0)
        e = (sx * 14.0, 1.0, sz * 12.0)
        s.seg(a, m, 3.6, 3.6, "A")
        s.seg(m, e, 2.8, 2.8, "A")
        s.part(i + 1, 0, a, role)
    for sx, aid, role in ((-1, 5, "ArmLeft"), (1, 7, "ArmRight")):
        s.on(aid)
        a = (sx * 10.0, y - 2.0, -6.0)
        s.seg(a, (sx * 18.0, y + 6.0, -12.0), 3.0, 3.0, "A")
        s.seg((sx * 18.0, y + 6.0, -12.0), (sx * 20.0, y + 14.0, -14.0), 2.2, 2.2, "A")
        s.part(aid, 0, a, role)
    s.on(0)
    names = ["kraang prime", "kraang prime tmnt", "kraang one"]
    r = R("Kraang Prime", "Kaiju", ["Grab", "Constrict", "Beam", "Stomp"], 14.0, 30000.0, KRANG_PINK, "kraang_prime",
          elem="Psychic", temp="Aggressive", inten=0.9)
    return names, r, s, "Kraang Prime (2012): the giant one-eyed brain on tentacles. Kaiju; grabs, constricts, eye-beam."


def dregg():
    s = S(k=1.5)
    b = humanoid(s, leg_h=9.4, leg_w=2.2, leg_d=2.4, hip_x=1.6, foot=(2.6, 1.2, 3.4), torso=(6.4, 8.8, 4.0),
                 torso_t="A", arm_w=2.0, arm_len=9.0, arm_x=4.2, hand=2.2, leg_t="D", arm_t="D")
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.on(7)
    s.box((0, (yh + ys) / 2 - 1.0, td / 2 + 0.5), (7.0, ys - yh + 6.0, 0.7), "D")  # cape
    s.part(7, 0, (0, ys, td / 2 + 0.5), "Cloth")
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        s.box((sx * 4.4, ys, 0), (3.4, 2.4, 3.6), "A")
        s.cone((sx * 4.8, ys + 2.0, 0), (1.2, 2.4, 1.2), "C", (0, 0, -sx * 25))
    s.on(5)
    hy = b["y_neck"] + 2.8
    s.ell((0, hy, 0), (4.6, 5.6, 5.0), "P")                                       # bug head
    for sx in (-1, 1):
        s.ell((sx * 1.4, hy + 0.8, -1.8), (2.2, 2.8, 1.8), "A")                   # insect eyes
        s.seg((sx * 0.8, hy - 1.8, -2.0), (sx * 0.3, hy - 3.2, -3.0), 0.7, 0.7, "C")   # mandibles
        s.seg((sx * 0.8, hy + 2.6, -0.4), (sx * 2.0, hy + 5.0, -1.2), 0.4, 0.4, "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["lord dregg", "dregg", "lord dregg tmnt", "dregg the warlord"]
    r = R("Lord Dregg", "Biped", ["Shoots", "Blade", "Guard"], 2.1, 150.0, (0.35, 0.75, 0.3), "lord_dregg",
          elem="Electric", temp="Aggressive", inten=0.8, feat=["Armor"])
    return names, r, s, "Lord Dregg: the insectoid alien warlord of the 1987 finale seasons. Electric, caped."


# --------------------------------------------------------------------------- 1987 toy mutants

def groundchuck():
    s = S(k=1.4)
    b = humanoid(s, leg_h=8.4, leg_w=3.2, leg_d=3.4, hip_x=2.4, foot=(3.6, 1.6, 4.4), torso=(9.4, 9.4, 6.0),
                 torso_kind="ell", arm_w=3.2, arm_len=9.6, arm_x=6.2, hand=3.4, leg_t="D")
    ys = b["y_sh"]
    s.on(4)
    s.box((6.4, ys - 5.0, 0), (4.4, 9.0, 4.4), "A")                               # cybernetic arm
    s.cz((6.4, ys - 10.4, -1.0), 2.4, 3.0, "C")
    s.on(0)
    hy = beast_head(s, b["y_neck"], snout=2.4, snout_w=4.8, head=(6.2, 5.6, 5.6), eye_t="C")
    s.on(5)
    for sx in (-1, 1):
        s.seg((sx * 2.8, hy + 1.4, 0), (sx * 6.0, hy + 2.2, -0.4), 1.4, 1.4, "C")    # horns out...
        s.seg((sx * 6.0, hy + 2.2, -0.4), (sx * 6.8, hy + 4.6, -1.0), 1.0, 1.0, "C")  # ...and up
    s.cyl((0, hy - 1.8, -5.2), (1.6, 0.4, 1.6), "C", ALONG_X)                      # nose ring
    s.on(0)
    names = ["groundchuck", "ground chuck tmnt", "groundchuck the bull", "groundchuck tmnt"]
    r = R("Groundchuck", "Biped", ["Charge", "Heavy", "Blunt"], 2.4, 400.0, (0.85, 0.2, 0.15), "groundchuck",
          temp="Aggressive", inten=0.9)
    return names, r, s, "Groundchuck: the cyborg bull mutant (with Dirtbag). Heavy horned charges."


def dirtbag():
    s = S(k=1.5)
    b = humanoid(s, leg_h=6.6, leg_w=2.6, leg_d=2.8, hip_x=1.9, foot=(3.2, 1.2, 4.0), torso=(7.6, 8.4, 5.4),
                 torso_kind="ell", arm_w=2.6, arm_len=8.6, arm_x=5.0, hand=3.0, leg_t="A")
    ys = b["y_sh"]
    s.box((0, ys - 4.0, 0), (8.0, 4.0, 5.8), "A")                                 # overalls
    x, y, z = b["hands"][1]
    s.on(4)
    s.box((x, y + 3.6, z - 1.0), (0.9, 9.0, 0.9), "D")                            # pickaxe
    s.seg((x, y + 8.0, z - 5.0), (x, y + 8.4, z + 3.0), 1.0, 1.0, "C")
    for sx in (-1, 1):
        s.on(3 if sx < 0 else 4)
        hx, hyy, hz = b["hands"][sx]
        for dx in (-0.9, 0.0, 0.9):
            s.cone((hx + dx, hyy - 1.6, hz - 0.4), (0.6, 1.8, 0.6), "C", (180, 0, 0))   # digging claws
    s.on(0)
    hy = beast_head(s, b["y_neck"] - 0.4, snout=4.4, snout_w=2.0, head=(5.0, 4.6, 5.0), eye_t="D", nose_t="A")
    s.on(5)
    s.ell((0, hy + 1.6, 0.2), (5.4, 2.6, 5.6), "A")                               # miner's helmet
    s.cz((0, hy + 2.0, -2.8), 1.4, 1.0, "C")                                      # headlamp
    s.on(0)
    names = ["dirtbag the mole", "dirtbag mole", "dirtbag tmnt", "dirtbag and groundchuck"]
    r = R("Dirtbag", "Biped", ["Blade", "Heavy"], 1.7, 120.0, (0.8, 0.65, 0.3), "dirtbag_the_mole", elem="Earth",
          temp="Sneaky", inten=0.7)
    return names, r, s, "Dirtbag: the mole miner with a pickaxe and headlamp. Earth, Sneaky (tunnels in)."


def wingnut():
    s = S(k=1.5)
    b = humanoid(s, leg_h=8.4, leg_w=2.0, leg_d=2.2, hip_x=1.5, foot=(2.4, 1.2, 3.4), torso=(6.0, 8.0, 3.8),
                 torso_kind="ell", arm_w=1.9, arm_len=8.4, arm_x=4.0, hand=2.0, leg_t="D")
    ys = b["y_sh"]
    for pid, sx, role in ((12, -1, "WingLeft"), (13, 1, "WingRight")):
        s.on(pid)
        for i, (dx, dy, l) in enumerate(((2.0, 1.0, 7.0), (5.0, 0.0, 7.6), (8.0, -1.4, 7.0))):
            s.seg((sx * 1.6, ys - 0.4, 2.2), (sx * (1.6 + dx), ys + 3.0 + dy, 2.6), 0.6, 0.6, "D")
        s.box((sx * 5.4, ys - 1.0, 2.4), (8.4, 7.0, 0.4), "A", (0, 0, sx * -10))  # membrane
        s.part(pid, 0, (sx * 1.6, ys - 0.4, 2.2), role)
    hy = beast_head(s, b["y_neck"] - 0.4, snout=1.4, snout_w=2.4, head=(4.6, 4.4, 4.4), ears="pointy")
    s.on(5)
    for sx in (-1, 1):
        s.cone((sx * 0.8, hy - 1.8, -2.6), (0.5, 1.2, 0.5), "C", (180, 0, 0))    # fangs
    s.on(0)
    s.ell((-4.0, ys + 1.8, 0.4), (1.6, 1.4, 2.4), "C")                            # Screwloose on his shoulder
    s.seg((-4.0, ys + 1.8, -1.0), (-4.0, ys + 1.4, -3.2), 0.4, 0.4, "D")
    names = ["wingnut and screwloose", "wingnut the bat", "wingnut tmnt", "screwloose", "screwloose tmnt"]
    r = R("Wingnut and Screwloose", "Biped", ["Flies", "Blade", "Fast"], 1.9, 60.0, (0.55, 0.35, 0.6),
          "wingnut_and_screwloose", temp="Chaotic", inten=0.8)
    return names, r, s, "Wingnut (bat) & Screwloose (mosquito): Flies, swooping Chaotic dives."


def scale_tail():
    s = S(k=1.4)
    s.box((0, 4.0, 0), (5.0, 8.0, 4.0), "P")                                      # reared-up body
    s.ell((0, 1.2, 1.0), (6.0, 2.6, 6.0), "P")                                    # coil base
    s.ell((0, 5.0, -1.8), (3.2, 6.0, 0.8), "A")                                   # belly scales
    s.on(5)
    y0 = 8.0
    s.ell((0, y0 + 3.4, 0.6), (8.4, 7.4, 1.4), "P")                               # the cobra hood
    s.ell((0, y0 + 3.4, 0.2), (6.0, 5.6, 1.0), "A")
    s.ell((0, y0 + 3.0, -1.4), (3.4, 3.0, 4.0), "P")                              # head
    for sx in (-1, 1):
        s.box((sx * 1.0, y0 + 3.6, -3.2), (0.8, 0.6, 0.4), "C")
        s.cone((sx * 0.6, y0 + 1.6, -3.0), (0.4, 1.2, 0.4), "C", (180, 0, 0))    # fangs
    s.part(5, 0, (0, y0, 0), "Head")
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):                # snake-like arms
        s.on(pid)
        s.seg((sx * 2.6, 7.0, 0), (sx * 4.6, 3.6, -1.8), 1.2, 1.2, "P")
        s.seg((sx * 4.6, 3.6, -1.8), (sx * 4.0, 2.4, -4.0), 1.0, 1.0, "P")
        s.part(pid, 0, (sx * 2.6, 7.0, 0), role)
    s.on(0)
    pts = [(0, 1.0, 3.0), (2.0, 0.9, 7.0), (0.0, 0.8, 11.0), (-2.4, 0.7, 14.4), (-1.0, 0.6, 18.0), (1.4, 0.5, 20.6)]
    tail(s, pts, [(3.2, 2.2, 4.6), (2.8, 2.0, 4.6), (2.4, 1.8, 4.4), (2.0, 1.4, 4.0), (1.4, 1.0, 3.2)])
    names = ["scale tail", "scaletail", "scale tail tmnt", "scale tail the cobra"]
    r = R("Scale Tail", "Serpent", ["ArticulatedTail", "Constrict", "Pierce"], 2.8, 120.0, (0.9, 0.75, 0.2),
          "scale_tail", elem="Poison", temp="Sneaky", inten=0.8)
    return names, r, s, "Scale Tail: the cobra bounty hunter from Dimension X. Hooded, fanged, constricts."


def build_all():
    return [shredder, super_shredder, shredder_gauntlet, shredder_helmet, krang, krang_android, bebop, rocksteady,
            bebop_and_rocksteady, baxter, baxter_fly, mousers, foot_soldiers, foot_bots, tokka, rahzar, karai, hun,
            purple_dragons, bishop, savanti, traag, granitor, rock_soldiers, triceratons, triceraton, kraang_droids,
            utroms, kraang_prime, dregg, groundchuck, dirtbag, wingnut, scale_tail]
