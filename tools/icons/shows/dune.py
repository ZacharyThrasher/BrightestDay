"""Dune (Villeneuve films; names only). film_tv.ron already owns shai-hulud/sandworm, the thumper and the ornithopter;
film_tv_names.txt owns 'paul atreides', 'muad'dib', 'harkonnen', 'baron harkonnen', 'sardaukar', 'carryall',
'spice harvester', 'holtzman shield', 'dune shield', 'dune crysknife', 'gom jabbar' - this file upgrades those concepts
to hand-authored icons under distinct aliases (no shared names)."""
import math

from reg import entry, simple, R, S
from slib import ALONG_X, ALONG_Z, TIP_NZ, TIP_NY
from rig import head, cape, humanoid, tail
from bodies import person, held, wheels

SPICE = (1.0, 0.55, 0.15)
FREMEN_BLUE = (0.2, 0.45, 1.0)
HARK = (0.2, 0.2, 0.22)


@entry
def lasgun():
    s = S()
    s.box((0, 0, 0.0), (1.4, 2.0, 7.0), "D")                                                    # body
    s.box((0, -1.8, 1.2), (1.0, 2.8, 1.4), "D", (15, 0, 0))                                      # grip
    s.box((0, 0.2, 5.0), (1.2, 1.8, 3.4), "D")                                                   # stock
    s.cz((0, 0.4, -5.6), 1.4, 5.0, "A")                                                          # emitter barrel
    s.cz((0, 0.4, -8.6), 0.8, 1.6, "C")
    s.box((0, 1.4, -1.0), (0.6, 0.8, 4.0), "C")                                                  # charge cell glow
    return (["lasgun", "dune lasgun", "atreides lasgun", "laser gun dune"],
            R("Lasgun", "Gun", ["Beam", "Pierce"], 1.0, 4, (1.0, 0.6, 0.25), "Fire", source="Dune"),
            s, "A lasgun: a continuous cutting Beam (Pierce). Never fire one at a shield...")


@entry
def holtzman_field():
    s = S()
    s.ell((0, 8.0, 0), (14.0, 16.0, 14.0), "A")                                                  # the shimmering field
    s.ell((0, 8.0, 0), (12.6, 14.6, 12.6), "A", sub=True)
    for i in range(4):
        s.ell((0, 2.0 + i * 3.6, 0), (14.2 - abs(i - 1.5) * 1.2, 0.6, 14.2 - abs(i - 1.5) * 1.2), "C")   # shimmer bands
    s.box((0, 7.0, 0), (4.6, 1.2, 3.0), "D")                                                     # the belt generator
    s.box((0, 7.0, -1.6), (1.4, 1.0, 0.4), "C")
    return (["holtzman field", "holtzman shield", "personal shield dune", "atreides shield belt", "dune shield belt", "body shield dune",
             "the slow blade penetrates the shield"],
            R("Holtzman Field", "Dome", ["Guard", "Barrier"], 2.2, 5, (0.4, 0.7, 1.0), source="Dune"),
            s, "A Holtzman personal shield: a shimmering Dome that Guards against fast projectiles.")


@entry
def crysknife():
    s = S()
    s.box((0, 0, 0.8), (1.2, 1.4, 3.2), "D")                                                     # grip
    s.box((0, 0, -1.0), (2.2, 0.8, 0.8), "D")
    for i in range(5):                                                                             # curved milky blade
        z = -2.0 - i * 1.8
        s.box((0, 0.2 + math.sin(i * 0.5) * 0.9, z), (0.6, 1.8 - i * 0.25, 2.0), "C", (-i * 6, 0, 0))
    s.cone((0, 1.6, -11.4), (0.6, 2.2, 0.8), "C", (-100, 0, 0))
    return (["crysknife", "dune crysknife", "fremen crysknife", "maker's tooth", "sandworm tooth knife", "sacred crysknife"],
            R("Crysknife", "Sword", ["Blade", "Pierce"], 0.6, 1, (0.95, 0.92, 0.85), source="Dune"),
            s, "A crysknife: carved from a sandworm's tooth, milky and curved. Fast Blade that Pierces.")


@entry
def baron():
    s = S()
    s.on(0)
    s.ell((0, 12.0, 0), (14.0, 14.0, 12.0), "D")                                                   # immense floating body
    s.box((0, 5.6, 0), (9.0, 4.0, 8.0), "D")                                                       # robes
    for sx in (-1, 1):
        s.box((sx * 2.4, 3.4, -0.6), (3.0, 2.4, 3.6), "D")                                         # dangling legs
    s.cyl((0, 1.0, 0), (8.0, 0.6, 8.0), "A")                                                       # suspensor glow
    s.cyl((0, 0.2, 0), (5.0, 0.4, 5.0), "C")
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):
        s.on(pid)
        s.seg((sx * 6.4, 15.0, 0), (sx * 8.4, 9.6, -2.4), 3.0, 3.0, "D")
        s.ell((sx * 8.6, 8.6, -2.8), (2.6, 2.6, 2.6), "P")
        s.part(pid, 0, (sx * 6.4, 15.0, 0), role)
    s.on(5)
    s.ell((0, 21.0, -0.6), (6.4, 6.4, 6.4), "P")                                                    # bald white head
    for sx in (-1, 1):
        s.box((sx * 1.3, 21.6, -3.6), (1.0, 0.7, 0.3), "D")
    s.part(5, 0, (0, 18.0, 0), "Head")
    s.on(0)
    return (["vladimir harkonnen", "baron harkonnen", "baron vladimir harkonnen", "the baron harkonnen", "harkonnen baron",
             "floating baron", "baron with suspensors"],
            R("Baron Harkonnen", "Biped", ["Flies", "Heavy", "Grab"], 2.6, 300, HARK, "Shadow", temp="Sneaky", inten=0.9,
              source="Dune"),
            s, "Baron Vladimir Harkonnen: immense, pale, floating on suspensors. Flies but Heavy, drifts in and Grabs.")


@entry
def feyd():
    s = S()
    b = person(s, top="D", pants="D", hair=None, style="bald", arms="D", hands="P", skin="C")
    s.on(5)
    s.box((0, b["hy"] - 1.4, -2.1), (1.8, 0.6, 0.3), "D")                                          # black teeth
    hx, hy, hz = held(s, b)
    s.box((hx, hy, hz - 3.6), (0.4, 0.8, 6.4), "C")                                                 # knife
    s.on(3)
    hx, hy, hz = b["hands"][-1]
    s.box((hx, hy, hz - 3.0), (0.4, 0.8, 5.0), "C")
    s.on(0)
    return (["feyd-rautha", "feyd rautha", "feyd-rautha harkonnen", "feyd harkonnen", "na-baron feyd"],
            R("Feyd-Rautha", "Biped", ["Blade", "Fast", "Pierce"], 1.9, 75, HARK, temp="Aggressive", inten=1.0,
              source="Dune"),
            s, "Feyd-Rautha: bald, chalk-white, black-toothed, twin knives. Fast, Aggressive gladiator.")


@entry
def navigator():
    s = S()
    for sx in (-1, 1):
        for sz in (-1, 1):
            s.box((sx * 6.0, 11.0, sz * 6.0), (1.2, 22.0, 1.2), "D")                                    # tank frame
    s.box((0, 11.0, 0), (10.6, 19.0, 10.6), "A")                                                    # orange spice gas
    s.ell((0, 11.0, -4.6), (6.0, 11.0, 4.4), "P")                                                   # the mutated navigator
    s.ell((0, 18.0, -5.0), (5.4, 4.8, 4.0), "P")
    for sx in (-1, 1):
        s.box((sx * 1.3, 18.4, -7.0), (1.2, 0.9, 0.3), "C")
        s.ell((sx * 3.0, 10.0, -6.0), (1.4, 5.0, 1.4), "P", (0, 0, sx * 15))
    s.box((0, 0.8, 0), (13.0, 1.6, 13.0), "D")                                                      # base
    s.box((0, 22.6, 0), (13.0, 1.6, 13.0), "D")
    return (["guild navigator", "spacing guild navigator", "third stage guild navigator", "navigator tank",
             "folding space", "fold space"],
            R("Guild Navigator", "Blob", ["Flies", "Portal"], 3.0, 900, SPICE, "Psychic", temp="Calm", inten=1.0,
              source="Dune"),
            s, "A Spacing Guild navigator in its spice-gas tank: it folds space (Portal) and floats serenely (Flies).")


@entry
def spice_crawler():
    s = S()
    s.box((0, 12.0, 0), (18.0, 16.0, 30.0), "A")                                                    # the crawler hull
    s.box((0, 21.0, -4.0), (12.0, 4.0, 14.0), "D")                                                   # upper works
    s.box((0, 21.0, -11.1), (8.0, 2.0, 0.3), "C")                                                    # bridge windows
    for i in range(3):
        s.cyl((-5.0 + i * 5.0, 26.0, 6.0), (2.0, 8.0, 2.0), "D")                                      # vents / stacks
    s.cone((0, 3.0, -17.0), (12.0, 6.0, 4.0), "D", (-90, 0, 0))                                      # intake scoop
    s.box((0, 2.0, 0), (22.0, 4.0, 26.0), "D")                                                       # tread housings
    for sx in (-1, 1):
        for i in range(5):
            s.box((sx * 10.6, 2.0, -11.0 + i * 5.5), (1.0, 3.6, 3.0), "P")                             # tread pads
    s.on(30)
    s.ell((0, 31.4, 6.0), (8.0, 4.0, 6.0), "A")                                                      # spice plume
    s.part(30, 0, (0, 30.0, 6.0), "Exhaust")
    s.on(0)
    return (["spice crawler", "spice harvester", "harvester dune", "harkonnen harvester", "dune spice harvester", "spice factory"],
            R("Spice Crawler", "Vehicle", ["Attract", "Heavy"], 12.0, 60000, SPICE, "Earth",
              feat=["Treads", "Pipes", "Smokestack"], source="Dune"),
            s, "A spice harvester: a huge tracked factory that vacuums up spice - and anyone near it (Attract). Worm sign!")


@entry
def dune_carryall():
    s = S()
    s.box((0, 12.0, 0), (6.0, 4.0, 24.0), "D")                                                      # spine
    s.box((0, 13.6, -12.0), (5.0, 3.0, 5.0), "A")                                                   # cockpit
    s.box((0, 13.6, -14.6), (4.0, 1.6, 0.3), "C")
    for sx in (-1, 1):
        s.box((sx * 9.0, 13.0, -4.0), (14.0, 1.0, 5.0), "A", (0, 0, sx * 6))                          # wings
        s.box((sx * 9.0, 13.0, 8.0), (14.0, 1.0, 5.0), "A", (0, 0, sx * 6))
        s.cyl((sx * 15.0, 13.8, -4.0), (3.0, 2.0, 3.0), "D")                                          # thrusters
        s.cyl((sx * 15.0, 13.8, 8.0), (3.0, 2.0, 3.0), "D")
        s.box((sx * 4.6, 6.0, 2.0), (0.8, 10.0, 0.8), "C")                                            # grapple cables
        s.box((sx * 4.6, 1.2, 2.0), (2.0, 1.4, 6.0), "D")                                             # clamps
    s.on(30)
    for sx in (-1, 1):
        for z in (-4.0, 8.0):
            s.cone((sx * 15.0, 11.2, z), (2.4, 3.0, 2.4), "C", TIP_NY)
    s.part(30, 0, (0, 12.6, 2.0), "Exhaust")
    s.on(0)
    return (["dune carryall", "carryall", "spice carryall", "harvester carryall", "carryall dune", "the carryall dune"],
            R("Carryall", "Vehicle", ["Flies", "Grab"], 14.0, 30000, (0.6, 0.55, 0.45), feat=["Jet", "CraneArm"],
              source="Dune"),
            s, "A carryall: a flying crane that lifts harvesters out of the sand - it Grabs and carries players too.")


@entry
def ride_the_worm():
    s = S()
    n = 6
    pts = []
    for i in range(n + 1):                                                                           # an arched worm body
        f = i / n
        pts.append((math.sin(f * 3.0) * 4.0, 6.0 + math.sin(f * math.pi) * 10.0, -22.0 + f * 46.0))
    s.on(5)
    s.seg(pts[0], pts[1], 12.0, 12.0, "A", kind="Cylinder")
    s.cyl((pts[0][0], pts[0][1] + 0.4, pts[0][2] - 3.2), (13.0, 1.4, 13.0), "D", (90, 0, 0))            # mouth ring
    for i in range(10):
        a = math.radians(i * 36)
        s.cone((pts[0][0] + math.cos(a) * 4.4, pts[0][1] + math.sin(a) * 4.4, pts[0][2] - 3.0), (0.8, 3.0, 0.8), "C",
               (-90, 0, 0))                                                                           # crystal teeth
    s.part(5, 0, pts[1], "Head")
    s.on(0)
    for i in range(1, 3):
        s.seg(pts[i], pts[i + 1], 12.0 - i * 0.6, 12.0 - i * 0.6, "A", kind="Cylinder")
    for i in range(1, 6):
        s.cyl(pts[i], (12.6 - i * 0.6, 0.8, 12.6 - i * 0.6), "D", (70, 0, 0))                           # ring segments
    tail(s, pts[3:], [(10.0, 10.0, 1), (9.0, 9.0, 1), (7.0, 7.0, 1)], "A", kind="seg")
    top = pts[2]
    s.on(255)
    s.box((top[0], top[1] + 6.4, top[2]), (1.6, 3.6, 1.2), "P")                                       # the rider
    s.ell((top[0], top[1] + 8.8, top[2]), (1.4, 1.4, 1.4), "P")
    s.seat(0, (top[0], top[1] + 6.0, top[2]))
    s.on(0)
    for sx in (-1, 1):
        s.seg((top[0] + sx * 0.8, top[1] + 7.0, top[2] - 0.6), (top[0] + sx * 3.6, top[1] + 5.2, top[2] - 4.4), 0.4, 0.4,
              "C")                                                                                     # maker hooks
    return (["ride the worm", "sandworm riding", "worm rider", "maker hooks", "sandrider", "ride a sandworm",
             "shai-hulud rider"],
            R("Worm Rider", "Kaiju", ["Piloted", "Grab", "Charge", "Stomp"], 20.0, 100000, SPICE, "Earth",
              temp="Aggressive", inten=0.6, source="Dune"),
            s, "Riding Shai-Hulud: set your maker hooks and pilot the sandworm itself - it Charges and swallows.")


@entry
def usul():
    s = S()
    b = person(s, top="P", pants="P", hair="D", style="long", arms="P")
    s.on(0)
    for i in range(3):
        s.box((0, b["y_hip"] + 1.4 + i * 2.6, 0), (b["tw"] + 0.3, 0.6, b["td"] + 0.3), "D")               # stillsuit ribs
    s.box((0, b["y_sh"] + 0.4, -b["td"] / 2 - 0.2), (1.2, 1.4, 0.8), "D")                                # nose-plug tube
    cape(s, b, t="A", flare=2.0)
    s.on(5)
    for sx in (-1, 1):
        s.box((sx * 0.8, b["hy"] + 0.3, -2.1), (0.8, 0.5, 0.3), "C")                                     # blue-in-blue eyes
    hx, hy, hz = held(s, b)
    s.box((hx, hy, hz - 3.6), (0.5, 1.2, 5.4), "C")                                                       # crysknife
    s.on(0)
    return (["usul", "paul atreides", "muad'dib", "kwisatz haderach", "lisan al gaib", "lisan al-gaib", "paul muad'dib atreides", "paul usul",
             "the voice from the outer world"],
            R("Paul (Usul)", "Biped", ["Blade", "Guard", "Repel"], 1.8, 70, FREMEN_BLUE, "Psychic", temp="Calm",
              inten=0.9, source="Dune"),
            s, "Paul as Usul: stillsuit, desert cloak, crysknife, blue-in-blue eyes. Blade + Guard, and the Voice (Repel).")


@entry
def chani():
    s = S()
    b = person(s, top="P", pants="P", fem=True, hair="D", style="long", arms="P")
    s.on(0)
    for i in range(3):
        s.box((0, b["y_hip"] + 1.4 + i * 2.4, 0), (b["tw"] + 0.3, 0.6, b["td"] + 0.3), "D")
    cape(s, b, t="A", flare=1.4)
    s.on(5)
    s.ell((0, b["hy"] + 0.8, 0.8), (4.6, 4.6, 4.6), "A")                                                  # hood
    s.on(0)
    return (["chani", "chani kynes", "sihaya", "chani dune"],
            R("Chani", "Biped", ["Blade", "Shoots"], 1.7, 55, FREMEN_BLUE, "Earth", temp="Sneaky", inten=0.9,
              source="Dune"),
            s, "Chani: Fremen stillsuit and hooded cloak. A Sneaky desert fighter.")


simple(["stillsuit", "fremen stillsuit", "dune stillsuit"],
       R("Stillsuit", "Cape", ["Guard", "Glide"], 1.6, 4, (0.45, 0.4, 0.35), "Water"), "Recycles every drop.")
simple(["fremen", "fremen warriors", "fedaykin", "fremen fedaykin", "fremen squad"],
       R("Fremen Fedaykin", "Biped", ["Blade", "Swarm"], 1.8, 65, FREMEN_BLUE, "Earth", count=4, temp="Sneaky",
         inten=0.9), "Rise from the sand: a Sneaky squad.")
simple(["sardaukar legion", "sardaukar", "sardaukar troops", "emperor's sardaukar", "sardaukar terror troops"],
       R("Sardaukar Legion", "Biped", ["Blade", "Swarm", "Heavy"], 1.9, 90, (0.25, 0.22, 0.2), count=4,
         temp="Aggressive", inten=1.0), "The Emperor's terror troops.")
simple(["stilgar", "stilgar of sietch tabr", "stilgar dune"],
       R("Stilgar", "Biped", ["Blade", "Guard"], 1.9, 80, FREMEN_BLUE, "Earth", temp="Protective", inten=0.9),
       "Naib of Sietch Tabr; true believer.")
simple(["duncan idaho", "swordmaster duncan idaho"],
       R("Duncan Idaho", "Biped", ["Blade", "Guard"], 1.9, 85, (0.3, 0.45, 0.3), temp="Protective", inten=1.0),
       "The swordmaster who holds the line.")
simple(["gurney halleck", "gurney's baliset"],
       R("Gurney Halleck", "Biped", ["Blade", "Repel"], 1.9, 90, (0.4, 0.35, 0.3), temp="Protective", inten=0.8),
       "Warrior-troubadour.")
simple(["glossu rabban", "rabban the beast", "beast rabban harkonnen"],
       R("Rabban", "Biped", ["Blunt", "Heavy"], 2.0, 130, HARK, temp="Aggressive", inten=1.0), "The Beast.")
simple(["lady jessica", "jessica atreides", "reverend mother jessica"],
       R("Lady Jessica", "Biped", ["Repel"], 1.7, 55, (0.4, 0.3, 0.5), "Psychic", temp="Protective", inten=0.8),
       "Bene Gesserit; uses the Voice.")
simple(["bene gesserit voice", "the voice dune", "using the voice", "bene gesserit"],
       R("The Voice", "Staff", ["Repel"], 0.6, 0.2, (0.6, 0.4, 0.8), "Psychic"),
       "A commanding shout that shoves (never mind control).")
simple(["hunter-seeker", "hunter seeker dune", "dune hunter seeker"],
       R("Hunter-Seeker", "Rocket", ["Homing", "Pierce"], 0.3, 0.2, (0.7, 0.7, 0.75)), "A tiny homing needle drone.")
simple(["spice melange", "the spice must flow", "arrakis spice", "spice blow"],
       R("Spice Melange", "Ball", ["Grow", "Explodes"], 0.6, 1, SPICE, "Psychic"),
       "The spice extends life and expands... everything (Grow).")
simple(["sietch tabr", "fremen sietch"],
       R("Sietch", "Dome", ["Barrier"], 6.0, 5000, (0.8, 0.6, 0.4), "Earth"), "A hidden rock refuge.")
simple(["atreides banner", "house atreides", "atreides hawk"],
       R("House Atreides", "Bird", ["Flies", "Pierce"], 1.4, 5, (0.2, 0.55, 0.3), temp="Protective", inten=0.8),
       "The Atreides hawk.")
simple(["house harkonnen", "harkonnen troops", "harkonnen soldiers"],
       R("Harkonnen Troops", "Biped", ["Shoots", "Swarm"], 1.8, 80, HARK, count=4, temp="Aggressive", inten=0.9),
       "Black-armoured Giedi Prime soldiers.")
simple(["water of life", "the water of life dune"],
       R("Water of Life", "Ball", ["Explodes"], 0.4, 0.5, (0.3, 0.6, 1.0), "Poison"), "Worm-bile poison.")
