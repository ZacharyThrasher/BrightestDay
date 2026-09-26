"""Hand-authored ShapeSpecs for the Marvel + X-Men pack: characters. Units: construct voxels (0.25 m).

Origin = feet centre, +Y up, -Z forward (the face). Rig: parts 1/2 legs (LegBackLeft/Right), 3/4 arms
(ArmLeft/Right), 5 head, 6 jaw, 7.. capes (Cloth), tails/tendrils (Tail), wings. Props held in a hand are
authored on that arm's part so they swing with it.
"""
from mvlib import Shape

SIDES = (-1, 1)
L, R = 0, 1  # index into SIDES


def check(s):
    ids = {0} | {p[0] for p in s.parts}
    for pr in s.prims:
        assert pr[5] in ids, ("undefined part", pr)
    return s


# ------------------------------------------------------------------ rig helpers

def humanoid(s, leg_h, torso_h, head_h, tw, td, lw, aw, arm_l, tl="P", tt="P", ta="P", th="P", hw=None,
             hd=None, head=True, head_z=0.0, arm_out=0.0):
    hip = leg_h
    lx = tw / 2 - lw / 2
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * lx, hip, 0), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * lx, hip / 2, 0), (lw, hip + 0.4, lw), tl, part=pid)
    s.bx((0, hip + torso_h / 2, 0), (tw, torso_h, td), tt)
    shy = hip + torso_h - aw / 2
    ax = tw / 2 + aw / 2 + arm_out
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * ax, shy, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.bx((sx * ax, shy - arm_l / 2 + aw / 2, 0), (aw, arm_l, aw), ta, part=pid)
    top = hip + torso_h
    hd_ = hd or head_h
    if head:
        s.part(5, 0, (0, top, head_z), "Head")
        s.bx((0, top + head_h / 2, head_z), (hw or head_h, head_h, hd_), th, part=5)
    return dict(hip=hip, top=top, shy=shy, ax=ax, lx=lx, hc=top + head_h / 2, hand=shy - arm_l + aw / 2,
                tw=tw, td=td, lw=lw, aw=aw, hh=head_h, hw=hw or head_h, hd=hd_, fz=head_z - hd_ / 2,
                cz=-td / 2, hz=head_z)


def hero(k=1.0, leg=4.4, torso=3.6, head=2.0, tw=3.2, td=1.9, lw=1.25, aw=1.15, arm=3.6, **kw):
    s = Shape()
    hd = kw.pop("hd", None)
    hw = kw.pop("hw", None)
    h = humanoid(s, leg * k, torso * k, head * k, tw * k, td * k, lw * k, aw * k, arm * k,
                 hd=hd * k if hd else None, hw=hw * k if hw else None, **kw)
    return s, h


def boots(s, h, t="D", ht=1.2, grow=0.25, toe=0.4):
    for i, sx in enumerate(SIDES):
        s.bx((sx * h["lx"], ht / 2, -toe / 2), (h["lw"] + grow, ht, h["lw"] + grow + toe), t, part=1 + i)


def gloves(s, h, t="D", ln=1.2, grow=0.2):
    for i, sx in enumerate(SIDES):
        s.bx((sx * h["ax"], h["hand"] + ln / 2, 0), (h["aw"] + grow, ln, h["aw"] + grow), t, part=3 + i)


def belt(s, h, t="D", ht=0.5):
    s.bx((0, h["hip"] + ht / 2 + 0.05, 0), (h["tw"] + 0.2, ht, h["td"] + 0.2), t)


def chest(s, h, t, c, size, rot=None):
    """Emblem on the chest front; c = (x, y offset from shoulder line)."""
    s.bx((c[0], h["shy"] + c[1], h["cz"] - 0.12), size, t, rot=rot)


def face(s, h, t, x, y, size, rot=None, kind="bx"):
    """Feature on the face front (head part); y is relative to the head centre."""
    z = h["fz"] - size[2] / 2 + 0.12
    getattr(s, kind)((x, h["hc"] + y, z), size, t, rot=rot, part=5)


def eyes(s, h, t="C", dx=0.45, y=0.15, w=0.6, ht=0.35, slant=0.0, kind="bx"):
    for sx in SIDES:
        face(s, h, t, sx * dx, y, (w, ht, 0.3), rot=(0, 0, sx * slant) if slant else None, kind=kind)


def cape(s, h, pid, t="A", length=None, width=None, flare=0.0, collar=0.0, top_off=0.0):
    length = length or (h["top"] - 0.6)
    width = width or (h["tw"] + 0.6)
    top = h["top"] - top_off
    zb = h["td"] / 2 + 0.25
    s.part(pid, 0, (0, top, zb), "Cloth")
    s.bx((0, top - length / 2, zb + 0.1 + flare / 2), (width, length, 0.35), t, rot=(-6 - flare * 4, 0, 0), part=pid)
    s.bx((0, top - 0.2, zb - 0.1), (width - 0.4, 0.5, 0.6), t, part=pid)
    if collar:
        for sx in SIDES:
            s.bx((sx * (h["hw"] / 2 + 0.35), top + collar / 2, zb - 0.4), (0.35, collar, 1.2), t,
                 rot=(0, sx * 20, sx * -12), part=pid)
    return pid


def pads(s, h, t="D", w=None, ht=0.8, dz=0.0):
    for sx in SIDES:
        s.bx((sx * (h["tw"] / 2 + 0.1), h["top"] - ht / 2 + 0.15, dz), (w or (h["aw"] + 0.9), ht, h["td"] + 0.3), t)


def fins(s, h, t, ht=1.2, dx=None, y=0.5, ang=25, z=0.0):
    """Upward head fins / ears (Wolverine cowl, Cap's wings, Panther ears)."""
    dx = dx if dx is not None else h["hw"] / 2 - 0.1
    for sx in SIDES:
        s.bx((sx * (dx + 0.2), h["hc"] + y + ht / 2, h["hz"] + z), (0.35, ht, 0.8), t, rot=(0, 0, sx * -ang), part=5)


def hand_c(h, side):
    sx = SIDES[side]
    return (sx * h["ax"], h["hand"] + 0.2, 0.0)


# ------------------------------------------------------------------ Avengers

def captain_america():
    s, h = hero(tt="A", ta="P", tl="A")
    boots(s, h, "D", 1.6)
    gloves(s, h, "D", 1.3)
    belt(s, h, "D")
    # red/white stripes at the waist, star on the chest
    for y in (0.6, 1.2, 1.8):
        s.bx((0, h["hip"] + y, h["cz"] - 0.08), (h["tw"] - 0.3, 0.3, 0.2), "C" if y != 1.2 else "D")
    chest(s, h, "C", (0, -0.8), (1.0, 1.0, 0.2), rot=(0, 0, 45))
    chest(s, h, "C", (0, -0.55), (0.35, 1.0, 0.22))
    fins(s, h, "C", ht=0.9, ang=35, y=-0.1)
    face(s, h, "C", 0, 0.9, (0.5, 0.5, 0.2), kind="bx")  # the A
    face(s, h, "D", 0, -0.55, (1.4, 0.7, 0.2))            # jaw showing
    # the shield on the left forearm, facing out
    ax, hy = h["ax"], h["hand"] + 1.4
    s.cx((-ax - 0.55, hy, 0), 4.4, 0.3, "A", part=3)
    s.cx((-ax - 0.7, hy, 0), 3.4, 0.3, "C", part=3)
    s.cx((-ax - 0.85, hy, 0), 2.4, 0.3, "A", part=3)
    s.cx((-ax - 1.0, hy, 0), 1.3, 0.3, "C", part=3)
    return check(s)


def thor():
    s, h = hero(tw=3.5, td=2.1, aw=1.3, tt="D", ta="P")
    boots(s, h, "D", 1.8)
    for sx in SIDES:  # the four chest discs
        for y in (-0.5, -1.5):
            s.cz((sx * 0.8, h["shy"] + y, h["cz"] - 0.1), 0.8, 0.3, "C")
    pads(s, h, "D", ht=0.7)
    cape(s, h, 6, "A", length=h["top"] - 0.4, flare=0.5)
    s.bx((0, h["hc"] + 0.2, 0.5), (h["hw"] + 0.3, h["hh"] + 0.6, 1.2), "A", part=5)  # hair
    fins(s, h, "C", ht=1.1, ang=40, y=0.2)
    eyes(s, h, "C", dx=0.45, y=0.25, w=0.5, ht=0.25)
    # Mjolnir in the right hand (head below the fist)
    x, y, _ = hand_c(h, R)
    s.cy((x, y - 0.9, 0), (0.45, 1.8, 0.45), "D", part=4)
    s.bx((x, y - 2.4, 0), (1.3, 1.4, 2.2), "A", part=4)
    s.bx((x, y - 1.95, 0), (1.35, 0.3, 2.25), "C", part=4)
    return check(s)


def black_panther():
    s, h = hero(tw=3.0, td=1.7, lw=1.15, aw=1.05, tt="D", tl="D", ta="D", th="D", hd=2.1)
    fins(s, h, "D", ht=0.7, dx=0.6, y=0.6, ang=15)  # ears
    eyes(s, h, "C", dx=0.45, y=0.2, w=0.7, ht=0.28, slant=12)
    # vibranium necklace + lines
    s.bx((0, h["top"] - 0.25, h["cz"] - 0.1), (2.0, 0.3, 0.3), "A")
    for sx in SIDES:
        s.bx((sx * 0.8, h["top"] - 0.9, h["cz"] - 0.1), (0.25, 1.2, 0.25), "A", rot=(0, 0, sx * -30))
        s.bx((sx * h["lx"], h["hip"] - 1.4, -0.7), (0.25, 1.6, 0.3), "A", part=1 + (0 if sx < 0 else 1))
    # claws
    for i, sx in enumerate(SIDES):
        for dx in (-0.35, 0.0, 0.35):
            s.cone((sx * h["ax"] + dx, h["hand"] - 0.5, -0.2), 0.3, 1.0, "C", "-y", part=3 + i)
    return check(s)


def doctor_strange():
    s, h = hero(tt="D", ta="P", tl="D")
    boots(s, h, "D", 1.3)
    cape(s, h, 6, "A", length=h["top"] - 0.2, width=h["tw"] + 1.2, flare=0.4, collar=2.2)
    s.bx((0, h["hip"] + 0.9, 0), (h["tw"] + 0.3, 1.8, h["td"] + 0.3), "P")  # tunic skirt
    belt(s, h, "A", 0.6)
    chest(s, h, "C", (0, -0.3), (0.8, 0.8, 0.3))                             # eye of agamotto
    s.bx((0, h["hc"] + 0.7, 0.2), (h["hw"] + 0.2, 0.6, h["hd"] + 0.2), "D", part=5)  # hair
    eyes(s, h, "C", dx=0.45, y=0.2, w=0.4, ht=0.2)
    # mandala shields at both hands, facing forward
    for i, sx in enumerate(SIDES):
        s.cz((sx * h["ax"], h["hand"] + 0.2, -1.2), 3.2, 0.15, "C", part=3 + i)
        s.cz((sx * h["ax"], h["hand"] + 0.2, -1.3), 2.2, 0.15, "A", part=3 + i)
    return check(s)


def scarlet_witch():
    s, h = hero(tw=2.8, td=1.6, lw=1.05, aw=1.0, torso=3.4, tt="A", ta="D", tl="D")
    boots(s, h, "A", 1.8)
    # long coat tails
    s.part(6, 0, (0, h["hip"] + 0.6, 0.6), "Cloth")
    s.bx((0, h["hip"] - 1.2, 0.95), (h["tw"] + 0.8, 3.6, 0.3), "A", rot=(-10, 0, 0), part=6)
    s.bx((0, h["hc"] - 0.3, 0.4), (h["hw"] + 0.5, h["hh"] + 1.6, 1.2), "A", part=5)  # hair
    for sx in SIDES:  # the tiara
        s.bx((sx * 0.55, h["hc"] + 1.2, h["fz"] + 0.3), (0.3, 1.5, 0.3), "A", rot=(0, 0, sx * -25), part=5)
    s.bx((0, h["hc"] + 0.7, h["fz"] + 0.15), (1.5, 0.35, 0.3), "A", part=5)
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.35, ht=0.2)
    for i, sx in enumerate(SIDES):  # hex bolts
        s.el((sx * h["ax"], h["hand"] - 0.3, -0.5), (1.4, 1.4, 1.4), "C", part=3 + i)
    return check(s)


def vision():
    s, h = hero(tt="A", ta="A", tl="A", th="A")
    boots(s, h, "P", 1.8)
    gloves(s, h, "P", 1.2)
    cape(s, h, 6, "P", length=h["top"] - 0.4, collar=1.0)
    for y in (-0.6, -1.6):  # the diamond chest pattern
        chest(s, h, "D", (0, y), (1.3, 1.3, 0.2), rot=(0, 0, 45))
    face(s, h, "C", 0, 0.65, (0.7, 0.5, 0.3))  # mind stone
    for sx in SIDES:
        face(s, h, "D", sx * 0.6, 0.0, (0.3, 1.2, 0.2))
    eyes(s, h, "C", dx=0.45, y=0.15, w=0.4, ht=0.2)
    return check(s)


def captain_marvel():
    s, h = hero(tw=3.0, td=1.8, torso=3.5, tt="P", ta="A", tl="P")
    boots(s, h, "A", 2.0)
    gloves(s, h, "A", 1.4)
    belt(s, h, "A", 0.6)
    chest(s, h, "C", (0, -0.9), (1.3, 1.3, 0.2), rot=(0, 0, 45))   # the star
    for sx in SIDES:
        chest(s, h, "A", (sx * 0.95, -0.9), (0.6, 0.35, 0.18))
    s.bx((0, h["hc"] + 0.9, 0.1), (h["hw"] + 0.1, 0.9, h["hd"] + 0.2), "C", part=5)  # mohawk/hair
    s.bx((0, h["hc"] - 0.2, 0.7), (h["hw"] + 0.3, h["hh"] + 0.8, 0.7), "C", part=5)
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.4, ht=0.2)
    for i, sx in enumerate(SIDES):  # photon fists
        s.el((sx * h["ax"], h["hand"] - 0.2, 0), (1.5, 1.5, 1.5), "C", part=3 + i)
    return check(s)


def star_lord():
    s, h = hero(tt="D", ta="A", tl="D", th="D")
    boots(s, h, "P", 1.5)
    # long coat
    s.part(6, 0, (0, h["hip"] + 0.5, 0.5), "Cloth")
    s.bx((0, h["hip"] - 1.0, 0.9), (h["tw"] + 0.6, 3.0, 0.3), "A", rot=(-8, 0, 0), part=6)
    s.bx((0, h["top"] - 1.6, 0), (h["tw"] + 0.25, 3.2, h["td"] + 0.25), "A")
    # the helmet: ribbed mask, big red eyes
    for y in (-0.5, 0.0, 0.5):
        face(s, h, "P", 0, y - 0.1, (0.3, 0.3, 0.2))
    eyes(s, h, "A", dx=0.5, y=0.3, w=0.7, ht=0.45)
    for sx in SIDES:
        s.cx((sx * (h["hw"] / 2 + 0.15), h["hc"], h["hz"]), 1.0, 0.4, "P", part=5)
    # element blasters
    for i, sx in enumerate(SIDES):
        x, y = sx * h["ax"], h["hand"]
        s.el((x, y - 0.1, 0.2), (1.2, 1.2, 1.3), "D", part=3 + i)
        s.cz((x, y - 0.1, -1.1), 0.6, 1.8, "C", part=3 + i)
    return check(s)


def rocket_raccoon():
    s = Shape()
    h = humanoid(s, 1.4, 1.6, 1.7, 1.8, 1.3, 0.7, 0.6, 1.5, tt="A", ta="P", tl="D", hw=2.0, hd=1.8)
    s.el((0, h["hc"] - 0.2, h["fz"] - 0.3), (1.0, 0.7, 1.0), "C", part=5)   # muzzle
    s.bx((0, h["hc"] - 0.1, h["fz"] - 0.8), (0.4, 0.3, 0.3), "D", part=5)    # nose
    s.bx((0, h["hc"] + 0.35, h["fz"] - 0.05), (1.9, 0.5, 0.2), "D", part=5)  # mask stripe
    for sx in SIDES:
        s.cone((sx * 0.7, h["hc"] + 1.1, h["hz"]), 0.6, 0.9, "P", "+y", part=5)
        s.bx((sx * 0.45, h["hc"] + 0.35, h["fz"] - 0.12), (0.3, 0.25, 0.2), "C", part=5)
    # ringed tail
    s.part(6, 0, (0, 1.6, 0.6), "Tail")
    s.seg((0, 1.6, 0.6), (0, 0.5, 2.8), 0.9, 0.9, "P", part=6, kind="Ellipsoid", over=0.5)
    for (y, z) in ((1.3, 1.2), (0.85, 2.1)):
        s.el((0, y, z), (1.0, 0.45, 0.45), "D", rot=(-60, 0, 0), part=6)
    # the huge gun, carried on the right arm
    x, y, _ = hand_c(h, R)
    s.bx((x - 0.6, y + 0.1, -0.4), (1.1, 1.1, 3.2), "D", part=4)
    s.cz((x - 0.6, y + 0.1, -2.6), 0.8, 1.6, "A", part=4)
    s.cz((x - 0.6, y + 0.8, -1.6), 0.5, 2.0, "C", part=4)
    s.bx((x - 0.6, y - 0.6, 0.6), (0.5, 0.9, 0.5), "D", part=4)
    return check(s)


def ultron(k=1.0):
    s, h = hero(k=k, tw=3.2, td=1.8, lw=1.1, aw=1.0, tt="P", ta="A", tl="A", th="A", hd=2.0, hw=1.8)
    s.cone((0, h["hc"] - 1.2 * k, h["hz"]), 1.6 * k, 1.0 * k, "A", "-y", part=5)  # pointed jaw
    eyes(s, h, "C", dx=0.45 * k, y=0.25 * k, w=0.6 * k, ht=0.3 * k, slant=15)
    face(s, h, "C", 0, -0.45 * k, (0.9 * k, 0.2 * k, 0.2))
    chest(s, h, "C", (0, -1.0 * k), (1.0 * k, 1.0 * k, 0.3))
    for sx in SIDES:
        chest(s, h, "D", (sx * 1.0 * k, -1.6 * k), (0.3 * k, 1.6 * k, 0.2), rot=(0, 0, sx * 25))
    pads(s, h, "A", ht=0.8 * k)
    boots(s, h, "D", 0.8 * k)
    return check(s)


def kang():
    s, h = hero(tt="A", ta="D", tl="A", th="D")
    boots(s, h, "D", 1.8)
    gloves(s, h, "D", 1.3)
    belt(s, h, "D")
    # big collar + helmet fins; the blue face mask
    for sx in SIDES:
        s.bx((sx * 1.3, h["top"] + 0.6, 0.3), (0.5, 1.8, 1.6), "D", rot=(0, 0, sx * -20))
        s.bx((sx * (h["hw"] / 2 + 0.25), h["hc"] + 0.4, h["hz"]), (0.45, 2.2, 1.6), "D", part=5)
    face(s, h, "P", 0, -0.05, (1.5, 1.4, 0.2))
    eyes(s, h, "C", dx=0.35, y=0.2, w=0.35, ht=0.25)
    chest(s, h, "C", (0, -1.2), (1.4, 0.4, 0.2))
    x, y, _ = hand_c(h, R)
    s.cz((x, y, -0.8), 0.8, 2.0, "C", part=4)  # time blaster
    return check(s)


def doctor_doom():
    s, h = hero(tt="P", ta="P", tl="P", th="P")
    boots(s, h, "D", 1.3)
    gloves(s, h, "D", 1.3)
    s.bx((0, h["hip"] + 0.8, 0), (h["tw"] + 0.4, 2.2, h["td"] + 0.4), "A")  # tunic
    belt(s, h, "D")
    cape(s, h, 6, "A", length=h["top"] - 0.3, width=h["tw"] + 1.0)
    s.bx((0, h["hc"] + 0.3, 0.25), (h["hw"] + 0.5, h["hh"] + 0.6, h["hd"] + 0.3), "A", part=5)  # hood
    face(s, h, "D", 0, 0.0, (1.5, 1.6, 0.2))                                                   # mask
    eyes(s, h, "C", dx=0.4, y=0.3, w=0.4, ht=0.2)
    face(s, h, "C", 0, -0.35, (0.8, 0.15, 0.3))
    for sx in SIDES:
        face(s, h, "C", sx * 0.2, -0.55, (0.12, 0.6, 0.3))
    return check(s)


def red_skull():
    s, h = hero(tt="D", ta="D", tl="D", th="A", hd=2.1)
    boots(s, h, "P", 1.8)
    gloves(s, h, "P", 1.3)
    belt(s, h, "P")
    pads(s, h, "P", ht=0.5)
    for sx in SIDES:
        face(s, h, "D", sx * 0.45, 0.2, (0.6, 0.6, 0.25))
    face(s, h, "D", 0, -0.2, (0.3, 0.4, 0.25))
    face(s, h, "C", 0, -0.65, (1.1, 0.25, 0.25))
    s.el((0, h["hc"] + 0.6, h["hz"]), (h["hw"] + 0.2, 1.0, h["hd"] + 0.1), "A", part=5)  # skull cap
    chest(s, h, "A", (0, -1.2), (1.0, 1.0, 0.2))
    return check(s)


def moon_knight():
    s, h = hero(tt="A", ta="A", tl="A", th="A")
    boots(s, h, "P", 1.2)
    gloves(s, h, "P", 1.2)
    belt(s, h, "P")
    cape(s, h, 6, "A", length=h["top"] + 0.2, width=h["tw"] + 1.8, flare=0.8, top_off=-0.3)
    s.bx((0, h["hc"] + 0.2, 0.4), (h["hw"] + 0.4, h["hh"] + 0.6, h["hd"] + 0.2), "A", part=5)  # hood
    eyes(s, h, "C", dx=0.4, y=0.15, w=0.55, ht=0.25)
    # crescent moon on the chest (two offset boxes)
    chest(s, h, "C", (-0.25, -1.0), (0.4, 1.4, 0.2))
    chest(s, h, "C", (0.05, -0.35), (0.7, 0.3, 0.2), rot=(0, 0, -20))
    chest(s, h, "C", (0.05, -1.65), (0.7, 0.3, 0.2), rot=(0, 0, 20))
    return check(s)


def mad_titan():
    s, h = hero(k=1.25, tw=3.8, td=2.3, lw=1.5, aw=1.4, head=2.0, tt="A", ta="P", tl="A", th="P")
    boots(s, h, "A", 2.0)
    gloves(s, h, "A", 1.6)
    belt(s, h, "D", 0.7)
    for y in (0.4, 0.9, 1.4):  # chin ridges
        face(s, h, "D", 0, -1.0 + y * 0.4, (1.4, 0.18, 0.2))
    for sx in SIDES:
        s.bx((sx * (h["hw"] / 2 + 0.2), h["hc"] + 0.6, h["hz"]), (0.5, 2.6, 1.6), "A", part=5)
        s.cone((sx * (h["hw"] / 2 + 0.3), h["hc"] + 2.2, h["hz"]), 0.6, 1.0, "A", "+y", part=5)
    eyes(s, h, "C", dx=0.5, y=0.3, w=0.45, ht=0.2)
    pads(s, h, "A", ht=1.0)
    # the gauntlet (left) with its stones
    x, y, _ = hand_c(h, L)
    s.bx((x, y - 0.1, 0), (1.9, 1.9, 1.9), "A", part=3)
    for dx in (-0.6, -0.2, 0.2, 0.6):
        s.bx((x + dx, y + 0.2, -1.0), (0.3, 0.3, 0.2), "C", part=3)
    return check(s)


def norrin_radd():
    s, h = hero(tw=2.9, td=1.6, lw=1.1, aw=1.0, arm_out=0.5, tt="A", ta="A", tl="A", th="A")
    # the board under the feet (root), standing surf-stance
    s.el((0, -0.6, 0), (3.6, 0.7, 11.0), "A")
    s.bx((0, -0.35, 0), (2.6, 0.3, 8.0), "C")
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.4, ht=0.25)
    chest(s, h, "C", (0, -1.0), (1.2, 0.2, 0.2))
    for i, sx in enumerate(SIDES):
        s.bx((sx * h["ax"], h["hand"] + 0.3, 0), (1.2, 0.5, 1.2), "C", part=3 + i)
    return check(s)


def mister_fantastic():
    s, h = hero(tw=2.9, td=1.7, lw=1.1, aw=0.85, arm=6.2, tt="A", ta="A", tl="A")
    boots(s, h, "D", 1.4)
    gloves(s, h, "D", 1.0)
    belt(s, h, "D")
    s.cz((0, h["shy"] - 0.8, h["cz"] - 0.1), 1.4, 0.2, "C")
    chest(s, h, "A", (0, -0.8), (0.5, 0.8, 0.22))                  # the 4
    for sx in SIDES:
        face(s, h, "C", sx * 0.75, 0.6, (0.3, 0.7, 0.2))           # grey temples
    s.bx((0, h["hc"] + 0.8, 0.1), (h["hw"] + 0.1, 0.5, h["hd"] + 0.1), "D", part=5)
    eyes(s, h, "C", dx=0.4, y=0.15, w=0.35, ht=0.2)
    return check(s)


def sue_storm():
    s, h = hero(tw=2.8, td=1.6, lw=1.05, aw=1.0, torso=3.4, tt="A", ta="A", tl="A")
    boots(s, h, "D", 1.4)
    gloves(s, h, "D", 1.0)
    belt(s, h, "D")
    s.cz((0, h["shy"] - 0.8, h["cz"] - 0.1), 1.4, 0.2, "C")
    s.bx((0, h["hc"] - 0.2, 0.4), (h["hw"] + 0.4, h["hh"] + 1.0, 1.2), "C", part=5)  # blonde hair
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.35, ht=0.2)
    # force-field disc projected from the right hand
    x, y, _ = hand_c(h, R)
    s.cz((x + 0.6, y + 1.4, -2.4), 3.2, 0.3, "C", part=4)
    s.cz((x + 0.6, y + 1.4, -2.5), 2.2, 0.3, "A", part=4)
    return check(s)


def modok():
    s = Shape()
    # the hover chair (root) with jets
    s.bx((0, 2.2, 0.6), (5.2, 1.2, 4.6), "A")
    s.bx((0, 3.6, 2.6), (5.0, 3.6, 0.8), "A")
    for sx in SIDES:
        s.bx((sx * 2.8, 3.2, 0.4), (0.6, 1.6, 4.0), "A")
    s.part(8, 0, (0, 1.4, 0.6), "Exhaust")
    s.cone((0, 0.7, 0.6), 2.2, 1.6, "C", "-y", part=8)
    s.cy((0, 1.4, 0.6), (3.0, 0.6, 3.0), "D")
    # the giant head
    s.part(5, 0, (0, 4.0, 0), "Head")
    s.el((0, 6.2, -0.4), (5.4, 5.6, 5.2), "P", part=5)
    s.bx((0, 7.4, -2.95), (5.0, 0.6, 0.4), "C", part=5)         # headband
    s.bx((0, 7.4, -3.2), (1.0, 1.0, 0.4), "A", part=5)
    s.bx((0, 4.9, -2.9), (2.6, 0.7, 0.3), "D", part=5)          # grimace
    s.bx((0, 4.9, -3.0), (2.2, 0.25, 0.3), "C", part=5)
    for sx in SIDES:
        s.bx((sx * 0.9, 6.3, -2.85), (0.9, 0.35, 0.3), "C", part=5)
    # stubby arms
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 2.3, 3.6, -1.0), "ArmLeft" if sx < 0 else "ArmRight")
        s.bx((sx * 2.4, 3.2, -1.6), (0.6, 0.6, 1.8), "P", part=pid)
    s.seat(0, (0, 3.0, 0.4))
    return check(s)


def hulk(k=1.0, cracks=False):
    s = Shape()
    K = lambda t: tuple(a * k for a in t)
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, K((sx * 2.0, 6.0, 0.4)), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx(K((sx * 2.1, 3.1, 0.4)), K((2.4, 6.2, 2.4)), "P", part=pid)
        s.bx(K((sx * 2.1, 0.5, -0.1)), K((2.7, 1.0, 3.2)), "D", part=pid)
    s.bx(K((0, 6.4, 0.4)), K((5.2, 1.8, 3.0)), "A")                       # torn shorts
    s.el(K((0, 9.2, 0.2)), K((6.8, 6.0, 4.4)), "P", rot=(-12, 0, 0))       # barrel chest
    s.el(K((0, 10.6, -1.6)), K((5.0, 3.0, 1.4)), "P", rot=(-12, 0, 0))     # pecs
    for sx in SIDES:
        s.el(K((sx * 3.1, 11.6, 0.4)), K((3.0, 2.4, 3.0)), "P")            # traps/shoulders
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, K((sx * 3.8, 11.6, 0.2)), "ArmLeft" if sx < 0 else "ArmRight")
        s.seg(K((sx * 4.1, 11.4, 0.2)), K((sx * 4.5, 7.6, -0.6)), 2.4 * k, 2.4 * k, "P", part=pid, kind="Ellipsoid")
        s.seg(K((sx * 4.5, 7.6, -0.6)), K((sx * 4.6, 4.4, -1.2)), 2.2 * k, 2.2 * k, "P", part=pid, kind="Ellipsoid")
        s.bx(K((sx * 4.6, 3.3, -1.3)), K((2.6, 2.4, 2.6)), "D", part=pid)  # fists
    s.part(5, 0, K((0, 12.0, -1.2)), "Head")
    s.bx(K((0, 13.0, -1.9)), K((2.2, 2.0, 2.0)), "P", part=5)
    s.bx(K((0, 14.1, -1.7)), K((2.4, 0.6, 2.2)), "D", part=5)             # hair
    s.bx(K((0, 13.3, -2.95)), K((1.8, 0.3, 0.2)), "D", part=5)            # brow
    for sx in SIDES:
        s.bx(K((sx * 0.45, 13.0, -2.95)), K((0.4, 0.25, 0.2)), "C", part=5)
    s.part(6, 5, K((0, 12.6, -2.0)), "Jaw")
    s.bx(K((0, 12.3, -2.3)), K((1.9, 0.7, 1.6)), "P", part=6)
    s.bx(K((0, 12.5, -3.05)), K((1.2, 0.25, 0.2)), "C", part=6)
    if cracks:  # world-breaker: glowing fissures
        for (x, y, r) in ((-1.2, 9.6, 30), (1.4, 8.4, -25), (0.2, 10.8, 70)):
            s.bx(K((x, y, -2.1)), K((0.3, 1.8, 0.3)), "C", rot=(0, 0, r))
        for sx in SIDES:
            s.bx(K((sx * 4.5, 7.0, -1.9)), K((0.3, 1.6, 0.3)), "C", part=3 if sx < 0 else 4)
    return check(s)


def she_hulk():
    s, h = hero(k=1.05, tw=3.3, td=1.9, lw=1.3, aw=1.2, tt="A", ta="P", tl="P")
    boots(s, h, "A", 1.6)
    s.bx((0, h["hip"] + 0.2, 0), (h["tw"] + 0.2, 1.2, h["td"] + 0.2), "A")
    s.bx((0, h["hc"] - 0.4, 0.45), (h["hw"] + 0.5, h["hh"] + 1.8, 1.3), "D", part=5)  # long dark hair
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.35, ht=0.2)
    return check(s)


def big_gun(s, h, side, ln=4.0, t="D"):
    x, y, _ = hand_c(h, side)
    sx = SIDES[side]
    s.bx((x - sx * 0.3, y + 0.3, -0.8), (1.2, 1.4, 3.4), t, part=3 + side)
    s.cz((x - sx * 0.3, y + 0.5, -2.9 - ln / 4), 0.7, ln / 2, "C", part=3 + side)
    s.bx((x - sx * 0.3, y - 0.5, 0.4), (0.5, 1.0, 0.6), t, part=3 + side)


# ------------------------------------------------------------------ Spider-Verse + rogues

def spider_suit(s, h, lens=True):
    if lens:
        eyes(s, h, "C", dx=0.45, y=0.2, w=0.75, ht=0.55, slant=18, kind="el")
    # web lines on the mask + chest spider
    face(s, h, "D", 0, 0.0, (0.12, h["hh"] - 0.2, 0.2))
    chest(s, h, "D", (0, -1.0), (0.45, 0.9, 0.2))
    for sx in SIDES:
        chest(s, h, "D", (sx * 0.45, -0.8), (0.12, 1.2, 0.2), rot=(0, 0, sx * 40))
        chest(s, h, "D", (sx * 0.45, -1.3), (0.12, 1.2, 0.2), rot=(0, 0, sx * -40))


def peter_parker():
    s, h = hero(tw=3.0, td=1.7, lw=1.15, aw=1.05, tt="A", ta="P", tl="P", th="A")
    boots(s, h, "A", 1.6)
    gloves(s, h, "A", 1.2)
    s.bx((0, h["hip"] + 0.7, 0), (h["tw"] + 0.15, 1.4, h["td"] + 0.15), "P")
    spider_suit(s, h)
    # a web line thwipping from the right wrist
    x, y, _ = hand_c(h, R)
    s.seg((x, y, -0.4), (x + 2.4, y + 6.0, -3.0), 0.25, 0.25, "C", part=4, kind="Cylinder")
    return check(s)


def spider_gwen():
    s, h = hero(tw=2.8, td=1.6, lw=1.05, aw=1.0, torso=3.4, tt="C", ta="P", tl="P", th="C")
    boots(s, h, "A", 1.6)
    gloves(s, h, "A", 1.0)
    s.el((0, h["hc"] + 0.15, 0.35), (h["hw"] + 0.5, h["hh"] + 0.5, h["hd"] + 0.3), "C", part=5)  # the hood
    face(s, h, "P", 0, -0.05, (h["hw"] - 0.3, h["hh"] - 0.5, 0.25))                              # mask
    for sx in SIDES:  # web lines on the arms
        s.bx((sx * h["ax"], h["shy"] - 0.9, -0.55), (0.12, 1.4, 0.12), "A", part=3 if sx < 0 else 4)
    spider_suit(s, h)
    return check(s)


def spider_2099():
    s, h = hero(tw=3.0, td=1.7, lw=1.15, aw=1.05, tt="A", ta="A", tl="A", th="A")
    boots(s, h, "P", 1.4)
    face(s, h, "P", 0, -0.1, (1.6, 1.6, 0.18))            # the red skull-face
    eyes(s, h, "C", dx=0.45, y=0.25, w=0.7, ht=0.45, slant=22, kind="el")
    chest(s, h, "P", (0, -1.0), (2.2, 2.0, 0.18))
    chest(s, h, "C", (0, -1.0), (0.4, 1.4, 0.24))
    for i, sx in enumerate(SIDES):  # the under-arm gliding capes
        s.bx((sx * (h["ax"] + 0.3), h["shy"] - 1.8, 0.3), (0.2, 3.0, 1.5), "P", rot=(0, 0, sx * 12), part=3 + i)
        for dx in (-0.3, 0.3):  # talons
            s.cone((sx * h["ax"] + dx, h["hand"] - 0.4, -0.3), 0.25, 0.8, "C", "-y", part=3 + i)
    return check(s)


def venom(k=1.0, carnage=False):
    s, h = hero(k=k, tw=4.0, td=2.4, lw=1.5, aw=1.5, leg=4.6, torso=4.0, head=2.2, hw=2.4, hd=2.4,
                tt="P", ta="P", tl="P", th="P", head_z=-0.4)
    eyes(s, h, "C", dx=0.6 * k, y=0.35 * k, w=1.0 * k, ht=0.55 * k, slant=28, kind="el")
    s.part(6, 5, (0, h["hc"] - 0.3 * k, h["fz"] + 0.6), "Jaw")
    s.bx((0, h["hc"] - 0.75 * k, h["fz"] - 0.3), (1.9 * k, 0.6 * k, 0.9 * k), "D", part=6)
    for dx in (-0.6, -0.2, 0.2, 0.6):
        s.cone((dx * k, h["hc"] - 0.45 * k, h["fz"] - 0.55), 0.3 * k, 0.5 * k, "C", "-y", part=6)
    if not carnage:
        s.bx((0, h["hc"] - 1.2 * k, h["fz"] - 0.9), (0.5 * k, 0.25 * k, 1.8 * k), "A", rot=(25, 0, 0), part=6)  # tongue
        # the white spider emblem
        chest(s, h, "A", (0, -1.2 * k), (0.9 * k, 1.4 * k, 0.2))
        for sx in SIDES:
            chest(s, h, "A", (sx * 0.9 * k, -0.6 * k), (0.4 * k, 1.8 * k, 0.2), rot=(0, 0, sx * 45))
            chest(s, h, "A", (sx * 0.8 * k, -2.0 * k), (0.4 * k, 1.8 * k, 0.2), rot=(0, 0, sx * -30))
    # back tendrils (Tail parts)
    base = h["top"] - 1.0 * k
    tips = [(-2.8, base + 2.4, 3.6), (2.8, base + 2.6, 3.4)] if not carnage else \
        [(-3.2, base + 2.8, 3.2), (3.2, base + 2.4, 3.6), (-1.6, base + 3.8, 3.8), (1.8, base - 1.6, 4.0)]
    for n, (tx, ty, tz) in enumerate(tips):
        pid = s.part(10 + n, 0, (tx * 0.2, base, h["td"] / 2), "Tail")
        mid = (tx * 0.6, (base + ty) / 2 + 0.6 * k, h["td"] / 2 + tz * 0.5)
        s.seg((tx * 0.2, base, h["td"] / 2), mid, 1.0 * k, 1.0 * k, "D" if not carnage else "A", part=pid, kind="Ellipsoid")
        s.seg(mid, (tx * k, ty, h["td"] / 2 + tz * k), 0.8 * k, 0.8 * k, "D" if not carnage else "A", part=pid, kind="Ellipsoid")
        s.cone((tx * k, ty + 0.5 * k, h["td"] / 2 + tz * k), 0.8 * k, 1.2 * k, "C", "+y", part=pid)
    if carnage:  # blade arm
        x, y, _ = hand_c(h, R)
        s.bx((x, y - 1.6 * k, -0.4), (0.4 * k, 3.2 * k, 1.1 * k), "C", part=4)
        s.cone((x, y - 3.6 * k, -0.4), 0.8 * k, 1.0 * k, "C", "-y", part=4)
        for (x2, y2, r) in ((-1.0, -0.6, 30), (0.9, -1.5, -40), (0.1, -2.4, 70)):
            chest(s, h, "A", (x2 * k, y2 * k), (0.3 * k, 1.6 * k, 0.2), rot=(0, 0, r))
    return check(s)


def green_goblin():
    s, h = hero(tw=2.9, td=1.7, lw=1.1, aw=1.05, tt="P", ta="P", tl="P", th="P")
    # he rides the glider: the glider is the root under the feet
    s.bx((0, -0.5, 0), (3.2, 0.7, 4.4), "D")
    for sx in SIDES:
        s.bx((sx * 3.3, -0.5, 0.4), (3.8, 0.4, 2.8), "D", rot=(0, sx * -10, sx * 8))
        s.bx((sx * 5.0, -0.2, 0.9), (1.4, 0.4, 1.4), "D", rot=(0, 45, sx * 8))
        s.cone((sx * 0.8, -0.7, 2.6), 0.8, 1.0, "C", "+z")
    s.bx((0, h["hip"] + 1.2, 0), (h["tw"] + 0.3, 2.6, h["td"] + 0.3), "A")   # purple tunic
    s.bx((0, h["top"] - 0.4, 0.5), (h["tw"] + 0.3, 0.8, 1.2), "A")          # satchel strap
    s.cone((0, h["hc"] + 1.6, h["hz"] + 0.5), 2.2, 2.2, "A", "+y", part=5)     # the hat
    for sx in SIDES:
        s.cone((sx * 1.2, h["hc"] + 0.3, h["hz"]), 0.6, 1.0, "P", "+x" if sx > 0 else "-x", part=5)  # ears
    eyes(s, h, "C", dx=0.45, y=0.2, w=0.5, ht=0.35, slant=-20)
    face(s, h, "C", 0, -0.5, (1.1, 0.3, 0.2))                                 # grin
    # a pumpkin bomb in the right hand
    x, y, _ = hand_c(h, R)
    s.el((x, y - 0.5, -0.2), (1.2, 1.1, 1.2), "C", part=4)
    return check(s)


def doctor_octopus():
    s, h = hero(tw=3.2, td=2.0, tt="A", ta="A", tl="D")
    boots(s, h, "D", 1.2)
    belt(s, h, "D", 0.8)
    for sx in SIDES:
        face(s, h, "C", sx * 0.45, 0.2, (0.6, 0.5, 0.3))  # goggles
    s.bx((0, h["hc"] + 0.8, 0.1), (h["hw"] + 0.1, 0.5, h["hd"] + 0.1), "D", part=5)
    # four arms from the harness
    base = (0, h["hip"] + 1.0, h["td"] / 2 + 0.2)
    ends = [(-4.4, h["top"] + 3.2, 1.0), (4.4, h["top"] + 3.2, 1.0), (-4.6, 1.5, -0.4), (4.6, 1.5, -0.4)]
    for n, e in enumerate(ends):
        pid = s.part(10 + n, 0, base, "Tail")
        mid = (e[0] * 0.55, (base[1] + e[1]) / 2 + 1.4, 1.8)
        s.seg(base, mid, 1.0, 1.0, "D", part=pid, kind="Cylinder")
        s.seg(mid, e, 0.9, 0.9, "D", part=pid, kind="Cylinder")
        s.el(mid, (1.2, 1.2, 1.2), "A", part=pid)
        for dx in (-0.45, 0.45):  # pronged claw
            s.cone((e[0] + dx, e[1], e[2]), 0.5, 1.2, "C", "+y" if e[1] > 4 else "-y", part=pid)
        s.el(e, (1.3, 1.3, 1.3), "C", part=pid)
    s.bx(base, (2.6, 1.2, 0.8), "D")
    return check(s)


def rhino():
    s, h = hero(k=1.15, tw=4.0, td=2.4, lw=1.6, aw=1.5, head=2.0, hw=2.4, hd=2.8, tt="A", ta="A", tl="A", th="A", head_z=-0.4)
    boots(s, h, "D", 1.2)
    gloves(s, h, "D", 1.4)
    s.cone((0, h["hc"] + 0.4, h["fz"] - 1.1), 1.0, 2.4, "C", "-z", part=5)    # the horn
    s.cone((0, h["hc"] + 1.2, h["fz"] - 0.3), 0.6, 1.2, "A", "-z", part=5)
    for sx in SIDES:
        s.cone((sx * 1.1, h["hc"] + 1.3, h["hz"] + 0.6), 0.5, 0.8, "A", "+y", part=5)
    face(s, h, "D", 0, -0.2, (1.6, 1.0, 0.2))
    eyes(s, h, "C", dx=0.4, y=0.0, w=0.35, ht=0.2)
    for x in (-1.0, 0.0, 1.0):
        chest(s, h, "D", (x, -1.4), (0.3, 2.0, 0.2))
    return check(s)


def electro():
    s, h = hero(tt="P", ta="A", tl="P", th="A")
    boots(s, h, "A", 1.6)
    gloves(s, h, "A", 1.3)
    # the star-burst mask spikes
    for n, (x, y, r) in enumerate(((-1.3, 1.1, 40), (1.3, 1.1, -40), (-0.6, 1.6, 15), (0.6, 1.6, -15), (-1.4, 0.2, 70), (1.4, 0.2, -70))):
        s.bx((x, h["hc"] + y, h["fz"] + 0.1), (0.45, 1.6, 0.3), "A", rot=(0, 0, r), part=5)
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.45, ht=0.25)
    # lightning zigzags down the chest + sparks from the hands
    for (x, y, r) in ((-0.3, -0.4, 30), (0.2, -1.3, -30), (-0.2, -2.2, 30)):
        chest(s, h, "A", (x, y), (0.4, 1.1, 0.2), rot=(0, 0, r))
    for i, sx in enumerate(SIDES):
        s.bx((sx * (h["ax"] + 0.3), h["hand"] - 0.6, 0), (0.3, 1.4, 0.3), "C", rot=(0, 0, sx * 30), part=3 + i)
    return check(s)


def mysterio():
    s, h = hero(tt="P", ta="P", tl="P", head=2.0)
    boots(s, h, "D", 1.6)
    gloves(s, h, "D", 1.2)
    cape(s, h, 6, "A", length=h["top"] - 0.2, width=h["tw"] + 1.6, flare=0.6)
    for x in (-0.9, 0, 0.9):  # scale-mail stripes
        chest(s, h, "A", (x, -1.2), (0.35, 2.2, 0.2))
    s.bx((0, h["top"] - 0.25, 0), (h["tw"] + 0.2, 0.5, h["td"] + 0.2), "A")
    s.el((0, h["hc"] + 0.2, h["hz"]), (h["hw"] + 1.4, h["hh"] + 1.4, h["hw"] + 1.4), "C", part=5)  # fishbowl
    return check(s)


def vulture():
    s, h = hero(tw=2.9, td=1.7, lw=1.1, aw=1.0, tt="A", ta="A", tl="A", th="P")
    boots(s, h, "D", 1.2)
    s.bx((0, h["top"] + 0.1, 0.2), (2.4, 0.9, 2.0), "D")  # feathered collar
    face(s, h, "D", 0, 0.0, (1.3, 0.5, 0.3))
    s.cone((0, h["hc"] - 0.2, h["fz"] - 0.5), 0.5, 0.9, "P", "-z", part=5)
    for i, sx in enumerate(SIDES):
        pid = s.part(7 + i, 0, (sx * 1.0, h["top"] - 0.4, h["td"] / 2 + 0.3), "WingLeft" if sx < 0 else "WingRight")
        s.bx((sx * 4.6, h["top"] + 0.4, h["td"] / 2 + 0.5), (7.6, 0.5, 3.0), "A", rot=(0, 0, sx * -12), part=pid)
        for n in range(4):
            s.bx((sx * (2.6 + n * 1.8), h["top"] - 1.6 + n * 0.2, h["td"] / 2 + 0.5), (1.2, 3.2 - n * 0.3, 0.4), "D",
                 rot=(0, 0, sx * -8), part=pid)
    return check(s)


# ------------------------------------------------------------------ X-Men

def wolverine():
    s, h = hero(k=0.95, tw=3.4, td=2.0, lw=1.35, aw=1.3, leg=4.0, tt="A", ta="P", tl="A", th="P")
    boots(s, h, "P", 1.8)
    gloves(s, h, "P", 1.3)
    belt(s, h, "D", 0.6)
    s.bx((0, h["hip"] + 1.0, h["cz"] - 0.1), (h["tw"] - 0.2, 1.4, 0.2), "P")  # the side panels
    pads(s, h, "P", ht=0.6)
    for sx in SIDES:                                                        # the cowl fins
        s.bx((sx * 1.1, h["hc"] + 1.0, h["hz"] + 0.1), (0.5, 2.2, 1.2), "P", rot=(0, 0, sx * -32), part=5)
        s.bx((sx * 1.0, h["hc"] + 0.1, h["fz"] + 0.2), (0.5, 1.0, 0.4), "P", rot=(0, 0, sx * -20), part=5)
    face(s, h, "P", 0, 0.25, (1.8, 0.8, 0.2))
    eyes(s, h, "C", dx=0.45, y=0.25, w=0.55, ht=0.25, slant=-15)
    face(s, h, "D", 0, -0.55, (1.3, 0.6, 0.2))                               # the jaw + sideburns
    for i, sx in enumerate(SIDES):
        for dx in (-0.35, 0.0, 0.35):
            s.bx((sx * h["ax"] + dx, h["hand"] - 1.1, -0.35), (0.18, 2.4, 0.3), "C", part=3 + i)
    return check(s)


def cyclops():
    s, h = hero(tt="A", ta="A", tl="A", th="A")
    boots(s, h, "P", 1.6)
    gloves(s, h, "P", 1.3)
    belt(s, h, "P", 0.6)
    s.bx((0, h["hip"] + 0.3, h["cz"] - 0.2), (0.8, 0.8, 0.2), "C", rot=(0, 0, 45))  # X buckle
    for sx in SIDES:
        chest(s, h, "P", (sx * 0.7, -0.8), (0.4, 2.4, 0.18), rot=(0, 0, sx * 20))  # straps
    face(s, h, "C", 0, 0.2, (h["hw"] + 0.2, 0.55, 0.3))                         # the visor
    face(s, h, "D", 0, -0.55, (1.3, 0.6, 0.2))
    return check(s)


def jean_grey():
    s, h = hero(tw=2.8, td=1.6, lw=1.05, aw=1.0, torso=3.4, tt="P", ta="P", tl="P")
    boots(s, h, "C", 1.8)
    gloves(s, h, "C", 1.3)
    belt(s, h, "C")
    s.el((0, h["hc"] - 0.1, 0.5), (h["hw"] + 1.2, h["hh"] + 1.8, 1.8), "A", part=5)  # big red hair
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.35, ht=0.2)
    # the phoenix emblem
    chest(s, h, "C", (0, -1.1), (0.5, 1.6, 0.2))
    for sx in SIDES:
        chest(s, h, "C", (sx * 0.7, -0.6), (1.2, 0.35, 0.2), rot=(0, 0, sx * 25))
    for i, sx in enumerate(SIDES):
        s.el((sx * h["ax"], h["hand"] - 0.2, -0.3), (1.2, 1.2, 1.2), "C", part=3 + i)  # telekinesis
    return check(s)


def storm():
    s, h = hero(tw=2.8, td=1.6, lw=1.05, aw=1.0, torso=3.4, arm_out=0.3, tt="D", ta="D", tl="D")
    boots(s, h, "D", 2.0)
    # the wide cape (arms-to-back)
    s.part(6, 0, (0, h["top"], h["td"] / 2 + 0.2), "Cloth")
    s.bx((0, h["top"] - 3.2, h["td"] / 2 + 0.4), (h["tw"] + 4.0, 6.4, 0.3), "A", rot=(-6, 0, 0), part=6)
    s.bx((0, h["hc"] - 0.4, 0.5), (h["hw"] + 0.8, h["hh"] + 2.2, 1.3), "A", part=5)   # long white hair
    face(s, h, "C", 0, 0.75, (1.4, 0.25, 0.2))                                       # tiara
    eyes(s, h, "C", dx=0.4, y=0.15, w=0.45, ht=0.3)
    for i, sx in enumerate(SIDES):  # lightning in both hands
        s.bx((sx * (h["ax"] + 0.2), h["hand"] - 0.8, 0), (0.35, 1.4, 0.35), "C", rot=(0, 0, sx * 25), part=3 + i)
        s.bx((sx * (h["ax"] + 0.1), h["hand"] - 1.9, 0), (0.35, 1.2, 0.35), "C", rot=(0, 0, sx * -25), part=3 + i)
    return check(s)


def magneto():
    s, h = hero(tt="A", ta="A", tl="A", th="A")
    boots(s, h, "P", 1.8)
    gloves(s, h, "P", 1.3)
    belt(s, h, "D")
    cape(s, h, 6, "P", length=h["top"] - 0.3, width=h["tw"] + 1.6, flare=0.5)
    chest(s, h, "P", (0, -0.6), (2.4, 0.3, 0.2), rot=(0, 0, 0))
    for sx in SIDES:
        chest(s, h, "P", (sx * 0.6, -1.3), (0.3, 1.6, 0.2), rot=(0, 0, sx * -25))
    # the helmet: dome + front crest + side flares; face opening
    s.el((0, h["hc"] + 0.5, h["hz"] + 0.2), (h["hw"] + 0.7, h["hh"] + 0.9, h["hd"] + 0.6), "A", part=5)
    s.bx((0, h["hc"] + 1.4, h["fz"] - 0.2), (0.5, 1.4, 0.8), "A", part=5)
    for sx in SIDES:
        s.bx((sx * (h["hw"] / 2 + 0.45), h["hc"] - 0.2, h["hz"]), (0.35, 2.0, 1.6), "A", part=5)
    face(s, h, "P", 0, -0.35, (1.3, 1.2, 0.3))
    eyes(s, h, "C", dx=0.35, y=0.0, w=0.35, ht=0.2)
    return check(s)


def nightcrawler():
    s, h = hero(k=0.95, leg=3.6, tw=2.8, td=1.6, lw=1.05, aw=0.95, arm=4.0, tt="D", ta="A", tl="A", th="A", head_z=-0.5)
    boots(s, h, "D", 1.2)
    for sx in SIDES:
        chest(s, h, "A", (sx * 0.8, -1.2), (0.6, 2.8, 0.2), rot=(0, 0, sx * -15))  # the V
        s.cone((sx * (h["hw"] / 2 + 0.3), h["hc"] + 0.3, h["hz"]), 0.6, 1.2, "A", "+x" if sx > 0 else "-x", part=5)  # pointed ears
    eyes(s, h, "C", dx=0.45, y=0.1, w=0.5, ht=0.35, kind="el")
    for i, sx in enumerate(SIDES):  # three-fingered hands
        for dx in (-0.3, 0.3):
            s.bx((sx * h["ax"] + dx, h["hand"] - 0.4, 0), (0.25, 0.8, 0.3), "A", part=3 + i)
    # the arrow-tipped tail
    s.part(7, 0, (0, h["hip"] + 0.2, h["td"] / 2), "Tail")
    s.seg((0, h["hip"] + 0.2, h["td"] / 2), (0, 1.8, 3.6), 0.45, 0.45, "A", part=7, kind="Cylinder")
    s.seg((0, 1.8, 3.6), (0, 3.6, 5.6), 0.4, 0.4, "A", part=7, kind="Cylinder")
    s.bx((0, 4.0, 5.9), (0.3, 1.1, 1.1), "A", rot=(45, 0, 0), part=7)
    return check(s)


def colossus():
    s, h = hero(k=1.2, tw=3.9, td=2.3, lw=1.55, aw=1.5, tt="A", ta="A", tl="A", th="A")
    boots(s, h, "P", 1.8)
    gloves(s, h, "A", 1.4, grow=0.45)
    s.bx((0, h["top"] - 1.4, h["cz"] - 0.1), (h["tw"] - 0.4, 2.8, 0.2), "P")  # red shirt
    belt(s, h, "D", 0.7)
    for y in (0.8, 1.8, 2.8):  # the steel plate seams
        for i, sx in enumerate(SIDES):
            s.bx((sx * h["ax"], h["shy"] - y, 0), (h["aw"] + 0.06, 0.15, h["aw"] + 0.06), "D", part=3 + i)
            s.bx((sx * h["lx"], h["hip"] - y, 0), (h["lw"] + 0.06, 0.15, h["lw"] + 0.06), "D", part=1 + i)
    face(s, h, "D", 0, 0.9, (h["hw"], 0.4, 0.2))
    eyes(s, h, "C", dx=0.45, y=0.2, w=0.4, ht=0.2)
    return check(s)


def iceman():
    s, h = hero(tt="A", ta="A", tl="A", th="A")
    # frosty crystal spikes
    for (x, y, r) in ((-0.8, 1.2, 20), (0.0, 1.5, 0), (0.8, 1.2, -20)):
        s.cone((x, h["hc"] + y, h["hz"] + 0.3), 0.7, 1.2, "C", "+y", part=5)
    for sx in SIDES:
        s.cone((sx * (h["tw"] / 2 + 0.3), h["top"] + 0.4, 0.3), 0.8, 1.4, "C", "+y")
    eyes(s, h, "C", dx=0.4, y=0.15, w=0.45, ht=0.25)
    chest(s, h, "C", (0, -1.2), (1.6, 0.2, 0.2), rot=(0, 0, 30))
    # the ice slide he surfs on (root)
    s.bx((0, -0.35, 0.8), (3.2, 0.7, 7.0), "C", rot=(-8, 0, 0))
    s.bx((0, -1.1, 3.6), (1.4, 1.4, 1.2), "A", rot=(-8, 0, 0))
    return check(s)


def gambit():
    s, h = hero(tt="D", ta="D", tl="D", th="A")
    boots(s, h, "D", 1.6)
    s.bx((0, h["top"] - 1.7, 0), (h["tw"] + 0.3, 3.4, h["td"] + 0.3), "P")  # coat body
    s.part(6, 0, (0, h["hip"] + 0.6, 0.4), "Cloth")                         # coat tails
    s.bx((0, h["hip"] - 1.6, 0.8), (h["tw"] + 1.2, 4.2, 0.35), "P", rot=(-8, 0, 0), part=6)
    face(s, h, "D", 0, -0.2, (1.4, 1.2, 0.2))
    eyes(s, h, "A", dx=0.4, y=0.15, w=0.4, ht=0.25)
    s.bx((0, h["hc"] + 0.3, 0.6), (h["hw"] + 0.2, h["hh"] + 0.8, 0.8), "D", part=5)
    # bo staff (right) and a charged card (left)
    x, y, _ = hand_c(h, R)
    s.cy((x, y + 0.5, 0), (0.4, 10.0, 0.4), "A", rot=(20, 0, 0), part=4)
    x, y, _ = hand_c(h, L)
    s.bx((x, y - 0.4, -0.6), (0.9, 1.3, 0.12), "C", rot=(0, 0, 20), part=3)
    return check(s)


def rogue():
    s, h = hero(tw=2.8, td=1.6, lw=1.05, aw=1.0, torso=3.4, tt="A", ta="A", tl="A")
    boots(s, h, "C", 1.4)
    gloves(s, h, "C", 1.4)
    s.bx((0, h["top"] - 1.1, 0), (h["tw"] + 0.25, 2.2, h["td"] + 0.25), "D")  # jacket
    belt(s, h, "D")
    s.bx((0, h["hc"] - 0.2, 0.5), (h["hw"] + 0.7, h["hh"] + 1.4, 1.3), "D", part=5)  # hair
    for sx in SIDES:
        face(s, h, "C", sx * 0.75, 0.6, (0.4, 1.1, 0.25))                           # white streaks
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.35, ht=0.2)
    return check(s)


def beast():
    s = Shape()
    h = humanoid(s, 3.4, 3.8, 1.9, 3.4, 2.4, 1.5, 1.5, 5.8, tt="A", ta="A", tl="A", th="A", hw=2.2, head_z=-1.0,
                 arm_out=0.35)
    for i, sx in enumerate(SIDES):  # big feet + fists
        s.bx((sx * h["lx"], 0.4, -0.6), (1.8, 0.8, 2.8), "A", part=1 + i)
        s.bx((sx * h["ax"], h["hand"] - 0.1, -0.3), (1.9, 1.5, 1.9), "D", part=3 + i)
    s.bx((0, h["hip"] + 0.2, 0), (h["tw"] + 0.2, 1.0, h["td"] + 0.2), "D")      # shorts
    s.el((0, h["top"] - 0.3, 0.5), (h["tw"] + 0.6, 1.8, h["td"]), "A")           # hunched back
    face(s, h, "C", 0, 0.1, (1.5, 0.3, 0.2))                                      # glasses
    face(s, h, "D", 0, -0.55, (1.6, 0.5, 0.25))
    for sx in SIDES:
        s.cone((sx * 1.2, h["hc"] + 0.4, h["hz"]), 0.5, 0.8, "A", "+y", part=5)
    return check(s)


def juggernaut():
    s, h = hero(k=1.3, tw=4.2, td=2.6, lw=1.7, aw=1.6, head=2.1, hw=2.6, hd=2.6, tt="A", ta="A", tl="A", th="A")
    boots(s, h, "D", 1.6)
    gloves(s, h, "D", 1.6, grow=0.4)
    belt(s, h, "D", 0.9)
    s.el((0, h["top"] + 0.2, 0), (h["tw"] + 1.2, 2.0, h["td"] + 0.6), "A")    # massive shoulders
    # the dome helmet with ridges
    s.el((0, h["hc"] + 0.3, h["hz"]), (h["hw"] + 0.8, h["hh"] + 1.4, h["hd"] + 0.6), "A", part=5)
    for dx in (-0.8, 0.0, 0.8):
        s.bx((dx, h["hc"] + 0.8, h["fz"] - 0.2), (0.35, 1.6, 0.4), "D", part=5)
    face(s, h, "D", 0, -0.3, (1.6, 1.0, 0.3))
    eyes(s, h, "C", dx=0.4, y=-0.1, w=0.4, ht=0.2)
    for x in (-1.2, 0.0, 1.2):
        chest(s, h, "D", (x, -1.4), (0.4, 2.8, 0.2))
    return check(s)


def cable():
    s, h = hero(k=1.1, tw=3.6, td=2.1, lw=1.4, aw=1.35, tt="D", ta="P", tl="D")
    boots(s, h, "D", 1.6)
    pads(s, h, "A", ht=1.0)
    belt(s, h, "A", 0.7)
    for sx in SIDES:
        chest(s, h, "A", (sx * 0.4, -1.2), (0.4, 3.2, 0.2), rot=(0, 0, sx * 30))
    s.bx((-h["ax"], h["shy"] - 1.8, 0), (h["aw"] + 0.2, 3.4, h["aw"] + 0.2), "C", part=3)  # techno-organic arm
    s.bx((0, h["hc"] + 0.8, 0.1), (h["hw"] + 0.1, 0.5, h["hd"] + 0.1), "C", part=5)
    s.el((-0.45, h["hc"] + 0.15, h["fz"] - 0.1), (0.7, 0.7, 0.4), "C", part=5)             # the glowing eye
    s.bx((0.45, h["hc"] + 0.15, h["fz"] - 0.05), (0.4, 0.2, 0.2), "D", part=5)
    big_gun(s, h, R, ln=5.0)
    return check(s)


def apocalypse():
    s, h = hero(k=1.35, tw=3.8, td=2.3, lw=1.5, aw=1.4, tt="A", ta="A", tl="A", th="P")
    boots(s, h, "D", 2.0)
    gloves(s, h, "D", 1.6)
    belt(s, h, "D", 0.7)
    pads(s, h, "D", ht=1.2, w=2.6)
    # the tall crested helmet + side tubes
    s.bx((0, h["hc"] + 1.2, h["hz"] + 0.2), (h["hw"] + 0.2, 1.2, h["hd"] + 0.3), "A", part=5)
    s.bx((0, h["hc"] + 2.3, h["hz"] + 0.2), (0.6, 2.0, h["hd"] + 0.4), "A", part=5)
    for sx in SIDES:
        s.cy((sx * (h["hw"] / 2 + 0.5), h["hc"] - 0.2, h["hz"] + 0.2), (0.6, 3.0, 0.6), "D", part=5)
    face(s, h, "D", 0, -0.2, (1.4, 1.4, 0.2))
    eyes(s, h, "C", dx=0.45, y=0.2, w=0.45, ht=0.25)
    for x in (-1.0, 1.0):
        chest(s, h, "D", (x, -1.2), (0.5, 2.8, 0.2))
    return check(s)


def sabretooth():
    s, h = hero(k=1.1, tw=3.6, td=2.1, lw=1.4, aw=1.35, tt="P", ta="P", tl="P", th="P")
    boots(s, h, "D", 1.4)
    s.cy((0, h["top"] + 0.1, 0), (h["tw"] + 0.4, 0.9, h["td"] + 0.8), "A")        # fur collar
    s.bx((0, h["hc"] - 0.2, 0.5), (h["hw"] + 0.6, h["hh"] + 1.4, 1.2), "A", part=5)  # mane
    eyes(s, h, "C", dx=0.45, y=0.15, w=0.4, ht=0.2, slant=-15)
    face(s, h, "C", 0, -0.55, (1.0, 0.25, 0.2))
    for i, sx in enumerate(SIDES):
        for dx in (-0.4, 0.0, 0.4):
            s.cone((sx * h["ax"] + dx, h["hand"] - 0.6, -0.3), 0.35, 1.1, "C", "-y", part=3 + i)
    return check(s)


def blob():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 1.6, 2.4, 0), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * 1.6, 1.2, 0), (1.9, 2.6, 1.9), "D", part=pid)
    s.el((0, 5.6, 0), (8.4, 7.2, 7.0), "A")                 # the belly (shirt)
    s.el((0, 3.2, -0.4), (7.0, 2.2, 6.0), "P")             # waistband
    s.bx((0, 6.4, -3.4), (2.2, 0.5, 0.4), "C")             # belt stripe
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 3.8, 7.4, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.el((sx * 4.6, 6.2, -0.2), (2.0, 3.4, 2.0), "P", part=pid)
    s.part(5, 0, (0, 9.0, 0), "Head")
    s.el((0, 10.0, -0.3), (2.4, 2.2, 2.2), "P", part=5)
    for sx in SIDES:
        s.bx((sx * 0.45, 10.2, -1.35), (0.35, 0.25, 0.2), "C", part=5)
    s.bx((0, 9.4, -1.35), (0.9, 0.25, 0.2), "D", part=5)
    return check(s)


def kitty_pryde():
    s, h = hero(k=0.92, tw=2.7, td=1.5, lw=1.0, aw=0.95, torso=3.3, tt="A", ta="D", tl="A")
    boots(s, h, "D", 1.4)
    gloves(s, h, "D", 1.0)
    belt(s, h, "D")
    s.bx((0, h["hc"] + 0.1, 0.5), (h["hw"] + 0.4, h["hh"] + 0.6, 1.2), "D", part=5)  # ponytail
    s.el((0, h["hc"] - 0.4, 1.5), (0.8, 1.8, 0.8), "D", part=5)
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.35, ht=0.2)
    # the phasing ghost: a thin echo outline slightly behind
    s.bx((0, h["top"] - 1.6, 1.4), (h["tw"] + 0.6, 3.2, 0.12), "C")
    # Lockheed on her shoulder
    s.el((1.4, h["top"] + 0.5, 0.2), (0.9, 0.8, 1.4), "A")
    s.bx((1.4, h["top"] + 0.9, 0.2), (1.8, 0.12, 0.8), "A", rot=(0, 0, 20))
    return check(s)


def psylocke():
    s, h = hero(tw=2.8, td=1.6, lw=1.05, aw=1.0, torso=3.4, tt="A", ta="D", tl="D")
    boots(s, h, "A", 2.0)
    belt(s, h, "P", 0.5)
    s.part(6, 0, (0, h["hip"] + 0.3, 0.2), "Cloth")                    # the sash
    s.bx((0, h["hip"] - 1.4, -0.95), (0.8, 3.0, 0.2), "P", part=6)
    s.bx((0, h["hc"] - 0.3, 0.5), (h["hw"] + 0.6, h["hh"] + 2.0, 1.2), "D", part=5)  # long hair
    face(s, h, "C", -0.55, 0.3, (0.8, 0.35, 0.2))                       # the psi-mark
    eyes(s, h, "C", dx=0.4, y=0.1, w=0.35, ht=0.2)
    x, y, _ = hand_c(h, R)                                              # psychic katana
    s.bx((x, y - 0.2, -2.6), (0.3, 0.8, 5.2), "C", part=4)
    return check(s)


def dark_phoenix():
    s = Shape()
    s.el((0, 6.0, 0), (4.2, 4.2, 8.0), "P")
    s.el((0, 5.4, -1.2), (3.0, 3.0, 5.0), "A")
    s.part(5, 0, (0, 7.2, -3.4), "Head")
    s.el((0, 8.0, -4.8), (2.6, 2.6, 3.0), "P", part=5)
    s.cone((0, 7.8, -6.9), 1.1, 1.8, "C", "-z", part=5)
    for sx in SIDES:
        s.bx((sx * 0.75, 8.3, -5.9), (0.5, 0.4, 0.4), "C", part=5)
    for dz, ht in ((-4.2, 2.6), (-3.2, 3.2), (-2.2, 2.4)):
        s.cone((0, 9.6 + ht / 2 - 0.6, dz), 1.0, ht, "A", "+y", part=5)      # crest flames
    for i, sx in enumerate(SIDES):
        pid = s.part(9 + i, 0, (sx * 1.8, 6.8, -0.8), "WingLeft" if sx < 0 else "WingRight")
        s.bx((sx * 5.0, 7.6, -0.4), (7.0, 1.0, 5.6), "P", rot=(0, 0, sx * -18), part=pid)       # inner wing
        s.bx((sx * 10.6, 9.6, 0.4), (5.6, 0.8, 4.2), "A", rot=(0, sx * -10, sx * -24), part=pid)  # outer wing
        for n in range(4):                                                                      # primaries
            s.bx((sx * (4.0 + n * 2.4), 6.0 + n * 0.9, 2.6 + n * 0.3), (1.6, 0.6, 3.6 - n * 0.3), "C",
                 rot=(0, sx * -10, sx * -18), part=pid)
        s.cone((sx * 14.2, 11.4, 1.0), 1.4, 3.0, "C", "+x" if sx > 0 else "-x", part=pid)
    # the flame tail
    s.part(12, 0, (0, 5.6, 3.8), "Tail")
    for n, (dx, r) in enumerate(((-1.6, -22), (0.0, 0), (1.6, 22))):
        s.bx((dx, 5.0, 8.0), (1.4, 0.7, 8.0), "A" if n != 1 else "C", rot=(-10, r, 0), part=12)
    s.cone((0, 4.4, 12.6), 1.6, 2.4, "C", "+z", part=12)
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 0.9, 4.4, 0.4), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * 0.9, 3.0, 0.6), (0.7, 2.8, 0.7), "D", part=pid)
        s.bx((sx * 0.9, 1.7, 0.0), (1.1, 0.4, 1.8), "C", part=pid)
    return check(s)


def sentinel():
    """Mutant-hunting robot, kaiju scale (~16 m)."""
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 5.0, 28, 0), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.bx((sx * 5.2, 20, 0), (6.0, 14, 6.0), "A", part=pid)
        s.el((sx * 5.2, 13, 0), (6.6, 4.0, 6.6), "D", part=pid)
        s.bx((sx * 5.2, 6.5, 0), (6.4, 11, 6.4), "A", part=pid)
        s.bx((sx * 5.2, 1.2, -1.2), (7.2, 2.4, 9.6), "D", part=pid)
    s.bx((0, 30, 0), (13, 5, 8), "P")                       # hips
    s.bx((0, 40, 0), (18, 14, 11), "P")                     # chest
    s.cz((0, 40, -5.7), 7, 0.8, "C")                        # chest emitter
    s.cz((0, 40, -6.0), 4, 0.8, "A")
    for i, sx in enumerate(SIDES):
        s.el((sx * 11, 45, 0), (8, 6, 9), "A")              # shoulder domes
        pid = s.part(3 + i, 0, (sx * 12, 44, 0), "ArmLeft" if sx < 0 else "ArmRight")
        s.bx((sx * 12.5, 37, 0), (5, 12, 5), "P", part=pid)
        s.bx((sx * 12.5, 26, -0.5), (6, 10, 6), "A", part=pid)
        s.bx((sx * 12.5, 19.5, -0.5), (6.6, 4, 6.6), "P", part=pid)
        s.cz((sx * 12.5, 17.2, -0.5), 2.6, 0.6, "C", part=pid)   # palm emitters (Beam)
    s.part(5, 0, (0, 47, 0), "Head")
    s.bx((0, 51, 0), (7.6, 7.6, 7.6), "A", part=5)           # the helmet
    s.bx((0, 50, -3.9), (5.4, 5.4, 0.4), "P", part=5)         # the face plate
    for sx in SIDES:
        s.bx((sx * 1.4, 51.4, -4.2), (1.6, 0.6, 0.3), "C", part=5)
        s.bx((sx * 4.2, 51, 0), (0.8, 5.0, 5.0), "D", part=5)
    s.bx((0, 48.6, -4.2), (2.6, 0.5, 0.3), "D", part=5)
    return check(s)


def fin_fang_foom():
    s = Shape()
    for i, sx in enumerate(SIDES):
        pid = s.part(1 + i, 0, (sx * 6, 20, 2), "LegBackLeft" if sx < 0 else "LegBackRight")
        s.el((sx * 6.4, 15, 2), (8, 12, 9), "P", part=pid)
        s.bx((sx * 6.4, 6, 2.5), (6, 11, 6), "P", part=pid)
        s.bx((sx * 6.4, 1.2, 0), (7, 2.4, 10), "D", part=pid)
    s.bx((0, 20, 2), (15, 5, 10), "A")                         # the purple trunks
    s.el((0, 30, 1), (16, 20, 13), "P", rot=(-10, 0, 0))
    s.el((0, 28, -4), (10, 14, 5), "A", rot=(-10, 0, 0))        # belly scales
    for i, sx in enumerate(SIDES):
        pid = s.part(3 + i, 0, (sx * 8, 36, -2), "ArmLeft" if sx < 0 else "ArmRight")
        s.seg((sx * 9, 35, -2), (sx * 10, 27, -7), 3.6, 3.6, "P", part=pid)
        for dz in (-0.8, 0.8):
            s.cone((sx * 10, 25, -7.8 + dz), 0.8, 2.2, "C", "-y", part=pid)
    s.part(5, 0, (0, 40, -3), "Head")
    s.el((0, 43, -5), (9, 8, 9), "P", part=5)
    s.bx((0, 44, -11), (6, 4.4, 9), "P", part=5)               # snout
    for sx in SIDES:
        s.bx((sx * 2.2, 46, -8), (0.9, 0.9, 1.6), "C", part=5)
        s.seg((sx * 2.8, 43.4, -14.6), (sx * 7.0, 40.0, -17.0), 0.7, 0.7, "C", part=5)  # the whiskers
        s.cone((sx * 3, 48.6, -3), 1.4, 4.0, "D", "+y", part=5)  # horns
    s.part(6, 5, (0, 42, -7), "Jaw")
    s.bx((0, 41.2, -11), (5.2, 1.8, 8), "D", part=6)
    for i, sx in enumerate(SIDES):  # small wings
        pid = s.part(9 + i, 0, (sx * 5, 36, 6), "WingLeft" if sx < 0 else "WingRight")
        s.bx((sx * 10, 39, 8), (10, 0.8, 6), "A", rot=(0, 0, sx * -25), part=pid)
    for (y, z) in ((44, 1), (39, 7.5), (33, 9.5)):
        s.bx((0, y, z), (1.4, 4.5, 4.5), "A", rot=(45, 0, 0))
    last = None
    prev = 0
    pts = [(0, 22, 8), (0, 15, 17), (0, 8, 26), (0, 4, 35)]
    for n in range(3):
        pid = s.part(12 + n, prev, pts[n], "Tail")
        s.seg(pts[n], pts[n + 1], (9 - 2.4 * n), (8 - 2.2 * n), "P", part=pid, kind="Ellipsoid", over=2)
        prev = pid
    s.cone((0, 3.5, 38), 2.4, 5, "D", "+z", part=prev)
    return check(s)


def johnny_blaze():
    s, h = hero(tt="D", ta="D", tl="D", th="C")
    boots(s, h, "D", 1.6)
    pads(s, h, "C", ht=0.6)
    for sx in SIDES:
        s.cone((sx * (h["tw"] / 2 + 0.2), h["top"] + 0.5, 0), 0.4, 0.9, "C", "+y")
    # the flaming skull
    for sx in SIDES:
        face(s, h, "D", sx * 0.45, 0.15, (0.55, 0.55, 0.25))
    face(s, h, "D", 0, -0.55, (1.0, 0.25, 0.25))
    for (x, ht) in ((-0.8, 1.6), (0.0, 2.6), (0.8, 1.9), (-0.3, 2.2), (0.4, 1.5)):
        s.cone((x, h["hc"] + h["hh"] / 2 + ht / 2 - 0.2, h["hz"] + 0.3), 0.9, ht, "A", "+y", part=5)
    for i, sx in enumerate(SIDES):
        s.cone((sx * h["ax"], h["hand"] + 0.2, 0), 1.0, 1.4, "A", "+y", part=3 + i)
    # the hellfire chain, trailing from the right hand
    x, y, _ = hand_c(h, R)
    pts = [(x, y - 0.4, 0), (x + 1.0, y - 2.0, -1.4), (x + 1.4, y - 2.8, -3.4), (x + 0.8, y - 3.0, -5.4)]
    for a, b in zip(pts, pts[1:]):
        s.seg(a, b, 0.45, 0.45, "C", part=4, kind="Cylinder", over=0.2)
    s.el(pts[-1], (0.9, 0.9, 0.9), "A", part=4)
    return check(s)


def wade_wilson():
    s, h = hero(tt="P", ta="P", tl="P", th="P")
    boots(s, h, "D", 1.6)
    gloves(s, h, "D", 1.3)
    belt(s, h, "D", 0.7)
    for x in (-1.0, 0.0, 1.0):
        s.bx((x, h["hip"] + 0.3, h["cz"] - 0.3), (0.7, 0.7, 0.5), "D")    # pouches
    s.cz((0, h["hip"] + 0.3, h["cz"] - 0.5), 0.8, 0.4, "C")               # the buckle
    for sx in SIDES:  # black eye patches with white lenses
        face(s, h, "D", sx * 0.5, 0.2, (0.9, 0.8, 0.2), rot=(0, 0, sx * -12))
        face(s, h, "C", sx * 0.5, 0.2, (0.45, 0.3, 0.3), rot=(0, 0, sx * -12))
    # crossed katanas on the back
    for sx in SIDES:
        s.bx((sx * 0.9, h["top"] + 0.8, h["td"] / 2 + 0.3), (0.5, 1.6, 0.5), "D", rot=(0, 0, sx * 30))
        s.bx((sx * -0.6, h["top"] - 2.4, h["td"] / 2 + 0.3), (0.35, 5.0, 0.25), "C", rot=(0, 0, sx * 30))
    return check(s)
