"""Image Comics & friends: Invincible, Spawn, The Boys (Compound V), The Tick."""
from dlib import (S, R, INV_YELLOW, INV_BLUE, EVE_PINK, VILTRUM_RED, TIP_NZ, ALONG_Z, ALONG_X)
from hero import hero, head, brute, cape, humanoid, tail


def invincible():
    s = S()
    b = hero(s, suit="A", legs="P", arms="P", boots="P", gloves="P", glove_h=3.0, arm_spread=6)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    for sx in (-1, 1):
        s.box((sx * tw * 0.42, (yh + ys) / 2 + 0.6, 0), (1.3, b["th"] * 0.8, td + 0.3), "P")   # blue side panels
    s.box((0, yh + 0.6, 0), (tw * 0.72, 1.6, td * 1.02), "P")                                  # blue shorts line
    s.on(5)
    hy = b["y_neck"] + 2.0
    s.ell((0, hy, 0), (3.7, 4.2, 4.0), "P")                                                     # blue cowl
    s.box((0, hy - 1.4, -1.0), (2.6, 1.4, 2.4), "A")                                            # bare jaw
    for sx in (-1, 1):
        s.ell((sx * 0.85, hy + 0.3, -1.85), (1.3, 0.9, 0.5), "C")                              # white goggles
        s.ell((sx * 0.85, hy + 0.3, -2.05), (0.6, 0.5, 0.3), "D")
    s.ell((0, hy + 1.4, 0.4), (3.9, 1.8, 3.8), "D")                                             # dark hair tuft
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["invincible mark grayson", "invincible hero", "invincible character", "invincible image comics",
             "mark grayson invincible", "invincible suit", "think mark"]
    r = R("Invincible", "Biped", ["Flies", "Blunt", "Charge", "Fast"], 1.8, 90.0, INV_YELLOW, temp="Aggressive",
          inten=0.8, source="Invincible (Image)")
    return names, r, s, ("Invincible (Mark Grayson): yellow top, blue sides and cowl, white goggles. Half-Viltrumite: flying\n"
                         "charges and punches that send people through buildings. (bare 'invincible' is left to the adjective.)")


def atom_eve():
    s = S()
    b = hero(s, fem=True, suit="A", legs="A", arms="A", boots="C", gloves="C", arm_spread=16)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, (yh + ys) / 2, -td / 2 - 0.25), (tw * 0.6, b["th"] * 0.8, 0.4), "C")            # white front panel
    s.ell((0, ys - 2.4, -td / 2 - 0.5), (2.2, 2.2, 0.4), "A")                                   # atom symbol
    for rz in (0, 60, 120):
        s.ell((0, ys - 2.4, -td / 2 - 0.6), (3.2, 1.0, 0.3), "C", (0, 0, rz))
    for pid, sx in ((3, -1), (4, 1)):                                                           # matter-shaping glow
        s.on(pid)
        hx, hy, hz = b["hands"][sx]
        s.box((hx, hy, hz - 0.6), (2.6, 2.6, 2.6), "C", (45, 45, 0))
    s.on(0)
    head(s, b, hair="D", style="long", w=3.3, h=4.0, d=3.6, mask="A")
    names = ["atom eve", "samantha eve wilkins", "eve wilkins", "atom eve invincible"]
    r = R("Atom Eve", "Biped", ["Flies", "Barrier", "Shoots", "Grow"], 1.8, 60.0, EVE_PINK, temp="Protective",
          inten=0.8, source="Invincible (Image)")
    return names, r, s, ("Atom Eve: pink matter-shaper - her constructs are hard light just like yours. Flies, throws up\n"
                         "barriers, fires pink blasts, and rearranges matter (Grow).")


def allen():
    s = S()
    b = brute(s, bulk=1.3, leg_h=8.4, torso=(10.0, 10.0, 6.0), arm_len=10.4, arm_spread=10)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, (yh + ys) / 2, -td / 2 - 0.3), (tw * 0.5, b["th"] * 0.8, 0.4), "C")             # white uniform stripe
    s.box((0, yh + 0.6, 0), (tw * 0.8, 1.6, td * 1.02), "D")
    s.on(5)
    hy = b["y_neck"] + 2.2
    s.ell((0, hy, -0.2), (5.0, 4.6, 4.6), "P")
    s.ell((0, hy + 0.4, -2.2), (3.2, 2.8, 1.2), "C")                                            # the one big eye
    s.ell((0, hy + 0.4, -2.8), (1.4, 1.4, 0.6), "D")
    s.box((0, hy - 1.6, -2.1), (2.6, 0.5, 0.4), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["allen the alien", "allen invincible", "the unopan champion", "unopan"]
    r = R("Allen the Alien", "Biped", ["Blunt", "Heavy", "Charge"], 2.4, 400.0, (0.3, 0.8, 0.3), temp="Protective",
          inten=0.8, source="Invincible (Image)")
    return names, r, s, "Allen the Alien: one-eyed Unopan champion (and very buff). Protective heavy hitter."


def battle_beast():
    s = S()
    b = brute(s, bulk=1.45, leg_h=8.8, torso=(10.4, 10.4, 6.4), arm_len=11.0, arm_spread=12)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, ys - 3.0, -td / 2 - 0.4), (tw * 0.8, 5.0, 0.6), "A")                             # armour
    s.box((0, yh + 0.8, 0), (tw * 0.84, 2.0, td * 1.04), "D")
    s.on(4)
    hx, hy, hz = b["hands"][1]
    s.seg((hx, hy - 3.0, hz + 1.2), (hx, hy + 5.0, hz - 2.4), 1.0, 1.0, "D")                    # battle-axe haft
    s.box((hx, hy + 5.0, hz - 4.4), (1.0, 6.0, 4.6), "C")                                       # axe blade
    s.box((hx, hy + 5.0, hz - 1.4), (1.2, 3.0, 1.8), "A")
    s.on(3)
    hx, hy, hz = b["hands"][-1]
    s.seg((hx, hy - 1.0, hz), (hx, hy + 4.0, hz - 3.0), 0.9, 0.9, "D")                          # mace
    s.ell((hx, hy + 4.6, hz - 3.6), (2.6, 2.6, 2.6), "A")
    s.on(5)
    hy2 = b["y_neck"] + 2.6
    s.ell((0, hy2 + 0.4, 1.0), (8.0, 7.6, 6.4), "A")                                            # the lion mane
    s.ell((0, hy2, -0.6), (4.6, 4.6, 4.6), "P")
    s.box((0, hy2 - 1.0, -2.8), (2.8, 2.2, 2.0), "P")                                            # muzzle
    s.box((0, hy2 - 0.4, -3.9), (1.2, 0.8, 0.4), "D")
    for sx in (-1, 1):
        s.box((sx * 1.2, hy2 + 0.8, -2.4), (1.0, 0.6, 0.4), "C")
        s.cone((sx * 1.0, hy2 - 2.0, -3.4), (0.5, 1.2, 0.5), "C", (180, 0, 0))                  # fangs
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["battle beast", "thokk", "battle beast invincible", "the battle beast"]
    r = R("Battle Beast", "Biped", ["Blade", "Heavy", "Charge", "Blunt"], 2.8, 500.0, (0.95, 0.7, 0.2),
          temp="Aggressive", inten=1.0, source="Invincible (Image)")
    return names, r, s, "Battle Beast (Thokk): lion-maned warrior with axe and mace. Aggressive 1.0: seeks a worthy fight."


def thragg():
    s = S()
    b = hero(s, bulk=1.25, suit="P", legs="P", arms="D", boots="D", gloves="D", belt="A", buckle="C", arm_spread=8)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, ys - 2.8, -td / 2 - 0.35), (tw * 0.6, 4.4, 0.5), "C")                             # white Viltrumite tunic
    s.box((0, ys - 2.8, -td / 2 - 0.65), (1.2, 3.4, 0.3), "A")
    head(s, b, hair="D", style="short", brows="D", w=4.0, h=4.4)
    s.on(5)
    s.box((0, b["hy"] - 1.6, -1.2), (3.2, 1.6, 1.6), "D")                                       # beard
    s.box((1.0, b["hy"] + 0.2, -2.0), (0.4, 3.0, 0.3), "C", (0, 0, 20))                          # the scar
    s.on(0)
    cape(s, b, t="A", flare=2.0, length=b["y_sh"] - 1.0, collar="A")
    names = ["thragg", "grand regent thragg", "viltrumite regent", "thragg invincible"]
    r = R("Thragg", "Biped", ["Flies", "Blunt", "Heavy", "Charge"], 2.2, 160.0, VILTRUM_RED, temp="Aggressive",
          inten=1.0, source="Invincible (Image)")
    return names, r, s, "Grand Regent Thragg: scarred, bearded Viltrumite with the red cape. The strongest: flying rams."


def mauler_twins():
    s = S()
    b = brute(s, bulk=1.35, leg_h=7.8, torso=(10.0, 9.6, 6.2), arm_len=10.4, arm_spread=12)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, yh + 0.8, 0), (tw * 0.84, 2.4, td * 1.04), "D")                                   # trunks
    for sx in (-1, 1):
        s.box((sx * 1.6, (yh + ys) / 2 + 1.0, -td / 2 - 0.3), (0.8, b["th"] * 0.9, 0.4), "D", (0, 0, sx * 12))   # straps
    s.on(5)
    hy = b["y_neck"] + 2.8
    s.ell((0, hy, 0), (4.6, 5.6, 4.6), "P")                                                     # tall domed head
    s.ell((0, hy + 1.4, 0.4), (4.8, 3.0, 4.6), "P")
    s.box((0, hy - 1.8, -1.6), (3.6, 1.8, 2.0), "P")
    for sx in (-1, 1):
        s.box((sx * 1.0, hy - 0.2, -2.3), (1.0, 0.6, 0.4), "C")
    s.box((0, hy - 2.2, -2.7), (2.4, 0.4, 0.3), "D")                                            # grimace
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["mauler twins", "the mauler twins", "maulers", "the maulers", "mauler twin"]
    r = R("Mauler Twins", "Biped", ["Blunt", "Heavy", "Swarm", "Charge"], 2.6, 400.0, (0.25, 0.45, 0.9), count=2,
          temp="Aggressive", inten=0.8, source="Invincible (Image)")
    return names, r, s, "The Mauler Twins: two big blue bickering brutes (count 2). Which one's the clone?"


def al_simmons():
    s = S()
    b = hero(s, bulk=1.1, suit="D", legs="D", arms="D", boots="D", gloves="D", glove_h=3.4, arm_spread=8)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, (yh + ys) / 2, -td / 2 - 0.25), (tw * 0.5, b["th"] * 0.8, 0.4), "C")             # white chest field
    s.ell((0, ys - 3.0, -td / 2 - 0.55), (2.6, 3.0, 0.5), "A")                                 # skull emblem
    for sx in (-1, 1):
        s.box((sx * 0.6, ys - 2.6, -td / 2 - 0.8), (0.8, 0.8, 0.3), "D")
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        hx, hy, hz = b["hands"][sx]
        for dy in (2.4, 4.0, 5.6):
            s.cone((hx + sx * 1.6, hy + dy, hz), (0.7, 1.8, 0.7), "C", (0, 0, -sx * 95))          # glove spikes
        s.seg((hx, hy - 0.4, hz), (hx + sx * 2.0, hy - 5.0, hz - 1.0), 0.6, 0.6, "A")             # hanging chain
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        s.cone((sx * b["hip_x"], 4.4, -1.8), (0.9, 1.8, 0.9), "C", (-80, 0, 0))                  # boot spikes
    s.on(0)
    s.box((0, yh + 1.4, 0), (tw * 0.8, 1.2, td * 1.04), "A")                                   # chain belt
    s.on(5)
    hy = b["y_neck"] + 2.2
    s.ell((0, hy, 0), (3.8, 4.4, 4.0), "D")                                                     # mask
    for sx in (-1, 1):
        s.box((sx * 0.9, hy + 0.3, -1.95), (1.2, 0.8, 0.4), "C", (0, 0, sx * 15))              # big green eyes
    s.box((0, hy + 1.4, -1.9), (0.6, 2.0, 0.4), "C")                                            # the white stripe
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    cape(s, b, t="A", flare=5.0, width=b["tw"] * 1.3, length=b["y_sh"] + 1.0, collar="A", scallops=6)
    names = ["al simmons", "hellspawn", "spawn image comics", "spawn character", "spawn todd mcfarlane", "spawn (image)"]
    r = R("Spawn", "Biped", ["Pull", "Glide", "Guard", "Blade"], 2.0, 100.0, (0.85, 0.1, 0.1), elem="Shadow",
          temp="Sneaky", inten=0.9, source="Spawn (Image)")
    return names, r, s, ("Spawn (Al Simmons): huge ragged red cape with a spiked collar, glowing green eyes, skull emblem,\n"
                         "living chains. Chains Pull, cape glides and guards. Sneaky Shadow.")


def spawn_chains():
    s = S()
    s.box((0, 0, 1.0), (1.6, 1.6, 3.0), "D")                                                    # gripped end
    for i in range(12):
        z = -1.0 - i * 1.4
        y = -0.03 * i * i
        s.box((0, y, z), (1.0 if i % 2 else 0.5, 0.5 if i % 2 else 1.0, 1.8), "A")              # alternating links
    s.cone((0, -4.6, -18.0), (2.2, 3.0, 1.0), "C", (-120, 0, 0))                                # the hook blade
    s.box((0, -3.8, -17.4), (1.2, 1.2, 1.2), "A")
    names = ["spawn's chains", "spawn chains", "hellspawn chains", "living chains", "necroplasmic chains",
             "chains of spawn"]
    r = R("Spawn's Chains", "Whip", ["Link", "Pull", "Blade"], 3.0, 4.0, (0.85, 0.1, 0.1), elem="Shadow",
          source="Spawn (Image)")
    return names, r, s, "Spawn's living chains: a hooked chain-whip that yanks (Pull) and chains people down (Link)."


def compound_v():
    s = S()
    s.cyl((0, 0, -3.0), (3.0, 10.0, 3.0), "C", ALONG_Z)                                         # syringe barrel
    s.cyl((0, 0, -3.8), (2.4, 7.6, 2.4), "A", ALONG_Z)                                          # the blue V fluid
    s.cyl((0, 0, 2.6), (4.2, 0.8, 4.2), "D", ALONG_Z)                                           # finger flange
    s.cyl((0, 0, 4.4), (1.0, 3.2, 1.0), "D", ALONG_Z)                                            # plunger
    s.cyl((0, 0, 6.2), (2.6, 0.6, 2.6), "D", ALONG_Z)
    s.cone((0, 0, -8.8), (1.8, 1.6, 1.8), "D", TIP_NZ)
    s.cyl((0, 0, -11.0), (0.4, 3.6, 0.4), "C", ALONG_Z)                                          # needle
    for sx in (-1, 1):                                                                           # the V label
        s.box((sx * 0.6, 1.55, -3.4), (0.5, 0.2, 2.6), "D", (0, sx * 20, 0))
    names = ["compound v", "compound-v", "compound v syringe", "vought compound v", "temp v", "v24"]
    r = R("Compound V", "Spear", ["Pierce"], 0.5, 0.3, (0.2, 0.55, 1.0), grants="random:origin",
          source="The Boys")
    return names, r, s, ("Compound V: inject it and you get a random superhero origin (grants \"random:origin\" - the\n"
                         "origin-kit pool: kryptonian, speed force, the Lantern rings, viltrumite...).")


def the_tick():
    s = S()
    b = hero(s, bulk=1.35, tall=0.8, suit="P", legs="P", arms="P", boots="P", gloves="P", arm_spread=14)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    for dy in (-1.4, 0.8):
        s.box((0, ys - 3.0 + dy, -td / 2 - 0.3), (tw * 0.7, 0.5, 0.4), "D")                    # ab lines
    s.box((0, ys - 4.0, -td / 2 - 0.3), (0.5, 4.0, 0.4), "D")
    s.on(5)
    hy = b["y_neck"] + 2.2
    s.ell((0, hy, 0), (4.0, 4.6, 4.2), "P")
    for sx in (-1, 1):
        s.box((sx * 0.8, hy + 0.4, -2.0), (1.2, 0.6, 0.4), "C")
        s.seg((sx * 1.0, hy + 2.0, -0.4), (sx * 2.6, hy + 6.4, -1.6), 0.6, 0.6, "P")             # antennae
        s.ell((sx * 2.8, hy + 6.8, -1.7), (1.0, 1.0, 1.0), "P")
    s.box((0, hy - 1.4, -2.0), (2.0, 0.5, 0.4), "C")                                             # the grin
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["the tick", "tick the hero", "the tick hero", "spoon hero"]
    r = R("The Tick", "Biped", ["Blunt", "Heavy", "Bounce", "Charge"], 2.2, 180.0, (0.2, 0.35, 0.95), temp="Playful",
          inten=1.0, source="The Tick")
    return names, r, s, "The Tick: big blue nigh-invulnerable lug with antennae. Bouncy, blunt and joyfully Playful. SPOON!"


def soldier_boy():
    s = S()
    b = hero(s, bulk=1.12, suit="D", legs="D", arms="D", boots="D", gloves="D", belt="A", buckle="C")
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    s.box((0, ys - 3.0, -td / 2 - 0.35), (tw * 0.8, 5.0, 0.6), "P")                             # tactical vest
    s.box((0, ys - 2.4, -td / 2 - 0.75), (2.0, 2.0, 0.4), "C", (0, 0, 45))                      # star
    s.on(3)
    hx, hy, hz = b["hands"][-1]
    s.cyl((hx - 0.6, hy + 4.0, hz - 1.2), (6.2, 0.8, 6.2), "A", (90, 0, 0))                    # round shield
    s.cyl((hx - 0.6, hy + 4.0, hz - 1.6), (2.2, 0.6, 2.2), "C", (90, 0, 0))
    s.on(5)
    hy2 = b["y_neck"] + 2.2
    s.ell((0, hy2, 0), (3.8, 4.4, 4.0), "P")
    s.ell((0, hy2 + 0.8, 0.2), (4.4, 3.0, 4.6), "D")                                             # helmet
    s.box((0, hy2 + 0.2, -2.0), (3.6, 0.8, 0.5), "D")                                            # goggles band
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["soldier boy", "soldier boy the boys", "soldier boy shield"]
    r = R("Soldier Boy", "Biped", ["Guard", "Explodes", "Blunt"], 1.9, 100.0, (0.9, 0.15, 0.15), elem="Atomic",
          temp="Aggressive", inten=0.9, source="The Boys")
    return names, r, s, "Soldier Boy: shield up, then the chest blast - an Atomic explosion (Guard + Explodes)."


def P(names, r, note):
    return lambda: (names, r, None, note)


PROC = [
    P(["rex splode", "rex sloan", "rex splode invincible"],
      R("Rex Splode", "Biped", ["Shoots", "Fast"], 1.8, 70.0, (0.9, 0.5, 0.1), temp="Aggressive", inten=0.8,
        payload=R("charged object", "Bomb", ["Explodes", "Fast"], 0.4, 1.0, elem="Fire"), source="Invincible (Image)"),
      "Rex Splode: charges any object and throws it - it explodes."),
    P(["monster girl", "amanda invincible", "monster girl invincible"],
      R("Monster Girl", "Biped", ["Grow", "Blunt", "Heavy", "Charge"], 3.0, 500.0, (0.3, 0.7, 0.3), temp="Aggressive",
        inten=0.9, source="Invincible (Image)"), "Monster Girl: transforms into a hulking monster (Grow)."),
    P(["rudy connors", "robot invincible", "robot guardians of the globe"],
      R("Robot", "Biped", ["Shoots", "Guard", "Flies"], 2.0, 200.0, (0.6, 0.6, 0.65), temp="Protective", inten=0.7,
        feat=["Antenna"], source="Invincible (Image)"), "Robot: the Guardians' strategist in a flying droid body."),
    P(["conquest", "conquest viltrumite", "conquest invincible"],
      R("Conquest", "Biped", ["Flies", "Blunt", "Heavy", "Charge"], 2.2, 160.0, VILTRUM_RED, temp="Aggressive", inten=1.0,
        source="Invincible (Image)"), "Conquest: the one-eyed Viltrumite brute."),
    P(["flaxans", "the flaxans", "flaxan army", "flaxan portal"],
      R("Flaxans", "Biped", ["Swarm", "Portal", "Shoots"], 1.2, 40.0, (0.4, 0.6, 0.9), count=6, temp="Aggressive",
        inten=0.8, source="Invincible (Image)"), "The Flaxans: tiny invaders pouring out of a portal (Swarm + Portal)."),
    P(["the immortal", "immortal invincible", "abraham lincoln immortal"],
      R("The Immortal", "Biped", ["Flies", "Blunt", "Fast"], 2.0, 110.0, (0.85, 0.2, 0.2), temp="Protective", inten=0.8,
        source="Invincible (Image)"), "The Immortal: axe-armed ancient Guardian."),
    P(["dupli-kate", "dupli kate", "kate cha"],
      R("Dupli-Kate", "Biped", ["Swarm", "Fast"], 1.7, 60.0, (0.9, 0.5, 0.1), count=4, temp="Playful", inten=0.7,
        source="Invincible (Image)"), "Dupli-Kate: four of her at once."),
    P(["shrinking rae", "rae invincible"],
      R("Shrinking Rae", "Biped", ["Shrink", "Fast"], 0.6, 5.0, (0.9, 0.3, 0.6), temp="Sneaky", inten=0.8,
        source="Invincible (Image)"), "Shrinking Rae: tiny - and so are you after she hits you."),
    P(["black samson", "black samson invincible"],
      R("Black Samson", "Biped", ["Blunt", "Guard"], 2.0, 120.0, (0.8, 0.7, 0.2), temp="Protective", inten=0.7,
        source="Invincible (Image)"), "Black Samson: armoured Guardian."),
    P(["angstrom levy", "angstrom levy portals"],
      R("Angstrom Levy", "Biped", ["Portal", "Shoots"], 1.9, 80.0, (0.4, 0.3, 0.7), temp="Sneaky", inten=0.8,
        source="Invincible (Image)"), "Angstrom Levy: dimension-hopping portals."),
    P(["anissa", "anissa viltrumite"],
      R("Anissa", "Biped", ["Flies", "Blunt", "Charge"], 1.9, 110.0, VILTRUM_RED, temp="Aggressive", inten=0.9,
        source="Invincible (Image)"), "Anissa: Viltrumite enforcer."),
    P(["oliver grayson", "kid omni-man", "kid omni man"],
      R("Kid Omni-Man", "Biped", ["Flies", "Fast", "Blunt"], 1.3, 40.0, VILTRUM_RED, temp="Aggressive", inten=0.7,
        source="Invincible (Image)"), "Oliver Grayson, Kid Omni-Man."),
    P(["the reanimen", "reanimen", "d.a. sinclair's reanimen"],
      R("Reanimen", "Biped", ["Swarm", "Blunt", "Walks"], 2.0, 150.0, (0.5, 0.5, 0.45), count=4, temp="Aggressive",
        inten=0.7, source="Invincible (Image)"), "D.A. Sinclair's Reanimen cyborg zombies."),
    P(["guardians of the globe", "the guardians of the globe"],
      R("Guardians of the Globe", "Biped", ["Flies", "Shoots", "Swarm"], 1.9, 90.0, INV_BLUE, count=5,
        temp="Protective", inten=0.8, source="Invincible (Image)"), "The Guardians of the Globe: a squad of five."),
    P(["violator", "the violator", "the clown spawn", "violator demon"],
      R("Violator", "Biped", ["Blade", "Charge", "ArticulatedTail"], 3.0, 400.0, (0.4, 0.6, 0.3), elem="Poison",
        temp="Chaotic", inten=1.0, source="Spawn (Image)"), "The Violator: the clown who's really a demon."),
    P(["malebolgia", "malebolgia hell lord"],
      R("Malebolgia", "Kaiju", ["Breath", "Stomp"], 16.0, 90000.0, (0.7, 0.2, 0.1), elem="Fire", temp="Aggressive",
        inten=1.0, source="Spawn (Image)"), "Malebolgia: ruler of the Eighth Circle."),
    P(["spawn's cape", "necroplasm cape", "necroplasmic cape", "spawn cape"],
      R("Spawn's Cape", "Cape", ["Glide", "Guard"], 2.0, 3.0, (0.85, 0.1, 0.1), elem="Shadow", source="Spawn (Image)"),
      "Spawn's living cape: glide, a mid-air dash, and it blocks."),
    P(["angela spawn", "angela's spear"],
      R("Angela's Spear", "Spear", ["Pierce", "Fast"], 2.2, 3.0, (0.9, 0.8, 0.3), source="Spawn (Image)"),
      "Angela the angel-hunter's spear."),
    P(["cy-gor", "cygor"],
      R("Cy-Gor", "Biped", ["Blunt", "Heavy", "Shoots"], 2.8, 400.0, (0.5, 0.5, 0.5), temp="Aggressive", inten=0.9,
        source="Spawn (Image)"), "Cy-Gor: cybernetic gorilla."),
    P(["a-train", "a train the boys", "a-train speedster"],
      R("A-Train", "Biped", ["Fast", "Charge"], 1.9, 80.0, (0.2, 0.35, 0.9), elem="Wind", temp="Aggressive", inten=0.9,
        source="The Boys"), "A-Train: the fastest (Fast + Charge)."),
    P(["black noir", "noir the boys", "black noir the boys"],
      R("Black Noir", "Biped", ["Blade", "Fast"], 1.9, 90.0, (0.15, 0.15, 0.18), temp="Sneaky", inten=1.0,
        source="The Boys"), "Black Noir: silent, masked, always behind you (Sneaky - never invisible)."),
    P(["queen maeve", "maggie shaw"],
      R("Queen Maeve", "Biped", ["Blunt", "Guard", "Charge"], 1.9, 90.0, (0.8, 0.7, 0.3), temp="Protective", inten=0.7,
        source="The Boys"), "Queen Maeve: sword-and-shield brawler."),
    P(["annie january", "starlight the boys", "starlight supe"],
      R("Starlight", "Biped", ["Beam", "Explodes"], 1.7, 60.0, (1.0, 0.95, 0.7), temp="Protective", inten=0.7,
        source="The Boys"), "Starlight: light-absorbing, light-blasting."),
    P(["the deep", "the deep the boys", "kevin moskowitz"],
      R("The Deep", "Biped", ["Shoots"], 1.9, 80.0, (0.2, 0.5, 0.4), elem="Water", temp="Fearful", inten=0.8,
        payload=R("angry octopus", "Fish", ["Grab", "Constrict"], 1.0, 10.0, elem="Water"), source="The Boys"),
      "The Deep: stays back and flings sea creatures (an octopus payload)."),
    P(["stormfront", "klara risinger"],
      R("Stormfront", "Biped", ["Flies", "Beam"], 1.8, 70.0, (0.9, 0.2, 0.2), elem="Electric", temp="Aggressive",
        inten=0.9, source="The Boys"), "Stormfront: flying electric blaster."),
    P(["lamplighter", "lamplighter's flame"],
      R("Lamplighter", "Staff", ["Breath"], 1.2, 2.0, (1.0, 0.6, 0.1), elem="Fire", source="The Boys"),
      "Lamplighter's flaming lamp-staff: a Fire Breath cone."),
    P(["billy butcher's crowbar", "butcher's crowbar", "billy butcher", "butcher crowbar"],
      R("Butcher's Crowbar", "Hammer", ["Blunt", "Pierce"], 0.9, 3.0, (0.5, 0.5, 0.55), source="The Boys"),
      "Billy Butcher's crowbar."),
    P(["arthur the tick", "arthur's moth suit", "moth suit"],
      R("Arthur's Moth Suit", "Wings", ["Glide", "SlowFall"], 2.2, 3.0, (0.95, 0.95, 0.95), source="The Tick"),
      "Arthur's moth suit: glide wings (Not-in-the-face!)."),
]


def build_all():
    return [invincible, atom_eve, allen, battle_beast, thragg, mauler_twins, al_simmons, spawn_chains, compound_v,
            the_tick, soldier_boy] + PROC
