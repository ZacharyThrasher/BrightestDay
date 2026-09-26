"""Cosmic DC + Teen Titans: Brainiac, the Anti-Monitor, Apokolips, Lobo, Starfire, Raven, Titans Tower."""
import math

from dlib import S, R, APOK_GREY, TIP_NZ, ALONG_Z, ALONG_X
from hero import hero, head, brute, cape, wings, humanoid


def skull_ship():
    s = S()
    s.ell((0, 8.0, 0), (16.0, 13.0, 18.0), "P")                                          # the cranium
    s.box((0, 3.6, -6.0), (10.0, 4.0, 6.0), "P")                                          # upper jaw
    for sx in (-1, 1):
        s.ell((sx * 3.4, 8.4, -7.6), (4.6, 4.2, 3.0), "D")                               # eye sockets
        s.ell((sx * 3.4, 8.4, -8.2), (2.0, 2.0, 1.4), "C")                               # glowing eyes
        s.ell((sx * 6.8, 6.4, -2.0), (3.0, 3.6, 5.0), "D")                               # temples
    s.box((0, 5.6, -9.0), (2.0, 2.4, 1.4), "D")                                           # nose cavity
    for x in (-3.6, -1.8, 0.0, 1.8, 3.6):
        s.box((x, 2.0, -8.8), (1.2, 2.0, 1.2), "C")                                       # teeth
    s.cyl((0, 15.0, 1.0), (4.0, 3.0, 4.0), "A")                                           # top spire base
    s.cone((0, 18.6, 1.0), (2.4, 4.4, 2.4), "A")
    for i in range(6):                                                                     # tentacles
        a = math.radians(i * 60 + 30)
        x0, z0 = math.cos(a) * 5.0, math.sin(a) * 5.0 + 3.0
        s.seg((x0, 2.6, z0), (x0 * 1.9, -3.0, z0 * 1.4), 1.6, 1.6, "D")
        s.seg((x0 * 1.9, -3.0, z0 * 1.4), (x0 * 2.2, -8.0, z0 * 1.6), 1.1, 1.1, "A")
    names = ["brainiac's skull ship", "skull ship", "brainiac skull ship", "brainiac ship", "brainiac's ship",
             "coluan skull ship"]
    r = R("Brainiac's Skull Ship", "Vehicle", ["Flies", "Grab", "Beam", "Piloted"], 14.0, 50000.0,
          (0.35, 0.9, 0.3), elem="Electric", feat=["Cockpit"], source="DC (Superman / Brainiac)")
    return names, r, s, ("Brainiac's skull ship: a flying skull trailing tentacles - tractor-grabs, beams. Pilot it or\n"
                         "let it hunt (Flies + Grab + Beam).")


def anti_monitor():
    s = S()
    b = humanoid(s, leg_h=12.0, leg_w=3.8, leg_d=3.6, hip_x=2.8, foot=(3.8, 1.6, 5.0), torso=(10.0, 12.0, 6.0),
                 torso_t="D", torso_kind="box", arm_w=3.2, arm_len=13.0, arm_x=6.8, hand=3.4, arm_t="D", leg_t="D",
                 hand_t="P", foot_t="P", arm_spread=14)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, (yh + ys) / 2 + 1.0, -td / 2 - 0.4), (5.0, 9.0, 0.8), "P")                  # chest plating
    s.box((0, (yh + ys) / 2 + 1.0, -td / 2 - 0.9), (1.6, 7.0, 0.5), "C")                  # glowing core seam
    for sx in (-1, 1):
        s.box((sx * (tw / 2 + 2.6), ys + 1.2, 0), (5.0, 2.0, 5.0), "P", (0, 0, -sx * 20))  # flared pauldrons
        s.cone((sx * (tw / 2 + 5.0), ys + 2.6, 0), (2.0, 5.0, 2.0), "A", (0, 0, -sx * 55))
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        s.box((sx * b["hip_x"], 8.0, 0), (b["leg_w"] + 1.0, 5.0, b["leg_d"] + 1.0), "P")  # shin armour
    s.on(5)
    hy = b["y_neck"] + 3.4
    s.ell((0, hy, 0), (5.4, 6.4, 5.4), "D")                                                # helmeted skull
    s.box((0, hy - 1.6, -2.2), (3.4, 2.8, 1.4), "C")                                       # glowing skull face
    for sx in (-1, 1):
        s.box((sx * 1.0, hy - 0.8, -2.9), (1.2, 1.0, 0.4), "D")                           # eye holes
        s.cone((sx * 3.2, hy + 3.4, 0), (1.8, 7.0, 1.8), "P", (0, 0, -sx * 28))           # huge horn-fins
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["anti-monitor", "the anti-monitor", "anti monitor", "mobius", "crisis on infinite earths",
             "antimonitor"]
    r = R("Anti-Monitor", "Kaiju", ["Beam", "Stomp", "Heavy"], 20.0, 200000.0, (0.95, 0.9, 0.7), elem="Atomic",
          temp="Aggressive", inten=1.0, source="DC (Crisis on Infinite Earths)")
    return names, r, s, "The Anti-Monitor: towering antimatter armour, glowing skull face, horn fins. Kaiju that beams and stomps."


def parademon():
    s = S()
    b = humanoid(s, leg_h=7.0, leg_w=2.0, leg_d=2.2, hip_x=1.6, foot=(2.2, 1.2, 3.4), torso=(6.4, 7.6, 4.0),
                 torso_t="P", torso_kind="box", arm_w=1.8, arm_len=8.4, arm_x=4.2, hand=2.0, arm_spread=18)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, ys - 2.0, -td / 2 - 0.3), (5.0, 3.0, 0.5), "D")
    s.box((0, ys - 1.4, td / 2 + 1.0), (3.6, 3.6, 2.0), "D")                               # backpack
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        hx, hy, hz = b["hands"][sx]
        s.cone((hx, hy - 1.4, hz - 0.4), (1.0, 2.0, 1.0), "C", (180, 0, 0))                # claws
    s.on(5)
    hy2 = b["y_neck"] + 2.0
    s.ell((0, hy2, 0), (3.6, 4.0, 4.2), "D")                                               # helmet
    s.box((0, hy2 - 0.8, -2.2), (2.4, 1.4, 1.0), "A")                                      # fanged maw
    for sx in (-1, 1):
        s.box((sx * 0.9, hy2 + 0.6, -2.0), (1.0, 0.5, 0.4), "C")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    wings(s, b, t="A", feather_t="D", span=12.0, h=7.0, feathers=4, angle=30)            # insectile wings
    names = ["parademon", "parademons", "apokolips parademons", "darkseid's parademons", "para-demons"]
    r = R("Parademons", "Biped", ["Flies", "Swarm", "Grab"], 1.8, 70.0, (0.55, 0.45, 0.2), count=4, temp="Aggressive",
          inten=1.0, source="DC (New Gods / Apokolips)")
    return names, r, s, "Parademons: Apokolips' winged shock troops - a swarm of four that swoops and seizes."


def steppenwolf_axe():
    s = S()
    s.cyl((0, 0, -6.0), (1.2, 22.0, 1.2), "D", ALONG_Z)                                    # long haft
    s.box((0, 0, 4.0), (1.6, 1.6, 2.0), "A")
    s.box((0, 3.0, -15.0), (1.2, 6.0, 4.0), "P")                                           # blade body
    s.box((0, 6.4, -14.4), (1.0, 3.0, 6.0), "P", (-30, 0, 0))
    s.box((0, 4.4, -12.6), (1.3, 7.4, 0.8), "C", (-6, 0, 0))                               # energy edge
    s.cone((0, -2.8, -15.2), (1.2, 4.0, 2.6), "P", (180, 0, 0))                            # back spike
    s.cone((0, 0, -18.4), (1.2, 3.4, 1.2), "A", TIP_NZ)
    names = ["steppenwolf's axe", "steppenwolf axe", "steppenwolf", "electro-axe", "steppenwolf's electro axe"]
    r = R("Steppenwolf's Electro-Axe", "Axe", ["Blade", "Heavy"], 1.9, 12.0, (0.6, 0.7, 1.0), elem="Electric",
          source="DC (New Gods)")
    return names, r, s, "Steppenwolf's electro-axe: a huge Electric cleaver (Blade + Heavy)."


def starfire():
    s = S()
    b = hero(s, fem=True, tall=0.8, suit="A", legs="P", arms="P", boots="A", boot_h=5.4, gloves="A", arm_spread=18)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, yh + 0.4, 0), (tw * 0.8, 1.8, td * 1.02), "A")
    s.cyl((0, ys - 1.8, -td / 2 - 0.4), (1.4, 0.4, 1.4), "C", (90, 0, 0))                  # gem
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        hx, hy, hz = b["hands"][sx]
        s.ell((hx, hy, hz), (3.0, 3.0, 3.0), "C")                                          # starbolt glow
    s.on(5)
    hy = b["y_neck"] + 2.0
    s.ell((0, hy, 0), (3.3, 4.0, 3.6), "P")
    for sx in (-1, 1):
        s.box((sx * 0.7, hy + 0.3, -1.8), (0.9, 0.5, 0.4), "C")
    s.ell((0, hy + 1.2, 0.8), (4.2, 3.0, 4.0), "A")                                        # the flame-mane
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(7)
    for i, (x, h) in enumerate(((-1.6, 14.0), (0.0, 17.0), (1.6, 14.0), (-0.8, 11.0), (0.8, 11.0))):
        s.box((x, b["y_neck"] + 2.0 - h / 2, 2.2 + i * 0.1), (1.6, h, 1.2), "A", (-8, 0, x * 3))   # hair to the knees
    s.part(7, 5, (0, b["y_neck"] + 2.8, 1.6), "Cloth")
    s.on(0)
    names = ["starfire", "koriand'r", "koriandr", "starfire teen titans", "kory anders"]
    r = R("Starfire", "Biped", ["Flies", "Shoots", "Blunt"], 1.9, 70.0, (1.0, 0.45, 0.2), elem="Fire", temp="Playful",
          inten=0.7, payload=R("starbolt", "Ball", ["Fast", "Explodes"], 0.6, 2.0, elem="Fire"),
          source="DC (Teen Titans)")
    return names, r, s, "Starfire: Tamaranean princess, flame-red mane to the knees, glowing fists. Flies, hurls starbolts."


def soul_self():
    s = S()
    s.ell((0, 8.0, 0), (7.0, 7.0, 13.0), "D")                                              # the raven body
    s.ell((0, 10.6, -7.0), (5.4, 5.4, 5.6), "D")                                           # head
    s.cone((0, 10.0, -11.2), (2.4, 4.4, 2.0), "A", (-95, 0, 0))                            # beak
    for sx in (-1, 1):
        s.ell((sx * 1.6, 11.4, -9.4), (1.4, 1.1, 0.6), "C")                                # four burning eyes
        s.ell((sx * 1.2, 12.6, -9.0), (1.0, 0.7, 0.6), "C")
    for pid, sx, role in ((12, -1, "WingLeft"), (13, 1, "WingRight")):
        s.on(pid)
        s.box((sx * 7.0, 12.0, 0.0), (12.0, 1.6, 6.0), "D", (0, 0, sx * 28))              # raised wing arm
        for i in range(5):
            x = sx * (3.5 + i * 2.6)
            top = 10.0 + i * 1.4
            L = 7.0 + i * 1.6
            s.box((x, top - L / 2 + 1.0, 1.4), (2.2, L, 1.0), "A" if i % 2 else "D", (0, 0, sx * (8 + i * 4)))   # primaries
        s.part(pid, 0, (sx * 2.4, 10.0, 0), role)
    s.on(8)
    for i in range(3):
        s.box(((i - 1) * 1.8, 7.0, 9.0 + i * 0.4), (1.8, 0.8, 6.0), "D", (0, (i - 1) * 18, 0))   # tail feathers
    s.part(8, 0, (0, 8.0, 5.0), "Tail")
    s.on(1)
    s.box((-1.4, 2.4, 0), (0.8, 4.0, 0.8), "D")
    s.part(1, 0, (-1.4, 4.4, 0), "LegBackLeft")
    s.on(2)
    s.box((1.4, 2.4, 0), (0.8, 4.0, 0.8), "D")
    s.part(2, 0, (1.4, 4.4, 0), "LegBackRight")
    s.on(0)
    names = ["raven's soul-self", "soul self", "soul-self", "raven's soul self", "azarath metrion zinthos",
             "raven teen titans", "rachel roth"]
    r = R("Raven's Soul-Self", "Bird", ["Flies", "Grab", "Pull"], 4.0, 50.0, (0.35, 0.2, 0.7), elem="Shadow",
          temp="Sneaky", inten=0.7, source="DC (Teen Titans)")
    return names, r, s, "Raven's soul-self: a great four-eyed shadow raven that swoops and seizes (Grab). Azarath Metrion Zinthos!"


def titans_tower():
    s = S()
    s.box((0, 14.0, 0), (8.0, 28.0, 8.0), "P")                                              # the stem of the T
    s.box((0, 32.0, 0), (30.0, 8.0, 8.0), "P")                                              # the crossbar
    for y in range(4, 28, 4):
        s.box((0, y, -4.1), (6.4, 1.6, 0.4), "C")                                           # window bands
    for x in (-12, -6, 6, 12):
        s.box((x, 32.0, -4.1), (4.0, 5.0, 0.4), "C")
    s.box((0, 32.0, -4.1), (6.0, 5.0, 0.4), "C")
    s.box((0, 0.8, 0), (16.0, 1.6, 14.0), "D")                                              # island base
    names = ["titans tower", "teen titans tower", "the titans tower", "t tower", "the t-tower"]
    r = R("Titans Tower", "Pillar", ["Barrier", "Heavy"], 12.0, 30000.0, (0.8, 0.9, 1.0), source="DC (Teen Titans)")
    return names, r, s, "Titans Tower: the giant T - a pillar/anchor you can tether from and hide behind."


def lobo():
    s = S()
    b = hero(s, bulk=1.3, suit="D", arms="P", legs="D", boots="D", gloves="D", belt="A", buckle="C", arm_spread=10)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, ys - 3.0, 0), (tw + 0.4, 6.0, td + 0.4), "D")                                 # biker vest
    s.box((0, ys - 3.2, -td / 2 - 0.5), (2.0, 5.6, 0.4), "P")                               # bare chest
    for sx in (-1, 1):                                                                       # crossed chain bandolier
        s.box((0, ys - 3.0, -td / 2 - 0.7), (0.7, 8.0, 0.4), "A", (0, 0, sx * 35))
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.seg((hx, hy, hz), (hx + 2.0, hy - 4.0, hz - 3.0), 0.7, 0.7, "A")                       # chain
    s.cone((hx + 2.4, hy - 5.6, hz - 3.6), (1.8, 3.0, 1.2), "C", (180, 0, 0))               # hook
    s.on(5)
    hy2 = b["y_neck"] + 2.2
    s.ell((0, hy2, 0), (3.8, 4.4, 4.0), "P")
    s.ell((0, hy2 + 1.0, 1.2), (4.2, 3.6, 3.6), "D")                                         # slicked mane
    s.box((0, hy2 - 1.0, 2.4), (3.6, 6.0, 1.2), "D")                                         # long hair down the back
    for sx in (-1, 1):
        s.box((sx * 0.8, hy2 + 0.3, -1.9), (1.0, 0.5, 0.4), "A")                             # red eyes
    s.box((0, hy2 - 1.2, -1.9), (2.4, 0.6, 0.4), "D")                                        # black lips
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["lobo", "the main man", "lobo the czarnian", "lobo dc", "the last czarnian"]
    r = R("Lobo", "Biped", ["Pull", "Blunt", "Heavy", "Charge"], 2.2, 180.0, (0.85, 0.12, 0.12), temp="Chaotic",
          inten=1.0, source="DC (Lobo)")
    return names, r, s, "Lobo, the Main Man: chalk skin, red eyes, biker leathers, chain-hook. Chaotic - frags anyone."


def build_all():
    return [skull_ship, anti_monitor, parademon, steppenwolf_axe, starfire, soul_self, titans_tower, lobo] + PROC


def P(names, r, note):
    return lambda: (names, r, None, note)


PROC = [
    P(["brainiac", "vril dox", "brainiac 5", "brainiac coluan"],
      R("Brainiac", "Biped", ["Beam", "Grab", "Guard"], 2.0, 150.0, (0.35, 0.9, 0.3), elem="Electric",
        temp="Fearful", inten=0.7, feat=["Antenna"], source="DC (Superman)"), "Brainiac: the Coluan collector (tractor Grab)."),
    P(["kalibak", "kalibak the cruel", "beta club"],
      R("Kalibak's Beta Club", "Hammer", ["Blunt", "Heavy", "Shoots"], 1.8, 25.0, APOK_GREY, elem="Electric",
        source="DC (New Gods)"), "Kalibak's beta club: a blasting mace."),
    P(["orion", "orion of the new gods", "astro-glider"],
      R("Orion", "Biped", ["Flies", "Blunt", "Charge"], 2.0, 120.0, (0.2, 0.3, 0.8), temp="Aggressive", inten=0.9,
        source="DC (New Gods)"), "Orion: Darkseid's son, raised on New Genesis, rage barely held."),
    P(["mister miracle", "scott free", "mister miracle's aero-discs", "aero discs"],
      R("Aero-Discs", "Platform", ["Flies", "Launch"], 1.2, 5.0, (0.9, 0.2, 0.2), source="DC (New Gods)"),
      "Mister Miracle's aero-discs: flying launch platforms."),
    P(["big barda", "barda", "mega-rod", "big barda's mega rod"],
      R("Mega-Rod", "Staff", ["Blunt", "Beam"], 1.8, 12.0, (0.2, 0.3, 0.8), source="DC (New Gods)"),
      "Big Barda's mega-rod."),
    P(["granny goodness", "female furies", "the female furies"],
      R("Female Furies", "Biped", ["Blade", "Fast", "Swarm"], 1.8, 70.0, (0.5, 0.3, 0.6), count=4, temp="Aggressive",
        inten=0.9, source="DC (New Gods)"), "Granny Goodness' Female Furies."),
    P(["desaad", "desaad's torture"],
      R("Desaad", "Biped", ["Shoots"], 1.8, 60.0, (0.5, 0.2, 0.2), elem="Shadow", temp="Fearful", inten=0.8,
        source="DC (New Gods)"), "Desaad: Darkseid's cowardly torturer."),
    P(["trigon", "trigon the terrible"],
      R("Trigon", "Kaiju", ["Beam", "Stomp", "Heavy"], 20.0, 150000.0, (0.9, 0.15, 0.1), elem="Fire", temp="Aggressive",
        inten=1.0, source="DC (Teen Titans)"), "Trigon: four-eyed demon lord, Raven's father."),
    P(["terra", "tara markov", "terra teen titans"],
      R("Terra", "Boulder", ["Flies", "Homing", "Heavy"], 2.5, 900.0, (0.9, 0.8, 0.3), elem="Earth", count=3,
        source="DC (Teen Titans)"), "Terra's flying boulders."),
    P(["beast boy", "garfield logan", "changeling", "beast boy teen titans"],
      R("Beast Boy", "Quadruped", ["Charge", "Grab", "Fast"], 2.2, 300.0, (0.3, 0.9, 0.3), temp="Playful", inten=1.0,
        source="DC (Teen Titans)"), "Beast Boy: whatever green animal he feels like - here a charging beast."),
    P(["starbolt", "starbolts", "starfire's starbolt"],
      R("Starbolt", "Ball", ["Fast", "Explodes"], 0.6, 2.0, (0.4, 1.0, 0.3), elem="Fire", count=2,
        source="DC (Teen Titans)"), "Starfire's starbolts."),
    P(["brother blood", "sebastian blood"],
      R("Brother Blood", "Biped", ["Pull", "Shoots"], 1.9, 80.0, (0.8, 0.1, 0.1), elem="Psychic", temp="Sneaky",
        inten=0.7, source="DC (Teen Titans)"), "Brother Blood: cult leader of the H.I.V.E."),
    P(["lobo's space hog", "space hog", "lobo's bike", "lobo motorcycle"],
      R("Lobo's Space Hog", "Vehicle", ["Piloted", "Flies", "Charge", "Heavy"], 3.4, 800.0, (0.85, 0.12, 0.12),
        feat=["Exhaust", "Wheels"], source="DC (Lobo)"), "Lobo's Space Hog: the flying chopper. Ram things."),
    P(["amazo", "amazo android"],
      R("Amazo", "Biped", ["Beam", "Flies", "Guard"], 2.4, 400.0, (0.4, 0.8, 0.3), temp="Aggressive", inten=0.8,
        source="DC (Justice League)"), "Amazo: the power-copying android."),
    P(["cheetah dc", "barbara minerva", "the cheetah wonder woman"],
      R("The Cheetah", "Biped", ["Fast", "Blade", "Charge"], 1.9, 70.0, (0.95, 0.75, 0.2), temp="Aggressive", inten=1.0,
        source="DC (Wonder Woman)"), "The Cheetah: Wonder Woman's feral foe."),
    P(["giganta", "doris zeul"],
      R("Giganta", "Kaiju", ["Stomp", "Grab"], 12.0, 20000.0, (0.9, 0.4, 0.2), temp="Aggressive", inten=0.8,
        source="DC (Wonder Woman)"), "Giganta: grows to kaiju size."),
    P(["ares dc", "god of war ares"],
      R("Ares", "Biped", ["Blade", "Heavy", "Charge"], 2.8, 400.0, (0.3, 0.3, 0.35), temp="Aggressive", inten=1.0,
        source="DC (Wonder Woman)"), "Ares: the armoured god of war."),
    P(["killer frost", "caitlin snow", "louise lincoln"],
      R("Killer Frost", "Biped", ["Shoots", "Beam"], 1.8, 60.0, (0.7, 0.9, 1.0), elem="Ice", temp="Aggressive",
        inten=0.7, source="DC (The Flash / Firestorm)"), "Killer Frost: ice blasts."),
    P(["deadshot", "floyd lawton", "deadshot's wrist guns"],
      R("Deadshot's Wrist Guns", "Gun", ["Homing", "Shoots"], 0.5, 2.0, (0.8, 0.2, 0.2), count=2,
        source="DC (Suicide Squad)"), "Deadshot never misses: wrist-mounted guns with curving (Homing) shots."),
    P(["ratcatcher", "ratcatcher 2", "ratcatcher's rats", "sebastian the rat"],
      R("Ratcatcher's Rats", "Quadruped", ["Swarm", "Fast", "Grab"], 0.4, 1.0, (0.5, 0.45, 0.4), count=12,
        temp="Aggressive", inten=0.8, source="DC (The Suicide Squad)"), "Ratcatcher 2's rat tide: twelve tiny biters."),
    P(["polka-dot man", "polka dot man", "abner krill", "polka dots"],
      R("Polka-Dot Man's Dots", "Ball", ["Swarm", "Explodes", "Homing"], 0.5, 1.0, (1.0, 0.4, 0.7), count=6,
        source="DC (The Suicide Squad)"), "Polka-Dot Man: six homing dots that explode on contact."),
    P(["bloodsport", "robert dubois", "bloodsport's gun"],
      R("Bloodsport's Gun", "Gun", ["Shoots", "Explodes"], 1.2, 8.0, (0.6, 0.6, 0.65), source="DC (The Suicide Squad)"),
      "Bloodsport's transforming super-rifle."),
    P(["peacemaker", "christopher smith", "peacemaker dc"],
      R("Peacemaker", "Biped", ["Shoots", "Guard"], 1.9, 90.0, (0.9, 0.9, 0.95), temp="Aggressive", inten=0.8,
        source="DC (Peacemaker)"), "Peacemaker: chrome-helmeted 'pacifist' with guns (helmet in film_tv)."),
    P(["kite man", "kite man hell yeah", "hell yeah kite"],
      R("Kite Man", "Wings", ["Glide", "SlowFall"], 2.5, 3.0, (0.3, 0.8, 0.3), source="DC (Batman / Harley Quinn)"),
      "Kite Man. Hell yeah. A wearable kite: glide + slow fall (a ring-out save)."),
    P(["the monitor", "mar novu"],
      R("The Monitor", "Biped", ["Guard", "Portal"], 2.4, 100.0, (0.4, 0.5, 0.9), elem="Psychic", temp="Protective",
        inten=0.6, source="DC (Crisis on Infinite Earths)"), "The Monitor: cosmic guardian of the multiverse."),
    P(["the spectre", "jim corrigan"],
      R("The Spectre", "Biped", ["Flies", "Grow", "Shoots"], 2.4, 90.0, (0.3, 0.6, 0.3), elem="Shadow", temp="Aggressive",
        inten=0.7, source="DC (The Spectre)"), "The Spectre: wrath of God, cloaked in green."),
    P(["anti-life equation", "the anti-life equation"],
      R("Anti-Life Equation", "Ball", ["Homing", "Explodes"], 1.0, 1.0, (0.3, 0.3, 0.3), elem="Shadow",
        source="DC (New Gods)"), "The Anti-Life Equation - here only a dark homing blast (no mind control, ever)."),
    P(["apokolips fire pit", "fire pits of apokolips", "apokolips"],
      R("Fire Pits of Apokolips", "Dome", ["Breath", "Barrier"], 6.0, 5000.0, APOK_GREY, elem="Fire",
        source="DC (New Gods)"), "The fire pits of Apokolips: a dome that belches fire."),
]
