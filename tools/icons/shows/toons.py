"""Adult animation: The Simpsons, Family Guy, South Park, Futurama, Rick and Morty, Bob's Burgers,
King of the Hill, Archer."""
import math

from reg import entry, simple, R, S
from slib import ALONG_X, ALONG_Z, TIP_NZ
from rig import humanoid, head
from bodies import toon, paper_kid, quad

YELLOW = (1.0, 0.85, 0.15)


def donut(s, c, k=1.0):
    x, y, z = c
    s.cyl((x, y, z), (3.0 * k, 1.1 * k, 3.0 * k), "D", (90, 0, 0))
    s.cyl((x, y, z - 0.35 * k), (2.8 * k, 0.5 * k, 2.8 * k), "C", (90, 0, 0))
    s.cyl((x, y, z - 0.4 * k), (1.0 * k, 1.6 * k, 1.0 * k), "D", (90, 0, 0), sub=True)


@entry
def homer():
    s = S()
    b = toon(s, skin="A", shirt="C", pants="P", torso=(7.8, 7.0, 5.2), belly=2.6, h_leg=5.6, head_h=7.4, head_w=4.8,
             overbite="D")
    s.on(5)
    t = b["top"]
    s.box((-0.5, t + 0.3, -0.4), (0.25, 1.4, 1.8), "D", (0, 0, 10))       # the two hairs
    s.box((0.5, t + 0.3, -0.2), (0.25, 1.4, 1.8), "D", (0, 0, -10))
    for sx in (-1, 1):
        s.box((sx * 2.45, b["hy"] - 0.2, 0.9), (0.4, 1.6, 1.6), "D")       # the M-shaped side hair
        s.ell((sx * 2.5, b["hy"] + 0.4, 0.2), (0.8, 1.4, 1.0), "A")        # ears
    s.on(4)
    hx, hy, hz = b["hands"][1]
    donut(s, (hx, hy + 0.2, hz - 1.6), 1.1)
    s.on(0)
    return (["homer j simpson", "homer simpson", "homer jay simpson", "d'oh", "doh homer", "mmm donuts", "mmm... donuts"],
            R("Homer J. Simpson", "Biped", ["Blunt", "Heavy", "Bounce"], 1.8, 130, YELLOW, temp="Lazy", inten=0.85),
            s, "Homer: bald with two hairs, white shirt, blue pants, donut in hand. The belly bounces people (Bounce),\n"
               "Blunt + Heavy body-slams; Lazy - he sits down and becomes a roadblock.")


@entry
def marge():
    s = S()
    b = toon(s, skin="C", shirt="P", pants="P", shoes="D", torso=(6.2, 8.4, 4.2), h_leg=6.4, head_h=6.2, head_w=4.2)
    s.on(0)
    s.box((0, 3.6, 0), (5.6, 5.2, 4.0), "P")                                 # long dress
    for i in range(7):
        s.ell((-2.1 + i * 0.7, b["y_sh"] - 0.4, -1.8 + abs(i - 3) * 0.25), (0.9, 0.9, 0.9), "C")   # pearls
    s.on(5)
    t = b["top"]
    for i, (w, h) in enumerate(((8.0, 6.0), (8.4, 6.0), (8.0, 5.8), (7.0, 5.2))):
        s.ell((0, t + 2.0 + i * 4.4, 0.6 + i * 0.1), (w, h, w * 0.95), "A")   # the famous tower of blue hair
    s.on(0)
    return (["marge simpson", "marjorie simpson", "marge"],
            R("Marge Simpson", "Biped", ["Guard", "Grab"], 2.2, 60, (0.15, 0.35, 0.95), temp="Protective", inten=0.9),
            s, "Marge (accent = her blue hair): green dress, red pearls, a skyscraper of blue hair. Protective mum: Guard + Grab (drags you home).")


@entry
def bart():
    s = S()
    b = toon(s, skin="A", shirt="P", pants="D", torso=(5.6, 5.0, 3.6), h_leg=4.2, head_h=6.0, head_w=4.2, arm_len=5.6,
             arm_w=1.4)
    s.on(5)
    t = b["top"] - 0.6
    s.box((0, t, 0), (4.3, 0.8, 4.1), "A")
    for i in range(9):
        x = -1.9 + i * 0.475
        s.cone((x, t + 1.0, -1.2 + (i % 2) * 1.6), (0.9, 1.6, 0.9), "A")          # zig-zag crew cut
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.box((hx, hy + 1.6, hz - 0.4), (0.4, 3.0, 0.4), "D")                        # slingshot
    for sx in (-1, 1):
        s.box((hx + sx * 0.6, hy + 3.3, hz - 0.4), (0.35, 1.4, 0.35), "D", (0, 0, -sx * 20))
    s.on(0)
    s.box((0, 0.35, -0.3), (3.2, 0.5, 9.0), "D")                                 # skateboard
    for z in (-3.2, 3.2):
        s.cx((0, 0.0, z), 0.9, 3.4, "C")
    return (["el barto", "bart simpson", "bartholomew j simpson", "bartholomew simpson", "eat my shorts", "ay caramba", "bartman"],
            R("Bart Simpson", "Biped", ["Fast", "Shoots"], 1.2, 30, YELLOW, temp="Playful", inten=0.9,
              payload=R("slingshot pebble", "Ball", ["Ricochet"], 0.3, 1)),
            s, "Bart: zig-zag hair, skateboard, slingshot. Fast, Playful prankster pinging you with ricochet pebbles.")


@entry
def lisa():
    s = S()
    b = toon(s, skin="A", shirt="P", pants="P", torso=(5.0, 5.4, 3.4), h_leg=4.0, head_h=5.6, head_w=4.0, arm_len=5.4,
             arm_w=1.3)
    s.on(0)
    s.box((0, 2.8, 0), (5.4, 3.0, 3.8), "P")                                   # dress hem
    s.on(5)
    t = b["top"] - 0.4
    for i in range(8):
        a = i * 45
        x, z = math.cos(math.radians(a)) * 2.0, math.sin(math.radians(a)) * 2.0
        s.cone((x * 1.1, t + 0.4, z * 1.1), (1.2, 2.4, 1.2), "A", (math.sin(math.radians(a)) * 45, 0, -math.cos(math.radians(a)) * 45))
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.cyl((hx - 1.2, hy + 1.8, hz - 1.4), (0.8, 5.0, 0.8), "C", (0, 0, 20))       # saxophone body
    s.cone((hx - 0.6, hy - 0.8, hz - 1.8), (2.0, 1.8, 2.0), "C", (-40, 0, 0))     # bell
    s.on(0)
    return (["lisa simpson", "lisa's saxophone", "lisa simpson saxophone"],
            R("Lisa Simpson", "Biped", ["Repel"], 1.2, 25, YELLOW, temp="Calm", inten=0.6),
            s, "Lisa: star-point hair, red dress, saxophone - her solo shoves everyone back (Repel).")


@entry
def maggie():
    s = S()
    b = toon(s, skin="A", shirt="P", pants="P", shoes="P", torso=(4.2, 3.6, 3.4), h_leg=1.6, head_h=4.2, head_w=3.8,
             arm_len=3.0, arm_w=1.2, leg_w=1.6, hip_x=1.0, overbite=False)
    s.on(5)
    t = b["top"]
    s.ell((0, t, -0.4), (2.2, 1.2, 1.4), "P")                                  # blue bow
    s.cone((0, b["hy"] - 0.9, -2.3), (1.6, 0.9, 1.6), "C", (-90, 0, 0))          # pacifier
    s.ell((0, b["hy"] - 0.9, -2.8), (1.8, 1.8, 0.5), "C")
    s.on(0)
    return (["maggie simpson", "margaret simpson", "baby maggie"],
            R("Maggie Simpson", "Biped", ["Shoots"], 0.7, 8, YELLOW, temp="Sneaky", inten=0.8),
            s, "Maggie: tiny, blue bow, pacifier. Sneaks up and Shoots (who shot Mr. Burns?).")


@entry
def burns():
    s = S()
    b = humanoid(s, leg_h=9.0, leg_w=1.5, leg_d=1.8, hip_x=1.2, foot=(1.8, 1.0, 3.0), torso=(5.0, 9.0, 3.2), torso_t="P",
                 arm_w=1.3, arm_len=8.0, arm_x=3.2, hand=1.4, arm_t="P", leg_t="P", hand_t="A", foot_t="D", neck=1.6)
    s.on(0)
    s.box((0, b["y_sh"] - 3.0, -1.7), (1.2, 4.5, 0.4), "C")                    # shirt + tie
    s.ell((0, b["y_sh"] - 0.6, 1.2), (4.6, 3.0, 2.6), "P")                      # hunch
    s.on(5)
    hy = b["y_neck"] + 3.4
    s.ell((0, hy, -1.2), (4.4, 5.6, 4.8), "A")                                  # big bald head, craned forward
    s.box((0, hy - 0.6, -4.0), (0.9, 2.4, 1.6), "A", (-25, 0, 0))                # hooked nose
    for sx in (-1, 1):
        s.box((sx * 1.2, hy + 0.6, -3.5), (0.8, 0.8, 0.4), "C")
    s.box((0, hy + 1.5, -3.4), (2.8, 0.4, 0.4), "D")                            # beetle brows
    s.part(5, 0, (0, b["y_neck"], -0.6), "Head")
    for pid, sx in ((3, -1), (4, 1)):                                           # tented fingers ... excellent
        s.on(pid)
        hx, hyy, hz = b["hands"][sx]
        s.cone((hx - sx * 0.8, hyy + 3.4, hz - 2.2), (1.0, 2.2, 1.0), "A", (0, 0, sx * 35))
    s.on(0)
    return (["mr burns", "mr. burns", "montgomery burns", "c. montgomery burns", "monty burns", "excellent mr burns"],
            R("Mr. Burns", "Biped", ["Shoots"], 2.0, 40, YELLOW, temp="Fearful", inten=0.7,
              payload=R("the hounds", "Quadruped", ["Charge", "Swarm"], 1.0, 35, count=3, temp="Aggressive", inten=0.9)),
            s, "Mr. Burns: frail, hunched, tented fingers. Keeps his distance (Fearful) and releases the hounds:\n"
               "his payload is a charging pack of dogs.")


@entry
def power_plant():
    s = S()
    ys = [0, 5, 10, 15, 20, 24]
    ws = [15, 13, 11.2, 10.4, 11, 12]
    for i in range(len(ys) - 1):
        s.cyl((0, (ys[i] + ys[i + 1]) / 2, 0), ((ws[i] + ws[i + 1]) / 2, ys[i + 1] - ys[i] + 0.2, (ws[i] + ws[i + 1]) / 2),
              "P" if i % 2 == 0 else "A")
    s.cyl((0, 24.1, 0), (9.6, 1.0, 9.6), "D", sub=True)
    for i, (x, z, d) in enumerate(((0, 0, 7), (1.5, -1, 6), (-1.5, 1.5, 5.4))):
        s.ell((x, 27 + i * 3.2, z), (d, 3.6, d), "C")                              # steam
    s.box((12, 4, 2), (8, 8, 10), "D")                                             # reactor building
    s.cyl((14.5, 11, 4.5), (1.6, 10, 1.6), "D")                                    # smokestack
    s.box((12, 6, -3.1), (4, 1.4, 0.3), "C")
    return (["springfield nuclear plant", "springfield nuclear power plant", "springfield power plant",
             "sector 7g", "sector 7-g"],
            R("Springfield Nuclear Power Plant", "Pillar", ["Barrier", "Breath"], 10.0, 20000, (0.55, 1.0, 0.3), "Atomic"),
            s, "The cooling tower: a huge tether anchor that vents glowing Atomic steam (Breath) and blocks shots.")


@entry
def peter():
    s = S()
    b = humanoid(s, leg_h=7.5, leg_w=2.6, leg_d=2.8, hip_x=1.7, foot=(2.6, 1.2, 3.6), torso=(8.2, 8.0, 5.4), torso_t="C",
                 torso_kind="ell", arm_w=2.0, arm_len=8.4, arm_x=4.9, hand=2.0, arm_t="C", leg_t="P", hand_t="A",
                 foot_t="D", neck=0.6)
    s.on(0)
    s.ell((0, 10.0, -1.6), (8.8, 6.4, 5.8), "C")                                # belly
    s.box((0, 8.0, 0), (7.0, 1.0, 5.2), "D")                                     # belt
    s.on(5)
    hy = b["y_neck"] + 3.2
    s.ell((0, hy, 0), (5.2, 6.4, 5.0), "A")
    s.ell((0, hy - 2.8, -1.6), (3.0, 2.2, 2.4), "A")                             # the chin
    s.box((0, hy - 3.0, -2.8), (0.3, 1.2, 0.3), "D")                             # chin cleft
    s.box((0, hy + 0.6, -2.5), (4.8, 1.6, 0.4), "D")                             # glasses frame
    for sx in (-1, 1):
        s.box((sx * 1.2, hy + 0.6, -2.75), (1.6, 1.1, 0.3), "C")
    s.ell((0, hy + 2.8, 0.4), (5.0, 1.8, 4.8), "D")                              # hair
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    return (["peter griffin", "peter lowenbrau griffin", "freakin sweet", "family guy peter"],
            R("Peter Griffin", "Biped", ["Blunt", "Charge"], 2.0, 125, (1.0, 0.85, 0.72), temp="Chaotic", inten=0.9),
            s, "Peter: glasses, chin, white shirt, green pants. Chaotic brawler who Charges anyone - ideally a giant chicken.")


@entry
def giant_chicken():
    s = S()
    s.on(0)
    s.ell((0, 13, 0.6), (9.0, 10.0, 11.0), "C")                                    # body
    s.ell((0, 15, 5.4), (6.0, 6.0, 4.0), "C", (-30, 0, 0))                          # tail feathers
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):
        s.on(pid)
        s.box((sx * 2.2, 5.0, 0.4), (1.0, 9.0, 1.0), "A")
        for dz, ry in ((-1.6, 0), (-1.0, -35), (-1.0, 35)):
            s.box((sx * 2.2, 0.35, dz), (0.6, 0.6, 2.4), "A", (0, ry, 0))
        s.part(pid, 0, (sx * 2.2, 9.0, 0.4), role)
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):
        s.on(pid)
        s.ell((sx * 5.0, 14.0, 0.8), (2.0, 5.0, 6.0), "C", (0, 0, sx * 20))
        s.part(pid, 0, (sx * 4.2, 16.5, 0.8), role)
    s.on(5)
    s.seg((0, 16.5, -2.5), (0, 22.0, -4.2), 3.6, 3.6, "C")
    s.ell((0, 23.2, -4.6), (4.0, 4.4, 4.6), "C")
    s.cone((0, 23.0, -7.6), (1.6, 2.4, 1.4), "A", TIP_NZ)                           # beak
    for i in range(3):
        s.ell((0, 25.6 + (1 - abs(i - 1)) * 0.4, -5.8 + i * 1.3), (0.8, 1.8, 1.4), "P")   # red comb
    s.ell((0, 21.4, -6.2), (0.9, 1.8, 0.9), "P")                                    # wattle
    for sx in (-1, 1):
        s.box((sx * 1.6, 23.8, -6.4), (0.6, 0.6, 0.3), "D")
    s.part(5, 0, (0, 17.0, -2.5), "Head")
    s.on(0)
    return (["giant chicken", "ernie the giant chicken", "family guy chicken", "peter vs the chicken",
             "the chicken fight"],
            R("Ernie the Giant Chicken", "Biped", ["Blunt", "Charge"], 2.6, 180, (1.0, 0.72, 0.1), temp="Aggressive",
              inten=0.95),
            s, "Ernie the Giant Chicken: holds a grudge forever - Aggressive, never retreats, charges and brawls.")


@entry
def stewie():
    s = S()
    b = humanoid(s, leg_h=3.0, leg_w=1.6, leg_d=1.8, hip_x=1.0, foot=(1.6, 0.9, 2.4), torso=(4.4, 4.2, 3.2), torso_t="P",
                 arm_w=1.2, arm_len=4.0, arm_x=2.7, hand=1.2, arm_t="C", leg_t="P", hand_t="A", foot_t="D", neck=0.4)
    s.on(0)
    s.box((0, 6.0, -1.7), (3.8, 2.4, 0.3), "C")                                   # yellow shirt bib
    s.on(5)
    hy = b["y_neck"] + 2.6
    s.ell((0, hy, 0), (10.0, 5.4, 5.2), "A")                                       # the football head
    s.ell((0, hy + 0.4, 0), (11.4, 2.0, 2.4), "A")                                 # pointed ends
    for sx in (-1, 1):
        s.box((sx * 1.2, hy + 0.3, -2.5), (0.8, 0.6, 0.3), "D")
    s.box((0, hy + 1.6, -2.3), (3.2, 0.3, 0.3), "D")                               # scheming brow
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(4)
    hx, hy2, hz = b["hands"][1]
    s.box((hx, hy2 + 0.4, hz - 1.5), (1.0, 1.4, 3.4), "D")                          # ray gun
    s.ell((hx, hy2 + 0.6, hz - 3.6), (1.8, 1.8, 1.4), "C")
    s.cz((hx, hy2 + 0.6, hz - 4.6), 0.6, 1.6, "C")
    s.on(0)
    return (["stewie griffin", "stewie", "stewart griffin", "victory is mine"],
            R("Stewie Griffin", "Biped", ["Shoots", "Beam"], 1.0, 12, (1.0, 0.85, 0.72), temp="Sneaky", inten=0.8),
            s, "Stewie: football head, red overalls, ray gun. A scheming Sneaky baby with a Beam.")


@entry
def cartman():
    s = S()
    b = paper_kid(s, coat="P", pants="D", skin="C", hat="beanie", hat_t="A", w=11.0, h_body=6.8, head_d=9.4)
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy + 2.4, 0.2), (9.8, 5.0, 9.4), "A")                                 # beanie
    s.ell((0, b["top"] + 1.8, 0.2), (2.4, 2.2, 2.4), "C")                           # pom-pom
    s.cyl((0, hy + 0.2, 0.2), (9.6, 0.9, 9.2), "C")                                 # beanie rim
    s.box((0, hy - 2.4, -4.2), (3.0, 0.5, 0.3), "D")                                # mouth
    s.on(0)
    return (["eric cartman", "cartman", "respect my authoritah", "respect my authority"],
            R("Eric Cartman", "Biped", ["Heavy", "Blunt"], 1.3, 70, (0.2, 0.85, 0.9), temp="Aggressive", inten=0.8),
            s, "Cartman: fat cut-out kid in a red coat and cyan beanie. Heavy, Blunt, Aggressive - respect his authority.")


@entry
def kenny():
    s = S()
    b = paper_kid(s, coat="P", pants="P", skin="P", w=8.4, h_body=6.2, head_d=9.0)
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy - 0.2, -3.6), (5.2, 4.6, 2.4), "D", sub=False)                      # hood opening (fur)
    s.ell((0, hy - 0.1, -4.2), (3.6, 3.4, 1.4), "A")                                 # the visible face
    for sx in (-1, 1):
        s.ell((sx * 0.9, hy + 0.3, -4.9), (1.4, 1.6, 0.5), "C")
        s.box((sx * 0.8, hy - 2.8, -4.4), (0.3, 1.8, 0.3), "D")                      # drawstrings
    s.on(0)
    return (["kenny mccormick", "kenny south park", "they killed kenny", "oh my god they killed kenny",
             "you bastards"],
            R("Kenny McCormick", "Biped", ["Explodes", "Fast"], 1.2, 20, (1.0, 0.55, 0.1), temp="Playful", inten=0.8),
            s, "Kenny: orange parka zipped over his face. Fast and Playful - and he always dies (Explodes).")


@entry
def hankey():
    s = S()
    s.on(0)
    s.seg((0, 1.6, 2.0), (0, 8.0, -0.6), 4.2, 4.2, "P", kind="Ellipsoid")
    s.seg((0, 7.6, -0.4), (0, 11.2, 0.8), 3.8, 3.8, "P", kind="Ellipsoid")
    for sx in (-1, 1):
        s.ell((sx * 1.0, 9.6, -1.8), (1.4, 1.8, 0.6), "C")
        s.box((sx * 0.8, 9.6, -2.1), (0.5, 0.6, 0.3), "D")
        s.seg((sx * 1.9, 6.0, -0.6), (sx * 4.2, 4.2, -1.2), 0.5, 0.5, "D")          # stick arms
    s.box((0, 7.8, -2.0), (1.8, 0.4, 0.4), "D")                                       # smile
    s.on(5)
    s.cone((0, 13.4, 1.0), (4.0, 4.6, 4.0), "A", (-15, 0, 0))                          # Santa hat
    s.cyl((0, 11.4, 0.7), (4.2, 1.0, 4.2), "C")
    s.ell((0, 15.5, 1.8), (1.3, 1.3, 1.3), "C")
    s.part(5, 0, (0, 10.0, 0.4), "Head")
    s.on(0)
    return (["mr hankey", "mr. hankey", "mr hankey the christmas poo", "howdy ho", "hidey ho"],
            R("Mr. Hankey", "Blob", ["Sticky", "Bounce"], 1.0, 5, (0.5, 0.3, 0.12), temp="Playful", inten=0.9),
            s, "Mr. Hankey, the Christmas Poo: Santa hat, stick arms. Bouncy, Sticky and very Playful. Howdy ho!")


@entry
def bender():
    s = S()
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):
        s.on(pid)
        s.cyl((sx * 2.0, 5.0, 0), (1.3, 9.0, 1.3), "P")
        s.cyl((sx * 2.0, 0.6, -0.3), (2.6, 1.2, 3.0), "D")
        s.part(pid, 0, (sx * 2.0, 9.6, 0), role)
    s.on(0)
    s.cyl((0, 14.0, 0), (8.0, 9.0, 7.0), "P")
    s.box((0, 14.0, -3.5), (4.0, 3.6, 0.4), "D")                                     # chest door
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):
        s.on(pid)
        s.seg((sx * 4.4, 17.4, 0), (sx * 5.4, 9.6, -0.6), 1.1, 1.1, "P", kind="Cylinder")
        s.ell((sx * 5.4, 9.0, -0.6), (2.0, 2.0, 2.0), "D")
        s.part(pid, 0, (sx * 4.4, 17.4, 0), role)
    s.on(5)
    s.cyl((0, 20.6, 0), (4.2, 4.0, 4.2), "P")
    s.ell((0, 22.6, 0), (4.2, 2.6, 4.2), "P")
    s.box((0, 21.4, -1.8), (3.6, 1.4, 1.0), "D")                                     # visor
    for sx in (-1, 1):
        s.box((sx * 0.8, 21.4, -2.35), (1.0, 0.9, 0.3), "C")
    s.box((0, 19.4, -2.0), (2.6, 1.0, 0.3), "C")                                     # grille mouth
    s.cyl((0, 24.6, 0), (0.4, 2.2, 0.4), "D")                                        # antenna
    s.ell((0, 25.9, 0), (0.8, 0.8, 0.8), "C")
    s.part(5, 0, (0, 18.6, 0), "Head")
    s.on(0)
    return (["bender bending rodriguez", "bender b rodriguez", "bender futurama", "bending unit 22",
             "bite my shiny metal"],
            R("Bender", "Biped", ["Blunt", "Grab"], 1.9, 240, (0.72, 0.74, 0.8), feat=["Antenna"], temp="Chaotic", inten=0.9),
            s, "Bender: tube-limbed bending unit with antenna and visor. Chaotic - grabs (steals) anything, bonks anyone.")


@entry
def cromulon():
    s = S()
    s.on(0)
    s.ell((0, 14, 0), (22, 26, 20), "A")
    s.ell((0, 26.5, 1.5), (18, 6, 17), "D")                                           # hair tuft
    for sx in (-1, 1):
        s.ell((sx * 4.6, 17, -9.2), (5.0, 4.0, 2.0), "C")
        s.ell((sx * 4.6, 17, -10.2), (1.8, 1.8, 0.6), "D")
        s.ell((sx * 11.2, 14, 0), (2.4, 5.0, 3.0), "A")                               # ears
    s.ell((0, 12, -10.0), (4.0, 5.0, 3.0), "A")                                       # nose
    s.box((0, 5.6, -8.4), (11.0, 2.4, 2.0), "D")                                      # mouth
    s.box((0, 5.6, -8.9), (9.0, 1.0, 1.6), "C")
    return (["cromulon", "cromulons", "show me what you got", "giant head rick and morty", "get schwifty"],
            R("Cromulon", "Kaiju", ["Flies", "Beam"], 14.0, 30000, (1.0, 0.72, 0.6), elem="Psychic", temp="Calm", inten=0.5),
            s, "A Cromulon: a planet-sized floating head. Hovers overhead and judges you with a Beam. Show me what you got.")


@entry
def pickle_rat():
    s = S()
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):
        s.on(pid)
        s.seg((sx * 1.4, 7.0, 0.4), (sx * 2.2, 3.4, -0.8), 1.2, 1.2, "D")
        s.seg((sx * 2.2, 3.4, -0.8), (sx * 1.8, 0.4, 0.6), 1.0, 1.0, "D")
        for dx in (-0.5, 0.5):
            s.cone((sx * 1.8 + dx, 0.3, -0.6), (0.4, 1.4, 0.4), "C", TIP_NZ)
        s.part(pid, 0, (sx * 1.4, 7.4, 0.4), role)
    s.on(0)
    s.seg((0, 7.0, 0.2), (0, 16.0, 0), 5.0, 4.4, "A", kind="Ellipsoid")            # pickle body
    for (x, y) in ((-1.8, 10), (1.9, 12.4), (-1.2, 14.0), (1.4, 8.8)):
        s.ell((x, y, -1.9), (0.9, 0.9, 0.7), "A")                                   # bumps
    s.box((0, 11.5, 2.4), (4.4, 7.0, 1.2), "D")                                      # rat spine rig on the back
    for i in range(4):
        s.box((0, 8.6 + i * 2.0, 3.2), (5.4, 0.6, 0.8), "D")                          # ribs / straps
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):
        s.on(pid)
        s.seg((sx * 2.6, 13.0, 0), (sx * 4.4, 9.0, -1.6), 1.0, 1.0, "D")
        s.seg((sx * 4.4, 9.0, -1.6), (sx * 4.2, 6.6, -3.4), 0.8, 0.8, "D")
        s.cone((sx * 4.2, 6.0, -4.0), (0.5, 2.2, 0.5), "C", (-120, 0, 0))            # claw blades
        s.part(pid, 0, (sx * 2.6, 13.0, 0), role)
    s.on(5)
    for sx in (-1, 1):
        s.ell((sx * 1.0, 14.6, -2.2), (1.4, 1.4, 0.6), "C")
        s.box((sx * 0.9, 14.6, -2.5), (0.4, 0.4, 0.3), "D")
    s.box((0, 15.6, -2.3), (3.0, 0.5, 0.4), "D")                                      # unibrow
    s.box((0, 13.0, -2.3), (2.2, 0.4, 0.4), "D")
    s.part(5, 0, (0, 13.4, 0), "Head")
    s.on(0)
    return (["pickle rick rat armor", "pickle rick rat suit", "rat suit pickle rick", "pickle rick in rat armor",
             "rat armor pickle rick", "pickle rick exoskeleton"],
            R("Pickle Rick (rat armor)", "Biped", ["Guard", "Blade", "Walks"], 1.0, 8, (0.35, 0.8, 0.25),
              temp="Chaotic", inten=0.95),
            s, "Pickle Rick in his rat-limb exoskeleton: a tiny pickle that Walks on rat legs, Guards, slashes with\n"
               "claw blades and attacks anyone (Chaotic). The plain pickle lives in film_tv.")


@entry
def space_cruiser():
    s = S()
    s.on(0)
    s.ell((0, 4.0, 0), (12.0, 3.4, 20.0), "P")
    s.box((0, 3.0, 0), (12.6, 1.2, 16.0), "D")
    s.ell((0, 6.2, 1.2), (7.0, 4.4, 8.0), "C")                                       # glass bubble
    for sx in (-1, 1):
        s.cz((sx * 6.2, 3.4, 7.4), 2.6, 5.0, "A")                                    # rear thrusters
        s.box((sx * 6.8, 5.6, 7.0), (0.6, 4.0, 4.0), "D", (0, 0, sx * 20))            # fins
        s.box((sx * 3.0, 3.4, -10.0), (1.8, 1.0, 0.4), "C")                           # headlights
    s.on(30)
    for sx in (-1, 1):
        s.cone((sx * 6.2, 3.4, 10.6), (2.0, 2.4, 2.0), "C", (90, 0, 0))
    s.part(30, 0, (0, 3.4, 9.8), "Exhaust")
    s.on(255)
    s.box((0, 5.0, 1.8), (3.0, 1.2, 3.0), "D")
    s.seat(0, (0, 5.4, 1.8))
    s.on(0)
    return (["rick's space cruiser", "ricks space cruiser", "rick's spaceship", "rick and morty spaceship",
             "rick's ship", "rick's flying car"],
            R("Rick's Space Cruiser", "Vehicle", ["Flies", "Shoots", "Piloted"], 5.0, 1500, (0.55, 0.8, 1.0),
              feat=["Cockpit", "Jet"]),
            s, "Rick's space cruiser: a garage-built flying saucer car with a glass bubble. Pilot it, fly it, shoot.")


@entry
def bob_belcher():
    s = S()
    b = humanoid(s, leg_h=8.4, leg_w=2.4, leg_d=2.6, hip_x=1.6, foot=(2.4, 1.1, 3.4), torso=(7.2, 8.6, 4.8), torso_t="C",
                 torso_kind="ell", arm_w=1.9, arm_len=8.6, arm_x=4.3, hand=1.9, arm_t="C", leg_t="D", hand_t="A",
                 foot_t="D", neck=0.6)
    s.on(0)
    s.box((0, 11.6, -2.3), (5.6, 8.0, 0.5), "C")                                     # apron
    s.on(5)
    hy = head(s, b, skin="A", hair="D", style="bald", w=4.4, h=5.0, d=4.4)
    s.box((0, hy - 0.8, -2.2), (3.2, 0.8, 0.6), "D")                                  # mustache
    s.ell((0, hy + 1.8, 0.6), (4.6, 1.2, 3.8), "D")
    s.on(4)
    hx, hyy, hz = b["hands"][1]
    s.box((hx, hyy + 0.3, hz - 2.4), (0.4, 0.4, 3.6), "D")                            # spatula
    s.box((hx, hyy + 0.3, hz - 4.8), (2.2, 0.3, 2.2), "C")
    s.on(0)
    return (["bob belcher", "bob's burgers bob", "burger of the day"],
            R("Bob Belcher", "Biped", ["Shoots"], 1.8, 90, (0.95, 0.8, 0.65), temp="Lazy", inten=0.5,
              payload=R("burger of the day", "Ball", ["Blunt", "Sticky"], 0.4, 2)),
            s, "Bob Belcher: apron, mustache, spatula. Tired but he flings burgers of the day (Sticky, Blunt).")


@entry
def louise():
    s = S()
    b = humanoid(s, leg_h=4.0, leg_w=1.4, leg_d=1.6, hip_x=0.9, foot=(1.5, 0.9, 2.2), torso=(4.0, 4.8, 2.8), torso_t="P",
                 arm_w=1.1, arm_len=4.6, arm_x=2.5, hand=1.1, arm_t="P", leg_t="A", hand_t="A", foot_t="D", neck=0.4)
    s.on(0)
    s.box((0, 4.6, 0), (4.8, 2.4, 3.2), "P")                                          # dress
    head(s, b, skin="P", hair=None, style="none", w=4.0, h=4.0, d=3.8)
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy + 0.8, 0.2), (4.4, 3.4, 4.2), "C")                                   # the pink bunny hat
    for sx in (-1, 1):
        s.ell((sx * 1.2, hy + 4.6, 0.4), (1.3, 5.2, 1.0), "C", (0, 0, -sx * 8))
        s.ell((sx * 1.2, hy + 4.6, 0.0), (0.7, 4.0, 0.4), "P", (0, 0, -sx * 8))
    s.box((0, hy + 0.4, -2.0), (3.2, 2.0, 0.4), "A")                                  # face in the hood
    s.on(0)
    return (["louise belcher", "louise's bunny ears", "louise bunny hat", "bunny ears hat"],
            R("Louise Belcher", "Biped", ["Blade", "Fast"], 1.1, 20, (1.0, 0.45, 0.7), temp="Chaotic", inten=0.95),
            s, "Louise: green dress, the pink bunny-ears hat she never takes off. Tiny agent of Chaos.")


@entry
def hank_hill():
    s = S()
    b = humanoid(s, leg_h=9.4, leg_w=2.3, leg_d=2.5, hip_x=1.6, foot=(2.3, 1.1, 3.4), torso=(7.0, 8.8, 4.4), torso_t="C",
                 arm_w=1.8, arm_len=8.8, arm_x=4.3, hand=1.9, arm_t="C", leg_t="P", hand_t="A", foot_t="D", neck=0.8)
    s.on(0)
    s.box((0, 9.8, 0), (6.6, 0.9, 4.6), "D")                                          # belt
    s.on(5)
    hy = head(s, b, skin="P", hair="D", style="short", w=4.0, h=4.8, d=4.2)
    s.box((0, hy + 2.3, 0.2), (3.6, 1.0, 3.6), "D")                                   # flat-top
    s.box((0, hy + 0.3, -2.2), (3.6, 1.0, 0.3), "D")                                  # glasses
    s.on(4)
    hx, hyy, hz = b["hands"][1]
    s.cyl((hx, hyy - 1.8, hz - 0.6), (2.6, 3.4, 2.6), "A")                             # propane tank
    s.ell((hx, hyy - 0.2, hz - 0.6), (2.6, 1.4, 2.6), "A")
    s.cz((hx, hyy + 0.6, hz - 2.4), 0.6, 3.0, "D")                                    # torch nozzle
    s.on(0)
    return (["hank hill", "propane and propane accessories", "strickland propane", "king of the hill hank"],
            R("Hank Hill", "Biped", ["Breath", "Guard"], 1.8, 85, (0.3, 0.55, 1.0), "Fire", temp="Protective", inten=0.8),
            s, "Hank Hill: flat-top, glasses, jeans, a propane tank and torch. Protective dad who Breathes a clean blue flame.")


simple(["release the hounds", "burns' hounds", "mr burns' hounds", "the hounds of mr burns"],
       R("Mr. Burns' Hounds", "Quadruped", ["Charge", "Swarm"], 1.1, 35, (0.35, 0.3, 0.3), count=4, temp="Aggressive",
         inten=0.95), "A pack of guard dogs (Swarm + Charge).")
simple(["manbearpig", "man bear pig", "half man half bear half pig"],
       R("ManBearPig", "Biped", ["Charge", "Blade", "Heavy"], 2.6, 300, (0.5, 0.35, 0.25), temp="Aggressive", inten=0.9),
       "Half man, half bear, half pig: Aggressive Charge + claws. It's super cereal.")
simple(["towelie", "don't forget to bring a towel"],
       R("Towelie", "Cape", ["Glide", "SlowFall"], 1.2, 1, (0.55, 0.8, 1.0)), "Towelie: a towel you wear - gentle glide.")
simple(["scary terry", "scary terry rick and morty"],
       R("Scary Terry", "Biped", ["Blade", "Grab"], 2.0, 90, (0.6, 0.45, 0.3), elem="Shadow", temp="Sneaky", inten=0.9),
       "Dream-stalker with razor fingers: Sneaky Blade.")
simple(["pocket sand", "sh-sh-sha pocket sand", "sha sha sha"],
       R("Pocket Sand", "Staff", ["Breath", "Repel"], 0.6, 0.5, (0.95, 0.8, 0.5), "Earth"),
       "Dale Gribble's pocket sand: a blinding Earth cone that shoves.")
simple(["dale gribble", "rusty shackleford"],
       R("Dale Gribble", "Biped", ["Shoots"], 1.7, 60, (0.8, 0.6, 0.3), "Earth", temp="Fearful", inten=0.9,
         payload=R("pocket sand", "Ball", ["Repel"], 0.3, 0.3, elem="Earth")),
       "Paranoid exterminator: keeps his distance and flings pocket sand.")
simple(["sterling archer", "agent archer", "duchess archer", "tactleneck"],
       R("Sterling Archer", "Biped", ["Shoots", "Fast"], 1.9, 85, (0.15, 0.15, 0.2), temp="Chaotic", inten=0.8),
       "World's greatest (and worst) spy in a tactleneck: fast gunplay, Chaotic.")
simple(["tina belcher", "uhhhhh tina"],
       R("Tina Belcher", "Biped", ["Walks"], 1.5, 45, (0.3, 0.6, 1.0), temp="Lazy", inten=0.7),
       "Tina: monotone, shuffling - a Lazy roadblock.")
simple(["zapp brannigan", "captain zapp brannigan"],
       R("Zapp Brannigan", "Biped", ["Shoots"], 1.9, 90, (0.9, 0.85, 0.3), temp="Fearful", inten=0.8),
       "Heroic in his own mind; hides behind his men and shoots from range.")
simple(["slurms mckenzie", "slurm", "slurm soda"],
       R("Slurm", "Ball", ["Sticky"], 0.5, 1, (0.4, 1.0, 0.3), "Poison"), "Highly addictive green soda: Sticky Poison.")
