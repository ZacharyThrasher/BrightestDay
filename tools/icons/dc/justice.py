"""Justice League: Superman family, Wonder Woman, Martian Manhunter, Hawkman, Black Canary, Cyborg, Shazam
family, Blue Beetle, Green Arrow bits, Aquaman allies/villains, Flash rogues."""
from dlib import S, R, SUPER_RED, SUPER_BLUE, WW_GOLD, TIP_NZ, TIP_PZ, ALONG_Z, ALONG_X
from hero import hero, head, cowl, cape, wings, emblem_s, emblem_bolt, emblem_disc, fist_glow, humanoid, brute

SRC = "DC"


# ------------------------------------------------------------------------------------ Superman

def superman():
    s = S()
    b = hero(s, bulk=1.08, boots="A", trunks="A", belt="A", buckle="C")
    head(s, b, hair="D", style="curl")
    emblem_s(s, (0, b["y_hip"] + b["th"] * 0.68, -b["td"] / 2 - 0.55), t="A", inner="P", k=1.15)
    cape(s, b, t="A", flare=2.0)
    names = ["last son of krypton", "man of tomorrow", "big blue boy scout", "superman flying", "flying superman",
             "super man", "superman character", "superman icon", "the last son of krypton"]
    r = R("Superman", "Biped", ["Flies", "Beam", "Blunt", "Heavy"], 2.0, 120.0, SUPER_RED, elem="Fire",
          temp="Protective", inten=0.8, source="DC (Superman)")
    return names, r, s, ("Superman: blue suit, red cape/trunks/boots, the S-shield, the kiss curl. Flies, heat-vision\n"
                         "Beam, Blunt + Heavy punches that send people through buildings. Protective.")


def heat_vision():
    s = S()
    for sx in (-1, 1):
        s.ell((sx * 1.6, 0, 0), (2.2, 1.4, 1.2), "A")                   # the glowing eyes
        s.cz((sx * 1.6, 0, -8.0), 0.9, 15.0, "C")                       # twin beams
        s.cz((sx * 1.6, 0, -8.0), 1.6, 14.0, "A")
        s.ell((sx * 1.6, 0, -16.2), (2.4, 2.4, 2.0), "C")               # impact flare
    s.box((0, 0.9, 0.4), (5.6, 0.5, 1.0), "D")                          # brow line
    names = ["heat vision", "superman heat vision", "superman's heat vision", "laser vision", "eye lasers",
             "heat ray eyes", "kryptonian heat vision"]
    r = R("Heat Vision", "Gun", ["Beam", "Pierce"], 1.0, 0.5, SUPER_RED, elem="Fire", source="DC (Superman)")
    return names, r, s, "Heat vision: twin red eye beams - a Fire railgun (hitscan Beam + Pierce)."


def freeze_breath():
    s = S()
    s.ell((0, 0, 1.0), (3.0, 2.4, 1.6), "D")                            # pursed lips
    for i in range(5):
        z = -2.0 - i * 3.0
        d = 1.8 + i * 1.3
        s.cyl((0, 0, z), (d, 2.2, d), "A" if i % 2 else "C", ALONG_Z)   # widening frost cone rings
    for (x, y) in ((-3.0, 1.6), (2.8, -1.8), (1.2, 3.0), (-2.2, -2.6)):
        s.box((x, y, -12.0), (0.9, 0.9, 0.9), "C", (45, 45, 0))         # snowflakes
    names = ["freeze breath", "super breath", "superman's freeze breath", "arctic breath", "superman freeze breath",
             "kryptonian breath"]
    r = R("Freeze Breath", "Staff", ["Breath", "Repel"], 1.2, 0.5, (0.7, 0.9, 1.0), elem="Ice", source="DC (Superman)")
    return names, r, s, "Super breath: an Ice Breath cone that also shoves (Repel) - frosts and pushes."


def fortress_of_solitude():
    s = S()
    s.box((0, 0.6, 0), (30.0, 1.2, 22.0), "D")                          # ice floor
    spires = [(0, 0, 4.0, 28.0, 0), (-5.0, -2.0, 3.2, 22.0, -12), (5.2, -1.0, 3.2, 21.0, 11), (-9.5, 2.0, 2.6, 15.0, -20),
              (9.8, 2.5, 2.6, 16.0, 18), (-2.5, 5.0, 2.6, 18.0, -5), (3.0, 5.5, 2.4, 17.0, 7), (-12.5, -4.0, 2.0, 10.0, -28),
              (12.8, -4.5, 2.0, 11.0, 26), (0.5, -6.0, 2.2, 12.0, 3), (-7.0, 7.0, 2.0, 11.0, -15), (7.5, 7.5, 2.0, 12.0, 15)]
    for i, (x, z, w, h, rz) in enumerate(spires):
        t = "C" if i % 3 == 0 else ("A" if i % 3 == 1 else "P")
        s.box((x, h / 2, z), (w, h, w), t, (0, 45, rz))
        s.cone((x - rz * 0.02 * h, h + 1.2, z), (w * 1.1, 3.0, w * 1.1), t, (0, 45, rz))
    s.box((0, 4.0, -9.0), (5.0, 7.0, 3.0), "D")                         # entrance
    s.box((0, 4.0, -10.2), (3.2, 5.6, 1.0), "C")
    names = ["fortress of solitude", "superman's fortress", "the fortress of solitude", "superman fortress",
             "crystal fortress", "kryptonian crystal fortress"]
    r = R("Fortress of Solitude", "Wall", ["Barrier", "Guard"], 8.0, 4000.0, (0.75, 0.92, 1.0), elem="Ice",
          source="DC (Superman)")
    return names, r, s, "Superman's arctic Fortress: a cluster of giant ice-crystal spires - an Ice Barrier that reflects."


def general_zod():
    s = S()
    b = hero(s, bulk=1.1, suit="D", boots="D", gloves="D", belt="A")
    head(s, b, hair="D", style="short", brows="D")
    s.on(5)
    s.box((0, b["hy"] - 1.8, -1.6), (2.0, 1.0, 0.5), "D")               # goatee
    s.on(0)
    c = (0, b["y_hip"] + b["th"] * 0.7, -b["td"] / 2 - 0.6)
    s.box(c, (3.4, 3.4, 0.5), "A", (0, 0, 45))
    s.box((c[0], c[1], c[2] - 0.2), (1.6, 1.6, 0.4), "D", (0, 0, 45))
    cape(s, b, t="D", flare=1.0, length=b["y_sh"] - 3.0)
    names = ["general zod", "zod", "kneel before zod", "dru-zod", "general dru-zod"]
    r = R("General Zod", "Biped", ["Flies", "Beam", "Blunt", "Charge"], 2.0, 120.0, (0.3, 0.3, 0.32), elem="Fire",
          temp="Aggressive", inten=0.9, source="DC (Superman)")
    return names, r, s, "General Zod: black Kryptonian armour, goatee. Kneel. Flying heat-vision charger, Aggressive."


def bizarro():
    s = S()
    b = hero(s, bulk=1.15, boots="A", trunks="A", belt="A")
    head(s, b, skin="P", hair="D", style="short", w=4.4, h=4.6)
    s.on(5)
    for (x, y, sx) in ((-0.8, 0.4, 1), (0.9, -0.4, -1), (0.2, 1.4, 1)):
        s.box((x, b["hy"] + y, -2.1), (1.2, 0.4, 0.3), "D", (0, 0, 20 * sx))   # cracked crystal face
    s.on(0)
    c = (0, b["y_hip"] + b["th"] * 0.68, -b["td"] / 2 - 0.55)
    s.box(c, (3.0, 3.0, 0.4), "A", (0, 0, 45))
    s.box((c[0], c[1], c[2] - 0.15), (0.5, 1.8, 0.3), "P", (0, 0, 20))       # backwards S
    cape(s, b, t="A", flare=1.0, scallops=4)
    names = ["bizarro", "bizarro superman", "bizarro no.1", "me am bizarro", "bizarro world"]
    r = R("Bizarro", "Biped", ["Flies", "Breath", "Blunt"], 2.0, 130.0, (0.55, 0.75, 0.9), elem="Fire", temp="Chaotic",
          inten=0.9, source="DC (Superman)")
    return names, r, s, "Bizarro: the backwards Superman with the cracked chalky face. Flame breath, Chaotic (hits anyone)."


def doomsday():
    s = S()
    b = brute(s, bulk=1.5, leg_h=8.0, torso=(10.4, 10.5, 6.6), arm_len=11.0)
    yh, ys, td, tw = b["y_hip"], b["y_sh"], b["td"], b["tw"]
    # bone spurs everywhere
    for (x, y, z, rx, rz, L) in ((-3.5, ys - 1.0, 0.8, 30, 40, 4.0), (3.5, ys - 1.0, 0.8, 30, -40, 4.0),
                                 (0, ys - 3.0, td / 2 + 1.0, 80, 0, 4.0), (0, ys - 6.5, td / 2 + 0.8, 80, 0, 3.4),
                                 (-2.0, yh + 3.0, td / 2 + 0.6, 70, 20, 3.0), (2.0, yh + 3.0, td / 2 + 0.6, 70, -20, 3.0)):
        s.cone((x, y, z), (1.6, L, 1.6), "C", (rx, 0, rz))
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        hx, hy, hz = b["hands"][sx]
        for dy in (3.0, 5.6, 8.2):
            s.cone((hx + sx * 1.8, hy + dy, hz), (1.2, 2.8, 1.2), "C", (0, 0, -sx * 80))   # forearm spikes
        for dx in (-0.9, 0.0, 0.9):
            s.cone((hx + dx, hy - 1.2, hz - 1.6), (0.7, 1.6, 0.7), "C", (-130, 0, 0))      # knuckle bones
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        s.cone((sx * b["hip_x"], 5.0, -2.2), (1.2, 2.4, 1.2), "C", (-75, 0, 0))             # knee spikes
    s.on(5)
    hy = b["y_neck"] + 2.2
    s.ell((0, hy, -0.6), (4.6, 4.8, 4.8), "P")
    s.box((0, hy - 1.6, -1.8), (3.6, 1.8, 2.6), "P")
    s.box((0, hy - 2.0, -3.0), (3.0, 0.6, 0.5), "C")                    # teeth
    for sx in (-1, 1):
        s.box((sx * 1.1, hy + 0.4, -2.9), (1.0, 0.6, 0.4), "C")
        s.cone((sx * 1.9, hy + 2.2, 0.4), (1.2, 3.6, 1.2), "C", (-20, 0, -sx * 35))       # skull horns
    s.cone((0, hy + 2.8, 0.8), (1.0, 3.0, 1.0), "C", (-30, 0, 0))
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    names = ["doomsday", "doomsday the monster", "doomsday creature", "doomsday kryptonian monster",
             "the thing that killed superman"]
    r = R("Doomsday", "Biped", ["Charge", "Blunt", "Heavy", "Stomp"], 3.0, 900.0, (0.55, 0.5, 0.45),
          temp="Aggressive", inten=1.0, feat=["Spikes"], source="DC (Superman)")
    return names, r, s, ("Doomsday: bone-spurred grey behemoth. Charges and pummels (Blunt + Heavy) - victims plough\n"
                         "through buildings. Relentless (Aggressive 1.0). film_tv's 'doomsday monster' is shape-less.")


# ------------------------------------------------------------------------------------ Batman

def batman():
    s = S()
    b = hero(s, bulk=1.05, suit="P", boots="D", gloves="D", belt="A", buckle="C", glove_h=3.2, trunks="D")
    cowl(s, b, t="D", chin="P")
    ch = (0, b["y_hip"] + b["th"] * 0.7, -b["td"] / 2 - 0.55)
    from hero import emblem_bat
    emblem_bat(s, ch, t="D", k=1.1, oval="A")
    for pid, sx in ((3, -1), (4, 1)):                                  # gauntlet fins
        s.on(pid)
        hx, hy, hz = b["hands"][sx]
        for dy in (2.2, 3.2, 4.2):
            s.cone((hx + sx * 1.4, hy + dy, hz + 0.3), (0.5, 1.3, 0.5), "D", (0, 0, -sx * 110))
    s.on(0)
    for x in (-2.2, -1.1, 1.1, 2.2):                                   # utility belt pouches
        s.box((x, b["y_hip"] + 1.6, -b["td"] / 2 - 0.2), (0.9, 1.3, 0.6), "A")
    cape(s, b, t="D", flare=3.0, scallops=5, length=b["y_sh"] - 0.6)
    payload = R("batarang", "Disc", ["Returning", "Ricochet"], 0.35, 0.3, (0.3, 0.3, 0.35))
    names = ["batman", "the batman", "the dark knight", "dark knight", "caped crusader", "bruce wayne",
             "the caped crusader", "world's greatest detective"]
    r = R("Batman", "Biped", ["Shoots", "Pull", "Glide", "Blunt"], 1.9, 95.0, (0.95, 0.8, 0.15),
          temp="Sneaky", inten=0.9, payload=payload, source="DC (Batman)")
    return names, r, s, ("Batman: cowl ears, scalloped cape, yellow-oval bat emblem, utility belt, gauntlet fins.\n"
                         "Sneaky: flanks out of view, flings returning batarangs, grapples (Pull), glides on the cape.")




def build_all():
    return [superman, heat_vision, freeze_breath, fortress_of_solitude, general_zod, bizarro, doomsday, batman]
