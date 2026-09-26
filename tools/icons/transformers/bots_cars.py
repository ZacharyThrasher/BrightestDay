"""Car-based Autobots on a shared car -> robot panel mapping (hood -> chest, halves -> legs...)."""
from tflib import TF
from bots_a import arm, pelvis


def car_bot(t, L=18.0, W=8.0, body=(1.2, 4.4), roof=7.0, cab=(-2.5, 4.5), wheel_d=3.6, wf=-5.6, wr=5.4,
            hip=12.5, leg_x=2.4, body_t="A", stripe_t=None, lights_t="C", door_rot=None, door_at=None,
            front_wheels="shoulders", sh_x=6.2, doors=True, cabin_at=None, cabin_rot=(90, 0, 0), win_t="P"):
    """A car whose hood -> chest (Rx -90: hood faces front, headlights at the abs), lower body halves ->
    legs (Rx 90), rear wheels -> calves, front wheels -> shoulders, doors -> back wings, cabin -> backpack.
    Panels: 0 hood/chest, 1-2 legs, 3 cabin, 4-5 doors, 6-7 rear wheels, 8-9 front wheels."""
    zf, zr = -L / 2, L / 2
    y0, y1 = body
    bh = y1 - y0
    cz0, cz1 = cab
    r = {"L": L, "W": W, "y0": y0, "y1": y1, "cab": cab, "roof": roof}
    # ---- part 0: nose / hood -> chest
    hood_len = cz0 - zf
    c = t.panel(0, anchor=(0, y1, cz0))
    c.bx((0, (y0 + y1) / 2, (zf + cz0) / 2), (W, bh, hood_len), body_t)
    c.bx((0, y0 + bh * 0.45, zf - 0.2), (W * 0.45, bh * 0.45, 0.5), "D")                  # grille
    for sx in (-1, 1):
        c.bx((sx * W * 0.34, y0 + bh * 0.62, zf - 0.15), (W * 0.2, bh * 0.28, 0.5), lights_t)  # headlights
    c.bx((0, y0 + 0.4, zf - 0.3), (W + 0.2, 0.9, 0.9), "D")                               # bumper
    if stripe_t:
        for sx in (-1, 1):
            c.bx((sx * 0.9, y1 + 0.05, (zf + cz0) / 2), (0.9, 0.5, hood_len), stripe_t)       # racing stripes
    chest_top = hip + 1.8 + hood_len
    t.place(0, at=(0, chest_top, -bh / 2), rot=(-90, 0, 0))
    r["chest_top"] = chest_top
    r["chest_front"] = -bh / 2
    r["chest_back"] = bh / 2
    # ---- lower body halves (floor, sills, rear quarters, trunk) -> legs
    legs = {}
    for side, pid in ((-1, 1), (1, 2)):
        x = side * W / 4
        b = t.panel(pid, anchor=(x, y1, cz0))
        b.bx((x, (y0 + y1) / 2, (cz0 + zr) / 2), (W / 2 - 0.2, bh, zr - cz0), body_t)
        b.bx((x, y1 + 0.3, zr - 1.2), (W / 2 - 0.6, 0.6, 2.2), "D")                         # trunk edge
        b.bx((x + side * W * 0.1, y0 + bh * 0.6, zr + 0.15), (W * 0.16, bh * 0.3, 0.4), "C")  # tail light -> heel
        b.bx((x, y0 - 0.5, zr - 1.0), (W / 2 - 0.4, 1.0, 2.0), "D")                          # foot (under rear)
        b.bx((x, y0 - 0.1, cz0 + 4.2), (W / 2 - 1.0, 0.4, 2.0), "D")                         # knee (underside)
        if stripe_t:
            b.bx((side * 0.9, y1 + 0.05, (zr + cz1) / 2 + 0.5), (0.9, 0.5, max(0.6, zr - cz1 - 1)), stripe_t)
        t.alt_part(pid, 0, (x, y1, cz0), "Other")
        t.place(pid, at=(side * leg_x, hip, bh - 1.4), rot=(90, 0, 0), parent=0, pivot=(side * leg_x, hip, 0.3),
                role="LegBackLeft" if side < 0 else "LegBackRight")
        legs[side] = pid
    r["legs"] = legs
    # ---- cabin (windshield, roof, rear window) -> backpack
    cb = t.panel(3, anchor=(0, y1, (cz0 + cz1) / 2))
    rh = roof - y1
    cb.bx((0, y1 + rh * 0.5, (cz0 + cz1) / 2 + 0.3), (W - 1.0, rh, cz1 - cz0 - 1.4), body_t)
    cb.bx((0, y1 + rh * 0.45, cz0 + 0.5), (W - 1.6, rh * 0.8, 0.6), win_t, rot=(-35, 0, 0))    # windshield
    cb.bx((0, y1 + rh * 0.45, cz1 - 0.3), (W - 1.8, rh * 0.7, 0.6), win_t, rot=(35, 0, 0))     # rear window
    t.place(3, at=cabin_at or (0, chest_top - (cz1 - cz0) / 2 - 0.3, r["chest_back"] + 0.1), rot=cabin_rot, parent=0)
    # ---- doors -> back wings
    if doors:
        for side, pid in ((-1, 4), (1, 5)):
            d = t.panel(pid, anchor=(side * (W / 2 + 0.2), y1, cz0 + 0.4))
            d.bx((side * (W / 2 + 0.2), (y0 + 0.4 + y1) / 2, (cz0 + cz1) / 2 - 0.6), (0.5, bh - 0.4, cz1 - cz0 - 1.2), body_t)
            d.bx((side * (W / 2 + 0.1), y1 + rh * 0.4, (cz0 + cz1) / 2 - 0.4), (0.5, rh * 0.7, cz1 - cz0 - 2.4), win_t)
            at = door_at(side) if door_at else (side * 3.2, chest_top - 1.0, r["chest_back"] + 0.6)
            rot = door_rot(side) if door_rot else (-90, side * 30, 0)
            t.place(pid, at=at, rot=rot, parent=0)
    # ---- wheels
    wid = 6
    for side in (-1, 1):
        w = t.panel(wid, anchor=(side * (W / 2 + 0.1), wheel_d / 2, wr))
        w.wheel((side * (W / 2 + 0.1), wheel_d / 2, wr), wheel_d, 1.3)
        t.alt_part(wid, 0, (side * (W / 2 + 0.1), wheel_d / 2, wr), "Wheel")
        t.follow(wid, legs[side], role="Wheel")
        wid += 1
    for side in (-1, 1):
        w = t.panel(wid, anchor=(side * (W / 2 + 0.1), wheel_d / 2, wf))
        w.wheel((side * (W / 2 + 0.1), wheel_d / 2, wf), wheel_d, 1.3)
        t.alt_part(wid, 0, (side * (W / 2 + 0.1), wheel_d / 2, wf), "Wheel")
        if front_wheels == "shoulders":
            t.place(wid, at=(side * (sh_x + 1.9), chest_top - 1.8, 0.2), parent=0, role="Wheel")
        else:
            t.place(wid, at=(side * 2.2, chest_top - 5.0, r["chest_back"] + 0.9), rot=(0, 90, 0), parent=0, role="Wheel")
        wid += 1
    r["sh"] = (sh_x, chest_top - 0.6, 0.2)
    r["hip"] = hip
    return r


def std_arms(t, r, pad_t="A", up_t="D", fore_t="A", fist_t="D", scale=1.0, sh_dx=0.0):
    sx, sy, sz = r["sh"]
    sx += sh_dx
    kw = dict(up_t=up_t, fore_t=fore_t, pad_t=pad_t, fist_t=fist_t, pad=(3.0 * scale, 3.0 * scale, 3.4 * scale),
              upper=(2.0 * scale, 3.4 * scale, 2.0 * scale), fore=(2.6 * scale, 4.4 * scale, 2.8 * scale),
              fist=(2.2 * scale, 2.0 * scale, 2.4 * scale))
    _, hl = arm(t, 21, -1, (-sx, sy, sz), **kw)
    _, hr = arm(t, 22, 1, (sx, sy, sz), **kw)
    return hl, hr


# ------------------------------------------------------------------------------ Bumblebee

def bumblebee():
    t = TF(k=1.3)
    r = car_bot(t, L=18, W=8, body=(1.2, 4.4), roof=6.8, cab=(-2.0, 4.8), stripe_t="D", hip=11.5, leg_x=2.3,
                door_rot=lambda s: (-50, s * 55, s * -25), door_at=lambda s: (s * 3.2, 18.6, 3.0), sh_x=5.8)
    c = t.panel(10, anchor=(0, 4.4, -6.0))
    c.bx((0, 4.9, -6.0), (2.6, 0.8, 3.0), "D")                                               # hood scoop
    t.place(10, at=t.robot_pt(0, (0, 4.4, -6.0)), rot=(-90, 0, 0), parent=0)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.6, w=5.4, t_=("D", "A"))
    hl, hr = std_arms(t, r)
    g = t.robot_only(24, 22, hr, "Weapon")                                                   # arm blaster
    g.bx((hr[0] + 1.4, hr[1] + 2.4, hr[2] - 0.8), (1.0, 1.0, 3.6), "D")
    g.bx((hr[0] + 1.4, hr[1] + 2.4, hr[2] - 2.8), (0.6, 0.6, 0.6), "C")
    hy = ct + 0.4
    h = t.robot_only(23, 0, (0, hy, 0.2), "Head")
    h.bx((0, hy + 0.5, 0.2), (1.6, 1.0, 1.6), "D")
    h.el((0, hy + 2.9, 0.2), (5.0, 4.8, 4.6), "A")                                          # round yellow helmet
    h.bx((0, hy + 2.0, -2.0), (3.0, 1.8, 0.8), "D")                                          # faceplate
    h.bx((-1.0, hy + 3.2, -2.1), (1.4, 0.9, 0.5), "C")                                       # big eyes
    h.bx((1.0, hy + 3.2, -2.1), (1.4, 0.9, 0.5), "C")
    for sx in (-1, 1):
        h.bx((sx * 1.5, hy + 5.6, 0.0), (0.7, 2.2, 0.7), "D", rot=(0, 0, -sx * 20))         # horn antennae
    t.finish()
    names = ["bumblebee transformer", "bumblebee robot", "bumblebee car", "bumblebee camaro", "bumblebee autobot",
             "autobot bumblebee", "bee transformer", "bumblebee the autobot"]
    recipe = dict(name="Bumblebee", form="Vehicle", mech=["Shoots", "Fast", "Transforms"], scale=4.2, mass=1600,
                  accent=(1.0, 0.82, 0.08), icon="bumblebee_transformer",
                  payload=dict(name="stinger bolt", form="Ball", mech=["Fast", "Pierce"], elem="Electric", scale=0.4,
                               mass=1.0, accent=(1.0, 0.85, 0.2), seed=127),
                  feat=["Wheels", "Headlight"], temp="Playful", inten=0.65)
    note = ("Bumblebee: yellow muscle car (Camaro) w/ black racing stripes. Hood -> chest (stripes + headlights),\n"
            "doors -> back door-wings, rear wheels -> calves, front wheels -> shoulders. Round helmet w/ horn antennae.\n"
            "Bare 'bumblebee' stays the insect: aliases are all qualified.")
    return names, recipe, t, note
