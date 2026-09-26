"""Giants: Devastator (six Constructicons -> one combiner), Unicron (planet <-> robot)."""
import math

from tflib import TF, axes
from bots_a import arm

GREEN = (0.36, 0.84, 0.26)
ORANGE = (0.96, 0.52, 0.12)


# ------------------------------------------------------------------------------ Devastator

def _wheels(b, x0, z_list, W, d=3.2, y=None):
    for z in z_list:
        for sx in (-1, 1):
            b.cx((x0 + sx * (W / 2 + 0.2), d / 2 if y is None else y, z), d, 1.0, "D")


def devastator():
    """Alt 'mode' = the six Constructicons parked in formation; robot = G1 Devastator. Each vehicle is
    one rigid panel that becomes a limb (Scrapper right leg, Mixmaster left leg, Long Haul waist, Hook
    torso + crane on the back, Scavenger right arm with the shovel as its claw, Bonecrusher left arm
    with the dozer blade as its hand)."""
    t = TF(k=1.0)
    # --- Hook (crane truck) -> torso (part 0); boom -> rises behind the head
    hx, hz = 0.0, -10.0
    h = t.panel(0, anchor=(hx, 7.0, hz))
    h.bx((hx, 3.6, hz + 1.0), (7.0, 3.2, 15.0), "A")                                         # truck body
    h.bx((hx, 7.2, hz - 5.2), (7.0, 4.6, 4.4), "A")                                          # cab
    h.bx((hx, 8.2, hz - 7.5), (5.6, 2.0, 0.4), "P")                                          # windshield
    h.bx((hx, 6.4, hz + 2.4), (5.6, 3.2, 5.6), "P")                                          # crane turret (purple)
    h.bx((hx - 1.6, 8.4, hz + 2.4), (2.0, 1.2, 2.4), "C")                                    # operator window
    _wheels(h, hx, (hz - 5.0, hz + 3.0, hz + 6.0), 7.0)
    t.place(0, at=(0, 28.0, -1.6), rot=(-90, 0, 0))
    bm = t.panel(6, anchor=(hx, 8.0, hz + 3.6))
    ang = 70.0
    L = 16.0
    cy_, cz_ = 8.0 + math.sin(math.radians(ang)) * L / 2, hz + 3.6 - math.cos(math.radians(ang)) * L / 2
    bm.bx((hx, cy_, cz_), (1.6, L, 1.6), "D", rot=(-(90 - ang), 0, 0))                     # crane boom (raised)
    bm.bx((hx, 8.0 + math.sin(math.radians(ang)) * L + 0.4, hz + 3.6 - math.cos(math.radians(ang)) * L),
          (1.2, 2.4, 1.2), "P")                                                              # hook block
    t.place(6, at=(0, 31.0, 4.6), rot=(90 - ang, 0, 0), parent=0)
    # --- Long Haul (dump truck) -> waist
    lx, lz = 0.0, 10.0
    lh = t.panel(8, anchor=(lx, 3.0, lz))
    lh.bx((lx, 5.0, lz - 5.0), (7.0, 5.6, 4.0), "A")                                         # cab
    lh.bx((lx, 5.8, lz - 7.1), (5.4, 2.0, 0.4), "P")
    lh.bx((lx, 5.6, lz + 2.6), (7.6, 4.8, 10.0), "A")                                        # dump bed
    lh.bx((lx, 8.2, lz + 2.6), (6.6, 0.4, 9.0), "P")                                         # load (purple)
    _wheels(lh, lx, (lz - 5.0, lz + 1.0, lz + 5.0), 7.0, d=3.6)
    t.place(8, at=(0, 19.0, 0.4), rot=(0, 90, 0), parent=0, role="Other")
    # --- legs: Scrapper (payloader, right) + Mixmaster (cement mixer, left), rear ends become feet
    sx_, sz = 16.0, -10.0
    s = t.panel(1, anchor=(sx_, 7.0, sz - 8.0))
    s.bx((sx_, 3.8, sz + 0.5), (7.0, 4.0, 13.0), "A")                                        # body
    s.bx((sx_, 7.4, sz + 2.5), (4.0, 3.4, 4.0), "P")                                         # cab
    s.bx((sx_, 2.2, sz - 7.4), (7.8, 3.0, 2.0), "D")                                         # loader bucket -> knee
    s.bx((sx_, 1.4, sz + 7.4), (7.0, 2.8, 1.8), "D")                                         # counterweight -> foot
    _wheels(s, sx_, (sz - 3.5, sz + 4.5), 7.0, d=4.2)
    t.alt_part(1, 0, (sx_, 7.0, sz - 8.0), "Other")
    t.place(1, at=(5.2, 16.4, 2.0), rot=(90, 0, 0), parent=0, pivot=(5.2, 16.4, 0.4), role="LegBackRight")
    mx, mz = -16.0, -10.0
    m = t.panel(2, anchor=(mx, 7.0, mz - 8.0))
    m.bx((mx, 2.6, mz + 0.5), (7.0, 2.6, 15.0), "A")                                         # chassis
    m.bx((mx, 6.2, mz - 5.0), (7.0, 5.0, 4.0), "A")                                          # cab
    m.bx((mx, 7.0, mz - 7.1), (5.4, 2.0, 0.4), "P")
    m.el((mx, 7.2, mz + 3.0), (6.2, 6.2, 9.6), "P", rot=(-12, 0, 0))                        # mixer drum
    m.bx((mx, 1.2, mz + 7.6), (7.0, 2.2, 1.6), "D")                                          # rear -> foot
    _wheels(m, mx, (mz - 4.5, mz + 3.0, mz + 6.0), 7.0)
    t.alt_part(2, 0, (mx, 7.0, mz - 8.0), "Other")
    t.place(2, at=(-5.2, 16.4, 2.0), rot=(90, 0, 0), parent=0, pivot=(-5.2, 16.4, 0.4), role="LegBackLeft")
    # --- arms: Scavenger (excavator) right, Bonecrusher (bulldozer) left: front ends become the hands
    vx, vz = 16.0, 10.0
    v = t.panel(3, anchor=(vx, 7.6, vz + 7.0))
    for side in (-1, 1):
        v.bx((vx + side * 2.6, 1.3, vz), (2.0, 2.6, 12.0), "D")                               # tracks
    v.bx((vx, 4.8, vz + 1.0), (6.4, 3.8, 9.0), "A")                                          # house
    v.bx((vx - 1.8, 7.6, vz - 2.4), (2.6, 2.4, 3.0), "P")                                    # cab
    v.bx((vx + 1.2, 7.2, vz - 5.0), (1.6, 1.6, 6.0), "D", rot=(-25, 0, 0))                  # boom
    t.alt_part(3, 0, (vx, 7.6, vz + 7.0), "Other")
    t.place(3, at=(10.8, 35.0, -3.0), rot=(-90, 0, 0), parent=0, pivot=(10.8, 34.0, 0.4), role="ArmRight")
    sh = t.panel(7, anchor=(vx + 1.2, 6.0, vz - 8.0))                                        # shovel -> claw hand
    sh.bx((vx + 1.2, 4.4, vz - 9.0), (3.6, 3.0, 2.8), "P")
    for tx in (-1.2, 0.0, 1.2):
        sh.cone((vx + 1.2 + tx, 2.4, vz - 9.6), 0.8, 1.4, "C", dir="-y")                     # teeth
    t.follow(7, 3)
    bx, bz = -16.0, 10.0
    b = t.panel(4, anchor=(bx, 7.2, bz + 6.0))
    for side in (-1, 1):
        b.bx((bx + side * 2.6, 1.3, bz), (2.0, 2.6, 11.0), "D")
    b.bx((bx, 4.4, bz + 0.6), (5.4, 3.6, 8.0), "A")                                          # body
    b.bx((bx, 7.2, bz + 2.4), (3.4, 2.6, 3.4), "P")                                          # cab
    b.bx((bx, 2.8, bz - 6.6), (8.4, 4.4, 1.0), "A", rot=(15, 0, 0))                          # dozer blade -> hand
    t.alt_part(4, 0, (bx, 7.2, bz + 6.0), "Other")
    t.place(4, at=(-10.8, 35.0, -3.0), rot=(-90, 0, 0), parent=0, pivot=(-10.8, 34.0, 0.4), role="ArmLeft")
    # --- robot-only: hips block, head
    hp = t.robot_only(20, 0, (0, 17.4, 0.4), "Other")
    hp.bx((0, 17.0, 0.6), (9.0, 3.4, 5.4), "D")
    hy = 36.4
    hd = t.robot_only(23, 0, (0, hy, 0.4), "Head")
    hd.bx((0, hy + 0.8, 0.4), (2.6, 1.6, 2.6), "D")
    hd.bx((0, hy + 3.4, 0.4), (5.6, 4.6, 5.0), "P")                                          # purple helmet
    hd.bx((0, hy + 3.6, -2.2), (4.4, 1.2, 0.4), "C")                                         # red visor
    hd.bx((0, hy + 2.2, -2.15), (3.0, 1.4, 0.4), "A")                                        # green mouthplate
    for sx in (-1, 1):
        hd.bx((sx * 3.0, hy + 4.4, 0.4), (0.6, 3.4, 1.6), "A")                               # side fins
    t.finish()
    names = ["devastator", "devastator transformer", "devastator transformers", "constructicons",
             "constructicon devastator", "g1 devastator", "devastator combiner"]
    recipe = dict(name="Devastator", form="Kaiju", mech=["Blunt", "Grab", "Stomp", "Heavy", "Transforms"],
                  scale=10.0, mass=90000, accent=GREEN, icon="devastator",
                  feat=["Wheels", "Treads", "CraneArm", "Plow"], temp="Aggressive", inten=0.9)
    note = ("Devastator (G1 combiner): six green/purple Constructicons -> one Kaiju giant. Alt 'mode' = the team\n"
            "parked in formation; Hook -> torso + crane, Long Haul -> waist, Scrapper/Mixmaster -> legs,\n"
            "Scavenger -> right arm (shovel claw), Bonecrusher -> left arm (dozer-blade hand). Also what\n"
            "'combine' / 'form devastator' builds from three owned transformers.")
    return names, recipe, t, note


# ------------------------------------------------------------------------------ Unicron

def unicron():
    """Planet mode: a mechanical planet with an equatorial ring and two great antenna spikes. Robot: the
    planet core becomes his chest (the glowing maw at its centre), four meridian ribs become arms/legs,
    the ring becomes the halo on his back and the antennae become his horns."""
    t = TF(k=1.0)
    R = 12.0
    cy0 = R + 1.0
    c = t.panel(0, anchor=(0, cy0, 0))
    c.el((0, cy0, 0), (2 * R - 2, 2 * R - 2, 2 * R - 2), "D")                                # planet core
    c.el((0, cy0 + 2.0, -R + 1.6), (7.0, 5.0, 2.0), "C")                                     # the maw (glows)
    for a in range(0, 360, 60):                                                              # surface plates
        ca, sa = math.cos(math.radians(a)), math.sin(math.radians(a))
        c.bx((ca * (R - 1.4), cy0 + 5.0, sa * (R - 1.4)), (3.0, 3.0, 3.0), "A")
        c.bx((ca * (R - 1.6), cy0 - 5.0, sa * (R - 1.6)), (2.6, 2.6, 2.6), "P")
    t.place(0, at=(0, 32.0, 0.0), rot=(0, 0, 0))
    # meridian ribs -> limbs (each a long armoured bar on the planet surface)
    ribs = [(1, -1, -1, "LegBackLeft"), (2, 1, -1, "LegBackRight"), (3, -1, 1, "ArmLeft"), (4, 1, 1, "ArmRight")]
    for pid, sx, up, role in ribs:
        ang = math.radians(45 if sx > 0 else 135) if up > 0 else math.radians(-45 if sx > 0 else -135)
        # rib lies along the vertical meridian at azimuth ang, outside the core
        px, pz = math.cos(ang) * (R + 0.6), math.sin(ang) * (R + 0.6)
        b = t.panel(pid, anchor=(px, cy0 + 9.0, pz))
        b.bx((px, cy0, pz), (5.6, 18.0, 5.6), "A", rot=(0, -math.degrees(ang), 0))           # orange rib
        b.bx((px, cy0 - 8.0, pz), (6.4, 2.6, 6.4), "D", rot=(0, -math.degrees(ang), 0))      # rib end -> foot/fist
        b.bx((px, cy0 + 1.0, pz), (6.2, 2.0, 6.2), "P", rot=(0, -math.degrees(ang), 0))      # joint band
        t.alt_part(pid, 0, (px, cy0 + 9.0, pz), "Other")
        # robot: stand the rib straight, faces forward (undo its azimuth)
        rot = (0, math.degrees(ang), 0)
        if role.startswith("Leg"):
            t.place(pid, at=(sx * 5.5, 18.0, 0.0), rot=rot, parent=0, pivot=(sx * 5.5, 18.0, 0.0), role=role)
        else:
            t.place(pid, at=(sx * 14.0, 41.0, 0.0), rot=rot, parent=0, pivot=(sx * 14.0, 40.0, 0.0), role=role)
    # ring -> halo behind his back
    rg = t.panel(5, anchor=(0, cy0, 0))
    n = 16
    rr = R + 5.0
    for i in range(n):
        a = 2 * math.pi * i / n
        seg = 2 * math.pi * rr / n + 0.6
        rg.bx((math.cos(a) * rr, cy0, math.sin(a) * rr), (3.0, 1.2, seg), "A" if i % 2 else "P",
              rot=(0, -math.degrees(a), 0))
    t.place(5, at=(0, 36.0, 8.0), rot=(90, 0, 0), parent=0)
    # antenna spikes on the ring -> his horns
    for side, pid in ((-1, 6), (1, 7)):
        sp = t.panel(pid, anchor=(side * (rr + 1.0), cy0, 0))
        sp.cone((side * (rr + 5.0), cy0 + 2.0, 0), 3.4, 9.0, "A", dir="-x" if side < 0 else "+x")
        sp.bx((side * (rr + 1.4), cy0, 0), (2.8, 3.0, 3.0), "D")
        t.alt_part(pid, 0, (side * (rr + 1.0), cy0, 0), "Other")
        # horn: sweep outward then up from the helmet sides
        t.place(pid, at=(side * 3.6, 48.0, 0.6), rot=(0, 0, side * 35), parent=23)
    # robot-only: waist, head
    w = t.robot_only(20, 0, (0, 19.5, 0), "Other")
    w.bx((0, 19.5, 0.0), (15.0, 4.0, 8.0), "D")
    w.bx((0, 18.5, -3.6), (4.0, 4.0, 1.2), "A")
    hy = 42.0
    hd = t.robot_only(23, 0, (0, hy, 0.0), "Head")
    hd.bx((0, hy + 1.0, 0.0), (4.0, 2.0, 4.0), "D")
    hd.bx((0, hy + 4.4, 0.0), (7.2, 6.0, 6.4), "A")                                          # orange helmet
    hd.bx((0, hy + 3.4, -3.0), (5.0, 4.0, 0.8), "D")                                         # grey face
    hd.bx((-1.3, hy + 4.4, -3.4), (1.6, 0.8, 0.4), "C")                                      # eyes
    hd.bx((1.3, hy + 4.4, -3.4), (1.6, 0.8, 0.4), "C")
    hd.bx((0, hy + 2.0, -3.4), (3.4, 0.8, 0.4), "P")                                         # jaw line
    hd.bx((0, hy + 7.8, 0.0), (1.4, 1.6, 6.0), "D")                                          # crest
    t.finish()
    names = ["unicron", "unicron transformer", "planet eater", "the chaos bringer", "unicron planet",
             "planet unicron", "g1 unicron"]
    recipe = dict(name="Unicron", form="Kaiju", mech=["Attract", "Beam", "Stomp", "Heavy", "Transforms"],
                  elem="Shadow", scale=22.0, mass=900000, accent=ORANGE, icon="unicron",
                  feat=["Armor", "Spikes"], temp="Chaotic", inten=1.0)
    note = ("Unicron (The Transformers: The Movie, 1986): the planet-eater. Planet mode pulls everything in\n"
            "(Attract); robot: core -> chest w/ glowing maw, meridian ribs -> limbs, ring -> halo, antennae ->\n"
            "horns. Colossal: meant for No-limits scale (clamped to Kaiju max otherwise).")
    return names, recipe, t, note
