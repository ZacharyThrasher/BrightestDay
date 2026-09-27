"""Generates assets/icons/folklore_cn.ron: Sun Wukong + Journey to the West, Fengshen Yanyi (Nezha, Erlang
Shen), Chinese mythology and beasts, Romance of the Three Kingdoms and festival things, all mapped onto the
closed bd_core mechanics. Usage: python tools/icons/folklore_cn/gen.py [out.ron]

Content rules: public-domain sources only (Journey to the West, Wu Cheng'en, 16th c.; Fengshen Yanyi, 16th c.;
Romance of the Three Kingdoms, 14th c.; classical Chinese mythology / Shanhaijing). Every shape is our own
blocky take on the classical description - nothing is modelled on a modern game, film or comic design.

The Ruyi Jingu Bang and the telescoping weapons use Mechanic::Extends (bd_core v1.7): a held melee weapon whose
length is live state (twirls, a charged thrust that shoots it out tens of metres, a falling-pillar slam, a pole
vault). Sun Wukong carries it (Extends on a creature = strikes with the extending staff from far away).

Names another pack already owns are dropped (so a regeneration stays warning-free). This pack loads after
icons.ron / anime / dc / film_tv and before games / internet / ...: the Wukong, Nezha, fenghuang and huli jing
names were moved here from internet.ron / film_tv.ron (their generators drop names another pack owns)."""
import glob
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools/icons/internet"))
sys.path.insert(0, str(ROOT / "tools/lexicon_build"))
import lib  # noqa: E402
from lib import E, R, ENTRIES, S, emit, check  # noqa: E402
from dedupe_icon_names import norm  # noqa: E402

for m in ("Transforms", "Extends"):
    if m not in lib.MECH_ORDER:
        lib.MECH_ORDER.append(m)
lib.FORMS.add("Hands")
lib.ROLES.update({"Seat", "Wheel", "Exhaust", "Cloth", "Hatch", "Eye", "Blade", "Grip", "Support"})
PACK = "folklore_cn.ron"
# Names this pack owns although another pack listed them first (removed there by hand, 2026-09): the Wukong kit
# and fenghuang / huli jing from internet.ron, the Nezha weapons from film_tv.ron, "extending staff" from anime.ron.
TAKE = {"sun wukong", "monkey king", "wukong", "ruyi jingu bang", "ruyi bang", "monkey king's staff",
        "monkey king staff", "golden cudgel", "extending staff", "somersault cloud", "jindou yun", "nimbus cloud",
        "fenghuang", "huli jing", "universe ring", "qiankun circle", "qiankun ring", "nezha ring", "ne zha ring",
        "fire-tipped spear", "fire tipped spear", "nezha spear", "ne zha spear", "wind fire wheels", "nezha wheels",
        "ne zha wind fire wheels"}

JTTW = "Journey to the West (Wu Cheng'en, 16th c.)"
FSYY = "Fengshen Yanyi (Investiture of the Gods, 16th c.)"
MYTH = "Chinese mythology"
SHJ = "Chinese mythology (Shanhaijing)"
ROTK = "Romance of the Three Kingdoms (Luo Guanzhong, 14th c.)"
HIST = "Chinese history & legend"
FEST = "Chinese festival tradition"

GOLD = (1.0, 0.78, 0.2)
RED = (0.9, 0.15, 0.12)
JADE = (0.35, 0.85, 0.55)
GREEN = (0.2, 0.75, 0.4)
SILVER = (0.82, 0.86, 0.95)
MOON = (0.85, 0.9, 1.0)
SEA = (0.2, 0.6, 0.95)
ORANGE = (1.0, 0.5, 0.12)
BONE = (0.92, 0.9, 0.82)
CLAY = (0.72, 0.45, 0.3)
STONE = (0.7, 0.66, 0.6)
STRAW = (0.9, 0.78, 0.4)
VERM = (1.0, 0.4, 0.15)

Z = (90, 0, 0)  # a cylinder lying along Z (the held-item axis)


# ============================================================================================ shapes
def ruyi():
    """The staff: an iron-dark shaft with a gold band at each end. Grip = origin, -Z forward."""
    s = S()
    s.cyl((0, 0, -3.2), (0.55, 9.6, 0.55), "D", rot=Z)          # shaft, z +1.6 .. -8.0
    s.cyl((0, 0, 1.15), (1.2, 0.9, 1.2), "A", rot=Z)            # gold band (back end)
    s.cyl((0, 0, -7.55), (1.2, 0.9, 1.2), "A", rot=Z)           # gold band (front end)
    s.cyl((0, 0, 0.0), (0.75, 1.1, 0.75), "D", part=1, rot=Z)   # grip wrap
    s.part(1, 0, (0, 0, 0), "Grip")
    s.part(2, 0, (0, 0, -2.2), "Support")
    return s


def monkey(staff=False, arms_up=False):
    """Sun Wukong: long arms, heart-shaped face, golden fillet, curling tail (+ the staff in the right hand)."""
    s = S()
    # legs (part 10/11) + feet
    s.cyl((-0.9, 2.6, 0.0), (1.2, 4.0, 1.2), "P", part=10)
    s.cyl((0.9, 2.6, 0.0), (1.2, 4.0, 1.2), "P", part=11)
    s.box((-0.9, 0.35, -0.3), (1.3, 0.7, 1.8), "D", part=10)
    s.box((0.9, 0.35, -0.3), (1.3, 0.7, 1.8), "D", part=11)
    # tiger-skin kilt + belt, torso, pale belly
    s.cyl((0, 4.6, 0.0), (3.2, 1.6, 2.4), "D")
    s.cyl((0, 5.35, 0.0), (3.0, 0.6, 2.3), "A")
    s.ell((0, 7.0, 0.1), (3.0, 3.6, 2.2), "P")
    s.ell((0, 6.8, -0.8), (1.8, 2.4, 0.8), "C")
    # tail (part 30): back, up, curl
    s.ell((0, 5.6, 2.3), (0.7, 0.7, 2.8), "P", part=30, rot=(-35, 0, 0))
    s.ell((0, 7.3, 3.4), (0.7, 2.4, 0.7), "P", part=30)
    s.ell((0, 8.6, 2.9), (0.7, 0.7, 1.4), "P", part=30)
    # arms (20 left, 21 right): long monkey arms + hands
    if arms_up:
        s.cyl((-2.2, 9.6, -0.3), (0.9, 3.4, 0.9), "P", part=20, rot=(0, 0, -20))
        s.ell((-2.8, 11.4, -0.4), (1.0, 1.0, 1.0), "C", part=20)
    else:
        s.cyl((-2.0, 6.6, -0.2), (0.9, 3.8, 0.9), "P", part=20)
        s.ell((-2.0, 4.5, -0.3), (1.0, 1.0, 1.0), "C", part=20)
    s.cyl((2.0, 6.8, -0.5), (0.9, 3.4, 0.9), "P", part=21)
    s.ell((2.1, 5.0, -0.8), (1.0, 1.0, 1.0), "C", part=21)
    # head (part 1): round head, pale heart face, dark brow, golden eyes, muzzle, ears, the golden fillet
    s.ell((0, 10.3, -0.1), (2.6, 2.6, 2.4), "P", part=1)
    s.ell((0, 10.1, -1.0), (1.9, 1.9, 0.8), "C", part=1)
    s.box((0, 10.8, -1.35), (1.9, 0.5, 0.5), "D", part=1)
    s.box((-0.5, 10.4, -1.45), (0.6, 0.5, 0.3), "A", part=1)
    s.box((0.5, 10.4, -1.45), (0.6, 0.5, 0.3), "A", part=1)
    s.ell((0, 9.7, -1.4), (1.3, 0.9, 0.8), "C", part=1)
    s.box((0, 9.5, -1.8), (0.7, 0.3, 0.2), "D", part=1)
    s.ell((-1.4, 10.3, 0.0), (0.6, 1.0, 0.8), "C", part=1)
    s.ell((1.4, 10.3, 0.0), (0.6, 1.0, 0.8), "C", part=1)
    s.cyl((0, 11.05, -0.1), (2.8, 0.8, 2.6), "A", part=1)     # jingu fillet
    s.cone((0, 12.0, 0.1), (1.0, 0.9, 1.0), "P", part=1)       # tuft
    s.part(10, 0, (-0.9, 4.6, 0.0), "LegBackLeft")
    s.part(11, 0, (0.9, 4.6, 0.0), "LegBackRight")
    s.part(30, 0, (0.0, 5.0, 1.1), "Tail")
    s.part(20, 0, (-1.8, 8.3, 0.0), "ArmLeft")
    s.part(21, 0, (1.8, 8.3, 0.0), "ArmRight")
    s.part(1, 0, (0.0, 8.9, 0.0), "Head")
    if staff:
        # the staff planted upright in the right hand (Weapon, parented to the right arm)
        s.cyl((2.1, 6.6, -0.8), (0.9, 12.8, 0.9), "D", part=22)
        s.cyl((2.1, 12.5, -0.8), (1.5, 1.0, 1.5), "A", part=22)
        s.cyl((2.1, 0.7, -0.8), (1.5, 1.0, 1.5), "A", part=22)
        s.part(22, 21, (2.1, 5.0, -0.8), "Weapon")
    return s


def temple():
    """72 transformations, alt mode: the little temple Wukong became to hide from Erlang Shen - eyes as windows,
    mouth as the door, and his tail left sticking up behind it as a flagpole. Part ids match monkey()."""
    s = S()
    s.box((0, 0.5, 0), (8.0, 1.0, 7.0), "D")                    # plinth
    s.box((0, 3.5, 0), (6.0, 5.0, 5.0), "P")                    # hall
    s.cyl((-2.6, 3.5, -2.6), (0.8, 5.0, 0.8), "A", part=10)     # pillars = legs
    s.cyl((2.6, 3.5, -2.6), (0.8, 5.0, 0.8), "A", part=11)
    s.box((-0.8, 2.8, -2.6), (1.5, 3.6, 0.4), "D", part=20)     # door leaves = arms
    s.box((0.8, 2.8, -2.6), (1.5, 3.6, 0.4), "D", part=21)
    s.box((0, 6.6, 0), (8.4, 0.8, 7.4), "P", part=1)            # roof = head
    s.box((0, 7.6, 0), (6.0, 1.2, 5.0), "P", part=1)
    s.box((0, 8.4, 0), (6.6, 0.5, 0.8), "A", part=1)            # ridge
    s.cone((-4.1, 7.3, -3.6), (0.8, 1.2, 0.8), "A", part=1, rot=(0, 0, 30))
    s.cone((4.1, 7.3, -3.6), (0.8, 1.2, 0.8), "A", part=1, rot=(0, 0, -30))
    s.box((-1.9, 4.6, -2.6), (1.0, 1.0, 0.4), "C", part=1)      # eye windows
    s.box((1.9, 4.6, -2.6), (1.0, 1.0, 0.4), "C", part=1)
    s.cyl((0, 5.8, 4.0), (0.5, 10.0, 0.5), "D", part=30)        # the tail flagpole
    s.box((0.9, 10.0, 4.0), (1.6, 1.2, 0.3), "A", part=30)
    s.part(10, 0, (-2.6, 1.0, -2.6), "LegBackLeft")
    s.part(11, 0, (2.6, 1.0, -2.6), "LegBackRight")
    s.part(30, 0, (0.0, 1.0, 4.0), "Tail")
    s.part(20, 0, (-1.6, 2.8, -2.6), "ArmLeft")
    s.part(21, 0, (1.6, 2.8, -2.6), "ArmRight")
    s.part(1, 0, (0.0, 6.2, 0.0), "Head")
    return s


def somersault_cloud():
    """Jindou yun: a curl of white auspicious-cloud puffs with gold spiral curls and a gold underside."""
    s = S()
    s.ell((0, -0.9, 0.4), (5.2, 0.9, 5.6), "A")                 # gold underside glow
    s.ell((0, 0, 0), (6.0, 2.6, 5.0), "C")
    s.ell((-2.4, 0.6, 1.2), (3.4, 2.8, 3.4), "C")
    s.ell((2.4, 0.6, 1.2), (3.4, 2.8, 3.4), "C")
    s.ell((0, 0.9, -1.8), (3.6, 2.8, 3.4), "C")
    s.cyl((-2.6, 0.4, -2.2), (2.2, 0.6, 2.2), "A", rot=(0, 0, 90))   # xiangyun spiral curls
    s.cyl((2.6, 0.4, -2.2), (2.2, 0.6, 2.2), "A", rot=(0, 0, 90))
    s.cyl((-2.6, 0.4, -2.2), (1.0, 0.8, 1.0), "C", rot=(0, 0, 90))
    s.cyl((2.6, 0.4, -2.2), (1.0, 0.8, 1.0), "C", rot=(0, 0, 90))
    s.ell((0, -0.2, 4.2), (2.2, 1.2, 3.6), "C", part=1)          # trailing wisp
    s.ell((0.6, -0.4, 6.6), (1.2, 0.8, 2.8), "A", part=1)
    s.ell((0, 1.4, 0.4), (3.2, 0.8, 3.0), "C", part=2)           # where the rider stands
    s.part(1, 0, (0, 0, 2.6), "Cloth")
    s.part(2, 0, (0, 1.8, 0.4), "Seat")
    return s


def rake():
    """Zhu Bajie's nine-toothed iron rake: long haft, a gold crossbar and nine tines hanging below it."""
    s = S()
    s.cyl((0, 0, -3.5), (0.6, 9.0, 0.6), "D", rot=Z)
    s.cyl((0, 0, -7.4), (1.0, 0.6, 1.0), "A", rot=Z)
    s.box((0, 0, -8.4), (8.6, 1.1, 1.1), "A")
    for i in range(9):
        x = -4.0 + i
        s.box((x, -1.5, -8.4), (0.5, 2.0, 0.5), "P")
        s.cone((x, -2.9, -8.4), (0.5, 0.9, 0.5), "P", rot=(180, 0, 0))
    return s


def crescent_spade():
    """Monk's spade: a flat spade blade in front, a crescent moon at the butt."""
    s = S()
    s.cyl((0, 0, -3.5), (0.55, 11.0, 0.55), "D", rot=Z)         # z +2.0 .. -9.0
    s.box((0, 0, -9.0), (1.2, 0.6, 0.6), "A")
    s.box((0, 0, -10.2), (2.4, 0.35, 2.2), "P")
    s.box((0, 0, -11.4), (1.8, 0.35, 0.6), "C")
    s.cyl((0, 0, 2.8), (3.2, 0.35, 2.4), "A")
    s.cyl((0, 0, 3.6), (2.6, 0.8, 2.2), "A", sub=True)
    s.box((0, 0, 1.8), (0.8, 0.6, 0.8), "A")
    return s


def palm_fan():
    """Princess Iron Fan's palm-leaf fan: a huge flat leaf with a gold rim, midrib and veins."""
    s = S()
    s.cyl((0, 0, -1.5), (0.7, 4.0, 0.7), "D", rot=Z)
    s.ell((0, -0.6, 0.9), (0.6, 1.4, 0.6), "A")                 # tassel
    s.ell((0, -0.05, -7.5), (8.0, 0.4, 9.0), "A")               # rim
    s.ell((0, 0, -7.4), (7.4, 0.6, 8.4), "P")                   # leaf
    s.box((0, 0.3, -7.2), (0.5, 0.5, 8.0), "A")                 # midrib
    s.box((-1.9, 0.3, -7.2), (0.35, 0.4, 5.4), "A", rot=(0, -32, 0))
    s.box((1.9, 0.3, -7.2), (0.35, 0.4, 5.4), "A", rot=(0, 32, 0))
    s.box((-2.6, 0.3, -6.2), (0.35, 0.4, 3.6), "A", rot=(0, -62, 0))
    s.box((2.6, 0.3, -6.2), (0.35, 0.4, 3.6), "A", rot=(0, 62, 0))
    return s


def ring(outer=6.0, inner=4.2, thick=1.2, studs=True):
    """A gold ring lying flat (disc forms spin flat): the universe ring / the tightening fillet."""
    s = S()
    s.cyl((0, 0, 0), (outer, thick, outer), "A")
    s.cyl((0, 0, 0), (inner, thick * 2, inner), "A", sub=True)
    if studs:
        r = (outer + inner) / 4
        for a in range(0, 360, 90):
            x, z = r * math.cos(math.radians(a)), r * math.sin(math.radians(a))
            s.box((x, thick * 0.55, z), (0.8, 0.6, 0.8), "C")
    return s


def fillet():
    s = ring(5.0, 3.8, 1.0, studs=False)
    s.box((0, 0.2, -2.3), (1.4, 1.4, 0.6), "C")                 # the front plate
    s.box((0, 0.2, -2.6), (0.6, 0.6, 0.3), "A")
    return s


def wheels():
    """Nezha's wind-fire wheels: two flame-rimmed wheels side by side, spinning under the rider's feet."""
    s = S()
    for side, pid in ((-1, 1), (1, 2)):
        x = side * 1.3
        s.cyl((x, 1.6, 0), (3.2, 0.6, 3.2), "P", part=pid, rot=(0, 0, 90))
        s.cyl((x, 1.6, 0), (1.8, 1.2, 1.8), "P", part=pid, rot=(0, 0, 90), sub=True)
        s.cyl((x, 1.6, 0), (0.9, 0.8, 0.9), "C", part=pid, rot=(0, 0, 90))
        for a in range(0, 360, 60):
            y, z = 1.6 + 2.0 * math.cos(math.radians(a)), 2.0 * math.sin(math.radians(a))
            s.cone((x, y, z), (0.6, 1.2, 0.6), "A", part=pid, rot=(a, 0, 0))
        s.part(pid, 0, (x, 1.6, 0), "Wheel")
    s.box((0, 1.6, 0), (2.0, 0.3, 0.3), "D")                    # axle the feet stand across
    return s


def fire_spear():
    """Nezha's fire-tipped spear: red shaft, gold collar, a flame-shaped point."""
    s = S()
    s.cyl((0, 0, -3.8), (0.55, 11.0, 0.55), "D", rot=Z)         # z +1.7 .. -9.3
    s.ell((0, 0, -9.0), (1.2, 1.2, 1.0), "A")                   # tassel knot
    s.cone((0, 0, -10.8), (0.9, 3.0, 0.9), "P", rot=(-90, 0, 0))
    s.cone((0, 0, -10.2), (1.6, 1.8, 1.6), "A", rot=(-90, 0, 0))
    s.cone((0.5, 0.4, -9.8), (0.5, 1.4, 0.5), "C", rot=(-70, 0, 20))
    s.cone((-0.5, -0.4, -9.8), (0.5, 1.4, 0.5), "C", rot=(-70, 0, -20))
    return s


def eye_beams(n=2, spread=2.4, pupil="C", eye="A"):
    """Fiery golden eyes (two) / Erlang's third eye (one, upright) with their beams."""
    s = S()
    xs = [0.0] if n == 1 else [-spread, spread]
    for x in xs:
        if n == 1:
            s.ell((x, 0, 0), (1.6, 3.0, 1.4), eye)
            s.ell((x, 0, -0.5), (0.8, 1.4, 0.6), pupil)
        else:
            s.ell((x, 0, 0), (3.3, 2.1, 1.8), eye)
            s.ell((x, 0, -0.6), (1.2, 1.2, 0.8), pupil)
        s.cyl((x, 0, -12.0), (1.35, 22.5, 1.35), "C", rot=Z)
        s.cyl((x, 0, -12.0), (2.4, 21.0, 2.4), eye, rot=Z)
        s.ell((x, 0, -24.3), (3.6, 3.6, 3.0), "C")
    s.box((0, 1.6 if n == 2 else 2.0, 0.6), (8.4 if n == 2 else 3.0, 0.75, 1.5), "D")
    return s


def tri_blade():
    """Erlang Shen's three-pointed double-edged blade."""
    s = S()
    s.cyl((0, 0, -3.8), (0.55, 11.0, 0.55), "D", rot=Z)
    s.box((0, 0, -9.3), (3.0, 0.5, 0.5), "A")                   # crossguard
    s.box((0, 0, -11.0), (0.9, 0.3, 3.2), "P")
    s.cone((0, 0, -13.0), (0.9, 1.2, 0.3), "P", rot=(-90, 0, 0))
    for x in (-1.2, 1.2):
        s.box((x, 0, -10.4), (0.5, 0.3, 1.8), "P")
        s.cone((x, 0, -11.7), (0.5, 0.9, 0.3), "P", rot=(-90, 0, 0))
    return s


def guandao():
    """The Green Dragon Crescent Blade: a long haft, a broad crescent blade, a dragon-mouth collar."""
    s = S()
    s.cyl((0, 0, -3.5), (0.6, 12.0, 0.6), "D", rot=Z)           # z +2.5 .. -9.5
    s.ell((0, 1.0, -11.2), (0.4, 4.6, 4.4), "P")
    s.ell((0, 2.4, -8.8), (1.0, 3.0, 2.8), "P", sub=True)       # carve the back into a crescent
    s.ell((0, 0.2, -12.9), (0.5, 3.2, 0.9), "C")                # bright edge
    s.ell((0, 0, -9.4), (1.3, 1.3, 1.5), "A")                   # green dragon collar
    s.cone((0, 0.9, -9.4), (0.5, 1.0, 0.5), "A", rot=(20, 0, 0))
    s.cone((0, 0, 3.1), (0.6, 1.2, 0.6), "A", rot=Z)            # butt spike
    return s


def halberd():
    """Lü Bu's sky-piercer: a spear point with a crescent blade on each side."""
    s = S()
    s.cyl((0, 0, -3.5), (0.55, 12.0, 0.55), "D", rot=Z)
    s.cone((0, 0, -11.6), (0.8, 2.8, 0.8), "P", rot=(-90, 0, 0))
    for side in (-1, 1):
        s.ell((side * 1.2, 0, -9.8), (1.8, 0.3, 2.6), "P")
        s.ell((side * 0.5, 0, -9.8), (1.4, 0.6, 2.0), "P", sub=True)
    s.cyl((0, 0, -10.0), (0.55, 1.6, 0.55), "D", rot=Z)         # re-fill the shaft through the cut
    s.ell((0, -0.6, -8.6), (0.9, 1.2, 0.9), "A")                # red tassel
    return s


def pangu_axe():
    """Pangu's axe: a colossal broad axe - haft, bearded blade with a gold edge, butt spike."""
    s = S()
    s.cyl((0, 0, -5.0), (0.9, 14.0, 0.9), "D", rot=Z)           # z +2.0 .. -12.0
    s.ell((0, 2.8, -11.0), (0.7, 7.5, 5.0), "P")
    s.ell((0, 2.8, -7.9), (1.4, 5.4, 3.0), "P", sub=True)
    s.ell((0, 2.8, -13.3), (0.8, 7.0, 1.2), "A")
    s.box((0, -1.2, -11.0), (0.6, 2.0, 1.2), "P")
    s.cyl((0, 0, -11.0), (1.3, 1.6, 1.3), "A", rot=Z)
    s.cyl((0, 0, 1.4), (1.2, 0.8, 1.2), "A", rot=Z)
    return s


def dragon(segments=9, dance=False):
    """A long wingless Chinese dragon: whiskered head with antlers, a snaking body of segments with a gold
    dorsal ridge, four small clawed legs and a flame tail tuft. dance=True: the dragon-dance costume - no
    claws, a pair of performer legs under every segment."""
    s = S()
    y0 = 6.0 if not dance else 5.0
    # head (1) + jaw (2)
    s.ell((0, y0 + 0.4, -2.8), (2.6, 2.4, 2.8), "P", part=1)
    s.box((0, y0 - 0.1, -4.6), (2.0, 1.4, 2.6), "P", part=1)
    s.ell((0, y0 + 0.4, -5.8), (1.4, 1.0, 0.8), "C", part=1)    # nose
    s.box((-0.8, y0 + 1.1, -3.9), (0.6, 0.5, 0.4), "C", part=1)
    s.box((0.8, y0 + 1.1, -3.9), (0.6, 0.5, 0.4), "C", part=1)
    for side in (-1, 1):
        s.cyl((side * 1.1, y0 + 2.4, -2.2), (0.4, 2.4, 0.4), "A", part=1, rot=(-35, 0, side * -20))  # antler
        s.cyl((side * 1.6, y0 + 3.2, -1.6), (0.35, 1.4, 0.35), "A", part=1, rot=(-10, 0, side * -60))
        s.cyl((side * 1.6, y0 - 0.9, -5.4), (0.3, 3.4, 0.3), "A", part=1, rot=(40, 0, side * 40))   # whisker
    s.cone((0, y0 + 1.4, -1.2), (1.6, 1.8, 1.0), "A", part=1, rot=(-60, 0, 0))                     # mane
    s.box((0, y0 - 1.0, -4.3), (1.8, 0.6, 2.6), "D", part=2)
    s.part(1, 0, (0, y0, -2.0), "Head")
    s.part(2, 1, (0, y0 - 0.8, -3.2), "Jaw")
    # body: a chain of Tail parts snaking in x and y
    prev, pid = 0, 3
    for i in range(segments):
        t = i / max(1, segments - 1)
        z = 0.4 + i * 2.6
        x = 1.6 * math.sin(i * 0.7)
        y = y0 + (0.0 if dance else 1.6 * math.sin(i * 0.9 + 0.6)) - 0.1 * i
        r = 1.5 - 0.9 * t
        s.ell((x, y, z), (2 * r, 2 * r, 3.4), "P", part=pid)
        s.ell((x, y - 0.5 * r, z), (1.4 * r, r, 3.0), "C", part=pid)                                  # pale belly
        s.cone((x, y + r + 0.3, z), (0.5, 0.9, 0.9), "A", part=pid)                                   # dorsal ridge
        if dance:
            for side in (-1, 1):
                s.cyl((x + side * 0.5, (y - r) / 2, z), (0.6, max(1.0, y - r), 0.6), "D", part=pid)
        elif i in (1, 5):
            for side in (-1, 1):
                s.cyl((x + side * r, y - r - 0.6, z), (0.5, 1.8, 0.5), "P", part=pid, rot=(0, 0, side * 25))
                s.box((x + side * (r + 0.4), y - r - 1.5, z - 0.4), (0.8, 0.4, 1.0), "A", part=pid)
        s.part(pid, prev, (x, y, z - 1.3), "Tail")
        prev, pid = pid, pid + 1
    s.cone((x, y + 0.2, z + 2.6), (1.4, 2.4, 0.6), "A", part=prev, rot=(90, 0, 0))                    # tail tuft
    return s


def fenghuang():
    """The fenghuang: a crested pheasant-like bird with long sweeping plume tails with eye spots."""
    s = S()
    s.ell((0, 3.0, 0), (2.2, 2.2, 4.0), "P")
    s.ell((0, 5.0, -2.2), (1.3, 1.6, 1.3), "P", part=1)
    s.cone((0, 5.0, -3.2), (0.5, 1.0, 0.5), "A", part=1, rot=(-90, 0, 0))
    s.cone((0, 6.5, -1.9), (0.4, 1.6, 0.3), "A", part=1, rot=(-20, 0, 0))
    s.cone((0, 6.4, -1.4), (0.4, 1.4, 0.3), "A", part=1, rot=(-35, 0, 0))
    s.box((-0.45, 5.3, -2.7), (0.35, 0.35, 0.3), "C", part=1)
    s.box((0.45, 5.3, -2.7), (0.35, 0.35, 0.3), "C", part=1)
    for side, pid in ((-1, 20), (1, 21)):
        s.ell((side * 2.8, 3.6, 0.2), (4.0, 0.6, 2.8), "P", part=pid)
        s.ell((side * 4.6, 3.8, 0.8), (1.4, 0.5, 2.6), "A", part=pid)
        s.part(pid, 0, (side * 1.0, 3.6, 0.0), "WingLeft" if side < 0 else "WingRight")
    for k, (x, rz) in enumerate(((-1.0, -12), (0.0, 0), (1.0, 12))):
        s.ell((x, 2.2 - 0.3 * k % 2, 5.2), (0.8, 0.4, 7.0), "A" if k != 1 else "P", part=30, rot=(12, rz, 0))
        s.ell((x * 1.6, 1.5, 8.6), (1.0, 0.5, 1.2), "C", part=30)
    s.cyl((-0.6, 1.0, 0.2), (0.3, 2.0, 0.3), "D")
    s.cyl((0.6, 1.0, 0.2), (0.3, 2.0, 0.3), "D")
    s.part(1, 0, (0, 4.2, -1.6), "Head")
    s.part(30, 0, (0, 2.6, 1.8), "Tail")
    return s


def great_wall():
    """A long stretch of rampart with crenellations and three watchtowers (walls scale by width = x)."""
    s = S()
    s.box((0, 3.0, 0), (60.0, 6.0, 4.0), "P")
    s.box((0, 6.2, 0), (60.0, 0.4, 3.2), "D")                   # walkway
    for i in range(15):
        x = -28.0 + i * 4.0
        s.box((x, 6.9, -1.7), (1.6, 1.4, 0.6), "P")
        s.box((x, 6.9, 1.7), (1.6, 1.4, 0.6), "P")
    for x in (-24.0, 0.0, 24.0):
        s.box((x, 4.5, 0), (6.0, 9.0, 6.0), "P")
        s.box((x, 9.4, 0), (6.8, 0.8, 6.8), "A")
        s.box((x, 7.0, -3.05), (1.2, 1.6, 0.3), "D")
    return s


def straw_boat():
    """Zhuge Liang's straw boats: a hull lined with straw bales and straw men that catch the arrows."""
    s = S()
    s.box((0, 1.0, 0), (12.0, 2.0, 4.0), "D")
    s.cone((6.8, 1.2, 0), (2.0, 1.8, 3.6), "D", rot=(0, 0, -90))
    s.cone((-6.8, 1.2, 0), (2.0, 1.8, 3.6), "D", rot=(0, 0, 90))
    for i in range(5):
        x = -4.0 + 2.0 * i
        s.cyl((x, 3.6, -0.8), (1.6, 3.2, 1.4), "A")
        s.ell((x, 5.6, -0.8), (1.0, 1.0, 1.0), "A")
    s.cyl((0, 6.5, 1.0), (0.4, 9.0, 0.4), "D")
    s.box((0, 7.0, 1.3), (4.4, 4.2, 0.2), "P")
    return s


def firecracker():
    s = S()
    s.cyl((0, 1.5, 0), (1.0, 3.0, 1.0), "A")
    s.cyl((0, 0.3, 0), (1.1, 0.4, 1.1), "C")
    s.cyl((0, 2.7, 0), (1.1, 0.4, 1.1), "C")
    s.cyl((0, 3.5, 0), (0.3, 1.0, 0.3), "D")
    return s


def sky_lantern():
    s = S()
    s.cyl((0, 1.8, 0), (2.0, 2.4, 2.0), "A")
    s.ell((0, 3.0, 0), (2.0, 1.0, 2.0), "A")
    s.cyl((0, 0.55, 0), (2.1, 0.3, 2.1), "D")
    s.cyl((0, 0.55, 0), (1.6, 0.6, 1.6), "D", sub=True)
    s.ell((0, 0.7, 0), (0.8, 0.9, 0.8), "C")
    return s


def iron_fan_shape():
    return palm_fan()


# ============================================================================================ entries
# ---------------------------------------------------------------- the staff (Extends)
E(["ruyi jingu bang", "ruyi", "ruyi bang", "jingu bang", "ruyi staff", "monkey king's staff", "monkey king staff",
   "the monkey king's staff", "wukong's staff", "wukong staff", "sun wukong's staff", "golden cudgel",
   "the golden cudgel", "as-you-will staff", "as you will staff", "as-you-will gold-banded cudgel",
   "golden-hooped rod", "compliant golden-hooped rod", "extending staff", "monkey staff", "ruyi jin gu bang",
   "ruyi jingubang", "golden hoop rod", "golden-banded staff", "gold banded staff", "iron cudgel of the monkey king"],
  JTTW,
  R("Ruyi Jingu Bang", "Spear", ["Blunt", "Extends"], "Light", 2.4, 60, accent=GOLD), ruyi(),
  "the as-you-will staff: twirls, shoots out tens of metres on a charged thrust, falls as a pillar, vaults you")

# ---------------------------------------------------------------- generic telescoping (adjectives cover the rest)
E(["stretchy fist", "stretching fist", "stretchy punch", "stretching punch", "rubber fist", "rubber punch",
   "stretchy arm", "stretching arm", "rubber arm", "elastic fist", "elastic punch", "long-range punch",
   "extending fist", "extending punch"], "Telescoping weapons",
  R("Stretchy Fist", "Gauntlet", ["Blunt", "Extends"], "Light", 0.6, 6, accent=(1.0, 0.55, 0.45)),
  None, "the fist shoots out on a stretching arm and snaps back")
E(["extending sword", "extendable sword", "telescoping sword", "telescopic sword", "retractable sword",
   "stretching sword", "extending blade"], "Telescoping weapons",
  R("Extending Sword", "Sword", ["Blade", "Extends"], "Light", 1.3, 4))
E(["telescoping spear", "telescopic spear", "extending spear", "extendable spear", "retractable spear"],
  "Telescoping weapons", R("Telescoping Spear", "Spear", ["Pierce", "Extends"], "Light", 2.2, 5))
E(["extending pole", "telescoping staff", "telescopic staff", "retractable staff", "extendable staff",
   "telescoping pole", "extendable pole", "expandable baton", "telescoping baton", "extending baton"],
  "Telescoping weapons", R("Extending Pole", "Staff", ["Blunt", "Extends"], "Light", 2.0, 6))

# ---------------------------------------------------------------- Sun Wukong and his kit
E(["sun wukong", "monkey king", "wukong", "the monkey king", "sun wu kong", "stone monkey", "handsome monkey king",
   "monkey king wukong", "sun wukong the monkey king", "great sage sun wukong", "sun xingzhe", "monkey king sun"],
  JTTW,
  R("Sun Wukong", "Biped", ["Blunt", "Fast", "Extends"], "Light", 1.7, 55, accent=GOLD, temp="Playful", inten=0.9),
  monkey(staff=True), "leaps about playfully and strikes with the as-you-will staff from far outside his body")
E(["somersault cloud", "jindou yun", "jindouyun", "jin dou yun", "cloud somersault", "wukong's cloud",
   "monkey king's cloud", "somersault cloud of the monkey king", "nimbus cloud", "golden cloud somersault"], JTTW,
  R("Somersault Cloud", "Vehicle", ["Flies", "Piloted", "Fast"], "Wind", 2.6, 20, accent=GOLD),
  somersault_cloud(), "hop on: a fast flying cloud mount (108,000 li per somersault)")
E(["hair clones", "monkey hair clones", "wukong's clones", "wukong clones", "clone army of monkeys", "monkey clones",
   "army of monkey clones", "hair clone", "monkey army", "army of monkeys", "pluck a hair", "wukong's hair clones",
   "monkey king clones", "clones of the monkey king"], JTTW,
  R("Hair Clones", "Biped", ["Blunt", "Swarm", "Fast"], "Light", 0.9, 12, count=7, accent=GOLD, temp="Playful",
    inten=1.0), monkey(staff=False, arms_up=True), "a pinch of hairs blown out as a scampering pack of little monkeys")
E(["72 transformations", "seventy-two transformations", "seventy two transformations", "72 earthly transformations",
   "seventy-two earthly transformations", "seventy two earthly transformations", "72 changes", "72 earthly changes",
   "wukong's transformations", "monkey king's transformations", "72 polymorphs", "the 72 transformations",
   "temple disguise", "monkey temple"], JTTW,
  R("Seventy-Two Transformations", "Biped", ["Blunt", "Fast", "Transforms"], "Light", 2.2, 80, accent=GOLD,
    temp="Playful", inten=0.9), temple(),
  "shapeshifting: a little temple (tail as the flagpole) slides up to you and unfolds into the monkey")
E(["fiery golden eyes", "huoyan jinjing", "huo yan jin jing", "fire eyes golden pupils", "golden eyes",
   "wukong's eyes", "monkey king's eyes", "fiery eyes", "golden gaze", "fire eyes and golden gaze"], JTTW,
  R("Fiery Golden Eyes", "Gun", ["Pierce", "Beam"], "Fire", 1.0, 0.5, accent=GOLD), eye_beams(2),
  "twin golden beams that see through any disguise")
E(["golden headband", "tightening crown", "golden fillet", "tightening headband", "headache spell",
   "tightening spell", "tight fillet spell", "fillet-tightening spell", "golden circlet", "golden hoop",
   "jingu er", "jingu quan", "golden band spell", "tight-fillet spell", "headband of the monkey king"], JTTW,
  R("Golden Fillet", "Disc", ["Trap", "Homing", "Constrict"], "Psychic", 0.8, 3, accent=GOLD), fillet(),
  "a homing gold ring that snaps onto a head and squeezes (the headache spell)")

# ---------------------------------------------------------------- Journey to the West: companions and demons
E(["zhu bajie's rake", "zhu bajie rake", "nine-tooth rake", "nine tooth rake", "nine-toothed rake",
   "nine toothed rake", "nine-toothed iron rake", "pigsy's rake", "pigsy rake", "bajie's rake", "jiuchi dingpa",
   "jiu chi ding pa", "nine-tooth iron rake", "zhu wuneng's rake"], JTTW,
  R("Nine-Toothed Rake", "Hammer", ["Blunt", "Pierce", "Heavy"], "Light", 2.2, 40, accent=GOLD), rake(),
  "a heavy overhead rake: blunt and pierce")
E(["sha wujing's spade", "sha wujing spade", "crescent moon spade", "crescent-moon spade", "monk's spade",
   "monks spade", "monk spade", "yueyachan", "yue ya chan", "sandy's spade", "sha seng's spade",
   "sha wujing's staff", "demon-quelling staff", "crescent spade"], JTTW,
  R("Crescent Moon Spade", "Spear", ["Blade", "Blunt"], "Light", 2.6, 25, accent=SILVER), crescent_spade())
E(["bull demon king", "the bull demon king", "niu mowang", "niu mo wang", "demon bull king", "bull demon",
   "great strength king", "niumowang", "bull king"], JTTW,
  R("Bull Demon King", "Kaiju", ["Charge", "Stomp", "Heavy"], "Fire", 12.0, 20000, accent=RED, temp="Aggressive",
    inten=0.9), None, "a horned giant that charges and stomps, burning")
E(["iron fan", "princess iron fan's fan", "princess iron fan fan", "princess iron fan", "palm leaf fan",
   "palm-leaf fan", "banana leaf fan", "banana-leaf fan", "banana fan", "bajiao shan", "bajiaoshan",
   "plantain fan", "palm fan", "iron fan princess", "tieshan gongzhu", "iron fan princess's fan",
   "magic palm leaf fan", "fan of princess iron fan"], JTTW,
  R("Palm-Leaf Fan", "Hammer", ["Blunt", "Repel"], "Wind", 3.2, 30, accent=GOLD), palm_fan(),
  "every swing is a gale that blows people off the map")
E(["white bone demon", "baigujing", "bai gu jing", "white bone spirit", "lady white bone", "white boned demon",
   "skeleton demon", "white bone demoness", "baigu furen"], JTTW,
  R("White Bone Demon", "Biped", ["Grab", "Swarm"], "Shadow", 1.8, 40, count=3, accent=BONE, temp="Sneaky",
    inten=0.9), None, "three disguises closing in from behind")
E(["dragon king", "dragon king of the east sea", "ao guang", "aoguang", "dragon kings", "four dragon kings",
   "longwang", "long wang", "east sea dragon king", "sea dragon king", "the dragon king", "dragon king of the sea"],
  JTTW,
  R("Dragon King of the East Sea", "Serpent", ["Flies", "Breath", "Constrict"], "Water", 8.0, 3000, accent=SEA,
    temp="Calm", inten=0.6), dragon(9), "a sea dragon lord that breathes a torrent")

# ---------------------------------------------------------------- Fengshen Yanyi: Nezha, Erlang Shen
E(["wind fire wheels", "wind-fire wheels", "wind and fire wheels", "nezha's wind fire wheels", "nezha wind fire wheels",
   "nezha wheels", "ne zha wind fire wheels", "nezha's wheels", "fenghuo lun", "feng huo lun", "fire wheels of nezha",
   "hot wind fire wheels"], FSYY,
  R("Wind Fire Wheels", "Vehicle", ["Flies", "Fast", "Piloted"], "Fire", 1.4, 10, accent=ORANGE), wheels(),
  "stand on two blazing wheels and fly")
E(["universe ring", "qiankun circle", "qiankun ring", "nezha ring", "ne zha ring", "nezha's ring", "qian kun quan",
   "qiankun quan", "heaven and earth ring", "cosmos ring", "nezha's universe ring"], FSYY,
  R("Universe Ring", "Disc", ["Blunt", "Returning", "Heavy"], "Light", 1.2, 30, accent=GOLD), ring(),
  "a heavy golden ring thrown like a boomerang")
E(["red armillary sash", "armillary sash", "huntian ling", "hun tian ling", "huntian silk", "nezha's sash",
   "nezha sash", "red silk sash", "red sky damask", "heaven covering silk", "nezha's red sash"], FSYY,
  R("Huntian Ling", "Whip", ["Constrict", "Link"], "Fire", 4.0, 2, accent=RED),
  None, "a flying red sash that wraps and binds")
E(["fire-tipped spear", "fire tipped spear", "nezha spear", "ne zha spear", "nezha's spear", "huojian qiang",
   "huo jian qiang", "fire point spear", "fire-point spear", "fire lance of nezha"], FSYY,
  R("Fire-Tipped Spear", "Spear", ["Pierce", "Breath"], "Fire", 2.6, 6, accent=RED), fire_spear(),
  "lunges and breathes fire from the point")
E(["erlang's third eye", "erlang third eye", "erlang shen's third eye", "erlang shen third eye", "third eye of erlang",
   "erlang's eye", "heavenly eye of erlang", "erlang's heavenly eye"], JTTW,
  R("Erlang's Third Eye", "Gun", ["Pierce", "Beam"], "Psychic", 1.0, 0.5, accent=SILVER),
  eye_beams(1, eye="A"), "a single upright eye that fires a truth-seeing beam")
E(["three-pointed double-edged blade", "three pointed double edged blade", "sanjian liangren dao",
   "san jian liang ren dao", "erlang's blade", "erlang shen's blade", "erlang's trident", "three-pointed blade",
   "three pointed blade", "sanjian dao", "erlang's spear", "three-pointed two-edged blade"], JTTW,
  R("Three-Pointed Blade", "Spear", ["Blade", "Pierce"], "Light", 2.7, 8, accent=SILVER), tri_blade())
E(["xiaotian", "xiao tian", "xiaotian quan", "xiaotianquan", "howling celestial dog", "howling celestial hound",
   "celestial hound", "celestial dog", "erlang's dog", "erlang shen's dog", "roaring sky hound",
   "erlang's hound", "howling dog of heaven"], JTTW,
  R("Xiaotian", "Quadruped", ["Grab", "Homing", "Fast"], "Light", 1.2, 40, accent=SILVER, temp="Aggressive",
    inten=0.9), None, "the celestial hound runs you down and bites")
E(["erlang shen", "erlang", "er lang shen", "yang jian", "erlang god", "erlang zhenjun", "the god erlang"], JTTW,
  R("Erlang Shen", "Biped", ["Blade", "Pierce", "Beam"], "Light", 1.9, 90, accent=SILVER, temp="Aggressive",
    inten=0.7))

# ---------------------------------------------------------------- mythology
E(["hou yi's bow", "houyi's bow", "hou yi bow", "houyi bow", "bow of hou yi", "sun-shooting bow", "sun shooting bow",
   "vermilion bow", "yi the archer's bow", "hou yi", "houyi", "yi the archer"], MYTH,
  R("Hou Yi's Bow", "Bow", ["Shoots", "Pierce"], "Fire", 1.8, 3, accent=RED))
E(["shoot down the suns", "shooting down the suns", "nine suns", "ten suns", "hou yi shoots the suns",
   "rain of suns", "fall of the nine suns", "shoot the suns"], MYTH,
  R("Nine Suns", "Ball", ["DropFromSky", "Explodes", "Swarm"], "Fire", 1.2, 40, count=9, accent=ORANGE),
  None, "nine suns shot out of the sky come down burning")
E(["chang'e", "chang e", "chang-e", "moon goddess chang'e", "chang'e the moon goddess", "goddess chang'e",
   "chang o", "heng e", "chang'e moon goddess", "lady of the moon"], MYTH,
  R("Chang'e", "Biped", ["Flies", "SlowFall"], "Light", 1.7, 45, accent=MOON, temp="Calm", inten=0.5),
  None, "drifts weightless up toward the moon")
E(["jade rabbit", "moon rabbit", "yutu", "yu tu", "jade hare", "moon hare", "rabbit in the moon",
   "the jade rabbit", "moon rabbit with pestle"], MYTH,
  R("Jade Rabbit", "Quadruped", ["Bounce", "Blunt"], "Light", 0.7, 6, accent=JADE, temp="Playful", inten=0.8),
  None, "a bouncing rabbit swinging its elixir pestle")
E(["jade emperor", "the jade emperor", "yu huang", "yuhuang dadi", "yu huang da di", "jade emperor of heaven",
   "august personage of jade", "emperor of heaven"], JTTW,
  R("Jade Emperor", "Biped", ["Guard", "Heavy", "Beam"], "Light", 4.0, 800, accent=GOLD, temp="Protective",
    inten=0.9), None, "a towering heavenly guardian that stands between you and harm")
E(["pangu's axe", "pangu axe", "pan gu's axe", "pan gu axe", "axe of pangu", "kaitian axe", "kaitian fu",
   "heaven-splitting axe", "heaven splitting axe", "axe that split heaven and earth", "creation axe",
   "pangu's great axe"], MYTH,
  R("Pangu's Axe", "Axe", ["Blade", "Heavy"], "Earth", 4.5, 400, accent=GOLD), pangu_axe(),
  "a colossal axe that splits the ground")
E(["nuwa's five-coloured stones", "nüwa's five-coloured stones", "nuwa's five coloured stones",
   "nuwa's five colored stones", "nuwa's stones", "nuwa stones", "five-coloured stones", "five coloured stones",
   "five colored stones", "five-colored stones", "nuwa mends the sky", "mending the sky", "patch the sky",
   "nuwa repairs the sky"], MYTH,
  R("Nüwa's Five-Coloured Stones", "Dome", ["Barrier", "Guard"], "Earth", 4.0, 800, accent=(0.95, 0.55, 0.85)),
  None, "a patched sky over your head: a stone dome that stops everything")
E(["chinese dragon", "loong", "chinese long", "long dragon", "eastern dragon", "oriental dragon", "celestial dragon",
   "lung dragon", "chinese storm dragon", "chinese sky dragon", "summon a chinese dragon"], MYTH,
  R("Chinese Dragon", "Serpent", ["Flies", "Constrict", "ArticulatedTail"], "Electric", 9.0, 4000, accent=GOLD,
    temp="Calm", inten=0.6), dragon(10), "a wingless storm dragon that swims through the air")
E(["qilin", "qi lin", "kylin", "chinese unicorn", "ki lin", "qilin beast"], MYTH,
  R("Qilin", "Quadruped", ["Charge", "Fast"], "Fire", 1.9, 300, accent=JADE, temp="Protective", inten=0.6),
  None, "a scaled, single-horned beast that charges wreathed in fire")
E(["fenghuang", "feng huang", "chinese phoenix", "fenghuang bird", "feng and huang", "vermilion phoenix",
   "phoenix of the east"], MYTH,
  R("Fenghuang", "Bird", ["Flies", "Shoots", "Homing"], "Light", 2.4, 20, accent=VERM, temp="Protective", inten=0.8,
    payload=R("fenghuang feather", "Ball", ["Homing"], "Fire", 0.4, 0.5)), fenghuang(),
  "the empress-bird guards you and loosens homing flame feathers (the western phoenix dives and explodes)")
E(["nian", "nian beast", "nian monster", "the nian", "nianshou", "nian shou", "new year beast", "new year monster"],
  FEST,
  R("Nian", "Quadruped", ["Charge", "Stomp", "Heavy"], "Fire", 4.0, 3000, accent=RED, temp="Aggressive", inten=0.9),
  None, "the horned new-year beast (red and firecrackers scare it)")
E(["pixiu", "pi xiu", "piyao", "pi yao", "bixie", "pixiu lion", "winged pixiu"], MYTH,
  R("Pixiu", "Quadruped", ["Flies", "Attract"], "Light", 1.6, 200, accent=GOLD, temp="Protective", inten=0.7),
  None, "a winged lion that swallows everything that comes near")
E(["huli jing", "hulijing", "chinese fox spirit", "fox spirit", "jiuwei hu", "jiuweihu", "nine-tailed fox spirit",
   "nine tailed fox spirit", "daji", "da ji", "fox demon"], FSYY,
  R("Huli Jing", "Quadruped", ["Fast", "Transforms"], "Psychic", 1.6, 25, accent=(1.0, 0.5, 0.6), temp="Sneaky",
    inten=0.9), None, "a shapeshifting fox spirit")
E(["stone lions", "stone lion", "guardian lions", "guardian lion", "chinese guardian lions", "imperial guardian lions",
   "foo dogs", "foo dog", "fu dogs", "fu dog", "lion statues", "temple lions", "shishi lions"], HIST,
  R("Guardian Lions", "Quadruped", ["Heavy", "Stomp"], "Earth", 1.6, 800, count=2, accent=STONE, temp="Protective",
    inten=0.9), None, "a pair of stone lions that guard you")

# ---------------------------------------------------------------- Three Kingdoms
E(["guan yu's blade", "guan yu blade", "green dragon crescent blade", "green dragon crescent moon blade", "guandao",
   "guan dao", "kwan dao", "qinglong yanyue dao", "guan yu's glaive", "guan yu's guandao", "green dragon blade",
   "blade of guan yu", "guan yu's crescent blade"], ROTK,
  R("Green Dragon Crescent Blade", "Axe", ["Blade", "Heavy"], "Light", 2.8, 50, accent=GREEN), guandao(),
  "a huge crescent glaive: wide, heavy arcs")
E(["lu bu's halberd", "lü bu's halberd", "lu bu halberd", "sky piercer", "sky-piercer", "sky piercer halberd",
   "fangtian huaji", "fang tian hua ji", "sky piercing halberd", "heaven piercing halberd", "lu bu's ji",
   "square sky painted halberd", "fangtian ji"], ROTK,
  R("Sky Piercer", "Spear", ["Blade", "Pierce"], "Light", 2.9, 12, accent=RED), halberd())
E(["red hare", "red hare horse", "chitu", "chi tu", "lu bu's horse", "guan yu's horse", "red rabbit horse",
   "chitu ma"], ROTK,
  R("Red Hare", "Quadruped", ["Piloted", "Fast", "Charge"], "Fire", 1.9, 500, accent=RED, temp="Aggressive",
    inten=0.6), None, "ride the fastest horse of the Three Kingdoms")
E(["zhuge liang's repeating crossbow", "zhuge liang repeating crossbow", "repeating crossbow", "zhuge nu", "zhugenu",
   "chu-ko-nu", "chu ko nu", "zhuge crossbow", "chinese repeating crossbow", "zhuge liang's crossbow"], ROTK,
  R("Repeating Crossbow", "Bow", ["Shoots", "Fast"], "Light", 1.2, 4, accent=(0.75, 0.5, 0.25)),
  None, "rapid-fire bolts")
E(["straw boats", "straw boats borrowing arrows", "borrowing arrows with straw boats", "straw boat",
   "straw boats to borrow arrows", "borrow arrows", "borrowing arrows", "caochuan jiejian", "straw men boats",
   "zhuge liang's straw boats"], ROTK,
  R("Straw Boats", "Wall", ["Barrier", "Sticky"], "Light", 6.0, 1500, accent=STRAW), straw_boat(),
  "a wall of straw that catches arrows (they stick)")
E(["terracotta army", "terracotta warriors", "terracotta soldiers", "terracotta warrior", "the terracotta army",
   "qin terracotta army", "clay army", "clay soldiers", "bingmayong", "terracotta army of qin shi huang"], HIST,
  R("Terracotta Army", "Biped", ["Pierce", "Swarm", "Heavy"], "Earth", 1.8, 150, count=10, accent=CLAY,
    temp="Protective", inten=0.6), None, "a marching rank of clay soldiers")
E(["great wall", "the great wall", "great wall of china", "the great wall of china", "wanli changcheng",
   "changcheng", "ten thousand li wall", "summon the great wall"], HIST,
  R("Great Wall", "Wall", ["Barrier", "Heavy"], "Earth", 30.0, 50000, accent=STONE), great_wall(),
  "a very long rampart with watchtowers")

# ---------------------------------------------------------------- festivals
E(["firecrackers", "firecracker", "chinese firecrackers", "string of firecrackers", "bianpao", "bian pao",
   "firecracker string", "lunar new year firecrackers", "new year firecrackers", "red firecrackers"], FEST,
  R("Firecrackers", "Bomb", ["Explodes", "Swarm", "Bounce"], "Fire", 0.2, 0.3, count=16, accent=RED), firecracker(),
  "a string of little bangs that hop and pop")
E(["dragon dance", "chinese dragon dance", "wu long", "wulong", "dragon dancers", "new year dragon",
   "dragon dance troupe", "dragon dance performers"], FEST,
  R("Dragon Dance", "Serpent", ["Fast", "Bounce"], "Light", 9.0, 600, accent=GOLD, temp="Playful", inten=1.0),
  dragon(9, dance=True), "a long cloth dragon on many dancing legs")
E(["sky lanterns", "sky lantern", "kongming lanterns", "kongming lantern", "kong ming lantern", "wish lanterns",
   "wish lantern", "floating lanterns", "flying lanterns", "chinese lanterns", "chinese lantern", "paper lanterns",
   "lantern festival"], FEST,
  R("Sky Lanterns", "Blob", ["Flies", "SlowFall", "Swarm"], "Fire", 0.8, 0.3, count=8, accent=ORANGE, temp="Calm",
    inten=0.3), sky_lantern(), "a gentle drift of paper lanterns")

# ============================================================================================ emit
OTHER, EARLIER = set(), set()
order = [ROOT / "assets/icons.ron"] + sorted((ROOT / "assets/icons").glob("*.ron"))
before = True
for p in order:
    if p.name == PACK:
        before = False
        continue
    src = p.read_text(encoding="utf-8")
    for m in re.finditer(r"names:\s*\[(.*?)\]", src, re.S):
        keys = {norm(x) for x in re.findall(r'"([^"]*)"', m.group(1))}
        OTHER.update(keys)
        if before:
            EARLIER.update(keys)
OTHER -= {norm(k) for k in TAKE}
kept, seen = [], set()
for names, source, r, shape, note in ENTRIES:
    nn = [n for n in names if norm(n) and norm(n) not in OTHER and norm(n) not in seen]
    if nn != names:
        print("  dropped taken names", [n for n in names if n not in nn], "from", names[0])
    if not nn:
        print("  DROPPED entry", names[0])
        continue
    seen.update(norm(n) for n in nn)
    kept.append((nn, source, r, shape, note))
ENTRIES[:] = kept
for names, source, r, shape, note in ENTRIES:
    if isinstance(shape, S):
        check(shape, names[0])

out, _ = emit()
# two-mode entry: splice the robot-mode (monkey) alt_shape after the temple shape
alt = monkey(staff=False)
head = '        names: ["72 transformations"'
if head in out:
    i = out.index(head)
    k = out.index("        source:", i)
    out = out[:k] + "        alt_shape: Some((\n" + alt.ron(12) + "\n        )),\n" + out[k:]
hdr = ("// Brightest Day icon pack: Sun Wukong / Journey to the West, Fengshen Yanyi, Chinese mythology & beasts,\n"
       "// Three Kingdoms, festivals. GENERATED by tools/icons/folklore_cn/gen.py - edit that, then re-run it.\n"
       "// Schema: docs/ICONS_SCHEMA.md. Public-domain sources only; every shape is our own.\n")
out = hdr + out.split("\n", 3)[3]
dst = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "assets/icons" / PACK
dst.write_text(out, encoding="utf-8")
print(len(ENTRIES), "entries,", sum(len(e[0]) for e in ENTRIES), "names ->", dst)
