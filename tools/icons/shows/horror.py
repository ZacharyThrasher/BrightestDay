"""Slasher / horror icons: A Nightmare on Elm Street, Friday the 13th, Halloween, Child's Play, Scream, It, The Texas
Chainsaw Massacre, Saw, The Ring, Weapons, The Shining, Final Destination, Five Nights at Freddy's, Evil Dead..."""
import math

from reg import entry, simple, R, S
from slib import ALONG_X, ALONG_Z, TIP_NZ, TIP_NY
from rig import head, cape
from bodies import person, held, wheels


def blades_glove(s, x, y, z, k=1.0, t="C"):
    """Freddy's glove at a hand position: leather back + four long finger blades pointing -Z."""
    s.box((x, y, z - 0.4 * k), (2.2 * k, 1.2 * k, 2.4 * k), "A")
    for i in range(4):
        bx = x - 0.9 * k + i * 0.6 * k
        s.box((bx, y - 0.1, z - 1.8 * k), (0.3, 0.5, 1.4 * k), "D")
        s.box((bx, y - 0.1, z - 4.4 * k), (0.25, 0.7, 4.0 * k), t)


@entry
def krueger():
    s = S()
    b = person(s, top="A", pants="D", hair=None, style="bald", arms="A")
    s.on(0)
    for i in range(4):
        s.box((0, b["y_hip"] + 1.2 + i * 2.2, 0), (b["tw"] + 0.25, 1.0, b["td"] + 0.25), "P")     # red/green stripes
    s.on(5)
    hy = b["hy"]
    s.cyl((0, hy + 2.0, 0), (6.6, 0.4, 6.6), "D")                                               # battered fedora
    s.cyl((0, hy + 3.0, 0), (4.2, 2.0, 4.2), "D")
    s.box((0, hy - 0.4, -2.1), (3.2, 3.6, 0.4), "P")                                            # scarred face
    hx, hyy, hz = held(s, b)
    blades_glove(s, hx, hyy, hz)
    s.on(0)
    return (["fred krueger", "freddy krueger", "springwood slasher", "nightmare on elm street", "a nightmare on elm street",
             "elm street killer", "the dream demon", "one two freddy's coming for you"],
            R("Freddy Krueger", "Biped", ["Blade", "Grab"], 1.8, 70, (0.75, 0.12, 0.1), "Shadow", temp="Sneaky",
              inten=0.9, source="A Nightmare on Elm Street"),
            s, "Freddy: striped sweater, fedora, burned face, the bladed glove. Stalks you in your dreams (Sneaky, Shadow).")


@entry
def razor_glove():
    s = S()
    blades_glove(s, 0, 0, 0, k=1.6)
    s.box((0, 0, 1.8), (2.6, 2.0, 3.0), "A")                                                    # cuff
    return (["razor glove", "freddy krueger's glove", "krueger glove", "freddy glove", "bladed glove", "elm street glove"],
            R("Razor Glove", "Gauntlet", ["Blade", "Grab"], 0.6, 1.5, (0.75, 0.12, 0.1), "Shadow",
              source="A Nightmare on Elm Street"),
            s, "The bladed glove: a Gauntlet with four long finger-knives.")


@entry
def jason():
    s = S()
    b = person(s, top="P", pants="D", hair=None, style="bald", bulk=1.2, tall=1.0, arms="P")
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy, -1.6), (4.2, 5.0, 1.6), "C")                                                  # hockey mask
    for (x, y) in ((-0.9, 0.6), (0.9, 0.6)):
        s.box((x, hy + y, -2.5), (0.9, 0.6, 0.3), "D")
    for (x, y) in ((-1.2, -0.8), (-0.4, -1.2), (0.4, -1.2), (1.2, -0.8), (0, -2.0)):
        s.box((x, hy + y, -2.45), (0.3, 0.3, 0.3), "D")
    s.box((0, hy + 1.8, -2.4), (0.6, 0.8, 0.3), "A", (0, 0, 30))                                 # red chevrons
    hx, hyy, hz = held(s, b)
    s.box((hx, hyy, hz - 0.8), (0.6, 1.2, 2.0), "D")                                             # machete
    s.box((hx, hyy - 0.4, hz - 6.4), (0.3, 2.2, 9.6), "C")
    s.on(0)
    return (["jason vorhees", "jason voorhees", "friday the 13th jason", "crystal lake killer", "camp crystal lake killer",
             "jason hockey mask", "ki ki ki ma ma ma"],
            R("Jason Voorhees", "Biped", ["Blade", "Heavy"], 2.0, 120, (0.9, 0.2, 0.15), temp="Aggressive", inten=0.8,
              source="Friday the 13th"),
            s, "Jason: hockey mask, machete. He only walks - Heavy and slow - but never stops coming (Aggressive).")


@entry
def myers():
    s = S()
    b = person(s, top="P", pants="P", hair=None, style="bald", arms="P", bulk=1.1, tall=0.8)
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy, 0), (4.0, 4.8, 4.2), "C")                                                     # blank white mask
    s.ell((0, hy + 1.6, 0.6), (4.4, 2.6, 4.2), "D")                                             # hair
    for sx in (-1, 1):
        s.box((sx * 0.9, hy + 0.4, -2.05), (0.8, 0.5, 0.3), "D")
    hx, hyy, hz = held(s, b)
    s.box((hx, hyy + 0.8, hz - 0.2), (0.6, 2.2, 0.8), "D")                                        # kitchen knife, overhand
    s.box((hx, hyy + 4.0, hz - 0.2), (0.3, 4.6, 1.2), "C")
    s.on(0)
    return (["the shape halloween", "michael myers", "haddonfield killer", "michael myers mask", "halloween killer",
             "michael audrey myers"],
            R("Michael Myers", "Biped", ["Blade", "Heavy"], 2.0, 110, (0.3, 0.35, 0.45), temp="Sneaky", inten=1.0,
              source="Halloween"),
            s, "The Shape: boiler suit, blank white mask, kitchen knife. Never runs; appears from out of view (Sneaky).")


@entry
def chucky():
    s = S()
    b = person(s, top="P", pants="P", shirt="A", hair="A", style="spiky", head_w=4.4, head_h=4.8)
    s.on(0)
    for sx in (-1, 1):
        s.box((sx * 1.8, b["y_sh"] - 2.2, -b["td"] / 2 - 0.2), (0.8, 4.0, 0.3), "P")               # overall straps
    s.box((0, b["y_hip"] + 2.2, -b["td"] / 2 - 0.2), (3.0, 2.0, 0.3), "C")                        # Good Guys patch
    s.on(5)
    for (x, y) in ((-1.0, -0.4), (0.8, 0.8), (1.2, -1.2)):
        s.box((x, b["hy"] + y, -2.35), (1.2, 0.3, 0.3), "D", (0, 0, 40))                           # stitched scars
    hx, hyy, hz = held(s, b)
    s.box((hx, hyy + 0.8, hz - 0.2), (0.6, 2.0, 0.8), "D")
    s.box((hx, hyy + 3.8, hz - 0.2), (0.3, 4.4, 1.2), "C")
    s.on(0)
    return (["charles lee ray", "chucky", "chucky doll", "child's play doll", "childs play chucky", "hi i'm chucky"],
            R("Chucky", "Biped", ["Blade", "Fast"], 0.8, 8, (0.95, 0.4, 0.1), temp="Chaotic", inten=1.0,
              source="Child's Play"),
            s, "Chucky: a Good Guys doll in overalls with stitched scars and a knife. Tiny, Fast, pure Chaos.")


@entry
def ghostface():
    s = S()
    b = person(s, top="D", pants="D", hair=None, style="bald", arms="D", hands="D")
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy + 0.4, 0.3), (5.0, 6.2, 5.0), "D")                                              # hood
    s.ell((0, hy - 0.2, -2.0), (3.4, 6.0, 1.4), "C")                                              # the long drooping mask
    for sx in (-1, 1):
        s.ell((sx * 0.8, hy + 1.2, -2.6), (1.0, 1.8, 0.5), "D", (0, 0, sx * 15))
    s.ell((0, hy - 1.8, -2.6), (1.0, 2.0, 0.5), "D")
    s.on(7)
    for i in range(5):
        s.cone((-3.6 + i * 1.8, 0.6, 0.4), (1.8, 2.4, 1.0), "D", (180, 0, 0))                    # tattered robe hem
    s.box((0, 6.0, 0.4), (b["tw"] + 3.0, 10.0, b["td"] + 2.4), "D")                               # robe
    s.part(7, 0, (0, b["y_hip"], 0), "Cloth")
    hx, hyy, hz = held(s, b)
    s.box((hx, hyy + 0.8, hz - 0.2), (0.6, 2.0, 0.8), "D")
    s.box((hx, hyy + 4.0, hz - 0.2), (0.3, 4.8, 1.3), "C")
    s.on(0)
    return (["scream ghostface", "ghostface", "ghost face killer", "ghostface mask", "scream mask", "ghostface scream 7",
             "what's your favorite scary movie"],
            R("Ghostface", "Biped", ["Blade", "Fast"], 1.8, 70, (0.95, 0.95, 0.95), temp="Sneaky", inten=0.8,
              source="Scream"),
            s, "Ghostface: black tattered robe, the long white screaming mask, a hunting knife. Sneaky phone-call stalker.")


@entry
def pennywise():
    s = S()
    b = person(s, top="C", pants="C", hair=None, style="bald", tall=2.0, arms="C", hands="C", head_w=4.6, head_h=5.8)
    s.on(0)
    for i in range(3):
        s.cyl((0, b["y_sh"] + 0.2 - i * 0.6, 0), (b["tw"] + 3.6 - i * 1.2, 0.8, b["td"] + 3.0 - i * 1.0), "A")   # ruff
    for i in range(3):
        s.ell((0, b["y_sh"] - 2.4 - i * 2.6, -b["td"] / 2 - 0.3), (1.2, 1.2, 0.8), "A")            # pom-poms
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy + 1.2, 0), (4.8, 5.0, 4.6), "C")                                                # huge forehead
    for sx in (-1, 1):
        s.ell((sx * 2.8, hy + 0.8, 0.6), (2.2, 3.0, 3.0), "A")                                    # red hair tufts
        s.box((sx * 1.0, hy + 0.4, -2.35), (0.9, 0.7, 0.3), "A")
        s.box((sx * 0.9, hy - 1.2, -2.3), (0.3, 2.6, 0.3), "A", (0, 0, sx * 10))                   # red lip lines
    s.box((0, hy - 2.2, -2.2), (2.4, 0.5, 0.3), "A")
    hx, hyy, hz = held(s, b, -1)
    s.box((hx, hyy + 6.0, hz), (0.2, 12.0, 0.2), "D")                                            # the balloon
    s.ell((hx, hyy + 14.0, hz), (4.4, 5.2, 4.4), "A")
    s.on(0)
    return (["pennywise the dancing clown", "pennywise", "pennywise the clown", "robert gray", "we all float down here",
             "derry clown", "you'll float too"],
            R("Pennywise", "Biped", ["Grab", "Shoots"], 2.1, 70, (0.9, 0.12, 0.12), temp="Chaotic", inten=0.9,
              source="It", payload=R("red balloon", "Ball", ["Homing", "Explodes", "SlowFall"], 0.6, 0.2,
                                      (0.9, 0.12, 0.12), temp="Playful", inten=0.8)),
            s, "Pennywise: ruffled clown suit, red hair tufts, a red balloon. Grabs - and floats homing red balloons that pop.")


@entry
def red_balloon():
    s = S()
    s.ell((0, 12.0, 0), (7.0, 8.4, 7.0), "A")
    s.cone((0, 7.4, 0), (1.6, 1.4, 1.6), "A", TIP_NY)
    s.ell((-2.0, 13.6, -2.6), (1.4, 2.2, 0.6), "C")                                              # shine
    s.box((0, 3.4, 0), (0.25, 7.0, 0.25), "D")                                                   # string
    return (["georgie's red balloon", "red balloon it", "pennywise balloon", "pennywise's balloon", "derry red balloon"],
            R("Red Balloon", "Ball", ["Homing", "Explodes", "SlowFall"], 0.8, 0.2, (0.9, 0.12, 0.12), temp="Playful",
              inten=0.8, source="It"),
            s, "A red balloon: drifts toward you (Homing, SlowFall)... and pops (Explodes).")


@entry
def bubba():
    s = S()
    b = person(s, top="P", pants="D", hair="D", style="short", bulk=1.4, shirt="C", belly=1.6, arms="P")
    s.on(0)
    s.box((0, b["y_hip"] + 3.0, -b["td"] / 2 - 1.4), (b["tw"] * 0.8, 9.0, 0.5), "C")              # butcher's apron
    s.box((0, b["y_sh"] - 0.4, -b["td"] / 2 - 0.2), (3.0, 0.5, 0.4), "D")                          # tie
    s.on(5)
    s.box((0, b["hy"] - 0.2, -2.1), (3.6, 3.8, 0.6), "A")                                         # the skin mask
    s.on(4)
    x = b["arm_x"]
    s.seg((x, b["y_sh"] - 0.4, 0), (x - 1.0, b["y_sh"] + 5.0, -1.4), 2.4, 2.4, "P")                 # arm raised
    s.box((x - 1.0, b["y_sh"] + 6.4, -1.8), (3.0, 3.2, 5.0), "D")                                   # chainsaw body
    s.box((x - 1.0, b["y_sh"] + 12.4, -2.4), (0.8, 10.0, 2.4), "C", (15, 0, 0))                     # bar + chain
    s.on(0)
    return (["bubba sawyer", "leatherface", "jedidiah sawyer", "texas chainsaw massacre", "the texas chainsaw massacre",
             "chainsaw dance", "leatherface chainsaw dance"],
            R("Leatherface", "Biped", ["Blade", "Spin", "Charge"], 2.1, 150, (0.75, 0.6, 0.45), temp="Chaotic",
              inten=1.0, source="The Texas Chain Saw Massacre"),
            s, "Leatherface: butcher's apron, skin mask, chainsaw raised high. Charges and saws (Spin), totally Chaotic.")


@entry
def billy_puppet():
    s = S()
    s.box((0, 4.0, -2.0), (0.8, 0.8, 10.0), "D", (-15, 0, 0))                                    # tricycle frame
    s.box((0, 7.6, -6.6), (5.0, 0.6, 0.6), "D")                                                  # handlebars
    s.box((0, 4.6, 1.0), (3.0, 0.8, 3.0), "D")                                                   # seat
    s.box((0, 2.0, 4.4), (7.0, 0.6, 0.6), "D")                                                   # rear axle
    s.box((0, 8.2, 0.8), (3.0, 5.4, 2.6), "D")                                                   # little black suit
    s.box((0, 10.6, -0.6), (1.6, 0.8, 0.4), "A")                                                 # red bow tie
    s.ell((0, 13.2, 0.6), (4.2, 5.0, 4.2), "C")                                                  # white puppet face
    for sx in (-1, 1):
        s.cyl((sx * 1.3, 12.4, -1.4), (1.3, 0.3, 1.3), "A", (90, 0, 0))                          # red spiral cheeks
        s.box((sx * 0.9, 14.0, -1.6), (0.8, 0.6, 0.3), "A")                                      # red eyes
        s.seg((sx * 1.4, 10.2, 0.6), (sx * 2.4, 7.8, -6.4), 0.6, 0.6, "C")                         # arms to handlebars
    s.ell((0, 15.6, 0.6), (4.0, 1.8, 4.0), "D")                                                  # black hair
    wheels(s, [(0, -8.0)], d=6.0, w=1.0)
    wheels(s, [(-3.4, 4.4), (3.4, 4.4)], d=3.4, w=0.8, first=21)
    return (["billy the puppet", "jigsaw puppet", "saw puppet", "jigsaw's tricycle", "i want to play a game",
             "let's play a game jigsaw"],
            R("Billy the Puppet", "Vehicle", ["Charge", "Trap"], 1.2, 15, (0.8, 0.1, 0.1), feat=["Wheels"],
              temp="Playful", inten=0.8, source="Saw"),
            s, "Billy on his tricycle: rolls up, and whoever it rams gets caught in a game (Trap).")


@entry
def samara():
    s = S()
    b = person(s, top="C", pants="C", fem=True, skin="C", hair="D", style="long", arms="C", hands="C")
    s.on(0)
    s.box((0, b["y_hip"] - 3.0, 0), (b["tw"] + 1.0, 7.0, b["td"] + 1.0), "C")                    # nightgown
    s.on(5)
    s.box((0, b["hy"] - 2.0, -2.2), (3.8, 8.4, 0.8), "D")                                        # hair over the face
    s.on(3)
    s.seg((-b["arm_x"], b["y_sh"] - 0.4, 0), (-b["arm_x"] + 0.4, b["y_sh"] - 1.4, -7.0), 1.4, 1.4, "C")   # reaching
    s.on(4)
    s.seg((b["arm_x"], b["y_sh"] - 0.4, 0), (b["arm_x"] - 0.4, b["y_sh"] - 1.4, -7.0), 1.4, 1.4, "C")
    s.on(0)
    return (["samara morgan", "the ring girl", "samara the ring", "girl from the well"],
            R("Samara", "Biped", ["Grab"], 1.4, 35, (0.3, 0.45, 0.5), "Water", temp="Sneaky", inten=1.0,
              source="The Ring"),
            s, "Samara: white nightgown, wet black hair over her face, arms reaching out. Crawls toward you (Sneaky Grab).")


@entry
def gladys():
    s = S()
    b = person(s, top="A", pants="A", fem=True, hair="A", style="long", arms="A")
    s.on(5)
    s.ell((0, b["hy"] + 1.8, 1.2), (6.6, 4.6, 5.2), "A")                                          # the wild red wig
    for sx in (-1, 1):
        s.ell((sx * 2.8, b["hy"] - 0.4, 0.8), (2.0, 4.0, 3.6), "A")
    for sx in (-1, 1):
        s.ell((sx * 1.2, b["hy"] - 0.8, -2.0), (1.2, 0.8, 0.4), "P")                               # rouge
        s.box((sx * 0.9, b["hy"] + 0.4, -2.2), (0.9, 0.6, 0.3), "D")
    s.on(0)
    return (["aunt gladys", "gladys weapons", "weapons aunt gladys", "gladys lilly", "aunt gladys weapons"],
            R("Aunt Gladys", "Biped", ["Shoots"], 1.7, 55, (0.95, 0.3, 0.25), temp="Sneaky", inten=0.9,
              source="Weapons (2025)",
              payload=R("the running kids", "Biped", ["Charge", "Fast", "Swarm"], 1.3, 30, count=4, temp="Chaotic",
                        inten=1.0)),
            s, "Aunt Gladys: garish red wig, rouge. Sends a swarm of kids sprinting at you, arms outstretched.")


@entry
def johnny_axe():
    s = S()
    s.box((0, 0, -4.0), (1.3, 1.3, 16.0), "D")                                                   # handle
    s.box((0, -1.6, -12.0), (0.8, 4.4, 3.2), "A")                                                # axe head
    s.box((0, -3.9, -12.0), (0.5, 1.0, 4.0), "C")                                                # the edge
    s.cone((0, 1.8, -12.0), (0.8, 2.6, 1.4), "A")                                                # pick
    return (["here's johnny", "heres johnny", "jack torrance's axe", "overlook hotel axe", "the shining axe",
             "jack torrance"],
            R("Here's Johnny!", "Axe", ["Blade", "Heavy"], 1.0, 4, (0.85, 0.1, 0.1), source="The Shining"),
            s, "Jack Torrance's fire axe: chops through doors (Blade, great vs constructs) - Heavy.")


@entry
def log_truck():
    s = S()
    s.box((0, 6.0, -15.0), (9.0, 8.0, 7.0), "A")                                                 # cab
    s.box((0, 7.4, -18.6), (7.4, 3.0, 0.3), "C")                                                 # windscreen
    s.box((0, 4.0, -18.8), (8.4, 3.0, 0.6), "D")                                                 # grille
    s.cyl((3.8, 12.0, -12.0), (1.0, 6.0, 1.0), "D")                                              # exhaust stack
    s.box((0, 3.0, 4.0), (9.0, 1.2, 30.0), "D")                                                  # trailer bed
    for sx in (-1, 1):
        for z in (-6.0, 4.0, 14.0):
            s.box((sx * 4.4, 7.0, z), (0.6, 8.0, 0.6), "D")                                      # stakes
    for (x, y) in ((-3.0, 5.2), (0.0, 5.2), (3.0, 5.2), (-1.5, 7.8), (1.5, 7.8), (0.0, 10.4)):
        s.cyl((x, y, 4.0), (3.0, 29.0, 3.0), "P", ALONG_Z)                                        # the logs
    for z in (-2.0, 10.0):
        s.box((0, 8.6, z), (9.4, 0.4, 0.4), "C")                                                 # chains (about to snap)
    wheels(s, [(-4.6, -15.0), (4.6, -15.0), (-4.6, 10.0), (4.6, 10.0), (-4.6, 16.0), (4.6, 16.0)], d=4.2, w=1.6)
    return (["final destination log truck", "final destination logs", "log truck", "logging truck",
             "final destination 2 truck"],
            R("Log Truck", "Vehicle", ["Charge", "Shoots", "Heavy"], 9.0, 20000, (0.85, 0.4, 0.2),
              feat=["Wheels", "Cab", "Chains", "Smokestack"], source="Final Destination 2",
              payload=R("rolling log", "Boulder", ["Heavy", "Blunt"], 3.0, 400, (0.6, 0.4, 0.25))),
            s, "The Final Destination 2 log truck: rams, and its chains snap - rolling logs (Boulder payload).")


@entry
def foxy():
    s = S()
    b = person(s, top="A", pants="D", hair=None, style="bald", arms="D", hands="D", tall=1.0)
    s.on(0)
    s.box((0, b["y_sh"] - 3.0, 0), (b["tw"] * 0.5, 5.0, b["td"] + 0.1), "D")                      # exposed endoskeleton
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy, 0), (4.4, 4.4, 4.6), "A")
    s.box((0, hy - 0.6, -3.2), (2.2, 1.8, 3.4), "A")                                              # snout
    for sx in (-1, 1):
        s.cone((sx * 1.6, hy + 3.2, 0.4), (1.4, 3.0, 1.0), "A")                                    # ears
    s.box((-1.0, hy + 0.9, -2.3), (1.4, 1.2, 0.3), "D")                                           # eye patch
    s.box((1.0, hy + 0.9, -2.3), (0.8, 0.8, 0.3), "C")
    s.on(6)
    s.box((0, hy - 2.2, -2.6), (2.0, 0.8, 3.0), "D")
    for x in (-0.6, 0.0, 0.6):
        s.cone((x, hy - 1.6, -3.8), (0.4, 0.8, 0.4), "C")
    s.part(6, 5, (0, hy - 1.4, -0.4), "Jaw")
    hx, hyy, hz = held(s, b)
    s.seg((hx, hyy, hz - 0.4), (hx, hyy + 0.8, hz - 2.4), 0.4, 0.4, "C")                           # the hook
    s.box((hx, hyy + 1.4, hz - 2.6), (0.4, 1.2, 0.4), "C")
    s.on(0)
    return (["foxy the pirate fox", "foxy fnaf", "pirate cove foxy", "foxy animatronic"],
            R("Foxy", "Biped", ["Fast", "Charge", "Pierce"], 2.0, 120, (0.8, 0.25, 0.15), temp="Sneaky", inten=0.9,
              source="Five Nights at Freddy's"),
            s, "Foxy: the tattered pirate-fox animatronic with a hook. Waits out of sight, then sprints at you.")


simple(["texas chainsaw", "leatherface's saw"],
       R("Texas Chainsaw", "Sword", ["Blade", "Spin"], 1.2, 6, (0.75, 0.6, 0.45)), "A roaring chainsaw.")
simple(["annabelle doll", "annabelle", "annabelle the doll", "conjuring doll"],
       R("Annabelle", "Biped", ["Grab", "DropFromSky"], 0.9, 5, (0.95, 0.9, 0.85), "Shadow", temp="Sneaky", inten=1.0),
       "She's never where you left her.")
simple(["the nun", "demon nun", "valak the nun", "nun conjuring"],
       R("The Nun", "Biped", ["Flies", "Grab"], 2.0, 60, (0.15, 0.15, 0.18), "Shadow", temp="Sneaky", inten=1.0),
       "The demon in a nun's habit.")
simple(["bonnie the bunny", "bonnie fnaf"],
       R("Bonnie", "Biped", ["Blunt"], 2.0, 120, (0.5, 0.4, 0.9), temp="Sneaky", inten=0.9), "The purple bunny.")
simple(["chica the chicken", "chica fnaf"],
       R("Chica", "Biped", ["Shoots"], 2.0, 120, (1.0, 0.85, 0.2), temp="Sneaky", inten=0.9,
         payload=R("mr cupcake", "Ball", ["Sticky"], 0.3, 1)), "Let's eat! (with Mr. Cupcake)")
simple(["golden freddy", "golden freddy fnaf"],
       R("Golden Freddy", "Biped", ["DropFromSky", "Grab"], 2.0, 120, (1.0, 0.8, 0.25), "Shadow", temp="Sneaky",
         inten=1.0), "It's me. Appears right on top of you.")
simple(["reverse bear trap", "jigsaw's bear trap"],
       R("Reverse Bear Trap", "Cage", ["Trap", "Heavy"], 0.8, 8, (0.6, 0.6, 0.62)), "Saw's head trap.")
simple(["grady twins", "come play with us", "the shining twins"],
       R("The Grady Twins", "Biped", ["Swarm"], 1.3, 30, (0.6, 0.75, 1.0), count=2, temp="Sneaky", inten=1.0),
       "Come play with us... forever.")
simple(["boomstick", "this is my boomstick", "ash's boomstick"],
       R("Boomstick", "Gun", ["Shoots", "Heavy"], 1.0, 4, (0.5, 0.35, 0.25), count=3), "Ash's double-barrel shotgun.")
simple(["ash williams", "ash vs evil dead"],
       R("Ash Williams", "Biped", ["Blade", "Spin", "Shoots"], 1.8, 80, (0.5, 0.45, 0.4), temp="Aggressive", inten=0.8),
       "Chainsaw hand + boomstick.")
simple(["candyman", "candyman's hook"],
       R("Candyman", "Biped", ["Pierce", "Shoots"], 2.0, 90, (0.6, 0.4, 0.25), temp="Sneaky", inten=1.0,
         payload=R("bee swarm", "Insect", ["Swarm", "Homing"], 0.2, 0.1, count=8)), "Hook hand and a cloud of bees.")
simple(["critters", "krites"],
       R("Critters", "Ball", ["Swarm", "Bounce", "Blade"], 0.5, 3, (0.5, 0.4, 0.35), count=6, temp="Aggressive",
         inten=0.9), "Rolling, biting alien furballs.")
simple(["m3gan 2.0", "m3gan", "megan doll", "megan robot doll", "m3gan dance"],
       R("M3GAN", "Biped", ["Fast", "Blade"], 1.3, 40, (0.95, 0.8, 0.6), temp="Playful", inten=1.0),
       "The dancing AI doll.")
simple(["weapons running kids", "running kids weapons", "the missing kids weapons"],
       R("The Running Kids", "Biped", ["Charge", "Fast", "Swarm"], 1.3, 30, (0.8, 0.7, 0.6), count=5, temp="Chaotic",
         inten=1.0), "2:17 a.m., arms outstretched.")
simple(["the babadook", "babadook", "mister babadook"],
       R("The Babadook", "Biped", ["Grab", "Flies"], 2.2, 60, (0.1, 0.1, 0.12), "Shadow", temp="Sneaky", inten=1.0),
       "Top hat, long claws. Ba-ba-ba DOOK.")
simple(["smile entity", "smile demon"],
       R("Smile Entity", "Biped", ["Grab"], 1.8, 60, (0.9, 0.9, 0.9), "Psychic", temp="Sneaky", inten=1.0),
       "That smile.")
