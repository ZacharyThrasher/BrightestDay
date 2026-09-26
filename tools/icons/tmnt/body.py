"""Shared creature bodies for the TMNT pack: a humanoid rig every mutant/human/robot is built on.

Part ids (all creatures): 1 leg L, 2 leg R, 3 arm L, 4 arm R, 5 head, 6 jaw, 7 cloth (mask tails /
cape / coat tails), 8..11 tail chain, 12 wings L, 13 wings R. Weapons are authored *inside* the arm
parts so they follow the swing rigidly (the rig's swipe animation drives the arm).
Creatures face -Z, left = -X, feet at y = 0.
"""
from tlib import S, TIP_NZ, TIP_PZ, ALONG_Z


def humanoid(s, leg_h=7.0, leg_w=2.8, leg_d=3.0, hip_x=2.4, foot=(3.0, 1.4, 4.2), torso=(7.5, 8.5, 5.0),
             torso_t="P", torso_kind="box", arm_w=2.4, arm_len=8.4, arm_x=5.0, hand=2.6, arm_t="P", leg_t="P",
             hand_t="P", foot_t="P", neck=1.4, belly=None):
    """Legs (1,2), torso (root), arms (3,4). Returns key points; head is authored by the caller on part 5."""
    y_hip = leg_h
    tw, th, td = torso
    y_sh = leg_h + th
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):
        x = sx * hip_x
        s.on(pid)
        s.box((x, (leg_h + 0.6) / 2 + 0.4, 0), (leg_w, leg_h - 0.2, leg_d), leg_t)
        s.box((x, foot[1] / 2, -(foot[2] - leg_d) / 2 - 0.3), foot, foot_t)
        s.part(pid, 0, (x, y_hip, 0), role)
    s.on(0)
    if torso_kind == "box":
        s.box((0, y_hip + th / 2, 0), (tw, th, td), torso_t)
    else:
        s.ell((0, y_hip + th / 2, 0), (tw, th * 1.08, td), torso_t)
        s.box((0, y_hip + 0.8, 0), (tw * 0.8, 1.6, td * 0.85), torso_t)
    s.box((0, y_sh + neck / 2, 0), (tw * 0.36, neck + 0.4, td * 0.5), torso_t)   # neck
    hands = {}
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):
        x = sx * arm_x
        s.on(pid)
        top = y_sh - 0.4
        s.box((x, top - arm_len / 2, 0), (arm_w, arm_len, arm_w), arm_t)
        hy = top - arm_len - hand / 2 + 0.3
        s.box((x, hy, -0.2), (hand, hand, hand), hand_t)
        hands[sx] = (x, hy, -0.2)
        s.part(pid, 0, (x, y_sh - 0.6, 0), role)
    s.on(0)
    return dict(y_hip=y_hip, y_sh=y_sh, y_neck=y_sh + neck, hands=hands, td=td, tw=tw)


def tail(s, pts, sizes, t="P", first_id=8, parent=0, kind="ell"):
    """Tail chain through pts (parts first_id..). Each part spans pts[i]..pts[i+1]."""
    pid = parent
    for i in range(len(pts) - 1):
        nid = first_id + i
        s.part(nid, pid, pts[i], "Tail")
        s.on(nid)
        a, b = pts[i], pts[i + 1]
        if kind == "seg":
            s.seg(a, b, sizes[i][0], sizes[i][1], t)
        else:
            c = tuple((a[k] + b[k]) / 2 for k in range(3))
            s.ell(c, sizes[i], t)
        pid = nid
    s.on(0)


# ------------------------------------------------------------------------------------ turtles

def turtle(s, mask="A", big=1.0, tall=0.0, head_kind="ell", shell_spikes=False, eyes="C", mask_tails=True,
           brow=False, beak=False):
    """A ninja turtle: stocky, big round head, plastron + domed shell, knee/elbow pads, belt.
    `big` widens (Raph/Slash/Tokka), `tall` adds leg/torso height (Donnie)."""
    k = big
    b = humanoid(s, leg_h=6.0 + tall, leg_w=3.0 * k, leg_d=3.2 * k, hip_x=2.3 * k, foot=(3.2 * k, 1.4, 4.2),
                 torso=(7.6 * k, 8.4 + tall * 0.6, 5.2 * k), torso_kind="ell", arm_w=2.7 * k, arm_len=7.4 + tall * 0.6,
                 arm_x=4.9 * k, hand=2.8 * k, neck=1.0)
    yh, ys, td = b["y_hip"], b["y_sh"], b["td"]
    mid = (yh + ys) / 2
    # plastron: grooves on the front of the belly
    s.box((0, mid - 0.2, -td / 2 + 0.2), (5.6 * k, ys - yh - 1.8, 0.8), "P")
    for dy in (-2.0, 0.2, 2.4):
        s.box((0, mid + dy, -td / 2 - 0.25), (5.8 * k, 0.35, 0.3), "D")
    s.box((0, mid, -td / 2 - 0.25), (0.35, ys - yh - 2.4, 0.3), "D")
    # shell: dark dome on the back + light rim + scute lines
    s.ell((0, mid + 0.3, td / 2 + 0.6), (8.8 * k, ys - yh + 2.4, 4.6 * k), "D")
    s.ell((0, mid + 0.3, td / 2 - 0.2), (9.4 * k, ys - yh + 2.8, 2.0), "P")
    for dy in (-1.8, 1.8):
        s.box((0, mid + dy, td / 2 + 2.85 * k), (4.6 * k, 0.4, 0.4), "P")
    s.box((0, mid, td / 2 + 2.9 * k), (0.4, ys - yh - 1.0, 0.4), "P")
    if shell_spikes:
        for (x, y) in ((0, mid + 3.2), (-2.6, mid + 1.2), (2.6, mid + 1.2), (-2.2, mid - 2.2), (2.2, mid - 2.2),
                       (0, mid - 0.6), (0, mid + 1.4)):
            s.cone((x * k, y, td / 2 + 2.9 * k + 0.9), (1.4, 2.4, 1.4), "D", (90, 0, 0))
    # belt + buckle
    s.box((0, yh + 0.7, 0), (7.4 * k, 1.2, td + 0.5), "D")
    s.box((0, yh + 0.7, -td / 2 - 0.4), (1.6, 1.2, 0.5), "C")
    # knee pads, elbow pads, wrist wraps
    for pid, sx in ((1, -1), (2, 1)):
        s.on(pid)
        s.box((sx * 2.3 * k, yh * 0.55, -1.6 * k), (2.6 * k, 1.8, 0.9), "D")
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        x = sx * 4.9 * k
        s.box((x, ys - 4.4, 1.3 * k), (2.7 * k, 1.5, 0.8), "D")
        s.box((x, ys - 6.9 - tall * 0.5, 0), (2.9 * k, 0.9, 2.9 * k), "D")
    # head (part 5)
    s.on(5)
    hy = b["y_neck"] + 3.0
    if head_kind == "ell":
        s.ell((0, hy, -0.4), (7.0 * k, 6.6, 7.2), "P")
    else:
        s.box((0, hy, -0.4), (6.6 * k, 6.0, 6.8), "P")
    s.ell((0, hy - 1.6, -3.0), (5.6 * k, 3.6, 4.0), "P")               # snout / jaw
    s.box((0, hy - 2.1, -4.9), (3.6 * k, 0.5, 0.5), "D")                # mouth line
    if beak:
        s.cone((0, hy - 1.4, -5.3), (2.6, 2.6, 2.0), "D", (-90, 0, 0))
    s.ell((0, hy + 0.6, -0.4), (7.7 * k, 2.2, 7.9), mask)               # mask band
    for sx in (-1, 1):
        s.box((sx * 1.6, hy + 0.6, -4.3), (1.7, 1.1, 0.6), eyes)        # white eye slits
    if brow:
        s.box((0, hy + 1.8, -3.9), (5.4, 0.7, 0.9), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    if mask_tails:
        s.on(7)
        s.box((0, hy + 0.6, 3.6), (1.8, 1.6, 1.2), mask)                # knot
        for sx in (-1, 1):
            s.seg((sx * 0.5, hy + 0.4, 3.9), (sx * 2.0, hy - 4.4, 5.8), 1.1, 0.5, mask)
        s.part(7, 5, (0, hy + 0.6, 3.6), "Cloth")
    s.on(0)
    b["hy"] = hy
    return b


def katana_in(s, hand, length=9.0, rise=2.6, guard="A"):
    """A katana gripped in `hand`, blade forward and slightly up (authored on the current part)."""
    x, y, z = hand
    s.box((x, y, z + 0.6), (0.8, 0.8, 3.0), "D")                        # grip
    s.box((x, y, z - 1.1), (1.8, 1.8, 0.5), guard)                      # tsuba
    s.seg((x, y + 0.1, z - 1.3), (x, y + rise, z - 1.3 - length), 0.5, 1.2, "C")


def sai_in(s, hand, length=6.0):
    x, y, z = hand
    s.box((x, y, z + 0.5), (0.8, 0.8, 2.4), "D")                        # grip
    s.box((x, y, z - 0.9), (0.6, 0.6, 0.6), "A")
    s.seg((x, y, z - 1.0), (x, y + 0.6, z - 1.0 - length), 0.55, 0.55, "C")    # monouchi
    for sx in (-1, 1):                                                   # yoku (side prongs)
        s.seg((x, y, z - 1.0), (x + sx * 1.3, y + 0.1, z - 1.3), 0.5, 0.5, "C")
        s.seg((x + sx * 1.3, y + 0.1, z - 1.3), (x + sx * 1.1, y + 0.2, z - 3.0), 0.5, 0.5, "C")


def nunchuck_in(s, hand, up=True):
    x, y, z = hand
    if up:
        s.seg((x, y - 0.6, z + 0.6), (x, y + 1.6, z - 2.2), 0.8, 0.8, "D")         # held stick
        s.seg((x, y + 1.6, z - 2.2), (x, y + 2.4, z - 3.0), 0.3, 0.3, "C")         # chain
        s.seg((x, y + 2.4, z - 3.0), (x, y + 5.6, z - 4.8), 0.9, 0.9, "A")         # swinging stick
    else:
        s.seg((x, y + 0.4, z + 0.4), (x, y - 2.2, z - 0.4), 0.8, 0.8, "D")
        s.seg((x, y - 2.2, z - 0.4), (x, y - 2.8, z - 1.2), 0.3, 0.3, "C")
        s.seg((x, y - 2.8, z - 1.2), (x, y - 3.4, z - 4.2), 0.9, 0.9, "A")


def blaster_in(s, hand, length=5.0, t="D"):
    x, y, z = hand
    s.box((x, y - 0.8, z + 0.2), (0.9, 1.8, 1.0), t)                    # grip
    s.box((x, y + 0.5, z - length / 2 + 0.8), (1.3, 1.5, length), t)    # body
    s.cz((x, y + 0.7, z - length + 0.2), 0.9, 1.6, "C")                 # muzzle


def beast_head(s, y0, snout=3.0, snout_w=3.0, head=(5.0, 5.0, 5.0), ears=None, eye_t="C", jaw=False, nose_t="D",
               t="P", tilt=0.0):
    """Animal head on part 5 (jaw on part 6 if jaw=True). y0 = neck top. Returns head centre y."""
    s.on(5)
    hw, hh, hd = head
    hy = y0 + hh / 2 - 0.2
    s.ell((0, hy, -0.3), head, t)
    sz = -hd / 2 - snout / 2 + 1.0
    s.box((0, hy - hh * 0.16, sz), (snout_w, hh * 0.42, snout + 1.0), t)
    s.box((0, hy - hh * 0.1 + 0.4, sz - snout / 2 - 0.2), (snout_w * 0.55, 0.9, 0.7), nose_t)   # nose
    for sx in (-1, 1):
        s.box((sx * hw * 0.26, hy + hh * 0.12, -hd / 2 + 0.1), (1.0, 1.0, 0.6), eye_t)
    if ears == "round":
        for sx in (-1, 1):
            s.ell((sx * hw * 0.42, hy + hh * 0.52, 0.2), (2.6, 2.6, 0.9), t)
            s.ell((sx * hw * 0.42, hy + hh * 0.52, -0.25), (1.6, 1.6, 0.4), "A")
    elif ears == "pointy":
        for sx in (-1, 1):
            s.cone((sx * hw * 0.3, hy + hh * 0.6, 0.3), (1.6, 2.6, 1.0), t)
    s.part(5, 0, (0, y0, 0), "Head")
    if jaw:
        s.on(6)
        s.box((0, hy - hh * 0.42, sz + 0.2), (snout_w * 0.9, hh * 0.22, snout + 0.6), t)
        s.part(6, 5, (0, hy - hh * 0.3, -hd * 0.25), "Jaw")
    s.on(0)
    return hy
