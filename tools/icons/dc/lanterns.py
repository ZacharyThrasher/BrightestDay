"""The emotional spectrum: Green Lantern Corps members + the other Corps (the many lanterns already in film_tv.ron
are not repeated here - see overlaps.txt). The whole game is Green-Lantern-inspired: every construct is already a
Lantern construct in the caster's corps colour."""
from dlib import (S, R, LANTERN_GREEN, FEAR_YELLOW, AVARICE_ORANGE, HOPE_BLUE, RAGE_RED, LOVE_VIOLET,
                  COMPASSION_INDIGO, TIP_NZ, ALONG_Z, ALONG_X)
from hero import hero, head, brute, emblem_lantern, ring_on, fist_glow, humanoid, cape


def _lantern_suit(s, b, bg="D"):
    """Green Lantern uniform: dark shoulders/upper chest, lantern emblem."""
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, ys - 1.4, 0), (tw + 0.3, 2.8, td + 0.3), bg)                          # dark yoke
    emblem_lantern(s, (0, ys - 2.6, -td / 2 - 0.6), t="C", k=1.0, bg=bg)


def hal_jordan():
    s = S()
    b = hero(s, suit="P", boots="D", gloves="C", glove_h=2.6, arms="D", legs="P")
    _lantern_suit(s, b)
    head(s, b, hair="D", style="short", mask="D")
    s.on(4)
    ring_on(s, b["hands"][1], "C")
    fist_glow(s, b["hands"][1], "C", 0.9)
    s.on(0)
    names = ["hal jordan", "green lantern hal jordan", "highball", "hal jordan green lantern", "green lantern of sector 2814",
             "the green lantern"]
    r = R("Hal Jordan", "Biped", ["Flies", "Shoots", "Guard"], 1.9, 85.0, LANTERN_GREEN, temp="Protective", inten=0.7,
          payload=R("construct fist", "Gauntlet", ["Blunt", "Fast"], 1.2, 20.0), source="DC (Green Lantern)")
    return names, r, s, ("Hal Jordan, Green Lantern of sector 2814: black yoke, white gloves, domino mask, glowing ring.\n"
                         "Flies, fires fist constructs, shields (Guard). In brightest day...")


def john_stewart():
    s = S()
    b = hero(s, bulk=1.14, suit="P", boots="P", gloves="P", arms="P", legs="D")
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    for sx in (-1, 1):
        s.box((sx * tw * 0.4, (yh + ys) / 2, 0), (1.2, b["th"], td + 0.25), "D")    # dark side panels
    emblem_lantern(s, (0, ys - 2.6, -td / 2 - 0.6), t="C", k=1.1)
    head(s, b, hair="D", style="short", brows="D")
    s.on(4)                                                                         # construct rifle
    hx, hy, hz = b["hands"][1]
    ring_on(s, (hx, hy, hz), "C")
    s.box((hx, hy + 0.9, hz - 4.0), (1.4, 1.8, 8.0), "C")
    s.box((hx, hy + 2.1, hz - 3.0), (0.8, 0.8, 2.6), "C")                          # scope
    s.cz((hx, hy + 0.9, hz - 9.4), 0.8, 3.0, "C")
    s.on(0)
    names = ["john stewart", "green lantern john stewart", "john stewart green lantern", "the marine lantern",
             "john stewart the architect"]
    r = R("John Stewart", "Biped", ["Flies", "Shoots", "Pierce", "Guard"], 2.0, 95.0, LANTERN_GREEN, temp="Protective",
          inten=0.8, payload=R("construct round", "Ball", ["Pierce", "Fast"], 0.4, 2.0), source="DC (Green Lantern)")
    return names, r, s, ("John Stewart: the Marine-architect Lantern - detailed, engineered constructs (here a construct\n"
                         "sniper rifle): Pierce shots, shields. film_tv's 'john stewart's mech' is his mech construct.")


def saint_walker():
    s = S()
    b = hero(s, bulk=0.95, tall=1.4, suit="P", arms="P", legs="P", boots="A", gloves="A")
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, (yh + ys) / 2, -td / 2 - 0.25), (2.6, b["th"] * 0.9, 0.4), "A")          # blue tabard
    s.cyl((0, ys - 2.6, -td / 2 - 0.6), (2.8, 0.4, 2.8), "C", (90, 0, 0))                # hope emblem (lantern ring)
    s.cyl((0, ys - 2.6, -td / 2 - 0.7), (1.6, 0.6, 1.6), "A", (90, 0, 0), sub=True)
    s.on(5)
    hy = b["y_neck"] + 2.8
    s.ell((0, hy, 0), (3.6, 5.2, 4.0), "A")                                              # tall blue alien head
    s.box((0, hy + 2.6, 0.4), (1.0, 2.0, 3.8), "D")                                      # crest ridge
    for sx in (-1, 1):
        s.box((sx * 0.8, hy + 0.2, -1.95), (1.0, 0.5, 0.4), "C")
        s.box((sx * 1.3, hy - 1.0, -1.6), (0.3, 2.0, 0.4), "D")                        # face lines
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        fist_glow(s, b["hands"][sx], "C", 0.8)
    s.on(0)
    names = ["saint walker", "blue lantern saint walker", "bro'dee walker", "brodee walker", "all will be well"]
    r = R("Saint Walker", "Biped", ["Flies", "Guard", "Barrier", "Repel"], 2.1, 80.0, HOPE_BLUE, temp="Protective",
          inten=1.0, source="DC (Blue Lantern Corps)")
    return names, r, s, ("Saint Walker, first Blue Lantern: tall crested alien, hope-blue. A pure guardian: shields,\n"
                         "barriers and shoves (Guard + Barrier + Repel), Protective 1.0. Never heals - hope defends.")


def arkillo():
    s = S()
    b = brute(s, bulk=1.6, leg_h=8.6, torso=(10.8, 11.0, 6.8), arm_len=11.8, arm_spread=12)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, ys - 1.4, 0), (tw + 0.4, 2.8, td + 0.4), "D")                              # Sinestro Corps yoke
    s.cyl((0, ys - 3.4, -td / 2 - 0.4), (3.2, 0.5, 3.2), "A", (90, 0, 0))                 # yellow emblem
    s.cone((0, ys - 3.4, -td / 2 - 0.7), (1.8, 2.2, 0.4), "D", (0, 0, 180))
    s.on(5)
    hy = b["y_neck"] + 2.8
    s.ell((0, hy, -0.6), (5.4, 5.0, 5.4), "P")
    s.box((0, hy - 1.8, -2.2), (4.6, 2.2, 3.0), "P")                                     # massive jaw
    for sx in (-1, 1):
        s.cone((sx * 1.8, hy - 1.0, -3.4), (1.0, 3.0, 1.0), "C", (-10, 0, sx * -12))     # tusks
        s.box((sx * 1.2, hy + 0.8, -3.0), (1.2, 0.6, 0.4), "A")                          # yellow eyes
        s.cone((sx * 2.4, hy + 2.8, 0), (1.2, 2.4, 1.2), "D", (0, 0, -sx * 30))          # head spikes
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(4)
    ring_on(s, b["hands"][1], "A")
    s.on(0)
    names = ["arkillo", "arkillo sinestro corps", "sinestro corps drill sergeant"]
    r = R("Arkillo", "Biped", ["Blunt", "Heavy", "Charge", "Shoots"], 3.0, 700.0, FEAR_YELLOW, temp="Aggressive",
          inten=0.9, source="DC (Sinestro Corps)")
    return names, r, s, "Arkillo: the Sinestro Corps' tusked brute drill sergeant, yellow ring. Charges and pummels."


def glomulus():
    s = S()
    s.ell((0, 5.0, 0), (9.0, 9.0, 8.4), "P")                                             # round orange body
    s.ell((0, 3.8, -3.0), (6.4, 3.4, 3.0), "D")                                          # big mouth
    for x in (-1.8, -0.6, 0.6, 1.8):
        s.box((x, 4.9, -4.2), (0.9, 0.9, 0.6), "C")                                      # teeth
    for sx in (-1, 1):
        s.ell((sx * 1.8, 7.4, -3.2), (2.4, 2.8, 1.6), "C")                               # goggly eyes
        s.ell((sx * 1.8, 7.4, -3.9), (1.0, 1.2, 0.6), "D")
    s.on(3)
    s.ell((-4.8, 4.8, -0.6), (2.0, 3.0, 2.0), "P")
    s.part(3, 0, (-4.0, 6.0, 0), "ArmLeft")
    s.on(4)
    s.ell((4.8, 4.8, -0.6), (2.0, 3.0, 2.0), "P")
    s.part(4, 0, (4.0, 6.0, 0), "ArmRight")
    s.on(1)
    s.ell((-2.0, 0.8, 0), (2.2, 1.8, 2.8), "P")
    s.part(1, 0, (-2.0, 1.6, 0), "LegBackLeft")
    s.on(2)
    s.ell((2.0, 0.8, 0), (2.2, 1.8, 2.8), "P")
    s.part(2, 0, (2.0, 1.6, 0), "LegBackRight")
    s.on(0)
    s.cone((0, 10.0, 0.4), (1.4, 2.4, 1.4), "A", (-20, 0, 0))                            # tuft
    names = ["glomulus", "larfleeze's glomulus", "orange lantern glomulus", "orange construct buddy"]
    r = R("Glomulus", "Blob", ["Grab", "Bounce", "Fast"], 1.0, 20.0, AVARICE_ORANGE, temp="Playful", inten=0.9,
          source="DC (Orange Lantern Corps)")
    return names, r, s, "Glomulus: Larfleeze's orange-light gremlin - snatches things (Grab), boings around, Playful."


def central_power_battery():
    s = S()
    s.cyl((0, 1.5, 0), (22.0, 3.0, 22.0), "D")                                           # base
    s.cyl((0, 4.0, 0), (18.0, 2.0, 18.0), "P")
    s.cyl((0, 16.0, 0), (13.0, 22.0, 13.0), "C")                                         # the glowing core
    for i in range(8):                                                                   # cage bars
        import math
        a = math.radians(i * 45)
        s.box((math.cos(a) * 7.2, 16.0, math.sin(a) * 7.2), (1.4, 22.0, 1.4), "P", (0, -i * 45, 0))
    s.cyl((0, 27.6, 0), (18.0, 2.4, 18.0), "P")                                          # top cap
    s.cyl((0, 30.4, 0), (12.0, 3.2, 12.0), "D")
    s.cyl((0, 32.8, 0), (6.0, 1.6, 6.0), "A")
    s.cyl((0, 16.0, 0), (15.0, 1.2, 15.0), "A")                                          # mid band
    names = ["central power battery", "the central power battery", "oa power battery", "power battery of oa",
             "giant power battery", "the great battery"]
    r = R("Central Power Battery", "Pillar", ["Beam", "Barrier", "Heavy"], 12.0, 20000.0, LANTERN_GREEN,
          source="DC (Green Lantern)")
    return names, r, s, "The Central Power Battery of Oa: a colossal lantern - an anchor pillar that beams and blocks."


def guardians():
    s = S()
    b = humanoid(s, leg_h=4.0, leg_w=2.0, leg_d=2.2, hip_x=1.2, foot=(2.0, 1.0, 2.8), torso=(5.0, 6.0, 3.6),
                 torso_t="A", arm_w=1.8, arm_len=6.0, arm_x=3.2, hand=1.8, arm_t="A", leg_t="A", hand_t="P")
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, yh - 1.8, 0), (5.8, 4.6, 4.4), "A")                                         # red robe skirt
    emblem_lantern(s, (0, ys - 2.0, -td / 2 - 0.5), t="C", k=0.8)
    s.on(5)
    hy = b["y_neck"] + 3.0
    s.ell((0, hy, 0), (5.6, 6.2, 5.4), "P")                                               # huge bald blue head
    for sx in (-1, 1):
        s.ell((sx * 2.8, hy - 0.6, 0.6), (1.4, 3.4, 3.0), "C")                           # white side hair
        s.box((sx * 1.1, hy - 0.2, -2.6), (1.2, 0.5, 0.4), "D")
    s.box((0, hy - 1.8, -2.4), (1.6, 0.4, 0.4), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["guardians of the universe", "the guardians of the universe", "oans", "the oans", "little blue men",
             "guardians of oa", "ganthet", "sayd"]
    r = R("Guardians of the Universe", "Biped", ["Beam", "Guard", "Swarm"], 1.1, 30.0, (0.85, 0.12, 0.12),
          elem="Psychic", count=3, temp="Fearful", inten=0.6, source="DC (Green Lantern)")
    return names, r, s, "The Guardians of the Universe: three tiny, big-headed blue Oans in red robes. Keep back and beam."


def P(names, r, note):
    return lambda: (names, r, None, note)


PROC = [
    P(["green lantern corps", "the green lantern corps", "lantern corps", "green lanterns", "the corps"],
      R("Green Lantern Corps", "Biped", ["Flies", "Shoots", "Swarm"], 1.9, 80.0, LANTERN_GREEN, count=4,
        temp="Protective", inten=0.8, source="DC (Green Lantern)"), "A squad of four Green Lanterns."),
    P(["manhunters", "manhunter robots", "manhunter androids", "manhunter"],
      R("Manhunters", "Biped", ["Beam", "Swarm", "Walks"], 2.2, 200.0, (0.85, 0.15, 0.1), count=3, temp="Aggressive",
        inten=0.8, source="DC (Green Lantern)"), "The Guardians' first police: red-and-blue android Manhunters."),
    P(["alan scott", "golden age green lantern", "starheart"],
      R("Alan Scott", "Biped", ["Flies", "Guard", "Shoots"], 1.9, 85.0, (0.85, 0.15, 0.15), temp="Protective",
        inten=0.7, source="DC (JSA)"), "Alan Scott: the golden-age Green Lantern (weak to wood - here just a legend)."),
    P(["jessica cruz", "green lantern jessica cruz"],
      R("Jessica Cruz", "Biped", ["Flies", "Guard", "Barrier"], 1.8, 65.0, LANTERN_GREEN, temp="Fearful", inten=0.6,
        source="DC (Green Lantern)"), "Jessica Cruz: the Lantern who overcomes fear - shields first."),
    P(["simon baz", "green lantern simon baz"],
      R("Simon Baz", "Biped", ["Flies", "Shoots", "Charge"], 1.9, 80.0, LANTERN_GREEN, temp="Aggressive", inten=0.6,
        source="DC (Green Lantern)"), "Simon Baz: Lantern with a sidearm and a temper."),
    P(["abin sur", "abin sur's ship"],
      R("Abin Sur", "Biped", ["Flies", "Shoots"], 2.0, 80.0, (0.8, 0.2, 0.3), temp="Protective", inten=0.6,
        source="DC (Green Lantern)"), "Abin Sur: the dying Lantern who chose Hal."),
    P(["tomar-re", "tomar re", "tomar-tu"],
      R("Tomar-Re", "Bird", ["Flies", "Shoots"], 1.8, 50.0, LANTERN_GREEN, temp="Protective", inten=0.6,
        source="DC (Green Lantern)"), "Tomar-Re: the beaked Lantern of Xudar."),
    P(["salaak", "salakk"],
      R("Salaak", "Biped", ["Guard", "Grab"], 2.0, 70.0, LANTERN_GREEN, temp="Protective", inten=0.6,
        source="DC (Green Lantern)"), "Salaak: four-armed Oan administrator."),
    P(["sodam yat", "ion", "the ion entity"],
      R("Ion", "Bird", ["Flies", "Beam", "Guard"], 4.0, 50.0, LANTERN_GREEN, temp="Protective", inten=0.9,
        source="DC (Green Lantern)"), "Ion: the willpower entity, a great green light-whale-thing (Protective)."),
    P(["the butcher", "rage entity", "butcher entity"],
      R("The Butcher", "Quadruped", ["Charge", "Breath"], 4.0, 900.0, RAGE_RED, elem="Fire", temp="Aggressive",
        inten=1.0, source="DC (Red Lantern Corps)"), "The Butcher: the red rage entity, a burning bull."),
    P(["ophidian", "avarice entity"],
      R("Ophidian", "Serpent", ["Grab", "Constrict"], 6.0, 600.0, AVARICE_ORANGE, temp="Aggressive", inten=0.8,
        source="DC (Orange Lantern Corps)"), "Ophidian: the orange avarice serpent-entity."),
    P(["adara", "hope entity"],
      R("Adara", "Bird", ["Flies", "Guard", "Barrier"], 5.0, 100.0, HOPE_BLUE, temp="Protective", inten=1.0,
        source="DC (Blue Lantern Corps)"), "Adara: the blue hope entity, a great bird."),
    P(["proselyte", "compassion entity"],
      R("Proselyte", "Serpent", ["Constrict", "Portal"], 5.0, 300.0, COMPASSION_INDIGO, temp="Calm", inten=0.5,
        source="DC (Indigo Tribe)"), "Proselyte: the indigo compassion entity (a squid-like thing)."),
    P(["predator entity", "love entity", "the predator"],
      R("Predator Entity", "Biped", ["Trap", "Grab"], 3.0, 200.0, LOVE_VIOLET, temp="Sneaky", inten=0.7,
        source="DC (Star Sapphires)"), "The Predator: the violet love entity - traps in crystal."),
    P(["black hand", "william hand", "black hand black lantern"],
      R("Black Hand", "Biped", ["Grab", "Pull"], 1.9, 80.0, (0.15, 0.15, 0.18), elem="Shadow", temp="Sneaky",
        inten=0.8, source="DC (Black Lantern Corps)"), "Black Hand: the first Black Lantern (no reviving players, ever)."),
    P(["krona", "krona the guardian"],
      R("Krona", "Biped", ["Beam", "Flies"], 2.2, 90.0, (0.6, 0.6, 0.75), elem="Psychic", temp="Chaotic", inten=0.8,
        source="DC (Green Lantern)"), "Krona: the renegade Guardian."),
    P(["yellow lantern construct", "fear construct", "sinestro corps construct", "yellow construct"],
      R("Fear Construct", "Blob", ["Flies", "Grab"], 2.4, 60.0, FEAR_YELLOW, elem="Shadow", temp="Sneaky", inten=0.9,
        source="DC (Sinestro Corps)"), "A Sinestro Corps fear construct: a nightmare that flanks from the dark."),
]


def build_all():
    return [hal_jordan, john_stewart, saint_walker, arkillo, glomulus, central_power_battery, guardians] + PROC
