"""Generates assets/icons/tabletop.ron: tabletop fantasy (D&D) spells, monsters, magic items, class moves and
table talk, mapped onto the closed bd_core mechanics. Usage: python tools/icons/tabletop/gen.py [out.ron]

Content rules (THIRD_PARTY_NOTICES.md): spell / monster / item NAMES from the SRD 5.1 (CC-BY-4.0) are used
freely; a few iconic non-SRD names (beholder, mind flayer, Bigby's / Tasha's / Mordenkainen's / Otiluke's /
Otto's / Leomund's / Tenser's / Evard's / Melf's prefixes) appear only as trigger words (SRC_NONSRD) and are
private-use-only until replaced before a public release. No rules text is reproduced anywhere: every
entry is our own mapping onto existing mechanics.

Names another pack already owns are dropped (so a regeneration stays warning-free), except TAKE: names this
pack deliberately owns (removed from the earlier pack by hand, see the note there)."""
import glob
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools/icons/internet"))
sys.path.insert(0, str(ROOT / "tools/lexicon_build"))
import lib  # noqa: E402
from lib import E, R, ENTRIES, S, emit, check  # noqa: E402
from dedupe_icon_names import norm  # noqa: E402

lib.FORMS.add("Hands")
PACK = "tabletop.ron"
SRC = "D&D 5e SRD 5.1 (CC-BY-4.0)"
SRC_NONSRD = "Dungeons & Dragons (name only; private use)"
FAN = "Tabletop table talk"

RED = (0.9, 0.15, 0.1)
BLU = (0.2, 0.45, 1.0)
GRN = (0.25, 0.75, 0.2)
BLK = (0.12, 0.12, 0.16)
WHT = (0.92, 0.95, 1.0)
GLD = (1.0, 0.8, 0.25)
SLV = (0.8, 0.82, 0.88)
BRZ = (0.75, 0.5, 0.2)
BRS = (0.85, 0.7, 0.3)
CPR = (0.8, 0.45, 0.3)
PUR = (0.6, 0.3, 0.9)
TEAL = (0.2, 0.8, 0.75)

TAKE = {"nat 20", "natural 20"}  # were aliases of internet.ron's bouncy d20; now the crit meteor below


# ------------------------------------------------------------------------------------------ shapes
def dragon_shape():
    """The base dragon's hand-authored shape (assets/icons.ron "dragon"), reused for the coloured dragons."""
    src = open(ROOT / "assets/icons.ron", encoding="utf-8").read()
    i = src.index('names: ["dragon", ')
    j = src.index("shape: Some((", i)
    k = src.index("source:", j)
    block = src[j:k].rstrip()
    assert block.endswith("),"), block[-40:]
    return block


class Raw:
    """A shape given as ready RON text (copied from another icon)."""

    def __init__(self, text):
        self.text = text
        self.prims = [0]  # emit() only checks the count


def _ico_normals():
    p = (1 + 5 ** 0.5) / 2
    vs = []
    for sx in (-1, 1):
        for sy in (-1, 1):
            for sz in (-1, 1):
                vs.append((sx, sy, sz))
    for a in (-1, 1):
        for b in (-1, 1):
            vs.append((0, a / p, b * p))
            vs.append((a / p, b * p, 0))
            vs.append((b * p, 0, a / p))
    out = []
    for v in vs:
        n = sum(x * x for x in v) ** 0.5
        out.append(tuple(x / n for x in v))
    return out


def d20(diam):
    """A twenty-sided die: a sphere with the 20 icosahedron facets sliced off by subtracting boxes."""
    import math
    s = S()
    r = diam / 2
    cy = r
    s.ell((0, cy, 0), (diam, diam, diam), "P")
    inr = r * 0.80
    big = diam * 2
    for n in _ico_normals():
        c = math.asin(max(-1.0, min(1.0, -n[0])))
        cc = math.cos(c)
        a = math.atan2(n[2] / cc, n[1] / cc)
        ctr = (n[0] * (inr + big / 2), cy + n[1] * (inr + big / 2), n[2] * (inr + big / 2))
        s.box(ctr, (big, big, big), "P", rot=(math.degrees(a), 0, math.degrees(c)), sub=True)
    # the "20" face glows: a thin plate on the facet facing forward and a little up
    n = (0.0, 0.357, -0.934)
    a = math.degrees(math.atan2(n[2], n[1]))
    s.box((0, cy + n[1] * inr, n[2] * inr), (diam * 0.32, max(1.0, diam * 0.06), diam * 0.32), "C", rot=(a, 0, 0))
    return s


# ------------------------------------------------------------------------------------------ table talk
E(["nat 20", "natural 20", "natural twenty", "nat twenty", "nat20", "rolled a nat 20", "rolled a natural 20",
   "critical hit", "crit hit", "critical strike"], FAN,
  R("Natural 20", "Boulder", ["DropFromSky", "Explodes", "Heavy"], "Light", 3.0, 600, accent=RED,
    temp="Aggressive", inten=1.0), d20(12), "the crit: a giant glowing d20 falls from the sky and detonates")
E(["nat 1", "natural 1", "natural one", "nat one", "nat1", "rolled a nat 1", "rolled a natural 1",
   "critical fail", "critical failure", "crit fail", "critical miss"], FAN,
  R("Natural 1", "Ball", ["Bounce"], "Light", 0.4, 0.5, accent=(0.6, 0.6, 0.65), temp="Lazy",
    inten=0.9), None, "the fumble: a tiny die that plops out and just... bounces")
E(["roll initiative", "roll for initiative", "roll init"], FAN,
  R("Initiative", "Boulder", ["Charge", "Bounce", "Heavy"], "Psychic", 2.0, 300, accent=RED, temp="Aggressive",
    inten=0.8), d20(8), "a d20 the size of a car comes rolling in")
E(["rocks fall everyone dies", "rocks fall", "rocks fall, everyone dies"], FAN,
  R("Rocks Fall", "Boulder", ["DropFromSky", "Heavy", "Swarm"], "Earth", 1.5, 400, count=12),
  None, "the DM's last word")
E(["total party kill", "tpk"], FAN,
  R("Total Party Kill", "Bomb", ["DropFromSky", "Explodes", "Heavy"], "Shadow", 4.0, 900, temp="Aggressive", inten=1.0))
E(["i attack the darkness", "attack the darkness"], FAN,
  R("I Attack the Darkness", "Blob", ["Charge", "Flies"], "Shadow", 2.0, 60, temp="Aggressive", inten=1.0))
E(["dungeon master", "the dungeon master", "dm screen"], FAN,
  R("Dungeon Master's Screen", "Wall", ["Barrier"], "Psychic", 3.0, 200, accent=PUR))
E(["leeroy jenkins", "leeroy", "leroy jenkins"], "World of Warcraft meme",
  R("Leeroy Jenkins", "Biped", ["Charge", "Fast"], "Fire", 1.9, 90, temp="Chaotic", inten=1.0))
E(["bardic inspiration", "inspiration die", "inspiration dice"], SRC,
  R("Bardic Inspiration", "Blob", ["Flies", "Homing"], "Light", 0.6, 1, accent=GLD, temp="Protective", inten=0.8))

# ------------------------------------------------------------------------------------------ class moves
E(["sneak attack", "sneak attacks"], SRC,
  R("Sneak Attack", "Sword", ["Pierce", "Fast", "Blade"], "Shadow", 0.5, 1, temp="Sneaky", inten=0.9))
E(["action surge"], SRC,
  R("Action Surge", "Blob", ["Fast", "Charge", "Flies"], "Electric", 1.0, 30, temp="Aggressive", inten=1.0))
E(["divine smite", "smite evil", "paladin smite", "holy smite"], SRC,
  R("Divine Smite", "Pillar", ["DropFromSky", "Explodes", "Beam"], "Light", 6.0, 500, accent=GLD))
E(["eldritch smite"], SRC,
  R("Eldritch Smite", "Pillar", ["DropFromSky", "Explodes"], "Shadow", 5.0, 400, accent=PUR))
E(["flurry of blows"], SRC,
  R("Flurry of Blows", "Gauntlet", ["Blunt", "Fast", "Swarm"], "Wind", 0.5, 5, count=4))
E(["stunning strike"], SRC,
  R("Stunning Strike", "Gauntlet", ["Blunt", "Trap"], "Psychic", 0.6, 5))
E(["lay on hands"], SRC,
  R("Lay on Hands", "Gauntlet", ["Guard", "Barrier"], "Light", 0.8, 5, accent=GLD, temp="Protective", inten=0.9))
E(["turn undead"], SRC,
  R("Turn Undead", "Dome", ["Repel"], "Light", 5.0, 50, accent=GLD))
E(["channel divinity"], SRC,
  R("Channel Divinity", "Pillar", ["Beam", "DropFromSky"], "Light", 5.0, 300, accent=GLD))
E(["cunning action"], SRC,
  R("Cunning Action", "Cape", ["Glide", "Fast"], "Shadow", 1.4, 3, temp="Sneaky", inten=0.8))
E(["reckless attack"], SRC,
  R("Reckless Attack", "Axe", ["Blade", "Charge", "Heavy"], "Fire", 1.3, 12, temp="Aggressive", inten=1.0))
E(["wild magic", "wild magic surge", "wild surge"], SRC,
  R("Wild Magic Surge", "Blob", ["Explodes", "Bounce", "Swarm"], "Psychic", 0.8, 5, count=5, accent=PUR,
    temp="Chaotic", inten=1.0))

# ------------------------------------------------------------------------------------------ spells
SP = SRC
E(["counterspell", "counter spell"], SP,
  R("Counterspell", "Shield", ["Guard", "Barrier", "Ricochet"], "Psychic", 1.6, 5, accent=PUR))
E(["misty step"], SP, R("Misty Step", "Platform", ["Portal"], "Water", 2.0, 5, accent=SLV))
E(["dimension door"], SP, R("Dimension Door", "Platform", ["Portal"], "Psychic", 2.4, 20, accent=PUR))
E(["teleportation circle", "teleport circle"], SP, R("Teleportation Circle", "Platform", ["Portal"], "Psychic", 3.0, 20, accent=PUR))
E(["hold person", "hold monster"], SP, R("Hold Person", "Cage", ["Trap"], "Psychic", 2.2, 30, accent=PUR))
E(["grease spell"], SP, R("Grease", "Platform", ["Sticky"], "Earth", 3.0, 20))
E(["fog cloud"], SP, R("Fog Cloud", "Dome", ["Barrier"], "Water", 5.0, 5, accent=SLV))
E(["invisibility", "greater invisibility", "invisibility spell", "cloak of invisibility"], SP,
  R("Invisibility", "Cape", ["Glide"], "Shadow", 1.4, 2, temp="Sneaky", inten=1.0))
E(["mirror image"], SP, R("Mirror Image", "Biped", ["Swarm", "Walks"], "Light", 1.8, 40, count=3, temp="Protective", inten=0.8))
E(["polymorph", "true polymorph", "polymorph ray"], SP, R("Polymorph", "Staff", ["Beam", "Shrink"], "Psychic", 1.4, 3, accent=PUR))
E(["enlarge reduce", "enlarge/reduce"], SP, R("Enlarge/Reduce", "Staff", ["Beam", "Grow"], "Psychic", 1.4, 3))
E(["spiritual weapon"], SP, R("Spiritual Weapon", "Hammer", ["Blunt", "Flies", "Returning"], "Light", 1.2, 10, accent=GLD))
E(["spirit guardians"], SP, R("Spirit Guardians", "Blob", ["Flies", "Swarm", "Homing"], "Light", 1.0, 5, count=4, accent=GLD,
    temp="Protective", inten=0.9))
E(["guiding bolt"], SP, R("Guiding Bolt", "Spear", ["Pierce", "Homing", "Fast"], "Light", 1.6, 3, accent=GLD))
E(["healing word", "mass healing word"], SP, R("Healing Word", "Blob", ["Flies", "Homing"], "Light", 0.6, 1, accent=GRN,
    temp="Protective", inten=0.9))
E(["cure wounds", "mass cure wounds", "heal spell"], SP, R("Cure Wounds", "Blob", ["Flies", "Guard"], "Light", 0.8, 1, accent=GRN,
    temp="Protective", inten=0.9))
E(["revivify", "raise dead", "resurrection", "true resurrection"], SP,
  R("Revivify", "Biped", ["Flies", "Guard"], "Light", 1.8, 50, accent=GLD, temp="Protective", inten=1.0))
E(["shield spell", "shield of faith"], SP, R("Shield of Faith", "Shield", ["Guard", "Barrier"], "Light", 1.2, 5, accent=GLD))
E(["shatter spell"], SP, R("Shatter", "Bomb", ["Explodes", "Repel"], "Electric", 1.2, 10))
E(["storm of vengeance"], SP, R("Storm of Vengeance", "Spear", ["DropFromSky", "Pierce", "Swarm"], "Electric", 2.2, 20, count=10))
E(["power word kill"], SP, R("Power Word Kill", "Hands", ["Beam", "Heavy"], "Shadow", 1.2, 5, temp="Aggressive", inten=1.0))
E(["power word stun"], SP, R("Power Word Stun", "Hands", ["Beam", "Trap"], "Psychic", 1.2, 5))
E(["time stop", "stop time"], SP, R("Time Stop", "Dome", ["Trap", "Barrier"], "Psychic", 6.0, 50, accent=PUR))
E(["otto's irresistible dance", "irresistible dance", "ottos irresistible dance"], SRC_NONSRD,
  R("Irresistible Dance", "Cage", ["Trap", "Spin"], "Psychic", 2.2, 20, temp="Playful", inten=1.0))
E(["tasha's hideous laughter", "hideous laughter", "tashas hideous laughter"], SRC_NONSRD,
  R("Hideous Laughter", "Cage", ["Trap"], "Psychic", 2.2, 20, temp="Playful", inten=1.0))
E(["bigby's hand", "bigbys hand", "arcane hand", "bigby's grasping hand", "bigby's crushing hand"], SRC_NONSRD,
  R("Arcane Hand", "Gauntlet", ["Grab", "Blunt", "Flies"], "Psychic", 3.0, 200, accent=PUR))
E(["otiluke's resilient sphere", "resilient sphere"], SRC_NONSRD,
  R("Resilient Sphere", "Dome", ["Trap", "Barrier"], "Psychic", 2.5, 30, accent=PUR))
E(["evard's black tentacles", "evards black tentacles"], SRC_NONSRD,
  R("Black Tentacles", "Whip", ["Constrict", "Grab", "Swarm"], "Shadow", 3.0, 30, count=4))
E(["melf's acid arrow", "acid arrow"], SRC_NONSRD, R("Acid Arrow", "Spear", ["Pierce", "Sticky"], "Poison", 1.0, 2))
E(["disintegrate", "disintegration ray"], SP, R("Disintegrate", "Hands", ["Beam", "Pierce"], "Atomic", 1.2, 5))
E(["finger of death"], SP, R("Finger of Death", "Hands", ["Beam"], "Shadow", 1.2, 5))
E(["prismatic spray"], SP, R("Prismatic Spray", "Hands", ["Beam", "Ricochet"], "Psychic", 1.2, 5, accent=PUR))
E(["sunbeam", "sun beam"], SP, R("Sunbeam", "Hands", ["Beam"], "Light", 1.2, 5, accent=GLD))
E(["moonbeam", "moon beam"], SP, R("Moonbeam", "Pillar", ["Beam", "DropFromSky"], "Light", 5.0, 20, accent=SLV))
E(["sacred flame"], SP, R("Sacred Flame", "Pillar", ["DropFromSky", "Beam"], "Light", 4.0, 20, accent=GLD))
E(["flaming sphere"], SP, R("Flaming Sphere", "Ball", ["Spin", "Homing"], "Fire", 1.5, 20))
E(["call lightning"], SP, R("Call Lightning", "Spear", ["DropFromSky", "Pierce"], "Electric", 3.0, 10))
E(["chain lightning"], SP, R("Chain Lightning", "Spear", ["Pierce", "Ricochet", "Fast"], "Electric", 2.2, 5))
E(["fire bolt", "firebolt"], SP, R("Fire Bolt", "Ball", ["Fast", "Explodes"], "Fire", 0.5, 2))
E(["acid splash"], SP, R("Acid Splash", "Bomb", ["Explodes", "Sticky"], "Poison", 0.5, 2))
E(["poison spray"], SP, R("Poison Spray", "Hands", ["Breath"], "Poison", 1.0, 2))
E(["shocking grasp"], SP, R("Shocking Grasp", "Gauntlet", ["Grab"], "Electric", 0.6, 3))
E(["chill touch"], SP, R("Chill Touch", "Gauntlet", ["Grab", "Flies"], "Shadow", 0.8, 3))
E(["witch bolt"], SP, R("Witch Bolt", "Whip", ["Link", "Beam"], "Electric", 3.0, 5))
E(["slow spell"], SP, R("Slow", "Dome", ["Trap", "Sticky"], "Psychic", 4.0, 30))
E(["haste spell"], SP, R("Haste", "Blob", ["Fast", "Flies"], "Wind", 0.8, 5, temp="Playful", inten=0.8))
E(["sleep spell"], SP, R("Sleep", "Dome", ["Trap"], "Psychic", 3.5, 20, temp="Lazy", inten=1.0))
E(["insect plague", "creeping doom"], SP, R("Insect Plague", "Insect", ["Swarm", "Flies"], "Poison", 0.3, 0.1, count=12,
    temp="Aggressive", inten=0.9))
E(["entangle", "entangling vines"], SP, R("Entangle", "Cage", ["Constrict", "Trap"], "Earth", 2.2, 30, accent=GRN))
E(["animate dead"], SP, R("Animate Dead", "Biped", ["Walks", "Swarm"], "Shadow", 1.7, 40, count=3, temp="Aggressive", inten=0.7))
E(["find familiar"], SP, R("Familiar", "Bird", ["Flies", "Homing"], "Psychic", 0.5, 2, temp="Protective", inten=0.8))
E(["feather fall"], SP, R("Feather Fall", "Parachute", ["SlowFall", "Glide"], "Wind", 2.0, 2))
E(["gust of wind"], SP, R("Gust of Wind", "Blob", ["Repel", "Fast"], "Wind", 1.5, 5))
E(["cloudkill", "cloud kill"], SP, R("Cloudkill", "Blob", ["Homing", "Flies", "Swarm"], "Poison", 2.0, 5, count=3,
    temp="Lazy", inten=0.6))
E(["hunger of hadar"], SP, R("Hunger of Hadar", "Dome", ["Trap", "Attract"], "Shadow", 5.0, 50))
E(["banishment", "banish"], SP, R("Banishment", "Platform", ["Portal", "Trap"], "Psychic", 2.4, 20, accent=PUR))
E(["antimagic field", "anti magic field", "antimagic zone"], SP, R("Antimagic Field", "Dome", ["Barrier", "Repel"], "Light", 5.0, 50))
E(["globe of invulnerability"], SP, R("Globe of Invulnerability", "Dome", ["Barrier"], "Light", 3.5, 50, accent=GLD))
E(["dispel magic"], SP, R("Dispel Magic", "Dome", ["Repel"], "Light", 4.0, 20))
E(["blade barrier"], SP, R("Blade Barrier", "Wall", ["Blade", "Spin", "Barrier"], "Light", 4.0, 100))
E(["wall of thorns"], SP, R("Wall of Thorns", "Wall", ["Pierce", "Barrier"], "Earth", 4.0, 200, feat=["Spikes"]))
E(["spike growth"], SP, R("Spike Growth", "Platform", ["Pierce"], "Earth", 4.0, 100, feat=["Spikes"]))
E(["hunter's mark", "hunters mark"], SP, R("Hunter's Mark", "Blob", ["Homing", "Flies", "Sticky"], "Light", 0.5, 1, accent=GRN))
E(["hex", "hex curse"], SP, R("Hex", "Blob", ["Homing", "Flies", "Sticky"], "Shadow", 0.5, 1, accent=PUR))
E(["bless", "blessing"], SP, R("Bless", "Blob", ["Flies", "Guard"], "Light", 0.8, 1, accent=GLD, temp="Protective", inten=0.8))
E(["vicious mockery"], SP, R("Vicious Mockery", "Ball", ["Fast", "Homing"], "Psychic", 0.6, 1, temp="Playful", inten=1.0))
E(["toll the dead"], SP, R("Toll the Dead", "Anvil", ["DropFromSky", "Blunt"], "Shadow", 2.0, 300, feat=["Bell"]))
E(["phantasmal killer"], SP, R("Phantasmal Killer", "Biped", ["Homing", "Walks"], "Psychic", 2.2, 50, temp="Sneaky", inten=1.0))
E(["sunburst"], SP, R("Sunburst", "Bomb", ["Explodes", "Repel"], "Light", 3.0, 50, accent=GLD))
E(["thunderwave"], SP, R("Thunderwave", "Dome", ["Repel", "Explodes"], "Wind", 4.0, 20))
E(["eldritch blast barrage", "eldritch blasts"], SP,
  R("Eldritch Barrage", "Hands", ["Beam", "Repel", "Swarm"], "Shadow", 1.2, 5, count=3, accent=PUR))
E(["mage armour"], SP, R("Mage Armor", "Shield", ["Guard"], "Psychic", 1.0, 3))
E(["conjure animals", "summon beast"], SP, R("Conjured Beasts", "Quadruped", ["Charge", "Swarm"], "Light", 1.4, 60, count=3,
    temp="Aggressive", inten=0.7))
E(["conjure elemental", "summon elemental"], SP, R("Conjured Elemental", "Biped", ["Stomp", "Heavy"], "Earth", 4.0, 800,
    temp="Aggressive", inten=0.8))
E(["wall of force", "force wall"], SP, R("Wall of Force", "Wall", ["Barrier", "Ricochet"], "Psychic", 4.5, 500, accent=PUR))
E(["gate spell", "planar gate"], SP, R("Gate", "Wall", ["Portal"], "Psychic", 4.0, 200, accent=PUR))
E(["wish", "i wish", "wish spell"], SP,
  R("Wish", "Boulder", ["DropFromSky", "Explodes", "Heavy"], "Light", 3.0, 500, accent=GLD),
  None, "the one spell that does anything: a falling star")
E(["meteor swarm"], SP, R("Meteor Swarm", "Boulder", ["DropFromSky", "Explodes", "Heavy", "Swarm"], "Fire", 1.8, 300, count=6))
E(["fireball spell", "fire ball spell"], SP, R("Fireball", "Ball", ["Explodes", "Fast"], "Fire", 0.8, 5))
E(["delayed blast fireball"], SP, R("Delayed Blast Fireball", "Bomb", ["Explodes", "Heavy"], "Fire", 1.2, 10))
E(["cone of cold"], SP, R("Cone of Cold", "Hands", ["Breath"], "Ice", 1.2, 5))
E(["burning hands"], SP, R("Burning Hands", "Hands", ["Breath"], "Fire", 1.0, 5))
E(["ray of frost"], SP, R("Ray of Frost", "Hands", ["Beam"], "Ice", 1.0, 5))
E(["scorching ray"], SP, R("Scorching Ray", "Hands", ["Beam", "Swarm"], "Fire", 1.0, 5, count=3))

# ------------------------------------------------------------------------------------------ monsters
DRAGONS = [
    ("red", "Fire", RED), ("blue", "Electric", BLU), ("green", "Poison", GRN), ("black", "Poison", BLK),
    ("white", "Ice", WHT), ("gold", "Fire", GLD), ("silver", "Ice", SLV), ("bronze", "Electric", BRZ),
    ("brass", "Fire", BRS), ("copper", "Poison", CPR),
]
DSHAPE = Raw(dragon_shape())
for col, el, acc in DRAGONS:
    E([f"{col} dragon", f"{col} dragons", f"chromatic {col} dragon" if col in ("red", "blue", "green", "black", "white")
       else f"metallic {col} dragon"], SRC,
      R(f"{col.title()} Dragon", "Quadruped", ["Flies", "Breath", "ArticulatedTail"], el, 8.0, 6000, accent=acc,
        temp="Aggressive" if col in ("red", "blue", "green", "black", "white") else "Protective", inten=0.7), DSHAPE)
    E([f"ancient {col} dragon", f"great wyrm {col}", f"adult {col} dragon"], SRC,
      R(f"Ancient {col.title()} Dragon", "Kaiju", ["Flies", "Breath", "ArticulatedTail", "Stomp", "Heavy"], el, 16.0,
        40000, accent=acc, temp="Aggressive", inten=0.9), DSHAPE)
E(["treant", "treants"], SRC, R("Treant", "Biped", ["Stomp", "Heavy", "Grab"], "Earth", 6.0, 3000, accent=GRN, temp="Protective", inten=0.6))
E(["purple worm"], SRC, R("Purple Worm", "Serpent", ["Trap", "Grab", "Charge", "Heavy"], "Poison", 12.0, 20000, accent=PUR,
    temp="Aggressive", inten=0.8))
E(["aboleth"], SRC, R("Aboleth", "Fish", ["Constrict", "Grab"], "Psychic", 6.0, 3000, accent=TEAL))
E(["otyugh"], SRC, R("Otyugh", "Blob", ["Grab", "Constrict"], "Poison", 2.5, 400))
E(["ankheg"], SRC, R("Ankheg", "Insect", ["Grab", "Breath"], "Poison", 3.0, 800))
E(["remorhaz"], SRC, R("Remorhaz", "Serpent", ["ArticulatedTail", "Charge"], "Fire", 8.0, 5000))
E(["myconid", "myconids", "mushroom folk"], SRC, R("Myconid", "Biped", ["Swarm", "Walks"], "Poison", 1.2, 20, count=4))
E(["ochre jelly"], SRC, R("Ochre Jelly", "Blob", ["Sticky", "Swarm"], "Poison", 1.2, 80, count=2))
E(["black pudding"], SRC, R("Black Pudding", "Blob", ["Sticky", "Trap"], "Poison", 1.6, 200, accent=BLK))
E(["death knight"], SRC, R("Death Knight", "Biped", ["Blade", "Guard", "Charge"], "Shadow", 2.1, 200, temp="Aggressive", inten=0.9))
E(["hook horror"], SRC_NONSRD, R("Hook Horror", "Biped", ["Grab", "Blade"], "Earth", 2.7, 400))
E(["mind flayer army", "illithid army"], SRC_NONSRD, R("Mind Flayers", "Biped", ["Grab", "Swarm"], "Psychic", 1.9, 80, count=3))
E(["beholder swarm", "beholder army"], SRC_NONSRD, R("Beholders", "Blob", ["Beam", "Flies", "Swarm"], "Psychic", 1.5, 100, count=3))

# ------------------------------------------------------------------------------------------ magic items
E(["flame tongue", "flametongue", "flame tongue sword"], SRC, R("Flame Tongue", "Sword", ["Blade"], "Fire", 1.1, 3, accent=RED))
E(["holy avenger"], SRC, R("Holy Avenger", "Sword", ["Blade", "Guard"], "Light", 1.2, 4, accent=GLD))
E(["sun blade", "sunblade"], SRC, R("Sun Blade", "Sword", ["Blade"], "Light", 1.1, 2, accent=GLD))
E(["sword of sharpness"], SRC, R("Sword of Sharpness", "Sword", ["Blade", "Pierce"], "Light", 1.1, 3))
E(["nine lives stealer"], SRC, R("Nine Lives Stealer", "Sword", ["Blade"], "Shadow", 1.1, 3))
E(["scimitar of speed"], SRC, R("Scimitar of Speed", "Sword", ["Blade", "Fast"], "Wind", 1.0, 2))
E(["dwarven thrower"], SRC, R("Dwarven Thrower", "Hammer", ["Blunt", "Returning"], "Earth", 0.9, 8))
E(["hammer of thunderbolts"], SRC, R("Hammer of Thunderbolts", "Hammer", ["Blunt", "Returning", "Heavy"], "Electric", 1.2, 15))
E(["oathbow"], SRC, R("Oathbow", "Bow", ["Shoots", "Homing"], "Light", 1.5, 2))
E(["trident of fish command"], SRC, R("Trident of Fish Command", "Spear", ["Pierce"], "Water", 2.0, 4))
E(["javelin of lightning"], SRC, R("Javelin of Lightning", "Spear", ["Pierce", "Fast"], "Electric", 2.0, 2))
E(["wand of wonder"], SRC, R("Wand of Wonder", "Staff", ["Shoots"], "Psychic", 0.5, 1, temp="Chaotic", inten=1.0))
E(["wand of fireballs"], SRC, R("Wand of Fireballs", "Staff", ["Shoots"], "Fire", 0.5, 1,
    payload=R("fireball", "Ball", ["Explodes"], "Fire", 0.6, 3)))
E(["staff of power"], SRC, R("Staff of Power", "Staff", ["Blunt", "Beam"], "Psychic", 1.8, 4, accent=PUR))
E(["staff of the magi", "staff of magi"], SRC, R("Staff of the Magi", "Staff", ["Blunt", "Beam", "Explodes"], "Psychic", 1.9, 4, accent=GLD))
E(["gauntlets of ogre power"], SRC, R("Gauntlets of Ogre Power", "Gauntlet", ["Blunt", "Heavy"], "Earth", 0.6, 10))
E(["horn of blasting"], SRC, R("Horn of Blasting", "Cannon", ["Repel", "Explodes"], "Wind", 1.0, 5))
E(["cube of force"], SRC, R("Cube of Force", "Dome", ["Barrier"], "Light", 3.0, 50))
E(["ioun stone", "ioun stones"], SRC, R("Ioun Stone", "Ball", ["Flies", "Returning"], "Psychic", 0.3, 0.5, temp="Protective", inten=0.7))
E(["bag of tricks"], SRC, R("Bag of Tricks", "Quadruped", ["Swarm", "Charge"], "Light", 1.0, 40, count=3, temp="Chaotic", inten=0.8))
E(["carpet of flying", "flying carpet", "magic carpet"], SRC, R("Carpet of Flying", "Platform", ["Flies", "Piloted", "Glide"], "Wind", 2.5, 10))
E(["cloak of elvenkind"], SRC, R("Cloak of Elvenkind", "Cape", ["Glide"], "Shadow", 1.4, 2, temp="Sneaky", inten=0.8))
E(["ring of invisibility"], SRC, R("Ring of Invisibility", "Cape", ["Glide"], "Shadow", 1.4, 2, temp="Sneaky", inten=1.0))
E(["apparatus of kwalish", "kwalish apparatus", "apparatus of the crab"], SRC,
  R("Apparatus of Kwalish", "Vehicle", ["Grab", "Piloted", "Walks"], "Water", 3.0, 800, feat=["Pistons", "Cockpit"]))
E(["decanter of endless water"], SRC, R("Decanter of Endless Water", "Gun", ["Shoots", "Beam", "Repel"], "Water", 0.5, 2))
E(["boots of speed"], SRC, R("Boots of Speed", "Jetpack", ["Fast", "Thrust"], "Wind", 0.8, 2))

# ------------------------------------------------------------------------------------------ emit
OTHER = set()
for p in glob.glob(str(ROOT / "assets/icons.ron")) + glob.glob(str(ROOT / "assets/icons/*.ron")):
    if p.replace("\\", "/").endswith("/" + PACK):
        continue
    src = open(p, encoding="utf-8").read()
    for m in re.finditer(r"names:\s*\[(.*?)\]", src, re.S):
        OTHER.update(norm(x) for x in re.findall(r'"([^"]*)"', m.group(1)))
OTHER -= {norm(k) for k in TAKE}
kept, seen = [], set()
for names, source, r, shape, note in ENTRIES:
    nn = [n for n in names if norm(n) and norm(n) not in OTHER and norm(n) not in seen]
    if nn != names:
        print("  dropped taken names", [n for n in names if n not in nn], "from", names[0])
    if not nn:
        continue
    seen.update(norm(n) for n in nn)
    kept.append((nn, source, r, shape, note))
ENTRIES[:] = kept
for names, source, r, shape, note in ENTRIES:
    if isinstance(shape, S):
        check(shape, names[0])

# emit() only knows S shapes: splice Raw shapes in afterwards.
raws = {}
for i, (names, source, r, shape, note) in enumerate(ENTRIES):
    if isinstance(shape, Raw):
        raws[names[0]] = shape.text
        tmp = S()
        tmp.box((0, 0, 0), (1, 1, 1))
        ENTRIES[i] = (names, source, r, tmp, note)
out, _ = emit()
for cid, text in raws.items():
    head = f'        names: ["{cid}"'
    i = out.index(head)
    j = out.index("        shape: Some((", i)
    k = out.index("        source:", j)
    out = out[:j] + "        " + text + "\n" + out[k:]
hdr = ("// Brightest Day icon pack: tabletop fantasy (D&D) spells, monsters, magic items, class moves, table talk.\n"
       "// GENERATED by tools/icons/tabletop/gen.py - edit that, then re-run it. Schema: docs/ICONS_SCHEMA.md.\n"
       "// Names from the SRD 5.1 (CC-BY-4.0); a few non-SRD names as trigger words only (see THIRD_PARTY_NOTICES.md).\n"
       "// No rules text: every recipe is our own mapping onto existing mechanics.\n")
out = hdr + out.split("\n", 3)[3]
dst = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "assets/icons" / PACK
dst.write_text(out, encoding="utf-8")
print(len(ENTRIES), "entries,", sum(len(e[0]) for e in ENTRIES), "names ->", dst)
