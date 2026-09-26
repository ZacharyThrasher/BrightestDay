"""Generate assets/icons/marvel.ron (Marvel + X-Men pack).

    python tools/icons/marvel/gen.py            # writes the RON, prints drops/trims/warnings
    sh tools/icons/marvel/run.sh [ids]          # gen + iconcheck + contact sheet (scratch)

Names/facts only (no quotes). Recipes use the closed bd_core sets. Names already owned by icons.ron or
another pack are trimmed automatically (see mvlib.emit); common-word aliases are additionally shadowed by
the lexicon at load time, so characters named after common words carry a qualified name first.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mvlib import E as _E, R, emit, ROOT  # noqa: E402
import shapes_heroes as H  # noqa: E402
import shapes_items as I  # noqa: E402

# Detail: shapes are authored at real size (0.25 m units) and upscaled for voxel detail (in-game the
# shape is voxelised at 1 unit = 1 voxel, then rescaled to recipe.scale). Target longest extent per
# form family (voxel caps: 6k normal, 40k kaiju; 64-unit / 128-unit extent before coarsening).
TARGET = {"Kaiju": 96.0, "Vehicle": 52.0, "Biped": 36.0, "Blob": 34.0, "Bird": 56.0, "Dome": 52.0,
          "Pillar": 56.0, "Cage": 34.0, "Wings": 44.0}


def E(names, r, src, shape=None, up=None):
    if shape is not None and up is None:
        lo, hi = shape.extent()
        ext = max(hi[i] - lo[i] for i in range(3))
        up = max(1.0, min(6.0, TARGET.get(r.form, 30.0) / ext))
        up = round(up, 2)
    _E(names, r, src, shape, up or 1.0)


MV = "Marvel"
XM = "Marvel (X-Men)"
RED = (0.85, 0.1, 0.12)
CRIMSON = (0.7, 0.08, 0.2)
BLUE = (0.15, 0.3, 0.85)
NAVY = (0.12, 0.18, 0.5)
GOLD = (0.95, 0.75, 0.15)
YELLOW = (1.0, 0.85, 0.1)
GREEN = (0.2, 0.7, 0.25)
DKGREEN = (0.12, 0.42, 0.18)
PURPLE = (0.5, 0.2, 0.7)
MAGENTA = (0.85, 0.2, 0.6)
SILVER = (0.82, 0.84, 0.88)
STEEL = (0.45, 0.5, 0.58)
WHITE = (0.95, 0.95, 0.97)
BLACK = (0.08, 0.08, 0.1)
ORANGE = (1.0, 0.45, 0.08)
FIRE = (1.0, 0.35, 0.05)
ICE = (0.6, 0.85, 1.0)
BROWN = (0.5, 0.3, 0.15)
RUST = (0.55, 0.25, 0.12)
GREY = (0.55, 0.56, 0.6)
TEAL = (0.1, 0.6, 0.6)
PINK = (1.0, 0.35, 0.7)

# ============================================================== Avengers & friends (characters)
E(["captain america", "steve rogers", "the first avenger", "cap america", "star-spangled avenger"],
  R("Captain America", "Biped", ["Charge", "Guard", "Blunt"], scale=None, mass=110, accent=BLUE,
    temp="Protective", inten=0.8), MV + " (Captain America)", H.captain_america())
E(["thor", "thor odinson", "god of thunder", "the mighty thor", "odinson"],
  R("Thor", "Biped", ["Blunt", "Flies", "Stomp"], el="Electric", scale=None, mass=290, accent=RED,
    temp="Aggressive", inten=0.6), MV + " (Thor)", H.thor())
E(["black panther", "t'challa", "king of wakanda", "tchalla", "shuri black panther"],
  R("Black Panther", "Biped", ["Blade", "Charge", "Fast"], scale=None, mass=90, accent=SILVER,
    temp="Sneaky", inten=0.7), MV + " (Black Panther)", H.black_panther())
E(["doctor strange", "dr strange", "dr. strange", "stephen strange", "sorcerer supreme", "strange supreme"],
  R("Doctor Strange", "Biped", ["Flies", "Portal", "Shoots"], el="Psychic", scale=None, mass=80, accent=RED,
    temp="Fearful", inten=0.6), MV + " (Doctor Strange)", H.doctor_strange())
E(["scarlet witch", "wanda maximoff", "wanda", "the scarlet witch"],
  R("Scarlet Witch", "Biped", ["Flies", "Homing", "Shoots"], el="Psychic", scale=None, mass=60, accent=CRIMSON,
    temp="Chaotic", inten=0.6), MV + " (Scarlet Witch)", H.scarlet_witch())
E(["the vision", "vision avengers", "synthezoid", "white vision", "vision marvel"],
  R("Vision", "Biped", ["Beam", "Flies"], el="Psychic", scale=None, mass=150, accent=CRIMSON,
    temp="Calm"), MV + " (Vision)", H.vision())
E(["captain marvel", "carol danvers", "captain marvel carol"],
  R("Captain Marvel", "Biped", ["Beam", "Fast", "Flies"], scale=None, mass=75, accent=RED,
    temp="Aggressive", inten=0.7), MV + " (Captain Marvel)", H.captain_marvel())
E(["star-lord", "star lord", "starlord", "peter quill"],
  R("Star-Lord", "Biped", ["Shoots", "Thrust"], el="Fire", scale=None, mass=80, accent=CRIMSON,
    temp="Playful", inten=0.6), MV + " (Guardians of the Galaxy)", H.star_lord())
E(["rocket raccoon", "rocket racoon", "rocket the raccoon", "89p13"],
  R("Rocket Raccoon", "Biped", ["Explodes", "Shoots"], scale=None, mass=25, accent=ORANGE,
    temp="Aggressive", inten=0.7, payload=R("rocket round", "Rocket", ["Explodes", "Homing"], scale=0.5, mass=4)),
  MV + " (Guardians of the Galaxy)", H.rocket_raccoon())
E(["ultron", "ultron prime", "ultron 5"],
  R("Ultron", "Biped", ["Beam", "Flies"], el="Electric", scale=None, mass=400, accent=SILVER,
    temp="Aggressive", inten=0.8), MV + " (Avengers)", H.ultron())
E(["ultron sentries", "ultron drones", "ultron army", "ultron sentry", "ultron drone"],
  R("Ultron Sentries", "Biped", ["Flies", "Shoots", "Swarm"], scale=None, mass=60, count=6, accent=SILVER,
    temp="Aggressive", inten=0.9), MV + " (Avengers)", H.ultron(0.55))
E(["kang the conqueror", "kang", "he who remains", "immortus"],
  R("Kang the Conqueror", "Biped", ["Portal", "Shoots"], scale=None, mass=90, accent=PURPLE,
    temp="Calm"), MV + " (Kang)", H.kang())
E(["doctor doom", "dr doom", "dr. doom", "victor von doom", "doom latveria", "lord of latveria"],
  R("Doctor Doom", "Biped", ["Beam", "Guard"], el="Electric", scale=None, mass=180, accent=DKGREEN,
    temp="Calm"), MV + " (Fantastic Four)", H.doctor_doom())
E(["red skull", "johann schmidt", "the red skull"],
  R("Red Skull", "Biped", ["Shoots"], el="Shadow", scale=None, mass=85, accent=RED, temp="Sneaky", inten=0.6),
  MV + " (Captain America)", H.red_skull())
E(["moon knight", "marc spector", "fist of khonshu", "mr. knight", "steven grant"],
  R("Moon Knight", "Biped", ["Blunt", "Charge"], scale=None, mass=95, accent=WHITE,
    temp="Sneaky", inten=0.7, payload=R("crescent dart", "Disc", ["Pierce", "Returning"], scale=0.4, mass=0.5)),
  MV + " (Moon Knight)", H.moon_knight())
E(["the mad titan", "mad titan", "thanos the mad titan", "armored thanos"],
  R("Thanos", "Biped", ["Blunt", "Grab", "Heavy", "Stomp"], scale=None, mass=450, accent=GOLD,
    temp="Calm"), MV + " (Thanos)", H.mad_titan())
E(["norrin radd", "sentinel of the spaceways", "surfer norrin radd"],
  R("Silver Surfer", "Biped", ["Beam", "Fast", "Flies"], el="Atomic", scale=None, mass=100, accent=SILVER,
    temp="Calm"), MV + " (Fantastic Four)", H.norrin_radd())
E(["mister fantastic", "mr fantastic", "mr. fantastic", "reed richards"],
  R("Mister Fantastic", "Biped", ["Bounce", "Grab", "Pull"], scale=None, mass=80, accent=BLUE,
    temp="Playful", inten=0.5), MV + " (Fantastic Four)", H.mister_fantastic())
E(["invisible woman", "sue storm", "susan storm", "invisible girl"],
  R("Invisible Woman", "Biped", ["Barrier", "Guard", "Repel"], scale=None, mass=60, accent=BLUE,
    temp="Protective", inten=0.8), MV + " (Fantastic Four)", H.sue_storm())
E(["modok", "m.o.d.o.k.", "m.o.d.o.k", "george tarleton"],
  R("M.O.D.O.K.", "Biped", ["Beam", "Flies"], el="Psychic", scale=None, mass=200, accent=GOLD,
    temp="Fearful", inten=0.7), MV + " (Avengers)", H.modok())
E(["savage hulk", "green goliath", "jade giant", "bruce banner", "grey hulk", "gray hulk", "joe fixit"],
  R("Savage Hulk", "Biped", ["Blunt", "Charge", "Grab", "Heavy", "Stomp"], scale=None, mass=700, accent=PURPLE,
    temp="Aggressive", inten=1.0), MV + " (Hulk)", H.hulk(0.95))
E(["world breaker hulk", "worldbreaker hulk", "world war hulk", "green scar", "immortal hulk"],
  R("World Breaker Hulk", "Kaiju", ["Blunt", "Charge", "Heavy", "Stomp"], el="Atomic", scale=None, mass=20000,
    accent=PURPLE, temp="Aggressive", inten=1.0), MV + " (Hulk)", H.hulk(3.2, cracks=True))
E(["she-hulk", "she hulk", "shehulk", "jennifer walters", "jen walters"],
  R("She-Hulk", "Biped", ["Blunt", "Charge", "Heavy"], scale=None, mass=320, accent=PURPLE,
    temp="Playful", inten=0.6), MV + " (She-Hulk)", H.she_hulk())

# ============================================================== Spider-Verse + rogues
E(["peter parker", "amazing spider-man", "friendly neighborhood spider-man", "spectacular spider-man",
   "the amazing spider-man", "web-slinger", "web slinger", "wall-crawler"],
  R("Spider-Man", "Biped", ["Fast", "Pull", "Sticky"], scale=None, mass=75, accent=RED,
    temp="Playful", inten=0.8, payload=R("web ball", "Ball", ["Sticky", "Trap"], scale=0.5, mass=1)),
  MV + " (Spider-Man)", H.peter_parker())
E(["spider-gwen", "spider gwen", "ghost-spider", "ghost spider", "gwen stacy"],
  R("Spider-Gwen", "Biped", ["Fast", "Pull", "Sticky"], scale=None, mass=60, accent=WHITE,
    temp="Playful", inten=0.7), MV + " (Spider-Verse)", H.spider_gwen())
E(["spider-man 2099", "spiderman 2099", "spider man 2099", "miguel o'hara", "miguel ohara"],
  R("Spider-Man 2099", "Biped", ["Blade", "Fast", "Glide"], scale=None, mass=80, accent=NAVY,
    temp="Aggressive", inten=0.6), MV + " (Spider-Verse)", H.spider_2099())
E(["venom marvel", "eddie brock", "we are venom", "lethal protector", "agent venom", "venom spider-man"],
  R("Venom", "Biped", ["Constrict", "Grab", "Pull"], el="Shadow", scale=None, mass=260, accent=WHITE,
    temp="Aggressive", inten=0.9), MV + " (Venom)", H.venom())
E(["carnage", "cletus kasady", "maximum carnage"],
  R("Carnage", "Biped", ["Blade", "Constrict", "Grab"], el="Shadow", scale=None, mass=200, accent=RED,
    temp="Chaotic", inten=1.0), MV + " (Venom)", H.venom(0.9, carnage=True))
E(["green goblin", "norman osborn", "the green goblin", "hobgoblin", "harry osborn"],
  R("Green Goblin", "Biped", ["Flies", "Shoots"], scale=None, mass=90, accent=PURPLE, temp="Chaotic", inten=0.8,
    payload=R("pumpkin bomb", "Bomb", ["Explodes"], scale=0.4, mass=1)), MV + " (Spider-Man)", H.green_goblin())
E(["doctor octopus", "doc ock", "dr octopus", "otto octavius", "dr. octopus"],
  R("Doctor Octopus", "Biped", ["ArticulatedTail", "Constrict", "Grab"], scale=None, mass=180, accent=DKGREEN,
    temp="Calm"), MV + " (Spider-Man)", H.doctor_octopus())
E(["rhino spider-man", "the rhino marvel", "aleksei sytsevich", "rhino villain"],
  R("Rhino", "Biped", ["Charge", "Heavy", "Pierce"], scale=None, mass=450, accent=GREY,
    temp="Aggressive", inten=0.9), MV + " (Spider-Man)", H.rhino())
E(["electro", "max dillon", "electro spider-man"],
  R("Electro", "Biped", ["Beam", "Shoots"], el="Electric", scale=None, mass=80, accent=YELLOW,
    temp="Aggressive", inten=0.6), MV + " (Spider-Man)", H.electro())
E(["mysterio", "quentin beck"],
  R("Mysterio", "Biped", ["Homing", "Shoots"], el="Psychic", scale=None, mass=80, accent=PURPLE,
    temp="Sneaky", inten=0.8), MV + " (Spider-Man)", H.mysterio())
E(["vulture spider-man", "the vulture marvel", "adrian toomes"],
  R("Vulture", "Biped", ["Flies", "Glide", "Grab"], scale=None, mass=70, accent=DKGREEN,
    temp="Sneaky", inten=0.6), MV + " (Spider-Man)", H.vulture())

# ============================================================== X-Men & mutants
E(["wolverine", "logan", "james howlett", "weapon x", "old man logan", "wolverine x-men", "x-men wolverine", "logan wolverine"],
  R("Wolverine", "Biped", ["Blade", "Charge", "Fast"], scale=None, mass=140, accent=YELLOW,
    temp="Aggressive", inten=0.9), XM, H.wolverine())
E(["cyclops x-men", "scott summers", "cyclops of the x-men", "slim summers"],
  R("Cyclops", "Biped", ["Beam", "Fast"], scale=None, mass=80, accent=NAVY, temp="Fearful", inten=0.5), XM, H.cyclops())
E(["jean grey", "marvel girl", "jean grey-summers", "jean grey summers"],
  R("Jean Grey", "Biped", ["Flies", "Grab", "Repel"], el="Psychic", scale=None, mass=60, accent=ORANGE,
    temp="Protective", inten=0.6), XM, H.jean_grey())
E(["dark phoenix", "the dark phoenix", "phoenix firebird", "phoenix force avatar"],
  R("Dark Phoenix", "Bird", ["Breath", "Explodes", "Flies"], el="Fire", scale=4.0, mass=300, accent=FIRE,
    temp="Aggressive", inten=0.9), XM, H.dark_phoenix())
E(["ororo munroe", "storm x-men", "x-men storm", "storm of the x-men", "windrider"],
  R("Storm", "Biped", ["Flies", "Glide", "Shoots"], el="Electric", scale=None, mass=65, accent=WHITE,
    temp="Fearful", inten=0.5, payload=R("lightning bolt", "Pillar", ["DropFromSky", "Explodes"], el="Electric",
                                        scale=5.0, mass=5)), XM, H.storm())
E(["magneto", "erik lehnsherr", "max eisenhardt", "master of magnetism"],
  R("Magneto", "Biped", ["Attract", "Flies", "Repel"], scale=None, mass=85, accent=CRIMSON, temp="Calm"), XM, H.magneto())
E(["nightcrawler", "kurt wagner", "the incredible nightcrawler"],
  R("Nightcrawler", "Biped", ["ArticulatedTail", "Fast", "Portal"], el="Shadow", scale=None, mass=60, accent=NAVY,
    temp="Playful", inten=0.8), XM, H.nightcrawler())
E(["colossus x-men", "piotr rasputin", "peter rasputin", "colossus of the x-men"],
  R("Colossus", "Biped", ["Blunt", "Charge", "Heavy"], scale=None, mass=500, accent=SILVER,
    temp="Protective", inten=0.8), XM, H.colossus())
E(["iceman", "bobby drake", "ice man x-men"],
  R("Iceman", "Biped", ["Fast", "Glide", "Shoots"], el="Ice", scale=None, mass=70, accent=ICE,
    temp="Playful", inten=0.7), XM, H.iceman())
E(["gambit", "remy lebeau", "le diable blanc"],
  R("Gambit", "Biped", ["Explodes", "Fast", "Shoots"], scale=None, mass=80, accent=MAGENTA, temp="Playful",
    inten=0.6, payload=R("charged card", "Ball", ["Explodes", "Fast"], scale=0.3, mass=0.2)), XM, H.gambit())
E(["rogue x-men", "anna marie", "rogue of the x-men", "rogue mutant"],
  R("Rogue", "Biped", ["Charge", "Flies", "Grab"], scale=None, mass=65, accent=DKGREEN,
    temp="Aggressive", inten=0.6), XM, H.rogue())
E(["beast x-men", "hank mccoy", "henry mccoy", "dr. hank mccoy"],
  R("Beast", "Biped", ["Bounce", "Fast", "Grab"], scale=None, mass=180, accent=BLUE,
    temp="Playful", inten=0.7), XM, H.beast())
E(["juggernaut", "the juggernaut", "cain marko", "juggernaut x-men", "unstoppable juggernaut"],
  R("Juggernaut", "Biped", ["Blunt", "Charge", "Heavy", "Stomp"], scale=None, mass=900, accent=RUST,
    temp="Aggressive", inten=1.0), XM, H.juggernaut())
E(["cable x-men", "nathan summers", "cable marvel", "nathan christopher summers"],
  R("Cable", "Biped", ["Heavy", "Shoots"], scale=None, mass=160, accent=STEEL, temp="Fearful", inten=0.6,
    payload=R("plasma round", "Ball", ["Explodes", "Fast"], scale=0.5, mass=2)), XM, H.cable())
E(["apocalypse", "en sabah nur", "the first mutant", "apocalypse x-men"],
  R("Apocalypse", "Biped", ["Blunt", "Grow", "Heavy", "Stomp"], scale=None, mass=600, accent=BLUE,
    temp="Calm"), XM, H.apocalypse())
E(["sabretooth", "victor creed"],
  R("Sabretooth", "Biped", ["Blade", "Charge", "Fast"], scale=None, mass=180, accent=(0.85, 0.7, 0.35),
    temp="Aggressive", inten=0.9), XM, H.sabretooth())
E(["blob x-men", "the blob marvel", "fred dukes", "frederick dukes"],
  R("The Blob", "Blob", ["Bounce", "Guard", "Heavy"], scale=None, mass=450, accent=(0.9, 0.75, 0.2),
    temp="Lazy", inten=0.8), XM, H.blob())
E(["kitty pryde", "shadowcat", "katherine pryde", "sprite x-men"],
  R("Kitty Pryde", "Biped", ["Fast", "Portal"], scale=None, mass=50, accent=YELLOW, temp="Sneaky", inten=0.7),
  XM, H.kitty_pryde())
E(["psylocke", "betsy braddock", "elizabeth braddock"],
  R("Psylocke", "Biped", ["Blade", "Fast"], el="Psychic", scale=None, mass=60, accent=PURPLE,
    temp="Sneaky", inten=0.7), XM, H.psylocke())
E(["mutant sentinel", "sentinel robot", "sentinels", "master mold", "sentinel mark iii", "giant sentinel"],
  R("Sentinel", "Kaiju", ["Beam", "Grab", "Heavy", "Stomp"], scale=None, mass=60000, accent=PURPLE,
    temp="Aggressive", inten=0.7), XM, H.sentinel())

# ============================================================== monsters, cosmic
E(["fin fang foom", "fing fang foom", "finfangfoom"],
  R("Fin Fang Foom", "Kaiju", ["ArticulatedTail", "Breath", "Stomp"], el="Fire", scale=None, mass=80000,
    accent=PURPLE, temp="Aggressive", inten=0.8), MV + " (Monsters)", H.fin_fang_foom())
E(["johnny blaze", "robbie reyes", "danny ketch", "spirit rider"],
  R("Ghost Rider", "Biped", ["Charge", "Link", "Pull"], el="Fire", scale=None, mass=90, accent=ORANGE,
    temp="Aggressive", inten=0.8), MV + " (Ghost Rider)", H.johnny_blaze())
E(["wade wilson", "merc with a mouth", "deadpool corps", "lady deadpool", "dogpool"],
  R("Deadpool", "Biped", ["Blade", "Fast", "Shoots"], scale=None, mass=95, accent=RED,
    temp="Chaotic", inten=0.9), MV + " (Deadpool)", H.wade_wilson())

# ============================================================== items
E(["wakandan spear", "vibranium spear", "okoye's spear", "dora milaje spear", "okoye spear"],
  R("Wakandan Spear", "Spear", ["Pierce", "Returning"], scale=None, mass=4, accent=SILVER), MV + " (Black Panther)",
  I.wakandan_spear())
E(["cap's heater shield", "captain america heater shield", "triangular shield", "cap's first shield",
   "original captain america shield"],
  R("Heater Shield", "Shield", ["Guard", "Ricochet"], scale=None, mass=6, accent=RED), MV + " (Captain America)",
  I.cap_heater_shield())
E(["wakandan shield", "border tribe shield", "wakandan cape shield", "dora milaje shield"],
  R("Wakandan Shield", "Shield", ["Barrier", "Guard"], scale=None, mass=5, accent=PURPLE), MV + " (Black Panther)",
  I.wakandan_shield())
E(["gambit's cards", "gambit cards", "charged playing cards", "kinetic cards", "gambit card", "charged card",
   "exploding playing cards"],
  R("Charged Cards", "Ball", ["Explodes", "Fast"], scale=None, mass=0.2, count=3, accent=MAGENTA), XM,
  I.gambit_card())
E(["yaka arrow", "yondu's arrow", "yondu arrow", "yaka-arrow"],
  R("Yaka Arrow", "Spear", ["Fast", "Homing", "Pierce", "Returning"], scale=None, mass=1, accent=RED),
  MV + " (Guardians of the Galaxy)", I.yaka_arrow())
E(["element guns", "star-lord's blasters", "quad blasters", "star lord blasters", "element gun",
   "star-lord's guns"],
  R("Element Guns", "Gun", ["Shoots"], el="Fire", scale=None, mass=2, count=2, accent=CRIMSON),
  MV + " (Guardians of the Galaxy)", I.element_gun())
E(["groot's branches", "groot branches", "groot branch", "groot vines", "groot arm"],
  R("Groot's Branches", "Whip", ["Constrict", "Grab", "Pull"], el="Earth", scale=None, mass=8, accent=BROWN),
  MV + " (Guardians of the Galaxy)", I.groot_branches())
E(["photon blast", "photon blasts", "captain marvel blast", "photon fist", "binary blast"],
  R("Photon Blast", "Gauntlet", ["Beam", "Repel"], scale=None, mass=3, accent=GOLD), MV + " (Captain Marvel)",
  I.photon_gauntlet())
E(["jarnbjorn", "jarnbjörn", "thor's axe jarnbjorn"],
  R("Jarnbjorn", "Axe", ["Blade", "Returning"], el="Electric", scale=None, mass=12, accent=SILVER), MV + " (Thor)",
  I.jarnbjorn())
E(["odin's gungnir", "all-father's spear", "marvel gungnir"],
  R("Gungnir", "Spear", ["Beam", "Pierce", "Returning"], el="Electric", scale=None, mass=10, accent=GOLD),
  MV + " (Thor)", I.gungnir())
E(["hofund", "heimdall's sword", "bifrost sword", "heimdall sword"],
  R("Hofund", "Sword", ["Blade", "Portal"], scale=None, mass=6, accent=GOLD), MV + " (Thor)", I.hofund())
E(["twilight sword", "surtur's sword", "sword of surtur"],
  R("Twilight Sword", "Sword", ["Blade", "Heavy"], el="Fire", scale=None, mass=60, accent=FIRE), MV + " (Thor)",
  I.twilight_sword())
E(["widow's bite", "widows bite", "black widow gauntlet", "widow's sting", "black widow's bite"],
  R("Widow's Bite", "Gauntlet", ["Fast", "Shoots"], el="Electric", scale=None, mass=1, accent=RED),
  MV + " (Black Widow)", I.widows_bite())
E(["winter soldier arm", "bucky's arm", "vibranium arm", "bionic arm winter soldier", "bucky barnes arm"],
  R("Winter Soldier Arm", "Gauntlet", ["Blunt", "Grab"], scale=None, mass=15, accent=RED), MV + " (Captain America)",
  I.vibranium_arm())
E(["elektra's sais", "elektra sais", "elektra sai", "sai daggers"],
  R("Elektra's Sais", "Sword", ["Fast", "Pierce"], scale=None, mass=1, count=2, accent=RED), MV + " (Daredevil)",
  I.elektra_sai())
E(["iron fist", "the iron fist", "danny rand's fist", "chi punch", "fist of shou-lao"],
  R("Iron Fist", "Gauntlet", ["Blunt", "Explodes"], scale=None, mass=4, accent=GOLD), MV + " (Iron Fist)",
  I.iron_fist())
E(["clobberin time", "it's clobberin time", "clobbering time", "thing's fist", "the thing's fist"],
  R("Clobberin' Time", "Gauntlet", ["Blunt", "Heavy"], el="Earth", scale=None, mass=60, accent=ORANGE),
  MV + " (Fantastic Four)", I.clobberin_fist())
E(["universal weapon", "ronan's hammer", "cosmi-rod", "ronan the accuser's hammer", "kree universal weapon"],
  R("Universal Weapon", "Hammer", ["Beam", "Blunt"], el="Atomic", scale=None, mass=30, accent=BLUE),
  MV + " (Guardians of the Galaxy)", I.ronan_hammer())
E(["crescent darts", "moon knight darts", "crescent dart", "moon darts"],
  R("Crescent Darts", "Disc", ["Pierce", "Returning", "Swarm"], scale=None, mass=0.3, count=3, accent=WHITE),
  MV + " (Moon Knight)", I.crescent_dart())
E(["falcon's wings", "falcon wings", "exo-7 falcon", "sam wilson's wings", "falcon wingsuit"],
  R("Falcon's Wings", "Wings", ["Glide", "Shoots"], scale=None, mass=20, accent=RED), MV + " (Falcon)",
  I.falcon_wings())
E(["the tesseract", "tesseract cube", "space cube"],
  R("Tesseract", "Ball", ["Portal"], scale=None, mass=3, accent=(0.3, 0.6, 1.0)), MV + " (Avengers)", I.tesseract())
for nm, al, acc, mech, el in (
        ("mind stone", ["mind infinity stone"], YELLOW, ["Homing"], "Psychic"),
        ("space stone", ["space infinity stone"], BLUE, ["Portal"], "Light"),
        ("reality stone", ["reality infinity stone", "the aether"], RED, ["Sticky"], "Shadow"),
        ("power stone", ["power infinity stone", "morag orb"], PURPLE, ["Explodes", "Heavy"], "Atomic"),
        ("time stone", ["time infinity stone"], GREEN, ["Returning"], "Light"),
        ("soul stone", ["soul infinity stone"], ORANGE, ["Grab"], "Shadow")):
    E([nm] + al, R(nm.title(), "Ball", mech, el=el, scale=None, mass=1, accent=acc), MV + " (Infinity Stones)",
      I.infinity_stone())
E(["storm's lightning", "storm lightning", "lightning of storm", "ororo's lightning", "storm's lightning bolt"],
  R("Storm's Lightning", "Pillar", ["Beam", "DropFromSky", "Explodes"], el="Electric", scale=None, mass=5,
    accent=WHITE), XM, I.storm_lightning())
E(["magneto's helmet", "magneto helmet", "magnetos helmet", "anti-telepathy helmet"],
  R("Magneto's Helmet", "Dome", ["Barrier", "Repel"], scale=None, mass=40, accent=CRIMSON), XM, I.magneto_helmet())
E(["web net", "impact webbing", "web ball", "web grenade", "spider net", "web cocoon"],
  R("Web Net", "Cage", ["Sticky", "Trap"], scale=None, mass=5, accent=WHITE), MV + " (Spider-Man)", I.web_net())
E(["pumpkin bomb", "pumpkin bombs", "goblin bomb", "goblin grenade", "jack-o-lantern bomb"],
  R("Pumpkin Bomb", "Bomb", ["Bounce", "Explodes"], el="Fire", scale=None, mass=1, count=3, accent=ORANGE),
  MV + " (Spider-Man)", I.pumpkin_bomb())
E(["goblin glider", "green goblin glider", "goblin's glider", "bat glider"],
  R("Goblin Glider", "Vehicle", ["Flies", "Piloted", "Shoots"], scale=None, mass=60, accent=PURPLE,
    feat=["Jet"], payload=R("pumpkin bomb", "Bomb", ["Explodes"], scale=0.4, mass=1)), MV + " (Spider-Man)",
  I.goblin_glider())
E(["spider-mobile", "spidermobile", "spider mobile", "spider buggy"],
  R("Spider-Mobile", "Vehicle", ["Charge", "Piloted", "Shoots"], scale=None, mass=900, accent=RED,
    feat=["Headlight", "Wheels"], payload=R("web ball", "Ball", ["Sticky", "Trap"], scale=0.5, mass=1)),
  MV + " (Spider-Man)", I.spider_mobile())
E(["x-jet", "x-men blackbird", "the blackbird jet", "blackbird jet", "sr-77 blackbird", "xjet"],
  R("X-Jet Blackbird", "Vehicle", ["Flies", "Piloted", "Shoots"], scale=None, mass=20000, accent=BLACK,
    feat=["Cockpit", "Jet"]), XM, I.blackbird())

# ============================================================== procedural (no hand-authored shape)
P = [
    (["black widow natasha", "natasha romanoff", "natasha romanova", "yelena belova"],
     R("Black Widow", "Biped", ["Fast", "Shoots"], el="Electric", scale=1.8, mass=60, accent=RED, temp="Sneaky", inten=0.8), "Black Widow"),
    (["winter soldier", "bucky barnes", "james buchanan barnes", "white wolf"],
     R("Winter Soldier", "Biped", ["Blunt", "Shoots"], scale=1.9, mass=120, accent=SILVER, temp="Sneaky", inten=0.6), "Captain America"),
    (["sam wilson", "falcon marvel", "the falcon avengers"],
     R("Falcon", "Biped", ["Flies", "Glide", "Shoots"], scale=1.9, mass=85, accent=RED, temp="Protective", inten=0.6), "Falcon"),
    (["redwing drone", "redwing falcon", "redwing"],
     R("Redwing", "Bird", ["Flies", "Homing", "Shoots"], scale=0.8, mass=3, accent=RED, temp="Protective", inten=0.7), "Falcon"),
    (["the wasp", "janet van dyne", "hope van dyne", "wasp marvel"],
     R("Wasp", "Insect", ["Fast", "Flies", "Shoots"], el="Electric", scale=0.6, mass=2, accent=YELLOW, temp="Playful", inten=0.7), "Ant-Man"),
    (["yellowjacket", "darren cross"],
     R("Yellowjacket", "Insect", ["Flies", "Shoots", "Shrink"], el="Electric", scale=0.7, mass=3, accent=YELLOW, temp="Aggressive", inten=0.7), "Ant-Man"),
    (["gamora", "gamora zen whoberi"],
     R("Gamora", "Biped", ["Blade", "Fast"], scale=1.9, mass=70, accent=GREEN, temp="Aggressive", inten=0.7), "Guardians of the Galaxy"),
    (["drax the destroyer", "drax"],
     R("Drax", "Biped", ["Blade", "Charge", "Heavy"], scale=2.1, mass=160, accent=RED, temp="Aggressive", inten=0.8), "Guardians of the Galaxy"),
    (["mantis marvel", "marvel mantis"],
     R("Mantis", "Biped", ["Fast"], el="Psychic", scale=1.7, mass=50, accent=GREEN, temp="Protective", inten=0.6), "Guardians of the Galaxy"),
    (["nebula marvel", "marvel nebula"],
     R("Nebula", "Biped", ["Blade", "Shoots"], el="Electric", scale=1.9, mass=90, accent=BLUE, temp="Sneaky", inten=0.7), "Guardians of the Galaxy"),
    (["baby groot", "little groot", "teen groot"],
     R("Baby Groot", "Biped", ["Grab"], el="Earth", scale=0.6, mass=5, accent=BROWN, temp="Playful", inten=0.9), "Guardians of the Galaxy"),
    (["the milano", "milano ship", "guardians ship", "benatar ship"],
     R("Milano", "Vehicle", ["Flies", "Piloted", "Shoots"], scale=10.0, mass=15000, accent=ORANGE, feat=["Cockpit", "Jet"]), "Guardians of the Galaxy"),
    (["luke cage", "power man", "hero for hire"],
     R("Luke Cage", "Biped", ["Blunt", "Guard", "Heavy"], scale=2.0, mass=190, accent=YELLOW, temp="Protective", inten=0.8), "Luke Cage"),
    (["danny rand", "immortal iron fist"],
     R("Iron Fist", "Biped", ["Blunt", "Fast"], scale=1.8, mass=80, accent=GOLD, temp="Calm"), "Iron Fist"),
    (["the punisher", "frank castle", "punisher marvel"],
     R("Punisher", "Biped", ["Heavy", "Shoots"], scale=1.9, mass=110, accent=WHITE, temp="Aggressive", inten=0.8), "Punisher"),
    (["elektra natchios", "elektra"],
     R("Elektra", "Biped", ["Blade", "Fast"], scale=1.8, mass=60, accent=RED, temp="Sneaky", inten=0.8), "Daredevil"),
    (["blade the vampire hunter", "eric brooks", "the daywalker", "daywalker"],
     R("Blade", "Biped", ["Blade", "Fast"], scale=1.9, mass=95, accent=BLACK, temp="Aggressive", inten=0.7), "Blade"),
    (["morbius", "michael morbius", "living vampire"],
     R("Morbius", "Biped", ["Fast", "Flies", "Grab"], el="Shadow", scale=1.9, mass=80, accent=BLACK, temp="Sneaky", inten=0.8), "Spider-Man"),
    (["man-thing", "man thing", "ted sallis"],
     R("Man-Thing", "Biped", ["Constrict", "Grab"], el="Poison", scale=3.2, mass=400, accent=DKGREEN, temp="Lazy", inten=0.6), "Monsters"),
    (["kraven the hunter", "kraven", "sergei kravinoff"],
     R("Kraven the Hunter", "Biped", ["Charge", "Pierce"], scale=1.9, mass=100, accent=BROWN, temp="Sneaky", inten=0.8), "Spider-Man"),
    (["the lizard marvel", "curt connors", "lizard spider-man"],
     R("The Lizard", "Biped", ["ArticulatedTail", "Charge", "Grab"], scale=2.4, mass=250, accent=GREEN, temp="Aggressive", inten=0.8), "Spider-Man"),
    (["flint marko", "sandman spider-man", "sandman marvel"],
     R("Sandman", "Biped", ["Blunt", "Heavy"], el="Earth", scale=2.6, mass=500, accent=(0.85, 0.75, 0.5), temp="Aggressive", inten=0.6), "Spider-Man"),
    (["scorpion spider-man", "mac gargan", "scorpion marvel"],
     R("Scorpion", "Biped", ["ArticulatedTail", "Pierce", "Shoots"], el="Poison", scale=2.0, mass=150, accent=GREEN, temp="Aggressive", inten=0.7), "Spider-Man"),
    (["shocker spider-man", "herman schultz", "shocker marvel"],
     R("Shocker", "Biped", ["Repel", "Shoots"], scale=1.8, mass=90, accent=YELLOW, temp="Fearful", inten=0.5), "Spider-Man"),
    (["sp//dr", "spdr", "peni parker", "peni parker mech"],
     R("SP//dr", "Arachnid", ["Piloted", "Shoots", "Sticky"], scale=3.2, mass=2000, accent=RED), "Spider-Verse"),
    (["spider-ham", "spider ham", "peter porker"],
     R("Spider-Ham", "Biped", ["Bounce", "Sticky"], scale=0.9, mass=20, accent=RED, temp="Playful", inten=1.0), "Spider-Verse"),
    (["spider-man noir", "spiderman noir", "spider noir"],
     R("Spider-Man Noir", "Biped", ["Pull", "Shoots", "Sticky"], el="Shadow", scale=1.8, mass=75, accent=BLACK, temp="Sneaky", inten=0.7), "Spider-Verse"),
    (["scarlet spider", "ben reilly", "kaine parker"],
     R("Scarlet Spider", "Biped", ["Fast", "Pull", "Sticky"], scale=1.8, mass=75, accent=RED, temp="Aggressive", inten=0.6), "Spider-Man"),
    (["spider-woman", "spider woman", "jessica drew"],
     R("Spider-Woman", "Biped", ["Flies", "Shoots"], el="Electric", scale=1.8, mass=65, accent=RED, temp="Aggressive", inten=0.6), "Spider-Woman"),
    (["silk marvel", "cindy moon"],
     R("Silk", "Biped", ["Fast", "Pull", "Sticky"], scale=1.7, mass=55, accent=WHITE, temp="Playful", inten=0.6), "Spider-Man"),
    (["quicksilver", "pietro maximoff"],
     R("Quicksilver", "Biped", ["Charge", "Fast"], el="Wind", scale=1.8, mass=75, accent=SILVER, temp="Playful", inten=0.8), "X-Men / Avengers"),
    (["hope summers", "phoenix hope"],
     R("Hope Summers", "Biped", ["Flies", "Shoots"], el="Fire", scale=1.8, mass=60, accent=FIRE, temp="Protective", inten=0.6), "X-Men"),
    (["mystique", "raven darkholme"],
     R("Mystique", "Biped", ["Blade", "Fast"], scale=1.8, mass=60, accent=BLUE, temp="Sneaky", inten=1.0), "X-Men"),
    (["professor x", "charles xavier", "professor xavier", "charles francis xavier"],
     R("Professor X", "Biped", ["Repel"], el="Psychic", scale=1.8, mass=80, accent=GOLD, temp="Protective", inten=0.9), "X-Men"),
    (["cerebro", "cerebra", "cerebro helmet"],
     R("Cerebro", "Dome", ["Barrier", "Homing"], el="Psychic", scale=6.0, mass=200, accent=(0.3, 0.5, 1.0)), "X-Men"),
    (["x-mansion", "xavier's school", "xavier institute"],
     R("X-Mansion", "Wall", ["Barrier"], scale=8.0, mass=50000, accent=BROWN), "X-Men"),
    (["danger room", "the danger room"],
     R("Danger Room", "Cage", ["Shoots", "Trap"], scale=6.0, mass=5000, accent=STEEL), "X-Men"),
    (["emma frost", "white queen"],
     R("Emma Frost", "Biped", ["Guard", "Heavy"], el="Psychic", scale=1.8, mass=70, accent=WHITE, temp="Calm"), "X-Men"),
    (["banshee x-men", "sean cassidy", "siryn"],
     R("Banshee", "Biped", ["Flies", "Repel"], el="Wind", scale=1.8, mass=75, accent=GREEN, temp="Fearful", inten=0.5), "X-Men"),
    (["havok", "alex summers"],
     R("Havok", "Biped", ["Explodes", "Shoots"], el="Atomic", scale=1.9, mass=80, accent=BLACK, temp="Aggressive", inten=0.5), "X-Men"),
    (["bishop x-men", "lucas bishop"],
     R("Bishop", "Biped", ["Heavy", "Shoots"], scale=2.0, mass=120, accent=NAVY, temp="Aggressive", inten=0.6), "X-Men"),
    (["domino x-men", "neena thurman"],
     R("Domino", "Biped", ["Fast", "Ricochet", "Shoots"], scale=1.8, mass=60, accent=WHITE, temp="Playful", inten=0.8), "X-Force"),
    (["x-23", "laura kinney", "all-new wolverine", "honey badger"],
     R("X-23", "Biped", ["Blade", "Charge", "Fast"], scale=1.6, mass=60, accent=YELLOW, temp="Aggressive", inten=0.9), "X-Men"),
    (["daken", "akihiro"],
     R("Daken", "Biped", ["Blade", "Fast"], el="Poison", scale=1.8, mass=80, accent=BLACK, temp="Sneaky", inten=0.8), "X-Men"),
    (["omega red", "arkady rossovich"],
     R("Omega Red", "Biped", ["Constrict", "Link", "Pull"], el="Poison", scale=2.1, mass=160, accent=RED, temp="Aggressive", inten=0.8), "X-Men"),
    (["mister sinister", "mr. sinister", "nathaniel essex"],
     R("Mister Sinister", "Biped", ["Beam", "Blade"], el="Shadow", scale=2.0, mass=100, accent=RED, temp="Sneaky", inten=0.6), "X-Men"),
    (["toad x-men", "mortimer toynbee"],
     R("Toad", "Biped", ["Bounce", "Pull", "Sticky"], el="Poison", scale=1.5, mass=50, accent=GREEN, temp="Playful", inten=0.8), "X-Men"),
    (["pyro x-men", "st. john allerdyce", "john allerdyce"],
     R("Pyro", "Biped", ["Breath"], el="Fire", scale=1.8, mass=65, accent=ORANGE, temp="Chaotic", inten=0.8), "X-Men"),
    (["avalanche x-men", "dominic petros"],
     R("Avalanche", "Biped", ["Stomp"], el="Earth", scale=1.9, mass=90, accent=BROWN, temp="Aggressive", inten=0.6), "X-Men"),
    (["lockheed", "kitty's dragon", "lockheed dragon"],
     R("Lockheed", "Bird", ["Breath", "Flies"], el="Fire", scale=0.7, mass=8, accent=PURPLE, temp="Protective", inten=0.9), "X-Men"),
    (["warlock new mutants", "technarch warlock"],
     R("Warlock", "Blob", ["Bounce", "Grab"], scale=1.8, mass=120, accent=YELLOW, temp="Playful", inten=0.9), "New Mutants"),
    (["archangel", "warren worthington", "angel x-men"],
     R("Archangel", "Biped", ["Blade", "Flies", "Shoots"], scale=2.0, mass=80, accent=BLUE, temp="Aggressive", inten=0.7), "X-Men"),
    (["nimrod sentinel", "nimrod"],
     R("Nimrod", "Kaiju", ["Beam", "Flies", "Shoots"], scale=12.0, mass=30000, accent=RED, temp="Aggressive", inten=0.9), "X-Men"),
    (["dormammu", "the dread dormammu"],
     R("Dormammu", "Kaiju", ["Breath", "Flies"], el="Fire", scale=18.0, mass=50000, accent=PURPLE, temp="Aggressive", inten=0.7), "Doctor Strange"),
    (["mephisto", "lord of hell mephisto"],
     R("Mephisto", "Biped", ["Breath", "Grab"], el="Fire", scale=2.4, mass=150, accent=RED, temp="Sneaky", inten=0.8), "Ghost Rider"),
    (["arishem", "arishem the judge", "celestial", "the celestials"],
     R("Arishem", "Kaiju", ["Beam", "Heavy", "Stomp"], el="Atomic", scale=22.0, mass=100000, accent=RED, temp="Lazy", inten=0.6), "Eternals"),
    (["ego the living planet", "ego planet"],
     R("Ego", "Ball", ["Attract", "DropFromSky", "Heavy"], scale=20.0, mass=100000, accent=BLUE), "Guardians of the Galaxy"),
    (["ronan the accuser", "ronan"],
     R("Ronan", "Biped", ["Blunt", "Heavy"], el="Atomic", scale=2.3, mass=180, accent=BLUE, temp="Aggressive", inten=0.7), "Guardians of the Galaxy"),
    (["annihilus", "annihilation wave"],
     R("Annihilus", "Insect", ["Flies", "Grab", "Swarm"], el="Poison", scale=1.2, mass=40, count=6, accent=GREEN, temp="Aggressive", inten=0.9), "Fantastic Four"),
    (["the brood", "brood swarm", "brood queen"],
     R("The Brood", "Insect", ["Flies", "Pierce", "Swarm"], el="Poison", scale=1.0, mass=30, count=8, accent=GREEN, temp="Aggressive", inten=1.0), "X-Men"),
    (["skrull", "skrull warrior", "skrull army"],
     R("Skrulls", "Biped", ["Shoots", "Swarm"], scale=1.8, mass=70, count=4, accent=GREEN, temp="Sneaky", inten=0.8), "Avengers"),
    (["chitauri", "chitauri army", "chitauri soldiers"],
     R("Chitauri", "Biped", ["Shoots", "Swarm"], el="Electric", scale=1.9, mass=80, count=6, accent=GREY, temp="Aggressive", inten=0.8), "Avengers"),
    (["chitauri leviathan", "leviathan chitauri"],
     R("Chitauri Leviathan", "Serpent", ["ArticulatedTail", "Charge", "Flies"], scale=20.0, mass=80000, accent=GREY, temp="Aggressive", inten=0.7), "Avengers"),
    (["outriders", "outrider swarm"],
     R("Outriders", "Quadruped", ["Charge", "Fast", "Swarm"], scale=1.2, mass=40, count=8, accent=GREY, temp="Aggressive", inten=1.0), "Avengers"),
    (["hydra agent", "hydra soldiers", "hail hydra"],
     R("Hydra Agents", "Biped", ["Shoots", "Swarm"], scale=1.8, mass=80, count=4, accent=GREEN, temp="Aggressive", inten=0.6), "Captain America"),
    (["baron zemo", "helmut zemo"],
     R("Baron Zemo", "Biped", ["Blade", "Shoots"], scale=1.8, mass=80, accent=PURPLE, temp="Sneaky", inten=0.8), "Captain America"),
    (["taskmaster", "tony masters"],
     R("Taskmaster", "Biped", ["Blade", "Guard", "Shoots"], scale=1.9, mass=90, accent=WHITE, temp="Sneaky", inten=0.6), "Avengers"),
    (["the leader", "samuel sterns"],
     R("The Leader", "Biped", ["Shoots"], el="Psychic", scale=1.8, mass=60, accent=GREEN, temp="Fearful", inten=0.8), "Hulk"),
    (["abomination", "emil blonsky"],
     R("Abomination", "Biped", ["Blunt", "Charge", "Heavy", "Stomp"], scale=3.8, mass=800, accent=DKGREEN, temp="Aggressive", inten=1.0), "Hulk"),
    (["wendigo marvel", "wendigo hulk"],
     R("Wendigo", "Biped", ["Blade", "Charge"], el="Ice", scale=3.2, mass=500, accent=WHITE, temp="Aggressive", inten=0.9), "Hulk"),
    (["devil dinosaur", "moon girl and devil dinosaur"],
     R("Devil Dinosaur", "Kaiju", ["ArticulatedTail", "Charge", "Stomp"], scale=10.0, mass=8000, accent=RED, temp="Playful", inten=0.8), "Moon Girl"),
    (["fantasticar", "fantasti-car", "the fantasticar"],
     R("Fantasticar", "Vehicle", ["Flies", "Piloted"], scale=5.0, mass=1500, accent=BLUE, feat=["Jet"]), "Fantastic Four"),
    (["captain america's motorcycle", "cap's motorcycle", "captain america bike"],
     R("Cap's Motorcycle", "Vehicle", ["Charge", "Fast", "Piloted"], scale=2.4, mass=250, accent=BLUE, feat=["Headlight", "Wheels"]), "Captain America"),
    (["hell charger", "ghost rider car", "robbie reyes' charger"],
     R("Hell Charger", "Vehicle", ["Charge", "Fast", "Piloted"], el="Fire", scale=5.0, mass=1600, accent=ORANGE, feat=["Exhaust", "Wheels"]), "Ghost Rider"),
    (["eye of agamotto", "agamotto amulet"],
     R("Eye of Agamotto", "Dome", ["Barrier", "Repel"], el="Psychic", scale=5.0, mass=100, accent=GREEN), "Doctor Strange"),
    (["crimson bands of cyttorak", "bands of cyttorak"],
     R("Crimson Bands of Cyttorak", "Cage", ["Link", "Trap"], el="Psychic", scale=2.2, mass=20, accent=CRIMSON), "Doctor Strange"),
    (["shield of seraphim", "shields of the seraphim", "seraphim shield"],
     R("Shield of Seraphim", "Shield", ["Barrier", "Guard", "Repel"], el="Psychic", scale=1.4, mass=2, accent=GOLD), "Doctor Strange"),
    (["darkhold", "the darkhold"],
     R("Darkhold", "Staff", ["Shoots"], el="Shadow", scale=0.6, mass=3, accent=CRIMSON), "Scarlet Witch"),
    (["ultimate nullifier", "the ultimate nullifier"],
     R("Ultimate Nullifier", "Gun", ["Beam", "Pierce"], el="Atomic", scale=1.0, mass=5, accent=SILVER), "Fantastic Four"),
    (["cosmic control rod", "annihilus rod"],
     R("Cosmic Control Rod", "Staff", ["Beam", "Repel"], el="Atomic", scale=1.4, mass=4, accent=GOLD), "Fantastic Four"),
    (["penance stare", "ghost rider stare"],
     R("Penance Stare", "Staff", ["Beam"], el="Fire", scale=1.0, mass=1, accent=ORANGE), "Ghost Rider"),
    (["kimoyo beads", "kimoyo bracelet"],
     R("Kimoyo Beads", "Gauntlet", ["Shoots"], el="Electric", scale=0.5, mass=1, accent=PURPLE), "Black Panther"),
    (["vibranium", "wakandan vibranium", "vibranium ore"],
     R("Vibranium", "Boulder", ["Heavy", "Repel"], scale=1.4, mass=400, accent=PURPLE), "Black Panther"),
    (["adamantium bullet", "adamantium bullets"],
     R("Adamantium Bullet", "Gun", ["Pierce", "Shoots"], scale=0.8, mass=2, accent=SILVER), "X-Men"),
    (["bone claws", "logan's bone claws"],
     R("Bone Claws", "Gauntlet", ["Blade", "Pierce"], scale=0.7, mass=2, accent=WHITE), "X-Men"),
    (["cyclops visor beam", "ruby quartz visor", "cyclops glasses"],
     R("Ruby Quartz Visor", "Staff", ["Beam", "Fast"], scale=0.6, mass=1, accent=RED), "X-Men"),
    (["gambit's bo staff", "gambit staff", "gambit bo staff"],
     R("Gambit's Bo Staff", "Staff", ["Blunt", "Explodes"], scale=1.8, mass=3, accent=MAGENTA), "X-Men"),
    (["psychic knife", "psi-blade", "psychic katana"],
     R("Psi-Blade", "Sword", ["Blade", "Fast"], el="Psychic", scale=1.2, mass=1, accent=PURPLE), "X-Men"),
    (["soulsword", "magik's soulsword", "magik sword"],
     R("Soulsword", "Sword", ["Blade", "Portal"], el="Psychic", scale=1.4, mass=3, accent=GOLD), "X-Men"),
    (["magik", "illyana rasputina"],
     R("Magik", "Biped", ["Blade", "Portal"], el="Psychic", scale=1.7, mass=55, accent=YELLOW, temp="Sneaky", inten=0.7), "X-Men"),
    (["deadpool's gun", "deadpool pistols", "deadpool guns"],
     R("Deadpool's Pistols", "Gun", ["Shoots"], scale=0.7, mass=2, count=2, accent=RED), "Deadpool"),
    (["cable's gun", "cable's rifle", "cable plasma rifle"],
     R("Cable's Rifle", "Gun", ["Explodes", "Shoots"], el="Atomic", scale=1.6, mass=12, accent=STEEL), "X-Men"),
    (["captain britain", "brian braddock"],
     R("Captain Britain", "Biped", ["Blunt", "Flies", "Guard"], scale=2.0, mass=110, accent=RED, temp="Protective", inten=0.7), "Excalibur"),
    (["squirrel girl", "doreen green", "unbeatable squirrel girl"],
     R("Squirrel Girl", "Quadruped", ["Fast", "Swarm"], scale=0.5, mass=2, count=8, accent=BROWN, temp="Playful", inten=1.0), "Squirrel Girl"),
    (["howard the duck"],
     R("Howard the Duck", "Bird", ["Shoots"], scale=0.9, mass=15, accent=WHITE, temp="Lazy", inten=0.6), "Howard the Duck"),
    (["cosmo the spacedog", "cosmo spacedog"],
     R("Cosmo", "Quadruped", ["Fast", "Repel"], el="Psychic", scale=0.9, mass=25, accent=RED, temp="Protective", inten=0.8), "Guardians of the Galaxy"),
    (["ms. marvel", "ms marvel", "kamala khan"],
     R("Ms. Marvel", "Biped", ["Blunt", "Grab", "Pull"], scale=1.7, mass=55, accent=RED, temp="Playful", inten=0.7), "Ms. Marvel"),
    (["america chavez", "miss america chavez"],
     R("America Chavez", "Biped", ["Blunt", "Portal"], scale=1.8, mass=65, accent=BLUE, temp="Aggressive", inten=0.6), "Young Avengers"),
    (["shang-chi", "shang chi", "ten rings"],
     R("Shang-Chi", "Biped", ["Blunt", "Fast", "Returning"], scale=1.8, mass=75, accent=RED, temp="Calm"), "Shang-Chi"),
    (["the ten rings", "ten rings weapon", "wenwu's rings"],
     R("The Ten Rings", "Disc", ["Link", "Returning", "Swarm"], scale=0.6, mass=3, count=5, accent=(0.3, 0.6, 1.0)), "Shang-Chi"),
    (["hercules marvel", "prince of power"],
     R("Hercules", "Biped", ["Blunt", "Heavy"], scale=2.2, mass=250, accent=GOLD, temp="Playful", inten=0.6), "Avengers"),
    (["valkyrie marvel", "brunnhilde"],
     R("Valkyrie", "Biped", ["Blade", "Flies"], scale=1.9, mass=80, accent=SILVER, temp="Aggressive", inten=0.6), "Thor"),
    (["hela", "goddess of death"],
     R("Hela", "Biped", ["Blade", "Shoots", "Swarm"], el="Shadow", scale=2.0, mass=80, accent=DKGREEN, temp="Aggressive", inten=0.7), "Thor"),
    (["frost giant", "jotun", "frost giants"],
     R("Frost Giant", "Kaiju", ["Blunt", "Stomp"], el="Ice", scale=10.0, mass=12000, accent=ICE, temp="Aggressive", inten=0.6), "Thor"),
    (["surtur", "fire demon surtur"],
     R("Surtur", "Kaiju", ["Blade", "Breath", "Stomp"], el="Fire", scale=20.0, mass=90000, accent=FIRE, temp="Aggressive", inten=0.9), "Thor"),
    (["bifrost", "rainbow bridge", "the bifrost"],
     R("Bifrost", "Pillar", ["Beam", "DropFromSky", "Portal"], scale=8.0, mass=100, accent=(0.6, 0.4, 1.0)), "Thor"),
    (["destroyer armor", "the destroyer asgard", "asgardian destroyer"],
     R("The Destroyer", "Biped", ["Beam", "Heavy", "Stomp"], el="Fire", scale=4.0, mass=3000, accent=SILVER, temp="Aggressive", inten=0.7), "Thor"),
    (["giganto", "mole man's monster"],
     R("Giganto", "Kaiju", ["Grab", "Stomp"], el="Earth", scale=16.0, mass=60000, accent=GREEN, temp="Aggressive", inten=0.6), "Fantastic Four"),
    (["mole man", "harvey elder"],
     R("Mole Man", "Biped", ["Stomp"], el="Earth", scale=1.4, mass=60, accent=GREEN, temp="Fearful", inten=0.5), "Fantastic Four"),
    (["namor", "the sub-mariner", "sub-mariner"],
     R("Namor", "Biped", ["Fast", "Flies", "Pierce"], el="Water", scale=1.9, mass=130, accent=GREEN, temp="Aggressive", inten=0.6), "Namor"),
    (["black bolt", "blackagar boltagon"],
     R("Black Bolt", "Biped", ["Flies", "Repel"], el="Wind", scale=1.9, mass=90, accent=BLACK, temp="Calm"), "Inhumans"),
    (["lockjaw", "inhuman teleporting dog"],
     R("Lockjaw", "Quadruped", ["Charge", "Portal"], scale=2.2, mass=900, accent=GREY, temp="Protective", inten=0.8), "Inhumans"),
    (["medusa inhuman", "medusalith amaquelin"],
     R("Medusa", "Biped", ["Constrict", "Grab", "Pull"], scale=1.8, mass=60, accent=RED, temp="Calm"), "Inhumans"),
    (["adam warlock", "him marvel"],
     R("Adam Warlock", "Biped", ["Beam", "Flies"], el="Atomic", scale=2.0, mass=90, accent=GOLD, temp="Calm"), "Guardians of the Galaxy"),
    (["nova corps", "richard rider", "sam alexander nova"],
     R("Nova", "Biped", ["Fast", "Flies", "Shoots"], el="Atomic", scale=1.8, mass=75, accent=GOLD, temp="Aggressive", inten=0.6), "Nova"),
    (["beta ray bill"],
     R("Beta Ray Bill", "Biped", ["Blunt", "Flies"], el="Electric", scale=2.2, mass=300, accent=GOLD, temp="Aggressive", inten=0.6), "Thor"),
    (["stormbreaker axe bill", "bill's stormbreaker"],
     R("Bill's Stormbreaker", "Hammer", ["Blunt", "Returning"], el="Electric", scale=1.0, mass=40, accent=SILVER), "Thor"),
    (["thunderstrike mace", "thunderstrike"],
     R("Thunderstrike Mace", "Hammer", ["Blunt", "Returning"], el="Electric", scale=1.0, mass=30, accent=SILVER), "Thor"),
    (["loki's daggers", "loki daggers", "loki knives"],
     R("Loki's Daggers", "Sword", ["Fast", "Pierce"], el="Psychic", scale=0.5, mass=1, count=2, accent=DKGREEN), "Thor"),
    (["alligator loki", "alli-gator loki"],
     R("Alligator Loki", "Quadruped", ["Grab"], el="Psychic", scale=1.0, mass=40, accent=DKGREEN, temp="Playful", inten=0.8), "Loki"),
    (["throg", "frog thor", "thor frog"],
     R("Throg", "Quadruped", ["Bounce", "Returning"], el="Electric", scale=0.4, mass=1, accent=RED, temp="Playful", inten=0.9), "Thor"),
]
for names, r, src in P:
    E(names, r, MV + " (%s)" % src)

if __name__ == "__main__":
    out = os.path.join(ROOT, "assets", "icons", "marvel.ron")
    n, ns = emit(out)
    print("wrote %s: %d entries, %d with shapes" % (out, n, ns))
