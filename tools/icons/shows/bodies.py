"""Extra body builders for the TV / movie pack: cartoon (Simpsons / South Park) bodies, quadrupeds, dragons,
cars and simple props. Same conventions as rig.py: feet at y = 0, facing -Z, left = -X.
Parts: 1/2 legs (bipeds) or 1..4 legs (quadrupeds: FL, FR, BL, BR), 3/4 arms, 5 head, 6 jaw, 7 cloth,
8..11 tail chain, 12/13 wings, 20..23 wheels, 255 seat.
"""
import math

from slib import ALONG_X, ALONG_Z, TIP_NZ, TIP_NY
from rig import humanoid, head, cape, tail


# ------------------------------------------------------------------------------------ cartoon humans

def toon(s, skin="A", shirt="P", pants="D", shoes="D", h_leg=6.0, torso=(7.0, 7.5, 4.6), belly=0.0, arm_len=7.5,
         arm_w=1.7, head_h=7.0, head_w=4.4, eyes=True, overbite=True, neck=0.6, hip_x=1.6, leg_w=2.0):
    """Springfield-style body: short legs, soft torso (optional belly), long tube head with bulging eyes.
    Returns the body dict (+ 'hy' head centre, 'top' head top)."""
    b = humanoid(s, leg_h=h_leg, leg_w=leg_w, leg_d=2.2, hip_x=hip_x, foot=(2.3, 1.2, 3.4), torso=torso, torso_t=shirt,
                 torso_kind="ell", arm_w=arm_w, arm_len=arm_len, arm_x=torso[0] / 2 + 0.7, hand=1.9, arm_t=shirt,
                 leg_t=pants, hand_t=skin, foot_t=shoes, neck=neck, arm_spread=6)
    if belly:
        s.ell((0, h_leg + torso[1] * 0.38, -torso[2] * 0.2), (torso[0] * 0.95 + belly, torso[1] * 0.75, torso[2] + belly), shirt)
    s.on(5)
    hy = b["y_neck"] + head_h / 2 - 0.2
    s.cyl((0, hy, 0), (head_w, head_h, head_w * 0.95), skin)
    s.ell((0, hy + head_h / 2, 0), (head_w, head_w * 0.6, head_w * 0.95), skin)
    if eyes:
        for sx in (-1, 1):
            s.ell((sx * head_w * 0.22, hy + head_h * 0.12, -head_w * 0.46), (1.7, 1.7, 1.2), "C")
            s.box((sx * head_w * 0.2, hy + head_h * 0.12, -head_w * 0.46 - 0.55), (0.4, 0.4, 0.3), "D")
    if overbite:
        s.ell((0, hy - head_h * 0.28, -head_w * 0.42), (head_w * 0.7, head_h * 0.3, 1.8),
              overbite if isinstance(overbite, str) else skin)
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    b["hy"], b["top"], b["head_w"], b["head_h"] = hy, hy + head_h / 2 + head_w * 0.3, head_w, head_h
    return b


def paper_kid(s, coat="P", pants="D", skin="A", hat=None, hat_t="A", w=8.0, h_body=6.0, head_d=8.5, legs=1.6):
    """South Park cut-out: huge round head on a small mitten body."""
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):
        s.on(pid)
        s.box((sx * 1.5, legs / 2, -0.2), (2.4, legs, 3.0), "D")
        s.part(pid, 0, (sx * 1.5, legs, 0), role)
    s.on(0)
    s.ell((0, legs + h_body / 2, 0), (w, h_body * 1.1, w * 0.7), coat)
    s.box((0, legs + 0.9, 0), (w * 0.8, 1.6, w * 0.6), pants)
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):
        s.on(pid)
        s.ell((sx * (w / 2 + 0.3), legs + h_body * 0.55, -0.4), (1.8, 3.4, 1.8), coat, (0, 0, sx * 20))
        s.ell((sx * (w / 2 + 0.9), legs + h_body * 0.25, -0.6), (1.6, 1.6, 1.6), hat_t if hat == "mitten" else "D")
        s.part(pid, 0, (sx * (w / 2), legs + h_body * 0.8, 0), role)
    s.on(5)
    hy = legs + h_body + head_d * 0.42
    s.ell((0, hy, 0), (head_d * 1.08, head_d * 0.9, head_d * 0.9), skin)
    for sx in (-1, 1):
        s.ell((sx * 1.3, hy + 0.6, -head_d * 0.43), (2.2, 2.6, 0.8), "C")
        s.box((sx * 0.9, hy + 0.6, -head_d * 0.46 - 0.3), (0.5, 0.5, 0.3), "D")
    s.part(5, 0, (0, legs + h_body, 0), "Head")
    s.on(0)
    return dict(hy=hy, top=hy + head_d * 0.45, legs=legs, h_body=h_body, w=w, head_d=head_d)


# ------------------------------------------------------------------------------------ quadrupeds

def quad(s, L=14.0, H=6.0, W=5.0, leg_h=6.0, leg_w=1.8, body_t="P", leg_t="P", head_c=None, head_s=(4.0, 4.0, 5.0),
         head_t="P", neck=True, tail_pts=None, tail_w=1.2, paws=None, snout=None, ears=None, ear_t=None):
    """Four-legged body. Returns key points. Head on part 5 at the front (-Z), legs 1..4, tail 8+."""
    y_body = leg_h + H / 2 - 0.6
    s.on(0)
    s.ell((0, y_body, 0), (W, H, L), body_t)
    legs = ((1, -1, -1, "LegFrontLeft"), (2, 1, -1, "LegFrontRight"), (3, -1, 1, "LegBackLeft"), (4, 1, 1, "LegBackRight"))
    for pid, sx, sz, role in legs:
        x, z = sx * (W / 2 - leg_w / 2 - 0.1), sz * (L / 2 - leg_w - 0.6)
        s.on(pid)
        s.box((x, leg_h / 2, z), (leg_w, leg_h + 0.6, leg_w * 1.1), leg_t)
        if paws:
            s.box((x, 0.5, z - 0.3), (leg_w + 0.4, 1.0, leg_w + 0.8), paws)
        s.part(pid, 0, (x, leg_h, z), role)
    hc = head_c if head_c is not None else (0, y_body + H * 0.55, -L / 2 - head_s[2] * 0.25)
    s.on(5)
    if neck:
        s.seg((0, y_body + H * 0.2, -L / 2 + 1.5), hc, W * 0.45, W * 0.45, body_t)
    s.ell(hc, head_s, head_t)
    if snout:
        s.box((hc[0], hc[1] - head_s[1] * 0.15, hc[2] - head_s[2] * 0.5), snout, head_t)
        s.box((hc[0], hc[1] - head_s[1] * 0.1, hc[2] - head_s[2] * 0.5 - snout[2] / 2), (1.0, 0.8, 0.4), "D")
    for sx in (-1, 1):
        s.box((hc[0] + sx * head_s[0] * 0.3, hc[1] + head_s[1] * 0.15, hc[2] - head_s[2] * 0.42), (0.7, 0.7, 0.4), "C")
        if ears:
            s.cone((hc[0] + sx * head_s[0] * 0.33, hc[1] + head_s[1] * 0.5 + ears / 2, hc[2] + 0.4), (1.3, ears, 1.0),
                   ear_t or head_t)
    s.part(5, 0, (0, y_body + H * 0.3, -L / 2 + 1.0), "Head")
    s.on(0)
    if tail_pts:
        tail(s, tail_pts, [(tail_w, tail_w, max(0.6, tail_w * 1.6))] * (len(tail_pts) - 1), body_t, kind="seg")
    return dict(y_body=y_body, hc=hc, L=L, H=H, W=W, leg_h=leg_h)


def dragon(s, L=20.0, H=8.0, W=7.0, leg_h=6.0, body_t="P", belly_t="A", wing_t="A", span=26.0, neck_len=9.0,
           head_s=(4.0, 3.6, 7.0), horns="D", spikes="D", tail_len=18.0, tail_w=2.4, jaw=True):
    """Western dragon: quadruped body, long raised neck, horned head, membrane wings (12/13), long tail (8..11)."""
    y_body = leg_h + H / 2 - 0.8
    s.on(0)
    s.ell((0, y_body, 0), (W, H, L), body_t)
    s.ell((0, y_body - H * 0.18, -L * 0.1), (W * 0.8, H * 0.7, L * 0.7), belly_t)
    for i in range(4):
        s.cone((0, y_body + H / 2 + 0.6, -L * 0.3 + i * L * 0.2), (1.4, 2.4, 1.4), spikes)
    legs = ((1, -1, -1, "LegFrontLeft"), (2, 1, -1, "LegFrontRight"), (3, -1, 1, "LegBackLeft"), (4, 1, 1, "LegBackRight"))
    for pid, sx, sz, role in legs:
        x, z = sx * (W / 2 - 0.4), sz * (L / 2 - 3.2)
        s.on(pid)
        s.box((x, leg_h / 2 + 0.3, z), (2.4, leg_h + 0.6, 2.6), body_t)
        s.box((x, 0.5, z - 0.8), (2.8, 1.0, 3.6), "D")
        for cx in (-0.8, 0.0, 0.8):
            s.cone((x + cx, 0.4, z - 2.8), (0.5, 1.0, 0.5), "C", TIP_NZ)
        s.part(pid, 0, (x, leg_h + 0.4, z), role)
    # neck + head (part 5), jaw (6)
    n0 = (0, y_body + H * 0.25, -L / 2 + 2.0)
    hc = (0, y_body + neck_len * 0.8, -L / 2 - neck_len * 0.55)
    s.on(5)
    s.seg(n0, hc, W * 0.42, W * 0.42, body_t)
    s.ell(hc, head_s, body_t)
    s.box((0, hc[1] - 0.3, hc[2] - head_s[2] * 0.55), (head_s[0] * 0.7, head_s[1] * 0.45, head_s[2] * 0.5), body_t)
    for sx in (-1, 1):
        s.box((sx * head_s[0] * 0.32, hc[1] + head_s[1] * 0.2, hc[2] - head_s[2] * 0.25), (0.8, 0.6, 0.8), "C")
        s.cone((sx * head_s[0] * 0.3, hc[1] + head_s[1] * 0.4, hc[2] + head_s[2] * 0.45), (1.0, 4.0, 1.0), horns, (-60, 0, sx * 15))
    s.part(5, 0, n0, "Head")
    if jaw:
        s.on(6)
        s.box((0, hc[1] - head_s[1] * 0.45, hc[2] - head_s[2] * 0.45), (head_s[0] * 0.6, 0.9, head_s[2] * 0.6), "D")
        s.part(6, 5, (0, hc[1] - head_s[1] * 0.3, hc[2]), "Jaw")
    # wings
    for pid, sx, role in ((12, -1, "WingLeft"), (13, 1, "WingRight")):
        s.on(pid)
        x0, y0, z0 = sx * (W / 2 - 0.5), y_body + H / 2, -L * 0.15
        s.seg((x0, y0, z0), (sx * span * 0.3, y0 + 7.0, z0 + 1.0), 1.4, 1.4, body_t)
        s.seg((sx * span * 0.3, y0 + 7.0, z0 + 1.0), (sx * span * 0.5, y0 + 3.0, z0 + 4.0), 1.0, 1.0, body_t)
        for i, (fx, fy) in enumerate(((0.18, 1.5), (0.32, 2.2), (0.44, 0.2))):
            s.box((sx * span * fx, y0 + fy, z0 + 3.0 + i), (span * 0.2, 6.0 - i * 1.2, 0.5), wing_t, (0, 0, sx * (15 - i * 12)))
        s.part(pid, 0, (x0, y0, z0), role)
    s.on(0)
    pts = [(0, y_body + 0.5, L / 2 - 1.0)]
    for i in range(1, 5):
        f = i / 4
        pts.append((math.sin(f * 2.2) * 2.0, y_body + 0.5 - f * (y_body - 2.0), L / 2 - 1.0 + tail_len * f))
    tail(s, pts, [(tail_w * (1 - 0.18 * i), tail_w * (1 - 0.18 * i), 1) for i in range(4)], body_t, kind="seg")
    s.on(11)
    s.cone(pts[-1], (2.4, 2.6, 0.6), spikes, (90, 0, 0))
    s.on(0)
    return dict(y_body=y_body, hc=hc)


# ------------------------------------------------------------------------------------ vehicles

def wheels(s, pts, d=4.0, w=1.6, t="D", hub="C", first=20):
    """Wheels as parts (role Wheel), axis along X."""
    for i, (x, z) in enumerate(pts):
        pid = first + i
        s.on(pid)
        s.cx((x, d / 2, z), d, w, t)
        s.cx((x + (0.1 if x > 0 else -0.1), d / 2, z), d * 0.45, w + 0.1, hub)
        s.part(pid, 0, (x, d / 2, z), "Wheel")
    s.on(0)


def car_body(s, L=22.0, W=9.0, h0=2.2, h1=4.0, cab=(8.0, 3.2), cab_z=1.5, body_t="P", glass="C", wheel_d=4.2):
    """A low car: chassis slab + greenhouse cab with windows. Returns top-of-body y."""
    s.box((0, h0 + h1 / 2, 0), (W, h1, L), body_t)
    y1 = h0 + h1
    s.box((0, y1 + cab[1] / 2, cab_z), (W * 0.86, cab[1], cab[0]), body_t)
    s.box((0, y1 + cab[1] * 0.5, cab_z - cab[0] / 2 - 0.05), (W * 0.76, cab[1] * 0.7, 0.3), glass)
    for sx in (-1, 1):
        s.box((sx * W * 0.43, y1 + cab[1] * 0.55, cab_z), (0.3, cab[1] * 0.6, cab[0] * 0.8), glass)
        s.box((sx * W * 0.33, h0 + h1 * 0.6, -L / 2 - 0.1), (1.8, 1.0, 0.3), "C")                # headlights
        s.box((sx * W * 0.36, h0 + h1 * 0.6, L / 2 + 0.1), (1.6, 0.8, 0.3), "A")                 # tail lights
    wheels(s, [(-W / 2, -L * 0.32), (W / 2, -L * 0.32), (-W / 2, L * 0.32), (W / 2, L * 0.32)], d=wheel_d)
    return y1


# ------------------------------------------------------------------------------------ everyday people

def person(s, top="P", pants="D", skin="P", hair="D", style="short", bulk=1.0, tall=0.0, fem=False, shoes="D",
           shirt=None, tie=None, belly=0.0, head_w=3.9, head_h=4.5, arms=None, hands=None):
    """A regular (non-heroic) human: box torso, optional open jacket over a shirt + tie, optional belly.
    Head on part 5 via rig.head. Returns the body dict (with 'hy')."""
    k = bulk
    b = humanoid(s, leg_h=(9.2 if not fem else 9.6) + tall, leg_w=2.3 * k, leg_d=2.5 * k, hip_x=1.6 * k,
                 foot=(2.3 * k, 1.1, 3.4), torso=((6.8 if not fem else 6.0) * k, 8.6 + tall * 0.3, 4.2 * k),
                 torso_t=top, arm_w=1.8 * k, arm_len=8.8 + tall * 0.3, arm_x=(3.4 if not fem else 3.0) * k + 0.9,
                 hand=1.9 * k, arm_t=arms or top, leg_t=pants, hand_t=hands or skin, foot_t=shoes, neck=1.0,
                 arm_spread=5)
    zf = -b["td"] / 2 - 0.1
    if shirt:
        s.box((0, b["y_sh"] - 3.2, zf), (b["tw"] * 0.34, 6.0, 0.4), shirt)
    if tie:
        s.box((0, b["y_sh"] - 3.4, zf - 0.25), (0.8, 5.4, 0.3), tie)
    if belly:
        s.ell((0, b["y_hip"] + 3.2, -belly * 0.3), (b["tw"] * 0.95, 6.0, b["td"] + belly), top)
    head(s, b, skin=skin, hair=hair, style=style, w=head_w, h=head_h, d=head_w * 1.05)
    return b


def held(s, b, side=1):
    """Switch to the arm part holding an item and return the hand centre."""
    s.on(4 if side > 0 else 3)
    return b["hands"][side]
