"""Action films: John Wick, Raiders of the Lost Ark, Mission: Impossible, Fast & Furious, Die Hard, Top Gun, Kill Bill,
The Matrix, 300, Gladiator II, Rocky, Home Alone."""
import math

from reg import entry, simple, R, S
from slib import ALONG_X, ALONG_Z, TIP_NZ, TIP_NY
from rig import head, cape, hero
from bodies import person, held, quad, wheels, car_body


@entry
def mr_wick():
    s = S()
    b = person(s, top="D", pants="D", shirt="D", hair="D", style="long", arms="D")
    s.on(5)
    s.box((0, b["hy"] - 1.6, -1.6), (3.4, 1.4, 1.0), "D")                                        # beard
    hx, hy, hz = held(s, b)
    s.box((hx, hy + 0.6, hz - 1.8), (0.8, 1.4, 3.2), "D")                                          # pistol
    s.on(3)
    hx, hy, hz = b["hands"][-1]
    s.box((hx, hy + 0.2, hz - 2.2), (0.4, 0.4, 4.4), "A")                                          # the pencil
    s.cone((hx, hy + 0.2, hz - 4.8), (0.4, 0.8, 0.4), "C", TIP_NZ)
    s.on(0)
    return (["mr wick", "john wick", "mister wick", "jonathan wick", "baba yaga john wick", "john wick character",
             "john wick chapter 4"],
            R("John Wick", "Biped", ["Shoots", "Fast", "Guard"], 1.8, 80, (0.1, 0.1, 0.12), temp="Aggressive", inten=0.9,
              source="John Wick"),
            s, "John Wick: black suit, long hair, a pistol - and a pencil. Relentless (Aggressive), Fast, Guarded.")


@entry
def wicks_dog():
    s = S()
    quad(s, L=11.0, H=5.6, W=4.8, leg_h=4.4, leg_w=1.6, body_t="A", leg_t="A", head_t="A", head_s=(4.0, 3.6, 4.0),
         snout=(2.6, 1.8, 1.6), ears=1.0, tail_pts=[(0, 7.0, 5.2), (0, 7.8, 7.4)], tail_w=0.8)
    s.on(0)
    s.cyl((0, 9.0, -5.0), (3.8, 0.8, 3.4), "D", (70, 0, 0))                                         # collar
    return (["john wick's dog", "john wicks dog", "wick's pit bull", "john wick's pit bull", "daisy the beagle",
             "john wick's puppy", "don't kill the dog"],
            R("John Wick's Dog", "Quadruped", ["Charge", "Grab"], 1.0, 30, (0.75, 0.65, 0.55), temp="Protective",
              inten=1.0, source="John Wick"),
            s, "John Wick's pit bull: Protective to the end - guards its owner, lunges and bites.")


@entry
def ark():
    s = S()
    s.box((0, 4.0, 0), (8.0, 5.0, 12.0), "A")                                                    # the gold chest
    s.box((0, 6.8, 0), (8.6, 0.8, 12.6), "A")                                                    # lid
    for sx in (-1, 1):
        s.box((sx * 4.8, 3.0, 0), (0.6, 0.6, 20.0), "D")                                          # carrying poles
        for sz in (-1, 1):
            s.box((sx * 3.6, 0.8, sz * 5.4), (0.8, 1.6, 0.8), "A")                                 # feet
    for sz in (-1, 1):                                                                            # the two cherubim
        s.ell((0, 9.0, sz * 3.6), (2.0, 3.0, 2.0), "A")
        s.ell((0, 11.2, sz * 3.6 - sz * 0.6), (1.4, 1.4, 1.4), "A")
        s.box((0, 11.4, sz * 1.6), (7.6, 1.2, 3.6), "C", (sz * 25, 0, 0))                          # wings meeting
    s.box((0, 4.0, -6.1), (5.0, 2.0, 0.3), "C")
    return (["ark of the covenant raiders", "raiders ark", "the lost ark", "face melting ark", "indiana jones ark",
             "raiders of the lost ark"],
            R("The Lost Ark", "Pillar", ["Beam", "Attract"], 1.6, 150, (1.0, 0.8, 0.25), "Fire",
              source="Raiders of the Lost Ark"),
            s, "The Ark: a golden chest with two cherubim. Draws everyone in (Attract) and melts faces with a Fire Beam.")


@entry
def toretto_charger():
    s = S()
    y1 = car_body(s, L=24.0, W=10.0, h0=1.8, h1=3.8, cab=(8.0, 3.0), cab_z=2.4, body_t="D", wheel_d=4.6)
    s.box((0, y1 + 1.2, -6.4), (3.0, 2.4, 3.2), "C")                                               # supercharger blower
    s.box((0, y1 + 2.8, -6.4), (3.6, 0.8, 3.6), "D")                                               # scoop
    s.box((0, y1 - 0.4, -12.2), (8.4, 2.4, 0.4), "C")                                              # chrome grille
    for sx in (-1, 1):
        s.cz((sx * 3.0, 2.0, 12.6), 1.2, 1.4, "C")                                                 # exhausts
    s.on(255)
    s.box((-2.0, y1 - 0.2, 2.4), (2.4, 1.0, 2.4), "D")
    s.seat(0, (-2.0, y1 + 0.2, 2.4))
    s.on(0)
    return (["dom toretto's charger", "dominic toretto", "dom toretto", "toretto's charger", "fast and furious charger",
             "dom's charger", "i live my life a quarter mile at a time"],
            R("Dom's Charger", "Vehicle", ["Charge", "Fast", "Heavy", "Piloted"], 5.0, 1800, (0.1, 0.1, 0.12),
              feat=["Wheels", "Exhaust", "Turbine"], source="Fast & Furious"),
            s, "Dom Toretto's '70 Dodge Charger with the blower: Heavy, Fast, rams everything. It's about family.")


@entry
def hans_gruber():
    s = S()
    b = person(s, top="A", pants="A", shirt="C", tie="D", hair="D", style="slick")
    for pid, sx in ((3, -1), (4, 1)):                                                               # arms flung up - falling
        s.on(pid)
        s.seg((sx * b["arm_x"], b["y_sh"] - 0.4, 0), (sx * (b["arm_x"] + 3.0), b["y_sh"] + 6.4, 0.6), 1.8, 1.8, "A")
        s.box((sx * (b["arm_x"] + 3.2), b["y_sh"] + 7.4, 0.6), (1.9, 1.9, 1.9), "A")
    s.on(0)
    return (["hans gruber", "falling hans gruber", "hans gruber falling", "nakatomi plaza fall", "nakatomi plaza"],
            R("Hans Gruber", "Biped", ["DropFromSky", "Blunt"], 1.8, 80, (0.35, 0.35, 0.4), temp="Calm", inten=0.8,
              source="Die Hard"),
            s, "Hans Gruber, mid-fall from Nakatomi Plaza: he DropsFromSky onto the target, arms flailing.")


@entry
def tomcat():
    s = S()
    s.box((0, 4.0, 0), (5.0, 4.0, 30.0), "P")                                                     # fuselage
    s.cone((0, 4.0, -18.0), (3.6, 6.0, 3.6), "P", TIP_NZ)                                         # nose
    s.ell((0, 6.4, -9.0), (3.0, 2.6, 7.0), "C")                                                   # two-seat canopy
    for sx in (-1, 1):
        s.box((sx * 8.0, 4.0, 2.0), (12.0, 0.6, 7.0), "P", (0, -sx * 25, 0))                        # swept wings
        s.cz((sx * 2.6, 3.6, 13.4), 2.8, 4.0, "D")                                                  # twin engines
        s.box((sx * 2.6, 9.0, 12.4), (0.5, 7.0, 4.0), "A", (0, 0, -sx * 12))                        # twin tails
        s.box((sx * 5.4, 4.0, 13.0), (5.0, 0.5, 4.0), "P")                                          # stabilisers
    s.on(30)
    for sx in (-1, 1):
        s.cone((sx * 2.6, 3.6, 17.0), (2.2, 3.0, 2.2), "C", (90, 0, 0))
    s.part(30, 0, (0, 3.6, 15.4), "Exhaust")
    s.on(255)
    s.box((0, 5.6, -10.0), (2.0, 1.0, 2.0), "D")
    s.seat(0, (0, 6.0, -10.0))
    s.on(0)
    return (["f-14 tomcat", "f14 tomcat", "top gun jet", "top gun tomcat", "tomcat fighter jet",
             "highway to the danger zone", "danger zone", "i feel the need for speed"],
            R("F-14 Tomcat", "Vehicle", ["Flies", "Fast", "Shoots", "Piloted"], 10.0, 18000, (0.6, 0.62, 0.66),
              feat=["Jet", "Cockpit", "Fins"], source="Top Gun",
              payload=R("sidewinder", "Rocket", ["Homing", "Explodes"], 1.5, 40)),
            s, "The Top Gun F-14: twin tails, swept wings. Fly it Fast into the danger zone; fires homing missiles.")


@entry
def the_bride():
    s = S()
    b = person(s, top="A", pants="A", fem=True, hair="A", style="long", arms="A")
    s.on(0)
    for sx in (-1, 1):
        s.box((sx * (b["tw"] / 2 + 0.05), b["y_hip"] + 4.0, 0), (0.4, 9.0, 0.8), "D")              # the black side stripe
    hx, hy, hz = held(s, b)
    s.box((hx, hy, hz - 1.2), (0.6, 0.8, 2.6), "D")                                               # katana
    s.cyl((hx, hy, hz - 2.6), (1.4, 0.4, 1.4), "D", (90, 0, 0))
    s.box((hx, hy + 0.1, hz - 9.4), (0.3, 0.8, 13.0), "C")
    s.on(0)
    return (["the bride kill bill", "beatrix kiddo", "black mamba kill bill", "kill bill bride"],
            R("The Bride", "Biped", ["Blade", "Fast"], 1.8, 60, (1.0, 0.85, 0.15), temp="Aggressive", inten=1.0,
              source="Kill Bill"),
            s, "Beatrix Kiddo: yellow jumpsuit with the black stripe, Hattori Hanzo katana. Fast Blade, pure revenge.")


@entry
def neo():
    s = S()
    b = person(s, top="D", pants="D", hair="D", style="slick", arms="D")
    s.on(5)
    s.box((0, b["hy"] + 0.3, -2.2), (3.8, 0.8, 0.3), "D")                                         # tiny sunglasses
    s.on(7)
    s.box((0, b["y_hip"] - 1.4, b["td"] / 2 + 0.6), (b["tw"] + 1.8, 13.0, 0.8), "D", (-6, 0, 0))    # long coat
    for sx in (-1, 1):
        s.box((sx * (b["tw"] / 2 + 0.6), b["y_hip"] - 2.4, -0.4), (0.6, 11.0, b["td"] * 0.8), "D", (0, 0, sx * 6))
    s.part(7, 0, (0, b["y_hip"] + 3.0, b["td"] / 2), "Cloth")
    s.on(3)
    hx, hy, hz = b["hands"][-1]
    s.seg((-b["arm_x"], b["y_sh"] - 0.4, 0), (-b["arm_x"] - 1.0, b["y_sh"] + 0.4, -7.0), 1.8, 1.8, "D")   # palm out: stop
    s.box((-b["arm_x"] - 1.0, b["y_sh"] + 0.6, -7.8), (1.8, 2.4, 0.8), "A")
    s.on(0)
    return (["thomas anderson", "neo", "neo the one", "the one matrix", "neo matrix", "mr anderson", "whoa neo"],
            R("Neo", "Biped", ["Fast", "Guard", "Repel"], 1.8, 75, (0.2, 0.9, 0.35), "Psychic", temp="Calm", inten=1.0,
              source="The Matrix"),
            s, "Neo: long black coat, tiny sunglasses, palm raised. Fast, Guards, and pushes bullets away (Repel).")


@entry
def leonidas():
    s = S()
    b = hero(s, bulk=1.2, suit="A", legs="A", arms="A", boots="D", trunks="D", torso_kind="v", arm_spread=6)
    head(s, b, skin="P", hair="D", style="short")
    s.on(5)
    s.ell((0, b["hy"] + 0.8, 0.2), (4.6, 4.4, 4.8), "D")                                          # Corinthian helmet
    s.box((0, b["hy"] + 3.4, 0.6), (0.9, 2.6, 6.0), "P")                                          # crest
    s.box((0, b["hy"] - 1.8, -1.8), (3.0, 1.6, 1.2), "D")                                          # beard
    cape(s, b, t="P", flare=2.2)
    s.on(3)
    hx, hy, hz = b["hands"][-1]
    s.cyl((hx - 1.4, hy + 3.0, hz - 0.4), (9.0, 1.0, 9.0), "A", (0, 0, 90))                         # round aspis shield
    s.cyl((hx - 1.9, hy + 3.0, hz - 0.4), (2.4, 0.6, 2.4), "C", (0, 0, 90))
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.cyl((hx, hy, hz - 4.0), (0.6, 22.0, 0.6), "D", ALONG_Z)                                       # dory spear
    s.cone((hx, hy, hz - 15.8), (1.2, 2.4, 1.2), "C", TIP_NZ)
    s.on(0)
    return (["king leonidas", "leonidas", "sparta kick", "spartan kick", "this is sparta kick", "the 300",
             "300 spartans"],
            R("King Leonidas", "Biped", ["Guard", "Pierce", "Launch", "Charge"], 2.0, 95, (0.8, 0.1, 0.1),
              temp="Aggressive", inten=0.95, source="300"),
            s, "Leonidas: red cape, crested helmet, round shield, spear. Guard + Pierce, and THE kick (Launch).")


@entry
def rocky():
    s = S()
    b = person(s, top="P", pants="P", hair="D", style="short", arms="P", bulk=1.1)
    for pid, sx in ((3, -1), (4, 1)):                                                               # arms raised in triumph
        s.on(pid)
        s.seg((sx * b["arm_x"], b["y_sh"] - 0.4, 0), (sx * (b["arm_x"] + 2.2), b["y_sh"] + 7.0, -0.4), 1.9, 1.9, "P")
        s.ell((sx * (b["arm_x"] + 2.4), b["y_sh"] + 8.2, -0.4), (3.0, 3.2, 3.4), "A")               # boxing gloves
    s.on(5)
    s.box((0, b["hy"] + 2.0, 0.2), (4.4, 1.2, 4.4), "D")                                            # beanie
    s.on(0)
    return (["rocky balboa", "italian stallion", "yo adrian", "adrian!", "gonna fly now", "eye of the tiger rocky"],
            R("Rocky Balboa", "Biped", ["Blunt", "Fast", "Guard"], 1.8, 80, (0.85, 0.12, 0.1), temp="Aggressive",
              inten=0.9, source="Rocky"),
            s, "Rocky: grey sweats, beanie, red gloves raised on the steps. A Blunt, Fast brawler who keeps his guard up.")


@entry
def rocky_steps():
    s = S()
    for i in range(10):
        s.box((0, (i + 1) * 0.6, i * 2.4), (20.0, (i + 1) * 1.2, 2.4), "P" if i % 3 else "A")
    s.box((0, 12.0, 26.0), (26.0, 1.0, 6.0), "A")                                                  # top landing
    for sx in (-1, 1):
        s.box((sx * 10.6, 6.0, 12.0), (1.2, 12.0, 24.0), "D", (-26, 0, 0))                           # balustrades
        s.cyl((sx * 8.0, 17.0, 29.0), (2.6, 10.0, 2.6), "C")                                         # museum columns
    s.box((0, 22.4, 29.0), (22.0, 1.4, 4.0), "C")
    return (["rocky steps", "the rocky steps", "rocky stairs", "philadelphia museum of art steps", "art museum steps"],
            R("The Rocky Steps", "Ramp", ["Launch"], 6.0, 3000, (0.85, 0.12, 0.1), source="Rocky"),
            s, "The Art Museum steps: a Ramp that Launches you up and over - arms raised at the top.")


@entry
def paint_can():
    s = S()
    s.cyl((0, 3.0, 0), (5.0, 6.0, 5.0), "A")
    s.cyl((0, 6.2, 0), (5.2, 0.5, 5.2), "D")
    s.cyl((0, 3.0, 0), (5.1, 2.4, 5.1), "C")                                                        # label
    s.box((0, 9.0, 0), (5.4, 0.4, 0.4), "D", (0, 0, 0))                                             # bail handle
    for sx in (-1, 1):
        s.box((sx * 2.6, 7.6, 0), (0.4, 3.0, 0.4), "D")
    s.box((0, 17.0, 0), (0.3, 16.0, 0.3), "D")                                                      # the rope it swings on
    return (["home alone paint can", "paint can trap", "swinging paint can", "home alone paint cans",
             "kevin mccallister's trap"],
            R("Swinging Paint Can", "Anvil", ["DropFromSky", "Blunt"], 0.8, 8, (0.9, 0.3, 0.2), source="Home Alone"),
            s, "The Home Alone paint can: swings down from above the stairs (DropFromSky) - bonk.")


simple(["continental coin", "continental gold coin", "john wick coin", "gold coin continental"],
       R("Continental Coin", "Disc", ["Ricochet", "Returning"], 0.2, 0.1, (1.0, 0.8, 0.25)), "The assassins' currency.")
simple(["this message will self-destruct", "self-destructing message", "mission impossible tape"],
       R("Self-Destructing Message", "Bomb", ["Explodes"], 0.4, 1, (0.9, 0.9, 0.9)), "Good luck, Ethan.")
simple(["ethan hunt", "impossible mission force", "mission impossible", "mission: impossible"],
       R("Ethan Hunt", "Biped", ["Fast", "Grab"], 1.8, 75, (0.2, 0.2, 0.25), temp="Protective", inten=0.9),
       "Hangs off planes, sprints everywhere: Fast + Grab.")
simple(["john mcclane", "yippee ki-yay", "yippee-ki-yay", "yippee ki yay"],
       R("John McClane", "Biped", ["Shoots", "Charge"], 1.8, 80, (0.9, 0.9, 0.85), temp="Aggressive", inten=0.9),
       "Barefoot, white tank top, very bad day.")
simple(["nos tank", "nitrous boost", "nos boost", "hit the nos"],
       R("NOS", "Jetpack", ["Thrust", "Fast"], 0.8, 8, (0.3, 0.6, 1.0)), "Nitrous: a burst of Thrust.")
simple(["crazy 88", "the crazy 88", "crazy eighty-eight"],
       R("The Crazy 88", "Biped", ["Blade", "Swarm"], 1.8, 70, (0.1, 0.1, 0.12), count=8, temp="Aggressive", inten=0.9),
       "O-Ren Ishii's masked army.")
simple(["ivan drago", "i must break you"],
       R("Ivan Drago", "Biped", ["Blunt", "Heavy"], 2.0, 120, (0.85, 0.1, 0.1), temp="Aggressive", inten=0.9),
       "If he dies, he dies.")
simple(["apollo creed", "apollo creed's gloves"],
       R("Apollo Creed", "Biped", ["Blunt", "Fast"], 1.9, 90, (0.2, 0.4, 0.9), temp="Playful", inten=0.8),
       "The showman champ.")
simple(["gladiator ii rhino", "gladiator rhino", "colosseum rhino"],
       R("Colosseum Rhino", "Quadruped", ["Charge", "Heavy", "Piloted"], 3.0, 2300, (0.55, 0.5, 0.45),
         temp="Aggressive", inten=0.9), "A war rhino in the arena.")
simple(["colosseum sharks", "gladiator ii sharks", "flooded colosseum sharks"],
       R("Colosseum Sharks", "Fish", ["Swarm", "Grab"], 2.0, 150, (0.5, 0.55, 0.6), "Water", count=4, temp="Aggressive",
         inten=0.9), "The flooded naval battle.")
simple(["kevin mccallister", "home alone kevin", "keep the change you filthy animal"],
       R("Kevin McCallister", "Biped", ["Shoots", "Trap"], 1.2, 30, (0.85, 0.2, 0.2), temp="Playful", inten=1.0,
         payload=R("micro machines", "Ball", ["Swarm", "Bounce"], 0.2, 0.1, count=8)), "Booby-traps everywhere.")
simple(["wet bandits", "the wet bandits"],
       R("The Wet Bandits", "Biped", ["Blunt"], 1.8, 80, (0.4, 0.35, 0.3), count=2, temp="Chaotic", inten=0.8),
       "Harry and Marv.")
