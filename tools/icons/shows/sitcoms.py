"""Sitcoms: The Office, Parks and Recreation, Friends, Seinfeld, Brooklyn Nine-Nine, It's Always Sunny in
Philadelphia, Community, Arrested Development."""
import math

from reg import entry, simple, R, S
from slib import ALONG_X, ALONG_Z, TIP_NZ
from rig import head, cape
from bodies import person, held, quad, wheels, car_body


@entry
def michael_scott():
    s = S()
    b = person(s, top="D", pants="D", shirt="C", tie="A", hair="D", style="short")
    hx, hy, hz = held(s, b)
    s.cyl((hx, hy + 0.8, hz - 1.6), (2.4, 3.0, 2.4), "C")                                  # the mug
    s.box((hx, hy + 1.2, hz - 1.6 - 1.25), (1.4, 1.2, 0.3), "A")                           # WORLD'S BEST BOSS
    s.on(0)
    return (["michael scott", "michael gary scott", "world's best boss", "worlds best boss mug",
             "that's what she said", "michael scarn"],
            R("Michael Scott", "Biped", ["Shoots"], 1.8, 80, (0.2, 0.35, 0.8), temp="Playful", inten=0.95,
              payload=R("world's best boss mug", "Ball", ["Blunt", "Bounce"], 0.3, 1)),
            s, "Michael Scott: suit, tie, World's Best Boss mug. Desperately Playful; lobs his mug (bouncy Blunt).")


@entry
def dwight():
    s = S()
    b = person(s, top="A", pants="D", shirt="A", tie="D", hair="D", style="slick")
    s.on(5)
    hy = b["hy"]
    s.box((0, hy + 0.3, -2.2), (3.8, 1.0, 0.3), "D")                                       # glasses
    for sx in (-1, 1):
        s.box((sx * 0.95, hy + 0.3, -2.4), (1.2, 0.8, 0.2), "C")
    s.box((0.5, hy + 2.1, -1.4), (0.3, 0.6, 3.0), "C")                                     # the centre part
    hx, hyy, hz = held(s, b)
    s.box((hx, hyy - 1.6, hz - 1.0), (0.7, 3.2, 0.7), "D")                                 # nunchuck
    s.box((hx, hyy - 3.6, hz - 2.6), (0.7, 3.2, 0.7), "D", (40, 0, 0))
    s.on(0)
    return (["dwight schrute", "dwight k schrute", "dwight k. schrute", "assistant to the regional manager",
             "bears beets battlestar galactica", "schrute"],
            R("Dwight Schrute", "Biped", ["Blunt", "Charge"], 1.9, 85, (0.85, 0.7, 0.25), temp="Aggressive", inten=0.85),
            s, "Dwight: mustard shirt, glasses, centre part, nunchucks. Volunteer sheriff's deputy: Aggressive Charge.")


@entry
def beets():
    s = S()
    s.ell((0, 0, 0), (6.4, 5.4, 6.4), "D")
    s.cone((0, -3.6, 0), (1.8, 2.2, 1.8), "D", (180, 0, 0))                                  # taproot
    for i, rz in enumerate((-35, 0, 35)):
        s.box((math.sin(math.radians(rz)) * 1.0, 3.3, 0), (0.5, 2.0, 0.5), "A", (0, 0, -rz))
        s.ell((math.sin(math.radians(rz)) * 1.9, 4.6 + (i == 1) * 0.4, 0), (1.8, 2.0, 0.6), "A", (0, 0, -rz))
    return (["schrute beets", "schrute farms beets", "dwight's beets", "schrute farms", "beet farm beets"],
            R("Schrute Farms Beets", "Ball", ["Swarm", "Blunt"], 0.4, 1.5, (0.3, 0.8, 0.3), "Earth", count=6),
            s, "Schrute Farms beets: a volley of six Earth beets (Swarm) that pelt the target.")


@entry
def jello_stapler():
    s = S()
    s.box((0, 5.0, 0), (10.0, 10.0, 10.0), "A")                                              # the jello mould
    s.box((0, 10.4, 0), (8.6, 0.8, 8.6), "A")
    s.box((0, 5.0, 0), (6.6, 6.6, 6.6), "P", sub=True)                                        # hollow so it can enclose
    s.box((0, 10.4, 0), (2.4, 2.0, 7.0), "D")                                                 # the stapler, set in the top
    s.box((0, 11.8, 0.4), (2.2, 1.0, 6.6), "D", (-8, 0, 0))
    for x in (-3.4, 3.4):
        s.box((x, 5.0, -5.05), (0.6, 9.0, 0.3), "C")                                           # jiggly highlights
    s.box((0, 0.3, 0), (12.0, 0.6, 12.0), "C")                                                # the plate
    return (["stapler in jello", "stapler in jell-o", "jello stapler", "jim's jello prank", "jim's prank"],
            R("Stapler in Jello", "Cage", ["Trap", "Bounce", "Sticky"], 2.2, 60, (1.0, 0.85, 0.2)),
            s, "Jim's prank: a wobbling jello mould that snaps shut around the target (Trap) - bouncy and sticky.")


@entry
def kevins_chili():
    s = S()
    s.cyl((0, 3.0, 0), (9.0, 6.0, 9.0), "P")                                                 # the pot
    s.cyl((0, 3.6, 0), (7.8, 6.0, 7.8), "P", sub=True)
    s.cyl((0, 5.0, 0), (7.8, 1.0, 7.8), "A")                                                  # chili
    for sx in (-1, 1):
        s.box((sx * 5.2, 5.0, 0), (1.6, 0.8, 1.2), "D")                                       # handles
    for (x, z, d) in ((-4, -6, 5), (2, -8, 6.5), (6, -4, 4), (-7, -1, 3.5), (0, -11, 3)):
        s.cyl((x, 0.3, z), (d, 0.6, d * 0.8), "A")                                            # the spill everywhere
    for (x, z) in ((-1.2, 0.8), (1.6, -1.0), (0.2, -2.4), (-2.4, -1.6)):
        s.box((x, 5.6, z), (0.8, 0.5, 0.8), "C")                                              # beans
    return (["kevin's chili", "kevins chili", "kevin's famous chili", "kevins famous chili", "kevin malone's chili",
             "kevin spills the chili"],
            R("Kevin's Famous Chili", "Ball", ["Sticky", "Trap", "Heavy"], 1.4, 30, (0.75, 0.2, 0.08), "Fire"),
            s, "Kevin's famous chili: a heavy pot that spills over whoever it hits - Sticky, Trap, spicy (Fire).")


@entry
def dundie():
    s = S()
    s.box((0, 0.6, 0), (3.2, 1.2, 3.2), "D")
    s.cyl((0, 2.2, 0), (0.8, 2.2, 0.8), "A")
    s.box((0, 5.6, 0), (1.6, 3.4, 1.0), "A")                                                   # the little figure
    s.ell((0, 8.0, 0), (1.4, 1.4, 1.4), "A")
    s.box((0, 5.8, 0), (3.4, 0.6, 0.6), "A")
    s.box((0, 9.4, -0.6), (0.8, 1.6, 0.4), "A", (-20, 0, 0))                                   # raised arm
    return (["dundie award", "dundie", "dundies", "a dundie"],
            R("Dundie Award", "Ball", ["Blunt", "Returning"], 0.5, 2, (0.95, 0.8, 0.3)),
            s, "A Dundie: the Office's tacky trophy. Thrown, bonks, comes back.")


@entry
def ron_swanson():
    s = S()
    b = person(s, top="A", pants="D", shirt="C", hair="D", style="short", bulk=1.1)
    s.on(5)
    s.box((0, b["hy"] - 0.6, -2.2), (3.4, 1.0, 0.6), "D")                                      # the mustache
    hx, hy, hz = held(s, b)
    s.box((hx, hy - 0.6, hz - 0.2), (0.7, 7.0, 0.7), "D", (-20, 0, 0))                          # axe handle
    s.box((hx, hy - 3.8, hz - 2.2), (0.5, 2.8, 3.0), "C", (-20, 0, 0))                          # axe head
    s.on(0)
    return (["ron swanson", "ronald ulysses swanson", "ron f swanson", "duke silver"],
            R("Ron Swanson", "Biped", ["Blade", "Heavy"], 1.9, 100, (0.55, 0.35, 0.2), temp="Lazy", inten=0.7),
            s, "Ron Swanson: mustache, polo, a woodworker's axe. Refuses to move for anyone (Lazy, Heavy) - or the government.")


@entry
def lil_sebastian():
    s = S()
    b = quad(s, L=9.0, H=5.0, W=3.6, leg_h=4.2, leg_w=1.2, body_t="A", leg_t="A", head_s=(2.4, 3.2, 4.2),
             head_c=(0, 10.0, -6.0), tail_pts=[(0, 7.2, 4.4), (0, 5.4, 6.0), (0, 3.2, 6.6)], tail_w=1.4, ears=1.2)
    s.on(5)
    for i in range(4):
        s.box((0, 8.6 + i * 1.0, -4.4 + i * 0.6), (0.8, 1.2, 1.4), "D")                        # mane
    s.on(0)
    return (["lil sebastian", "li'l sebastian", "little sebastian", "5000 candles in the wind", "pawnee mini horse"],
            R("Li'l Sebastian", "Quadruped", ["Fast", "Charge"], 0.9, 60, (0.8, 0.6, 0.35), temp="Playful", inten=0.95),
            s, "Li'l Sebastian: Pawnee's beloved miniature horse. Tiny, Fast, Playful charges. 5000 candles in the wind.")


@entry
def pivot_couch():
    s = S()
    s.box((0, 2.8, 0), (22.0, 3.0, 8.0), "P")                                                  # seat base
    s.box((0, 8.0, 3.0), (22.0, 8.0, 2.4), "P")                                                 # backrest
    for sx in (-1, 1):
        s.box((sx * 11.0, 5.4, 0.2), (2.4, 5.6, 8.4), "P")                                       # arms
        for sz in (-1, 1):
            s.box((sx * 9.6, 0.6, sz * 3.0), (1.0, 1.2, 1.0), "D")                                # feet
    for i in range(3):
        s.box((-6.6 + i * 6.6, 4.8, -0.6), (6.4, 1.4, 6.4), "A")                                  # cushions
    return (["pivot couch", "pivot pivot pivot", "ross's couch", "ross couch", "central perk couch",
             "friends couch"],
            R("Pivot Couch", "Wall", ["Barrier", "Spin", "Heavy"], 2.4, 90, (0.9, 0.55, 0.2)),
            s, "Ross's couch jammed in the stairwell: a Heavy barrier that keeps pivoting (Spin) and swats you away.")


@entry
def kramer():
    s = S()
    b = person(s, top="A", pants="D", hair="D", style="short", tall=2.0)
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy + 2.6, 0.2), (5.4, 4.0, 5.0), "D")                                             # the tall pompadour
    s.ell((0, hy + 4.2, 0.0), (4.2, 2.8, 4.0), "D")
    s.on(3)
    hx, hy2, hz = b["hands"][-1]
    s.seg((b["arm_x"] * -1, b["y_sh"], 0), (hx - 2.4, b["y_sh"] + 5.0, -1.0), 1.8, 1.8, "A")       # flailing arm up
    s.on(0)
    return (["cosmo kramer", "kramer", "kramer seinfeld", "the kramer entrance", "kramer entrance"],
            R("Kramer", "Biped", ["Charge", "Fast"], 2.0, 80, (0.8, 0.7, 0.55), temp="Chaotic", inten=0.95),
            s, "Kramer: towering hair, loud shirt, arms flailing. Bursts through doors - Fast Charge, pure Chaos.")


@entry
def festivus():
    s = S()
    s.cyl((0, 0, -12.0), (1.6, 30.0, 1.6), "C", ALONG_Z)                                        # unadorned aluminium
    s.cyl((0, 0, 3.4), (2.6, 0.8, 2.6), "D", ALONG_Z)                                           # base plate
    s.cyl((0, 0, 1.2), (1.1, 4.0, 1.1), "D", ALONG_Z)
    return (["festivus pole", "festivus", "feats of strength", "a festivus for the rest of us",
             "airing of grievances"],
            R("Festivus Pole", "Staff", ["Blunt", "Heavy"], 1.9, 6, (0.8, 0.82, 0.86), elem="Light"),
            s, "The Festivus pole: an unadorned aluminium pole (very high strength-to-weight). Feats of strength: Blunt, Heavy.")


@entry
def terry():
    s = S()
    from rig import hero
    b = hero(s, bulk=1.45, suit="C", legs="D", arms="A", gloves="A", torso_kind="v", arm_spread=10)
    s.on(0)
    head(s, b, skin="A", hair="D", style="bald", w=4.2, h=4.6, d=4.4)
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.cyl((hx, hy + 0.6, hz - 1.8), (2.0, 2.6, 2.0), "C")                                      # yogurt cup
    s.on(0)
    return (["terry jeffords", "sergeant jeffords", "terry loves yogurt", "terry crews b99"],
            R("Terry Jeffords", "Biped", ["Blunt", "Heavy", "Guard"], 2.0, 120, (0.4, 0.28, 0.2), temp="Protective",
              inten=0.95),
            s, "Terry Jeffords: enormous arms, tank top, yogurt. Terry protects the squad: Guard + Blunt + Heavy.")


@entry
def rum_ham():
    s = S()
    s.ell((0, 0, -3.4), (7.0, 6.0, 9.0), "P")                                                    # the ham
    s.ell((0, 1.0, -4.0), (6.2, 4.6, 7.6), "A")                                                  # glaze
    for i in range(4):
        s.box((0, 3.4, -6.4 + i * 2.0), (6.4, 0.4, 0.5), "D", (0, 45, 0))                         # criss-cross
    s.cyl((0, 0, 2.6), (2.0, 4.0, 2.0), "C", ALONG_Z)                                            # the bone
    s.ell((0, 0, 4.8), (2.8, 2.2, 1.6), "C")
    return (["rum ham", "charlie's rum ham", "frank's rum ham", "rum hammm"],
            R("Rum Ham", "Ball", ["Blunt", "Heavy"], 0.8, 8, (0.9, 0.45, 0.35)),
            s, "The Rum Ham, lost at sea: a heavy glazed ham you throw at people.")


@entry
def dayman():
    s = S()
    from rig import hero
    b = hero(s, bulk=1.0, suit="A", legs="A", boots="D", belt="D", torso_kind="v")
    head(s, b, skin="P", hair="D", style="long", mask="D")
    cape(s, b, t="C", flare=2.0)
    s.on(0)
    s.cyl((0, b["y_sh"] - 2.6, -b["td"] / 2 - 0.3), (3.0, 0.4, 3.0), "C", (90, 0, 0))               # sun emblem
    return (["dayman", "fighter of the nightman", "champion of the sun", "master of karate and friendship",
             "dayman aaaa"],
            R("Dayman", "Biped", ["Blunt", "Beam"], 2.0, 80, (1.0, 0.75, 0.2), "Fire", temp="Protective", inten=0.9),
            s, "Dayman (ah-ah-ah): champion of the sun, master of karate and friendship. A solar Beam and a cape.")


@entry
def banana_stand():
    s = S()
    s.box((0, 4.0, 0), (12.0, 8.0, 7.0), "A")                                                   # the booth
    s.box((0, 5.2, -3.6), (8.0, 3.4, 0.4), "D", sub=False)                                       # serving window
    s.box((0, 8.4, 0), (14.0, 0.8, 9.0), "P")                                                    # roof
    for i in range(5):
        s.box((-5.6 + i * 2.8, 8.8, -4.6), (1.4, 1.0, 0.4), "C")                                  # awning scallops
    s.ell((0, 13.0, 0), (4.0, 7.0, 3.0), "A", (0, 0, 40))                                         # giant banana on top
    s.ell((0, 13.6, 0), (3.4, 6.2, 2.6), "P", (0, 0, 40), sub=True)
    s.box((-2.6, 16.2, 0), (0.8, 1.8, 0.8), "D", (0, 0, 40))
    s.box((0, 5.4, -3.9), (6.0, 1.2, 0.2), "C")                                                   # sign
    return (["banana stand", "bluth banana stand", "bluth's frozen banana stand", "there's always money in the banana stand",
             "frozen banana stand"],
            R("Bluth's Banana Stand", "Wall", ["Barrier", "Explodes"], 3.0, 400, (1.0, 0.85, 0.2), "Fire"),
            s, "The Bluth banana stand: a Barrier until someone burns it down (Explodes). There's always money in it.")


@entry
def stair_car():
    s = S()
    y1 = car_body(s, L=22.0, W=8.4, h0=2.0, h1=3.2, cab=(6.0, 3.6), cab_z=-6.0, body_t="C")
    for i in range(6):                                                                           # the airport stairs
        s.box((0, y1 + 0.6 + i * 1.4, 7.0 - i * 2.0), (6.4, 1.4 + i * 2.8, 2.0), "A")
    for sx in (-1, 1):
        s.seg((sx * 3.6, y1 + 1.0, 8.4), (sx * 3.6, y1 + 9.6, -2.4), 0.5, 0.5, "D")               # handrails
    s.on(255)
    s.box((0, y1 + 0.4, -6.0), (2.4, 1.0, 2.4), "D")
    s.seat(0, (0, y1 + 0.8, -6.0))
    s.on(0)
    return (["stair car", "bluth stair car", "the stair car", "hop-ons", "you're gonna get some hop-ons"],
            R("Bluth Stair Car", "Vehicle", ["Charge", "Launch", "Piloted"], 5.0, 2500, (1.0, 1.0, 1.0),
              feat=["Wheels", "Cab"]),
            s, "The Bluth stair car: airport stairs on a truck. Rams (Charge) and flings hop-ons up the stairs (Launch).")


simple(["scranton strangler", "the scranton strangler"],
       R("Scranton Strangler", "Biped", ["Constrict", "Grab"], 1.8, 80, (0.4, 0.4, 0.45), temp="Sneaky", inten=0.9),
       "Sneaks up and Constricts.")
simple(["prison mike"],
       R("Prison Mike", "Biped", ["Blunt"], 1.8, 80, (0.3, 0.3, 0.8), temp="Aggressive", inten=0.8),
       "Michael's bandana-wearing 'hardened convict' alter ego.")
simple(["burt macklin", "burt macklin fbi"],
       R("Burt Macklin, FBI", "Biped", ["Shoots", "Fast"], 1.9, 90, (0.2, 0.2, 0.25), temp="Sneaky", inten=0.7),
       "Andy Dwyer's FBI alter ego: Sneaky gunplay.")
simple(["leslie knope", "leslie knope waffles"],
       R("Leslie Knope", "Biped", ["Shoots", "Fast"], 1.6, 55, (1.0, 0.8, 0.3), temp="Protective", inten=0.95,
         payload=R("jj's waffles", "Disc", ["Sticky"], 0.3, 0.3)),
       "Pawnee's most tireless public servant: Protective, flings waffles (Sticky).")
simple(["mouse rat", "mouse rat band", "andy dwyer's band"],
       R("Mouse Rat", "Staff", ["Repel"], 1.0, 5, (0.9, 0.3, 0.2)), "Andy's band: a sound wave that shoves (Repel).")
simple(["ross's unagi", "unagi awareness", "state of total awareness"],
       R("Unagi", "Dome", ["Guard", "Barrier"], 2.2, 20, (0.4, 0.7, 1.0), elem="Psychic"),
       "Ross's 'unagi': a state of total awareness - a guarding dome.")
simple(["marcel the monkey", "ross's monkey"],
       R("Marcel", "Biped", ["Grab", "Fast"], 0.7, 6, (0.6, 0.45, 0.3), temp="Playful", inten=0.9),
       "Ross's capuchin: small, Fast, grabs things.")
simple(["smelly cat", "smelly cat song"],
       R("Smelly Cat", "Quadruped", ["Breath"], 0.6, 5, (0.7, 0.8, 0.3), "Poison", temp="Lazy", inten=0.6),
       "Phoebe's Smelly Cat: a lazy cat with a Poison cloud.")
simple(["newman seinfeld", "hello newman"],
       R("Newman", "Biped", ["Heavy"], 1.7, 110, (0.3, 0.35, 0.55), temp="Sneaky", inten=0.8), "Kramer's mailman neighbour.")
simple(["puffy shirt", "the puffy shirt"],
       R("Puffy Shirt", "Cape", ["Glide", "SlowFall"], 1.2, 1, (1.0, 0.95, 0.85)), "The pirate shirt Jerry never wanted.")
simple(["cheddar the corgi", "captain holt's dog", "holt's corgi", "cheddar b99"],
       R("Cheddar", "Quadruped", ["Charge"], 0.6, 12, (0.9, 0.65, 0.3), temp="Lazy", inten=0.6), "Captain Holt's corgi.")
simple(["jake peralta", "noice"],
       R("Jake Peralta", "Biped", ["Shoots", "Fast"], 1.8, 75, (0.3, 0.35, 0.5), temp="Playful", inten=0.9),
       "Detective Peralta: Playful and Fast.")
simple(["captain raymond holt", "captain holt", "raymond holt"],
       R("Captain Holt", "Biped", ["Guard"], 1.9, 85, (0.2, 0.2, 0.25), temp="Calm", inten=1.0), "Unflappable: pure Calm.")
simple(["doug judy", "pontiac bandit", "the pontiac bandit"],
       R("The Pontiac Bandit", "Vehicle", ["Fast", "Charge"], 4.5, 1400, (0.9, 0.2, 0.2), feat=["Wheels"],
         temp="Sneaky", inten=0.8), "Doug Judy's stolen Pontiac.")
simple(["kitten mittens", "kitten mittens gloves"],
       R("Kitten Mittens", "Gauntlet", ["Blunt", "Bounce"], 0.4, 1, (0.85, 0.8, 0.9)), "Soft, quiet paws for cats.")
simple(["nightman", "the nightman"],
       R("The Nightman", "Biped", ["Blade", "Grab"], 2.0, 80, (0.2, 0.15, 0.3), "Shadow", temp="Sneaky", inten=0.9),
       "Dayman's shadowy nemesis.")
simple(["green man charlie", "charlie's green man", "green man suit"],
       R("Green Man", "Biped", ["Fast", "Bounce"], 1.8, 70, (0.2, 0.9, 0.3), temp="Chaotic", inten=1.0),
       "Charlie in a full green body suit: Chaotic and bouncy.")
simple(["charlie kelly", "wild card charlie"],
       R("Charlie Kelly", "Biped", ["Blunt", "Fast"], 1.7, 60, (0.35, 0.55, 0.3), temp="Chaotic", inten=1.0),
       "Wild card, bitches.")
simple(["frank reynolds", "danny devito's frank"],
       R("Frank Reynolds", "Biped", ["Shoots"], 1.5, 70, (0.5, 0.4, 0.3), temp="Sneaky", inten=0.9),
       "Short, sneaky, armed - and in the couch.")
simple(["greendale paintball", "community paintball", "paintball assassin"],
       R("Greendale Paintball Gun", "Gun", ["Shoots", "Sticky"], 1.0, 3, (0.2, 0.9, 0.9)), "Community's paintball war.")
simple(["the floor is lava", "floor is lava", "hot lava game"],
       R("The Floor Is Lava", "Platform", ["Trap"], 4.0, 400, (1.0, 0.35, 0.1), "Fire"), "Don't touch the floor.")
simple(["greendale human being", "human being mascot"],
       R("Greendale Human Being", "Biped", ["Bounce"], 1.8, 60, (0.9, 0.85, 0.8), temp="Playful", inten=0.8),
       "Greendale's faceless mascot.")
simple(["senor chang", "señor chang", "ben chang"],
       R("Señor Chang", "Biped", ["Blunt"], 1.6, 60, (0.4, 0.3, 0.2), temp="Chaotic", inten=1.0), "Greendale's unhinged Spanish teacher: pure Chaos.")
simple(["gob bluth", "gob's illusion", "gob's illusions"],
       R("GOB's Illusion", "Biped", ["Portal"], 1.8, 80, (0.3, 0.3, 0.35), temp="Playful", inten=0.8),
       "An illusion, Michael: GOB vanishes through a portal.")
simple(["loose seal", "buster's seal", "the loose seal"],
       R("Loose Seal", "Fish", ["Grab"], 1.6, 90, (0.4, 0.45, 0.5), "Water", temp="Sneaky", inten=0.8),
       "The seal that took Buster's hand: Sneaky Grab.")
