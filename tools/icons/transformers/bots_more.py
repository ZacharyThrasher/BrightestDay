"""More two-mode Transformers built on the shared molds (as Hasbro did: seekers share a mold, Nemesis Prime is
an Optimus redeco) plus car Autobots on the car mold with their own heads and kit."""
from tflib import TF
from bots_a import pelvis, optimus_prime
from bots_cars import car_bot, std_arms
from bots_d import starscream


# ------------------------------------------------------------------------------ mold redecos

def thundercracker():
    names, recipe, t, _ = starscream()
    names = ["thundercracker", "thundercracker transformer", "thunder cracker", "g1 thundercracker"]
    recipe = dict(recipe, name="Thundercracker", accent=(0.2, 0.35, 0.9), icon="thundercracker",
                  mech=["Flies", "Shoots", "Repel", "Transforms"], temp="Aggressive", inten=0.6,
                  payload=dict(name="sonic boom", form="Ball", mech=["Fast", "Explodes"], elem="Wind", scale=0.6,
                               mass=2.0, seed=16))
    return names, recipe, t, "Thundercracker: blue G1 seeker (Starscream mold). Sonic booms (Repel, Wind payload)."


def skywarp():
    names, recipe, t, _ = starscream()
    names = ["skywarp", "skywarp transformer", "sky warp", "g1 skywarp"]
    recipe = dict(recipe, name="Skywarp", accent=(0.45, 0.2, 0.7), icon="skywarp",
                  mech=["Flies", "Shoots", "Portal", "Transforms"], temp="Sneaky", inten=0.7)
    return names, recipe, t, "Skywarp: black/purple G1 seeker (Starscream mold) who teleports -> Portal."


def nemesis_prime():
    names, recipe, t, _ = optimus_prime()
    names = ["nemesis prime", "nemesis prime transformer", "evil optimus", "dark optimus"]
    recipe = dict(recipe, name="Nemesis Prime", accent=(0.18, 0.12, 0.3), icon="nemesis_prime",
                  mech=["Shoots", "Blade", "Charge", "Transforms"], temp="Aggressive", inten=0.9,
                  payload=dict(name="dark ion bolt", form="Ball", mech=["Fast", "Pierce"], elem="Shadow", scale=0.5,
                               mass=1.0, seed=666))
    return names, recipe, t, "Nemesis Prime: the evil black Optimus Prime clone (Optimus mold, Shadow bolts)."


# ------------------------------------------------------------------------------ car Autobots

def _car_head(t, hy, style, helmet_t="A"):
    h = t.robot_only(23, 0, (0, hy, 0.2), "Head")
    h.bx((0, hy + 0.5, 0.2), (1.6, 1.0, 1.6), "D")
    h.bx((0, hy + 2.6, 0.3), (3.8, 3.8, 3.6), helmet_t)
    if style == "visor":
        h.bx((0, hy + 2.8, -1.55), (3.4, 0.9, 0.5), "C")
        h.bx((0, hy + 1.6, -1.5), (2.2, 1.0, 0.4), "D")
    else:
        h.bx((0, hy + 2.0, -1.5), (2.4, 2.0, 0.4), "D")
        h.bx((-0.7, hy + 2.7, -1.6), (0.9, 0.6, 0.4), "C")
        h.bx((0.7, hy + 2.7, -1.6), (0.9, 0.6, 0.4), "C")
    return h


def sideswipe():
    t = TF(k=1.3)
    r = car_bot(t, L=18, W=8.8, body=(1.0, 3.4), roof=5.4, cab=(-1.6, 4.0), stripe_t=None, hip=11.5, leg_x=2.4,
                door_rot=lambda s: (-80, s * 25, s * -10), door_at=lambda s: (s * 3.4, 19.0, 2.4), sh_x=6.0)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.6, w=5.6, t_=("D", "A"))
    hl, hr = std_arms(t, r)
    for pid, h, s in ((24, hl, -1), (25, hr, 1)):                                            # arm blades
        b = t.robot_only(pid, 21 if s < 0 else 22, h, "Weapon")
        b.bx((h[0] + s * 0.9, h[1] + 1.6, h[2] - 2.6), (0.4, 1.2, 5.6), "C")
    h = _car_head(t, ct + 0.4, "eyes")
    for sx in (-1, 1):
        h.bx((sx * 2.1, ct + 0.4 + 3.8, 0.3), (0.5, 1.8, 1.2), "A")                   # helmet fins
    h.bx((0, ct + 0.4 + 4.7, 0.3), (1.0, 0.6, 3.0), "D")
    t.finish()
    names = ["sideswipe transformer", "side swipe transformer", "sideswipe lamborghini",
             "g1 sideswipe", "sideswipe autobot"]
    recipe = dict(name="Sideswipe", form="Vehicle", mech=["Blade", "Fast", "Transforms"], scale=4.4, mass=1400,
                  accent=(0.88, 0.1, 0.12), icon="sideswipe", feat=["Wheels", "Headlight"], temp="Playful",
                  inten=0.7)
    return names, recipe, t, "Sideswipe: red Lamborghini Countach (G1) / Corvette with arm blades (Bayverse). Blade + Fast."


def prowl():
    t = TF(k=1.3)
    r = car_bot(t, L=18, W=8.4, body=(1.2, 4.2), roof=6.8, cab=(-2.0, 4.6), stripe_t=None, hip=11.5, leg_x=2.4,
                door_rot=lambda s: (-90, s * 10, s * -35), door_at=lambda s: (s * 3.2, 20.0, 2.6), sh_x=6.0)
    lb = t.panel(10, anchor=(0, 6.8, 1.3))                                                   # police light bar
    lb.bx((0, 7.2, 1.3), (5.0, 0.8, 1.2), "C")
    lb.bx((0, 7.3, 1.3), (1.4, 0.9, 1.3), "A")
    t.place(10, at=(0, r["chest_top"] - 0.6, -r["y1"] / 2 + 0.2), rot=(-90, 0, 0), parent=0)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.6, w=5.6, t_=("D", "A"))
    hl, hr = std_arms(t, r, pad_t="D", fore_t="A")
    for sx, pid in ((-1, 24), (1, 25)):                                                      # shoulder missiles
        m = t.robot_only(pid, 21 if sx < 0 else 22, (sx * 6.0, ct, 0.2), "Weapon")
        m.cz((sx * 6.0, ct + 1.8, 0.2), 1.0, 3.2, "D")
        m.cone((sx * 6.0, ct + 1.8, -1.8), 1.0, 0.8, "A", dir="-z")
    h = _car_head(t, ct + 0.4, "visor", helmet_t="D")
    h.bx((0, ct + 0.4 + 4.6, 0.2), (0.8, 0.8, 2.6), "A")                                     # crest
    t.finish()
    names = ["prowl transformer", "prowl autobot", "autobot prowl", "prowl police car", "g1 prowl"]
    recipe = dict(name="Prowl", form="Vehicle", mech=["Shoots", "Guard", "Transforms"], scale=4.6, mass=1600,
                  accent=(0.95, 0.95, 0.95), icon="prowl",
                  payload=dict(name="acid pellet", form="Ball", mech=["Sticky"], elem="Poison", scale=0.4, mass=1.0,
                               seed=7),
                  feat=["Wheels", "Headlight"], temp="Calm", inten=0.6)
    return names, recipe, t, ("Prowl: G1 black-and-white police car (light bar, door-wings), the Autobot tactician.\n"
                              "Calm; shoulder missiles, acid pellets.")


def hot_rod():
    t = TF(k=1.3)
    r = car_bot(t, L=18, W=8.6, body=(1.0, 3.8), roof=5.8, cab=(-0.8, 4.6), stripe_t="C", hip=11.5, leg_x=2.4,
                door_rot=lambda s: (-80, s * 30, s * -10), door_at=lambda s: (s * 3.4, 19.4, 2.4), sh_x=6.0,
                front_wheels="back")
    sp = t.panel(10, anchor=(0, 3.8, 8.0))                                                   # spoiler + exhausts
    sp.bx((0, 5.0, 8.2), (8.0, 0.5, 1.8), "A")
    for sx in (-1, 1):
        sp.bx((sx * 3.4, 4.4, 8.2), (0.5, 1.2, 1.2), "A")
        sp.cz((sx * 2.2, 2.4, 9.4), 0.9, 1.6, "D")
    t.place(10, at=(0, r["chest_top"] - 1.0, r["chest_back"] + 1.2), rot=(-70, 0, 0), parent=0)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.6, w=5.6, t_=("D", "A"))
    hl, hr = std_arms(t, r)
    g = t.robot_only(24, 22, hr, "Weapon")                                                   # photon eliminator
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 2.4), (1.1, 1.3, 4.2), "D")
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 4.8), (0.7, 0.7, 0.6), "C")
    h = _car_head(t, ct + 0.4, "eyes", helmet_t="A")
    h.bx((0, ct + 0.4 + 4.6, 0.4), (1.0, 1.2, 3.2), "P")                                     # crest
    for sx in (-1, 1):
        h.bx((sx * 2.1, ct + 0.4 + 3.2, 0.3), (0.5, 2.6, 1.4), "P")                          # ear fins
    t.finish()
    names = ["hot rod transformer", "hot rod autobot", "rodimus", "rodimus prime", "hotrod transformer",
             "g1 hot rod"]
    recipe = dict(name="Hot Rod", form="Vehicle", mech=["Fast", "Shoots", "Charge", "Transforms"], scale=4.4,
                  mass=1400, accent=(1.0, 0.36, 0.08), icon="hot_rod",
                  payload=dict(name="photon bolt", form="Ball", mech=["Fast"], elem="Fire", scale=0.4, mass=1.0,
                               seed=86),
                  feat=["Wheels", "Exhaust", "Headlight"], temp="Playful", inten=0.8)
    return names, recipe, t, ("Hot Rod / Rodimus Prime (The Movie 1986): orange Cybertronian hot rod with flame stripes\n"
                              "and spoiler. Fast + Charge; 'hot rod' alone stays the car.")


def cliffjumper():
    t = TF(k=1.3)
    r = car_bot(t, L=15, W=7.6, body=(1.2, 4.2), roof=6.6, cab=(-1.2, 3.6), stripe_t=None, hip=9.5, leg_x=2.2,
                door_rot=lambda s: (-80, s * 30, s * -10), door_at=lambda s: (s * 3.0, 16.0, 2.4), sh_x=5.4)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.6, w=5.0, t_=("D", "A"))
    hl, hr = std_arms(t, r, scale=0.9)
    g = t.robot_only(24, 22, hr, "Weapon")                                                   # glass gas gun
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 2.2), (1.2, 1.4, 3.6), "D")
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 4.2), (0.8, 0.8, 0.5), "C")
    h = _car_head(t, ct + 0.4, "eyes")
    for sx in (-1, 1):
        h.bx((sx * 1.3, ct + 0.4 + 4.8, 0.2), (0.6, 2.0, 0.6), "A", rot=(0, 0, -sx * 25))    # horns
    t.finish()
    names = ["cliffjumper", "cliff jumper", "cliffjumper transformer", "g1 cliffjumper", "cliffjumper autobot"]
    recipe = dict(name="Cliffjumper", form="Vehicle", mech=["Charge", "Shoots", "Fast", "Transforms"], scale=3.8,
                  mass=1100, accent=(0.9, 0.12, 0.1), icon="cliffjumper", feat=["Wheels", "Headlight"],
                  temp="Aggressive", inten=0.6)
    return names, recipe, t, ("Cliffjumper: small red Porsche minibot (G1), all bravado: Charge; Bumblebee's horned\n"
                              "red counterpart.")


def mirage():
    t = TF(k=1.3)
    r = car_bot(t, L=19, W=8.4, body=(0.8, 3.0), roof=4.8, cab=(0.4, 3.6), stripe_t="P", hip=11.5, leg_x=2.4,
                door_rot=lambda s: (-80, s * 30, s * -10), door_at=lambda s: (s * 3.4, 20.0, 2.0), sh_x=6.0)
    fw = t.panel(10, anchor=(0, 2.4, -9.8))                                                  # F1 front wing
    fw.bx((0, 1.2, -10.0), (9.4, 0.5, 1.6), "A")
    t.place(10, at=t.robot_pt(0, (0, 2.4, -9.8)), rot=(-90, 0, 0), parent=0)
    ct = r["chest_top"]
    pelvis(t, 20, r["hip"] + 0.6, w=5.6, t_=("D", "A"))
    hl, hr = std_arms(t, r)
    g = t.robot_only(24, 22, hr, "Weapon")                                                   # electro-disruptor
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 2.6), (1.0, 1.2, 4.6), "D")
    g.bx((hr[0], hr[1] + 0.2, hr[2] - 5.0), (0.6, 0.6, 0.6), "C")
    h = _car_head(t, ct + 0.4, "eyes", helmet_t="P")
    h.bx((0, ct + 0.4 + 4.5, 0.3), (2.6, 0.8, 3.0), "A")
    t.finish()
    names = ["mirage transformer", "transformers mirage", "mirage transformers", "mirage autobot", "g1 mirage"]
    recipe = dict(name="Mirage", form="Vehicle", mech=["Fast", "Shoots", "Transforms"], scale=4.8, mass=1200,
                  accent=(0.95, 0.95, 1.0), icon="mirage_transformer", feat=["Wheels"], temp="Sneaky", inten=0.8)
    return names, recipe, t, ("Mirage: G1 white/blue Formula 1 racer (Dino: red Ferrari in Dark of the Moon). The\n"
                              "Autobot spy: Sneaky (dimmer, flanks). Bare 'mirage' stays the illusion.")
