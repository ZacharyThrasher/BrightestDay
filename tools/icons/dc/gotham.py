"""Gotham: the Bat-family, Deathstroke, the rogues' gallery, the Tumbler."""
from dlib import S, R, BAT_GREY, BAT_YELLOW, TIP_NZ, ALONG_Z, ALONG_X
from hero import hero, head, cape, brute, emblem_bat, tail


def _stick(s, hand, length=7.0, t="A", up=0.6):
    x, y, z = hand
    s.seg((x, y - 1.2, z + 0.8), (x, y + up, z - length), 0.9, 0.9, t)
    s.box((x, y - 0.1, z - 0.1), (1.3, 1.1, 1.1), "D")                                    # grip wrap


def nightwing():
    s = S()
    b = hero(s, suit="D", legs="D", arms="D", boots="D", gloves="D", arm_spread=10.0)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    # the blue wing across the chest and down the arms
    for sx in (-1, 1):
        s.box((sx * 2.6, ys - 1.6, -td / 2 - 0.3), (5.6, 1.1, 0.4), "A", (0, 0, sx * -18))
    s.box((0, ys - 2.4, -td / 2 - 0.3), (1.2, 1.6, 0.4), "A")
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        hx, hy, hz = b["hands"][sx]
        s.seg((sx * b["arm_x"], ys - 0.8, -b["arm_w"] / 2 - 0.2), (hx, hy + 1.6, hz - b["arm_w"] / 2 - 0.1), 0.6, 0.4, "A")
        _stick(s, b["hands"][sx], 7.5, "A", up=2.0)
    s.on(0)
    s.box((0, ys - 3.6, td / 2 + 0.4), (1.2, 7.0, 0.8), "D")                                # holster on the back
    head(s, b, hair="D", style="spiky", mask="D")
    names = ["nightwing", "dick grayson nightwing", "nightwing dc", "the nightwing", "nightwing character"]
    r = R("Nightwing", "Biped", ["Fast", "Blunt", "Bounce"], 1.8, 75.0, (0.15, 0.45, 1.0), elem="Electric",
          temp="Playful", inten=0.8, source="DC (Batman / Teen Titans)")
    return names, r, s, "Nightwing: black suit with the blue wing across the chest, twin escrima sticks. Acrobatic, Playful, shocking."


def escrima():
    s = S()
    for x in (-1.4, 1.4):
        s.cyl((x, 0, -5.0), (1.3, 14.0, 1.3), "P", ALONG_Z)                                 # batons
        s.cyl((x, 0, 1.0), (1.6, 3.0, 1.6), "D", ALONG_Z)                                   # grips
        s.cyl((x, 0, -12.2), (1.5, 1.2, 1.5), "C", ALONG_Z)                                 # electric tips
        for zz in (-4.0, -8.0):
            s.cyl((x, 0, zz), (1.5, 0.5, 1.5), "A", ALONG_Z)
    names = ["escrima sticks", "nightwing's escrima sticks", "nightwing escrima", "escrima batons",
             "nightwing's batons", "eskrima sticks"]
    r = R("Escrima Sticks", "Sword", ["Blunt", "Fast"], 0.8, 1.0, (0.15, 0.45, 1.0), elem="Electric", count=2,
          source="DC (Batman)")
    return names, r, s, "Nightwing's escrima sticks: dual electrified batons (count 2, Blunt + Fast, Electric micro-stun)."


def boy_wonder():
    s = S()
    b = hero(s, bulk=0.85, suit="A", legs="P", arms="P", boots="D", gloves="P", belt="C", buckle="C",
             trunks=None)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, ys - 2.4, -td / 2 - 0.25), (1.8, 1.8, 0.4), "C")                                # R emblem
    s.box((-0.3, ys - 2.4, -td / 2 - 0.45), (0.4, 1.4, 0.3), "A")
    s.box((0.2, ys - 2.0, -td / 2 - 0.45), (0.8, 0.4, 0.3), "A")
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.seg((hx, hy - 6.0, hz + 1.0), (hx, hy + 7.0, hz - 2.0), 0.9, 0.9, "D")                 # bo staff
    s.on(0)
    head(s, b, hair="D", style="spiky", mask="D")
    cape(s, b, t="C", flare=1.4, length=b["y_sh"] - 3.0, scallops=0)
    names = ["boy wonder", "robin boy wonder", "robin dc", "robin batman", "dick grayson robin", "tim drake",
             "damian wayne", "robin sidekick", "batman and robin robin"]
    r = R("Robin", "Biped", ["Fast", "Blunt", "Bounce"], 1.5, 55.0, (0.9, 0.1, 0.1), temp="Playful", inten=0.9,
          source="DC (Batman)")
    return names, r, s, "Robin, the Boy Wonder: red tunic with the R, green sleeves, yellow cape, bo staff. Bouncy and Playful."


def bane():
    s = S()
    b = brute(s, bulk=1.45, leg_h=8.4, torso=(10.4, 10.0, 6.4), arm_len=10.8, arm_spread=12)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, yh + 1.2, 0), (tw * 0.8, 2.0, td * 1.02), "D")                                  # belt
    s.box((0, ys - 2.0, td / 2 + 1.2), (3.6, 4.4, 2.4), "D")                                  # venom pump
    for sx in (-1, 1):
        s.seg((sx * 1.4, ys + 0.2, td / 2 + 1.8), (sx * 1.6, b["y_neck"] + 2.8, 2.4), 0.8, 0.8, "A")   # venom tubes
    s.on(5)
    hy = b["y_neck"] + 2.4
    s.ell((0, hy, -0.4), (4.4, 4.8, 4.6), "D")                                                # luchador mask
    for sx in (-1, 1):
        s.ell((sx * 1.0, hy + 0.4, -2.4), (1.4, 1.0, 0.5), "C")                               # white eye patches
        s.cyl((sx * 2.0, hy - 0.6, 0.6), (1.0, 1.0, 1.0), "A", ALONG_X)                        # tube sockets
    s.box((0, hy - 1.4, -2.5), (1.8, 0.8, 0.4), "P")                                           # mouth grille
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["bane", "bane batman", "the man who broke the bat", "bane dc", "bane venom"]
    r = R("Bane", "Biped", ["Grab", "Blunt", "Heavy", "Charge"], 2.6, 350.0, (0.3, 0.9, 0.3), temp="Aggressive",
          inten=0.9, source="DC (Batman)")
    return names, r, s, "Bane: luchador mask, Venom tubes to a back pump. Grabs and slams (Grab + Blunt + Heavy), charges."


def killer_croc():
    s = S()
    b = brute(s, bulk=1.4, leg_h=7.6, torso=(10.0, 10.0, 6.8), arm_len=11.0, arm_spread=14)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    for (x, y) in ((-2.2, ys - 1.4), (0, ys - 0.8), (2.2, ys - 1.4), (-1.2, ys - 4.0), (1.2, ys - 4.0), (0, ys - 6.6)):
        s.box((x, y, td / 2 + 0.8), (1.8, 1.4, 1.2), "D", (0, 45, 0))                          # back scutes
    s.box((0, yh + 0.6, 0), (tw * 0.84, 2.0, td * 1.04), "D")                                   # tattered shorts
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        hx, hy, hz = b["hands"][sx]
        for dx in (-1.0, 0.0, 1.0):
            s.cone((hx + dx, hy - 2.0, hz - 0.4), (0.6, 1.4, 0.6), "C", (180, 0, 0))            # claws
    s.on(5)
    hy = b["y_neck"] + 2.6
    s.ell((0, hy, 0), (4.6, 4.0, 5.0), "P")
    s.box((0, hy + 0.2, -4.0), (3.6, 1.8, 5.0), "P")                                             # upper snout
    for sx in (-1, 1):
        s.box((sx * 1.3, hy + 1.4, -1.4), (1.0, 0.8, 0.6), "C")                                  # yellow eyes
        for zz in (-2.4, -3.8, -5.2):
            s.cone((sx * 1.5, hy - 0.8, zz), (0.5, 1.0, 0.5), "C", (180, 0, 0))                  # teeth
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(6)
    s.box((0, hy - 1.4, -3.6), (3.2, 1.2, 4.6), "P")
    s.part(6, 5, (0, hy - 0.6, -0.8), "Jaw")
    s.on(0)
    tail(s, [(0, yh + 0.6, td / 2), (0, yh - 1.8, td / 2 + 4.0), (0.8, 1.2, td / 2 + 8.0), (2.0, 0.6, td / 2 + 11.0)],
         [(3.0, 2.6, 4.6), (2.4, 2.0, 4.6), (1.6, 1.4, 3.8)], t="P")
    names = ["killer croc", "waylon jones", "croc batman", "killer croc dc"]
    r = R("Killer Croc", "Biped", ["Grab", "ArticulatedTail", "Blunt", "Charge"], 2.7, 400.0, (0.4, 0.6, 0.25),
          elem="Water", temp="Aggressive", inten=1.0, source="DC (Batman)")
    return names, r, s, "Killer Croc: scaly sewer brute with a snapping snout and tail. Grabs, tail-sweeps, Aggressive."


def solomon_grundy():
    s = S()
    b = brute(s, bulk=1.5, leg_h=8.2, torso=(10.6, 10.6, 6.8), arm_len=11.6, arm_spread=8)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, yh + b["th"] * 0.45, -0.2), (tw + 0.4, b["th"] * 0.9, td + 0.6), "D")           # tattered suit jacket
    s.box((0, ys - 3.0, -td / 2 - 0.5), (2.4, 5.6, 0.4), "A")                                   # rag shirt
    for (x, y) in ((-2.6, yh + 1.0), (2.8, yh + 2.2), (-1.0, ys - 1.0)):
        s.box((x, y, -td / 2 - 0.6), (1.4, 1.4, 0.4), "P", (0, 0, 30))                           # rips
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        s.box((sx * b["hip_x"], 4.6, 0), (b["leg_w"] + 0.5, 8.0, b["leg_d"] + 0.5), "D")        # trousers
    s.on(5)
    hy = b["y_neck"] + 2.4
    s.ell((0, hy, -0.4), (4.8, 5.0, 4.8), "P")
    s.ell((0, hy + 1.6, 0.4), (5.2, 3.0, 5.0), "D")                                              # matted hair
    for sx in (-1, 1):
        s.box((sx * 1.1, hy + 0.2, -2.6), (1.0, 0.7, 0.4), "C")
    s.box((0, hy - 1.6, -2.4), (2.6, 0.6, 0.4), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["solomon grundy", "grundy", "born on a monday", "solomon grundy zombie"]
    r = R("Solomon Grundy", "Biped", ["Blunt", "Heavy", "Stomp"], 3.0, 700.0, (0.85, 0.88, 0.8), elem="Earth",
          temp="Lazy", inten=0.8, source="DC (Batman / JSA)")
    return names, r, s, "Solomon Grundy: chalk-white swamp zombie in a rotting suit. Lazy roadblock that stomps and smashes."


def joy_buzzer():
    s = S()
    s.box((0, 0, 0.6), (3.2, 3.0, 3.6), "P")                                                    # purple glove
    s.box((0, 0, 3.2), (3.4, 3.2, 2.0), "A")                                                    # cuff
    for i, x in enumerate((-1.2, -0.4, 0.4, 1.2)):
        s.box((x, 0.5, -2.2 - (0.3 if i in (1, 2) else 0)), (0.7, 0.8, 2.8), "P")              # fingers
    s.box((-2.0, 0.4, -0.4), (0.8, 0.8, 2.4), "P", (0, 30, 0))                                  # thumb
    s.cyl((0, -1.7, -0.2), (2.6, 0.6, 2.6), "C")                                                # the buzzer in the palm
    s.cyl((0, -2.0, -0.2), (1.2, 0.4, 1.2), "A")
    for (x, z) in ((-2.4, -1.0), (2.4, 0.4), (1.2, -2.8), (-1.6, 1.8)):
        s.box((x, -1.8, z), (1.4, 0.3, 0.3), "C", (0, 40, 0))                                   # zaps
    names = ["joy buzzer", "joker's joy buzzer", "hand buzzer", "joker joy buzzer", "shock buzzer"]
    r = R("Joy Buzzer", "Gauntlet", ["Grab", "Blunt"], 0.6, 0.5, (0.55, 0.2, 0.75), elem="Electric",
          source="DC (Batman / Joker)")
    return names, r, s, "The Joker's joy buzzer: shake hands... Gauntlet that seizes (Grab) with an Electric jolt."


def tumbler():
    s = S()
    s.box((0, 5.0, 1.0), (10.0, 4.0, 16.0), "P")                                                 # armoured hull
    s.box((0, 7.4, 2.4), (8.0, 1.6, 10.0), "P", (-8, 0, 0))                                      # roof plates
    s.box((0, 4.6, -9.0), (6.0, 2.6, 5.0), "D", (14, 0, 0))                                      # nose ramp
    s.box((0, 6.2, -4.6), (5.4, 1.4, 4.0), "C", (-18, 0, 0))                                     # windscreen slit
    for sx in (-1, 1):
        s.cyl((sx * 6.8, 3.6, 7.0), (7.2, 3.4, 7.2), "D", ALONG_X)                               # huge rear tyres
        s.cyl((sx * 7.0, 3.6, 7.0), (3.4, 3.6, 3.4), "A", ALONG_X)
        s.cyl((sx * 4.8, 2.0, -9.6), (4.0, 2.0, 4.0), "D", ALONG_X)                              # front wheels
        s.box((sx * 4.6, 4.6, -8.4), (1.0, 1.0, 5.0), "D")                                       # exposed axles
        s.cz((sx * 1.6, 3.8, -11.0), 0.9, 3.0, "C")                                               # twin cannons
    s.cz((0, 5.2, 9.8), 3.0, 1.6, "C")                                                            # jet exhaust
    s.cz((0, 5.2, 11.0), 2.0, 1.2, "A")
    s.seat(0, (0, 5.4, -3.0))
    names = ["tumbler", "the tumbler", "tumbler batmobile", "dark knight batmobile", "batman begins batmobile",
             "nolan batmobile", "bat tank"]
    r = R("Tumbler", "Vehicle", ["Piloted", "Charge", "Shoots", "Heavy"], 5.0, 3500.0, BAT_GREY,
          feat=["Wheels", "Barrels", "Jet", "Armor"], source="DC (The Dark Knight trilogy)")
    return names, r, s, ("The Tumbler: Nolan's tank-Batmobile - giant rear tyres, jet exhaust, twin nose cannons.\n"
                         "Drive it (Piloted), ram (Charge), fire (Shoots). Rams launch people through walls.")


def riddler_cane():
    s = S()
    s.cyl((0, 0, -2.0), (1.0, 16.0, 1.0), "D", ALONG_Z)                                          # cane shaft (grip at origin)
    s.cyl((0, 0, 6.2), (1.2, 0.6, 1.2), "A", ALONG_Z)
    # the question mark head, in the XY plane at the far (-Z) end? no: at the TOP of a cane (+Y) -> use -Z as up
    qz = -10.0
    pts = [(0, 0, qz), (0, 0, qz - 1.6), (0, 1.4, qz - 3.0), (0, 3.4, qz - 3.2), (0, 5.0, qz - 2.2),
           (0, 5.2, qz - 0.4), (0, 4.0, qz + 0.8)]
    for a, c in zip(pts, pts[1:]):
        s.seg(a, c, 1.4, 1.4, "A")
    s.ell((0, -1.8, qz + 1.4), (1.8, 1.8, 1.8), "C")                                              # the dot
    names = ["riddler's cane", "the riddler", "riddler", "edward nygma", "question mark cane", "riddler cane",
             "riddle me this"]
    r = R("Riddler's Cane", "Staff", ["Blunt", "Shoots"], 1.2, 1.5, (0.2, 0.8, 0.2), elem="Psychic",
          payload=R("riddle trap", "Cage", ["Trap"], 1.6, 5.0), source="DC (Batman)")
    return names, r, s, "The Riddler's question-mark cane: whacks, and fires riddle traps (a Cage payload that snaps shut)."


def deathstroke():
    s = S()
    b = hero(s, bulk=1.12, suit="D", legs="D", arms="A", boots="D", gloves="D", belt="A", buckle="C", shoulder="A")
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, ys - 3.2, -td / 2 - 0.3), (tw * 0.8, 5.0, 0.5), "D")                               # chest armour
    for x in (-1.8, 1.8):
        s.box((x, yh + 3.4, -td / 2 - 0.35), (1.0, 3.6, 0.4), "A")                               # abdomen stripes
    s.box((1.4, ys - 1.0, td / 2 + 0.6), (0.9, 12.0, 0.9), "D", (0, 0, 28))                      # katana on the back
    s.box((-2.6, ys + 3.4, td / 2 + 0.6), (1.2, 3.2, 1.2), "A", (0, 0, 28))
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.box((hx, hy, hz + 0.4), (0.8, 0.8, 2.0), "D")
    s.box((hx, hy, hz - 0.7), (1.8, 1.8, 0.4), "A")
    s.seg((hx, hy + 0.2, hz - 0.9), (hx, hy + 2.6, hz - 10.0), 0.4, 1.1, "C")                   # sword
    s.on(5)
    hy2 = b["y_neck"] + 2.2
    s.ell((0, hy2, 0), (3.8, 4.4, 4.0), "D")
    s.box((-0.95, hy2, -0.2), (1.9, 4.4, 4.2), "A")                                              # orange half
    s.box((0.9, hy2 + 0.4, -1.9), (1.1, 0.5, 0.4), "C")                                          # the one eye
    s.box((-0.9, hy2 + 0.4, -2.2), (1.2, 0.5, 0.3), "D")                                         # the patched side
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["deathstroke", "slade wilson", "deathstroke the terminator", "slade", "deathstroke mask"]
    r = R("Deathstroke", "Biped", ["Blade", "Fast", "Shoots"], 1.9, 100.0, (1.0, 0.5, 0.1), temp="Sneaky", inten=0.9,
          source="DC (Teen Titans / Batman)")
    return names, r, s, "Deathstroke: the half-orange, one-eyed mask, sword drawn, katana on the back. A Sneaky, deadly flanker."


def build_all():
    return [nightwing, escrima, boy_wonder, bane, killer_croc, solomon_grundy, joy_buzzer, tumbler, riddler_cane,
            deathstroke] + PROC


def P(names, r, note):
    return lambda: (names, r, None, note)


PROC = [
    P(["batgirl", "barbara gordon", "batgirl dc", "cassandra cain"],
      R("Batgirl", "Biped", ["Fast", "Blunt", "Glide"], 1.7, 60.0, BAT_YELLOW, temp="Sneaky", inten=0.7,
        source="DC (Batman)"), "Batgirl: cowl, cape-glide, fast strikes."),
    P(["red hood", "jason todd", "red hood's pistols", "red hood pistols"],
      R("Red Hood's Pistols", "Gun", ["Shoots", "Fast"], 0.5, 1.5, (0.8, 0.1, 0.1), count=2, source="DC (Batman)"),
      "Red Hood's twin pistols: dual-wield hand cannons."),
    P(["catwoman", "selina kyle", "catwoman's whip", "catwoman whip"],
      R("Catwoman's Whip", "Whip", ["Pull", "Fast"], 2.2, 1.0, (0.25, 0.25, 0.3), source="DC (Batman)"),
      "Catwoman's bullwhip: yank people off their feet (Pull)."),
    P(["scarecrow", "jonathan crane", "fear toxin", "scarecrow's fear gas", "fear gas"],
      R("Fear Toxin", "Bomb", ["Explodes"], 0.5, 1.0, (0.6, 0.55, 0.3), elem="Shadow", count=2, source="DC (Batman)"),
      "Scarecrow's fear toxin canisters: Shadow gas bursts (vision vignette)."),
    P(["two-face", "harvey dent", "two-face's coin", "two face coin"],
      R("Two-Face's Coin", "Disc", ["Ricochet"], 0.4, 0.2, (0.9, 0.9, 0.9), temp="Chaotic", inten=0.5,
        source="DC (Batman)"), "Two-Face's scarred coin: flip it - it ricochets somewhere."),
    P(["clayface", "basil karlo", "clayface batman"],
      R("Clayface", "Blob", ["Sticky", "Grab", "Heavy"], 3.0, 800.0, (0.6, 0.45, 0.3), elem="Earth", temp="Aggressive",
        inten=0.7, source="DC (Batman)"), "Clayface: a sticky, grabbing mound of living clay."),
    P(["man-bat", "man bat", "kirk langstrom"],
      R("Man-Bat", "Bird", ["Flies", "Grab"], 2.2, 90.0, (0.4, 0.3, 0.25), temp="Aggressive", inten=0.8,
        source="DC (Batman)"), "Man-Bat: a giant bat-man that swoops and seizes."),
    P(["oswald cobblepot", "the penguin batman", "penguin villain", "penguin's rocket umbrella"],
      R("The Penguin", "Biped", ["Shoots", "SlowFall"], 1.5, 90.0, (0.2, 0.2, 0.25), temp="Fearful", inten=0.7,
        payload=R("penguin rocket", "Rocket", ["Homing", "Explodes"], 0.6, 5.0), source="DC (Batman)"),
      "The Penguin: keeps his distance and fires homing penguin rockets from his umbrella."),
    P(["batpod", "the batpod", "bat-pod", "batcycle", "bat cycle", "batman motorcycle"],
      R("Batpod", "Vehicle", ["Piloted", "Fast", "Shoots"], 3.0, 400.0, BAT_GREY, feat=["Wheels", "Barrels"],
        source="DC (The Dark Knight)"), "The Batpod: ride it, shoot it (Piloted + Shoots)."),
    P(["smoke pellet", "batman smoke bomb", "bat smoke pellet", "smoke pellets"],
      R("Smoke Pellet", "Bomb", ["Explodes", "Fast"], 0.2, 0.1, (0.4, 0.4, 0.45), elem="Shadow", count=3,
        source="DC (Batman)"), "Smoke pellets: three tiny Shadow bursts for the vanishing act (vision only)."),
    P(["batman's utility belt", "bat utility belt", "batman belt"],
      R("Batman's Utility Belt", "Bomb", ["Explodes", "Sticky"], 0.3, 0.3, BAT_YELLOW, count=3, source="DC (Batman)"),
      "Out of the utility belt: three sticky gadget charges."),
    P(["batcopter", "bat copter", "bat-copter"],
      R("Batcopter", "Vehicle", ["Flies", "Piloted"], 6.0, 2000.0, BAT_GREY, feat=["Rotor", "Cockpit"],
        source="DC (Batman '66)"), "The '66 Batcopter: a pilotable helicopter."),
    P(["mad hatter", "jervis tetch", "mad hatter's hat"],
      R("Mad Hatter", "Biped", ["Shoots"], 1.5, 55.0, (0.3, 0.5, 0.3), elem="Psychic", temp="Chaotic", inten=0.7,
        source="DC (Batman)"), "Mad Hatter: dazing (Psychic) - never mind control."),
    P(["firefly batman", "garfield lynns", "firefly flamethrower"],
      R("Firefly", "Biped", ["Flies", "Breath"], 1.8, 90.0, (0.95, 0.5, 0.1), elem="Fire", temp="Aggressive", inten=0.8,
        feat=["Jet"], source="DC (Batman)"), "Firefly: jetpack arsonist with a flamethrower."),
    P(["killer moth", "drury walker", "killer moth cocoon gun"],
      R("Killer Moth", "Gun", ["Sticky", "Trap"], 0.8, 2.0, (0.6, 0.4, 0.8), source="DC (Batman)"),
      "Killer Moth's cocoon gun: sticky trapping shots."),
    P(["hush", "tommy elliot"],
      R("Hush", "Biped", ["Shoots", "Fast"], 1.8, 80.0, (0.5, 0.5, 0.55), temp="Sneaky", inten=0.8, source="DC (Batman)"),
      "Hush: bandaged sniper, Sneaky."),
    P(["court of owls", "talon", "talons of the court of owls"],
      R("Talons", "Biped", ["Blade", "Fast", "Swarm"], 1.8, 70.0, (0.7, 0.6, 0.4), count=3, temp="Sneaky", inten=0.9,
        source="DC (Batman)"), "The Court of Owls' Talons: three undead blade-assassins."),
    P(["ra's al ghul", "ras al ghul", "the demon's head", "league of assassins"],
      R("League of Assassins", "Biped", ["Blade", "Fast", "Swarm"], 1.8, 70.0, (0.2, 0.5, 0.2), count=4,
        temp="Sneaky", inten=0.8, source="DC (Batman)"), "Ra's al Ghul's League of Assassins: a squad of swordsmen."),
    P(["lady shiva", "sandra wu-san"],
      R("Lady Shiva", "Biped", ["Fast", "Blunt", "Blade"], 1.7, 60.0, (0.2, 0.2, 0.25), temp="Aggressive", inten=0.9,
        source="DC (Batman)"), "Lady Shiva: the deadliest martial artist."),
    P(["the question", "vic sage", "renee montoya"],
      R("The Question", "Biped", ["Fast", "Blunt"], 1.8, 75.0, (0.2, 0.5, 0.6), temp="Sneaky", inten=0.7,
        source="DC (The Question)"), "The Question: faceless detective."),
    P(["huntress", "helena bertinelli", "huntress crossbow"],
      R("Huntress' Crossbow", "Bow", ["Pierce", "Fast"], 0.8, 2.0, (0.5, 0.2, 0.7), source="DC (Batman)"),
      "Huntress's crossbow: fast piercing bolts."),
    P(["azrael", "jean-paul valley", "sword of sin"],
      R("Azrael", "Biped", ["Blade", "Breath"], 1.9, 90.0, (0.9, 0.25, 0.1), elem="Fire", temp="Aggressive", inten=0.9,
        source="DC (Batman)"), "Azrael: flaming-sword avenging angel."),
]
