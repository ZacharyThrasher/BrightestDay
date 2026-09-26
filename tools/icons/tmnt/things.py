"""TMNT vehicles, places and items: Technodrome, Party Wagon, Shellraiser, Blimp, pizza, ooze, portal..."""
import math

from tlib import S, R, KRANG_PINK, FOOT_RED, OOZE_GREEN, MIKEY_ORANGE, ALONG_Z, ALONG_X, TIP_NZ

PIZZA = R("pizza", "Disc", ["Sticky", "Blunt"], 0.5, 1.0, (1.0, 0.8, 0.3), seed=1987)
VAN = (0.95, 0.78, 0.3)


def _wheels(s, pts, d=4.8, w=1.6, first=20, hub="A"):
    for i, (x, y, z) in enumerate(pts):
        s.on(first + i)
        s.cx((x, y, z), d, w, "D")
        s.cx((x, y, z), d * 0.5, w + 0.2, hub)
        s.part(first + i, 0, (x, y, z), "Wheel")
    s.on(0)


def technodrome():
    s = S(k=1.0)
    s.ell((0, 15.0, 0), (28.0, 26.0, 28.0), "A")                                  # the battle sphere
    s.cyl((0, 13.0, 0), (29.6, 3.0, 29.6), "D")                                   # equator band
    for i in range(12):
        a = i / 12 * 2 * math.pi
        s.box((math.sin(a) * 14.9, 13.0, -math.cos(a) * 14.9), (1.6, 1.2, 0.6), "C", (0, -i * 30, 0))   # port lights
    for i in range(4):                                                            # meridian plates
        s.ell((0, 15.0, 0), (0.8, 26.3, 28.3), "D", (0, i * 45, 0))
    s.cyl((0, 27.8, 0), (8.0, 2.0, 8.0), "D")                                     # eye mount
    s.ell((0, 31.4, 0), (8.6, 8.6, 8.6), "C")                                     # the eye turret
    s.ell((0, 31.4, -3.8), (4.4, 4.4, 1.6), "A")
    s.box((0, 31.4, -4.6), (0.9, 3.8, 0.4), "D")                                  # slit pupil
    for sx in (-1, 1):                                                            # main guns
        s.cz((sx * 5.0, 17.0, -15.0), 1.6, 6.0, "D")
        s.cz((sx * 5.0, 17.0, -18.2), 1.0, 1.0, "C")
    for i in range(8):                                                            # tread ring at the base
        a = i / 8 * 2 * math.pi
        s.box((math.sin(a) * 10.0, 2.0, -math.cos(a) * 10.0), (7.0, 4.0, 3.4), "D", (0, -i * 45, 0))
    for i, sx in enumerate((-1, 1)):                                              # burrowing drills
        s.on(30 + i)
        s.cone((sx * 7.0, 5.0, -14.6), (4.0, 6.0, 4.0), "C", TIP_NZ)
        s.part(30 + i, 0, (sx * 7.0, 5.0, -13.0), "Wheel")
    s.on(0)
    s.seat(0, (0, 15.0, 0))
    names = ["technodrome", "the technodrome", "technodrome tmnt", "krang's technodrome", "shredder's technodrome",
             "technodrome fortress", "technodrome battle station", "technodrome eye"]
    r = R("Technodrome", "Vehicle", ["Piloted", "Charge", "Shoots", "Heavy", "Stomp"], 60.0, 5000000.0,
          (0.62, 0.62, 0.72), "technodrome", feat=["Treads", "Drill", "Barrels", "Turret", "Armor"],
          payload=R("technodrome blast", "Ball", ["Explodes", "Fast"], 1.5, 50.0, elem="Electric", seed=87))
    return names, r, s, ("The Technodrome: Krang and Shredder's mobile fortress - a sphere on treads with the eye\n"
                          "turret on top and burrowing drills. Pilot it; colossal in no-limits (60 m), else clamped.")


def party_wagon():
    s = S(k=1.5)
    s.box((0, 8.2, 1.0), (12.0, 10.0, 25.0), "P")                                 # van body
    s.box((0, 5.6, -13.4), (12.0, 4.8, 4.0), "P")                                 # hood
    s.box((0, 10.6, -11.8), (10.4, 4.2, 0.5), "C", (-18, 0, 0))                   # windshield
    for sx in (-1, 1):
        s.box((sx * 6.05, 10.2, -7.4), (0.3, 3.2, 6.0), "D")                       # cab windows
        s.box((sx * 6.05, 6.4, 3.0), (0.3, 1.2, 20.0), "A")                        # stripe
    s.ell((0, 13.0, 2.0), (11.0, 3.4, 21.0), "D")                                 # turtle-shell roof
    for dz in (-4.0, 2.0, 8.0):
        s.box((0, 14.4, dz), (8.0, 0.5, 0.5), "P")
    s.box((0, 14.5, 2.0), (0.5, 0.5, 16.0), "P")
    for sx in (-1, 1):                                                            # twin laser cannons
        s.cz((sx * 3.2, 15.2, -5.0), 1.2, 8.0, "D")
        s.cz((sx * 3.2, 15.2, -9.2), 0.8, 0.6, "C")
    s.box((0, 15.4, 12.4), (11.0, 0.8, 3.0), "D")                                 # super-spoiler
    s.box((0, 3.4, -15.8), (12.6, 1.8, 1.4), "A")                                 # beak bumper
    s.cone((0, 3.6, -16.8), (4.0, 1.8, 2.0), "A", TIP_NZ)
    s.cz((0, 6.0, -15.7), 4.2, 1.2, "D")                                          # spare tyre w/ logo
    s.cz((0, 6.0, -16.3), 2.0, 0.4, "A")
    for sx in (-1, 1):
        s.box((sx * 4.6, 6.4, -15.5), (1.6, 1.2, 0.4), "C")                        # headlights
        s.box((sx * 3.0, 5.2, -15.6), (1.2, 1.4, 0.3), "D")                        # grimacing grille teeth
    _wheels(s, [(-6.2, 2.4, -10.0), (6.2, 2.4, -10.0), (-6.2, 2.4, 9.0), (6.2, 2.4, 9.0)])
    s.seat(0, (0, 8.0, -7.0))
    names = ["party wagon", "turtle party wagon", "turtle van", "the turtle van", "tmnt van", "ninja turtle van",
             "turtle party van", "tmnt party wagon"]
    r = R("Turtle Party Wagon", "Vehicle", ["Piloted", "Shoots", "Charge"], 5.5, 3500.0, VAN, "party_wagon",
          feat=["Wheels", "Barrels", "Windows", "Headlight"], payload=PIZZA)
    return names, r, s, ("The Turtle Van / Party Wagon (1987): shell roof, twin roof lasers, beak bumper, spare tyre.\n"
                          "Drive it, ram with Charge, and it fires sticky pizzas.")


def shellraiser():
    s = S(k=1.5)
    s.box((0, 8.0, 0), (11.0, 11.0, 38.0), "P")                                  # subway car
    for sx in (-1, 1):
        s.box((sx * 5.55, 10.0, 0), (0.3, 2.6, 32.0), "D")                         # window strip
        for dz in (-12.0, 0.0, 12.0):
            s.box((sx * 5.6, 7.0, dz), (0.3, 5.4, 3.0), "A")                        # doors
    s.box((0, 13.8, 0), (10.0, 1.2, 36.0), "D")                                   # roof armour
    s.box((0, 8.0, -19.2), (10.6, 10.0, 0.8), "A")                                # front plate
    for sx in (-1, 1):
        s.ell((sx * 2.6, 10.4, -19.8), (3.0, 2.2, 1.0), "C")                       # turtle-face eyes
    s.box((0, 10.4, -19.9), (10.0, 1.0, 0.6), "D")                                # mask band
    s.box((0, 5.6, -19.9), (6.0, 1.6, 0.6), "D")                                  # grille mouth
    s.box((0, 2.8, -20.2), (10.8, 1.2, 1.4), "D")                                 # plough
    for dz in (-6.0, 6.0):                                                        # manhole launchers
        s.cyl((0, 16.0, dz), (5.0, 3.6, 5.0), "D", (-35, 0, 0))
        s.cyl((0, 17.2, dz - 1.4), (3.6, 0.8, 3.6), "C", (-35, 0, 0))
    _wheels(s, [(-5.6, 2.0, -13.0), (5.6, 2.0, -13.0), (-5.6, 2.0, 0.0), (5.6, 2.0, 0.0), (-5.6, 2.0, 13.0),
                (5.6, 2.0, 13.0)], d=4.0, w=1.2)
    s.seat(0, (0, 8.0, -14.0))
    manhole = R("manhole cover", "Disc", ["Blunt", "Heavy"], 0.8, 40.0, seed=2012)
    names = ["shellraiser", "the shellraiser", "shell raiser", "shellraiser tmnt", "turtle subway car"]
    r = R("Shellraiser", "Vehicle", ["Piloted", "Shoots", "Charge", "Heavy"], 6.0, 20000.0, (0.35, 0.8, 0.35),
          "shellraiser", feat=["Wheels", "Barrels", "Plow", "Armor", "Headlight"], payload=manhole)
    return names, r, s, "The Shellraiser (2012): Donnie's armoured subway car. Rams and launches manhole covers."


def turtle_blimp():
    s = S(k=1.4)
    s.ell((0, 17.0, 0), (14.0, 12.0, 36.0), "P")                                  # envelope
    s.ell((0, 19.4, 1.0), (13.0, 9.0, 30.0), "D")                                 # shell pattern on top
    for dz in (-8.0, 0.0, 8.0):
        s.box((0, 23.9, dz), (7.0, 0.5, 0.5), "P")
    s.ell((0, 16.0, -18.0), (7.6, 7.0, 6.4), "P")                                 # turtle head nose
    s.ell((0, 17.2, -18.0), (8.4, 1.8, 7.0), "A")                                 # mask band
    for sx in (-1, 1):
        s.box((sx * 1.6, 17.2, -21.4), (1.6, 1.0, 0.5), "C")
    for rz in (0, 90, 180, 270):                                                  # tail fins
        s.box((0, 17.0, 16.0), (0.8, 12.0, 6.0), "A", (0, 0, rz))
    s.box((0, 8.0, -2.0), (5.6, 3.6, 11.0), "A")                                  # gondola
    s.box((0, 8.4, -7.6), (4.6, 2.0, 0.4), "C")
    for i, sx in enumerate((-1, 1)):
        s.box((sx * 3.8, 8.2, 3.0), (2.4, 0.8, 0.8), "D")
        s.on(20 + i)
        s.cyl((sx * 5.4, 8.2, 4.0), (4.0, 0.4, 4.0), "C", ALONG_Z)                # propellers
        s.box((sx * 5.4, 8.2, 4.0), (4.0, 0.6, 0.3), "D")
        s.part(20 + i, 0, (sx * 5.4, 8.2, 4.0), "Wheel")
    s.on(0)
    s.seat(0, (0, 8.0, -3.0))
    names = ["turtle blimp", "the turtle blimp", "tmnt blimp", "ninja turtle blimp", "turtle airship"]
    r = R("Turtle Blimp", "Vehicle", ["Flies", "Piloted", "Shoots", "SlowFall"], 6.0, 1500.0, (0.95, 0.8, 0.25),
          "turtle_blimp", feat=["Propeller", "Cockpit"],
          payload=R("water balloon", "Bomb", ["Explodes", "Bounce"], 0.5, 2.0, elem="Water", seed=88))
    return names, r, s, "The Turtle Blimp: a shell-backed airship with a turtle face. Flies slow, drops water balloons."


def turtle_tank():
    s = S(k=1.5)
    s.box((0, 5.0, 0), (12.0, 4.2, 22.0), "P")                                    # hull
    s.box((0, 5.2, -11.8), (11.0, 3.0, 2.4), "P", (-30, 0, 0))
    for sx in (-1, 1):
        s.box((sx * 6.8, 3.2, 0), (2.8, 5.0, 24.0), "D")                           # tread housings
    _wheels(s, [(sx * 6.8, 3.0, z) for sx in (-1, 1) for z in (-8.0, 0.0, 8.0)], d=4.2, w=3.0)
    s.ell((0, 9.4, 1.0), (11.0, 6.0, 13.0), "D")                                  # shell turret
    for dz in (-2.0, 3.0):
        s.box((0, 12.2, dz + 1.0), (6.0, 0.5, 0.5), "P")
    s.ell((0, 9.4, -6.8), (4.8, 4.4, 4.8), "P")                                   # turtle head
    s.ell((0, 10.2, -6.8), (5.4, 1.4, 5.4), "A")                                  # mask band
    for sx in (-1, 1):
        s.box((sx * 1.1, 10.2, -9.4), (1.0, 0.7, 0.4), "C")
    s.cz((0, 8.2, -13.0), 1.8, 10.0, "D")                                         # cannon out of the mouth
    s.cz((0, 8.2, -18.2), 2.4, 1.0, "C")
    s.seat(0, (0, 9.0, 1.0))
    names = ["turtle tank", "the turtle tank", "tmnt tank", "ninja turtle tank", "shell tank"]
    r = R("Turtle Tank", "Vehicle", ["Piloted", "Shoots", "Heavy", "Guard"], 5.0, 30000.0, (0.9, 0.6, 0.2),
          "turtle_tank", feat=["Treads", "Turret", "Barrels", "Armor"], payload=PIZZA)
    return names, r, s, "Turtle Tank: a shell turret with a turtle head and a cannon in its mouth. Shoots pizza."


def foot_copter():
    s = S(k=1.5)
    s.ell((0, 7.0, -2.0), (8.0, 7.0, 14.0), "P")                                  # fuselage
    s.ell((0, 7.8, -7.0), (6.0, 4.4, 5.0), "C")                                   # canopy
    s.box((0, 8.0, 10.0), (2.0, 2.2, 14.0), "P")                                  # tail boom
    s.box((0, 10.4, 16.4), (0.6, 5.0, 3.0), "P")                                  # fin
    for sx in (-1, 1):
        s.box((sx * 4.02, 7.0, -1.0), (0.4, 3.0, 3.0), "A")                        # Foot emblem
        s.box((sx * 3.4, 1.2, -2.0), (0.8, 0.8, 12.0), "D")                        # skids
        s.box((sx * 3.0, 2.4, -5.0), (0.6, 2.4, 0.6), "D")
        s.box((sx * 3.0, 2.4, 1.0), (0.6, 2.4, 0.6), "D")
        s.cz((sx * 4.6, 5.0, -6.0), 1.0, 4.0, "D")                                 # rocket pods
    s.cyl((0, 11.0, -2.0), (1.2, 1.6, 1.2), "D")
    s.on(20)
    s.box((0, 12.0, -2.0), (26.0, 0.4, 1.4), "D")                                 # main rotor
    s.box((0, 12.0, -2.0), (1.4, 0.4, 26.0), "D")
    s.part(20, 0, (0, 12.0, -2.0), "Wheel")
    s.on(21)
    s.box((0.8, 10.4, 16.8), (0.3, 5.0, 1.0), "D")                                # tail rotor
    s.box((0.8, 10.4, 16.8), (0.3, 1.0, 5.0), "D")
    s.part(21, 0, (0.8, 10.4, 16.8), "Wheel")
    s.on(0)
    s.seat(0, (0, 7.0, -4.0))
    names = ["foot copter", "foot clan helicopter", "foot helicopter", "foot clan copter", "footcopter"]
    r = R("Foot Clan Helicopter", "Vehicle", ["Flies", "Piloted", "Shoots"], 6.0, 2500.0, FOOT_RED, "foot_copter",
          feat=["Rotor", "Cockpit", "Barrels"],
          payload=R("foot rocket", "Rocket", ["Homing", "Explodes"], 0.8, 5.0, elem="Fire", seed=66))
    return names, r, s, "The Foot Clan helicopter: rotor, rocket pods, Foot emblem. Flies and fires homing rockets."


def news_van():
    s = S(k=1.5)
    s.box((0, 8.0, 1.5), (11.0, 10.0, 22.0), "P")
    s.box((0, 5.4, -11.8), (11.0, 4.4, 4.0), "P")
    s.box((0, 10.2, -9.9), (9.6, 3.6, 0.5), "C", (-15, 0, 0))                     # windshield
    for sx in (-1, 1):
        s.box((sx * 5.55, 8.4, 3.0), (0.3, 5.0, 5.0), "A")                         # the big "6" panel
        s.box((sx * 5.7, 9.2, 3.0), (0.3, 2.2, 1.0), "C")
        s.box((sx * 5.7, 7.6, 3.4), (0.3, 1.6, 2.0), "C")
        s.box((sx * 4.2, 6.0, -13.9), (1.6, 1.0, 0.4), "C")
    s.cyl((0, 14.0, 4.0), (0.8, 2.4, 0.8), "D")                                   # mast
    s.cyl((0, 16.0, 4.0), (7.0, 0.6, 7.0), "C", (-55, 0, 0))                      # satellite dish
    s.cyl((0, 16.4, 3.4), (0.4, 3.0, 0.4), "D", (-55, 0, 0))
    s.cyl((-4.0, 15.0, 10.0), (0.3, 6.0, 0.3), "D")                               # whip antenna
    _wheels(s, [(-5.8, 2.2, -8.4), (5.8, 2.2, -8.4), (-5.8, 2.2, 8.4), (5.8, 2.2, 8.4)], d=4.4, w=1.4)
    s.seat(0, (0, 8.0, -6.0))
    names = ["april's news van", "april o'neil's van", "channel 6 news van", "channel 6 van", "channel six van",
             "channel 6 news", "april's van"]
    r = R("Channel 6 News Van", "Vehicle", ["Piloted", "Charge", "Fast"], 5.0, 2800.0, (0.95, 0.85, 0.2), "news_van",
          feat=["Wheels", "Dish", "Antenna", "Headlight", "Windows"])
    return names, r, s, "April's Channel 6 news van: satellite dish, antenna, the big 6. Drive it and ram."


# ------------------------------------------------------------------------------------- items

def _pizza_disc(s, r=7.0, y=0.0, missing=True):
    s.cyl((0, y, 0), (2 * r, 1.0, 2 * r), "A")                                    # crust
    s.cyl((0, y + 0.4, 0), (2 * r - 1.6, 0.6, 2 * r - 1.6), "P")                  # cheese
    for (x, z) in ((-2.6, -2.0), (2.4, -3.0), (0.4, 1.6), (-3.2, 3.2), (3.4, 2.4), (0.0, -4.8), (-4.6, -0.4)):
        s.cyl((x, y + 0.8, z), (2.0, 0.4, 2.0), "D")                               # pepperoni
    if missing:
        s.box((r * 0.35, y, r * 0.35), (r * 0.9, 2.0, r * 0.9), "P", (0, 45, 0), sub=True)   # a slice gone
    for (x, z) in ((-r + 0.4, 2.0), (r - 0.6, -2.4), (-1.0, -r + 0.4)):
        s.box((x, y - 0.9, z), (1.0, 1.4, 0.8), "C")                               # cheese drips


def mikey_pizza():
    s = S(k=1.4)
    _pizza_disc(s)
    names = ["mikey's pizza", "michelangelo's pizza", "ninja pizza", "turtle pizza", "tmnt pizza", "cowabunga pizza",
             "pizza power", "sewer pizza"]
    r = R("Mikey's Pizza", "Disc", ["Sticky", "Blunt"], 0.6, 1.0, (1.0, 0.75, 0.3), "mikey_s_pizza")
    return names, r, s, "Mikey's pizza, thrown like a frisbee: Returning Disc, the cheese is Sticky."


def pizza_cutter():
    s = S(k=1.8)
    s.cyl((0, 0, -5.0), (7.0, 0.5, 7.0), "C", ALONG_X)                            # the wheel blade
    s.cyl((0, 0, -5.0), (2.0, 1.4, 2.0), "D", ALONG_X)
    s.box((0.0, 0.0, -1.2), (0.6, 1.0, 5.0), "D")                                 # fork
    s.box((0, 0, 2.2), (1.4, 1.4, 5.0), "A")                                      # handle (grip at origin)
    names = ["mikey's pizza cutter", "michelangelo's pizza cutter", "ninja pizza cutter", "turtle pizza cutter"]
    r = R("Mikey's Pizza Cutter", "Disc", ["Blade", "Spin"], 0.5, 1.0, MIKEY_ORANGE, "mikey_s_pizza_cutter")
    return names, r, s, "A thrown pizza-cutter wheel: Disc + Blade + Spin."


def pizza_thrower():
    s = S(k=1.5)
    s.cz((0, 0, -6.0), 4.2, 14.0, "P")                                            # barrel
    s.cz((0, 0, -13.2), 5.0, 1.0, "A")
    s.box((0, 4.2, -2.0), (8.0, 1.6, 8.0), "A")                                   # stacked pizza boxes
    s.box((0, 5.8, -2.0), (7.6, 1.4, 7.6), "P")
    s.box((0, 7.2, -2.0), (7.2, 1.4, 7.2), "A")
    s.box((0, -2.6, 1.6), (1.6, 3.6, 1.8), "D")                                   # grip
    s.box((0, -0.4, 4.0), (3.0, 3.0, 5.0), "D")                                   # stock
    names = ["pizza thrower", "pizza launcher", "turtle pizza thrower", "pizza cannon", "pizza bazooka"]
    r = R("Pizza Thrower", "Cannon", ["Shoots", "Sticky"], 1.4, 20.0, (1.0, 0.75, 0.3), "pizza_thrower", payload=PIZZA)
    return names, r, s, "The Pizza Thrower: a cannon that lobs sticky pizzas."


def _canister(s, liquid="A", h=8.0, d=4.0, label="D"):
    s.cyl((0, 0, 0), (d, h, d), liquid)                                           # glowing contents
    for yy in (-h * 0.3, h * 0.3):
        s.cyl((0, yy, 0), (d + 0.3, 0.5, d + 0.3), "C")                           # glass rings
    s.cyl((0, h / 2 + 0.4, 0), (d + 0.6, 1.0, d + 0.6), "D")                       # caps
    s.cyl((0, -h / 2 - 0.4, 0), (d + 0.6, 1.0, d + 0.6), "D")
    s.cyl((0, h / 2 + 1.2, 0), (1.2, 0.8, 1.2), "D")
    s.cyl((0, 0.8, 0), (d + 0.2, 2.2, d + 0.2), label)                            # label band
    for i in range(4):
        a = i * math.pi / 2
        s.box((math.cos(a) * d * 0.52, 0, math.sin(a) * d * 0.52), (0.6, h, 0.6), "D")   # cage bars


def ooze_canister():
    s = S(k=1.6)
    _canister(s)
    names = ["ooze canister", "mutagen canister", "canister of ooze", "ooze bomb", "mutagen bomb", "tgri canister",
             "mutagen grenade", "ooze grenade"]
    r = R("Mutagen Canister", "Bomb", ["Explodes", "Grow", "Sticky"], 0.6, 3.0, OOZE_GREEN, "ooze_canister",
          elem="Poison")
    return names, r, s, "A TCRI canister of glowing ooze, thrown: splashes, sticks, and what it hits Grows (mutates)."


def retro_mutagen():
    s = S(k=1.6)
    _canister(s, h=6.0, d=3.4, label="A")
    names = ["retro mutagen", "retro-mutagen", "retromutagen", "retro mutagen canister", "anti mutagen", "retro ooze"]
    r = R("Retro-Mutagen", "Bomb", ["Explodes", "Shrink"], 0.5, 2.0, (0.35, 0.7, 1.0), "retro_mutagen", elem="Water")
    return names, r, s, ("Retro-mutagen: reverses mutation - what it splashes Shrinks, and as a Water hit it drains a\n"
                          "held power (incl. mutagen) for a few seconds.")


def dimension_x_portal():
    s = S(k=1.2)
    R0 = 10.0
    for i in range(16):
        a = i / 16 * 2 * math.pi
        s.box((math.cos(a) * R0, 12.0 + math.sin(a) * R0, 0), (2.4, 4.2, 2.4), "A", (0, 0, math.degrees(a)))
        if i % 2 == 0:
            s.box((math.cos(a) * (R0 + 1.6), 12.0 + math.sin(a) * (R0 + 1.6), 0), (1.2, 1.2, 3.0), "C",
                  (0, 0, math.degrees(a)))
    for i in range(6):                                                            # swirl spokes (light only)
        a = i / 6 * 2 * math.pi + 0.3
        s.box((math.cos(a) * 4.5, 12.0 + math.sin(a) * 4.5, 0.2), (6.0, 0.5, 0.3), "C", (0, 0, math.degrees(a) + 60))
    s.box((0, 0.8, 0), (12.0, 1.6, 5.0), "D")                                     # base
    for sx in (-1, 1):
        s.box((sx * 5.0, 2.4, 0), (2.0, 3.4, 3.0), "D")
    names = ["dimension x portal", "portal to dimension x", "dimension x", "technodrome portal",
             "transdimensional portal", "dimension x gate", "kraang portal"]
    r = R("Dimension X Portal", "Wall", ["Portal"], 6.0, 500.0, KRANG_PINK, "dimension_x_portal", elem="Psychic")
    return names, r, s, "A portal to Dimension X: makes a linked Portal pair (momentum preserved)."


def _shell(s, w=12.0, l=14.0, h=5.0):
    s.ell((0, 0.6, 0), (w, h * 2, l), "D")                                        # carapace
    s.box((0, -h / 2 - 0.2, 0), (w + 2, h + 0.4, l + 2), "D", sub=True)          # flat bottom
    s.ell((0, 0.2, 0), (w + 1.0, 1.4, l + 1.0), "P")                              # rim
    s.cyl((0, -0.2, 0), (w - 1.0, 0.8, l - 1.0), "P")                             # plastron
    for (x, z) in ((0, 0), (-2.8, -3.2), (2.8, -3.2), (-2.8, 3.2), (2.8, 3.2)):
        s.cyl((x * w / 12, h * 0.78 if (x, z) == (0, 0) else h * 0.55, z * l / 14), (3.0, 0.6, 3.0), "P")   # scutes


def turtle_shell_slide():
    s = S(k=1.5)
    _shell(s)
    names = ["turtle shell slide", "shell slide", "sliding shell kick", "shell kick", "ninja turtle shell",
             "tmnt shell", "half shell", "shell sled"]
    r = R("Turtle Shell Slide", "Ball", ["Ricochet", "Bounce", "Spin", "Blunt"], 0.9, 25.0, (0.4, 0.8, 0.3),
          "turtle_shell_slide")
    return names, r, s, "A sliding ninja-turtle shell: spins, bounces and ricochets between enemies."


def turtle_power():
    s = S(k=1.4)
    s.ell((0, 0, 0), (28.0, 20.0, 28.0), "D")                                     # giant shell dome
    s.box((0, -6.0, 0), (30.0, 12.0, 30.0), "D", sub=True)
    s.cyl((0, 0.6, 0), (29.0, 1.2, 29.0), "P")                                    # rim
    for (x, z, y) in ((0, 0, 9.4), (-7.0, -5.0, 6.6), (7.0, -5.0, 6.6), (-7.0, 5.0, 6.6), (7.0, 5.0, 6.6),
                      (0, -10.0, 4.6), (0, 10.0, 4.6), (-11.0, 0, 4.0), (11.0, 0, 4.0)):
        s.cyl((x, y, z), (6.0, 0.8, 6.0), "A", ((-math.degrees(math.atan2(z, 14)) * 0.8), 0, math.degrees(math.atan2(x, 14)) * 0.8))
    s.box((0, 5.0, -13.4), (3.0, 3.0, 1.0), "C", sub=False)                       # doorway glow
    names = ["turtle power", "turtle power shell", "shell shield", "turtle shell dome", "tmnt turtle power"]
    r = R("Turtle Power", "Dome", ["Barrier", "Bounce", "Guard"], 3.5, 200.0, (0.4, 0.85, 0.3), "turtle_power")
    return names, r, s, "TURTLE POWER! A giant hard-light turtle shell dome around you: blocks and bounces."


def cowabunga():
    s = S(k=1.4)
    s.box((0, 3.0, 0), (6.0, 2.0, 20.0), "A", (-12, 0, 0))                        # surfboard as a ramp
    s.ell((0, 0.9, -10.0), (6.0, 2.0, 8.0), "A", (-12, 0, 0))                     # pointed nose
    s.ell((0, 5.1, 10.0), (6.0, 2.0, 4.0), "A", (-12, 0, 0))                      # rounded tail
    s.box((0, 4.0, 0), (1.0, 0.6, 24.0), "C", (-12, 0, 0))                        # stringer
    s.box((0, 2.0, 9.0), (4.0, 4.0, 3.0), "D")                                    # prop
    s.cone((0, 1.0, 11.6), (0.6, 2.4, 2.4), "D", (180, 0, 0))                     # fin
    for (x, z) in ((-1.4, -5.0), (1.4, -1.0), (-0.8, 3.0)):
        s.ell((x, 4.1 + z * 0.21, z), (1.6, 0.5, 1.6), "P")                         # stickers
    names = ["cowabunga", "cowabunga dude", "kowabunga", "cowabunga surfboard", "cowabunga dudes", "cowabunga ramp"]
    r = R("Cowabunga!", "Ramp", ["Launch", "Bounce"], 3.2, 20.0, MIKEY_ORANGE, "cowabunga")
    return names, r, s, "COWABUNGA! A surfboard ramp that launches whoever hits it, bouncily."


def manhole_cover():
    s = S(k=1.5)
    s.cyl((0, 0, 0), (12.0, 1.2, 12.0), "P")
    s.cyl((0, 0.7, 0), (10.0, 0.4, 10.0), "D")
    for i in range(-2, 3):
        s.box((i * 1.8, 0.95, 0), (0.5, 0.3, 9.0), "P")
        s.box((0, 0.95, i * 1.8), (9.0, 0.3, 0.5), "P")
    s.cyl((0, 1.1, 0), (3.0, 0.3, 3.0), "A")
    names = ["sewer manhole cover", "manhole cover shuriken", "turtle manhole cover", "sewer lid", "shellraiser manhole"]
    r = R("Manhole Cover", "Disc", ["Blunt", "Heavy"], 0.8, 40.0, (0.5, 0.5, 0.55), "sewer_manhole_cover")
    return names, r, s, "A sewer manhole cover thrown like a giant shuriken (Returning Disc, Heavy)."


def turtle_com():
    s = S(k=2.0)
    s.ell((0, 0, 0), (4.0, 1.4, 5.0), "D")                                        # shell-backed case
    s.box((0, -0.5, 0), (4.4, 1.2, 5.4), "D", sub=True)
    s.box((0, -0.3, 0), (3.6, 0.6, 4.4), "P")
    s.box((0, 0.1, -0.6), (2.6, 0.3, 2.0), "C")                                   # screen
    s.cyl((1.4, 0.4, -2.8), (0.3, 2.4, 0.3), "A", (-60, 0, 0))                    # antenna
    s.ell((1.4, 1.0, -3.8), (0.6, 0.6, 0.6), "C")
    names = ["turtle com", "turtlecom", "turtle comm", "t-phone", "shell cell", "turtle communicator"]
    r = R("Turtle Com", "Ball", ["Homing", "Fast"], 0.3, 0.5, (0.4, 0.8, 0.3), "turtle_com", elem="Electric")
    return names, r, s, "The Turtle Com / Shell Cell: thrown, it homes in and zaps (Electric)."


def sewer_lair():
    s = S(k=1.2)
    s.cz((0, 0, 0), 27.0, 22.0, "P")                                              # sewer pipe arch
    s.cz((0, 0, 0), 22.0, 23.0, "P", sub=True)
    s.box((0, -8.0, 0), (30.0, 16.0, 24.0), "P", sub=True)
    for dz in (-8.0, 0.0, 8.0):
        s.cz((0, 0, dz), 27.6, 1.2, "D")                                          # brick ribs
        s.cz((0, 0, dz), 22.0, 1.4, "D", sub=True)
        s.box((0, -8.0, dz), (30.0, 16.0, 2.0), "D", sub=True)
    s.box((0, 0.4, 0), (24.0, 0.8, 22.0), "D")                                    # floor
    s.box((0, 0.6, -2.0), (4.0, 0.3, 22.0), "A")                                  # sewer water channel
    s.box((-6.0, 2.0, 4.0), (6.0, 2.4, 3.0), "A")                                 # the couch
    s.box((-6.0, 3.6, 5.2), (6.0, 2.0, 0.8), "A")
    for i in range(4):
        s.box((6.0, 1.2 + i * 0.8, 3.0), (4.0, 0.7, 4.0), "P" if i % 2 else "C")  # pizza box tower
    names = ["turtle lair", "sewer lair", "the lair tmnt", "turtles' lair", "tmnt lair", "turtle sewer lair"]
    r = R("Turtle Lair", "Dome", ["Barrier", "Guard"], 4.0, 2000.0, (0.4, 0.7, 0.9), "turtle_lair")
    return names, r, s, "The turtles' sewer lair: a bricked sewer-arch shelter (couch and pizza boxes included)."


def build_all():
    return [technodrome, party_wagon, shellraiser, turtle_blimp, turtle_tank, foot_copter, news_van, mikey_pizza,
            pizza_cutter, pizza_thrower, ooze_canister, retro_mutagen, dimension_x_portal, turtle_shell_slide,
            turtle_power, cowabunga, manhole_cover, turtle_com, sewer_lair]
