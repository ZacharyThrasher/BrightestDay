"""Fan favourites: Pichu/Raichu, the eight Eeveelutions, Lucario, Greninja, Garchomp, Tyranitar, Dragonite,
Lapras, Mimikyu, Ditto, Psyduck, Slowpoke, Machamp, Scyther, Arcanine, Growlithe (+ Mr. Mime, Wobbuffet,
Hitmonlee, Hitmonchan).

Authored at ~2x in-game resolution: height in units ~= 16 * max(scale, 0.9). Creatures face -Z, feet at origin.
Quadrupeds are normalised by their LONGEST side (bd_shapes::icon_extent), bipeds by height — so quadrupeds are
kept compact (tails curl up, length ~ height) or they shrink in game.
"""
import math

from plib import (S, icon, tail_chain, quad_legs, biped_legs, arms, wing_pair,
                  ALONG_Z, ALONG_X, TIP_NZ, TIP_PZ, TIP_NY, TIP_PX, TIP_NX,
                  HEAD, JAW, TAIL, LFL, LFR, LBL, LBR, ARM_L, ARM_R, WING_L, WING_R, SEAT, CLOTH, OTHER)

SEAT_ID = 255          # bd_shapes::voxelize::SEAT_PART — the pilot seat marker must use this id


# ============================================================================================ helpers
def bolt(s, base, h, w, t, part, flip=1, th=1.8):
    """Flat lightning bolt (thin in X) rising from `base` in the YZ plane, `h` tall, zig-zagging along Z."""
    x, y, z = base
    p0 = (x, y, z)
    p1 = (x, y + h * 0.4, z + flip * w * 0.5)
    p2 = (x, y + h * 0.45, z - flip * w * 0.2)
    p3 = (x, y + h, z + flip * w * 0.6)
    s.seg(p0, p1, w * 0.55, th, t, part=part)       # seg: w = in-plane width, h = thickness across the plane
    s.seg(p1, p2, w * 0.5, th, t, part=part)
    s.seg(p2, p3, w * 0.75, th, t, part=part)


def fox(s, *, body=(5.4, 4.8, 8.0), body_y=5.2, leg_top=4.6, xs=1.6, zf=-1.8, zb=3.4, leg_w=2.0,
        head_y=9.6, head_z=-3.6, hs=(6.6, 5.8, 5.8), muzzle=(3.2, 2.6, 2.6), eye_t='D', shine=False,
        leg_t='P', paw='P', eye_w=1.3, eye_h=1.8):
    """Eevee-family quadruped, compact (length ~ height, since quadrupeds normalise by their longest side):
    body, four legs (parts 4-7), neck + big head (part 1) with muzzle, nose, eyes."""
    s.ell((0, body_y, 0.8), body, 'P')
    s.ell((0, body_y + 0.2, -1.8), (body[0] * 0.95, body[1] * 0.95, 3.6), 'P')      # chest
    quad_legs(s, xs, zf, zb, top=leg_top, w=leg_w, d=leg_w, t=leg_t, paw=paw)
    s.part(1, 0, (0, body_y + 1.6, zf - 0.4), HEAD)
    s.seg((0, body_y + 0.8, zf), (0, head_y - 1.0, head_z + 0.6), 3.4, 3.4, 'P', part=1, kind='Ellipsoid')
    s.ell((0, head_y, head_z), hs, 'P', part=1)
    s.ell((0, head_y - 1.3, head_z - hs[2] * 0.42), muzzle, 'P', part=1)
    s.ell((0, head_y - 0.8, head_z - hs[2] * 0.42 - muzzle[2] * 0.5), (1.0, 0.8, 0.6), 'D', part=1)
    s.eyes(hs[0] * 0.27, head_y + 0.35, head_z - hs[2] * 0.4, part=1, w=eye_w, h=eye_h, d=0.8, t=eye_t, shine=shine)
    return dict(head_y=head_y, head_z=head_z, hs=hs)


def ears(s, hd, tip_dx, tip_dy, w=3.4, d=1.8, t='P', inner=None, base_dx=None, dz=0.5, part=1):
    """Pair of pointed ears on the head from `fox`."""
    hy, hz, hs = hd['head_y'], hd['head_z'], hd['hs']
    bx = base_dx if base_dx is not None else hs[0] * 0.26
    for sx in (-1, 1):
        b = (sx * bx, hy + hs[1] * 0.25, hz + 0.3)
        tip = (sx * (bx + tip_dx), hy + hs[1] * 0.25 + tip_dy, hz + 0.3 + dz)
        s.spike(b, tip, w, t, part=part, d=d)
        if inner:
            b2 = (b[0] + sx * 0.3, b[1] + 0.8, b[2] - 0.7)
            t2 = (tip[0] - sx * 0.7, tip[1] - 1.4, tip[2] - 0.7)
            s.spike(b2, t2, w * 0.55, inner, part=part, d=0.8)


def ring(s, c, r, t, part, axis='z', th=0.7):
    """Flat ring (disc with a primary-tinted centre) — Umbreon markings."""
    rot = {'z': ALONG_Z, 'x': ALONG_X, 'y': (0, 0, 0)}[axis]
    s.cyl(c, (2 * r, th, 2 * r), t, rot, part=part)
    inner = (c[0], c[1], c[2] - 0.15) if axis == 'z' else c
    s.cyl(inner, (r, th + 0.1, r), 'P', rot, part=part)


# ============================================================================================ Pichu line
def pichu():
    s = S()
    s.ell((0, 3.8, 0.2), (5.2, 5.4, 4.6), 'P')                             # body
    s.ell((0, 6.1, 0.2), (4.8, 1.2, 4.2), 'D')                             # black collar
    biped_legs(s, 1.4, 2.2, w=2.0, d=2.2, foot_len=3.0)
    arms(s, 2.3, 5.2, 2.2, w=1.6, d=1.6, down=(0.5, -1, -0.7))
    s.part(1, 0, (0, 6.4, 0), HEAD)
    s.ell((0, 9.4, -0.3), (8.4, 7.2, 6.8), 'P', part=1)                    # huge head
    for sx in (-1, 1):
        s.spike((sx * 2.6, 11.0, 0.2), (sx * 7.4, 16.0, 0.6), 5.2, 'P', part=1, d=1.8)   # big triangle ears
        s.spike((sx * 6.0, 14.6, 0.5), (sx * 7.6, 16.3, 0.6), 1.9, 'D', part=1, d=2.0)   # black tips
        s.ell((sx * 3.0, 8.1, -2.9), (1.9, 1.6, 1.0), 'A', part=1)                      # pink cheeks
    s.eyes(1.8, 9.8, -3.4, part=1, w=1.5, h=2.0, d=0.8, shine=False)
    s.box((0, 8.7, -3.7), (0.6, 0.5, 0.5), 'D', part=1)                    # nose
    tail_chain(s, 12, 0, [(0, 2.6, 2.0), (0, 4.0, 3.8), (0, 5.6, 3.2)], [(2.0, 2.0), (2.0, 2.0)], 'D', kind='Box')
    return s


def raichu():
    s = S()
    s.ell((0, 5.0, 0.3), (6.8, 7.2, 5.8), 'P')                             # pear body
    s.ell((0, 4.4, -1.4), (4.8, 5.2, 3.2), 'A')                            # cream belly
    biped_legs(s, 1.9, 3.0, w=2.2, d=2.4, foot_len=3.6)
    arms(s, 2.8, 7.0, 2.6, w=1.8, d=1.8, down=(0.4, -1, -0.8))
    s.part(1, 0, (0, 8.2, 0), HEAD)
    s.ell((0, 10.4, -0.3), (7.0, 5.8, 5.8), 'P', part=1)
    for sx in (-1, 1):
        s.spike((sx * 2.0, 12.0, 0.3), (sx * 4.9, 16.6, 0.6), 3.4, 'P', part=1, d=1.6)   # long ears
        s.spike((sx * 2.3, 12.8, -0.3), (sx * 4.3, 15.4, -0.1), 1.6, 'A', part=1, d=0.8)  # yellow inner
        s.ell((sx * 5.3, 16.4, 0.6), (2.2, 1.8, 1.6), 'D', part=1)                      # curled brown tip
        s.ell((sx * 2.8, 9.4, -2.4), (2.0, 1.8, 1.0), 'A', part=1)                      # yellow cheeks
    s.eyes(1.5, 10.9, -2.9, part=1, w=1.3, h=1.7, d=0.8, shine=False)
    s.box((0, 9.8, -3.2), (0.6, 0.5, 0.5), 'D', part=1)
    # long thin black whip tail ending in a lightning-bolt tip
    ids = tail_chain(s, 12, 0, [(0, 3.4, 2.8), (0, 2.4, 6.2), (0, 4.4, 9.2), (0, 7.4, 10.8)],
                     [(2.0, 2.0), (1.8, 1.8), (1.8, 1.8)], 'D', kind='Box')
    s.part(16, ids[-1], (0, 7.4, 10.8), TAIL)
    bolt(s, (0, 7.0, 11.0), 7.4, 4.6, 'A', 16)
    return s


# ============================================================================================ Eeveelutions
def vaporeon():
    s = S()
    fox(s)
    for sx in (-1, 1):                                                     # fin ears flaring sideways
        s.spike((sx * 2.0, 11.6, -3.4), (sx * 6.0, 13.8, -2.0), 4.2, 'A', part=1, d=1.4)
        s.spike((sx * 2.4, 12.0, -3.0), (sx * 5.8, 14.0, -1.8), 1.4, 'P', part=1, d=1.6)
    s.spike((0, 12.2, -4.0), (0, 15.2, -1.4), 3.4, 'P', part=1, d=1.6)     # head crest fin
    s.cyl((0, 8.0, -2.2), (8.4, 1.6, 6.6), 'A', (-55, 0, 0))                # big neck frill ruff
    s.cyl((0, 8.2, -1.8), (7.4, 1.8, 5.2), 'A', (-30, 0, 0))
    s.spike((0, 7.2, 1.0), (0, 10.2, 3.8), 3.4, 'D', d=1.6)                # dorsal fin ridge
    # mermaid tail: thick tapering chain curling up to a V fluke
    ids = tail_chain(s, 12, 0, [(0, 5.6, 4.4), (0, 5.0, 7.4), (0, 7.4, 9.0), (0, 10.4, 9.0)],
                     [(3.8, 3.6), (3.2, 3.0), (2.6, 2.4)], 'P')
    for sx in (-1, 1):
        s.spike((sx * 0.6, 10.2, 9.0), (sx * 4.2, 13.6, 9.8), 3.4, 'P', part=ids[-1], d=1.6)
        s.spike((sx * 2.6, 12.0, 9.5), (sx * 4.3, 13.8, 9.9), 1.8, 'D', part=ids[-1], d=1.8)
    return s


def jolteon():
    s = S()
    hd = fox(s, body=(5.2, 4.6, 8.0))
    ears(s, hd, 2.0, 6.0, w=3.6, d=1.8)
    s.ell((0, 7.8, -2.6), (5.6, 3.8, 3.8), 'A')                            # white collar ruff
    for k in range(7):                                                     # ...bristling with spikes
        a = math.radians(-165 + k * 50)
        b = (math.cos(a) * 1.8, 7.8 + math.sin(a) * 1.2, -2.4)
        tip = (math.cos(a) * 5.0, 8.2 + math.sin(a) * 3.0, 0.4)
        s.spike(b, tip, 2.8, 'A', d=2.2)
    for z, y in ((0.8, 7.2), (3.2, 6.8)):                                  # back spikes
        s.spike((0, y, z), (0, y + 3.2, z + 2.4), 3.2, 'P', d=2.2)
    s.spike((0, 11.8, -4.0), (0, 13.8, -1.6), 2.8, 'P', part=1, d=2.0)     # spiky crest between ears
    s.part(12, 0, (0, 6.0, 4.2), TAIL)                                     # spiky rump tuft
    for dx, dy in ((-1.0, 0.0), (0, 1.8), (1.0, 0.0)):
        s.spike((dx * 0.6, 6.0, 4.2), (dx * 2.4, 7.2 + dy, 8.4), 3.0, 'P', part=12, d=2.2)
    return s


def flareon():
    s = S()
    hd = fox(s, body=(5.8, 5.0, 8.0))
    ears(s, hd, 2.6, 5.4, w=3.6, d=1.8, inner='D')
    s.ell((0, 7.8, -2.2), (7.8, 5.6, 5.6), 'A')                            # huge fluffy mane
    for sx in (-1, 0, 1):
        s.spike((sx * 2.0, 7.4, -3.6), (sx * 2.8, 5.4, -5.6), 2.8, 'A', d=2.2)   # mane tufts hanging forward
    s.ell((0, 12.6, -4.4), (3.2, 2.4, 2.8), 'A', part=1)                   # head tuft
    s.spike((0, 12.6, -5.2), (0, 12.0, -7.6), 2.2, 'A', part=1, d=2.0)
    ids = tail_chain(s, 12, 0, [(0, 6.0, 4.2), (0, 8.8, 6.4), (0, 12.0, 6.6)], [(3.6, 3.6), (5.0, 5.0)], 'A')
    s.ell((0, 12.8, 5.8), (5.0, 4.6, 4.6), 'A', part=ids[-1])              # bushy tail curl
    return s


def espeon():
    s = S()
    hd = fox(s, body=(5.0, 4.6, 8.0))
    ears(s, hd, 3.0, 6.0, w=4.0, d=1.8, inner='D')
    s.ell((0, 11.3, -6.2), (1.8, 1.8, 1.2), 'A', part=1)                   # red forehead gem
    ids = tail_chain(s, 12, 0, [(0, 6.0, 4.4), (0, 6.2, 7.2), (0, 9.0, 8.4)], [(2.2, 2.2), (2.0, 2.0)], 'P')
    for k, sx in enumerate((-1, 1)):                                       # forked tail tips
        pid = 20 + k
        s.part(pid, ids[-1], (0, 9.0, 8.4), TAIL)
        s.seg((0, 9.0, 8.4), (sx * 2.6, 12.2, 8.8), 2.0, 2.0, 'P', part=pid, kind='Ellipsoid')
        s.spike((sx * 2.6, 12.2, 8.8), (sx * 3.2, 14.8, 8.8), 2.0, 'P', part=pid, d=2.0)
    return s


def umbreon():
    s = S()
    hd = fox(s, body=(5.2, 4.6, 8.0), eye_t='C')
    s.eyes(1.8, 9.95, -6.2, part=1, w=0.8, h=1.1, d=0.5, t='D', shine=False)   # pupils on glowing eyes
    ears(s, hd, 2.4, 6.4, w=3.8, d=1.8)
    for sx in (-1, 1):                                                     # ear rings
        s.ell((sx * 3.2, 14.9, -3.0), (3.4, 1.4, 2.8), 'A', (0, 0, sx * 20), part=1)
    ring(s, (0, 11.4, -5.8), 1.7, 'A', 1, th=1.2)                                  # forehead ring
    for pid, x, z in ((4, -1.6, -1.8), (5, 1.6, -1.8), (6, -1.6, 3.4), (7, 1.6, 3.4)):   # leg bands
        s.box((x, 2.6, z), (2.6, 1.2, 2.6), 'A', part=pid)
    ids = tail_chain(s, 12, 0, [(0, 6.0, 4.4), (0, 7.6, 6.8), (0, 10.8, 7.6)], [(2.8, 2.8), (2.8, 2.8)], 'P')
    s.ell((0, 9.2, 7.2), (3.6, 1.4, 3.6), 'A', part=ids[-1])               # tail ring
    return s


def leafeon():
    s = S()
    fox(s)
    for sx in (-1, 1):                                                     # broad leaf ears
        s.seg((sx * 2.0, 11.2, -3.4), (sx * 5.8, 15.4, -2.8), 3.8, 1.6, 'P', part=1, kind='Ellipsoid')
        s.spike((sx * 5.4, 15.0, -2.9), (sx * 6.8, 17.0, -2.7), 2.0, 'P', part=1, d=1.6)
        s.seg((sx * 2.8, 12.2, -3.9), (sx * 5.2, 14.8, -3.5), 0.8, 0.8, 'D', part=1)    # midrib
    s.seg((0, 11.8, -5.0), (0, 15.0, -7.2), 3.0, 1.8, 'A', part=1, kind='Ellipsoid')    # forehead leaf
    for sx in (-1, 1):                                                     # chest + back leaf sprouts
        s.seg((sx * 1.4, 7.0, -3.0), (sx * 3.6, 8.8, -4.6), 1.6, 2.6, 'A', kind='Ellipsoid')
        s.seg((sx * 0.8, 7.2, 1.6), (sx * 2.6, 10.0, 2.8), 1.6, 2.4, 'A', kind='Ellipsoid')
    # big leaf tail (flat blade standing up, thin in X)
    s.part(12, 0, (0, 6.0, 4.4), TAIL)
    s.seg((0, 6.0, 4.4), (0, 8.0, 6.0), 2.2, 2.2, 'P', part=12)
    s.seg((0, 7.4, 5.8), (0, 15.2, 7.8), 5.2, 1.8, 'A', part=12, kind='Ellipsoid')
    s.spike((0, 14.6, 7.6), (0, 17.0, 8.4), 2.6, 'A', part=12, d=1.8)
    s.seg((0, 8.4, 6.2), (0, 14.6, 7.8), 0.8, 2.0, 'D', part=12)
    return s


def glaceon():
    s = S()
    fox(s)
    for sx in (-1, 1):
        s.spike((sx * 1.6, 11.2, -3.4), (sx * 4.2, 15.8, -3.0), 4.0, 'P', part=1, d=1.8)   # diamond ears
        s.spike((sx * 4.2, 15.8, -3.0), (sx * 3.0, 13.4, -3.2), 4.0, 'P', part=1, d=1.8)
        # long diamond pendants hanging from the sides of the head
        s.box((sx * 3.4, 8.0, -3.6), (1.4, 3.2, 3.2), 'A', (45, 0, 0), part=1)
        s.box((sx * 3.4, 6.0, -3.8), (1.4, 2.2, 2.2), 'A', (45, 0, 0), part=1)
    s.box((0, 11.8, -5.8), (2.2, 2.2, 1.2), 'A', (0, 0, 45), part=1)       # forehead diamond
    for z in (-0.4, 2.6):                                                  # diamond patches on the back
        s.box((0, 7.6, z), (2.4, 1.8, 2.4), 'A', (0, 45, 0))
    ids = tail_chain(s, 12, 0, [(0, 6.0, 4.4), (0, 7.8, 6.8)], [(2.6, 2.6)], 'P')
    s.box((0, 10.6, 7.4), (1.8, 4.8, 4.8), 'P', (45, 0, 0), part=ids[-1])  # diamond tail
    s.box((0, 10.6, 7.4), (2.0, 2.4, 2.4), 'A', (45, 0, 0), part=ids[-1])
    return s


def sylveon():
    s = S()
    hd = fox(s, paw='A')
    ears(s, hd, 3.2, 5.0, w=4.0, d=1.8, inner='A')
    s.ell((-3.4, 12.6, -3.0), (3.2, 3.0, 2.0), 'A', part=1)                 # bow on the left ear
    s.ell((-3.4, 12.6, -4.0), (1.4, 1.4, 1.0), 'C', part=1)
    s.part(30, 1, (-3.6, 12.2, -2.6), CLOTH)                                # ear-bow ribbon sways
    s.seg((-3.6, 12.2, -2.6), (-5.2, 9.4, -0.8), 2.0, 1.2, 'A', part=30)
    s.part(31, 30, (-5.2, 9.4, -0.8), CLOTH)
    s.seg((-5.2, 9.4, -0.8), (-5.0, 6.8, 1.2), 2.0, 1.2, 'A', part=31)
    s.ell((0, 7.8, -3.6), (4.4, 3.0, 2.2), 'A')                             # neck bow
    s.ell((0, 7.8, -4.7), (1.6, 1.6, 1.0), 'C')
    # ribbon feelers reaching forward to grab (arm roles so Grab reads)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        a = (sx * 1.2, 7.8, -3.6)
        m = (sx * 4.4, 9.4, -5.2)
        b = (sx * 5.4, 6.2, -6.4)
        s.part(pid, 0, a, role)
        s.seg(a, m, 2.0, 1.2, 'A', part=pid)
        s.seg(m, b, 2.0, 1.2, 'A', part=pid)
        s.ell(b, (1.8, 1.8, 1.8), 'C', part=pid)
    tail_chain(s, 12, 0, [(0, 6.0, 4.4), (0, 8.0, 6.8), (0, 11.2, 7.2)], [(2.8, 2.8), (2.6, 2.6)], 'A')
    return s


# ============================================================================================ big favourites
def limb(s, pid, parent, role, pts, w, h=None, t='P', kind='Ellipsoid', end=None, end_t=None):
    """Rigid multi-segment limb (one part) through pts; optional end blob (hand/foot) of size `end`."""
    s.part(pid, parent, pts[0], role)
    for a, b in zip(pts, pts[1:]):
        s.seg(a, b, w, h if h is not None else w, t, part=pid, kind=kind)
    if end:
        s.ell(pts[-1], end, end_t or t, part=pid)


def leg3(s, pid, role, hip, knee, ankle, w, t_up='P', t_low='P', foot=None, foot_t='P', claws=None):
    """Digitigrade / bent leg: thigh + shin (+ foot ellipsoid) on one part pivoting at the hip."""
    s.part(pid, 0, hip, role)
    s.seg(hip, knee, w * 1.2, w * 1.2, t_up, part=pid, kind='Ellipsoid')
    s.seg(knee, ankle, w, w, t_low, part=pid, kind='Ellipsoid')
    if foot:
        fc = (ankle[0], foot[1] * 0.5, ankle[2] - foot[2] * 0.3)
        s.seg(ankle, (fc[0], fc[1], fc[2] + foot[2] * 0.2), w, w, t_low, part=pid)   # solid ankle joint
        s.ell(fc, foot, foot_t, part=pid)
        if claws:
            for k in (-1, 0, 1):
                s.spike((fc[0] + k * foot[0] * 0.3, fc[1], fc[2] - foot[2] * 0.4),
                        (fc[0] + k * foot[0] * 0.35, fc[1] - 0.2, fc[2] - foot[2] * 0.5 - 1.4), 1.2, claws, part=pid)


def lucario():
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):                      # black thighs, blue shins
        leg3(s, pid, role, (sx * 1.8, 7.4, 0.4), (sx * 2.0, 4.4, -1.0), (sx * 2.0, 1.6, 0.6), 3.0,
             t_up='D', t_low='P', foot=(2.6, 1.4, 3.8))
    s.ell((0, 10.2, 0.2), (5.8, 6.8, 4.4), 'P')                            # torso
    s.ell((0, 8.2, 0.2), (5.4, 2.6, 4.2), 'D')                             # black hips
    s.ell((0, 11.4, -1.3), (4.6, 3.8, 2.6), 'A')                           # cream chest fur
    s.spike((0, 11.2, -2.2), (0, 11.2, -4.8), 2.0, 'C', d=2.0)             # chest aura spike
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        limb(s, pid, 0, role, [(sx * 3.2, 12.8, 0), (sx * 4.4, 10.4, -0.8), (sx * 4.8, 8.4, -2.0)], 2.4,
             end=(2.8, 2.8, 2.8), end_t='D')
        s.spike((sx * 4.6, 8.8, -1.4), (sx * 5.0, 9.2, 1.4), 1.8, 'C', part=pid, d=1.8)   # paw spikes
    s.part(1, 0, (0, 13.4, 0), HEAD)
    s.ell((0, 15.8, -0.4), (5.0, 4.6, 4.6), 'P', part=1)
    s.spike((0, 15.2, -2.0), (0, 14.6, -5.6), 2.8, 'P', part=1, d=2.6)      # jackal muzzle
    s.ell((0, 14.8, -5.2), (1.2, 1.0, 1.0), 'D', part=1)                   # nose
    s.box((0, 16.2, -2.2), (5.2, 1.6, 1.4), 'D', part=1)                   # black mask band
    for sx in (-1, 1):
        s.box((sx * 1.3, 16.3, -2.9), (0.9, 0.8, 0.5), 'C', part=1)        # eyes glint in the mask
        s.spike((sx * 1.4, 17.4, 0.0), (sx * 2.6, 20.6, 0.6), 2.2, 'P', part=1, d=1.6)   # tall ears
    for pid, sx in ((30, -1), (32, 1)):                                    # aura-sensor tassels (sway)
        s.part(pid, 1, (sx * 1.0, 16.4, 1.8), CLOTH)
        s.seg((sx * 1.0, 16.4, 1.8), (sx * 1.8, 13.8, 3.2), 1.6, 1.6, 'D', part=pid, kind='Ellipsoid')
        s.ell((sx * 1.9, 13.2, 3.4), (1.8, 2.2, 1.8), 'D', part=pid)
        s.part(pid + 1, 1, (sx * 1.6, 16.8, 1.6), CLOTH)
        s.seg((sx * 1.6, 16.8, 1.6), (sx * 3.0, 14.8, 3.6), 1.6, 1.6, 'D', part=pid + 1, kind='Ellipsoid')
        s.ell((sx * 3.2, 14.2, 3.9), (1.8, 2.2, 1.8), 'D', part=pid + 1)
    tail_chain(s, 12, 0, [(0, 8.2, 2.2), (0, 6.4, 4.4)], [(2.2, 2.2)], 'P')
    return s


def greninja():
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):                      # crouched frog legs, big webbed feet
        leg3(s, pid, role, (sx * 2.0, 9.4, 0.8), (sx * 3.4, 5.6, -2.0), (sx * 2.8, 1.8, 1.0), 3.0,
             foot=(3.6, 1.4, 5.6), foot_t='P')
        s.ell((sx * 3.3, 5.4, -2.3), (1.8, 1.8, 1.8), 'C', part=pid)       # white knee marks
    s.ell((0, 12.2, 0.8), (5.0, 7.0, 3.8), 'P', (12, 0, 0))                # slim leaning torso
    s.ell((0, 12.0, -0.8), (3.4, 4.6, 1.8), 'C', (12, 0, 0))               # pale chest
    # right arm cocked back holding a water shuriken, left arm forward
    limb(s, 8, 0, ARM_L, [(-2.8, 14.4, 0.2), (-4.6, 11.8, -1.6), (-4.4, 10.2, -4.2)], 2.6, end=(3.0, 3.0, 3.0))
    limb(s, 9, 0, ARM_R, [(2.8, 14.4, 0.2), (5.2, 14.4, 1.4), (5.6, 17.4, 2.4)], 2.6, end=(3.0, 3.0, 3.0))
    s.cyl((5.8, 19.8, 2.6), (3.2, 0.9, 3.2), 'C', ALONG_Z, part=9)         # water shuriken
    for k in range(4):
        a = math.radians(45 + 90 * k)
        s.spike((5.8 + math.cos(a) * 1.2, 19.8 + math.sin(a) * 1.2, 2.6),
                (5.8 + math.cos(a) * 3.6, 19.8 + math.sin(a) * 3.6, 2.6), 2.0, 'C', part=9, d=1.0)
    s.part(1, 0, (0, 15.6, -0.2), HEAD)
    s.ell((0, 18.4, -0.8), (6.4, 4.8, 5.8), 'P', part=1)                   # wide frog head
    for sx in (-1, 1):
        s.spike((sx * 1.6, 19.8, 0.8), (sx * 3.0, 19.4, 5.6), 2.8, 'P', part=1, d=2.2)   # hood points
        s.ell((sx * 1.8, 18.8, -3.3), (1.8, 1.2, 0.8), 'C', part=1)                     # eyes
        s.ell((sx * 1.8, 18.7, -3.6), (0.8, 0.8, 0.5), 'D', part=1)
    s.box((0, 19.8, -2.9), (5.6, 1.0, 1.0), 'D', part=1)                   # brow / mask line
    s.ell((0, 16.9, -2.2), (4.6, 1.6, 2.8), 'C', part=1)                   # pale jaw
    # the long pink tongue scarf wrapped round the neck, tails streaming behind
    s.ell((0, 15.4, -0.2), (5.8, 2.8, 5.0), 'A')
    s.part(30, 0, (0.4, 15.2, 2.0), CLOTH)
    s.seg((0.4, 15.2, 2.0), (-1.4, 13.4, 5.6), 2.6, 1.2, 'A', part=30)
    s.part(31, 30, (-1.4, 13.4, 5.6), CLOTH)
    s.seg((-1.4, 13.4, 5.6), (-2.6, 10.6, 8.6), 2.4, 1.2, 'A', part=31)
    s.part(32, 0, (-0.4, 15.0, 2.0), CLOTH)
    s.seg((-0.4, 15.0, 2.0), (1.2, 12.0, 5.0), 2.4, 1.2, 'A', part=32)
    return s


def garchomp():
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        leg3(s, pid, role, (sx * 3.0, 11.0, 1.2), (sx * 3.6, 6.0, -1.0), (sx * 3.4, 2.0, 0.6), 3.4,
             foot=(3.8, 2.0, 5.2), claws='C')
        s.spike((sx * 3.8, 6.2, -1.6), (sx * 4.0, 6.4, -4.6), 2.2, 'C', part=pid, d=2.2)   # knee spikes
    s.ell((0, 14.4, 1.2), (9.0, 11.0, 7.4), 'P', (10, 0, 0))               # torso
    s.ell((0, 13.2, -1.4), (6.4, 8.4, 3.6), 'A', (10, 0, 0))               # red belly
    s.box((0, 21.4, 5.8), (1.8, 8.0, 5.0), 'P', (40, 0, 0))                # dorsal fin
    s.box((0, 24.0, 8.4), (1.8, 2.4, 2.4), 'P', (40, 0, 0), sub=True)      # ...with its notch
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                  # arm fins = blades
        limb(s, pid, 0, role, [(sx * 4.2, 18.0, 0.0), (sx * 6.4, 14.4, -1.4), (sx * 6.8, 11.2, -3.2)], 2.6)
        s.box((sx * 7.2, 13.4, 0.8), (1.2, 7.4, 4.2), 'P', (-35, 0, 0), part=pid)      # big arm fin
        s.box((sx * 7.3, 13.4, 0.8), (1.0, 7.0, 1.2), 'D', (-35, 0, 0), part=pid)
        for k in (-1, 1):                                                  # white claws
            s.spike((sx * 6.8 + k * 0.6, 11.0, -3.4), (sx * 6.9 + k * 0.8, 9.0, -5.6), 1.6, 'C', part=pid)
    s.part(1, 0, (0, 19.6, -1.6), HEAD)
    s.seg((0, 18.4, -0.8), (0, 22.0, -2.4), 4.6, 4.6, 'P', part=1, kind='Ellipsoid')
    s.ell((0, 23.0, -3.2), (7.4, 5.4, 7.6), 'P', part=1)                   # hammerhead
    for sx in (-1, 1):
        s.spike((sx * 3.0, 23.6, -3.0), (sx * 7.2, 24.0, 2.2), 3.4, 'P', part=1, d=3.0)   # jet lobes
        s.ell((sx * 2.6, 23.8, -6.2), (1.4, 1.0, 0.8), 'C', part=1)
        s.ell((sx * 2.6, 23.7, -6.5), (0.6, 0.6, 0.5), 'D', part=1)
    s.spike((0, 25.4, -2.8), (0, 27.6, 1.2), 2.6, 'P', part=1, d=2.0)      # head crest fin
    s.box((0, 21.8, -6.2), (5.6, 1.0, 3.2), 'D', part=1)                   # mouth line
    s.part(2, 1, (0, 22.0, -1.6), JAW)
    s.ell((0, 20.8, -4.6), (5.8, 2.6, 5.8), 'A', part=2)                   # red lower jaw
    s.box((0, 21.8, -6.8), (4.6, 0.9, 1.0), 'C', part=2)                   # teeth
    ids = tail_chain(s, 12, 0, [(0, 11.0, 5.0), (0, 8.0, 10.0), (0, 5.4, 14.0), (0, 4.4, 17.4)],
                     [(5.4, 5.4), (4.2, 4.2), (3.0, 3.0)], 'P')
    s.box((0, 6.4, 17.0), (1.4, 5.0, 3.4), 'P', (30, 0, 0), part=ids[-1])  # tail fin
    return s


def tyranitar():
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):                      # tree-trunk legs
        s.part(pid, 0, (sx * 4.2, 11.0, 1.0), role)
        s.ell((sx * 4.4, 8.4, 1.0), (6.0, 8.4, 6.8), 'P', part=pid)
        s.box((sx * 4.4, 3.2, 0.2), (5.0, 5.4, 5.4), 'P', part=pid)
        s.ell((sx * 4.4, 0.9, -0.8), (6.0, 1.8, 7.6), 'P', part=pid)
        for k in (-1, 0, 1):
            s.spike((sx * 4.4 + k * 1.8, 1.0, -4.0), (sx * 4.4 + k * 2.0, 0.6, -6.0), 1.8, 'D', part=pid)
    s.ell((0, 15.6, 1.6), (12.4, 14.0, 10.0), 'P')                         # massive torso
    s.box((0, 13.4, -3.2), (4.6, 4.6, 1.6), 'A', (0, 0, 45))               # blue belly diamond
    s.box((0, 13.4, -3.8), (1.8, 1.8, 1.0), 'D', (0, 0, 45))
    for sx in (-1, 1):
        s.box((sx * 3.2, 18.4, -2.8), (3.0, 3.0, 1.4), 'A', (0, 0, 45))    # chest triangles
        s.ell((sx * 5.8, 19.8, 0.4), (4.6, 4.2, 5.2), 'P')                 # armoured shoulders
        s.spike((sx * 6.2, 21.0, 0.8), (sx * 8.0, 23.6, 1.8), 2.8, 'P', d=2.8)
    for y, z, h in ((21.6, 5.0, 4.6), (17.6, 6.8, 4.2), (13.4, 7.2, 3.6)):   # rows of back spikes
        for sx in (-1, 1):
            s.spike((sx * 2.4, y, z - 1.0), (sx * 3.4, y + h * 0.6, z + h * 0.7), 3.2, 'P', d=3.2)
        s.spike((0, y + 1.0, z - 0.6), (0, y + h, z + h * 0.8), 3.4, 'P', d=3.4)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        limb(s, pid, 0, role, [(sx * 6.2, 18.4, -0.4), (sx * 7.4, 15.6, -2.4), (sx * 7.2, 13.6, -4.2)], 3.0,
             end=(3.0, 3.0, 3.0))
        for k in (-1, 0, 1):
            s.spike((sx * 7.2 + k * 0.9, 13.4, -5.0), (sx * 7.2 + k * 1.0, 12.4, -6.8), 1.4, 'D', part=pid)
    s.part(1, 0, (0, 22.4, -2.0), HEAD)
    s.ell((0, 25.2, -3.4), (6.8, 5.8, 7.2), 'P', part=1)
    s.box((0, 26.4, -6.6), (6.2, 1.4, 1.6), 'D', part=1)                   # heavy brow
    for sx in (-1, 1):
        s.ell((sx * 1.9, 25.6, -6.7), (1.4, 0.9, 0.6), 'C', part=1)
        s.spike((sx * 2.6, 25.8, -1.2), (sx * 5.6, 25.2, 2.4), 2.8, 'P', part=1, d=2.6)   # crown spikes
        s.spike((sx * 2.4, 23.6, -1.4), (sx * 5.0, 22.2, 1.4), 2.4, 'P', part=1, d=2.4)
    s.spike((0, 27.6, -3.6), (0, 32.0, -2.4), 3.0, 'P', part=1, d=3.0)     # horn
    s.part(2, 1, (0, 23.8, -1.8), JAW)
    s.ell((0, 23.0, -5.0), (5.6, 2.4, 5.4), 'P', part=2)
    s.box((0, 24.0, -7.4), (4.4, 0.9, 1.0), 'C', part=2)                   # teeth
    ids = tail_chain(s, 12, 0, [(0, 11.0, 6.4), (0, 7.6, 11.4), (0, 5.2, 15.4), (0, 4.2, 19.0)],
                     [(6.4, 6.4), (5.0, 5.0), (3.6, 3.6)], 'P')
    for k, pid in enumerate(ids):                                          # tail spikes
        z = (6.4, 11.4, 15.4)[k] + 2.2
        y = (11.0, 7.6, 5.2)[k] - 1.0
        s.spike((0, y + 1.8, z), (0, y + 4.4 - k * 0.6, z + 2.0), 2.8 - k * 0.3, 'P', part=pid, d=2.8)
    return s


def dragonite():
    s = S()
    s.ell((0, 13.0, 0.8), (12.4, 16.0, 10.4), 'P')                         # pot belly
    s.ell((0, 12.2, -2.6), (9.2, 12.4, 5.6), 'A')                          # cream belly
    for y in (8.4, 11.6, 14.8):                                            # belly plate lines
        s.box((0, y, -5.1), (7.6, 0.7, 1.2), 'P')
    biped_legs(s, 3.2, 6.0, w=4.2, d=4.6, foot_len=6.4, claws='C')
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        limb(s, pid, 0, role, [(sx * 5.2, 18.0, -0.4), (sx * 7.0, 14.6, -2.8)], 3.0, end=(3.2, 3.2, 3.2))
        for k in (-1, 0, 1):
            s.spike((sx * 7.0 + k * 0.9, 13.8, -3.8), (sx * 7.0 + k * 1.0, 12.6, -5.2), 1.3, 'C', part=pid)
    s.part(1, 0, (0, 20.4, -0.4), HEAD)
    s.seg((0, 19.6, 0.0), (0, 24.6, -1.0), 5.4, 5.4, 'P', part=1, kind='Ellipsoid')      # neck
    s.ell((0, 27.2, -1.4), (7.0, 6.4, 7.0), 'P', part=1)
    s.ell((0, 26.0, -4.8), (5.4, 4.2, 4.4), 'P', part=1)                   # round snout
    for sx in (-1, 1):
        s.ell((sx * 0.9, 26.8, -7.0), (0.7, 0.6, 0.4), 'D', part=1)        # nostrils
        s.ell((sx * 2.0, 28.4, -4.4), (1.4, 1.8, 0.8), 'D', part=1)        # friendly eyes
        s.box((sx * 2.0, 28.9, -4.8), (0.6, 0.6, 0.5), 'C', part=1)
        s.spike((sx * 1.8, 29.8, 0.2), (sx * 2.4, 32.4, 1.4), 2.0, 'P', part=1, d=2.0)   # little horns
        # the long curling antennae
        s.seg((sx * 0.7, 30.0, -3.4), (sx * 1.4, 33.6, -5.4), 1.6, 1.6, 'P', part=1)
        s.seg((sx * 1.4, 33.6, -5.4), (sx * 3.4, 34.2, -7.4), 1.6, 1.6, 'P', part=1)
    s.box((0, 24.8, -6.4), (3.6, 0.6, 0.8), 'D', part=1)                   # smile
    wing_pair(s, 3.2, 19.4, 3.8, 8.0, 5.6, t='D', ids=(10, 11), tilt=35, sweep=-20, thick=1.2, bone='P')
    tail_chain(s, 12, 0, [(0, 8.0, 5.0), (0, 5.2, 10.0), (0, 3.4, 14.4)], [(5.4, 5.4), (3.8, 3.8)], 'P')
    return s


def lapras():
    s = S()
    s.ell((0, 8.0, 3.0), (15.0, 10.0, 20.0), 'P')                          # body
    s.ell((0, 5.6, 1.0), (12.0, 5.0, 16.0), 'P')
    s.ell((0, 11.0, 5.0), (15.6, 8.4, 17.0), 'A')                          # big shell
    for x, z in ((0, -0.6), (-4.4, 2.0), (4.4, 2.0), (-4.6, 7.6), (4.6, 7.6), (0, 10.6)):   # rounded shell knobs
        y = 11.0 + 4.2 * math.sqrt(max(0.0, 1 - (x / 7.8) ** 2 - ((z - 5.0) / 8.5) ** 2))
        s.ell((x, y + 0.4, z), (3.4, 3.0, 3.4), 'A')
        s.cone((x, y + 2.0, z), (2.2, 2.4, 2.2), 'A')
    for pid, sx, z, span in ((20, -1, -2.6, 10.0), (21, 1, -2.6, 10.0), (22, -1, 10.0, 6.0), (23, 1, 10.0, 6.0)):
        s.part(pid, 0, (sx * 6.4, 6.4, z), OTHER)                          # four flippers (paddle sway)
        s.ell((sx * (6.4 + span * 0.45), 4.4, z - 1.2), (span, 1.6, span * 0.45), 'P', (0, sx * 20, sx * -12), part=pid)
    s.part(1, 0, (0, 10.0, -5.4), HEAD)                                   # long neck + head
    s.seg((0, 9.0, -5.0), (0, 18.0, -9.4), 5.0, 5.0, 'P', part=1, kind='Ellipsoid')
    s.seg((0, 17.0, -9.2), (0, 25.4, -10.4), 4.4, 4.4, 'P', part=1, kind='Ellipsoid')
    s.ell((0, 27.0, -11.6), (6.0, 5.4, 7.4), 'P', part=1)
    s.ell((0, 26.0, -14.2), (4.4, 3.4, 3.4), 'P', part=1)                  # snout
    s.spike((0, 29.2, -11.8), (0, 32.4, -10.2), 2.4, 'P', part=1, d=2.4)   # curled horn
    for sx in (-1, 1):
        s.spike((sx * 2.6, 28.4, -10.2), (sx * 4.6, 30.6, -9.4), 2.4, 'P', part=1, d=1.6)  # ears
        s.ell((sx * 2.2, 27.4, -14.4), (1.4, 1.6, 0.8), 'D', part=1)
        s.box((sx * 2.2, 27.9, -14.8), (0.6, 0.6, 0.5), 'C', part=1)
    s.box((0, 25.2, -15.6), (3.0, 0.6, 0.8), 'D', part=1)                  # mouth
    tail_chain(s, 12, 0, [(0, 6.4, 12.4), (0, 5.4, 15.6), (0, 5.0, 18.2)], [(3.6, 3.0), (2.4, 2.0)], 'P')
    s.part(SEAT_ID, 0, (0, 16.4, 4.4), SEAT)                               # rider sits between the knobs
    s.ell((0, 15.4, 4.4), (4.6, 1.4, 4.4), 'P', part=SEAT_ID)              # saddle blanket
    return s


def mimikyu():
    s = S()
    s.cone((0, 5.2, 0.2), (11.0, 10.4, 10.0), 'P')                         # the draped rag
    s.ell((0, 3.4, 0.2), (10.4, 6.8, 9.6), 'P')
    for k in range(5):                                                     # ragged hem
        a = math.radians(20 + 70 * k)
        s.box((math.cos(a) * 5.0, 0.5, math.sin(a) * 4.6 + 0.2), (1.6, 1.4, 1.6), 'D', (0, 20 * k, 0))
    s.ell((0, 3.8, -4.4), (4.8, 2.0, 1.4), 'D')                            # dark gap under the costume
    for sx in (-1, 1):
        s.ell((sx * 1.1, 3.9, -5.0), (1.0, 1.0, 0.6), 'C')                 # the real eyes peeking out
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):                      # tiny shadow feet
        s.part(pid, 0, (sx * 1.8, 2.0, -0.6), role)
        s.box((sx * 1.8, 0.8, -1.4), (1.8, 1.6, 2.4), 'D', part=pid)
    # shadow claw reaching out from under the rag (Grab)
    s.part(9, 0, (3.4, 3.2, -2.6), ARM_R)
    s.seg((3.4, 3.2, -2.6), (5.4, 4.4, -6.2), 2.0, 2.0, 'D', part=9, kind='Ellipsoid')
    for k in (-1, 0, 1):
        s.spike((5.4 + k * 0.7, 4.6 + k * 0.5, -6.4), (5.8 + k * 1.2, 5.0 + k * 0.9, -8.6), 1.3, 'D', part=9)
    s.part(1, 0, (0, 8.6, 0), HEAD)                                        # lopsided Pikachu-disguise head
    s.ell((0.4, 11.4, -0.4), (8.4, 6.6, 7.0), 'P', (0, 0, -16), part=1)
    s.spike((-2.4, 13.6, -0.2), (-4.2, 17.8, 0.4), 3.2, 'P', part=1, d=1.8)            # upright ear
    s.spike((-3.5, 16.2, 0.2), (-4.3, 18.0, 0.4), 1.8, 'D', part=1, d=2.0)
    s.seg((2.8, 13.4, -0.2), (4.4, 15.6, 0.0), 2.8, 1.8, 'P', part=1, kind='Ellipsoid')  # bent, drooping ear
    s.spike((4.2, 15.6, 0.0), (7.0, 13.8, 0.4), 2.4, 'P', part=1, d=1.8)
    s.spike((5.9, 14.5, 0.3), (7.2, 13.6, 0.4), 1.6, 'D', part=1, d=2.0)
    for sx, dy in ((-1, 0.4), (1, -0.4)):                                  # scribbled face
        s.ell((sx * 1.8, 11.8 + dy, -3.9), (1.4, 1.8, 0.8), 'D', part=1)
        s.ell((sx * 3.1, 10.2 + dy, -3.3), (1.8, 1.4, 0.8), 'A', part=1)
    for k in range(3):
        s.box((-1.2 + k * 1.2, 9.6 + (0.3 if k % 2 else 0), -4.0), (1.2, 0.5, 0.6), 'D', (0, 0, 30 if k % 2 else -30), part=1)
    tail_chain(s, 12, 0, [(0, 4.2, 4.6), (0, 6.6, 6.4), (0, 8.2, 5.4), (0, 10.4, 7.0)],
               [(1.8, 1.8), (1.8, 1.8), (2.2, 2.2)], 'D', kind='Box')
    return s


def ditto():
    s = S()
    s.ell((0, 2.4, 0.2), (14.0, 5.0, 11.0), 'P')                           # melted puddle
    s.ell((5.2, 1.4, 3.2), (4.0, 2.6, 4.0), 'P')                           # uneven drips
    s.ell((-4.6, 1.2, 3.8), (3.4, 2.2, 3.6), 'P')
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                  # stubby pseudopods
        s.part(pid, 0, (sx * 5.0, 4.0, -1.0), role)
        s.ell((sx * 6.4, 4.6, -1.6), (3.2, 2.8, 3.0), 'P', part=pid)
    s.part(1, 0, (0, 4.0, 0), HEAD)
    s.ell((0, 5.6, 0.0), (10.4, 6.6, 8.4), 'P', part=1)
    s.cone((0.6, 9.2, 0.2), (3.4, 2.6, 3.4), 'P', part=1)                  # little top peak
    for sx in (-1, 1):
        s.ell((sx * 1.8, 6.9, -3.6), (1.3, 1.4, 0.9), 'D', part=1)         # dot eyes
        s.box((sx * 2.3, 5.2, -3.8), (0.8, 0.8, 0.6), 'D', part=1)         # smile corners
    s.box((0, 4.7, -4.0), (3.8, 0.7, 0.8), 'D', part=1)                    # wide smile
    return s


def psyduck():
    s = S()
    biped_legs(s, 1.8, 3.0, w=2.0, d=2.2, feet='A', foot_len=4.2)          # cream webbed feet
    s.ell((0, 5.2, 0.4), (7.4, 7.4, 6.2), 'P')                             # round body
    s.part(1, 0, (0, 8.4, 0), HEAD)
    s.ell((0, 11.0, -0.2), (7.8, 6.6, 6.8), 'P', part=1)
    s.ell((0, 9.8, -3.8), (4.8, 1.8, 3.8), 'A', part=1)                    # flat duck bill
    for sx in (-1, 1):
        s.ell((sx * 1.9, 11.8, -2.9), (2.2, 2.4, 1.0), 'C', part=1)        # blank staring eyes
        s.ell((sx * 1.9, 11.8, -3.5), (0.6, 0.6, 0.4), 'D', part=1)
    for dx in (-1, 0, 1):                                                  # three hair strands
        s.seg((dx * 0.7, 13.8, -0.2), (dx * 1.6, 16.2, 0.2), 1.2, 1.2, 'D', part=1)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                  # clutching its aching head
        limb(s, pid, 0, role, [(sx * 3.2, 7.2, 0), (sx * 4.8, 9.2, -0.4), (sx * 4.2, 11.0, -0.8)], 1.8,
             end=(2.0, 2.0, 2.0))
    tail_chain(s, 12, 0, [(0, 3.8, 3.2), (0, 4.8, 4.8)], [(2.2, 2.2)], 'P')
    return s


def slowpoke():
    s = S()
    s.ell((0, 5.2, 1.6), (8.0, 6.4, 11.0), 'P')                            # long lazy body
    quad_legs(s, 2.6, -2.4, 5.0, top=4.0, w=2.6, d=2.6)
    s.part(1, 0, (0, 7.0, -3.4), HEAD)
    s.ell((0, 9.4, -5.2), (8.4, 7.6, 7.8), 'P', part=1)                    # big round head
    s.ell((0, 7.8, -8.8), (6.2, 3.6, 3.8), 'A', part=1)                    # cream muzzle
    s.box((0, 7.4, -10.6), (4.4, 0.6, 0.6), 'D', part=1)                   # vacant grin
    for sx in (-1, 1):
        s.ell((sx * 2.2, 10.4, -8.6), (1.2, 1.4, 0.6), 'D', part=1)
        s.ell((sx * 3.2, 12.8, -4.8), (2.4, 2.4, 1.8), 'P', part=1)        # little round ears
    ids = tail_chain(s, 12, 0, [(0, 6.0, 7.0), (0, 5.2, 10.4), (0, 6.4, 13.4), (0, 9.0, 14.8)],
                     [(3.0, 3.0), (2.6, 2.6), (2.4, 2.4)], 'P')
    s.ell((0, 9.6, 15.0), (3.4, 3.4, 3.4), 'A', part=ids[-1])              # white tail tip (the bait)
    return s


def machamp():
    s = S()
    biped_legs(s, 2.6, 8.0, w=3.4, d=3.4, foot_len=4.6)
    s.box((0, 8.8, 0.3), (7.8, 3.8, 5.0), 'D')                             # black trunks
    s.box((0, 10.8, 0.3), (8.2, 1.4, 5.4), 'A')                            # gold belt
    s.box((0, 10.8, -2.4), (2.4, 1.8, 0.6), 'A')                           # buckle
    s.ell((0, 15.6, 0.4), (10.4, 9.0, 6.2), 'P')                           # V-shaped torso
    s.ell((0, 17.4, -1.6), (8.6, 4.0, 3.0), 'P')                           # pecs
    for sx in (-1, 1):
        s.ell((sx * 5.2, 18.6, 0.4), (4.4, 3.8, 4.4), 'P')                 # big deltoids
        s.box((sx * 1.2, 13.4, -2.4), (1.8, 1.4, 0.8), 'D')                # abs lines
    s.part(1, 0, (0, 19.6, 0), HEAD)
    s.ell((0, 22.0, -0.4), (5.0, 5.2, 5.0), 'P', part=1)
    s.box((0, 25.0, -0.2), (1.4, 2.6, 4.4), 'P', part=1)                   # triple crest
    for sx in (-1, 1):
        s.box((sx * 1.4, 24.4, 0.2), (1.2, 1.8, 3.4), 'P', part=1)
        s.ell((sx * 1.2, 22.6, -2.7), (1.2, 0.8, 0.5), 'C', part=1)
    s.box((0, 21.2, -2.8), (1.8, 0.9, 0.6), 'D', part=1)                   # mouth / big lips
    s.box((0, 23.4, -2.7), (3.6, 0.7, 0.6), 'D', part=1)                   # brow
    # four arms: upper pair flexing overhead, lower pair fists at the hips
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        limb(s, pid, 0, role, [(sx * 5.6, 19.0, 0.2), (sx * 9.2, 20.2, -0.4), (sx * 9.4, 24.4, -1.0)], 3.6,
             end=(3.8, 3.8, 3.8))
        s.ell((sx * 8.8, 21.6, -1.0), (3.6, 3.4, 3.4), 'P', part=pid)      # flexed biceps
    for pid, sx, role in ((10, -1, ARM_L), (11, 1, ARM_R)):
        limb(s, pid, 0, role, [(sx * 4.8, 15.4, 0.6), (sx * 8.0, 12.6, -0.2), (sx * 8.4, 9.4, -1.8)], 3.4,
             end=(3.8, 3.8, 3.8))
    return s


def scyther():
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        leg3(s, pid, role, (sx * 2.0, 10.4, 2.2), (sx * 2.8, 5.8, -0.4), (sx * 2.6, 1.6, 1.8), 3.0,
             foot=(2.4, 1.4, 3.6), claws='D')
    s.ell((0, 10.4, 5.2), (5.6, 5.2, 9.0), 'P', (-20, 0, 0))               # segmented abdomen
    for z in (3.6, 5.8, 8.0):
        s.ell((0, 10.4 - (z - 5.2) * 0.35, z), (5.8, 5.2, 0.9), 'A', (-20, 0, 0))
    s.ell((0, 14.6, 0.6), (5.4, 6.8, 4.8), 'P', (10, 0, 0))                # thorax
    s.ell((0, 14.4, -1.2), (3.8, 4.8, 2.2), 'A', (10, 0, 0))               # cream chest plate
    wing_pair(s, 1.6, 16.4, 2.4, 7.4, 4.4, t='C', ids=(10, 11), tilt=30, sweep=-35, thick=1.6, bone='A')
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                  # scythe arms (Blade)
        s.part(pid, 0, (sx * 2.8, 16.2, -0.6), role)
        s.seg((sx * 2.8, 16.2, -0.6), (sx * 4.6, 13.2, -3.2), 2.6, 2.6, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 4.6, 13.2, -3.2), (2.4, 2.4, 2.4), 'P', part=pid)
        s.seg((sx * 4.6, 13.4, -3.4), (sx * 4.8, 18.6, -6.2), 3.4, 1.8, 'C', part=pid)    # blade
        s.spike((sx * 4.8, 18.4, -6.2), (sx * 4.6, 19.2, -8.6), 2.2, 'C', part=pid, d=1.2)  # hooked tip
    s.part(1, 0, (0, 17.8, -0.4), HEAD)
    s.ell((0, 19.6, -1.6), (4.6, 4.2, 5.6), 'P', part=1)
    s.spike((0, 21.0, -0.8), (0, 22.4, 3.0), 2.4, 'P', part=1, d=2.0)      # swept-back head horn
    for sx in (-1, 1):
        s.ell((sx * 1.9, 20.0, -3.2), (1.4, 1.8, 1.8), 'D', part=1)        # wide insect eyes
        s.box((sx * 1.9, 20.5, -3.9), (0.5, 0.5, 0.5), 'C', part=1)
    s.box((0, 18.4, -4.2), (2.2, 0.8, 0.8), 'D', part=1)                   # mandibles
    return s


def arcanine():
    s = S()
    s.ell((0, 11.0, 1.2), (9.0, 8.0, 15.0), 'P')                           # long powerful body
    for z in (0.8, 3.8, 6.8):                                              # black tiger stripes
        s.ell((0, 12.8, z), (9.4, 5.4, 1.2), 'D')
    quad_legs(s, 3.0, -4.0, 6.6, top=10.0, w=3.2, d=3.2)
    for pid, x, z in ((4, -3.0, -4.0), (5, 3.0, -4.0), (6, -3.0, 6.6), (7, 3.0, 6.6)):
        s.ell((x * 1.08, 7.6, z + 1.4), (3.8, 3.2, 3.2), 'A', part=pid)    # cream leg feathering
        s.box((x, 4.2, z - 0.4), (3.4, 0.8, 3.4), 'D', part=pid)           # leg stripe
    s.ell((0, 15.0, -5.0), (10.6, 9.4, 8.0), 'A')                          # great cream mane
    s.ell((0, 10.4, -6.8), (7.4, 7.4, 4.4), 'A')                           # chest fluff
    s.part(1, 0, (0, 16.0, -6.0), HEAD)
    s.ell((0, 19.8, -8.6), (7.2, 6.2, 7.0), 'P', part=1)
    s.ell((0, 18.4, -12.0), (4.2, 3.4, 4.4), 'P', part=1)                  # muzzle
    s.ell((0, 19.0, -14.1), (1.4, 1.1, 0.8), 'D', part=1)                  # nose
    for sx in (-1, 1):
        s.ell((sx * 2.1, 20.6, -11.4), (1.3, 1.4, 0.7), 'D', part=1)
        s.box((sx * 2.1, 21.0, -11.8), (0.5, 0.5, 0.5), 'C', part=1)
        s.spike((sx * 2.4, 22.0, -7.8), (sx * 3.8, 25.4, -6.8), 3.2, 'P', part=1, d=2.0)  # ears
        s.box((sx * 3.2, 19.4, -10.6), (1.0, 0.8, 2.4), 'D', (0, sx * 20, 0), part=1)    # face stripes
    s.ell((0, 22.8, -8.6), (4.4, 2.8, 4.6), 'A', part=1)                   # cream head tuft
    s.spike((0, 23.0, -9.4), (0, 24.6, -12.4), 2.6, 'A', part=1, d=2.4)
    ids = tail_chain(s, 12, 0, [(0, 13.0, 8.2), (0, 15.8, 11.0), (0, 18.4, 11.4)], [(4.6, 4.6), (6.0, 6.0)], 'A')
    s.ell((0, 18.2, 10.8), (6.4, 6.4, 6.2), 'A', part=ids[-1])             # bushy tail
    return s


def growlithe():
    s = S()
    hd = fox(s, body=(5.6, 5.0, 8.0), hs=(7.2, 6.2, 6.2), muzzle=(3.6, 2.8, 2.8), head_y=9.8)
    for z in (0.4, 2.8):                                                   # black stripes
        s.ell((0, 6.4, z), (5.9, 3.6, 1.0), 'D')
    for sx in (-1, 1):                                                     # floppy-pointed ears
        s.spike((sx * 2.4, 11.6, -3.2), (sx * 5.6, 13.0, -2.6), 3.6, 'P', part=1, d=1.8)
        s.box((sx * 3.0, 9.4, -6.2), (1.0, 0.8, 1.8), 'D', (0, sx * 20, 0), part=1)     # cheek stripes
    s.ell((0, 13.2, -3.6), (4.4, 2.8, 4.2), 'A', part=1)                   # cream head tuft
    s.spike((0, 13.2, -4.4), (0, 14.4, -7.0), 2.6, 'A', part=1, d=2.2)
    s.ell((0, 6.0, -3.6), (4.6, 4.2, 3.2), 'A')                            # chest fluff
    ids = tail_chain(s, 12, 0, [(0, 6.0, 4.2), (0, 8.4, 6.2), (0, 11.0, 6.4)], [(3.2, 3.2), (4.2, 4.2)], 'A')
    s.ell((0, 11.2, 5.8), (4.4, 4.2, 4.2), 'A', part=ids[-1])              # fluffy tail
    return s


# ---------------------------------------------------------------------------------------------- stretch
def mr_mime():
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):                      # thin legs, pink knees, curly shoes
        s.part(pid, 0, (sx * 1.8, 8.0, 0), role)
        s.seg((sx * 1.8, 8.0, 0), (sx * 2.0, 1.4, 0), 2.2, 2.2, 'P', part=pid, kind='Ellipsoid')
        s.ell((sx * 1.9, 4.6, -0.2), (2.4, 2.4, 2.4), 'A', part=pid)
        s.ell((sx * 2.0, 0.8, -0.8), (2.6, 1.6, 4.0), 'D', part=pid)
        s.spike((sx * 2.0, 1.0, -2.6), (sx * 2.0, 2.8, -3.8), 1.8, 'D', part=pid)
    s.ell((0, 10.8, 0.2), (7.0, 6.4, 5.4), 'P')                            # round torso
    s.ell((0, 10.4, -2.3), (3.4, 3.4, 1.2), 'A')                           # pink belly circle
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):                  # palms pressed on an invisible wall
        s.ell((sx * 3.8, 13.2, 0), (2.6, 2.6, 2.6), 'A')                   # pink shoulder balls
        limb(s, pid, 0, role, [(sx * 3.8, 13.2, 0), (sx * 5.2, 13.6, -2.4), (sx * 3.6, 15.4, -4.6)], 2.0)
        s.ell((sx * 3.6, 15.6, -5.2), (3.6, 4.0, 1.4), 'C', part=pid)      # big white gloves
        for k in (-1, 0, 1):
            s.box((sx * 3.6 + k * 1.0, 18.0, -5.2), (0.8, 1.8, 1.0), 'C', part=pid)
    s.part(1, 0, (0, 14.2, 0), HEAD)
    s.ell((0, 16.8, -0.2), (6.0, 5.6, 5.2), 'P', part=1)
    for sx in (-1, 1):
        s.seg((sx * 1.6, 18.8, 0), (sx * 3.8, 21.0, 0.6), 2.4, 2.4, 'D', part=1, kind='Ellipsoid')   # blue hair horns
        s.ell((sx * 4.2, 20.4, 0.8), (2.0, 2.0, 2.0), 'D', part=1)
        s.ell((sx * 1.4, 17.4, -2.5), (0.9, 1.3, 0.5), 'D', part=1)
        s.ell((sx * 2.3, 16.0, -2.1), (1.4, 1.2, 0.6), 'A', part=1)        # red cheeks
    s.box((0, 15.2, -2.6), (1.8, 0.5, 0.5), 'D', part=1)
    return s


def wobbuffet():
    s = S()
    for pid, sx, role in ((4, -1, LBL), (5, 1, LBR)):
        s.part(pid, 0, (sx * 2.0, 2.4, 0), role)
        s.ell((sx * 2.2, 0.9, -0.6), (2.8, 1.8, 3.6), 'P', part=pid)
    s.ell((0, 8.6, 0), (10.4, 14.0, 8.0), 'P')                             # punching-bag body
    s.part(1, 0, (0, 12.0, 0), HEAD)
    s.ell((0, 16.0, 0), (9.0, 10.0, 7.2), 'P', part=1)                     # the head is the top of the body
    for sx in (-1, 1):
        s.box((sx * 2.0, 17.8, -3.4), (2.4, 1.0, 1.2), 'D', part=1)        # squeezed-shut eyes
    s.ell((0, 14.6, -3.4), (3.6, 2.0, 1.2), 'D', part=1)                   # open mouth
    s.part(8, 0, (-4.8, 13.6, 0), ARM_L)                                   # flat arm pressed to the side
    s.ell((-5.4, 10.8, 0), (2.0, 7.0, 3.2), 'P', part=8)
    s.part(9, 0, (4.8, 13.6, 0), ARM_R)                                    # the famous salute
    s.seg((4.8, 13.6, 0), (7.0, 17.4, -1.0), 3.4, 3.4, 'P', part=9, kind='Ellipsoid')
    s.seg((7.0, 17.4, -1.0), (2.6, 19.4, -4.0), 3.2, 3.2, 'P', part=9, kind='Ellipsoid')
    ids = tail_chain(s, 12, 0, [(0, 2.6, 3.8), (0, 3.2, 6.8)], [(2.6, 2.6)], 'A')
    s.ell((0, 4.2, 8.4), (4.4, 4.4, 4.0), 'A', part=ids[-1])               # black tail head
    for sx in (-1, 1):
        s.box((sx * 1.1, 4.8, 10.3), (0.9, 1.0, 0.6), 'C', part=ids[-1])   # its eyes
    return s


def hitmonlee():
    s = S()
    s.ell((0, 16.0, 0), (8.0, 10.0, 6.4), 'P')                             # head-body
    for y in (13.2, 14.8):
        s.box((0, y, -3.0), (5.0, 0.6, 0.8), 'D')                          # belly ridges
    s.part(1, 0, (0, 19.0, 0), HEAD)
    s.ell((0, 20.4, -0.2), (7.0, 6.0, 5.8), 'P', part=1)
    for sx in (-1, 1):
        s.ell((sx * 1.6, 20.6, -2.8), (1.4, 1.2, 0.6), 'D', part=1)
        s.box((sx * 1.6, 20.9, -3.1), (0.5, 0.5, 0.5), 'C', part=1)
    for pid, sx, role in ((8, -1, ARM_L), (9, 1, ARM_R)):
        limb(s, pid, 0, role, [(sx * 3.8, 16.4, 0), (sx * 5.6, 13.4, -0.8)], 1.8, end=(2.4, 2.4, 2.4), end_t='A')
    # left leg snapping out in a high kick (Launch), right leg planted — springy coiled legs
    s.part(4, 0, (-2.0, 12.0, 0), LBL)
    s.seg((-2.0, 12.0, 0), (-2.4, 12.8, -5.2), 2.8, 2.8, 'P', part=4, kind='Ellipsoid')
    s.seg((-2.4, 12.8, -5.2), (-2.4, 14.4, -10.4), 2.8, 2.8, 'P', part=4, kind='Ellipsoid')
    for k in range(3):
        s.ell((-2.3, 12.4 + k * 0.6, -2.4 - k * 3.2), (2.8, 2.8, 1.0), 'D', (-10, 0, 0), part=4)
    s.ell((-2.4, 14.8, -11.6), (3.0, 4.2, 2.4), 'A', (-20, 0, 0), part=4)  # kicking foot
    s.part(5, 0, (2.0, 12.0, 0), LBR)
    s.seg((2.0, 12.0, 0), (2.4, 1.4, 0.2), 2.8, 2.8, 'P', part=5, kind='Ellipsoid')
    for k in range(3):
        s.ell((2.2, 9.6 - k * 2.6, 0.1), (2.8, 1.0, 2.8), 'D', part=5)
    s.ell((2.4, 0.9, -1.2), (3.0, 1.8, 4.6), 'A', part=5)
    return s


def hitmonchan():
    s = S()
    biped_legs(s, 1.8, 7.0, w=2.2, d=2.4, foot_len=3.6)
    s.cyl((0, 8.2, 0.2), (7.2, 3.6, 5.4), 'D')                             # purple tunic skirt
    s.ell((0, 11.4, 0.2), (6.6, 7.4, 4.8), 'P')                            # torso
    s.box((0, 11.6, 0.2), (6.8, 5.2, 5.0), 'D')                            # tunic
    for sx in (-1, 1):
        s.ell((sx * 3.6, 14.2, 0.2), (3.0, 2.2, 3.4), 'D')                 # shoulder pads
    s.part(1, 0, (0, 15.2, 0), HEAD)
    s.ell((0, 17.4, -0.2), (5.2, 5.2, 5.0), 'P', part=1)
    s.box((0, 19.8, 0.2), (1.4, 2.0, 4.4), 'P', part=1)                    # helmet ridge
    for sx in (-1, 1):
        s.ell((sx * 1.4, 17.6, -2.5), (1.2, 1.0, 0.5), 'D', part=1)
    # right jab thrown forward, left glove guarding the chin — red boxing gloves
    limb(s, 9, 0, ARM_R, [(3.4, 13.8, 0), (3.8, 14.0, -3.6), (3.0, 14.4, -6.8)], 2.4, end=(3.8, 3.8, 4.2), end_t='A')
    limb(s, 8, 0, ARM_L, [(-3.4, 13.8, 0), (-4.4, 12.0, -2.2), (-2.6, 15.2, -3.6)], 2.4, end=(3.6, 3.6, 3.8),
         end_t='A')
    return s


# ============================================================================================ register
def register():
    icon(["pichu", "pichuu", "pichoo", "peachu"], "Pichu", "Biped", ["Shoots", "Fast"], 0.3, 2.0,
         element="Electric", accent=(1.0, 0.55, 0.72), temperament="Playful", intensity=0.8, shape=pichu())
    icon(["raichu", "raichuu", "raichoo", "rychu"], "Raichu", "Biped", ["Shoots", "Fast", "ArticulatedTail"], 0.8, 30.0,
         element="Electric", accent=(1.0, 0.84, 0.35), temperament="Playful", intensity=0.5, shape=raichu())
    icon(["vaporeon", "vaporean", "vaporion", "vapereon"], "Vaporeon", "Quadruped", ["Breath", "ArticulatedTail"],
         1.0, 29.0, element="Water", accent=(0.95, 0.94, 0.78), temperament="Calm", intensity=0.5, shape=vaporeon())
    icon(["jolteon", "joltean", "joltion", "jolt eon"], "Jolteon", "Quadruped", ["Shoots", "Fast", "Pierce"],
         0.8, 24.5, element="Electric", accent=(0.97, 0.97, 0.95), temperament="Aggressive", intensity=0.4,
         shape=jolteon())
    icon(["flareon", "flarion", "flarean", "flare eon"], "Flareon", "Quadruped", ["Breath", "Charge"],
         0.9, 25.0, element="Fire", accent=(1.0, 0.86, 0.55), temperament="Playful", intensity=0.4, shape=flareon())
    icon(["espeon", "espion", "espean", "espeeon"], "Espeon", "Quadruped", ["Beam", "Fast"],
         0.9, 26.5, element="Psychic", accent=(0.9, 0.12, 0.3), temperament="Calm", intensity=0.6, shape=espeon())
    icon(["umbreon", "umbrean", "umbrion", "umbreeon"], "Umbreon", "Quadruped", ["Guard", "Blunt"],
         1.0, 27.0, element="Shadow", accent=(1.0, 0.85, 0.2), temperament="Sneaky", intensity=0.7, shape=umbreon())
    icon(["leafeon", "leafion", "leafean", "leaf eon"], "Leafeon", "Quadruped", ["Blade", "Fast"],
         1.0, 25.5, element="Earth", accent=(0.35, 0.8, 0.35), temperament="Calm", intensity=0.4, shape=leafeon())
    icon(["glaceon", "glacion", "glacean", "glaseon"], "Glaceon", "Quadruped", ["Breath", "Pierce"],
         0.8, 25.9, element="Ice", accent=(0.2, 0.4, 0.78), temperament="Calm", intensity=0.4, shape=glaceon())
    icon(["sylveon", "sylvion", "silveon", "sylvean"], "Sylveon", "Quadruped", ["Grab", "Pull"],
         1.0, 23.5, element="Psychic", accent=(1.0, 0.55, 0.75), temperament="Playful", intensity=0.7, shape=sylveon())
    icon(["lucario", "lucareo", "lucaryo", "lukario"], "Lucario", "Biped", ["Shoots", "Homing", "Blunt"],
         1.2, 54.0, element="Light", accent=(0.95, 0.86, 0.55), temperament="Protective", intensity=0.6,
         shape=lucario())
    icon(["greninja", "greninga", "grenija", "gre ninja"], "Greninja", "Biped", ["Shoots", "Blade", "Fast"],
         1.5, 40.0, element="Water", accent=(0.95, 0.5, 0.62), temperament="Sneaky", intensity=0.7, shape=greninja())
    icon(["garchomp", "garchom", "gar chomp", "garchomb"], "Garchomp", "Biped", ["Charge", "Fast", "Blade"],
         1.9, 95.0, element="Earth", accent=(0.85, 0.25, 0.2), temperament="Aggressive", intensity=0.8,
         shape=garchomp())
    icon(["tyranitar", "tyrannitar", "tyranatar", "tyrantar"], "Tyranitar", "Biped",
         ["Heavy", "Stomp", "ArticulatedTail"], 2.0, 202.0, element="Earth", accent=(0.42, 0.52, 0.78),
         temperament="Aggressive", intensity=0.7, shape=tyranitar())
    icon(["dragonite", "dragonight", "dragonnite", "dragonit"], "Dragonite", "Biped", ["Flies", "Blunt", "Beam"],
         2.2, 210.0, element="Light", accent=(0.97, 0.88, 0.62), temperament="Protective", intensity=0.5,
         shape=dragonite())
    icon(["lapras", "lapris", "laprass", "lapres"], "Lapras", "Fish", ["Piloted", "Breath"],
         2.5, 220.0, element="Water", accent=(0.72, 0.68, 0.8), temperament="Calm", intensity=0.7, shape=lapras())
    icon(["mimikyu", "mimikyuu", "mimikyo", "mimic kyu"], "Mimikyu", "Biped", ["Grab", "Guard"],
         0.2, 0.7, element="Shadow", accent=(1.0, 0.5, 0.55), temperament="Sneaky", intensity=0.8, shape=mimikyu())
    icon(["ditto", "dito", "dittoh"], "Ditto", "Blob", ["Bounce", "Sticky"],
         0.3, 4.0, element="Light", accent=(0.82, 0.66, 0.96), temperament="Chaotic", intensity=0.7, shape=ditto())
    icon(["psyduck", "psyduk", "sy duck", "sighduck"], "Psyduck", "Biped", ["Repel", "Beam"],
         0.8, 19.6, element="Psychic", accent=(1.0, 0.9, 0.68), temperament="Chaotic", intensity=0.5,
         shape=psyduck())
    icon(["slowpoke", "slowpok", "slow poke", "slowpoak"], "Slowpoke", "Quadruped", ["Pull", "ArticulatedTail"],
         1.2, 36.0, element="Water", accent=(0.98, 0.9, 0.76), temperament="Lazy", intensity=0.9, shape=slowpoke())
    icon(["machamp", "machamps", "mashamp", "machomp"], "Machamp", "Biped", ["Grab", "Blunt"],
         1.6, 130.0, element="Light", accent=(0.95, 0.78, 0.3), temperament="Aggressive", intensity=0.6,
         shape=machamp())
    icon(["scyther", "scyter", "sythe", "scythr"], "Scyther", "Insect", ["Blade", "Flies", "Fast"],
         1.5, 56.0, element="Wind", accent=(0.96, 0.94, 0.82), temperament="Aggressive", intensity=0.6,
         shape=scyther())
    icon(["arcanine", "arcanin", "arkanine", "arcanien"], "Arcanine", "Quadruped", ["Charge", "Fast", "Breath"],
         1.9, 155.0, element="Fire", accent=(0.98, 0.9, 0.7), temperament="Protective", intensity=0.6,
         shape=arcanine())
    icon(["growlithe", "growlith", "growlie", "growlyth"], "Growlithe", "Quadruped", ["Breath", "Fast"],
         0.7, 19.0, element="Fire", accent=(0.98, 0.9, 0.7), temperament="Protective", intensity=0.7,
         shape=growlithe())
    icon(["mr. mime", "mr mime", "mister mime", "mr.mime"], "Mr. Mime", "Biped", ["Barrier", "Repel"],
         1.3, 54.5, element="Psychic", accent=(1.0, 0.55, 0.68), temperament="Playful", intensity=0.6,
         shape=mr_mime(), id="mr_mime")
    icon(["wobbuffet", "wobbufet", "wobufet", "wobbafet"], "Wobbuffet", "Biped", ["Guard", "Bounce"],
         1.3, 28.5, element="Psychic", accent=(0.12, 0.12, 0.18), temperament="Protective", intensity=0.5,
         shape=wobbuffet())
    icon(["hitmonlee", "hitmonly", "hitmonle", "hit mon lee"], "Hitmonlee", "Biped", ["Launch", "Blunt"],
         1.5, 49.8, element="Light", accent=(0.95, 0.85, 0.65), temperament="Aggressive", intensity=0.5,
         shape=hitmonlee())
    icon(["hitmonchan", "hitmonchon", "hitmanchan", "hit mon chan"], "Hitmonchan", "Biped", ["Blunt", "Fast"],
         1.4, 50.2, element="Light", accent=(0.9, 0.18, 0.15), temperament="Aggressive", intensity=0.6,
         shape=hitmonchan())
