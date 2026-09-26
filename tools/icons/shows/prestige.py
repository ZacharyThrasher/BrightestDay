"""Prestige TV: Breaking Bad / Better Call Saul, Game of Thrones / House of the Dragon, The Last of Us, Squid Game,
The Mandalorian, Severance, Fallout, The Witcher, Peaky Blinders, The Walking Dead, Wednesday, Succession."""
import math

from reg import entry, simple, R, S
from slib import ALONG_X, ALONG_Z, TIP_NZ, TIP_NY
from rig import head, cape, hero
from bodies import person, held, quad, dragon, wheels, car_body

GOT_ICE = (0.55, 0.85, 1.0)


# ------------------------------------------------------------------------------------ Breaking Bad

@entry
def rv():
    s = S()
    L, W = 30.0, 10.0
    s.box((0, 8.4, 1.5), (W, 12.0, L - 3.0), "A")                                    # the box
    s.box((0, 5.2, -L / 2 + 1.5), (W, 6.0, 3.6), "A")                                 # cab nose
    s.box((0, 11.4, -L / 2 + 3.2), (W - 0.4, 3.4, 1.6), "A", (-25, 0, 0))              # cab-over bulge
    s.box((0, 7.0, 1.5), (W + 0.2, 1.4, L - 3.2), "D")                                 # the brown stripe
    s.box((0, 5.4, -L / 2 - 0.35), (W * 0.8, 1.4, 0.4), "D")                           # bumper grille
    s.box((0, 8.6, -L / 2 + 3.4), (W * 0.8, 2.6, 0.3), "C")                            # windscreen
    for sx in (-1, 1):
        s.box((sx * W / 2, 10.2, -1.0), (0.3, 2.6, 5.0), "C")                           # side windows
        s.box((sx * W / 2, 10.2, 8.0), (0.3, 2.6, 4.0), "C")
        s.box((sx * 3.4, 5.6, -L / 2 - 0.1), (1.6, 1.2, 0.3), "C")                      # headlights
    s.box((W / 2 + 0.1, 7.6, 3.2), (0.3, 7.0, 3.0), "D")                               # the door
    s.cyl((2.4, 16.2, 6.0), (1.4, 3.6, 1.4), "D")                                       # the cook vent on the roof
    s.cyl((2.4, 18.2, 6.0), (2.4, 0.6, 2.4), "D")
    s.box((-2.0, 14.8, 10.0), (3.4, 1.2, 3.4), "D")                                     # roof AC unit
    wheels(s, [(-W / 2, -L * 0.3), (W / 2, -L * 0.3), (-W / 2, L * 0.3), (W / 2, L * 0.3)], d=4.8, w=1.8)
    s.on(30)
    s.ell((2.4, 20.4, 6.0), (2.6, 2.4, 2.6), "C")                                       # fumes
    s.part(30, 0, (2.4, 18.4, 6.0), "Exhaust")
    s.on(255)
    s.box((-2.0, 6.4, -L / 2 + 4.2), (2.6, 1.0, 2.6), "D")
    s.seat(0, (-2.0, 6.8, -L / 2 + 4.2))
    s.on(0)
    return (["walter white's rv", "walter whites rv", "breaking bad rv", "the crystal ship", "the krystal ship",
             "fleetwood bounder", "heisenberg's rv", "the rv from breaking bad"],
            R("The Krystal Ship", "Vehicle", ["Piloted", "Shoots"], 7.5, 5000, (0.95, 0.9, 0.75), "Poison",
              feat=["Wheels", "Windows", "Pipes", "Exhaust"],
              payload=R("fulminated mercury", "Bomb", ["Explodes"], 0.4, 1, (0.6, 0.85, 1.0), "Poison")),
            s, "The Fleetwood Bounder lab: drive it, and it lobs exploding blue crystal (Poison payload Bomb).")


@entry
def heisenberg():
    s = S()
    b = person(s, top="D", pants="D", shirt="A", hair=None, style="bald")
    s.on(5)
    hy = b["hy"]
    s.cyl((0, hy + 2.0, 0), (6.4, 0.5, 6.4), "D")                                        # porkpie brim
    s.cyl((0, hy + 3.0, 0), (4.4, 1.8, 4.4), "D")
    s.box((0, hy + 2.3, 0), (4.5, 0.4, 4.5), "A")                                        # hat band
    s.box((0, hy + 0.3, -2.2), (3.8, 0.9, 0.3), "D")                                      # glasses
    s.box((0, hy - 1.7, -1.9), (1.6, 1.4, 0.6), "D")                                      # goatee
    hx, hyy, hz = held(s, b)
    s.ell((hx, hyy + 0.4, hz - 1.2), (1.6, 1.2, 1.6), "C")                                # blue crystal
    s.on(0)
    return (["heisenberg", "breaking bad heisenberg", "walter white", "walt white", "say my name", "the one who knocks", "i am the one who knocks",
             "mr white breaking bad"],
            R("Heisenberg", "Biped", ["Shoots"], 1.8, 75, (0.35, 0.7, 1.0), "Poison", temp="Aggressive", inten=0.6,
              payload=R("fulminated mercury", "Bomb", ["Explodes"], 0.4, 1, (0.6, 0.85, 1.0), "Poison")),
            s, "Heisenberg: bald, goatee, glasses, black porkpie hat. Throws exploding 'crystal' (fulminated mercury).")


@entry
def saul():
    s = S()
    b = person(s, top="A", pants="A", shirt="C", tie="P", hair="D", style="slick")
    hx, hy, hz = held(s, b)
    s.box((hx, hy + 1.0, hz - 1.0), (0.6, 2.4, 3.6), "D")                                  # briefcase
    s.box((hx, hy + 2.6, hz - 1.0), (0.3, 0.6, 1.4), "D")
    s.on(0)
    return (["saul goodman", "better call saul", "jimmy mcgill", "slippin jimmy", "s'all good man", "james mcgill"],
            R("Saul Goodman", "Biped", ["Repel", "Guard"], 1.8, 75, (0.95, 0.75, 0.3), temp="Fearful", inten=0.8),
            s, "Saul Goodman: loud suit, briefcase. Objection! He shoves you back (Repel) and hides behind loopholes.")


@entry
def gus():
    s = S()
    b = person(s, top="A", pants="D", shirt="C", tie="D", hair="D", style="short")
    s.on(5)
    hy = b["hy"]
    s.box((0, hy + 0.3, -2.2), (3.8, 0.8, 0.3), "D")                                        # glasses
    s.ell((1.1, hy - 0.2, -0.4), (2.2, 4.6, 4.3), "C")                                      # the half-blown face
    s.box((1.1, hy + 0.2, -2.2), (0.9, 0.9, 0.3), "D")
    hx, hyy, hz = held(s, b)
    s.box((hx, hyy - 0.2, hz - 1.6), (0.5, 0.8, 2.4), "C")                                  # box cutter
    s.on(0)
    return (["gus fring", "gustavo fring", "los pollos hermanos", "the chicken man"],
            R("Gus Fring", "Biped", ["Blade", "Explodes"], 1.8, 75, (1.0, 0.8, 0.25), temp="Calm", inten=1.0,
              source="Breaking Bad"),
            s, "Gus Fring: impeccably Calm, box cutter in hand - and he Explodes (straightens his tie first).")


@entry
def hector():
    s = S()
    s.box((0, 4.2, 0.4), (6.4, 1.0, 6.0), "D")                                               # seat
    s.box((0, 8.0, 3.2), (6.4, 7.0, 0.8), "D")                                               # backrest
    for sx in (-1, 1):
        s.box((sx * 3.4, 6.4, 0.4), (0.6, 0.6, 6.0), "D")                                    # armrests
    s.box((-3.4, 7.0, -2.2), (0.8, 1.0, 0.8), "C")                                           # the bell
    s.cone((-3.4, 7.8, -2.2), (1.8, 1.4, 1.8), "C", (180, 0, 0))
    s.ell((0, 7.8, 0.8), (5.0, 6.0, 3.8), "P")                                              # Tio in his chair
    s.ell((0, 12.6, 0.4), (3.4, 3.8, 3.4), "A")
    s.box((0, 14.0, 0.8), (3.2, 1.0, 3.2), "C")                                              # white hair
    for sx in (-1, 1):
        s.box((sx * 1.2, 5.0, -3.2), (1.6, 1.2, 3.0), "P")                                   # knees / legs
        s.box((sx * 1.2, 2.2, -4.2), (1.4, 4.6, 1.4), "P")
    wheels(s, [(-3.9, 1.0), (3.9, 1.0)], d=7.0, w=0.8)
    for i, sx in enumerate((-1, 1)):
        s.on(22 + i)
        s.cx((sx * 2.6, 0.8, -4.6), 1.6, 0.8, "D")
        s.part(22 + i, 0, (sx * 2.6, 0.8, -4.6), "Wheel")
    s.on(0)
    return (["hector salamanca", "tio salamanca", "hector's bell", "tio's bell", "ding ding ding",
             "hector salamanca's wheelchair"],
            R("Hector Salamanca", "Vehicle", ["Explodes", "Charge"], 1.6, 120, (0.9, 0.85, 0.55),
              feat=["Wheels", "Bell"], temp="Aggressive", inten=0.9),
            s, "Tio Salamanca in his wheelchair: ding... ding... ding. Rams you and Explodes (the bell rings first).")


# ------------------------------------------------------------------------------------ Game of Thrones

@entry
def hodor():
    s = S()
    s.box((0, 13.0, 0), (18.0, 26.0, 4.0), "P")                                                # stone frame
    s.box((0, 11.5, 0), (12.0, 23.0, 4.4), "P", sub=True)
    s.ell((0, 23.0, 0), (12.0, 6.0, 4.4), "P", sub=True)
    for i in range(5):
        s.box((-4.8 + i * 2.4, 11.5, -0.4), (2.2, 23.0, 1.6), "A")                              # oak planks
    s.ell((0, 22.6, -0.4), (11.6, 5.4, 1.6), "A")
    for y in (4.0, 12.0, 19.0):
        s.box((0, y, -1.3), (12.0, 1.0, 0.4), "D")                                              # iron bands
    s.cyl((3.6, 11.0, -1.6), (1.6, 0.4, 1.6), "D", (90, 0, 0))                                 # ring handle
    for sx in (-1, 1):                                                                          # Hodor's hands holding it shut
        s.ell((sx * 5.4, 15.0, 2.8), (2.4, 3.0, 2.4), "C")
    return (["hodor", "hold the door", "hold the door hodor", "hodor door", "hodor holding the door"],
            R("Hodor", "Wall", ["Barrier", "Guard", "Heavy"], 3.2, 800, (0.6, 0.42, 0.25)),
            s, "Hodor: a door that holds. Barrier + Guard + Heavy - nothing gets through while it stands.")


@entry
def jon_snow():
    s = S()
    b = person(s, top="D", pants="D", hair="D", style="long", arms="D", hands="D")
    s.on(5)
    s.box((0, b["hy"] - 1.6, -1.6), (3.4, 1.4, 1.0), "D")                                    # beard
    cape(s, b, t="D", collar="C", flare=1.6)
    s.on(0)
    s.ell((0, b["y_sh"] + 0.6, 0.2), (b["tw"] + 2.4, 2.4, b["td"] + 1.6), "C")                # the fur mantle
    hx, hy, hz = held(s, b)
    s.box((hx, hy, hz - 1.0), (0.5, 1.4, 1.6), "D")                                             # hilt
    s.ell((hx, hy + 0.2, hz + 0.1), (1.0, 1.0, 1.0), "C")                                       # wolf pommel
    s.box((hx, hy - 0.2, hz - 1.8), (2.6, 0.5, 0.5), "D")                                       # crossguard
    s.box((hx, hy - 0.2, hz - 8.2), (0.9, 0.3, 12.0), "C")                                      # Longclaw
    s.on(0)
    return (["jon snow", "king in the north", "aegon targaryen", "lord snow", "you know nothing jon snow"],
            R("Jon Snow", "Biped", ["Blade", "Guard"], 1.8, 80, (0.3, 0.3, 0.34), "Ice", temp="Protective", inten=0.9),
            s, "Jon Snow: Night's Watch black, fur mantle, Longclaw. Guards his people (Protective), Ice-cold Blade.")


@entry
def dracarys():
    s = S()
    dragon(s, L=22.0, H=9.0, W=7.6, span=30.0, neck_len=10.0, horns="C", spikes="A")
    return (["dracarys", "drogon", "drogon dracarys", "dany's black dragon", "daenerys' dragon", "daenerys dragon",
             "dany's dragon"],
            R("Drogon (Dracarys)", "Quadruped", ["Flies", "Breath", "ArticulatedTail"], 12.0, 20000, (0.85, 0.15, 0.1),
              "Fire", temp="Aggressive", inten=0.9, source="Game of Thrones"),
            s, "Dracarys: Daenerys' great black-and-red dragon. Flies, sweeps its tail, breathes fire on command.")


@entry
def wight_viserion():
    s = S()
    dragon(s, L=21.0, H=8.4, W=7.2, span=28.0, neck_len=9.6, horns="C", spikes="C", belly_t="C", wing_t="C")
    return (["wight viserion", "viserion", "undead viserion", "ice dragon viserion", "ice viserion", "zombie dragon got",
             "night king's dragon", "blue fire dragon"],
            R("Viserion (wight)", "Quadruped", ["Flies", "Breath", "ArticulatedTail"], 12.0, 18000, GOT_ICE, "Ice",
              temp="Chaotic", inten=0.9, source="Game of Thrones"),
            s, "Viserion raised by the Night King: an undead ice dragon breathing blue fire. Chaotic - the dead hate everyone.")


@entry
def daenerys():
    s = S()
    b = person(s, top="A", pants="A", fem=True, hair="C", style="long")
    cape(s, b, t="A", flare=1.2)
    s.on(5)
    for i in range(4):
        s.ell((0, b["hy"] + 1.6 - i * 0.2, 1.0 + i * 1.1), (1.4, 1.2, 1.3), "C")                  # braids
    s.on(0)
    x = b["arm_x"] + 0.4
    s.ell((x, b["y_sh"] + 1.6, 0.4), (2.0, 1.8, 3.4), "D")                                     # baby dragon on the shoulder
    s.ell((x, b["y_sh"] + 2.8, -1.8), (1.2, 1.2, 1.8), "D")
    for sx in (-1, 1):
        s.box((x + sx * 1.8, b["y_sh"] + 2.6, 0.8), (2.4, 1.6, 0.3), "P", (0, 0, sx * 25))
    return (["daenerys targaryen", "daenerys stormborn", "mother of dragons", "khaleesi", "breaker of chains",
             "dany targaryen"],
            R("Daenerys Targaryen", "Biped", ["Shoots"], 1.7, 55, (0.35, 0.55, 0.85), "Fire", temp="Protective",
              inten=0.7, payload=R("baby dragon", "Bird", ["Flies", "Breath", "Swarm"], 0.8, 8, (0.85, 0.15, 0.1),
                                  "Fire", count=3, temp="Aggressive", inten=0.8)),
            s, "Daenerys, Mother of Dragons: silver braids, a dragon on her shoulder. Her payload is three fire-breathing hatchlings.")


@entry
def direwolf():
    s = S()
    quad(s, L=15.0, H=6.4, W=5.0, leg_h=6.4, leg_w=1.9, body_t="A", leg_t="A", head_t="A",
         head_s=(3.8, 3.8, 4.6), snout=(2.2, 1.8, 2.6), ears=2.0,
         tail_pts=[(0, 9.6, 7.0), (0, 8.0, 10.0), (0, 5.6, 11.6)], tail_w=2.0)
    s.on(5)
    s.ell((0, 11.2, -6.4), (5.6, 5.0, 3.6), "A")                                              # neck ruff
    s.on(0)
    return (["direwolf", "dire wolf", "ghost the direwolf", "jon snow's direwolf", "stark direwolf", "nymeria",
             "summer the direwolf"],
            R("Direwolf", "Quadruped", ["Charge", "Grab"], 2.4, 150, (0.95, 0.95, 1.0), "Ice", temp="Protective",
              inten=0.9, source="Game of Thrones"),
            s, "A Stark direwolf (Ghost: white, red-eyed, silent). Protective - guards its owner, then lunges and bites.")


@entry
def the_wall():
    s = S()
    s.box((0, 20.0, 0), (64.0, 40.0, 10.0), "C")                                               # 700 feet of ice
    for i in range(6):
        s.box((-26.0 + i * 10.4, 40.6, -3.0), (5.2, 2.4, 3.0), "A")                              # battlements
    for i in range(5):                                                                          # zig-zag wooden stair
        y = 4.0 + i * 7.0
        s.box((-6.0 + (i % 2) * 12.0, y, -6.0), (14.0, 0.8, 2.4), "D", (0, 0, 20 if i % 2 else -20))
    s.box((22.0, 4.0, -9.0), (10.0, 8.0, 8.0), "P")                                             # Castle Black keep
    s.box((22.0, 9.6, -9.0), (11.0, 1.2, 9.0), "D")
    s.box((10.0, 6.0, -5.8), (3.0, 12.0, 2.0), "D")                                             # the lift
    return (["the wall westeros", "the wall of ice", "the wall got", "castle black wall", "the great wall of westeros"],
            R("The Wall", "Wall", ["Barrier", "Heavy"], 20.0, 200000, GOT_ICE, "Ice"),
            s, "The Wall: a colossal cliff of ice with Castle Black at its foot. The ultimate Barrier.")


@entry
def iron_throne():
    s = S()
    s.box((0, 1.0, 0), (16.0, 2.0, 14.0), "D")                                                 # dais
    s.box((0, 2.6, -1.0), (12.0, 1.2, 10.0), "D")
    s.box((0, 5.6, 1.0), (8.0, 4.4, 6.0), "P")                                                  # seat block
    s.box((0, 13.0, 4.2), (8.0, 14.0, 2.0), "P")                                                # back
    for sx in (-1, 1):
        s.box((sx * 4.6, 8.4, 0.6), (1.6, 2.6, 6.4), "P")                                        # arms
        for i in range(3):
            s.box((sx * 4.6, 8.6, -3.2 - i * 0.2), (0.4, 0.4, 3.0), "C", (0, sx * (10 + i * 15), 0))   # blades out front
    for i in range(15):                                                                          # the fan of swords
        a = math.radians(-80 + i * (160 / 14))
        L = 10.0 + (7 - abs(i - 7)) * 1.0
        cx, cy = math.sin(a) * (L / 2 + 3), 14.0 + math.cos(a) * (L / 2 + 3)
        s.box((cx, cy, 5.0 + (i % 2) * 0.8), (0.6, L, 0.4), "C" if i % 2 else "A", (0, 0, -math.degrees(a)))
    return (["iron throne of westeros", "iron throne", "throne of swords", "the throne of a thousand swords", "westeros throne",
             "the iron throne got"],
            R("The Iron Throne", "Wall", ["Barrier", "Blade"], 4.5, 3000, (0.6, 0.6, 0.65)),
            s, "The Iron Throne: a seat forged from a thousand surrendered swords. A Barrier that cuts whoever touches it.")


# ------------------------------------------------------------------------------------ The Last of Us / Squid Game

@entry
def giraffe():
    s = S()
    quad(s, L=12.0, H=6.0, W=4.6, leg_h=13.0, leg_w=1.4, body_t="A", leg_t="A", head_t="A", head_s=(2.4, 2.4, 4.4),
         head_c=(0, 34.0, -9.0), tail_pts=[(0, 16.0, 5.8), (0, 12.0, 6.6)], tail_w=0.6, ears=1.2)
    s.on(5)
    for sx in (-1, 1):
        s.cyl((sx * 0.7, 36.2, -8.6), (0.5, 2.0, 0.5), "D")                                      # ossicones
    for i in range(4):
        s.box((0, 21.0 + i * 3.0, -6.0 - i * 0.8), (0.8, 1.6, 1.0), "D")                          # mane
    s.on(0)
    for (x, y, z) in ((-2.3, 16, -3), (2.3, 17, 1), (-2.3, 17.5, 3), (2.3, 15.5, -2), (0, 19.2, -1), (0, 19.0, 3)):
        s.box((x, y, z), (0.4 if x else 2.0, 1.6, 1.8), "D")                                      # patches
    return (["tlou giraffe", "the last of us giraffe", "salt lake city giraffe", "ellie's giraffe"],
            R("The Giraffe", "Quadruped", ["Heavy"], 5.0, 900, (0.95, 0.75, 0.35), temp="Calm", inten=1.0,
              source="The Last of Us"),
            s, "The Salt Lake City giraffe: a moment of peace. Calm, tall, gentle - a living tether anchor.")


@entry
def front_man():
    s = S()
    b = person(s, top="D", pants="D", hair=None, style="bald", arms="D", hands="D")
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy, 0), (4.4, 5.0, 4.6), "D")                                                    # hood
    s.box((0, hy, -2.1), (3.6, 4.2, 1.0), "D")                                                  # angular mask
    s.box((0, hy + 0.4, -2.65), (3.0, 0.3, 0.2), "C")
    s.box((0, hy - 0.8, -2.65), (0.3, 2.2, 0.2), "C")
    s.box((0, hy - 1.8, -2.65), (2.4, 0.3, 0.2), "C")
    hx, hyy, hz = held(s, b)
    s.box((hx, hyy + 0.6, hz - 2.0), (0.8, 1.2, 3.4), "D")                                      # pistol
    s.on(0)
    cape(s, b, t="D", flare=1.0)
    return (["front man", "the front man squid game", "squid game front man", "squid game frontman"],
            R("The Front Man", "Biped", ["Shoots"], 1.9, 80, (0.15, 0.15, 0.17), temp="Calm", inten=1.0,
              source="Squid Game"),
            s, "The Front Man: black cloak and angular black mask. Calm, never hurries, shoots from range.")


# ------------------------------------------------------------------------------------ Fallout

@entry
def vault_boy():
    s = S()
    b = person(s, top="P", pants="P", hair="A", style="short", head_w=5.2, head_h=5.8, arms="P", hands="C")
    s.on(0)
    s.box((0, b["y_hip"] + 4.0, -b["td"] / 2 - 0.1), (1.2, 8.6, 0.4), "A")                       # yellow zip stripe
    s.box((0, b["y_hip"] + 0.4, 0), (b["tw"] + 0.2, 1.0, b["td"] + 0.2), "A")                      # belt
    s.on(5)
    hy = b["hy"]
    s.ell((0.6, hy + 3.0, -1.4), (4.0, 2.4, 3.2), "A", (0, 0, -15))                                # blond swoop
    s.box((-1.1, hy + 0.4, -2.7), (0.8, 0.9, 0.3), "D")
    s.box((1.1, hy + 0.4, -2.7), (1.0, 0.3, 0.3), "D")                                             # wink
    s.box((0, hy - 1.4, -2.7), (2.6, 0.5, 0.3), "D")                                               # grin
    s.on(4)
    x = b["arm_x"]
    s.seg((x, b["y_sh"] - 0.4, 0), (x + 2.0, b["y_sh"] + 3.4, -2.4), 1.8, 1.8, "P")                 # thumbs-up arm
    s.box((x + 2.0, b["y_sh"] + 4.2, -2.4), (1.9, 1.9, 1.9), "C")
    s.box((x + 2.0, b["y_sh"] + 5.6, -2.4), (0.8, 1.6, 0.8), "C")
    s.on(0)
    return (["vault boy", "vault-tec boy", "vault tec boy", "pip-boy mascot", "fallout thumbs up"],
            R("Vault Boy", "Biped", ["Bounce", "Blunt"], 1.6, 60, (1.0, 0.85, 0.2), "Atomic", temp="Playful", inten=0.9,
              source="Fallout"),
            s, "Vault Boy: blue-and-yellow jumpsuit, blond swoop, wink and thumbs-up. Irrepressibly Playful, faintly Atomic.")


@entry
def mini_nuke():
    s = S()
    s.ell((0, 0, 0), (5.0, 5.0, 6.4), "A")
    s.cone((0, 0, -4.2), (3.4, 3.0, 3.4), "A", TIP_NZ)
    s.cyl((0, 0, 3.6), (3.0, 2.0, 3.0), "D", ALONG_Z)
    s.box((0, 0, 4.6), (6.4, 0.4, 2.6), "D")                                                     # tail fins
    s.box((0, 0, 4.6), (0.4, 6.4, 2.6), "D")
    s.cyl((0, 0, -0.6), (5.2, 0.8, 5.2), "D", ALONG_Z)                                          # stripe
    s.cyl((0, 0, 1.4), (5.1, 0.6, 5.1), "C", ALONG_Z)
    return (["mini nuke", "mini-nuke", "fallout mini nuke", "fallout nuke"],
            R("Mini Nuke", "Bomb", ["Explodes", "Heavy"], 0.7, 12, (1.0, 0.85, 0.2), "Atomic", source="Fallout"),
            s, "A Fallout mini nuke: small, heavy, and it leaves a glowing Atomic crater.")


@entry
def nuka_cola():
    s = S()
    s.cyl((0, 2.0, 0), (3.0, 4.0, 3.0), "A")
    s.ell((0, 5.6, 0), (3.4, 4.0, 3.4), "A")                                                    # the rocket-bottle curve
    s.cyl((0, 8.6, 0), (1.6, 3.4, 1.6), "A")
    s.cyl((0, 10.6, 0), (1.9, 0.8, 1.9), "C")                                                   # cap
    s.cyl((0, 4.0, 0), (3.5, 1.6, 3.5), "C")                                                    # label
    for i in range(3):
        a = math.radians(i * 120)
        s.box((math.cos(a) * 1.8, 1.2, math.sin(a) * 1.8), (0.6, 2.4, 0.6), "D", (math.sin(a) * 25, 0, -math.cos(a) * 25))
    return (["nuka-cola", "nuka cola", "nuka-cola quantum", "nuka cola quantum", "nuka-cola bottle"],
            R("Nuka-Cola Quantum", "Ball", ["Explodes", "Bounce"], 0.5, 1, (0.3, 0.7, 1.0), "Atomic", source="Fallout"),
            s, "Nuka-Cola Quantum: a glowing blue bottle - shake, throw, it bounces and fizzes into an Atomic pop.")


@entry
def mr_handy():
    s = S()
    s.ell((0, 10.0, 0), (7.0, 6.0, 7.0), "A")                                                   # chrome body
    s.cyl((0, 10.0, 0), (7.4, 1.0, 7.4), "D")
    for i in range(3):                                                                           # three eye stalks
        a = math.radians(90 + i * 120)
        x, z = math.cos(a) * 1.6, -math.sin(a) * 1.6
        s.seg((x, 12.6, z), (x * 2.0, 16.0, z * 2.0 - 0.6), 0.5, 0.5, "D")
        s.ell((x * 2.0, 16.6, z * 2.0 - 0.6), (1.6, 1.6, 1.6), "C")
    for pid, a, role, tool in ((3, 210, "ArmLeft", "saw"), (4, 330, "ArmRight", "flame"), (14, 90, "Other", "claw")):
        r = math.radians(a)
        x, z = math.cos(r) * 3.0, -math.sin(r) * 3.0
        s.on(pid)
        tx, tz = math.cos(r) * 5.4, -math.sin(r) * 5.4 - 2.0
        s.seg((x, 9.0, z), (tx, 6.4, tz), 0.7, 0.7, "D")
        if tool == "saw":
            s.cyl((tx, 6.0, tz - 0.8), (3.4, 0.4, 3.4), "C", (90, 0, 0))
        elif tool == "flame":
            s.cz((tx, 6.2, tz - 1.0), 1.0, 2.4, "D")
        else:
            s.cone((tx, 5.8, tz - 0.4), (1.2, 1.8, 1.2), "C", TIP_NY)
        s.part(pid, 0, (x, 9.0, z), role)
    s.on(30)
    s.cone((0, 5.6, 0), (3.4, 3.6, 3.4), "C", TIP_NY)                                             # jet flame
    s.part(30, 0, (0, 7.0, 0), "Exhaust")
    s.on(0)
    s.cyl((0, 6.8, 0), (2.4, 1.4, 2.4), "D")
    return (["mr handy", "mr. handy", "mister handy", "codsworth", "mister gutsy"],
            R("Mr. Handy", "Blob", ["Flies", "Spin", "Breath"], 1.4, 60, (0.8, 0.82, 0.86), "Fire", temp="Protective",
              inten=0.8, source="Fallout"),
            s, "Mr. Handy (Codsworth): a hovering butler-bot with buzzsaw, claw and flamer. Protective, sir.")


@entry
def vertibird():
    s = S()
    s.ell((0, 6.0, 0), (6.0, 6.0, 18.0), "P")                                                  # fuselage
    s.ell((0, 7.0, -7.0), (4.4, 3.6, 5.0), "C")                                                 # canopy
    s.box((0, 7.0, 13.0), (1.8, 2.2, 12.0), "P")                                                # tail boom
    s.box((0, 10.0, 18.0), (0.6, 6.0, 3.4), "D")                                                # tail fin
    s.box((0, 7.6, 18.6), (8.0, 0.6, 2.6), "D")
    s.box((0, 8.6, 0.0), (24.0, 1.0, 4.0), "D")                                                 # wing
    for i, sx in enumerate((-1, 1)):
        s.cyl((sx * 12.0, 8.6, 0.0), (2.6, 5.0, 2.6), "A")                                       # engine nacelles
        s.on(40 + i)
        s.box((sx * 12.0, 11.6, 0.0), (15.0, 0.4, 1.4), "D")                                     # rotor blades
        s.box((sx * 12.0, 11.6, 0.0), (1.4, 0.4, 15.0), "D")
        s.part(40 + i, 0, (sx * 12.0, 11.6, 0.0), "Other")
        s.on(0)
    s.cz((0, 3.2, -8.0), 1.2, 4.0, "D")                                                         # chin minigun
    for sx in (-1, 1):
        s.box((sx * 2.6, 1.0, 0.0), (0.6, 2.0, 12.0), "D")                                       # skids
    s.on(255)
    s.box((0, 6.4, -6.0), (2.4, 1.0, 2.4), "D")
    s.seat(0, (0, 6.8, -6.0))
    s.on(0)
    return (["vertibird", "brotherhood vertibird", "fallout vertibird", "vertibird gunship"],
            R("Vertibird", "Vehicle", ["Flies", "Shoots", "Piloted"], 9.0, 8000, (0.5, 0.52, 0.45),
              feat=["Rotor", "Cockpit", "Turret"], source="Fallout"),
            s, "The Brotherhood Vertibird: tilt-rotor gunship. Pilot it; chin minigun Shoots.")


@entry
def power_armor():
    s = S()
    b = hero(s, bulk=1.55, suit="P", legs="P", arms="P", boots="D", gloves="D", torso_kind="box", arm_spread=10,
             shoulder="A")
    s.on(0)
    s.box((0, b["y_sh"] - 3.0, b["td"] / 2 + 1.6), (6.0, 7.0, 3.2), "D")                        # fusion-core backpack
    s.cyl((0, b["y_sh"] - 3.0, b["td"] / 2 + 3.4), (2.2, 0.6, 2.2), "C", (90, 0, 0))
    s.box((0, b["y_hip"] + 2.0, -b["td"] / 2 - 0.4), (5.0, 3.0, 0.8), "D")                        # abdomen plates
    s.on(5)
    hy = b["y_neck"] + 2.6
    s.box((0, hy, 0), (5.4, 5.4, 5.6), "A")                                                    # helmet
    s.box((0, hy + 0.4, -2.9), (4.0, 0.9, 0.3), "C")                                            # visor slit
    s.box((0, hy - 1.6, -3.0), (2.4, 1.6, 0.8), "D")                                            # respirator
    s.cyl((2.6, hy + 1.6, -2.4), (1.0, 1.0, 1.0), "C", (90, 0, 0))                               # helmet lamp
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(255)
    s.box((0, b["y_hip"] + 5.0, 0), (2.0, 2.0, 2.0), "D")
    s.seat(0, (0, b["y_hip"] + 5.0, 0))
    s.on(0)
    return (["t-60 power armor", "t-60 power armour", "t-45 power armor", "t-51 power armor", "brotherhood power armor",
             "power armor fallout", "fallout power armour"],
            R("T-60 Power Armor", "Biped", ["Piloted", "Heavy", "Guard"], 2.6, 450, (0.55, 0.58, 0.5), "Atomic",
              feat=["Armor"], source="Fallout"),
            s, "Fallout power armor: step inside and pilot a walking tank. Heavy, Guarded, fusion-core powered.")


# ------------------------------------------------------------------------------------ The Witcher / Peaky / TWD / Wednesday

@entry
def geralt():
    s = S()
    b = person(s, top="D", pants="D", hair="C", style="pony", arms="D", hands="D")
    s.on(5)
    for sx in (-1, 1):
        s.box((sx * 1.9, b["hy"] - 0.2, -0.2), (0.6, 4.4, 2.4), "C")                                # long white hair
    s.on(0)
    s.cyl((0, b["y_sh"] - 3.0, -b["td"] / 2 - 0.3), (1.8, 0.4, 1.8), "A", (90, 0, 0))               # wolf medallion
    for rz in (35, -35):                                                                         # two swords on the back
        s.box((0, b["y_sh"] - 1.0, b["td"] / 2 + 0.6), (0.6, 17.0, 0.5), "C", (0, 0, rz))
        s.box((math.sin(math.radians(-rz)) * 7.0, b["y_sh"] - 1.0 + math.cos(math.radians(rz)) * 7.0,
               b["td"] / 2 + 0.6), (0.9, 3.2, 0.9), "D", (0, 0, rz))
    hx, hy, hz = held(s, b)
    s.box((hx, hy - 0.2, hz - 7.8), (0.8, 0.3, 12.0), "C")
    s.box((hx, hy - 0.2, hz - 1.6), (2.4, 0.5, 0.5), "D")
    s.on(0)
    return (["geralt of rivia", "the witcher geralt", "white wolf geralt", "butcher of blaviken", "gwynbleidd"],
            R("Geralt of Rivia", "Biped", ["Blade", "Guard"], 1.9, 90, (0.85, 0.85, 0.9), temp="Calm", inten=0.9,
              source="The Witcher"),
            s, "Geralt: white hair, wolf medallion, steel and silver swords. Calm, Guarded swordsman. Hmm.")


@entry
def peaky_cap():
    s = S()
    s.ell((0, 0.8, 0.6), (8.0, 2.6, 8.4), "D")                                                  # flat cap crown
    s.box((0, 0.2, -3.6), (7.0, 0.6, 3.4), "D")                                                  # peak
    for i in range(5):
        x = -2.8 + i * 1.4
        s.box((x, 0.2, -5.4), (1.0, 0.3, 0.8), "C")                                              # razor blades in the peak
    s.cyl((0, 2.0, 0.8), (1.2, 0.4, 1.2), "D")                                                   # button
    return (["peaky blinders cap", "peaky blinder cap", "razor blade cap", "razor cap", "shelby cap", "peaky cap"],
            R("Peaky Blinders Cap", "Disc", ["Blade", "Returning"], 0.5, 0.5, (0.3, 0.28, 0.25), source="Peaky Blinders"),
            s, "A flat cap with razor blades sewn into the peak: thrown, it Blades and comes back (Returning).")


@entry
def tommy_shelby():
    s = S()
    b = person(s, top="D", pants="D", shirt="C", hair="D", style="short", arms="D")
    s.on(5)
    hy = b["hy"]
    s.ell((0, hy + 1.6, 0.4), (5.0, 1.8, 5.4), "D")                                             # flat cap
    s.box((0, hy + 1.2, -2.4), (4.2, 0.4, 2.0), "D")
    s.box((0, hy + 1.2, -3.4), (4.0, 0.2, 0.4), "C")                                            # razor glint
    s.on(7)
    s.box((0, b["y_hip"] - 1.0, b["td"] / 2 + 0.6), (b["tw"] + 0.8, 8.0, 0.8), "D", (-8, 0, 0))    # overcoat tails
    s.part(7, 0, (0, b["y_hip"] + 2.0, b["td"] / 2 + 0.4), "Cloth")
    s.on(0)
    return (["tommy shelby", "thomas shelby", "by order of the peaky blinders", "the peaky blinders", "peaky blinders"],
            R("Tommy Shelby", "Biped", ["Blade", "Shoots"], 1.8, 75, (0.35, 0.33, 0.3), temp="Calm", inten=0.9,
              source="Peaky Blinders"),
            s, "Thomas Shelby: flat cap (razor in the peak), overcoat. By order of the Peaky Blinders: Calm, then Blade.")


@entry
def lucille():
    s = S()
    s.cyl((0, 0, 0.8), (1.0, 5.0, 1.0), "D", ALONG_Z)                                            # handle
    s.cyl((0, 0, 3.4), (1.6, 0.6, 1.6), "D", ALONG_Z)                                            # knob
    s.cone((0, 0, -6.0), (2.6, 12.0, 2.6), "A", (90, 0, 0))                                      # the bat, fat end forward
    s.cyl((0, 0, -12.6), (2.6, 2.4, 2.6), "A", ALONG_Z)
    for i in range(7):                                                                            # barbed wire coils
        z = -5.0 - i * 1.3
        s.box((0, 0, z), (3.4, 0.4, 0.4), "C", (0, 0, i * 52))
        s.box((0, 0, z - 0.6), (0.4, 3.4, 0.4), "C", (0, 0, i * 52 + 20))
    return (["lucille", "negan's bat", "lucille bat", "negan's lucille", "barbed wire bat"],
            R("Lucille", "Hammer", ["Blunt", "Pierce"], 1.0, 2, (0.75, 0.6, 0.4), source="The Walking Dead"),
            s, "Lucille: Negan's baseball bat wrapped in barbed wire. Blunt + Pierce.")


@entry
def daryls_crossbow():
    s = S()
    s.box((0, 0, -1.0), (1.2, 1.8, 10.0), "D")                                                  # stock
    s.box((0, -1.6, 2.4), (1.0, 3.0, 2.2), "D", (-20, 0, 0))                                     # butt
    s.box((0, 1.4, -2.4), (0.8, 1.0, 4.0), "D")                                                  # scope
    for sx in (-1, 1):
        s.box((sx * 4.0, 0.2, -5.4), (8.0, 0.8, 1.0), "A", (0, sx * 18, 0))                       # limbs
        s.seg((sx * 7.6, 0.4, -4.2), (0, 0.4, -0.2), 0.2, 0.2, "C")                               # string
    s.cyl((0, 0.9, -5.0), (0.4, 10.0, 0.4), "C", ALONG_Z)                                        # bolt
    s.cone((0, 0.9, -10.4), (0.8, 1.2, 0.8), "C", TIP_NZ)
    return (["daryl's crossbow", "daryl dixon's crossbow", "daryl dixon", "daryls crossbow"],
            R("Daryl's Crossbow", "Bow", ["Pierce", "Shoots"], 1.0, 3, (0.4, 0.4, 0.35), source="The Walking Dead"),
            s, "Daryl Dixon's crossbow: a heavy piercing bolt.")


@entry
def wednesday():
    s = S()
    b = person(s, top="D", pants="D", fem=True, skin="C", hair="D", style="short", arms="D")
    s.on(0)
    s.box((0, b["y_hip"] - 1.4, 0), (b["tw"] + 1.0, 4.0, b["td"] + 1.0), "D")                      # dress
    s.box((0, b["y_sh"] + 0.2, -b["td"] / 2 - 0.3), (3.2, 1.0, 0.4), "C")                          # white collar
    s.on(5)
    for sx in (-1, 1):
        s.seg((sx * 1.8, b["hy"] - 0.6, 0.4), (sx * 2.2, b["hy"] - 7.0, -1.2), 1.0, 1.0, "D")        # twin braids
    s.box((0, b["hy"] + 1.6, -1.7), (3.8, 0.8, 0.6), "D")                                          # blunt fringe
    s.on(0)
    return (["wednesday addams", "wednesday dance", "wednesday's dance", "nevermore academy wednesday"],
            R("Wednesday Addams", "Biped", ["Blade", "Grab"], 1.6, 45, (0.12, 0.12, 0.14), "Shadow", temp="Sneaky",
              inten=0.9, source="Wednesday"),
            s, "Wednesday Addams: twin braids, black dress, white collar, no smile. Sneaky and quietly lethal.")


@entry
def grogu_pram():
    s = S()
    s.ell((0, 6.0, 0), (9.0, 7.0, 9.0), "A")                                                    # the pod
    s.ell((0, 8.4, -1.0), (8.0, 6.0, 8.0), "A", sub=True)
    s.cyl((0, 8.9, 0), (9.4, 0.8, 9.4), "D")                                                     # rim
    s.ell((0, 10.6, -0.2), (5.0, 4.4, 4.4), "P")                                                  # Grogu peeking
    for sx in (-1, 1):
        s.ell((sx * 4.2, 11.4, -0.2), (4.4, 1.6, 1.2), "P", (0, 0, sx * 12))                         # the ears
        s.box((sx * 1.0, 11.0, -2.5), (1.1, 1.1, 0.3), "D")
    s.cyl((0, 1.6, 0), (4.0, 1.2, 4.0), "D")                                                     # repulsor
    s.on(30)
    s.cyl((0, 0.6, 0), (3.0, 1.0, 3.0), "C")
    s.part(30, 0, (0, 1.2, 0), "Exhaust")
    s.on(0)
    return (["grogu's pram", "grogus pram", "baby yoda pram", "baby yoda's pram", "grogu's hover pram", "hover pram",
             "the pram mandalorian"],
            R("Grogu's Pram", "Vehicle", ["Flies", "Guard"], 1.2, 40, (0.8, 0.82, 0.86), temp="Protective", inten=0.9,
              source="The Mandalorian"),
            s, "Grogu's floating pram: a hover-pod that follows you around and Guards you (Protective).")


# ------------------------------------------------------------------------------------ procedural

simple(["hank's minerals", "they're minerals marie", "hank schrader's minerals"],
       R("Hank's Minerals", "Boulder", ["Swarm", "Blunt"], 0.4, 3, (0.6, 0.5, 0.7), "Earth", count=5),
       "Jesus Marie, they're minerals: a scatter of rocks.")
simple(["jesse pinkman", "yeah science", "yeah science bitch"],
       R("Jesse Pinkman", "Biped", ["Shoots", "Fast"], 1.8, 65, (0.9, 0.8, 0.2), "Poison", temp="Chaotic", inten=0.8),
       "Jesse: hoodie, beanie, Chaotic energy. Yeah, science!")
simple(["fulminated mercury", "heisenberg's crystal", "walter's crystal bomb"],
       R("Fulminated Mercury", "Bomb", ["Explodes"], 0.4, 1, (0.6, 0.85, 1.0), "Poison"), "It's not meth.")
simple(["pizza on the roof", "walter throws pizza", "breaking bad pizza"],
       R("Pizza on the Roof", "Disc", ["Ricochet", "Blunt"], 0.6, 1, (0.95, 0.7, 0.3)), "Flung onto the roof.")
simple(["wight horde", "army of the dead got", "the dead westeros", "wights of the north"],
       R("Wight Horde", "Biped", ["Swarm", "Grab"], 1.8, 50, GOT_ICE, "Ice", count=6, temp="Chaotic", inten=1.0),
       "The army of the dead: a Chaotic Ice swarm.")
simple(["arya stark", "a girl has no name", "no one arya"],
       R("Arya Stark", "Biped", ["Blade", "Fast"], 1.5, 45, (0.4, 0.35, 0.3), temp="Sneaky", inten=1.0),
       "Faceless assassin with a thin sword: Sneaky, Fast Blade.")
simple(["pyromancer's wildfire", "wildfire jar", "king's landing wildfire", "battle of the blackwater"],
       R("Wildfire", "Bomb", ["Explodes"], 0.5, 3, (0.3, 1.0, 0.3), "Fire"), "Green alchemist's fire.")
simple(["valyrian steel", "valyrian steel blade", "valyrian sword"],
       R("Valyrian Steel Sword", "Sword", ["Blade", "Pierce"], 1.2, 2, (0.4, 0.42, 0.5), "Light"),
       "Rippled dragon-forged steel.")
simple(["cordyceps horde", "infected horde", "tlou infected"],
       R("Cordyceps Horde", "Biped", ["Swarm", "Grab"], 1.8, 60, (0.7, 0.55, 0.3), "Poison", count=6, temp="Chaotic",
         inten=1.0), "A sprinting Poison swarm.")
simple(["joel miller", "joel tlou"],
       R("Joel Miller", "Biped", ["Shoots", "Guard"], 1.8, 85, (0.45, 0.4, 0.3), temp="Protective", inten=1.0),
       "Joel: Protective to a fault.")
simple(["ellie williams", "ellie tlou", "ellie's switchblade"],
       R("Ellie Williams", "Biped", ["Blade", "Fast"], 1.6, 50, (0.4, 0.5, 0.3), temp="Sneaky", inten=0.8),
       "Ellie: switchblade and a stealthy approach.")
simple(["squid game tug of war", "tug of war rope"],
       R("Tug of War Rope", "Whip", ["Link", "Pull"], 2.0, 4, (0.85, 0.75, 0.5)),
       "Squid Game's tug of war: a rope that Links you to someone and yanks.")
simple(["squid game marbles", "gganbu marbles"],
       R("Squid Game Marbles", "Ball", ["Swarm", "Ricochet"], 0.2, 0.2, (0.6, 0.8, 1.0), count=10),
       "A fistful of glass marbles.")
simple(["ddakji", "ddakji tile", "squid game ddakji", "paper tile slap"],
       R("Ddakji", "Disc", ["Blunt", "Launch"], 0.5, 0.3, (0.3, 0.5, 1.0)), "The folded paper tile game: slap and flip.")
simple(["squid game glass stepping stones", "tempered glass bridge"],
       R("Glass Stepping Stones", "Platform", ["Swarm"], 1.6, 20, (0.8, 0.95, 1.0), count=6),
       "Half the panels are tempered...")
simple(["lumon goat", "severance goat", "emile the goat", "lumon goats"],
       R("Lumon Goat", "Quadruped", ["Charge"], 1.0, 50, (0.95, 0.95, 0.9), temp="Calm", inten=0.8),
       "Mammalians Nurturable's baby goats.")
simple(["music dance experience", "defiant jazz"],
       R("Music Dance Experience", "Blob", ["Flies", "Repel"], 1.0, 1, (1.0, 0.4, 0.8), temp="Playful", inten=1.0),
       "Defiant jazz: a Playful spirit that shoves.")
simple(["kier eagan statue", "kier eagan", "lumon statue"],
       R("Kier Eagan Statue", "Pillar", ["Barrier"], 4.0, 3000, (0.6, 0.6, 0.62)), "Praise Kier.")
simple(["macrodata refinement", "scary numbers", "the scary numbers"],
       R("Scary Numbers", "Ball", ["Swarm", "Homing"], 0.3, 0.2, (0.4, 0.9, 1.0), "Psychic", count=8),
       "The numbers that feel scary: a homing Psychic swarm.")
simple(["dogmeat", "fallout dog"],
       R("Dogmeat", "Quadruped", ["Grab", "Charge"], 1.0, 35, (0.55, 0.45, 0.35), temp="Protective", inten=1.0),
       "Good boy. Protective.")
simple(["radroach", "radroaches", "rad roach"],
       R("Radroach", "Insect", ["Swarm"], 0.5, 1, (0.6, 0.4, 0.25), "Atomic", count=8, temp="Chaotic", inten=0.8),
       "Irradiated roaches.")
simple(["super mutant", "fallout super mutant", "super mutant behemoth"],
       R("Super Mutant", "Biped", ["Blunt", "Heavy"], 2.8, 300, (0.5, 0.65, 0.35), "Atomic", temp="Aggressive", inten=0.9),
       "Hulking green FEV mutant.")
simple(["cooper howard", "the ghoul fallout", "fallout ghoul"],
       R("The Ghoul", "Biped", ["Shoots"], 1.9, 70, (0.7, 0.5, 0.35), "Atomic", temp="Sneaky", inten=0.8),
       "A two-hundred-year-old gunslinger.")
simple(["quen", "quen sign", "witcher shield sign"],
       R("Quen", "Dome", ["Guard"], 2.0, 5, (1.0, 0.8, 0.3)), "The witcher's protective sign.")
simple(["toss a coin to your witcher", "jaskier", "dandelion bard"],
       R("Toss a Coin", "Disc", ["Swarm", "Ricochet"], 0.2, 0.1, (1.0, 0.85, 0.2), count=8),
       "O valley of plenty: a shower of ricocheting coins.")
simple(["roach the horse", "geralt's horse"],
       R("Roach", "Quadruped", ["Fast", "Piloted"], 2.0, 450, (0.55, 0.35, 0.2), temp="Calm", inten=0.6),
       "Geralt's horse (they're all called Roach).")
simple(["the walking dead zombies", "walking dead walkers", "twd walkers", "walker herd"],
       R("Walker Herd", "Biped", ["Swarm", "Grab"], 1.8, 60, (0.5, 0.55, 0.45), "Poison", count=6, temp="Chaotic",
         inten=0.7), "A slow, grabbing herd of walkers.")
simple(["negan", "negan smith"],
       R("Negan", "Biped", ["Blunt", "Pierce"], 1.9, 90, (0.5, 0.3, 0.25), temp="Aggressive", inten=0.9),
       "Leather jacket and Lucille.")
simple(["michonne", "michonne's katana"],
       R("Michonne", "Biped", ["Blade", "Link"], 1.8, 65, (0.4, 0.35, 0.3), temp="Protective", inten=0.8),
       "Katana, plus her two chained walkers (Link).")
simple(["rick grimes", "rick grimes' colt python"],
       R("Rick Grimes", "Biped", ["Shoots", "Guard"], 1.8, 80, (0.6, 0.5, 0.35), temp="Protective", inten=0.9),
       "The sheriff's deputy with the Colt Python.")
simple(["enid sinclair", "enid werewolf"],
       R("Enid Sinclair", "Quadruped", ["Charge", "Blade"], 1.8, 70, (1.0, 0.6, 0.85), temp="Playful", inten=0.9),
       "Wednesday's roommate, fully wolfed out.")
simple(["boar on the floor", "boar on the floor game"],
       R("Boar on the Floor", "Quadruped", ["Charge"], 1.0, 80, (0.6, 0.45, 0.4), count=3, temp="Playful", inten=1.0),
       "Logan Roy's cruel little game.")
