"""Vans, sports cars and a motorbike: Ironhide, Ratchet, Bulkhead, Jazz, Wheeljack, Arcee."""
from tflib import TF, axes
from bots_a import pelvis
from bots_cars import car_bot, std_arms


# ------------------------------------------------------------------------------ generic van -> robot

def van_bot(t, L=18.0, W=8.4, floor=1.2, roof=9.0, cabd=6.0, belt=5.0, wheel_d=3.6, hip=12.0, body_t="A",
            leg_x=2.5, arm_x=6.6, win_t="P", grille_t="D"):
    """G1-style van (Ironhide/Ratchet mold): front cab -> chest (upright, windshield high on the chest),
    lower rear body halves -> legs (Rx 90) with rear wheels on the calves, upper rear body halves (the
    boxy sides) -> ARMS (shared panels, Rx 90), front wheels -> hips. Panels: 0 cab, 1-2 legs,
    3-4 arms, 6-7 rear wheels, 8-9 front wheels."""
    zf, zr = -L / 2, L / 2
    cz = zf + cabd
    H = roof - floor
    r = {}
    c = t.panel(0, anchor=(0, floor, (zf + cz) / 2))
    c.bx((0, floor + H / 2, (zf + cz) / 2), (W, H, cabd), body_t)
    c.bx((0, roof - H * 0.27, zf - 0.05), (W - 1.4, H * 0.34, 0.6), win_t)                   # windshield
    c.bx((0, floor + 1.9, zf - 0.1), (W * 0.42, 1.8, 0.5), grille_t)                         # grille
    for sx in (-1, 1):
        c.bx((sx * W * 0.36, floor + 2.0, zf - 0.1), (1.4, 1.1, 0.5), "C")                   # headlights
        c.bx((sx * (W / 2 + 0.05), roof - H * 0.3, (zf + cz) / 2 - 0.4), (0.4, H * 0.3, cabd - 2), win_t)
    c.bx((0, floor + 0.5, zf - 0.3), (W + 0.3, 1.0, 0.9), "D")                               # bumper -> belt
    chest_bot = hip + 1.4
    t.place(0, at=(0, chest_bot, 0.4))
    r["chest_top"] = chest_bot + H
    r["chest_front"] = 0.4 - cabd / 2
    r["chest_back"] = 0.4 + cabd / 2
    legs = {}
    for side, pid in ((-1, 1), (1, 2)):
        x = side * W / 4
        b = t.panel(pid, anchor=(x, belt, cz))
        b.bx((x, (floor + belt) / 2, (cz + zr) / 2), (W / 2 - 0.2, belt - floor, zr - cz), body_t)
        b.bx((x, floor - 0.5, zr - 1.0), (W / 2 - 0.2, 1.0, 2.2), "D")                         # foot
        b.bx((x, floor - 0.1, cz + 4.6), (W / 2 - 1.0, 0.4, 2.0), "D")                         # knee
        b.bx((x + side * W * 0.1, floor + 2.2, zr + 0.1), (1.2, 1.0, 0.4), "C")                # tail light
        t.alt_part(pid, 0, (x, belt, cz), "Other")
        t.place(pid, at=(side * leg_x, hip, belt - floor - 1.6), rot=(90, 0, 0), parent=0,
                pivot=(side * leg_x, hip, 0.3), role="LegBackLeft" if side < 0 else "LegBackRight")
        legs[side] = pid
    r["legs"] = legs
    arms = {}
    ah = roof - belt
    wall = 1.8
    for side, pid in ((-1, 3), (1, 4)):
        x = side * (W / 2 - wall / 2)
        b = t.panel(pid, anchor=(x, roof, cz))
        b.bx((x, belt + ah / 2, (cz + zr) / 2), (wall, ah, zr - cz), body_t)
        b.bx((side * (W / 2 + 0.02), belt + ah * 0.55, cz + 2.4), (0.4, ah * 0.5, 3.0), win_t)  # side window
        b.bx((x, roof - 0.2, zr - 0.4), (wall - 0.2, 0.6, 0.6), "D")                           # rear lip -> wrist
        t.alt_part(pid, 0, (x, roof, cz), "Other")
        # side wall -> arm: alt Z (length) points down, the roof edge faces outward, the outer skin faces front
        rot = axes((0, 0, 1), (-1, 0, 0), (0, -1, 0)) if side < 0 else axes((0, 0, -1), (1, 0, 0), (0, -1, 0))
        t.place(pid, at=(side * (arm_x + ah / 2 - 0.2), r["chest_top"] - 0.3, 0.4), rot=rot, parent=0,
                pivot=(side * arm_x, r["chest_top"] - 1.0, 0.4), role="ArmLeft" if side < 0 else "ArmRight")
        arms[side] = pid
    bp = t.panel(5, anchor=(0, roof, cz))                                                      # inner rear box -> backpack
    bp.bx((0, belt + ah / 2, (cz + zr) / 2), (W - 2 * wall - 0.2, ah, zr - cz), body_t)
    bp.bx((0, roof + 0.1, (cz + zr) / 2), (W - 2 * wall - 1.2, 0.4, zr - cz - 2), "D")           # roof rack -> spine
    t.place(5, at=(0, r["chest_top"] - 0.5, r["chest_back"]), rot=(90, 0, 0), parent=0)
    r["arms"] = arms
    r["hand_y"] = r["chest_top"] - 0.3 - (zr - cz)
    r["arm_x"] = arm_x
    r["ah"] = ah
    wid = 6
    for side in (-1, 1):
        wp = (side * (W / 2 + 0.1), wheel_d / 2, zr - 3.4)
        w = t.panel(wid, anchor=wp)
        w.wheel(wp, wheel_d, 1.3)
        t.alt_part(wid, 0, wp, "Wheel")
        t.follow(wid, legs[side], role="Wheel")
        wid += 1
    for side in (-1, 1):
        wp = (side * (W / 2 + 0.1), wheel_d / 2, zf + 3.0)
        w = t.panel(wid, anchor=wp)
        w.wheel(wp, wheel_d, 1.3)
        t.alt_part(wid, 0, wp, "Wheel")
        t.place(wid, at=(side * (W / 2 + 0.6), hip + 1.2, 0.2), parent=0, role="Wheel")
        wid += 1
    r["hip"] = hip
    return r


def fists(t, r, pid_l=21, pid_r=22, t_="D", size=(2.6, 2.4, 2.8)):
    """Robot-only fists below shared van arms."""
    hs = {}
    for side, pid in ((-1, pid_l), (1, pid_r)):
        x = side * (r["arm_x"] - 0.2)
        y = r["hand_y"] - size[1] / 2 + 0.3
        f = t.robot_only(pid, r["arms"][side], (x, y + size[1] / 2, 0.2), "Other")
        f.bx((x, y, 0.0), size, t_)
        hs[side] = (x, y, 0.0)
    return hs


def ironhide():
    t = TF(k=1.3)
    r = van_bot(t, body_t="A", hip=11.5)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.4, w=5.8, t_=("D", "A"))
    hs = fists(t, r)
    for side, pid in ((-1, 24), (1, 25)):                        # twin arm cannons (Bayverse)
        x, y, z = hs[side]
        g = t.robot_only(pid, r["arms"][side], (x, y, z), "Weapon")
        g.cz((x + side * 0.4, y + 3.0, z - 3.2), 1.6, 5.0, "D")
        g.cz((x + side * 0.4, y + 3.0, z - 5.8), 1.0, 0.6, "C")
    hy = ct + 0.1
    h = t.robot_only(23, 0, (0, hy, 0.4), "Head")
    h.bx((0, hy + 2.0, 0.6), (4.0, 3.8, 3.6), "D")                                           # grey helmet
    h.bx((0, hy + 1.6, -1.3), (2.6, 2.2, 0.4), "P")                                          # face
    h.bx((0, hy + 2.3, -1.45), (2.4, 0.6, 0.4), "C")                                         # eyes
    h.bx((0, hy + 4.1, 0.2), (1.0, 0.8, 3.2), "A")                                           # crest
    t.finish()
    names = ["ironhide transformer", "transformers ironhide", "ironhide transformers", "ironhide autobot",
             "autobot ironhide", "ironhide van", "g1 ironhide"]
    recipe = dict(name="Ironhide", form="Vehicle", mech=["Shoots", "Heavy", "Guard", "Transforms"], scale=5.0,
                  mass=5000, accent=(0.8, 0.1, 0.08), icon="ironhide_transformer",
                  payload=dict(name="ironhide cannon blast", form="Ball", mech=["Explodes"], elem="Fire",
                               scale=0.6, mass=4.0, seed=81),
                  feat=["Wheels", "Armor", "Barrels"], temp="Protective", inten=0.8)
    note = ("Ironhide: G1 red van (the Ironhide/Ratchet mold). Van cab -> chest, lower body -> legs,\n"
            "boxy upper sides -> arms (shared panels), twin arm cannons. Weapons specialist.")
    return names, recipe, t, note


def ratchet():
    t = TF(k=1.3)
    r = van_bot(t, body_t="A", hip=11.5)
    cr = t.panel(10, anchor=(0, 4.2, -9.2))                                                  # medic cross
    cr.bx((0, 5.8, -9.35), (0.9, 3.0, 0.4), "P")
    cr.bx((0, 5.8, -9.35), (3.0, 0.9, 0.4), "P")
    t.place(10, at=t.robot_pt(0, (0, 4.2, -9.2)), parent=0)
    lb = t.panel(11, anchor=(0, 9.0, -6.0))
    lb.bx((0, 9.4, -6.0), (5.0, 0.8, 1.2), "C")                                              # light bar
    lb.bx((0, 9.4, -6.0), (1.2, 0.9, 1.3), "P")
    t.place(11, at=(0, r["chest_top"] + 0.1, 2.4), parent=0)
    for side, pid in ((-1, 12), (1, 13)):
        x = side * 4.23
        s = t.panel(pid, anchor=(x, 7.0, 2.0))
        s.bx((x, 7.0, 2.0), (0.4, 2.6, 0.8), "P")
        s.bx((x, 7.0, 2.0), (0.4, 0.8, 2.6), "P")
        t.follow(pid, r["arms"][side])
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.4, w=5.8, t_=("D", "A"))
    hs = fists(t, r)
    x, y, z = hs[1]
    g = t.robot_only(24, r["arms"][1], (x, y, z), "Weapon")                                  # arc welder hand
    g.bx((x, y - 1.8, z - 0.6), (0.6, 2.2, 0.6), "D")
    g.bx((x, y - 3.0, z - 0.6), (0.9, 0.6, 0.9), "C")
    hy = ct + 0.1
    h = t.robot_only(23, 0, (0, hy, 0.4), "Head")
    h.bx((0, hy + 2.0, 0.6), (4.0, 3.8, 3.6), "A")                                           # white helmet
    h.bx((0, hy + 1.6, -1.3), (2.6, 2.2, 0.4), "D")
    h.bx((0, hy + 2.3, -1.45), (2.4, 0.6, 0.4), "C")
    for sx in (-1, 1):
        h.bx((sx * 2.2, hy + 2.8, 0.6), (0.5, 2.0, 1.0), "P")                                # "ear" lights
    t.finish()
    names = ["ratchet transformer", "transformers ratchet", "ratchet transformers", "ratchet autobot",
             "autobot ratchet", "ratchet ambulance", "autobot medic", "g1 ratchet"]
    recipe = dict(name="Ratchet", form="Vehicle", mech=["Guard", "Barrier", "Transforms"], scale=5.0, mass=4500,
                  accent=(0.95, 0.95, 0.95), icon="ratchet_transformer",
                  feat=["Wheels", "Headlight"], temp="Protective", inten=0.9)
    note = ("Ratchet: G1 white ambulance van (crosses + light bar), Autobot medic -> Protective, guards his\n"
            "owner. Same mold as Ironhide, like the 1984 toys. Bare 'ratchet' stays the tool.")
    return names, recipe, t, note


def bulkhead():
    t = TF(k=1.3)
    r = van_bot(t, L=19, W=9.6, roof=9.6, cabd=6.4, body_t="A", hip=11.5, leg_x=2.9, arm_x=7.6, win_t="D")
    ab = t.panel(10, anchor=(0, 1.2, -9.5))                                                  # bull bar
    ab.bx((0, 2.6, -9.9), (8.0, 0.8, 0.8), "D")
    for sx in (-3.0, 0.0, 3.0):
        ab.bx((sx, 2.6, -9.9), (0.8, 2.6, 0.8), "D")
    t.place(10, at=t.robot_pt(0, (0, 1.2, -9.5)), parent=0)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.4, w=6.6, t_=("D", "A"))
    hs = fists(t, r, size=(3.2, 3.0, 3.4))
    x, y, z = hs[1]
    wb = t.robot_only(24, r["arms"][1], (x, y, z), "Weapon")                                 # wrecking ball
    wb.bx((x, y - 2.4, z), (0.8, 2.4, 0.8), "D")
    wb.el((x, y - 5.0, z), (4.4, 4.4, 4.4), "D")
    for d in ("+x", "-x", "+z", "-z"):
        dx = {"+x": 1, "-x": -1}.get(d, 0)
        dz = {"+z": 1, "-z": -1}.get(d, 0)
        wb.cone((x + dx * 2.3, y - 5.0, z + dz * 2.3), 1.0, 1.4, "A", dir=d)
    hy = ct - 1.2
    h = t.robot_only(23, 0, (0, hy, 0.4), "Head")
    h.bx((0, hy + 1.8, 0.4), (3.6, 3.2, 3.2), "A")
    h.bx((0, hy + 3.0, -1.1), (3.8, 0.8, 0.8), "D")                                          # heavy brow
    h.bx((0, hy + 1.4, -1.25), (2.2, 1.6, 0.4), "D")
    h.bx((0, hy + 2.3, -1.3), (2.2, 0.5, 0.4), "C")
    t.finish()
    names = ["bulkhead transformer", "transformers bulkhead", "bulkhead autobot", "autobot bulkhead",
             "bulkhead wrecker", "bulkhead prime"]
    recipe = dict(name="Bulkhead", form="Vehicle", mech=["Blunt", "Charge", "Heavy", "Transforms"], scale=5.4,
                  mass=9000, accent=(0.3, 0.6, 0.22), icon="bulkhead_transformer",
                  feat=["Wheels", "Armor", "Chains"], temp="Protective", inten=0.7)
    note = ("Bulkhead (Transformers Prime / Animated): big green armoured truck, Wrecker -> wrecking-ball hand,\n"
            "Blunt + Charge + Heavy.")
    return names, recipe, t, note


# ------------------------------------------------------------------------------ Jazz, Wheeljack

def jazz():
    t = TF(k=1.3)
    r = car_bot(t, L=18, W=8.4, body=(1.0, 3.8), roof=6.0, cab=(-1.2, 4.2), stripe_t="P", hip=11.5, leg_x=2.4,
                door_rot=lambda s: (-80, s * 20, 0), door_at=lambda s: (s * 3.6, 19.2, 2.2), sh_x=6.0,
                front_wheels="back")
    sp = t.panel(10, anchor=(0, 4.2, 8.4))                                                   # whale-tail spoiler
    sp.bx((0, 4.9, 8.6), (8.0, 0.5, 2.2), "A")
    for sx in (-1, 1):
        sp.bx((sx * 3.6, 4.4, 8.6), (0.5, 1.0, 1.6), "A")
    t.place(10, at=(0, r["chest_top"] - 0.4, r["chest_back"] + 1.4), rot=(-60, 0, 0), parent=0)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.6, w=5.6, t_=("D", "A"))
    hl, hr = std_arms(t, r, pad_t="A", fore_t="A")
    g = t.robot_only(24, 22, hr, "Weapon")                                                   # photon rifle
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 2.6), (1.1, 1.4, 4.4), "D")
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 5.0), (0.7, 0.7, 0.6), "C")
    sl = t.robot_only(25, 21, (-r["sh"][0], r["sh"][1], 0), "Weapon")                        # shoulder launcher
    sl.bx((-r["sh"][0], r["sh"][1] + 2.0, 0.6), (1.6, 1.4, 3.6), "D")
    sl.bx((-r["sh"][0], r["sh"][1] + 2.0, -1.3), (1.0, 0.8, 0.4), "C")
    hy = ct + 0.4
    h = t.robot_only(23, 0, (0, hy, 0.2), "Head")
    h.bx((0, hy + 0.5, 0.2), (1.6, 1.0, 1.6), "D")
    h.bx((0, hy + 2.6, 0.3), (3.8, 3.8, 3.6), "A")                                           # white helmet
    h.bx((0, hy + 2.8, -1.55), (3.6, 1.0, 0.5), "C")                                         # the visor
    h.bx((0, hy + 1.6, -1.5), (2.2, 1.0, 0.4), "D")                                          # mouth
    for sx in (-1, 1):
        h.bx((sx * 2.1, hy + 4.2, 0.4), (0.5, 2.4, 0.8), "A", rot=(0, 0, -sx * 12))         # horn "ears"
    t.finish()
    names = ["jazz transformer", "transformers jazz", "jazz transformers", "jazz autobot", "autobot jazz",
             "jazz porsche", "g1 jazz"]
    recipe = dict(name="Jazz", form="Vehicle", mech=["Shoots", "Fast", "Transforms"], scale=4.4, mass=1400,
                  accent=(0.93, 0.94, 1.0), icon="jazz_transformer",
                  payload=dict(name="photon bolt", form="Ball", mech=["Fast"], elem="Light", scale=0.4, mass=1.0,
                               seed=935),
                  feat=["Wheels", "Headlight"], temp="Playful", inten=0.7)
    note = ("Jazz: G1 white Porsche 935 racer (stripes + whale-tail spoiler). Visor head w/ horn ears,\n"
            "photon rifle + shoulder launcher. Playful, stylish.")
    return names, recipe, t, note


def wheeljack():
    t = TF(k=1.3)
    r = car_bot(t, L=17, W=8.4, body=(1.0, 3.6), roof=5.8, cab=(-2.4, 3.6), stripe_t="P", hip=11.5, leg_x=2.4,
                door_rot=lambda s: (-100, s * 15, s * -30), door_at=lambda s: (s * 3.4, 21.0, 2.4), sh_x=6.0)
    sp = t.panel(10, anchor=(0, 3.6, 7.8))                                                   # Stratos rear wing
    sp.bx((0, 4.4, 7.8), (7.6, 0.5, 1.6), "D")
    t.place(10, at=(0, r["hip"] + 2.2, r["chest_back"] + 0.8), rot=(-90, 0, 0), parent=0)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.6, w=5.6, t_=("D", "A"))
    hl, hr = std_arms(t, r, pad_t="A", fore_t="A")
    sc = t.robot_only(24, 22, (r["sh"][0], r["sh"][1], 0), "Weapon")                         # shoulder cannon
    sc.cz((r["sh"][0], r["sh"][1] + 2.1, -0.4), 1.4, 4.0, "D")
    sc.cz((r["sh"][0], r["sh"][1] + 2.1, -2.6), 0.9, 0.5, "C")
    hy = ct + 0.4
    h = t.robot_only(23, 0, (0, hy, 0.2), "Head")
    h.bx((0, hy + 0.5, 0.2), (1.6, 1.0, 1.6), "D")
    h.bx((0, hy + 2.6, 0.3), (3.6, 3.8, 3.6), "A")
    h.bx((0, hy + 2.0, -1.55), (2.6, 2.2, 0.5), "D")                                         # full faceplate
    h.bx((0, hy + 3.5, -1.6), (2.6, 0.6, 0.4), "C")                                          # eyes
    for sx in (-1, 1):
        h.bx((sx * 2.2, hy + 2.8, 0.3), (0.8, 2.6, 1.6), "C")                                # flashing ear lights
    t.finish()
    names = ["wheeljack transformer", "transformers wheeljack", "wheeljack transformers", "wheeljack autobot",
             "autobot wheeljack", "wheeljack lancia", "g1 wheeljack"]
    recipe = dict(name="Wheeljack", form="Vehicle", mech=["Shoots", "Explodes", "Transforms"], scale=4.2,
                  mass=1300, accent=(0.95, 0.95, 0.9), icon="wheeljack_transformer",
                  payload=dict(name="experimental gadget", form="Bomb", mech=["Explodes", "Bounce"], elem="Electric",
                               scale=0.5, mass=3.0, seed=77),
                  feat=["Wheels", "Headlight"], temp="Playful", inten=0.5)
    note = ("Wheeljack: G1 Lancia Stratos rally car, gull-wing doors -> wings on the back; ear lights flash\n"
            "when he talks. The inventor: his gadgets explode (Explodes payload).")
    return names, recipe, t, note


# ------------------------------------------------------------------------------ Arcee (motorbike)

def arcee():
    t = TF(k=1.3)
    c = t.panel(0, anchor=(0, 4.6, -4.0))                                                    # fairing -> chest
    c.bx((0, 5.6, -5.0), (2.6, 3.0, 3.2), "A")
    c.bx((0, 7.0, -4.2), (2.0, 1.4, 1.8), "P", rot=(-30, 0, 0))                             # windscreen
    c.bx((0, 5.2, -6.7), (1.2, 0.8, 0.4), "C")                                               # headlight
    c.bx((0, 4.6, -3.0), (2.8, 2.2, 2.2), "A")
    t.place(0, at=(0, 15.6, 0.4), rot=(-80, 0, 0))
    tk = t.panel(1, anchor=(0, 6.0, -1.0))                                                   # tank/seat -> hips
    tk.bx((0, 5.4, 0.2), (2.4, 2.0, 4.4), "A")
    tk.bx((0, 6.3, 2.8), (1.8, 0.6, 2.8), "D")
    t.place(1, at=(0, 12.4, 1.4), rot=(90, 0, 0), parent=0)
    for side, pid in ((-1, 2), (1, 3)):                                                      # exhausts -> calves
        e = t.panel(pid, anchor=(side * 1.3, 3.8, 3.0))
        e.cz((side * 1.3, 3.8, 4.4), 1.0, 3.6, "D")
        t.place(pid, at=(side * 2.1, 6.4, 1.0), rot=(90, 0, 0), parent=10 if side < 0 else 11)
    for pid, z in ((4, -6.2), (5, 6.0)):                                                     # wheels -> back
        w = t.panel(pid, anchor=(0, 2.4, z))
        w.cx((0, 2.4, z), 4.8, 1.0, "D")
        w.cx((0, 2.4, z), 2.2, 1.2, "A")
        t.alt_part(pid, 0, (0, 2.4, z), "Wheel")
        side = -1 if pid == 4 else 1
        t.place(pid, at=(side * 1.6, 16.8, 2.4), rot=(0, 90 + side * 25, 0), parent=0, role="Wheel")
    fk = t.panel(6, anchor=(0, 4.4, -5.8))                                                   # forks -> blade
    fk.bx((0, 3.6, -6.0), (1.8, 2.2, 0.5), "D", rot=(-20, 0, 0))
    t.place(6, at=(4.3, 11.0, -0.8), parent=13)
    for side, pid in ((-1, 10), (1, 11)):
        lg = t.robot_only(pid, 0, (side * 1.7, 11.6, 0.2), "LegBackLeft" if side < 0 else "LegBackRight")
        lg.bx((side * 1.7, 9.0, 0.2), (1.8, 5.2, 2.0), "D")                                  # thigh
        lg.bx((side * 1.8, 3.8, 0.3), (2.2, 5.6, 2.4), "A")                                  # shin
        lg.bx((side * 1.8, 0.6, -0.4), (1.8, 1.2, 3.4), "D")                                 # foot
    for side, pid in ((-1, 12), (1, 13)):
        sx = side * 3.6
        a = t.robot_only(pid, 0, (sx, 18.6, 0.3), "ArmLeft" if side < 0 else "ArmRight")
        a.bx((sx, 18.2, 0.3), (1.8, 1.8, 2.0), "A")
        a.bx((sx, 15.8, 0.3), (1.3, 3.2, 1.3), "D")
        a.bx((sx, 12.6, 0.2), (1.8, 3.4, 1.9), "A")
        a.bx((sx, 10.4, 0.2), (1.4, 1.2, 1.5), "D")
        a.bx((sx + side * 0.7, 12.6, -1.2), (0.4, 3.2, 1.0), "C")                            # arm blade
    t.robot_only(20, 0, (0, 11.8, 0.2), "Other").bx((0, 11.8, 0.3), (3.4, 1.8, 2.2), "D")
    hy = 19.0
    h = t.robot_only(23, 0, (0, hy, 0.4), "Head")
    h.bx((0, hy + 0.4, 0.4), (1.0, 1.0, 1.0), "D")
    h.el((0, hy + 2.2, 0.4), (3.0, 3.2, 3.0), "A")                                           # helmet
    h.bx((0, hy + 1.9, -0.9), (1.8, 1.8, 0.5), "D")                                          # face
    h.bx((0, hy + 2.3, -1.05), (1.8, 0.5, 0.4), "C")                                         # eyes
    h.bx((0, hy + 4.0, 0.2), (0.6, 1.4, 2.2), "P", rot=(-20, 0, 0))                          # crest
    for sx in (-1, 1):
        h.bx((sx * 1.6, hy + 3.0, 0.6), (0.5, 1.6, 1.2), "P", rot=(0, 0, -sx * 20))          # helmet fins
    t.finish()
    names = ["arcee transformer", "transformers arcee", "arcee transformers", "arcee autobot", "autobot arcee",
             "arcee motorcycle", "arcee motorbike", "g1 arcee", "arcee prime"]
    recipe = dict(name="Arcee", form="Vehicle", mech=["Blade", "Fast", "Transforms"], scale=3.2, mass=500,
                  accent=(1.0, 0.36, 0.66), icon="arcee_transformer",
                  feat=["Wheels", "Headlight", "Exhaust"], temp="Calm", inten=0.6)
    note = ("Arcee: pink motorbike (Prime / Bayverse / Rise of the Beasts). Fairing -> chest, wheels -> back,\n"
            "exhausts -> calves, forks -> blade; slim, fast, arm blades.")
    return names, recipe, t, note
