"""Game shows, reality TV and iconic TV personalities/objects: Wheel of Fortune, The Price Is Right, Hell's Kitchen,
The Joy of Painting, Mister Rogers' Neighborhood, Jeopardy!, Wipeout, American Ninja Warrior, Family Feud,
Double Dare."""
import math

from reg import entry, simple, R, S
from slib import ALONG_X, ALONG_Z, TIP_NZ, TIP_NY
from rig import head
from bodies import person, held


@entry
def wheel_of_fortune():
    s = S()
    n = 12
    for i in range(n):                                                                           # coloured wedges
        a = math.radians(i * 360 / n)
        s.box((math.cos(a) * 5.0, 0.6, math.sin(a) * 5.0), (10.0, 1.0, 2.7), ("P", "A", "C", "A")[i % 4],
              (0, -math.degrees(a), 0))
    s.cyl((0, -0.4, 0), (22.0, 0.8, 22.0), "D")
    s.cyl((0, 1.2, 0), (3.2, 1.2, 3.2), "D")                                                     # hub
    for i in range(24):                                                                          # pegs on the rim
        a = math.radians(i * 15)
        s.cyl((math.cos(a) * 10.6, 1.4, math.sin(a) * 10.6), (0.5, 1.4, 0.5), "C")
    s.cone((0, 1.4, -12.6), (1.6, 2.6, 1.0), "A", (90, 0, 0))                                     # the pointer
    return (["wheel of fortune", "wheel of fortune wheel", "big money wheel", "the big wheel"],
            R("Wheel of Fortune", "Disc", ["Spin", "Ricochet", "Blunt"], 2.4, 60, (1.0, 0.8, 0.2)),
            s, "The Wheel of Fortune: a giant coloured wedge wheel. Thrown, it Spins through everything and ricochets.")


@entry
def plinko():
    s = S()
    s.cyl((0, 0, 0), (6.0, 1.6, 6.0), "A", (90, 0, 0))                                            # the chip (stands on edge)
    s.cyl((0, 0, 0), (4.4, 1.9, 4.4), "C", (90, 0, 0))
    s.cyl((0, 0, 0), (2.0, 2.0, 2.0), "A", (90, 0, 0))
    for i in range(8):
        a = math.radians(i * 45)
        s.box((math.cos(a) * 2.8, math.sin(a) * 2.8, 0), (0.4, 0.4, 2.1), "D")                       # ridges
    return (["plinko", "plinko chip", "plinko board", "price is right plinko", "the price is right"],
            R("Plinko Chip", "Disc", ["DropFromSky", "Bounce", "Ricochet"], 0.8, 3, (1.0, 0.3, 0.2)),
            s, "A Plinko chip: dropped from way up (DropFromSky), pinballs off everything (Bounce + Ricochet).")


@entry
def idiot_sandwich():
    s = S()
    for sx in (-1, 1):
        s.box((sx * 4.4, 7.0, 0), (1.6, 12.0, 11.0), "A")                                        # the two slices
        s.ell((sx * 4.4, 13.0, 0), (1.6, 3.6, 11.0), "A")                                        # the crown of the loaf
        s.box((sx * 4.6, 7.0, 0), (1.4, 11.6, 10.4), "C")                                        # crumb
    s.box((0, 0.6, 0), (10.4, 1.2, 12.0), "D")                                                   # plate
    return (["idiot sandwich", "an idiot sandwich", "what are you an idiot sandwich", "gordon ramsay idiot sandwich"],
            R("Idiot Sandwich", "Cage", ["Trap", "Blunt"], 2.2, 20, (0.95, 0.75, 0.4)),
            s, "The idiot sandwich: two slices of bread slap shut on either side of the target's head (Cage + Trap).")


@entry
def gordon_ramsay():
    s = S()
    b = person(s, top="C", pants="D", hair="A", style="spiky", arms="C")
    s.on(0)
    for i in range(2):
        for sx in (-1, 1):
            s.cyl((sx * 1.6, b["y_sh"] - 2.4 - i * 2.4, -b["td"] / 2 - 0.2), (0.7, 0.3, 0.7), "D", (90, 0, 0))   # buttons
    s.on(5)
    s.box((0, b["hy"] + 0.9, -2.1), (3.2, 0.6, 0.3), "D", (0, 0, 0))                                     # furrowed brow
    s.box((0, b["hy"] - 1.4, -2.1), (1.8, 1.0, 0.3), "D")                                                # shouting mouth
    hx, hy, hz = held(s, b)
    s.box((hx, hy, hz - 1.0), (0.6, 1.0, 2.0), "D")                                                       # chef's knife
    s.box((hx, hy - 0.3, hz - 4.6), (0.3, 1.6, 5.2), "C")
    s.on(0)
    return (["gordon ramsay", "chef ramsay", "where's the lamb sauce", "wheres the lamb sauce", "it's raw",
             "it's fucking raw"],
            R("Gordon Ramsay", "Biped", ["Repel", "Blade"], 1.9, 85, (1.0, 0.8, 0.35), temp="Aggressive", inten=1.0,
              source="Hell's Kitchen"),
            s, "Chef Ramsay: whites, spiky blond hair, a chef's knife. He yells you backwards (Repel). Aggressive.")


def tree(s, x, y, z, k=1.0):
    s.cyl((x, y + 1.6 * k, z), (1.4 * k, 3.2 * k, 1.4 * k), "D")
    for i, (w, h) in enumerate(((8.0, 6.0), (6.4, 5.4), (4.6, 4.8), (2.8, 4.0))):
        s.cone((x, y + (3.4 + i * 2.8 + h / 2) * k, z), (w * k, h * k, w * k), "A" if i % 2 == 0 else "P")


@entry
def happy_trees():
    s = S()
    tree(s, 0, 0, 0, 1.3)
    return (["happy little trees", "happy little tree", "happy trees", "a happy little tree", "bob ross trees"],
            R("Happy Little Trees", "Pillar", ["Swarm"], 5.0, 300, (0.25, 0.75, 0.35), "Earth", count=5),
            s, "Bob Ross's happy little trees: a whole grove sprouts (Pillar Swarm) - five fresh tether anchors.")


@entry
def bob_ross():
    s = S()
    b = person(s, top="A", pants="D", shirt="C", hair="D", style="short")
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy + 1.0, 0.4), (7.6, 7.0, 7.4), "D")                                               # the perm
    s.ell((0, hy - 1.6, -1.2), (4.2, 3.4, 3.2), "D")                                              # beard
    s.box((0, hy + 0.2, -2.6), (3.0, 1.0, 0.4), "P")                                              # face in the fluff
    s.on(3)
    hx, hy2, hz = b["hands"][-1]
    s.cyl((hx - 1.0, hy2 + 1.4, hz - 2.0), (6.0, 0.4, 5.0), "C", (0, 0, 80))                      # palette
    for i, t in enumerate(("P", "A", "D")):
        s.cyl((hx - 1.4, hy2 + 2.2 + i * 1.2, hz - 3.2 + i * 0.6), (1.0, 0.4, 1.0), t, (0, 0, 80))
    hx, hy3, hz = held(s, b)
    s.box((hx, hy3 + 1.6, hz - 1.6), (0.4, 4.0, 0.4), "D", (-30, 0, 0))                           # brush
    s.box((hx, hy3 + 3.8, hz - 2.8), (0.8, 1.2, 0.8), "A", (-30, 0, 0))
    s.on(0)
    return (["bob ross", "the joy of painting", "joy of painting", "happy little accidents bob ross", "bob ross painter"],
            R("Bob Ross", "Biped", ["Shoots"], 1.8, 80, (0.4, 0.75, 1.0), temp="Calm", inten=1.0, source="The Joy of Painting",
              payload=R("happy little tree", "Pillar", ["Barrier"], 4.0, 200, (0.25, 0.75, 0.35), "Earth")),
            s, "Bob Ross: the perm, the beard, palette and brush. Perfectly Calm - he paints happy little trees into the\n"
               "arena (payload: a Pillar tree you can tether to).")


@entry
def mister_rogers():
    s = S()
    b = person(s, top="A", pants="D", shirt="C", tie="D", hair="D", style="short", arms="A")
    s.on(0)
    s.box((0, b["y_hip"] + 4.4, -b["td"] / 2 - 0.2), (1.0, 8.4, 0.3), "D")                           # cardigan zip
    s.box((0, b["y_sh"] - 0.4, 0), (b["tw"] + 0.4, 1.2, b["td"] + 0.4), "A")
    for sx in (-1, 1):
        s.box((sx * 1.6, 0.9, -0.4), (2.4, 1.2, 3.8), "C")                                            # sneakers
    return (["mister rogers", "mr rogers", "mr. rogers", "fred rogers", "won't you be my neighbor",
             "mister rogers' neighborhood"],
            R("Mister Rogers", "Biped", ["Guard"], 1.8, 70, (0.85, 0.15, 0.2), temp="Protective", inten=1.0,
              source="Mister Rogers' Neighborhood"),
            s, "Mister Rogers: red cardigan, sneakers. The most Protective neighbour there is.")


@entry
def sweeper_arm():
    s = S()
    s.cyl((0, 4.0, 0), (4.0, 8.0, 4.0), "D")                                                      # pivot post
    s.cyl((0, 8.6, 0), (5.0, 1.2, 5.0), "A")
    s.cyl((12.0, 7.0, 0), (2.4, 22.0, 2.4), "A", ALONG_X)                                           # the padded arm
    for i in range(5):
        s.cyl((4.0 + i * 4.0, 7.0, 0), (2.8, 1.0, 2.8), "C", ALONG_X)                                  # padding rings
    s.cyl((0, 0.4, 0), (30.0, 0.8, 30.0), "P")                                                     # the pool ring
    return (["sweeper arm", "wipeout sweeper", "the sweeper", "wipeout sweeper arm"],
            R("The Sweeper", "Wall", ["Spin", "Launch"], 7.0, 400, (1.0, 0.3, 0.2), source="Wipeout"),
            s, "Wipeout's sweeper arm: a padded bar that Spins around and Launches everyone off their feet.")


@entry
def warped_wall():
    s = S()
    for i in range(10):                                                                            # quarter-pipe curve
        a = math.radians(i * 9.0)
        a2 = math.radians((i + 1) * 9.0)
        R0 = 16.0
        z0, y0 = -math.sin(a) * R0, R0 - math.cos(a) * R0
        z1, y1 = -math.sin(a2) * R0, R0 - math.cos(a2) * R0
        s.seg((0, y0, z0 + 8.0), (0, y1, z1 + 8.0), 14.0, 1.2, "P" if i % 2 else "A")
    s.box((0, 16.6, -9.0), (14.0, 1.2, 4.0), "D")                                                  # top deck
    s.box((0, 8.0, -10.6), (14.0, 16.0, 1.0), "D")
    s.box((0, 17.8, -7.4), (15.0, 0.8, 0.8), "C")                                                  # the lip
    return (["warped wall", "ninja warrior warped wall", "american ninja warrior wall", "the warped wall"],
            R("Warped Wall", "Ramp", ["Launch"], 4.2, 800, (1.0, 0.6, 0.15), source="American Ninja Warrior"),
            s, "American Ninja Warrior's warped wall: a curved Ramp that Launches your run up and over.")


@entry
def survey_says():
    s = S()
    s.box((0, 10.0, 0), (4.0, 22.0, 2.0), "A", (0, 0, 35))
    s.box((0, 10.0, 0), (4.0, 22.0, 2.0), "A", (0, 0, -35))
    s.box((0, 0.6, 0), (16.0, 1.2, 4.0), "D")
    return (["survey says", "family feud x", "big red x", "family feud strike", "strike three family feud"],
            R("Survey Says X", "Wall", ["Barrier", "Guard"], 3.0, 200, (0.95, 0.1, 0.1), source="Family Feud"),
            s, "The big red X: survey says... blocked (Barrier + Guard).")


simple(["jeopardy buzzer", "jeopardy clicker", "jeopardy signaling device"],
       R("Jeopardy Buzzer", "Gun", ["Shoots", "Fast"], 0.4, 0.3, (0.3, 0.4, 1.0), "Electric"), "Buzz in first.")
simple(["daily double", "daily double jeopardy"],
       R("Daily Double", "Bomb", ["Explodes"], 0.6, 2, (0.2, 0.3, 1.0), count=2), "Double the stakes.")
simple(["happy little accident", "happy accident", "happy accidents"],
       R("Happy Little Accident", "Bomb", ["Explodes", "Bounce"], 0.6, 2, (0.4, 0.75, 1.0), temp="Playful", inten=0.8),
       "We don't make mistakes.")
simple(["neighborhood trolley", "mister rogers trolley", "trolley of make-believe", "the neighborhood of make-believe"],
       R("Neighborhood Trolley", "Vehicle", ["Fast"], 1.2, 20, (0.85, 0.15, 0.2), feat=["Wheels", "Bell"],
         temp="Playful", inten=0.8), "Ding-ding: off to Make-Believe.")
simple(["nickelodeon slime", "you got slimed", "double dare slime", "green slime dump"],
       R("Nickelodeon Slime", "Blob", ["DropFromSky", "Sticky"], 1.6, 30, (0.4, 1.0, 0.2), "Poison", temp="Playful",
         inten=0.9), "Dumped from above.")
simple(["wipeout big balls", "big red balls", "the big balls wipeout"],
       R("Big Red Balls", "Trampoline", ["Bounce", "Swarm"], 2.4, 30, (0.95, 0.1, 0.1), count=4), "Bounce... splash.")
simple(["come on down", "come on down price is right"],
       R("Come On Down!", "Trampoline", ["Launch", "Bounce"], 2.0, 50, (1.0, 0.8, 0.2)), "You're the next contestant.")
simple(["showcase showdown", "price is right big wheel"],
       R("Showcase Showdown Wheel", "Disc", ["Spin", "Returning"], 2.0, 30, (1.0, 0.8, 0.2)), "A dollar! A dollar!")
simple(["gladiators pugil stick", "pugil stick", "american gladiators pugil stick"],
       R("Pugil Stick", "Staff", ["Blunt", "Bounce"], 1.4, 3, (1.0, 0.3, 0.3)), "Padded jousting pole.")
simple(["deal or no deal briefcase", "deal or no deal"],
       R("Deal or No Deal Briefcase", "Bomb", ["Explodes", "Swarm"], 0.5, 3, (0.8, 0.8, 0.85), count=3),
       "One of them has the million.")
simple(["the weakest link", "you are the weakest link"],
       R("The Weakest Link", "Whip", ["Link", "Pull"], 2.0, 3, (0.3, 0.4, 1.0)), "Goodbye.")
simple(["survivor torch", "the tribe has spoken"],
       R("Survivor Torch", "Staff", ["Breath"], 1.4, 2, (1.0, 0.5, 0.1), "Fire"), "The tribe has spoken.")
simple(["the golden buzzer", "golden buzzer"],
       R("Golden Buzzer", "Bomb", ["Explodes", "DropFromSky"], 0.8, 5, (1.0, 0.8, 0.2), "Light"),
       "Confetti from the sky.")
