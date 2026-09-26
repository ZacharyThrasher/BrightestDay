"""Justice League members, Wonder Woman, Shazam family, Blue Beetle, Lex, Aquaman & Flash rogues."""
from dlib import S, R, SUPER_RED, WW_GOLD, TIP_NZ, ALONG_Z
from hero import hero, head, cape, wings, emblem_bolt

WW_RED = (0.85, 0.1, 0.15)


# ------------------------------------------------------------------------------- Wonder Woman

def diana():
    s = S()
    b = hero(s, fem=True, suit="P", boots="A", boot_h=4.4, gloves="C", glove_h=2.6, arm_spread=8.0)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, yh + b["th"] * 0.62, 0), (tw + 0.3, b["th"] * 0.62, td + 0.3), "A")          # red bustier
    for sx in (-1, 1):                                                                     # golden eagle / W
        s.box((sx * 1.0, ys - 1.4, -td / 2 - 0.35), (2.2, 0.6, 0.4), "C", (0, 0, sx * -28))
        s.box((sx * 0.5, ys - 2.2, -td / 2 - 0.35), (0.5, 1.6, 0.4), "C", (0, 0, sx * 18))
    s.box((0, yh + 0.9, 0), (tw * 0.9, 0.7, td * 1.02), "C")                                # belt
    s.box((0, yh - 0.4, 0), (tw * 0.94, 2.4, td * 1.08), "P")                               # blue skirt
    for (x, y) in ((-1.4, -0.5), (0.2, -0.1), (1.6, -0.7)):
        s.box((x, yh + y, -td * 0.54 - 0.1), (0.5, 0.5, 0.3), "C", (0, 0, 45))              # stars
    s.on(3)                                                                                 # lasso coil at the hip
    hx, hy, hz = b["hands"][-1]
    s.cyl((hx - 0.2, hy + 1.2, hz + 0.4), (2.8, 0.5, 2.8), "C", (0, 0, 80))
    s.box((hx - 0.4, hy + 5.0, hz - 1.4), (3.8, 4.6, 0.6), "A")                            # shield on the forearm
    s.box((hx - 0.4, hy + 5.0, hz - 1.8), (1.2, 3.0, 0.4), "C")
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.box((hx, hy, hz + 0.5), (0.7, 0.7, 2.0), "D")                                         # sword grip
    s.box((hx, hy, hz - 0.6), (2.4, 0.6, 0.6), "C")                                          # crossguard
    s.seg((hx, hy + 0.2, hz - 0.9), (hx, hy + 1.2, hz - 9.0), 0.4, 1.1, "C")                # blade
    s.on(0)
    head(s, b, hair="D", style="long", w=3.3, h=4.0, d=3.6)
    s.on(5)
    s.box((0, b["hy"] + 1.1, -1.7), (3.4, 0.6, 0.6), "C")                                   # tiara
    s.box((0, b["hy"] + 1.4, -2.0), (0.8, 0.8, 0.4), "A", (0, 0, 45))                       # red star
    s.on(0)
    names = ["diana prince", "princess diana", "diana of themyscira", "wonder woman character", "ww84 wonder woman",
             "amazon princess", "the amazing amazon"]
    r = R("Wonder Woman", "Biped", ["Guard", "Pull", "Blade", "Fast"], 1.9, 80.0, WW_RED, temp="Protective",
          inten=0.8, source="DC (Wonder Woman)")
    return names, r, s, ("Wonder Woman: red bustier with the gold W-eagle, starred blue skirt, tiara, silver bracelets,\n"
                         "shield + sword, lasso coiled at the hip. Parries (Guard), lassos (Pull), cuts, Protective.")


def tiara():
    s = S()
    for sx in (-1, 1):
        s.box((sx * 2.4, 0, -0.9), (4.4, 1.6, 1.4), "C", (0, sx * 22, 0))                  # the band, V toward -Z
    s.box((0, 0.3, -2.0), (2.2, 2.2, 0.8), "A", (0, 0, 45))                                 # the star
    s.box((0, 0.3, -2.3), (1.0, 1.0, 0.5), "C", (0, 0, 45))
    names = ["wonder woman's tiara", "wonder woman tiara", "diana's tiara", "amazon tiara", "throwing tiara"]
    r = R("Wonder Woman's Tiara", "Disc", ["Returning", "Ricochet", "Blade"], 0.75, 0.4, WW_RED,
          source="DC (Wonder Woman)")
    return names, r, s, "The tiara is a throwing weapon: a returning, ricocheting bladed disc."


def godkiller():
    s = S()
    s.box((0, 0, 1.4), (1.1, 1.1, 4.4), "D")                                                # grip
    s.ell((0, 0, 3.9), (1.6, 1.6, 1.4), "A")                                                # pommel
    s.box((0, 0, -0.9), (5.0, 1.2, 1.0), "A")                                                # crossguard
    for sx in (-1, 1):
        s.cone((sx * 2.9, 0.4, -0.9), (0.9, 1.6, 0.9), "A", (0, 0, -sx * 70))
    s.box((0, 0, -11.0), (2.2, 0.5, 19.0), "P")                                               # broad blade
    s.box((0, -0.05, -11.0), (0.5, 0.55, 18.0), "C")                                          # fuller glow
    s.cone((0, 0, -21.4), (2.2, 2.4, 0.5), "P", TIP_NZ)
    names = ["godkiller", "the godkiller", "godkiller sword", "wonder woman's sword", "wonder woman sword",
             "sword of athena", "god killer"]
    r = R("Godkiller", "Sword", ["Blade", "Heavy", "Pierce"], 1.3, 5.0, WW_RED, source="DC (Wonder Woman)")
    return names, r, s, "The Godkiller: Diana's great sword - Blade + Pierce + Heavy."


def invisible_jet():
    s = S()
    s.box((0, 2.2, -1.0), (3.6, 2.6, 22.0), "P")                                             # fuselage
    s.cone((0, 2.2, -14.5), (3.4, 5.0, 2.6), "P", TIP_NZ)                                    # nose
    s.ell((0, 3.8, -6.0), (2.6, 2.0, 6.0), "C")                                               # canopy (you see her!)
    s.box((0, 3.2, -6.0), (1.0, 1.2, 1.2), "A")                                               # pilot
    for sx in (-1, 1):
        s.box((sx * 6.0, 1.8, 2.0), (10.0, 0.6, 7.0), "P", (0, sx * -28, 0))                # swept wings
        s.box((sx * 10.6, 1.8, 5.0), (1.0, 0.9, 3.0), "A")                                    # wingtip
        s.box((sx * 2.6, 5.0, 9.0), (0.6, 5.0, 3.6), "P", (0, 0, sx * 18))                   # twin tails
        s.cz((sx * 1.2, 1.8, 10.8), 1.8, 2.0, "C")                                            # engines
    s.seat(0, (0, 3.2, -6.0))
    names = ["invisible jet", "wonder woman's invisible jet", "the invisible jet", "invisible plane",
             "wonder woman's jet", "invisible airplane"]
    r = R("Invisible Jet", "Vehicle", ["Flies", "Piloted", "Fast"], 7.0, 3000.0, (0.85, 0.95, 1.0),
          feat=["Jet", "Cockpit"], source="DC (Wonder Woman)")
    return names, r, s, ("Wonder Woman's Invisible Jet - built of hard light it's perfectly visible (no invisibility,\n"
                         "ever). A pilotable jet: fly it, eject for a launch.")


# ------------------------------------------------------------------------------ League members

def martian_manhunter():
    s = S()
    b = hero(s, bulk=1.12, tall=1.0, suit="P", boots="A", belt="A", buckle="C", arm_spread=6.0)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    for sx in (-1, 1):                                                                        # red X straps
        s.box((0, (yh + ys) / 2 + 0.8, -td / 2 - 0.3), (0.9, b["th"] * 1.05, 0.4), "A", (0, 0, sx * 32))
    s.on(5)
    hy = b["y_neck"] + 2.4
    s.ell((0, hy, 0), (3.8, 4.8, 4.2), "P")                                                  # tall bald head
    s.box((0, hy + 1.0, -1.5), (3.8, 1.2, 1.4), "P")                                          # heavy brow
    for sx in (-1, 1):
        s.box((sx * 0.9, hy + 0.2, -2.05), (1.1, 0.6, 0.4), "A")                             # red eyes
    s.box((0, hy - 1.6, -1.2), (2.4, 1.6, 1.8), "P")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    cape(s, b, t="D", flare=2.2, collar="D", length=b["y_sh"] - 0.6)
    names = ["martian manhunter", "j'onn j'onzz", "jonn jonzz", "manhunter from mars", "the manhunter from mars",
             "john jones martian"]
    r = R("Martian Manhunter", "Biped", ["Flies", "Grow", "Blunt"], 2.3, 130.0, (0.85, 0.12, 0.15), elem="Psychic",
          temp="Protective", inten=0.7, source="DC (Justice League)")
    return names, r, s, ("J'onn J'onzz: green, bald, red X-straps, high-collared blue cape. Flies; his blows make\n"
                         "things Grow (density shift) and daze (Psychic). No phasing/invisibility here.")


def hawkman():
    s = S()
    b = hero(s, bulk=1.1, suit="P", boots="A", belt="A", buckle="C", legs="A")
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    for sx in (-1, 1):                                                                        # harness X
        s.box((0, (yh + ys) / 2 + 1.0, -td / 2 - 0.3), (0.8, b["th"] * 1.0, 0.4), "A", (0, 0, sx * 30))
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.seg((hx, hy - 0.6, hz + 0.8), (hx, hy + 2.2, hz - 3.4), 0.8, 0.8, "D")                 # mace haft
    s.ell((hx, hy + 2.8, hz - 4.4), (3.2, 3.2, 3.2), "A")                                    # mace head
    for (dx, dy, dz, r) in ((1.7, 0, 0, (0, 0, -90)), (-1.7, 0, 0, (0, 0, 90)), (0, 1.7, 0, (0, 0, 0)),
                            (0, 0, -1.7, (-90, 0, 0)), (0, -1.7, 0, (180, 0, 0))):
        s.cone((hx + dx, hy + 2.8 + dy, hz - 4.4 + dz), (0.9, 1.5, 0.9), "C", r)
    s.on(5)
    hy2 = b["y_neck"] + 2.2
    s.ell((0, hy2, 0), (3.8, 4.4, 4.0), "P")
    s.ell((0, hy2 + 0.6, 0.2), (4.4, 3.8, 4.6), "A")                                          # hawk helmet
    s.cone((0, hy2 + 0.2, -3.0), (1.6, 2.6, 1.4), "A", (-100, 0, 0))                         # beak
    for sx in (-1, 1):
        s.box((sx * 0.9, hy2 + 0.4, -2.1), (1.0, 0.5, 0.4), "C")
        s.box((sx * 2.4, hy2 + 1.8, 0.8), (0.6, 2.6, 2.0), "A", (30, 0, sx * -20))           # helmet wings
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    wings(s, b, t="P", feather_t="A", span=16.0, h=11.0, feathers=5, angle=20)
    names = ["hawkman", "carter hall", "katar hol", "hawkman character"]
    r = R("Hawkman", "Biped", ["Flies", "Blunt", "Heavy"], 2.0, 110.0, (0.95, 0.72, 0.15), temp="Aggressive", inten=0.8,
          feat=["Spikes"], source="DC (Hawkman)")
    return names, r, s, "Hawkman: hawk helmet with beak, great feathered Nth-metal wings, spiked mace. Dive-bombs (Flies, Blunt)."


def black_canary():
    s = S()
    b = hero(s, fem=True, suit="D", legs="D", boots="D", gloves="D", arm_spread=6.0)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, yh + b["th"] * 0.55, 0), (tw + 0.4, b["th"] * 0.95, td + 0.4), "D")         # leather jacket
    s.box((0, ys - 0.6, -td / 2 - 0.3), (2.2, 1.8, 0.4), "P")                                # open collar
    for sx in (-1, 1):
        s.on(1 if sx < 0 else 2)
        for yy in (3.0, 5.0, 7.0):
            s.box((sx * b["hip_x"], yy, -b["leg_d"] / 2 - 0.2), (b["leg_w"] + 0.2, 0.3, 0.3), "P", (0, 0, 45))  # fishnets
    s.on(0)
    head(s, b, hair="A", style="long", w=3.3, h=4.0, d=3.6, mask="D")
    s.on(5)
    s.ell((0, b["hy"] - 1.2, -1.9), (1.2, 1.0, 0.5), "D")                                    # mouth open - the cry
    for i in range(3):
        d = 2.0 + i * 1.6
        s.cyl((0, b["hy"] - 1.0, -3.2 - i * 1.6), (d, 0.4, d), "C", (90, 0, 0))
        s.cyl((0, b["hy"] - 1.0, -3.2 - i * 1.6), (d - 0.9, 0.6, d - 0.9), "C", (90, 0, 0), sub=True)
    s.on(0)
    names = ["black canary", "dinah lance", "dinah drake", "black canary character", "birds of prey canary"]
    r = R("Black Canary", "Biped", ["Repel", "Breath", "Fast", "Blunt"], 1.8, 70.0, (1.0, 0.85, 0.3), elem="Wind",
          temp="Aggressive", inten=0.7, source="DC (Black Canary)")
    return names, r, s, "Black Canary: leather jacket, fishnets, blonde hair; the Canary Cry is a Wind cone that shoves (Repel)."


def canary_cry():
    s = S()
    for i in range(6):
        d = 2.0 + i * 2.2
        z = -1.0 - i * 2.8
        s.cyl((0, 0, z), (d, 0.8, d), "C" if i % 2 == 0 else "A", ALONG_Z)
        s.cyl((0, 0, z), (max(0.6, d - 1.4), 1.2, max(0.6, d - 1.4)), "C", ALONG_Z, sub=True)
    s.ell((0, 0, 1.2), (2.4, 1.6, 1.4), "D")
    names = ["canary cry", "the canary cry", "black canary's cry", "canary scream", "sonic scream", "sonic cry"]
    r = R("Canary Cry", "Staff", ["Repel", "Breath"], 1.6, 0.5, (1.0, 0.85, 0.3), elem="Wind", source="DC (Black Canary)")
    return names, r, s, "The Canary Cry: widening sonic rings - a Wind Breath cone that blasts people away (Repel)."


def victor_stone():
    s = S()
    b = hero(s, bulk=1.12, suit="A", legs="A", arms="A", boots="D", gloves="D")
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    for (x, y, w, h) in ((-1.6, ys - 2.0, 2.6, 2.8), (1.6, ys - 2.0, 2.6, 2.8), (0, yh + 3.2, 3.4, 3.0)):
        s.box((x, y, -td / 2 - 0.3), (w, h, 0.5), "P")                                        # armour plates
    s.cyl((0, ys - 2.6, -td / 2 - 0.5), (1.4, 0.5, 1.4), "C", (90, 0, 0))                     # chest core
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.cyl((hx, hy + 1.2, hz - 1.0), (3.0, 4.2, 3.0), "P", (-90, 0, 0))                        # sonic cannon arm
    s.cyl((hx, hy + 1.2, hz - 3.2), (2.0, 0.8, 2.0), "C", (-90, 0, 0))
    s.on(3)
    hx, hy, hz = b["hands"][-1]
    s.box((hx, hy + 4.0, hz), (b["arm_w"] + 0.5, 3.0, b["arm_w"] + 0.5), "P")
    s.on(0)
    head(s, b, skin="P", style="bald", w=3.6, h=4.2)
    s.on(5)
    s.box((0.9, b["hy"] + 0.3, -0.4), (2.2, 4.4, 3.8), "A")                                   # half metal face
    s.box((0.8, b["hy"] + 0.3, -2.1), (0.9, 0.7, 0.4), "C")                                   # the red eye
    s.on(0)
    names = ["victor stone", "cyborg justice league", "cyborg teen titans", "vic stone", "cyborg dc", "booyah"]
    r = R("Cyborg", "Biped", ["Shoots", "Beam", "Guard", "Heavy"], 2.0, 180.0, (0.72, 0.74, 0.8), elem="Electric",
          temp="Protective", inten=0.7, feat=["Armor"], source="DC (Justice League / Teen Titans)")
    return names, r, s, "Cyborg: half-machine Victor Stone - red eye, arm sonic cannon (Shoots + Beam), armour (Guard). Booyah!"


def black_adam():
    s = S()
    b = hero(s, bulk=1.15, suit="D", boots="A", gloves="A", belt="A", buckle="C", glove_h=3.0)
    emblem_bolt(s, (0, b["y_hip"] + b["th"] * 0.68, -b["td"] / 2 - 0.6), t="A", k=1.3)
    head(s, b, hair="D", style="short", brows="D")
    cape(s, b, t="D", flare=1.0, length=6.0, collar="A")
    names = ["black adam", "teth-adam", "teth adam", "black adam shazam"]
    r = R("Black Adam", "Biped", ["Flies", "Blunt", "Heavy", "Charge"], 2.1, 130.0, WW_GOLD, elem="Electric",
          temp="Aggressive", inten=0.9, source="DC (Shazam)")
    return names, r, s, "Black Adam: black suit, gold lightning bolt, belt and bracers. A lightning-charged Aggressive brawler."


def billy_batson():
    s = S()
    b = hero(s, bulk=1.12, suit="A", boots="C", belt="C", buckle="C", gloves="C", glove_h=2.4)
    emblem_bolt(s, (0, b["y_hip"] + b["th"] * 0.66, -b["td"] / 2 - 0.6), t="C", k=1.5)
    head(s, b, hair="D", style="short")
    cape(s, b, t="C", flare=1.4, length=7.0, collar="C")
    names = ["billy batson", "captain marvel shazam", "the big red cheese", "earth's mightiest mortal",
             "shazam character", "captain thunder"]
    r = R("Shazam", "Biped", ["Flies", "Blunt", "Fast"], 2.0, 110.0, (0.9, 0.12, 0.1), elem="Electric", temp="Playful",
          inten=0.8, source="DC (Shazam)")
    return names, r, s, "Shazam (Billy Batson): red suit, gold bolt, white half-cape. Flies, fast lightning punches, Playful."


def magic_lightning():
    s = S()
    pts = [(0, 40, 0), (3, 32, 1), (-2, 25, -1), (2.5, 17, 0.5), (-1.5, 9, 0), (0.5, 0, 0)]
    for a, c in zip(pts, pts[1:]):
        s.seg(a, c, 3.4, 3.4, "C")
        s.seg(a, c, 5.4, 1.6, "A")
    s.cyl((0, 0.4, 0), (8.0, 0.8, 8.0), "C")                                                  # ground flash
    s.ell((0, 42, 0), (7.0, 3.0, 7.0), "D")                                                   # storm cloud
    names = ["magic lightning", "shazam lightning", "shazam's lightning", "shazam bolt", "the magic lightning",
             "rock of eternity lightning", "magic lightning bolt"]
    r = R("Magic Lightning", "Pillar", ["DropFromSky", "Explodes", "Beam"], 10.0, 50.0, WW_GOLD, elem="Electric",
          source="DC (Shazam)")
    return names, r, s, "The magic lightning that answers 'SHAZAM': a bolt from the sky onto the aim point."


def blue_beetle():
    s = S()
    b = hero(s, bulk=1.15, suit="P", legs="D", arms="D", boots="P", gloves="P", glove_h=3.2, arm_spread=8.0)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, ys - 3.0, -td / 2 - 0.3), (tw * 0.9, 5.0, 0.6), "P")                             # chest carapace
    s.box((0, ys - 3.0, -td / 2 - 0.65), (0.5, 4.6, 0.3), "C")
    for sx in (-1, 1):                                                                         # scarab wing-cases
        s.ell((sx * 2.2, ys - 2.6, td / 2 + 1.4), (4.2, 9.0, 2.2), "A", (0, 0, sx * 12))
    s.ell((0, ys - 0.6, td / 2 + 1.2), (3.6, 3.0, 2.4), "C")                                   # the scarab
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.cone((hx, hy + 0.8, hz - 2.4), (2.6, 4.2, 2.6), "P", TIP_NZ)                            # arm blaster
    s.on(5)
    hy2 = b["y_neck"] + 2.2
    s.ell((0, hy2, 0), (3.8, 4.4, 4.2), "P")
    for sx in (-1, 1):
        s.ell((sx * 1.0, hy2 + 0.4, -1.8), (1.6, 1.2, 0.8), "C", (0, 0, sx * 20))             # big yellow eyes
        s.cone((sx * 1.4, hy2 + 2.6, 0.8), (0.8, 2.6, 0.8), "A", (-20, 0, -sx * 15))         # head fins
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.seat(0, (0, b["y_hip"] + 3.0, 0))
    s.on(0)
    names = ["blue beetle", "jaime reyes", "khaji da", "khaji-da", "scarab suit", "blue beetle suit", "the scarab"]
    r = R("Blue Beetle", "Biped", ["Piloted", "Shoots", "Guard", "Thrust"], 2.2, 160.0, (0.15, 0.35, 0.95),
          feat=["Armor"], source="DC (Blue Beetle)")
    return names, r, s, ("The Blue Beetle scarab suit: wear it (Piloted, human-scale = a power suit): arm blasters\n"
                         "(Shoots), carapace block (Guard), wing-case thrust.")


def lex_warsuit():
    s = S()
    b = hero(s, bulk=1.45, tall=0.6, suit="P", legs="P", arms="P", boots="A", gloves="A", glove_h=4.0,
             shoulder="A", arm_spread=10.0)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, ys - 2.8, -td / 2 - 0.4), (tw * 0.8, 5.4, 0.8), "A")                              # breastplate
    s.cyl((0, ys - 2.6, -td / 2 - 0.9), (2.0, 0.5, 2.0), "C", (90, 0, 0))                       # kryptonite core
    for sx in (-1, 1):
        s.cz((sx * 2.4, ys + 0.2, td / 2 + 1.2), 1.8, 3.0, "D")                                  # back thrusters
        s.cz((sx * 2.4, ys + 0.2, td / 2 + 2.9), 1.2, 0.6, "C")
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.cz((hx, hy + 2.6, hz - 2.4), 1.2, 3.4, "C")                                                # wrist cannon
    s.on(5)
    hy2 = b["y_neck"] + 2.2
    s.ell((0, hy2, 0), (4.6, 4.6, 4.6), "A")                                                    # helmet
    s.box((0, hy2 + 0.3, -2.2), (3.4, 1.0, 0.5), "C")                                           # visor
    s.box((0, hy2 - 1.4, -2.0), (2.6, 1.0, 0.6), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.seat(0, (0, b["y_hip"] + 3.0, 0))
    s.on(0)
    names = ["lex luthor's warsuit", "lex luthor warsuit", "luthor warsuit", "lexcorp warsuit", "lex luthor power armor",
             "lex luthor battle suit", "lex luthor", "lex luthor armor"]
    r = R("Lex Luthor's Warsuit", "Biped", ["Piloted", "Beam", "Guard", "Heavy", "Thrust"], 2.5, 400.0,
          (0.25, 0.8, 0.3), feat=["Armor", "Jet"], source="DC (Superman)")
    return names, r, s, ("Lex Luthor's green-and-purple warsuit: a Piloted power suit (<= 2.6 m) - kryptonite-core Beam,\n"
                         "armour Guard, back thrusters (flight capped like every suit).")


def daily_planet_globe():
    s = S()
    s.ell((0, 8.0, 0), (12.0, 12.0, 12.0), "P")                                                  # the globe
    for (x, y, z, w, h) in ((-2.0, 10.0, -5.4, 4.0, 3.4), (2.6, 6.0, -5.2, 3.0, 4.4), (-3.4, 5.6, -4.6, 2.4, 2.4)):
        s.box((x, y, z), (w, h, 1.4), "A")                                                       # continents
    s.cyl((0, 8.0, 0), (15.0, 1.0, 15.0), "C", (0, 0, 23))                                       # the ring
    s.cyl((0, 8.0, 0), (13.4, 1.4, 13.4), "C", (0, 0, 23), sub=True)
    s.cyl((0, 1.0, 0), (4.0, 2.0, 4.0), "D")                                                     # stand
    names = ["daily planet globe", "the daily planet globe", "daily planet", "the daily planet"]
    r = R("Daily Planet Globe", "Ball", ["DropFromSky", "Heavy", "Bounce"], 3.5, 2500.0, (0.9, 0.75, 0.2),
          source="DC (Superman)")
    return names, r, s, "The Daily Planet's rooftop globe, ringed: dropped from the sky onto the aim point, then bounces."


def black_manta():
    s = S()
    b = hero(s, bulk=1.1, suit="D", legs="D", arms="D", boots="P", gloves="P", belt="A", buckle="C")
    s.on(5)
    hy = b["y_neck"] + 3.0
    s.ell((0, hy, 0.2), (6.0, 6.0, 6.2), "P")                                                    # huge helmet
    for sx in (-1, 1):
        s.ell((sx * 1.6, hy + 0.4, -2.6), (2.6, 2.6, 1.4), "A")                                  # red bug eyes
        s.ell((sx * 1.6, hy + 0.4, -3.0), (1.2, 1.2, 0.6), "C")
    s.box((0, hy - 2.0, -2.4), (2.4, 1.2, 1.0), "D")                                             # respirator
    s.cyl((0, hy + 3.1, 0), (1.0, 1.2, 1.0), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    s.box((0, ys - 3.0, -td / 2 - 0.3), (5.4, 4.2, 0.5), "P")
    s.box((0, ys - 3.4, td / 2 + 1.0), (3.2, 5.0, 2.0), "P")                                    # air tank
    names = ["black manta", "david kane", "black manta helmet"]
    r = R("Black Manta", "Biped", ["Beam", "Fast", "Blade"], 1.9, 100.0, (0.9, 0.1, 0.1), elem="Water",
          temp="Sneaky", inten=0.8, source="DC (Aquaman)")
    return names, r, s, "Black Manta: the giant bug-eyed helmet fires optic Beams; a Sneaky Water assassin."


def cold_gun():
    s = S()
    s.box((0, -1.4, 0.6), (1.2, 3.0, 1.4), "D", (15, 0, 0))                                     # grip
    s.box((0, 0.6, -2.0), (2.4, 2.4, 7.0), "P")                                                  # body
    s.box((0, 2.2, 1.0), (1.6, 1.2, 3.0), "A")                                                   # canister
    s.cyl((0, 2.2, -2.2), (1.8, 3.0, 1.8), "C", ALONG_Z)                                         # coolant tank
    for i in range(3):
        s.cyl((0, 0.6, -6.0 - i * 1.4), (2.8 - i * 0.5, 0.6, 2.8 - i * 0.5), "A", ALONG_Z)       # cooling fins
    s.cz((0, 0.6, -9.4), 1.2, 2.0, "C")
    names = ["cold gun", "captain cold's cold gun", "captain cold gun", "captain cold", "leonard snart", "snart's cold gun"]
    r = R("Cold Gun", "Gun", ["Beam", "Pierce"], 0.9, 4.0, (0.6, 0.85, 1.0), elem="Ice", source="DC (The Flash)")
    return names, r, s, "Captain Cold's cold gun: a hitscan Ice beam (Beam + Pierce railgun archetype) that chills."


def build_all():
    return [diana, tiara, godkiller, invisible_jet, martian_manhunter, hawkman, black_canary, canary_cry, victor_stone,
            black_adam, billy_batson, magic_lightning, blue_beetle, lex_warsuit, daily_planet_globe, black_manta,
            cold_gun]
