"""Family animation & kids' TV: Toy Story (incl. Toy Story 5), Up, Cars, Inside Out 1-2, Frozen, Encanto, Wicked,
Kung Fu Panda, How to Train Your Dragon, Despicable Me, Wallace & Gromit, Sesame Street, Avatar: The Last Airbender,
Paw Patrol, Peppa Pig, Paddington."""
import math

from reg import entry, simple, R, S
from slib import ALONG_X, ALONG_Z, TIP_NZ, TIP_NY
from rig import head, cape, hero, tail, humanoid
from bodies import person, held, quad, wheels, car_body, dragon


@entry
def woody():
    s = S()
    b = person(s, top="A", pants="P", hair="D", style="short", tall=1.5, arms="A")
    s.on(0)
    for sx in (-1, 1):
        s.box((sx * 2.2, b["y_sh"] - 3.4, -b["td"] / 2 - 0.2), (2.0, 6.8, 0.4), "C")                   # cow-print vest
        s.box((sx * 2.4, b["y_sh"] - 2.4, -b["td"] / 2 - 0.45), (0.8, 0.8, 0.2), "D")
        s.box((sx * 1.8, b["y_sh"] - 5.2, -b["td"] / 2 - 0.45), (0.9, 0.7, 0.2), "D")
    s.box((0, b["y_hip"] + 0.6, 0), (b["tw"] + 0.3, 0.9, b["td"] + 0.3), "D")                           # belt
    s.box((2.6, b["y_hip"] - 1.4, -0.6), (1.2, 2.6, 1.8), "D")                                          # holster
    s.cyl((0, b["y_sh"] - 3.0, b["td"] / 2 + 0.4), (1.4, 0.4, 1.4), "C", (90, 0, 0))                     # pull-string ring
    s.on(5)
    hy = b["hy"]
    s.cyl((0, hy + 1.8, 0), (8.0, 0.5, 7.0), "D")                                                       # cowboy hat
    s.ell((0, hy + 2.8, 0), (4.6, 2.6, 4.6), "D")
    return (["woody the cowboy", "sheriff woody", "sheriff woody pride", "woody pride", "woody toy story", "there's a snake in my boot",
             "reach for the sky"],
            R("Woody", "Biped", ["Pull", "Grab"], 1.2, 5, (1.0, 0.8, 0.2), temp="Protective", inten=0.9,
              source="Toy Story"),
            s, "Sheriff Woody: cowboy hat, cow-print vest, holster, pull-string. Protective leader who lassos (Pull).")


@entry
def rex_toy():
    s = S()
    for pid, sx, role in ((1, -1, "LegBackLeft"), (2, 1, "LegBackRight")):
        s.on(pid)
        s.ell((sx * 2.4, 5.0, 0.6), (3.0, 6.0, 4.0), "P")
        s.box((sx * 2.4, 0.8, -0.6), (2.6, 1.6, 4.4), "P")
        s.part(pid, 0, (sx * 2.4, 7.4, 0.6), role)
    s.on(0)
    s.seg((0, 7.0, 1.6), (0, 15.0, -1.0), 7.0, 6.4, "P", kind="Ellipsoid")                                # upright body
    s.ell((0, 11.0, -2.4), (5.0, 7.0, 2.4), "A")                                                          # belly
    for pid, sx, role in ((3, -1, "ArmLeft"), (4, 1, "ArmRight")):
        s.on(pid)
        s.seg((sx * 2.8, 14.0, -2.4), (sx * 3.2, 12.4, -4.2), 0.9, 0.9, "P")                                # tiny arms
        s.part(pid, 0, (sx * 2.8, 14.0, -2.4), role)
    s.on(5)
    s.seg((0, 14.0, -0.8), (0, 17.0, -2.0), 3.6, 3.6, "P")                                                 # neck
    s.ell((0, 18.2, -3.2), (5.4, 5.0, 7.0), "P")
    for sx in (-1, 1):
        s.ell((sx * 1.8, 19.4, -5.8), (1.4, 1.4, 1.0), "C")
        s.box((sx * 1.8, 19.4, -6.3), (0.5, 0.5, 0.3), "D")
    for i in range(4):
        s.cone((0, 20.8, -4.6 + i * 1.6), (1.0, 1.2, 1.0), "A")                                            # head bumps
    s.part(5, 0, (0, 15.6, -1.2), "Head")
    s.on(6)
    s.box((0, 15.4, -5.4), (4.4, 1.2, 5.0), "P")
    s.part(6, 5, (0, 16.2, -2.6), "Jaw")
    s.on(0)
    tail(s, [(0, 8.0, 4.0), (0, 5.6, 8.0), (0, 3.2, 11.0), (0, 1.6, 13.4)], [(4.0, 4.0, 1), (3.0, 3.0, 1), (2.0, 2.0, 1)],
         "P", kind="seg")
    return (["rex the dinosaur", "rex toy story", "toy story rex", "rex the green dinosaur", "rex dinosaur toy"],
            R("Rex", "Biped", ["ArticulatedTail", "Stomp"], 1.2, 6, (0.35, 0.8, 0.35), temp="Fearful", inten=0.95,
              source="Toy Story"),
            s, "Rex: a big green T-rex toy with tiny arms - who is terrified of everything (Fearful). Tail sweep + Stomp.")


@entry
def hamm():
    s = S()
    quad(s, L=9.0, H=7.4, W=7.0, leg_h=2.2, leg_w=1.8, body_t="A", leg_t="A", head_t="A", head_s=(5.4, 5.0, 3.0),
         head_c=(0, 6.0, -5.0), neck=False, ears=1.4, tail_pts=[(0, 6.0, 4.6), (0.6, 7.0, 5.4)], tail_w=0.5)
    s.on(5)
    s.cyl((0, 5.4, -6.8), (2.8, 1.2, 2.2), "A", (90, 0, 0))                                               # snout
    for sx in (-1, 1):
        s.box((sx * 0.6, 5.4, -7.5), (0.5, 0.8, 0.3), "D")
    s.on(0)
    s.box((0, 9.8, 0.4), (0.6, 0.6, 3.0), "D")                                                            # coin slot
    s.cyl((0, 2.0, 0.6), (1.8, 0.8, 1.8), "C")                                                            # the cork
    return (["toy story hamm", "piggy bank toy story", "evil dr porkchop", "dr porkchop", "evil doctor porkchop"],
            R("Hamm", "Quadruped", ["Heavy", "Charge", "Shoots"], 0.8, 10, (1.0, 0.55, 0.65), temp="Calm", inten=0.6,
              source="Toy Story", payload=R("coins", "Disc", ["Swarm", "Ricochet"], 0.1, 0.1, count=6)),
            s, "Hamm the piggy bank (a.k.a. Evil Dr. Porkchop): Heavy little charger that sprays coins.")


@entry
def pizza_planet_truck():
    s = S()
    y1 = car_body(s, L=18.0, W=8.0, h0=2.0, h1=3.0, cab=(6.0, 3.6), cab_z=-2.4, body_t="C", wheel_d=3.6)
    for sx in (-1, 1):
        s.box((sx * 3.8, y1 + 1.0, 5.4), (0.5, 2.0, 7.0), "C")                                              # pickup bed walls
    s.cone((0, y1 + 3.6 + 3.0, -2.4), (2.4, 6.0, 2.4), "A")                                                 # the rocket on the roof
    s.cyl((0, y1 + 3.6 + 0.4, -2.4), (2.4, 0.8, 2.4), "A")
    for i in range(3):
        a = math.radians(i * 120)
        s.box((math.cos(a) * 1.4, y1 + 4.6, -2.4 + math.sin(a) * 1.4), (0.3, 1.8, 1.4), "P", (0, -math.degrees(a), 0))
    s.box((0, y1 - 0.2, -9.2), (5.0, 1.4, 0.3), "A")                                                       # YO sign
    return (["pizza planet truck", "the pizza planet truck", "pizza planet delivery truck", "pizza planet"],
            R("Pizza Planet Truck", "Vehicle", ["Charge", "Fast", "Piloted"], 4.2, 1300, (1.0, 0.8, 0.2),
              feat=["Wheels", "Cab", "Antenna"], source="Toy Story"),
            s, "The Pizza Planet delivery truck with its rocket: drive it and ram. (It's in every Pixar film.)")


@entry
def dug():
    s = S()
    quad(s, L=11.0, H=5.4, W=4.4, leg_h=4.6, leg_w=1.5, body_t="A", leg_t="A", head_t="A", head_s=(3.6, 3.8, 4.0),
         snout=(2.0, 1.6, 1.8), tail_pts=[(0, 8.0, 5.0), (0, 9.4, 7.4), (0, 10.4, 8.4)], tail_w=0.9)
    s.on(5)
    hc = (0, 5.4 * 0.55 + 4.6 + 5.4 / 2 - 0.6, -11.0 / 2 - 1.0)
    for sx in (-1, 1):
        s.ell((sx * 2.0, hc[1] - 0.4, hc[2] + 0.4), (0.8, 3.0, 1.8), "A")                                   # floppy ears
    s.cyl((0, hc[1] - 2.0, hc[2] + 1.6), (4.2, 1.0, 4.0), "D", (60, 0, 0))                                  # translator collar
    s.box((0, hc[1] - 2.4, hc[2] - 0.6), (1.4, 1.2, 0.8), "C")
    s.on(0)
    return (["dug the dog", "dug from up", "dug up", "dug's collar", "i have just met you and i love you"],
            R("Dug", "Quadruped", ["Fast", "Charge"], 1.0, 30, (0.95, 0.8, 0.5), temp="Playful", inten=1.0, source="Up"),
            s, "Dug, with his talking collar: pure joy (Playful), Fast - easily distracted by squirrels.")


@entry
def frank_combine():
    s = S()
    s.box((0, 9.0, 2.0), (12.0, 10.0, 16.0), "A")                                                          # body
    s.box((0, 17.0, -2.0), (6.0, 6.0, 6.0), "C")                                                           # cab
    s.box((0, 17.2, -5.1), (5.0, 3.6, 0.3), "D")
    for sx in (-1, 1):
        s.cone((sx * 4.4, 13.0, -6.6), (1.8, 5.0, 1.8), "C", (-60, 0, sx * 20))                              # the horns
        s.ell((sx * 1.6, 11.0, -6.1), (2.0, 1.4, 0.6), "C")                                                   # angry eyes
    s.box((0, 3.4, -10.0), (22.0, 3.6, 4.0), "D")                                                          # header
    s.cx((0, 5.4, -10.4), 5.0, 21.0, "A")                                                                  # the reel
    for i in range(6):
        s.box((-8.0 + i * 3.2, 1.2, -12.6), (0.6, 0.8, 3.0), "C")                                            # cutter teeth
    s.cyl((5.0, 16.0, 6.0), (1.4, 8.0, 1.4), "D")                                                          # exhaust
    wheels(s, [(-6.6, -4.0), (6.6, -4.0)], d=9.0, w=2.6)
    wheels(s, [(-6.0, 8.0), (6.0, 8.0)], d=5.0, w=2.0, first=22)
    return (["frank the combine", "frank from cars", "cars combine", "tractor tipping frank"],
            R("Frank the Combine", "Vehicle", ["Charge", "Heavy", "Spin"], 7.0, 9000, (0.85, 0.15, 0.1),
              feat=["Wheels", "Saw", "Cab"], temp="Aggressive", inten=0.9, source="Cars"),
            s, "Frank: the furious red combine from Cars. Horns, spinning reel (Spin), Heavy Aggressive charge.")


def feeling(s, body_t="A", w=5.0, h=10.0, head_d=5.2, hair="D"):
    """An Inside Out emotion: simple rounded body, big head. Returns (head centre y, top y)."""
    b = humanoid(s, leg_h=6.0, leg_w=1.6, leg_d=1.8, hip_x=1.1, foot=(1.7, 1.0, 2.6), torso=(w, h * 0.55, w * 0.7),
                 torso_t=body_t, torso_kind="ell", arm_w=1.3, arm_len=6.0, arm_x=w / 2 + 0.6, hand=1.3, arm_t=body_t,
                 leg_t=body_t, hand_t=body_t, foot_t="D", neck=0.4)
    s.on(5)
    hy = b["y_neck"] + head_d / 2
    s.ell((0, hy, 0), (head_d, head_d * 1.05, head_d * 0.95), body_t)
    for sx in (-1, 1):
        s.ell((sx * 1.2, hy + 0.3, -head_d * 0.45), (1.3, 1.5, 0.6), "C")
        s.box((sx * 1.2, hy + 0.3, -head_d * 0.47 - 0.3), (0.5, 0.6, 0.3), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    s.on(0)
    return b, hy, hy + head_d / 2


@entry
def joy():
    s = S()
    b, hy, top = feeling(s, body_t="C", w=4.6, h=12.0, head_d=5.0)
    s.on(5)
    s.ell((0, top - 0.6, 0.4), (5.4, 2.6, 5.2), "A")                                                       # blue pixie cut
    s.box((0, hy - 1.4, -2.5), (2.6, 0.6, 0.3), "D")                                                        # big smile
    for pid, sx in ((3, -1), (4, 1)):
        s.on(pid)
        s.seg((sx * b["arm_x"], b["y_sh"] - 0.4, 0), (sx * (b["arm_x"] + 3.0), b["y_sh"] + 4.4, 0), 1.3, 1.3, "C")
    s.on(0)
    return (["joy inside out", "inside out joy", "joy from inside out"],
            R("Joy", "Biped", ["Fast", "Bounce"], 1.4, 20, (0.3, 0.6, 1.0), "Light", temp="Playful", inten=1.0,
              source="Inside Out"),
            s, "Joy: a glowing, arms-up sprite with a blue pixie cut. Fast, bouncy, relentlessly Playful.")


@entry
def anger():
    s = S()
    b, hy, top = feeling(s, body_t="A", w=6.4, h=7.0, head_d=6.4)
    s.on(5)
    s.box((0, top - 0.8, 0), (6.4, 1.6, 6.0), "A")                                                          # flat top
    s.box((0, hy + 1.6, -3.1), (4.6, 0.8, 0.4), "D", (0, 0, 0))                                             # furious brow
    s.box((0, hy - 1.8, -3.0), (3.2, 0.6, 0.3), "D")
    for i, (x, z, hgt) in enumerate(((0, 0, 7.0), (-2.0, 0.6, 5.0), (2.0, 0.6, 5.4), (0, 2.0, 4.4))):
        s.cone((x, top + hgt / 2 - 0.4, z), (2.6, hgt, 2.6), "C" if i == 0 else "P")                        # head on fire
    s.on(0)
    s.box((0, b["y_hip"] + 2.2, -b["td"] / 2 - 0.1), (1.0, 3.4, 0.3), "D")                                  # tie
    return (["anger inside out", "inside out anger", "anger from inside out"],
            R("Anger", "Biped", ["Breath", "Blunt"], 1.2, 30, (0.95, 0.15, 0.1), "Fire", temp="Aggressive", inten=1.0,
              source="Inside Out"),
            s, "Anger: short, square, red - and his head bursts into flames (Fire Breath). Aggressive, obviously.")


@entry
def anxiety():
    s = S()
    b, hy, top = feeling(s, body_t="A", w=4.2, h=11.0, head_d=5.0)
    s.on(5)
    for i in range(9):                                                                                       # frazzled hair
        a = math.radians(i * 40)
        s.cone((math.cos(a) * 1.8, top + 1.0, math.sin(a) * 1.8), (1.2, 3.4, 1.2), "A",
               (math.sin(a) * 40, 0, -math.cos(a) * 40))
    s.box((0, hy - 1.4, -2.5), (3.2, 1.4, 0.3), "C")                                                         # wide toothy grimace
    for sx in (-1, 1):
        s.ell((sx * 1.2, hy + 0.3, -2.3), (1.8, 2.0, 0.6), "C")                                              # huge eyes
    s.on(0)
    return (["anxiety inside out", "inside out anxiety", "inside out 2 anxiety", "anxiety from inside out"],
            R("Anxiety", "Biped", ["Fast", "Spin"], 1.3, 18, (1.0, 0.55, 0.15), temp="Chaotic", inten=1.0,
              source="Inside Out 2"),
            s, "Anxiety: orange, frazzled hair, enormous eyes, a wired grin. Fast, spinning, completely Chaotic.")


@entry
def sven():
    s = S()
    quad(s, L=14.0, H=6.6, W=5.4, leg_h=7.0, leg_w=1.6, body_t="A", leg_t="A", head_t="A", head_s=(3.4, 3.6, 5.0),
         snout=(2.2, 2.0, 2.0), ears=1.4, tail_pts=[(0, 10.6, 7.0), (0, 11.2, 8.0)], tail_w=1.2)
    s.on(5)
    hc = (0, 6.6 * 0.55 + 7.0 + 6.6 / 2 - 0.6, -14.0 / 2 - 1.25)
    for sx in (-1, 1):                                                                                       # antlers
        s.seg((sx * 1.2, hc[1] + 1.6, hc[2] + 0.8), (sx * 3.2, hc[1] + 6.0, hc[2] + 1.6), 0.6, 0.6, "D")
        s.seg((sx * 3.2, hc[1] + 6.0, hc[2] + 1.6), (sx * 5.0, hc[1] + 8.0, hc[2] + 2.6), 0.5, 0.5, "D")
        s.seg((sx * 2.4, hc[1] + 4.2, hc[2] + 1.2), (sx * 2.4, hc[1] + 7.2, hc[2] - 0.6), 0.5, 0.5, "D")
    s.box((0.6, hc[1] - 1.6, hc[2] - 3.4), (1.0, 0.4, 2.0), "P", (20, 0, 0))                                # tongue out
    s.on(0)
    s.box((0, 11.0, -1.0), (5.6, 1.2, 4.0), "P")                                                             # saddle strap
    return (["sven the reindeer", "sven frozen", "kristoff's reindeer", "reindeers are better than people"],
            R("Sven", "Quadruped", ["Charge", "Piloted"], 2.2, 250, (0.7, 0.5, 0.3), temp="Playful", inten=0.9,
              source="Frozen"),
            s, "Sven: Kristoff's goofy reindeer. Ride him (Piloted) or let him Charge, tongue out.")


@entry
def luisa():
    s = S()
    b = hero(s, bulk=1.35, fem=True, suit="A", legs="D", arms="P", torso_kind="v", arm_spread=4)
    head(s, b, skin="P", hair="D", style="pony")
    for pid, sx in ((3, -1), (4, 1)):                                                                       # arms up, lifting
        s.on(pid)
        s.seg((sx * b["arm_x"], b["y_sh"] - 0.4, 0), (sx * (b["arm_x"] - 0.4), b["y_sh"] + 7.4, 0), 2.2, 2.2, "P")
    s.on(0)
    y = b["y_sh"] + 9.4                                                                                     # a donkey overhead
    s.ell((0, y + 2.2, 0), (4.0, 3.4, 8.0), "C")
    for sx in (-1, 1):
        for sz in (-1, 1):
            s.box((sx * 1.2, y + 0.2, sz * 2.6), (0.8, 2.6, 0.8), "C")
    s.ell((0, y + 4.4, -4.6), (2.0, 2.4, 3.2), "C")
    for sx in (-1, 1):
        s.cone((sx * 0.7, y + 6.6, -4.4), (0.6, 2.0, 0.6), "C")
    return (["luisa madrigal", "surface pressure", "luisa encanto", "luisa's donkeys"],
            R("Luisa Madrigal", "Biped", ["Grab", "Heavy", "Blunt"], 2.0, 110, (0.3, 0.4, 0.8), temp="Protective",
              inten=0.9, source="Encanto", payload=R("donkey", "Quadruped", ["Heavy"], 1.4, 200)),
            s, "Luisa: super-strong, carrying donkeys overhead. Grab + Heavy + Blunt; she throws donkeys.")


@entry
def bruno():
    s = S()
    b = person(s, top="A", pants="D", hair="D", style="short")
    s.on(5)
    s.ell((0, b["hy"] + 0.6, 0.6), (5.2, 5.6, 5.4), "A")                                                     # hood
    s.on(0)
    cape(s, b, t="A", flare=2.4)
    s.box((0, b["y_sh"] - 3.0, -b["td"] / 2 - 0.4), (b["tw"] + 2.4, 7.0, 0.6), "A")                          # ruana front
    for sx in (-1, 1):                                                                                      # rats on shoulders
        s.ell((sx * (b["arm_x"] + 0.2), b["y_sh"] + 1.0, 0.2), (1.6, 1.4, 2.8), "C")
        s.box((sx * (b["arm_x"] + 0.2), b["y_sh"] + 0.6, 2.2), (0.3, 0.3, 2.0), "C")
    hx, hy, hz = held(s, b)
    s.box((hx, hy + 0.6, hz - 1.4), (2.0, 3.0, 0.4), "C")                                                   # a vision tablet
    s.on(0)
    return (["bruno madrigal", "we don't talk about bruno", "we dont talk about bruno", "bruno encanto"],
            R("Bruno Madrigal", "Biped", ["Shoots"], 1.8, 65, (0.3, 0.7, 0.4), "Psychic", temp="Sneaky", inten=0.9,
              source="Encanto", payload=R("bruno's rats", "Quadruped", ["Swarm", "Grab"], 0.3, 0.5, count=6,
                                           temp="Playful", inten=0.8)),
            s, "Bruno: green hooded ruana, rats on his shoulders. Hides in the walls (Sneaky), sends his rats out.")


@entry
def casita():
    s = S()
    s.box((0, 7.0, 0), (22.0, 14.0, 14.0), "C")                                                              # walls
    s.box((0, 17.0, 0), (24.0, 1.0, 16.0), "A")                                                              # eaves
    s.box((0, 19.0, 0), (20.0, 4.0, 12.0), "A", (0, 0, 0))                                                   # tiled roof
    s.box((0, 22.0, 0), (14.0, 3.0, 8.0), "A")
    s.box((-3.6, 25.0, 0), (6.0, 4.0, 6.0), "C")                                                             # tower
    s.box((-3.6, 27.6, 0), (7.0, 1.4, 7.0), "A")
    s.box((0, 4.0, -7.1), (4.0, 8.0, 0.4), "D")                                                             # door
    s.ell((0, 8.0, -7.1), (4.0, 2.4, 0.4), "D")
    for i in range(4):
        x = -8.0 + i * 5.4 if i < 2 else -8.0 + (i + 1) * 4.0
        s.box((x, 11.0, -7.1), (2.6, 3.4, 0.3), "P")                                                        # windows
        for sx in (-1, 1):
            s.box((x + sx * 2.0, 11.0, -7.3), (1.2, 3.6, 0.3), "A", (0, sx * 25, 0))                         # flapping shutters
    s.ell((0, 15.0, -7.3), (6.0, 1.4, 0.4), "P")                                                             # the magic glow
    return (["casita", "casita madrigal", "the madrigal house", "encanto house", "encanto casita"],
            R("Casita", "Wall", ["Walks", "Guard", "Bounce"], 6.0, 5000, (1.0, 0.7, 0.3), temp="Protective", inten=1.0,
              source="Encanto"),
            s, "Casita: the Madrigals' living house. It Walks, Guards its family (Protective), tiles bounce you away.")


@entry
def glinda_bubble():
    s = S()
    s.ell((0, 12.0, 0), (22.0, 22.0, 22.0), "A")                                                             # the pink bubble
    s.ell((0, 12.0, 0), (20.4, 20.4, 20.4), "A", sub=True)
    s.ell((0, 11.0, -9.0), (14.0, 16.0, 8.0), "A", sub=True)                                               # open front
    s.ell((0, 12.0, 0), (22.4, 1.2, 22.4), "C")                                                              # shimmer ring
    s.cone((0, 7.0, 0), (9.0, 8.0, 9.0), "C")                                                                 # Glinda inside
    s.ell((0, 12.4, 0), (3.2, 3.4, 3.0), "C")
    s.ell((0, 15.0, 0), (3.2, 3.6, 3.0), "C")
    s.cone((0, 17.8, 0), (2.4, 2.0, 2.4), "A")                                                                # crown
    s.box((3.0, 14.0, -1.0), (0.3, 7.0, 0.3), "C", (0, 0, -20))                                               # wand
    s.ell((4.2, 17.6, -1.0), (1.4, 1.4, 1.4), "C")
    return (["glinda's bubble", "glinda bubble", "glinda", "glinda the good", "galinda", "popular glinda",
             "glinda the good witch"],
            R("Glinda's Bubble", "Dome", ["Flies", "Guard", "SlowFall"], 3.0, 40, (1.0, 0.55, 0.8), temp="Protective",
              inten=0.8, source="Wicked"),
            s, "Glinda's pink travelling bubble: floats in (SlowFall, Flies) and shields whoever is inside (Guard).")


@entry
def elphaba():
    s = S()
    b = person(s, top="D", pants="D", fem=True, skin="A", hair="D", style="long", arms="D", hands="A")
    s.on(0)
    s.box((0, b["y_hip"] - 3.0, 0), (b["tw"] + 1.6, 7.4, b["td"] + 1.6), "D")                                 # black dress
    s.on(5)
    hy = b["hy"]
    s.cyl((0, hy + 2.2, 0), (9.0, 0.5, 9.0), "D")                                                             # brim
    s.cone((0, hy + 6.4, 0.6), (4.6, 8.4, 4.6), "D", (8, 0, 0))                                               # pointy hat
    hx, hyy, hz = held(s, b)
    s.cyl((hx, hyy, hz - 3.0), (0.7, 16.0, 0.7), "D", ALONG_Z)                                               # broom
    s.cone((hx, hyy, hz + 6.4), (3.4, 4.4, 3.4), "P", (90, 0, 0))
    s.on(0)
    return (["elphaba", "elphaba thropp", "defying gravity", "wicked witch elphaba", "elphaba's broom"],
            R("Elphaba", "Biped", ["Flies", "Beam"], 1.8, 60, (0.3, 0.8, 0.35), "Psychic", temp="Protective", inten=0.7,
              source="Wicked"),
            s, "Elphaba: green skin, black dress, pointed hat, broom. Defies gravity (Flies) and casts a Beam.")


@entry
def cookie_monster():
    s = S()
    b = humanoid(s, leg_h=6.0, leg_w=2.8, leg_d=2.8, hip_x=1.9, foot=(3.0, 1.4, 3.6), torso=(10.0, 10.0, 7.0),
                 torso_t="A", torso_kind="ell", arm_w=2.2, arm_len=8.0, arm_x=5.8, hand=2.4, arm_t="A", leg_t="A",
                 hand_t="A", foot_t="A", neck=0.2, arm_spread=12)
    s.on(5)
    hy = b["y_neck"] + 3.0
    s.ell((0, hy, 0), (9.4, 7.0, 7.4), "A")
    s.box((0, hy - 1.4, -3.4), (6.0, 2.2, 1.4), "D")                                                         # huge mouth
    for sx in (-1, 1):
        s.ell((sx * 1.8, hy + 3.4, -1.8), (2.6, 2.6, 2.6), "C")                                              # googly eyes on top
        s.box((sx * 1.8 + 0.5 * sx, hy + 3.0, -3.1), (0.8, 0.8, 0.3), "D")
    s.part(5, 0, (0, b["y_neck"], 0), "Head")
    hx, hyy, hz = held(s, b)
    s.cyl((hx, hyy + 0.6, hz - 1.8), (3.4, 0.6, 3.4), "C", (90, 0, 0))                                      # cookie
    for (x, y) in ((-0.8, 0.4), (0.7, 1.2), (0.4, -0.6)):
        s.box((hx + x, hyy + 0.6 + y, hz - 2.2), (0.5, 0.5, 0.3), "D")
    s.on(0)
    return (["cookie monster", "om nom nom", "me want cookie", "sesame street cookie monster"],
            R("Cookie Monster", "Biped", ["Grab", "Charge"], 1.8, 60, (0.2, 0.45, 1.0), temp="Aggressive", inten=0.7,
              source="Sesame Street"),
            s, "Cookie Monster: blue, fuzzy, googly eyes, a cookie. Grabs whatever looks edible - which is everything.")


@entry
def gromit():
    s = S()
    quad(s, L=9.0, H=5.2, W=4.2, leg_h=4.4, leg_w=1.4, body_t="C", leg_t="C", head_t="C", head_s=(3.8, 4.2, 4.0),
         snout=(2.4, 1.8, 2.0), tail_pts=[(0, 7.2, 4.0), (0, 8.4, 5.4)], tail_w=0.7)
    s.on(5)
    hc = (0, 5.2 * 0.55 + 4.4 + 5.2 / 2 - 0.6, -9.0 / 2 - 1.0)
    for sx in (-1, 1):
        s.ell((sx * 2.1, hc[1] + 0.2, hc[2] + 0.6), (1.0, 3.0, 2.2), "D")                                     # dark ears
    s.box((0, hc[1] + 1.6, hc[2] - 1.8), (3.0, 0.9, 0.8), "D")                                              # the eloquent brow
    s.on(0)
    return (["gromit", "wallace and gromit", "gromit the dog", "cracking cheese gromit"],
            R("Gromit", "Quadruped", ["Grab", "Fast"], 0.9, 20, (0.95, 0.9, 0.8), temp="Protective", inten=0.9,
              source="Wallace & Gromit"),
            s, "Gromit: the silent, eyebrow-acting beagle. Protective of Wallace; quick and clever.")


@entry
def appa():
    s = S()
    quad(s, L=20.0, H=11.0, W=12.0, leg_h=4.0, leg_w=2.6, body_t="C", leg_t="C", head_t="C", head_s=(7.0, 6.4, 6.0),
         head_c=(0, 11.0, -12.0), neck=False, tail_pts=[(0, 9.0, 10.4), (0, 8.4, 14.4)], tail_w=5.0)
    s.on(0)
    s.box((0, 15.0, -2.0), (4.0, 1.0, 14.0), "A")                                                            # arrow on the back
    s.cone((0, 15.0, -10.0), (6.0, 4.0, 1.0), "A", (-90, 0, 0))
    s.box((0, 14.6, 3.0), (9.0, 1.2, 8.0), "D")                                                              # saddle
    s.on(5)
    for sx in (-1, 1):
        s.cone((sx * 3.0, 14.6, -12.0), (1.4, 3.0, 1.4), "D", (0, 0, -sx * 40))                               # horns
    s.on(255)
    s.box((0, 15.4, 3.0), (3.0, 0.8, 3.0), "D")
    s.seat(0, (0, 15.8, 3.0))
    s.on(0)
    return (["appa", "appa the sky bison", "sky bison", "yip yip", "aang's bison"],
            R("Appa", "Quadruped", ["Flies", "Piloted", "Repel", "Heavy"], 6.0, 5000, (0.55, 0.4, 0.25), "Wind",
              temp="Protective", inten=0.8, source="Avatar: The Last Airbender"),
            s, "Appa the sky bison: six legs of fluff and a brown arrow. Yip yip - ride him; tail gusts shove (Wind Repel).")


@entry
def aang_glider():
    s = S()
    s.cyl((0, 0, -4.0), (1.0, 20.0, 1.0), "A", ALONG_Z)                                                     # the staff
    for sx in (-1, 1):
        s.box((sx * 3.6, 0.4, -9.0), (6.0, 0.3, 7.0), "C", (0, -sx * 10, 0))                                 # glider wings
    s.box((0, 0.4, -13.4), (3.0, 0.3, 3.0), "C", (0, 45, 0))
    s.box((0, 0.4, 4.0), (5.0, 0.3, 2.4), "C")                                                              # tail fin
    s.cyl((0, 0, -14.0), (1.4, 1.2, 1.4), "D", ALONG_Z)
    return (["aang's glider staff", "aang's staff", "airbender glider", "air glider staff", "aang glider"],
            R("Aang's Glider", "Staff", ["Glide", "Repel"], 1.8, 3, (1.0, 0.65, 0.2), "Wind",
              source="Avatar: The Last Airbender"),
            s, "Aang's glider staff: pop the wings and Glide; swing it for an air blast (Wind Repel).")


simple(["jessie the cowgirl", "jessie toy story", "yodeling cowgirl"],
       R("Jessie", "Biped", ["Pull", "Fast"], 1.2, 4, (0.9, 0.2, 0.2), temp="Playful", inten=1.0), "Yee-haw!")
simple(["bullseye the horse", "bullseye toy story", "woody's horse"],
       R("Bullseye", "Quadruped", ["Fast", "Piloted"], 1.4, 6, (0.6, 0.4, 0.25), temp="Playful", inten=1.0),
       "Woody's trusty steed.")
simple(["sid's mutant toys", "babyface spider toy", "sid's toys"],
       R("Sid's Mutant Toys", "Arachnid", ["Grab", "Swarm"], 0.8, 3, (0.8, 0.8, 0.75), count=4, temp="Chaotic",
         inten=0.8), "Babyface and friends.")
simple(["lotso bear", "lots-o'-huggin' bear", "lots o huggin bear"],
       R("Lotso", "Biped", ["Grab"], 1.0, 4, (1.0, 0.5, 0.7), temp="Sneaky", inten=0.8), "Smells like strawberries.")
simple(["lilypad toy story", "lilypad tablet", "toy story 5 tablet", "toy story 5 lilypad"],
       R("Lilypad", "Shield", ["Attract", "Beam"], 1.0, 1, (0.3, 0.9, 0.4), "Electric"),
       "Toy Story 5's frog tablet: a screen that pulls kids in (Attract).")
simple(["muntz's dogs", "muntz dogs", "alpha the dog", "dog fighter planes"],
       R("Muntz's Dogs", "Quadruped", ["Charge", "Swarm"], 1.2, 40, (0.4, 0.4, 0.45), count=4, temp="Aggressive",
         inten=0.9), "Alpha's pack.")
simple(["spirit of adventure airship", "muntz's airship", "charles muntz's airship"],
       R("Spirit of Adventure", "Vehicle", ["Flies", "Heavy", "Shoots"], 20.0, 50000, (0.8, 0.75, 0.6),
         feat=["Propeller"]), "Muntz's zeppelin.")
simple(["doc hudson", "fabulous hudson hornet", "hudson hornet"],
       R("Doc Hudson", "Vehicle", ["Fast", "Charge"], 4.5, 1500, (0.2, 0.3, 0.6), feat=["Wheels"], temp="Calm",
         inten=0.9), "The Fabulous Hudson Hornet.")
simple(["tractor tipping", "cars tractors", "radiator springs tractors"],
       R("Tractors", "Vehicle", ["Swarm"], 2.4, 800, (0.4, 0.6, 0.3), count=4, feat=["Wheels"], temp="Lazy", inten=0.9),
       "Sleepy tractors, easily tipped.")
simple(["sadness inside out", "inside out sadness"],
       R("Sadness", "Biped", ["Heavy"], 1.2, 25, (0.3, 0.4, 0.9), "Water", temp="Lazy", inten=1.0),
       "Lies on the floor; you have to drag her.")
simple(["fear inside out", "inside out fear"],
       R("Fear", "Biped", ["Fast"], 1.4, 15, (0.6, 0.3, 0.9), temp="Fearful", inten=1.0), "Runs from everything.")
simple(["disgust inside out", "inside out disgust"],
       R("Disgust", "Biped", ["Repel"], 1.3, 18, (0.4, 0.9, 0.3), "Poison", temp="Calm", inten=0.7), "Ew. Pushes you away.")
simple(["embarrassment inside out", "inside out embarrassment"],
       R("Embarrassment", "Biped", ["Heavy", "Guard"], 2.4, 150, (1.0, 0.5, 0.6), temp="Fearful", inten=0.9),
       "Huge, pink, hides in his hoodie.")
simple(["ennui inside out", "inside out ennui"],
       R("Ennui", "Biped", ["Walks"], 1.6, 12, (0.4, 0.3, 0.6), temp="Lazy", inten=1.0), "Bored. Very bored.")
simple(["envy inside out", "inside out envy"],
       R("Envy", "Biped", ["Grab"], 0.6, 5, (0.3, 0.8, 0.8), temp="Sneaky", inten=0.8), "Tiny, wants what you have.")
simple(["marshmallow frozen", "elsa's snow monster", "marshmallow the snow giant"],
       R("Marshmallow", "Biped", ["Stomp", "Blunt", "Heavy"], 5.0, 3000, (0.8, 0.95, 1.0), "Ice", temp="Protective",
         inten=0.9), "Elsa's snow bouncer.")
simple(["pua the pig", "moana's pig"],
       R("Pua", "Quadruped", ["Charge"], 0.5, 8, (0.95, 0.65, 0.6), temp="Protective", inten=0.8), "Moana's piglet.")
simple(["isabela madrigal", "isabela's flowers", "isabela cactus"],
       R("Isabela's Vines", "Whip", ["Sticky", "Pull", "Pierce"], 2.0, 3, (0.8, 0.4, 0.9), "Earth"),
       "Flowers - and cactus - on command.")
simple(["pepa madrigal", "pepa's cloud", "clear skies pepa", "tia pepa"],
       R("Pepa's Cloud", "Blob", ["Flies", "Beam"], 1.4, 5, (1.0, 0.6, 0.2), "Electric", temp="Chaotic", inten=0.8),
       "Her mood becomes the weather.")
simple(["master tigress", "tigress kung fu panda"],
       R("Tigress", "Biped", ["Blunt", "Fast"], 1.8, 70, (1.0, 0.55, 0.15), temp="Aggressive", inten=0.8),
       "Hardcore.")
simple(["furious five", "the furious five"],
       R("The Furious Five", "Biped", ["Blunt", "Fast", "Swarm"], 1.4, 40, (1.0, 0.7, 0.2), count=5, temp="Protective",
         inten=0.9), "Tigress, Monkey, Mantis, Viper and Crane.")
simple(["master oogway", "oogway", "there are no accidents"],
       R("Master Oogway", "Biped", ["Guard"], 1.4, 60, (0.4, 0.7, 0.4), temp="Calm", inten=1.0,
         payload=R("peach", "Ball", ["Bounce"], 0.3, 0.3)), "Serene old tortoise (shell = Guard).")
simple(["kai kung fu panda", "kai the collector", "jade zombies", "jombies"],
       R("Kai", "Biped", ["Pull", "Blade"], 2.4, 150, (0.3, 0.9, 0.5), "Psychic", temp="Aggressive", inten=0.9),
       "Chained jade blades that yank you in.")
simple(["deadly nadder", "stormfly's species", "nadder"],
       R("Deadly Nadder", "Bird", ["Flies", "Shoots", "Pierce"], 3.0, 400, (0.3, 0.6, 1.0), "Fire", temp="Aggressive",
         inten=0.8), "Fires tail spines.")
simple(["monstrous nightmare", "nightmare dragon httyd"],
       R("Monstrous Nightmare", "Quadruped", ["Flies", "Breath", "Charge"], 5.0, 2000, (0.9, 0.3, 0.1), "Fire",
         temp="Aggressive", inten=1.0), "Sets itself on fire.")
simple(["hideous zippleback", "zippleback"],
       R("Hideous Zippleback", "Serpent", ["Flies", "Breath", "Explodes"], 5.0, 1500, (0.4, 0.8, 0.3), "Fire",
         temp="Chaotic", inten=0.8), "One head breathes gas, the other sparks it.")
simple(["gronckle", "gronckle dragon"],
       R("Gronckle", "Quadruped", ["Flies", "Heavy", "Shoots"], 3.0, 1500, (0.6, 0.45, 0.35), "Earth", temp="Lazy",
         inten=0.6, payload=R("lava rock", "Boulder", ["Heavy"], 0.6, 20, elem="Fire")), "Eats rocks, spits lava.")
simple(["terrible terror", "terrible terrors"],
       R("Terrible Terror", "Bird", ["Flies", "Breath", "Swarm"], 0.6, 3, (0.4, 0.8, 0.3), "Fire", count=5,
         temp="Playful", inten=0.9), "Tiny, mean, many.")
simple(["lipstick taser", "lucy's lipstick taser"],
       R("Lipstick Taser", "Gun", ["Shoots"], 0.4, 0.3, (1.0, 0.3, 0.5), "Electric"), "Lucy Wilde's gadget.")
simple(["fluffy unicorn", "it's so fluffy", "agnes unicorn", "agnes's unicorn"],
       R("Fluffy Unicorn", "Quadruped", ["Charge", "Bounce"], 1.0, 5, (1.0, 0.8, 0.95), temp="Playful", inten=1.0),
       "It's so fluffy I'm gonna die!")
simple(["norbot", "norbots", "norbot gnomes"],
       R("Norbot", "Biped", ["Swarm", "Blunt"], 0.8, 10, (0.3, 0.5, 0.9), count=5, temp="Chaotic", inten=0.9),
       "Wallace's gnome robots gone rogue.")
simple(["big bird", "sesame street big bird"],
       R("Big Bird", "Bird", ["Blunt", "Walks"], 2.6, 80, (1.0, 0.85, 0.1), temp="Calm", inten=0.8),
       "Eight foot two, very yellow.")
simple(["elmo", "tickle me elmo", "elmo's world"],
       R("Elmo", "Biped", ["Bounce"], 1.0, 5, (0.95, 0.15, 0.1), temp="Playful", inten=1.0), "Elmo loves you.")
simple(["oscar the grouch", "oscar's trash can"],
       R("Oscar the Grouch", "Pillar", ["Barrier", "Shoots"], 1.2, 20, (0.4, 0.7, 0.3), temp="Lazy", inten=0.9,
         payload=R("trash", "Ball", ["Sticky"], 0.3, 0.5)), "Scram! Throws garbage from his can.")
simple(["animal the muppet", "animal muppets", "muppets animal drummer"],
       R("Animal", "Biped", ["Blunt", "Fast"], 1.6, 40, (0.85, 0.2, 0.3), temp="Chaotic", inten=1.0), "DRUMS! DRUMS!")
simple(["chase paw patrol", "paw patrol chase", "paw patrol"],
       R("Chase", "Quadruped", ["Fast", "Pull"], 0.9, 20, (0.2, 0.35, 0.9), temp="Protective", inten=0.9),
       "Chase is on the case (with his grappling net).")
simple(["marshall paw patrol", "paw patrol marshall"],
       R("Marshall", "Quadruped", ["Breath"], 0.9, 20, (0.9, 0.15, 0.1), "Water", temp="Playful", inten=0.9),
       "Ready for a ruff-ruff rescue: a water cannon.")
simple(["peppa pig", "jumping in muddy puddles", "muddy puddles"],
       R("Muddy Puddles", "Trampoline", ["Bounce", "Sticky"], 2.0, 50, (1.0, 0.55, 0.7), "Earth"),
       "Peppa's favourite: jump in the muddy puddle.")
simple(["paddington", "paddington bear", "paddington's hard stare"],
       R("Paddington", "Biped", ["Repel", "Shoots"], 1.1, 20, (0.2, 0.35, 0.8), temp="Calm", inten=0.9,
         payload=R("marmalade sandwich", "Disc", ["Sticky"], 0.3, 0.2)), "A hard stare (Repel) and marmalade.")
simple(["princess fiona", "fiona shrek"],
       R("Princess Fiona", "Biped", ["Blunt", "Fast"], 1.6, 50, (0.4, 0.8, 0.3), temp="Aggressive", inten=0.7),
       "Karate princess.")
simple(["toph beifong", "toph", "the blind bandit"],
       R("Toph Beifong", "Biped", ["Stomp", "Shoots"], 1.3, 35, (0.3, 0.7, 0.3), "Earth", temp="Aggressive", inten=0.8,
         payload=R("earthbent boulder", "Boulder", ["Heavy"], 1.0, 80, elem="Earth")), "The greatest earthbender.")
simple(["katara", "katara waterbender"],
       R("Katara", "Biped", ["Constrict", "Pull"], 1.6, 45, (0.2, 0.5, 1.0), "Water", temp="Protective", inten=0.8),
       "Water whips and bloodbending-free holds.")
simple(["prince zuko", "zuko", "zuko firebender"],
       R("Zuko", "Biped", ["Breath", "Blade"], 1.7, 60, (0.9, 0.2, 0.1), "Fire", temp="Aggressive", inten=0.8),
       "Honor! Dual broadswords and fire.")
simple(["azula", "princess azula", "blue fire azula"],
       R("Azula", "Biped", ["Beam", "Fast"], 1.7, 55, (0.3, 0.5, 1.0), "Electric", temp="Aggressive", inten=1.0),
       "Blue fire and lightning.")
simple(["avatar aang", "aang the last airbender", "avatar state"],
       R("Aang", "Biped", ["Glide", "Repel", "Fast"], 1.4, 35, (1.0, 0.6, 0.15), "Wind", temp="Playful", inten=0.9),
       "The last airbender.")
simple(["momo the lemur", "momo avatar", "winged lemur"],
       R("Momo", "Bird", ["Flies", "Grab"], 0.5, 2, (0.9, 0.85, 0.7), temp="Playful", inten=1.0), "Aang's winged lemur.")
