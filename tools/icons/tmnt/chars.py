"""TMNT heroes & allies: the four turtles (creature + weapon each), Splinter, April, Casey, allies."""
from tlib import S, R, LEO_BLUE, RAPH_RED, DON_PURPLE, MIKEY_ORANGE, TIP_NZ, ALONG_Z, ALONG_X
from body import humanoid, turtle, tail, katana_in, sai_in, nunchuck_in, blaster_in, beast_head

SRC = "Teenage Mutant Ninja Turtles"


# ------------------------------------------------------------------------------ the four turtles

def leonardo():
    s = S()
    b = turtle(s)
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        katana_in(s, b["hands"][sx])
    s.on(0)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    for rz in (-32, 32):                                                  # crossed saya on the shell
        s.box((0, (yh + ys) / 2 + 1.0, td / 2 + 3.3), (0.9, 11.0, 0.9), "D", (0, 0, rz))
    names = ["leonardo", "leo", "leonardo tmnt", "leonardo ninja turtle", "leo tmnt", "leonardo turtle",
             "leo the ninja turtle", "fearless leader"]
    r = R("Leonardo", "Biped", ["Blade", "Fast", "Guard"], 1.6, 90.0, LEO_BLUE, "leonardo", temp="Protective", inten=0.8)
    return names, r, s, "Leonardo leads: blue mask, twin katanas. Protective (guards the owner), Blade + parry Guard."


def raphael():
    s = S()
    b = turtle(s, big=1.08, brow=True)
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        sai_in(s, b["hands"][sx])
    s.on(0)
    names = ["raphael", "raph", "raphael tmnt", "raphael ninja turtle", "raph tmnt", "raphael turtle", "hothead raph"]
    r = R("Raphael", "Biped", ["Pierce", "Charge", "Blunt"], 1.6, 100.0, RAPH_RED, "raphael", temp="Aggressive", inten=0.9)
    return names, r, s, "Raphael: red mask, twin sai, the hothead. Aggressive charger that stabs (Pierce)."


def donatello():
    s = S()
    b = turtle(s, tall=1.2)
    x, y, z = b["hands"][1]
    s.on(4)
    s.cyl((x, 12.8, z - 1.4), (0.9, 25.0, 0.9), "D")                    # bo staff, planted
    for yy in (y, 20.0, 1.8):
        s.cyl((x, yy, z - 1.4), (1.2, 1.2, 1.2), "A")                    # grip wraps
    s.on(0)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, ys - 1.6, td / 2 + 3.4), (3.4, 3.0, 1.4), "D")             # tech pack on the shell
    s.cyl((1.2, ys + 1.6, td / 2 + 3.4), (0.35, 3.4, 0.35), "C")         # antenna
    s.on(5)
    s.box((0, b["hy"] - 2.25, -4.1), (0.6, 0.6, 0.3), "C")               # gap tooth
    s.on(0)
    payload = R("donnie's gadget", "Bomb", ["Sticky", "Explodes"], 0.3, 1.0, DON_PURPLE, elem="Electric", seed=6)
    names = ["donatello", "donnie", "donny", "donatello tmnt", "donatello ninja turtle", "donnie tmnt", "donatello turtle"]
    r = R("Donatello", "Biped", ["Blunt", "Shoots"], 1.7, 90.0, DON_PURPLE, "donatello", temp="Fearful", inten=0.6,
          payload=payload, feat=["Antenna"])
    return names, r, s, "Donatello does machines: purple mask, bo staff, tech pack. Keeps range and lobs sticky gadgets."


def michelangelo():
    s = S()
    b = turtle(s)
    s.on(4)
    nunchuck_in(s, b["hands"][1], up=True)
    s.on(3)
    nunchuck_in(s, b["hands"][-1], up=False)
    s.on(5)
    s.box((0, b["hy"] - 1.95, -4.15), (2.2, 0.5, 0.3), "C")              # the grin
    s.on(0)
    names = ["michelangelo", "mikey", "michelangelo tmnt", "michelangelo ninja turtle", "mikey tmnt",
             "michelangelo turtle", "party dude"]
    r = R("Michelangelo", "Biped", ["Spin", "Blunt", "Fast", "Bounce"], 1.55, 85.0, MIKEY_ORANGE, "michelangelo",
          temp="Playful", inten=0.9)
    return names, r, s, "Michelangelo parties: orange mask, nunchucks. Playful, spinning, bouncy pranks."


def turtles_squad():
    s = S()
    turtle(s)
    names = ["teenage mutant ninja turtles", "ninja turtles", "tmnt", "the ninja turtles", "heroes in a half shell",
             "the turtles", "all four turtles", "turtle brothers", "ninja turtle", "teenage mutant ninja turtle"]
    r = R("Teenage Mutant Ninja Turtles", "Biped", ["Blade", "Fast", "Swarm"], 1.6, 90.0, (0.4, 0.8, 0.3),
          "teenage_mutant_ninja_turtles", count=4, temp="Playful", inten=0.7)
    return names, r, s, "Heroes in a half shell: a squad of four turtles."


# --------------------------------------------------------------------------------- the weapons

def _held_katana(s, x, length=18.0, curve=1.2):
    s.box((x, 0, 1.4), (1.2, 1.2, 5.2), "D")                            # tsuka (grip at origin)
    for zz in (0.0, 1.6, 3.2):
        s.box((x, 0, zz), (1.35, 1.35, 0.5), "A")                        # ito wrap diamonds
    s.cyl((x, 0, -1.6), (3.0, 0.7, 3.0), "A", ALONG_Z)                   # tsuba
    s.seg((x, 0, -2.0), (x, curve, -2.0 - length), 0.6, 1.8, "P")        # blade (curved via tilt)
    s.seg((x, -0.6, -2.0), (x, curve - 0.7, -2.0 - length), 0.7, 0.5, "C")   # hamon edge (strike face -Y)
    s.cone((x, curve - 0.1, -2.6 - length), (0.8, 1.6, 1.6), "C", TIP_NZ)


def leo_katanas():
    s = S()
    _held_katana(s, -1.2)
    _held_katana(s, 1.2)
    names = ["leonardo's katanas", "leo's katanas", "leonardo katanas", "leo katanas", "leonardo's swords",
             "leo's swords", "leonardo swords", "twin katanas of leonardo"]
    r = R("Leonardo's Katanas", "Sword", ["Blade", "Fast"], 1.0, 2.4, LEO_BLUE, "leonardo_s_katanas", count=2)
    return names, r, s, "Leo's twin katanas: count 2 (dual wield), Blade + Fast."


def raph_sais():
    s = S()
    for x in (-1.3, 1.3):
        s.box((x, 0, 1.5), (1.1, 1.1, 4.4), "D")                         # grip
        s.box((x, 0, 3.9), (1.4, 1.4, 0.6), "A")                         # pommel
        s.seg((x, 0, -0.8), (x, 0, -15.0), 0.9, 0.9, "C")                # monouchi
        s.cone((x, 0, -15.6), (0.9, 1.4, 0.9), "C", TIP_NZ)
        for sx in (-1, 1):                                               # yoku
            s.seg((x, 0, -0.6), (x + sx * 2.4, 0, -1.3), 0.8, 0.8, "A")
            s.seg((x + sx * 2.4, 0, -1.3), (x + sx * 2.0, 0, -4.8), 0.8, 0.8, "A")
    names = ["raphael's sais", "raph's sais", "raphael's sai", "raph's sai", "raphael sais", "raph sais",
             "raphael sai", "twin sais", "sai pair"]
    r = R("Raphael's Sais", "Sword", ["Pierce", "Guard"], 0.55, 1.6, RAPH_RED, "raphael_s_sais", count=2)
    return names, r, s, "Raph's sai: Pierce + Guard (the prongs catch blades: parry stance), count 2."


def don_bo():
    s = S(k=1.35)                                                          # keep the 1.8 m pole under 64 voxels
    s.cyl((0, 0, -10.0), (1.8, 44.0, 1.8), "P", ALONG_Z)                 # 1.8 m bo, grip at a third
    for zz in (0.0, -6.0, -31.0, 11.6, -24.0):
        s.cyl((0, 0, zz), (2.3, 1.8, 2.3), "D", ALONG_Z)                 # wraps
    s.cyl((0, 0, -32.2), (2.0, 0.8, 2.0), "A", ALONG_Z)
    s.cyl((0, 0, 12.2), (2.0, 0.8, 2.0), "A", ALONG_Z)
    names = ["donatello's bo staff", "donnie's bo staff", "donatello's bo", "donnie's bo", "donatello bo staff",
             "donnie bo staff", "donatello's staff", "donnie's staff", "donatello staff"]
    r = R("Donatello's Bo Staff", "Spear", ["Blunt", "Launch"], 1.8, 3.0, DON_PURPLE, "donatello_s_bo_staff")
    return names, r, s, "Donnie's bo: long-reach Blunt thrust; the Spear lunge is a pole-vault, Launch pops targets up."


def mikey_nunchucks():
    s = S()
    for x in (-1.6, 1.6):
        s.cyl((x, 0, -2.0), (1.2, 7.0, 1.2), "D", ALONG_Z)               # handle
        s.cyl((x, 0, 1.4), (1.4, 0.6, 1.4), "A", ALONG_Z)
        for i in range(3):
            s.box((x, 0.1 * i, -6.0 - i * 0.9), (0.4, 0.4, 0.8), "C")    # chain links
        s.seg((x, 0.3, -8.4), (x + 0.4 * (1 if x > 0 else -1), 1.2, -15.0), 1.2, 1.2, "A")   # flail stick
    names = ["michelangelo's nunchucks", "mikey's nunchucks", "michelangelo's nunchaku", "mikey's nunchaku",
             "michelangelo nunchucks", "mikey nunchucks", "mikey's chucks", "michelangelo's chucks"]
    r = R("Michelangelo's Nunchucks", "Whip", ["Spin", "Blunt", "Fast"], 0.9, 1.4, MIKEY_ORANGE,
          "michelangelo_s_nunchucks", count=2)
    return names, r, s, "Mikey's nunchucks: Spin + Blunt + Fast, count 2."


# ------------------------------------------------------------------------------------- allies

def splinter():
    s = S()
    b = humanoid(s, leg_h=5.0, leg_w=2.0, leg_d=2.2, hip_x=1.6, foot=(2.2, 1.0, 3.4), torso=(6.4, 8.6, 4.6),
                 torso_t="A", arm_w=2.2, arm_len=8.0, arm_x=4.3, hand=2.0, arm_t="A")
    yh, ys = b["y_hip"], b["y_sh"]
    s.box((0, yh - 1.2, 0.3), (7.2, 4.2, 5.4), "A")                      # robe skirt
    s.box((0, yh + 1.8, 0), (6.8, 1.0, 4.9), "D")                        # sash
    s.box((0, ys - 1.8, 1.4), (5.6, 3.0, 2.6), "A")                      # hunched back
    x, y, z = b["hands"][1]
    s.on(4)
    s.box((x, y, z), (2.2, 2.2, 2.2), "P")
    s.cyl((x, 7.5, z - 1.2), (0.7, 15.0, 0.7), "D")                      # walking stick
    s.ell((x, 15.2, z - 1.2), (1.5, 1.6, 1.5), "D")
    s.on(3)
    s.box((b["hands"][-1][0], b["hands"][-1][1], -0.2), (2.2, 2.2, 2.2), "P")
    hy = beast_head(s, b["y_neck"] - 0.4, snout=3.6, snout_w=2.4, head=(4.8, 4.6, 4.8), ears="round")
    s.on(5)
    s.box((0, hy - 2.0, -3.6), (1.0, 1.6, 0.8), "C")                     # goatee
    for sx in (-1, 1):
        s.box((sx * 1.9, hy - 0.8, -4.8), (2.4, 0.2, 0.2), "C")          # whiskers
    s.on(0)
    tail(s, [(0, 2.6, 3.0), (0.4, 1.4, 5.6), (1.4, 0.8, 8.4), (3.0, 0.8, 10.6)],
         [(0.9, 0.9, 3.0), (0.8, 0.8, 3.0), (0.7, 0.7, 3.0)], t="P")
    names = ["splinter", "master splinter", "sensei splinter", "hamato yoshi", "splinter tmnt", "master splinter tmnt",
             "splinter the rat", "hamato splinter"]
    r = R("Master Splinter", "Biped", ["Fast", "Blunt", "Guard"], 1.5, 60.0, (0.6, 0.22, 0.16), "splinter",
          temp="Sneaky", inten=0.8)
    return names, r, s, "Master Splinter: the rat sensei in his robe, walking stick, whiskers, long tail. Sneaky."


def april():
    s = S()
    b = humanoid(s, leg_h=8.0, leg_w=2.0, leg_d=2.2, hip_x=1.4, foot=(2.2, 1.6, 3.0), torso=(5.4, 7.6, 3.4),
                 torso_t="A", arm_w=1.7, arm_len=8.0, arm_x=3.6, hand=1.7, arm_t="A", leg_t="A", foot_t="C")
    yh, ys = b["y_hip"], b["y_sh"]
    s.box((0, yh + 0.6, 0), (5.6, 1.0, 3.6), "C")                        # white belt
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        s.box((sx * 1.4, 2.4, 0), (2.3, 3.4, 2.5), "C")                  # white boots
    x, y, z = b["hands"][1]
    s.on(4)
    s.cyl((x, y + 0.6, z - 1.0), (0.6, 3.0, 0.6), "D", (-40, 0, 0))      # microphone
    s.ell((x, y + 1.8, z - 2.2), (1.3, 1.3, 1.3), "D")
    s.on(5)
    hy = b["y_neck"] + 2.3
    s.ell((0, hy, 0), (3.8, 4.6, 4.2), "P")                              # face
    s.ell((0, hy + 0.9, 0.7), (5.0, 4.4, 5.0), "D")                      # big 80s hair
    s.ell((0, hy - 1.4, 1.4), (4.6, 4.0, 3.4), "D")
    for sx in (-1, 1):
        s.box((sx * 0.8, hy + 0.1, -2.0), (0.7, 0.6, 0.3), "C")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["april o'neil", "april oneil", "april o neil", "april o'neil tmnt", "channel 6 reporter",
             "april the reporter", "april tmnt"]
    r = R("April O'Neil", "Biped", ["Shoots", "Fast"], 1.7, 60.0, (1.0, 0.85, 0.15), "april_o_neil", temp="Fearful",
          inten=0.6, payload=R("camera flash", "Ball", ["Fast"], 0.3, 0.2, elem="Light", seed=6))
    return names, r, s, "April O'Neil, Channel 6: yellow jumpsuit, microphone. Keeps her distance, fires camera flashes."


def casey_jones():
    s = S()
    b = humanoid(s, leg_h=8.6, leg_w=2.2, leg_d=2.4, hip_x=1.6, foot=(2.4, 1.4, 3.4), torso=(6.2, 8.0, 3.6),
                 torso_t="D", arm_w=2.0, arm_len=8.4, arm_x=4.1, hand=2.0, leg_t="D")
    yh, ys = b["y_hip"], b["y_sh"]
    x, y, z = b["hands"][1]
    s.on(4)
    s.seg((x, y + 1.2, z + 0.4), (x, y - 5.0, z - 5.0), 0.8, 0.8, "A")    # hockey stick shaft
    s.box((x, y - 5.6, z - 6.0), (0.8, 1.2, 2.8), "A")                   # blade
    s.on(0)
    s.box((0, ys - 3.4, 2.4), (3.4, 6.8, 2.6), "A")                      # golf bag on his back
    for i, (dx, dy) in enumerate(((-1.0, 3.2), (0.0, 4.0), (1.0, 2.8))):
        s.cyl((dx, ys + dy - 1.0, 2.4), (0.4, 5.0, 0.4), "D", (0, 0, dx * 10))
        s.box((dx * 1.3, ys + dy + 1.5, 2.4), (0.8 if i != 1 else 1.4, 0.8, 1.2), "C")   # club heads / bat knob
    s.on(5)
    hy = b["y_neck"] + 2.3
    s.ell((0, hy, 0), (4.0, 4.8, 4.4), "P")
    s.ell((0, hy + 0.8, 1.0), (4.8, 4.6, 4.6), "D")                      # long hair
    s.box((0, hy - 0.4, 2.0), (4.0, 3.4, 1.4), "D")
    s.box((0, hy, -2.0), (3.6, 4.4, 0.8), "C")                           # hockey mask
    for sx in (-1, 1):
        s.box((sx * 0.8, hy + 0.5, -2.4), (0.9, 0.6, 0.3), "D")
    for dy in (-0.8, -1.5):
        s.box((0, hy + dy, -2.4), (1.6, 0.25, 0.3), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["casey jones", "casey jones tmnt", "casey", "casey jones vigilante", "goongala"]
    r = R("Casey Jones", "Biped", ["Blunt", "Charge"], 1.8, 85.0, (0.85, 0.2, 0.2), "casey_jones", temp="Aggressive",
          inten=0.9)
    return names, r, s, "Casey Jones: hockey mask, golf bag full of clubs and bats, hockey stick. Charges in swinging."


def casey_stick():
    s = S()
    s.box((0, 0, -9.0), (1.6, 1.6, 26.0), "P")                          # shaft (grip at origin)
    s.box((0, 0, 1.2), (1.2, 1.2, 3.0), "D")                             # tape
    s.box((0, -2.2, -22.4), (1.6, 4.8, 1.8), "P", (-15, 0, 0))           # heel
    s.box((0, -4.4, -25.0), (1.6, 2.2, 7.0), "A")                        # blade
    s.box((0, -4.4, -25.4), (1.7, 0.6, 3.6), "D")
    names = ["casey jones' hockey stick", "casey jones hockey stick", "casey's hockey stick", "casey hockey stick"]
    r = R("Casey Jones' Hockey Stick", "Hammer", ["Blunt", "Fast"], 1.6, 2.0, (0.85, 0.2, 0.2), "casey_jones_hockey_stick")
    return names, r, s, "Casey's hockey stick: a fast blunt slap-shot swing."


def casey_bat():
    s = S()
    s.cyl((0, 0, -8.0), (1.6, 18.0, 1.6), "P", ALONG_Z)
    s.cone((0, 0, 2.0), (1.2, 4.0, 1.2), "P", (90, 0, 0))
    s.cyl((0, 0, -13.0), (2.4, 10.0, 2.4), "P", ALONG_Z)
    s.ell((0, 0, -18.0), (2.4, 2.4, 1.2), "P")
    s.cyl((0, 0, 4.2), (2.0, 0.6, 2.0), "D", ALONG_Z)
    s.cyl((0, 0, 1.2), (1.5, 3.0, 1.5), "D", ALONG_Z)                    # grip tape
    for zz in (-11.0, -15.0):
        s.cyl((0, 0, zz), (2.6, 0.6, 2.6), "A", ALONG_Z)                 # gaffer tape bands
    names = ["casey jones' bat", "casey jones bat", "casey's bat", "casey's baseball bat", "casey jones baseball bat"]
    r = R("Casey Jones' Bat", "Hammer", ["Blunt", "Heavy"], 1.0, 2.0, (0.85, 0.2, 0.2), "casey_jones_bat")
    return names, r, s, "Casey's taped baseball bat: Blunt + Heavy. Goongala!"


def casey_golf_bag():
    s = S()
    s.cyl((0, 0, -4.0), (5.0, 12.0, 5.0), "A", ALONG_Z)                  # the bag as a barrel
    s.cyl((0, 0, -10.2), (5.6, 1.0, 5.6), "D", ALONG_Z)
    s.box((0, 2.8, -4.0), (1.0, 1.0, 8.0), "D")                          # strap
    for i, (dx, dy) in enumerate(((-1.2, 1.0), (1.2, 0.6), (0.0, -1.2), (-0.8, -0.4))):
        s.box((dx, dy, -12.5), (0.6, 0.6, 5.0), "D")
        s.box((dx, dy - 0.4, -15.2), (1.2, 1.4, 1.0) if i % 2 else (1.0, 1.0, 1.2), "C")
    s.box((0, -2.8, 1.0), (1.4, 2.4, 1.4), "D")                          # grip
    payload = R("golf club", "Axe", ["Blunt", "Fast"], 0.9, 1.0, seed=18)
    names = ["casey jones' golf bag", "casey jones golf bag", "casey's golf bag", "golf bag of weapons"]
    r = R("Casey Jones' Golf Bag", "Cannon", ["Shoots", "Blunt"], 1.0, 12.0, (0.85, 0.2, 0.2),
          "casey_jones_golf_bag", payload=payload)
    return names, r, s, "Casey's golf bag as a launcher: hurls spinning golf clubs."


def metalhead():
    s = S()
    b = turtle(s, head_kind="box", mask="D", eyes="C", mask_tails=False)
    s.on(5)
    hy = b["hy"]
    s.box((0, hy + 0.5, -3.4), (4.8, 1.2, 0.6), "C")                     # visor
    s.cyl((2.2, hy + 3.8, 0), (0.4, 3.2, 0.4), "D")                       # antenna
    s.ell((2.2, hy + 5.4, 0), (1.0, 1.0, 1.0), "C")
    for sx in (-1, 1):
        s.box((sx * 2.9, hy, -0.4), (0.6, 2.0, 2.0), "A")                # ear bolts
    s.on(0)
    for (x, y) in ((-2.0, 12.0), (2.0, 12.0), (0, 14.0), (0, 9.4)):
        s.box((x, y, 5.9), (0.8, 0.8, 0.6), "A")                         # rivets
    names = ["metalhead", "metalhead tmnt", "metal head tmnt", "robot turtle", "metalhead robot turtle"]
    r = R("Metalhead", "Biped", ["Shoots", "Guard", "Blunt"], 1.6, 180.0, (0.62, 0.66, 0.76), "metalhead",
          elem="Electric", temp="Protective", inten=0.7, feat=["Antenna", "Armor"])
    return names, r, s, "Metalhead: Donnie's robot turtle. Protective, Electric head-laser, armoured."


def lizard(s, big=1.0, snout=5.0, tail_len=4, jaw=True, eye_t="C", t="P", slim=False, head_w=None):
    k = big
    b = humanoid(s, leg_h=7.2 * k, leg_w=3.0 * k if not slim else 2.0, leg_d=3.2 * k if not slim else 2.2,
                 hip_x=2.4 * k if not slim else 1.5, foot=(3.4 * k, 1.3, 4.6 * k), torso=(7.4 * k, 9.0 * k, 5.2 * k) if not slim else (5.2, 8.0, 3.6),
                 torso_kind="ell", arm_w=2.6 * k if not slim else 1.8, arm_len=8.4 * k if not slim else 8.0,
                 arm_x=5.0 * k if not slim else 3.6, hand=2.6 * k if not slim else 1.9)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.ell((0, (yh + ys) / 2 - 0.4, -td / 2 + 0.5), ((7.4 if not slim else 5.0) * k * 0.7, (ys - yh) * 0.8, 1.4), "A")   # pale belly
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        for dx in (-1, 0, 1):
            s.cone((sx * b["tw"] * 0.32 + dx * 0.9, 0.5, -3.0 * k - 0.6), (0.5, 1.2, 0.5), "C", (-90, 0, 0))   # claws
    s.on(5)
    y0 = b["y_neck"]
    hw = head_w or (4.6 * k if not slim else 3.8)
    hy = y0 + 1.9 * k
    s.ell((0, hy, 0.2), (hw, 3.8 * k, 4.6 * k), t)
    s.box((0, hy - 0.3 * k, -2.0 * k - snout / 2), (hw * 0.72, 1.6 * k, snout), t)       # upper jaw
    for sx in (-1, 1):
        s.ell((sx * hw * 0.34, hy + 1.3 * k, -1.2 * k), (1.6 * k, 1.6 * k, 1.6 * k), t)   # eye bumps
        s.box((sx * hw * 0.34, hy + 1.5 * k, -1.9 * k), (0.8, 0.8, 0.5), eye_t)
        s.box((sx * 0.8, hy + 0.5 * k, -2.0 * k - snout + 0.3), (0.5, 0.4, 0.4), "D")   # nostrils
    s.part(5, 0, (0, y0, 0), "Head")
    if jaw:
        s.on(6)
        s.box((0, hy - 1.4 * k, -2.0 * k - snout / 2 + 0.2), (hw * 0.66, 1.0 * k, snout - 0.4), t)
        for i in range(int(snout)):
            s.box((0, hy - 0.8 * k, -2.4 * k - i * 0.95), (hw * 0.62, 0.5, 0.35), "C")   # teeth row
        s.part(6, 5, (0, hy - 0.9 * k, -1.4 * k), "Jaw")
    s.on(0)
    pts = [(0, yh + 0.4, td / 2 - 0.4)]
    for i in range(tail_len):
        pts.append((0, max(0.8, yh - 1.6 * (i + 1) * k), td / 2 + 3.2 * (i + 1) * k))
    tail(s, pts, [(2.6 * k * (1 - i * 0.18), 2.2 * k * (1 - i * 0.18), 4.0 * k) for i in range(tail_len)])
    b["hy"] = hy
    return b


def leatherhead():
    s = S()
    b = lizard(s, big=1.15, snout=8.0, head_w=6.2)
    s.box((0, b["y_sh"] - 3.0, 0), (8.6, 5.2, 6.2), "D")                 # vest
    s.box((0, b["y_sh"] - 3.0, -3.0), (2.0, 5.3, 0.4), "A")
    names = ["leatherhead", "leatherhead tmnt", "leather head tmnt", "leatherhead the gator", "leatherhead alligator"]
    r = R("Leatherhead", "Biped", ["Grab", "ArticulatedTail", "Blunt", "Heavy"], 2.4, 260.0, (0.55, 0.38, 0.2),
          "leatherhead", elem="Water", temp="Aggressive", inten=0.7)
    return names, r, s, "Leatherhead: the mutant gator. Grabs with his jaws, sweeps with his tail, Water."


def mondo_gecko():
    s = S()
    b = lizard(s, big=0.9, snout=2.2, jaw=False, tail_len=3)
    s.box((0, b["y_sh"] - 2.6, 0.2), (7.4, 4.6, 5.6), "D")               # black vest
    x, y, z = b["hands"][1]
    s.on(4)
    s.box((x + 1.2, y + 2.4, z), (0.7, 11.0, 3.4), "A", (0, 0, 8))       # skateboard under the arm
    for dy in (-3.6, 3.6):
        s.cz((x + 2.0, y + 2.4 + dy, z - 0.9), 0.9, 0.7, "D")
        s.cz((x + 2.0, y + 2.4 + dy, z + 0.9), 0.9, 0.7, "D")
    s.on(0)
    names = ["mondo gecko", "mondo gecko tmnt", "mondo", "mondo the gecko", "skateboarding gecko"]
    r = R("Mondo Gecko", "Biped", ["Fast", "Bounce", "Sticky"], 1.6, 70.0, (1.0, 0.6, 0.1), "mondo_gecko",
          temp="Playful", inten=0.9)
    return names, r, s, "Mondo Gecko: skater gecko with sticky feet. Playful, Fast, Bounce."


def mondo_skateboard():
    s = S()
    s.box((0, 1.6, 0), (4.4, 0.6, 14.0), "A")
    s.box((0, 2.0, -6.8), (4.4, 0.6, 2.0), "A", (20, 0, 0))              # kicktails
    s.box((0, 2.0, 6.8), (4.4, 0.6, 2.0), "A", (-20, 0, 0))
    s.box((0, 1.95, 0), (3.6, 0.2, 10.0), "D")                           # grip tape
    s.box((0, 1.0, -4.4), (3.0, 0.6, 1.0), "D")
    s.box((0, 1.0, 4.4), (3.0, 0.6, 1.0), "D")
    for i, (zz, x) in enumerate(((-4.4, -1.8), (-4.4, 1.8), (4.4, -1.8), (4.4, 1.8))):
        s.on(10 + i)
        s.cx((x, 0.6, zz), 1.2, 0.9, "C")
        s.part(10 + i, 0, (x, 0.6, zz), "Wheel")
    s.on(0)
    s.seat(0, (0, 2.0, 0))
    names = ["mondo gecko's skateboard", "mondo's skateboard", "mondo gecko skateboard", "tmnt skateboard"]
    r = R("Mondo Gecko's Skateboard", "Vehicle", ["Piloted", "Fast", "Launch"], 1.5, 5.0, (1.0, 0.6, 0.1),
          "mondo_gecko_s_skateboard", feat=["Wheels"])
    return names, r, s, "Mondo's skateboard: ride it (Piloted, Fast), ollie off with Launch."


def mona_lisa():
    s = S()
    b = lizard(s, big=0.85, snout=1.8, jaw=False, tail_len=3, slim=True)
    s.on(5)
    s.ell((0, b["hy"] + 0.6, 1.6), (3.6, 1.6, 3.0), "A")                 # headband
    s.on(0)
    s.box((0, b["y_sh"] - 2.0, 0), (5.4, 3.0, 3.8), "A")                 # top
    names = ["mona lisa tmnt", "mona lisa salamander", "mona lisa the salamander", "mona lisa mutant"]
    r = R("Mona Lisa", "Biped", ["Fast", "Blade", "Sticky"], 1.6, 55.0, (0.9, 0.35, 0.5), "mona_lisa_tmnt",
          temp="Sneaky", inten=0.6)
    return names, r, s, "Mona Lisa: Raph's salamander friend. Fast, Sneaky."


def venus():
    s = S()
    b = turtle(s)
    x, y, z = b["hands"][1]
    s.on(4)
    s.cyl((x, y + 1.2, z - 2.6), (6.0, 0.4, 6.0), "A", (70, 0, 0))       # tessen fan (open)
    s.cyl((x, y + 1.2, z - 2.6), (1.2, 0.6, 1.2), "D", (70, 0, 0))
    s.on(0)
    names = ["venus de milo", "venus de milo tmnt", "venus tmnt", "venus the fifth turtle", "fifth ninja turtle",
             "venus ninja turtle"]
    r = R("Venus de Milo", "Biped", ["Fast", "Repel", "Blade"], 1.55, 80.0, (0.3, 0.85, 0.9), "venus_de_milo",
          elem="Psychic", temp="Sneaky", inten=0.7)
    return names, r, s, "Venus de Milo (Next Mutation): the fifth turtle, shinobi magic tessen. Psychic Repel."


def usagi():
    s = S()
    b = humanoid(s, leg_h=6.0, leg_w=2.2, leg_d=2.4, hip_x=1.7, foot=(2.4, 1.2, 3.8), torso=(6.2, 7.6, 4.2),
                 torso_t="A", arm_w=2.2, arm_len=7.6, arm_x=4.2, hand=2.2, arm_t="A")
    yh, ys = b["y_hip"], b["y_sh"]
    s.box((0, yh - 1.4, 0), (6.8, 3.6, 4.8), "A")                         # hakama top
    s.box((0, yh + 0.8, 0), (6.4, 1.0, 4.4), "D")                         # obi
    s.box((-3.6, yh + 0.2, -2.4), (0.8, 0.8, 9.0), "D", (0, 20, 30))      # daisho at the hip
    x, y, z = b["hands"][1]
    s.on(4)
    katana_in(s, (x, y, z), length=8.0, rise=4.0)
    s.on(0)
    hy = beast_head(s, b["y_neck"] - 0.2, snout=1.2, snout_w=2.6, head=(5.0, 4.6, 4.6), nose_t="C")
    s.on(7)
    s.box((0, hy + 3.2, 1.6), (2.4, 1.4, 1.6), "D")                       # topknot tie
    for sx in (-1, 1):
        s.seg((sx * 0.6, hy + 2.0, 0.6), (sx * 0.5, hy + 4.4, 1.6), 1.8, 0.9, "P")   # ears up into the knot...
        s.seg((sx * 0.5, hy + 4.4, 1.6), (sx * 1.4, hy + 0.4, 7.0), 1.8, 0.8, "P")   # ...and tied back
    s.part(7, 5, (0, hy + 3.2, 1.6), "Cloth")
    s.on(0)
    names = ["usagi yojimbo", "miyamoto usagi", "usagi", "usagi the rabbit ronin", "rabbit ronin", "usagi yojimbo tmnt"]
    r = R("Usagi Yojimbo", "Biped", ["Blade", "Fast", "Guard"], 1.4, 55.0, (0.3, 0.4, 0.8), "usagi_yojimbo",
          temp="Protective", inten=0.7)
    return names, r, s, "Miyamoto Usagi: the rabbit ronin, ears tied in a topknot, katana. Protective swordsman."


def slash():
    s = S()
    b = turtle(s, big=1.15, shell_spikes=True, mask="D", brow=True)
    x, y, z = b["hands"][1]
    s.on(4)
    s.box((x, y, z - 1.4), (1.0, 1.0, 4.0), "D")                          # morning star
    s.ell((x, y + 0.6, z - 4.4), (3.0, 3.0, 3.0), "A")
    for d in ((0, 1, 0), (0, -1, 0), (1, 0, 0), (-1, 0, 0), (0, 0, -1)):
        s.cone((x + d[0] * 1.8, y + 0.6 + d[1] * 1.8, z - 4.4 + d[2] * 1.8), (0.8, 1.4, 0.8), "C",
               {(0, 1, 0): (0, 0, 0), (0, -1, 0): (180, 0, 0), (1, 0, 0): (0, 0, -90), (-1, 0, 0): (0, 0, 90),
                (0, 0, -1): (-90, 0, 0)}[d])
    s.on(0)
    names = ["slash tmnt", "slash turtle", "evil turtle slash", "slash the evil turtle", "slash evil turtle",
             "slash mutant turtle", "spike tmnt"]
    r = R("Slash the Evil Turtle", "Biped", ["Blunt", "Charge", "Heavy"], 1.8, 130.0, (0.25, 0.25, 0.3), "slash_tmnt",
          temp="Aggressive", inten=0.95, feat=["Spikes"])
    return names, r, s, "Slash: the evil turtle with the spiked shell and a morning star. Aggressive charger."


def rat_king():
    s = S()
    b = humanoid(s, leg_h=9.0, leg_w=1.8, leg_d=2.0, hip_x=1.4, foot=(2.0, 1.2, 3.0), torso=(5.0, 8.4, 3.2),
                 arm_w=1.6, arm_len=9.0, arm_x=3.4, hand=1.8, leg_t="D", arm_t="P")
    yh, ys = b["y_hip"], b["y_sh"]
    for dy in (-2.0, 0.0, 2.0, 4.0):
        s.box((0, yh + 2.4 + dy, 0), (5.2, 0.4, 3.4), "D", (0, 0, 12))    # bandage wraps
    s.on(7)
    s.box((0, ys - 5.0, 2.2), (6.4, 12.0, 0.8), "A")                      # tattered cloak
    s.part(7, 0, (0, ys, 2.0), "Cloth")
    x, y, z = b["hands"][1]
    s.on(4)
    s.cyl((x, 9.0, z - 1.0), (0.7, 18.0, 0.7), "D")                       # flute-staff
    s.on(0)
    for sx in (-1, 1):                                                    # rats riding his shoulders
        s.ell((sx * 3.2, ys + 1.2, 0.2), (1.4, 1.2, 2.4), "A")
        s.cone((sx * 3.2, ys + 1.2, -1.6), (0.8, 1.4, 0.8), "A", TIP_NZ)
        s.box((sx * 3.2, ys + 1.8, -1.0), (0.8, 0.4, 0.3), "C")
    s.on(5)
    hy = b["y_neck"] + 2.2
    s.ell((0, hy, 0), (3.6, 4.4, 4.0), "D")                               # wrapped head
    for dy in (-1.0, 0.2, 1.4):
        s.box((0, hy + dy, 0), (3.8, 0.4, 4.2), "P", (0, 0, -10))
    for sx in (-1, 1):
        s.box((sx * 0.8, hy + 0.5, -2.0), (0.7, 0.6, 0.3), "C")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    payload = R("rat swarm", "Quadruped", ["Swarm", "Fast", "Grab"], 0.4, 1.0, count=8, temp="Aggressive", seed=8)
    names = ["rat king tmnt", "tmnt rat king", "the rat king", "ratking tmnt"]
    r = R("The Rat King", "Biped", ["Shoots", "Fast"], 1.9, 60.0, (0.45, 0.35, 0.25), "rat_king",
          temp="Sneaky", inten=0.8, payload=payload)
    return names, r, s, "The Rat King: bandaged, cloaked, commands the rats (payload: a biting rat swarm)."


def muckman():
    s = S()
    b = humanoid(s, leg_h=4.0, leg_w=3.4, leg_d=3.4, hip_x=2.6, foot=(4.0, 1.4, 4.4), torso=(10.0, 9.0, 7.0),
                 torso_kind="ell", arm_w=3.0, arm_len=9.0, arm_x=6.4, hand=3.4)
    for (x, y, z, sz) in ((-3.0, 11.0, -2.8, 3.0), (2.6, 8.0, -3.0, 2.6), (0.4, 13.6, 2.6, 3.4), (-2.0, 6.4, 2.8, 2.4)):
        s.ell((x, y, z), (sz, sz, sz), "D")                               # garbage lumps
    s.box((3.4, 12.4, -3.2), (1.6, 2.4, 1.0), "A", (0, 0, 20))            # a can stuck in him
    s.on(5)
    hy = b["y_neck"] + 1.2
    s.ell((0, hy, -0.6), (6.0, 4.0, 5.4), "P")
    s.box((0, hy - 0.6, -3.2), (3.2, 0.8, 0.5), "D")
    for sx in (-1, 1):
        s.ell((sx * 1.3, hy + 0.9, -2.9), (1.2, 1.2, 0.8), "C")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    s.ell((-5.4, b["y_sh"] + 1.4, 0.4), (2.4, 2.4, 2.4), "C")              # Joe Eyeball on the shoulder
    s.ell((-5.4, b["y_sh"] + 1.4, -0.8), (1.0, 1.0, 0.6), "D")
    names = ["muckman", "muck man", "muckman and joe eyeball", "joe eyeball", "muckman tmnt"]
    r = R("Muckman", "Blob", ["Sticky", "Blunt", "Walks"], 2.2, 200.0, (0.55, 0.6, 0.25), "muckman", elem="Poison",
          temp="Lazy", inten=0.7)
    return names, r, s, "Muckman (+ Joe Eyeball): a walking garbage heap. Lazy roadblock, Sticky Poison muck."


def mutagen_man():
    s = S()
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):   # walker legs
        s.on(pid)
        s.box((sx * 2.4, 3.4, 0), (1.4, 6.0, 1.4), "D")
        s.box((sx * 2.4, 0.6, -0.6), (2.6, 1.2, 3.4), "D")
        s.cx((sx * 2.4, 6.4, 0), 2.0, 2.0, "A")
        s.part(pid, 0, (sx * 2.4, 6.6, 0), role)
    s.on(0)
    s.cyl((0, 13.0, 0), (7.0, 11.0, 7.0), "A")                           # the tank of ooze
    s.cyl((0, 7.0, 0), (8.0, 1.2, 8.0), "D")
    s.cyl((0, 19.2, 0), (8.0, 1.2, 8.0), "D")
    for a in range(4):                                                   # glass frame struts
        import math
        s.box((3.8 * math.cos(a * math.pi / 2 + 0.78), 13.0, 3.8 * math.sin(a * math.pi / 2 + 0.78)), (0.6, 11.0, 0.6), "C")
    s.ell((0, 14.4, -3.1), (4.2, 5.0, 1.2), "P")                          # Seymour's face in the ooze
    s.box((0, 12.6, -3.8), (2.2, 0.6, 0.4), "D")
    for sx in (-1, 1):
        s.ell((sx * 1.0, 15.2, -3.7), (1.2, 1.4, 0.6), "C")
        s.on(3 if sx < 0 else 4)
        s.seg((sx * 4.0, 16.0, 0), (sx * 6.0, 11.0, -2.0), 1.2, 1.2, "D")   # manipulator arms
        s.box((sx * 6.0, 10.4, -2.4), (1.6, 1.6, 1.6), "D")
        s.part(3 if sx < 0 else 4, 0, (sx * 4.0, 16.0, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.on(0)
    names = ["mutagen man", "mutagen man tmnt", "seymour gutz"]
    r = R("Mutagen Man", "Biped", ["Shoots", "Sticky"], 1.9, 180.0, (0.45, 1.0, 0.2), "mutagen_man", elem="Poison",
          temp="Calm", payload=R("ooze glob", "Ball", ["Sticky"], 0.4, 1.0, elem="Poison", seed=3))
    return names, r, s, "Mutagen Man: Seymour Gutz sloshing in a tank on walker legs. Spits sticky ooze."


def build_all():
    return [leonardo, raphael, donatello, michelangelo, turtles_squad, leo_katanas, raph_sais, don_bo,
            mikey_nunchucks, splinter, april, casey_jones, casey_stick, casey_bat, casey_golf_bag, metalhead,
            leatherhead, mondo_gecko, mondo_skateboard, mona_lisa, venus, usagi, slash, rat_king, muckman,
            mutagen_man]
